//! The library's folder: one file, its pictures, and the atomic write that keeps both whole.
//!
//! The folder is the workshop's settings folder, so `library.json` and `textures/` sit beside
//! `settings.json` and a shop moves one folder to carry its whole workspace. The library file itself
//! is portable: a record names its picture by a relative path, so a folder copied to another machine
//! arrives with the same materials.
//!
//! Reading is one value or one refusal naming the record and the file; writing is a temporary
//! sibling that is flushed, synced and renamed over the target, so a crash during a save cannot
//! leave a half-written library behind. A missing file is an empty library and not a failure: a
//! workspace that has never named a material keeps the shell's placeholder tones.
//!
//! [`Revision`] is the cheap half, as in the nomenclature directory: one stat of the file and one of
//! each picture, so a window can poll a folder a person edits by hand without reading it.

use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::id::MaterialId;
use crate::library::Library;
use crate::material::{Base, Material};
use crate::picture::{Picture, PictureError};
use crate::wire::{LIBRARY_VERSION, LibraryWire};

/// The library's file inside the workshop's settings folder.
pub const LIBRARY_FILE: &str = "library.json";

/// The folder of decor pictures beside the library, which its records name relatively.
pub const TEXTURES_DIR: &str = "textures";

/// Why a library could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    /// A file could not be read.
    #[error("cannot read {path}: {reason}")]
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The reason the operating system gave.
        reason: String,
    },
    /// The library file is not the JSON this build reads.
    #[error("{path} is not a material library this build reads: {reason}")]
    Parse {
        /// The file that was read.
        path: PathBuf,
        /// What the parser said.
        reason: String,
    },
    /// The file declares a schema this build does not read.
    #[error("{path} declares schema {found}; this build reads {expected}")]
    Version {
        /// The file that was read.
        path: PathBuf,
        /// The version the file declares.
        found: u32,
        /// The version this build writes.
        expected: u32,
    },
    /// One record of the file breaks a rule.
    #[error("{path}{}: {reason}", .material.map(|id| format!(", {}", id)).unwrap_or_default())]
    Record {
        /// The file that was read.
        path: PathBuf,
        /// The record the refusal is about, when one record is to blame.
        material: Option<MaterialId>,
        /// What is wrong with it.
        reason: String,
    },
    /// A decor names a picture that is not there, or one this build cannot read.
    #[error("{path}: material {material} names the picture {picture}: {reason}")]
    Picture {
        /// The file that was read.
        path: PathBuf,
        /// The record whose decor named the picture.
        material: MaterialId,
        /// The picture, resolved against the library's folder.
        picture: PathBuf,
        /// What is wrong with the picture.
        reason: String,
    },
}

/// Why a library could not be written.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SaveError {
    /// The file could not be written.
    #[error("cannot write {path}: {reason}")]
    Write {
        /// The file that could not be written.
        path: PathBuf,
        /// The reason the operating system gave.
        reason: String,
    },
}

/// A stamp of the library's folder: the file and every picture its records name.
///
/// Two reads of one untouched folder answer the same number, and replacing one scan answers another,
/// which is what the shell polls for (`crate::Store::revision`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Revision(u64);

impl Revision {
    /// The number itself, for a log line or a test that prints it.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// The folder a workshop's material library lives in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// The store in `root`, the folder that also holds the settings file.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The folder this store reads and writes.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The library's file.
    pub fn library_path(&self) -> PathBuf {
        self.root.join(LIBRARY_FILE)
    }

    /// The folder the decor pictures live in.
    pub fn textures(&self) -> PathBuf {
        self.root.join(TEXTURES_DIR)
    }

