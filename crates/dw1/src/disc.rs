//! Finding the player's disc image (Phase 9.1). The window build uses, in order: `DW1_DISC`, the
//! image chosen on an earlier run (`disc.txt` in Application Support), and the repo's
//! `disc/Digimon World (USA).cue` (a build run where it was made); with none of them it asks
//! (`Window::choose_disc`). Every image is checked against the redump's SHA-1 before the game
//! starts, and a wrong one gets a plain message saying what it looks like. The image is only
//! read: never copied or changed. `DW1_ASK_DISC=1` asks even when an image is known (to choose
//! another, or to see the first run).

use std::path::{Path, PathBuf};

use pack::Disc;

/// Where the chosen image's path is kept, or None without a home directory.
fn remembered_file() -> Option<PathBuf> {
    crate::default_save_dir().map(|d| d.join("disc.txt"))
}

/// The image chosen on an earlier run (the file's first line), if any.
pub fn remembered() -> Option<PathBuf> {
    read_remembered(&remembered_file()?)
}

fn read_remembered(file: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(file).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| PathBuf::from(line))
}

/// Keeps `image` for the next runs; a failure is reported and otherwise ignored (the player is
/// asked again next time).
pub fn remember(image: &Path) {
    if let Some(file) = remembered_file()
        && let Err(e) = write_remembered(&file, image)
    {
        eprintln!(
            "dw1: could not remember the disc in {}: {e}",
            file.display()
        );
    }
}

fn write_remembered(file: &Path, image: &Path) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&tmp, format!("{}\n", image.display()))?;
    std::fs::rename(&tmp, file)
}

/// The repo's image, where the build expects it (`DW1_DISC`'s default).
pub fn repo_image() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../disc/Digimon World (USA).cue")
}

/// Opens and checks the image at `path` (a `.cue`, or a `.bin` with or without its `.cue`).
/// The error is a sentence or two for the player.
pub fn open_checked(path: &Path) -> Result<Disc, String> {
    let disc = open(path)?;
    // Reads the whole image once (about a second), so a wrong or damaged dump is caught here
    // and not as garbage in the game.
    match disc.verify_as(decomp_c::DW1_VERSION) {
        Ok(()) => Ok(disc),
        Err(pack::DiscError::Checksum { .. }) => Err(wrong_image(disc.boot_name().as_deref())),
        Err(e) => Err(format!("The image couldn't be read: {}.", describe(&e))),
    }
}

/// A path's file name, for messages (the whole path if it has none).
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// A disc error for the screen: a read failure gives its cause and the file's name (the whole
/// path, which can be long, goes to stderr with the message).
fn describe(e: &pack::DiscError) -> String {
    match e {
        pack::DiscError::Io(p, io) => format!("{} ({})", io.kind(), file_name(p)),
        e => e.to_string(),
    }
}

/// A file that isn't there, said plainly (the `.cue`, or the `.bin` it names).
fn gone(e: &pack::DiscError) -> Option<String> {
    match e {
        pack::DiscError::Io(p, io) if io.kind() == std::io::ErrorKind::NotFound => Some(format!(
            "{} isn't there any more (moved or renamed?).",
            file_name(p)
        )),
        _ => None,
    }
}

fn open(path: &Path) -> Result<Disc, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "cue" => Disc::open(path).map_err(|e| match e {
            ref e if gone(e).is_some() => gone(e).unwrap_or_default(),
            pack::DiscError::Cue(_) => "This cue sheet isn't a single-track PlayStation image \
                                        (the USA release is one MODE2/2352 track)."
                .to_string(),
            e => format!("The image couldn't be opened: {}.", describe(&e)),
        }),
        "bin" => {
            // The cue sheet beside it names the same file; without one the .bin is read as
            // the single MODE2/2352 track it should be.
            let cue = path.with_extension("cue");
            if cue.exists() {
                open(&cue)
            } else {
                Disc::open_bin(path).map_err(|e| match e {
                    ref e if gone(e).is_some() => gone(e).unwrap_or_default(),
                    pack::DiscError::BadSize(_) => {
                        "This .bin isn't a raw PlayStation image (MODE2/2352 sectors).".to_string()
                    }
                    e => format!("The image couldn't be opened: {}.", describe(&e)),
                })
            }
        }
        "chd" | "pbp" | "ecm" | "cso" | "zip" | "7z" | "rar" => Err(format!(
            "A .{ext} file is compressed. The game needs the plain .cue/.bin pair (chdman \
             extractcd makes one from a .chd)."
        )),
        "iso" | "img" | "mdf" | "ccd" | "toc" | "nrg" => Err(format!(
            "A .{ext} image isn't read: the game needs the raw .cue/.bin pair (the redump \
             layout), which keeps the movies' and voices' sectors."
        )),
        // another name (a link, no extension): a cue sheet, else a raw image, as DW1_DISC always
        // allowed
        _ => Disc::open(path)
            .or_else(|_| Disc::open_bin(path))
            .map_err(|_| "Choose the .cue file of the disc image (or its .bin).".to_string()),
    }
}

/// The message for an image whose hash isn't the redump's, from the executable it boots.
fn wrong_image(boot: Option<&str>) -> String {
    match boot {
        Some(pack::USA_BOOT) => "This is the USA release, but the image differs from the known \
                                 good dump (a damaged, patched or translated copy?). The game \
                                 needs an unmodified redump-verified image."
            .to_string(),
        Some(b) if b.starts_with("SLES") => {
            format!("This is a European disc ({b}). The game is the USA release (SLUS-01032) only.")
        }
        Some(b) if b.starts_with("SLPS") || b.starts_with("SCPS") => {
            format!("This is a Japanese disc ({b}). The game is the USA release (SLUS-01032) only.")
        }
        Some(b) => {
            format!("This disc boots {b}, not Digimon World (USA). The game needs SLUS-01032.")
        }
        None => "This isn't a PlayStation disc image the game knows. It needs Digimon World \
                 (USA), SLUS-01032."
            .to_string(),
    }
}

