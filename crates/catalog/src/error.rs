//! The one sentence a refusal speaks: which package row, which rule.

use model::ItemId;
use thiserror::Error;

use crate::family::FamilyCode;
use crate::kind::KindCode;
use crate::property::PropertyCode;
use crate::slug::MAX_SLUG_CHARS;

/// Why a directory row, a value or a whole package was refused.
///
/// Every variant names the row it refuses, because a package arrives from a supplier and a person
/// has to find the line: "which package, which item, which property, which value"
/// (`docs/domain/nomenclature.md` §16).
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum CatalogError {
    /// A row with an empty name, label or slug.
    #[error("`{row}` is empty; a row that names itself must have one")]
    Empty {
        /// What was empty, in the words a person reads.
        row: &'static str,
    },
    /// A text that cannot be a slug.
    #[error("`{text}` is not a slug: use 1 to {MAX_SLUG_CHARS} characters of ASCII letters, digits, `-` and `_`")]
    Slug {
        /// The text that was refused.
        text: String,
    },
    /// A row longer than its budget.
    #[error("`{text}` is {length} characters long; the limit is {limit}")]
    TooLong {
        /// The text that was refused.
        text: String,
        /// How many characters it has.
        length: usize,
        /// How many it may have.
        limit: usize,
    },
    /// Two rows of one registry claim the same code.
    #[error("`{code}` is declared twice; a registry row has one code")]
    DuplicateCode {
        /// The code that repeated, as the registry prints it.
        code: String,
    },
    /// Two rows of one registry claim the same slug.
    #[error("`{slug}` is declared twice; a registry row has one slug")]
    DuplicateSlug {
        /// The slug that repeated.
        slug: String,
    },
    /// A kind requires a property it does not accept.
    #[error("kind `{kind}` requires property `{property}`, which it does not accept")]
    RequiredNotAccepted {
        /// The kind row.
        kind: KindCode,
        /// The property row.
        property: PropertyCode,
    },
    /// A closed list with no value, or with one value twice.
    #[error("the closed list of `{slug}` is empty or repeats a value")]
    EnumList {
        /// The property row's slug.
        slug: String,
    },
    /// An item names a kind the connected packages do not declare.
    #[error("item `{item}` is of kind `{kind}`, which no connected package declares")]
    UnknownKind {
        /// The item.
        item: ItemId,
        /// The kind it names.
        kind: KindCode,
    },
    /// An item carries a property the registries do not declare.
    #[error("item `{item}` carries property `{property}`, which no connected package declares")]
    UnknownProperty {
        /// The item.
        item: ItemId,
        /// The property it names.
        property: PropertyCode,
    },
    /// An item is missing a property its kind requires.
    #[error("item `{item}` is missing `{property}`, which kind `{kind}` requires")]
    MissingProperty {
        /// The item.
        item: ItemId,
        /// The kind that requires it.
        kind: KindCode,
        /// The property that is absent.
        property: PropertyCode,
    },
    /// An item carries a property its kind does not accept.
    #[error("item `{item}` carries `{property}`, which kind `{kind}` does not accept")]
    PropertyNotAccepted {
        /// The item.
        item: ItemId,
        /// The kind that refuses it.
        kind: KindCode,
        /// The property that is not accepted.
        property: PropertyCode,
    },
    /// A value does not fit the property row that declares it.
    #[error("item `{item}`, property `{property}`: {reason}")]
    Property {
        /// The item.
        item: ItemId,
        /// The property row.
        property: PropertyCode,
        /// What is wrong, in the words the row itself supplies.
        reason: String,
    },
    /// An item repeats another item's slug inside one kind.
    #[error("item `{item}` takes the slug `{slug}`, which item `{other}` already holds in kind `{kind}`")]
    DuplicateItemSlug {
        /// The item.
        item: ItemId,
        /// The item that holds the slug.
        other: ItemId,
        /// The kind both are of.
        kind: KindCode,
        /// The slug both claim.
        slug: String,
    },
    /// An item repeats another item's id.
    #[error("item `{id}` is declared twice; an item id is never reused")]
    DuplicateItem {
        /// The id that repeated.
        id: ItemId,
    },
    /// An item names its external key twice.
    #[error("item `{item}` names `{system}` twice; an item has one key per system")]
    DuplicateExternalKey {
        /// The item.
        item: ItemId,
        /// The system whose key repeated.
        system: String,
    },
    /// A family code is not declared.
    #[error("`{code}` is not a family any connected package declares")]
    UnknownFamily {
        /// The family that was asked for.
        code: FamilyCode,
    },
    /// A date the calendar does not have.
    #[error("`{text}` is not a date this calendar has; write a year, a month and a day")]
    Date {
        /// The date as it was written.
        text: String,
    },
}
