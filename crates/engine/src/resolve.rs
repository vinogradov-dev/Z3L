//! Catalogue-aware resolution of panel geometry.
//!
//! The model stores fallback thicknesses so a missing material never makes a scene unreadable. The
//! session resolves the catalogue first and uses those fallbacks only when a material is absent.

use std::collections::BTreeMap;

use catalog::{Directory, Selection, Slot, SlotId, Step};
use model::{BandingSpec, Id, ItemId, Node, Panel, PanelGeometry, Project, ProjectError, PropertyValue, Revision};
use units::Length;

/// The resolved panel thickness: catalogue material first, stored fallback otherwise.
pub(crate) fn panel_thickness(panel: &Panel, catalog: &Directory) -> Length {
    panel
        .material()
        .and_then(|id| catalog.board(id))
        .map_or(panel.last_known_thickness(), |material| material.thickness)
}

/// The resolved thickness for any banding owner.
pub(crate) fn spec_thickness(spec: &BandingSpec, catalog: &Directory) -> Length {
    catalog.banding(spec.material()).map_or(spec.last_known_thickness(), |material| material.thickness)
}

/// The resolved width for any banding owner.
pub(crate) fn spec_width(spec: &BandingSpec, catalog: &Directory) -> Length {
    catalog.banding(spec.material()).map_or(spec.last_known_width(), |material| material.width)
}

/// Resolves one panel's core, overall and effective thickness.
pub(crate) fn panel_geometry(
    project: &Project,
    catalog: &Directory,
    panel_id: Id,
) -> Result<PanelGeometry, ProjectError> {
    let panel = project.panel(panel_id).ok_or(ProjectError::PanelNotFound { id: panel_id })?;
    let thickness = panel_thickness(panel, catalog);
    Ok(panel.geometry_with_banding(thickness, |spec| spec_thickness(spec, catalog))?)
}

/// The record a node's slots are resolved against: the board a panel is cut from.
///
/// A slot asks "which record belongs to this corpus", so the anchor is the catalogue record the node
/// already names — a panel's material. A node that names none has no corpus, and its slots are a miss
/// rather than a guess.
pub fn anchor_of(project: &Project, node: Id) -> Option<ItemId> {
    match project.node(node)? {
        Node::Panel(panel) => panel.material(),
        Node::Container(_) | Node::Object(_) | Node::Camera(_) | Node::Light(_) => None,
    }
}

/// The record a person chose for `slot` on `node`, when the node carries one.
///
/// The choice is an ordinary property keyed by the slot id (`§10`), so a later directory update cannot
/// take it away and a card that edits properties needs no second path.
pub fn choice_of(project: &Project, node: Id, slot: &Slot) -> Option<ItemId> {
    match project.properties(node)?.get(slot.id().as_str())? {
        PropertyValue::Item { id } => Some(*id),
        _ => None,
    }
}

/// Resolves `slot` for `node`, reading the person's choice from the node's own properties.
///
/// The six steps themselves live in the directory (`catalog::Directory::select`); this is the one
/// place a document node becomes an anchor and a choice. A node with no anchor answers a miss on the
/// compatibility step, because no record can belong to a corpus the node does not name.
pub fn resolve_slot(project: &Project, catalog: &Directory, node: Id, slot: &Slot) -> Selection {
    let Some(anchor) = anchor_of(project, node) else {
        return Selection::Miss {
            step: Step::Compatibility,
            reason: format!(
                "\"{}\" names no material, so {} has no corpus to belong to",
                project.node(node).map_or("a missing node", Node::name),
                slot.id()
            ),
        };
    };
    catalog.select(anchor, slot, choice_of(project, node, slot))
}

/// What one cached selection is valid for: the document's revision and the directory's.
///
/// Both move for reasons of their own — a node edit and a reloaded package — and a selection depends
/// on both, so one stamp naming both is the only key that cannot answer a stale record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    /// The document revision the answer was read at.
    pub document: Revision,
    /// The directory revision it was read from.
    pub directory: Revision,
}

/// The selections a screen has already resolved, until either revision moves (`§10`).
#[derive(Clone, Debug, Default)]
pub struct SelectionCache {
    stamp: Stamp,
    entries: BTreeMap<(Id, SlotId), Selection>,
}

