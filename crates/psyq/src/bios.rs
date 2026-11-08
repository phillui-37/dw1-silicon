//! PS1 BIOS (kernel) calls.
//!
//! The SDK's `rand`/`srand` in DW1's executable are three-instruction stubs that jump into the
//! BIOS (A(2Fh) and A(30h); `decomp/asm/us/main/psyq/libc.s`), so the game's random stream is
//! the BIOS generator, documented in psx-spx "Kernel (BIOS)":
//! `x = x * 0x41C64E6D + 0x3039`, result `(x >> 16) & 0x7FFF`.
//!
//! Ember's retail BIOS (`scph1001.bin`) keeps the state at RAM 0x9010 and implements exactly
//! that recurrence at 0xBFC021F0; DW1 never calls `srand`. Measured 2026-10-13 in PCSX-Redux
//! (S0.8, `oracle/probe-boot.lua`): the state is 0 after boot, and every change through the
//! first 206 calls is reachable by stepping it. Whether real hardware also starts at 0 depends
//! on the BIOS clearing that RAM at power-on (unverified).

use std::sync::atomic::{AtomicU32, Ordering};

const RAND_MULTIPLIER: u32 = 0x41C6_4E6D;
const RAND_INCREMENT: u32 = 0x3039;

/// Seed before the first `rand` call: 0, measured on the emulator (see module docs).
pub const INITIAL_RAND_SEED: u32 = 0;

static RAND_STATE: AtomicU32 = AtomicU32::new(INITIAL_RAND_SEED);

/// One step of the BIOS generator: the next state and the 15-bit value `rand` returns.
pub const fn rand_step(state: u32) -> (u32, i32) {
    let next = state
        .wrapping_mul(RAND_MULTIPLIER)
        .wrapping_add(RAND_INCREMENT);
    (next, ((next >> 16) & 0x7FFF) as i32)
}

/// The generator's current state (the PS1's RAM 0x9010), for the canonical game-state record.
pub fn rand_state() -> u32 {
    RAND_STATE.load(Ordering::Relaxed)
}

/// BIOS A(2Fh) `rand()`, called by the game C as `rand` (renamed `dw1_bios_rand` there so it
/// can't bind to the host libc's).
#[unsafe(no_mangle)]
pub extern "C" fn dw1_bios_rand() -> i32 {
    // The game is single-threaded; the atomic only keeps this safe Rust.
    let (next, value) = rand_step(RAND_STATE.load(Ordering::Relaxed));
    RAND_STATE.store(next, Ordering::Relaxed);
    if rand_log() {
        log_rand_call(value);
    }
    value
}

/// `DW1_RAND_LOG=1`: print every `rand()` call with its VBlank and callers, to find where a run's
/// RNG use departs from the emulator's (`oracle/capture-calls.lua` with `DW1_CAPTURE_FUNCS=rand`).
fn rand_log() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("DW1_RAND_LOG").is_some())
}

#[inline(never)]
fn log_rand_call(value: i32) {
    eprintln!(
        "dw1: rand {value:#06x} at vblank {}",
        crate::libetc::vblank_count()
    );
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn backtrace(buffer: *mut *mut std::ffi::c_void, size: i32) -> i32;
            fn backtrace_symbols_fd(buffer: *const *mut std::ffi::c_void, size: i32, fd: i32);
        }
        let mut frames = [std::ptr::null_mut(); 5];
        unsafe {
            let n = backtrace(frames.as_mut_ptr(), frames.len() as i32);
            // Skip this function and dw1_bios_rand.
            if n > 2 {
                backtrace_symbols_fd(frames.as_ptr().add(2), (n - 2).min(2), 2);
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // execinfo is a macOS libc extension; the standard library's backtrace prints the
        // same callers.
        let trace = std::backtrace::Backtrace::force_capture();
        eprintln!("{trace}");
    }
}

/// BIOS A(30h) `srand(seed)`. DW1's C never calls it (S0.5 survey); kept for tests and the
/// oracle, which sets the measured initial seed.
#[unsafe(no_mangle)]
pub extern "C" fn dw1_bios_srand(seed: u32) {
    RAND_STATE.store(seed, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rand_step_matches_the_documented_recurrence() {
        // From seed 0: x1 = 0x3039, x2 = 0x3039 * 0x41C64E6D + 0x3039 (mod 2^32).
        let (x1, r1) = rand_step(0);
        assert_eq!((x1, r1), (0x3039, 0));
        let (x2, r2) = rand_step(x1);
        assert_eq!(x2, 0x3039u32.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039));
        assert_eq!(r2, ((x2 >> 16) & 0x7FFF) as i32);
    }

    #[test]
    fn rand_matches_the_emulator() {
        // oracle/probe-boot.lua on Ember's disc and BIOS: the state at 0x9010 after the first
        // 6 calls, then after 206 calls in total.
        let mut state = INITIAL_RAND_SEED;
        for _ in 0..6 {
            state = rand_step(state).0;
        }
        assert_eq!(state, 0xC21F_1C8A);
        for _ in 6..206 {
            state = rand_step(state).0;
        }
        assert_eq!(state, 0x5732_0F22);
    }

    #[test]
    fn rand_is_15_bit() {
        let mut state = 1;
        for _ in 0..10_000 {
            let (next, value) = rand_step(state);
            assert!((0..=0x7FFF).contains(&value));
            state = next;
        }
    }
}
