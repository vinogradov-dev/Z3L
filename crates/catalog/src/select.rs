//! Choosing the record that fills a slot: six steps, and the name of the one that failed.
//!
//! A workshop does not ask for "a plug", it asks for the plug that belongs to this corpus
//! (`docs/domain/nomenclature.md` §10). That is a resolution with a fixed order, and every step is a
//! closed predicate over the registries, so the answer is the same on two loads of one directory and
//! a miss is a sentence rather than a `None`:
//!
//! 1. **a person's choice**, which always wins;
//! 2. **compatibility** with the slot: the record's kind and the traits the slot names;
//! 3. **the same family** as the anchor — an oak corpus takes an oak plug;
//! 4. **the nearest rank** to the family's reference sample;
//! 5. **the anchor's own default**, when its family has nothing;
//! 6. **nothing found** — a [`Selection::Miss`] naming the anchor, the slot and the step that failed.
//!
//! What the caller supplies is the **slot** (the id a rule writes and what it accepts) and the
//! person's choice; what the directory supplies is the candidates, their families and their ranks.
//! Caching a result by revision is the caller's, because the anchor and the slot are the caller's.

use std::collections::BTreeSet;
use std::fmt;

use model::ItemId;

use crate::directory::Directory;
use crate::error::CatalogError;
use crate::item::NomenclatureItem;
use crate::kind::{KindCode, Trait};
use crate::property::PropertyValue;

/// The property row an anchor may declare its own fallback in, of type `Ref`.
pub const DEFAULT_SLUG: &str = "default";

/// The id a rule writes for a slot: `plug.confirmat`, `runner.colour`.
///
/// A dot names the family of slots a record fills and a dash joins a word, so this is a vocabulary of
/// its own and not a record slug: `§10` writes the id with a dot, and a package's own slot is a row a
/// rule names, not a path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotId(String);

impl SlotId {
    /// The longest accepted slot id, in characters.
    pub const MAX_CHARS: usize = 48;

    /// Validates a slot id: lower-case ASCII letters and digits, joined by `.`, `-` or `_`.
    pub fn new(text: impl Into<String>) -> Result<Self, CatalogError> {
        let text = text.into();
        let length = text.chars().count();
        let shaped = !text.is_empty()
            && length <= Self::MAX_CHARS
            && text
                .chars()
                .all(|letter| letter.is_ascii_lowercase() || letter.is_ascii_digit() || ".-_".contains(letter))
            && !text.starts_with(['.', '-', '_'])
            && !text.ends_with(['.', '-', '_']);
        if !shaped {
            return Err(CatalogError::SlotId { text });
        }
        Ok(Self(text))
    }

    /// The id as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SlotId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// What a slot accepts: the id a rule writes, and the kinds and traits a candidate must have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    id: SlotId,
    kinds: BTreeSet<KindCode>,
    traits: BTreeSet<Trait>,
}

impl Slot {
    /// A slot that accepts the records of `kinds` carrying `traits`.
    pub fn new(id: SlotId, kinds: impl IntoIterator<Item = KindCode>, traits: impl IntoIterator<Item = Trait>) -> Self {
        Self { id, kinds: kinds.into_iter().collect(), traits: traits.into_iter().collect() }
    }

    /// The id a rule writes: `plug.confirmat`, `runner.colour`.
    pub const fn id(&self) -> &SlotId {
        &self.id
    }

    /// Whether a record of `kind` carrying `traits` may fill the slot.
    pub fn accepts(&self, kind: KindCode, traits: &BTreeSet<Trait>) -> bool {
        self.kinds.contains(&kind) && self.traits.iter().all(|wanted| traits.contains(wanted))
    }
}

/// Which step of the resolution answered, or which one ran out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// A person's explicit choice.
    Choice,
    /// No record of the slot's kind and traits.
    Compatibility,
    /// No candidate in the anchor's family, and no default to stand in.
    Family,
    /// The nearest rank inside the family.
    Rank,
    /// The anchor's own default.
    Default,
}

impl Step {
    /// The step in the words a diagnostic uses.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Choice => "the person's choice",
            Self::Compatibility => "compatibility",
            Self::Family => "the family",
            Self::Rank => "the rank",
            Self::Default => "the default",
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// What the directory answered for one slot: the record, the step, or the miss.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    /// The record that fills the slot, and the step that chose it.
    Chosen {
        /// The record.
        item: ItemId,
        /// The step that answered.
        step: Step,
    },
    /// Nothing fills the slot, and the sentence says which step failed.
    Miss {
        /// The step that ran out, which is already named in `reason`.
        step: Step,
        /// The sentence a card or a diagnostic shows.
        reason: String,
    },
}

