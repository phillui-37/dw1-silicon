//! Where an app run's messages go (Phase 9.2). Opened from Finder the app has no terminal, so
//! when it runs from a `.app` bundle (or with `DW1_LOG=1`) everything it prints goes to
//! `~/Library/Logs/dw1-silicon/dw1-<UTC time>.log` (the newest `KEEP` are kept), the crash trace
//! is on, and in the window build a stop shows a message box naming the log (the PS1 layer's
//! `trap::stop`, a panic, `fatal`). A crash can't show anything, so a window run leaves a
//! `running-<log name>` marker that any orderly exit removes (an `atexit` handler). A run holds
//! a lock on its log while it lasts, so the next start knows a marker whose log is unlocked
//! is a run that ended without exiting (an abort, a signal), says so, and only then removes the
//! marker. Headless runs only log.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Logs kept.
const KEEP: usize = 10;

/// This run's log, when there is one, and the file (kept open and locked while the run lasts).
static LOG: OnceLock<PathBuf> = OnceLock::new();
static LOG_FILE: OnceLock<std::fs::File> = OnceLock::new();
/// An earlier crash's log, held locked until it has been reported (another copy starting
/// meanwhile leaves it alone), and its marker, removed once reported.
static RECOVERED: std::sync::Mutex<Option<(std::fs::File, PathBuf)>> = std::sync::Mutex::new(None);
/// The window build: stops are shown, and the run has a marker.
static INTERACTIVE: AtomicBool = AtomicBool::new(false);

/// Running from a `.app` bundle.
pub fn bundled() -> bool {
    std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
}

fn log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    return std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Logs/dw1-silicon"));
    #[cfg(not(target_os = "macos"))]
    return dirs::data_local_dir().map(|d| d.join("dw1-silicon/logs"));
}

/// Starts logging when bundled (or `DW1_LOG=1`); `interactive`: the window build (message
/// boxes, a marker). Returns the log of an earlier window run that ended without an orderly
/// exit, if any (for `report_earlier_crash`).
pub fn start(interactive: bool) -> Option<PathBuf> {
    let wanted =
        bundled() || std::env::var_os("DW1_LOG").is_some_and(|v| !v.is_empty() && v != "0");
    if !wanted {
        return None;
    }
    // The message boxes first: they matter most when no log can be written.
    if interactive {
        INTERACTIVE.store(true, Ordering::Relaxed);
    }
    psyq::trap::set_stop_hook(|message| show_stop(message));
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        show_stop(&info.to_string());
    }));
    let dir = log_dir()?;
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("dw1: no log ({}): {e}", dir.display());
        return None;
    }
    // One start at a time from here to the pruning: finding a crash and locking its log,
    // making and locking this run's log, and removing old ones never interleave between copies.
    let folder = std::fs::File::create(dir.join(".lock"));
    if let Ok(f) = &folder {
        let _ = f.lock();
    }
    let crashed = if interactive {
        earlier_crash(&dir)
    } else {
        None
    };
    let crashed = crashed.map(|(log, marker, file)| {
        *RECOVERED.lock().unwrap() = Some((file, marker));
        log
    });
    // the pid too: two starts in one second each keep their own log
    let path = dir.join(format!(
        "dw1-{}-{}.log",
        utc_stamp(std::time::SystemTime::now()),
        std::process::id()
    ));
    let file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("dw1: no log ({}): {e}", path.display());
            return crashed;
        }
    };
    // held for the whole run: `prune` leaves a locked log alone (a run still writing it)
    let _ = file.try_lock();
    redirect(&file);
    let _ = LOG_FILE.set(file);
    let _ = LOG.set(path.clone());
    if interactive {
        let _ = std::fs::write(marker_for(&path), format!("{}\n", path.display()));
        unsafe extern "C" {
            fn atexit(f: extern "C" fn()) -> i32;
        }
        // SAFETY: registers a plain function; it runs once at an orderly exit.
        unsafe { atexit(clean_exit_at_exit) };
    }
    // (the crashed run's log too: it is reported after this)
    prune(&dir, KEEP, crashed.as_deref());
    drop(folder);
    crashed
}

/// Stdout and stderr to `f` (the descriptors, so C code and the crash trace write there too).
#[cfg(unix)]
fn redirect(f: &std::fs::File) {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn dup2(from: i32, to: i32) -> i32;
    }
    // SAFETY: `f` is open; 1 and 2 are replaced by copies of it for the rest of the run.
    unsafe {
        dup2(f.as_raw_fd(), 1);
        dup2(f.as_raw_fd(), 2);
    }
}

