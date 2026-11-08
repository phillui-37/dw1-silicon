# i18n / font notes — the JP dialogue font, and what a zh-Hant pack must replace

Research task, no game code changed. All offsets below were read out of the project's own
dump, `DigimonWorld (Japan) (Rev 1).bin` (JP Rev 1, boot `SLPS_017.97`, MODE2/2352,
user data at `lba*2352+24`), and cross-checked against `port/decomp` `config/jp_rev1/symbols.txt`.

**Headline finding, and it contradicts the brief's assumption:** the JP dialogue font is
**not** a `.TIM` on the disc. It is **compiled into `SLPS_017.97`** as two static arrays
(`CHAR_TO_GLYPH_TABLE` and `GLYPH_DATA`). There is no font sheet to swap in `/STDDAT/` or
elsewhere, and no `OpenTIM`/`LoadTIM` path touches the glyphs. `/STDDAT/STDTIM.BIN` — the
name that looks most like "the font" — is the **VS/tournament UI** artwork (six stacked
TIMs; 64×256 4bpp pages at VRAM 768..1024 x 0..256). A zh-Hant pack therefore has to
replace **EXE bytes**, not a disc TIM. See §5 for what that means for the design.

---

## 1. Inventory — font-related files on the JP disc

The ISO9660 walk (818 files, root at LBA 22) turns up **no** file named after a font.
Names matching `FNT`/`FONT`/`MOJI`/`GLYPH`: **zero**.
`/SCN/DG.SCN` (LBA 142261, 692224 B) is the dialogue *text* store; the *glyphs* are not on
the disc at all.

The four files a reader might mistake for the font, and what they actually are:

| path | LBA | size (B) | what it is |
|---|---|---|---|
| `/STDDAT/STDTIM.BIN` | 146747 | 134528 | **VS/tournament UI art**, 6 stacked 4bpp TIMs (below). Not glyphs. |
| `/STDDAT/TIME.TIM` | 146852 | 2458 | 8bpp 33×29 tile at VRAM (704,120), CLUT 256×1. Timer digits. |
| `/STDDAT/TAISEN1.TIM` | 146813 | 33312 | 4bpp 64×256, VS screen background art. |
| `/CHDAT/ALLTIM.TIM` | 950 | 3317760 | Digimon character textures. |

`/STDDAT/STDTIM.BIN` decoded (each entry: `u32 magic=0x10`, `u32 flag`; then optional
CLUT block, then image block, each `u32 len` + `x,y,w,h` words):

| # | offset | mode | image x,y,w,h | CLUT x,y,w,h |
|---|---|---|---|---|
| 0 | `0x00000` | 4bpp | 896,0,64,256 | 288,480,16,16 |
| 1 | `0x08220` | 4bpp | 960,0,64,256 | 304,480,16,16 |
| 2 | `0x10440` | 4bpp | 832,0,64,120 | 16,480,16,6 |
| 3 | `0x14120` | 4bpp | 768,0,64,256 | 0,480,16,2 |
| 4 | `0x1C180` | 4bpp | 832,256,16,64 | 48,480,16,1 |
| 5 | `0x1C9C0` | 4bpp | 832,120,64,135 | 32,480,16,1 |

Total 134528 B = 8 header bytes + the six blocks, i.e. the file is exactly these six TIMs
with nothing left over.

### Where the font really lives

`/SLPS_017.97`, **LBA 148059, 716800 (0xAF000) B**. PS-X EXE header:
magic `PS-X EXE`, dest `0x80090800`, text size `0xAE800`, entry `0x800A6068`,
stack base `0x8013E93C`. File offset ↔ RAM address is therefore
`ram = 0x80090800 + (file_off - 0x800)`.

| array | file offset | RAM address | size | source |
|---|---|---|---|---|
| `CHAR_TO_GLYPH_TABLE` | `0x098E90` | `0x80128E90` | `0x7C0` = 992 × `u16` | `symbols.txt`, verified by pattern match |
| `GLYPH_DATA` | `0x099650` | `0x80129650` | `0x5540` = 992 × 22 B | `symbols.txt`, verified |
| `FONT_CLUT` | `0x09EB90` | `0x8012EB90` | 16 × `u16` = 32 B | `symbols.txt`, verified |

