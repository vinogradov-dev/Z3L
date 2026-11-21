//! The rules: every check that compares the three files against each other.
//!
//! Everything here is pure: text in, violations out. The filesystem and git live in [`crate::check`].

use crate::parse::{
    STATE_PREFIX, backlog_entries, parse_backlog, parse_map, parse_session, parse_state, parse_task_line, session_field,
};
use crate::streams::Streams;
use crate::{NO_TASK, NOTHING_SCHEDULED, StreamState, Summary, Task, TaskStatus};

/// Longest accepted `plan/STATUS.md`, in lines.
pub const MAX_STATUS_LINES: usize = 70;
/// Longest accepted session note, in lines: one budget per stream, because each note is one screen.
pub const MAX_SESSION_LINES: usize = 50;
/// The section a session starts from.
const NOW_SECTION: &str = "Now";
/// The section that holds finished tasks.
const DONE_SECTION: &str = "Done";
/// The one place development state may live.
pub const README_RULE: &str = "README must not carry development status; it lives in plan/STATUS.md";

/// Checks the plan files against each other, without touching the filesystem or git.
///
/// `sessions` is every session note the plan holds, as `(path, text)`: one per stream at most, and
/// the path is what tells a note from the stream it belongs to. A plan that declares no stream has
/// one, `plan/SESSION.md`, and the rules are the ones it always had.
pub fn analyze(status: &str, backlog: &str, sessions: &[(String, String)]) -> Result<Summary, Vec<String>> {
    let state = parse_state(status)?;
    let streams = match Streams::parse(status) {
        Ok(streams) => streams,
        Err(errors) => {
            // The declaration is unusable, so the rules that read it run over the plan's own single
            // stream; every sentence about the declaration is already in the list.
            let tasks = parse_backlog(backlog);
            let mut violations = errors;
            violations.extend(parse_violations(status, backlog, &tasks));
            violations.extend(unparsed_task_lines(backlog));
            violations.extend(legend_violations(backlog, &tasks));
            return Err(violations);
        }
    };
    let tasks = parse_backlog(backlog);
    let mut violations = parse_violations(status, backlog, &tasks);
    violations.extend(budget_violations(status, sessions));
    violations.extend(unparsed_task_lines(backlog));
    violations.extend(legend_violations(backlog, &tasks));

    if let Some(violation) = updated_format_violation(&state.updated) {
        violations.push(violation);
    }
    if let Some(violation) = next_violation(status, state.current()) {
        violations.push(violation);
    }

    violations.extend(note_violations(&streams, sessions, &tasks));
    violations.extend(primary_violations(&streams, &tasks, &state));

    if violations.is_empty() {
        let streams = stream_states(&streams, sessions, &tasks);
        Ok(Summary { state, tasks, streams, head: None, interrupted: false })
    } else {
        Err(violations)
    }
}

/// The streams as the plan holds them: which task each one works, and the note it keeps.
pub(crate) fn stream_states(streams: &Streams, sessions: &[(String, String)], tasks: &[Task]) -> Vec<StreamState> {
    streams
        .iter()
        .map(|stream| {
            let note_path = stream.note_path();
            StreamState {
                key: stream.key.clone(),
                primary: stream.is_primary(),
                live: live_task(streams, tasks, stream),
                session: sessions.iter().find(|(path, _)| *path == note_path).map(|(_, text)| parse_session(text)),
                note_path,
            }
        })
        .collect()
}

/// The task a stream has marked in progress, in file order.
fn live_task(streams: &Streams, tasks: &[Task], stream: &crate::Stream) -> Option<String> {
    tasks
        .iter()
        .find(|task| task.status == TaskStatus::Doing && streams.owner(&task.id).key == stream.key)
        .map(|task| task.id.clone())
}

/// One stream works one task, and the note beside it is about that task.
///
/// Four rules, and every one of them is the same rule the plan always had, read per stream instead of
/// per plan: a `[~]` marker lives in `## Now`, no stream holds two of them, a stream that works a task
/// keeps its note and no stream keeps a note it is not working under. A note whose path names no
/// stream is the fifth: a renamed stream leaves its note behind, and the file then says what nothing
/// else does.
fn note_violations(streams: &Streams, sessions: &[(String, String)], tasks: &[Task]) -> Vec<String> {
    let mut violations = Vec::new();
    for task in tasks.iter().filter(|task| task.status == TaskStatus::Doing) {
        if task.section != NOW_SECTION {
            violations
                .push(format!("`[~]` task {} must sit in `## {NOW_SECTION}`, not `## {}`", task.id, task.section));
        }
    }
    for stream in streams.iter() {
        let doing: Vec<&Task> = tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Doing && streams.owner(&task.id).key == stream.key)
            .collect();
        if doing.len() > 1 {
            let ids: Vec<&str> = doing.iter().map(|task| task.id.as_str()).collect();
            violations.push(format!(
                "the `{}` stream marks {} in progress at once; a stream works one task at a time",
                stream.key,
                ids.join(", ")
            ));
        }
        let path = stream.note_path();
        let note = sessions.iter().find(|(note, _)| *note == path).map(|(_, text)| text.as_str());
        match (doing.first(), note) {
            (Some(task), Some(note)) => {
                match session_field(note, "task") {
                    Some(found) if found == task.id => {}
                    Some(found) => violations.push(format!(
                        "`{path}` is about task {found}, but `{}` works the `{}` stream",
                        task.id, stream.key
                    )),
                    None => violations.push(format!("`{path}` needs a `task: <id>` line")),
                }
                for key in ["goal", "now", "run"] {
                    if session_field(note, key).is_none() {
                        violations.push(format!("`{path}` needs a non-empty `{key}:` line"));
                    }
                }
            }
            (Some(task), None) => violations.push(if stream.is_primary() {
                format!(
                    "`STATE: state=doing` needs a `{path}`; copy `plan/SESSION.template.md` and start {} there",
                    task.id
                )
            } else {
                format!(
                    "the `{}` stream works {} and needs its `{path}`; `cargo xtask task start {}` writes it",
                    stream.key, task.id, task.id
                )
            }),
            (None, Some(_)) if stream.is_primary() => {
                violations.push(format!("`STATE: state=idle` must not leave a `{path}` behind"))
            }
            (None, Some(_)) => violations.push(format!(
                "`{path}` is a session note no stream is working under; `{}` has no `[~]` task",
                stream.key
            )),
            (None, None) => {}
        }
    }
    for (path, _) in sessions {
        if streams.stream_at(path).is_none() {
            let declared = streams.notes();
            violations
                .push(format!("`{path}` is a session note no stream declares; the notes are {}", declared.join(", ")));
        }
    }
    violations
}

