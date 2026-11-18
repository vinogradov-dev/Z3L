//! Where a directory comes from: one port, and the folder that implements it.
//!
//! The port is what the shell and the tests hold; the folder is the first implementation
//! (`docs/domain/nomenclature.md` §14). A reader takes one snapshot and never writes, and a snapshot
//! is a whole `Directory` or an error — there is no half-loaded directory, because the editor runs
//! on the previous one until a new one validates (§13).
//!
//! ```text
//! nomenclature/
//!   directory.json                 the connected packages, in the order they load
//!   packages/<name>/manifest.json   the package's number, name and version
//!   packages/<name>/registry.json   the kinds, properties and families it declares
//!   packages/<name>/items/*.json    one record per file, stable order, diffable
//! ```
//!
//! Every refusal names the package and the file: a package arrives from a supplier, and a person has
//! to open the line the reader complains about.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Deserialize;

use crate::directory::Directory;
use crate::family::FamilyRegistry;
use crate::kind::KindRegistry;
use crate::property::PropertyRegistry;
use crate::wire::{DIRECTORY_VERSION, MANIFEST_VERSION, ManifestWire, REGISTRY_VERSION, RegistryWire};

/// The name of the file that lists the connected packages.
const DIRECTORY_FILE: &str = "directory.json";
/// The folder the packages live in.
const PACKAGES_DIR: &str = "packages";
/// The manifest of one package.
const MANIFEST_FILE: &str = "manifest.json";
/// The registry rows of one package.
const REGISTRY_FILE: &str = "registry.json";
/// The folder one package's records live in.
const ITEMS_DIR: &str = "items";

/// Why a directory could not be read.
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
    /// A file does not match the schema of the package it belongs to.
    #[error("package `{package}`, {path}: {reason}")]
    File {
        /// The package the file belongs to.
        package: String,
        /// The file, relative to the directory's root.
        path: PathBuf,
        /// What is wrong with it.
        reason: String,
    },
    /// A package declares a schema this build does not read.
    #[error("package `{package}`, {path}: schema {found}, this build reads {expected}")]
    Version {
        /// The package the file belongs to.
        package: String,
        /// The file, relative to the directory's root.
        path: PathBuf,
        /// The version the file declares.
        found: u32,
        /// The version this build writes.
        expected: u32,
    },
    /// A record names a package other than the one that carries it.
    #[error("package `{package}`, {path}: item `{item}` belongs to package {owner}")]
    ForeignItem {
        /// The package that carries the file.
        package: String,
        /// The file, relative to the directory's root.
        path: PathBuf,
        /// The id the record carries.
        item: model::ItemId,
        /// The package that id belongs to.
        owner: u32,
    },
}

/// The stamp of the files a directory is read from.
///
/// A directory is edited by another application, so a reader needs one cheap value that moves when
/// any file it reads changes and stays put when none did: the shell polls it off the frame path and
/// reads a new snapshot only when it moved (`docs/domain/nomenclature.md` §13).
///
/// It is the hash of every file's name, size and modification time, so it moves when a file is
/// added, removed or rewritten. It is compared, never stored and never written to a file, so a
/// build may change its value without a migration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Revision(u64);

impl Revision {
    /// The number itself, for a log line or a test that prints it.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// A source of directory snapshots.
///
/// A reader hands over one value and never a live store (`§15`), so a consumer cannot hold half a
/// directory or accidentally depend on a package. The two halves are separate on purpose: a
/// revision costs one stat of each file, a snapshot parses and validates all of them, so the shell
/// polls the first far more often than it reads the second (`§13`).
pub trait DirectorySource {
    /// Reads the directory as one validated snapshot.
    fn snapshot(&self) -> Result<Directory, LoadError>;

    /// The stamp of the files [`DirectorySource::snapshot`] would read, without reading them.
    ///
    /// A file that is not there is part of the stamp too: a folder that gained its first package
    /// and one that lost one both move it, and a reader that cannot tell `NotFound` from a file it
    /// failed to stat cannot watch a folder at all.
    fn revision(&self) -> Result<Revision, LoadError>;
}

/// The names of the packages a directory lists, in the order they load.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectoryWire {
    version: u32,
    #[serde(default)]
    packages: Vec<String>,
}

