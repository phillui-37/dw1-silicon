//! The decomp's game C, compiled for arm64 (see `build.rs`), and the Rust view of it: extern
//! declarations for the C functions Rust calls, and bindgen bindings for the decomp and SDK
//! structs Rust reads, each checked against its PS1 size.

// Keep the psyq symbols the C needs in the link.
use psyq as _;

pub mod overlays;

/// The decomp version target this build compiles (`DW1_VERSION`, default `us`).
pub const DW1_VERSION: &str = env!("DW1_VERSION");

#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code,
    clippy::all,
    clippy::pedantic,
    unsafe_op_in_unsafe_fn
)]
pub mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

/// PS1 layout sizes. A struct shared with saves, disc data or Rust must keep its PS1 size;
/// growing one (a pointer or `long` member) fails the build here.
const _: () = {
    assert!(size_of::<bindings::DigimonPara>() == 0x34);
    assert!(size_of::<bindings::PartnerPara>() == 0x84);
    // SDK types with `long` members: 32-bit `long` (dw1_native.h) keeps the PS1 layout.
    assert!(size_of::<bindings::VECTOR>() == 16);
    assert!(size_of::<bindings::SVECTOR>() == 8);
    assert!(size_of::<bindings::MATRIX>() == 32);
    // A TMD object record and the model header keep the file layout (native libgs.h).
    assert!(size_of::<bindings::TMD_STRUCT>() == 28);
    assert!(core::mem::offset_of!(bindings::TMDModel, obj) == 12);
    assert!(core::mem::offset_of!(bindings::MATRIX, t) == 20);
};

/// psyq's libcd structs have the layout of Psy-Q's `CdlLOC` and `CdlFILE`.
const _: () = {
    use core::mem::{offset_of, size_of};
    assert!(size_of::<psyq::libcd::CdlLoc>() == size_of::<bindings::CdlLOC>());
    assert!(size_of::<psyq::libcd::CdlLoc>() == 4);
    assert!(size_of::<psyq::libcd::CdlFile>() == size_of::<bindings::CdlFILE>());
    assert!(size_of::<psyq::libcd::CdlFile>() == 24);
    assert!(offset_of!(psyq::libcd::CdlFile, size) == offset_of!(bindings::CdlFILE, size));
    assert!(offset_of!(psyq::libcd::CdlFile, name) == offset_of!(bindings::CdlFILE, name));
    assert!(offset_of!(psyq::libcd::CdlLoc, sector) == offset_of!(bindings::CdlLOC, sector));
    assert!(offset_of!(psyq::libcd::CdlLoc, track) == offset_of!(bindings::CdlLOC, track));
};

/// psyq's libmcrd `DirEntry` has the layout of `struct DIRENTRY` (native pointer included).
const _: () = {
    use core::mem::{offset_of, size_of};
    use psyq::libmcrd::DirEntry;
    assert!(size_of::<DirEntry>() == size_of::<bindings::DIRENTRY>());
    assert!(offset_of!(DirEntry, name) == offset_of!(bindings::DIRENTRY, name));
    assert!(offset_of!(DirEntry, attr) == offset_of!(bindings::DIRENTRY, attr));
    assert!(offset_of!(DirEntry, size) == offset_of!(bindings::DIRENTRY, size));
    assert!(offset_of!(DirEntry, next) == offset_of!(bindings::DIRENTRY, next));
    assert!(offset_of!(DirEntry, head) == offset_of!(bindings::DIRENTRY, head));
    assert!(offset_of!(DirEntry, system) == offset_of!(bindings::DIRENTRY, system));
};

/// psyq's `TimImage` has the layout of `TIM_IMAGE`.
const _: () = {
    use core::mem::{offset_of, size_of};
    assert!(size_of::<psyq::libgpu::TimImage>() == size_of::<bindings::TIM_IMAGE>());
    assert!(offset_of!(psyq::libgpu::TimImage, paddr) == offset_of!(bindings::TIM_IMAGE, paddr));
};

