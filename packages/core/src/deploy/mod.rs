//! Binary options markets an operator creates, updates and settles: Lighter only. Lighter
//! assigns an operator its market slots (1000 to 2000) and runs the Up/Down series itself.
//!
//! - [`slot`]: market slots, the internal positions books sit on, and what each range holds.
//! - [`create`]: `BinaryOptionsOperatorCreateMarket`, a book with its window, payout and limits.
//! - [`update`]: `BinaryOptionsOperatorUpdateMarket`, its mutable terms, a freeze among them.
//! - [`settle`]: `BinaryOptionsOperatorSettleMarket`, Up, Down, any price, or a refund.
//!
//! [`send_operator`] sends any of them once, refused on a network that lists no binary options;
//! `Client::send` does not ask.

pub mod create;
pub mod settle;
pub mod slot;
pub mod update;

pub use create::{
    CreateMarket, MAX_EXTENSION_MULTIPLIER, MAX_OPEN_INTEREST, MAX_ORDER_BASE_AMOUNT, MAX_ORDER_QUOTE_AMOUNT,
    SYMBOL_BYTES, Settlement,
};
pub use settle::SettleMarket;
pub use slot::{BINARY_OPTIONS_SLOTS, MarketSlot, PERPS_SLOTS, SPOT_SLOTS};
pub use update::{MAX_LIQUIDATION_FEE, UpdateMarket};

use crate::client::Network;
use crate::utils::sign::tx::{BadTx, Tx};
#[cfg(feature = "sign")]
use crate::{
    account::keys::ApiKey,
    client::{Client, Http, SendError, Sent},
    utils::sign::SignTxError,
};

/// A binary options operator's transaction: [`CreateMarket`], [`UpdateMarket`] or
/// [`SettleMarket`].
pub trait OperatorTx: Tx {}

impl OperatorTx for CreateMarket {}
impl OperatorTx for UpdateMarket {}
impl OperatorTx for SettleMarket {}

/// Refuses, at signing, the chain of a network that lists no binary options.
pub(crate) fn check_chain(chain_id: u32) -> Result<(), BadTx> {
    match Network::ALL.into_iter().find(|network| network.chain_id() == chain_id) {
        Some(network) => check_network(network),
        None => Ok(()),
    }
}

/// Whether `network` takes operator transactions: Lighter does; Lighter on Robinhood Chain lists
/// no binary options.
pub fn check_network(network: Network) -> Result<(), BadTx> {
    match network.has_binary_options() {
        true => Ok(()),
        false => Err(BadTx::new("Network", format!("{network} lists no binary options"))),
    }
}

/// Signs `tx` at the key's next nonce and sends it once, refused before anything is signed on a
/// network without binary options.
#[cfg(feature = "sign")]
pub async fn send_operator<H: Http>(client: &Client<H, ApiKey>, tx: &impl OperatorTx) -> Result<Sent, SendError> {
    check_network(client.network()).map_err(|bad| SendError::Sign(SignTxError::Bad(bad)))?;
    client.send(tx).await
}

#[cfg(test)]
#[path = "tests/deploy.rs"]
mod tests;
