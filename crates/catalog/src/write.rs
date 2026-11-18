//! Where a directory is written: one writer, one lock, and the revision moved last.
//!
//! The reader is [`crate::FolderSource`]; this is its only writer (`docs/domain/nomenclature.md`
//! §14). Three rules are the whole of it:
//!
//! - **one writer at a time**, guarded by the `lock` file: a second writer refuses instead of
//!   racing, and the reader never locks;
//! - **nothing is written that the reader would refuse**: the candidate directory is built and
//!   validated with the same rows the loader uses, so a save either lands whole or leaves the folder
//!   exactly as it was;
//! - **the revision moves last**: the record is written first and the package's manifest — which the
//!   reader's stamp watches — last, so a window polling the folder sees one change, after every file
//!   of the write is in place, and never reads a half-written package (`§13`).
//!
//! Every write is one file: a temporary sibling, then a rename over the target, so a failure in the
//! middle leaves the old file whole.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::directory::Directory;
use crate::error::CatalogError;
use crate::item::NomenclatureItem;
use crate::source::{DirectorySource, FolderSource, LoadError, Revision};
use crate::wire::{ItemWire, MANIFEST_VERSION, ManifestWire, RegistryWire};

/// The lock file one writer holds, in the directory's root.
const LOCK_FILE: &str = "lock";
/// The folder a package's records live in.
const PACKAGES_DIR: &str = "packages";
/// The manifest of one package.
const MANIFEST_FILE: &str = "manifest.json";
/// The registry rows of one package.
const REGISTRY_FILE: &str = "registry.json";
/// The folder one package's records live in.
const ITEMS_DIR: &str = "items";

