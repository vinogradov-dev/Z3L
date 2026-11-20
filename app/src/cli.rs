//! The command line: `z3l export` writes an order package without a window, `z3l run` dispatches a
//! recipe, `z3l query` answers a question about the scene, `z3l <scene>` opens one.
//!
//! A package is useless if producing it needs a window and a person, so the subcommand is read and
//! performed in [`main`](crate::main) before eframe creates a surface: a headless job over saved
//! files gets the same writer the editor will use, with no GPU, no UI and no second binary. A scene
//! named on the line is the editor's own Open, so a double click in a file manager and a script that
//! hands the program a saved file need no click through Home.
//!
//! The three headless surfaces all reach the document through the one door: [`recipe`] dispatches
//! `engine` commands into one session and one undo entry, [`query`] reads the document and the
//! catalogue without mutating either, and [`commands`] lists what this build can dispatch. Nothing
//! here writes a scene except through [`project::SceneFile`], so a script cannot bypass the
//! validation a command performs (ADR 0028, `docs/domain/prototypes.md` §8).
//!
//! The parsing, the export and the error are one value each, so the test drives the whole job and
//! no window is ever constructed: [`parse`] turns the argument list into one [`Command`], [`export`]
//! writes the package and returns what to print, and [`CliError`] carries a sentence the user can
//! read. [`run`] writes the report and returns the exit code. Nothing here panics: a closed pipe is
//! a failed write like any other, and the only exit is the caller's `std::process::exit`.

mod commands;
mod query;
mod recipe;

pub use query::QueryKind;

use std::io::Write as _;
use std::path::PathBuf;

use engine::CommandError;
use export::{DocumentMeta, ExportError, ExportOptions, OrderPackage, now_timestamp};
use model::Id;
use project::{SceneFile, SceneFileError};

/// The exit code of a command that did its job.
pub const EXIT_OK: i32 = 0;

/// The exit code of a command that could not do its job.
pub const EXIT_FAILED: i32 = 1;

/// The exit code of an argument list that names no command.
pub const EXIT_USAGE: i32 = 2;

/// The help text, printed by `z3l --help` and `z3l export --help`.
pub const USAGE: &str = "\
usage: z3l                 open the editor
       z3l <scene.z3l>     open the editor on that scene
       z3l directory [<folder>]  open the nomenclature directory application
       z3l --help          print this text
       z3l export <scene.z3l> --out <path> [--format bundle|single] [--no-timestamp]
       z3l commands [--json]
       z3l run <scene.z3l> --commands <recipe.jsonl> [--out <scene>] [--product <node>] [--dry-run]
       z3l query <scene.z3l> tree|box|parts|diagnostics|niche [--id <node>] [--at <x,y,z>]

export writes an order package from a saved scene without opening a window:
  --out <path>       the package directory for `bundle`, the package file for `single`
  --format <form>    `bundle` (the default) writes the manifest plus one JSON Lines file and a CSV
                     copy per table; `single` writes the manifest and every table into one JSON file
  --no-timestamp     leave the export time out of the manifest, so a package can be compared

commands lists what this build can dispatch: one id, its label and what it creates.
  --json             print one JSON array instead of one line per command

run dispatches a recipe -- one JSON object per line, `{\"command\": <id>, \"params\": {...}}` --
into one session. The whole file is one undo entry's worth of work: the scene is read once, the
commands run in order, and the result is written once, at the end -- or not at all if a line fails.
  --commands <path>  the recipe file, one command per line; a blank line is skipped
  --out <scene>      write the result here instead of over the scene the line read
  --product <node>   the product the recipe works in; a scene with exactly one product opens it
                     by itself, and a part can only be created inside an open one
  --dry-run          dispatch every command and write nothing, so the recipe can be checked first

query reads the scene and prints one JSON report; it never writes:
  tree         every node with its id, kind, name and children
  box          the box a product reserves, in its own frame and in world millimetres
  parts        the cut list -- the published `parts` table of the order package
  diagnostics  every panel problem the catalogue resolves, with a stable code
  niche        the empty space around a point: `--at <x,y,z>` in world millimetres, or `--id`
               of a node for the space at its origin
  --id <node>  the node a report is about, where the report takes one
  --at <x,y,z> the point `niche` measures, in world millimetres
";