/// A directory read from a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderSource {
    root: PathBuf,
}

impl FolderSource {
    /// The folder at `root`.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The folder this source reads.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The packages a connected directory lists, in load order.
    ///
    /// A missing `directory.json` is an empty directory, not a failure: a workspace that has never
    /// connected a package has nothing to read, and the shell offers the starter one instead.
    pub fn packages(&self) -> Result<Vec<String>, LoadError> {
        let path = self.root.join(DIRECTORY_FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(LoadError::Read { path, reason: error.to_string() }),
        };
        let wire: DirectoryWire = serde_json::from_str(&text).map_err(|error| LoadError::File {
            package: DIRECTORY_FILE.to_owned(),
            path: path.clone(),
            reason: error.to_string(),
        })?;
        if wire.version != DIRECTORY_VERSION {
            return Err(LoadError::Version {
                package: DIRECTORY_FILE.to_owned(),
                path,
                found: wire.version,
                expected: DIRECTORY_VERSION,
            });
        }
        Ok(wire.packages)
    }
}

/// One file read into the snapshot, with where it came from.
struct Loaded<T> {
    package: String,
    path: PathBuf,
    value: T,
}

impl FolderSource {
    /// The files a snapshot reads: the connected list, then every package's manifest, registry and
    /// records, in the order the loader reads them.
    ///
    /// A folder that is missing a level contributes no files instead of failing: the stamp exists
    /// to notice a change, and the snapshot is what refuses a package that is not there.
    fn watched(&self) -> Result<Vec<PathBuf>, LoadError> {
        let mut files = vec![self.root.join(DIRECTORY_FILE)];
        for package in self.packages()? {
            let folder = self.root.join(PACKAGES_DIR).join(&package);
            files.push(folder.join(MANIFEST_FILE));
            files.push(folder.join(REGISTRY_FILE));
            let items = folder.join(ITEMS_DIR);
            let names = json_names(&items).map_err(|error| LoadError::Read {
                path: items.clone(),
                reason: format!("package `{package}`: {error}"),
            })?;
            for name in names.unwrap_or_default() {
                files.push(items.join(name));
            }
        }
        Ok(files)
    }
}

impl DirectorySource for FolderSource {
    fn revision(&self) -> Result<Revision, LoadError> {
        let mut hasher = DefaultHasher::new();
        for path in self.watched()? {
            path.hash(&mut hasher);
            let (modified, len) = stamp(&path)?;
            modified.hash(&mut hasher);
            len.hash(&mut hasher);
        }
        Ok(Revision(hasher.finish()))
    }

