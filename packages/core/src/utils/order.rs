//! The small enums an order, a position and a market are described by, each with the number a
//! transaction signs (lighter-go's constants) and the word a read answers.
//!
//! - [`Side`]: bid or ask; the venue writes `is_ask`.
//! - [`OrderKind`]: limit, market, the four triggers, TWAP, and the venue's own sub-orders.
//! - [`TimeInForce`]: immediate-or-cancel, good-till-time, post-only.
//! - [`MarginMode`]: cross or isolated, per position.
//! - [`MarketKind`]: perps, spot or binary options.

use serde::{Deserialize, Deserializer, de};

/// Which side of the book: a bid buys, an ask sells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Bid,
    Ask,
}

impl Side {
    #[must_use]
    pub const fn from_is_ask(is_ask: bool) -> Side {
        match is_ask {
            true => Side::Ask,
            false => Side::Bid,
        }
    }

    /// `IsAsk` as a transaction signs it.
    #[must_use]
    pub const fn is_ask(self) -> bool {
        matches!(self, Side::Ask)
    }

    #[must_use]
    pub const fn opposite(self) -> Side {
        match self {
            Side::Bid => Side::Ask,
            Side::Ask => Side::Bid,
        }
    }

    /// `1` for a bid, which a long position grows by; `-1` for an ask.
    #[must_use]
    pub const fn sign(self) -> i8 {
        match self {
            Side::Bid => 1,
            Side::Ask => -1,
        }
    }
}

/// An order's kind. A program places the first seven; the venue makes TWAP slices and
/// liquidations itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderKind {
    Limit,
    Market,
    StopLoss,
    StopLossLimit,
    TakeProfit,
    TakeProfitLimit,
    Twap,
    TwapSub,
    Liquidation,
}

impl OrderKind {
    pub const ALL: [OrderKind; 9] = [
        OrderKind::Limit,
        OrderKind::Market,
        OrderKind::StopLoss,
        OrderKind::StopLossLimit,
        OrderKind::TakeProfit,
        OrderKind::TakeProfitLimit,
        OrderKind::Twap,
        OrderKind::TwapSub,
        OrderKind::Liquidation,
    ];

    /// `Type` as a transaction signs it.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            OrderKind::Limit => 0,
            OrderKind::Market => 1,
            OrderKind::StopLoss => 2,
            OrderKind::StopLossLimit => 3,
            OrderKind::TakeProfit => 4,
            OrderKind::TakeProfitLimit => 5,
            OrderKind::Twap => 6,
            OrderKind::TwapSub => 7,
            OrderKind::Liquidation => 8,
        }
    }

    #[must_use]
    pub fn from_code(code: u8) -> Option<OrderKind> {
        OrderKind::ALL.into_iter().find(|kind| kind.code() == code)
    }

    /// The word a read answers: `limit`, `stop-loss-limit`, `twap-sub`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            OrderKind::Limit => "limit",
            OrderKind::Market => "market",
            OrderKind::StopLoss => "stop-loss",
            OrderKind::StopLossLimit => "stop-loss-limit",
            OrderKind::TakeProfit => "take-profit",
            OrderKind::TakeProfitLimit => "take-profit-limit",
            OrderKind::Twap => "twap",
            OrderKind::TwapSub => "twap-sub",
            OrderKind::Liquidation => "liquidation",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<OrderKind> {
        OrderKind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// Whether a program may place it: not a TWAP slice or a liquidation.
    #[must_use]
    pub const fn is_placeable(self) -> bool {
        self.code() <= OrderKind::Twap.code()
    }

    /// Whether it waits for a trigger price: the stops and take-profits.
    #[must_use]
    pub const fn is_trigger(self) -> bool {
        matches!(
            self,
            OrderKind::StopLoss | OrderKind::StopLossLimit | OrderKind::TakeProfit | OrderKind::TakeProfitLimit
        )
    }
}

/// How long an order stays on the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeInForce {
    /// Fills what it can at once and cancels the rest: every market order.
    ImmediateOrCancel,
    /// Rests until filled, cancelled or its expiry.
    GoodTillTime,
    /// Rests, and is cancelled rather than take liquidity.
    PostOnly,
}

impl TimeInForce {
    pub const ALL: [TimeInForce; 3] =
        [TimeInForce::ImmediateOrCancel, TimeInForce::GoodTillTime, TimeInForce::PostOnly];