/// What one run of the program does.
///
/// The comparison is partial, because a query may name a world point and a point is a float.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Arguments name no command: open the editor, which is what a double click does.
    Window,
    /// Open the editor on a scene: the same tab the File menu's *Open* makes.
    ///
    /// Whether the file exists is the opener's business, so a mistyped name reaches the status bar
    /// as a sentence instead of ending the process with a usage code.
    Scene(PathBuf),
    /// Open the directory application: the window that edits a nomenclature folder.
    ///
    /// The folder is the one the editor watches unless the line names another, so a workshop can hold
    /// two packages side by side and edit one of them.
    Directory(Option<PathBuf>),
    /// Print this text and exit.
    Help(&'static str),
    /// Write an order package and exit.
    Export(ExportArgs),
    /// List the commands this build can dispatch and exit.
    Commands {
        /// Print one JSON array instead of one line per command.
        json: bool,
    },
    /// Dispatch a recipe into one session and save the result once.
    Run(RunArgs),
    /// Read the scene and print one JSON report.
    Query(QueryArgs),
}

/// One `run` request, already checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunArgs {
    /// The saved scene to read.
    pub scene: PathBuf,
    /// The recipe file: one command per line.
    pub commands: PathBuf,
    /// Where the result is written; the scene read when it is absent.
    pub out: Option<PathBuf>,
    /// The product the run works in; the document's only product when it is absent.
    pub product: Option<Id>,
    /// Whether the recipe runs without writing anything.
    pub dry_run: bool,
}

/// One `query` request, already checked.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryArgs {
    /// The saved scene to read.
    pub scene: PathBuf,
    /// Which report to print.
    pub kind: QueryKind,
    /// The node the report is about, when the report takes one.
    pub id: Option<Id>,
    /// The point `niche` measures, in world millimetres.
    pub at: Option<[f64; 3]>,
}

/// One `export` request, already checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportArgs {
    /// The saved scene to read.
    pub scene: PathBuf,
    /// The package directory (`bundle`) or package file (`single`).
    pub out: PathBuf,
    /// Which of the two package forms to write.
    pub format: Format,
    /// Whether the manifest carries the export time.
    pub timestamp: bool,
}

/// The two forms the writer produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// A directory: the manifest, one JSON Lines file per table and a CSV copy of each.
    Bundle,
    /// One JSON file carrying the manifest and every table.
    Single,
}

impl Format {
    /// The flag's value, or a sentence naming the two forms.
    fn parse(word: &str) -> Result<Self, CliError> {
        match word {
            "bundle" => Ok(Self::Bundle),
            "single" => Ok(Self::Single),
            other => Err(CliError::Usage(format!("`{other}` is not a package format; name `bundle` or `single`"))),
        }
    }
}

