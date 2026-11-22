//! The decision index: `<decisions>/README.md` must list every record once and nothing else.
//!
//! The index is how a session finds a decision it did not know it needed; a record that is not in the
//! table is invisible, a row that points at a missing file sends the reader nowhere, and two records
//! sharing one number make every "ADR NNNN" in the tree ambiguous — a decision is cited by its number,
//! so a number names exactly one record. All three are the same rot the plan gate rejects for
//! `plan/map.md`, so they are checked the same way.

use std::path::Path;

/// The number of decisions the index and the directory agree on, or every mismatch.
pub fn check(root: &Path) -> Result<usize, Vec<String>> {
    check_in(root, Path::new("docs/decisions"))
}

/// [`check`] with the decisions directory named by the caller.
pub fn check_in(root: &Path, relative: &Path) -> Result<usize, Vec<String>> {
    let (count, unlisted) = scan(root, relative, false)?;
    debug_assert!(unlisted.is_empty(), "a strict scan reports the unlisted records as violations");
    Ok(count)
}

/// [`check_in`] from a checkout beside the plan's own: a record the primary has not indexed yet is a
/// row that checkout adds after the merge, so it is counted and named instead of refused.
///
/// The index is one table in one file, so `README.md` has one writer — the primary checkout, like
/// every other shared file (P26). A stream that writes a decision on its own branch cannot add the
/// row without meeting `dev` in the same hunk, and refusing the record would leave the stream unable
/// to run the gate at all; it names the rows waiting instead, and the primary's own run is strict.
pub fn check_in_stream(root: &Path, relative: &Path) -> Result<(usize, Vec<String>), Vec<String>> {
    scan(root, relative, true)
}

/// The count of records and the ones the index does not list; `pending_ok` turns the second into a
/// sentence instead of a value.
fn scan(root: &Path, relative: &Path, pending_ok: bool) -> Result<(usize, Vec<String>), Vec<String>> {
    let directory = root.join(relative);
    let shown = relative.display();
    let entries = std::fs::read_dir(&directory).map_err(|error| vec![format!("cannot read `{shown}/`: {error}")])?;
    // The file name travels beside its number, so a mismatch or a clash names the record it means.
    let mut files: Vec<(String, String)> = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            number(&name).map(|number| (number, name))
        })
        .collect();
    files.sort();
    let index = std::fs::read_to_string(directory.join("README.md"))
        .map_err(|error| vec![format!("cannot read `{shown}/README.md`: {error}")])?;
    let listed = rows(&index);
    let mut violations = Vec::new();
    for (number, names) in shared(&files) {
        violations.push(format!(
            "`{shown}/` has {} records numbered {number}, so a reference to it names no one record: {}",
            names.len(),
            names.join(", ")
        ));
    }
    for (number, targets) in shared(&listed) {
        let times = if targets.len() == 2 { "twice".to_owned() } else { format!("{} times", targets.len()) };
        violations.push(format!(
            "`{shown}/README.md` lists {number} {times}, so a reference to it names no one record: {}",
            targets.join(", ")
        ));
    }
    let mut unlisted: Vec<String> = Vec::new();
    for (number, name) in &files {
        if !listed.iter().any(|(listed, _)| listed == number) {
            if pending_ok {
                unlisted.push(name.clone());
            } else {
                violations.push(format!("`{shown}/` has {name}, but `README.md` has no row for it"));
            }
        }
    }
    let mut reported: Vec<&str> = Vec::new();
    for (number, _) in &listed {
        if !files.iter().any(|(file, _)| file == number) && !reported.contains(&number.as_str()) {
            reported.push(number);
            violations.push(format!("`{shown}/README.md` lists {number}, but no file carries that number"));
        }
    }
    if violations.is_empty() { Ok((files.len(), unlisted)) } else { Err(violations) }
}

/// The number and the link target of every index row, in the order the table carries them.
fn rows(index: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for (offset, _) in index.match_indices("](") {
        let rest = index.get(offset.saturating_add(2)..).unwrap_or_default();
        let target = rest.split([')', ' ']).next().unwrap_or_default();
        if let Some(number) = number(target) {
            rows.push((number, target.to_owned()));
        }
    }
    rows
}

