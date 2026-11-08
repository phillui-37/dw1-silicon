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
## Task 3-4 (2026-11-08 14:10 JST): full workspace builds and tests on Windows

- render: wgpu backends per platform (mac: metal; windows: vulkan+dx12; linux: vulkan) - target tables in Cargo.toml (792ee0b, delegated).
- dw1: dirs-based config/data paths, DuckStation card per OS, UCRT _open_osfhandle/_dup2 stdout/stderr redirect,
  SetUnhandledExceptionFilter crash trace, dbghelp SymFromAddr function_name, main-thread capture for message boxes.
- psyq/dw1: directory fsync skipped on Windows (FlushFileBuffers: access denied on dirs).
- Overlays on COFF, final mechanism: `data_seg` pragma places initialised data in `.d{u}$d`;
  the wrapper objects are compiled in build.rs and their `.bss` section header is patched in place
  (rename to `.z{u}$d` + converted to zero-filled PROGBITS, because llvm-objcopy cannot rename COFF
  sections and no pragma places uninitialised globals on COFF); sentinels in `$a`/`$z` are
  initialised so the pragma reaches them; link.exe $-group ordering then gives the bounds.
- link_window: PE walk had NumberOfSections read from the Machine field (34404 "sections" of .text
  bytes); fixed to coff+2. All 47 test suites green (`cargo test --workspace` exit 0).
## Task 5 (2026-11-08 14:50 JST): DW1_VERSION=jp_rev1

- pack: expected_sha1(version) map (us redump + our JP Rev 1 dump 1d10389d...) with verify_as;
  disc.rs verifies against decomp_c::DW1_VERSION (compile-time env).
- build.rs: DW1_VERSION parameterizes the mk file (with `include mk/version/*.mk` expansion;
  jp_rev1.mk includes jp.mk), the VERSION_ macro, gen_text.py's release, the config overlay.ld
  and symbols.txt, and rerun-if-changed. `$(VERSION)` in file names expands (file_table).
- overlay.ld parser: the non-US scripts write `.o(.data .data.*)` (two patterns); the marker
  dropped its trailing dot.
- decomp (branch dw1-windows-port): native guards the US-only SAVE_TITLE_RESERVED and
  dw1_text_width (per-version bodies: US glyph table vs fixed GLYPH_WIDTH), native
  STD_getEntityIndex calls in the non-US intro paths (v0 fallback = the US-audited one),
  and the merged adjacent PS1 arrays in std_bss_jp_rev1.c / vs_bss_jp_rev1.c that dw/std.h
  and dw/vs.h expect of native builds. sjis.c compiles on every version for the native save
  seeding.
- `DW1_VERSION=jp_rev1 cargo test --workspace`: 47 suites, 0 failed. Default us build unchanged.
## Task 6 (2026-11-08 15:05 JST): JP Rev 1 boots on Windows (Vulkan)

`DW1_VERSION=jp_rev1 cargo build -p dw1 --bin dw1`, then run with `DW1_DISC` pointing at
DigimonWorld (Japan) (Rev 1).cue:

- The disc hash check passes (the 1d10389d... dump) and the game reads JP data from it:
  "dw1: 4 of the save machine's memory card lines weren't found on this disc; they keep their
  wording" (US save-machine lines absent on the JP disc, as expected).
- Saves go to C:\Users\user\AppData\Roaming\dw1-silicon\saves (0 slots in use, first run).
- The window opens (title "Digimon World (dw1-silicon)") and renders animated content for the
  whole run (45 s+; screenshots in captures/windows-jp-boot: ~700 distinct colours, differing
  between 30 s and 45 s). A headless run (`DW1_HEADLESS=1`) is likewise stable for 35 s+ with
  no trap.

Not yet verified: actual gameplay input (needs a human at the window), and which wgpu backend
was selected (Vulkan vs DX12) - add a backend log line if wanted.
## Task 9 step 0 (2026-11-08 15:30 JST): cc_mips.dll extracted