/// Why a run stopped: every variant prints one sentence the user can act on.
#[derive(Debug)]
pub enum CliError {
    /// The argument list names no command or misses a value.
    Usage(String),
    /// The scene file a headless command read or wrote cannot be parsed.
    Scene(SceneFileError),
    /// The default catalogue a new session starts from cannot be built.
    Catalog(String),
    /// The package cannot be built or written.
    Export(ExportError),
    /// The recipe file cannot be read, or one of its lines is not a step.
    Recipe(String),
    /// One recipe line dispatched a command that failed, so nothing was written.
    Dispatch {
        /// The line of the recipe file, counting from one.
        line: usize,
        /// The command id the line named.
        command: String,
        /// Why the command refused.
        error: CommandError,
    },
    /// A query cannot answer what it was asked.
    Query(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) => formatter.write_str(message),
            Self::Scene(error) => error.fmt(formatter),
            Self::Catalog(message) => formatter.write_str(message),
            Self::Export(error) => error.fmt(formatter),
            Self::Recipe(message) => formatter.write_str(message),
            Self::Dispatch { line, command, error } => write!(formatter, "line {line} `{command}`: {error}"),
            Self::Query(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CliError {}

/// Reads the argument list, without the program name.
///
/// A macOS launch from Finder carries the process serial number as an argument; it asks for the
/// window like an empty list does, because it is the platform talking, not a person.
pub fn parse(args: &[String]) -> Result<Command, CliError> {
    match args.first().map(String::as_str) {
        None => Ok(Command::Window),
        Some("-h" | "--help" | "help") => Ok(Command::Help(USAGE)),
        Some("export") => export_args(&args[1..]),
        Some("commands") => commands_args(&args[1..]),
        Some("run") => run_args(&args[1..]),
        Some("query") => query_args(&args[1..]),
        Some("directory") => directory_args(&args[1..]),
        Some(other) if other.starts_with("-psn_") => Ok(Command::Window),
        // One scene path opens the editor on it; the extension is the claim, because a path that
        // does not read yet is the opener's error and not a command-line one.
        Some(other) if is_scene_path(other) => scene_path(args),
        Some(other) => Err(CliError::Usage(format!("`{other}` is not a command; run `z3l --help`"))),
    }
}

/// Whether a word names a scene file.
fn is_scene_path(word: &str) -> bool {
    std::path::Path::new(word).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("z3l"))
}

/// Reads the words after `directory`: no folder, or exactly one.
fn directory_args(args: &[String]) -> Result<Command, CliError> {
    let mut words = args.iter();
    let Some(folder) = words.next() else { return Ok(Command::Directory(None)) };
    if folder.starts_with('-') {
        return Err(CliError::Usage(format!("`{folder}` is not a directory option; run `z3l --help`")));
    }
    if let Some(extra) = words.next() {
        return Err(CliError::Usage(format!("the directory application opens one folder, so `{extra}` has no place")));
    }
    Ok(Command::Directory(Some(PathBuf::from(folder))))
}

/// Reads the whole line of a window run: exactly one scene path and nothing else.
fn scene_path(args: &[String]) -> Result<Command, CliError> {
    let mut words = args.iter();
    let Some(scene) = words.next().map(PathBuf::from) else {
        return Err(CliError::Usage("the window opens one scene".to_owned()));
    };
    if let Some(extra) = words.next() {
        return Err(CliError::Usage(format!("the window opens one scene, so `{extra}` has no place")));
    }
    Ok(Command::Scene(scene))
}

/// Reads the words after `export`.
fn export_args(args: &[String]) -> Result<Command, CliError> {
    let mut scene: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut format = Format::Bundle;
    let mut timestamp = true;
    let mut words = args.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "-h" | "--help" => return Ok(Command::Help(USAGE)),
            "--out" => out = Some(PathBuf::from(value(&mut words, "--out")?)),
            "--format" => format = Format::parse(value(&mut words, "--format")?)?,
            "--no-timestamp" => timestamp = false,
            other if other.starts_with('-') => {
                return Err(CliError::Usage(format!("`{other}` is not an export option; run `z3l export --help`")));
            }
            other if scene.is_some() => {
                return Err(CliError::Usage(format!("export reads one scene, so `{other}` has no place")));
            }
            other => scene = Some(PathBuf::from(other)),
        }
    }
    let scene = scene
        .ok_or_else(|| CliError::Usage("export needs a scene: `z3l export <scene.z3l> --out <path>`".to_owned()))?;
    let out = out.ok_or_else(|| CliError::Usage("export needs `--out <path>` to know where to write".to_owned()))?;
    Ok(Command::Export(ExportArgs { scene, out, format, timestamp }))
}

/// The value that follows a flag.
fn value<'a>(words: &mut std::slice::Iter<'a, String>, flag: &str) -> Result<&'a str, CliError> {
    words.next().map(String::as_str).ok_or_else(|| CliError::Usage(format!("`{flag}` needs a value")))
}

/// Reads the words after `commands`: nothing, or `--json`.
fn commands_args(args: &[String]) -> Result<Command, CliError> {
    let mut json = false;
    for word in args {
        match word.as_str() {
            "-h" | "--help" => return Ok(Command::Help(USAGE)),
            "--json" => json = true,
            other => {
                return Err(CliError::Usage(format!("`{other}` is not an option of `commands`; run `z3l --help`")));
            }
        }
    }
    Ok(Command::Commands { json })
}

