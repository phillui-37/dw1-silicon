//! Digimon World as a native Apple Silicon app.
//!
//! For now (Phase 2) the whole game C is linked and its `main()` runs until the first SDK call
//! with no Rust implementation, which stops the program with that call's name.

use std::path::PathBuf;

mod applog;
mod canon;
mod disc;
mod headless;
mod pc_text;
mod replay;
mod saves;
mod settings;
mod window;

/// The disc for headless runs and the card commands: `DW1_DISC`, else
/// `<workspace>/disc/Digimon World (USA).cue`. (The window build finds it with `disc`.)
fn open_disc() -> Result<pack::Disc, String> {
    let path = std::env::var_os("DW1_DISC")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(disc::repo_image);
    if !path.exists() {
        return Err(format!(
            "no disc at {}: set DW1_DISC to the .cue of your Digimon World (USA) image",
            path.display()
        ));
    }
    disc::open_checked(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// `DW1_CRASH_TRACE=1`: on SIGSEGV/SIGBUS print the call stack (symbols) to stderr. lldb can't
/// attach in the agent sandbox, and the game's crashes are otherwise silent (exit 139).
///
/// The handler runs on its own stack (so a stack overflow is reported too) and resets to the
/// default action on entry (so a fault inside it ends the process). It walks the frame-pointer
/// chain of the interrupted code from the registers the kernel saved (arm64 macOS keeps frame
/// pointers), so it doesn't depend on the system's stack walker, which finds nothing from an
/// alternate stack on older macOS. Output uses only `write`, `backtrace_symbols_fd` (writes to
/// the descriptor without allocating) and `_exit`.
#[cfg(target_os = "macos")]
fn install_crash_trace() {
    use std::ffi::c_void;
    #[repr(C)]
    struct SigAction {
        handler: extern "C" fn(i32, *mut c_void, *mut c_void),
        mask: u32,
        flags: i32,
    }
    #[repr(C)]
    struct StackT {
        sp: *mut c_void,
        size: usize,
        flags: i32,
    }
    // <sys/signal.h>
    const SA_ONSTACK: i32 = 0x0001;
    const SA_RESETHAND: i32 = 0x0004;
    const SA_SIGINFO: i32 = 0x0040;
    const SIGBUS: i32 = 10;
    const SIGSEGV: i32 = 11;
    // <sys/_types/_ucontext.h>, <mach/arm/_structs.h> (arm64): `uc_mcontext` is the pointer at
    // byte 48 of the ucontext; in the mcontext, the 16-byte exception state is followed by the
    // thread state x0-x28, then fp, lr, sp, pc.
    const UC_MCONTEXT: usize = 48;
    const MC_FP: usize = 16 + 29 * 8;
    const MC_LR: usize = MC_FP + 8;
    const MC_PC: usize = MC_FP + 24;
    unsafe extern "C" {
        fn sigaction(sig: i32, act: *const SigAction, old: *mut SigAction) -> i32;
        fn sigaltstack(ss: *const StackT, old: *mut StackT) -> i32;
        fn backtrace_symbols_fd(buffer: *const *mut c_void, size: i32, fd: i32);
        fn write(fd: i32, buf: *const c_void, n: usize) -> isize;
        fn _exit(code: i32) -> !;
    }
    extern "C" fn on_crash(_sig: i32, _info: *mut c_void, uc: *mut c_void) {
        const MSG: &[u8] = b"dw1: fatal signal (SIGSEGV or SIGBUS); call stack:\n";
        let mut frames = [std::ptr::null_mut::<c_void>(); 32];
        unsafe {
            write(2, MSG.as_ptr().cast(), MSG.len());
            let mc = uc.cast::<u8>().add(UC_MCONTEXT).cast::<*const u8>().read();
            let reg = |off: usize| mc.add(off).cast::<usize>().read();
            frames[0] = reg(MC_PC) as *mut c_void;
            frames[1] = reg(MC_LR) as *mut c_void;
            let mut n = 2;
            // Each frame record is (caller's fp, return address). Stop at anything that doesn't
            // look like a record further up the same stack.
            let mut fp = reg(MC_FP);
            while n < frames.len() && fp != 0 && fp.is_multiple_of(16) {
                let next = (fp as *const usize).read();
                let ret = (fp as *const usize).add(1).read();
                if ret == 0 {
                    break;
                }
                frames[n] = ret as *mut c_void;
                n += 1;
                if next <= fp || next - fp > 8 << 20 {
                    break;
                }
                fp = next;
            }
            backtrace_symbols_fd(frames.as_ptr(), n as i32, 2);
            _exit(139);
        }
    }
    unsafe {
        // 256 KB for the handler, kept for the life of the process.
        let stack = Box::leak(vec![0u8; 256 * 1024].into_boxed_slice());
        let ss = StackT {
            sp: stack.as_mut_ptr().cast(),
            size: stack.len(),
            flags: 0,
        };
        let act = SigAction {
            handler: on_crash,
            mask: 0,
            flags: SA_ONSTACK | SA_RESETHAND | SA_SIGINFO,
        };
        if sigaltstack(&ss, std::ptr::null_mut()) != 0
            || sigaction(SIGSEGV, &act, std::ptr::null_mut()) != 0
            || sigaction(SIGBUS, &act, std::ptr::null_mut()) != 0
        {
            eprintln!("dw1: DW1_CRASH_TRACE: could not install the handler");
        }
    }
}

/// The Windows counterpart of the crash trace: an unhandled-exception filter that prints the
/// exception code, its address and the stack addresses (SEH runs on a normal thread, so the
/// standard macros are safe here; the frame-pointer walk of the macOS handler has no
/// equivalent that works without dbghelp's unwinder).
#[cfg(windows)]
fn install_crash_trace() {
    use std::ffi::c_void;
    #[repr(C)]
    struct ExceptionRecord {
        code: u32,
        flags: u32,
        record: *mut ExceptionRecord,
        address: *mut c_void,
        nparams: u32,
        info: [*mut c_void; 15],
    }
    #[repr(C)]
    struct ExceptionPointers {
        record: *mut ExceptionRecord,
        context: *mut c_void,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetUnhandledExceptionFilter(
            filter: Option<unsafe extern "system" fn(*mut ExceptionPointers) -> i32>,
        ) -> *mut c_void;
        fn RtlCaptureStackBackTrace(
            frames_to_skip: u32,
            frames_to_capture: u32,
            backtrace: *mut *mut c_void,
            backtrace_hash: *mut u32,
        ) -> u16;
    }
    unsafe extern "system" fn on_crash(x: *mut ExceptionPointers) -> i32 {
        // EXCEPTION_EXECUTE_HANDLER: end the process after the filter.
        const EXECUTE_HANDLER: i32 = 1;
        let mut frames = [std::ptr::null_mut::<c_void>(); 32];
        let n = unsafe {
            RtlCaptureStackBackTrace(0, frames.len() as u32, frames.as_mut_ptr(), std::ptr::null_mut())
        };
        let (code, address) = unsafe { ((*(*x).record).code, (*(*x).record).address) };
        eprintln!("dw1: fatal exception {code:#010x} at {address:p}; call stack:");
        for f in &frames[..n as usize] {
            eprintln!("  {f:p}");
        }
        EXECUTE_HANDLER
    }
    unsafe { SetUnhandledExceptionFilter(Some(on_crash)) };
}

/// The Linux counterpart: SIGSEGV/SIGBUS/SIGILL on an alternate stack, printing the
/// stack with glibc's backtrace (not async-signal-safe, but this is a fatal-path trace
/// like the macOS one; _exit follows immediately).
#[cfg(target_os = "linux")]
fn install_crash_trace() {
    use std::ffi::c_void;
    #[repr(C)]
    struct SigAction {
        handler: extern "C" fn(i32, *mut c_void, *mut c_void),
        mask: [u64; 16],
        flags: i32,
        restorer: *mut c_void,
    }
    // glibc's sigaction wrapper installs its own restorer; SA_RESTORER must stay unset.
    const SA_ONSTACK: i32 = 0x0800_0000;
    const SA_RESETHAND: i32 = 0x8000_0000u32 as i32;
    const SA_SIGINFO: i32 = 0x0000_0004;
    const SIGSEGV: i32 = 11;
    const SIGBUS: i32 = 7;
    const SIGILL: i32 = 4;
    unsafe extern "C" {
        fn sigaction(sig: i32, act: *const SigAction, old: *mut SigAction) -> i32;
        fn sigaltstack(ss: *const [u64; 2], old: *mut [u64; 2]) -> i32;
        fn backtrace(buffer: *mut *mut c_void, size: i32) -> i32;
        fn backtrace_symbols_fd(buffer: *const *mut c_void, size: i32, fd: i32);
        fn write(fd: i32, buf: *const c_void, n: usize) -> isize;
        fn _exit(code: i32) -> !;
    }
    extern "C" fn on_crash(_sig: i32, _info: *mut c_void, _uc: *mut c_void) {
        const MSG: &[u8] = b"dw1: fatal signal; call stack:\n";
        let mut frames = [std::ptr::null_mut::<c_void>(); 32];
        unsafe {
            let _ = write(2, MSG.as_ptr().cast(), MSG.len());
            let n = backtrace(frames.as_mut_ptr(), frames.len() as i32);
            backtrace_symbols_fd(frames.as_ptr(), n, 2);
            _exit(139);
        }
    }
    unsafe {
        let stack = Box::leak(vec![0u8; 256 * 1024].into_boxed_slice());
        let ss = [stack.as_mut_ptr() as u64, stack.len() as u64];
        let act = SigAction {
            handler: on_crash,
            mask: [0; 16],
            flags: SA_ONSTACK | SA_RESETHAND | SA_SIGINFO,
            restorer: std::ptr::null_mut(),
        };
        if sigaltstack(&ss, std::ptr::null_mut()) != 0
            || sigaction(SIGSEGV, &act, std::ptr::null_mut()) != 0
            || sigaction(SIGBUS, &act, std::ptr::null_mut()) != 0
            || sigaction(SIGILL, &act, std::ptr::null_mut()) != 0
        {
            eprintln!("dw1: DW1_CRASH_TRACE: could not install the handler");
        }
    }
}

/// Where the window build keeps the memory cards: `DW1_SAVE_DIR`, else
/// [`default_save_dir`]; an empty `DW1_SAVE_DIR` keeps nothing.
pub(crate) fn save_dir() -> Option<std::path::PathBuf> {
    match std::env::var_os("DW1_SAVE_DIR") {
        Some(d) if d.is_empty() => None,
        Some(d) => Some(d.into()),
        None => default_save_dir(),
    }
}

/// `~/Library/Application Support/dw1-silicon`; `%APPDATA%\dw1-silicon` on Windows,
/// `$XDG_CONFIG_HOME/dw1-silicon` (via `dirs`) elsewhere.
pub fn default_save_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    return std::env::var_os("HOME").map(|h| {
        std::path::PathBuf::from(h).join("Library/Application Support/dw1-silicon")
    });
    #[cfg(not(target_os = "macos"))]
    return dirs::config_dir().map(|d| d.join("dw1-silicon"));
}

/// The cards (Phase 7). Slot n's card is, in order: `card<n>.mcd` in the save directory when
/// cards are kept (the window build; headless only with `DW1_SAVE_DIR`), else the
/// `DW1_CARD<n>` file, else a fresh formatted card. A kept card that doesn't exist yet starts as
/// the `DW1_CARD<n>` file (so the launcher's fixture only seeds the player's card) or fresh,
/// and is written back as the game changes it (libmcrd: atomic, durable, with a `.bak`). An
/// unreadable or wrong-sized card file stops the program rather than play on with a card that
/// isn't kept. The save directory is locked for the run: a second copy of the game won't
/// share it.
fn insert_cards(keep: bool) {
    let dir = if keep { save_dir() } else { None };
    if let Some(dir) = &dir {
        lock_save_dir(dir);
    }
    let read_card = |what: &str, path: &std::path::Path| -> psyq::libmcrd::Card {
        let bytes = std::fs::read(path).unwrap_or_else(|e| {
            eprintln!("dw1: {what} {}: {e}", path.display());
            std::process::exit(1);
        });
        psyq::libmcrd::Card::from_bytes(&bytes).unwrap_or_else(|| {
            eprintln!("dw1: {what} {} is not a 128 KB card image", path.display());
            std::process::exit(1);
        })
    };
    for (slot, var) in ["DW1_CARD1", "DW1_CARD2"].into_iter().enumerate() {
        let kept = dir
            .as_ref()
            .map(|d| d.join(format!("card{}.mcd", slot + 1)));
        // A kept card that isn't there yet (only that: any other error stops) starts as the
        // seed.
        let saved = kept
            .as_ref()
            .and_then(|path| match std::fs::metadata(path) {
                Ok(_) => Some(path),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => {
                    eprintln!("dw1: saved card {}: {e}", path.display());
                    std::process::exit(1);
                }
            });
        let card = match saved {
            Some(path) => {
                if std::env::var_os("DW1_REPLAY").is_some() {
                    eprintln!(
                        "dw1: a replay starts from its recorded cards, but {} exists",
                        path.display()
                    );
                    std::process::exit(2);
                }
                Some(read_card("saved card", path))
            }
            None => std::env::var_os(var).map(|p| read_card(var, std::path::Path::new(&p))),
        };
        if let Some(card) = card {
            psyq::libmcrd::insert_card(slot, Some(card));
        }
        if let Some(path) = kept {
            eprintln!("dw1: memory card {} kept in {}", slot + 1, path.display());
            psyq::libmcrd::set_card_file(slot, Some(path));
        }
    }
    if dir.is_some() {
        // SAFETY: registering a plain extern "C" function.
        unsafe { atexit(flush_cards_at_exit) };
    }
}

/// The saves to start the slots with: the cards earlier builds kept in the save directory
/// (`card1.mcd`, `card2.mcd`), else `DW1_CARD1`/`DW1_CARD2`; card 1's saves keep their slots.
/// DuckStation's memory card 1 for Digimon World (USA), if it has one.
fn duckstation_card() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    let card = {
        let home = std::env::var_os("HOME")?;
        std::path::PathBuf::from(home)
            .join("Library/Application Support/DuckStation/memcards/Digimon World (USA)_1.mcd")
    };
    #[cfg(windows)]
    let card = dirs::document_dir()?.join(
        "DuckStation/memcards/Digimon World (USA)_1.mcd",
    );
    #[cfg(all(unix, not(target_os = "macos")))]
    let card = dirs::config_dir()?
        .join("duckstation/memcards/Digimon World (USA)_1.mcd");
    card.exists().then_some(card)
}