No InstallShield extraction was needed: the CodeWarrior ISO stores the compiler as plain
files under /CODEWARR (the _SYS1/_USER1 CABs are only redistributables). CC_MIPS.DLL
(759,296 bytes, SHA-1 2175392ee54c4a4aa56681b29cbdf2afd25d8c9e) is extracted from LBA 47462
to decomp/bin/cc_mips/cc_mips_40.dll (gitignored there, as dw_decomp expects). LN_MIPS.DLL
(the linker, 115,712 bytes) is staged at cwps4/LN_MIPS.DLL if the byte-match build wants it.
The ISO itself is a raw 2352-byte MODE1 image misnamed .iso - extract with 2352/16 addressing.
## Task 7 (2026-11-08 18:30 JST): i18n level 1 -- DW1_LANG / gen_text --lang

- gen_text.py: --lang (default: the release's language), __FALLBACKS zh_hant->ja, zh_hant
  charmap = identity (text must be CP932-encodable; __c_string fails loudly otherwise).
  Community contract: add `zh_hant:` keys to text yaml entries; `DW1_LANG=zh_hant` at build.
- build.rs passes DW1_LANG through (rerun-if-env-changed).
- POC: the six STAT_LABELS (text/main/ui.yaml) - 體力/魔力/攻勢/防禦/速度/聰明. Verified:
  gen_text emits their CP932 octal escapes; all six byte sequences present in dw1.exe built
  with DW1_VERSION=jp_rev1 DW1_LANG=zh_hant; build without DW1_LANG unchanged.
- Runtime caveat: a CP932 codepoint not in the JP font's codepoint table draws as the last
  glyph (font.c) - whether each kyujitai (體/禦/聰) is in the shipped table is what the
  Task 8 font spike quantifies; the stat screen itself needs a player to reach.
## Task 8 (2026-11-08 18:40 JST): font spike findings (docs/i18n-font-notes.md)

The font is not a disc TIM: it lives in the EXE (SLPS_017.97) as CHAR_TO_GLYPH_TABLE
(0x98E90, 992 u16 codepoints, JIS rows 1-5 + kanji level 1) + GLYPH_DATA (0x99650, 992x22B
1-bit 12x11 cells) + FONT_CLUT, expanded at runtime - and a byte-identical copy sits inside
MOV_REL.BIN (0x47B2C), which a font pack must also patch. zh-Hant coverage with the shipped
table is 11.7% of Big5 level 1 (81.7% after a Big5->JIS transcode); a full pack is a 116 KiB
glyph sheet plus NUM_GLYPHS/render changes, not a data swap. The zh_hant POC labels were
re-picked (体力/気力/攻撃/防御/速度/精神) to glyphs the shipped table has - verified in the
exe AND against the real table, so they render.
## Task 9 (2026-11-08 20:10 JST): jp_rev1 byte-match - BLOCKED on a damaged dump

The pipeline is fully proven in a root-podman Ubuntu 24.04 container (cwps4/compare.sh,
tools preseeded in cwps4/preseed after a GitHub-CDN 504 storm; 32-bit wobo-i686 = official
1.2.0 asset, sha256-verified, plus libc6:i386 & friends for its runtime):

- dumpsxiso dumps the JP bin; `make VERSION=jp_rev1 regenerate` completes (splat over all
  overlays); metrowrap -> wibo-i686 -> mwccwrap.exe chain executes (needs MWCIncludes and
  WIBO := bin/wibo-i686 in local.mk, since mwccwrap.exe is PE32 i386).
- BLOCKER: cc_mips.dll extracted from the CodeWarrior ISO is corrupt in its data region -
  the PE parses (valid PE32 i386 DLL, 10 sections, real .text) but the .idata at file
  0xae600 contains x86 code instead of import descriptors, so both LoadLibrary (error 193)
  and wibo (std::length_error) fail. The damage is in the disc dump itself, not the
  extraction (MODE1 +16 addressing is right; the PE header and early sections are intact).
- Next step needs a clean CodeWarrior for PlayStation R4/R4.1 image or a community cc_mips
  dll mirror; everything else in the compare path is done and scripted.
## Task 9 RESOLVED (2026-11-08 20:12 JST): it was MY extraction bug, not a damaged dump

The "corrupt .idata" verdict is retracted. The first extraction read `size` bytes contiguously
from the raw 2352-byte-sector image WITHOUT skipping the 304 EDC/ECC bytes per sector: the
first 2048-byte sector (PE header) came out fine, everything after was misaligned garbage.
Sector-wise extraction (2048 at +16 per sector) yields a perfect DLL - mwccwrap -version on
Windows reports: "MIPS Compiler for PlayStation, Copyright c1998, Version 2.44.14 build 0".

With the fixed dll, cwps4/compare.sh runs the WHOLE byte-match pipeline end to end in the
container: dumpsxiso -> splat regenerate -> mwcc (CodeWarrior through metrowrap/wibo) builds
all 17 binaries -> compare. Result: all 17 differ from the retail dumps - expected, since
jype0's jp_rev1 matching is itself in progress (most differ within the first ~750 bytes,
the battle overlays also around 0x4200). That is the baseline measurement Task 9 wanted;
closing the gap is upstream decomp work, not port work. ISO identity for the record:
SHA-1 edbd27984708df31190beeb9e68e5b511a3fe208, MD5 d874c79884da2e62e5b019fa7d2f15f2.
## Bonus verification (2026-11-08 20:20 JST)

