//! The render's material base: what a surface a workshop buys is made of.
//!
//! This crate is the **renderer's** library and not the workshop's catalogue. A workshop names
//! materials in its nomenclature — a chipboard, a glass, a veneer, a lacquer — and a record there
//! points at one record here by [`MaterialId`] (`docs/domain/nomenclature.md` §7, task O16). The
//! render reads only this library, so it can be lifted into another project with its file and its
//! pictures and draw the same surfaces there.
//!
//! The vocabulary follows the renderers people already use (V-Ray, Cycles): a record is a **base** —
//! the albedo and its decor, roughness, metallic, ior, transmission and absorption — and at most one
//! **coat**, a clear dielectric layer with a weight, a gloss in percent and its own ior (ADR 0243).
//! Veneer is a base with a decor, patina is a base, and lacquer is the coat: three words, two places,
//! no record per combination.
//!
//! ```text
//! <the workshop's settings folder>
//!   settings.json          the workspace (the shell's own file)
//!   library.json           this library: one record per material
//!   textures/oak.png       the picture a decor is faced with, at a relative path
//! ```
//!
//! The folder is watched and read whole the way the nomenclature directory is: [`Store`] reads one
//! validated [`Library`] or refuses with a sentence naming the record and the file, and a picture is
//! an ordinary file a person replaces — no hash pins it, unlike the part documents of a package
//! (ADR 0149), because a decor is replaced and not versioned.
//!
//! Numbers are the frame's: a length is [`units::Length`] in micrometres and is written in the file
//! as `…_um`, a colour is linear RGB in `0..=1` like the scene file's light colours, and a gloss is
//! a percent of 0 to 100. The one conversion this crate owns is the file's own: nothing here knows
//! about a document, a scene, a frame or a GPU.
//!
//! Changes when: what a material a shop can name is made of changes, or where its file and its
//! pictures live.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod id;
mod library;
mod material;
mod picture;
mod store;
mod wire;

pub use id::MaterialId;
pub use library::Library;
pub use material::{
    Base, Coat, Decor, MAX_ABSORPTION, MAX_IOR, MAX_NAME_CHARS, MAX_PICTURE_CHARS, Material, MaterialError, Turns,
};
pub use picture::{MAX_PICTURE_SIDE, Picture, PictureError};
pub use store::{ImportError, LIBRARY_FILE, LoadError, MAX_PICTURE_COPIES, Revision, SaveError, Store, TEXTURES_DIR};
pub use wire::LIBRARY_VERSION;
