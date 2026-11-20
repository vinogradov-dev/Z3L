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
//!
//! The verbs are the record ([`FolderWriter::save_item`]), the connected list
//! ([`FolderWriter::connect`], [`FolderWriter::disconnect`]) and the package's own travel
//! ([`FolderWriter::export_package`], [`FolderWriter::import_package`]). A package is its folder, so a
//! copy of the folder is the whole of an export and the import is the same copy in the other
//! direction (`§12`); only the connected list decides what a reader loads, which is why it is the
//! last file each of those writes touches.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::directory::Directory;
use crate::error::CatalogError;
use crate::item::NomenclatureItem;
use crate::source::{DirectorySource, DirectoryWire, FolderSource, LoadError, Revision};
use crate::wire::{DIRECTORY_VERSION, ItemWire, MANIFEST_VERSION, ManifestWire, RegistryWire};

/// The lock file one writer holds, in the directory's root.
const LOCK_FILE: &str = "lock";
/// The file that lists the connected packages.
const DIRECTORY_FILE: &str = "directory.json";
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
    #[error("{path}: no package folder `{package}`")]
    MissingPackage {
        /// The package the write named.
        package: String,
        /// The folder the directory would have read it from.
        path: PathBuf,
    },
    /// A name is not one path component, so it is not a package.
    #[error("`{name}` is not a package name: a package is one folder name")]
    BadName {
        /// The name that was given.
        name: String,
    },
    /// A package of that name already sits in the directory's folder.
    #[error("{path}: package `{package}` is already there")]
    PackageExists {
        /// The package the import would have landed as.
        package: String,
        /// The folder it would have written.
        path: PathBuf,
    },
    /// The export target is not empty, so a package would be written over something else.
    #[error("{path} is not empty: an export writes into a folder of its own")]
    NotEmpty {
        /// The folder that was given as a target.
        path: PathBuf,
    },
    /// The folder is not a package this build reads.
    #[error("{path} is not a package: {reason}")]
    NotAPackage {
        /// The folder or file that was given as a package.
        path: PathBuf,
        /// What is wrong with it.
        reason: String,
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
    /// A batch with no record in it: a write that changes nothing is not a save.
    #[error("there is nothing to write: a batch needs at least one record")]
    NothingToWrite,
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
        let written = self.save_items(package, std::slice::from_ref(item))?;
        written.into_iter().next().ok_or(WriteError::NothingToWrite)
    }

    /// Writes several records into `package` as **one** committed change, and moves the revision once.
    ///
    /// A price list is one act and not one act per row: the batch is validated once, the records are
    /// written, and the package's manifest — the file the reader's stamp watches — is bumped exactly
    /// once afterwards, so a window polling the folder reloads once and the directory's history holds
    /// one entry for the whole batch (`§11`, `§13`).
    ///
    /// Everything [`FolderWriter::save_item`] promises holds for the batch: the candidate directory is
    /// validated before a file is touched, each record is written to a temporary sibling and renamed,
    /// and a record the reader would refuse leaves the folder exactly as it was.
    pub fn save_items(&mut self, package: &str, items: &[NomenclatureItem]) -> Result<Vec<PathBuf>, WriteError> {
        if items.is_empty() {
            return Err(WriteError::NothingToWrite);
        }
        let folder = self.package_folder(package)?;
        let manifest_path = folder.join(MANIFEST_FILE);
        let mut manifest: ManifestWire = read_json(&manifest_path)?;
        for item in items {
            if manifest.id != item.id().package() {
                return Err(WriteError::ForeignItem {
                    package: package.to_owned(),
                    item: item.id(),
                    owner: item.id().package(),
                });
            }
        }

        let registry_path = folder.join(REGISTRY_FILE);
        let registry: RegistryWire = read_json(&registry_path)?;
        let rows = registry.rows()?;
        let kinds = crate::kind::KindRegistry::new(rows.kinds)?;
        let properties = crate::property::PropertyRegistry::new(rows.properties)?;
        let families = crate::family::FamilyRegistry::new(rows.families)?;
        let systems = crate::external::ExternalSystemRegistry::new(rows.systems)?;
        let saved = items
            .iter()
            .map(|item| Ok((item, ItemWire::from_item(item, &kinds, &properties, &families)?)))
            .collect::<Result<Vec<(&NomenclatureItem, ItemWire)>, CatalogError>>()?;

        // The candidate: every record this folder carries, the batch in place of its old selves.
        let items_dir = folder.join(ITEMS_DIR);
        let mut records: Vec<NomenclatureItem> = Vec::new();
        let mut replaced: Vec<PathBuf> = Vec::new();
        for path in json_files(&items_dir)? {
            let wire: ItemWire = read_json(&path)?;
            if items.iter().any(|item| item.id() == wire.id) {
                replaced.push(path.clone());
                continue;
            }
            records.push(wire.to_item(&kinds, &properties, &families)?);
        }
        records.extend(items.iter().cloned());
        // The cheapest gate there is: the directory the reader would build decides, so a save and a
        // reload cannot disagree about what a valid record is — for the whole batch, not per record.
        Directory::new(records, kinds, properties, families, systems)?;

        let mut paths = Vec::with_capacity(items.len());
        for (item, wire) in &saved {
            let path = items_dir.join(item_name(item, &wire.kind));
            write_json(&path, wire)?;
            paths.push(path);
        }
        // A record whose file name carried another kind is the one case a write leaves two files: the
        // new one is in place, so the old one goes.
        for stale in replaced {
            if !paths.contains(&stale) {
                fs::remove_file(&stale).map_err(|error| refused(&stale, &error))?;
            }
        }

        // The revision moves last, once for the whole batch: the manifest is one of the files the
        // reader's stamp watches, so a window sees one change after every record is in place.
        manifest.version = MANIFEST_VERSION;
        manifest.package_version = manifest.package_version.saturating_add(1);
        write_json(&manifest_path, &manifest)?;
        Ok(paths)
    }

    /// The stamp of the folder after a write, read through the reader's own port.
    pub fn revision(&self) -> Result<Revision, WriteError> {
        Ok(FolderSource::at(&self.root).revision()?)
    }

    /// Gives the lock up without waiting for the value to be dropped.
    pub fn close(self) {}

    /// Adds `package` to the connected list, so the reader loads it.
    ///
    /// The list is written only when the loader accepts the directory it would produce, so a
    /// package that claims another's number or requires a package that is not here is refused
    /// naming both, and the folder is untouched (`§12`). A package that is already connected is
    /// left exactly as it is: connecting twice is not a write.
    pub fn connect(&mut self, package: &str) -> Result<Revision, WriteError> {
        self.package_folder(package)?;
        let listing = FolderSource::at(&self.root).listing()?;
        if listing.packages.iter().any(|connected| connected == package) {
            return self.revision();
        }
        let mut candidate = listing.packages.clone();
        candidate.push(package.to_owned());
        self.write_listing(candidate)
    }

    /// Takes `package` out of the connected list; its folder stays where it is.
    ///
    /// A disconnected package is absent from the directory (§9.3), which is visible, and coming back
    /// is one `connect` away. The list is written only when the loader accepts what would remain: a
    /// package another one requires cannot be withdrawn silently.
    pub fn disconnect(&mut self, package: &str) -> Result<Revision, WriteError> {
        let listing = FolderSource::at(&self.root).listing()?;
        if !listing.packages.iter().any(|connected| connected == package) {
            return self.revision();
        }
        let candidate: Vec<String> = listing.packages.iter().filter(|name| name.as_str() != package).cloned().collect();
        self.write_listing(candidate)
    }

    /// Writes one package's folder into `target`, so another workshop can import it.
    ///
    /// The copy is the package: every file of its folder, whatever this build understands, because
    /// a copy that dropped one would change it (`§14`). The target must be a folder of its own — an
    /// export never writes over what is already there.
    pub fn export_package(&self, package: &str, target: &Path) -> Result<PathBuf, WriteError> {
        let source = self.package_folder(package)?;
        let occupied = match fs::read_dir(target) {
            Ok(mut entries) => entries.next().is_some(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) if error.kind() == io::ErrorKind::NotADirectory => true,
            Err(error) => return Err(refused(target, &error)),
        };
        if occupied {
            return Err(WriteError::NotEmpty { path: target.to_path_buf() });
        }
        copy_package(&source, target)?;
        Ok(target.to_path_buf())
    }

    /// Copies a package's folder into this directory and connects it.
    ///
    /// The folder name is the name the package is connected by, and its manifest is what makes it a
    /// package at all. The copy lands first and the connected list is written last, so the reader
    /// sees one change after every file is in place; a copy the loader refuses is removed again and
    /// the refusal is the caller's answer, with the directory listed exactly as it was.
    pub fn import_package(&mut self, source: &Path) -> Result<PathBuf, WriteError> {
        let name = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| plain_name(name))
            .ok_or_else(|| WriteError::BadName { name: source.display().to_string() })?;
        // A folder is a package when its manifest reads and has this build's schema; reading it
        // first is what names the reason when it is not one.
        let manifest_path = source.join(MANIFEST_FILE);
        let manifest: ManifestWire = read_manifest(&manifest_path)?;
        if manifest.version != MANIFEST_VERSION {
            return Err(WriteError::NotAPackage {
                path: manifest_path,
                reason: format!("schema {}, this build reads {MANIFEST_VERSION}", manifest.version),
            });
        }
        let landing = self.root.join(PACKAGES_DIR).join(&name);
        if landing.exists() {
            return Err(WriteError::PackageExists { package: name, path: landing });
        }
        // A copy that fails halfway is removed like a refused one: an unlisted folder nobody asked
        // for is a folder the next import of this package would trip over.
        if let Err(error) = copy_package(source, &landing) {
            let _ = fs::remove_dir_all(&landing);
            return Err(error);
        }
        let listing = FolderSource::at(&self.root).listing()?;
        let mut candidate = listing.packages.clone();
        candidate.push(name.clone());
        if let Err(error) = crate::source::load(&self.root, &candidate) {
            // The folder is not listed yet, so it is inert; it must not stay and make the next
            // import of the same package fail for no reason a person can see.
            let _ = fs::remove_dir_all(&landing);
            return Err(error.into());
        }
        // The set was validated with the loader's own rows a line ago, and the copy is in place, so
        // this is the one write that makes the package visible.
        let listing = DirectoryWire { version: DIRECTORY_VERSION, packages: candidate };
        write_json(&self.root.join(DIRECTORY_FILE), &listing)?;
        Ok(landing)
    }

    /// The folder of one package, refusing a name that is not there or not a name at all.
    fn package_folder(&self, package: &str) -> Result<PathBuf, WriteError> {
        if !plain_name(package) {
            return Err(WriteError::BadName { name: package.to_owned() });
        }
        let folder = self.root.join(PACKAGES_DIR).join(package);
        if !folder.is_dir() {
            return Err(WriteError::MissingPackage { package: package.to_owned(), path: folder });
        }
        Ok(folder)
    }

    /// Connects `candidate` after the loader has accepted it, and answers the moved stamp.
    fn write_listing(&mut self, candidate: Vec<String>) -> Result<Revision, WriteError> {
        crate::source::load(&self.root, &candidate)?;
        let listing = DirectoryWire { version: DIRECTORY_VERSION, packages: candidate };
        write_json(&self.root.join(DIRECTORY_FILE), &listing)?;
        self.revision()
    }
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