fn seed_saves(dir: Option<&std::path::Path>) -> Result<saves::Saves, String> {
    let mut all = saves::Saves::new();
    for (n, var) in ["DW1_CARD1", "DW1_CARD2"].into_iter().enumerate() {
        // A kept card that isn't there falls back to the env's; any other error stops (the
        // slots are seeded once, and would miss its saves for good).
        let kept = match dir.map(|d| d.join(format!("card{}.mcd", n + 1))) {
            Some(p) => match std::fs::metadata(&p) {
                Ok(_) => Some(p),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(format!("{}: {e}", p.display())),
            },
            None => None,
        };
        // With no card named, the first run of kept slots (the window build's) takes
        // DuckStation's card for this game (read only), as `dw1-mac` did; a throwaway run
        // (`dir` None) starts empty.
        let duckstation = || (n == 0 && dir.is_some()).then(duckstation_card).flatten();
        let Some(path) = kept
            .or_else(|| std::env::var_os(var).map(Into::into))
            .or_else(duckstation)
        else {
            continue;
        };
        let found = saves::saves_on(&saves::read_card(&path)?);
        eprintln!("dw1: {} save(s) taken from {}", found.len(), path.display());
        let left = saves::merge(&mut all, found);
        if left > 0 {
            eprintln!(
                "dw1: WARNING: {left} save(s) from {} didn't fit",
                path.display()
            );
        }
    }
    Ok(all)
}

