//! Opening a sub-account: `CreateSubAccount`, signed by an API key of the master account. The new
//! account takes the next index down from 2^48 - 2, is owned by the master's L1 address and holds
//! nothing; it needs an API key of its own registered before it trades, and money moved in with a
//! transfer. How many an owner may hold depends on its tier: on Standard, 4 on Lighter and 8 on
//! Robinhood Chain ([`crate::client::Network::standard_sub_accounts`]); 16 on Plus, 64 on Premium.
//!
//! Only a master account opens one (lighter-go refuses an account index from 2^47 up), which
//! [`create_sub_account`] checks before it signs. The new index comes back on the account's
//! `account_tx` channel or from `accountsByL1Address`.

use crate::utils::json::Json;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{Tx, TxType};
#[cfg(feature = "sign")]
use crate::{
    account::keys::ApiKey,
    client::{Client, Http, SendError, Sent},
    utils::sign::{BadTx, SignTxError},
};

/// `CreateSubAccount`: no fields of its own; the envelope's account is the master.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CreateSubAccount;

impl Tx for CreateSubAccount {
    fn tx_type(&self) -> TxType {
        TxType::CreateSubAccount
    }

    fn hash_fields(&self) -> Vec<Fp> {
        Vec::new()
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        Vec::new()
    }
}

/// Opens a sub-account of the client's account, which must be a master.
#[cfg(feature = "sign")]
pub async fn create_sub_account<H: Http>(client: &Client<H, ApiKey>) -> Result<Sent, SendError> {
    if !client.key().account().is_master() {
        let why = format!("account {} is not a master: only a master opens sub-accounts", client.key().account());
        return Err(SendError::Sign(SignTxError::Bad(BadTx::new("AccountIndex", why))));
    }
    client.send(&CreateSubAccount).await
}

#[cfg(test)]
#[cfg(feature = "sign")]
#[path = "tests/subaccounts.rs"]
mod tests;
