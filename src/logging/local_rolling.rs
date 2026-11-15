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
pub(crate) fn file_name_for(prefix: &str, rotation: &LogRotation, now: &DateTime<Local>) -> String {
    match rotation {
        LogRotation::Never => prefix.to_string(),
        LogRotation::Daily => format!("{}.{}", prefix, now.format("%Y-%m-%d")),
        LogRotation::Hourly => format!("{}.{}", prefix, now.format("%Y-%m-%d-%H")),
    }
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
/// 存在する次の正時を使う。
fn first_existing_local_time(start: NaiveDateTime) -> Option<DateTime<Local>> {
    (0..48).find_map(|h| {
        Local
            .from_local_datetime(&(start + Duration::hours(h)))
            .earliest()
    })
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
        let current_name = file_name_for(prefix, rotation, &now);
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

    /// 今書いているファイルの名前
    pub(crate) fn current_file_name(&self) -> &str {
        &self.current_name
    }

    /// 今書いているファイルのパス（切り替えに合わせて更新される）
    pub(crate) fn active_path(&self) -> Arc<Mutex<PathBuf>> {
        Arc::clone(&self.active_path)
    }

    /// 切り替えの時刻を過ぎていれば、新しいファイルに切り替える。
    /// 切り替えた場合は、新しいファイルの先頭に書く行を返す。
    fn roll_if_needed(&mut self) -> Option<String> {
        let next = self.next_rollover?;
        let now = (self.clock)();
        if now < next {
            return None;
        }

        let mut header = None;

        let name = file_name_for(&self.prefix, &self.rotation, &now);
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
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = path;
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

impl Write for LocalRollingAppender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.roll_if_needed() {
            // 切り替えたときは、先頭の行とこの1件のログを1回で書く
            // （同じファイルに書く別のプロセスの行と混ざらないように）
            Some(header) => {
                let mut bytes = header.into_bytes();
                bytes.extend_from_slice(buf);
                self.file.write_all(&bytes)?;
            }
            // 1件のログを1回で書く（途中で分けない）
            None => self.file.write_all(buf)?,
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
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
        let t = local(10, 1, 30);
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
