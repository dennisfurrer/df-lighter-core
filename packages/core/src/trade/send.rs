//! Each transaction of this module signed with a client's key at its next nonce and sent once: a
//! thin layer over [`Client::send`] and [`Client::send_batch`], which count the nonce, verify the
//! signature and settle the count by the answer. A refusal before signing (a leverage past what
//! the venue takes, an amount finer than a millionth) comes back as [`SendError::Sign`] and spends
//! no nonce.

use crate::account::keys::ApiKey;
use crate::client::{Client, Http, SendError, Sent, SentBatch};
use crate::trade::error::TradeError;
use crate::trade::manage::{CancelAllOrders, CancelOrder, ModifyOrder};
use crate::trade::margin::{UpdateLeverage, UpdateMargin};
use crate::trade::orders::{CreateGroupedOrders, CreateOrder, Order};
use crate::utils::sign::{SignTxError, Tx};
use crate::utils::{Dec, MarginMode, MarketId, OrderRef};

/// A builder's refusal as a send's.
fn refused(error: TradeError) -> SendError {
    SendError::Sign(SignTxError::Bad(error.into()))
}

/// Places `order`, with no attributes.
pub async fn place<H: Http>(client: &Client<H, ApiKey>, order: &Order) -> Result<Sent, SendError> {
    client.send(&CreateOrder::new(*order)).await
}

/// Places an entry with its children, or a take-profit and a stop that cancel each other.
pub async fn place_group<H: Http>(client: &Client<H, ApiKey>, group: &CreateGroupedOrders) -> Result<Sent, SendError> {
    client.send(group).await
}

/// Cancels `order` on `market`.
pub async fn cancel<H: Http>(
    client: &Client<H, ApiKey>,
    market: MarketId,
    order: impl Into<OrderRef>,
) -> Result<Sent, SendError> {
    client.send(&CancelOrder::new(market, order)).await
}

/// Cancels every open order on every market, now.
pub async fn cancel_all<H: Http>(client: &Client<H, ApiKey>) -> Result<Sent, SendError> {
    client.send(&CancelAllOrders::now()).await
}

/// Cancels every open order on `market`, now.
pub async fn cancel_all_in<H: Http>(client: &Client<H, ApiKey>, market: MarketId) -> Result<Sent, SendError> {
    client.send(&CancelAllOrders::in_market(market)).await
}

/// Cancels every open order on each of `markets` (each once, at most fifty), now, as one batch:
/// the markets of one kind an account has orders on, say, leaving the others' orders alone.
pub async fn cancel_all_in_markets<H: Http>(
    client: &Client<H, ApiKey>,
    markets: &[MarketId],
) -> Result<SentBatch, SendError> {
    let cancels = CancelAllOrders::in_markets(markets);
    let txs: Vec<&dyn Tx> = cancels.iter().map(|cancel| cancel as &dyn Tx).collect();
    client.send_batch(&txs).await
}

/// Cancels every open order at `at`, unix milliseconds 5 minutes to 15 days from the client's now.
pub async fn schedule_cancel_all<H: Http>(client: &Client<H, ApiKey>, at: u64) -> Result<Sent, SendError> {
    let scheduled = CancelAllOrders::scheduled(at, client.now_ms()).map_err(refused)?;
    client.send(&scheduled).await
}

/// Drops the scheduled cancel-all.
pub async fn abort_cancel_all<H: Http>(client: &Client<H, ApiKey>) -> Result<Sent, SendError> {
    client.send(&CancelAllOrders::abort()).await
}

/// Sends `modify`.
pub async fn modify<H: Http>(client: &Client<H, ApiKey>, modify: &ModifyOrder) -> Result<Sent, SendError> {
    client.send(modify).await
}

/// Sets `market`'s position to `leverage`x, margined `mode`.
pub async fn set_leverage<H: Http>(
    client: &Client<H, ApiKey>,
    market: MarketId,
    leverage: u16,
    mode: MarginMode,
) -> Result<Sent, SendError> {
    let update = UpdateLeverage::new(market, leverage, mode).map_err(refused)?;
    client.send(&update).await
}

/// Moves `amount` of collateral (`25` is 25 USDC) into `market`'s isolated position.
pub async fn add_margin<H: Http>(client: &Client<H, ApiKey>, market: MarketId, amount: Dec) -> Result<Sent, SendError> {
    let update = UpdateMargin::add(market, amount).map_err(refused)?;
    client.send(&update).await
}

/// Moves `amount` of collateral out of `market`'s isolated position.
pub async fn remove_margin<H: Http>(
    client: &Client<H, ApiKey>,
    market: MarketId,
    amount: Dec,
) -> Result<Sent, SendError> {
    let update = UpdateMargin::remove(market, amount).map_err(refused)?;
    client.send(&update).await
}

#[cfg(test)]
#[path = "tests/send.rs"]
mod tests;