    fn snapshot(&self) -> Result<Directory, LoadError> {
        let mut kinds = Vec::new();
        let mut properties = Vec::new();
        let mut families = Vec::new();
        let mut items = Vec::new();
        for package in self.packages()? {
            let folder = self.root.join(PACKAGES_DIR).join(&package);
            let manifest_path = folder.join(MANIFEST_FILE);
            let manifest: ManifestWire = read_json(&manifest_path, &package)?;
            if manifest.version != MANIFEST_VERSION {
                return Err(LoadError::Version {
                    package: package.clone(),
                    path: manifest_path,
                    found: manifest.version,
                    expected: MANIFEST_VERSION,
                });
            }

            let registry_path = folder.join(REGISTRY_FILE);
            let registry: RegistryWire = read_json(&registry_path, &package)?;
            if registry.version != REGISTRY_VERSION {
                return Err(LoadError::Version {
                    package: package.clone(),
                    path: registry_path,
                    found: registry.version,
                    expected: REGISTRY_VERSION,
                });
            }
            let (package_kinds, package_properties, package_families) = registry.rows().map_err(|error| {
                LoadError::File { package: package.clone(), path: registry_path.clone(), reason: error.to_string() }
            })?;
            for row in package_kinds {
                kinds.push(Loaded { package: package.clone(), path: registry_path.clone(), value: row });
            }
            for row in package_properties {
                properties.push(Loaded { package: package.clone(), path: registry_path.clone(), value: row });
            }
            for row in package_families {
                families.push(Loaded { package: package.clone(), path: registry_path.clone(), value: row });
            }

            let items_dir = folder.join(ITEMS_DIR);
            for entry in read_dir(&items_dir, &package)? {
                let path = items_dir.join(&entry);
                let wire: crate::wire::ItemWire = read_json(&path, &package)?;
                if wire.id.package() != manifest.id {
                    return Err(LoadError::ForeignItem {
                        package: package.clone(),
                        path,
                        item: wire.id,
                        owner: wire.id.package(),
                    });
                }
                items.push(Loaded { package: package.clone(), path, value: wire });
            }
        }

        let kinds = KindRegistry::new(kinds.iter().map(|row| row.value.clone())).map_err(|error| LoadError::File {
            package: String::new(),
            path: PathBuf::from(REGISTRY_FILE),
            reason: error.to_string(),
        })?;
        let properties = PropertyRegistry::new(properties.iter().map(|row| row.value.clone())).map_err(|error| {
            LoadError::File { package: String::new(), path: PathBuf::from(REGISTRY_FILE), reason: error.to_string() }
        })?;
        let families = FamilyRegistry::new(families.iter().map(|row| row.value.clone())).map_err(|error| {
            LoadError::File { package: String::new(), path: PathBuf::from(REGISTRY_FILE), reason: error.to_string() }
        })?;

        let mut records = Vec::with_capacity(items.len());
        for item in &items {
            let record = item.value.to_item(&kinds, &properties).map_err(|error| LoadError::File {
                package: item.package.clone(),
                path: item.path.clone(),
                reason: error.to_string(),
            })?;
            records.push(record);
        }
        Directory::new(records, kinds, properties, families).map_err(|error| {
            let (package, path) = attributes(&items, &error);
            LoadError::File { package, path, reason: error.to_string() }
        })
    }
}

/// The package and file an item-level refusal belongs to.
fn attributes(items: &[Loaded<crate::wire::ItemWire>], error: &crate::CatalogError) -> (String, PathBuf) {
    let id = match error {
        crate::CatalogError::MissingProperty { item, .. }
        | crate::CatalogError::PropertyNotAccepted { item, .. }
        | crate::CatalogError::Property { item, .. }
        | crate::CatalogError::UnknownKind { item, .. }
        | crate::CatalogError::UnknownProperty { item, .. }
        | crate::CatalogError::DuplicateItem { id: item }
        | crate::CatalogError::DuplicateItemSlug { item, .. }
        | crate::CatalogError::DuplicateExternalKey { item, .. } => Some(*item),
        _ => None,
    };
    items
        .iter()
        .find(|item| Some(item.value.id) == id)
        .map(|item| (item.package.clone(), item.path.clone()))
        .unwrap_or_else(|| (String::new(), PathBuf::from(ITEMS_DIR)))
}

/// Reads one JSON file, naming it and its package when it cannot be read or parsed.
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, package: &str) -> Result<T, LoadError> {
    let text = fs::read_to_string(path)
        .map_err(|error| LoadError::Read { path: path.to_path_buf(), reason: error.to_string() })?;
    serde_json::from_str(&text).map_err(|error| LoadError::File {
        package: package.to_owned(),
        path: path.to_path_buf(),
        reason: error.to_string(),
    })
}

/// The `*.json` files of one folder, in name order, so two loads of one directory agree.
fn read_dir(folder: &Path, package: &str) -> Result<Vec<String>, LoadError> {
    let named = |reason: String| LoadError::Read {
        path: folder.to_path_buf(),
        reason: format!("package `{package}`: {reason}"),
    };
    match json_names(folder) {
        Ok(Some(names)) => Ok(names),
        Ok(None) => Err(named("the folder is not there".to_owned())),
        Err(error) => Err(named(error.to_string())),
    }
}

/// The `*.json` files of one folder in name order, or `None` when the folder is not there.
fn json_names(folder: &Path) -> io::Result<Option<Vec<String>>> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut names = Vec::new();
    for entry in entries {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.ends_with(".json") {
            names.push(name);
        }
    }
    names.sort();
    Ok(Some(names))
}