The three are contiguous in that order with no padding, from `0x098E90` to `0x09EBB0`.
`0x800 + 0xAE800 = 0xAF000` = the whole file, so this region is plain static `.text`/`.data`.

### There is a second copy of the font in `/MOV_REL.BIN`

`/MOV_REL.BIN` (LBA 147559, 472652 B, magic `MWo1`) contains a **byte-identical** copy of the
whole font region at file offset `0x47B2C` — table, 992×22 B glyph sheet and `FONT_CLUT` all
match `SLPS_017.97` exactly (0 differing bytes). It sits inside a longer identical run of
**26136 bytes** (`SLPS_017.97[0x98D70..0x9F388]` == `/MOV_REL.BIN[0x47A0C..0x4E024]`), i.e.
the overlay carries a snapshot of the main EXE's `0x80128D70..0x8012F388` data.

`/STD_REL.BIN` and `/VS_REL.BIN` contain no copy (both start with TIM magic, not code), and
`src/mov/mov.c` references neither `drawString` nor `drawGlyph`, so this copy is almost
certainly dead data from the overlay's independent link. **A font pack must still patch it**,
or the first movie that touches the overlay region can restore stale glyphs. Treat
"the font lives in exactly one place" as false.

---

## 2. Renderer — which code sites draw glyphs, and what loads the "sheet"

Everything is in `src/main/font.c`. There is **no** `OpenTIM`, `LoadTIM`, or file read on
any font path — grep for `OpenTIM|LoadTIM|loadTIMFile` returns 37 hits across the tree and
**none** of them is the font.

| what | file:line |
|---|---|
| glyph count per region | `src/main/font.c:8-14` — `NUM_GLYPHS` = 992 JP (line 11), 188 EU, 79 US |
| cell height | `src/main/font.c:15` — `GLYPH_HEIGHT` = 11 |
| scratch VRAM page for the rasterised cell | `src/main/font.c:17-19` — `TEXT_VRAM_X` 704, `TEXT_VRAM_Y` 256, `TEXT_AREA_SIZE` 256 |
| cell width / draw limit | `src/main/font.c:20-21` — `GLYPH_CELL_WIDTH` 12, `TEXT_DRAW_LIMIT` = 256-12 = 244 |
| scratch slot count / size | `src/main/font.c:23-28` — `NUM_RENDER_SLOTS` 21 (JP), `RENDER_SLOT_SIZE` 72 |
| CLUT upload target | `src/main/font.c:30-32` — `NUM_FONT_CLUT_ENTRIES` 16, VRAM (208, 488) |
| codepoint→glyph-index table (JP) | `src/main/font.c:265-390` |
| packed glyph bitmaps (JP) | `src/main/font.c:394-1387` |
| 16-entry palette | `src/main/font.c:1491-1496` |
| `initializeFontCLUT` — uploads `FONT_CLUT` | `src/main/font.c:1503-1516` |
| `clearTextArea` / `clearTextSubArea` | `src/main/font.c:1518-1535` |
| `setTextColor` (sets the ink palette index) | `src/main/font.c:1537-1541` |
| `drawGlyph` — bounds check | `src/main/font.c:1576,1584` |
| `drawGlyph` — byte-swap, space special case | `src/main/font.c:1592-1600` |
| `drawGlyph` — sentinel + linear scan | `src/main/font.c:1603-1610` |
| `drawGlyph` — select cell, pick scratch slot | `src/main/font.c:1621-1623` |
| `drawGlyph` — 11 rows × 12 px 1bpp→4bpp expander | `src/main/font.c:1625-1694` |
| `drawGlyph` — `LoadImage` into VRAM | `src/main/font.c:1696-1697` |
| `drawString` (JP: fixed 12 px advance) | `src/main/font.c:1709-1724` (advance at `:1722`) |
| `dw1_text_width` (native, fixed 12 px) | `src/main/font.c:1793-1801` |
| `initializeFontCLUT()` call site (one, at boot) | `src/main/main.c:3420` |
| advance width constant | `include/dw/font.h:9-13` — `GLYPH_WIDTH` 12 (JP), 8 (EU) |

### The rendering pipeline (per character, every frame)

