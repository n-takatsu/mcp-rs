//! ローカル時刻の日付（または時）でファイルを切り替える、ログのwriter（#464）。
//!
//! tracing-appenderの`RollingFileAppender`は、ファイル名の日付と切り替えを
//! UTCで決める（ローカル時刻にする設定が無い）。ログの中の時刻はローカル
//! 時刻なので、日本では`mcp-rs.log.2026-11-09`に11/9 9:00〜11/10 8:59の
//! 記録が入り、ファイル名と中身がずれていた。このwriterは、ファイル名も
//! 切り替えもローカル時刻で決める。ファイル名の形式は変えない
//! （`<prefix>.YYYY-MM-DD`、時間単位なら`<prefix>.YYYY-MM-DD-HH`）。
//!
//! - 切り替えの判定は書き込みのたびに行う。次の境界の時刻を覚えておき、
//!   それを過ぎたかだけを比べる（タイマーは使わない）
//! - 1回の`write`で受け取った内容は、1回の`write_all`でファイルに書く。
//!   `non_blocking`のワーカーは1件のログを1回で渡すので、同じファイルに
//!   追記する別のプロセス（Claude Desktopは2つ起動する）と行が混ざらない
//! - 時計が戻っても、前のファイルには戻らない（今のファイルに書き続ける）

use super::LogRotation;
use chrono::{DateTime, Duration, Local, NaiveDateTime, TimeZone, Timelike};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// ファイルを開き直せなかった場合に、次に試すまでの間隔
const RETRY_INTERVAL_SECS: i64 = 60;

/// `now`の時点で使うログファイル名。起動時の表示・`mcp_rs_status`・
/// writerの切り替えのすべてで、この関数を使う。
pub(crate) fn file_name_for(prefix: &str, rotation: &LogRotation, now: &NaiveDateTime) -> String {
    match rotation {
        LogRotation::Never => prefix.to_string(),
        LogRotation::Daily => format!("{}.{}", prefix, now.format("%Y-%m-%d")),
        LogRotation::Hourly => format!("{}.{}", prefix, now.format("%Y-%m-%d-%H")),
    }
}

/// `name`が、このwriterが`prefix`で作るファイルの名前か（#759）。どの
/// `rotation`の形も受け付ける（設定を変える前に作ったファイルも含める）:
/// `<prefix>`、`<prefix>.YYYY-MM-DD`、`<prefix>.YYYY-MM-DD-HH`
pub(crate) fn is_file_name_of(prefix: &str, name: &str) -> bool {
    if name == prefix {
        return true;
    }
    let Some(suffix) = name
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('.'))
    else {
        return false;
    };
    // 数字と`-`の位置で比べる（`file_name_for`の`%Y-%m-%d`・`%Y-%m-%d-%H`）
    let pattern: &[u8] = match suffix.len() {
        10 => b"dddd-dd-dd",
        13 => b"dddd-dd-dd-dd",
        _ => return false,
    };
    suffix.bytes().zip(pattern).all(|(c, p)| {
        if *p == b'd' {
            c.is_ascii_digit()
        } else {
            c == *p
        }
    })
}

/// `now`の次の切り替えの時刻（ローカル時刻の翌日0時、または次の正時）。
/// 切り替えない場合はNone。
fn next_rollover(rotation: &LogRotation, now: &DateTime<Local>) -> Option<DateTime<Local>> {
    let naive = now.naive_local();
    let start = match rotation {
        LogRotation::Never => return None,
        LogRotation::Daily => naive.date().and_hms_opt(0, 0, 0)? + Duration::days(1),
        LogRotation::Hourly => naive.date().and_hms_opt(naive.hour(), 0, 0)? + Duration::hours(1),
    };
    first_existing_local_time(start)
}

/// 夏時間の切り替えで、その時刻がローカル時刻に存在しない場合は、
/// 存在する最初の時刻（分の単位）を使う。
fn first_existing_local_time(start: NaiveDateTime) -> Option<DateTime<Local>> {
    first_existing_time(start, |time| Local.from_local_datetime(&time).earliest())
}

/// `start`から1分ずつ進め、`resolve`が時刻を返す最初の時刻（48時間まで）。
/// 1時間ずつ進めると、30分だけ飛ぶ夏時間（`Australia/Lord_Howe`は02:00から
/// 02:30）で03:00になり、02:30〜02:59のログが前の時間のファイルに入る（#744）
fn first_existing_time<T>(
    start: NaiveDateTime,
    resolve: impl Fn(NaiveDateTime) -> Option<T>,
) -> Option<T> {
    (0..48 * 60).find_map(|minutes| resolve(start + Duration::minutes(minutes)))
}