/// The save slots (Phase 7, `saves.rs`): the game sees one memory card holding the slots'
/// saves, instantly, and no second card; its screens skip the card steps. When saves are kept,
/// they're `saves/slot-NN.dw1save` in the save directory (locked for the run), seeded the first
/// time ([`seed_saves`]); otherwise the slots start as the `DW1_CARD1`/`DW1_CARD2` saves and
/// nothing is written.
fn insert_slots(keep: bool) {
    if std::env::var_os("DW1_REPLAY").is_some() {
        eprintln!("dw1: a replay runs with the memory cards it was recorded with, not save slots");
        std::process::exit(2);
    }
    // (in the window build a message box says why; from Finder there's no terminal)
    let fail = |e: String| -> ! { applog::fatal(&e) };
    match save_dir().filter(|_| keep) {
        Some(dir) => {
            lock_save_dir(&dir);
            let (slots, saves) =
                saves::open(&dir, || seed_saves(Some(&dir))).unwrap_or_else(|e| fail(e));
            eprintln!(
                "dw1: {} save slot(s) in use, kept in {}",
                saves.len(),
                slots.display()
            );
            psyq::libmcrd::insert_card(0, Some(saves::card_with(&saves)));
            psyq::libmcrd::set_card_keeper(0, Some(Box::new(saves::SlotKeeper::new(slots, saves))));
            // SAFETY: registering a plain extern "C" function.
            unsafe { atexit(flush_cards_at_exit) };
        }
        None => {
            let saves = seed_saves(None).unwrap_or_else(|e| fail(e));
            psyq::libmcrd::insert_card(0, Some(saves::card_with(&saves)));
        }
    }
    psyq::libmcrd::insert_card(1, None);
    psyq::libmcrd::set_instant(true);
    psyq::native::set_pc_saves(true);
    pc_text::apply();
}

