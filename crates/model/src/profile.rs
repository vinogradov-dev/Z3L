//! The workshop's own numbers: the defaults a rule reads when the document carries none.
//!
//! These values are settings and never document data (`PROJECT.md`, §Domain specifications): a scene
//! file, a package and a product know nothing about the shop that made them, so the inspector reads
//! them from here and a workshop with another board changes one value instead of editing code
//! (ADR 0191). The record's values are the defaults, and a later session fills the same type from
//! the workspace settings (J12) without moving a rule.
//!
//! The type is plain data with a [`Default`] a caller can start from. It is deliberately small: a
//! field exists when a rule reads it, so the profile cannot drift into a second, unused copy of the
//! standards. The material, the drawer type, the edge type and the rail type a prototype will seed
//! from are the same setting one layer up and arrive with the prototypes that read them.
//!
//! Changes when: a rule needs a number the document cannot answer, and the workshop owns it.

use serde::{Deserialize, Serialize};
use units::Length;

/// The sheet a part is cut from, when neither the document nor the catalogue names one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sheet {
    /// The long side of the sheet.
    pub length: Length,
    /// The short side of the sheet.
    pub width: Length,
}

impl Default for Sheet {
    /// The 2800 x 2070 mm sheet the starter catalogue stocks, the commonest board a workshop buys.
    fn default() -> Self {
        Self { length: Length::from_micrometres(2_800_000), width: Length::from_micrometres(2_070_000) }
    }
}

/// The numbers a workshop builds by: gaps, tolerances, the rail's drop and the sheet.
///
/// Every field is a `Length` in whole micrometres, so a rule can compare it with the document
/// without a second unit. A missing field of a serialized profile takes its default, and a field
/// this build does not read is refused rather than ignored (`AGENTS.md` §4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkshopProfile {
    /// The clearance on each side of a shelf in its opening: the shelf is cut this much narrower
    /// than the span between its uprights, on both ends together with [`Self::shelf_loose`].
    pub shelf_gap: Length,
    /// How much narrower than the nominal shelf [`Self::shelf_gap`] allows still counts as loose.
    ///
    /// A shelf narrower than `opening - 2 * shelf_gap - shelf_loose` is a warning and not a fit.
    pub shelf_loose: Length,
    /// The gap between two fronts of a pair, and the reveal of a single front.
    pub door_gap: Length,
    /// How far a front's gap or width may drift from the nominal before it is a warning.
    pub door_tolerance: Length,
    /// How far the centre of a hanging rail stands below the underside of the top above it.
    pub rail_drop: Length,
    /// How far [`Self::rail_drop`] may drift before it is a warning.
    pub rail_tolerance: Length,
    /// The least clear distance between a hanging rail and a shelf.
    pub rail_clearance: Length,
    /// The sheet a part is cut from when no material names one.
    pub sheet: Sheet,
}

impl Default for WorkshopProfile {
    /// The numbers ADR 0191 records: a 2 mm shelf gap, a 2 mm front gap with a 0.5 mm tolerance, a
    /// rail 40 mm below the top with 5 mm of slack and 50 mm clear of a shelf, and a 2800 x 2070 mm
    /// sheet.
    fn default() -> Self {
        Self {
            shelf_gap: Length::from_micrometres(2_000),
            shelf_loose: Length::from_micrometres(2_000),
            door_gap: Length::from_micrometres(2_000),
            door_tolerance: Length::from_micrometres(500),
            rail_drop: Length::from_micrometres(40_000),
            rail_tolerance: Length::from_micrometres(5_000),
            rail_clearance: Length::from_micrometres(50_000),
            sheet: Sheet::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn the_defaults_are_the_numbers_the_record_holds() {
        let profile = WorkshopProfile::default();
        assert_eq!(profile.shelf_gap, Length::from_millimetres(2).expect("a length"));
        assert_eq!(profile.door_gap, Length::from_millimetres(2).expect("a length"));
        assert_eq!(profile.door_tolerance, Length::from_micrometres(500));
        assert_eq!(profile.rail_drop, Length::from_millimetres(40).expect("a length"));
        assert_eq!(profile.rail_tolerance, Length::from_millimetres(5).expect("a length"));
        assert_eq!(profile.rail_clearance, Length::from_millimetres(50).expect("a length"));
        assert_eq!(profile.sheet.length, Length::from_millimetres(2_800).expect("a length"));
        assert_eq!(profile.sheet.width, Length::from_millimetres(2_070).expect("a length"));
    }

    #[test]
    fn a_missing_field_takes_its_default_and_an_unknown_one_is_refused() {
        let read: WorkshopProfile = serde_json::from_str(r#"{"door_gap": 3000}"#).expect("a profile with one field");
        assert_eq!(read.door_gap, Length::from_micrometres(3_000));
        assert_eq!(read.rail_drop, WorkshopProfile::default().rail_drop, "the rest keeps its default");
        assert!(
            serde_json::from_str::<WorkshopProfile>(r#"{"door_gap": 3000, "door_gapp": 1}"#).is_err(),
            "a key the profile does not read is refused, not ignored"
        );
    }

    #[test]
    fn the_profile_round_trips_through_json() {
        let profile = WorkshopProfile { shelf_gap: Length::from_micrometres(4_000), ..WorkshopProfile::default() };
        let text = serde_json::to_string(&profile).expect("serialize");
        let read: WorkshopProfile = serde_json::from_str(&text).expect("read back");
        assert_eq!(read, profile);
    }
}
