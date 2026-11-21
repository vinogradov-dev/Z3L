//! What the interface says about the snap kinds: one word and one hint per kind.
//!
//! The kinds live in `model` (`SnapKind`, `SnapKinds`) and their words live in `text`; this module
//! is the one place the two are joined, so the ribbon's Snaps group, the Settings page's Snaps
//! section and the status bar's readout cannot name one kind three ways (ADR 0086, U116).
//!
//! Changes when: a kind appears in `model::SnapKind`, which turns this match into a compile error.

use model::SnapKind;
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
}