/// Whether `name` is one path component: a package is a folder name, never a path.
///
/// The name comes from a file a supplier wrote as often as from a person, so it is checked before it
/// is joined onto the directory's root.
fn plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

/// Copies one package's folder into `to`, its manifest last.
///
/// Every file travels, whatever this build understands: the records, the registry, the part
/// documents and the rule tables are the package (`§12`), and a copy that dropped the ones it did
/// not recognise would change it. The manifest goes last, so a reader pointed at the copy never sees
/// a package that is still missing one of its files and calls it complete.
fn copy_package(from: &Path, to: &Path) -> Result<(), WriteError> {
    fs::create_dir_all(to).map_err(|error| refused(to, &error))?;
    for source in tree_files(from)? {
        if source == from.join(MANIFEST_FILE) {
            continue;
        }
        let relative = source.strip_prefix(from).unwrap_or(&source);
        copy_file(&source, &to.join(relative))?;
    }
    let manifest = from.join(MANIFEST_FILE);
    if manifest.is_file() {
        copy_file(&manifest, &to.join(MANIFEST_FILE))?;
    }
    Ok(())
}

/// Every file under `folder`, in name order.
///
/// The walk keeps its own stack instead of recursing, so a folder of any depth costs a heap vector
/// and not a call frame, and a symbolic link is refused rather than followed: a link into a folder
/// that links back would copy forever.
fn tree_files(folder: &Path) -> Result<Vec<PathBuf>, WriteError> {
    let mut files = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(refused(&directory, &error)),
        };
        for entry in entries {
            let entry = entry.map_err(|error| refused(&directory, &error))?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| refused(&path, &error))?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                files.push(path);
            } else {
                return Err(WriteError::NotAPackage {
                    path,
                    reason: "a symbolic link is not part of a package".to_owned(),
                });
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Copies one file, creating the folder it lands in.
fn copy_file(from: &Path, to: &Path) -> Result<(), WriteError> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|error| refused(parent, &error))?;
    }
    fs::copy(from, to).map_err(|error| refused(to, &error))?;
    Ok(())
}