/// The numbers more than one of the `(number, name)` pairs carries, with the names carrying each.
///
/// A decision is cited by its number alone, so two files — or two rows — under one number leave a
/// reader unable to say which record was meant; the gate names both, and the record that keeps the
/// number is the one every reference already pointed at.
fn shared(pairs: &[(String, String)]) -> Vec<(String, Vec<String>)> {
    let mut numbers: Vec<&str> = pairs.iter().map(|(number, _)| number.as_str()).collect();
    numbers.sort_unstable();
    numbers.dedup();
    let mut shared = Vec::new();
    for number in numbers {
        let names: Vec<String> =
            pairs.iter().filter(|(carried, _)| carried == number).map(|(_, name)| name.clone()).collect();
        if names.len() > 1 {
            shared.push((number.to_owned(), names));
        }
    }
    shared
}

/// The four-digit number a decision file or a link starts with.
fn number(name: &str) -> Option<String> {
    let digits: String = name.chars().take(4).collect();
    let rest = name.get(4..)?;
    (digits.len() == 4 && digits.chars().all(|ch| ch.is_ascii_digit()) && rest.starts_with('-')).then_some(digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch project with the given decision files and index rows.
    fn project(name: &str, files: &[&str], rows: &[&str]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("xtask-docs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("docs/decisions")).expect("the scratch directory is writable");
        for file in files {
            std::fs::write(root.join("docs/decisions").join(file), "# a decision\n").expect("the file is writable");
        }
        let mut index = String::from("# Architecture decisions\n\n| # | Title | Status |\n|---|---|---|\n");
        for row in rows {
            index.push_str(&format!("| [{row}]({row}-a-decision.md) | a decision | Accepted |\n"));
        }
        std::fs::write(root.join("docs/decisions/README.md"), index).expect("the index is writable");
        root
    }

    #[test]
    fn an_index_that_matches_passes() {
        let root = project("ok", &["0001-a-decision.md", "0002-a-decision.md"], &["0001", "0002"]);
        assert_eq!(check(&root).expect("the index is complete"), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_record_without_a_row_and_a_row_without_a_record_are_both_reported() {
        let root = project("mismatch", &["0001-a-decision.md", "0007-a-decision.md"], &["0001", "0009"]);
        let violations = check(&root).expect_err("the index disagrees with the directory");
        assert!(violations.iter().any(|v| v.contains("0007") && v.contains("no row")), "{violations:?}");
        assert!(violations.iter().any(|v| v.contains("0009") && v.contains("no file")), "{violations:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn two_records_under_one_number_are_both_named() {
        let root = project("clash", &["0180-a-decision.md", "0180-another-decision.md"], &["0180"]);
        let violations = check(&root).expect_err("one number names one record");
        assert!(
            violations.iter().any(|v| v.contains("0180")
                && v.contains("0180-a-decision.md")
                && v.contains("0180-another-decision.md")),
            "{violations:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_number_listed_twice_is_reported() {
        let root = project("twice", &["0180-a-decision.md"], &["0180", "0180"]);
        let violations = check(&root).expect_err("a number is listed once");
        assert!(violations.iter().any(|v| v.contains("0180") && v.contains("twice")), "{violations:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stream_checkout_waits_for_the_primary_to_index_a_record() {
        let root = project("stream", &["0001-a-decision.md", "0002-a-decision.md"], &["0001"]);
        let (count, unlisted) =
            check_in_stream(&root, Path::new("docs/decisions")).expect("the record is a row to come");
        assert_eq!(count, 2, "the record is counted");
        assert_eq!(unlisted, vec!["0002-a-decision.md".to_owned()], "the missing row is named");
        // A row that points at no file stays a violation: the stream did not add that row.
        std::fs::write(root.join("docs/decisions/README.md"), "| [0003](0003-nothing.md) | a decision | Accepted |\n")
            .expect("the index is writable");
        check_in_stream(&root, Path::new("docs/decisions")).expect_err("a row with no record is still rot");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_directory_or_index_is_an_error() {
        let root = std::env::temp_dir().join(format!("xtask-docs-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the scratch directory is writable");
        assert!(check(&root).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
