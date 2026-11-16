//! The selection and the set of open containers.
//!
//! Both are view state: `Selection` keeps the multi-selection in the order the user built it and
//! the open set remembers which containers show their children. The rules that turn a viewport hit
//! into a selected node live in `crate::target`; this module only applies them.
//!
//! A click carries a [`Pick`]: a plain one replaces the selection, Shift adds the node at the end of
//! the order or takes it out again, and Ctrl removes it and nothing else. The tree row and the
//! viewport read the same modifiers through that type, so the two surfaces cannot disagree.
//!
//! Refuses nothing: an empty click replaces with nothing and `exit_level` with no open level above
//! the selection does nothing, so a gesture the scene cannot honour leaves the state unchanged.
//!
//! Changes when: a gesture selects, adds to the selection, or opens and closes a container.

use std::collections::HashSet;

use engine::Session;
use model::{Container, Id, Project};

use super::Editor;
use crate::scene_tree::Pick;
use crate::target::{child_containing, resolve_viewport_pick};

impl Editor {
    /// The primary selected node, if any.
    pub fn selected_node(&self) -> Option<Id> {
        self.selection.primary()
    }

    /// Every selected node in selection order, oldest first; the last one is the primary.
    ///
    /// Multi-selection is view state, so a gizmo drag moves all of these nodes as one document
    /// change while the inspector keeps showing the primary.
    pub fn selected_nodes(&self) -> &[Id] {
        self.selection.ids()
    }

    /// Replaces the selection with one node, or clears it for `None`.
    ///
    /// A deliberate selection also counts as the first show, so the tree does not replace it with
    /// the first panel afterwards.
    pub fn select_node(&mut self, node: Option<Id>) {
        self.initialized = true;
        match node {
            Some(node) => self.selection.replace(node),
            None => self.selection.clear(),
        }
    }

    /// Replaces the selection with several nodes, in the given order; the last one is the primary.
    ///
    /// The tree's Shift-click builds the list one row at a time; a surface that knows the set up
    /// front — a test, and later a marquee or the assistant — sets it here.
    pub fn select_nodes(&mut self, nodes: Vec<Id>) {
        self.initialized = true;
        self.selection.replace_all(nodes);
    }

    /// The containers whose contents the pointer reaches directly: the open levels of the viewport.
    ///
    /// The picker resolves a click through this set and the shell draws the bracket of every level in
    /// it (U89), so both read one field instead of each keeping a copy of the open state (ADR 0098).
    pub fn open_containers(&self) -> &HashSet<Id> {
        &self.open
    }

    /// Selects the result of a viewport click on `hit`, honouring the modifier the click carried.
    ///
    /// The viewport returns the panel under the pointer; a closed container above it is selected
    /// instead, so a click picks the cabinet rather than one of its boards. `open_product` is the
    /// product the session works in, which exposes its parts exactly as an open group does.
    ///
    /// A plain click on the scene replaces the selection and a plain click outside it clears it;
    /// Shift adds the picked node at the end of the order (a second Shift-click takes it back out)
    /// and Ctrl removes it alone. A modified click on empty space changes nothing, because there is
    /// no node for the modifier to act on.
    pub fn select_viewport_hit(&mut self, project: &Project, open_product: Option<Id>, hit: Option<Id>, pick: Pick) {
        self.initialized = true;
        match hit.and_then(|hit| resolve_viewport_pick(project, &self.open, open_product, hit)) {
            Some(node) => self.selection.pick(node, pick),
            None if pick == Pick::Replace => self.selection.clear(),
            None => {}
        }
    }

    /// Selects a node the shell already resolved, with the modifier its click carried.
    ///
    /// A camera or light glyph is picked as itself, without the container resolution a viewport hit
    /// goes through, so the shell hands the node over and the rule for the modifier stays here.
    pub fn pick_node(&mut self, node: Id, pick: Pick) {
        self.initialized = true;
        self.selection.pick(node, pick);
    }

    /// The subject a context menu opened at a viewport hit is about.
    ///
    /// It resolves the hit the way a click resolves its selection — through the open levels and
    /// the working product — so a right click on a board of a closed cabinet offers the cabinet's
    /// actions, not the board's. Empty space is its own subject, because the menu it opens is the
    /// document's rather than a node's.
    pub fn context_menu_subject(
        &self,
        project: &Project,
        open_product: Option<Id>,
        hit: Option<Id>,
    ) -> crate::node_actions::Subject {
        match hit.and_then(|hit| resolve_viewport_pick(project, &self.open, open_product, hit)) {
            Some(node) if node == project.root().id() => crate::node_actions::Subject::Root,
            Some(node) => crate::node_actions::Subject::Node(node),
            None => crate::node_actions::Subject::Empty,
        }
    }

    /// Acts on a double click in the viewport: opens a product, or enters one level below a group.
    ///
    /// A double click on a product opens it as the session's working product (ADR 0098, ADR 0100)
    /// and arms it for creation; a double click on a group reveals its contents and selects the
    /// child under the pointer. A double click on a panel, a loose object or the root does nothing
    /// beyond the selection the first click already made — in particular it never opens a product
    /// that does not own the hit.
    ///
    /// Returns whether the open product changed, so the shell can rebuild the envelope inks in the
    /// frame the click lands in.
    pub fn enter_viewport_level(&mut self, session: &mut Session, hit: Option<Id>) -> bool {
        let Some(hit) = hit else {
            return false;
        };
        self.initialized = true;
        let open_product = session.active_product();
        let project = session.project();
        let Some(selected) = resolve_viewport_pick(project, &self.open, open_product, hit) else {
            return false;
        };
        self.selection.replace(selected);
        if !project.is_container(selected) {
            return false;
        }
        if project.container(selected).is_some_and(Container::is_product) {
            return self.open_product(session, selected);
        }
        self.open.insert(selected);
        if let Some(child) = child_containing(project, selected, hit) {
            self.selection.replace(child);
        }
        false
    }

    /// Closes one level and returns the selection to the container that owns it.
    ///
    /// The current node closes when it is an open container; otherwise the nearest open ancestor
    /// closes. With no open container above the selection inside the product the session works in,
    /// that product closes instead, so the key leaves a piece the same way it leaves a group
    /// (ADR 0098). With nothing open this does nothing, and a selection outside the open product
    /// never closes it. Returns whether the open product closed, because that repaints its envelope
    /// without a document revision (ADR 0100).
    pub fn exit_level(&mut self, session: &mut Session) -> bool {
        let Some(selected) = self.selection.primary() else {
            return false;
        };
        let root = session.project().root().id();
        if session.project().is_container(selected) && self.open.remove(&selected) {
            if let Some(parent) = session.project().parent_of(selected)
                && parent != root
            {
                self.selection.replace(parent);
            }
            return false;
        }
        let mut node = selected;
        while let Some(parent) = session.project().parent_of(node) {
            if parent == root {
                break;
            }
            if self.open.remove(&parent) {
                self.selection.replace(parent);
                return false;
            }
            node = parent;
        }
        let Some(product) = session.active_product() else {
            return false;
        };
        if session.project().product_of(selected) != Some(product) {
            return false;
        }
        self.close_product(session);
        self.selection.replace(product);
        true
    }

    /// Drops open flags for nodes that no longer exist or no longer own children.
    pub(super) fn retain_open(&mut self, project: &Project) {
        self.open.retain(|id| project.is_container(*id));
    }
}
