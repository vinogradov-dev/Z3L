//! The external material catalogue.
//!
//! Materials are not part of the scene document: a project stores only stable material ids and
//! fallbacks. This module defines the catalogue data and its invariants; loading a file is the
//! application shell's job.

use std::collections::HashSet;
use std::fmt;
use std::num::NonZeroU64;

use furnicraft_units::Length;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Panel;

/// A stable catalogue identifier.
///
/// Material ids belong to the catalogue, not to [`crate::Project`]'s id space. They are non-zero,
/// unique within a catalogue and never reused, so an old scene cannot silently point at a new
/// material with the same number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaterialId(NonZeroU64);

impl MaterialId {
    /// Creates an id from a non-zero raw value.
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// The numeric value.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for MaterialId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// An amount of money in whole minor units of the catalogue currency.
///
/// A price is unsigned: a refund or a discount is not a price, and a signed quantity would make every
/// sum a signed quantity for no reason. How many minor units make one unit is the currency's, not the
/// amount's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(u64);

impl Money {
    /// Creates an amount from whole minor units.
    pub const fn from_minor(minor: u64) -> Self {
        Self(minor)
    }

    /// The amount in whole minor units.
    pub const fn minor(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Money {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// The currency a catalogue's prices are in.
///
/// The major part of the unit is not ours to guess: a currency names its code and how many minor
/// units make one unit, so a reader prints `12345` as `123.45` without a table of currencies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CurrencyWire")]
pub struct Currency {
    code: String,
    minor_scale: u32,
}

impl Currency {
    /// Largest accepted number of minor units in one unit.
    pub const MAX_MINOR_SCALE: u32 = 10_000;

    /// Creates a validated currency.
    ///
    /// The code is the three upper-case letters of ISO 4217; the scale is how many minor units make
    /// one unit, so a currency without minor units uses `1`.
    pub fn new(code: impl Into<String>, minor_scale: u32) -> Result<Self, CatalogError> {
        let code = code.into();
        if code.len() != 3 || !code.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(CatalogError::InvalidCurrency { code });
        }
        if minor_scale == 0 || minor_scale > Self::MAX_MINOR_SCALE {
            return Err(CatalogError::InvalidMinorScale { scale: minor_scale });
        }
        Ok(Self { code, minor_scale })
    }

    /// The ISO 4217 code, three upper-case letters.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// How many minor units make one unit.
    pub const fn minor_scale(&self) -> u32 {
        self.minor_scale
    }
}

/// The stock sheet a panel material is bought in.
///
/// A sheet is a nominal rectangle: the millimetre allowance the saw and the trim take is the
/// package's declared rule, not a property of the board, so it is not stored here. A sheet is read
/// only as the field of a material's stock block, so [`PanelStock`] owns the reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SheetSize {
    length: Length,
    width: Length,
}

impl SheetSize {
    /// Creates a validated sheet size: both sides positive and no larger than a panel may be.
    pub fn new(length: Length, width: Length) -> Result<Self, StockError> {
        validate_sheet_dimension("length", length)?;
        validate_sheet_dimension("width", width)?;
        Ok(Self { length, width })
    }

    /// The long side of the sheet.
    pub const fn length(&self) -> Length {
        self.length
    }

    /// The short side of the sheet.
    pub const fn width(&self) -> Length {
        self.width
    }
}

/// The stock a panel material is bought in.
///
/// A material without a stock block is known but not stocked: the order cannot say what to buy. A
/// sheet is required, because a price is the price of one sheet and a group groups sheets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PanelStock {
    sheet: SheetSize,
    group: Option<String>,
    price: Option<Money>,
}

impl PanelStock {
    /// Creates a validated stock block; a present group must not be empty.
    pub fn new(sheet: SheetSize, group: Option<String>, price: Option<Money>) -> Result<Self, StockError> {
        if let Some(group) = &group
            && group.trim().is_empty()
        {
            return Err(StockError::EmptyGroup);
        }
        Ok(Self { sheet, group, price })
    }