/// The rules `STATE:` carries, which speak about the plan's own stream and not about the others.
///
/// `state=` and `task=` are the primary stream's: its task in progress, or the head of its work
/// order. A stream beside it keeps its `[~]` and its note, and neither moves the state line — which
/// is what lets several checkouts work one backlog without several writers of one line.
fn primary_violations(streams: &Streams, tasks: &[Task], state: &crate::State) -> Vec<String> {
    let mut violations = Vec::new();
    let primary = streams.primary();
    let live = live_task(streams, tasks, primary);
    let head = first_unfinished(streams, tasks);

    // An empty work order is a legal state of the plan: `task=-` says so, and it is allowed only
    // while `## Now` holds nothing unfinished of the plan's own stream. The moment one is in the
    // order, the line names it: another stream's entry is its own business and never moves it.
    match state.current() {
        None => {
            if let Some(first) = head {
                violations.push(format!(
                    "`STATE: task={NO_TASK}` but `## {NOW_SECTION}` still holds {}; name it or defer it",
                    first.id
                ));
            }
        }
        Some(id) => match tasks.iter().find(|task| task.id == id) {
            Some(task) if task.status == TaskStatus::Done => {
                violations.push(format!("backlog task {id} is done, but `STATE: task={id}` names it"));
            }
            Some(task) => {
                if task.section != NOW_SECTION {
                    violations.push(format!(
                        "`STATE: task={id}` sits in `## {}`; the current task must be in `## {NOW_SECTION}`",
                        task.section
                    ));
                } else if streams.owner(&task.id).key != primary.key {
                    violations.push(format!(
                        "`STATE: task={id}` belongs to the `{}` stream; the state line names the `{}` stream's \
                         task, and `{}` starts with `cargo xtask task start {id}`",
                        streams.of(task).key,
                        primary.key,
                        primary.key
                    ));
                } else if let Some(first) = head
                    && first.id != task.id
                {
                    violations.push(format!(
                        "`STATE: task={id}` is not the first unfinished task in `## {NOW_SECTION}`; `{}` comes before it",
                        first.id
                    ));
                }
            }
            None => violations.push(format!("`STATE: task={id}` is not a task in `plan/BACKLOG.md`")),
        },
    }

    match state.status.as_str() {
        "doing" => {
            if state.current().is_none() {
                violations.push(format!(
                    "`STATE: state=doing` needs a task; `task={NO_TASK}` is the empty order, which has nothing \
                     to work on"
                ));
            }
            if live.is_none() {
                violations.push(format!(
                    "`STATE: state=doing` needs exactly one `[~]` task in the `{}` stream, the backlog has none",
                    primary.key
                ));
            }
            if let Some(working) = &live
                && *working != state.task
            {
                violations
                    .push(format!("`STATE: task={}` does not match the `[~]` backlog task {working}", state.task));
            }
        }
        "idle" => {
            if let Some(working) = &live {
                violations.push(format!(
                    "`STATE: state=idle` but the backlog marks {working} still in progress; set `[~]` back to \
                     `[ ]` or start the task"
                ));
            }
        }
        other => violations.push(format!("`STATE: state={other}` must be `idle` or `doing`")),
    }
    violations
}

/// The `## Next` paragraph of `plan/STATUS.md` must name the task `STATE:` does: it is the first
/// prose a fresh session reads, and a paragraph about another task sends it to the wrong place.
///
/// The check is silent when the section is absent, because a minimal plan may not have one.
pub(crate) fn next_violation(status: &str, task: Option<&str>) -> Option<String> {
    let mut in_next = false;
    let mut paragraph = String::new();
    for line in status.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            in_next = trimmed.get(3..).is_some_and(|heading| heading.trim().eq_ignore_ascii_case("next"));
            if !in_next && !paragraph.is_empty() {
                break;
            }
            continue;
        }
        if in_next {
            paragraph.push(' ');
            paragraph.push_str(trimmed);
        }
    }
    let Some(task) = task else {
        // No task is current, so the paragraph says that instead of naming one; a paragraph about a
        // parked task would send a fresh session to work that is not in the order.
        if paragraph.trim().starts_with(crate::NOTHING_SCHEDULED) {
            return None;
        }
        return Some(format!(
            "`plan/STATUS.md` › `## Next` must open with `{NOTHING_SCHEDULED}` while `STATE: task={NO_TASK}`"
        ));
    };
    if paragraph.trim().is_empty() {
        return None;
    }
    match first_id(&paragraph) {
        Some(found) if found == task => None,
        Some(found) => {
            Some(format!("`plan/STATUS.md` › `## Next` names {found}, but `STATE: task={task}` is the current task"))
        }
        None => Some(format!("`plan/STATUS.md` › `## Next` must name the current task {task}")),
    }
}

/// The first task id in `text`: capital letters followed by digits, as the backlog spells them.
fn first_id(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let letter = bytes[index].is_ascii_uppercase();
        let boundary = index == 0 || !bytes[index - 1].is_ascii_alphanumeric();
        if letter && boundary {
            let mut end = index;
            while bytes.get(end).is_some_and(u8::is_ascii_uppercase) {
                end += 1;
            }
            let digits = end;
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            let closed = bytes.get(end).is_none_or(|next| !next.is_ascii_alphanumeric());
            if end > digits && closed {
                return text.get(index..end);
            }
        }
        index += 1;
    }
    None
}

/// Every `plan/map.md` row must name paths that exist: a stale row sends the reader to a file that
/// is gone, which is worse than no row at all.
///
/// A token with a source extension is a file; a token without one is a directory. A token that does
/// not contain a `/` is prose or an anchor, never a path.
pub(crate) fn map_violations<F: Fn(&str) -> bool>(map: &str, exists: F) -> Vec<String> {
    let mut violations = Vec::new();
    for row in parse_map(map) {
        for path in row.paths() {
            if !path.contains('/') || exists(&path) {
                continue;
            }
            violations.push(format!(
                "`plan/map.md` row `{}` names `{path}`, which does not exist; fix the row or add the file",
                row.feature
            ));
        }
    }
    violations
}