/// Why a directory could not be written.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    /// The folder cannot be read, so a write would build on a state nobody measured.
    #[error("{0}")]
    Load(#[from] LoadError),
    /// Another writer holds the lock.
    #[error("{path} is held by another writer; a directory has one writer at a time")]
    Locked {
        /// The lock file that is already there.
        path: PathBuf,
    },
    /// A file could not be written.
    #[error("cannot write {path}: {reason}")]
    Write {
        /// The file that could not be written.
        path: PathBuf,
        /// The reason the operating system gave.
        reason: String,
    },
    /// The package the write names is not one this directory carries.
    #[error("package `{package}` is not connected to this directory")]
    UnknownPackage {
        /// The package the write named.
        package: String,
    },
    /// The record belongs to a package other than the one it would be written into.
    #[error("package `{package}`: item `{item}` belongs to package {owner}")]
    ForeignItem {
        /// The package that would carry the file.
        package: String,
        /// The id the record carries.
        item: model::ItemId,
        /// The package that id belongs to.
        owner: u32,
    },
    /// The written records would not read back, so nothing was written.
    #[error("{0}")]
    Refused(#[from] CatalogError),
}

/// The one writer of a directory folder.
///
/// Holding a `FolderWriter` means holding the lock: [`FolderWriter::open`] creates the `lock` file
/// exclusively and refuses when it is already there, and dropping the value removes it again. A
/// dropped process leaves the file behind, so the shell opens the writer when the person asks to
/// edit and closes it when they are done, rather than holding it for the life of the window.
pub struct FolderWriter {
    root: PathBuf,
    lock: PathBuf,
}

impl FolderWriter {
    /// Takes the lock on the directory at `root`.
    ///
    /// The lock is created, never waited for: one writer at a time is the rule (`§14`), and a person
    /// who is told "another application is editing this directory" can close it, while a window that
    /// blocks on a lock file tells them nothing.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, WriteError> {
        let root = root.into();
        let lock = root.join(LOCK_FILE);
        if let Some(parent) = lock.parent() {
            fs::create_dir_all(parent).map_err(|error| refused(&lock, &error))?;
        }
        let mut file = match fs::OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(WriteError::Locked { path: lock });
            }
            Err(error) => return Err(refused(&lock, &error)),
        };
        // Who holds it, for the person who finds the file after a crash: a lock nobody can attribute
        // is a folder nobody dares edit.
        let _ = writeln!(file, "pid {}", std::process::id());
        Ok(Self { root, lock })
    }

    /// The folder this writer edits.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The lock file it holds.
    pub fn lock_path(&self) -> &Path {
        &self.lock
    }

    /// The directory as it stands, read through the same port the editor uses.
    pub fn snapshot(&self) -> Result<Directory, WriteError> {
        Ok(FolderSource::at(&self.root).snapshot()?)
    }

    /// Writes one record into `package` and moves the revision.
    ///
    /// The candidate directory — every record the folder carries, with this one in place of the
    /// record of the same id — is built and validated first, so an item the reader would refuse is a
    /// sentence and an untouched folder. The record is written to `items/<kind>-<serial>.json`, any
    /// file that holds the same id under another name is removed, and the package's manifest is
    /// written last, which is what moves the stamp a window polls.
    ///
    /// The name is stable across a rename of the item's slug, because it is the kind and the id that
    /// name the file: a record a person edited twice keeps one path, and `git diff` shows one line.
    pub fn save_item(&mut self, package: &str, item: &NomenclatureItem) -> Result<PathBuf, WriteError> {
        let folder = self.root.join(PACKAGES_DIR).join(package);
        if !folder.is_dir() {
            return Err(WriteError::UnknownPackage { package: package.to_owned() });
        }
        let manifest_path = folder.join(MANIFEST_FILE);
        let mut manifest: ManifestWire = read_json(&manifest_path)?;
        if manifest.id != item.id().package() {
            return Err(WriteError::ForeignItem {
                package: package.to_owned(),
                item: item.id(),
                owner: item.id().package(),
            });
        }

        let registry_path = folder.join(REGISTRY_FILE);
        let registry: RegistryWire = read_json(&registry_path)?;
        let (kind_rows, property_rows, family_rows) = registry.rows()?;
        let kinds = crate::kind::KindRegistry::new(kind_rows)?;
        let properties = crate::property::PropertyRegistry::new(property_rows)?;
        let families = crate::family::FamilyRegistry::new(family_rows)?;
        let saved = ItemWire::from_item(item, &kinds, &properties)?;

        // The candidate: every record this folder carries, the saved one in place of its old self.
        let items_dir = folder.join(ITEMS_DIR);
        let mut records: Vec<NomenclatureItem> = Vec::new();
        let mut replaced: Vec<PathBuf> = Vec::new();
        for path in json_files(&items_dir)? {
            let wire: ItemWire = read_json(&path)?;
            if wire.id == item.id() {
                replaced.push(path.clone());
                continue;
            }
            records.push(wire.to_item(&kinds, &properties)?);
        }
        records.push(item.clone());
        // The cheapest gate there is: the directory the reader would build decides, so a save and a
        // reload cannot disagree about what a valid record is.
        Directory::new(records, kinds, properties, families)?;

        let path = items_dir.join(item_name(item, &saved.kind));
        write_json(&path, &saved)?;
        // A record whose file name carried another kind is the one case a write leaves two files: the
        // new one is in place, so the old one goes.
        for stale in replaced {
            if stale != path {
                fs::remove_file(&stale).map_err(|error| refused(&stale, &error))?;
            }
        }

        // The revision moves last: the manifest is one of the files the reader's stamp watches.
        manifest.version = MANIFEST_VERSION;
        manifest.package_version = manifest.package_version.saturating_add(1);
        write_json(&manifest_path, &manifest)?;
        Ok(path)
    }

    /// The stamp of the folder after a write, read through the reader's own port.
    pub fn revision(&self) -> Result<Revision, WriteError> {
        Ok(FolderSource::at(&self.root).revision()?)
    }

    /// Gives the lock up without waiting for the value to be dropped.
    pub fn close(self) {}
}

impl Drop for FolderWriter {
    fn drop(&mut self) {
        // A lock that cannot be removed is left for the person to see; the sentence they already got
        // from the failed write says which file it is.
        let _ = fs::remove_file(&self.lock);
    }
}

/// The file name of one record: its kind and its serial, both of which outlive a rename.
fn item_name(item: &NomenclatureItem, kind: &str) -> String {
    format!("{}-{}.json", kind.trim(), item.id().serial())
}

/// The `*.json` files of one folder, in name order, so two writes of one folder agree.
fn json_files(folder: &Path) -> Result<Vec<PathBuf>, WriteError> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(refused(folder, &error)),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| refused(folder, &error))?;
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "json") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Reads one JSON file of a package.
fn read_json<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<T, WriteError> {
    let text = fs::read_to_string(path).map_err(|error| refused(path, &error))?;
    serde_json::from_str(&text)
        .map_err(|error| WriteError::Write { path: path.to_path_buf(), reason: error.to_string() })
}