impl Selection {
    /// The chosen record, when one was chosen.
    pub const fn chosen(&self) -> Option<ItemId> {
        match self {
            Self::Chosen { item, .. } => Some(*item),
            Self::Miss { .. } => None,
        }
    }

    /// The step that answered or failed.
    pub const fn step(&self) -> Step {
        match self {
            Self::Chosen { step, .. } | Self::Miss { step, .. } => *step,
        }
    }
}

impl Directory {
    /// Resolves the record that fills `slot` for `anchor`, walking the six steps of §10.
    ///
    /// `choice` is the person's explicit pick for this anchor and slot, read by the caller from the
    /// document node: it wins over every rule, and a rule never quietly replaces it. A choice naming a
    /// record the directory does not carry is a miss on the first step, not a silent fall-through — the
    /// person has to see that their pick is gone (`§9.3`).
    pub fn select(&self, anchor: ItemId, slot: &Slot, choice: Option<ItemId>) -> Selection {
        let anchor_name = self.item(anchor).map_or_else(|| anchor.to_string(), |item| item.name().to_owned());
        if let Some(choice) = choice {
            return match self.item(choice) {
                Some(item) if fits(item, slot, self) => Selection::Chosen { item: choice, step: Step::Choice },
                Some(item) => Selection::Miss {
                    step: Step::Compatibility,
                    reason: format!(
                        "`{}` was chosen for {} on \"{anchor_name}\", and is not one of its records",
                        item.name(),
                        slot.id()
                    ),
                },
                None => Selection::Miss {
                    step: Step::Choice,
                    reason: format!(
                        "the record chosen for {} on \"{anchor_name}\" is absent from the directory",
                        slot.id()
                    ),
                },
            };
        }

        let candidates: Vec<&NomenclatureItem> =
            self.items().filter(|item| !item.is_archived() && fits(item, slot, self)).collect();
        if candidates.is_empty() {
            return Selection::Miss {
                step: Step::Compatibility,
                reason: format!("no record may fill {}: the slot names no kind this directory carries", slot.id()),
            };
        }

        let family = self.item(anchor).and_then(NomenclatureItem::family);
        let in_family: Vec<&NomenclatureItem> = match family {
            Some(family) => candidates.iter().copied().filter(|item| item.family() == Some(family)).collect(),
            // An anchor outside every family has nothing to compare, so the step filters nothing.
            None => candidates.clone(),
        };
        if let Some(chosen) = nearest_rank(self, in_family) {
            return Selection::Chosen { item: chosen, step: Step::Rank };
        }

        let family_name = family
            .and_then(|code| self.families().get(code))
            .map_or_else(|| "no family".to_owned(), |row| format!("\"{}\"", row.label()));
        if let Some(default) = self.default_of(anchor, slot) {
            return Selection::Chosen { item: default, step: Step::Default };
        }
        Selection::Miss {
            step: Step::Family,
            reason: format!(
                "for \"{anchor_name}\" no record fills {} in family {family_name}; the family step found nothing and the anchor names no default",
                slot.id()
            ),
        }
    }

    /// The anchor's own fallback for `slot`, when it declares one that the slot accepts.
    ///
    /// `§10` step 5: an anchor may name the record that stands in when its family has nothing, and
    /// the value is an ordinary `Ref` property row slugged `default`, so a card can edit it.
    pub fn default_of(&self, anchor: ItemId, slot: &Slot) -> Option<ItemId> {
        let item = self.item(anchor)?;
        let (_, value) = self.prop_by_slug(item, DEFAULT_SLUG)?;
        let PropertyValue::Ref(target) = value else {
            return None;
        };
        let candidate = self.item(*target)?;
        fits(candidate, slot, self).then_some(*target)
    }
}

/// Whether `item` may fill `slot`: its kind, the slot's traits and its own archive flag.
fn fits(item: &NomenclatureItem, slot: &Slot, directory: &Directory) -> bool {
    let Some(kind) = directory.kinds().get(item.kind()) else {
        return false;
    };
    !item.is_archived() && slot.accepts(item.kind(), kind.traits())
}

