# Windows port log

## Baseline (2026-11-08 13:10 JST)

`cargo check --workspace` (rustc 1.90.0, after `scoop install cmake llvm`, llvm 23.1.5):

Fails first in `crates/pack`:

```
error[E0433]: could not find `unix` in `os`   -- crates/pack/src/lib.rs:20
    use std::os::unix::fs::FileExt;
error[E0599]: no method named `read_exact_at` -- crates/pack/src/lib.rs:212
```

`std::os::unix::fs::FileExt` is POSIX-only; Windows needs `std::os::windows::fs::FileExt::seek_read`.
Because `pack` is a dependency of `psyq` -> `decomp-c`, the decomp-c/build.rs failure the plan
expected is not observable until pack compiles. (pack build script itself runs: it warns that no
disc is present and skips disc tests - expected.)

Toolchain notes:
- scoop.ps1 blocked by execution policy; use `scoop.cmd` (via `cmd /d /c`) - do not bypass policy.
- llvm adds no `clang` shim into existing shells; prepend `C:\Users\user\scoop\apps\llvm\current\bin`
  to PATH in the build shell. LIBCLANG_PATH is set user-wide (bindgen ready).
## Task 2 (2026-11-08 13:10-13:55 JST): decomp-c builds and tests on Windows

Fixes beyond the plan, in the order they surfaced:

1. pack: `std::os::unix::fs::FileExt::read_exact_at` -> cfg-gated helper with a seek_read loop
   on Windows (committed separately: 0e0a713).
2. Nightly Rust required: psyq's libc.rs uses c_variadic (VaList). Pinned rust-toolchain.toml
   = nightly; rustc 1.102.0-nightly works with the existing two-lifetime VaList.
3. clang rejects `\\?\` verbatim paths in -I (canonicalize output): unverbatim() strips it.
4. `#line 1 "D:\...\"` backslashes are escape sequences: forward-slash the shown path; same
   for the wrapper `#include` path (overlays.rs).
5. Psy-Q's sys/types.h and stddef.h typedefs (guards _SIZE_T/_WCHAR_T, matching Apple's SDK)
   collide with MSVC vcruntime: dw1_native.h pre-defines the guards on Windows.
6. MSVC `()` == (void) prototype semantics break the game's K&R declarations:
   -fno-ms-compatibility; clang>=16 warn-to-error promotions downgraded
   (-Wno-error=incompatible-pointer-types, implicit-function-declaration).
7. Mach-O -> COFF overlay sections: wrappers place data in `.d{unit}$d` / `.z{unit}$d`
   groups; bounds_c emits $a/$z sentinels instead of ld64 section$start$/end$ symbols.
8. setjmp: x64 vcruntime has no _setjmp/_longjmp; the replacement header declares plain
   setjmp/longjmp with a 256-byte 16-aligned buffer. vcruntime.lib linked from build.rs.
9. psyq link_window: segment$start$__DATA -> PE image walk (64K page-walk down to MZ,
   union of writable sections, cached in OnceLock; must stay < 16MB for 24-bit links).
10. execinfo (backtrace/backtrace_symbols_fd) is macOS libc only: gated to macOS in
    bios.rs log_rand_call and trap.rs unimplemented (std::backtrace elsewhere); libetc.rs
    caller_address uses kernel32 RtlCaptureStackBackTrace with the same frame numbering
    (this SDK has no CaptureStackBackTrace export). kernel32.lib linked from build.rs.

Result: `cargo test -p decomp-c` green (6 passed, 1 ignored = disc test without a disc).
