//! The integers Lighter names things by, each its own type so one is never signed as another:
//! markets, accounts, API key slots, assets and orders. Each is checked against the range
//! lighter-go accepts when it is made.
//!
//! - A market id is any index below 2^48 but 255, which is no market. The id says nothing of the
//!   market's kind: read `market_type` from the venue, never the id's range.
//! - Master accounts count up from 0 in order of creation; sub-accounts and pools count down from
//!   2^48 - 2, the LLP. An index at or above 2^47 is a sub-account or a pool.
//! - An order is named by the client index it was placed with (1 to 2^48 - 1) or by the venue's
//!   order index (2^48 to 2^60 - 1); a cancel or modify takes either.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// Every market, asset and account count stays below 2^48.
pub const WIDTH_48: u64 = (1 << 48) - 1;

/// The market index that names no market (lighter-go's `NilMarketIndex`): legacy cancel-all
/// signatures read it as "every market".
pub const NO_MARKET: u64 = 255;

/// The highest account index: the LLP.
pub const MAX_ACCOUNT_INDEX: u64 = (1 << 48) - 2;

/// The lowest sub-account or pool index; master accounts are below it.
pub const MIN_SUB_ACCOUNT_INDEX: u64 = 1 << 47;

/// The highest API key slot; 255 asks for every key in a read and is never a slot.
pub const MAX_API_KEY_INDEX: u8 = 254;

/// The lowest venue order index; client indexes are below it.
pub const MIN_ORDER_INDEX: u64 = 1 << 48;

/// The highest venue order index.
pub const MAX_ORDER_INDEX: u64 = (1 << 60) - 1;

/// The highest asset index.
pub const MAX_ASSET_INDEX: u16 = (1 << 6) - 2;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadId {
    #[error("{0} is not a market: an index is below 2^48 and never 255")]
    Market(u64),
    #[error("{0} is not an account index: it is at most 2^48 - 2")]
    Account(u64),
    #[error("{0} is not an API key slot: they run from 0 to 254")]
    ApiKey(u64),
    #[error("{0} is not an asset: assets run from 1 to 62")]
    Asset(u64),
    #[error("{0} is not a client order index: it runs from 1 to 2^48 - 1")]
    ClientOrder(u64),
    #[error("{0} is not an order index: one runs from 2^48 to 2^60 - 1")]
    Order(u64),
    #[error("`{0}` is not a whole number")]
    NotANumber(String),
}

macro_rules! id {
    ($(#[$doc:meta])* $name:ident($inner:ty), $check:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($inner);

        impl $name {
            /// The value, when lighter-go would take it.
            pub fn new(value: $inner) -> Result<$name, BadId> {
                let check: fn($inner) -> Result<(), BadId> = $check;
                check(value).map(|()| $name(value))
            }

            #[must_use]
            pub const fn get(self) -> $inner {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = BadId;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                let text = text.trim();
                let value = text.parse::<$inner>().map_err(|_| BadId::NotANumber(text.to_owned()))?;
                $name::new(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0.serialize(serializer)
            }
        }

        /// From a JSON integer or its digits as a string; a float is refused.
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = whole(deserializer)?;
                let value = <$inner>::try_from(value).map_err(|_| de::Error::custom(format!("{value} is out of range")))?;
                $name::new(value).map_err(de::Error::custom)
            }
        }
    };
}

id!(
    /// A market: perp, spot or binary option alike, by the index `orderBookDetails` and `events`
    /// list it under.
    MarketId(u64),
    |value| if value > WIDTH_48 || value == NO_MARKET { Err(BadId::Market(value)) } else { Ok(()) }
);

id!(
    /// An account: a master, a sub-account or a pool.
    AccountIndex(u64),
    |value| if value > MAX_ACCOUNT_INDEX { Err(BadId::Account(value)) } else { Ok(()) }
);

id!(
    /// One of an account's 255 API key slots. Slots 0 to 3 are Lighter's own apps' (and 157 too
    /// on Robinhood Chain); [`crate::client::Network::reserves_api_key`] says which.
    ApiKeyIndex(u8),
    |value| if value > MAX_API_KEY_INDEX { Err(BadId::ApiKey(u64::from(value))) } else { Ok(()) }
);

id!(
    /// An asset: 1 is ETH, 2 LIT and 3 the collateral (USDC on Lighter, USDG on Robinhood Chain);
    /// `assetDetails` lists the rest.
    AssetId(u16),
    |value| if value == 0 || value > MAX_ASSET_INDEX { Err(BadId::Asset(u64::from(value))) } else { Ok(()) }
);

id!(
    /// The index an order is placed with, unique across every market while it is open.
    ClientOrderIndex(u64),
    |value| if value == 0 || value > WIDTH_48 { Err(BadId::ClientOrder(value)) } else { Ok(()) }
);

id!(
    /// The index the venue gives an order: `(market + 1) << 48` and up, past what a JS number holds.
    OrderIndex(u64),
    |value| if !(MIN_ORDER_INDEX..=MAX_ORDER_INDEX).contains(&value) { Err(BadId::Order(value)) } else { Ok(()) }
);

