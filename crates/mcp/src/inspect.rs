//! What `ui_inspect` answers: the window's own state, as data a client can read.
//!
//! The value is the shell's, not the engine's: the document is `query`'s, and this is the one thing
//! a headless session cannot tell a client — which scenes are open, which one is in front, what the
//! person has selected and which gesture is armed. Every field is plain data, so the shell fills the
//! struct and the protocol serialises it without `mcp` learning what a tab is (ADR 0186, ADR 0188).
//!
//! Changes when: the window has a piece of state an agent must know to work in it — a new mode, a
//! new per-tab fact — never when the document gains a field (that is `query`'s table).

use serde::{Deserialize, Serialize};

/// One open scene, as the tab row draws it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tab {
    /// The name the tab draws, in the wire's own language.
    pub title: String,
    /// The file the scene was opened from or saved to; `None` for a scene never written.
    pub path: Option<String>,
    /// Whether the document holds changes the file does not.
    pub dirty: bool,
    /// The document's revision, so a client can tell whether its own write landed.
    pub revision: u64,
    /// The open product, if the scene has one open.
    pub product: Option<u64>,
}

/// The window a person is looking at.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    /// Every open scene, in the order the tab row draws them.
    pub tabs: Vec<Tab>,
    /// The index of the active tab in [`Self::tabs`].
    pub active: usize,
    /// The armed transform gesture, by its inventory id, or `select` when none is armed.
    pub mode: String,
    /// The selected node ids, oldest first; the last one is the primary.
    pub selection: Vec<u64>,
    /// Whether a gesture holds the window, so a command would land in the middle of it.
    pub busy: bool,
    /// The sentence the status bar shows, if it shows one.
    pub status: Option<String>,
}

impl Window {
    /// The tab the person is looking at, if the window has one.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn window() -> Window {
        Window {
            tabs: vec![
                Tab { title: "Cabinet".to_owned(), path: Some("/tmp/cabinet.z3l".to_owned()), ..Tab::default() },
                Tab { title: "Untitled".to_owned(), dirty: true, revision: 7, product: Some(3), ..Tab::default() },
            ],
            active: 1,
            mode: "transform.object".to_owned(),
            selection: vec![3, 4],
            busy: false,
            status: Some("Two bodies are through each other".to_owned()),
        }
    }

    #[test]
    fn the_answer_round_trips_and_names_the_active_tab() {
        let window = window();
        let written = serde_json::to_string(&window).expect("the window serialises");
        let read: Window = serde_json::from_str(&written).expect("and reads back");
        assert_eq!(read, window);
        assert_eq!(read.active_tab().map(|tab| tab.title.as_str()), Some("Untitled"));
        assert_eq!(read.tabs.first().and_then(|tab| tab.path.as_deref()), Some("/tmp/cabinet.z3l"));
    }

    #[test]
    fn an_empty_window_names_no_tab() {
        let window = Window::default();
        assert!(window.active_tab().is_none(), "a window with no scene is not a tab called nothing");
        assert_eq!(window.active, 0);
    }
}
