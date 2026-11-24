//! The writer half of the plan: the verbs that change `plan/*.md`.
//!
//! Every verb builds the new text with [`crate::edit`], then runs the gate over the whole plan as
//! it would be after the change, with the *real* repository's git facts, and only then lets the
//! bytes land through a temp file and a rename. A verb that would break the plan writes nothing and
//! prints the violations instead (ADR 0085).
//!
//! `--repair` is the one exception: a plan can carry several violations at once, and fixing them one
//! verb at a time would be refused at every step, because the others are still there. The flag lets
//! a write through when it strictly shrinks the set of violations — it removes at least one and adds
//! none — so the plan can be brought back with the commands instead of by hand.
//!
//! `--dry-run` prints the diff and stops. `plan/.lock` keeps two writers apart, and it covers the whole
//! read-modify-write cycle rather than the bytes: a verb reads the plan, splices the lines it owns and
//! validates the result, so two writers that both read the same backlog compute the same next id and
//! the later write replaces the earlier entry (P25). The lock is taken at a verb's entrance, a second
//! writer waits for it and then refuses with the file named, and a `--dry-run` holds it too, because a
//! preview of a write nobody else can land under is the only honest one.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{Git, ProcessGit, TaskStatus, edit};

/// The command line a verb that changes the plan accepts.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Print the diff and write nothing.
    pub dry_run: bool,
    /// Let the write through when it shrinks the set of violations, even though it does not empty it.
    pub repair: bool,
    /// The arguments that are not flags, in order.
    pub words: Vec<String>,
    /// `--key value` pairs, in order.
    pub values: Vec<(String, String)>,
    /// Flags that take no value.
    pub flags: Vec<String>,
}

impl Options {
    /// Parses `--key value`, `--flag` and bare words; `value_flags` names the flags that take a value.
    pub fn parse(args: &[String], value_flags: &[&str]) -> Result<Self, String> {
        let mut options = Options::default();
        let mut index = 0;
        while index < args.len() {
            let arg = args[index].as_str();
            index += 1;
            if arg == "--dry-run" {
                options.dry_run = true;
                continue;
            }
            if arg == "--repair" {
                options.repair = true;
                continue;
            }
            if let Some(flag) = arg.strip_prefix("--") {
                if value_flags.contains(&flag) {
                    let value = args.get(index).ok_or_else(|| format!("--{flag} needs a value"))?;
                    index += 1;
                    options.values.push((flag.to_owned(), value.clone()));
                } else {
                    // A bare flag: `--tick`, `--stdin`, `--keep-note`. A misspelled value flag
                    // still fails later, because its value is then missing.
                    options.flags.push(flag.to_owned());
                }
                continue;
            }
            options.words.push(arg.to_owned());
        }
        Ok(options)
    }

    /// The value of `--key`, when it was given.
    pub fn value(&self, key: &str) -> Option<&str> {
        self.values.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
    }

    /// The value of `--key` or the message that names the missing flag.
    pub fn required(&self, key: &str) -> Result<&str, String> {
        self.value(key).ok_or_else(|| format!("--{key} is required"))
    }

    /// Whether the flag was given.
    pub fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|flag| flag == name)
    }

    /// The body `--key` names: the value, or standard input with `--stdin`.
    pub fn body(&self, key: &str) -> Result<String, String> {
        if self.flag("stdin") {
            return read_stdin();
        }
        self.required(key).map(str::to_owned)
    }
}

/// Reads standard input to the end.
fn read_stdin() -> Result<String, String> {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text).map_err(|error| format!("cannot read stdin: {error}"))?;
    Ok(text)
}

/// `task add`, `start`, `set`, `step`, `note`, `close`, `reopen`, `take`, `defer`, `drop` and `phase`,
/// plus `state set`.
///
/// A checkout on a declared `stream/<key>` branch writes only its own two files — `task step` and
/// `task note` for a task of its own stream — and every other verb here refuses with the rule.
pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    run_with(root, args, &ProcessGit::new(root))
}

/// [`run`] with the git facts supplied by `git`, so a test needs no repository.
pub fn run_with(root: &Path, args: &[String], git: &dyn Git) -> Result<(), String> {
    let Some(verb) = args.first().map(String::as_str) else {
        return Err(
            "usage: cargo xtask task <add|start|set|step|note|close|reopen|take|defer|unstart|drop|phase> …".to_owned()
        );
    };
    // Before any plan file is read, because the read is half of the cycle the lock guards: a verb
    // that waits reads the backlog the writer in front of it left. Every refusal below releases it.
    let lock = Lock::acquire(root)?;
    if matches!(verb, "step" | "note") {
        if let Some(id) = args.get(1) {
            own_task_refusal(root, git, &format!("task {verb}"), id)?;
        }
    } else {
        shared_plan_refusal(root, git, &format!("task {verb}"))?;
    }
    let rest = &args[1..];
    match verb {
        "add" => add(root, rest, git, &lock),
        "start" => start(root, rest, git, &lock),
        "set" => set(root, rest, git, &lock),
        "step" => step(root, rest, git, &lock),
        "note" => note(root, rest, git, &lock),
        "close" => close(root, rest, git, &lock),
        "reopen" => reopen(root, rest, git, &lock),
        "drop" => drop(root, rest, git, &lock),
        "take" => take(root, rest, git, &lock),
        "defer" => defer(root, rest, git, &lock),
        "unstart" => unstart(root, rest, git, &lock),
        "phase" => phase(root, rest, git, &lock),
        other => Err(format!("unknown verb `{other}`")),
    }
}

/// `task add --phase <L> [--phase-name <text>] --files <paths> [--section Now|Next|Bugs] [--at top|end]
/// [--why/--shape/--done <text>] <lead>`
///
/// A new entry lands at the end of `## Next`, the section where work is collected before a stream
/// takes it, so filing a task never disturbs the order a session is working through. `--section`
/// sends it elsewhere (`Bugs` for a defect, `Now` for work that is already next), and and `--at end`
/// is what that section does by default.
///
/// `--at top` is the interjection: it puts the task at the head of `## Now` and points `STATE: task=`
/// at it, because the tool cannot leave a plan in which the current task is not the first open one.
/// `--at top` implies the `Now` section, since a task at the top of `## Next` is not an interjection;
/// naming another section beside it is refused.
///
/// `--phase` is one capital letter, and `--phase-name` declares it in the legend in the same write,
/// so the first task of a new phase does not need a second command. Without a name the phase must
/// already be declared: a file whose ids use a letter its legend does not explain is the drift this
/// gate reports.
fn add(root: &Path, args: &[String], git: &dyn Git, lock: &Lock) -> Result<(), String> {
    let options =
        Options::parse(args, &["phase", "phase-name", "section", "files", "why", "shape", "done", "at", "next"])?;
    let phase = options.required("phase")?.to_uppercase();
    let letter = one_letter(&phase)?;
    let files = options.required("files")?;
    let lead = options.words.join(" ");
    if lead.trim().is_empty() {
        return Err("a new task needs its opening sentence: `task add --phase S --files … \"what it is\"`".to_owned());
    }
    let at = match options.value("at") {
        None | Some("end") => edit::At::End,
        Some("top") => edit::At::Top,
        Some(other) => return Err(format!("`--at` takes `top` or `end`, not `{other}`")),
    };
    let section = match (options.value("section"), at) {
        (Some(section), _) => section,
        (None, edit::At::Top) => "Now",
        (None, edit::At::End) => "Next",
    };
    if at == edit::At::Top && !section.eq_ignore_ascii_case("Now") {
        return Err(format!(
            "`--at top` puts the task in the work order, so it needs the `Now` section, not `{section}`"
        ));
    }
    let backlog = read(root, "plan/BACKLOG.md")?;
    let backlog = match options.value("phase-name") {
        Some(name) => edit::set_phases(&backlog, &[(letter, name.to_owned())])?,
        None => {
            if let Some(legend) = edit::legend(&backlog)
                && legend.problems.is_empty()
                && legend.name(letter).is_none()
            {
                return Err(format!(
                    "phase {letter} is not declared in the legend; `cargo xtask task phase {letter} --name \"…\"` or `--phase-name`"
                ));
            }
            backlog
        }
    };
    let id = edit::next_id(&backlog, &phase);
    let mut fields: Vec<(String, String)> = Vec::new();
    for (label, key) in [("Why", "why"), ("Shape", "shape"), ("Done", "done")] {
        if let Some(body) = options.value(key) {
            fields.push((label.to_owned(), body.to_owned()));
        }
    }
    let block = edit::new_entry(&id, lead.trim(), &fields, files);
    let backlog = edit::insert_entry(&backlog, section, at, &block)?;
    let mut changes = Changes::new(root).with("plan/BACKLOG.md", backlog);
    if at == edit::At::Top {
        let status = read(root, "plan/STATUS.md")?;
        // A declared stream's entry joins the order without touching the state line, which describes
        // the plan's own checkout alone.
        let streams = crate::Streams::parse(&status).map_err(|sentences| sentences.join("; "))?;
        if streams.owner(&id).is_primary() {
            let mut status = edit::set_state(&status, &[("task".to_owned(), id.clone())])?;
            if edit::has_section(&status, "Next") {
                let body = match options.value("next") {
                    Some(next) => next.to_owned(),
                    None => format!("{id} is next: {}", lead.trim()),
                };
                status = edit::set_section(&status, "Next", &body)?;
            }
            changes = changes.with("plan/STATUS.md", status);
        }
    }
    apply(root, changes, &options, git, lock)?;
    if !options.dry_run {
        println!("task {id} added to `## {section}`");
    }
    Ok(())
}

/// `task phase <L> --name <text>`, or `task phase --declare "L=text" [--declare "M=text" …]`
///
/// The legend of `plan/BACKLOG.md` — what each id prefix means — is written here and nowhere else,
/// so a session can declare a phase when it opens one and the paragraph cannot drift from the ids
/// the file uses. Several letters may travel in one call because a file whose legend is still prose
/// has to gain every letter its ids use at once: the gate refuses a plan whose ids name a phase the
/// legend does not declare, so a one-letter-at-a-time migration could not land.
fn phase(root: &Path, args: &[String], git: &dyn Git, lock: &Lock) -> Result<(), String> {
    let options = Options::parse(args, &["name", "declare"])?;
    let mut declares: Vec<(char, String)> = Vec::new();
    for (key, value) in &options.values {
        if key != "declare" {
            continue;
        }
        let Some((letter, name)) = value.split_once('=') else {
            return Err(format!("`--declare` takes `L=text`, not `{value}`"));
        };
        declares.push((one_letter(&letter.trim().to_uppercase())?, name.trim().to_owned()));
    }
    if let Some(word) = options.words.first() {
        let letter = one_letter(&word.to_uppercase())?;
        declares.push((letter, options.required("name")?.to_owned()));
    }
    let backlog = read(root, "plan/BACKLOG.md")?;
    let backlog = edit::set_phases(&backlog, &declares)?;
    let changes = Changes::new(root).with("plan/BACKLOG.md", backlog);
    apply(root, changes, &options, git, lock)?;
    if !options.dry_run {
        println!("the phase legend changed");
    }
    Ok(())
}

/// The one capital letter a phase is, or the sentence that says what a phase is.
fn one_letter(text: &str) -> Result<char, String> {
    let mut chars = text.chars();
    let Some(letter) = chars.next() else {
        return Err("a phase is one capital letter, like `K`".to_owned());
    };
    if chars.next().is_some() || !letter.is_ascii_uppercase() {
        return Err(format!("`{text}` is not a phase: a phase is one capital letter, like `K`"));
    }
    Ok(letter)
}

/// `task start <ID> [--goal <text>] [--now <text>] [--run <cmd>]`
/// `task start <ID> [--goal|--now|--run|--resume <text>] [--force]`
///
/// A task at the head of `## Next` starts straight away while the order is empty: the verb takes it
/// into `## Now` in the same write, because the two commands it replaces (`take` then `start`) leave
/// a plan whose `STATE:` names nothing in between, and three sessions in the reviewed period paid for
/// that sequence with a minute each. A task behind another open task, or in `## Bugs`, is still
/// `take`'s business: starting it would jump a work order that has a head.
fn start(root: &Path, args: &[String], git: &dyn Git, lock: &Lock) -> Result<(), String> {
    let options = Options::parse(args, &["goal", "now", "run", "resume"])?;
    let id = options.words.first().ok_or("task start needs a task id")?.clone();
    let summary = crate::check_with(root, git).map_err(violations)?;
    let task = find(&summary.tasks, &id)?;
    let status = read(root, "plan/STATUS.md")?;
    let streams = crate::Streams::parse(&status).map_err(|sentences| sentences.join("; "))?;
    let stream = streams.owner(&id).clone();
    let note_path = stream.note_path();
    // The head of `## Next` and the occupation of `## Now` are read inside the task's own stream: a
    // checklist that works one entry at a time does not hold another checkout's order hostage.
    let same_stream = |task: &crate::Task| streams.owner(&task.id).key == stream.key;
    let head_of_next = summary
        .tasks
        .iter()
        .find(|task| task.section == "Next" && task.status != TaskStatus::Done && same_stream(task));
    let takes = task.section != "Now"
        && task.section == "Next"
        && head_of_next.is_some_and(|head| head.id == id)
        && first_open_in(&streams, &summary.tasks, &stream).is_none();
    if task.section != "Now" && !takes {
        return Err(match task.section.as_str() {
            "Next" => format!(
                "task {id} sits in `## Next`; `cargo xtask task take {id} --at top` moves it into the order \
                 first, and `start` takes the head of `## Next` only while `## Now` is empty"
            ),
            _ => format!("task {id} sits in `## {}`; only a task in `## Now` can start", task.section),
        });
    }
    if let Some(other) =
        summary.tasks.iter().find(|task| task.status == TaskStatus::Doing && task.id != id && same_stream(task))
    {
        return Err(if stream.is_primary() {
            format!(
                "task {} is still in progress; close it before starting {id} (`cargo xtask task close {}`)",
                other.id, other.id
            )
        } else {
            format!(
                "task {} still works the `{}` stream; close it before starting {id} (`cargo xtask task close {}`)",
                other.id, stream.key, other.id
            )
        });
    }
    // Starting the task that is already running would overwrite the note a session is working from.
    if task.status == TaskStatus::Doing && !options.flag("force") {
        return Err(format!(
            "task {id} is already in progress; `--force` rewrites `{note_path}`, `task step` adds to it"
        ));
    }
    let backlog = read(root, "plan/BACKLOG.md")?;
    let backlog = if takes { edit::move_entry(&backlog, &id, "Now", edit::At::End)? } else { backlog };
    let backlog = edit::set_marker(&backlog, &id, edit::DOING)?;
    let status = if stream.is_primary() {
        let status =
            edit::set_state(&status, &[("task".to_owned(), id.clone()), ("state".to_owned(), "doing".to_owned())])?;
        // Taking the head of `## Next` changes which task is current, and the paragraph names it.
        if takes { edit::set_section(&status, "Next", &format!("{id} is next: {}", task.lead))? } else { status }
    } else {
        // A stream beside the plan's own keeps its own note and marker; the state line stays the
        // primary checkout's, which is what one writer of one line means.
        status
    };
    let session = session_note(&id, task, &options);
    let mut changes = Changes::new(root).with("plan/BACKLOG.md", backlog);
    if stream.is_primary() {
        changes = changes.with("plan/STATUS.md", status);
    }
    changes = changes.with(&note_path, session);
    apply(root, changes, &options, git, lock)?;
    if !options.dry_run {
        println!("task {id} started; `{note_path}` holds the note");
    }
    Ok(())
}

