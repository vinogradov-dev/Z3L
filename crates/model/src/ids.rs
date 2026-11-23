//! Stable document identifiers and the revision counter.
//!
//! Ids are non-zero and never reused while a document is edited, so commands, hardware references
//! and the undo stack refer to a container, panel or catalogue entry without holding a pointer.
//! The small typed ids ([`VertexId`], [`EdgeId`], [`ContourSegmentId`]) label positions inside one
//! panel or ring and share the same non-zero rule.

use std::fmt;
use std::num::NonZeroU64;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// A stable document identifier.
///
/// Identifiers are never reused while a document is edited, so commands, hardware references and
/// the undo stack can refer to a container, panel or catalogue entry without holding a pointer.
/// Zero is reserved for "no entity" and is rejected on construction and deserialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Id(NonZeroU64);

impl Id {
    /// Creates an id from a non-zero raw value.
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// The raw numeric value.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for Id {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// A stable id of one machining operation on a panel.
///
/// Machining ids share the document id space: they are allocated from the project counter, scanned
/// for uniqueness together with nodes, and a file whose `next_id` does not exceed them is rejected.
/// The wrapper keeps a record id from being passed where a node id is expected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct MachiningId(Id);

impl MachiningId {
    /// Creates a machining id from a non-zero raw value.
    pub const fn new(value: u64) -> Option<Self> {
        match Id::new(value) {
            Some(id) => Some(Self(id)),
            None => None,
        }
    }

    /// The raw numeric value.
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// The same value as a document id.
    pub const fn as_id(self) -> Id {
        self.0
    }

    /// Builds a machining id from an id that is already known to be non-zero inside this crate.
    pub(crate) const fn from_id(id: Id) -> Self {
        Self(id)
    }
}

impl fmt::Display for MachiningId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// A monotonically increasing document revision.
///
/// The revision changes after every document mutation and keys every derived cache. It starts at
/// zero and never wraps: [`Revision::checked_next`] returns `None` at the end of the range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(u64);

impl Revision {
    /// The initial revision of an unmodified document.
    pub const ZERO: Self = Self(0);

    /// The raw numeric value.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The next revision, or `None` if the counter would overflow.
    pub fn checked_next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

macro_rules! stable_id {
    ($(#[$meta:meta])* $name:ident, $description:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(u32);

        impl $name {
            /// Creates an id from a non-zero value.
            pub const fn new(value: u32) -> Option<Self> {
                if value == 0 { None } else { Some(Self(value)) }
            }

            /// The numeric value.
            pub const fn get(self) -> u32 {
                self.0
            }

            /// Builds an id from a value that is known to be non-zero inside this crate.
            ///
            /// Only the id types with const constructors call this; the others reach it never, which
            /// is why the allow is part of the generated item rather than of one invocation.
            #[allow(dead_code)]
            pub(crate) const fn from_known(value: u32) -> Self {
                Self(value)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = u32::deserialize(deserializer)?;
                Self::new(value)
                    .ok_or_else(|| <D::Error as serde::de::Error>::custom(concat!($description, " must be non-zero")))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}", self.0)
            }
        }
    };
}

stable_id!(
    /// A corner id of a rectangular panel outline.
    VertexId,
    "vertex id"
);
stable_id!(
    /// An edge id of a rectangular panel outline.
    EdgeId,
    "edge id"
);
stable_id!(
    /// A stable id of one segment of a constructive contour.
    ///
    /// Ids are unique inside their ring and never reused, so a command can name a segment while the
    /// ring is edited. They are labels, not keys: their numbers carry no order.
    ContourSegmentId,
    "contour segment id"
);

/// A catalogue item's identifier: the package that owns the record and its serial inside it.
///
/// An item id is not a document [`Id`]: it is composite, it says which package brought the record
/// and it is never reused, so a scene cannot silently point at a new item with the same number
/// once a package is disconnected. It is written `712-34` and read back from that form, so a
/// package's records stay diffable and a person can read the number that names the package.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ItemId {
    package: u32,
    serial: u32,
}

impl ItemId {
    /// Creates an id from a package number and a serial inside that package.
    ///
    /// Both are non-zero: a package is registered with a number of its own and its records start
    /// at one, so zero cannot name an item.
    pub const fn new(package: u32, serial: u32) -> Option<Self> {
        if package == 0 || serial == 0 {
            return None;
        }
        Some(Self { package, serial })
    }