/// Writes one JSON file: a temporary sibling, then a rename over the target.
///
/// The scene file's writer has the same shape in `app`; this crate cannot reach it, and one folder's
/// rules are not worth a crate of their own yet (`plan/debt.md`).
fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), WriteError> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| WriteError::Write { path: path.to_path_buf(), reason: error.to_string() })?;
    write_atomically(path, text.as_bytes())
}

/// Writes `bytes` to a temporary sibling, flushes it and renames it over `target`.
fn write_atomically(target: &Path, bytes: &[u8]) -> Result<(), WriteError> {
    let parent = target
        .parent()
        .ok_or_else(|| WriteError::Write { path: target.to_path_buf(), reason: "the file has no folder".to_owned() })?;
    fs::create_dir_all(parent).map_err(|error| refused(parent, &error))?;
    let name = target.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let temporary = parent.join(format!(".{name}.tmp"));
    {
        let mut file = fs::File::create(&temporary).map_err(|error| refused(&temporary, &error))?;
        file.write_all(bytes).map_err(|error| refused(&temporary, &error))?;
        file.sync_all().map_err(|error| refused(&temporary, &error))?;
    }
    match fs::rename(&temporary, target) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(refused(target, &error))
        }
    }
}

/// One refusal the operating system gave, naming the file it was about.
fn refused(path: &Path, error: &io::Error) -> WriteError {
    WriteError::Write { path: path.to_path_buf(), reason: error.to_string() }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    use model::ItemId;

    use super::*;
    use crate::kind::KindCode;
    use crate::property::{PropertyCode, PropertyValue};
    use crate::slug::Slug;
    use crate::wire::{KindWire, PropertyTypeWire, PropertyWire, REGISTRY_VERSION};

    /// A directory under the system temporary folder, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_nanos()).unwrap_or(0);
            let path = std::env::temp_dir().join(format!("catalog-write-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&path).expect("create the temporary directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A folder holding one package with one kind and two properties, listed as connected.
    fn package_folder(root: &Path) -> PathBuf {
        let package = root.join(PACKAGES_DIR).join("starter");
        fs::create_dir_all(package.join(ITEMS_DIR)).expect("create the package");
        fs::write(
            root.join("directory.json"),
            serde_json::json!({ "version": 1, "packages": ["starter"] }).to_string(),
        )
        .expect("write the connected list");
        let manifest = ManifestWire::new(1, "Starter");
        write_json(&package.join(MANIFEST_FILE), &manifest).expect("write the manifest");
        let registry = RegistryWire {
            version: REGISTRY_VERSION,
            kinds: vec![KindWire {
                code: 1,
                slug: "sheet-material".to_owned(),
                label: "Sheet material".to_owned(),
                accepts: vec![1, 2],
                required: vec![1],
                traits: Vec::new(),
            }],
            properties: vec![
                PropertyWire {
                    code: 1,
                    slug: "thickness".to_owned(),
                    label: "Thickness".to_owned(),
                    ty: PropertyTypeWire::Length { min_um: 1_000, max_um: 100_000 },
                },
                PropertyWire {
                    code: 2,
                    slug: "decor".to_owned(),
                    label: "Decor".to_owned(),
                    ty: PropertyTypeWire::Text { max_chars: 48 },
                },
            ],
            families: Vec::new(),
        };
        write_json(&package.join(REGISTRY_FILE), &registry).expect("write the registry");
        package
    }

    fn an_item(serial: u32, name: &str, thickness: i64) -> NomenclatureItem {
        let id = ItemId::new(1, serial).expect("an id");
        let mut props = BTreeMap::new();
        props.insert(
            PropertyCode::new(1).expect("a code"),
            PropertyValue::Length(units::Length::from_micrometres(thickness)),
        );
        NomenclatureItem::new(
            id,
            KindCode::new(1).expect("a code"),
            name,
            Slug::new(
                name.to_lowercase()
                    .chars()
                    .map(|letter| if letter.is_ascii_alphanumeric() { letter } else { '-' })
                    .collect::<String>(),
            )
            .expect("a slug"),
            props,
        )
        .expect("an item")
    }

    #[test]
    fn a_saved_record_reads_back_under_its_own_name() {
        let temp = TempDir::new("save");
        package_folder(temp.path());
        let mut writer = FolderWriter::open(temp.path()).expect("the lock");
        let item = an_item(7, "Oak shelf", 18_000);
        let path = writer.save_item("starter", &item).expect("the write");
        assert_eq!(path.file_name().and_then(|name| name.to_str()), Some("sheet-material-7.json"));
        drop(writer);

        let directory = FolderSource::at(temp.path()).snapshot().expect("the folder reads back");
        let saved = directory.item(item.id()).expect("the record is there");
        assert_eq!(saved.name(), "Oak shelf");
        assert_eq!(
            saved.prop(PropertyCode::new(1).expect("a code")),
            Some(&PropertyValue::Length(units::Length::from_micrometres(18_000)))
        );
    }

    #[test]
    fn a_save_moves_the_revision_and_writes_the_record_before_the_manifest() {
        let temp = TempDir::new("revision");
        let package = package_folder(temp.path());
        let before = FolderSource::at(temp.path()).revision().expect("a stamp");
        let mut writer = FolderWriter::open(temp.path()).expect("the lock");
        writer.save_item("starter", &an_item(7, "Oak shelf", 18_000)).expect("the write");
        let after = writer.revision().expect("a stamp");
        assert_ne!(before, after, "a window polling the folder sees the change");
        drop(writer);

        let manifest: ManifestWire = read_json(&package.join(MANIFEST_FILE)).expect("the manifest");
        assert_eq!(manifest.package_version, 2, "the revision of the package moved with the record");
    }

    #[test]
    fn a_second_writer_is_refused_and_the_first_keeps_the_lock() {
        let temp = TempDir::new("lock");
        package_folder(temp.path());
        let first = FolderWriter::open(temp.path()).expect("the lock");
        let second = FolderWriter::open(temp.path());
        assert!(matches!(second, Err(WriteError::Locked { .. })), "one writer at a time");
        drop(first);
        let third = FolderWriter::open(temp.path()).expect("the lock is free again");
        assert!(third.lock_path().is_file());
    }

    #[test]
    fn a_record_the_reader_would_refuse_writes_nothing() {
        let temp = TempDir::new("refused");
        let package = package_folder(temp.path());
        let mut writer = FolderWriter::open(temp.path()).expect("the lock");
        // The kind requires `thickness`, so a record without it is the loader's refusal, not the
        // writer's own rule.
        let id = ItemId::new(1, 9).expect("an id");
        let bare = NomenclatureItem::new(
            id,
            KindCode::new(1).expect("a code"),
            "Bare",
            Slug::new("bare").expect("a slug"),
            BTreeMap::new(),
        )
        .expect("an item");
        let refusal = writer.save_item("starter", &bare).expect_err("a required property is missing");
        assert!(refusal.to_string().contains("9"), "{refusal}");
        assert_eq!(json_files(&package.join(ITEMS_DIR)).expect("a listing").len(), 0, "no file was written");
        let manifest: ManifestWire = read_json(&package.join(MANIFEST_FILE)).expect("the manifest");
        assert_eq!(manifest.package_version, 1, "and the revision did not move");
    }

    #[test]
    fn a_record_of_another_package_is_refused() {
        let temp = TempDir::new("foreign");
        package_folder(temp.path());
        let mut writer = FolderWriter::open(temp.path()).expect("the lock");
        let mut item = an_item(7, "Oak shelf", 18_000);
        let foreign = ItemId::new(2, 7).expect("an id");
        item = NomenclatureItem::new(foreign, item.kind(), item.name(), item.slug().clone(), item.props().clone())
            .expect("an item");
        let refusal = writer.save_item("starter", &item).expect_err("another package's record");
        assert!(matches!(refusal, WriteError::ForeignItem { .. }), "{refusal}");
    }

    #[test]
    fn a_saved_record_replaces_its_old_file_instead_of_adding_one() {
        let temp = TempDir::new("replace");
        let package = package_folder(temp.path());
        let mut writer = FolderWriter::open(temp.path()).expect("the lock");
        writer.save_item("starter", &an_item(7, "Oak shelf", 18_000)).expect("the first write");
        writer.save_item("starter", &an_item(7, "Oak shelf, 25", 25_000)).expect("the second write");
        drop(writer);
        let files = json_files(&package.join(ITEMS_DIR)).expect("a listing");
        assert_eq!(files.len(), 1, "one record keeps one file: {files:?}");
        let directory = FolderSource::at(temp.path()).snapshot().expect("the folder reads back");
        let saved = directory.item(ItemId::new(1, 7).expect("an id")).expect("the record");
        assert_eq!(
            saved.prop(PropertyCode::new(1).expect("a code")),
            Some(&PropertyValue::Length(units::Length::from_micrometres(25_000))),
            "the second save is what the reader sees"
        );
    }
}