    /// `TimeInForce` as a transaction signs it.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            TimeInForce::ImmediateOrCancel => 0,
            TimeInForce::GoodTillTime => 1,
            TimeInForce::PostOnly => 2,
        }
    }

    #[must_use]
    pub fn from_code(code: u8) -> Option<TimeInForce> {
        TimeInForce::ALL.into_iter().find(|tif| tif.code() == code)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            TimeInForce::ImmediateOrCancel => "immediate-or-cancel",
            TimeInForce::GoodTillTime => "good-till-time",
            TimeInForce::PostOnly => "post-only",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<TimeInForce> {
        TimeInForce::ALL.into_iter().find(|tif| tif.name() == name)
    }
}

/// How a position is margined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MarginMode {
    /// Shares the account's collateral with every other cross position.
    #[default]
    Cross,
    /// Holds its own margin, added and removed by hand.
    Isolated,
}

impl MarginMode {
    /// `MarginMode` as `UpdateLeverage` signs it, and as an account's position reads.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            MarginMode::Cross => 0,
            MarginMode::Isolated => 1,
        }
    }

    #[must_use]
    pub const fn from_code(code: u8) -> Option<MarginMode> {
        match code {
            0 => Some(MarginMode::Cross),
            1 => Some(MarginMode::Isolated),
            _ => None,
        }
    }
}

/// From the number a position reads.
impl<'de> Deserialize<'de> for MarginMode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = u8::deserialize(deserializer)?;
        MarginMode::from_code(code).ok_or_else(|| de::Error::custom(format!("{code} is not a margin mode")))
    }
}

/// What a market trades.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarketKind {
    Perps,
    Spot,
    /// Yes/no contracts that settle at 0 or 1: Lighter only.
    BinaryOptions,
}

impl MarketKind {
    pub const ALL: [MarketKind; 3] = [MarketKind::Perps, MarketKind::Spot, MarketKind::BinaryOptions];

    /// lighter-go's `MarketType`.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            MarketKind::Perps => 0,
            MarketKind::Spot => 1,
            MarketKind::BinaryOptions => 2,
        }
    }

    /// The word a read answers (`market_kind`, `market_type`): `perps`, `spot`, `bo`. A filter
    /// writes perps as `perp`; see [`MarketKind::filter`].
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            MarketKind::Perps => "perps",
            MarketKind::Spot => "spot",
            MarketKind::BinaryOptions => "bo",
        }
    }

    /// The word a `market_type` filter takes: `perp`, `spot`, `bo`.
    #[must_use]
    pub const fn filter(self) -> &'static str {
        match self {
            MarketKind::Perps => "perp",
            MarketKind::Spot => "spot",
            MarketKind::BinaryOptions => "bo",
        }
    }

    /// Reads any spelling the venue uses: `perps`, `perp`, `spot`, `bo`, `binary_options`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<MarketKind> {
        match name {
            "perps" | "perp" => Some(MarketKind::Perps),
            "spot" => Some(MarketKind::Spot),
            "bo" | "binary_options" | "binary-options" => Some(MarketKind::BinaryOptions),
            _ => None,
        }
    }
}

/// Anything read by its word.
pub(crate) trait Named: Sized {
    fn from_name(name: &str) -> Option<Self>;
}

macro_rules! by_name {
    ($name:ident, $what:literal) => {
        /// From the word a read answers.
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let name = String::deserialize(deserializer)?;
                $name::from_name(&name).ok_or_else(|| de::Error::custom(format!("`{name}` is not {}", $what)))
            }
        }

        impl Named for $name {
            fn from_name(name: &str) -> Option<Self> {
                $name::from_name(name)
            }
        }
    };
}

/// A word read leniently: `None` for one this build does not know (the venue writes `Unknown`
/// for some time-in-force), rather than refusing the whole row.
pub(crate) fn known<'de, D: Deserializer<'de>, T: Named>(deserializer: D) -> Result<Option<T>, D::Error> {
    let name = Option::<String>::deserialize(deserializer)?;
    Ok(name.as_deref().and_then(T::from_name))
}

by_name!(OrderKind, "an order kind");
by_name!(TimeInForce, "a time in force");
by_name!(MarketKind, "a market kind");

#[cfg(test)]
#[path = "tests/order.rs"]
mod tests;
