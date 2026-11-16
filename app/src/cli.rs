//! The command line: `z3l export` writes an order package without opening a window (K3).
//!
//! A package is useless if producing it needs a window and a person, so the subcommand is read and
//! performed in [`main`](crate::main) before eframe creates a surface: a headless job over saved
//! files gets the same writer the editor will use, with no GPU, no UI and no second binary.
//!
//! The parsing, the export and the error are one value each, so the test drives the whole job and
//! no window is ever constructed: [`parse`] turns the argument list into one [`Command`], [`export`]
//! writes the package and returns what to print, and [`CliError`] carries a sentence the user can
//! read. [`run`] writes the report and returns the exit code. Nothing here panics: a closed pipe is
//! a failed write like any other, and the only exit is the caller's `std::process::exit`.

use std::io::Write as _;
use std::path::PathBuf;

use export::{DocumentMeta, ExportError, ExportOptions, OrderPackage, now_timestamp};
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
       z3l --help          print this text
       z3l export <scene.z3l> --out <path> [--format bundle|single] [--no-timestamp]

export writes an order package from a saved scene without opening a window:
  --out <path>       the package directory for `bundle`, the package file for `single`
  --format <form>    `bundle` (the default) writes the manifest plus one JSON Lines file and a CSV
                     copy per table; `single` writes the manifest and every table into one JSON file
  --no-timestamp     leave the export time out of the manifest, so a package can be compared
";

/// What one run of the program does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Arguments name no command: open the editor, which is what a double click does.
    Window,
    /// Print this text and exit.
    Help(&'static str),
    /// Write an order package and exit.
    Export(ExportArgs),
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
    /// The scene file cannot be read or parsed.
    Scene(SceneFileError),
    /// The default catalogue a new session starts from cannot be built.
    Catalog(String),
    /// The package cannot be built or written.
    Export(ExportError),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) => formatter.write_str(message),
            Self::Scene(error) => error.fmt(formatter),
            Self::Catalog(message) => formatter.write_str(message),
            Self::Export(error) => error.fmt(formatter),
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
        Some(other) if other.starts_with("-psn_") => Ok(Command::Window),
        Some(other) => Err(CliError::Usage(format!("`{other}` is not a command; run `z3l --help`"))),
    }
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
        Command::Help(text) => Some(print_stdout(text)),
        Command::Export(args) => Some(match export(args) {
            Ok(lines) => print_stdout(&(lines.join("\n") + "\n")),
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
        for flag in ["--out", "--format", "--no-timestamp", "bundle|single"] {
            assert!(USAGE.contains(flag), "the help names `{flag}`: {USAGE}");
        }
        assert_eq!(run(&Command::Window), None, "only the window command asks the caller to open one");
    }

    fn read_text(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_str(&read_text(path)).unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()))
    }
}