/// The SDK globals psyq defines in Rust have the layout the C headers declare.
const _: () = {
    // libgpu/libgs structs psyq reads and writes.
    assert!(size_of::<psyq::libgpu::Rect>() == size_of::<bindings::RECT>());
    assert!(size_of::<psyq::libgpu::DrawEnv>() == size_of::<bindings::DRAWENV>());
    assert!(
        core::mem::offset_of!(psyq::libgpu::DrawEnv, dr_env)
            == core::mem::offset_of!(bindings::DRAWENV, dr_env)
    );
    assert!(
        core::mem::offset_of!(psyq::libgpu::DrawEnv, isbg)
            == core::mem::offset_of!(bindings::DRAWENV, isbg)
    );
    assert!(size_of::<psyq::libgpu::DispEnv>() == size_of::<bindings::DISPENV>());
    assert!(
        core::mem::offset_of!(psyq::libgpu::DispEnv, isrgb24)
            == core::mem::offset_of!(bindings::DISPENV, isrgb24)
    );
    assert!(size_of::<psyq::libgpu::DrOffset>() == size_of::<bindings::DR_OFFSET>());
    assert!(size_of::<psyq::libgs::GsOt>() == size_of::<bindings::GsOT>());
    assert!(
        core::mem::offset_of!(psyq::libgs::GsOt, org) == core::mem::offset_of!(bindings::GsOT, org)
    );
    assert!(
        core::mem::offset_of!(psyq::libgs::GsOt, point)
            == core::mem::offset_of!(bindings::GsOT, point)
    );
    assert!(
        core::mem::offset_of!(psyq::libgs::GsOt, tag) == core::mem::offset_of!(bindings::GsOT, tag)
    );
    assert!(size_of::<psyq::libgs::GsDobj2>() == size_of::<bindings::GsDOBJ2>());
    assert!(
        core::mem::offset_of!(psyq::libgs::GsDobj2, tmd)
            == core::mem::offset_of!(bindings::GsDOBJ2, tmd)
    );
    assert!(size_of::<psyq::libgs::GsCoordinate2>() == size_of::<bindings::GsCOORDINATE2>());
    assert!(
        core::mem::offset_of!(psyq::libgs::GsCoordinate2, sup)
            == core::mem::offset_of!(bindings::GsCOORDINATE2, super_)
    );
    assert!(size_of::<psyq::libgs::GsImage>() == size_of::<bindings::GsIMAGE>());
    assert!(
        core::mem::offset_of!(psyq::libgs::GsImage, clut)
            == core::mem::offset_of!(bindings::GsIMAGE, clut)
    );
    assert!(size_of::<psyq::libgs::GsRview2>() == size_of::<bindings::GsRVIEW2>());
    assert!(
        core::mem::offset_of!(psyq::libgs::GsRview2, sup)
            == core::mem::offset_of!(bindings::GsRVIEW2, super_)
    );
    assert!(size_of::<psyq::libgs::GsView2>() == size_of::<bindings::GsVIEW2>());
    assert!(
        core::mem::offset_of!(psyq::libgs::GsView2, sup)
            == core::mem::offset_of!(bindings::GsVIEW2, super_)
    );
    assert!(size_of::<psyq::libgs::GsFLight>() == size_of::<bindings::GsF_LIGHT>());
    assert!(
        core::mem::offset_of!(psyq::libgs::GsFLight, r)
            == core::mem::offset_of!(bindings::GsF_LIGHT, r)
    );
    assert!(size_of::<psyq::globals::Matrix>() == size_of::<bindings::MATRIX>());
    assert!(
        core::mem::offset_of!(psyq::globals::Matrix, t)
            == core::mem::offset_of!(bindings::MATRIX, t)
    );
    assert!(
        size_of::<[Option<unsafe extern "C" fn()>; psyq::globals::GS_FCALL_SLOTS]>()
            == size_of::<bindings::_GsFCALL>()
    );
};

/// The libsnd/libspu structs psyq mirrors have the layout the C headers declare.
const _: () = {
    use core::mem::{offset_of, size_of};
    assert!(size_of::<psyq::libsnd::VabHdr>() == size_of::<bindings::VabHdr>());
    assert!(offset_of!(psyq::libsnd::VabHdr, ps) == offset_of!(bindings::VabHdr, ps));
    assert!(offset_of!(psyq::libsnd::VabHdr, ts) == offset_of!(bindings::VabHdr, ts));
    assert!(offset_of!(psyq::libsnd::VabHdr, vs) == offset_of!(bindings::VabHdr, vs));
    assert!(offset_of!(psyq::libsnd::VabHdr, reserved1) == offset_of!(bindings::VabHdr, reserved1));
    assert!(size_of::<psyq::libspu::SpuVolume>() == size_of::<bindings::SpuVolume>());
    assert!(size_of::<psyq::libspu::SpuReverbAttr>() == size_of::<bindings::SpuReverbAttr>());
    assert!(
        offset_of!(psyq::libspu::SpuReverbAttr, mode) == offset_of!(bindings::SpuReverbAttr, mode)
    );
    assert!(
        offset_of!(psyq::libspu::SpuReverbAttr, depth)
            == offset_of!(bindings::SpuReverbAttr, depth)
    );
    assert!(
        offset_of!(psyq::libspu::SpuReverbAttr, delay)
            == offset_of!(bindings::SpuReverbAttr, delay)
    );
    assert!(
        offset_of!(psyq::libspu::SpuReverbAttr, feedback)
            == offset_of!(bindings::SpuReverbAttr, feedback)
    );
};

