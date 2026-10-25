//! A fast withdrawal: the collateral to any L1 address at once, paid out of the venue's liquidity
//! pool rather than proved on the L1. It is a [`Transfer`] to the pool's account with the
//! address in the memo, signed by the API key and by the owner, posted to `fastwithdraw` rather
//! than `sendTx`:
//!
//! 1. `fastwithdraw/info` (the account's own read): the pool's account and the limits.
//! 2. `transferFeeInfo` for the pool's account: the fee, charged on top.
//! 3. The transfer, its memo the address and twelve zeros, and the owner's signature over it.
//! 4. `fastwithdraw` with the signed `tx_info` and the address (weight 3,000).
//!
//! Collateral only: USDC on Lighter (4 USDC at least, by the docs), USDG on Robinhood Chain (1
//! USDG). An address may make [`Network::fast_withdrawals_per_hour`] of them an hour. An offline
//! build refuses the post.

use serde::Deserialize;

#[cfg(feature = "sign")]
use crate::account::keys::{ApiKey, Wallet, verify};
use crate::client::Network;
use crate::client::http::{ApiError, Call, Transport, read};
#[cfg(feature = "sign")]
use crate::client::{Client, Http, SendError};
use crate::transfer::send::{Memo, Transfer};
use crate::utils::units::BadUnits;
use crate::utils::{AccountIndex, Address, Dec};

/// The least a fast withdrawal pays out, by the docs: 4 USDC on Lighter, 1 USDG on Robinhood
/// Chain. The venue refuses less; nothing here does.
#[must_use]
pub const fn minimum_fast_withdrawal(network: Network) -> Dec {
    match network {
        Network::Lighter => Dec::from_int(4),
        Network::Robinhood => Dec::from_int(1),
    }
}

/// What `fastwithdraw/info` answers for an account.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FastWithdrawInfo {
    /// The pool's account, which a fast withdrawal is a transfer to.
    #[serde(rename = "to_account_index")]
    pub pool: AccountIndex,
    /// What the pool can pay out now, as the venue writes it.
    pub withdraw_limit: Dec,
    /// The most one fast withdrawal may carry, as the venue writes it.
    pub max_withdrawal_amount: Dec,
}

/// What `fastwithdraw` answers: the transfer taken.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FastWithdrawn {
    /// The transfer's hash, when the venue gives it.
    #[serde(default)]
    pub tx_hash: Option<String>,
}

/// What `fastwithdraw/info` asks: the account's own read.
#[must_use]
pub fn fast_withdraw_info_call(account: AccountIndex) -> Call {
    Call::get("fastwithdraw/info").field("account_index", account).authed()
}

/// The pool a fast withdrawal from `account` goes to, and its limits.
pub async fn fast_withdraw_info(
    transport: &impl Transport,
    account: AccountIndex,
) -> Result<FastWithdrawInfo, ApiError> {
    read(transport, fast_withdraw_info_call(account)).await
}

/// The transfer a fast withdrawal of `amount` collateral to `to` signs: to the pool, perps to
/// perps, with `fee` (the collateral's millionths, from `transferFeeInfo` for the pool) and `to`
/// in the memo.
pub fn fast_withdrawal(
    network: Network,
    pool: AccountIndex,
    to: Address,
    amount: Dec,
    fee: u64,
) -> Result<Transfer, BadUnits> {
    Ok(Transfer::collateral(network, pool, amount)?.with_fee(fee).with_memo(Memo::address(to)))
}

/// What `fastwithdraw` posts: the signed transfer's `tx_info`, the owner's signature in it, and
/// the address it pays. Sent with the account's auth token; never retried.
#[must_use]
pub fn fast_withdraw_call(tx_info: String, to: Address) -> Call {
    Call::post("fastwithdraw").field("tx_info", tx_info).field("to_address", to.to_hex()).authed()
}

/// Withdraws `amount` of the network's collateral from the client's account to `to` on the L1 at
/// once: the pool and its fee asked, the transfer signed by the key and by `owner` (checked before
/// anything is posted), posted to `fastwithdraw` once. On any failure the key's nonce count is
/// forgotten, so the next transaction asks `nextNonce`.
#[cfg(feature = "sign")]
pub async fn fast_withdraw<H: Http>(
    client: &Client<H, ApiKey>,
    owner: &impl Wallet,
    to: Address,
    amount: Dec,
) -> Result<FastWithdrawn, SendError> {
    let account = client.key().account();
    let info = fast_withdraw_info(client, account).await?;
    let fee = crate::transfer::fees::transfer_fee(client, account, Some(info.pool)).await?;
    let transfer = fast_withdrawal(client.network(), info.pool, to, amount, fee)
        .map_err(|error| crate::transfer::send::refused_units("Amount", &error))?;
    let signed = client.sign(&transfer).await?;
    let posted = async {
        let message = signed.l1_message().unwrap_or_default();
        let signature = owner.sign_message(message).await?;
        verify(message, &signature, owner.address())?;
        let tx_info = signed.tx_info_with_l1(&signature.to_hex()).unwrap_or_else(|| signed.tx_info());
        Ok::<_, SendError>(read::<FastWithdrawn>(client, fast_withdraw_call(tx_info, to)).await?)
    }
    .await;
    if posted.is_err() {
        client.forget_nonce();
    }
    posted
}

#[cfg(test)]
#[path = "tests/fast.rs"]
mod tests;
