//! Helpers every Lighter app needs: exact decimals, a market's grid, the ids things are named by,
//! integer units, time, hex and base64, nonces, the enums orders are described by and the signing
//! primitives.
pub mod address;
pub mod base64;
pub mod dec;
pub mod figure;
pub mod grid;
pub mod hex;
pub mod ids;
pub mod json;
pub(crate) mod lenient;
pub mod nonce;
pub mod order;
pub mod sign;
pub mod time;
pub mod units;

pub use address::{Address, BadAddress};
pub use base64::BadBase64;
pub use dec::{BadDec, Dec, Round};
pub use figure::Figure;
pub use grid::{Grid, MAX_PRICE_UNITS, MAX_SIZE_UNITS};
pub use hex::BadHex;
pub use ids::{AccountIndex, ApiKeyIndex, AssetId, BadId, ClientOrderIndex, MarketId, OrderIndex, OrderRef, Route};
pub use json::Json;
pub use nonce::NonceCounter;
pub use order::{MarginMode, MarketKind, OrderKind, Side, TimeInForce};
pub use units::{BadUnits, COLLATERAL_DECIMALS, LIT_DECIMALS, cut_units, exact_units, from_units};