/// The note a start writes: the four fields the gate requires, filled from the entry.
fn session_note(id: &str, task: &crate::Task, options: &Options) -> String {
    let goal = options.value("goal").unwrap_or(&task.lead);
    let now = options.value("now").unwrap_or("starting the task");
    let run = options.value("run").unwrap_or("cargo xtask task");
    let resume = options.value("resume").unwrap_or("read the first `files:` path and its callers");
    format!(
        "# Session\n\n```text\ntask: {id}\ngoal: {goal}\n\nsteps:\n  [ ] {now}\n\nnow: {now}\nblocked: nothing\n\
         run: {run} {id}\nresume: {resume}\n```\n",
    )
}

/// `task set <ID> --field <Label> (--body <text>|--stdin) [--files <paths>]`
fn set(root: &Path, args: &[String], git: &dyn Git, lock: &Lock) -> Result<(), String> {
    let options = Options::parse(args, &["field", "body", "files"])?;
    let id = options.words.first().ok_or("task set needs a task id")?.clone();
    let typed = options.required("field")?;
    // The declared spelling, so `--field FILES` takes the `files:` branch and the message names the
    // label the file carries; an invented one is left as typed for `set_field` to refuse (P21).
    let field = crate::parse::canonical_label(typed).unwrap_or(typed).to_owned();
    let mut backlog = read(root, "plan/BACKLOG.md")?;
    let mut changes = Changes::new(root);
    if let Some(files) = options.value("files") {
        backlog = edit::set_files(&backlog, &id, files)?;
        changes = changes.with("plan/BACKLOG.md", backlog.clone());
    }
    if field != crate::parse::FILES_LABEL {
        let body = options.body("body")?;
        backlog = edit::set_field(&backlog, &id, &field, &body)?;
        changes = changes.with("plan/BACKLOG.md", backlog);
    }
    apply(root, changes, &options, git, lock)?;
    if !options.dry_run {
        println!("task {id}: `{field}` updated");
    }
    Ok(())
}

/// `task step <ID> [--tick | --add <text> | --now|--run|--resume <text>]`: the session note.
///
/// `--now`, `--run` and `--resume` rewrite the field beside the step list. A running session could
/// not keep its own resume hint true before this: `task set --field now` writes the backlog entry and
/// `task start` refuses a task that is already running, so a dead session was left believing a step
/// that was already taken (the `[plan]` debt entry that named this).
fn step(root: &Path, args: &[String], git: &dyn Git, lock: &Lock) -> Result<(), String> {
    let options = Options::parse(args, &["add", "now", "run", "resume"])?;
    let id = options.words.first().ok_or("task step needs a task id")?.clone();
    let fields: Vec<(&str, String)> = edit::SESSION_FIELDS
        .iter()
        .filter_map(|key| options.value(key).map(|value| (*key, value.to_owned())))
        .collect();
    let note_path = stream_of(root, &id)?.note_path();
    let session = match std::fs::read_to_string(root.join(&note_path)) {
        Ok(text) => text,
        // A stream checkout sees only what its branch carries, so the `task start` the primary ran
        // in its own working tree leaves the `[~]` marker in history and the note out of it. The
        // entry says which task runs and what its goal is, so the note is written from it here
        // instead of failing on a file another checkout owns (P28).
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let backlog = read(root, "plan/BACKLOG.md")?;
            let tasks = crate::parse_backlog(&backlog);
            let task = find(&tasks, &id)?;
            if task.status != TaskStatus::Doing {
                return Err(format!(
                    "`{note_path}` is not there and task {id} is not in progress; `cargo xtask task start \
                     {id}` writes the note, and `step` only keeps a running one true"
                ));
            }
            session_note(&id, task, &options)
        }
        Err(error) => return Err(format!("cannot read `{note_path}`: {error}")),
    };
    let mut session = if let Some(text) = options.value("add") {
        edit::add_step(&session, text)?
    } else if fields.is_empty() || options.flag("tick") {
        edit::tick_step(&session)?
    } else {
        session
    };
    for (key, value) in fields {
        session = edit::set_session_field(&session, key, &value)?;
    }
    let changes = Changes::new(root).with(&note_path, session);
    apply(root, changes, &options, git, lock)?;
    if !options.dry_run {
        println!("task {id}: `{note_path}` changed");
    }
    Ok(())
}

/// `task note <ID> (--body <text>|--stdin)`: appends to the research note, never rewrites it.
fn note(root: &Path, args: &[String], git: &dyn Git, lock: &Lock) -> Result<(), String> {
    let options = Options::parse(args, &["body"])?;
    let id = options.words.first().ok_or("task note needs a task id")?.clone();
    let body = options.body("body")?;
    let summary = crate::check_with(root, git).map_err(violations)?;
    find(&summary.tasks, &id)?;
    let path = format!("plan/notes/{id}.md");
    let existing = std::fs::read_to_string(root.join(&path)).unwrap_or_default();
    let text = if existing.trim().is_empty() {
        format!("# {id} — {}\n\n{}\n", first_line(&body), body.trim())
    } else {
        format!("{}\n{}\n", existing.trim_end(), body.trim())
    };
    let changes = Changes::new(root).with(&path, text);
    apply(root, changes, &options, git, lock)?;
    if !options.dry_run {
        println!("task {id}: `{path}` appended");
    }
    Ok(())
}

/// The first sentence of a note body, for the heading `note` writes.
fn first_line(body: &str) -> String {
    let line = body.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("research");
    line.trim_start_matches('#').trim().chars().take(80).collect()
}

/// `task close <ID> [--commit <hash|HEAD>] [--next <text>]`
fn close(root: &Path, args: &[String], git: &dyn Git, lock: &Lock) -> Result<(), String> {
    let options = Options::parse(args, &["commit", "next"])?;
    let id = options.words.first().ok_or("task close needs a task id")?.clone();
    let summary = crate::check_with(root, git).map_err(violations)?;
    let task = find(&summary.tasks, &id)?;
    if task.status == TaskStatus::Done {
        return Err(format!("task {id} is already done"));
    }
    let commit = options.value("commit").unwrap_or("HEAD");
    let hash = git.short(commit).ok_or_else(|| format!("`{commit}` is not a commit in this repository"))?;
    let suffix = if task.text.lines().count() > 1 { format!(" `[x]` `{hash}`") } else { format!(" — `{hash}`") };
    let backlog = read(root, "plan/BACKLOG.md")?;
    let backlog = edit::set_marker(&backlog, &id, edit::DONE)?;
    let backlog = edit::append_to_entry(&backlog, &id, &suffix)?;
    let backlog = edit::move_entry(&backlog, &id, "Done", edit::At::Top)?;
    let status = read(root, "plan/STATUS.md")?;
    let streams = crate::Streams::parse(&status).map_err(|sentences| sentences.join("; "))?;
    let stream = streams.owner(&id).clone();
    let note_path = stream.note_path();
    let tasks = crate::parse_backlog(&backlog);
    // The order may end here: `task=-` says no task is scheduled, and `## Next` says so in words.
    // Both speak for the plan's own stream, so another stream's close leaves them alone.
    let next = if stream.is_primary() { first_open_in(&streams, &tasks, &stream) } else { None };
    let current = next.map_or_else(|| crate::NO_TASK.to_owned(), |task| task.id.clone());
    // The plan describes the tree that is checked out now, so `base` and `updated` follow `HEAD`,
    // not the commit the closed task names (a task may be closed long after it landed).
    let mut values = vec![("task".to_owned(), current), ("state".to_owned(), "idle".to_owned())];
    if let Some(head) = git.short("HEAD") {
        values.push(("base".to_owned(), head));
    }
    if let Some(date) = git.commit_date("HEAD") {
        values.push(("updated".to_owned(), date));
    }
    let status = if stream.is_primary() { edit::set_state(&status, &values)? } else { status };
    // The `## Next` paragraph names the task `STATE:` does, so the close writes it in every case
    // rather than refusing one: a next task without `--next` gets the sentence `task take` writes for
    // an empty slot, an order that ends here gets the sentence that says so, and `--next` supplies
    // the words a person wants. Four closes in the reviewed period were refused on this one rule.
    // The paragraph names the plan's own next task, so a stream's close leaves it alone: the note
    // that changed belongs to the stream, and the plan's prose still describes the plan's order.
    let paragraph = match (stream.is_primary(), options.value("next"), next) {
        (false, _, _) => None,
        (true, Some(text), Some(_)) => Some(text.to_owned()),
        (true, Some(text), None) if text.trim().starts_with(crate::NOTHING_SCHEDULED) => Some(text.to_owned()),
        (true, Some(text), None) => Some(format!("{}: {text}", crate::NOTHING_SCHEDULED)),
        (true, None, Some(task)) => Some(format!("{} is next: {}", task.id, task.lead)),
        (true, None, None) => Some(nothing_scheduled(&tasks)),
    };
    let status = match paragraph {
        Some(body) if edit::has_section(&status, "Next") => edit::set_section(&status, "Next", &body)?,
        _ => status,
    };
    let mut changes = Changes::new(root).with("plan/BACKLOG.md", backlog).with("plan/STATUS.md", status);
    if root.join(&note_path).exists() {
        changes = changes.removed(&note_path);
    }
    apply(root, changes, &options, git, lock)?;
    if !options.dry_run {
        match next {
            Some(task) => println!("task {id} closed into `## Done` at `{hash}`; `{}` is next", task.id),
            None => println!("task {id} closed into `## Done` at `{hash}`; the work order is empty"),
        }
    }
    Ok(())
}

/// `task take <ID> [--at top|end] [--next <text>]`: moves an open entry into `## Now`.
///
/// The order of work is the order of `## Now`, and `## Bugs` and `## Next` are not part of it, so a
/// session that wants to work on an entry filed there has to move the line first. Doing that by hand
/// is forbidden (ADR 0085), and no verb could do it, so the work order could only advance while
/// `## Now` already held the next task. Taking behind another open task leaves `STATE:` alone; at the
/// top, and into an empty order, the taken task becomes current and `## Next` says so, because the
/// gate requires the paragraph to name the task `STATE:` does. The top is how an owner-directed
/// defect jumps the order, which is why it writes the state line rather than refusing.
fn take(root: &Path, args: &[String], git: &dyn Git, lock: &Lock) -> Result<(), String> {
    let options = Options::parse(args, &["at", "next"])?;
    let id = options.words.first().ok_or("task take needs a task id")?.clone();
    let at = match options.value("at") {
        None | Some("end") => edit::At::End,
        Some("top") => edit::At::Top,
        Some(other) => return Err(format!("`--at` takes `top` or `end`, not `{other}`")),
    };
    // Parsed straight from the file rather than through the gate: an empty `## Now` leaves the plan
    // with no valid current task, and taking an entry into it is the write that repairs that. The
    // gate still runs on the result, so `--repair` is needed while a violation stands.
    let backlog_text = read(root, "plan/BACKLOG.md")?;
    let tasks = crate::parse_backlog(&backlog_text);
    let task = find(&tasks, &id)?;
    if task.status == TaskStatus::Done {
        return Err(format!("task {id} is done; `cargo xtask task reopen {id}` puts it back in the order"));
    }
    if task.section == "Now" {
        return Err(format!("task {id} is already in `## Now`"));
    }
    let streams = streams_of(root)?;
    let stream = streams.owner(&id).clone();
    // The head of the order is the slot `STATE:` names, so nothing may take it from a task in
    // progress; the end of `## Now` is behind that task and disturbs nothing.
    if matches!(at, edit::At::Top)
        && let Some(other) =
            tasks.iter().find(|task| task.status == TaskStatus::Doing && streams.owner(&task.id).key == stream.key)
    {
        return Err(format!(
            "task {} is still in progress; `task take {id}` joins the end of `## Now` without taking its place",
            other.id
        ));
    }
    let occupied = first_open_in(&streams, &tasks, &stream).is_some();
    let backlog = edit::move_entry(&backlog_text, &id, "Now", at)?;
    let mut changes = Changes::new(root).with("plan/BACKLOG.md", backlog);
    // The head of `## Now` is the slot `STATE:` names, so an entry that lands in front of the order
    // becomes the current task in the same write — as `task add --at top` does. Behind another open
    // task the taken entry is not the head and the state line stays where it is.
    if stream.is_primary() && (at == edit::At::Top || !occupied) {
        let status = read(root, "plan/STATUS.md")?;
        let status =
            edit::set_state(&status, &[("task".to_owned(), id.clone()), ("state".to_owned(), "idle".to_owned())])?;
        let status = if !edit::has_section(&status, "Next") {
            status
        } else {
            match options.value("next") {
                Some(next) => edit::set_section(&status, "Next", next)?,
                None => edit::set_section(&status, "Next", &format!("{id} is next: {}", task.lead))?,
            }
        };
        changes = changes.with("plan/STATUS.md", status);
    }
    apply(root, changes, &options, git, lock)?;
    if !options.dry_run {
        println!("task {id} taken into `## Now`");
    }
    Ok(())
}

/// `task defer <ID> [--next <text>]`: moves an open entry out of the work order into `## Next`.
///
/// The inverse of [`take`], and the one write an owner needs when a task is real but its moment is
/// later than the rest of the order (R1 sat there until the release was planned): the entry keeps its fields, its
/// hint and its note, and stops being scheduled, so no session-start block names it until somebody
/// takes it back. The order must keep a head, so deferring the current task points `STATE:` at the
/// next open entry and names it in `## Next` in the same write; deferring a later entry leaves the
/// state alone. A running task is not deferred: its session note is the record of the step in hand.
fn defer(root: &Path, args: &[String], git: &dyn Git, lock: &Lock) -> Result<(), String> {
    let options = Options::parse(args, &["next"])?;
    let id = options.words.first().ok_or("task defer needs a task id")?.clone();
    let backlog_text = read(root, "plan/BACKLOG.md")?;
    let tasks = crate::parse_backlog(&backlog_text);
    let task = find(&tasks, &id)?;
    if task.status == TaskStatus::Done {
        return Err(format!("task {id} is done; there is nothing to defer"));
    }
    if task.status == TaskStatus::Doing {
        return Err(format!(
            "task {id} is in progress; `cargo xtask task close {id}` or `cargo xtask task reopen {id}` \
             ends its session before it can be deferred"
        ));
    }
    if task.section != "Now" {
        return Err(format!("task {id} sits in `## {}`, not in the work order", task.section));
    }
    let backlog = edit::move_entry(&backlog_text, &id, "Next", edit::At::End)?;
    let moved = crate::parse_backlog(&backlog);
    let streams = streams_of(root)?;
    let stream = streams.owner(&id).clone();
    let mut changes = Changes::new(root).with("plan/BACKLOG.md", backlog);
    if stream.is_primary() && first_open_in(&streams, &tasks, &stream).is_some_and(|head| head.id == id) {
        // Deferring the head hands the order to the next open entry, or leaves it empty; either way
        // `STATE:` and `## Next` say the same thing in the same write.
        let next = first_open_in(&streams, &moved, &stream);
        let current = next.map_or_else(|| crate::NO_TASK.to_owned(), |task| task.id.clone());
        let status = read(root, "plan/STATUS.md")?;
        let status = edit::set_state(&status, &[("task".to_owned(), current)])?;
        let status = match options.value("next") {
            Some(paragraph) => edit::set_section(&status, "Next", paragraph)?,
            None => {
                let paragraph = next
                    .map_or_else(|| nothing_scheduled(&moved), |task| format!("{} is next: {}", task.id, task.lead));
                edit::set_section(&status, "Next", &paragraph)?
            }
        };
        changes = changes.with("plan/STATUS.md", status);
    }
    apply(root, changes, &options, git, lock)?;
    if !options.dry_run {
        println!("task {id} deferred to `## Next`");
    }
    Ok(())
}