    /// The package that owns the record.
    pub const fn package(self) -> u32 {
        self.package
    }

    /// The record's serial inside its package.
    pub const fn serial(self) -> u32 {
        self.serial
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}-{}", self.package, self.serial)
    }
}

/// Why a written item id could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ItemIdError {
    /// The text is not `package-serial`.
    #[error("`{0}` is not an item id; write it as `package-serial`, for example `712-34`")]
    Form(String),
    /// One of the two numbers is zero or does not fit in 32 bits.
    #[error("`{value}` in `{text}` is not a package or serial number")]
    Part {
        /// The whole text that was read.
        text: String,
        /// The part that failed.
        value: String,
    },
}

impl std::str::FromStr for ItemId {
    type Err = ItemIdError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (package, serial) = text.split_once('-').ok_or_else(|| ItemIdError::Form(text.to_owned()))?;
        let part = |value: &str| {
            value.parse::<u32>().map_err(|_| ItemIdError::Part { text: text.to_owned(), value: value.to_owned() })
        };
        Self::new(part(package)?, part(serial)?)
            .ok_or_else(|| ItemIdError::Part { text: text.to_owned(), value: text.to_owned() })
    }
}

impl TryFrom<String> for ItemId {
    type Error = ItemIdError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<ItemId> for String {
    fn from(id: ItemId) -> Self {
        id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_id_names_its_package_and_reads_back_from_its_written_form() {
        let id = ItemId::new(712, 34).expect("a non-zero id");
        assert_eq!((id.package(), id.serial()), (712, 34));
        assert_eq!(id.to_string(), "712-34");
        assert_eq!("712-34".parse::<ItemId>(), Ok(id));
        let json = serde_json::to_string(&id).expect("serialize an item id");
        assert_eq!(json, "\"712-34\"", "an item id is a written pair, not a document id number");
        assert_eq!(serde_json::from_str::<ItemId>(&json).expect("read it back"), id);
    }

    #[test]
    fn an_item_id_refuses_zero_and_a_malformed_pair() {
        assert_eq!(ItemId::new(0, 3), None);
        assert_eq!(ItemId::new(3, 0), None);
        assert!("0-3".parse::<ItemId>().is_err(), "a zero package is not an item");
        assert!("3".parse::<ItemId>().is_err());
        assert!("3-4-5".parse::<ItemId>().is_err(), "the pair is two numbers, not three");
        assert!(serde_json::from_str::<ItemId>("\"x-y\"").is_err());
    }

    #[test]
    fn ids_are_non_zero_and_serialize_as_numbers() {
        assert!(Id::new(0).is_none());
        let id = Id::new(42).expect("non-zero id");
        assert_eq!(id.get(), 42);
        assert_eq!(serde_json::to_string(&id).expect("serialize id"), "42");
        assert_eq!(serde_json::from_str::<Id>("42").expect("deserialize id"), id);
        assert!(serde_json::from_str::<Id>("0").is_err());
        assert_eq!(std::mem::size_of::<Option<Id>>(), std::mem::size_of::<u64>());
    }

    #[test]
    fn revisions_are_monotonic_and_do_not_wrap() {
        assert_eq!(Revision::ZERO.get(), 0);
        let one = Revision::ZERO.checked_next().expect("first revision");
        assert_eq!(one.get(), 1);
        assert_eq!(one.checked_next().expect("second revision").get(), 2);

        assert_eq!(Revision(u64::MAX).checked_next(), None);
        assert_eq!(serde_json::from_str::<Revision>("7").expect("deserialize revision").get(), 7);
    }

    #[test]
    fn ids_reject_zero_and_serialize_as_numbers() {
        assert!(VertexId::new(0).is_none());
        assert!(EdgeId::new(0).is_none());
        assert!(serde_json::from_str::<VertexId>("0").is_err());
        assert!(serde_json::from_str::<EdgeId>("0").is_err());

        let id = EdgeId::new(3).expect("non-zero edge id");
        assert_eq!(serde_json::to_string(&id).expect("serialize id"), "3");
        let decoded: EdgeId = serde_json::from_str("3").expect("deserialize id");
        assert_eq!(decoded, id);
    }
}