The crew research (while chasing the mirror that turned out unneeded) pulled redump's public
datfile: "CodeWarrior for PlayStation Release 4 (USA)" (disc 48799, serial D54041) has
SHA-1 edbd27984708df31190beeb9e68e5b511a3fe208 / MD5 d874c79884da2e62e5b019fa7d2f15f2 over the
raw 2352-byte image - EXACTLY our ISO's hashes. Our disc is a byte-perfect redump-verified copy;
the earlier corruption claim is fully dead. For the record, if R4.1's compiler variant is ever
needed for matching: archive.org/details/ps1_sdks "CodeWarrior for PlayStation updates.zip"
contains CW_PSX_R4_Update.exe (9.4 MB), and the same item carries redump R3/R4/R5 discs.
## Task 10 (2026-11-08 22:10 JST): Linux build + GUI run GREEN

Linux support landed (commit 16e6bf6): ELF overlay sections (identifier-named sections +
__start_/__stop_ bounds + anchors so empty sections still bound; -z nostart-stop-gc because
lld's --gc-sections sweeps encapsulation symbols), ELF link window (PT_LOAD PF_W union over
the base from /proc/self/maps), per-OS backtraces (execinfo on all unix), Linux crash trace
(sigaction alt-stack + glibc backtrace), Linux on_main_thread (gettid == pid), dladdr
symbolisation on Linux, CI matrix workflow (windows-latest + ubuntu-latest x us/jp_rev1),
upstream notes (docs/upstream-notes.md).

Verified in a root-podman ubuntu:24.04 container with the full SDL3 dependency set:
- cargo build --workspace: OK (7m23s cold; DW1_BUILT)
- DW1_VERSION=jp_rev1 cargo test --workspace: 14 suites, 0 failed
- GUI run under WSLg (Wayland + Mesa llvmpipe Vulkan over /dev/dxg): the port opens its
  window and runs the full 60-75s window with no trap; it reads the JP disc
  ("4 of the save machine's memory card lines weren't found on this disc"), uses
  /root/.config/dw1-silicon for saves, and survives to timeout (exit 124), i.e. the
  earlier "GPU packet outside the linkable data segment" trap is gone after the ELF
  segment-walk fix.
- X screenshots from inside the container fail (SDL surface is Wayland; the X root has no
  image) - the window appears on the Windows desktop via WSLg instead; visual confirmation
  therefore needs a human or a Wayland screencopy tool. The frame-level evidence that
  matters (no trap, full-run stability, disc read) is captured.
