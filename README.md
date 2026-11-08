# dw1-silicon

A native port of **Digimon World** (PlayStation, 1999; USA release SLUS-01032) for
**Windows**, **Linux** and **macOS**, through [wgpu](https://wgpu.rs): Metal on a Mac, Vulkan
on Windows and Linux, DX12 as Windows' fallback. The default decomp target is the US
release; `DW1_VERSION=jp_rev1` builds the JP Rev 1 target instead.

It isn't an emulator. The game's own code, recovered as C by the
[dw_decomp](https://github.com/jype0/dw_decomp) decompilation project, is compiled for the
host, and the PlayStation's libraries and hardware underneath it are replaced by a host layer
written in Rust. The game's rules, RNG, scripts, text, music and saves behave as on the
original; improvements like higher resolution, no texture wobble and gamepad support are
options, each labelled as original or added, with the original picture always available.
The long-term goal is the whole game in Rust, migrated one module at a time and checked
against the original at every step.

**Status:** playable, being checked. The game runs natively with sound,
saves (as 15 save slots) and a sharper picture. New games, File City, raising your partner,
battles, training, the Item Keeper and saving, tournaments, Battle Mode, fishing, curling,
digivolving, death and rebirth, recruiting, the gyms, the ending and the credits are all checked
against the original game step by step. A first full playthrough is under way. See
[PLAN.md](PLAN.md).

## Building and playing

You need a machine with a Vulkan or DX12 driver on Windows 10 or 11, Vulkan on Linux, or a Mac
with Apple Silicon (Metal); and your own disc image of Digimon World as a `.cue`/`.bin` pair.
The default build targets the US release (SLUS-01032); set `DW1_VERSION=jp_rev1` to build the
JP Rev 1 target (SLPS-017.97) instead.

### Prerequisites

| | Windows | Linux | macOS |
|---|---|---|---|
| Rust nightly | `winget install Rustlang.Rustup` then `rustup toolchain install nightly` | `curl -sSf https://sh.rustup.rs \| sh -s -- --default-toolchain nightly` | same |
| C compiler (clang) | `scoop install llvm` (or download from [releases](https://github.com/llvm/llvm-project/releases)) | `sudo apt install clang libclang-dev` (Ubuntu) / `sudo dnf install clang clang-devel` (Fedora) | `xcode-select --install` |
| Python 3 + PyYAML | `scoop install python; pip install pyyaml` | `sudo apt install python3 python3-pip; pip3 install pyyaml` | included |
| CMake (for SDL3) | `scoop install cmake` | `sudo apt install cmake` | `brew install cmake` |
| SDL3 deps | — (built from source, no extras needed) | `sudo apt install libx11-dev libxext-dev libxrandr-dev libxcursor-dev libxi-dev libxinerama-dev libxss-dev libxtst-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libdecor-0-dev libdrm-dev libgbm-dev libegl1-mesa-dev libgl1-mesa-dev libasound2-dev libpulse-dev libudev-dev libdbus-1-dev` | — |

### Build (any platform)

```sh
git clone https://github.com/phillui-37/dw1-silicon.git
cd dw1-silicon
git submodule update --init decomp
git -C decomp submodule update --init external/psyq_headers
```

**macOS**:

```sh
python3 -m venv .venv && .venv/bin/pip install pyyaml cmake
tools/make-app.sh --install    # Digimon World.app in ~/Applications
```

**Windows** (PowerShell):

```powershell
# set LLVM on PATH (adjust if installed elsewhere)
$env:PATH = "$env:USERPROFILE\scoop\apps\llvm\current\bin;$env:PATH"
$env:LIBCLANG_PATH = "$env:USERPROFILE\scoop\apps\llvm\current\bin"
pip install pyyaml

cargo build --workspace --release
# binary at target\release\dw1.exe
```

**Linux**:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --workspace --release
# binary at target/release/dw1
```

**JP Rev 1 build** (all platforms) — add the env var:

```sh
DW1_VERSION=jp_rev1 cargo build --workspace --release     # Linux/macOS
```

```powershell
$env:DW1_VERSION = 'jp_rev1'; cargo build --workspace --release   # Windows
```

### Running

Start the binary (double-click `dw1.exe` on Windows, or `./target/release/dw1` on Linux/macOS).
The first start asks for your disc image in a file dialog, or set the environment variable
`DW1_DISC=/path/to/DigimonWorld.cue` to skip the dialog. The image's hash is checked before
the game starts.

Options in play: **F2** render scale (auto from window size), **F3** no texture wobble
(PGXP), **F4** fit-to-window, **F5** full colour, **F11** fullscreen, **F12** bug snapshot.
Saves go to `%APPDATA%\dw1-silicon` (Windows), `~/.config/dw1-silicon` (Linux), or
`~/Library/Application Support/dw1-silicon` (macOS), and are interchangeable with DuckStation
memory cards.

**[The guide](docs/guide.html)** (open it in a browser) covers installing the tools, controls,
the picture settings, saves and moving them to and from DuckStation, and what to do when
something goes wrong.

## Community localization (i18n)

The game's text lives in YAML files under `decomp/text/**/*.yaml`, already keyed per language:

```yaml
STAT_LABELS:
  entries:
    - name: STR_STAT_LABEL_HP
      ja: 最大ＨＰ          # Japanese (the original)
      en_us: HP             # English (US release)
      zh_hant: 体力         # Traditional Chinese (POC)
```

To add or change a language:

1. **Add your language key** to the YAML entries. The key is any name that isn't already
   taken (`zh_hant`, `ko`, `pt_br`, ...). If an entry has no text for your language, it
   falls back to the original Japanese.

2. **Register the language** in `decomp/tools/gen_text.py`:
   ```python
   __CHARMAPS = {
       'ja': ...,
       'en_us': ...,
       'zh_hant': lambda text: text,  # add yours (identity = no transformation)
   }
   __FALLBACKS = {'zh_hant': 'ja', ...}  # what to use when an entry has no text
   ```

3. **Build with your language**:
   ```sh
   DW1_VERSION=jp_rev1 DW1_LANG=zh_hant cargo build --workspace --release
   ```

### Font / glyph coverage (important!)

The game's font is compiled into the executable (not a disc file): a 992-glyph table
covering JIS X 0208 Level 1 kanji + kana + full-width ASCII, stored as 12×11 one-bit
cells. Any character you write **must be in that table** or it draws as a hollow tofu box.

- Characters encodable in CP932 (Shift-JIS) **and** present in the font's 992-glyph table
  render correctly. This includes most common kanji and all kana.
- The build fails loudly (`gen_text.py: no Shift-JIS character for ...`) if a character
  can't be encoded at all — a useful safety net.
- Full details, coverage percentages, and a proposal for a replaceable font pack are in
  [docs/i18n-font-notes.md](docs/i18n-font-notes.md).

### Example: the zh_hant POC

```sh
# from the repo root
DW1_VERSION=jp_rev1 DW1_LANG=zh_hant cargo build -p dw1 --bin dw1
DW1_DISC=/path/to/DigimonWorld.cue ./target/debug/dw1
```

The six stat labels (体力/気力/攻撃/防御/速度/精神) appear in Chinese in the stat menu.
These are deliberately limited to glyphs the shipped font already has.

## You need your own copy of the game

This repository contains no game data: no disc image, no assets, no text, no music. The
port reads your own disc image of Digimon World (USA) and checks it against the known good
dump's hash before it starts. The app you build holds code and text made from the game, so it's
for your own Mac: please don't share it. Please don't open issues asking where to get the
game.

## Credits

This port stands on other people's work:

- **[dw_decomp](https://github.com/jype0/dw_decomp)** by jype0 and contributors (MIT): the
  matching decompilation of Digimon World. Every line of the game's logic this port runs
  comes from it. Without it this project couldn't exist.
- **SydMontague**'s [DW1-Code](https://github.com/SydMontague/DW1-Code) and
  [DW1-SydPatches](https://github.com/SydMontague/DW1-SydPatches): years of
  reverse-engineering behind the decomp's names and structures.
- **[DW2-PC](https://github.com/Wyrelade/DW2-PC)** by Wyrelade (CC0): a native PC port of
  Digimon World 2 from its decompilation. Its design (data packs built from the player's
  disc, a hash manifest, overlays linked into one program, a GTE model checked against an
  emulator) is the template for this one.
- **[PCSX-Redux](https://github.com/grumpycoders/pcsx-redux)** by grumpycoders and
  contributors: the emulator we use as the reference, recording inputs and comparing the
  port's state and pictures against the original game.
- **[psx-spx](https://psx-spx.consoledev.net/)** (nocash and contributors): the PlayStation
  hardware documentation.
- **[splat](https://github.com/ethteck/splat)** by ethteck, and
  **[Decompollaborate](https://github.com/Decompollaborate)**'s
  [spimdisasm](https://github.com/Decompollaborate/spimdisasm) and
  [rabbitizer](https://github.com/Decompollaborate/rabbitizer): the decompilation tooling
  dw_decomp's build relies on.
- **[mkpsxiso / dumpsxiso](https://github.com/Lameguy64/mkpsxiso)** by Lameguy64 and
  contributors: reading files from the disc image.
- **[PsyCross](https://github.com/OpenDriver2/PsyCross)** by the OpenDriver2 project: a
  platform-independent reimplementation of the Psy-Q SDK. A reference for what replacing the
  PlayStation's libraries on a desktop takes, and for how the GTE and the GPU are modelled.
- **[PSXRecomp](https://github.com/RetroPortingToolKit/psxrecomp)** and the
  **[RetroPortingToolKit](https://github.com/RetroPortingToolKit)** ecosystem: the other way
  to run these games natively, by static recompilation of the original executable. A
  reference for how far a PS1 game's hardware layer has to be covered.
- **[mwccwrap](https://github.com/ThirstyWraith/mwccwrap)**, **[metrowrap](https://github.com/ThirstyWraith/metrowrap)**
  and **[wibo](https://github.com/decompals/wibo)**: the Win32 shim chain that lets the
  original CodeWarrior compiler plugin run on non-Windows hosts, used by dw_decomp's
  byte-matching build (which this fork's CI also exercises).
- **CodeWarrior for PlayStation** (Metrowerks): the original compiler, supplied by the
  player from their own disc, only ever used for byte-match verification and never
  redistributed.

Thanks also to the reviewers and maintainers of the upstream projects above, whose
comments and handoff notes (`PLAN.md`) shaped the Windows and Linux port in this fork.

## License

The code in this repository is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution you intentionally
submit for inclusion in this work, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.

This covers this repository only. dw_decomp (the `decomp/` submodule) is under its own MIT
license, and code adapted from DW2-PC keeps a note of its origin (CC0).

## Legal

Digimon and Digimon World are trademarks of Bandai Namco and Toei Animation. This is an
unofficial fan project, not affiliated with or endorsed by them. This repository contains
none of the game's assets; the decompiled game code comes from dw_decomp (fetched as a git
submodule), and the game data comes from your own disc.