/// The modification time and the size of one file, or zeroes when it is not there.
///
/// A file that cannot be stat'ed for any other reason is a refusal: a reader that silently stamped
/// `NotFound` over a permission error would reload the folder on every poll of a broken mount.
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::time::{SystemTime, UNIX_EPOCH};

    use model::ItemId;

    use super::*;

    /// A directory under the system temporary folder, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_nanos()).unwrap_or(0);
            let path = std::env::temp_dir().join(format!("catalog-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&path).expect("create the temporary directory");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Writes the starter package's folder and lists it as connected.
    fn starter_folder(root: &Path) -> PathBuf {
        let package = root.join(PACKAGES_DIR).join("starter");
        fs::create_dir_all(package.join(ITEMS_DIR)).expect("create the package");
        fs::write(root.join(DIRECTORY_FILE), serde_json::json!({ "version": 1, "packages": ["starter"] }).to_string())
            .expect("write the connected list");
        fs::write(
            package.join(MANIFEST_FILE),
            serde_json::json!({ "version": 1, "id": 1, "name": "Starter", "package_version": 1 }).to_string(),
        )
        .expect("write the manifest");
        fs::write(
            package.join(REGISTRY_FILE),
            serde_json::json!({
                "version": 1,
                "properties": [
                    { "code": 1, "slug": "thickness", "label": "Thickness", "type": "length", "min_um": 1000, "max_um": 1000000 },
                    { "code": 2, "slug": "width", "label": "Width", "type": "length", "min_um": 1000, "max_um": 1000000 },
                    { "code": 3, "slug": "color", "label": "Decor", "type": "text", "max_chars": 48 }
                ],
                "kinds": [{
                    "code": 1, "slug": "sheet-material", "label": "Sheet material",
                    "accepts": [1, 2, 3], "required": [1], "traits": ["sheet", "nestable"]
                }, {
                    "code": 2, "slug": "edge-banding", "label": "Edge banding",
                    "accepts": [1, 2, 3], "required": [1, 2], "traits": ["banded", "bought"]
                }],
                "families": []
            })
            .to_string(),
        )
        .expect("write the registry");
        fs::write(
            package.join(ITEMS_DIR).join("board-1.json"),
            serde_json::json!({
                "id": "1-1", "kind": "sheet-material", "name": "White melamine", "slug": "board-1",
                "props": { "thickness": { "type": "length", "value": 18000 }, "color": { "type": "text", "value": "white" } }
            })
            .to_string(),
        )
        .expect("write the record");
        fs::write(
            package.join(ITEMS_DIR).join("band-101.json"),
            serde_json::json!({
                "id": "1-101", "kind": "edge-banding", "name": "PVC edge", "slug": "band-101",
                "props": {
                    "thickness": { "type": "length", "value": 2000 },
                    "width": { "type": "length", "value": 22000 },
                    "color": { "type": "text", "value": "white" }
                }
            })
            .to_string(),
        )
        .expect("write the band");
        package
    }

    #[test]
    fn a_package_folder_loads_as_one_snapshot() {
        let temp = TempDir::new("load");
        starter_folder(&temp.0);
        let directory = FolderSource::at(&temp.0).snapshot().expect("the package loads");
        assert_eq!(directory.len(), 2, "a sheet and a band");
        let id = ItemId::new(1, 1).expect("an item id");
        let board = directory.board(id).expect("a board");
        assert_eq!(board.name, "White melamine");
        assert_eq!(board.thickness, units::Length::from_micrometres(18_000));
        assert_eq!(board.color.as_deref(), Some("white"), "a property is named by its slug");
        let band = directory.banding(ItemId::new(1, 101).expect("an item id")).expect("a band");
        assert_eq!(
            (band.thickness, band.width),
            (units::Length::from_micrometres(2_000), units::Length::from_micrometres(22_000))
        );
    }

    #[test]
    fn an_empty_folder_is_an_empty_directory_and_not_a_failure() {
        let temp = TempDir::new("empty");
        let directory = FolderSource::at(&temp.0).snapshot().expect("nothing to load");
        assert!(directory.is_empty(), "a workspace that connected nothing has no records");
    }

    #[test]
    fn the_revision_moves_when_a_file_the_loader_reads_changes() {
        let temp = TempDir::new("revision");
        let source = FolderSource::at(&temp.0);
        // Nothing is connected yet, so the stamp describes a folder that will refuse a package and
        // a folder that holds none alike; either way it is one value the shell can poll.
        let empty = source.revision().expect("a revision of a folder with nothing in it");
        assert_eq!(
            empty,
            source.revision().expect("a second read of an untouched folder"),
            "an idle folder keeps its stamp"
        );

        let package = starter_folder(&temp.0);
        let connected = source.revision().expect("a revision of a connected folder");
        assert_ne!(connected, empty, "connecting a package moves the stamp");

        let record = package.join(ITEMS_DIR).join("board-1.json");
        let text = fs::read_to_string(&record).expect("read the record");
        fs::write(&record, text.replace("18000", "25000")).expect("write the record");
        assert_ne!(source.revision().expect("a revision"), connected, "editing a record moves the stamp");

        fs::remove_file(&record).expect("remove the record");
        assert!(source.revision().is_ok(), "a file that is gone is part of the stamp, not a refusal");
    }

    #[test]
    fn the_revision_of_a_folder_nobody_created_is_a_value_and_not_a_failure() {
        let temp = TempDir::new("absent");
        let source = FolderSource::at(temp.0.join("never-created"));
        assert!(source.revision().is_ok(), "a workspace that connected no folder still has a stamp to poll");
    }

    #[test]
    fn a_record_whose_id_belongs_to_another_package_is_refused_with_both_named() {
        let temp = TempDir::new("foreign");
        let package = starter_folder(&temp.0);
        fs::write(
            package.join(ITEMS_DIR).join("board-1.json"),
            serde_json::json!({
                "id": "2-1", "kind": "sheet-material", "name": "Someone else's", "slug": "board-1",
                "props": { "thickness": { "type": "length", "value": 18000 } }
            })
            .to_string(),
        )
        .expect("write the record");
        let refusal = FolderSource::at(&temp.0).snapshot().expect_err("a foreign id");
        let sentence = refusal.to_string();
        assert!(
            sentence.contains("2-1") && sentence.contains("starter") && sentence.contains("board-1.json"),
            "{sentence}"
        );
    }

    #[test]
    fn a_record_breaking_a_row_rule_is_refused_with_the_package_and_the_file() {
        let temp = TempDir::new("row");
        let package = starter_folder(&temp.0);
        fs::write(
            package.join(ITEMS_DIR).join("board-1.json"),
            serde_json::json!({
                "id": "1-1", "kind": "sheet-material", "name": "White melamine", "slug": "board-1",
                "props": { "thickness": { "type": "length", "value": 5_000_000 } }
            })
            .to_string(),
        )
        .expect("write the record");
        let refusal = FolderSource::at(&temp.0).snapshot().expect_err("5000 mm is outside the row");
        let sentence = refusal.to_string();
        assert!(sentence.contains("starter"), "the package is named: {sentence}");
        assert!(sentence.contains("board-1.json"), "the file is named: {sentence}");
        assert!(sentence.contains("5000 mm"), "the value is named: {sentence}");
    }

    #[test]
    fn a_mistyped_row_is_refused_with_its_file() {
        let temp = TempDir::new("mistyped");
        let package = starter_folder(&temp.0);
        fs::write(
            package.join(REGISTRY_FILE),
            serde_json::json!({ "version": 1, "kinds": [{ "code": 1, "slug": "sheet-material" }] }).to_string(),
        )
        .expect("write the registry");
        let refusal = FolderSource::at(&temp.0).snapshot().expect_err("a kind without a label");
        let sentence = refusal.to_string();
        assert!(sentence.contains("starter") && sentence.contains("registry.json"), "{sentence}");
    }

    #[test]
    fn a_package_of_another_schema_is_a_readable_refusal() {
        let temp = TempDir::new("version");
        let package = starter_folder(&temp.0);
        fs::write(
            package.join(MANIFEST_FILE),
            serde_json::json!({ "version": 2, "id": 1, "name": "Starter" }).to_string(),
        )
        .expect("write the manifest");
        let refusal = FolderSource::at(&temp.0).snapshot().expect_err("a newer package");
        let sentence = refusal.to_string();
        assert!(sentence.contains("schema 2") && sentence.contains("reads 1"), "{sentence}");
    }
}