    /// Reads one whole library, or the sentence that refused it.
    ///
    /// A missing file is an empty library: a workshop that has named no material still opens a
    /// scene, and the shell draws the tones it always did. A file that is there but breaks a rule is
    /// a refusal, not a partial library.
    pub fn read(&self) -> Result<Library, LoadError> {
        let path = self.library_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Library::default()),
            Err(error) => return Err(LoadError::Read { path, reason: error.to_string() }),
        };
        let wire: LibraryWire = serde_json::from_str(&text)
            .map_err(|error| LoadError::Parse { path: path.clone(), reason: error.to_string() })?;
        if wire.version != LIBRARY_VERSION {
            return Err(LoadError::Version { path, found: wire.version, expected: LIBRARY_VERSION });
        }
        let library = wire.into_library(&path)?;
        self.check_pictures(&library, &path)?;
        Ok(library)
    }

    /// That every picture a record names is a file this build can start to read.
    ///
    /// Only the file's presence is checked here, not its texels: a library of thirty decors must not
    /// decode thirty megabytes to open a window. A picture that cannot be decoded is a refusal when
    /// a frame asks for it ([`Store::picture`]), which names the record the same way.
    fn check_pictures(&self, library: &Library, path: &Path) -> Result<(), LoadError> {
        for (material, decor) in library.pictures() {
            let picture = self.root.join(decor.picture());
            match fs::metadata(&picture) {
                Ok(metadata) if metadata.is_file() => {}
                Ok(_) => {
                    return Err(LoadError::Picture {
                        path: path.to_path_buf(),
                        material: material.id(),
                        picture,
                        reason: "is a folder, not a picture".to_owned(),
                    });
                }
                Err(error) => {
                    return Err(LoadError::Picture {
                        path: path.to_path_buf(),
                        material: material.id(),
                        picture,
                        reason: error.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Writes the library atomically, creating its folder if it is missing.
    pub fn write(&self, library: &Library) -> Result<(), SaveError> {
        let path = self.library_path();
        let mut text = serde_json::to_string_pretty(&library.to_wire())
            .map_err(|error| SaveError::Write { path: path.clone(), reason: error.to_string() })?;
        text.push('\n');
        write_atomically(&path, text.as_bytes())
    }

    /// The stamp of the file and of every picture its records name, without reading any of them.
    ///
    /// A file that is not there is part of the stamp too, so a folder that gains its first library
    /// moves the stamp and a window that is waiting reloads. A file that does not parse still
    /// contributes its own stamp: a person fixing it moves it again.
    pub fn revision(&self) -> Result<Revision, LoadError> {
        let mut hasher = DefaultHasher::new();
        let path = self.library_path();
        path.hash(&mut hasher);
        stamp(&path)?.hash(&mut hasher);
        if let Ok(text) = fs::read_to_string(&path)
            && let Ok(wire) = serde_json::from_str::<LibraryWire>(&text)
        {
            for material in &wire.materials {
                let Some(decor) = material.base.as_ref().and_then(|base| base.decor.as_ref()) else {
                    continue;
                };
                let picture = self.root.join(&decor.picture);
                picture.hash(&mut hasher);
                stamp(&picture)?.hash(&mut hasher);
            }
        }
        Ok(Revision(hasher.finish()))
    }

    /// The picture `material`'s decor names, or `None` when it has no decor.
    ///
    /// Every frame asks only for the materials it draws, so a library of thirty decors decodes the
    /// ones on screen. A picture that is not there or does not decode is a refusal naming the record
    /// and the file, never a board that draws black.
    pub fn picture(&self, material: &Material) -> Result<Option<Picture>, LoadError> {
        let Some(decor) = material.base().and_then(Base::decor) else {
            return Ok(None);
        };
        let picture = self.root.join(decor.picture());
        Picture::read(&picture).map(Some).map_err(|error: PictureError| LoadError::Picture {
            path: self.library_path(),
            material: material.id(),
            picture,
            reason: error.to_string(),
        })
    }
}

/// The modification time and size of `path`, or zeroes when it is not there.
fn stamp(path: &Path) -> Result<(u128, u64), LoadError> {
    match fs::metadata(path) {
        Ok(metadata) => {
            let modified = metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok());
            Ok((modified.map_or(0, |elapsed| elapsed.as_nanos()), metadata.len()))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok((0, 0)),
        Err(error) => Err(LoadError::Read { path: path.to_path_buf(), reason: error.to_string() }),
    }
}

/// Writes `bytes` to a temporary sibling, flushes it and renames it over `target`.
fn write_atomically(target: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    let parent = target.parent().ok_or_else(|| SaveError::Write {
        path: target.to_path_buf(),
        reason: "the library path has no folder".to_owned(),
    })?;
    fs::create_dir_all(parent).map_err(|error| refused(parent, &error))?;
    let temporary = target.with_file_name(format!(".{LIBRARY_FILE}.tmp"));
    let result = (|| {
        let mut file = File::create(&temporary).map_err(|error| refused(&temporary, &error))?;
        file.write_all(bytes).map_err(|error| refused(&temporary, &error))?;
        file.sync_all().map_err(|error| refused(&temporary, &error))?;
        drop(file);
        fs::rename(&temporary, target).map_err(|error| refused(target, &error))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// One refusal the operating system gave, naming the file it was about.
fn refused(path: &Path, error: &io::Error) -> SaveError {
    SaveError::Write { path: path.to_path_buf(), reason: error.to_string() }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::path::PathBuf;

    use units::Length;

    use super::*;
    use crate::material::{Coat, Decor, Turns};

    fn id(value: u32) -> MaterialId {
        MaterialId::new(value).expect("a non-zero id")
    }

    fn millimetres(value: i64) -> Length {
        Length::from_millimetres(value).expect("a length")
    }

    /// A store in a folder this test owns, emptied before use.
    fn scratch(name: &str) -> Store {
        Store::at(testkit::scratch_dir(&format!("materials-store-{name}")))
    }

    /// Writes `pixels` as the PNG at `path`.
    fn write_png(path: &Path, pixels: &[[u8; 4]]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("the textures folder");
        }
        let file = File::create(path).expect("a fresh file");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let bytes: Vec<u8> = pixels.iter().flatten().copied().collect();
        let mut writer = encoder.write_header().expect("a PNG header");
        writer.write_image_data(&bytes).expect("the pixels");
    }

    /// A record with every channel away from its default.
    fn full() -> Material {
        let decor = Decor::new("textures/oak.png", millimetres(600), Turns::ACROSS).expect("a decor");
        let base = Base::new([0.62, 0.42, 0.22], Some(decor), 0.7, 0.0, 1.9, 0.3, [0.5, 1.5, 2.5]).expect("a board");
        let coat = Coat::new(0.75, 70.0, 1.62).expect("a coat");
        Material::new(id(1), "Oak, 70 % lacquer", Some(base), Some(coat)).expect("a record")
    }

    #[test]
    fn a_library_round_trips_through_its_file_with_every_channel() {
        let store = scratch("round-trip");
        write_png(&store.textures().join("oak.png"), &[[10, 20, 30, 255]]);
        let library = Library::new([full()]).expect("one record");
        store.write(&library).expect("the write");
        assert!(store.library_path().exists(), "the file is the folder's own name");
        assert!(!store.root().join(format!(".{LIBRARY_FILE}.tmp")).exists(), "the temporary is renamed away");

        let read = store.read().expect("the file this build wrote");
        assert_eq!(read, library);
        let material = read.get(id(1)).expect("the record");
        let base = material.base().expect("a base");
        assert_eq!(base.colour(), [0.62, 0.42, 0.22]);
        assert_eq!((base.roughness(), base.metallic(), base.ior()), (0.7, 0.0, 1.9));
        assert_eq!((base.transmission(), base.absorption()), (0.3, [0.5, 1.5, 2.5]));
        let decor = base.decor().expect("the decor");
        assert_eq!(
            (decor.picture(), decor.covers(), decor.turns()),
            ("textures/oak.png", millimetres(600), Turns::ACROSS)
        );
        let coat = material.coat().expect("the coat");
        assert_eq!((coat.weight(), coat.gloss(), coat.ior()), (0.75, 70.0, 1.62));
    }

    #[test]
    fn a_workspace_that_names_no_material_reads_as_an_empty_library() {
        let store = scratch("absent");
        assert!(!store.library_path().exists());
        assert_eq!(store.read(), Ok(Library::default()));
    }

    /// Writes `text` as the library file and reads it back.
    fn refusal(store: &Store, text: &str) -> LoadError {
        fs::create_dir_all(store.root()).expect("the folder");
        fs::write(store.library_path(), text).expect("the file");
        store.read().expect_err("a refusal")
    }

    #[test]
    fn a_duplicate_id_is_a_sentence_that_names_the_record() {
        let store = scratch("duplicate");
        let error = refusal(
            &store,
            r#"{ "version": 1, "materials": [
                 { "id": 4, "name": "Oak", "base": { "colour": [0.5, 0.4, 0.3] } },
                 { "id": 4, "name": "Beech", "base": { "colour": [0.5, 0.4, 0.3] } } ] }"#,
        );
        let text = error.to_string();
        assert!(text.contains("material 4") && text.contains("twice"), "{text}");
        assert!(text.contains("library.json"), "the sentence names the file a person opens: {text}");
    }

    #[test]
    fn an_empty_name_and_a_decor_that_covers_nothing_are_sentences_too() {
        let store = scratch("records");
        let nameless = refusal(
            &store,
            r#"{ "version": 1, "materials": [ { "id": 2, "name": "  ", "base": { "colour": [0.5, 0.4, 0.3] } } ] }"#,
        );
        assert!(nameless.to_string().contains("no name"), "{nameless}");
        let coverless = refusal(
            &store,
            r#"{ "version": 1, "materials": [ { "id": 3, "name": "Oak", "base": { "colour": [0.5, 0.4, 0.3],
                 "decor": { "picture": "textures/oak.png", "covers_um": 0 } } } ] }"#,
        );
        assert!(coverless.to_string().contains("more than nothing"), "{coverless}");
    }

    #[test]
    fn a_decor_whose_picture_is_missing_is_refused_with_the_record_and_the_file() {
        let store = scratch("missing-picture");
        let error = refusal(
            &store,
            r#"{ "version": 1, "materials": [ { "id": 6, "name": "Oak", "base": { "colour": [0.5, 0.4, 0.3],
                 "decor": { "picture": "textures/oak.png", "covers_um": 600000 } } } ] }"#,
        );
        let text = error.to_string();
        assert!(text.contains("material 6") && text.contains("oak.png"), "{text}");
        assert!(text.contains("No such file"), "the operating system's reason reaches the person: {text}");
    }

    #[test]
    fn a_file_of_another_schema_is_refused_with_both_numbers() {
        let store = scratch("version");
        let error = refusal(&store, r#"{ "version": 99, "materials": [] }"#);
        assert_eq!(error, LoadError::Version { path: store.library_path(), found: 99, expected: LIBRARY_VERSION });
        assert!(error.to_string().contains("99") && error.to_string().contains("1"));
    }

    #[test]
    fn a_replaced_picture_moves_the_stamp_and_answers_its_new_texels() {
        let store = scratch("replaced");
        let picture = store.textures().join("oak.png");
        write_png(&picture, &[[255, 0, 0, 255]]);
        let library = Library::new([full()]).expect("one record");
        store.write(&library).expect("the write");
        let material = library.get(id(1)).expect("the record");
        let before = store.picture(material).expect("a readable picture").expect("a decor");
        assert_eq!(before.texel(0, 0), Some([255, 0, 0, 255]));
        let stamp = store.revision().expect("a stamp");

        // A person replaces the scan; the library's own bytes are untouched.
        std::thread::sleep(std::time::Duration::from_millis(2));
        write_png(&picture, &[[0, 0, 255, 255]]);
        assert_ne!(store.revision().map(Revision::get), Ok(stamp.get()), "a replaced picture moves the stamp");
        let after = store.picture(material).expect("a readable picture").expect("a decor");
        assert_eq!(after.texel(0, 0), Some([0, 0, 255, 255]), "the frame reads the picture the shop replaced");
    }

    #[test]
    fn a_record_with_no_decor_asks_for_no_picture() {
        let store = scratch("flat");
        let board =
            Material::of_base(id(1), "White", Base::matte([0.9, 0.9, 0.9]).expect("a board")).expect("a record");
        assert_eq!(store.picture(&board), Ok(None));
    }

    #[test]
    fn a_failed_save_leaves_no_temporary_behind() {
        let store = scratch("failed");
        // A folder where the file should be makes the rename fail after the temporary is written.
        fs::create_dir_all(store.library_path()).expect("a folder in place of the file");
        assert!(store.write(&Library::default()).is_err());
        assert!(!store.root().join(format!(".{LIBRARY_FILE}.tmp")).exists());
    }

    #[test]
    fn the_store_is_a_folder_and_not_a_machine() {
        let root: PathBuf = scratch("paths").root().to_path_buf();
        let store = Store::at(&root);
        assert_eq!(store.library_path(), root.join("library.json"));
        assert_eq!(store.textures(), root.join("textures"));
    }
}
