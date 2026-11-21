//! The work streams a plan declares: which phases of the backlog are worked in which checkout.
//!
//! A project that runs one session at a time needs nothing from this module: a plan without a
//! `STREAMS:` line has one stream, the primary, and every rule keeps the shape it always had. A
//! project that splits its work across checkouts — Z3L runs five over one backlog —
//! declares the others beside the `STATE:` line:
//!
//! ```text
//! STATE: milestone=M7 phase=T task=P17 state=doing base=36a1cd4 updated=2026-11-19
//! STREAMS: agent=J catalog=O prototypes=Q
//! ```
//!
//! The key names the stream and its session note (`plan/SESSION.agent.md`); the letters are the
//! phases it owns. A task belongs to a stream by its own id: the phase letter in front of the number
//! is the whole declaration, so a plan cannot say one thing about a task and another about the
//! checkout that works it. The primary stream is `app`: it owns every letter no other stream names,
//! its note is `plan/SESSION.md`, and its task is the one `STATE: task=` schedules — the plan is
//! written from one checkout, and that checkout's own work is what the state line is about.

use crate::Task;

/// The marker that starts the stream line.
pub const STREAMS_PREFIX: &str = "STREAMS:";

/// The key of the primary stream: the plan's own checkout, which `STATE:` and `task=` speak about.
pub const PRIMARY: &str = "app";

/// The stem every session note is named after, and the primary stream's note with `.md` on the end.
pub const NOTE_STEM: &str = "plan/SESSION";

/// The primary stream's note, which is the one a project without a declaration has.
pub const NOTE: &str = "plan/SESSION.md";

/// One stream: the phases whose tasks are worked in one checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stream {
    /// The key that names the stream: `app`, `agent`, `catalog`, `prototypes`.
    pub key: String,
    /// The phase letters the stream owns; empty for the primary, which owns every letter no other
    /// stream names.
    pub phases: Vec<char>,
}

impl Stream {
    /// Whether this is the primary stream, whose work `STATE:` describes.
    pub fn is_primary(&self) -> bool {
        self.key == PRIMARY
    }

    /// The session note this stream keeps.
    pub fn note_path(&self) -> String {
        if self.is_primary() { NOTE.to_owned() } else { format!("{NOTE_STEM}.{}.md", self.key) }
    }
}

/// The streams a plan declares, the primary always first and always present.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Streams {
    primary: Stream,
    others: Vec<Stream>,
}

impl Default for Streams {
    fn default() -> Self {
        Self { primary: Stream { key: PRIMARY.to_owned(), phases: Vec::new() }, others: Vec::new() }
    }
}

impl Streams {
    /// The one stream a plan without a `STREAMS:` line has.
    pub fn one() -> Self {
        Self::default()
    }

    /// Whether the plan declares a stream beside the primary.
    pub fn declared(&self) -> bool {
        !self.others.is_empty()
    }

    /// The primary stream, whose task `STATE:` names.
    pub fn primary(&self) -> &Stream {
        &self.primary
    }

    /// Every stream, the primary first.
    pub fn iter(&self) -> impl Iterator<Item = &Stream> {
        std::iter::once(&self.primary).chain(self.others.iter())
    }

    /// How many streams the plan has: the primary always, plus the declared ones.
    pub fn len(&self) -> usize {
        self.others.len().saturating_add(1)
    }

    /// Whether the plan has no stream at all, which the primary makes impossible.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The stream that owns a task, by the phase letter of its id.
    pub fn of(&self, task: &Task) -> &Stream {
        self.owner(&task.id)
    }

    /// The stream that owns the task with this id.
    ///
    /// A phase no stream names belongs to the primary: the declaration widens a plan that already
    /// works, it does not have to repeat every letter of its legend.
    pub fn owner(&self, id: &str) -> &Stream {
        let phase = id.chars().next().filter(char::is_ascii_uppercase);
        match phase {
            Some(letter) => self.others.iter().find(|stream| stream.phases.contains(&letter)).unwrap_or(&self.primary),
            None => &self.primary,
        }
    }

    /// The session note of the stream with this key, when the plan declares one.
    pub fn note_of(&self, key: &str) -> Option<String> {
        self.iter().find(|stream| stream.key == key).map(Stream::note_path)
    }

    /// The stream whose note lives at `path`, when one does.
    ///
    /// It is the reverse of [`Stream::note_path`], and it is how a note left behind by a stream the
    /// plan no longer declares is caught instead of silently ignored.
    pub fn stream_at(&self, path: &str) -> Option<&Stream> {
        self.iter().find(|stream| stream.note_path() == path)
    }

    /// The paths of every note the declaration names, in stream order.
    pub fn notes(&self) -> Vec<String> {
        self.iter().map(Stream::note_path).collect()
    }

