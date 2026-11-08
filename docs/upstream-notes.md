# Upstream coordination notes (for Phil to post)

Three upstreamable changes, each already an isolated commit series on `windows-port`.
dw1-silicon's PLAN.md is an agent-handoff doc, so open a tracking issue first, then PRs.

## 1. Tracking issue (dw1-silicon) — Windows & Linux support

Title: Windows and Linux builds of the native port (COFF/ELF section machinery)

Body sketch:
- Full port of the runtime to Windows (MSVC + clang for the game C) and Linux,
  on top of the existing macOS build. All three build + test green; JP Rev 1
  (DW1_VERSION=jp_rev1) boots on Windows via Vulkan.
- Key pieces: COFF `$`-ordered overlay groups with in-object .bss section-header
  patching (no pragma places uninit data on COFF; llvm-objcopy cannot rename COFF
  sections); ELF identifier-named sections with __start_/__stop_ bounds; PE image
  walk / /proc/self/maps for the GPU link window; per-platform backtrace and crash
  trace; wgpu backend tables (vulkan/dx12 off macOS); DW1_VERSION selects the
  decomp release (us | jp_rev1) end to end.
- Full port log: docs/windows-port.md in the fork.

## 2. PR — build(render): enable vulkan/dx12 wgpu backends off macOS

Single-file Cargo.toml change; zero code (no Metal-specific code exists).

## 3. PR — build(decomp-c): clang on every host + portable paths

Small, host-agnostic: DW1_CC override, python candidate list, macOS-only host
symbol check, verbatim-path strip, `#line`/`#include` forward slashes. Does not
change the macOS build's behaviour.

## 4. dw_decomp (jype0) — gen_text.py --lang

Community-localization hook: `--lang <key>` (default the release's own),
fallback map extended (zh_hant -> ja). Byte-identical output without --lang.
Pairs with text yaml gaining new language keys; no release logic touched.
Also: the native-build fixes on our branch (SAVE_TITLE_RESERVED/dw1_text_width
guards, jp native guards in std_main, merged adjacent arrays in
std_bss_jp_rev1.c/vs_bss_jp_rev1.c) are needed by anyone building jp_rev1
natively.

## Notes
- Phil pushes `windows-port` + `dw1-windows-port` from the second account; the
  decomp branch needs a dw_decomp fork to PR against jype0.
- Keep PRs scoped: (2) and (3) can go immediately after the tracking issue;
  the rest after upstream responds.