/// Every tag a `plan/debt.md` entry carries must be declared in the file's `Tags:` line, so a typo
/// cannot silently drop an entry out of the task report's selection.
pub(crate) fn debt_violations(debt: &str) -> Vec<String> {
    let declared: Vec<String> =
        debt.lines().take_while(|line| !line.trim_start().starts_with("- ")).flat_map(bracketed).collect();
    let used: Vec<String> =
        debt.lines().filter_map(|line| line.trim_start().strip_prefix("- ")).flat_map(bracketed).collect();
    if used.is_empty() {
        return Vec::new();
    }
    if declared.is_empty() {
        return vec!["`plan/debt.md` uses tags but has no `Tags:` line that declares them".to_owned()];
    }
    used.into_iter()
        .filter(|tag| !declared.contains(tag))
        .map(|tag| format!("`plan/debt.md` uses `[{tag}]`, which the `Tags:` line does not declare"))
        .collect()
}

/// The bracketed tags of one line: `[engine]` reads as `engine`.
fn bracketed(line: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('[') {
        let after = rest.get(start.saturating_add(1)..).unwrap_or_default();
        let Some(end) = after.find(']') else { break };
        let tag = after.get(..end).unwrap_or_default();
        if !tag.is_empty() && tag.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
            tags.push(tag.to_owned());
        }
        rest = after.get(end.saturating_add(1)..).unwrap_or_default();
    }
    tags
}

/// A closed task's ADR citation must name a decision the project has: a closed entry is the record
/// of what landed, and an "ADR 0081" nobody can open is a record of a promise, not of a decision.
///
/// An open task may reserve a number it will write — the report prints it as missing, and the gate
/// leaves it alone until the task closes.
pub(crate) fn adr_cite_violations(tasks: &[Task], decisions: &[String]) -> Vec<String> {
    let mut violations = Vec::new();
    for task in tasks.iter().filter(|task| task.status == TaskStatus::Done) {
        for number in crate::parse::adr_numbers(&task.text) {
            if decisions.contains(&number) {
                continue;
            }
            violations.push(format!(
                "backlog task {} is closed but cites ADR {number}, which `docs/decisions/` does not hold",
                task.id
            ));
        }
    }
    violations
}

/// The reference files carry no plan state: a `STATE:` line or a task marker in `plan/debt.md` or
/// `plan/questions.md` is a second source of truth for the same fact, and a second one is the stale
/// one. An empty entry is the same rot in another shape: it reads as a decision nobody wrote down.
pub(crate) fn reference_violations(name: &str, text: &str) -> Vec<String> {
    let mut violations = Vec::new();
    if let Some(line) = text.lines().find(|line| line.trim_start().starts_with(STATE_PREFIX)) {
        violations.push(format!("`{name}` carries `{}`; the state lives in `plan/STATUS.md`", line.trim()));
    }
    for line in text.lines() {
        let Some(entry) = line.trim_start().strip_prefix("- ") else { continue };
        // A marker is written in inline code, `` `[ ]` ``, as the backlog spells it.
        let entry = entry.trim_start().trim_start_matches('`').trim_start();
        if let Some(marker) = ["[ ]", "[~]", "[x]"].iter().find(|marker| entry.starts_with(*marker)) {
            violations.push(format!(
                "`{name}` marks an entry `{marker}`; a reference file carries no status — that is `plan/BACKLOG.md`"
            ));
        }
        if entry.is_empty() {
            violations.push(format!("`{name}` has an empty entry"));
        }
    }
    violations
}

/// The first task in `## Now` that is not finished, in file order.
///
/// Order is the order of work, so this is the task the plan is actually on: `STATE: task=` may name
/// it, not any later one. `## Bugs`, `## Next` and `## Done` are outside the work order.
/// The first unfinished task of the plan's own stream in `## Now`, which is the task `STATE:` must
/// name: another stream's entry sits in the same order and is not this checkout's head.
fn first_unfinished<'a>(streams: &Streams, tasks: &'a [Task]) -> Option<&'a Task> {
    tasks.iter().find(|task| {
        task.section == NOW_SECTION
            && task.status != TaskStatus::Done
            && streams.owner(&task.id).key == streams.primary().key
    })
}

/// A `README.md` that carries a status line or a milestone is a second source of truth, and the
/// stale one. `plan/STATUS.md` is the only place development state lives.
pub fn readme_violations(readme: &str) -> Vec<String> {
    let mut violations = Vec::new();
    if readme.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("**Status:**") || line.starts_with("Status:")
    }) {
        violations.push(README_RULE.to_owned());
    }
    if let Some(token) = milestone(readme) {
        violations.push(format!("{README_RULE}: it names the milestone `{token}`"));
    }
    violations
}

