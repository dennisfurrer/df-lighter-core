//! How an account margins: its trading mode (`UpdateAccountConfig`: classic, or unified across spot
//! and perps) and, in unified mode, which spot assets count as perps margin
//! (`UpdateAccountAssetConfig`, multi-asset margin). Both are signed by the account's API key
//! alone.
//!
//! The venue refuses a mode the account already has (21142), and leaving unified mode while any
//! asset still backs perps (21158): turn each off first.

use serde::{Deserialize, Deserializer};

use crate::utils::AssetId;
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{Tx, TxType, element, number};

/// An account's trading mode, `account_trading_mode` on the account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TradingMode {
    /// Spot and perps apart: perps margin is the collateral alone. The default.
    #[default]
    Classic,
    /// One margin across spot and perps (a UTA), the collateral and any asset enabled as margin.
    Unified,
}

impl TradingMode {
    /// `AccountTradingMode` as the transaction signs it and the account reads it.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            TradingMode::Classic => 0,
            TradingMode::Unified => 1,
        }
    }

    #[must_use]
    pub const fn from_code(code: u64) -> Option<TradingMode> {
        match code {
            0 => Some(TradingMode::Classic),
            1 => Some(TradingMode::Unified),
            _ => None,
        }
    }
}

/// From the number an account reads; one this build does not know is `None`, not a failed page.
pub(crate) fn known_mode<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<TradingMode>, D::Error> {
    let code = Option::<u64>::deserialize(deserializer)?;
    Ok(code.and_then(TradingMode::from_code))
}

/// `UpdateAccountConfig`: switches the signing account's trading mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateAccountConfig {
    pub mode: TradingMode,
}

impl UpdateAccountConfig {
    #[must_use]
    pub const fn new(mode: TradingMode) -> UpdateAccountConfig {
        UpdateAccountConfig { mode }
    }
}

impl Tx for UpdateAccountConfig {
    fn tx_type(&self) -> TxType {
        TxType::UpdateAccountConfig
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![element(self.mode.code())]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![("AccountTradingMode", number(self.mode.code()))]
    }
}

/// `UpdateAccountAssetConfig`: whether `asset`, held in spot, counts as perps margin (multi-asset
/// margin, unified mode only). The collateral always does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateAccountAssetConfig {
    pub asset: AssetId,
    pub margin: bool,
}

impl UpdateAccountAssetConfig {
    /// `asset` counts as margin.
    #[must_use]
    pub const fn enable(asset: AssetId) -> UpdateAccountAssetConfig {
        UpdateAccountAssetConfig { asset, margin: true }
    }

    /// `asset` no longer counts as margin.
    #[must_use]
    pub const fn disable(asset: AssetId) -> UpdateAccountAssetConfig {
        UpdateAccountAssetConfig { asset, margin: false }
    }

    /// `AssetMarginMode`: 1 enabled, 0 disabled.
    #[must_use]
    pub const fn margin_code(&self) -> u8 {
        match self.margin {
            true => 1,
            false => 0,
        }
    }
}

impl Tx for UpdateAccountAssetConfig {
    fn tx_type(&self) -> TxType {
        TxType::UpdateAccountAssetConfig
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![element(self.asset.get()), element(self.margin_code())]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![("AssetIndex", number(self.asset.get())), ("AssetMarginMode", number(self.margin_code()))]
    }
}

#[cfg(test)]
#[path = "tests/mode.rs"]
mod tests;