impl MarketId {
    /// For a constant: `MarketId::of::<1>()` is BTC on both venues. 255, or an id past 2^48 - 1,
    /// does not compile.
    ///
    /// ```compile_fail,E0080
    /// const NONE: df_lighter_core::utils::MarketId = df_lighter_core::utils::MarketId::of::<255>();
    /// ```
    #[must_use]
    pub const fn of<const ID: u64>() -> MarketId {
        const { assert!(ID <= WIDTH_48 && ID != NO_MARKET, "not a market") };
        MarketId(ID)
    }
}

impl AccountIndex {
    /// The LLP, the pool every liquidation's insurance runs through, on both venues.
    pub const LLP: AccountIndex = AccountIndex(MAX_ACCOUNT_INDEX);

    /// For a constant; an index past 2^48 - 2 does not compile.
    #[must_use]
    pub const fn of<const INDEX: u64>() -> AccountIndex {
        const { assert!(INDEX <= MAX_ACCOUNT_INDEX, "not an account") };
        AccountIndex(INDEX)
    }

    /// A master account: the one an address gets with its first deposit.
    #[must_use]
    pub const fn is_master(self) -> bool {
        self.0 < MIN_SUB_ACCOUNT_INDEX
    }
}

impl ApiKeyIndex {
    /// For a constant; 255 does not compile.
    #[must_use]
    pub const fn of<const SLOT: u8>() -> ApiKeyIndex {
        const { assert!(SLOT <= MAX_API_KEY_INDEX, "not an API key slot") };
        ApiKeyIndex(SLOT)
    }
}

impl AssetId {
    pub const ETH: AssetId = AssetId(1);
    pub const LIT: AssetId = AssetId(2);
    /// USDC on Lighter, USDG on Robinhood Chain: the only asset perps take.
    pub const COLLATERAL: AssetId = AssetId(3);

    /// For a constant; 0 or an index past 62 does not compile.
    #[must_use]
    pub const fn of<const ID: u16>() -> AssetId {
        const { assert!(ID != 0 && ID <= MAX_ASSET_INDEX, "not an asset") };
        AssetId(ID)
    }
}

impl ClientOrderIndex {
    /// For a constant; 0 or an index past 2^48 - 1 does not compile.
    #[must_use]
    pub const fn of<const INDEX: u64>() -> ClientOrderIndex {
        const { assert!(INDEX != 0 && INDEX <= WIDTH_48, "not a client order index") };
        ClientOrderIndex(INDEX)
    }
}

/// An order as a cancel or a modify names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderRef {
    /// The index it was placed with. An order placed from the venue's own app has none.
    Client(ClientOrderIndex),
    /// The index the venue gave it.
    Venue(OrderIndex),
}

impl OrderRef {
    /// The integer a cancel or a modify signs: a client index below 2^48, a venue index above.
    #[must_use]
    pub const fn index(self) -> u64 {
        match self {
            OrderRef::Client(client) => client.get(),
            OrderRef::Venue(order) => order.get(),
        }
    }

    /// Either kind, by its range; 0 and anything past 2^60 - 1 are refused.
    pub fn from_index(index: u64) -> Result<OrderRef, BadId> {
        match index {
            0..MIN_ORDER_INDEX => ClientOrderIndex::new(index).map(OrderRef::Client),
            _ => OrderIndex::new(index).map(OrderRef::Venue),
        }
    }
}

impl From<ClientOrderIndex> for OrderRef {
    fn from(client: ClientOrderIndex) -> OrderRef {
        OrderRef::Client(client)
    }
}

impl From<OrderIndex> for OrderRef {
    fn from(order: OrderIndex) -> OrderRef {
        OrderRef::Venue(order)
    }
}

/// Which of an account's two ledgers an amount is in: perps (the collateral margins positions)
/// or spot (every asset held outright).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Route {
    #[default]
    Perps,
    Spot,
}

impl Route {
    /// `0` perps, `1` spot, as a transaction signs it.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Route::Perps => 0,
            Route::Spot => 1,
        }
    }
}

/// A whole number written as a JSON integer or as its digits, `u64` exactly; floats and negatives
/// are refused.
pub(crate) fn whole<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    struct Visitor;

    impl de::Visitor<'_> for Visitor {
        type Value = u64;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a whole number or its digits")
        }

        fn visit_u64<E: de::Error>(self, value: u64) -> Result<u64, E> {
            Ok(value)
        }

        fn visit_i64<E: de::Error>(self, value: i64) -> Result<u64, E> {
            u64::try_from(value).map_err(|_| E::custom(format!("{value} is negative")))
        }

        fn visit_str<E: de::Error>(self, text: &str) -> Result<u64, E> {
            text.trim().parse().map_err(|_| E::custom(format!("`{text}` is not a whole number")))
        }
    }

    deserializer.deserialize_any(Visitor)
}

#[cfg(test)]
#[path = "tests/ids.rs"]
mod tests;
