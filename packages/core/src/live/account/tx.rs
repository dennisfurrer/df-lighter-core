//! `account_tx`: the transactions that touch an account, as the sequencer executed them, each the
//! [`Transaction`] the `tx` read answers. A transaction `sendTx` took (over HTTP or the socket) is
//! only taken; this channel says whether it ran.

use crate::account::history::Transaction;
use crate::utils::AccountIndex;

/// `account_tx`. Needs the account's auth token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountTx {
    /// From the channel.
    pub account: AccountIndex,
    pub txs: Vec<Transaction>,
}