/// `dw1 export-card <file>` / `dw1 import-card <file>`: moves saves between the slots and a
/// memory card image an emulator uses. Returns false if the arguments aren't one of these.
fn card_command(args: &[String]) -> bool {
    let (cmd, file) = match args {
        [cmd, file] if cmd == "export-card" || cmd == "import-card" => (cmd.as_str(), file),
        _ => return false,
    };
    let fail = |e: String| -> ! {
        eprintln!("dw1: {e}");
        std::process::exit(1);
    };
    let Some(dir) = save_dir() else {
        fail("no save directory (DW1_SAVE_DIR is empty)".into())
    };
    lock_save_dir(&dir);
    let (slots, mut saves) =
        saves::open(&dir, || seed_saves(Some(&dir))).unwrap_or_else(|e| fail(e));
    let file = std::path::Path::new(file);
    if cmd == "export-card" {
        if file.exists() {
            fail(format!("{} already exists", file.display()));
        }
        psyq::libmcrd::write_atomic(file, saves::card_with(&saves).as_bytes())
            .unwrap_or_else(|e| fail(format!("{}: {e}", file.display())));
        eprintln!("dw1: {} save(s) written to {}", saves.len(), file.display());
    } else {
        let found = saves::saves_on(&saves::read_card(file).unwrap_or_else(|e| fail(e)));
        let before = saves.clone();
        let left = saves::merge(&mut saves, found);
        let keeper = saves::SlotKeeper::new(slots, before.clone());
        let mut added = Vec::new();
        for (&slot, data) in saves.iter().filter(|(s, _)| !before.contains_key(s)) {
            // A slot file that isn't a valid save (left out by the load) is backed up first.
            keeper
                .write(slot, data)
                .unwrap_or_else(|e| fail(format!("slot {}: {e}", slot + 1)));
            added.push(slot + 1);
        }
        eprintln!(
            "dw1: {} save(s) copied into slots {added:?}{}",
            added.len(),
            if left > 0 {
                format!("; {left} didn't fit")
            } else {
                String::new()
            }
        );
    }
    true
}

