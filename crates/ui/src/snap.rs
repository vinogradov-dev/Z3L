//! What the interface says about the snap kinds and the snap mode: one word and one hint each.
//!
//! The kinds live in `model` (`SnapKind`, `SnapKinds`, `SnapMode`) and their words live in `text`;
//! this module is the one place the two are joined, so the ribbon's Snaps group, the Settings page's
//! Snaps section and the status bar's readout cannot name one kind three ways (ADR 0086, U116).
//!
//! Changes when: a kind appears in `model::SnapKind` or a mode in `model::SnapMode`, which turns
//! this match into a compile error.

use model::{SnapKind, SnapMode};
use text::Message;

/// The word a kind prints.
pub const fn word(kind: SnapKind) -> Message {
    match kind {
        SnapKind::Vertex => Message::SnapVertex,
        SnapKind::Intersection => Message::SnapIntersection,
        SnapKind::Midpoint => Message::SnapMidpoint,
        SnapKind::Centre => Message::SnapCentre,
        SnapKind::Perpendicular => Message::SnapPerpendicular,
        SnapKind::Tangent => Message::SnapTangent,
        SnapKind::Edge => Message::SnapEdge,
        SnapKind::Face => Message::SnapFace,
        SnapKind::Axis => Message::SnapAxis,
        SnapKind::Grid => Message::SnapGrid,
    }
}

/// The sentence that explains a kind, for a cell's hover and a settings row.
pub const fn hint(kind: SnapKind) -> Message {
    match kind {
        SnapKind::Vertex => Message::SnapVertexHint,
        SnapKind::Intersection => Message::SnapIntersectionHint,
        SnapKind::Midpoint => Message::SnapMidpointHint,
        SnapKind::Centre => Message::SnapCentreHint,
        SnapKind::Perpendicular => Message::SnapPerpendicularHint,
        SnapKind::Tangent => Message::SnapTangentHint,
        SnapKind::Edge => Message::SnapEdgeHint,
        SnapKind::Face => Message::SnapFaceHint,
        SnapKind::Axis => Message::SnapAxisHint,
        SnapKind::Grid => Message::SnapGridHint,
    }
}

/// The word a snap mode prints: what the reading of a corner above the plane is called.
pub const fn mode_word(mode: SnapMode) -> Message {
    match mode {
        SnapMode::Space => Message::SnapModeSpace,
        SnapMode::Foot => Message::SnapModeFoot,
    }
}

/// The sentence that explains a snap mode, for a settings row.
pub const fn mode_hint(mode: SnapMode) -> Message {
    match mode {
        SnapMode::Space => Message::SnapModeSpaceHint,
        SnapMode::Foot => Message::SnapModeFootHint,
    }
}

#[cfg(test)]
mod tests {
    use text::Lang;

    use super::*;

    #[test]
    fn every_kind_prints_two_words_and_two_hints() {
        for kind in SnapKind::ALL {
            for lang in Lang::ALL {
                assert!(!word(kind).text(lang).is_empty(), "{kind:?} has no word");
                assert!(!hint(kind).text(lang).is_empty(), "{kind:?} has no hint");
            }
        }
        assert_ne!(word(SnapKind::Vertex).text(Lang::En), word(SnapKind::Grid).text(Lang::En), "two kinds, two words");
    }

    #[test]
    fn every_mode_prints_two_words_and_two_hints() {
        for mode in SnapMode::ALL {
            for lang in Lang::ALL {
                assert!(!mode_word(mode).text(lang).is_empty(), "{mode:?} has no word");
                assert!(!mode_hint(mode).text(lang).is_empty(), "{mode:?} has no hint");
            }
        }
        assert_ne!(
            mode_word(SnapMode::Space).text(Lang::En),
            mode_word(SnapMode::Foot).text(Lang::En),
            "two modes, two words"
        );
    }
}