/// Where the window build's image came from: what a failed check means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// `DW1_DISC`: a wrong image stops the program, as before.
    Env,
    /// Chosen on an earlier run (`disc.txt`).
    Remembered,
    /// The repo's `disc/`.
    Repo,
    /// Chosen just now on the first-run screen.
    Chosen,
}

/// What the window build starts with: an image to check (`Window::find_disc` checks it with the
/// window answering meanwhile), or the first-run screen with why it's needed (None: no image
/// known yet).
#[derive(Debug, PartialEq, Eq)]
pub enum Start {
    Check(PathBuf, Origin),
    Ask(Option<String>),
}

pub fn at_startup() -> Start {
    if let Some(path) = std::env::var_os("DW1_DISC").filter(|v| !v.is_empty()) {
        return Start::Check(PathBuf::from(path), Origin::Env);
    }
    if std::env::var_os("DW1_ASK_DISC").is_some_and(|v| !v.is_empty() && v != "0") {
        return Start::Ask(None);
    }
    // (not looked at here: a stalled drive would hold up the window; the check says if it's gone)
    if let Some(path) = remembered() {
        return Start::Check(path, Origin::Remembered);
    }
    let repo = repo_image();
    if repo.exists() {
        return Start::Check(repo, Origin::Repo);
    }
    Start::Ask(None)
}

/// What the first-run screen says when the image at `path` (from `origin`) failed with `err`.
pub fn problem(origin: Origin, path: &Path, err: &str) -> String {
    match origin {
        // the reason first: a long path would push it off the screen
        Origin::Remembered => format!("{err} (The image chosen before: {}.)", file_name(path)),
        _ => err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_remembered_path_round_trips() {
        let dir = std::env::temp_dir().join(format!("dw1-disc-test-{}", std::process::id()));
        let file = dir.join("disc.txt");
        assert_eq!(read_remembered(&file), None);
        let image = Path::new("/Volumes/Games/Digimon World (USA).cue");
        write_remembered(&file, image).unwrap();
        assert_eq!(read_remembered(&file).as_deref(), Some(image));
        std::fs::write(&file, "\n").unwrap();
        assert_eq!(read_remembered(&file), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn wrong_files_get_a_plain_reason() {
        let dir = std::env::temp_dir().join(format!("dw1-disc-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let msg = |name: &str| open(&dir.join(name)).unwrap_err();
        assert!(msg("game.chd").contains("compressed"));
        assert!(msg("game.iso").contains("raw .cue/.bin"));
        assert!(msg("notes.txt").contains("Choose the .cue"));
        // a .bin that isn't whole sectors, alone and named by a cue sheet beside it
        std::fs::write(dir.join("short.bin"), [0u8; 100]).unwrap();
        assert!(msg("short.bin").contains("MODE2/2352"));
        std::fs::write(
            dir.join("short.cue"),
            "FILE \"short.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
        )
        .unwrap();
        assert!(msg("short.bin").contains("couldn't be opened"));
        // a synthetic image: opens, but it isn't the game
        std::fs::write(dir.join("other.bin"), pack::testing::synthetic_image()).unwrap();
        let err = open_checked(&dir.join("other.bin")).err().unwrap();
        assert!(
            err.contains("isn't a PlayStation disc image the game knows"),
            "{err}"
        );
        // gone: the .cue, or the .bin a .cue names
        assert!(msg("missing.cue").contains("missing.cue isn't there"));
        std::fs::write(
            dir.join("lost.cue"),
            "FILE \"lost.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
        )
        .unwrap();
        assert!(msg("lost.cue").contains("lost.bin isn't there"));
        // a link without an extension to a real cue sheet still opens (DW1_DISC allowed it)
        std::fs::write(
            dir.join("other.cue"),
            "FILE \"other.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("other.cue"), dir.join("disc-link")).unwrap();
            assert!(open(&dir.join("disc-link")).is_ok());
        }
        // a read error names its cause before the file, not the whole path
        let e = pack::DiscError::Io(
            PathBuf::from("/Volumes/A Long Library/Games/x.bin"),
            std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        );
        assert_eq!(describe(&e), "permission denied (x.bin)");
        let long = Path::new("/Volumes/A Long Library/Games/PlayStation/Digimon World (USA).cue");
        let p = problem(Origin::Remembered, long, "lost.bin isn't there any more.");
        assert!(p.starts_with("lost.bin isn't there"), "{p}");
        assert!(!p.contains("/Volumes"), "{p}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_real_image_checks_out_when_present() {
        let repo = repo_image();
        if !repo.exists() {
            eprintln!("no disc image at {}: skipped", repo.display());
            return;
        }
        let disc = open_checked(&repo).unwrap();
        assert_eq!(disc.boot_name().as_deref(), Some(pack::USA_BOOT));
        // the .bin alone (its .cue beside it is used)
        let bin = repo.with_extension("bin");
        if bin.exists() {
            open_checked(&bin).unwrap();
        }
    }

    #[test]
    fn a_wrong_release_is_named() {
        assert!(wrong_image(Some("SLUS_010.32")).contains("differs from the known good dump"));
        assert!(wrong_image(Some("SLES_028.14")).contains("European"));
        assert!(wrong_image(Some("SLPS_017.97")).contains("Japanese"));
        assert!(wrong_image(Some("SCUS_944.26")).contains("boots SCUS_944.26"));
    }
}