unsafe extern "C" {
    // src/main/main.c: the game's main(), renamed by dw1_native.h.
    pub fn dw1_game_main() -> i32;

    // src/mov/mov.c: plays a movie (0 = the intro) to its end or until Start; 1 if it ended.
    pub fn MOV_playMovie(movie_id: i32) -> i32;

    // src/main/rng.c (declared with `long`, 32-bit in the native build)
    pub fn customRandom(min: i32, max: i32) -> i32;
    pub static mut CUSTOM_RNG_VAL1: u32;
    pub static mut CUSTOM_RNG_VAL2: u32;

    // src/main/math.c
    pub fn randomLimit(limit: i32) -> i32;
    pub fn _sin(input: i32) -> i32;
    pub fn _cos(input: i32) -> i32;
    pub fn _atan(dy: i32, dx: i32) -> i32;

    // src/main/file_table_us.c (via dw/sin_lookup_table.h)
    pub static SIN_LOOKUP_TABLE: [i16; 130];

    // src/main/main_menu.c: the save buffer (a `SaveFile`, 0x2000 bytes) and the game's own
    // checksum over one of its two payloads.
    pub static mut SAVE_FILE: [u8; 0x2000];
    pub fn createSavegameChecksum(slot: i32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use psyq::bios;

    /// `_sin` as math.c computes it: a quarter-wave table over a 512-step circle.
    fn sin_model(input: i32) -> i32 {
        let table = unsafe { &SIN_LOOKUP_TABLE };
        let i = (input & 0x1FF) as usize;
        match i {
            0..128 => table[i].into(),
            128..256 => table[256 - i].into(),
            256..384 => -i32::from(table[i - 256]),
            _ => -i32::from(table[512 - i]),
        }
    }

    #[test]
    fn trig_uses_the_linked_tables() {
        for input in -600..1200 {
            assert_eq!(unsafe { _sin(input) }, sin_model(input), "_sin({input})");
            assert_eq!(
                unsafe { _cos(input) },
                sin_model(input + 128),
                "_cos({input})"
            );
        }
        // _atan reads ATAN_LOOKUP_TABLE; the four axis directions are a smoke test that it
        // links and stays in range, not a model of it.
        for (dy, dx) in [(0, 100), (100, 0), (0, -100), (-100, 0), (37, -91)] {
            let angle = unsafe { _atan(dy, dx) };
            assert!((0..4096).contains(&angle), "_atan({dy}, {dx}) = {angle}");
        }
    }

    const CUSTOM_RNG_FACTOR: u32 = 0x41C6_50AD;
    const CUSTOM_RNG_VALUE: u32 = 0x3039;

    /// `customRandom` as the PS1 computes it (32-bit `long`, unsigned modulo).
    fn custom_random_model(state: &mut (u32, u32), min: i32, max: i32) -> i32 {
        if min == max {
            return min;
        }
        let (min, max) = if max < min { (max, min) } else { (min, max) };
        state.0 = state
            .0
            .wrapping_mul(CUSTOM_RNG_FACTOR)
            .wrapping_add(CUSTOM_RNG_VALUE);
        state.1 = state
            .1
            .wrapping_mul(CUSTOM_RNG_FACTOR)
            .wrapping_add(CUSTOM_RNG_VALUE);
        let mixed = (state.0 >> 16) | (state.1 << 16);
        let range = (max.wrapping_sub(min).wrapping_add(1)) as u32;
        min.wrapping_add((mixed % range) as i32)
    }

    /// Both generators are global state in C and Rust, so everything stateful runs in this one
    /// test, in order.
    #[test]
    fn game_c_rngs_match_the_ps1_models() {
        // customRandom: C against the model, from the game's initial state.
        let mut model = (0u32, 0x0013_CC25u32);
        unsafe {
            CUSTOM_RNG_VAL1 = model.0;
            CUSTOM_RNG_VAL2 = model.1;
        }
        for (min, max) in [(0, 99), (5, 5), (10, 1), (-50, 50), (0, 0x7FFF), (1, 3)] {
            for _ in 0..100 {
                let c = unsafe { customRandom(min, max) };
                assert_eq!(
                    c,
                    custom_random_model(&mut model, min, max),
                    "customRandom({min}, {max})"
                );
            }
        }

        // randomLimit: the C calls rand(), which must land in psyq's BIOS generator.
        bios::dw1_bios_srand(0x1234_5678);
        let mut state = 0x1234_5678;
        for limit in [1i32, 2, 10, 100, 1000, 0x7FFF, -7] {
            let (next, r) = bios::rand_step(state);
            state = next;
            let expected = limit.wrapping_mul(r) >> 15;
            assert_eq!(
                unsafe { randomLimit(limit) },
                expected,
                "randomLimit({limit})"
            );
        }
    }

    /// Where a `SaveFile` keeps its two payloads and each one's checksum (main_menu.c's
    /// `SavegamePayload`: 0x4FC bytes of data, the checksum, then 40 battle records).
    const PAYLOAD_START: [usize; 2] = [0x200, 0x200 + 0xF00];
    const CHECKSUM_AT: usize = 0x4FC;

    /// The fixture card's bytes. `DW1_FIXTURE_CARD` names one explicitly (an unreadable file
    /// fails the test); otherwise the default fixture, which the build script found (the test
    /// is ignored without it).
    fn fixture() -> Vec<u8> {
        let path = match std::env::var_os("DW1_FIXTURE_CARD") {
            Some(p) => std::path::PathBuf::from(p),
            None => std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../captures/fixtures/ember-day2-gabumon.mcd"),
        };
        std::fs::read(&path).unwrap_or_else(|e| panic!("fixture card {}: {e}", path.display()))
    }

    /// The fixture's 8 KB save block.
    fn fixture_save() -> Vec<u8> {
        let bytes = fixture();
        let card = psyq::libmcrd::Card::from_bytes(&bytes).expect("a 128 KB raw card");
        let files = card.files();
        let file = files
            .iter()
            .find(|f| f.name.starts_with(b"BASLUS-01032DMR"))
            .expect("the DW1 save file");
        assert_eq!(file.blocks.len(), 1);
        let data = card.as_bytes()[(file.blocks[0] + 1) * 8192..][..8192].to_vec();
        // The save header starts with its id, "SC" (a PS1 save file, not the card's "MC").
        assert_eq!(&data[..2], b"SC");
        data
    }

    fn stored(data: &[u8], slot: usize) -> i32 {
        let at = PAYLOAD_START[slot] + CHECKSUM_AT;
        i32::from_le_bytes(data[at..at + 4].try_into().unwrap())
    }

    /// What the CONTINUE read (main_menu.c, menu state 0x19) does with a loaded save buffer, by
    /// the game's own `createSavegameChecksum`: the payload it loads (0 or 1), or `None` for
    /// the "bad data" error. The selection mirrors that function's two `if`s; its effect on a
    /// real card was checked against Redux (docs/phase3-plan.md).
    fn continue_picks(buf: &[u8]) -> Option<usize> {
        unsafe { (*core::ptr::addr_of_mut!(SAVE_FILE)).copy_from_slice(buf) };
        (0..2).find(|&slot| unsafe { createSavegameChecksum(slot as i32) } == stored(buf, slot))
    }

    /// Ember's fixture card, read as-is (DuckStation raw format), holds a save the game's own
    /// checksum accepts in both payloads (the second is the backup copy). A flipped bit in
    /// either checksummed range of a payload rejects that payload: with the first bad the
    /// backup is used, with both bad the load is refused. The save buffer is global state, so
    /// all of it is one test.
    #[test]
    #[cfg_attr(
        not(dw1_fixture_card),
        ignore = "needs captures/fixtures/ember-day2-gabumon.mcd or DW1_FIXTURE_CARD"
    )]
    fn fixture_save_checksums_backup_and_rejection() {
        let data = fixture_save();
        assert_eq!(continue_picks(&data), Some(0), "both payloads valid");
        assert_ne!(stored(&data, 0), 0);
        assert_eq!(stored(&data, 1), stored(&data, 0), "the backup is a copy");

        // Each corruption: a bit in the 0x4E4 summed bytes, and one in the 0xA00 battle records.
        let spots = [0x100, 0x500 + 0x10];
        for (i, &spot) in spots.iter().enumerate() {
            let mut bad = data.clone();
            bad[PAYLOAD_START[0] + spot] ^= 1;
            assert_eq!(continue_picks(&bad), Some(1), "payload 0 bad (spot {i})");
            let mut bad = data.clone();
            bad[PAYLOAD_START[1] + spot] ^= 1;
            assert_eq!(continue_picks(&bad), Some(0), "payload 1 bad (spot {i})");
            for (j, &other) in spots.iter().enumerate() {
                let mut bad = data.clone();
                bad[PAYLOAD_START[0] + spot] ^= 1;
                bad[PAYLOAD_START[1] + other] ^= 1;
                assert_eq!(continue_picks(&bad), None, "both bad (spots {i}, {j})");
            }
        }
    }
}

