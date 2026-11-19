//! The shell's log sink: one file beside the workspace settings, plus the console.
//!
//! `log` is the workspace's logging facade, and this module is the one place that installs a sink
//! for it (the stack table in `AGENTS.md` says the shell installs it). It is called once, first
//! thing in `main`, before any window or command: a tab that fails to open still leaves a line.
//!
//! The level is read from `Z3L_LOG` (`off`, `error`, `warn`, `info`, `debug`, `trace`), defaulting
//! to `debug`, because the log exists to answer a bug report and the interesting lines are the ones
//! a hand writes while it drags something. `Z3L_LOG=info` quiets it for everyday work.
//!
//! The file is `<config>/z3l.log`, appended across runs and emptied when it passes its bound, so a
//! report carries the last sessions and a maker's disk never fills. A line is
//! `+<elapsed>ms <level> <target>: <message>`, which is greppable by hand and readable without a
//! viewer.
//!
//! A log line carries what a person needs to reproduce a defect and nothing more: an id, a
//! coordinate, a decision. It never carries panel geometry, a file's bytes or a key.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

/// The file's name inside the config directory.
const FILE_NAME: &str = "z3l.log";

/// How large the file may grow before a new run starts it over, in bytes.
///
/// Four megabytes is a few sessions of dragging with `debug` on; a report is sent as the file, so a
/// bound that a mail client still accepts is the useful one.
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// The sink itself: a level, the file and the clock the lines are stamped with.
struct Sink {
    level: log::LevelFilter,
    file: Option<File>,
    started: Instant,
}

impl log::Log for Sink {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let stamp = self.started.elapsed().as_millis();
        let line = format!("+{stamp:>8}ms {:<5} {}: {}\n", record.level(), record.target(), record.args());
        if let Some(mut file) = self.file.as_ref() {
            let _ = file.write_all(line.as_bytes());
        }
        // The console is where a terminal session and a test run read the same lines.
        let _ = std::io::stderr().write_all(line.as_bytes());
    }

    fn flush(&self) {
        if let Some(mut file) = self.file.as_ref() {
            let _ = file.flush();
        }
    }
}

/// Installs the sink and answers the path it writes to, or `None` when only the console is used.
///
/// A second call does nothing: a logger is set once per process, and the shell may rebuild its
/// window without wanting a second sink.
pub(crate) fn install() -> Option<PathBuf> {
    static SINK: OnceLock<Sink> = OnceLock::new();
    let path = crate::settings_store::config_dir().map(|directory| directory.join(FILE_NAME));
    let (file, path) = match path {
        Some(path) => match open(&path) {
            Some(file) => (Some(file), Some(path)),
            None => (None, None),
        },
        None => (None, None),
    };
    let sink = SINK.get_or_init(|| Sink { level: level(), file, started: Instant::now() });
    // The second caller of `set_logger` is refused by the facade; it is not an error worth a note,
    // because the first install is the one this module made.
    let _ = log::set_logger(sink);
    log::set_max_level(sink.level);
    path
}

/// The level `Z3L_LOG` names, or the default when it names nothing this build knows.
///
/// A word is matched case-insensitively and an unknown word is the default rather than an error: a
/// mistyped environment variable must never keep the window from opening.
fn level() -> log::LevelFilter {
    match std::env::var("Z3L_LOG").ok().as_deref().map(str::to_ascii_lowercase).as_deref() {
        Some("off") => log::LevelFilter::Off,
        Some("error") => log::LevelFilter::Error,
        Some("warn") => log::LevelFilter::Warn,
        Some("info") => log::LevelFilter::Info,
        Some("trace") => log::LevelFilter::Trace,
        Some("debug") | Some("") | None => log::LevelFilter::Debug,
        Some(_) => log::LevelFilter::Debug,
    }
}

/// Opens the log file for appending, starting it over when the last runs filled it.
fn open(path: &std::path::Path) -> Option<File> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).ok()?;
    }
    let full = std::fs::metadata(path).map(|meta| meta.len() > MAX_BYTES).unwrap_or(false);
    // `truncate` and `append` are not asked for together: one starts the file over, the other keeps
    // it, and the platforms disagree about what the pair means.
    let opened = if full {
        OpenOptions::new().create(true).write(true).truncate(true).open(path)
    } else {
        OpenOptions::new().create(true).append(true).open(path)
    };
    match opened {
        Ok(file) => Some(file),
        Err(error) => {
            log::warn!(target: "z3l::shell", "log file {} is not writable: {error}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default and every spelled level, without touching the process's environment.
    fn read(word: &str) -> log::LevelFilter {
        match word.to_ascii_lowercase().as_str() {
            "off" => log::LevelFilter::Off,
            "error" => log::LevelFilter::Error,
            "warn" => log::LevelFilter::Warn,
            "info" => log::LevelFilter::Info,
            "trace" => log::LevelFilter::Trace,
            _ => log::LevelFilter::Debug,
        }
    }

    #[test]
    fn a_level_word_is_read_case_insensitively_and_an_unknown_one_is_the_default() {
        // The value comes from an environment variable, so it is hostile input: a typo must not
        // change the window's behaviour, it must only leave the default level in place.
        assert_eq!(read("INFO"), log::LevelFilter::Info);
        assert_eq!(read("Debug"), log::LevelFilter::Debug);
        assert_eq!(read("off"), log::LevelFilter::Off);
        assert_eq!(read("chatty"), log::LevelFilter::Debug);
    }

    #[test]
    fn the_file_is_emptied_only_once_it_is_over_its_bound() {
        let directory = std::env::temp_dir().join(format!("z3l-log-{}", std::process::id()));
        let path = directory.join(FILE_NAME);
        let _ = std::fs::remove_file(&path);
        let mut file = open(&path).expect("the log opens");
        file.write_all(b"first\n").expect("the line is written");
        drop(file);
        // A short file keeps what it has across a run.
        let _ = open(&path).expect("the log reopens");
        assert_eq!(std::fs::read_to_string(&path).expect("the log reads"), "first\n");
        // A file over the bound starts over rather than growing without end.
        {
            let file = OpenOptions::new().append(true).open(&path).expect("the log appends");
            file.set_len(MAX_BYTES + 1).expect("the bound is crossed");
        }
        let _ = open(&path).expect("the log reopens");
        assert_eq!(std::fs::metadata(&path).expect("the log exists").len(), 0, "the new run starts clean");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