/// The first `M<digit>` milestone token in `text`, ignoring the inside of a longer word.
fn milestone(text: &str) -> Option<&str> {
    text.match_indices('M').find_map(|(offset, _)| {
        let rest = &text[offset + 1..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let preceded_by_word = text[..offset].chars().next_back().is_some_and(|ch| ch.is_alphanumeric());
        (digits > 0 && !preceded_by_word).then(|| &text[offset..offset + 1 + digits])
    })
}

/// A plan file over its line budget is an error: an ignored warning is how the plan rots.
///
/// The message names the width as well as the count, because the count alone is what cost a session
/// seven minutes: it printed the result (a candidate of 94 lines against a budget of 70) and not the
/// cause (a section body re-wraps at [`crate::edit::PROSE_WIDTH`], so every attempt grew the file).
fn budget_violations(status: &str, sessions: &[(String, String)]) -> Vec<String> {
    let mut violations = Vec::new();
    let mut over = |name: &str, text: &str, max: usize| {
        let lines = text.lines().count();
        if lines > max {
            violations.push(format!(
                "{name} has {lines} lines, over the {max}-line budget: a section body re-wraps at {} \
                 columns, so a paragraph longer than the one it replaces grows the file",
                crate::edit::PROSE_WIDTH
            ));
        }
    };
    over("plan/STATUS.md", status, MAX_STATUS_LINES);
    for (path, text) in sessions {
        // Each stream's note is one screen of its own: the budget counts per file, not per plan.
        over(path, text, MAX_SESSION_LINES);
    }
    violations
}

/// Task-looking lines that did not parse, so a malformed line cannot silently drop a task.
fn unparsed_task_lines(backlog: &str) -> Vec<String> {
    backlog_entries(backlog)
        .into_iter()
        .filter(|entry| parse_task_line(&entry.text).is_none())
        .map(|entry| format!("`plan/BACKLOG.md` has a task line without a `**bold**` id: `{}`", entry.text))
        .collect()
}

/// The phase legend: every id prefix must be a letter the legend explains.
///
/// A plan without a legend is not a violation — a project may not name its phases — but once the
/// paragraph exists it is the vocabulary: a task whose id starts with a letter it does not declare,
/// a clause the writer would not produce, or a letter declared twice means the paragraph and the ids
/// have drifted apart, and the sentence names the verb that fixes it.
fn legend_violations(backlog: &str, tasks: &[Task]) -> Vec<String> {
    let Some(legend) = crate::edit::legend(backlog) else {
        return Vec::new();
    };
    if let Some(problem) = legend.problems.first() {
        // One sentence, not one per line: the paragraph is rewritten whole by the verb that owns it.
        return vec![format!("`plan/BACKLOG.md` {problem}; `cargo xtask task phase` rewrites it")];
    }
    let mut violations = Vec::new();
    let mut reported: Vec<char> = Vec::new();
    for task in tasks {
        let Some(letter) = task.id.chars().next().filter(|letter| letter.is_ascii_uppercase()) else {
            continue;
        };
        if legend.name(letter).is_some() || reported.contains(&letter) {
            continue;
        }
        reported.push(letter);
        violations.push(format!(
            "backlog id {} starts with phase {letter}, which the legend in `plan/BACKLOG.md` does not declare; \
             `cargo xtask task phase {letter} --name \"…\"`",
            task.id
        ));
    }
    violations
}

/// The `base=` violation for the facts gathered from git.
///
/// `HEAD` is a legal `base`: the plan edit is committed on top of it while it is still uncommitted.
pub(crate) fn base_violation(base: &str, head: Option<&str>, exists: bool, is_ancestor: bool) -> Option<String> {
    if !exists {
        return Some(format!("`STATE: base={base}` is not a commit in this repository"));
    }
    let Some(head) = head else {
        return Some("cannot resolve `HEAD`: the plan check needs a git checkout".to_owned());
    };
    (!is_ancestor).then(|| {
        format!(
            "`STATE: base={base}` is not an ancestor of `HEAD` ({head}); point `base` at a commit the plan describes"
        )
    })
}

/// The `updated=` violation for a value that is not a calendar date in `YYYY-MM-DD` form.
///
/// The date is a promise the plan makes to its reader: it says when the state was written, so a
/// typo or a free-form word hides how stale the plan is. The calendar is checked too — `2026-02-30`
/// parses as three numbers but is not a day.
pub(crate) fn updated_format_violation(updated: &str) -> Option<String> {
    if calendar_date(updated).is_some() {
        return None;
    }
    Some(format!("`STATE: updated={updated}` is not a date; write it as `YYYY-MM-DD`, for example `2026-11-04`"))
}

/// The `updated=` violation for a date that is earlier than the `base` commit was made.
///
/// The plan describes the tree at `base`, so its own date cannot run behind that commit: an
/// `updated` from before the code landed would claim a state older than the code it describes.
pub(crate) fn updated_base_violation(updated: &str, base: &str, committed: Option<&str>) -> Option<String> {
    let updated_date = calendar_date(updated)?;
    let committed_date = calendar_date(committed?)?;
    (updated_date < committed_date).then(|| {
        format!(
            "`STATE: updated={updated}` is earlier than `base={base}`, committed on {committed_date}; the \
             plan cannot describe a tree from before its own base"
        )
    })
}

/// The text of a `YYYY-MM-DD` calendar date, or `None` when it is not one.
///
/// The input slice is returned as it was read: an ISO date sorts as text, so two dates compare
/// without any date arithmetic. The calendar is checked as well — `2026-02-30` has the right shape
/// but is not a day.
pub(crate) fn calendar_date(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 {
        return None;
    }
    let shaped = bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| if index == 4 || index == 7 { *byte == b'-' } else { byte.is_ascii_digit() });
    if !shaped {
        return None;
    }
    let year = number(text.get(..4)?)?;
    let month = number(text.get(5..7)?)?;
    let day = number(text.get(8..)?)?;
    if year == 0 || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some(text)
}

/// A decimal field of a date, or `None` when it holds anything but digits.
fn number(field: &str) -> Option<u32> {
    if field.is_empty() || !field.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    field.parse().ok()
}

/// How many days the month has, February following the Gregorian leap-year rule.
fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => 29,
        _ => 28,
    }
}