/// The build script's CP932 literal encoder, tested here (build scripts have no tests).
#[cfg(test)]
mod literals;

#[cfg(test)]
mod literal_tests {
    use super::literals::{encode_literals, literal_chars};

    /// Python's cp932 for the characters used here (build.rs asks Python for the real table).
    fn cp932(c: char) -> Option<Vec<u8>> {
        match c {
            'あ' => Some(vec![0x82, 0xA0]),
            'Ｂ' => Some(vec![0x82, 0x61]),
            '〜' => Some(vec![0x81, 0x60]),
            _ => None,
        }
    }

    fn enc(src: &str) -> Option<String> {
        encode_literals(src, "test.c", &mut cp932)
    }

    #[test]
    fn literals_become_cp932_escapes() {
        assert_eq!(
            enc("char s[] = \"あＢ1〜\";").unwrap(),
            "char s[] = \"\\202\\240\\202\\1411\\201\\140\";"
        );
        assert_eq!(enc("int c = 'あ';").unwrap(), "int c = '\\202\\240';");
        // Escaped quotes don't end the literal.
        assert_eq!(enc("\"\\\"あ\"").unwrap(), "\"\\\"\\202\\240\"");
        // Universal-character names are encoded the same way; ASCII ones are left alone.
        assert_eq!(
            enc("\"\\u3042\\U00003042\\u0041\"").unwrap(),
            "\"\\202\\240\\202\\240\\u0041\""
        );
        assert_eq!(
            literal_chars("/* ア */ \"あ\\u3042\" 'Ｂ'"),
            ['あ', 'あ', 'Ｂ']
        );
    }