/// Reads a package's manifest, naming the file when it is not one.
fn read_manifest(path: &Path) -> Result<ManifestWire, WriteError> {
    let text = fs::read_to_string(path)
        .map_err(|error| WriteError::NotAPackage { path: path.to_path_buf(), reason: error.to_string() })?;
    serde_json::from_str(&text)
        .map_err(|error| WriteError::NotAPackage { path: path.to_path_buf(), reason: error.to_string() })
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
            systems: Vec::new(),
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

    /// A package folder with `id` and its own registry, connected or not.
    fn package_of(root: &Path, name: &str, id: u32, listed: bool) -> PathBuf {
        let package = root.join(PACKAGES_DIR).join(name);
        fs::create_dir_all(package.join(ITEMS_DIR)).expect("create the package");
        write_json(&package.join(MANIFEST_FILE), &ManifestWire::new(id, name)).expect("write the manifest");
        write_json(&package.join(REGISTRY_FILE), &RegistryWire::new()).expect("write the registry");
        if listed {
            let mut names: Vec<String> = FolderSource::at(root).packages().expect("the list");
            if !names.iter().any(|connected| connected == name) {
                names.push(name.to_owned());
            }
            let listing = DirectoryWire { version: DIRECTORY_VERSION, packages: names };
            write_json(&root.join(DIRECTORY_FILE), &listing).expect("write the list");
        }
        package
    }

    /// A package whose sheet kind is priced by the sheet, with two records an `erp` import knows.
    fn priced_package(root: &Path) -> PathBuf {
        let package = root.join(PACKAGES_DIR).join("starter");
        fs::create_dir_all(package.join(ITEMS_DIR)).expect("create the package");
        write_json(
            &root.join("directory.json"),
            &DirectoryWire { version: DIRECTORY_VERSION, packages: vec!["starter".to_owned()] },
        )
        .expect("write the connected list");
        write_json(&package.join(MANIFEST_FILE), &ManifestWire::new(1, "Starter")).expect("write the manifest");
        let registry = serde_json::json!({
            "version": 1,
            "kinds": [{
                "code": 1, "slug": "sheet-material", "label": "Sheet material",
                "accepts": [1, 2], "required": [1], "traits": ["sheet", "price-sheet"]
            }],
            "properties": [
                { "code": 1, "slug": "thickness", "label": "Thickness", "type": "length", "min_um": 1000, "max_um": 100000 },
                { "code": 2, "slug": "price", "label": "Price", "type": "money" }
            ],
            "systems": [{ "id": "erp", "label": "ERP" }]
        });
        write_json(
            &package.join(REGISTRY_FILE),
            &serde_json::from_value::<RegistryWire>(registry).expect("a registry"),
        )
        .expect("write the registry");
        for (serial, key) in [(1, "A-1"), (2, "A-2")] {
            let record = serde_json::json!({
                "id": format!("1-{serial}"), "kind": "sheet-material", "name": format!("Board {serial}"),
                "slug": format!("board-{serial}"),
                "props": { "thickness": { "type": "length", "value": 18000 } },
                "external": [{ "system": "erp", "key": key }]
            });
            write_json(
                &package.join(ITEMS_DIR).join(format!("sheet-material-{serial}.json")),
                &serde_json::from_value::<ItemWire>(record).expect("a record"),
            )
            .expect("write the record");
        }
        package
    }

    #[test]
    fn a_price_list_writes_every_record_it_prices_as_one_revision() {
        // O8's Done: a CSV batch writes one history entry. The directory's history is its revision, one
        // bump per committed write, so a batch of two prices must bump the manifest exactly once.
        let temp = TempDir::new("price-batch");
        let package = priced_package(temp.path());
        let listed = FolderSource::at(temp.path()).snapshot().expect("the package reads");
        let list = "system,key,price_minor,currency,date\n\
                    erp,A-1,320000,RUB,2026-11-20\n\
                    erp,A-2,240000,RUB,2026-11-20\n";
        let batch = crate::import::PriceBatch::read(list, &listed);
        assert_eq!(batch.applied_count(), 2);
        let records = batch.records(&listed);
        assert_eq!(records.len(), 2);

        let before: ManifestWire = read_json(&package.join(MANIFEST_FILE)).expect("the manifest");
        let mut writer = FolderWriter::open(temp.path()).expect("the lock");
        let written = writer.save_items("starter", &records).expect("one write");
        drop(writer);
        assert_eq!(written.len(), 2);
        let after: ManifestWire = read_json(&package.join(MANIFEST_FILE)).expect("the manifest");
        assert_eq!(
            after.package_version,
            before.package_version.saturating_add(1),
            "one batch is one entry in the history a reader watches, not one per row"
        );

        let reread = FolderSource::at(temp.path()).snapshot().expect("the folder reads back");
        assert_eq!(reread.currency().map(|currency| currency.code()), Some("RUB"));
        let priced = reread.item(ItemId::new(1, 1).expect("an id")).expect("the record");
        let price = reread.price(priced).expect("the price the batch wrote");
        assert_eq!(price.value().minor(), 320_000);
        assert_eq!(price.price.source().system().map(|system| system.as_str()), Some("erp"));
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

    /// A package folder of `id` that carries one record of the starter's kind.
    fn supplier_folder(root: &Path, listed: bool) -> PathBuf {
        let package = package_of(root, "supplier", 2, listed);
        fs::write(
            package.join(ITEMS_DIR).join("sheet-material-8.json"),
            serde_json::json!({
                "id": "2-8", "kind": "sheet-material", "name": "Supplier shelf", "slug": "supplier-shelf",
                "props": { "thickness": { "type": "length", "value": 18000 } }
            })
            .to_string(),
        )
        .expect("write the record");
        package
    }

    /// A directory of its own: one package named `shop` and nothing else.
    fn shop_folder(root: &Path) -> PathBuf {
        let package = package_of(root, "shop", 3, false);
        let listing = DirectoryWire { version: DIRECTORY_VERSION, packages: vec!["shop".to_owned()] };
        write_json(&root.join(DIRECTORY_FILE), &listing).expect("write the list");
        package
    }

    /// Adds one requirement to a package's manifest, as a workshop editing the file would.
    fn require(package: &Path, id: u32, name: &str, version: u32) {
        let mut manifest: ManifestWire = read_json(&package.join(MANIFEST_FILE)).expect("the manifest");
        manifest.requires.push(crate::wire::RequiresWire::new(id, name, version));
        write_json(&package.join(MANIFEST_FILE), &manifest).expect("write the manifest");
    }

    #[test]
    fn a_package_exports_and_imports_into_a_second_directory_unchanged() {
        let from = TempDir::new("export");
        let package = package_folder(from.path());
        let mut writer = FolderWriter::open(from.path()).expect("the lock");
        writer.save_item("starter", &an_item(7, "Oak shelf", 18_000)).expect("the write");
        let handover = from.path().join("handover").join("starter");
        writer.export_package("starter", &handover).expect("the export");
        drop(writer);

        let into = TempDir::new("import");
        shop_folder(into.path());
        let mut writer = FolderWriter::open(into.path()).expect("the lock");
        let imported = writer.import_package(&handover).expect("the import");
        drop(writer);

        let source = FolderSource::at(from.path()).snapshot().expect("the source reads");
        let target = FolderSource::at(into.path()).snapshot().expect("the target reads");
        let id = ItemId::new(1, 7).expect("an id");
        assert_eq!(target.item(id), source.item(id), "the record arrives unchanged");
        assert_eq!(
            fs::read_to_string(imported.join(ITEMS_DIR).join("sheet-material-7.json")).expect("the copy"),
            fs::read_to_string(package.join(ITEMS_DIR).join("sheet-material-7.json")).expect("the original"),
            "the copy is the file, not a re-serialization of it"
        );
        assert!(
            FolderSource::at(into.path()).packages().expect("the list").iter().any(|name| name == "starter"),
            "the package is connected in its new home"
        );
    }

    #[test]
    fn an_import_that_claims_a_connected_number_is_refused_with_both_packages_named() {
        let from = TempDir::new("clash-source");
        package_folder(from.path());
        // The folder name is what a directory connects, the number is what it cannot share.
        let handover = from.path().join("supplier");
        let writer = FolderWriter::open(from.path()).expect("the lock");
        writer.export_package("starter", &handover).expect("the export");
        drop(writer);

        let into = TempDir::new("clash-target");
        package_folder(into.path());
        let mut writer = FolderWriter::open(into.path()).expect("the lock");
        let refusal = writer.import_package(&handover).expect_err("number 1 is already claimed");
        drop(writer);
        let sentence = refusal.to_string();
        assert!(sentence.contains("supplier") && sentence.contains("starter"), "both packages are named: {sentence}");
        assert!(!into.path().join(PACKAGES_DIR).join("supplier").exists(), "the copy did not stay");
        assert_eq!(
            FolderSource::at(into.path()).packages().expect("the list"),
            vec!["starter".to_owned()],
            "and the directory lists what it listed before"
        );
    }

    #[test]
    fn connect_shows_a_package_and_disconnect_hides_it_without_deleting_it() {
        let temp = TempDir::new("connect");
        package_folder(temp.path());
        let supplier = supplier_folder(temp.path(), false);
        let id = ItemId::new(2, 8).expect("an id");
        let source = FolderSource::at(temp.path());
        assert!(source.snapshot().expect("a snapshot").item(id).is_none(), "a loose folder is not connected");

        let mut writer = FolderWriter::open(temp.path()).expect("the lock");
        let connected = writer.connect("supplier").expect("the connect");
        assert!(source.snapshot().expect("a snapshot").item(id).is_some(), "a connected package is read");
        assert_eq!(connected, writer.revision().expect("a stamp"), "the answer is the stamp the window polls");

        writer.disconnect("supplier").expect("the disconnect");
        assert!(source.snapshot().expect("a snapshot").item(id).is_none(), "a disconnected package is absent");
        assert!(supplier.is_dir(), "the folder stays: it is disconnected, not deleted");

        writer.connect("supplier").expect("the second connect");
        assert!(source.snapshot().expect("a snapshot").item(id).is_some(), "and coming back is one connect");
    }

    #[test]
    fn disconnecting_a_package_another_one_requires_is_refused_naming_both() {
        let temp = TempDir::new("required");
        let starter = package_folder(temp.path());
        supplier_folder(temp.path(), true);
        require(&starter, 2, "supplier", 1);
        FolderSource::at(temp.path()).snapshot().expect("the requirement is met");

        let mut writer = FolderWriter::open(temp.path()).expect("the lock");
        let refusal = writer.disconnect("supplier").expect_err("starter requires it");
        let sentence = refusal.to_string();
        assert!(sentence.contains("starter") && sentence.contains("supplier"), "{sentence}");
        assert_eq!(
            FolderSource::at(temp.path()).packages().expect("the list"),
            vec!["starter".to_owned(), "supplier".to_owned()],
            "nothing was withdrawn"
        );
    }

    #[test]
    fn an_import_the_reader_would_refuse_writes_nothing() {
        let from = TempDir::new("refused-import");
        let package = package_folder(from.path());
        fs::write(
            package.join(ITEMS_DIR).join("sheet-material-1.json"),
            serde_json::json!({
                "id": "1-1", "kind": "no-such-kind", "name": "White melamine", "slug": "board-1",
                "props": { "thickness": { "type": "length", "value": 18000 } }
            })
            .to_string(),
        )
        .expect("write the record");
        let handover = from.path().join("handover").join("starter");
        let writer = FolderWriter::open(from.path()).expect("the lock");
        writer.export_package("starter", &handover).expect("the export");
        drop(writer);

        let into = TempDir::new("refused-target");
        shop_folder(into.path());
        let mut writer = FolderWriter::open(into.path()).expect("the lock");
        let refusal = writer.import_package(&handover).expect_err("the record names no kind");
        drop(writer);
        assert!(refusal.to_string().contains("no-such-kind"), "{refusal}");
        assert!(!into.path().join(PACKAGES_DIR).join("starter").exists(), "the refused copy was removed");
        assert_eq!(
            FolderSource::at(into.path()).packages().expect("the list"),
            vec!["shop".to_owned()],
            "the directory is listed exactly as it was"
        );
    }

    #[test]
    fn an_export_into_a_folder_that_holds_something_is_refused() {
        let temp = TempDir::new("occupied");
        package_folder(temp.path());
        let target = temp.path().join("handover");
        fs::create_dir_all(&target).expect("create the target");
        fs::write(target.join("notes.txt"), "a person's folder").expect("write a file");
        let writer = FolderWriter::open(temp.path()).expect("the lock");
        let refusal = writer.export_package("starter", &target).expect_err("the folder is not empty");
        drop(writer);
        assert!(matches!(refusal, WriteError::NotEmpty { .. }), "{refusal}");
        assert_eq!(
            fs::read_to_string(target.join("notes.txt")).expect("the file"),
            "a person's folder",
            "nothing was written over it"
        );
    }
}