/// Same through the UCRT: the file's handle becomes a CRT descriptor, which then replaces
/// stdout (1) and stderr (2), so C `printf` writes to the log as on macOS.
#[cfg(windows)]
fn redirect(f: &std::fs::File) {
    use std::os::windows::io::AsRawHandle;
    unsafe extern "C" {
        fn _open_osfhandle(osfhandle: isize, flags: i32) -> i32;
        fn _dup2(from: i32, to: i32) -> i32;
    }
    const O_WRONLY: i32 = 1;
    // SAFETY: `f` is open for writing; its handle backs a descriptor that replaces 1 and 2
    // for the rest of the run (the descriptor is never closed, like the dup above).
    unsafe {
        let fd = _open_osfhandle(f.as_raw_handle() as isize, O_WRONLY);
        _dup2(fd, 1);
        _dup2(fd, 2);
    }
}

/// A run's marker: `running-<its log's name>` beside the log.
fn marker_for(log: &Path) -> PathBuf {
    let name = log
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    log.with_file_name(format!("running-{name}"))
}

/// An earlier window run that ended without exiting: a marker whose log nobody holds locked.
/// Returns the newest such log, its marker and the log opened and locked (held until it's
/// reported, so another copy starting meanwhile treats it as in use). Older ones' markers and
/// markers of logs that are gone are removed. Called with the folder's `.lock` held.
fn earlier_crash(dir: &Path) -> Option<(PathBuf, PathBuf, std::fs::File)> {
    let mut found: Option<(PathBuf, PathBuf, std::fs::File)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("running-") {
            continue;
        }
        let marker = entry.path();
        let Some(log) = std::fs::read_to_string(&marker)
            .ok()
            .map(|t| PathBuf::from(t.trim()))
            .filter(|l| l.exists())
        else {
            let _ = std::fs::remove_file(&marker);
            continue;
        };
        let Ok(file) = std::fs::File::open(&log) else {
            continue;
        };
        if file.try_lock().is_err() {
            // its run is still going (or another start is reporting it)
            continue;
        }
        match &found {
            Some((newest, _, _)) if *newest >= log => {
                let _ = std::fs::remove_file(&marker);
            }
            _ => {
                if let Some((_, older, _)) = found.take() {
                    let _ = std::fs::remove_file(older);
                }
                found = Some((log, marker, file));
            }
        }
    }
    found
}

/// Removes all but the newest `keep` logs (`dw1-*.log`; their names sort by time), never one a
/// run's marker names (a crash not reported yet), `also` (one being reported), or one a run
/// still holds locked (it is writing it). Called with the folder's `.lock` held.
fn prune(dir: &Path, keep: usize, also: Option<&Path>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let marked: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("running-"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .map(|t| PathBuf::from(t.trim()))
        .collect();
    let mut logs: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with("dw1-") && n.ends_with(".log")
            })
        })
        .collect();
    logs.sort();
    let n = logs.len().saturating_sub(keep);
    let in_use = |l: &Path| {
        std::fs::File::open(l)
            .is_ok_and(|f| matches!(f.try_lock(), Err(std::fs::TryLockError::WouldBlock)))
    };
    for old in logs[..n]
        .iter()
        .filter(|l| !marked.contains(l) && Some(l.as_path()) != also && !in_use(l))
    {
        let _ = std::fs::remove_file(old);
    }
}

extern "C" fn clean_exit_at_exit() {
    clean_exit();
}

/// An orderly exit: this run's marker goes.
pub fn clean_exit() {
    if INTERACTIVE.load(Ordering::Relaxed)
        && let Some(log) = LOG.get()
    {
        let _ = std::fs::remove_file(marker_for(log));
    }
}

/// The log's place, for messages.
fn log_note() -> String {
    match LOG.get() {
        Some(p) => format!("\n\nWhat happened is written in {}.", p.display()),
        None => String::new(),
    }
}

/// Whether this is the main thread: macOS shows windows (a message box too) only from there.
/// A stop elsewhere is in the log, and the next start reports it.
#[cfg(target_os = "macos")]
fn on_main_thread() -> bool {
    unsafe extern "C" {
        fn pthread_main_np() -> i32;
    }
    // SAFETY: no arguments, no state.
    unsafe { pthread_main_np() != 0 }
}

/// Windows has no `pthread_main_np`; the main thread's id is captured at startup (`start`
/// runs first on the main thread) and compared against the current one.
#[cfg(windows)]
fn on_main_thread() -> bool {
    use std::sync::atomic::{AtomicU32, Ordering};
    static MAIN_TID: AtomicU32 = AtomicU32::new(0);
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    let current = unsafe { GetCurrentThreadId() };
    let seen = MAIN_TID.load(Ordering::Relaxed);
    if seen == 0 {
        // The first caller is on the main thread: `start` ran there before anything else.
        MAIN_TID.store(current, Ordering::Relaxed);
        return true;
    }
    current == seen
}

/// The message box for a stop (when logging: an app run, on the main thread), then back to
/// the caller, which ends the program.
fn show_stop(message: &str) {
    if !INTERACTIVE.load(Ordering::Relaxed) || !on_main_thread() {
        return;
    }
    let text = format!(
        "The game stopped: the Mac port met something it doesn't handle yet.\n\n{message}{}\n\n\
         Your saves up to the last one are safe. Please keep the log for a bug report.",
        log_note()
    );
    let _ = sdl3::messagebox::show_simple_message_box(
        sdl3::messagebox::MessageBoxFlag::ERROR,
        "Digimon World stopped",
        &text,
        None,
    );
}