1. `drawString` advances **12 px per codepoint, unconditionally** (`font.c:1722`). No
   proportional metrics on JP/EU; only the US build has per-glyph `width`.
2. `drawGlyph` byte-swaps the codepoint (`font.c:1585`), returns early on `0x8140` (the
   ideographic space, `font.c:1587-1589`), then does a **linear scan** for a match
   (`font.c:1603-1610`), using slot `NUM_GLYPHS-1` as the scratch sentinel. Not found ⇒
   it matches the `0x0000` sentinel at index 991 and renders **glyph 991**.
3. It expands the 11 stored `u16` words into 12 rows of 4bpp, **`bit == 0` means ink**
   (`font.c:1625-1694`). The 12th bit of each word is unused padding (bits 15..4 give 12 px;
   bits 3..0 are dead) — which is exactly why the cell is 12 px wide but only 11 bits carry
   data per row.
4. `LoadImage` blits the 12×11 cell into the scratch VRAM page at
   `(x/4)+704, y+256` (`font.c:1696-1697`), and the caller's sprite draws sample from
   there through the 16-entry CLUT at VRAM (208, 488). So **the font is rebuilt in VRAM
   every frame, character by character**; nothing is cached.

`COLORCODE_LOWBITS` / `COLORCODE_HIGHBITS` (`font.c:1497-1498`, set by `setTextColor`) pick
a *palette index 0..15*; `drawGlyph` maps the glyph's 1bpp bits to that 4bpp index, and the
CLUT entry supplies the actual colour. `FONT_CLUT` entries 0/3/4,6/5,7,… are the ink
colours; index 0 is transparent-ish black.

Call sites that consume this API (there are dozens; these are representative):
`src/main/script_textbox.c` (dialogue), `src/main/script_common.c:974,980` and
`:3013` (choice cursor width = `chars * GLYPH_WIDTH + 2`), `src/btl/command_shout.c:80-82`,
`src/shop/script_menu.c:3316-3317`, `src/kar/kar.c:979,2490-2536` (`setTextColor`),
`src/trn/trn_hud.c:170-187`, `src/trn2/trn2_hud.c:373-390`, `src/std/std_main.c:1637-1638`.

---

## 3. Format — what the "font sheet" actually is

There is no TIM, so the brief's TIM-header decode has no font to apply to. What the pack
must produce instead is the built-in structure:

```
CHAR_TO_GLYPH_TABLE[992]  : u16, N = 992, index = glyph number
                            Shift-JIS 2-byte codes, stored little-endian in the file
                            1 blank slot (index 59 = 0xFFFF)
                            1 sentinel   (index 991 = 0x0000) — also the missing-glyph cell
GLYPH_DATA[992]           : 11 × u16 per glyph, little-endian, row-major
                            row word: bit 15 = leftmost of 12 px ... bit 4 = rightmost
                            bit == 0 -> ink, bit == 1 -> transparent
                            bits 3..0 unused
FONT_CLUT[16]             : u16 BGR555, uploaded to VRAM (208,488)
```

**Pixel format of the glyph source**: 1bpp-in-12-of-16-bits, 12 × 11 cell, expanded at
runtime to 4bpp and CLUT-looked-up. `NUM_RENDER_SLOTS` = 21 scratch slots × 72 B (`font.c:23-28`).

Palette as shipped (`font.c:1491-1496`), BGR555:

| idx | value | idx | value |
|---|---|---|---|
| 0 | `0x0000` | 8 | `0x0400` |
| 1 | `0x6B5A` | 9 | `0x4210` |
| 2 | `0x7C00` | 10 | `0x2E3C` |
| 3 | `0x14DC` | 11..15 | `0x0000` |
| 4 | `0x6A7F` | | |
| 5 | `0x4B73` | | |
| 6 | `0x7EB2` | | |
| 7 | `0x03FF` | | |

Glyph 991, the missing-codepoint fallback, is a **hollow 12×11 box** — so any zh-Hant
character a pack forgets renders as a tofu box, not as garbage. That is a genuinely useful
implementation property.

---

## 4. Coverage

### The shipped JP set

- `NUM_GLYPHS` = **992** table slots; **990 usable codepoints** — one `0xFFFF` blank at
  **index 60**, one `0x0000` sentinel at **index 991**.