impl SelectionCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves `slot` for `node`, answering from the cache while both revisions are unchanged.
    pub fn resolve(
        &mut self,
        project: &Project,
        catalog: &Directory,
        directory: Revision,
        node: Id,
        slot: &Slot,
    ) -> Selection {
        let stamp = Stamp { document: project.revision(), directory };
        if stamp != self.stamp {
            self.entries.clear();
            self.stamp = stamp;
        }
        if let Some(hit) = self.entries.get(&(node, slot.id().clone())) {
            return hit.clone();
        }
        let selection = resolve_slot(project, catalog, node, slot);
        self.entries.insert((node, slot.id().clone()), selection.clone());
        selection
    }

    /// How many selections the cache holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds nothing, which is the state after either revision moved.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::collections::BTreeMap;

    use catalog::PropertyValue as CatalogueValue;
    use catalog::{
        ExternalSystemRegistry, Family, FamilyCode, FamilyRegistry, Kind, KindCode, KindRegistry, NomenclatureItem,
        Property as Row, PropertyCode, PropertyRegistry, PropertyType, Slug, Trait,
    };
    use model::EdgeBanding;
    use testkit::{millimetres, sample_edge_material_id, sample_material_id, sample_panel_thickness, sample_project};

    use super::*;

    /// A directory with two families — `oak` and `white` — and one board and one plug in each.
    ///
    /// The oak plug is the first of its family, so a corpus that belongs to `oak` resolves it by rank;
    /// `archived` marks it out of production, which is how a second, changed directory is built.
    fn slot_catalog(archived: bool) -> Directory {
        let code = |value: u16| PropertyCode::new(value).expect("a code");
        let slug = |text: &str| Slug::new(text).expect("a slug");
        let kind = |value: u16| KindCode::new(value).expect("a kind code");
        let family = |value: u16| FamilyCode::new(value).expect("a family code");
        let rows = PropertyRegistry::new([
            Row::new(
                code(1),
                slug("thickness"),
                "Thickness",
                PropertyType::Length { min: millimetres(1), max: millimetres(60) },
            )
            .expect("a row"),
            Row::new(code(2), slug(catalog::DEFAULT_SLUG), "Default", PropertyType::Ref).expect("a row"),
        ])
        .expect("rows");
        let kinds = KindRegistry::new([
            Kind::new(kind(1), slug("sheet-material"), "Sheet", [code(1), code(2)], [code(1)], [Trait::Sheet])
                .expect("a kind"),
            Kind::new(kind(2), slug("plug"), "Plug", [], [], [Trait::Piece]).expect("a kind"),
        ])
        .expect("kinds");
        let families = FamilyRegistry::new([
            Family::new(family(1), slug("oak"), "Oak").expect("a family"),
            Family::new(family(2), slug("white"), "White").expect("a family"),
        ])
        .expect("families");
        let record = |serial: u32, kind_code: u16, name: &str, family_code: u16, rank: Option<u16>| {
            let mut props = BTreeMap::new();
            if kind_code == 1 {
                props.insert(code(1), CatalogueValue::Length(millimetres(18)));
            }
            NomenclatureItem::new(
                ItemId::new(712, serial).expect("an id"),
                kind(kind_code),
                name,
                slug(&format!("record-{serial}")),
                props,
            )
            .expect("an item")
            .in_family(family(family_code), rank)
        };
        let mut oak_plug = record(200, 2, "Oak plug", 1, Some(1));
        if archived {
            oak_plug = oak_plug.archived();
        }
        Directory::new(
            [
                record(100, 1, "Oak board", 1, None),
                record(101, 1, "White board", 2, None),
                oak_plug,
                record(201, 2, "White plug", 2, Some(1)),
            ],
            kinds,
            rows,
            families,
            ExternalSystemRegistry::default(),
        )
        .expect("a directory")
    }

    fn plug_slot() -> Slot {
        Slot::new(
            SlotId::new("plug.confirmat").expect("a slot id"),
            [KindCode::new(2).expect("a kind code")],
            [Trait::Piece],
        )
    }

    fn oak() -> ItemId {
        ItemId::new(712, 100).expect("an id")
    }

    fn oak_plug() -> ItemId {
        ItemId::new(712, 200).expect("an id")
    }

    fn white_plug() -> ItemId {
        ItemId::new(712, 201).expect("an id")
    }

    /// A sample scene whose shelf is cut from the catalogue's oak board.
    fn oak_project() -> (Project, Id) {
        let mut project = sample_project();
        let shelf = Id::new(2).expect("id");
        project.set_panel_material(shelf, oak(), millimetres(18)).expect("the shelf is oak");
        (project, shelf)
    }

    #[test]
    fn catalogue_values_override_stored_fallbacks() {
        let mut project = sample_project();
        let panel_id = Id::new(2).expect("id");
        let edge_id = model::EdgeId::new(1).expect("edge");
        let band =
            EdgeBanding::new(edge_id, sample_edge_material_id(), true, millimetres(2), millimetres(22)).expect("band");
        project.set_edge_banding(panel_id, band).expect("banding");
        project.set_panel_material(panel_id, sample_material_id(), sample_panel_thickness()).expect("material");

        let catalog = testkit::sample_catalog_with(millimetres(25), millimetres(4), millimetres(30));

        let geometry = panel_geometry(&project, &catalog, panel_id).expect("resolved geometry");
        assert_eq!(geometry.thickness(), millimetres(25));
        assert_eq!(geometry.core_y(), millimetres(4));
        assert_eq!(geometry.core_width(), millimetres(496));
        assert_eq!(geometry.overall_width(), millimetres(500));

        let panel = project.panel(panel_id).expect("panel");
        assert_eq!(spec_width(&panel.edge_banding()[0].spec(), &catalog), millimetres(30));
    }

    #[test]
    fn missing_materials_keep_the_stored_fallbacks() {
        let project = sample_project();
        let catalog = Directory::default();
        let geometry = panel_geometry(&project, &catalog, Id::new(2).expect("id")).expect("resolved geometry");
        assert_eq!(geometry.thickness(), sample_panel_thickness());
    }

    #[test]
    fn missing_panels_are_reported() {
        let project = sample_project();
        let error =
            panel_geometry(&project, &Directory::default(), Id::new(999).expect("id")).expect_err("missing panel");
        assert!(matches!(error, ProjectError::PanelNotFound { id } if id == Id::new(999).expect("id")));
    }

    #[test]
    fn a_panel_resolves_its_slot_through_the_board_it_is_cut_from() {
        let (project, shelf) = oak_project();
        assert_eq!(anchor_of(&project, shelf), Some(oak()), "the anchor is the board the panel names");
        let selection = resolve_slot(&project, &slot_catalog(false), shelf, &plug_slot());
        assert_eq!(selection, Selection::Chosen { item: oak_plug(), step: Step::Rank });
    }

    #[test]
    fn a_persons_choice_on_the_node_wins_over_the_family_rule() {
        let (mut project, shelf) = oak_project();
        project
            .set_property(shelf, "plug.confirmat", PropertyValue::item(white_plug()))
            .expect("the person chooses the white plug");
        assert_eq!(choice_of(&project, shelf, &plug_slot()), Some(white_plug()));
        let selection = resolve_slot(&project, &slot_catalog(false), shelf, &plug_slot());
        assert_eq!(selection, Selection::Chosen { item: white_plug(), step: Step::Choice });
    }

    #[test]
    fn a_node_without_a_corpus_and_a_slot_without_a_kind_are_both_misses() {
        let project = sample_project();
        let shelf = Id::new(2).expect("id");
        let Selection::Miss { step, reason } = resolve_slot(&project, &slot_catalog(false), shelf, &plug_slot()) else {
            panic!("the sample shelf names no material");
        };
        assert_eq!(step, Step::Compatibility);
        assert!(reason.contains("names no material") && reason.contains("plug.confirmat"), "{reason}");

        let (project, shelf) = oak_project();
        let absent = Slot::new(
            SlotId::new("plug.minifix").expect("a slot id"),
            [KindCode::new(9).expect("a kind code")],
            [Trait::Piece],
        );
        let Selection::Miss { step, reason } = resolve_slot(&project, &slot_catalog(false), shelf, &absent) else {
            panic!("no package declares kind 9");
        };
        assert_eq!(step, Step::Compatibility);
        assert!(reason.contains("plug.minifix"), "{reason}");
    }

    #[test]
    fn a_cached_selection_answers_until_either_revision_moves() {
        let (mut project, shelf) = oak_project();
        let live = slot_catalog(false);
        let retired = slot_catalog(true);
        let directory = Revision::ZERO;
        let mut cache = SelectionCache::new();
        assert!(cache.is_empty());

        let first = cache.resolve(&project, &live, directory, shelf, &plug_slot());
        assert_eq!(first.chosen(), Some(oak_plug()));
        assert_eq!(cache.len(), 1);

        // A directory that retired the plug answers the cached record while neither revision moved:
        // this is the one thing the cache promises, and the reason it is keyed by both.
        let stale = cache.resolve(&project, &retired, directory, shelf, &plug_slot());
        assert_eq!(stale, first, "the answer is the one the stamp covers");

        // The document moved, so the cache is empty and the same call reads the retired plug: the oak
        // family has nothing left and the anchor names no default, which is a miss on the family step.
        project.set_property(shelf, "note", PropertyValue::text("checked")).expect("one edit");
        let fresh = cache.resolve(&project, &retired, directory, shelf, &plug_slot());
        let Selection::Miss { step, reason } = fresh else {
            panic!("the retired plug leaves the oak family empty");
        };
        assert_eq!(step, Step::Family);
        assert!(reason.contains("family"), "{reason}");
        assert_eq!(cache.len(), 1, "the clear left the one new answer");
    }
}
