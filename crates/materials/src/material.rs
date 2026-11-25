//! One material: the base a part is made of and, at most, the one coat over it.
//!
//! The channels are the renderer's own vocabulary and nothing else: a colour, the decor it is faced
//! with, roughness, metallic, ior, transmission and absorption for the base, and a weight, a gloss
//! and an ior for the coat (ADR 0243). A field nothing draws is not written here, so a record cannot
//! promise a picture that no shader reads.
//!
//! Every number is validated where the record is built, and a refusal names the field and the range:
//! a hostile file is a sentence a person can act on, never a surface that shades itself.

use units::Length;

use crate::id::MaterialId;

/// The longest name a record may carry, in characters: a card prints it on one line.
pub const MAX_NAME_CHARS: usize = 120;

/// The longest relative path a decor may name, in characters.
pub const MAX_PICTURE_CHARS: usize = 240;

/// The highest index of refraction a record may name (diamond is 2.42).
pub const MAX_IOR: f32 = 4.0;

/// The strongest absorption a record may name, per millimetre, in linear RGB.
///
/// A metre of it attenuates by `e^-4096`, which is opaque in any beam the tracer traces; past this
/// the number describes nothing a picture could show.
pub const MAX_ABSORPTION: f32 = 4096.0;

/// Why a record, or a library of them, was refused.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum MaterialError {
    /// A record names nothing.
    #[error("a material has no name")]
    NameEmpty,
    /// A name past [`MAX_NAME_CHARS`].
    #[error("the name `{name}` is longer than {MAX_NAME_CHARS} characters")]
    NameTooLong {
        /// The name that was given.
        name: String,
    },
    /// A record is neither a base nor a coat: it describes no surface at all.
    #[error("a material has neither a base (the board) nor a coat (the finish)")]
    NoSurface,
    /// A channel outside the range the tracer reads.
    #[error("`{field}` is {value}, which is not a number inside {min}..={max}")]
    Field {
        /// The channel, as the file writes it (`base.roughness`, `coat.gloss`).
        field: &'static str,
        /// The value the file carried.
        value: f32,
        /// The smallest value the channel takes.
        min: f32,
        /// The largest value the channel takes.
        max: f32,
    },
    /// A decor that names no picture.
    #[error("a decor names no picture file")]
    PictureEmpty,
    /// A picture past [`MAX_PICTURE_CHARS`].
    #[error("the decor picture `{picture}` is a path longer than {MAX_PICTURE_CHARS} characters")]
    PictureTooLong {
        /// The path that was given.
        picture: String,
    },
    /// A picture path that leaves the folder the library owns.
    #[error("the decor picture `{picture}` is not a path inside the textures folder: it must be relative")]
    PictureOutside {
        /// The path that was given.
        picture: String,
    },
    /// A decor that covers no board.
    #[error("a decor covers {covers:?}: it must cover more than nothing")]
    Covers {
        /// The length the record named.
        covers: Length,
    },
    /// A quarter-turn count past three.
    #[error("{turns} quarter turns is not a decor turn: the file names 0, 1, 2 or 3")]
    Turns {
        /// The count that was given.
        turns: u8,
    },
    /// An id two records claim.
    #[error("the library holds material {id} twice: an id is one record")]
    Duplicate {
        /// The id both records named.
        id: MaterialId,
    },
    /// Two records of one library with one name, which a person cannot tell apart.
    #[error("the library holds a second material named `{name}`: pick another name")]
    DuplicateName {
        /// The name both records carried.
        name: String,
    },
}

/// A channel that must be finite and inside `min..=max`.
fn channel(field: &'static str, value: f32, min: f32, max: f32) -> Result<f32, MaterialError> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(MaterialError::Field { field, value, min, max })
    }
}

/// Three channels that must be finite and inside `min..=max`.
fn channels(field: &'static str, value: [f32; 3], min: f32, max: f32) -> Result<[f32; 3], MaterialError> {
    for component in value {
        channel(field, component, min, max)?;
    }
    Ok(value)
}

