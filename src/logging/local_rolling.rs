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

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

type Clock = Box<dyn Fn() -> DateTime<Local> + Send>;

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
        })
    }

    /// 今書いているファイルの名前
    pub(crate) fn current_file_name(&self) -> &str {
        &self.current_name
    }

    /// 今書いているファイルのパス（切り替えに合わせて更新される）
    pub(crate) fn active_path(&self) -> Arc<Mutex<PathBuf>> {
        Arc::clone(&self.active_path)
    }

    fn roll_if_needed(&mut self) {
        let Some(next) = self.next_rollover else {
            return;
        };
        let now = (self.clock)();
        if now < next {
            return;
        }

        let name = file_name_for(&self.prefix, &self.rotation, &now);
        // 同じ形式の名前なので、文字列の比較で日付の前後が分かる。
        // 時計が戻っていた等で、今のファイル以前の名前になる場合は、
        // 切り替えずに今のファイルへ書き続ける。
        if name > self.current_name {
            let path = self.dir.join(&name);
            match open_append(&path) {
                Ok(file) => {
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
                    return;
                }
            }
        }
        self.next_rollover = next_rollover(&self.rotation, &now);
    }
}

impl Write for LocalRollingAppender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.roll_if_needed();
        // 1件のログを1回で書く（途中で分けない）
        self.file.write_all(buf)?;
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

    #[test]
    fn test_new_fails_when_dir_is_not_a_directory() {
        let dir = tempdir().unwrap();
        let not_a_dir = dir.path().join("file");
        fs::write(&not_a_dir, "x").unwrap();

        assert!(LocalRollingAppender::new(&not_a_dir, &LogRotation::Daily, "mcp-rs.log").is_err());
    }
}