    /// The sheet the material is bought in.
    pub const fn sheet(&self) -> SheetSize {
        self.sheet
    }

    /// The workshop's own stock group, when it has one.
    pub fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }

    /// The price of one sheet in the catalogue currency, when the catalogue carries one.
    pub const fn price(&self) -> Option<Money> {
        self.price
    }
}

/// A panel-body material.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PanelMaterialWire")]
pub struct PanelMaterial {
    id: MaterialId,
    name: String,
    kind: String,
    nominal_thickness: Length,
    color: String,
    stock: Option<PanelStock>,
}

impl PanelMaterial {
    /// Creates a validated panel material.
    pub fn new(
        id: MaterialId,
        name: impl Into<String>,
        kind: impl Into<String>,
        nominal_thickness: Length,
        color: impl Into<String>,
    ) -> Result<Self, CatalogError> {
        let name = name.into();
        let kind = kind.into();
        validate_text(id, "name", &name)?;
        validate_text(id, "kind", &kind)?;
        validate_dimension(id, "nominal_thickness", nominal_thickness, Panel::MAX_THICKNESS)?;
        Ok(Self { id, name, kind, nominal_thickness, color: color.into(), stock: None })
    }

    /// The same material with its stock block: the sheet, the stock group and the sheet price.
    ///
    /// The block was validated by [`PanelStock::new`] and [`SheetSize::new`], so nothing is checked
    /// here. A catalogue refuses a priced material when it declares no currency.
    pub fn with_stock(mut self, stock: PanelStock) -> Self {
        self.stock = Some(stock);
        self
    }

    /// The stable material id.
    pub const fn id(&self) -> MaterialId {
        self.id
    }

    /// The user-visible name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The material kind, such as `chipboard` or `mdf`.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The nominal board thickness.
    pub const fn nominal_thickness(&self) -> Length {
        self.nominal_thickness
    }

    /// The colour or decor marker.
    pub fn color(&self) -> &str {
        &self.color
    }

    /// The stock the material is bought in, when the catalogue stocks it.
    pub const fn stock(&self) -> Option<&PanelStock> {
        self.stock.as_ref()
    }
}

/// A ready-made edge-banding material.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "EdgeBandingMaterialWire")]
pub struct EdgeBandingMaterial {
    id: MaterialId,
    name: String,
    kind: String,
    thickness: Length,
    width: Length,
    color: String,
}

impl EdgeBandingMaterial {
    /// Creates a validated edge-banding material.
    pub fn new(
        id: MaterialId,
        name: impl Into<String>,
        kind: impl Into<String>,
        thickness: Length,
        width: Length,
        color: impl Into<String>,
    ) -> Result<Self, CatalogError> {
        let name = name.into();
        let kind = kind.into();
        validate_text(id, "name", &name)?;
        validate_text(id, "kind", &kind)?;
        validate_dimension(id, "thickness", thickness, Panel::MAX_LENGTH)?;
        validate_dimension(id, "width", width, Panel::MAX_THICKNESS)?;
        Ok(Self { id, name, kind, thickness, width, color: color.into() })
    }

    /// The stable material id.
    pub const fn id(&self) -> MaterialId {
        self.id
    }

    /// The user-visible name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The material kind, such as `abs` or `pvc`.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The banding profile thickness in the panel plane.
    pub const fn thickness(&self) -> Length {
        self.thickness
    }

    /// The banding profile width across the panel thickness.
    pub const fn width(&self) -> Length {
        self.width
    }

    /// The colour or decor marker.
    pub fn color(&self) -> &str {
        &self.color
    }
}

/// An external, read-only collection of materials.
///
/// The catalogue is not stored in a scene and is not part of [`crate::Project`]. A session receives
/// one immutable instance; material ids are unique across both lists and are never reused. The
/// currency is the one the sheet prices are in; a catalogue that prices a material without declaring
/// one is refused, because a price with no unit is not money.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CatalogWire")]
pub struct Catalog {
    panel_materials: Vec<PanelMaterial>,
    edge_materials: Vec<EdgeBandingMaterial>,
    currency: Option<Currency>,
}