/// How many quarter turns a decor picture is turned by around the surface's normal, `0..=3`.
///
/// The same number [`scene::Decor`] carries, stated here so the renderer's library owns its own
/// vocabulary: a decor has a grain, and a piece of board is cut along it or across it.
///
/// [`scene::Decor`]: https://docs.rs/scene
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Turns(u8);

impl Turns {
    /// The picture as its own file has it: its width runs along the surface's local `+X`.
    pub const ALONG: Self = Self(0);

    /// A quarter turn: the picture's width — its grain — runs along the surface's local `+Y`.
    pub const ACROSS: Self = Self(1);

    /// `turns` quarter turns, or `None` past three, which is the count a file may name.
    pub const fn new(turns: u8) -> Option<Self> {
        if turns < 4 { Some(Self(turns)) } else { None }
    }

    /// How many quarter turns.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// The picture a base is faced with, how much board it covers and the way its grain runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decor {
    picture: String,
    covers: Length,
    turns: Turns,
}

impl Decor {
    /// The decor `picture` repeats every `covers` millimetres of board, turned by `turns`.
    ///
    /// The picture is a path relative to the library's folder — `textures/oak.png` — because the
    /// textures folder travels with the file and an absolute path would pin one machine's disk. The
    /// file itself is read when a frame first asks for the decor ([`crate::Store::picture`]), so a
    /// person replaces a scan without touching the library.
    pub fn new(picture: impl Into<String>, covers: Length, turns: Turns) -> Result<Self, MaterialError> {
        let picture = picture.into();
        let trimmed = picture.trim();
        if trimmed.is_empty() {
            return Err(MaterialError::PictureEmpty);
        }
        if trimmed.chars().count() > MAX_PICTURE_CHARS {
            return Err(MaterialError::PictureTooLong { picture });
        }
        let path = std::path::Path::new(trimmed);
        if path.is_absolute()
            || path
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_) | std::path::Component::CurDir))
        {
            return Err(MaterialError::PictureOutside { picture });
        }
        if covers.micrometres() <= 0 {
            return Err(MaterialError::Covers { covers });
        }
        Ok(Self { picture, covers, turns })
    }

    /// The picture, as a path relative to the library's folder.
    pub fn picture(&self) -> &str {
        &self.picture
    }

    /// How many millimetres of board one image width spans.
    pub const fn covers(&self) -> Length {
        self.covers
    }

    /// The quarter turns the picture is turned by.
    pub const fn turns(&self) -> Turns {
        self.turns
    }
}

/// The base: what the part is made of.
#[derive(Clone, Debug, PartialEq)]
pub struct Base {
    colour: [f32; 3],
    decor: Option<Decor>,
    roughness: f32,
    metallic: f32,
    ior: f32,
    transmission: f32,
    absorption: [f32; 3],
}