- All 990 resolve as valid Shift-JIS. Distinct codepoint ranges (SJIS lead byte):

  `0x81` 28 · `0x82` 131 · `0x83` 81 · `0x88` 18 · `0x89` 54 · `0x8A` 43 · `0x8B` 46 ·
  `0x8C` 59 · `0x8D` 39 · `0x8E` 60 · `0x8F` 47 · `0x90` 55 · `0x91` 49 · `0x92` 45 ·
  `0x93` 50 · `0x94` 38 · `0x95` 41 · `0x96` 46 · `0x97` 52 · `0x98` 8

- In JIS X 0208 `ku` terms: rows **1-5** (punctuation, digits, Latin, hiragana at ku 4,
  katakana at ku 5) and rows **16-47** — i.e. **kanji level 1**. **No level-2 rows (48-84)
  at all.** Within level 1 the game ships a curated 990-char subset spread across all 32
  rows rather than a contiguous run, which is why the SJIS lead bytes look scattered; the
  table is a hand-picked list, not a slice.
- 0x81xx = ASCII/punctuation remapped into full-width forms (`sjis.c` `convertAsciiToJis`),
  0x82xx = hiragana, 0x83xx = katakana.

This matters for coverage: the shipped font draws only from the *frequently used* kanji
block, so it overlaps zh-Hant far better than a level-2-heavy set would — but it still only
carries 990 cells, which is the binding constraint (below).

### zh-Hant requirement, measured

Enumeration method: JIS X 0208 by the exact 94×94 `ku`/`ten` grid (6879 assigned);
Big5 by code ordinal (level 1 = `0xA440`-`0xC67E` = **5401** chars; level 2 from `0xC940`
= 6817). Then intersect through Unicode.