    #[test]
    fn everything_outside_literals_is_untouched() {
        assert_eq!(enc("/* \"あ\" */ // 'あ'\nint x;"), None);
        assert_eq!(enc("int x; /* it's */ char y = 'a';"), None);
        // A spliced line comment stays a comment on the next line.
        assert_eq!(enc("// note \\\n\"あ\"\nint x;"), None);
        assert_eq!(enc("// note \\\r\n\"あ\"\nint x;"), None);
    }

    #[test]
    fn splices_are_removed_before_lexing_and_kept_in_the_output() {
        // Inside a literal, LF and CRLF.
        assert_eq!(enc("\"a\\\r\nあ\"").unwrap(), "\"a\\\r\n\\202\\240\"");
        assert_eq!(enc("\"a\\\nあ\"").unwrap(), "\"a\\\n\\202\\240\"");
        // A block-comment terminator split by a splice still ends the comment.
        assert_eq!(
            enc("/* x *\\\n/ \"あ\"").unwrap(),
            "/* x *\\\n/ \"\\202\\240\""
        );
        // A splice inside a \u escape: the bytes replace the escape, the splice stays.
        assert_eq!(enc("\"\\u30\\\n42\"").unwrap(), "\"\\202\\240\\\n\"");
        // Splices at the very end survive.
        assert_eq!(enc("\"あ\"\\\n").unwrap(), "\"\\202\\240\"\\\n");
    }

    #[test]
    #[should_panic(expected = "test.c: no CP932 encoding for '😀'")]
    fn an_unencodable_character_names_its_file() {
        enc("\"😀\"");
    }
}