impl Base {
    /// A base with every channel stated.
    ///
    /// A record's roughness, metallic, ior, transmission and absorption name their ranges in
    /// [`Base::matte`]; a value outside one is a [`MaterialError::Field`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        colour: [f32; 3],
        decor: Option<Decor>,
        roughness: f32,
        metallic: f32,
        ior: f32,
        transmission: f32,
        absorption: [f32; 3],
    ) -> Result<Self, MaterialError> {
        Ok(Self {
            colour: channels("base colour", colour, 0.0, 1.0)?,
            decor,
            roughness: channel("base roughness", roughness, 0.0, 1.0)?,
            metallic: channel("base metallic", metallic, 0.0, 1.0)?,
            ior: channel("base ior", ior, 1.0, MAX_IOR)?,
            transmission: channel("base transmission", transmission, 0.0, 1.0)?,
            absorption: channels("base absorption", absorption, 0.0, MAX_ABSORPTION)?,
        })
    }

    /// Painted or veneered board: opaque, dielectric, as rough as a sample gets.
    ///
    /// The five channels a record usually leaves out are the tracer's own defaults — matte, not
    /// metal, a 1.5 glass index, opaque, and no absorption — so a file that names a colour and a
    /// picture describes a board and not a hole in its own defaults.
    pub fn matte(colour: [f32; 3]) -> Result<Self, MaterialError> {
        Self::new(colour, None, 1.0, 0.0, 1.5, 0.0, [0.0; 3])
    }

    /// The linear RGB albedo the decor multiplies.
    pub const fn colour(&self) -> [f32; 3] {
        self.colour
    }

    /// The picture the base is faced with, or `None` for the flat colour alone.
    pub const fn decor(&self) -> Option<&Decor> {
        self.decor.as_ref()
    }

    /// Microfacet roughness of the base, `0..=1`.
    pub const fn roughness(&self) -> f32 {
        self.roughness
    }

    /// `0` for a dielectric, `1` for a metal.
    pub const fn metallic(&self) -> f32 {
        self.metallic
    }

    /// Index of refraction of the dielectric.
    pub const fn ior(&self) -> f32 {
        self.ior
    }

    /// How much of the beam crosses the base, `0..=1`.
    pub const fn transmission(&self) -> f32 {
        self.transmission
    }

    /// Beer-Lambert extinction inside the base, per millimetre, linear RGB.
    pub const fn absorption(&self) -> [f32; 3] {
        self.absorption
    }
}

/// The coat: one clear dielectric layer over the base, which is what a lacquer is (ADR 0243).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coat {
    weight: f32,
    gloss: f32,
    ior: f32,
}

impl Coat {
    /// The default index of refraction of a clear coat: the glass index a lacquer reads as.
    pub const IOR: f32 = 1.5;

    /// A coat of `weight` in `0..=1`, glossed `gloss` percent, with its own `ior`.
    ///
    /// The gloss is the number a workshop says — 90 % is a lacquer, 50 % is a satin — and the
    /// renderer derives its roughness from it once, where a record becomes a frame, so the shader
    /// never sees a percent (ADR 0243).
    pub fn new(weight: f32, gloss: f32, ior: f32) -> Result<Self, MaterialError> {
        Ok(Self {
            weight: channel("coat weight", weight, 0.0, 1.0)?,
            gloss: channel("coat gloss", gloss, 0.0, 100.0)?,
            ior: channel("coat ior", ior, 1.0, MAX_IOR)?,
        })
    }

    /// A full coat at `gloss` percent and the default index.
    pub fn glossed(gloss: f32) -> Result<Self, MaterialError> {
        Self::new(1.0, gloss, Self::IOR)
    }

    /// How much of the surface the coat covers, `0..=1`.
    pub const fn weight(&self) -> f32 {
        self.weight
    }

    /// The gloss in percent, `0..=100`.
    pub const fn gloss(&self) -> f32 {
        self.gloss
    }

    /// The coat's own index of refraction.
    pub const fn ior(&self) -> f32 {
        self.ior
    }
}

/// One material: a stable id, the name a person reads, and the surface it describes.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    id: MaterialId,
    name: String,
    base: Option<Base>,
    coat: Option<Coat>,
}

