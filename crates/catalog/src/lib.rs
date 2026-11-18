//! The nomenclature directory: the items a workshop works from.
//!
//! This crate is the directory's data and its rules, never its files: a package is read and written
//! by the shell, and `param`, `mesh`, `engine` and `export` reach the directory through the queries
//! this crate answers (`docs/domain/nomenclature.md` §15).
//!
//! The design is one decision repeated at four scales (ADR 0148):
//!
//! - **one item envelope** ([`NomenclatureItem`]): an id, a kind, a name, a slug, typed properties
//!   and nothing a consumer may special-case;
//! - **registries, not code**: [`Kind`], [`Property`] and [`Family`] are rows a package carries, so a
//!   workshop's own vocabulary arrives as data;
//! - **traits are the closed vocabulary** ([`Trait`]): a consumer asks whether an item is a sheet or
//!   banded, never what its kind is called, so a new kind needs no code change;
//! - **a snapshot, not a live store**: a reader holds one value of one revision and the shell swaps
//!   it whole (ADR 0148, `§13`).
//!
//! The `ItemId` itself lives in `model`, because a scene node stores it and `model` must not depend
//! on the directory: the id is document data, the record behind it is not.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod directory;
mod error;
mod family;
mod item;
mod kind;
mod money;
mod property;
mod slug;
mod view;

pub use directory::Directory;
pub use error::CatalogError;
pub use family::{Family, FamilyCode, FamilyRegistry};
pub use item::{ExternalKey, ExternalSystemId, MAX_EXTERNAL_KEYS, MAX_ITEM_NAME_CHARS, NomenclatureItem};
pub use kind::{Kind, KindCode, KindRegistry, Trait};
pub use money::{Currency, CurrencyError, Money};
pub use property::{Date, Property, PropertyCode, PropertyType, PropertyValue, Rgb};
pub use slug::{MAX_SLUG_CHARS, Slug};
pub use view::{
    BandingView, BoardView, COLOR_SLUG, SHEET_LENGTH_SLUG, SHEET_WIDTH_SLUG, SheetView, THICKNESS_SLUG, WIDTH_SLUG,
};
