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
