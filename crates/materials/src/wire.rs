//! The written form: the one file a workshop carries, and the pictures beside it.
//!
//! The file is a person's to read and to edit before the interface learns to edit it, so it names
//! its channels plainly and refuses a key it does not know (`AGENTS.md` §4.5): an unknown field is a
//! mistake in a hand-edited file, not a value to ignore. A length is micrometres with a `_um`
//! suffix, as the scene file and a catalogue package write it, so one integer travels from the file
//! to a frame and back without a rounding step. A colour is linear RGB in `0..=1`, the same space
//! `scene::Drawable::color` and the scene file's light colours are in.
//!
//! Only the channels a renderer reads are here; the version is the whole file's, and a file that
//! declares another one is a refusal naming both numbers (ADR 0096: before the first release the
//! format has no past, so a bump replaces this layout instead of migrating it).

use std::path::Path;

use serde::{Deserialize, Serialize};

use units::Length;

use crate::id::MaterialId;
use crate::library::Library;
use crate::material::{Base, Coat, Decor, Material, MaterialError, Turns};
use crate::store::LoadError;

/// The schema of `library.json` this build writes and reads.
pub const LIBRARY_VERSION: u32 = 1;

/// The roughness a record that names none is: the tracer's matte, not a number invented here.
const MATTE_ROUGHNESS: f32 = 1.0;

/// The index of refraction a record that names none is: glass, the tracer's own default.
const GLASS_IOR: f32 = 1.5;

/// One library, as its file holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LibraryWire {
    /// The schema of this file.
    pub(crate) version: u32,
    /// One record per material, in the order a person reads them.
    #[serde(default)]
    pub(crate) materials: Vec<MaterialWire>,
}

/// One material, as its file holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MaterialWire {
    /// The stable id a catalogue record names.
    pub(crate) id: MaterialId,
    /// The name a person reads.
    pub(crate) name: String,
    /// What the part is made of, or nothing for a finish that is only a coat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) base: Option<BaseWire>,
    /// The coat over the base, or nothing for a surface with no finish.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) coat: Option<CoatWire>,
}

/// A base, as its file holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BaseWire {
    /// The linear RGB albedo the decor multiplies.
    pub(crate) colour: [f32; 3],
    /// The picture the surface is faced with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) decor: Option<DecorWire>,
    /// Microfacet roughness, `0..=1`.
    #[serde(default = "matte_roughness")]
    pub(crate) roughness: f32,
    /// `0` for a dielectric, `1` for a metal.
    #[serde(default)]
    pub(crate) metallic: f32,
    /// Index of refraction of the dielectric.
    #[serde(default = "glass_ior")]
    pub(crate) ior: f32,
    /// How much of the beam crosses the base, `0..=1`.
    #[serde(default)]
    pub(crate) transmission: f32,
    /// Beer-Lambert extinction per millimetre, linear RGB.
    #[serde(default)]
    pub(crate) absorption: [f32; 3],
}

/// A decor, as its file holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DecorWire {
    /// The picture, as a path relative to the library's folder.
    pub(crate) picture: String,
    /// How many micrometres of board one image width spans.
    pub(crate) covers_um: i64,
    /// Quarter turns of the grain, `0..=3`.
    #[serde(default)]
    pub(crate) turns: u8,
}

/// A coat, as its file holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CoatWire {
    /// How much of the surface the coat covers, `0..=1`.
    pub(crate) weight: f32,
    /// The gloss in percent, `0..=100`.
    pub(crate) gloss: f32,
    /// The coat's own index of refraction.
    #[serde(default = "glass_ior")]
    pub(crate) ior: f32,
}

fn matte_roughness() -> f32 {
    MATTE_ROUGHNESS
}

fn glass_ior() -> f32 {
    GLASS_IOR
}

impl LibraryWire {
    /// The records this file holds, validated as one library, with a refusal that names the record
    /// and the file a person has to open.
    pub(crate) fn into_library(self, path: &Path) -> Result<Library, LoadError> {
        let mut records = Vec::with_capacity(self.materials.len());
        for material in self.materials {
            let id = material.id;
            records.push(material.into_material().map_err(|error| LoadError::Record {
                path: path.to_path_buf(),
                material: Some(id),
                reason: error.to_string(),
            })?);
        }
        Library::new(records).map_err(|error| LoadError::Record {
            path: path.to_path_buf(),
            material: None,
            reason: error.to_string(),
        })
    }
}

impl Library {
    /// This library as its file holds it.
    pub(crate) fn to_wire(&self) -> LibraryWire {
        LibraryWire { version: LIBRARY_VERSION, materials: self.records().map(Material::to_wire).collect() }
    }

    /// The pictures the records name, relative to the library's folder, in id order.
    pub(crate) fn pictures(&self) -> impl Iterator<Item = (&Material, &Decor)> {
        self.records().filter_map(|material| material.base()?.decor().map(|decor| (material, decor)))
    }
}

impl MaterialWire {
    /// The record this file holds, validated.
    pub(crate) fn into_material(self) -> Result<Material, MaterialError> {
        let base = self.base.map(BaseWire::into_base).transpose()?;
        let coat = self.coat.map(CoatWire::into_coat).transpose()?;
        Material::new(self.id, self.name, base, coat)
    }
}