    /// Parses the `STREAMS:` line of `plan/STATUS.md`.
    ///
    /// A plan without the line has one stream and no violation: the declaration is what a project
    /// with several checkouts opts into, and a project that already works keeps working without it.
    /// A line the reader cannot use is a list of sentences naming the pair that is wrong.
    pub fn parse(status: &str) -> Result<Self, Vec<String>> {
        let mut lines = status.lines().map(str::trim).filter(|line| line.starts_with(STREAMS_PREFIX));
        let Some(line) = lines.next() else {
            return Ok(Self::default());
        };
        let mut violations = Vec::new();
        if lines.next().is_some() {
            violations.push(format!(
                "`plan/STATUS.md` carries two `{STREAMS_PREFIX}` lines; a plan declares its streams once"
            ));
        }
        let mut streams = Self::default();
        for pair in line.trim_start_matches(STREAMS_PREFIX).split_whitespace() {
            let Some((key, letters)) = pair.split_once('=') else {
                violations.push(format!("`{STREAMS_PREFIX}` carries `{pair}`, which is not `key=AB`"));
                continue;
            };
            if !valid_key(key) {
                violations.push(format!(
                    "`{STREAMS_PREFIX}` stream `{key}` is not a key: lowercase letters, digits and `-`, \
                     starting with a letter"
                ));
                continue;
            }
            if key == PRIMARY {
                violations.push(format!(
                    "`{STREAMS_PREFIX}` may not declare `{PRIMARY}`: it is the plan's own checkout, which \
                     `STATE:` describes"
                ));
                continue;
            }
            if streams.note_of(key).is_some() {
                violations.push(format!("`{STREAMS_PREFIX}` declares the stream `{key}` twice"));
                continue;
            }
            let mut phases: Vec<char> = Vec::new();
            for letter in letters.split(',').filter(|letter| !letter.is_empty()) {
                let mut chars = letter.chars();
                match (chars.next(), chars.next()) {
                    (Some(letter), None) if letter.is_ascii_uppercase() => {
                        if let Some(owner) = streams.iter().find(|stream| stream.phases.contains(&letter)) {
                            violations.push(format!(
                                "`{STREAMS_PREFIX}` gives phase {letter} to `{key}`, but `{}` already owns it",
                                owner.key
                            ));
                        } else if !phases.contains(&letter) {
                            phases.push(letter);
                        }
                    }
                    _ => violations.push(format!(
                        "`{STREAMS_PREFIX}` stream `{key}` names `{letter}`; a phase is one capital letter, like `K`"
                    )),
                }
            }
            if phases.is_empty() {
                violations.push(format!(
                    "`{STREAMS_PREFIX}` stream `{key}` owns no phase; name the letters it works, like `{key}=J`"
                ));
                continue;
            }
            streams.others.push(Stream { key: key.to_owned(), phases });
        }
        if violations.is_empty() { Ok(streams) } else { Err(violations) }
    }
}

/// A stream key: lowercase letters, digits and `-`, starting with a letter.
fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|ch| ch.is_ascii_lowercase())
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str) -> Task {
        Task {
            id: id.to_owned(),
            status: crate::TaskStatus::Open,
            section: "Now".to_owned(),
            text: format!("{id} whatever"),
            lead: "whatever".to_owned(),
            fields: Vec::new(),
            files: None,
            paths: Vec::new(),
        }
    }

    #[test]
    fn a_plan_without_the_line_has_one_stream_and_owns_every_phase() {
        let streams = Streams::parse("STATE: milestone=M1 task=A1 state=idle\n").expect("no declaration is legal");
        assert!(!streams.declared());
        assert_eq!(streams.len(), 1);
        assert_eq!(streams.primary().note_path(), "plan/SESSION.md");
        assert_eq!(streams.of(&task("J1")).key, "app");
        assert_eq!(streams.notes(), vec!["plan/SESSION.md"]);
    }

    #[test]
    fn the_declared_streams_own_their_letters_and_the_primary_keeps_the_rest() {
        let status = "STATE: milestone=M7 task=P17 state=doing\nSTREAMS: agent=J catalog=O prototypes=Q\n";
        let streams = Streams::parse(status).expect("three streams are declared");
        assert!(streams.declared());
        assert_eq!(streams.len(), 4);
        assert_eq!(streams.of(&task("J1")).key, "agent");
        assert_eq!(streams.of(&task("O12")).key, "catalog");
        assert_eq!(streams.of(&task("Q1")).key, "prototypes");
        assert_eq!(streams.of(&task("X85")).key, "app");
        assert_eq!(streams.note_of("agent").as_deref(), Some("plan/SESSION.agent.md"));
        assert_eq!(streams.stream_at("plan/SESSION.catalog.md").map(|stream| stream.key.as_str()), Some("catalog"));
        assert!(streams.stream_at("plan/SESSION.md").is_some_and(Stream::is_primary));
        assert!(streams.stream_at("plan/SESSION.notes.md").is_none());
    }

    #[test]
    fn a_stream_may_own_several_phases_and_the_line_holds_them_in_order() {
        let streams = Streams::parse("STREAMS: agent=J,K\n").expect("one stream may own two phases");
        assert_eq!(streams.of(&task("K4")).key, "agent");
        assert_eq!(streams.of(&task("J1")).key, "agent");
        let keys: Vec<&str> = streams.iter().map(|stream| stream.key.as_str()).collect();
        assert_eq!(keys, vec!["app", "agent"]);
    }

    #[test]
    fn a_declaration_the_reader_cannot_use_is_a_sentence() {
        for (line, expected) in [
            ("STREAMS: agent\n", "which is not `key=AB`"),
            ("STREAMS: Agent=J\n", "is not a key"),
            ("STREAMS: app=J\n", "may not declare `app`"),
            ("STREAMS: agent=J agent=K\n", "twice"),
            ("STREAMS: agent=J,K catalog=K\n", "already owns it"),
            ("STREAMS: agent=jk\n", "one capital letter"),
            ("STREAMS: agent=\n", "owns no phase"),
        ] {
            let violations = Streams::parse(line).expect_err("the declaration is unusable");
            assert!(
                violations.iter().any(|violation| violation.contains(expected)),
                "{line} must be refused with `{expected}`: {violations:?}"
            );
        }
    }

    #[test]
    fn two_lines_are_refused_because_the_declaration_has_one_home() {
        let violations = Streams::parse("STREAMS: agent=J\nSTREAMS: prototypes=Q\n").expect_err("two lines");
        assert!(violations.iter().any(|violation| violation.contains("two `STREAMS:` lines")), "{violations:?}");
    }
}