/// `task unstart <ID> [--next <text>]`: ends a session and leaves the entry where it stood.
///
/// A session that starts a task and finds it waiting on another one — a rule that has to land first,
/// an answer from the owner — has no word for the step back: `defer` refuses a `[~]` entry, `reopen`
/// takes only a done one, and moving the marker by hand is the write this tool exists to forbid. The
/// gap cost two sessions before the verb existed (`plan/debt.md`, `[plan]`).
///
/// Unlike `defer`, the entry keeps its place in the order: the session ended, the work did not move,
/// and a task taken out of the order has to be taken back at the top or the end — which is how a
/// prerequisite would otherwise end up behind the task it unblocks. The `plan/SESSION*.md` note goes
/// with the session, `state` returns to `idle`, and `## Next` names the head again because the gate
/// requires it to name the task `STATE:` does.
fn unstart(root: &Path, args: &[String], git: &dyn Git, lock: &Lock) -> Result<(), String> {
    let options = Options::parse(args, &["next"])?;
    let id = options.words.first().ok_or("task unstart needs a task id")?.clone();
    let backlog_text = read(root, "plan/BACKLOG.md")?;
    let tasks = crate::parse_backlog(&backlog_text);
    let task = find(&tasks, &id)?;
    if task.status != TaskStatus::Doing {
        return Err(format!(
            "task {id} is not in progress; `task take {id}` moves an open entry into the order and \
             `task reopen {id}` brings a done one back"
        ));
    }
    let backlog = edit::set_marker(&backlog_text, &id, edit::OPEN)?;
    let moved = crate::parse_backlog(&backlog);
    let streams = streams_of(root)?;
    let stream = streams.owner(&id).clone();
    let mut changes = Changes::new(root).with("plan/BACKLOG.md", backlog);
    if stream.is_primary() {
        let head = first_open_in(&streams, &moved, &stream);
        let current = head.map_or_else(|| crate::NO_TASK.to_owned(), |task| task.id.clone());
        let status = read(root, "plan/STATUS.md")?;
        // The plan goes back to `idle`, which is the state that claims it describes the tree that is
        // checked out: `base` and `updated` follow `HEAD` in the same write, as `close` moves them,
        // because the session that just ended may have committed.
        let mut values = vec![("task".to_owned(), current), ("state".to_owned(), "idle".to_owned())];
        if let Some(head) = git.short("HEAD") {
            values.push(("base".to_owned(), head));
        }
        if let Some(date) = git.commit_date("HEAD") {
            values.push(("updated".to_owned(), date));
        }
        let status = edit::set_state(&status, &values)?;
        let status = match options.value("next") {
            Some(paragraph) => edit::set_section(&status, "Next", paragraph)?,
            None if edit::has_section(&status, "Next") => {
                let paragraph = head
                    .map_or_else(|| nothing_scheduled(&moved), |task| format!("{} is next: {}", task.id, task.lead));
                edit::set_section(&status, "Next", &paragraph)?
            }
            None => status,
        };
        changes = changes.with("plan/STATUS.md", status);
    }
    let note_path = stream.note_path();
    if root.join(&note_path).exists() {
        changes = changes.removed(&note_path);
    }
    apply(root, changes, &options, git, lock)?;
    if !options.dry_run {
        println!("task {id} unstarted; it keeps its place in the order");
    }
    Ok(())
}

/// `task reopen <ID> [--next <text>]`
fn reopen(root: &Path, args: &[String], git: &dyn Git, lock: &Lock) -> Result<(), String> {
    let options = Options::parse(args, &["next"])?;
    let id = options.words.first().ok_or("task reopen needs a task id")?.clone();
    let summary = crate::check_with(root, git).map_err(violations)?;
    let task = find(&summary.tasks, &id)?;
    if task.status != TaskStatus::Done {
        return Err(format!("task {id} is not done"));
    }
    let backlog = read(root, "plan/BACKLOG.md")?;
    let backlog = edit::strip_commit(&backlog, &id)?;
    let backlog = edit::set_marker(&backlog, &id, edit::OPEN)?;
    let backlog = edit::move_entry(&backlog, &id, "Now", edit::At::Top)?;
    let status = read(root, "plan/STATUS.md")?;
    // Only the plan's own stream moves the state line; a reopened stream task is its own to start.
    let status = if is_primary(root, &id) {
        edit::set_state(&status, &[("task".to_owned(), id.clone()), ("state".to_owned(), "idle".to_owned())])?
    } else {
        status
    };
    // The reopened task becomes the current one, so `## Next` has to name it in the same write;
    // otherwise the gate refuses, and two hand edits are the only way to say what the plan means.
    let status = match options.value("next") {
        Some(next) => edit::set_section(&status, "Next", next)?,
        None if crate::validate::next_violation(&status, Some(&id)).is_some() => {
            return Err(format!(
                "`plan/STATUS.md` › `## Next` must name the reopened task {id}; \
                 `task reopen {id} --next \"{id} is next: …\"` rewrites it in the same write"
            ));
        }
        None => status,
    };
    let changes = Changes::new(root).with("plan/BACKLOG.md", backlog).with("plan/STATUS.md", status);
    apply(root, changes, &options, git, lock)?;
    if !options.dry_run {
        println!("task {id} reopened at the top of `## Now`");
    }
    Ok(())
}

/// `task drop <ID> [--keep-note]`
fn drop(root: &Path, args: &[String], git: &dyn Git, lock: &Lock) -> Result<(), String> {
    let options = Options::parse(args, &[])?;
    let id = options.words.first().ok_or("task drop needs a task id")?.clone();
    let summary = crate::check_with(root, git).map_err(violations)?;
    if summary.state.task == id {
        return Err(format!("task {id} is the current task; move `STATE:` to another task first"));
    }
    let backlog = read(root, "plan/BACKLOG.md")?;
    let backlog = edit::remove_entry(&backlog, &id)?;
    let mut changes = Changes::new(root).with("plan/BACKLOG.md", backlog);
    // A session note about a task that no longer exists would be refused by the gate as a leftover,
    // so the note of the dropped task's stream goes with it — but only when the note is *about* that
    // task. A live note always names the task its stream works, and dropping any other entry of the
    // same stream used to take the running session with it, which the gate then read as a doing
    // plan without a note (U119).
    if let Ok(stream) = stream_of(root, &id) {
        let session_note = stream.note_path();
        let about = std::fs::read_to_string(root.join(&session_note))
            .ok()
            .is_some_and(|note| crate::parse::session_field(&note, "task").is_some_and(|task| task == id));
        if about {
            changes = changes.removed(&session_note);
        }
    }
    let note = format!("plan/notes/{id}.md");
    if !options.flag("keep-note") && root.join(&note).exists() {
        changes = changes.removed(&note);
    }
    apply(root, changes, &options, git, lock)?;
    if !options.dry_run {
        println!("task {id} dropped");
    }
    Ok(())
}

/// Refuses a write to a shared plan file from a stream's checkout, naming the two files it does own.
///
/// `plan/` has one writer, the primary checkout (AGENTS.md §6.2): `plan/BACKLOG.md` carries every
/// stream's entries, and `plan/map.md`, `plan/debt.md` and the state line carry every stream's prose,
/// so a second writer turns every merge into the same files on both sides (P26). The stream's own two
/// files are `plan/SESSION.<key>.md` and `plan/notes/<ID>.md`, which no other checkout writes.
fn shared_plan_refusal(root: &Path, git: &dyn Git, verb: &str) -> Result<(), String> {
    let Some(key) = crate::stream_checkout(root, git) else { return Ok(()) };
    Err(format!(
        "`{verb}` writes the shared plan files, and `plan/` has one writer: the primary checkout, not a \
         `stream/` worktree. This one is `stream/{key}`, whose own files are `plan/SESSION.{key}.md` \
         (`task step`) and `plan/notes/<ID>.md` (`task note`); run `{verb}` from the primary checkout, \
         and the code here travels with the merge (AGENTS.md §6.2)"
    ))
}

/// Refuses a note for a task of another stream: each stream's note is written by its own checkout.
///
/// `task step` and `task note` stay open in a stream checkout because they write one task's own file;
/// the same verb for a task another stream owns would write that stream's file from here, which is the
/// second writer the rule above refuses.
fn own_task_refusal(root: &Path, git: &dyn Git, verb: &str, id: &str) -> Result<(), String> {
    let Some(key) = crate::stream_checkout(root, git) else { return Ok(()) };
    let owner = stream_of(root, id)?;
    if owner.key == key {
        return Ok(());
    }
    Err(format!(
        "task {id} belongs to the `{}` stream, so `{verb}` for it runs in that stream's checkout; this one \
         is `stream/{key}`",
        owner.key
    ))
}

/// The stream a task belongs to, read from the plan the way the gate reads it.
fn stream_of(root: &Path, id: &str) -> Result<crate::Stream, String> {
    let status = read(root, "plan/STATUS.md")?;
    let streams = crate::Streams::parse(&status).map_err(|violations| violations.join("; "))?;
    Ok(streams.owner(id).clone())
}

/// Whether `id` is the plan's own stream's task, the one `STATE: task=` schedules.
fn is_primary(root: &Path, id: &str) -> bool {
    stream_of(root, id).is_ok_and(|stream| stream.is_primary())
}

/// The first unfinished task of one stream in `## Now`: the stamp a close, a defer and a take read.
///
/// The order holds every stream's entries, so the head a `STATE:` line names is the head of the plan's
/// own stream — another stream's entry is in front of it and is not this checkout's work.
fn first_open_in<'a>(
    streams: &crate::Streams,
    tasks: &'a [crate::Task],
    stream: &crate::Stream,
) -> Option<&'a crate::Task> {
    tasks.iter().find(|task| {
        task.section == "Now" && task.status != TaskStatus::Done && streams.owner(&task.id).key == stream.key
    })
}

/// The stream declaration of `plan/STATUS.md`, or the gate's sentences when it cannot be read.
fn streams_of(root: &Path) -> Result<crate::Streams, String> {
    crate::Streams::parse(&read(root, "plan/STATUS.md")?).map_err(|violations| violations.join("; "))
}

/// The task with this id, or a message naming the id that is missing.
fn find<'a>(tasks: &'a [crate::Task], id: &str) -> Result<&'a crate::Task, String> {
    tasks.iter().find(|task| task.id == id).ok_or_else(|| format!("`plan/BACKLOG.md` has no task `{id}`"))
}

/// The first unfinished task in `## Now`, which is the task `STATE:` must name.
/// The paragraph an empty work order writes: nothing is scheduled, and the parked work is counted.
///
/// A fresh session reads `## Next` first, so the empty order has to read as a state and not as a
/// missing sentence; the count tells it that the backlog is not empty, only the order.
fn nothing_scheduled(tasks: &[crate::Task]) -> String {
    let parked = tasks.iter().filter(|task| task.status != TaskStatus::Done && task.section != "Now").count();
    match parked {
        0 => format!("{}: the backlog holds no open task.", crate::NOTHING_SCHEDULED),
        1 => format!("{}: 1 open task waits in `## Next`.", crate::NOTHING_SCHEDULED),
        count => format!("{}: {count} open tasks wait in `## Next`.", crate::NOTHING_SCHEDULED),
    }
}

/// The files a verb wants to change, and the files it wants gone.
struct Changes {
    root: PathBuf,
    files: Vec<(String, String)>,
    removed: Vec<String>,
}

impl Changes {
    fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf(), files: Vec::new(), removed: Vec::new() }
    }

    fn with(mut self, path: &str, text: String) -> Self {
        self.files.push((path.to_owned(), text));
        self
    }

    fn removed(mut self, path: &str) -> Self {
        self.removed.push(path.to_owned());
        self
    }

    /// Whether anything is asked for at all.
    fn is_empty(&self) -> bool {
        self.files.is_empty() && self.removed.is_empty()
    }

    /// The files whose text actually changes, with the text that is on disk now.
    fn changed(&self) -> Vec<(&str, Option<String>, &str)> {
        self.files
            .iter()
            .map(|(path, text)| (path.as_str(), std::fs::read_to_string(self.root.join(path)).ok(), text.as_str()))
            .filter(|(_, old, new)| old.as_deref() != Some(*new))
            .collect()
    }
}

/// Validates the plan as it would be, then lands the change — or prints why it refused and writes
/// nothing. The caller holds the writer [`Lock`] from before it read the plan, so the whole cycle is
/// one writer's.
///
/// A plan that is already broken cannot be repaired one verb at a time, because every verb is refused
/// while the other violations stand: `--repair` lets a write through when it strictly shrinks the set
/// of violations, so `plan/BACKLOG.md` and `plan/STATUS.md` can be brought back with commands.
fn apply(root: &Path, changes: Changes, options: &Options, git: &dyn Git, _lock: &Lock) -> Result<(), String> {
    if changes.is_empty() {
        return Err("nothing to change".to_owned());
    }
    let changed = changes.changed();
    if changed.is_empty() && changes.removed.is_empty() {
        println!("no change");
        return Ok(());
    }
    let status = resulting(root, &changes, "plan/STATUS.md").ok_or("`plan/STATUS.md` is missing")?;
    let backlog = resulting(root, &changes, "plan/BACKLOG.md").ok_or("`plan/BACKLOG.md` is missing")?;
    // The declaration in the new `plan/STATUS.md` names the notes, so a stream's own note is checked
    // beside the plan's: a verb that writes one cannot quietly break another.
    let streams = crate::Streams::parse(&status).unwrap_or_else(|_| crate::Streams::one());
    let sessions: Vec<(String, String)> = streams
        .notes()
        .into_iter()
        .filter_map(|path| resulting(root, &changes, &path).map(|text| (path, text)))
        .collect();
    let notes = notes_after(root, &changes)?;
    // A reference file the change writes is checked as it will be, not as it is on disk: a stale map
    // row was invisible to the write that landed it (P27).
    let references: Vec<(String, String)> =
        changes.files.iter().filter(|(path, _)| REFERENCE_FILES.contains(&path.as_str())).cloned().collect();
    let plan = crate::PlanText {
        status: &status,
        backlog: &backlog,
        sessions: &sessions,
        notes: Some(&notes),
        references: Some(&references),
    };
    let after = violations_of(root, &plan, git);
    let repairing = if after.is_empty() {
        false
    } else {
        let before = crate::check_with(root, git).err().unwrap_or_default();
        if options.repair && shrinks(&before, &after) {
            true
        } else {
            return Err(refusal(&after, options.repair, &before));
        }
    };

    for (path, old, new) in &changed {
        println!("{}", diff(path, old.as_deref().unwrap_or(""), new));
    }
    for path in &changes.removed {
        println!("--- {path}\n(deleted)");
    }
    if repairing {
        println!("repair: {} violation(s) still stand", after.len());
    }
    if options.dry_run {
        println!("dry run: nothing written");
        return Ok(());
    }

    for (path, _, text) in &changed {
        write_atomic(&root.join(path), text)?;
    }
    for path in &changes.removed {
        std::fs::remove_file(root.join(path)).map_err(|error| format!("cannot delete `{path}`: {error}"))?;
    }
    Ok(())
}

/// The text `path` will have once the change lands, or `None` when the change deletes it.
fn resulting(root: &Path, changes: &Changes, path: &str) -> Option<String> {
    if changes.removed.iter().any(|removed| removed == path) {
        return None;
    }
    changes
        .files
        .iter()
        .find(|(candidate, _)| candidate == path)
        .map(|(_, text)| text.clone())
        .or_else(|| std::fs::read_to_string(root.join(path)).ok())
}