/// Stops the program with `message`: printed, and in an app run shown in a message box.
pub fn fatal(message: &str) -> ! {
    eprintln!("dw1: {message}");
    if INTERACTIVE.load(Ordering::Relaxed) && on_main_thread() {
        let _ = sdl3::messagebox::show_simple_message_box(
            sdl3::messagebox::MessageBoxFlag::ERROR,
            "Digimon World can't start",
            &format!("{message}{}", log_note()),
            None,
        );
    }
    clean_exit();
    std::process::exit(1);
}

/// Says that an earlier run ended without a clean quit (a crash), naming its log.
pub fn report_earlier_crash(log: &Path) {
    eprintln!(
        "dw1: the last run ended without quitting; its log is {}",
        log.display()
    );
    let _ = sdl3::messagebox::show_simple_message_box(
        sdl3::messagebox::MessageBoxFlag::WARNING,
        "Digimon World closed unexpectedly last time",
        &format!(
            "The last time it ran, the game closed without quitting (a crash). What it wrote is \
             in {}.\n\nPlease keep that file for a bug report.",
            log.display()
        ),
        None,
    );
    // reported: its marker goes, and it can be pruned like any other log now
    if let Some((_, marker)) = RECOVERED.lock().unwrap().take() {
        let _ = std::fs::remove_file(marker);
    }
}

/// This run's log, if it has one (app runs).
pub fn log_path() -> Option<&'static Path> {
    LOG.get().map(PathBuf::as_path)
}

/// `YYYYMMDD-HHMMSS` in UTC.
pub fn utc_stamp(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // days since 1970-01-01 to a civil date (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_are_utc_and_sort_by_time() {
        let at = |s: u64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(s);
        assert_eq!(utc_stamp(at(0)), "19700101-000000");
        assert_eq!(utc_stamp(at(951_782_400)), "20000229-000000");
        assert_eq!(utc_stamp(at(1_791_936_000 + 3_723)), "20261014-010203");
    }

    #[test]
    fn a_log_a_run_still_names_is_kept() {
        let dir = std::env::temp_dir().join(format!("dw1-applog-keep-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..4 {
            std::fs::write(dir.join(format!("dw1-2026101{i}-000000-1.log")), "x").unwrap();
        }
        let oldest = dir.join("dw1-20261010-000000-1.log");
        std::fs::write(dir.join("running-1"), format!("{}\n", oldest.display())).unwrap();
        // one a run is writing (locked), and one being reported
        let busy = dir.join("dw1-20261011-000000-1.log");
        let held = std::fs::File::open(&busy).unwrap();
        held.try_lock().unwrap();
        let reported = dir.join("dw1-20261012-000000-1.log");
        prune(&dir, 1, Some(&reported));
        assert!(oldest.exists());
        assert!(busy.exists());
        assert!(reported.exists());
        assert!(dir.join("dw1-20261013-000000-1.log").exists());
        drop(held);
        prune(&dir, 1, None);
        assert!(!busy.exists());
        assert!(!reported.exists());
        assert!(oldest.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn old_logs_go_and_dead_runs_are_found() {
        let dir = std::env::temp_dir().join(format!("dw1-applog-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..5 {
            std::fs::write(dir.join(format!("dw1-2026101{i}-000000.log")), "x").unwrap();
        }
        std::fs::write(dir.join("notes.txt"), "keep").unwrap();
        prune(&dir, 3, None);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "dw1-20261012-000000.log",
                "dw1-20261013-000000.log",
                "dw1-20261014-000000.log",
                "notes.txt"
            ]
        );
        // a marker whose log nobody holds is a crash; one whose log is held is a run going on
        let crashed = dir.join("dw1-20261013-000000-7.log");
        let going = dir.join("dw1-20261014-000000-8.log");
        std::fs::write(marker_for(&crashed), format!("{}\n", crashed.display())).unwrap();
        std::fs::write(marker_for(&going), format!("{}\n", going.display())).unwrap();
        std::fs::write(&crashed, "x").unwrap();
        std::fs::write(&going, "x").unwrap();
        let held = std::fs::File::open(&going).unwrap();
        held.try_lock().unwrap();
        // and a marker whose log is gone is dropped
        std::fs::write(dir.join("running-gone.log"), "/nowhere/gone.log\n").unwrap();
        let (log, marker, lock) = earlier_crash(&dir).unwrap();
        assert_eq!(
            (log.as_path(), marker.as_path()),
            (crashed.as_path(), marker_for(&crashed).as_path())
        );
        assert!(!dir.join("running-gone.log").exists());
        // the marker stays until reported; while its log is held, another start doesn't take it
        assert!(marker.exists());
        assert!(earlier_crash(&dir).is_none());
        drop(lock);
        assert!(earlier_crash(&dir).is_some());
        assert!(marker_for(&going).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