unsafe extern "C" {
    fn atexit(f: extern "C" fn()) -> i32;
}

/// A card write that failed during the run gets one more try as the program exits; if the
/// cards still aren't on disk, the program says so and exits with status 3.
extern "C" fn flush_cards_at_exit() {
    unsafe extern "C" {
        fn _exit(status: i32) -> !;
    }
    psyq::libmcrd::flush_cards();
    if !psyq::libmcrd::cards_saved() {
        eprintln!("dw1: WARNING: the last save could not be written to disk");
        // SAFETY: ends the process at once (calling exit from an exit handler isn't allowed).
        unsafe { _exit(3) };
    }
}

/// Holds an exclusive lock on `<dir>/lock` for the rest of the run, or stops: two copies of
/// the game writing the same cards would undo each other's saves.
fn lock_save_dir(dir: &std::path::Path) {
    static LOCK: std::sync::OnceLock<std::fs::File> = std::sync::OnceLock::new();
    let result = std::fs::create_dir_all(dir).and_then(|()| {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join("lock"))?;
        f.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => {
                std::io::Error::other("another copy of the game is using it (close it first)")
            }
            std::fs::TryLockError::Error(e) => e,
        })?;
        Ok(f)
    });
    match result {
        Ok(f) => {
            let _ = LOCK.set(f);
        }
        Err(e) => applog::fatal(&format!("save directory {}: {e}", dir.display())),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if card_command(&args) {
        return;
    }
    // Headless for the emulator-comparison runs (and `DW1_HEADLESS=1`); the window otherwise.
    let headless = [
        "DW1_HEADLESS",
        "DW1_REPLAY",
        "DW1_SHOTS",
        "DW1_STOP_AT",
        "DW1_START_AT",
    ]
    .iter()
    .any(|v| std::env::var_os(v).is_some_and(|x| !x.is_empty() && x != "0"))
        // A movie number, where 0 (the intro) is a value, not "off".
        || std::env::var_os("DW1_MOVIE").is_some_and(|x| !x.is_empty());
    // An app run (no terminal) writes to a log; the window build shows stops in message boxes.
    let crashed_before = applog::start(!headless);
    if std::env::var_os("DW1_CRASH_TRACE").is_some() || applog::bundled() {
        install_crash_trace();
    }
    // The disc before the saves: the slots' text patches (`pc_text`) read it. The window build
    // opens its window first, so the first-run screen can ask for the image.
    let mut window = None;
    if headless {
        match open_disc() {
            Ok(disc) => psyq::libcd::insert_disc(disc),
            Err(e) => {
                eprintln!("dw1: {e}");
                std::process::exit(1);
            }
        }
    } else {
        match window::Window::new() {
            Ok(mut w) => {
                if let Some(log) = crashed_before {
                    applog::report_earlier_crash(&log);
                }
                let disc = w.find_disc(disc::at_startup());
                psyq::libcd::insert_disc(disc);
                window = Some(w);
            }
            Err(e) => applog::fatal(&format!("cannot open the window: {e}")),
        }
    }
    // Headless runs keep saves only when asked to (`DW1_SAVE_DIR` set): the replays write
    // nothing. The window build has save slots; headless runs have the memory cards the
    // emulator comparisons need (`DW1_SAVES=slots` or `cards` to choose).
    let keep = !headless || std::env::var_os("DW1_SAVE_DIR").is_some_and(|d| !d.is_empty());
    match std::env::var("DW1_SAVES").as_deref() {
        Ok("slots") => insert_slots(keep),
        Ok("cards") => insert_cards(keep),
        Ok(other) if !other.is_empty() => {
            eprintln!("dw1: DW1_SAVES is slots or cards, not {other:?}");
            std::process::exit(2);
        }
        _ if headless => insert_cards(keep),
        _ => insert_slots(keep),
    }
    match window {
        None => psyq::libetc::set_host(Box::new(headless::Headless::from_env())),
        Some(mut w) => {
            if !std::env::var_os("DW1_NO_SPLASH").is_some_and(|x| !x.is_empty() && x != "0") {
                w.splash();
            }
            psyq::libetc::set_host(Box::new(w));
        }
    }
    // DW1_MOVIE=<id> plays one movie on its own (0 = the intro) and stops: the game's movie
    // player, without the boot that precedes it in main().
    let movie = std::env::var("DW1_MOVIE")
        .ok()
        .filter(|v| !v.is_empty())
        .map(|v| {
            // MOV_playMovie indexes its four-entry table without a check.
            match v.parse::<i32>() {
                Ok(id @ 0..=3) => id,
                _ => {
                    eprintln!("dw1: DW1_MOVIE must be a movie number from 0 to 3, not {v:?}");
                    std::process::exit(2);
                }
            }
        });
    if let Some(id) = movie {
        psyq::libetc::ResetCallback();
        psyq::libgpu::ResetGraph(0);
        psyq::libcd::CdInit();
        let completed = unsafe { decomp_c::MOV_playMovie(id) };
        eprintln!(
            "dw1: movie {id} {} at vblank {}",
            if completed == 1 {
                "played to its end"
            } else {
                "was skipped"
            },
            psyq::libetc::vblank_count()
        );
        return;
    }
    // The overlays' initialised data as linked, for their reloads (decomp_c::overlays).
    decomp_c::overlays::snapshot();
    // The game's main() never returns on the PS1.
    let status = unsafe { decomp_c::dw1_game_main() };
    eprintln!("dw1: the game's main() returned {status}");
}