/// The notes directory as it will be: what is on disk, with this change applied.
fn notes_after(root: &Path, changes: &Changes) -> Result<Vec<(String, String)>, String> {
    let mut notes: Vec<(String, String)> = match std::fs::read_dir(root.join("plan/notes")) {
        Ok(entries) => entries
            .flatten()
            .filter(|entry| entry.path().is_file())
            .map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
                (name, text)
            })
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(format!("cannot read `plan/notes/`: {error}")),
    };
    notes.retain(|(name, _)| !changes.removed.iter().any(|path| path == &format!("plan/notes/{name}")));
    for (path, text) in &changes.files {
        let Some(name) = path.strip_prefix("plan/notes/") else { continue };
        notes.retain(|(candidate, _)| candidate != name);
        notes.push((name.to_owned(), text.clone()));
    }
    notes.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(notes)
}

/// The one-writer lock: `plan/.lock`, created exclusively and removed when the verb ends.
///
/// It is taken before a verb reads the plan and released after the bytes land, because the read is
/// half of the cycle: two writers that both read the backlog compute the same next free id, and the
/// later write replaces the earlier entry while the gate sees nothing wrong (P25). `create_new` is
/// the test-and-set, so no reader can see a half-taken lock; a second writer waits for [`PATIENCE`]
/// and then refuses with the file named, because a lock that outlives a verb is a stale file.
#[derive(Debug)]
struct Lock {
    path: PathBuf,
}

/// How long a second writer waits for the one in front of it. A verb's cycle is tens of milliseconds
/// on a plan of any size, so a lock that outlives this is a session that died or a stale file.
const PATIENCE: Duration = Duration::from_secs(2);

/// How often the waiting writer tries again: short enough that a verb's own cycle is not delayed.
const POLL: Duration = Duration::from_millis(10);

impl Lock {
    /// Takes the lock, waiting for the writer ahead of this one and refusing once it has waited long
    /// enough to call the file stale.
    fn acquire(root: &Path) -> Result<Self, String> {
        Self::acquire_within(root, PATIENCE)
    }

