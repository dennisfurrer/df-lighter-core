//! What moving money costs and how long it takes: a transfer's fee (`transferFeeInfo`, the
//! account's own read, weight 500) and the time a secure withdrawal waits before the L1 pays it
//! (`withdrawalDelay`, public).

use serde::Deserialize;

use crate::client::http::{ApiError, Call, Transport, read};
use crate::utils::AccountIndex;
use crate::utils::ids::whole;

#[derive(Deserialize)]
struct TransferFeeInfo {
    #[serde(deserialize_with = "whole")]
    transfer_fee_usdc: u64,
}

#[derive(Deserialize)]
struct WithdrawalDelay {
    #[serde(deserialize_with = "whole")]
    seconds: u64,
}

/// What `transferFeeInfo` asks: the fee from `account` to `to`, or to any account when `None`.
#[must_use]
pub fn transfer_fee_call(account: AccountIndex, to: Option<AccountIndex>) -> Call {
    Call::get("transferFeeInfo").field("account_index", account).maybe("to_account_index", to).authed()
}

/// The fee a transfer from `account` to `to` must carry (`USDCFee`), in the collateral's
/// millionths: what the venue charges on top of the amount, and exactly what it takes.
pub async fn transfer_fee(
    transport: &impl Transport,
    account: AccountIndex,
    to: Option<AccountIndex>,
) -> Result<u64, ApiError> {
    Ok(read::<TransferFeeInfo>(transport, transfer_fee_call(account, to)).await?.transfer_fee_usdc)
}

/// What `withdrawalDelay` asks.
#[must_use]
pub fn withdrawal_delay_call() -> Call {
    Call::get("withdrawalDelay")
}

/// How long a secure withdrawal waits before the L1 pays it, in seconds: each venue's own, and it
/// moves with the batches being proved.
pub async fn withdrawal_delay(transport: &impl Transport) -> Result<u64, ApiError> {
    Ok(read::<WithdrawalDelay>(transport, withdrawal_delay_call()).await?.seconds)
}

#[cfg(test)]
#[path = "tests/fees.rs"]
mod tests;