/// Reads the words after `run`: one scene, one recipe, an optional output and the dry-run flag.
fn run_args(args: &[String]) -> Result<Command, CliError> {
    let mut scene: Option<PathBuf> = None;
    let mut commands: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut product: Option<Id> = None;
    let mut dry_run = false;
    let mut words = args.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "-h" | "--help" => return Ok(Command::Help(USAGE)),
            "--commands" => commands = Some(PathBuf::from(value(&mut words, "--commands")?)),
            "--out" => out = Some(PathBuf::from(value(&mut words, "--out")?)),
            "--product" => product = Some(node_id(value(&mut words, "--product")?)?),
            "--dry-run" => dry_run = true,
            other if other.starts_with('-') => {
                return Err(CliError::Usage(format!("`{other}` is not a run option; run `z3l --help`")));
            }
            other if scene.is_some() => {
                return Err(CliError::Usage(format!("run reads one scene, so `{other}` has no place")));
            }
            other => scene = Some(PathBuf::from(other)),
        }
    }
    let scene = scene
        .ok_or_else(|| CliError::Usage("run needs a scene: `z3l run <scene.z3l> --commands <recipe>`".to_owned()))?;
    let commands =
        commands.ok_or_else(|| CliError::Usage("run needs `--commands <path>` to know what to dispatch".to_owned()))?;
    Ok(Command::Run(RunArgs { scene, commands, out, product, dry_run }))
}

/// Reads the words after `query`: one scene, one report kind, and the flag that report takes.
fn query_args(args: &[String]) -> Result<Command, CliError> {
    let mut scene: Option<PathBuf> = None;
    let mut kind: Option<QueryKind> = None;
    let mut id: Option<Id> = None;
    let mut at: Option<[f64; 3]> = None;
    let mut words = args.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "-h" | "--help" => return Ok(Command::Help(USAGE)),
            "--id" => id = Some(node_id(value(&mut words, "--id")?)?),
            "--at" => at = Some(point(value(&mut words, "--at")?)?),
            other if other.starts_with('-') => {
                return Err(CliError::Usage(format!("`{other}` is not a query option; run `z3l --help`")));
            }
            other if scene.is_none() => scene = Some(PathBuf::from(other)),
            other if kind.is_none() => kind = Some(QueryKind::parse(other)?),
            other => {
                return Err(CliError::Usage(format!(
                    "a query reads one scene and one report, so `{other}` has no place"
                )));
            }
        }
    }
    let scene = scene.ok_or_else(|| CliError::Usage("query needs a scene: `z3l query <scene.z3l> tree`".to_owned()))?;
    let kind = kind.ok_or_else(|| CliError::Usage(format!("query needs a report: {}", QueryKind::WORDS)))?;
    Ok(Command::Query(QueryArgs { scene, kind, id, at }))
}

/// Reads a node id: a non-zero whole number, because zero is the document's "no entity".
fn node_id(text: &str) -> Result<Id, CliError> {
    text.parse::<u64>()
        .ok()
        .and_then(Id::new)
        .ok_or_else(|| CliError::Usage(format!("`{text}` is not a node id; it is one whole number, and zero is no id")))
}

/// Reads a point: three world millimetres, comma separated.
fn point(text: &str) -> Result<[f64; 3], CliError> {
    let mut axes = [0.0; 3];
    let mut count = 0_usize;
    for word in text.split(',') {
        let axis = word.trim().parse::<f64>().ok().filter(|value| value.is_finite()).ok_or_else(|| {
            CliError::Usage(format!("`{word}` is not a coordinate; write `--at x,y,z` in millimetres"))
        })?;
        let Some(slot) = axes.get_mut(count) else {
            return Err(CliError::Usage(format!("`{text}` has more than three coordinates")));
        };
        *slot = axis;
        count = count.saturating_add(1);
    }
    if count != 3 {
        return Err(CliError::Usage(format!("`{text}` needs three coordinates, not {count}")));
    }
    Ok(axes)
}

/// Reads the scene and writes the package, returning one line per written table.
///
/// The catalogue is the one a new session starts from: the scene file does not carry it
/// (`docs/domain/scene-file.md`), and the package embeds the materials the order refers to, so a
/// headless run must open the same default catalogue the editor would. The scene file's own name and
/// revision are recorded in the manifest, which is how a reader ties a package to its document.
pub fn export(args: &ExportArgs) -> Result<Vec<String>, CliError> {
    let scene = SceneFile::read(&args.scene).map_err(CliError::Scene)?;
    let catalog = crate::scene_build::default_catalog().map_err(CliError::Catalog)?;
    let timestamp = if args.timestamp { Some(now_timestamp().map_err(CliError::Export)?) } else { None };
    let options = ExportOptions {
        timestamp,
        document: DocumentMeta {
            file: args.scene.file_name().map(|name| name.to_string_lossy().into_owned()),
            revision: scene.document().revision().get(),
            hash: None,
        },
        order: None,
    };
    let package = OrderPackage::build(scene.document(), &catalog, options).map_err(CliError::Export)?;
    let mut lines = Vec::new();
    match args.format {
        Format::Bundle => {
            package.write_dir(&args.out).map_err(CliError::Export)?;
            lines.push(format!("wrote {}/manifest.json", args.out.display()));
            for table in package.tables() {
                lines.push(format!("{:<11} {:>4} rows  {}", table.spec().name, table.rows().len(), table.spec().file));
            }
        }
        Format::Single => {
            package.write_single(&args.out).map_err(CliError::Export)?;
            lines.push(format!("wrote {}", args.out.display()));
            for table in package.tables() {
                lines.push(format!("{:<11} {:>4} rows", table.spec().name, table.rows().len()));
            }
        }
    }
    Ok(lines)
}

