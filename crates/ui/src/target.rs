//! Which node a viewport click selects and which container a creation tool lands in.
//!
//! Both are rules over document data: they read the tree, the open set and the armed container and
//! return ids. They never change the document; the editor stores the answer as its selection, or
//! uses it as the `parent` of a creation command.
//!
//! The contract: a pick takes a document id and returns a document id or `None`; a creation target
//! is always a container that is in the tree, the root by default. What a click sees through is view
//! state ([ADR 0029]), so the picking rule takes it as an argument instead of reading it from the
//! project: the containers the editor opened, and the one product the session works in ([ADR 0098]),
//! which lives with the session rather than in the open set.
//!
//! It refuses nothing: an unknown id has no pick and no child, and a stale arm falls back to the
//! root rather than failing.
//!
//! Changes when: the picking rule or the creation-target rule changes.
//!
//! It does not do: the selection state, the open set or the armed container, which the editor
//! owns, and the conversion of a world placement into the parent frame, which is
//! [`Project::placement_in_parent`].
//!
//! [ADR 0022]: ../docs/decisions/0022-closed-containers-pick-as-one-object.md
//! [ADR 0029]: ../docs/decisions/0029-view-state-belongs-to-the-session.md
//! [ADR 0098]: ../docs/decisions/0098-one-product-is-open-and-parts-are-born-inside-it.md

use std::collections::HashSet;

use model::{Id, Project};

/// The node a viewport click should select: the outermost closed container above the hit node.
///
/// Walking up stops at the nearest open level, so an open level exposes its children — whether it is
/// a container a double click revealed or the product the session works in (`open_product`). A
/// closed product picks as one object, which is what makes opening it change what the next click
/// does. The root is not a selectable node; a top-level panel or container is selected directly
/// (ADR 0022).
pub(crate) fn resolve_viewport_pick(
    project: &Project,
    open: &HashSet<Id>,
    open_product: Option<Id>,
    hit: Id,
) -> Option<Id> {
    if !project.contains_node(hit) {
        return None;
    }
    if project.is_container(hit) {
        return Some(hit);
    }
    let root = project.root().id();
    let mut node = hit;
    while let Some(parent) = project.parent_of(node) {
        if parent == root || open.contains(&parent) || open_product == Some(parent) {
            break;
        }
        node = parent;
    }
    Some(node)
}

/// The container every creation tool lands in.
///
/// The scene root is the default parent, whatever the selection is: choosing a node must not
/// silently change where a new path, panel, assembly, primitive, camera or light goes (ADR 0056).
/// A child is created through an explicit entry instead, which arms a container; the arm falls back
/// to the root as soon as that node is gone or cannot own children. Extruding a path is not a
/// creation tool, so it stays a sibling of its path.
pub(crate) fn creation_target(project: &Project, armed: Option<Id>) -> Id {
    let root = project.root().id();
    armed.filter(|id| *id != root && project.is_container(*id)).unwrap_or(root)
}

/// The direct child of `container` whose subtree contains `node`.
pub(crate) fn child_containing(project: &Project, container: Id, node: Id) -> Option<Id> {
    if node == container {
        return None;
    }
    let mut current = node;
    for _ in 0..Project::MAX_ASSEMBLY_DEPTH {
        let parent = project.parent_of(current)?;
        if parent == container {
            return Some(current);
        }
        current = parent;
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use engine::PanelCreate;
    use model::Placement;
    use testkit::{millimetres, sample_product_project};

    /// A product of 600 x 560 x 720 mm holding one shelf, and that shelf's id.
    fn product_with_a_shelf() -> (Project, Id, Id) {
        let (mut project, product) = sample_product_project();
        let shelf = project
            .add_panel(
                product,
                "Shelf",
                PanelCreate::default_nominal_size().expect("a panel size"),
                None,
                millimetres(18),
                Placement::IDENTITY,
            )
            .expect("a shelf inside the product");
        (project, product, shelf)
    }

    #[test]
    fn an_open_product_exposes_the_part_and_a_closed_one_picks_as_one_object() {
        // A product is a container, but its openness lives in the session rather than in the editor's
        // open set (ADR 0098): the walk up from the part has to stop at it once it is open, and pass
        // through it while it is closed.
        let (project, product, shelf) = product_with_a_shelf();
        let open = HashSet::new();
        assert_eq!(
            resolve_viewport_pick(&project, &open, None, shelf),
            Some(product),
            "a closed product is one object"
        );
        assert_eq!(
            resolve_viewport_pick(&project, &open, Some(product), shelf),
            Some(shelf),
            "an open product exposes the part"
        );
        assert_eq!(
            resolve_viewport_pick(&project, &open, Some(product), product),
            Some(product),
            "the piece itself still picks itself"
        );
    }

    #[test]
    fn an_open_product_exposes_a_group_and_the_open_group_exposes_its_children() {
        // The two kinds of open level compose: the product stops the walk first, so the group inside
        // it is the outermost closed container, and opening that group reaches the shelf.
        let (mut project, product, shelf) = product_with_a_shelf();
        let back = project
            .add_panel(
                product,
                "Back",
                PanelCreate::default_nominal_size().expect("a panel size"),
                None,
                millimetres(18),
                Placement::IDENTITY,
            )
            .expect("a second part to group with");
        let group = project.group_nodes(&[shelf, back], "Group").expect("a group inside the product");
        let open = HashSet::new();
        assert_eq!(
            resolve_viewport_pick(&project, &open, None, shelf),
            Some(product),
            "a closed product hides the tree"
        );
        assert_eq!(
            resolve_viewport_pick(&project, &open, Some(product), shelf),
            Some(group),
            "an open product exposes the group as one object"
        );
        let open: HashSet<Id> = [group].into_iter().collect();
        assert_eq!(
            resolve_viewport_pick(&project, &open, Some(product), shelf),
            Some(shelf),
            "and the open group reaches the shelf"
        );
    }
}
