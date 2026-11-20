//! What the eye and the ray may pass: the parts one query must read as switched off.
//!
//! Visibility is session view state and never document data (ADR 0065), so the document cannot ask
//! it and a query cannot read it from a field: a caller hands it in, the way [`SnapSettings`] is
//! handed to [`Project::move_rule`] (ADR 0175). [`Sight`] is that value: the nodes the user switched
//! off, plus the parent walk that makes a hidden container hide its subtree — the same walk the
//! scene builder, the pick and the light rig make, now in one place.
//!
//! The set is *explicit*: hiding a container writes no flag on each child, so a query asks
//! [`Sight::hides`] and the walk answers for the whole subtree. The walk is bounded by the document's
//! nesting limit, so a malformed tree cannot spin here.
//!
//! A sight says what a *measurement* passes, never what the document holds: a hidden part loses its
//! picture and its pointer, never its body, so the placement guard still reads it (ADR 0162). The
//! queries that take a sight are the ones that look at free space — [`Project::probe`],
//! [`Project::niche`] and the fill plan — and the tool that *moves material* hands in
//! [`Sight::none`] on purpose.
//!
//! Changes when: the rule that hides a subtree changes, or a second visibility reading appears.

use std::collections::HashSet;

use crate::{Id, Project};

/// The parts one query reads as switched off.
///
/// It borrows the session's set, so asking costs no copy and no revision: a sight is a parameter of a
/// query and never a value a query stores.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sight<'a> {
    hidden: Option<&'a HashSet<Id>>,
}

impl<'a> Sight<'a> {
    /// A sight that switches nothing off: every part is there.
    pub const fn none() -> Self {
        Self { hidden: None }
    }

    /// The sight a session's hidden set gives.
    pub const fn of(hidden: &'a HashSet<Id>) -> Self {
        Self { hidden: Some(hidden) }
    }

    /// The nodes the user switched off, before the parent walk, when the sight has any.
    pub const fn hidden(self) -> Option<&'a HashSet<Id>> {
        self.hidden
    }

    /// Whether the node itself carries the hidden flag.
    pub fn is_hidden(self, id: Id) -> bool {
        self.hidden.is_some_and(|hidden| hidden.contains(&id))
    }

    /// Whether the node is switched off, by its own flag or by an ancestor's.
    ///
    /// This is the question every consumer that projects or measures the tree asks. A sight with no
    /// hidden node answers `false` without walking, so the common case costs one comparison.
    pub fn hides(self, project: &Project, id: Id) -> bool {
        let Some(hidden) = self.hidden.filter(|hidden| !hidden.is_empty()) else {
            return false;
        };
        let mut current = Some(id);
        for _ in 0..=Project::MAX_ASSEMBLY_DEPTH {
            let Some(node) = current else {
                return false;
            };
            if hidden.contains(&node) {
                return true;
            }
            current = project.parent_of(node);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use crate::fixtures::sample_project;
    use crate::{Container, Node, Placement};

    use super::*;

    fn id(value: u64) -> Id {
        Id::new(value).expect("non-zero id")
    }

    #[test]
    fn a_sight_with_nothing_hidden_passes_every_part() {
        let project = sample_project();
        let sight = Sight::none();
        assert!(!sight.hides(&project, id(2)));
        assert!(sight.hidden().is_none(), "the plain sight borrows nothing");
        assert_eq!(Sight::default(), sight, "the default is the plain one");
    }

    #[test]
    fn a_hidden_container_hides_its_whole_subtree() {
        let project = sample_project();
        let hidden: HashSet<Id> = [id(3)].into_iter().collect();
        let sight = Sight::of(&hidden);
        assert!(sight.hides(&project, id(3)));
        assert!(sight.is_hidden(id(3)));
        assert!(sight.hides(&project, id(4)), "a child of a hidden container is hidden too");
        assert!(!sight.is_hidden(id(4)), "the child carries no flag of its own");
        assert!(!sight.hides(&project, id(2)), "a sibling of the hidden container is untouched");
        assert!(!sight.hides(&project, id(999)), "a node the document does not hold is not hidden");
    }

    #[test]
    fn an_empty_set_costs_no_walk_and_hides_nothing() {
        let inner = Container::new(
            id(20),
            "Inner",
            Placement::IDENTITY,
            vec![Node::Container(Container::new(id(21), "Target", Placement::IDENTITY, Vec::new()))],
        );
        let project = Project::new(Container::new(id(1), "Root", Placement::IDENTITY, vec![Node::Container(inner)]))
            .expect("project");
        let hidden = HashSet::new();
        assert!(!Sight::of(&hidden).hides(&project, id(21)));
    }
}