**Against a full JIS X 0208 kanji set** (the brief's question):

| quantity | count | % of Big5 L1 |
|---|---|---|
| Big5 level-1 chars present in JIS X 0208 | **4412** | **81.7 %** |
| Big5 level-1 chars absent from JIS X 0208 | **989** | **18.3 %** |
| present in JIS X 0208 kanji **level 1** only | 2385 | 44.2 % |

So ~**18 %** of the common traditional set has no JIS X 0208 glyph at all. Those are the
structurally different characters (simplification/traditional pairs and zh-only
composites) — a Big5→JIS transcoder would have to fall back or draw them fresh.

**Against the set this game actually ships** — the number that matters for the port:

| quantity | count | % |
|---|---|---|
| Big5 level-1 chars present in the shipped 990 | **630** | **11.7 %** |
| Big5 level-1 chars absent from the shipped 990 | **4771** | **88.3 %** |

Only 630 of 5401 common traditional characters can be displayed today; a zh-Hant build needs
roughly **4771 new glyphs** for level 1 alone (+ up to 6817 more if level 2 is wanted).

Cost: 22 B/glyph packed, so 5401 glyphs = 116.0 KiB, level 1+2 (12218) = 262.5 KiB, versus
the shipped 992 (= 21.8 KiB).

### What a zh-Hant pack must replace

1. **Mandatory — `GLYPH_DATA`**: 992 × 22 B at EXE `0x099650`, grown or re-pointed to hold
   ≥5401 unique cells (or a sparse index into a bigger sheet).
2. **Mandatory — `CHAR_TO_GLYPH_TABLE`**: 992 `u16` at `0x098E90`. The table *is* the
   codepoint↔glyph mapping, and it is Shift-JIS-valued. For zh-Hant you must decide the
   encoding: either (a) keep Shift-JIS codes and remap a code→the-traditional-glyph
   (cheap, but capped at JIS-representable codepoints), or (b) change the codepoint space
   to Big5/UTF-16 and re-encode `DG.SCN` — which also touches the text in
   `/SCN/DG.SCN` (LBA 142261, 692224 B) and `sjis.c`.
3. **Optional — `FONT_CLUT`**: 16 `u16` at `0x09EB90`, if the pack wants different ink
   colours. Not required for coverage.
4. Not needed: any disc TIM. Do not patch `/STDDAT/STDTIM.BIN`.

---

## 5. Risks

1. **The font is EXE bytes, not a disc file — this is the single biggest risk.** A
   "replace the font TIM" pack design cannot work. Any pack must patch or overlay RAM at
   `0x80128E90` / `0x80129650` / `0x8012EB90` inside `SLPS_017.97`, which means the pack is
   coupled to (a) exact EXE offsets, (b) `NUM_GLYPHS` = 992 being a *compile-time constant*
   in `font.c:11`, and (c) the 22 B/11-word cell being baked into `drawGlyph`'s unrolled
   expander (`font.c:1625-1694`) and into the 72 B render-slot size (`font.c:28`). Growing
   to 5401 glyphs is not a data swap; the region is only 0x5540 B and is followed
   immediately by `FONT_CLUT` and then more `.data`.
2. **The font exists twice on the disc.** `/MOV_REL.BIN` carries a byte-identical 26136-byte
   copy at `0x47B2C` (§1). A pack that patches only `SLPS_017.97` is incomplete: any code
   path that reloads the overlay and touches that window silently reverts glyphs. This is
   the risk most likely to be missed, because everything *looks* right until a movie plays.
   Any pack format must therefore address the font by **(region, address)** pairs, not by a
   single fixed offset.
3. **Dialog box widths are hard-coded in pixels AND in columns.** `scriptSetTextboxSize`
   (`src/main/script_textbox.c:1267`) computes `width = cols * 12 + 10`, `height = rows * 13
   + 7`, and the string-fitting pass measures columns (`:1208`) — the 12 is hard-coded
   there separately from `GLYPH_WIDTH`. Zh-Hant text is typically *shorter* than JP for the
   same meaning, so overflow is unlikely, but any pack that changes the advance (e.g. to
   11 px or proportional) will misalign every box, every centred string
   (`(length/2)*GLYPH_WIDTH` in `command_shout.c:80`, `item.c:3062`,
   `partner_impl.c:1913`) and every choice cursor (`script_common.c:3013`,
   `script_textbox.c:1031`).
4. **12 px fixed advance, 11 px tall ink, and `TEXT_DRAW_LIMIT` = 244.** `drawGlyph`
   silently **drops** any glyph at `x >= 244` or `y >= 244` (`font.c:1576,1584`) — text
   past that is lost, not clipped. Dense hanzi at 12×11 (the useful box is really 12 wide ×
   11 tall) is at the edge of legibility; traditional characters have more strokes than the
   kana-heavy JP set, so legibility needs sign-off from a native reader, not a pass/fail
   check.
5. **CLUT colours.** Only 16 entries, shared by all text, and the ink index is chosen at
   runtime by `setTextColor` (0..15). Entries 11-15 are `0x0000` — using them for
   anti-aliasing/shading isn't possible without a new palette. `kar.c:2498,2532` uses
   `HINT_HIGHLIGHT_COLOR` (= 6 or 10 depending on region, `kar.c:2469,2473`), so a pack
   that renumbers colours breaks hint highlighting.
6. **Missing-glyph behaviour is a tofu box (glyph 991), not an error.** That is friendly for
   partial packs, but it also means a mis-encoded `DG.SCN` line renders as a row of boxes
   with no diagnostic. `drawGlyph`'s lookup is a linear scan with the sentinel appended at
   index 991 (`font.c:1603`), so an entry that accidentally matches `0x0000` also silently
   becomes a box.
7. **ASCII is remapped, not passed through.** `drawString` on JP takes pre-encoded
   Shift-JIS `u16`; the `0x8140`/`0x8142`/`0x8175` special cases and `convertAsciiToJis`
   (`sjis.c:63-108`) mean a pack cannot assume byte transparency. Any new codepoint space
   must keep those three specials or fix their call sites.
8. **`DG.SCN` is 692 KiB of script with text inline between instructions**
   (see `crates/dw1/src/pc_text.rs` for the same problem on the US side). Rewriting text in
   a new encoding is a byte-length change in a mixed code/data stream — the highest-effort
   part of any real localization, and independent of the font work.

---

## 6. Proposal

Keep the pack model, but target the EXE rather than a disc TIM, and split the problem:

**a. One override file per language, loaded by the pack crate.** Add
`<pack>/font/zh_hant.bin` — a self-describing blob:

```
magic  "DWF1"  u16 format_version
u16 num_glyphs             (>= 992; 5401 for Big5 L1)
u32 table_off, u32 glyph_off, u32 clut_off
u8  encoding               (0 = SJIS u16, 1 = Big5, 2 = UTF-16LE)
then: CHAR_TO_GLYPH_TABLE[num_glyphs] u16
      GLYPH_DATA[num_glyphs]          11 x u16 each, same bit sense (0 = ink)
      FONT_CLUT[16] u16
```

The pack crate (`crates/pack`) already reads the disc and knows `SLPS_017.97`'s LBA
(148059); it can carry the same blob as a *patch set* keyed by RAM address, so the native
build writes into its own emulated RAM at `0x80128E90`/`0x80129650`/`0x8012EB90` instead of
patching the file on disk. That keeps the disc image pristine and lets several languages
coexist. `psyq::libcd::patch_file` (`crates/psyq/src/libcd.rs:452`) is the existing
precedent for the drive serving modified bytes — `crates/dw1/src/pc_text.rs` already uses it
to rewrite `\SCN\DG.SCN`.

**The blob must name every region that carries a copy.** As §1 shows, `SLPS_017.97` and
`/MOV_REL.BIN` both hold the font; the loader should apply the same payload to both, and
the format should carry an explicit region list rather than one offset, so a future
discovery of a third copy is a data edit and not a code change.

**b. Make `NUM_GLYPHS` and the cell size runtime data.** The blob's `num_glyphs` is
authoritative. The minimum change to `font.c` is to read the count from a variable seeded
from the blob (defaulting to the compiled constant) and to replace `drawGlyph`'s
`for (n = 0;; ++n)` linear scan with a small lookup (a 64K-entry `u8`/`u16` direct table is
128 KiB and makes the scan O(1)); keep the 22 B packed layout so the existing expander is
untouched.

**c. Encoding decision — recommend keeping Shift-JIS codepoints (option a) for v1.** It
avoids re-encoding `DG.SCN` entirely and keeps `sjis.c` and all the `0x8140` specials
working; the cost is that the codepoint space is capped at what Shift-JIS can express, so
the ~989 Big5-L1 characters with no JIS X 0208 mapping need either a Private Use Area
convention or a banked extension (e.g. `0xFA40`-`0xFC4B` user-defined rows, which `drawGlyph`
treats like any other code). A Big5/UTF-16 mode is the right long-term answer and is why the
blob carries an `encoding` byte, but it pulls in the `DG.SCN` re-encoder and should be a
separate step.

**d. Do not touch `/STDDAT/STDTIM.BIN`, `/STDDAT/TIME.TIM`, or `/CHDAT/ALLTIM.TIM`.** They
are UI/character art; editing them gains nothing for text coverage.

**e. Ship a glyph-coverage check as part of the pack build.** The 990-glyph table, the
5401-char Big5 L1 set and the 4771-glyph gap are all cheap to compute offline (see
`cwps4/coverage.py`); a pack should fail its build if any character used in `DG.SCN` has no
glyph, rather than letting it render as glyph 991's tofu box at runtime.

**f. Get the 12 px advance reviewed by a native zh-Hant reader early.** It is the one item
here that cannot be validated by counting.

---

### Reproducing this

Scratch scripts live in `cwps4/` (not part of the build):

| script | what it does |
|---|---|
| `iso_font_scan.py` | ISO9660 walk; lists font-ish / TIM-magic files with LBA+size |
| `iso_walk.py` | full 818-file listing into `iso_list.txt` |
| `verify_font_location.py` | extracts `SLPS_017.97`, proves `GLYPH_DATA`/`FONT_CLUT` are in it |
| `font_blob.py` | locates `CHAR_TO_GLYPH_TABLE`, decodes `/STDDAT/STDTIM.BIN` |
| `render_glyphs.py` | ASCII-art dump of selected glyphs; confirms 12×11 and `0 = ink` |
| `tim_inventory.py` | every TIM block on the disc; renders the glyph-991 fallback |
| `coverage.py` | JIS X 0208 / Big5 L1+L2 enumeration and the 81.7 % / 11.7 % figures |