impl Material {
    /// This record as its file holds it.
    pub(crate) fn to_wire(&self) -> MaterialWire {
        MaterialWire {
            id: self.id(),
            name: self.name().to_owned(),
            base: self.base().map(BaseWire::of),
            coat: self.coat().map(CoatWire::of),
        }
    }
}

impl BaseWire {
    fn into_base(self) -> Result<Base, MaterialError> {
        let decor = self.decor.map(DecorWire::into_decor).transpose()?;
        Base::new(self.colour, decor, self.roughness, self.metallic, self.ior, self.transmission, self.absorption)
    }

    fn of(base: &Base) -> Self {
        Self {
            colour: base.colour(),
            decor: base.decor().map(DecorWire::of),
            roughness: base.roughness(),
            metallic: base.metallic(),
            ior: base.ior(),
            transmission: base.transmission(),
            absorption: base.absorption(),
        }
    }
}

impl DecorWire {
    fn into_decor(self) -> Result<Decor, MaterialError> {
        let turns = Turns::new(self.turns).ok_or(MaterialError::Turns { turns: self.turns })?;
        Decor::new(self.picture, Length::from_micrometres(self.covers_um), turns)
    }

    fn of(decor: &Decor) -> Self {
        Self {
            picture: decor.picture().to_owned(),
            covers_um: decor.covers().micrometres(),
            turns: decor.turns().get(),
        }
    }
}

impl CoatWire {
    fn into_coat(self) -> Result<Coat, MaterialError> {
        Coat::new(self.weight, self.gloss, self.ior)
    }

    fn of(coat: &Coat) -> Self {
        Self { weight: coat.weight(), gloss: coat.gloss(), ior: coat.ior() }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn id(value: u32) -> MaterialId {
        MaterialId::new(value).expect("a non-zero id")
    }

    /// A record with every channel away from its default, so the round trip proves each one.
    fn full() -> Material {
        let decor = Decor::new("textures/oak.png", Length::from_millimetres(600).expect("600 mm"), Turns::ACROSS)
            .expect("a decor");
        let base = Base::new([0.62, 0.42, 0.22], Some(decor), 0.7, 1.0, 1.9, 0.4, [0.5, 1.5, 2.5]).expect("a metal");
        let coat = Coat::new(0.75, 70.0, 1.62).expect("a coat");
        Material::new(id(3), "Oak, 70 % lacquer", Some(base), Some(coat)).expect("a record")
    }

    #[test]
    fn every_channel_survives_the_written_form() {
        let material = full();
        let text = serde_json::to_string(&material.to_wire()).expect("serialise");
        let read: MaterialWire = serde_json::from_str(&text).expect("parse");
        assert_eq!(read.into_material().expect("a record"), material);
        // The file writes a length in micrometres and a turn as its count, not as a newtype.
        assert!(text.contains("\"covers_um\":600000"), "{text}");
        assert!(text.contains("\"turns\":1"), "{text}");
    }

    #[test]
    fn a_file_that_names_no_roughness_is_a_matte_board_and_not_a_hole() {
        let material = MaterialWire::into_material(MaterialWire {
            id: id(1),
            name: "White".to_owned(),
            base: Some(BaseWire {
                colour: [0.9, 0.9, 0.9],
                decor: None,
                roughness: MATTE_ROUGHNESS,
                metallic: 0.0,
                ior: GLASS_IOR,
                transmission: 0.0,
                absorption: [0.0; 3],
            }),
            coat: None,
        })
        .expect("a board");
        let base = material.base().expect("a base");
        assert_eq!((base.roughness(), base.metallic(), base.ior()), (1.0, 0.0, 1.5));
        let text =
            r#"{ "version": 1, "materials": [ { "id": 1, "name": "White", "base": { "colour": [0.9, 0.9, 0.9] } } ] }"#;
        let wire: LibraryWire = serde_json::from_str(text).expect("the omitted channels are the defaults");
        let library = wire.into_library(Path::new("library.json")).expect("a library");
        assert_eq!(library.get(id(1)).and_then(Material::base).map(Base::roughness), Some(1.0));
    }

    #[test]
    fn an_unknown_key_and_a_past_turn_are_refused() {
        let unknown = r#"{ "version": 1, "materials": [ { "id": 1, "name": "Oak", "shininess": 4 } ] }"#;
        assert!(serde_json::from_str::<LibraryWire>(unknown).is_err(), "an unknown channel is a mistake");
        let turns = r#"{ "version": 1, "materials": [ { "id": 1, "name": "Oak", "base": { "colour": [0.5, 0.4, 0.3], "decor": { "picture": "textures/oak.png", "covers_um": 600000, "turns": 7 } } } ] }"#;
        let wire: LibraryWire = serde_json::from_str(turns).expect("the file parses");
        let refusal = wire.into_library(Path::new("library.json")).expect_err("seven quarter turns");
        assert!(refusal.to_string().contains("material 1") && refusal.to_string().contains("7"), "{refusal}");
        let zero = r#"{ "version": 1, "materials": [ { "id": 0, "name": "Oak" } ] }"#;
        assert!(serde_json::from_str::<LibraryWire>(zero).is_err(), "zero names no material");
    }
}