impl Catalog {
    /// Creates a catalogue after checking that every material id is unique.
    pub fn new(
        panel_materials: Vec<PanelMaterial>,
        edge_materials: Vec<EdgeBandingMaterial>,
        currency: Option<Currency>,
    ) -> Result<Self, CatalogError> {
        let mut ids = HashSet::new();
        for material in &panel_materials {
            if !ids.insert(material.id) {
                return Err(CatalogError::DuplicateId(material.id));
            }
            let priced = material.stock().is_some_and(|stock| stock.price().is_some());
            if priced && currency.is_none() {
                return Err(CatalogError::PriceWithoutCurrency { material: material.id });
            }
        }
        for material in &edge_materials {
            if !ids.insert(material.id) {
                return Err(CatalogError::DuplicateId(material.id));
            }
        }
        Ok(Self { panel_materials, edge_materials, currency })
    }

    /// The panel materials, in catalogue order.
    pub fn panel_materials(&self) -> &[PanelMaterial] {
        &self.panel_materials
    }

    /// The edge-banding materials, in catalogue order.
    pub fn edge_materials(&self) -> &[EdgeBandingMaterial] {
        &self.edge_materials
    }

    /// The currency every sheet price is in, when the catalogue carries prices.
    pub const fn currency(&self) -> Option<&Currency> {
        self.currency.as_ref()
    }

    /// Looks up a panel material by id.
    pub fn panel_material(&self, id: MaterialId) -> Option<&PanelMaterial> {
        self.panel_materials.iter().find(|material| material.id == id)
    }

    /// Looks up an edge-banding material by id.
    pub fn edge_material(&self, id: MaterialId) -> Option<&EdgeBandingMaterial> {
        self.edge_materials.iter().find(|material| material.id == id)
    }
}

/// Why a catalogue or one of its records is not usable.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CatalogError {
    /// A required text field is empty.
    #[error("material {material} has an empty {field}")]
    EmptyField {
        /// The material being validated.
        material: MaterialId,
        /// The rejected field.
        field: &'static str,
    },
    /// A dimension is zero or negative.
    #[error("material {material} {field} must be positive, got {value}")]
    NonPositive {
        /// The material being validated.
        material: MaterialId,
        /// The rejected dimension.
        field: &'static str,
        /// The rejected value.
        value: Length,
    },
    /// A dimension exceeds the supported maximum.
    #[error("material {material} {field} must not exceed {max}, got {value}")]
    TooLarge {
        /// The material being validated.
        material: MaterialId,
        /// The rejected dimension.
        field: &'static str,
        /// The rejected value.
        value: Length,
        /// The largest accepted value.
        max: Length,
    },
    /// Two materials use the same id.
    #[error("duplicate material id {0}")]
    DuplicateId(MaterialId),
    /// A currency code is not three upper-case ISO 4217 letters.
    #[error("`{code}` is not a three-letter currency code")]
    InvalidCurrency {
        /// The rejected code.
        code: String,
    },
    /// A currency declares no minor units or too many.
    #[error("a minor-unit scale of {scale} is not between 1 and {}", Currency::MAX_MINOR_SCALE)]
    InvalidMinorScale {
        /// The rejected scale.
        scale: u32,
    },
    /// A material carries a price while the catalogue names no currency.
    #[error("material {material} carries a price but the catalogue declares no currency")]
    PriceWithoutCurrency {
        /// The priced material.
        material: MaterialId,
    },
    /// A stock block is not usable.
    #[error("material {material}: {source}")]
    Stock {
        /// The material whose stock block failed.
        material: MaterialId,
        /// The rejected stock field.
        source: StockError,
    },
}