/// Overlay data lifetimes (decomp_c::overlays), in this binary because it links every overlay
/// object the way the app does.
#[cfg(test)]
mod overlay_tests {
    #[test]
    fn every_overlay_has_a_section_and_reset_restores_it() {
        use decomp_c::overlays::{dw1_overlay_reset, section_info, sections, snapshot};
        snapshot();
        let found = sections();
        // Every overlay unit but SHOP (resident) has initialised data.
        let ids: Vec<u32> = found.iter().map(|s| s.0).collect();
        for id in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 13, 14, 15, 16] {
            assert!(
                ids.contains(&id),
                "overlay {id} has no data section: {found:?}"
            );
        }
        // BTL's resident initialised variables (its section's size depends on what the linker
        // keeps: the test binary dead-strips the game code that uses most of the data).
        let btl = found.iter().find(|s| s.0 == 1).unwrap();
        assert!(btl.1 > 0 && btl.2 >= 20, "BTL section {btl:?}");
        // A reset puts the linked bytes back but keeps the resident variables' values.
        let (start, len, keeps) = section_info(1).unwrap();
        let kept_byte = |p: *mut u8| {
            keeps
                .iter()
                .any(|&(a, n)| p >= a && (p as usize) < a as usize + n)
        };
        unsafe {
            let (kept, _) = keeps[0];
            let old_kept = kept.read();
            kept.write(old_kept ^ 0xA5);
            let probe = (0..len).map(|i| start.add(i)).find(|&p| !kept_byte(p));
            let old_probe = probe.map(|p| p.read());
            if let Some(p) = probe {
                p.write(old_probe.unwrap() ^ 0x5A);
            }
            dw1_overlay_reset(1);
            if let Some(p) = probe {
                assert_eq!(
                    p.read(),
                    old_probe.unwrap(),
                    "reset didn't restore the section"
                );
            }
            assert_eq!(
                kept.read(),
                old_kept ^ 0xA5,
                "reset didn't keep the resident variable"
            );
            kept.write(old_kept);
        }
    }
}