/// The member of `members` nearest the family's reference sample, by rank and then by id.
///
/// Distance is measured in ranks, so a family whose sample sits at rank 2 answers rank 3 before rank 9.
/// A family with no sample is read by rank alone, and an unranked member comes last: the package said
/// nothing about where it stands.
fn nearest_rank(directory: &Directory, members: Vec<&NomenclatureItem>) -> Option<ItemId> {
    let sample = members
        .first()
        .and_then(|item| item.family())
        .and_then(|family| directory.families().get(family))
        .and_then(|family| family.sample());
    let sample_rank = sample.and_then(|id| directory.item(id)).and_then(NomenclatureItem::rank);
    members
        .into_iter()
        .min_by_key(|item| {
            let rank = item.rank();
            let distance = match (rank, sample_rank) {
                (Some(rank), Some(sample_rank)) => rank.abs_diff(sample_rank),
                (None, _) => u16::MAX,
                (Some(_), None) => 0,
            };
            (distance, rank.unwrap_or(u16::MAX), item.id())
        })
        .map(NomenclatureItem::id)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::collections::BTreeMap;

    use crate::external::ExternalSystemRegistry;
    use crate::family::{Family, FamilyCode, FamilyRegistry};
    use crate::kind::{Kind, KindRegistry};
    use crate::property::{Property, PropertyCode, PropertyRegistry, PropertyType};
    use units::Length;

    use super::*;

    fn code(value: u16) -> PropertyCode {
        PropertyCode::new(value).expect("a property code")
    }

    fn slug(text: &str) -> crate::slug::Slug {
        crate::slug::Slug::new(text).expect("a slug")
    }

    fn kind_code(value: u16) -> KindCode {
        KindCode::new(value).expect("a kind code")
    }

    fn family_code(value: u16) -> FamilyCode {
        FamilyCode::new(value).expect("a family code")
    }

    fn item(serial: u32) -> ItemId {
        ItemId::new(712, serial).expect("an item id")
    }

    fn slot(text: &str, kinds: impl IntoIterator<Item = KindCode>, traits: impl IntoIterator<Item = Trait>) -> Slot {
        Slot::new(SlotId::new(text).expect("a slot id"), kinds, traits)
    }

    fn plug_slot() -> Slot {
        slot("plug.confirmat", [kind_code(2)], [Trait::Piece])
    }

    fn record(serial: u32, kind: u16, name: &str, family: u16, rank: Option<u16>) -> NomenclatureItem {
        let props = if kind == 1 {
            BTreeMap::from([(code(1), PropertyValue::Length(Length::from_micrometres(18_000)))])
        } else {
            BTreeMap::new()
        };
        NomenclatureItem::new(item(serial), kind_code(kind), name, slug(&format!("record-{serial}")), props)
            .expect("an item")
            .in_family(family_code(family), rank)
    }

    /// A directory with a sheet kind and a plug kind and three families.
    ///
    /// `oak` (code 1) is read against its plug at rank 4 and holds a corpus, a near plug at rank 4 and a
    /// far one at rank 30; `white` (code 2) holds a corpus and one plug; `walnut` (code 3) holds a corpus
    /// and no plug at all, and optionally names the record that stands in for it.
    fn directory(fallback: Option<u32>) -> Directory {
        let properties = PropertyRegistry::new([
            Property::new(
                code(1),
                slug("thickness"),
                "Thickness",
                PropertyType::Length { min: Length::from_micrometres(1_000), max: Length::from_micrometres(60_000) },
            )
            .expect("a row"),
            Property::new(code(2), slug(DEFAULT_SLUG), "Default", PropertyType::Ref).expect("a row"),
        ])
        .expect("two rows");
        let kinds = KindRegistry::new([
            Kind::new(
                kind_code(1),
                slug("sheet-material"),
                "Sheet material",
                [code(1), code(2)],
                [code(1)],
                [Trait::Sheet],
            )
            .expect("a kind"),
            Kind::new(kind_code(2), slug("plug"), "Plug", [], [], [Trait::Piece, Trait::Bought]).expect("a kind"),
        ])
        .expect("two kinds");
        let families = FamilyRegistry::new([
            Family::new(family_code(1), slug("oak"), "Oak").expect("a family").sampled(item(200)),
            Family::new(family_code(2), slug("white"), "White").expect("a family"),
            Family::new(family_code(3), slug("walnut"), "Walnut").expect("a family"),
        ])
        .expect("three families");
        let corpus = |serial: u32, name: &str, family: u16| {
            let mut row = record(serial, 1, name, family, None);
            // Only the walnut corpus of a package names the record that stands in for it.
            if let (Some(fallback), 102) = (fallback, serial) {
                row = row.with_prop(code(2), PropertyValue::Ref(item(fallback)));
            }
            row
        };
        Directory::new(
            [
                corpus(100, "Oak corpus", 1),
                corpus(101, "White corpus", 2),
                corpus(102, "Walnut corpus", 3),
                record(200, 2, "Oak plug", 1, Some(4)),
                record(201, 2, "White plug", 2, Some(5)),
                record(202, 2, "Oak plug, far", 1, Some(30)),
            ],
            kinds,
            properties,
            families,
            ExternalSystemRegistry::default(),
        )
        .expect("a directory")
    }

    #[test]
    fn a_slot_id_is_lower_case_joined_by_a_dot_and_a_word_never_a_path() {
        assert_eq!(SlotId::new("plug.confirmat").expect("an id").as_str(), "plug.confirmat");
        assert_eq!(SlotId::new("runner.colour").expect("an id").to_string(), "runner.colour");
        assert!(SlotId::new("").is_err());
        assert!(SlotId::new("Plug.confirmat").is_err(), "a person reads lower case");
        assert!(SlotId::new("plug/confirmat").is_err(), "a slot is not a path");
        assert!(SlotId::new(".plug").is_err(), "and it does not start with its joint");
    }

    #[test]
    fn an_oak_corpus_picks_the_oak_plug_and_a_white_corpus_picks_its_own() {
        let directory = directory(None);
        let slot = plug_slot();
        let oak = directory.select(item(100), &slot, None);
        assert_eq!(oak, Selection::Chosen { item: item(200), step: Step::Rank }, "the nearest oak rank wins");
        let white = directory.select(item(101), &slot, None);
        assert_eq!(white, Selection::Chosen { item: item(201), step: Step::Rank }, "and the white corpus re-picks");
        assert_ne!(oak.chosen(), white.chosen());
    }

    #[test]
    fn the_nearest_rank_is_measured_from_the_family_sample_outwards() {
        // The oak family is read against the plug at rank 4, so the plug at rank 4 answers before the
        // one at rank 30; without a sample the lower rank would answer, which is the same record here
        // only because the package ranked it first.
        let directory = directory(None);
        let family = family_code(1);
        let members: Vec<&NomenclatureItem> =
            directory.family_members(family).filter(|item| item.kind() == kind_code(2)).collect();
        assert_eq!(nearest_rank(&directory, members), Some(item(200)));

        // `white` names no sample at all, so the one white plug answers by rank alone.
        let family = family_code(2);
        let members: Vec<&NomenclatureItem> =
            directory.family_members(family).filter(|item| item.kind() == kind_code(2)).collect();
        assert_eq!(nearest_rank(&directory, members), Some(item(201)), "a family with no sample is read by rank");
    }

    #[test]
    fn a_persons_choice_wins_and_a_choice_of_another_slot_is_a_diagnostic() {
        let directory = directory(None);
        let slot = plug_slot();
        let chosen = directory.select(item(100), &slot, Some(item(201)));
        assert_eq!(
            chosen,
            Selection::Chosen { item: item(201), step: Step::Choice },
            "the person's pick is never overruled silently"
        );

        let refused = directory.select(item(100), &slot, Some(item(101)));
        let Selection::Miss { step, reason } = refused else {
            panic!("a board cannot fill a plug's slot");
        };
        assert_eq!(step, Step::Compatibility);
        assert!(reason.contains("White corpus") && reason.contains("plug.confirmat"), "{reason}");

        let absent = directory.select(item(100), &slot, Some(item(999)));
        assert_eq!(absent.step(), Step::Choice, "a choice whose record is gone is the first step's miss");
    }

    #[test]
    fn a_miss_names_the_slot_the_anchor_and_the_step_that_failed() {
        // No record of the slot's kind at all: the compatibility step is the one that ran out.
        let directory = directory(None);
        let absent_kind = slot("plug.minifix", [kind_code(7)], [Trait::Piece]);
        let Selection::Miss { step, reason } = directory.select(item(100), &absent_kind, None) else {
            panic!("no package declares kind 7");
        };
        assert_eq!(step, Step::Compatibility);
        assert!(reason.contains("plug.minifix"), "{reason}");

        // A kind the directory carries, a family with none of it and no default: the family step.
        let shelf = slot("shelf.pin", [kind_code(2)], [Trait::Bought]);
        let Selection::Miss { step, reason } = directory.select(item(102), &shelf, None) else {
            panic!("the walnut family holds no bought record");
        };
        assert_eq!(step, Step::Family);
        assert!(reason.contains("Walnut corpus") && reason.contains("shelf.pin"), "{reason}");
        assert!(reason.contains("\"Walnut\""), "the family is named as a person reads it: {reason}");
    }

    #[test]
    fn an_anchor_default_answers_when_its_family_has_nothing() {
        // `walnut` holds no plug, and its corpus names the oak plug as the record that stands in.
        let directory = directory(Some(202));
        let chosen = directory.select(item(102), &plug_slot(), None);
        assert_eq!(chosen, Selection::Chosen { item: item(202), step: Step::Default });
        assert_eq!(directory.default_of(item(102), &plug_slot()), Some(item(202)));
        assert_eq!(directory.default_of(item(100), &plug_slot()), None, "an oak corpus names no fallback");
    }
}
