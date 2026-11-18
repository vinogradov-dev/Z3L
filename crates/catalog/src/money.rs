//! Money and the currency it is written in.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// An amount of money in whole minor units of the directory's currency.
///
/// A price is unsigned: a refund or a discount is not a price, and a signed quantity would make every
/// sum a signed quantity for no reason. How many minor units make one unit is the currency's, not the
/// amount's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(u64);

impl Money {
    /// The smallest price: free, which a person rarely types but an import often carries.
    pub const ZERO: Self = Self(0);

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

/// Why a currency or a minor scale was refused.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum CurrencyError {
    /// The code is not three upper-case ASCII letters.
    #[error("`{code}` is not an ISO 4217 code: three upper-case letters, for example `EUR`")]
    Code {
        /// The code that was refused.
        code: String,
    },
    /// The scale is zero or larger than one unit's minor units may be.
    #[error("{scale} is not a usable minor scale; one to {} units", Currency::MAX_MINOR_SCALE)]
    MinorScale {
        /// The scale that was refused.
        scale: u32,
    },
}

/// The currency a directory's prices are in.
///
/// The major part of the unit is not ours to guess: a currency names its code and how many minor
/// units make one unit, so a reader prints `12345` as `123.45` without a table of currencies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CurrencyWire")]
pub struct Currency {
    code: String,
    minor_scale: u32,
}

#[derive(Deserialize)]
struct CurrencyWire {
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
    pub fn new(code: impl Into<String>, minor_scale: u32) -> Result<Self, CurrencyError> {
        let code = code.into();
        if code.len() != 3 || !code.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(CurrencyError::Code { code });
        }
        if minor_scale == 0 || minor_scale > Self::MAX_MINOR_SCALE {
            return Err(CurrencyError::MinorScale { scale: minor_scale });
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

impl TryFrom<CurrencyWire> for Currency {
    type Error = CurrencyError;

    fn try_from(wire: CurrencyWire) -> Result<Self, Self::Error> {
        Self::new(wire.code, wire.minor_scale)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn a_currency_names_its_code_and_its_scale() {
        let euro = Currency::new("EUR", 100).expect("a currency");
        assert_eq!((euro.code(), euro.minor_scale()), ("EUR", 100));
        assert_eq!(Money::from_minor(12_345).minor(), 12_345);
        assert!(
            serde_json::from_str::<Currency>("{\"code\":\"eur\",\"minor_scale\":100}").is_err(),
            "a lower-case code is refused, not folded"
        );
        assert!(Currency::new("EURO", 100).is_err());
        assert!(Currency::new("EUR", 0).is_err());
    }
}