impl Material {
    /// The material `id` named `name`, made of `base` and at most one `coat`.
    ///
    /// A record with neither is refused: it would draw nothing, and a shop that names a material
    /// expects to see it. A coat alone is a finish — a lacquer — and a base alone is a veneer, a
    /// paint or a metal (ADR 0243, O18).
    pub fn new(
        id: MaterialId,
        name: impl Into<String>,
        base: Option<Base>,
        coat: Option<Coat>,
    ) -> Result<Self, MaterialError> {
        let name = name.into();
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(MaterialError::NameEmpty);
        }
        if trimmed.chars().count() > MAX_NAME_CHARS {
            return Err(MaterialError::NameTooLong { name });
        }
        if base.is_none() && coat.is_none() {
            return Err(MaterialError::NoSurface);
        }
        Ok(Self { id, name, base, coat })
    }

    /// The base alone, under `name`: a board, a paint, a metal or a patina.
    pub fn of_base(id: MaterialId, name: impl Into<String>, base: Base) -> Result<Self, MaterialError> {
        Self::new(id, name, Some(base), None)
    }

    /// The library's stable name.
    pub const fn id(&self) -> MaterialId {
        self.id
    }

    /// The name a person reads.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// What the part is made of, or `None` for a finish that is only a coat.
    pub const fn base(&self) -> Option<&Base> {
        self.base.as_ref()
    }

    /// The coat over the base, or `None` for a surface with no finish.
    pub const fn coat(&self) -> Option<&Coat> {
        self.coat.as_ref()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;

    fn id(value: u32) -> MaterialId {
        MaterialId::new(value).expect("a non-zero id")
    }

    fn millimetres(value: i64) -> Length {
        Length::from_millimetres(value).expect("a length")
    }

    #[test]
    fn a_decor_must_cover_a_board_and_stay_inside_its_folder() {
        let covers = millimetres(600);
        let decor = Decor::new("textures/oak.png", covers, Turns::ACROSS).expect("a decor");
        assert_eq!(decor.covers(), covers);
        assert_eq!(decor.turns(), Turns::ACROSS);
        assert_eq!(Decor::new("  ", covers, Turns::ALONG).err(), Some(MaterialError::PictureEmpty));
        assert_eq!(
            Decor::new("../elsewhere/oak.png", covers, Turns::ALONG).err(),
            Some(MaterialError::PictureOutside { picture: "../elsewhere/oak.png".to_owned() })
        );
        assert_eq!(
            Decor::new("/abs/oak.png", covers, Turns::ALONG).err(),
            Some(MaterialError::PictureOutside { picture: "/abs/oak.png".to_owned() })
        );
        assert_eq!(
            Decor::new("textures/oak.png", millimetres(0), Turns::ALONG).err(),
            Some(MaterialError::Covers { covers: millimetres(0) })
        );
        assert_eq!(Turns::new(4), None, "a file names a turn, not a count modulo four");
        assert_eq!(Turns::new(3).map(Turns::get), Some(3));
    }

    #[test]
    fn a_channel_outside_its_range_names_the_field_and_the_range() {
        let error = Base::new([0.5; 3], None, 2.5, 0.0, 1.5, 0.0, [0.0; 3]).expect_err("roughness past one");
        assert_eq!(error, MaterialError::Field { field: "base roughness", value: 2.5, min: 0.0, max: 1.0 });
        let text = error.to_string();
        assert!(text.contains("base roughness") && text.contains("2.5"), "the sentence is for a person: {text}");
        // Not a number is not a value either.
        assert!(Base::matte([f32::NAN, 0.0, 0.0]).is_err());
        assert!(Base::new([0.5; 3], None, 0.5, 0.0, 0.8, 0.0, [0.0; 3]).is_err(), "an index below air");
        assert!(Coat::new(1.0, 120.0, 1.5).is_err(), "a gloss is a percent");
        assert_eq!(Coat::glossed(90.0).map(|coat| coat.gloss()), Ok(90.0));
        assert!(Base::new([0.5; 3], None, 0.5, 0.0, 1.5, 0.0, [-1.0; 3]).is_err(), "absorption cannot be negative");
    }

    #[test]
    fn a_record_that_describes_no_surface_is_refused() {
        assert_eq!(Material::new(id(1), "Oak", None, None).err(), Some(MaterialError::NoSurface));
        assert_eq!(
            Material::new(id(1), "  ", None, Some(Coat::glossed(90.0).expect("a coat"))).err(),
            Some(MaterialError::NameEmpty)
        );
        let lacquer = Material::new(id(2), "Lacquer 90", None, Some(Coat::glossed(90.0).expect("a coat")));
        assert!(lacquer.is_ok(), "a finish is a coat and no base");
        let veneer = Material::of_base(id(3), "Veneer oak", Base::matte([0.6, 0.45, 0.25]).expect("a board"));
        assert!(veneer.is_ok(), "a veneer is a base and no coat");
    }
}