/// Performs a parsed command, writes its report and returns the exit code the process leaves with.
///
/// `None` means the command asks for the editor: the caller opens the window. A write that fails —
/// a closed pipe under `| head` — is a failed run, never a panic.
pub fn run(command: &Command) -> Option<i32> {
    match command {
        Command::Window => None,
        Command::Scene(_) => None,
        Command::Directory(_) => None,
        Command::Help(text) => Some(print_stdout(text)),
        Command::Export(args) => Some(match export(args) {
            Ok(lines) => print_stdout(&(lines.join("\n") + "\n")),
            Err(error) => fail(&error, EXIT_FAILED),
        }),
        Command::Commands { json } => Some(print_stdout(&(commands::report(*json) + "\n"))),
        Command::Run(args) => Some(match recipe::perform(args) {
            Ok(lines) => print_stdout(&(lines.join("\n") + "\n")),
            Err(error) => fail(&error, EXIT_FAILED),
        }),
        Command::Query(args) => Some(match query::report(args) {
            Ok(report) => print_stdout(&(report + "\n")),
            Err(error) => fail(&error, EXIT_FAILED),
        }),
    }
}

/// Prints one sentence about a failed run and returns the code to leave with.
pub fn fail(error: &CliError, code: i32) -> i32 {
    let mut stream = std::io::stderr().lock();
    let _ = writeln!(stream, "z3l: {error}");
    code
}