/// Why a stock sheet or a stock block is not usable.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum StockError {
    /// A sheet side is zero or negative.
    #[error("the sheet {field} must be positive, got {value}")]
    NonPositive {
        /// The rejected side.
        field: &'static str,
        /// The rejected value.
        value: Length,
    },
    /// A sheet side is longer than a panel may be.
    #[error("the sheet {field} must not exceed {max}, got {value}")]
    TooLarge {
        /// The rejected side.
        field: &'static str,
        /// The rejected value.
        value: Length,
        /// The largest accepted value.
        max: Length,
    },
    /// A stock group is present but empty.
    #[error("the stock group is empty")]
    EmptyGroup,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PanelMaterialWire {
    id: MaterialId,
    name: String,
    kind: String,
    nominal_thickness: Length,
    color: String,
    #[serde(default)]
    stock: Option<StockWire>,
}

impl TryFrom<PanelMaterialWire> for PanelMaterial {
    type Error = CatalogError;

    fn try_from(wire: PanelMaterialWire) -> Result<Self, Self::Error> {
        let material = Self::new(wire.id, wire.name, wire.kind, wire.nominal_thickness, wire.color)?;
        let Some(stock) = wire.stock else {
            return Ok(material);
        };
        let stock = build_stock(material.id, stock)?;
        Ok(material.with_stock(stock))
    }
}

