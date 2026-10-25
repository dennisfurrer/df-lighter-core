//! `Withdraw` (type 13): a secure withdrawal, an asset out of the signing account to the owner's
//! own L1 address, the only place it can go, so the API key alone signs it.
//!
//! The amount is in the venue's decimals for the asset, as a transfer's is. The venue proves the
//! withdrawal on the L1 after `withdrawalDelay` ([`crate::transfer::fees::withdrawal_delay`]) and
//! usually claims it for the owner; one it has not claimed waits in the contract for
//! `withdrawPendingBalance` ([`crate::client::evm::zklighter::withdraw_pending_balance`]). For an
//! address other than the owner's, or money on the L1 at once, see [`crate::transfer::fast`].

#[cfg(feature = "sign")]
use crate::account::keys::ApiKey;
use crate::client::Network;
#[cfg(feature = "sign")]
use crate::client::{Client, Http, SendError, Sent};
use crate::transfer::send::{MAX_AMOUNT, halves};
use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{BadTx, Tx, TxType, element, number};
use crate::utils::units::{BadUnits, exact_units};
use crate::utils::{AssetId, Dec, Route};

/// `Withdraw`: `amount` of `asset` out of the signing account's `route`, to its owner's L1
/// address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Withdraw {
    pub asset: AssetId,
    pub route: Route,
    /// Units of the asset at its venue decimals, 1 to [`MAX_AMOUNT`].
    pub amount: u64,
}

impl Withdraw {
    /// `amount` of `asset` counted in `decimals` (the asset's venue decimals from
    /// `assetDetails`), out of `route`. An amount finer than `decimals` is refused, never rounded.
    pub fn new(asset: AssetId, decimals: u32, route: Route, amount: Dec) -> Result<Withdraw, BadUnits> {
        Ok(Withdraw { asset, route, amount: exact_units(amount, decimals, MAX_AMOUNT)? })
    }

    /// `amount` of the network's collateral (USDC, or USDG) out of perps.
    pub fn collateral(network: Network, amount: Dec) -> Result<Withdraw, BadUnits> {
        let collateral = network.collateral();
        Withdraw::new(collateral.asset, collateral.decimals, Route::Perps, amount)
    }
}

impl Tx for Withdraw {
    fn tx_type(&self) -> TxType {
        TxType::Withdraw
    }

    fn hash_fields(&self) -> Vec<Fp> {
        let mut fields = vec![element(self.asset.get()), element(self.route.code())];
        fields.extend(halves(self.amount));
        fields
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![
            ("AssetIndex", number(self.asset.get())),
            ("RouteType", number(self.route.code())),
            ("Amount", number(self.amount)),
        ]
    }

    fn account_key(&self) -> &'static str {
        "FromAccountIndex"
    }

    /// lighter-go's `L2WithdrawTxInfo.Validate`; the asset and route are checked by their types.
    fn check(&self) -> Result<(), BadTx> {
        if self.amount == 0 {
            return Err(BadTx::new("Amount", "a withdrawal moves at least 1 unit (WithdrawalAmountTooLow)"));
        }
        if self.amount > MAX_AMOUNT {
            return Err(BadTx::new("Amount", "a withdrawal moves at most 2^60 - 1 units"));
        }
        Ok(())
    }
}

/// Withdraws `amount` of the network's collateral out of the client's account's perps, to its
/// owner's L1 address. Sent once.
#[cfg(feature = "sign")]
pub async fn withdraw<H: Http>(client: &Client<H, ApiKey>, amount: Dec) -> Result<Sent, SendError> {
    let withdraw = Withdraw::collateral(client.network(), amount)
        .map_err(|error| crate::transfer::send::refused_units("Amount", &error))?;
    client.send(&withdraw).await
}

#[cfg(test)]
#[path = "tests/withdraw.rs"]
mod tests;