/// Writes a report and returns the exit code the write earned.
fn print_stdout(text: &str) -> i32 {
    let mut stream = std::io::stdout().lock();
    match stream.write_all(text.as_bytes()).and_then(|()| stream.flush()) {
        Ok(()) => EXIT_OK,
        Err(_) => EXIT_FAILED,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::path::Path;

    use model::{PanelNominalSize, Placement, Project};
    use serde_json::Value;
    use testkit::{millimetres, sample_material_id, sample_product_project, scratch_dir};

    use super::*;

    /// The words a test passes, without the program name.
    fn words(line: &[&str]) -> Vec<String> {
        line.iter().map(|word| (*word).to_owned()).collect()
    }

    /// Writes the fixture scene and returns its path inside a fresh scratch directory.
    fn saved_scene(directory: &Path) -> PathBuf {
        let (mut project, product) = sample_product_project();
        add_side(&mut project, product);
        let path = directory.join("cabinet.z3l");
        SceneFile::new(project).write(&path).expect("the fixture scene is saved");
        path
    }

    /// One panel under the product, so at least one table has a record that came from the document.
    fn add_side(project: &mut Project, product: model::Id) {
        let size = PanelNominalSize::new(millimetres(700), millimetres(500)).expect("a valid size");
        project
            .add_panel(product, "Side", size, Some(sample_material_id()), millimetres(18), Placement::IDENTITY)
            .expect("the side is added");
    }

    #[test]
    fn a_bundle_is_written_from_a_saved_scene_without_a_window() {
        let directory = scratch_dir("k3-bundle");
        let scene = saved_scene(&directory);
        let out = directory.join("package");
        let command = parse(&words(&[
            "export",
            &scene.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--format",
            "bundle",
            "--no-timestamp",
        ]))
        .expect("the line parses");
        let Command::Export(args) = command else {
            panic!("the line names an export");
        };
        let lines = export(&args).expect("the package is written");

        let manifest = read_json(&out.join("manifest.json"));
        assert_eq!(manifest["format"], "furniture.order");
        assert_eq!(manifest["document"]["file"], "cabinet.z3l");
        assert!(manifest["exported_at"].is_null(), "`--no-timestamp` leaves the export time out");
        let tables = manifest["tables"].as_array().expect("the manifest declares its tables");
        assert_eq!(lines.len(), tables.len() + 1, "one line names the package, one per table");

        let parts = manifest["counts"]["parts"].as_u64().expect("parts are counted");
        assert_eq!(parts, 3, "the sample's shelf and front plus the panel the fixture added are three parts");
        let rows = read_text(&out.join("parts.jsonl"));
        assert_eq!(rows.lines().count() as u64, parts, "the table holds one record per count");
        assert!(rows.contains("\"length_um\":700000"), "the part carries its measured size: {rows}");
    }

    #[test]
    fn the_single_form_is_one_file_that_names_every_table() {
        let directory = scratch_dir("k3-single");
        let scene = saved_scene(&directory);
        let out = directory.join("package.json");
        let command = parse(&words(&[
            "export",
            &scene.display().to_string(),
            "--out",
            &out.display().to_string(),
            "--format",
            "single",
            "--no-timestamp",
        ]))
        .expect("the line parses");
        let Command::Export(args) = command else {
            panic!("the line names an export");
        };
        let lines = export(&args).expect("the package is written");
        let package = read_json(&out);
        assert_eq!(package["manifest"]["format"], "furniture.order");
        assert!(lines.iter().any(|line| line.starts_with("parts")), "a line per table: {lines:?}");
        let tables = package["tables"].as_object().expect("the single file carries every table");
        assert_eq!(tables.len(), package["manifest"]["tables"].as_array().expect("declared").len());
        assert_eq!(lines.len(), tables.len() + 1);
    }

    #[test]
    fn the_export_time_is_written_unless_it_is_refused() {
        let directory = scratch_dir("k3-clock");
        let scene = saved_scene(&directory);
        let command = parse(&words(&[
            "export",
            &scene.display().to_string(),
            "--out",
            &directory.join("package").display().to_string(),
        ]))
        .expect("the line parses");
        let Command::Export(args) = command else {
            panic!("the line names an export");
        };
        export(&args).expect("the package is written");
        let manifest = read_json(&directory.join("package/manifest.json"));
        let exported_at = manifest["exported_at"].as_str().expect("the default manifest carries its time");
        assert!(exported_at.ends_with('Z') && exported_at.contains('T'), "the time is RFC 3339: {exported_at}");
    }

    #[test]
    fn the_directory_application_takes_one_folder_or_the_one_the_editor_watches() {
        // O6: the window that edits a package is a mode of this binary, and the folder is its one
        // argument — no folder means the directory the editor itself reads.
        assert_eq!(parse(&words(&["directory"])).expect("no folder"), Command::Directory(None));
        let named = parse(&words(&["directory", "/tmp/workshop"])).expect("a folder");
        assert_eq!(named, Command::Directory(Some(PathBuf::from("/tmp/workshop"))));
        let error = parse(&words(&["directory", "one", "two"])).expect_err("one folder at a time");
        assert!(error.to_string().contains("one folder"), "{error}");
        assert!(run(&named).is_none(), "the command asks for a window, so nothing is printed");
    }

    #[test]
    fn an_unknown_format_is_refused_by_name() {
        let error =
            parse(&words(&["export", "scene.z3l", "--out", "out", "--format", "csv"])).expect_err("csv is not a form");
        let message = error.to_string();
        assert!(message.contains("csv"), "the message quotes the value: {message}");
        assert!(message.contains("bundle") && message.contains("single"), "and names both forms: {message}");
    }

    #[test]
    fn a_missing_scene_or_out_is_a_sentence_not_a_guess() {
        for line in [&["export"][..], &["export", "scene.z3l"][..]] {
            let error = parse(&words(line)).expect_err("the line is incomplete");
            assert!(error.to_string().starts_with("export needs"), "{error}");
        }
        let error = parse(&words(&["export", "scene.z3l", "--out"])).expect_err("`--out` has no value");
        assert!(error.to_string().contains("needs a value"), "{error}");
    }

    #[test]
    fn an_unreadable_scene_and_an_unknown_command_are_readable_errors() {
        let directory = scratch_dir("k3-errors");
        let command = parse(&words(&[
            "export",
            &directory.join("absent.z3l").display().to_string(),
            "--out",
            &directory.join("out").display().to_string(),
        ]))
        .expect("the line parses");
        let Command::Export(args) = command else {
            panic!("the line names an export");
        };
        let message = export(&args).expect_err("the scene does not exist").to_string();
        assert!(message.contains("cannot read"), "the scene error says what failed: {message}");
        assert!(message.contains("absent.z3l"), "and which file: {message}");

        let message = parse(&words(&["expotr"])).expect_err("a typo is not a command").to_string();
        assert!(message.contains("expotr") && message.contains("--help"), "{message}");
    }

    #[test]
    fn no_words_asks_for_the_window_and_help_prints_the_flags() {
        assert_eq!(parse(&[]).expect("an empty list asks for the window"), Command::Window);
        assert_eq!(parse(&words(&["-psn_0_12345"])).expect("a Finder launch asks for the window"), Command::Window);
        assert_eq!(parse(&words(&["--help"])).expect("help is a command"), Command::Help(USAGE));
        assert_eq!(parse(&words(&["export", "--help"])).expect("the subcommand has help"), Command::Help(USAGE));
        for flag in [
            "--out",
            "--format",
            "--no-timestamp",
            "bundle|single",
            "--json",
            "--commands",
            "--product",
            "--dry-run",
            "--id",
            "--at",
            "tree|box|parts|diagnostics|niche",
        ] {
            assert!(USAGE.contains(flag), "the help names `{flag}`: {USAGE}");
        }
        assert_eq!(run(&Command::Window), None, "only the window command asks the caller to open one");
    }

    #[test]
    fn the_headless_commands_read_their_own_flags() {
        assert_eq!(parse(&words(&["commands"])).expect("no flag"), Command::Commands { json: false });
        assert_eq!(parse(&words(&["commands", "--json"])).expect("`--json`"), Command::Commands { json: true });
        let error = parse(&words(&["commands", "--yaml"])).expect_err("there is no such flag");
        assert!(error.to_string().contains("--yaml"), "{error}");

        let run = parse(&words(&[
            "run",
            "scene.z3l",
            "--commands",
            "recipe.jsonl",
            "--out",
            "built.z3l",
            "--product",
            "5",
            "--dry-run",
        ]))
        .expect("every run flag at once");
        assert_eq!(
            run,
            Command::Run(RunArgs {
                scene: PathBuf::from("scene.z3l"),
                commands: PathBuf::from("recipe.jsonl"),
                out: Some(PathBuf::from("built.z3l")),
                product: model::Id::new(5),
                dry_run: true,
            })
        );

        let query = parse(&words(&["query", "scene.z3l", "niche", "--at", "1.5,-2,3"])).expect("a niche");
        assert_eq!(
            query,
            Command::Query(QueryArgs {
                scene: PathBuf::from("scene.z3l"),
                kind: QueryKind::Niche,
                id: None,
                at: Some([1.5, -2.0, 3.0]),
            })
        );
    }

    #[test]
    fn one_scene_path_asks_for_the_window_on_that_scene() {
        // A double click in a file manager and a script hand the window one saved file: the
        // extension is the claim, so a path that does not exist yet still reaches the opener, which
        // is the surface that can say why it did not open.
        let scene = PathBuf::from("/tmp/cabinet.z3l");
        assert_eq!(parse(&words(&[&scene.display().to_string()])).expect("one path opens it"), Command::Scene(scene));
        assert_eq!(
            parse(&words(&["/tmp/absent.z3l"])).expect("a missing scene is the opener's error"),
            Command::Scene(PathBuf::from("/tmp/absent.z3l"))
        );
        assert_eq!(
            parse(&words(&["CABINET.Z3L"])).expect("the extension is not case sensitive"),
            Command::Scene(PathBuf::from("CABINET.Z3L"))
        );
        assert_eq!(run(&parse(&words(&["/tmp/cabinet.z3l"])).expect("one path")), None, "the window opens it");

        let error = parse(&words(&["a.z3l", "b.z3l"])).expect_err("the window holds one scene");
        assert!(error.to_string().contains("b.z3l"), "the extra word is named: {error}");
        let error = parse(&words(&["cabinet.obj"])).expect_err("another format is not a command");
        assert!(error.to_string().contains("--help"), "{error}");
    }

    fn read_text(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_str(&read_text(path)).unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()))
    }
}