/// Reads a stock block, naming the material in a refusal.
fn build_stock(material: MaterialId, wire: StockWire) -> Result<PanelStock, CatalogError> {
    let sheet = SheetSize::new(wire.sheet.length, wire.sheet.width)
        .map_err(|source| CatalogError::Stock { material, source })?;
    PanelStock::new(sheet, wire.group, wire.price).map_err(|source| CatalogError::Stock { material, source })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StockWire {
    sheet: SheetSizeWire,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    price: Option<Money>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SheetSizeWire {
    length: Length,
    width: Length,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrencyWire {
    code: String,
    minor_scale: u32,
}

impl TryFrom<CurrencyWire> for Currency {
    type Error = CatalogError;

    fn try_from(wire: CurrencyWire) -> Result<Self, Self::Error> {
        Self::new(wire.code, wire.minor_scale)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeBandingMaterialWire {
    id: MaterialId,
    name: String,
    kind: String,
    thickness: Length,
    width: Length,
    color: String,
}

impl TryFrom<EdgeBandingMaterialWire> for EdgeBandingMaterial {
    type Error = CatalogError;

    fn try_from(wire: EdgeBandingMaterialWire) -> Result<Self, Self::Error> {
        Self::new(wire.id, wire.name, wire.kind, wire.thickness, wire.width, wire.color)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogWire {
    panel_materials: Vec<PanelMaterial>,
    edge_materials: Vec<EdgeBandingMaterial>,
    #[serde(default)]
    currency: Option<Currency>,
}

impl TryFrom<CatalogWire> for Catalog {
    type Error = CatalogError;

    fn try_from(wire: CatalogWire) -> Result<Self, Self::Error> {
        Self::new(wire.panel_materials, wire.edge_materials, wire.currency)
    }
}

fn validate_text(id: MaterialId, field: &'static str, value: &str) -> Result<(), CatalogError> {
    if value.trim().is_empty() {
        return Err(CatalogError::EmptyField { material: id, field });
    }
    Ok(())
}

fn validate_dimension(id: MaterialId, field: &'static str, value: Length, max: Length) -> Result<(), CatalogError> {
    if !value.is_positive() {
        return Err(CatalogError::NonPositive { material: id, field, value });
    }
    if value > max {
        return Err(CatalogError::TooLarge { material: id, field, value, max });
    }
    Ok(())
}

fn validate_sheet_dimension(field: &'static str, value: Length) -> Result<(), StockError> {
    if !value.is_positive() {
        return Err(StockError::NonPositive { field, value });
    }
    if value > Panel::MAX_LENGTH {
        return Err(StockError::TooLarge { field, value, max: Panel::MAX_LENGTH });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::testing::millimetres;

    use super::*;

    fn id(value: u64) -> MaterialId {
        MaterialId::new(value).expect("test ids are non-zero")
    }

    fn panel_material() -> PanelMaterial {
        PanelMaterial::new(id(1), "Chipboard 18 mm, oak", "chipboard", millimetres(18), "#b98a5a")
            .expect("valid panel material")
    }

    fn edge_material() -> EdgeBandingMaterial {
        EdgeBandingMaterial::new(id(101), "ABS 2 mm, oak", "abs", millimetres(2), millimetres(22), "#7a4f2a")
            .expect("valid edge material")
    }

    fn sheet() -> SheetSize {
        SheetSize::new(millimetres(2800), millimetres(2070)).expect("a valid sheet")
    }

    fn roubles() -> Currency {
        Currency::new("RUB", 100).expect("a currency")
    }

    fn stocked() -> PanelMaterial {
        let stock = PanelStock::new(sheet(), Some("18-oak".to_owned()), Some(Money::from_minor(320_000)))
            .expect("a valid stock block");
        panel_material().with_stock(stock)
    }

    #[test]
    fn material_ids_reject_zero() {
        assert_eq!(MaterialId::new(0), None);
        assert!(serde_json::from_str::<MaterialId>("0").is_err());
        assert_eq!(serde_json::to_string(&id(7)).expect("serialize id"), "7");
    }

    #[test]
    fn catalogue_accepts_unique_materials_and_looks_them_up() {
        let catalog = Catalog::new(vec![panel_material()], vec![edge_material()], None).expect("unique ids");
        assert_eq!(catalog.panel_material(id(1)).map(PanelMaterial::id), Some(id(1)));
        assert_eq!(catalog.edge_material(id(101)).map(EdgeBandingMaterial::id), Some(id(101)));
        assert!(catalog.panel_material(id(101)).is_none());
        assert!(catalog.currency().is_none());
    }

    #[test]
    fn duplicate_ids_are_rejected_across_the_whole_catalogue() {
        let duplicate =
            PanelMaterial::new(id(1), "Second", "mdf", millimetres(18), "#fff").expect("valid panel material");
        assert_eq!(
            Catalog::new(vec![panel_material(), duplicate], Vec::new(), None),
            Err(CatalogError::DuplicateId(id(1)))
        );

        let edge_duplicate = EdgeBandingMaterial::new(id(1), "Edge", "abs", millimetres(1), millimetres(20), "#000")
            .expect("valid edge material");
        assert_eq!(
            Catalog::new(vec![panel_material()], vec![edge_duplicate], None),
            Err(CatalogError::DuplicateId(id(1)))
        );
    }

    #[test]
    fn records_reject_empty_text_and_bad_dimensions() {
        assert_eq!(
            PanelMaterial::new(id(1), " ", "chipboard", millimetres(18), "#fff"),
            Err(CatalogError::EmptyField { material: id(1), field: "name" })
        );
        assert_eq!(
            PanelMaterial::new(id(1), "Board", "", millimetres(18), "#fff"),
            Err(CatalogError::EmptyField { material: id(1), field: "kind" })
        );
        assert_eq!(
            EdgeBandingMaterial::new(id(101), "Edge", "abs", Length::ZERO, millimetres(22), "#fff"),
            Err(CatalogError::NonPositive { material: id(101), field: "thickness", value: Length::ZERO })
        );
        let too_wide = Length::from_micrometres(Panel::MAX_THICKNESS.micrometres() + 1);
        assert_eq!(
            EdgeBandingMaterial::new(id(101), "Edge", "abs", millimetres(2), too_wide, "#fff"),
            Err(CatalogError::TooLarge {
                material: id(101),
                field: "width",
                value: too_wide,
                max: Panel::MAX_THICKNESS,
            })
        );
    }

    #[test]
    fn catalogue_round_trips_and_rejects_unknown_fields() {
        let catalog = Catalog::new(vec![panel_material()], vec![edge_material()], None).expect("unique ids");
        let text = serde_json::to_string(&catalog).expect("serialize catalogue");
        let decoded: Catalog = serde_json::from_str(&text).expect("deserialize catalogue");
        assert_eq!(decoded, catalog);
        assert!(serde_json::from_str::<Catalog>(r#"{"panel_materials":[],"edge_materials":[],"extra":1}"#).is_err());
    }

    #[test]
    fn a_stock_block_round_trips_with_its_currency() {
        let catalog = Catalog::new(vec![stocked()], vec![edge_material()], Some(roubles()))
            .expect("a priced catalogue names its currency");
        let text = serde_json::to_string(&catalog).expect("serialize catalogue");
        let decoded: Catalog = serde_json::from_str(&text).expect("deserialize catalogue");
        assert_eq!(decoded, catalog);
        let stock = decoded.panel_material(id(1)).and_then(PanelMaterial::stock).expect("the stock block");
        assert_eq!(stock.sheet(), sheet());
        assert_eq!(stock.group(), Some("18-oak"));
        assert_eq!(stock.price(), Some(Money::from_minor(320_000)));
        assert_eq!(decoded.currency().map(Currency::code), Some("RUB"));
        assert_eq!(decoded.currency().map(Currency::minor_scale), Some(100));
    }

    #[test]
    fn a_price_without_a_currency_is_refused() {
        assert_eq!(
            Catalog::new(vec![stocked()], Vec::new(), None),
            Err(CatalogError::PriceWithoutCurrency { material: id(1) })
        );
        assert!(Catalog::new(vec![stocked()], Vec::new(), Some(roubles())).is_ok());
    }

    #[test]
    fn a_sheet_must_be_positive_and_fit_a_board() {
        assert_eq!(
            SheetSize::new(Length::ZERO, millimetres(2070)),
            Err(StockError::NonPositive { field: "length", value: Length::ZERO })
        );
        let too_wide = Length::from_micrometres(Panel::MAX_LENGTH.micrometres() + 1);
        assert_eq!(
            SheetSize::new(millimetres(2800), too_wide),
            Err(StockError::TooLarge { field: "width", value: too_wide, max: Panel::MAX_LENGTH })
        );
        assert_eq!(PanelStock::new(sheet(), Some(" ".to_owned()), None), Err(StockError::EmptyGroup));
        assert!(PanelStock::new(sheet(), None, None).is_ok(), "a stock block needs no group and no price");
    }

    #[test]
    fn a_currency_names_its_code_and_its_minor_scale() {
        assert_eq!(Currency::new("rub", 100), Err(CatalogError::InvalidCurrency { code: "rub".to_owned() }));
        assert_eq!(Currency::new("ROUBLE", 100), Err(CatalogError::InvalidCurrency { code: "ROUBLE".to_owned() }));
        assert_eq!(Currency::new("RUB", 0), Err(CatalogError::InvalidMinorScale { scale: 0 }));
        assert_eq!(
            Currency::new("RUB", Currency::MAX_MINOR_SCALE + 1),
            Err(CatalogError::InvalidMinorScale { scale: Currency::MAX_MINOR_SCALE + 1 })
        );
        assert!(Currency::new("JPY", 1).is_ok(), "a currency without minor units declares a scale of one");
    }

    #[test]
    fn a_bad_stock_block_from_a_file_names_its_material() {
        let text = r#"{
            "panel_materials": [{
                "id": 1, "name": "Board", "kind": "chipboard", "nominal_thickness": 18000,
                "color": "white",
                "stock": { "sheet": { "length": 0, "width": 2070000 }, "group": "18" }
            }],
            "edge_materials": []
        }"#;
        let error = serde_json::from_str::<Catalog>(text).expect_err("a zero sheet side is refused");
        assert!(error.to_string().contains("material 1"), "the message names the material: {error}");
    }
}