/// ログのファイルを追記で開く（#543）。Unix系では、新しく作るファイルを
/// 本人だけが読み書きできる権限（600）で作り、シンボリックリンクはたどらない
/// （`O_NOFOLLOW`）。ログにはツールの名前・エラーの文等が入り、ほかの利用者が
/// 先にリンクを置くと、mcp-rsに別のファイルへ追記させられるため。既にある
/// ファイルの権限は変えない。監査ログ（`audit_log.rs`）と同じ
fn open_append(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

type Clock = Box<dyn Fn() -> DateTime<Local> + Send>;

/// ファイルを切り替えたときに、新しいファイルの先頭に書く行を作る関数
/// （切り替えた時刻と、新しいファイルのパスを受け取る。末尾に改行を含める）
pub(crate) type RolloverHeader = Box<dyn Fn(&DateTime<Local>, &Path) -> String + Send>;

/// ファイルを切り替えた後に呼ぶ関数（新しいファイルのパスを受け取る）。
/// ログを書く処理の中で呼ぶので、時間のかかる処理は別のスレッドで行う
pub(crate) type RolloverHook = Box<dyn Fn(&Path) + Send>;

/// ローカル時刻でファイルを切り替える、追記用のwriter。
pub(crate) struct LocalRollingAppender {
    dir: PathBuf,
    prefix: String,
    rotation: LogRotation,
    clock: Clock,
    file: File,
    current_name: String,
    next_rollover: Option<DateTime<Local>>,
    /// 今書いているファイルのパス。`LogDestination`と共有し、
    /// `mcp_rs_status`が実際のファイルを表示できるようにする。
    active_path: Arc<Mutex<PathBuf>>,
    /// 切り替えたときに新しいファイルの先頭に書く行（#472）
    rollover_header: Option<RolloverHeader>,
    /// 切り替えたが、まだ書けていない先頭の行（#744。書き込みが失敗しても
    /// 失わず、次の書き込みで書く）
    pending_header: Option<Vec<u8>>,
    /// 切り替えた後に呼ぶ関数（古いログの削除、#759）
    rollover_hook: Option<RolloverHook>,
}

impl LocalRollingAppender {
    /// `dir`に、今のローカル時刻のファイルを開く（無ければ作る）。
    /// `dir`は存在している必要がある。
    pub(crate) fn new(dir: &Path, rotation: &LogRotation, prefix: &str) -> io::Result<Self> {
        Self::with_clock(dir, rotation, prefix, Box::new(Local::now))
    }

    fn with_clock(
        dir: &Path,
        rotation: &LogRotation,
        prefix: &str,
        clock: Clock,
    ) -> io::Result<Self> {
        let now = clock();
        let current_name = file_name_for(prefix, rotation, &now.naive_local());
        let path = dir.join(&current_name);
        let file = open_append(&path)?;

        Ok(Self {
            dir: dir.to_path_buf(),
            prefix: prefix.to_string(),
            rotation: rotation.clone(),
            clock,
            file,
            current_name,
            next_rollover: next_rollover(rotation, &now),
            active_path: Arc::new(Mutex::new(path)),
            rollover_header: None,
            pending_header: None,
            rollover_hook: None,
        })
    }

    /// ファイルを切り替えたときに、新しいファイルの先頭に書く行を設定する。
    /// 起動したときの見出しの行は新しいファイルに無いので、これが無いと
    /// どのプロセス（instance）の記録かを前のファイルまでさかのぼって
    /// 調べる必要がある。
    pub(crate) fn with_rollover_header(mut self, header: RolloverHeader) -> Self {
        self.rollover_header = Some(header);
        self
    }

    /// ファイルを切り替えた後に呼ぶ関数を設定する（#759。起動したときだけで
    /// なく、切り替えるたびに古いログを消す）
    pub(crate) fn with_rollover_hook(mut self, hook: RolloverHook) -> Self {
        self.rollover_hook = Some(hook);
        self
    }

    /// 今書いているファイルの名前（テストで確かめる）
    #[cfg(test)]
    pub(crate) fn current_file_name(&self) -> &str {
        &self.current_name
    }

    /// 今書いているファイルのパス（切り替えに合わせて更新される）
    pub(crate) fn active_path(&self) -> Arc<Mutex<PathBuf>> {
        Arc::clone(&self.active_path)
    }

    /// 切り替えの時刻（`now`）を過ぎていれば、新しいファイルに切り替える。
    /// 切り替えた場合は、新しいファイルの先頭に書く行を返す。
    fn roll_if_needed(&mut self, now: DateTime<Local>) -> Option<String> {
        let next = self.next_rollover?;
        if now < next {
            return None;
        }

        let mut header = None;

        let name = file_name_for(&self.prefix, &self.rotation, &now.naive_local());
        // 同じ形式の名前なので、文字列の比較で日付の前後が分かる。
        // 時計が戻っていた等で、今のファイル以前の名前になる場合は、
        // 切り替えずに今のファイルへ書き続ける。
        if name > self.current_name {
            let path = self.dir.join(&name);
            match open_append(&path) {
                Ok(file) => {
                    header = self.rollover_header.as_ref().map(|h| h(&now, &path));
                    self.file = file;
                    self.current_name = name;
                    *self
                        .active_path
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = path.clone();
                    if let Some(hook) = &self.rollover_hook {
                        hook(&path);
                    }
                }
                Err(_) => {
                    // 開けなければ今のファイルに書き続け、少し後に試し直す
                    self.next_rollover = Some(now + Duration::seconds(RETRY_INTERVAL_SECS));
                    return None;
                }
            }
        }
        self.next_rollover = next_rollover(&self.rotation, &now);
        header
    }
}

impl LocalRollingAppender {
    /// 切り替えの時刻を過ぎていれば、今切り替える（先頭の行は、次の書き込みで
    /// 書く）。書く前に書き先を決め、その書き先の名前を行に入れるため（起動の
    /// 行、PR #745のレビュー）
    ///
    /// 今のファイルの先頭の行がまだ書けていなければ（書き込みが失敗した後）、
    /// 切り替える前に書き終える（その行を新しいファイルの行で置き換えて失わない
    /// ように。PR #745のレビュー）。書けなければ、その行をあきらめて標準エラーに
    /// 知らせ、切り替える（#774。今のファイルが書けない間、新しいファイルへ
    /// 移れなくならないように）
    ///
    /// 時刻は1回だけ読み、切り替えの判定と切り替えの両方に使う（#774。2回
    /// 読むと、その間に境目を越えたとき、残った行を書かずに置き換える）
    pub(crate) fn roll_now(&mut self) {
        let now = (self.clock)();
        let due = self.next_rollover.is_some_and(|next| now >= next);
        if due && self.pending_header.is_some() {
            if let Err(e) = write_with_header(&mut self.file, &mut self.pending_header, b"") {
                eprintln!(
                    "mcp-rs: could not write the log header to {} ({e}): switching to the next log file without it",
                    self.dir.join(&self.current_name).display()
                );
                self.pending_header = None;
            }
        }
        if let Some(header) = self.roll_if_needed(now) {
            self.pending_header = Some(header.into_bytes());
        }
    }

    /// 起動の印のように、書き先のパスを入れた1行を書く（#774）。切り替えは
    /// 1回だけ行い、その書き先に直接書く（`write`を通すと、行を作った後に
    /// もう一度切り替わり、行の`log:`が実際のファイルと違いうる）。書いた
    /// ファイルのパスを返す
    pub(crate) fn write_marker(
        &mut self,
        line: impl FnOnce(&Path) -> String,
    ) -> io::Result<PathBuf> {
        self.roll_now();
        let path = self.dir.join(&self.current_name);
        let line = line(&path);
        write_with_header(&mut self.file, &mut self.pending_header, line.as_bytes())?;
        self.file.flush()?;
        Ok(path)
    }
}

impl Write for LocalRollingAppender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.roll_now();
        write_with_header(&mut self.file, &mut self.pending_header, buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// 1件のログ（`buf`）を1回で書く（途中で分けない）。切り替えた後の先頭の
/// 行（`pending`）があれば、その後に続けて1回で書く（同じファイルに書く
/// 別のプロセスの行と混ざらないように）。
///
/// 先頭の行は、書けた分だけ消す（#744）。失敗したら、まだ書けていない残りを
/// `pending`に残し、次の書き込みで続きから書く（途中まで書いた行を、もう一度
/// 頭から書かない）
fn write_with_header(
    file: &mut impl Write,
    pending: &mut Option<Vec<u8>>,
    buf: &[u8],
) -> io::Result<()> {
    let Some(header) = pending.take() else {
        return file.write_all(buf);
    };
    let header_len = header.len();
    let mut bytes = header;
    bytes.extend_from_slice(buf);
    let mut written = 0;
    while written < bytes.len() {
        let error = match file.write(&bytes[written..]) {
            Ok(0) => io::Error::from(io::ErrorKind::WriteZero),
            Ok(n) => {
                written += n;
                continue;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => e,
        };
        if written < header_len {
            *pending = Some(bytes[written..header_len].to_vec());
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicI64, Ordering};
    use tempfile::tempdir;

    /// テスト用の時計。`set`でローカル時刻を変えられる。
    #[derive(Clone)]
    struct TestClock(Arc<AtomicI64>);

    impl TestClock {
        fn new(t: DateTime<Local>) -> Self {
            Self(Arc::new(AtomicI64::new(t.timestamp_millis())))
        }

        fn set(&self, t: DateTime<Local>) {
            self.0.store(t.timestamp_millis(), Ordering::SeqCst);
        }

        fn boxed(&self) -> Clock {
            let millis = Arc::clone(&self.0);
            Box::new(move || {
                Local
                    .timestamp_millis_opt(millis.load(Ordering::SeqCst))
                    .unwrap()
            })
        }
    }

    /// 夏時間の切り替えを避けた日付（1月）のローカル時刻
    fn local(d: u32, h: u32, m: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 1, d, h, m, 0)
            .single()
            .unwrap()
    }

    fn read(dir: &Path, name: &str) -> String {
        fs::read_to_string(dir.join(name)).unwrap_or_default()
    }

    #[test]
    fn test_file_name_uses_local_date() {
        let t = local(10, 1, 30).naive_local();
        assert_eq!(
            file_name_for("mcp-rs.log", &LogRotation::Daily, &t),
            "mcp-rs.log.2026-01-10"
        );
        assert_eq!(
            file_name_for("mcp-rs.log", &LogRotation::Hourly, &t),
            "mcp-rs.log.2026-01-10-01"
        );
        assert_eq!(
            file_name_for("mcp-rs.log", &LogRotation::Never, &t),
            "mcp-rs.log"
        );
    }

    #[test]
    fn test_next_rollover_is_local_midnight_or_next_hour() {
        let t = local(10, 23, 59);
        assert_eq!(
            next_rollover(&LogRotation::Daily, &t),
            Some(local(11, 0, 0))
        );
        assert_eq!(
            next_rollover(&LogRotation::Hourly, &t),
            Some(local(11, 0, 0))
        );
        assert_eq!(
            next_rollover(&LogRotation::Hourly, &local(10, 8, 15)),
            Some(local(10, 9, 0))
        );
        assert_eq!(next_rollover(&LogRotation::Never, &t), None);
    }

    /// ローカル時刻の0時をまたぐと、次の日付のファイルに切り替わる
    #[test]
    fn test_rolls_over_at_local_midnight() {
        let dir = tempdir().unwrap();
        let clock = TestClock::new(local(10, 23, 59));
        let mut appender = LocalRollingAppender::with_clock(
            dir.path(),
            &LogRotation::Daily,
            "mcp-rs.log",
            clock.boxed(),
        )
        .unwrap();
        let active = appender.active_path();

        appender.write_all(b"before midnight\n").unwrap();
        clock.set(local(11, 0, 0));
        appender.write_all(b"after midnight\n").unwrap();
        appender.flush().unwrap();

        assert_eq!(
            read(dir.path(), "mcp-rs.log.2026-01-10"),
            "before midnight\n"
        );
        assert_eq!(
            read(dir.path(), "mcp-rs.log.2026-01-11"),
            "after midnight\n"
        );
        assert_eq!(appender.current_file_name(), "mcp-rs.log.2026-01-11");
        assert_eq!(
            *active.lock().unwrap(),
            dir.path().join("mcp-rs.log.2026-01-11")
        );
    }

    /// 境界の前は切り替えない（日本時間の9:00等、UTCの境界では切り替えない）
    #[test]
    fn test_does_not_roll_over_before_local_midnight() {
        let dir = tempdir().unwrap();
        let clock = TestClock::new(local(10, 0, 0));
        let mut appender = LocalRollingAppender::with_clock(
            dir.path(),
            &LogRotation::Daily,
            "mcp-rs.log",
            clock.boxed(),
        )
        .unwrap();

        for h in [8, 9, 12, 23] {
            clock.set(local(10, h, 0));
            appender.write_all(format!("{}\n", h).as_bytes()).unwrap();
        }

        assert_eq!(read(dir.path(), "mcp-rs.log.2026-01-10"), "8\n9\n12\n23\n");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    /// 時計が前の日に戻っても、前の日のファイルには戻らない
    #[test]
    fn test_keeps_current_file_when_clock_goes_back() {
        let dir = tempdir().unwrap();
        let clock = TestClock::new(local(10, 23, 59));
        let mut appender = LocalRollingAppender::with_clock(
            dir.path(),
            &LogRotation::Daily,
            "mcp-rs.log",
            clock.boxed(),
        )
        .unwrap();

        clock.set(local(11, 0, 1));
        appender.write_all(b"a\n").unwrap();
        // 時計が前の日に戻る（NTPの補正等）
        clock.set(local(10, 23, 58));
        appender.write_all(b"b\n").unwrap();
        // 戻った時計がもう一度0時を過ぎても、同じファイルのまま
        clock.set(local(11, 0, 2));
        appender.write_all(b"c\n").unwrap();

        assert_eq!(read(dir.path(), "mcp-rs.log.2026-01-10"), "");
        assert_eq!(read(dir.path(), "mcp-rs.log.2026-01-11"), "a\nb\nc\n");
        assert_eq!(appender.current_file_name(), "mcp-rs.log.2026-01-11");
    }

    /// 既にあるファイルには追記する（同じファイルに書く別のプロセスや、
    /// 再起動の前の内容を消さない）
    #[test]
    fn test_appends_to_existing_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("mcp-rs.log.2026-01-10"), "existing\n").unwrap();
        let clock = TestClock::new(local(10, 12, 0));
        let mut appender = LocalRollingAppender::with_clock(
            dir.path(),
            &LogRotation::Daily,
            "mcp-rs.log",
            clock.boxed(),
        )
        .unwrap();

        appender.write_all(b"new\n").unwrap();

        assert_eq!(read(dir.path(), "mcp-rs.log.2026-01-10"), "existing\nnew\n");
    }

    /// 新しいログのファイルは600、リンクはたどらない（#543）
    #[cfg(unix)]
    #[test]
    fn test_log_file_is_private_and_not_followed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.log");
        open_append(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        let target = dir.path().join("target.txt");
        std::fs::write(&target, "x").unwrap();
        let link = dir.path().join("link.log");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(open_append(&link).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "x");
    }

    #[test]
    fn test_new_fails_when_dir_is_not_a_directory() {
        let dir = tempdir().unwrap();
        let not_a_dir = dir.path().join("file");
        fs::write(&not_a_dir, "x").unwrap();

        assert!(LocalRollingAppender::new(&not_a_dir, &LogRotation::Daily, "mcp-rs.log").is_err());
    }

    fn test_header() -> RolloverHeader {
        Box::new(|at, path| {
            format!(
                "=== continued at {} / log: {} ===\n",
                at.format("%Y-%m-%dT%H:%M"),
                path.file_name().unwrap().to_string_lossy()
            )
        })
    }

    /// 0時をまたいで切り替えると、新しいファイルの先頭に見出しの行が入り、
    /// その後に切り替えのきっかけになった1件のログが続く（#472）
    #[test]
    fn test_writes_header_at_top_of_new_file_on_rollover() {
        let dir = tempdir().unwrap();
        let clock = TestClock::new(local(10, 23, 59));
        let mut appender = LocalRollingAppender::with_clock(
            dir.path(),
            &LogRotation::Daily,
            "mcp-rs.log",
            clock.boxed(),
        )
        .unwrap()
        .with_rollover_header(test_header());

        appender.write_all(b"before\n").unwrap();
        clock.set(local(11, 0, 5));
        appender.write_all(b"after 1\n").unwrap();
        appender.write_all(b"after 2\n").unwrap();

        // 前のファイルには見出しの行を書かない
        assert_eq!(read(dir.path(), "mcp-rs.log.2026-01-10"), "before\n");
        assert_eq!(
            read(dir.path(), "mcp-rs.log.2026-01-11"),
            "=== continued at 2026-01-11T00:05 / log: mcp-rs.log.2026-01-11 ===\nafter 1\nafter 2\n"
        );
    }

    /// 今のファイルの先頭の行が書けていないまま次の切り替えの時刻になったら、
    /// その行を今のファイルに書き終えてから切り替える（PR #745のレビュー）。
    /// 新しいファイルの行で置き換えて失わない
    #[test]
    fn test_pending_header_is_written_before_the_next_rollover() {
        let dir = tempdir().unwrap();
        let clock = TestClock::new(local(10, 23, 59));
        let mut appender = LocalRollingAppender::with_clock(
            dir.path(),
            &LogRotation::Daily,
            "mcp-rs.log",
            clock.boxed(),
        )
        .unwrap()
        .with_rollover_header(test_header());
        // 10日のファイルの先頭の行が、書き込みの失敗で残っている状態
        appender.pending_header = Some(b"=== pending for the 10th ===\n".to_vec());
        clock.set(local(11, 0, 5));
        appender.write_all(b"after\n").unwrap();

        assert_eq!(
            read(dir.path(), "mcp-rs.log.2026-01-10"),
            "=== pending for the 10th ===\n"
        );
        assert_eq!(
            read(dir.path(), "mcp-rs.log.2026-01-11"),
            "=== continued at 2026-01-11T00:05 / log: mcp-rs.log.2026-01-11 ===\nafter\n"
        );
        assert!(appender.pending_header.is_none());
    }

    /// 今のファイルの先頭の行が書けないまま切り替えの時刻になったら、その行を
    /// あきらめて新しいファイルに切り替える（#774。書けないファイルに
    /// 留まり続けない）
    #[test]
    fn test_switches_even_if_pending_header_cannot_be_written() {
        let dir = tempdir().unwrap();
        let clock = TestClock::new(local(10, 23, 59));
        let mut appender = LocalRollingAppender::with_clock(
            dir.path(),
            &LogRotation::Daily,
            "mcp-rs.log",
            clock.boxed(),
        )
        .unwrap()
        .with_rollover_header(test_header());
        // 10日のファイルが書けなくなり（読み取りだけのハンドル）、先頭の行が
        // 残っている状態
        appender.file = File::open(dir.path().join("mcp-rs.log.2026-01-10")).unwrap();
        appender.pending_header = Some(b"=== pending for the 10th ===\n".to_vec());
        clock.set(local(11, 0, 5));
        appender.write_all(b"after\n").unwrap();

        assert_eq!(read(dir.path(), "mcp-rs.log.2026-01-10"), "");
        assert_eq!(
            read(dir.path(), "mcp-rs.log.2026-01-11"),
            "=== continued at 2026-01-11T00:05 / log: mcp-rs.log.2026-01-11 ===\nafter\n"
        );
        assert_eq!(appender.current_file_name(), "mcp-rs.log.2026-01-11");
    }

    /// 呼ぶたびに`times`の次の時刻を返す時計（最後の時刻の後は、最後のまま）
    fn advancing_clock(times: Vec<DateTime<Local>>) -> Clock {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        Box::new(move || {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            times[n.min(times.len() - 1)]
        })
    }

    /// 切り替えの判定と切り替えで、同じ時刻を使う（#774）。判定の後に境目を
    /// 越えても、残った先頭の行を書かずに置き換えない
    #[test]
    fn test_roll_now_reads_the_clock_once() {
        let dir = tempdir().unwrap();
        let mut appender = LocalRollingAppender::with_clock(
            dir.path(),
            &LogRotation::Daily,
            "mcp-rs.log",
            advancing_clock(vec![local(10, 23, 58), local(10, 23, 59), local(11, 0, 5)]),
        )
        .unwrap()
        .with_rollover_header(test_header());
        appender.pending_header = Some(b"=== pending for the 10th ===\n".to_vec());
        appender.write_all(b"a\n").unwrap();
        appender.write_all(b"b\n").unwrap();

        assert_eq!(
            read(dir.path(), "mcp-rs.log.2026-01-10"),
            "=== pending for the 10th ===\na\n"
        );
        assert_eq!(
            read(dir.path(), "mcp-rs.log.2026-01-11"),
            "=== continued at 2026-01-11T00:05 / log: mcp-rs.log.2026-01-11 ===\nb\n"
        );
    }

    /// 起動の印の`log:`は、実際に書いたファイルと同じ（#774。行を作った後に
    /// 境目を越えても、もう一度切り替えない）
    #[test]
    fn test_write_marker_writes_to_the_path_in_the_line() {
        let dir = tempdir().unwrap();
        let mut appender = LocalRollingAppender::with_clock(
            dir.path(),
            &LogRotation::Daily,
            "mcp-rs.log",
            advancing_clock(vec![local(10, 23, 58), local(10, 23, 59), local(11, 0, 5)]),
        )
        .unwrap();
        let written = appender
            .write_marker(|path| format!("log: {}\n", path.file_name().unwrap().to_string_lossy()))
            .unwrap();

        assert_eq!(written, dir.path().join("mcp-rs.log.2026-01-10"));
        assert_eq!(
            read(dir.path(), "mcp-rs.log.2026-01-10"),
            "log: mcp-rs.log.2026-01-10\n"
        );
        assert_eq!(read(dir.path(), "mcp-rs.log.2026-01-11"), "");
    }

    /// 夏時間で時刻が飛んだら、飛んだ後の最初の時刻で切り替える（#744）。
    /// 30分だけ飛ぶ地域（02:00〜02:29が無い）でも、03:00ではなく02:30
    #[test]
    fn test_first_existing_time_after_a_gap() {
        let at = |h: u32, m: u32| {
            chrono::NaiveDate::from_ymd_opt(2026, 10, 4)
                .unwrap()
                .and_hms_opt(h, m, 0)
                .unwrap()
        };
        let skip = |from: NaiveDateTime, to: NaiveDateTime| {
            move |time: NaiveDateTime| (time < from || time >= to).then_some(time)
        };
        assert_eq!(
            first_existing_time(at(2, 0), skip(at(2, 0), at(2, 30))),
            Some(at(2, 30))
        );
        assert_eq!(
            first_existing_time(at(2, 0), skip(at(2, 0), at(3, 0))),
            Some(at(3, 0))
        );
        // 時刻が飛ばない普通の日は、そのまま
        assert_eq!(first_existing_time(at(2, 0), Some), Some(at(2, 0)));
        assert_eq!(
            first_existing_time(at(2, 0), |_| None::<NaiveDateTime>),
            None
        );
    }

    /// 書き込みが失敗しても、先頭の行は失わず、次に書けたときに書く（#744）
    #[test]
    fn test_pending_header_survives_a_failed_write() {
        /// `accept`バイトまで書き、それを超えると失敗する書き先（`None`は
        /// いくらでも書ける）
        struct Flaky {
            accept: Option<usize>,
            written: Vec<u8>,
        }
        impl Write for Flaky {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                let n = match self.accept {
                    Some(0) => return Err(io::Error::other("disk full")),
                    Some(left) => left.min(buf.len()),
                    None => buf.len(),
                };
                if let Some(left) = &mut self.accept {
                    *left -= n;
                }
                self.written.extend_from_slice(&buf[..n]);
                Ok(n)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let header = || Some(b"=== continued ===\n".to_vec());

        // 何も書けずに失敗: 先頭の行を全部残す
        let mut file = Flaky {
            accept: Some(0),
            written: Vec::new(),
        };
        let mut pending = header();
        assert!(write_with_header(&mut file, &mut pending, b"lost\n").is_err());
        assert_eq!(pending, header());
        file.accept = None;
        write_with_header(&mut file, &mut pending, b"next\n").unwrap();
        assert!(pending.is_none());
        write_with_header(&mut file, &mut pending, b"more\n").unwrap();
        assert_eq!(file.written, b"=== continued ===\nnext\nmore\n");

        // 先頭の行の途中で失敗: 残りから書き、頭から書き直さない（PR #745の
        // レビュー）
        let mut file = Flaky {
            accept: Some(4),
            written: Vec::new(),
        };
        let mut pending = header();
        assert!(write_with_header(&mut file, &mut pending, b"lost\n").is_err());
        assert_eq!(pending.as_deref(), Some(&b"continued ===\n"[..]));
        file.accept = None;
        write_with_header(&mut file, &mut pending, b"next\n").unwrap();
        assert_eq!(file.written, b"=== continued ===\nnext\n");

        // 先頭の行を書き終え、ログの途中で失敗: 先頭の行は残さない
        let mut file = Flaky {
            accept: Some(20),
            written: Vec::new(),
        };
        let mut pending = header();
        assert!(write_with_header(&mut file, &mut pending, b"partial\n").is_err());
        assert!(pending.is_none());
    }

    /// 切り替えたときだけ、新しいファイルのパスで切り替えの後の関数を呼ぶ
    /// （#759。古いログを消す）
    #[test]
    fn test_rollover_hook_is_called_only_when_switching() {
        let dir = tempdir().unwrap();
        let clock = TestClock::new(local(10, 23, 59));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&calls);
        let mut appender = LocalRollingAppender::with_clock(
            dir.path(),
            &LogRotation::Daily,
            "mcp-rs.log",
            clock.boxed(),
        )
        .unwrap()
        .with_rollover_hook(Box::new(move |path| {
            recorded.lock().unwrap().push(path.to_path_buf())
        }));

        appender.write_all(b"a\n").unwrap();
        assert!(calls.lock().unwrap().is_empty());
        clock.set(local(11, 0, 5));
        appender.write_all(b"b\n").unwrap();
        appender.write_all(b"c\n").unwrap();

        assert_eq!(
            *calls.lock().unwrap(),
            vec![dir.path().join("mcp-rs.log.2026-01-11")]
        );
    }

    /// このwriterが作る名前の形だけを、ログのファイルとして扱う（#759）
    #[test]
    fn test_is_file_name_of_matches_only_generated_names() {
        let prefix = "mcp-rs.log";
        for name in [
            "mcp-rs.log",
            "mcp-rs.log.2026-01-10",
            "mcp-rs.log.2026-01-10-23",
        ] {
            assert!(is_file_name_of(prefix, name), "{name}");
        }
        for name in [
            "mcp-rs-backup.log",
            "mcp-rs.log.bak",
            "mcp-rs.log.2026-01-10.bak",
            "mcp-rs.log.2026-01-1x",
            "mcp-rs.log.2026-01-10-2",
            "mcp-rs.log2026-01-10",
            "mcp-rs.2026-01-10.log",
            "other.log.2026-01-10",
        ] {
            assert!(!is_file_name_of(prefix, name), "{name}");
        }
    }

    /// 切り替えの時刻は過ぎたが、時計が戻っていて今のファイル以前の名前に
    /// なる場合は、切り替えず、見出しの行も書かない
    #[test]
    fn test_does_not_write_header_when_name_is_not_newer() {
        let dir = tempdir().unwrap();
        let clock = TestClock::new(local(11, 0, 1));
        let mut appender = LocalRollingAppender::with_clock(
            dir.path(),
            &LogRotation::Daily,
            "mcp-rs.log",
            clock.boxed(),
        )
        .unwrap()
        .with_rollover_header(test_header());
        appender.write_all(b"a\n").unwrap();

        // 切り替えの時刻を過去にしておき、時計を前の日に戻す
        appender.next_rollover = Some(local(10, 23, 0));
        clock.set(local(10, 23, 30));
        appender.write_all(b"b\n").unwrap();

        assert_eq!(read(dir.path(), "mcp-rs.log.2026-01-11"), "a\nb\n");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(appender.current_file_name(), "mcp-rs.log.2026-01-11");
    }

    /// 新しいファイルを開けなかった場合は、今のファイルに書き続け、
    /// 見出しの行は書かない
    #[test]
    fn test_does_not_write_header_when_new_file_cannot_be_opened() {
        let dir = tempdir().unwrap();
        // 次の日のファイルと同じ名前のディレクトリを作り、開けないようにする
        fs::create_dir(dir.path().join("mcp-rs.log.2026-01-11")).unwrap();
        let clock = TestClock::new(local(10, 23, 59));
        let mut appender = LocalRollingAppender::with_clock(
            dir.path(),
            &LogRotation::Daily,
            "mcp-rs.log",
            clock.boxed(),
        )
        .unwrap()
        .with_rollover_header(test_header());

        appender.write_all(b"a\n").unwrap();
        clock.set(local(11, 0, 5));
        appender.write_all(b"b\n").unwrap();

        assert_eq!(read(dir.path(), "mcp-rs.log.2026-01-10"), "a\nb\n");
        assert_eq!(appender.current_file_name(), "mcp-rs.log.2026-01-10");
        // 少し後に試し直す
        assert_eq!(
            appender.next_rollover,
            Some(local(11, 0, 5) + Duration::seconds(RETRY_INTERVAL_SECS))
        );
    }
}