    /// [`Lock::acquire`] with the wait named, so a test proves the refusal without spending it.
    fn acquire_within(root: &Path, patience: Duration) -> Result<Self, String> {
        let path = root.join("plan/.lock");
        let started = Instant::now();
        loop {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if started.elapsed() >= patience {
                        return Err(format!(
                            "`plan/.lock` is taken: another session is writing the plan, and this verb waited {} \
                             ms for it and wrote nothing; run it again once that session ends, and delete the \
                             file only if no session runs",
                            patience.as_millis()
                        ));
                    }
                    std::thread::sleep(POLL.min(patience));
                }
                Err(error) => return Err(format!("cannot create `plan/.lock`: {error}")),
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Writes through a sibling temp file and a rename, so a half-written plan never reaches the disk.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("plan");
    let temporary = path.with_file_name(format!(".{name}.tmp"));
    std::fs::write(&temporary, text).map_err(|error| format!("cannot write `{}`: {error}", path.display()))?;
    std::fs::rename(&temporary, path).map_err(|error| format!("cannot replace `{}`: {error}", path.display()))
}

/// The diff of one file: the lines that changed, with two lines of context either side.
fn diff(path: &str, old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let prefix = old_lines.iter().zip(&new_lines).take_while(|(a, b)| a == b).count();
    let suffix = old_lines
        .iter()
        .rev()
        .zip(new_lines.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(old_lines.len().saturating_sub(prefix))
        .min(new_lines.len().saturating_sub(prefix));
    let context_start = prefix.saturating_sub(2);
    let mut out = format!("--- {path}\n");
    for line in &old_lines[context_start..prefix] {
        out.push_str(&format!("  {line}\n"));
    }
    for line in &old_lines[prefix..old_lines.len().saturating_sub(suffix)] {
        out.push_str(&format!("- {line}\n"));
    }
    for line in &new_lines[prefix..new_lines.len().saturating_sub(suffix)] {
        out.push_str(&format!("+ {line}\n"));
    }
    for line in new_lines.iter().skip(new_lines.len().saturating_sub(suffix)).take(2) {
        out.push_str(&format!("  {line}\n"));
    }
    out
}

/// The gate's violations as one message: they are printed once and the verb writes nothing.
fn violations(errors: Vec<String>) -> String {
    for error in &errors {
        eprintln!("plan: {error}");
    }
    format!("refused: the plan has {} violation(s); nothing was written", errors.len())
}

/// The violations the plan has once the change lands, or none when it is valid.
fn violations_of(root: &Path, plan: &crate::PlanText<'_>, git: &dyn Git) -> Vec<String> {
    crate::check_text(root, plan, git).err().unwrap_or_default()
}

/// Whether `after` is a strict subset of `before`: at least one violation gone, none new.
///
/// A repair may not trade one violation for another, and may not repeat one it claims to fix, so
/// `--repair` cannot be used to land a change that leaves the plan in a state nobody looked at.
fn shrinks(before: &[String], after: &[String]) -> bool {
    after.len() < before.len() && after.iter().all(|violation| before.contains(violation))
}

/// The refusal for a write that would leave violations, with the one line that says how to proceed.
///
/// `before` is what the plan has now: when the write takes a violation away and leaves the rest, the
/// message names `--repair`, which is the only way to bring a plan with several violations back.
fn refusal(after: &[String], repair: bool, before: &[String]) -> String {
    for error in after {
        eprintln!("plan: {error}");
    }
    if repair {
        eprintln!(
            "plan: `--repair` passes only a write that removes a violation and adds none; this one \
             leaves {} where the plan has {}",
            after.len(),
            before.len()
        );
    } else if after.len() < before.len() {
        eprintln!("plan: this write removes a violation; `--repair` passes it and leaves the rest");
    }
    format!("refused: the plan has {} violation(s); nothing was written", after.len())
}

/// Reads a required plan file.
fn read(root: &Path, path: &str) -> Result<String, String> {
    std::fs::read_to_string(root.join(path)).map_err(|error| format!("cannot read `{path}`: {error}"))
}

/// `state set key=value …`, `state streams key=AB …`, `state next (--body <text>|--stdin)
/// [--task <id>]` and `state where (--body <text>|--stdin)`.
///
/// `state streams` owns the `STREAMS:` line, the declaration of the checkouts beside the plan's own:
/// it rewrites the whole line from the pairs it is given, so a stream joins by naming the ones
/// already there (`state streams agent=J catalog=O render=Y,L`), and no pairs return the plan to one
/// stream. It is the only verb that line has, which is why the usage below names it: a fifth
/// checkout that cannot find this verb cannot mark its own task (P22).
///
/// `state next --task <id>` names a new current task in the same write: the gate requires the
/// paragraph to name the task `STATE:` does, so a paragraph that moves on its own would be refused,
/// and the only way out would be a hand edit. `state where` owns the prose that describes the tree
/// `base` points at, so a session that sets `base=HEAD` can say what changed in the same command.
pub fn state(root: &Path, args: &[String], git: &dyn Git) -> Result<(), String> {
    let options = Options::parse(args, &["body", "task"])?;
    let verb = format!("state {}", options.words.first().map(String::as_str).unwrap_or("set"));
    let lock = Lock::acquire(root)?;
    shared_plan_refusal(root, git, &verb)?;
    match options.words.first().map(String::as_str) {
        Some("next") => {
            let body = options.body("body")?;
            let status = read(root, "plan/STATUS.md")?;
            let status = edit::set_section(&status, "Next", &body)?;
            let status = match options.value("task") {
                Some(task) => edit::set_state(&status, &[("task".to_owned(), task.to_owned())])?,
                None => status,
            };
            return apply(root, Changes::new(root).with("plan/STATUS.md", status), &options, git, &lock);
        }
        Some("where") => {
            let body = options.body("body")?;
            let status = read(root, "plan/STATUS.md")?;
            let status = edit::set_section(&status, "Where we are", &body)?;
            return apply(root, Changes::new(root).with("plan/STATUS.md", status), &options, git, &lock);
        }
        // `state streams agent=J catalog=O` declares the checkouts beside this one; no pairs puts the
        // plan back on its single stream.
        Some("streams") => {
            let pairs: Vec<String> = options.words.iter().skip(1).cloned().collect();
            let status = read(root, "plan/STATUS.md")?;
            let status = edit::set_streams(&status, &pairs)?;
            return apply(root, Changes::new(root).with("plan/STATUS.md", status), &options, git, &lock);
        }
        Some("set") => {}
        _ => {
            return Err(
                "usage: cargo xtask state <set key=value …|streams key=AB …|next|where (--body <text>|--stdin)>"
                    .to_owned(),
            );
        }
    }
    let mut values: Vec<(String, String)> = Vec::new();
    for word in options.words.iter().skip(1) {
        let (key, value) = word.split_once('=').ok_or_else(|| format!("`{word}` is not key=value"))?;
        values.push((key.to_owned(), value.to_owned()));
    }
    if values.is_empty() {
        return Err("state set needs at least one key=value".to_owned());
    }
    let status = read(root, "plan/STATUS.md")?;
    let status = edit::set_state(&status, &values)?;
    apply(root, Changes::new(root).with("plan/STATUS.md", status), &options, git, &lock)
}

/// The reference files a command owns: the plan's data lives here too, and a session that edits one
/// by hand leaves no diff through the tool and no gate run to catch a stale row.
const REFERENCE_FILES: &[&str] = &[
    "plan/map.md",
    "plan/debt.md",
    "plan/questions.md",
    "plan/refactor.md",
    "plan/rename.md",
    "plan/archive/done.md",
    "plan/SESSION.template.md",
];

/// The reference files the plan calls append-only: their entries accumulate, so a body that drops
/// one is refused by [`file`] unless the person asks for a whole-file rewrite with `--replace`.
const APPEND_ONLY: &[&str] = &["plan/debt.md", "plan/questions.md"];

/// `file <path> (--body <text>|--stdin) [--replace] [--dry-run]`: one reference file, written through
/// the gate.
///
/// `plan/BACKLOG.md`, `plan/STATUS.md` and `plan/SESSION.md` are not on the list: the `task` and
/// `state` verbs own their lines, and a whole-file rewrite would lose the entry formatting their
/// splices preserve.
///
/// An append-only file ([`APPEND_ONLY`]) is written only when every entry it holds is in the new
/// body: the body a session passes is usually its own text plus the entries it read, and a body that
/// forgot one destroys it without a diff anybody was shown (X129). `--replace` is the reset — an
/// entry leaves `plan/debt.md` when the fix lands, and that is a whole-file rewrite by design.
pub fn file(root: &Path, args: &[String], git: &dyn Git) -> Result<(), String> {
    let options = Options::parse(args, &["body"])?;
    let lock = Lock::acquire(root)?;
    shared_plan_refusal(root, git, "file")?;
    let path = options.words.first().ok_or("`file` needs the path of a reference file")?;
    if !REFERENCE_FILES.contains(&path.as_str()) {
        // The three plan files `file` refuses are the ones a `task` or `state` verb splices line by
        // line, so the refusal names the verb that owns the file instead of leaving the caller to
        // conclude the plan cannot be written at all (P22).
        if let Some(owner) = match path.as_str() {
            "plan/BACKLOG.md" | "plan/SESSION.md" => Some("the `task` verbs"),
            "plan/STATUS.md" => Some("the `state` and `task` verbs"),
            _ => None,
        } {
            return Err(format!(
                "`{path}` is written by {owner}, not by `file`: a whole-file rewrite would lose the \
                 formatting their splices keep"
            ));
        }
        return Err(format!("`{path}` is not a reference file a command owns; one of {}", REFERENCE_FILES.join(", ")));
    }
    let body = options.body("body")?;
    if body.trim().is_empty() {
        return Err(format!("`{path}` would be empty; a reference file says something or it does not exist"));
    }
    let mut text = body.trim_end().to_owned();
    text.push('\n');
    if APPEND_ONLY.contains(&path.as_str()) && !options.flag("replace") {
        let old = std::fs::read_to_string(root.join(path)).ok();
        let left = dropped_entries(old.as_deref().unwrap_or_default(), &text);
        if !left.is_empty() {
            return Err(format!(
                "`{path}` is append-only: this body drops {} entr{} it holds — {}; pass `--replace` to \
                 rewrite the whole file",
                left.len(),
                if left.len() == 1 { "y" } else { "ies" },
                left.join("; ")
            ));
        }
    }
    apply(root, Changes::new(root).with(path, text), &options, git, &lock)
}

/// The entries `body` leaves out of `old`, each quoted by its opening words.
///
/// An entry is a top-level `- ` bullet with its wrapped lines joined onto it (the same reading the
/// task report and the debt selection use), so re-wrapping a body at another column leaves its
/// entries present, while a reworded entry counts as gone: append-only means the text stays.
fn dropped_entries(old: &str, body: &str) -> Vec<String> {
    let mut held = crate::parse::markdown_bullets(body);
    let mut dropped: Vec<String> = Vec::new();
    for entry in crate::parse::markdown_bullets(old) {
        if let Some(at) = held.iter().position(|candidate| *candidate == entry) {
            held.remove(at);
        } else {
            dropped.push(opening_words(&entry));
        }
    }
    dropped
}

/// The first words of an entry, enough for a person to find it in the file, and an ellipsis when
/// there is more. The inline-code backticks are dropped, so an entry's own quoting cannot nest inside
/// the quotes the refusal puts around it.
fn opening_words(entry: &str) -> String {
    const WORDS: usize = 12;
    let mut words = entry.split_whitespace();
    let opening: Vec<&str> = words.by_ref().take(WORDS).map(|word| word.trim_matches('`')).collect();
    let quoted = opening.join(" ");
    if words.next().is_some() { format!("\"{quoted} …\"") } else { format!("\"{quoted}\"") }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::{Git, TaskStatus};

    /// The git facts a verb needs, without a repository.
    struct FakeGit;

    impl Git for FakeGit {
        fn head(&self) -> Option<String> {
            Some("abc1234".to_owned())
        }

        fn commit_exists(&self, hash: &str) -> bool {
            !hash.is_empty() && hash.chars().all(|ch| ch.is_ascii_hexdigit())
        }

        fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
            ancestor == "abc1234" && (descendant == "abc1234" || descendant == "fedcba9")
        }

        fn commit_date(&self, hash: &str) -> Option<String> {
            (hash == "HEAD" || self.commit_exists(hash)).then(|| "2026-11-10".to_owned())
        }

        fn short(&self, hash: &str) -> Option<String> {
            (hash == "HEAD" || self.commit_exists(hash)).then(|| "abc1234".to_owned())
        }

        fn subject(&self, hash: &str) -> Option<String> {
            self.commit_exists(hash).then(|| "a commit".to_owned())
        }
    }

    const STATUS: &str = "# Status\n\nSTATE: milestone=M1 phase=P task=P6 state=idle base=abc1234 updated=2026-11-10\n";
    const BACKLOG: &str = "## Now\n\n\
- `[ ]` **P6** the writer\n  `files: src/a.rs`\n\n\
- `[ ]` **S14** the banding\n  `files: src/b.rs`\n\n\
## Bugs\n\n\
## Next\n\n\
## Done\n";
    const NOTE: &str = "task: P6\ngoal: the writer\n\nsteps:\n  [x] the layout\n  [ ] the writer\n\n\
now: the writer\nblocked: nothing\nrun: cargo test\nresume: the writer\n";

    /// A backlog that declares its phases the canonical way and holds one task of each.
    const LEGEND: &str = "# Backlog\n\nIntro prose.\n\n\
Phases come from the shape-base plan; `cargo xtask task phase` owns this list:\n\
`P` — the plan and `xtask` itself\n\
`S` — a structural refactor that changes module boundaries only\n\n\
## Now\n\n\
- `[ ]` **P6** the writer\n  `files: src/a.rs`\n\n\
- `[ ]` **S14** the banding\n  `files: src/b.rs`\n\n\
## Bugs\n\n\
## Next\n\n\
## Done\n";

    #[test]
    fn a_phase_is_declared_and_then_updated_by_one_verb() {
        let root = project("phase", STATUS, LEGEND, None);
        run(&root, &["phase", "K", "--name", "the order package"]).expect("K is declared");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(backlog.contains("`K` — the order package\n"), "the clause added: {backlog}");
        assert!(backlog.contains("`P` — the plan and `xtask` itself\n"), "the others stayed: {backlog}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");

        run(&root, &["phase", "K", "--name", "the order package and its money"]).expect("K is updated");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(backlog.contains("`K` — the order package and its money\n"), "{backlog}");
        assert!(!backlog.contains("`K` — the order package\n"), "the old clause is gone: {backlog}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn several_phases_travel_in_one_write() {
        let root = project("phase-many", STATUS, LEGEND, None);
        run(&root, &["phase", "--declare", "K=the order package", "--declare", "R=the pre-release rename"])
            .expect("both letters are declared");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(backlog.contains("`K` — the order package\n"), "{backlog}");
        assert!(backlog.contains("`R` — the pre-release rename\n"), "{backlog}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_partial_legend_that_drops_a_used_phase_is_refused() {
        // The prose legend declares P and K; rewriting it with P alone would leave task K4 unexplained.
        let prose = LEGEND
            .replace(
                "`P` — the plan and `xtask` itself\n`S` — a structural refactor that changes module boundaries only",
                "P is the plan, K is the order package",
            )
            .replace("**S14** the banding", "**K4** the package");
        let root = project("phase-drop", STATUS, &prose, None);
        let before = std::fs::read_to_string(root.join("plan/BACKLOG.md")).expect("the backlog is readable");
        let error = run(&root, &["phase", "P", "--name", "the plan"]).expect_err("K would lose its clause");
        assert!(error.contains("nothing was written"), "{error}");
        // The sentence itself belongs to the gate, so it is asserted there: this verb only refuses.
        assert!(
            crate::check_with(&root, &FakeGit).is_err(),
            "the fixture's prose legend is what the gate reports, and the rewrite would have left K undeclared"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("plan/BACKLOG.md")).expect("readable"),
            before,
            "a refusal writes nothing"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_task_in_an_undeclared_phase_is_refused_unless_it_names_one() {
        let root = project("phase-add", STATUS, LEGEND, None);
        let error =
            run(&root, &["add", "--phase", "K", "--files", "src/a.rs", "the package"]).expect_err("K is not declared");
        assert!(error.contains("task phase K"), "the sentence names the verb: {error}");

        run(&root, &["add", "--phase", "K", "--phase-name", "the order package", "--files", "src/a.rs", "the package"])
            .expect("the declaration travels with the task");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(backlog.contains("`K` — the order package\n"), "{backlog}");
        assert!(backlog.contains("**K1** the package"), "the task landed in its phase: {backlog}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_phase_is_one_capital_letter() {
        let root = project("phase-letter", STATUS, LEGEND, None);
        let error = run(&root, &["phase", "KK", "--name", "two letters"]).expect_err("two letters is no phase");
        assert!(error.contains("one capital letter"), "{error}");
        // A lowercase letter is read as the capital it names, the same way `add --phase` reads one.
        run(&root, &["phase", "k", "--name", "lowercase but read as K"]).expect("a lowercase letter names its phase");
        assert!(text(&root, "plan/BACKLOG.md").contains("`K` — lowercase but read as K\n"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A scratch project: two plan files, two source files, a note only when the test needs one.
    ///
    /// Every call gets its own directory. Two tests may name the same fixture — `close-last` is
    /// used twice — and a shared root is removed under the other test's feet, which lands as an
    /// `ENOENT` from a `create_dir_all` that has nothing to do with the behaviour under test.
    fn project(name: &str, status: &str, backlog: &str, session: Option<&str>) -> PathBuf {
        static SCRATCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = SCRATCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("xtask-write-{name}-{}-{serial}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("plan/notes")).expect("the scratch plan is writable");
        std::fs::create_dir_all(root.join("src")).expect("the scratch sources are writable");
        std::fs::write(root.join("plan/STATUS.md"), status).expect("STATUS is writable");
        std::fs::write(root.join("plan/BACKLOG.md"), backlog).expect("BACKLOG is writable");
        std::fs::write(root.join("src/a.rs"), "fn a() {}\n").expect("a.rs is writable");
        std::fs::write(root.join("src/b.rs"), "fn b() {}\n").expect("b.rs is writable");
        if let Some(session) = session {
            std::fs::write(root.join("plan/SESSION.md"), session).expect("SESSION is writable");
        }
        root
    }

    fn run(root: &Path, args: &[&str]) -> Result<(), String> {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        run_with(root, &args, &FakeGit)
    }

    fn text(root: &Path, path: &str) -> String {
        std::fs::read_to_string(root.join(path)).unwrap_or_default()
    }

    /// Every file under `plan/`, so a refusal can be checked byte for byte.
    fn snapshot(root: &Path) -> Vec<(String, String)> {
        let mut files: Vec<(String, String)> = std::fs::read_dir(root.join("plan"))
            .expect("the plan directory is readable")
            .flatten()
            .filter(|entry| entry.path().is_file())
            .map(|entry| {
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    text(root, &format!("plan/{}", entry.file_name().to_string_lossy())),
                )
            })
            .collect();
        files.sort();
        files
    }

    #[test]
    fn a_repair_passes_only_a_write_that_shrinks_the_violations() {
        let stale = |left: &str, right: &str| format!("before {left} after {right}");
        let (before, after) = (vec![stale("a", "b")], vec![stale("a", "c")]);
        assert!(!shrinks(&before, &after), "a trade is not a repair");
        assert!(!shrinks(&before, &before), "a write that fixes nothing is not a repair");
        let two = vec![stale("a", "b"), stale("c", "d")];
        assert!(shrinks(&two, &[stale("c", "d")]), "one violation gone, none new");
        assert!(!shrinks(&two, &[stale("c", "d"), stale("e", "f")]), "one gone, one new is a trade");
        assert!(shrinks(&two, &Vec::new()), "an empty plan is a strict subset");
    }

    #[test]
    fn a_verb_repairs_a_plan_with_several_violations_only_when_told_to() {
        // Two hints name files that are gone: one verb can fix one of them.
        let backlog = BACKLOG.replace("src/a.rs", "src/gone.rs").replace("src/b.rs", "src/also_gone.rs");
        let root = project("repair", STATUS, &backlog, None);
        let before = snapshot(&root);
        let error =
            run(&root, &["set", "P6", "--field", "files", "--files", "src/a.rs"]).expect_err("two violations stand");
        assert!(error.contains("refused"), "{error}");
        assert_eq!(snapshot(&root), before, "a refused repair writes nothing");

        run(&root, &["set", "P6", "--field", "files", "--files", "src/a.rs", "--repair"])
            .expect("one violation goes, none comes");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(backlog.contains("`files: src/a.rs`"), "the hint is fixed: {backlog}");
        let remaining = crate::check_with(&root, &FakeGit).expect_err("the other violation stands");
        assert_eq!(remaining.len(), 1, "{remaining:?}");
        assert!(remaining[0].contains("also_gone.rs"), "{remaining:?}");

        // The last one is repairable the same way, and then the plan is valid again.
        run(&root, &["set", "S14", "--field", "files", "--files", "src/b.rs", "--repair"])
            .expect("the second violation goes");
        crate::check_with(&root, &FakeGit).expect("the plan is whole again");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn starting_a_task_marks_it_and_writes_the_note() {
        let root = project("start", STATUS, BACKLOG, None);
        run(&root, &["start", "P6"]).expect("P6 starts");
        assert!(text(&root, "plan/BACKLOG.md").contains("- `[~]` **P6** the writer"), "the marker moved");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=P6 state=doing"), "{status}");
        let session = text(&root, "plan/SESSION.md");
        assert!(session.contains("task: P6") && session.contains("now: "), "{session}");
        crate::check_with(&root, &FakeGit).expect("the plan is still valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_start_while_another_task_runs_is_refused_and_writes_nothing() {
        let running = STATUS.replace("state=idle", "state=doing");
        let backlog = BACKLOG.replace("- `[ ]` **P6**", "- `[~]` **P6**");
        let root = project("start-refused", &running, &backlog, Some(NOTE));
        let before = snapshot(&root);
        let error = run(&root, &["start", "S14"]).expect_err("P6 is still in progress");
        assert!(error.contains("still in progress"), "{error}");
        assert_eq!(snapshot(&root), before, "a refused verb writes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_close_lands_the_commit_moves_the_entry_and_drops_the_note() {
        let running = STATUS.replace("state=idle", "state=doing");
        let backlog = BACKLOG.replace("- `[ ]` **P6**", "- `[~]` **P6**");
        let root = project("close", &running, &backlog, Some(NOTE));
        run(&root, &["close", "P6"]).expect("P6 closes");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(backlog.contains("- `[x]` **P6** the writer"), "{backlog}");
        assert!(backlog.contains("abc1234"), "the commit is recorded: {backlog}");
        let done = backlog.find("## Done").expect("Done is there");
        assert!(backlog[done..].contains("**P6**"), "the entry moved to Done: {backlog}");
        assert!(!backlog[..done].contains("**P6**"), "and left Now: {backlog}");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=S14 state=idle"), "{status}");
        assert!(status.contains("base=abc1234"), "the plan describes the checkout: {status}");
        assert!(!root.join("plan/SESSION.md").exists(), "the note is gone");
        crate::check_with(&root, &FakeGit).expect("the plan is still valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_new_task_joins_next_and_leaves_the_order_alone() {
        let root = project("add", STATUS, BACKLOG, None);
        run(&root, &["add", "--phase", "S", "--files", "src/b.rs", "--why", "1446 lines", "the pipelines move"])
            .expect("a task is added");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(backlog.contains("- `[ ]` **S15** the pipelines move"), "{backlog}");
        assert!(backlog.contains("  Why: 1446 lines"), "{backlog}");
        let added = backlog.find("**S15**").expect("S15 is there");
        let next = backlog.find("## Next").expect("Next is there");
        assert!(added > next, "the new task joins `## Next`: {backlog}");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=P6"), "the current task does not change: {status}");
        crate::check_with(&root, &FakeGit).expect("the plan is still valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_section_named_beside_the_end_puts_the_task_there() {
        let root = project("add-now", STATUS, BACKLOG, None);
        run(
            &root,
            &["add", "--section", "Now", "--at", "end", "--phase", "S", "--files", "src/b.rs", "the pipelines move"],
        )
        .expect("a task joins the order");
        let backlog = text(&root, "plan/BACKLOG.md");
        let added = backlog.find("**S15**").expect("S15 is there");
        let last = backlog.find("**S14**").expect("S14 is there");
        let bugs = backlog.find("## Bugs").expect("Bugs is there");
        assert!(added > last && added < bugs, "the new task joins the end of the order: {backlog}");
        crate::check_with(&root, &FakeGit).expect("the plan is still valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_field_and_a_files_hint_are_set_in_place() {
        let root = project("set", STATUS, BACKLOG, None);
        run(&root, &["set", "P6", "--field", "Done", "--body", "a test per refusal"]).expect("the field is set");
        run(&root, &["set", "P6", "--field", "files", "--files", "src/a.rs, src/b.rs"]).expect("the hint is set");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(backlog.contains("  Done: a test per refusal"), "{backlog}");
        assert!(backlog.contains("  `files: src/a.rs, src/b.rs`"), "{backlog}");
        crate::check_with(&root, &FakeGit).expect("the plan is still valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_field_label_typed_in_another_case_lands_in_the_declared_spelling() {
        // P21: the label was written as typed, and the gate then read `shape:` as the field.
        let root = project("set-case", STATUS, BACKLOG, None);
        run(&root, &["set", "P6", "--field", "shape", "--body", "one body"]).expect("the field is set");
        run(&root, &["set", "P6", "--field", "FILES", "--files", "src/b.rs"]).expect("the hint is set");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(backlog.contains("  Shape: one body"), "the declared spelling: {backlog}");
        assert!(!backlog.contains("shape:"), "no second spelling of the same field: {backlog}");
        assert!(backlog.contains("  `files: src/b.rs`"), "`--field FILES` edits the hint: {backlog}");
        assert!(!backlog.contains("FILES:"), "and never writes a second hint: {backlog}");
        crate::check_with(&root, &FakeGit).expect("the plan is still valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_verb_on_an_unknown_task_is_refused_and_writes_nothing() {
        let root = project("unknown", STATUS, BACKLOG, None);
        let before = snapshot(&root);
        for verb in
            [&["start", "S99"][..], &["set", "S99", "--field", "Why", "--body", "x"], &["note", "S99", "--body", "x"]]
        {
            let error = run(&root, verb).expect_err("S99 does not exist");
            assert!(error.contains("no task `S99`") || error.contains("has no task"), "{verb:?}: {error}");
            assert_eq!(snapshot(&root), before, "{verb:?} writes nothing");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_note_is_created_with_a_heading_and_appended_to() {
        let root = project("note", STATUS, BACKLOG, None);
        run(&root, &["note", "P6", "--body", "the layout scanner\n\nit walks the lines"]).expect("the note is added");
        let note = text(&root, "plan/notes/P6.md");
        assert!(note.starts_with("# P6 — the layout scanner\n"), "{note}");
        run(&root, &["note", "P6", "--body", "a second thought"]).expect("the note is appended");
        let note = text(&root, "plan/notes/P6.md");
        assert!(note.contains("a second thought"), "{note}");
        assert_eq!(note.matches("# P6").count(), 1, "the heading is written once: {note}");
        crate::check_with(&root, &FakeGit).expect("the plan is still valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_reopen_returns_the_task_to_the_top_of_now() {
        let done = BACKLOG.replace("- `[ ]` **P6** the writer\n  `files: src/a.rs`\n\n", "");
        let done = format!("{done}\n- `[x]` **P6** the writer\n  `files: src/a.rs` — `abc1234`\n");
        let status = STATUS.replace("task=P6", "task=S14");
        let root = project("reopen", &status, &done, None);
        run(&root, &["reopen", "P6"]).expect("P6 reopens");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(backlog.contains("- `[ ]` **P6** the writer\n"), "{backlog}");
        assert!(!backlog.contains("abc1234"), "the commit is gone: {backlog}");
        assert!(text(&root, "plan/STATUS.md").contains("task=P6 state=idle"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A backlog with a bug, a next-order entry and the current task.
    const FILED_BACKLOG: &str = "## Now\n\n\
- `[ ]` **P6** the writer\n  `files: src/a.rs`\n\n\
## Bugs\n\n\
- `[ ]` **X2** a defect\n  Why: it breaks on 1 µm\n  `files: src/b.rs`\n\n\
## Next\n\n\
- `[ ]` **R1** the rename\n  Why: the name is in three places\n  `files: src/a.rs`\n\n\
## Done\n";

    /// A plan that declares one stream beside its own, with the agent's head parked in `## Next`.
    const STREAM_STATUS: &str = "# Status\n\nSTATE: milestone=M1 phase=J task=- state=idle base=abc1234 \
                                  updated=2026-11-10\nSTREAMS: agent=J\n\n## Next\n\nNothing is scheduled: the \
                                  plan's own order is empty.\n";
    const STREAM_BACKLOG: &str = "## Now\n\n## Bugs\n\n## Next\n\n\
- `[ ]` **J2** the report\n  `files: src/b.rs`\n\n## Done\n";

    #[test]
    fn a_declared_stream_works_its_own_note_and_leaves_the_state_line_alone() {
        let root = project("stream-start", STREAM_STATUS, STREAM_BACKLOG, None);
        let before = text(&root, "plan/STATUS.md");
        run(&root, &["start", "J2"]).expect("the agent's head starts");
        assert!(text(&root, "plan/BACKLOG.md").contains("- `[~]` **J2**"), "the marker moved");
        let note = text(&root, "plan/SESSION.agent.md");
        assert!(note.contains("task: J2"), "the stream keeps its own note: {note}");
        assert!(!root.join("plan/SESSION.md").exists(), "the plan's own note is not touched");
        assert_eq!(text(&root, "plan/STATUS.md"), before, "the state line is the plan's own");

        run(&root, &["step", "J2", "--now", "the line parser"]).expect("the step lands in the stream's note");
        assert!(text(&root, "plan/SESSION.agent.md").contains("now: the line parser"));
        assert!(!root.join("plan/SESSION.md").exists(), "`step` writes the stream's note and no other");

        run(&root, &["close", "J2", "--commit", "abc1234"]).expect("the task closes");
        assert!(text(&root, "plan/BACKLOG.md").contains("- `[x]` **J2**"), "the entry is done");
        assert!(!root.join("plan/SESSION.agent.md").exists(), "the note goes with the session");
        assert_eq!(text(&root, "plan/STATUS.md"), before, "the state line is still the plan's own");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn one_stream_works_one_task_and_the_refusal_names_the_other_one() {
        // Both in the order: the refusal is the stream's, not the order's.
        let backlog = STREAM_BACKLOG
            .replace("## Now\n\n", "## Now\n\n- `[~]` **J1** the runner\n  `files: src/a.rs`\n\n")
            .replace("## Bugs", "- `[ ]` **J2** the report\n  `files: src/b.rs`\n\n## Bugs")
            .replace("- `[ ]` **J2** the report\n  `files: src/b.rs`\n\n## Done\n", "## Done\n");
        let note = "task: J1\ngoal: run a recipe\nnow: the parser\nrun: cargo test\n";
        let root = project("stream-two", STREAM_STATUS, &backlog, None);
        std::fs::write(root.join("plan/SESSION.agent.md"), note).expect("the stream's note is writable");
        let error = run(&root, &["start", "J2"]).expect_err("the agent stream already works J1");
        assert!(error.contains("still works the `agent` stream"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The same facts, in a checkout on `branch`: the plan beside the primary is one of these.
    struct BranchGit<'a> {
        facts: &'a FakeGit,
        branch: &'static str,
    }

    impl Git for BranchGit<'_> {
        fn head(&self) -> Option<String> {
            self.facts.head()
        }

        fn commit_exists(&self, hash: &str) -> bool {
            self.facts.commit_exists(hash)
        }

        fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
            self.facts.is_ancestor(ancestor, descendant)
        }

        fn commit_date(&self, hash: &str) -> Option<String> {
            self.facts.commit_date(hash)
        }

        fn short(&self, hash: &str) -> Option<String> {
            self.facts.short(hash)
        }

        fn subject(&self, hash: &str) -> Option<String> {
            self.facts.subject(hash)
        }

        fn branch(&self) -> Option<String> {
            Some(self.branch.to_owned())
        }
    }

    fn run_on(root: &Path, branch: &'static str, args: &[&str]) -> Result<(), String> {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        let git = BranchGit { facts: &FakeGit, branch };
        run_with(root, &args, &git)
    }

    #[test]
    fn a_stream_checkout_writes_no_shared_plan_file() {
        let root = project("stream-shared", STREAM_STATUS, STREAM_BACKLOG, None);
        let before = snapshot(&root);
        let cases: &[(&str, &[&str])] = &[
            ("task start", &["start", "J2"]),
            ("task close", &["close", "J2", "--commit", "abc1234"]),
            ("task add", &["add", "--phase", "J", "--files", "src/a.rs", "a thing"]),
            ("task set", &["set", "J2", "--field", "Why", "--body", "because"]),
            ("task take", &["take", "J2"]),
            ("state set", &["state", "set", "updated=2026-11-10"]),
            ("file", &["file", "plan/map.md", "--body", "| a | b |"]),
        ];
        for (verb, args) in cases {
            let error = run_on(&root, "stream/agent", args).expect_err(verb);
            assert!(error.contains("`stream/agent`"), "{verb} names the checkout: {error}");
            assert!(error.contains("primary checkout"), "{verb} names the writer: {error}");
            assert!(error.contains("plan/SESSION.agent.md"), "{verb} names the stream's own file: {error}");
        }
        assert_eq!(snapshot(&root), before, "not one byte of the plan is written");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stream_checkout_keeps_its_own_note_and_refuses_another_streams() {
        // The stream's own task runs, its note and its research note are the two files it writes.
        let backlog = STREAM_BACKLOG
            .replace("## Now\n\n", "## Now\n\n- `[~]` **J2** the report\n  `files: src/b.rs`\n\n")
            .replace("- `[ ]` **J2** the report\n  `files: src/b.rs`\n\n## Done\n", "## Done\n");
        let status = STREAM_STATUS.replace("STREAMS: agent=J", "STREAMS: agent=J render=Y");
        let note = "task: J2\ngoal: the report\n\nsteps:\n  [ ] the report\n\nnow: the report\nblocked: nothing\n\
                    run: cargo test\nresume: src/b.rs\n";
        let root = project("stream-own", &status, &backlog, None);
        std::fs::write(root.join("plan/SESSION.agent.md"), note).expect("the stream's note is writable");
        let order = text(&root, "plan/BACKLOG.md");

        run_on(&root, "stream/agent", &["step", "J2", "--now", "the table"]).expect("its own step lands");
        assert!(text(&root, "plan/SESSION.agent.md").contains("now: the table"), "the step is in the note");
        assert_eq!(text(&root, "plan/BACKLOG.md"), order, "the shared order is untouched");
        run_on(&root, "stream/agent", &["note", "J2", "--body", "what the renderer measured"])
            .expect("its own research note lands");
        assert!(text(&root, "plan/notes/J2.md").contains("what the renderer measured"));

        let error = run_on(&root, "stream/agent", &["step", "Y1", "--now", "someone else's step"])
            .expect_err("Y1 is the render stream's task");
        assert!(error.contains("`render` stream"), "{error}");
        assert!(error.contains("`stream/agent`"), "{error}");
        run_on(&root, "stream/agent", &["note", "Y1", "--body", "not mine"]).expect_err("not this stream's note");
        assert!(!root.join("plan/notes/Y1.md").exists(), "another stream's note is not written");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stream_checkout_writes_the_note_its_primary_started_elsewhere() {
        // J15/P28: the primary ran `task start J2` in its own working tree, so the `[~]` marker
        // arrived through `dev` and the note — a working-tree file of another checkout — did not.
        // The entry carries the goal, so the stream's `step` writes the note instead of refusing.
        let backlog = STREAM_BACKLOG
            .replace("## Now\n\n", "## Now\n\n- `[~]` **J2** the report\n  `files: src/b.rs`\n\n")
            .replace("- `[ ]` **J2** the report\n  `files: src/b.rs`\n\n## Done\n", "## Done\n");
        let root = project("stream-note-lost", STREAM_STATUS, &backlog, None);
        assert!(!root.join("plan/SESSION.agent.md").exists(), "the stream's copy holds no note");
        run_on(&root, "stream/agent", &["step", "J2", "--now", "the table", "--resume", "src/b.rs"])
            .expect("the running task's note is written from its entry");
        let note = text(&root, "plan/SESSION.agent.md");
        assert!(note.contains("task: J2"), "{note}");
        assert!(note.contains("goal: the report"), "the entry's lead is the goal: {note}");
        assert!(note.contains("now: the table"), "the step reaches the new note: {note}");
        assert!(note.contains("resume: src/b.rs"), "{note}");
        assert_eq!(text(&root, "plan/BACKLOG.md"), backlog, "the shared order is untouched");
        crate::check_with(&root, &FakeGit).expect("the plan is valid with the note it wrote");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_primary_checkout_writes_the_shared_files_it_always_did() {
        // `dev` is the plan's own branch: no `stream/` prefix, which is what the rule reads.
        let root = project("primary-writes", STREAM_STATUS, STREAM_BACKLOG, None);
        run_on(&root, "dev", &["start", "J2"]).expect("the primary starts a stream's task");
        assert!(text(&root, "plan/BACKLOG.md").contains("- `[~]` **J2**"), "the marker moved");
        assert!(!text(&root, "plan/STATUS.md").contains("task=J2"), "a stream's entry leaves the state line alone");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn taking_a_bug_moves_the_entry_whole() {
        let root = project("take-bug", STATUS, FILED_BACKLOG, None);
        run(&root, &["take", "X2"]).expect("the bug joins the end of Now");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(
            backlog.contains("- `[ ]` **X2** a defect\n  Why: it breaks on 1 µm\n  `files: src/b.rs`"),
            "the entry keeps its text, fields and hint: {backlog}"
        );
        let now = backlog.find("## Now").expect("Now is there");
        let bugs = backlog.find("## Bugs").expect("Bugs is there");
        let order = &backlog[now..bugs];
        let (current, taken) = (order.find("**P6**").expect("P6 stays"), order.find("**X2**").expect("X2 arrived"));
        assert!(current < taken, "the taken entry sits behind the current one: {order}");
        assert!(!backlog[bugs..].contains("**X2**"), "and left `## Bugs`: {backlog}");
        assert!(
            text(&root, "plan/STATUS.md").contains("task=P6 state=idle"),
            "an occupied `## Now` keeps its `STATE:`"
        );
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn taking_into_an_empty_now_makes_the_task_current() {
        // The state a plan is left in when `## Now` runs out: no valid current task until an entry
        // is taken into the empty order, which is the write that repairs it.
        let status = format!("{}\n## Next\n\nP6 is next: the writer.\n", STATUS.replace("task=P6", "task=R1"));
        let backlog = FILED_BACKLOG.replace("- `[ ]` **P6** the writer\n  `files: src/a.rs`\n\n", "");
        let root = project("take-empty", &status, &backlog, None);
        assert!(crate::check_with(&root, &FakeGit).is_err(), "the plan starts with no valid current task");
        run(&root, &["take", "R1"]).expect("the take fills the empty order and repairs the plan");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=R1 state=idle"), "{status}");
        assert!(status.contains("R1 is next: the rename"), "the paragraph is rewritten: {status}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn taking_a_bug_at_the_top_makes_it_current() {
        // An owner-directed defect jumps the order: the top of `## Now` is the slot `STATE:` names,
        // so the take has to move the line in the same write or the gate refuses the result.
        let root = project("take-top-idle", &with_next(STATUS), FILED_BACKLOG, None);
        run(&root, &["take", "X2", "--at", "top"]).expect("the bug takes the head of the order");
        let backlog = text(&root, "plan/BACKLOG.md");
        let now = backlog.find("## Now").expect("Now is there");
        let bugs = backlog.find("## Bugs").expect("Bugs is there");
        let order = &backlog[now..bugs];
        let (taken, current) = (order.find("**X2**").expect("X2 arrived"), order.find("**P6**").expect("P6 stays"));
        assert!(taken < current, "the taken entry sits in front of the order: {order}");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=X2 state=idle"), "the taken bug is the current task: {status}");
        assert!(status.contains("X2 is next: a defect"), "the paragraph names it: {status}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn taking_a_done_or_unknown_task_is_refused() {
        let backlog =
            FILED_BACKLOG.replace("## Done\n", "## Done\n\n- `[x]` **X1** older\n  `files: src/a.rs` — `abc1234`\n");
        let root = project("take-refused", STATUS, &backlog, None);
        let before = snapshot(&root);
        let error = run(&root, &["take", "X1"]).expect_err("a done task is not taken");
        assert!(error.contains("is done"), "{error}");
        let error = run(&root, &["take", "X9"]).expect_err("an unknown id is refused");
        assert!(error.contains("no task `X9`"), "{error}");
        assert_eq!(snapshot(&root), before, "a refusal writes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn taking_at_the_top_while_a_task_runs_is_refused() {
        let running = STATUS.replace("state=idle", "state=doing");
        let backlog = FILED_BACKLOG.replace("- `[ ]` **P6**", "- `[~]` **P6**");
        let root = project("take-top", &running, &backlog, Some(NOTE));
        let before = snapshot(&root);
        let error = run(&root, &["take", "X2", "--at", "top"]).expect_err("the top belongs to the running task");
        assert!(error.contains("still in progress"), "{error}");
        assert_eq!(snapshot(&root), before, "a refusal writes nothing");

        run(&root, &["take", "X2"]).expect("the end of `## Now` does not disturb the running task");
        assert!(text(&root, "plan/STATUS.md").contains("task=P6 state=doing"));
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A status with the `## Next` paragraph the gate wants when `STATE:` moves.
    fn with_next(status: &str) -> String {
        format!("{status}\n## Next\n\nP6 is next: the writer.\n")
    }

    #[test]
    fn deferring_the_current_task_points_the_state_at_the_next_one() {
        let root = project("defer-head", &with_next(STATUS), BACKLOG, None);
        run(&root, &["defer", "P6"]).expect("P6 leaves the order");
        let backlog = text(&root, "plan/BACKLOG.md");
        let now = backlog.find("## Now").expect("Now is there");
        let next = backlog.find("## Next").expect("Next is there");
        assert!(!backlog[now..next].contains("**P6**"), "P6 left the order: {backlog}");
        assert!(
            backlog[next..].contains("- `[ ]` **P6** the writer\n  `files: src/a.rs`"),
            "the entry moves whole, fields and hint included: {backlog}"
        );
        assert!(backlog[now..next].contains("**S14**"), "the rest of the order is untouched: {backlog}");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=S14 state=idle"), "{status}");
        assert!(status.contains("S14 is next: the banding"), "the paragraph names the new head: {status}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deferring_a_later_task_leaves_the_state_alone() {
        let root = project("defer-later", &with_next(STATUS), BACKLOG, None);
        run(&root, &["defer", "S14"]).expect("S14 leaves the order");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=P6 state=idle"), "the head did not move: {status}");
        assert!(status.contains("P6 is next: the writer"), "the paragraph still names it: {status}");
        assert!(!text(&root, "plan/BACKLOG.md").contains("**S14** the banding\n  `files: src/b.rs`\n\n## Bugs"));
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_deferred_task_keeps_its_note_and_stops_being_scheduled() {
        let done = format!("{STATUS}\n## Next\n\nP6 is next: the writer.\n");
        let root = project("defer-note", &done, FILED_BACKLOG, None);
        run(&root, &["note", "X2", "--body", "a measured defect"]).expect("the bug has a note");
        run(&root, &["defer", "X2"]).expect_err("X2 is filed under `## Bugs`, not the order");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn deferring_a_running_or_unknown_task_is_refused() {
        let running = with_next(&STATUS.replace("state=idle", "state=doing"));
        let backlog = BACKLOG.replace("- `[ ]` **P6**", "- `[~]` **P6**");
        let root = project("defer-running", &running, &backlog, Some(NOTE));
        let before = snapshot(&root);
        let error = run(&root, &["defer", "P6"]).expect_err("a running task keeps its session");
        assert!(error.contains("in progress"), "{error}");
        assert_eq!(snapshot(&root), before, "a refusal writes nothing");

        let status = with_next(STATUS);
        let root = project("defer-unknown", &status, BACKLOG, None);
        let before = snapshot(&root);
        let error = run(&root, &["defer", "P9"]).expect_err("P9 does not exist");
        assert!(error.contains("no task `P9`"), "{error}");
        assert_eq!(snapshot(&root), before);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unstarting_the_current_task_keeps_its_place_in_the_order() {
        // The case the verb exists for: a started task turns out to wait on another one, and the
        // prerequisite has to go in front of it. A deferred task loses its place, so the order
        // could only put the prerequisite behind the task it unblocks.
        let running = with_next(&STATUS.replace("state=idle", "state=doing"));
        let backlog = BACKLOG.replace("- `[ ]` **P6**", "- `[~]` **P6**");
        let root = project("unstart-head", &running, &backlog, Some(NOTE));
        run(&root, &["unstart", "P6"]).expect("the session ends");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(backlog.contains("- `[ ]` **P6** the writer"), "the marker is open again: {backlog}");
        let now = backlog.find("## Now").expect("Now is there");
        let bugs = backlog.find("## Bugs").expect("Bugs is there");
        let order = &backlog[now..bugs];
        let (first, second) = (order.find("**P6**").expect("P6 stays"), order.find("**S14**").expect("S14 stays"));
        assert!(first < second, "the entry keeps its place in the order: {order}");
        assert!(!root.join("plan/SESSION.md").exists(), "the session note goes with the session");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=P6 state=idle"), "the head is the task that was stopped: {status}");
        assert!(status.contains("P6 is next: the writer"), "the paragraph still names it: {status}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unstarting_leaves_a_prerequisite_able_to_take_the_top() {
        // The whole point of keeping the place: the unstarted entry is still the head, so the task
        // that unblocks it can take the top of the order in front of it.
        let backlog = "## Now\n\n\
- `[~]` **P6** the writer\n  `files: src/a.rs`\n\n\
## Bugs\n\n\
## Next\n\n\
- `[ ]` **S14** the banding\n  `files: src/b.rs`\n\n\
## Done\n";
        let running = with_next(&STATUS.replace("state=idle", "state=doing"));
        let root = project("unstart-then-take", &running, backlog, Some(NOTE));
        run(&root, &["unstart", "P6"]).expect("the session ends");
        run(&root, &["take", "S14", "--at", "top"]).expect("the prerequisite takes the head");
        let backlog = text(&root, "plan/BACKLOG.md");
        let now = backlog.find("## Now").expect("Now is there");
        let bugs = backlog.find("## Bugs").expect("Bugs is there");
        let order = &backlog[now..bugs];
        let (prerequisite, stopped) =
            (order.find("**S14**").expect("S14 arrived"), order.find("**P6**").expect("P6 stays"));
        assert!(prerequisite < stopped, "the prerequisite stands in front of it: {order}");
        assert!(text(&root, "plan/STATUS.md").contains("task=S14 state=idle"));
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unstarting_an_open_or_unknown_task_is_refused() {
        let root = project("unstart-open", &with_next(STATUS), BACKLOG, None);
        let before = snapshot(&root);
        let error = run(&root, &["unstart", "P6"]).expect_err("P6 is not in progress");
        assert!(error.contains("is not in progress"), "{error}");
        let error = run(&root, &["unstart", "P9"]).expect_err("P9 does not exist");
        assert!(error.contains("no task `P9`"), "{error}");
        assert_eq!(snapshot(&root), before, "a refusal writes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A plan whose whole order is one open task, with a paragraph that names it.
    fn one_task_order() -> (&'static str, &'static str) {
        let backlog = "## Now\n\n- `[ ]` **P6** the writer\n  `files: src/a.rs`\n\n## Bugs\n\n## Next\n\n## Done\n";
        let status = "# Status\n\nSTATE: milestone=M1 phase=P task=P6 state=idle base=abc1234 updated=2026-11-10\n\n\
                      ## Next\n\nP6 is next: the writer\n";
        (status, backlog)
    }

    #[test]
    fn deferring_the_last_task_leaves_a_valid_empty_order() {
        let (status, backlog) = one_task_order();
        let root = project("defer-last", status, backlog, None);
        run(&root, &["defer", "P6"]).expect("the order may be empty");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=- state=idle"), "the state says no task is scheduled: {status}");
        assert!(status.contains("Nothing is scheduled"), "`## Next` says the order is empty: {status}");
        let backlog = text(&root, "plan/BACKLOG.md");
        let next = backlog.find("## Next").expect("Next is there");
        assert!(backlog[next..].contains("**P6**"), "the entry is parked under `## Next`: {backlog}");
        assert!(!backlog[..next].contains("**P6**"), "and it left the order: {backlog}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn closing_the_last_task_leaves_a_valid_empty_order() {
        let (status, backlog) = one_task_order();
        let root = project("close-last", status, backlog, None);
        run(&root, &["close", "P6", "--commit", "abc1234"]).expect("the order may be empty");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=- state=idle"), "{status}");
        assert!(status.contains("Nothing is scheduled"), "{status}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn taking_a_parked_task_restores_the_current_one() {
        let (status, backlog) = one_task_order();
        let root = project("take-into-empty", status, backlog, None);
        run(&root, &["defer", "P6"]).expect("the order is empty");
        run(&root, &["take", "P6"]).expect("the parked task claims the order");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=P6 state=idle"), "{status}");
        assert!(status.contains("P6 is next"), "the paragraph names it again: {status}");
        assert!(!status.contains("Nothing is scheduled"), "{status}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_order_cannot_still_hold_a_task() {
        // `task=-` is the empty order; the gate refuses it while `## Now` has an unfinished task.
        let status = "# Status\n\nSTATE: milestone=M1 phase=P task=- state=idle base=abc1234 updated=2026-11-10\n\n\
                      ## Next\n\nNothing is scheduled: 1 open task waits in `## Next`\n";
        let root = project("empty-with-task", status, BACKLOG, None);
        let error = crate::check_with(&root, &FakeGit).expect_err("P6 is still in the order");
        assert!(
            error.iter().any(|violation| violation.contains("task=-") && violation.contains("still holds P6")),
            "{error:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_order_needs_the_sentence_that_says_so() {
        let (_, backlog) = one_task_order();
        let parked =
            backlog.replace("- `[ ]` **P6** the writer\n  `files: src/a.rs`", "").replace("## Now\n\n", "## Now\n");
        let status = "# Status\n\nSTATE: milestone=M1 phase=P task=- state=idle base=abc1234 updated=2026-11-10\n\n\
                      ## Next\n\nP6 waits for the owner.\n";
        let root = project("empty-paragraph", status, &parked, None);
        let error = crate::check_with(&root, &FakeGit).expect_err("the paragraph names a parked task");
        assert!(error.iter().any(|violation| violation.contains("Nothing is scheduled")), "{error:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_current_task_cannot_be_dropped() {
        let root = project("drop-current", STATUS, BACKLOG, None);
        let before = snapshot(&root);
        let error = run(&root, &["drop", "P6"]).expect_err("the current task stays");
        assert!(error.contains("current task"), "{error}");
        assert_eq!(snapshot(&root), before);
        run(&root, &["drop", "S14"]).expect("S14 is not current");
        assert!(!text(&root, "plan/BACKLOG.md").contains("S14"), "the entry is gone");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_drop_keeps_the_running_sessions_note() {
        // A live note names the task its stream works. Dropping another entry of the same stream is
        // not dropping that task, so the note stays: removing it left the gate reading a doing plan
        // with no session note, and refused a write that had nothing to do with the running task
        // (U119).
        let running = STATUS.replace("state=idle", "state=doing");
        let backlog = BACKLOG.replace("- `[ ]` **P6**", "- `[~]` **P6**");
        let root = project("drop-running", &running, &backlog, Some(NOTE));
        run(&root, &["drop", "S14"]).expect("another entry of the stream is dropped");
        assert!(!text(&root, "plan/BACKLOG.md").contains("S14"), "the entry is gone");
        assert!(text(&root, "plan/SESSION.md").contains("task: P6"), "the running session keeps its note");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_start_on_a_running_task_does_not_rewrite_its_note() {
        let running = STATUS.replace("state=idle", "state=doing");
        let backlog = BACKLOG.replace("- `[ ]` **P6**", "- `[~]` **P6**");
        let root = project("start-again", &running, &backlog, Some(NOTE));
        let before = snapshot(&root);
        let error = run(&root, &["start", "P6"]).expect_err("a running task is not restarted");
        assert!(error.contains("already in progress") && error.contains("--force"), "{error}");
        assert_eq!(snapshot(&root), before, "a refusal writes nothing");
        run(&root, &["start", "P6", "--force"]).expect("--force rewrites the note");
        assert!(text(&root, "plan/SESSION.md").contains("now: "), "the note is rebuilt");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn closing_a_done_task_and_reopening_an_open_one_are_refused() {
        let done = STATUS.replace("task=P6", "task=S14");
        let backlog = BACKLOG.replace("- `[ ]` **P6** the writer\n  `files: src/a.rs`\n\n", "");
        let backlog = format!("{backlog}\n- `[x]` **P6** the writer\n  `files: src/a.rs` — `abc1234`\n");
        let root = project("transitions", &done, &backlog, None);
        let before = snapshot(&root);
        let error = run(&root, &["close", "P6"]).expect_err("P6 is already done");
        assert!(error.contains("already done"), "{error}");
        assert_eq!(snapshot(&root), before);

        let error = run(&root, &["reopen", "S14"]).expect_err("S14 is not done");
        assert!(error.contains("is not done"), "{error}");
        assert_eq!(snapshot(&root), before, "a refusal writes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_reopen_that_moves_the_current_task_names_the_next_paragraph() {
        let done = BACKLOG.replace("- `[ ]` **P6** the writer\n  `files: src/a.rs`\n\n", "");
        let done = format!("{done}\n- `[x]` **P6** the writer\n  `files: src/a.rs` — `abc1234`\n");
        let status = format!("{}\n## Next\n\nS14 is next: the banding.\n", STATUS.replace("task=P6", "task=S14"));
        let root = project("reopen-next", &status, &done, None);
        let before = snapshot(&root);
        let error = run(&root, &["reopen", "P6"]).expect_err("the paragraph names another task");
        assert!(error.contains("--next") && error.contains("P6"), "{error}");
        assert_eq!(snapshot(&root), before, "a refusal writes nothing");

        run(&root, &["reopen", "P6", "--next", "P6 is back: the writer."]).expect("both atoms in one write");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=P6 state=idle"), "{status}");
        let next = status.split("## Next").nth(1).unwrap_or_default();
        assert!(next.contains("P6 is back"), "the paragraph moved: {status}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_task_added_at_the_top_becomes_the_current_one() {
        let status = format!("{}\n## Next\n\nP6 is next: the writer.\n", STATUS);
        let root = project("add-top", &status, BACKLOG, None);
        run(&root, &["add", "--phase", "X", "--at", "top", "--files", "src/b.rs", "an interjection"])
            .expect("the task joins the head of the order");
        let backlog = text(&root, "plan/BACKLOG.md");
        let added = backlog.find("**X1**").expect("X1 is there");
        let first = backlog.find("**P6**").expect("P6 is there");
        assert!(added < first, "the new task is first in `## Now`: {backlog}");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=X1 state=idle"), "the plan points at it: {status}");
        assert!(status.contains("X1 is next"), "the paragraph follows: {status}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");

        // A task that cannot be the current one may not take the top of a section.
        let error =
            run(&root, &["add", "--phase", "X", "--at", "top", "--section", "Bugs", "--files", "src/b.rs", "a defect"])
                .expect_err("only `## Now` orders the work");
        assert!(error.contains("`Now`"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_next_paragraph_moves_with_the_task_it_names() {
        // An interrupted session: `STATE:` still names P6, which already landed, and the paragraph
        // has to move to S14 with it — one write, or the gate refuses either half.
        let done = BACKLOG.replace("- `[ ]` **P6** the writer\n  `files: src/a.rs`\n\n", "");
        let done = format!("{done}\n- `[x]` **P6** the writer\n  `files: src/a.rs` — `abc1234`\n");
        let status = format!("{}\n## Next\n\nP6 is next: the writer.\n", STATUS);
        let root = project("state-next", &status, &done, None);
        let before = snapshot(&root);
        let error =
            state(&root, &["next".to_owned(), "--body".to_owned(), "S14 is next: the banding.".to_owned()], &FakeGit)
                .expect_err("the paragraph must name the current task");
        assert!(error.contains("refused"), "{error}");
        assert_eq!(snapshot(&root), before);

        state(
            &root,
            &[
                "next".to_owned(),
                "--body".to_owned(),
                "S14 is next: the banding.".to_owned(),
                "--task".to_owned(),
                "S14".to_owned(),
            ],
            &FakeGit,
        )
        .expect("the paragraph and the current task move together");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=S14"), "{status}");
        assert!(status.contains("S14 is next"), "{status}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_where_we_are_prose_is_written_by_a_command() {
        let status = format!("{}\n## Where we are\n\nthe old tree.\n\n## Next\n\nP6 is next.\n", STATUS);
        let root = project("state-where", &status, BACKLOG, None);
        state(&root, &["where".to_owned(), "--body".to_owned(), "the tree at abc1234.".to_owned()], &FakeGit)
            .expect("the prose is replaced");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("the tree at abc1234."), "{status}");
        assert!(!status.contains("the old tree."), "{status}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_reference_file_is_written_through_the_gate() {
        let root = project("file", STATUS, BACKLOG, None);
        file(
            &root,
            &["plan/map.md".to_owned(), "--body".to_owned(), "# Map\n\n| Feature | Where |\n|---|---|\n".to_owned()],
            &FakeGit,
        )
        .expect("a reference file is written");
        assert!(text(&root, "plan/map.md").contains("| Feature |"), "the file landed");

        let error = file(&root, &["plan/BACKLOG.md".to_owned(), "--body".to_owned(), "gone\n".to_owned()], &FakeGit)
            .expect_err("the task verbs own the backlog");
        assert!(error.contains("the `task` verbs"), "the refusal names the verb that owns it: {error}");
        assert!(text(&root, "plan/BACKLOG.md").contains("**P6**"), "the backlog is untouched");

        // `plan/STATUS.md` was the dead end P22 walked into: refused by `file`, and no help named the
        // verb that would take it.
        let error =
            file(&root, &["plan/STATUS.md".to_owned(), "--body".to_owned(), "STATE: gone\n".to_owned()], &FakeGit)
                .expect_err("the state and task verbs own the status line");
        assert!(error.contains("the `state` and `task` verbs"), "{error}");
        assert!(text(&root, "plan/STATUS.md").contains("STATE:"), "the state line is untouched");

        let error = file(&root, &["plan/debt.md".to_owned(), "--body".to_owned(), "   \n".to_owned()], &FakeGit)
            .expect_err("an empty file says nothing");
        assert!(error.contains("would be empty"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_append_only_reference_file_refuses_a_body_that_drops_an_entry() {
        // X129: a session appending one entry passes the whole body, and the body it built held only
        // the entries it had in hand — the write then deleted every other one, and the gate saw a
        // shorter file that is still legal.
        let root = project("file-append", STATUS, BACKLOG, None);
        let write =
            |path: &str, body: &str| file(&root, &[path.to_owned(), "--body".to_owned(), body.to_owned()], &FakeGit);
        let debt = |entries: &str| {
            format!(
                "# Known debt\n\nGaps accepted for now.\n\nTags: `[engine]` commands, `[render]` lighting, `[plan]` the plan tool.\n\n{entries}"
            )
        };
        let first = debt(
            "- `[engine]` the first gap, in the words a person reads\n  and it wraps onto a second line.\n\n\
             - `[render]` the second gap.\n",
        );
        write("plan/debt.md", &first).expect("the file a command owns may be created");

        let before = snapshot(&root);
        let dropped = debt("- `[render]` the second gap.\n\n- `[engine]` a gap that arrived today.\n");
        let error = write("plan/debt.md", &dropped).expect_err("an entry the file holds would leave");
        assert!(error.contains("append-only"), "the refusal names the rule: {error}");
        assert!(
            error.contains("[engine] the first gap, in the words a person reads"),
            "the refusal quotes the entry's opening words: {error}"
        );
        assert!(error.contains("--replace"), "the refusal names the flag that resets it: {error}");
        assert_eq!(snapshot(&root), before, "a refusal writes nothing");

        // The same body lands when a person asks for the whole-file rewrite: that is how an entry
        // leaves once its fix is in.
        file(
            &root,
            &["plan/debt.md".to_owned(), "--body".to_owned(), dropped.clone(), "--replace".to_owned()],
            &FakeGit,
        )
        .expect("--replace rewrites the whole file");
        assert!(!text(&root, "plan/debt.md").contains("the first gap"), "the entry left");

        // A body that adds an entry while keeping every one the file holds lands: re-wrapping an
        // entry at another column is the same entry, so the guard does not cry wolf over a reflow.
        let added = debt(
            "- `[render]` the second gap.\n\n- `[engine]` a gap that arrived today.\n\n\
             - `[plan]` one more, wrapped\n  somewhere else.\n",
        );
        write("plan/debt.md", &added).expect("an entry added to a file is written");
        assert!(
            text(&root, "plan/debt.md").contains("one more, wrapped\n  somewhere else."),
            "the entry is there, in the wrapping the body asked for"
        );

        // `plan/questions.md` is the second append-only file, and it carries the same guard.
        write("plan/questions.md", "# Open questions\n\n- A question the owner answers.\n")
            .expect("the first body creates the file");
        let error = write("plan/questions.md", "# Open questions\n\n- Another question.\n")
            .expect_err("a question the file holds would leave");
        assert!(error.contains("A question the owner answers."), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_write_is_checked_against_the_text_it_writes() {
        // P27: the gate read `plan/map.md` from the disk, so `file` validated the map it replaced and
        // a row naming a file no branch held landed as "valid". The reference files now enter the
        // checked plan the way the notes do.
        let root = project("file-map", STATUS, BACKLOG, None);
        let map = |path: &str| {
            format!("# Map\n\n| Feature | Where | Notes |\n|---|---|---|\n| Src | `{path}` (`Src`) | whole file |\n")
        };
        let write =
            |body: &str| file(&root, &["plan/map.md".to_owned(), "--body".to_owned(), body.to_owned()], &FakeGit);
        write(&map("src/a.rs")).expect("a row beside the tree that holds it lands");

        let error = write(&map("src/gone.rs")).expect_err("a row naming a path the workspace does not hold");
        assert!(error.contains("refused"), "the gate refused the write: {error}");
        assert!(text(&root, "plan/map.md").contains("src/a.rs"), "the refused write left the map alone");

        // The sentence the gate printed names the row, so the guard is the one that reported it.
        let (status, backlog) = (text(&root, "plan/STATUS.md"), text(&root, "plan/BACKLOG.md"));
        let references = [("plan/map.md".to_owned(), map("src/gone.rs"))];
        let plan = crate::PlanText {
            status: &status,
            backlog: &backlog,
            sessions: &[],
            notes: None,
            references: Some(&references),
        };
        let violations = crate::check_text(&root, &plan, &FakeGit).expect_err("the row is checked");
        assert!(
            violations.iter().any(|violation| violation.contains("src/gone.rs")),
            "the violation names the row's path: {violations:?}"
        );

        std::fs::write(root.join("src/gone.rs"), "fn gone() {}\n").expect("the file the row will name");
        write(&map("src/gone.rs")).expect("the same row passes beside a tree that holds the file");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_step_without_a_note_says_which_verb_writes_one_and_refuses_a_task_that_is_not_running() {
        let root = project("step-none", STATUS, BACKLOG, None);
        let before = snapshot(&root);
        let error = run(&root, &["step", "P6", "--tick"]).expect_err("there is no note");
        assert!(error.contains("`plan/SESSION.md` is not there"), "{error}");
        assert!(error.contains("not in progress"), "{error}");
        assert!(error.contains("cargo xtask task start P6"), "the refusal names the verb: {error}");
        assert_eq!(snapshot(&root), before);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_dry_run_reports_and_writes_nothing() {
        let root = project("dry", STATUS, BACKLOG, None);
        let before = snapshot(&root);
        run(&root, &["start", "P6", "--dry-run"]).expect("the dry run succeeds");
        assert_eq!(snapshot(&root), before, "--dry-run writes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_state_field_is_set_one_at_a_time() {
        let root = project("state", STATUS, BACKLOG, None);
        state(&root, &["set".to_owned(), "phase=A".to_owned()], &FakeGit).expect("phase moves to A");
        assert!(text(&root, "plan/STATUS.md").contains("phase=A"));
        crate::check_with(&root, &FakeGit).expect("the plan is still valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stream_is_declared_by_the_one_verb_that_owns_the_line() {
        let root = project("streams", STATUS, BACKLOG, None);
        state(&root, &["streams".to_owned(), "render=Y,L".to_owned()], &FakeGit).expect("a stream joins");
        assert!(text(&root, "plan/STATUS.md").contains("STREAMS: render=Y,L"), "{}", text(&root, "plan/STATUS.md"));
        assert!(text(&root, "plan/STATUS.md").contains("task=P6"), "the state line keeps its task");

        // An unusable declaration is a sentence, not a write.
        let before = snapshot(&root);
        let error =
            state(&root, &["streams".to_owned(), "Render=Y".to_owned()], &FakeGit).expect_err("a key is lowercase");
        assert!(error.contains("is not a key"), "{error}");
        assert_eq!(snapshot(&root), before, "a refused declaration writes nothing");

        // A subcommand the verb does not have names the ones it does, `streams` among them.
        let error = state(&root, &["nonsense".to_owned()], &FakeGit).expect_err("there is no such subcommand");
        assert!(error.contains("streams"), "the usage names the declaration: {error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_session_step_is_ticked_and_added() {
        let running = STATUS.replace("state=idle", "state=doing");
        let backlog = BACKLOG.replace("- `[ ]` **P6**", "- `[~]` **P6**");
        let root = project("step", &running, &backlog, Some(NOTE));
        run(&root, &["step", "P6", "--tick"]).expect("the step is ticked");
        assert!(text(&root, "plan/SESSION.md").contains("  [x] the writer"));
        run(&root, &["step", "P6", "--add", "the verbs"]).expect("a step is added");
        assert!(text(&root, "plan/SESSION.md").contains("  [ ] the verbs"));
        crate::check_with(&root, &FakeGit).expect("the plan is still valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_running_session_keeps_its_own_now_and_resume_true() {
        // `task set --field now` writes the backlog and `task start` refuses a running task, so a
        // session that stops mid-step could not leave a true resume hint (the `[plan]` debt entry).
        let running = STATUS.replace("state=idle", "state=doing");
        let backlog = BACKLOG.replace("- `[ ]` **P6**", "- `[~]` **P6**");
        let root = project("step-fields", &running, &backlog, Some(NOTE));
        run(&root, &["step", "P6", "--now", "the second verb", "--resume", "the first refusal"])
            .expect("the fields are rewritten");
        let session = text(&root, "plan/SESSION.md");
        assert!(session.contains("now: the second verb"), "{session}");
        assert!(session.contains("resume: the first refusal"), "{session}");
        assert!(session.contains("  [ ] the writer"), "no step is ticked by a field write: {session}");
        crate::check_with(&root, &FakeGit).expect("the plan is still valid");

        let before = snapshot(&root);
        let error = run(&root, &["step", "P6", "--now", "two\nlines"]).expect_err("a field is one line");
        assert!(error.contains("takes one line"), "{error}");
        assert_eq!(snapshot(&root), before, "a refused verb writes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A STATUS with the two prose sections a real plan has, so a verb that writes a paragraph finds
    /// one to write into.
    fn with_sections(status: &str, next: &str) -> String {
        format!("{status}\n## Where we are\n\nThe tree.\n\n## Next\n\n{next}\n")
    }

    #[test]
    fn a_start_takes_the_head_of_next_while_the_order_is_empty() {
        // The reviewed period paid a minute per session for `take` then `start`: `take` alone left
        // `state=idle`, and `start` refused a task outside `## Now`.
        let parked = LEGEND
            .replace("- `[ ]` **S14** the banding\n  `files: src/b.rs`\n\n## Bugs", "## Bugs")
            .replace("## Next\n\n## Done", "## Next\n\n- `[ ]` **S14** the banding\n  `files: src/b.rs`\n\n## Done");
        let empty_now = parked.replace("- `[ ]` **P6** the writer\n  `files: src/a.rs`\n\n", "");
        let idle = with_sections(&STATUS.replace("task=P6", "task=-"), "Nothing is scheduled: the order is empty.");
        let root = project("start-next", &idle, &empty_now, None);
        run(&root, &["start", "S14"]).expect("the head of `## Next` starts");
        let backlog = text(&root, "plan/BACKLOG.md");
        assert!(backlog.contains("- `[~]` **S14**"), "the marker moved with it: {backlog}");
        let now = backlog.find("## Now").expect("Now is there");
        let bugs = backlog.find("## Bugs").expect("Bugs is there");
        assert!(backlog[now..bugs].contains("**S14**"), "the entry is in the order: {backlog}");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("task=S14 state=doing"), "{status}");
        assert!(status.contains("S14 is next:"), "the paragraph names the current task: {status}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_start_behind_an_open_task_names_the_verb_that_moves_it() {
        let parked =
            BACKLOG.replace("## Next\n\n## Done", "## Next\n\n- `[ ]` **S20** later\n  `files: src/b.rs`\n\n## Done");
        let root = project("start-behind", STATUS, &parked, None);
        let error = run(&root, &["start", "S20"]).expect_err("P6 is still open ahead of it");
        assert!(error.contains("task take S20 --at top"), "the refusal names the verb: {error}");
        assert_eq!(text(&root, "plan/BACKLOG.md"), parked, "a refusal writes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_close_writes_its_own_next_paragraph() {
        // Two shapes, both refused in the reviewed period: a close that leaves a next task without
        // `--next`, and a close that ends the order while `--next` still names one.
        let running = with_sections(&STATUS.replace("state=idle", "state=doing"), "P6 is next: the writer");
        let backlog = BACKLOG.replace("- `[ ]` **P6**", "- `[~]` **P6**");
        let root = project("close-next", &running, &backlog, Some(NOTE));
        run(&root, &["close", "P6"]).expect("the close writes the paragraph");
        let status = text(&root, "plan/STATUS.md");
        assert!(status.contains("## Next\n\nS14 is next:"), "{status}");
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);

        // The order ends here and the owner still says what he wanted to say: the paragraph keeps
        // the sentence the gate requires in front of his words instead of refusing the write.
        let solo = backlog.replace("- `[ ]` **S14** the banding\n  `files: src/b.rs`\n\n", "");
        let root = project("close-last", &running, &solo, Some(NOTE));
        run(&root, &["close", "P6", "--next", "U91 waits in the backlog"]).expect("the close lands");
        let status = text(&root, "plan/STATUS.md");
        assert!(
            status.contains("Nothing is scheduled: U91 waits in the backlog"),
            "the required opening is written in front of the owner's words: {status}"
        );
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_field_the_entry_format_does_not_have_is_refused() {
        let root = project("set-unknown", STATUS, BACKLOG, None);
        let before = snapshot(&root);
        let error = run(&root, &["set", "P6", "--field", "now", "--body", "x"]).expect_err("`now` is no entry field");
        assert!(error.contains("`now` is not a field of an entry"), "{error}");
        assert!(error.contains("Why"), "the refusal names the labels: {error}");
        assert_eq!(snapshot(&root), before, "a refused verb writes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_drift_the_gate_would_reject_is_refused_before_it_lands() {
        // A task without a `files:` hint cannot be added: the gate rejects it, so the verb refuses.
        let root = project("refuse-gate", STATUS, BACKLOG, None);
        let before = snapshot(&root);
        let error = run(&root, &["add", "--phase", "S", "--why", "x", "a task with no hint"])
            .expect_err("the gate requires a hint");
        assert!(error.contains("--files is required"), "{error}");
        assert_eq!(snapshot(&root), before);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn two_writers_in_one_checkout_both_land() {
        // Measured 2026-11-21: two `task add` runs in one checkout both read the backlog, both
        // computed X107, and the later write replaced the earlier entry while the gate stayed green.
        let root = project("writers-race", STATUS, BACKLOG, None);
        let start = std::sync::Arc::new(std::sync::Barrier::new(4));
        let writers: Vec<std::thread::JoinHandle<Result<(), String>>> = (0..4)
            .map(|index| {
                let root = root.clone();
                let start = start.clone();
                std::thread::spawn(move || {
                    let lead = format!("entry {index}");
                    let args: Vec<String> = ["add", "--phase", "S", "--files", "src/a.rs", lead.as_str()]
                        .iter()
                        .map(|arg| (*arg).to_owned())
                        .collect();
                    start.wait();
                    run_with(&root, &args, &FakeGit)
                })
            })
            .collect();
        for writer in writers {
            writer.join().expect("the writer thread ends").expect("each write either lands or waits, never loses");
        }
        let backlog = text(&root, "plan/BACKLOG.md");
        for index in 0..4 {
            let lead = format!("entry {index}");
            assert_eq!(backlog.matches(lead.as_str()).count(), 1, "`{lead}` landed once: {backlog}");
        }
        crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_writer_lock_covers_the_cycle_and_is_released_afterwards() {
        let root = project("writers-lock", STATUS, BACKLOG, None);
        let held = Lock::acquire_within(&root, Duration::ZERO).expect("the lock is free");
        let error = Lock::acquire_within(&root, Duration::ZERO).expect_err("a second writer refuses");
        assert!(error.contains("plan/.lock"), "the refusal names the lock: {error}");
        assert!(root.join("plan/.lock").exists(), "the refusal leaves the holder's file alone");

        // A verb that arrives behind a held lock waits for it and lands on the text the writer in
        // front of it produced, instead of computing its id from the text that writer read.
        let waiting = {
            let root = root.clone();
            std::thread::spawn(move || run(&root, &["add", "--phase", "S", "--files", "src/a.rs", "a waiting entry"]))
        };
        std::thread::sleep(Duration::from_millis(50));
        assert!(!text(&root, "plan/BACKLOG.md").contains("a waiting entry"), "the verb waits, it does not race");
        std::mem::drop(held);
        waiting.join().expect("the writer thread ends").expect("it lands once the lock is free");
        assert!(text(&root, "plan/BACKLOG.md").contains("a waiting entry"), "the entry landed");
        assert!(!root.join("plan/.lock").exists(), "a finished verb releases the lock");

        // A refusal releases it too, and a dry run takes it for its reading and writes nothing.
        let error = run(&root, &["add", "--phase", "S", "a task with no hint"]).expect_err("the gate refuses it");
        assert!(error.contains("--files is required"), "{error}");
        assert!(!root.join("plan/.lock").exists(), "a refused verb releases the lock too");
        run(&root, &["add", "--phase", "S", "--files", "src/a.rs", "--dry-run", "a preview"])
            .expect("a dry run answers");
        assert!(!root.join("plan/.lock").exists(), "a dry run releases the lock");
        assert!(!text(&root, "plan/BACKLOG.md").contains("a preview"), "a dry run writes nothing");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_task_status_is_read_back_as_the_gate_sees_it() {
        let root = project("status", STATUS, BACKLOG, None);
        run(&root, &["start", "P6"]).expect("P6 starts");
        let summary = crate::check_with(&root, &FakeGit).expect("the plan is valid");
        let task = summary.tasks.iter().find(|task| task.id == "P6").expect("P6 is in the backlog");
        assert_eq!(task.status, TaskStatus::Doing);
        let _ = std::fs::remove_dir_all(&root);
    }
}