/// Duplicates, sections and an id that is not a plain label.
fn parse_violations(status: &str, backlog: &str, tasks: &[Task]) -> Vec<String> {
    let mut violations = Vec::new();
    if !backlog.contains("## Now") || !backlog.contains("## Done") {
        violations.push("`plan/BACKLOG.md` needs `## Now` and `## Done` sections".to_owned());
    }
    let mut seen = Vec::new();
    for task in tasks {
        if seen.contains(&task.id) {
            violations.push(format!("backlog id {} appears twice", task.id));
        } else {
            seen.push(task.id.clone());
        }
        if task.status != TaskStatus::Done && task.files.is_none() {
            violations.push(format!("backlog task {} has no `files:` hint; name where the work starts", task.id));
        }
        match (task.status, task.section.as_str()) {
            (TaskStatus::Done, DONE_SECTION) => {}
            (TaskStatus::Done, section) => violations.push(format!(
                "backlog task {} is `[x]` in `## {section}`; a finished task belongs in `## {DONE_SECTION}`",
                task.id
            )),
            (_, DONE_SECTION) => violations.push(format!(
                "`## {DONE_SECTION}` holds {} without `[x]`; move it back to `## {NOW_SECTION}` or mark it done",
                task.id
            )),
            _ => {}
        }
    }
    let mut markers = 0usize;
    let mut state_line = None;
    for line in status.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(STATE_PREFIX) {
            markers += 1;
            state_line = Some(line);
        }
    }
    if markers != 1 {
        violations.push(format!("`plan/STATUS.md` has {markers} `STATE:` lines, expected exactly one"));
    }
    // A repeated heading is a section written twice: the second copy is the one a reader skips.
    let mut headings: Vec<String> = Vec::new();
    for line in status.lines() {
        let Some(name) = line.trim().strip_prefix("## ") else { continue };
        let name = name.trim();
        if headings.iter().any(|seen| seen.eq_ignore_ascii_case(name)) {
            violations.push(format!("`plan/STATUS.md` has two `## {name}` sections"));
        } else {
            headings.push(name.to_owned());
        }
    }
    if let Some(line) = state_line
        && line.trim() != line.trim_start()
    {
        violations.push("`STATE:` must start at the beginning of its line, without indentation".to_owned());
    }
    violations
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    const HEADER: &str = "STATE: milestone=M7 phase=A task=A3 state=idle base=32c659f updated=2026-10-30\n";
    /// A session note a fresh session can resume from.
    const NOTE: &str =
        "task: A3\ngoal: edit arcs in the card\nnow: the arc arm in `object_card.rs`\nrun: cargo test -p ui\n";

    /// The primary stream's note, as `analyze` takes it: one session, no declaration.
    fn note(text: &str) -> Vec<(String, String)> {
        vec![("plan/SESSION.md".to_owned(), text.to_owned())]
    }

    fn backlog() -> &'static str {
        "## Now\n\n\
         - `[ ]` **A3** arcs in the card `files: crates/ui/src/object_card.rs`\n\
         - `[ ]` **A4** stable ids `files: crates/model/src/lib.rs`\n\n\
         ## Done\n\n\
         - `[x]` **A2** ring of segments — `0833b4c`\n"
    }

    /// A plan that declares one stream beside its own: the agent's tasks are `J`.
    fn declared_plan(backlog: &str, notes: &[(&str, &str)]) -> Result<Summary, Vec<String>> {
        let status = "STATE: milestone=M7 phase=A task=A3 state=doing base=32c659f updated=2026-10-30\n\
                      STREAMS: agent=J\n"
            .to_owned();
        let notes: Vec<(String, String)> =
            notes.iter().map(|(path, text)| ((*path).to_owned(), (*text).to_owned())).collect();
        analyze(&status, backlog, &notes)
    }

    /// One `[~]` entry of each stream, each with its own note: the shape the plan now allows.
    fn two_streams() -> &'static str {
        "## Now\n\n\
         - `[~]` **A3** arcs in the card `files: crates/ui/src/object_card.rs`\n\
         - `[~]` **J1** the recipe runner `files: app/src/cli.rs`\n\n\
         ## Done\n"
    }

    const AGENT_NOTE: &str = "task: J1\ngoal: run a recipe\nnow: the line parser\nrun: cargo test -p app\n";

    #[test]
    fn every_declared_stream_works_one_task_and_keeps_its_own_note() {
        let summary = declared_plan(two_streams(), &[("plan/SESSION.md", NOTE), ("plan/SESSION.agent.md", AGENT_NOTE)])
            .expect("two streams, two notes");
        assert_eq!(summary.streams.len(), 2);
        assert_eq!(summary.streams[0].key, "app");
        assert_eq!(summary.streams[0].live.as_deref(), Some("A3"));
        assert_eq!(summary.streams[1].key, "agent");
        assert_eq!(summary.streams[1].live.as_deref(), Some("J1"));
        assert_eq!(summary.streams[1].note_path, "plan/SESSION.agent.md");
        assert_eq!(
            summary.stream_of("J1").and_then(|stream| stream.session.as_ref()).and_then(|note| note.now.as_deref()),
            Some("the line parser")
        );
        assert_eq!(summary.stream_of("A4"), None, "an open task works no stream");
    }

    #[test]
    fn one_stream_may_not_run_two_tasks_at_once() {
        let backlog = two_streams().replace("## Done", "- `[~]` **J2** the report `files: app/src/cli.rs`\n\n## Done");
        let error = declared_plan(&backlog, &[("plan/SESSION.md", NOTE), ("plan/SESSION.agent.md", AGENT_NOTE)])
            .expect_err("two `[~]` tasks of one stream");
        assert!(
            error.iter().any(|violation| violation.contains("the `agent` stream marks J1, J2 in progress")),
            "{error:?}"
        );
    }

    #[test]
    fn a_stream_that_works_needs_its_note_and_a_note_needs_its_stream() {
        let error = declared_plan(two_streams(), &[("plan/SESSION.md", NOTE)])
            .expect_err("the agent stream works without a note");
        assert!(
            error.iter().any(|violation| violation.contains("plan/SESSION.agent.md") && violation.contains("J1")),
            "{error:?}"
        );

        // The other direction: a note that names no declared stream is a file nothing explains.
        let idle = two_streams().replace("- `[~]` **J1**", "- `[ ]` **J1**");
        let error = declared_plan(&idle, &[("plan/SESSION.md", NOTE), ("plan/SESSION.agent.md", AGENT_NOTE)])
            .expect_err("a note without a `[~]` task");
        assert!(
            error.iter().any(|violation| violation.contains("works the `agent` stream")
                || violation.contains("no stream is working under")),
            "{error:?}"
        );

        let undeclared =
            declared_plan(two_streams(), &[("plan/SESSION.md", NOTE), ("plan/SESSION.agent.md", AGENT_NOTE)])
                .expect("the declared shape is valid");
        assert_eq!(undeclared.streams.len(), 2);
        let status = "STATE: milestone=M7 phase=A task=A3 state=doing base=32c659f updated=2026-10-30\n";
        let notes = vec![("plan/SESSION.catalog.md".to_owned(), AGENT_NOTE.to_owned())];
        let error = analyze(status, two_streams(), &notes).expect_err("a note no stream declares");
        assert!(error.iter().any(|violation| violation.contains("plan/SESSION.catalog.md")), "{error:?}");
    }

    #[test]
    fn the_state_line_names_the_plans_own_task_and_not_another_streams() {
        // A declared stream's entry in `## Now` is not the head `STATE:` names.
        let idle = "STATE: milestone=M7 phase=A task=- state=idle base=32c659f updated=2026-10-30\nSTREAMS: agent=J\n\
                    ## Next\n\nNothing is scheduled: the plan's own order is empty.\n"
            .to_owned();
        let backlog = "## Now\n\n- `[ ]` **J1** the recipe runner `files: app/src/cli.rs`\n\n## Done\n";
        let summary = analyze(&idle, backlog, &[]).expect("an empty primary order beside a declared stream");
        assert_eq!(summary.state.current(), None);

        let named = idle.replace("task=-", "task=J1");
        let error = analyze(&named, backlog, &[]).expect_err("the state line may not name the agent's task");
        assert!(error.iter().any(|violation| violation.contains("belongs to the `agent` stream")), "{error:?}");
    }

    #[test]
    fn an_idle_plan_with_a_matching_task_is_consistent() {
        let summary = analyze(HEADER, backlog(), &[]).expect("the plan is consistent");
        assert_eq!(summary.state.task, "A3");
        assert_eq!(summary.tasks.len(), 3);
        assert_eq!(summary.tasks[0].status, TaskStatus::Open);
        assert_eq!(summary.tasks[0].id, "A3");
        assert_eq!(summary.tasks[0].section, "Now");
        assert_eq!(summary.tasks[0].files.as_deref(), Some("crates/ui/src/object_card.rs"));
        assert_eq!(summary.tasks[2].status, TaskStatus::Done);
    }

    /// A backlog whose intro declares its phases, one clause per letter, and holds one task of each.
    fn legend_backlog(clauses: &str, tasks: &str) -> String {
        format!(
            "# Backlog\n\nIntro.\n\n\
             Phases come from the shape-base plan; `cargo xtask task phase` owns this list:\n\
             {clauses}\n\n\
             ## Now\n\n{tasks}\n\
             ## Done\n\n- `[x]` **A2** ring of segments — `0833b4c`\n"
        )
    }

    #[test]
    fn a_legend_that_declares_every_used_letter_is_consistent() {
        let backlog =
            legend_backlog("`A` — contour\n`K` — the order package", "- `[ ]` **K4** the package `files: src/a.rs`");
        let summary = analyze(&HEADER.replace("task=A3", "task=K4"), &backlog, &[]).expect("the plan is consistent");
        assert_eq!(summary.tasks[0].id, "K4");
    }

    #[test]
    fn an_id_in_a_phase_the_legend_does_not_declare_is_reported() {
        let backlog = legend_backlog("`A` — contour", "- `[ ]` **K4** the package `files: src/a.rs`");
        let error = analyze(&HEADER.replace("task=A3", "task=K4"), &backlog, &[]).expect_err("K is undeclared");
        assert!(
            error
                .iter()
                .any(|violation| violation.contains("phase K") && violation.contains("`cargo xtask task phase K")),
            "{error:?}"
        );
    }

    #[test]
    fn a_prose_legend_is_reported_with_the_verb_that_rewrites_it() {
        let backlog =
            legend_backlog("Phases: A contour, K the order package", "- `[ ]` **K4** the package `files: src/a.rs`");
        let error = analyze(&HEADER.replace("task=A3", "task=K4"), &backlog, &[])
            .expect_err("a prose clause line is not the canonical form");
        assert!(
            error.iter().any(|violation| violation.contains("is not a `L` — text clause")
                && violation.contains("cargo xtask task phase")),
            "{error:?}"
        );
    }

    #[test]
    fn a_plan_without_a_legend_names_no_phases_and_is_not_a_violation() {
        // A project that does not declare phases keeps working: only an existing legend is the vocabulary.
        let backlog = legend_backlog("", "- `[ ]` **K4** the package `files: src/a.rs`")
            .replace("Phases come from the shape-base plan; `cargo xtask task phase` owns this list:\n\n", "");
        assert!(!backlog.contains("Phases come from"), "{backlog}");
        analyze(&HEADER.replace("task=A3", "task=K4"), &backlog, &[]).expect("no legend, no phase rule");
    }

    #[test]
    fn a_doing_plan_needs_a_session_note_and_one_marked_task() {
        let doing = HEADER.replace("state=idle", "state=doing");
        let marked = backlog().replace("- `[ ]` **A3**", "- `[~]` **A3**");
        assert!(
            analyze(&doing, &marked, &[])
                .expect_err("a doing task without a note must fail")
                .iter()
                .any(|violation| violation.contains("needs a `plan/SESSION.md`")),
            "a `[~]` task without its note is refused"
        );
        assert!(
            analyze(&doing, backlog(), &[])
                .expect_err("a doing state without a `[~]` marker must fail")
                .iter()
                .any(|violation| violation.contains("exactly one `[~]`")),
            "a state nothing marks is refused"
        );

        let summary = analyze(&doing, &marked, &note(NOTE)).expect("every file agrees");
        assert_eq!(summary.state.status, "doing");
    }

    #[test]
    fn a_session_note_about_another_task_is_rejected() {
        let doing = HEADER.replace("state=idle", "state=doing");
        let marked = backlog().replace("- `[ ]` **A3**", "- `[~]` **A3**");
        let error = analyze(&doing, &marked, &note("task: A4\ngoal: x\nnow: y\nrun: z\n"))
            .expect_err("a foreign session note must fail");
        assert!(error.iter().any(|violation| violation.contains("is about task A4")), "{error:?}");

        let error = analyze(&doing, &marked, &note("# note without a task line\n"))
            .expect_err("a session note without a task line must fail");
        assert!(error.iter().any(|violation| violation.contains("needs a `task: <id>` line")), "{error:?}");
    }

    #[test]
    fn a_session_note_needs_a_goal_the_current_step_and_a_command() {
        let doing = HEADER.replace("state=idle", "state=doing");
        let marked = backlog().replace("- `[ ]` **A3**", "- `[~]` **A3**");
        let error = analyze(&doing, &marked, &note("task: A3\ngoal:\nrun:   \n")).expect_err("an empty note must fail");
        for key in ["goal", "now", "run"] {
            assert!(
                error.iter().any(|violation| violation.contains(&format!("`{key}:` line"))),
                "the error must name `{key}:`: {error:?}"
            );
        }
    }

    #[test]
    fn a_doing_task_that_the_backlog_does_not_mark_is_rejected() {
        let doing = HEADER.replace("state=idle", "state=doing");
        let error =
            analyze(&doing, backlog(), &note(NOTE)).expect_err("a doing state without a `[~]` marker must fail");
        assert!(error.iter().any(|violation| violation.contains("exactly one `[~]`")), "{error:?}");
    }

    #[test]
    fn an_idle_plan_must_not_hide_a_session_note_or_an_open_task() {
        let error = analyze(HEADER, backlog(), &note(NOTE)).expect_err("an idle session note must fail");
        assert!(error.iter().any(|violation| violation.contains("must not leave")), "{error:?}");

        let marked = backlog().replace("- `[ ]` **A3**", "- `[~]` **A3**");
        let error = analyze(HEADER, &marked, &[]).expect_err("an idle open task must fail");
        assert!(error.iter().any(|violation| violation.contains("still in progress")), "{error:?}");
    }

    #[test]
    fn the_state_must_name_a_task_in_the_backlog() {
        let error =
            analyze(&HEADER.replace("task=A3", "task=A9"), backlog(), &[]).expect_err("an unknown task id must fail");
        assert!(error.iter().any(|violation| violation.contains("not a task in `plan/BACKLOG.md`")), "{error:?}");
    }

    #[test]
    fn the_current_task_must_sit_in_the_now_section() {
        let split = "## Now\n\n\
                     - `[ ]` **A3** first `files: crates/ui`\n\n\
                     ## Next\n\n\
                     - `[ ]` **A4** later `files: crates/model`\n\n\
                     ## Done\n\n\
                     - `[x]` **A2** ring of segments — `0833b4c`\n";
        let error = analyze(&HEADER.replace("task=A3", "task=A4"), split, &[])
            .expect_err("a `## Next` task cannot be the current one");
        assert!(error.iter().any(|violation| violation.contains("must be in `## Now`")), "{error:?}");
    }

    #[test]
    fn the_current_task_must_be_the_first_unfinished_one_in_now() {
        let backlog = "## Now\n\n\
                      - `[ ]` **A3** first `files: crates/ui`\n\
                      - `[ ]` **A4** later `files: crates/model`\n\n\
                      ## Done\n\n\
                      - `[x]` **A2** ring of segments — `0833b4c`\n";
        let error = analyze(&HEADER.replace("task=A3", "task=A4"), backlog, &[])
            .expect_err("a task behind the first open one must fail");
        assert!(
            error.iter().any(|violation| violation.contains("is not the first unfinished task in `## Now`")),
            "{error:?}"
        );
        assert!(error.iter().any(|violation| violation.contains("`A3` comes before it")), "{error:?}");

        analyze(HEADER, backlog, &[]).expect("the first open task is the current one");
    }

    #[test]
    fn a_finished_task_ahead_in_now_does_not_hold_the_work_order() {
        // A `[x]` line in `## Now` is its own error; the work order must not pile a second one on it.
        let backlog = "## Now\n\n\
                      - `[x]` **A2** already shipped — `0833b4c`\n\
                      - `[ ]` **A3** first open `files: crates/ui`\n\n\
                      ## Done\n\n\
                      - `[x]` **A1** older — `0833b4c`\n";
        let error = analyze(HEADER, backlog, &[]).expect_err("a `[x]` outside `## Done` still fails");
        assert!(error.iter().any(|violation| violation.contains("belongs in `## Done`")), "{error:?}");
        assert!(
            !error.iter().any(|violation| violation.contains("is not the first unfinished task")),
            "the stray marker is not a work-order problem: {error:?}"
        );
    }

    #[test]
    fn a_bug_or_a_next_task_does_not_hold_the_work_order() {
        let backlog = "## Now\n\n\
                      - `[ ]` **A3** first `files: crates/ui`\n\n\
                      ## Bugs\n\n\
                      - `[ ]` **X2** a defect `files: crates/param/src/lib.rs`\n\n\
                      ## Next\n\n\
                      - `[ ]` **A4** later `files: crates/model`\n\n\
                      ## Done\n\n";
        analyze(HEADER, backlog, &[]).expect("only `## Now` orders the work");
    }

    #[test]
    fn a_finished_marker_only_belongs_in_the_done_section() {
        let stray = backlog().replace("- `[ ]` **A4**", "- `[x]` **A4**");
        let error = analyze(HEADER, &stray, &[]).expect_err("`[x]` outside `## Done` must fail");
        assert!(error.iter().any(|violation| violation.contains("belongs in `## Done`")), "{error:?}");

        let reopened = backlog().replace("- `[x]` **A2**", "- `[ ]` **A2**");
        let error = analyze(HEADER, &reopened, &[]).expect_err("`## Done` holding an open task must fail");
        assert!(error.iter().any(|violation| violation.contains("without `[x]`")), "{error:?}");
    }

    #[test]
    fn an_unfinished_task_needs_a_files_hint() {
        let hintless = backlog().replace(" `files: crates/model/src/lib.rs`", "");
        let error = analyze(HEADER, &hintless, &[]).expect_err("a task without a hint must fail");
        assert!(error.iter().any(|violation| violation.contains("has no `files:` hint")), "{error:?}");
    }

    #[test]
    fn an_updated_value_must_be_a_calendar_date() {
        for good in ["2026-11-04", "2024-02-29", "2000-02-29", "2026-12-31"] {
            assert_eq!(calendar_date(good), Some(good), "`{good}` is a date");
            assert!(
                updated_format_violation(good).is_none(),
                "`{good}` must pass: {:?}",
                updated_format_violation(good)
            );
        }

        for bad in [
            "",
            "2026-1-4",
            "26-11-04",
            "2026-11-04T00:00",
            "2026/11/04",
            "tomorrow",
            "2026-13-01",
            "2026-00-10",
            "2026-02-30",
            "2100-02-29",
            "0000-01-01",
            "2026-11-0x",
        ] {
            assert_eq!(calendar_date(bad), None, "`{bad}` is not a date");
            let violation = updated_format_violation(bad).expect("a non-date must be reported");
            assert!(violation.contains("`YYYY-MM-DD`"), "the message names the form: {violation}");
        }
    }

    #[test]
    fn an_updated_date_cannot_run_behind_the_base_commit() {
        let old = "`STATE: updated=2026-10-01` is earlier";
        let violation =
            updated_base_violation("2026-10-01", "abc1234", Some("2026-10-30")).expect("an older date fails");
        assert!(violation.starts_with(old), "{violation}");
        assert!(violation.contains("2026-10-30"), "the message names the commit date: {violation}");

        assert!(updated_base_violation("2026-10-30", "abc1234", Some("2026-10-30")).is_none(), "the same day is fine");
        assert!(updated_base_violation("2026-11-04", "abc1234", Some("2026-10-30")).is_none(), "a later day is fine");
    }

    #[test]
    fn an_unreadable_base_date_is_left_to_the_base_rule() {
        assert!(updated_base_violation("2026-10-01", "abc1234", None).is_none(), "no date means no comparison");
        assert!(
            updated_base_violation("2026-10-01", "abc1234", Some("not a date")).is_none(),
            "the commit's own date is not the plan's business"
        );
    }

    #[test]
    fn the_plan_checks_updated_when_it_reads_the_state_line() {
        let error = analyze(&HEADER.replace("updated=2026-10-30", "updated=last week"), backlog(), &[])
            .expect_err("a word is not a date");
        assert!(error.iter().any(|violation| violation.contains("is not a date")), "{error:?}");
    }

    #[test]
    fn the_base_must_be_a_commit_that_leads_to_head() {
        assert!(base_violation("HEAD", Some("db8374d"), true, true).is_none(), "`HEAD` is a legal base");
        let stale = base_violation("ea3153a", Some("db8374d"), true, false).expect("a side commit must fail");
        assert!(stale.contains("not an ancestor"), "{stale}");
        let unknown = base_violation("nope", Some("db8374d"), false, false).expect("an unknown commit must fail");
        assert!(unknown.contains("not a commit"), "{unknown}");
    }

    #[test]
    fn a_readme_may_describe_the_product_but_not_the_state() {
        let clean = "# Z3L\n\nA native editor. Build it with `cargo run -p app`.\n\
                     The state lives in `plan/STATUS.md`.\n";
        assert!(readme_violations(clean).is_empty(), "a product README is fine");

        for line in ["**Status:** M0 — scaffold", "Status: early"] {
            let readme = format!("# Project\n\n{line}\n");
            let violations = readme_violations(&readme);
            assert!(violations.iter().any(|violation| violation == README_RULE), "{violations:?}");
        }

        let milestone = readme_violations("# Project\n\nWe are at M7 now.\n");
        assert!(milestone.iter().any(|violation| violation.starts_with(README_RULE)), "{milestone:?}");
        assert!(milestone.iter().any(|violation| violation.contains("`M7`")), "{milestone:?}");
    }

    #[test]
    fn a_milestone_inside_a_longer_word_is_not_a_status() {
        assert!(readme_violations("An AMD64 build and a 16M2 room.\n").is_empty());
        assert_eq!(milestone("phase M10 starts"), Some("M10"));
    }

    #[test]
    fn an_over_long_plan_file_fails_the_gate() {
        let long_status = format!("{HEADER}{}", "filler\n".repeat(MAX_STATUS_LINES));
        let error = analyze(&long_status, backlog(), &[]).expect_err("the budget is an error, not a warning");
        assert!(error.iter().any(|violation| violation.contains("over the 70-line budget")), "{error:?}");
        // The count alone is what cost a session seven minutes: the message names the width too.
        assert!(
            error
                .iter()
                .any(|violation| violation.contains(&format!("re-wraps at {} columns", crate::edit::PROSE_WIDTH))),
            "the refusal names the cause, not only the result: {error:?}"
        );

        let doing = HEADER.replace("state=idle", "state=doing");
        let marked = backlog().replace("- `[ ]` **A3**", "- `[~]` **A3**");
        let long_note = format!("{NOTE}{}", "filler\n".repeat(MAX_SESSION_LINES));
        let error = analyze(&doing, &marked, &note(&long_note)).expect_err("an over-long note must fail");
        assert!(error.iter().any(|violation| violation.contains("over the 50-line budget")), "{error:?}");
    }

    #[test]
    fn a_task_line_without_a_bold_id_fails_the_gate() {
        let broken = format!("{}- `[ ]` A9 an editor without a bold id\n", backlog());
        let error = analyze(HEADER, &broken, &[]).expect_err("a malformed task line must fail");
        assert!(error.iter().any(|violation| violation.contains("without a `**bold**` id")), "{error:?}");
    }

    #[test]
    fn a_missing_state_line_or_field_is_reported() {
        let error = analyze("# Status\n", backlog(), &[]).expect_err("a missing state line must fail");
        assert!(error.iter().any(|violation| violation.contains("needs a `STATE:` line")), "{error:?}");

        let error = analyze("STATE: milestone=M7 task=A3\n", backlog(), &[]).expect_err("missing fields must fail");
        assert!(error.iter().any(|violation| violation.contains("needs `state=`")), "{error:?}");
        assert!(error.iter().any(|violation| violation.contains("needs `base=`")), "{error:?}");
    }

    #[test]
    fn a_repeated_status_section_is_reported() {
        let doubled = format!("{HEADER}\n## Next\n\nA3 is next.\n\n## Next\n\nA3 again.\n");
        let error = analyze(&doubled, backlog(), &[]).expect_err("one section, one body");
        assert!(error.iter().any(|violation| violation.contains("two `## Next` sections")), "{error:?}");
    }

    #[test]
    fn a_duplicate_backlog_id_is_rejected() {
        let doubled = backlog().replace("- `[ ]` **A4**", "- `[ ]` **A3**");
        let error = analyze(HEADER, &doubled, &[]).expect_err("a duplicate id must fail");
        assert!(error.iter().any(|violation| violation.contains("appears twice")), "{error:?}");
    }

    #[test]
    fn the_summary_holds_the_current_task_and_the_next_open_ones() {
        let summary = analyze(HEADER, backlog(), &[]).expect("the plan is consistent");
        assert_eq!(summary.state.milestone, "M7");
        assert_eq!(summary.tasks.iter().filter(|task| task.status == TaskStatus::Open).count(), 2);
    }

    #[test]
    fn the_state_may_not_point_at_a_finished_task() {
        let error = analyze(&HEADER.replace("task=A3", "task=A2"), backlog(), &[])
            .expect_err("a finished task cannot be the current one");
        assert!(error.iter().any(|violation| violation.contains("is done")), "{error:?}");
    }

    #[test]
    fn the_next_paragraph_must_name_the_current_task() {
        let status = "STATE: milestone=M7 phase=A task=A3 state=idle base=32c659f updated=2026-10-30\n\
                      ## Next\n\nA3 is next: the card.\n";
        assert!(next_violation(status, Some("A3")).is_none());

        let wrong = status.replace("A3 is next", "A4 is next");
        let violation = next_violation(&wrong, Some("A3")).expect("another task is named");
        assert!(violation.contains("names A4"), "{violation}");

        let silent = "STATE: milestone=M7 task=A3 state=idle base=32c659f updated=2026-10-30\n";
        assert!(next_violation(silent, Some("A3")).is_none(), "a plan without `## Next` is not a violation");

        let unnamed = status.replace("A3 is next: the card.", "the card comes next.");
        assert!(next_violation(&unnamed, Some("A3")).is_some_and(|violation| violation.contains("must name")));
    }

    #[test]
    fn a_map_row_naming_a_missing_path_is_reported() {
        let map = "# Map\n\n| Feature | Where | Notes |\n|---|---|---|\n\
                   | Ring | `crates/model/src/contour.rs` (`Contour`) | whole file |\n\
                   | Gone | `crates/model/src/gone.rs` (`Gone`) | moved |\n\
                   | Anchors | `crates/model/src/contour.rs` (Line/Arc) | prose stays prose |\n";
        let violations = map_violations(map, |path| path == "crates/model/src/contour.rs");
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("gone.rs") && violations[0].contains("Gone"), "{violations:?}");
    }

    #[test]
    fn a_debt_tag_the_header_does_not_declare_is_reported() {
        let debt = "# Known debt\n\nGaps.\n\nTags: `[engine]` commands, `[panel]` panels.\n\n\
                    - `[engine]` a gap\n- `[engin]` a typo\n- `[panel]` another gap\n";
        let violations = debt_violations(debt);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("[engin]"), "{violations:?}");

        let silent = "# Known debt\n\nProse without entries.\n";
        assert!(debt_violations(silent).is_empty());
        let undeclared = "# Known debt\n\n- `[engine]` a gap\n";
        assert!(
            debt_violations(undeclared).iter().any(|violation| violation.contains("no `Tags:` line")),
            "{:?}",
            debt_violations(undeclared)
        );
    }
}
