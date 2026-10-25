//! Deposits through the ZkLighter contract on the network's L1, the way the venue's own app makes
//! them:
//!
//! 1. Checking: the node is on the network's L1, the wallet holds the amount, and the allowance
//!    it gave the contract ([`plan_deposit`]).
//! 2. Approving, only when the allowance is short and only for the amount: `approve(contract,
//!    amount)` on the collateral, mined before anything else.
//! 3. Depositing: `deposit(to, 3, perps, amount)` on the contract, the amount in the token's L1
//!    decimals.
//! 4. Confirming: its receipt, mined and not reverted. The venue credits the account once it sees
//!    the deposit on the L1; the first deposit makes the account.
//!
//! A plain token transfer to the contract is not a deposit and strands the funds. Everything up
//! to the transaction is a pure function ([`deposit_call`], [`approve_call`]) or a [`Query`], for
//! a wallet elsewhere (a browser's `eth_sendTransaction` takes [`DepositPlan::calls`] as they
//! are); [`deposit`] runs the four steps with a key held here.

#[cfg(feature = "sign")]
use crate::account::keys::LocalWallet;
use crate::account::keys::WalletError;
use crate::client::evm::records::Receipt;
use crate::client::evm::rpc::{BlockTag, CallRequest, L1Error, Query, Rpc, RpcError, eth};
use crate::client::evm::tx::Call;
#[cfg(feature = "sign")]
use crate::client::evm::tx::tx_hash;
use crate::client::evm::u256::{B256, BadAmount, U256};
use crate::client::evm::zklighter;
use crate::client::{Http, Network};
use crate::transfer::erc20;
use crate::utils::{Address, AssetId, Dec, Route};

/// The least a deposit of the collateral carries, by the docs: 1 USDC on Lighter, 1 USDG on
/// Robinhood Chain. Less is refused here.
pub const MINIMUM_DEPOSIT: Dec = Dec::from_int(1);

/// Why an amount is not deposited: nothing was signed or sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadDeposit {
    #[error("{0} is not a positive amount")]
    NotPositive(Dec),
    #[error("{value} has more than the token's {decimals} decimal places")]
    TooFine { value: Dec, decimals: u32 },
    #[error("{amount} {symbol} is below the {minimum} {symbol} a deposit carries at least")]
    BelowMinimum { amount: Dec, minimum: Dec, symbol: &'static str },
    #[error("{0} is more than this can count")]
    TooLarge(Dec),
}

impl BadDeposit {
    fn from_amount(error: BadAmount, value: Dec) -> BadDeposit {
        match error {
            BadAmount::NotPositive(value) => BadDeposit::NotPositive(value),
            BadAmount::TooFine { value, decimals } => BadDeposit::TooFine { value, decimals },
            BadAmount::TooLarge | BadAmount::Unkept { .. } => BadDeposit::TooLarge(value),
        }
    }
}

/// `amount` of `decimals` places as the token's units, exactly: positive, and no finer.
fn units(amount: Dec, decimals: u32) -> Result<U256, BadDeposit> {
    if !amount.is_positive() {
        return Err(BadDeposit::NotPositive(amount));
    }
    U256::from_dec(amount, decimals).map_err(|error| BadDeposit::from_amount(error, amount))
}

/// `amount` of the network's collateral in its L1 units (6 decimals for USDC and USDG), at least
/// [`MINIMUM_DEPOSIT`].
pub fn collateral_units(network: Network, amount: Dec) -> Result<U256, BadDeposit> {
    let collateral = network.collateral();
    if amount.is_positive() && amount < MINIMUM_DEPOSIT {
        return Err(BadDeposit::BelowMinimum { amount, minimum: MINIMUM_DEPOSIT, symbol: collateral.symbol });
    }
    units(amount, collateral.l1_decimals)
}

/// `approve(contract, amount)` on the network's collateral: what lets the contract take `amount`
/// units ([`collateral_units`]) from the wallet.
#[must_use]
pub fn approve_call(network: Network, amount: U256) -> Call {
    Call::to(network.collateral().token, erc20::approve(network.contract(), amount))
}

/// `approve(contract, 2^256 - 1)`: an allowance that never runs out, as the venues' own apps set,
/// so later deposits skip the approval.
#[must_use]
pub fn approve_max_call(network: Network) -> Call {
    approve_call(network, U256::MAX)
}

/// `deposit(to, collateral, perps, amount)` on the network's contract: `amount` of USDC (or USDG)
/// credited to `to`'s account, its perps.
pub fn deposit_call(network: Network, to: Address, amount: Dec) -> Result<Call, BadDeposit> {
    let collateral = network.collateral();
    let units = collateral_units(network, amount)?;
    Ok(Call::to(network.contract(), zklighter::deposit(to, collateral.asset, Route::Perps, units)))
}

/// `deposit(to, asset, route, amount)` for any listed asset: `token` is its L1 contract
/// (`assetDetails`' `l1_address`), `None` for ETH, which goes as the transaction's value; the
/// amount is counted in `l1_decimals`. Only the collateral goes to perps; an ERC-20 needs its own
/// approval ([`erc20::approve`] to [`Network::contract`]).
pub fn asset_deposit_call(
    network: Network,
    to: Address,
    asset: AssetId,
    route: Route,
    token: Option<Address>,
    l1_decimals: u32,
    amount: Dec,
) -> Result<Call, BadDeposit> {
    let units = units(amount, l1_decimals)?;
    let mut call = Call::to(network.contract(), zklighter::deposit(to, asset, route, units));
    if token.is_none() {
        call.value = units;
    }
    Ok(call)
}

/// The owner's collateral on the network's L1 (`balanceOf` at the latest block); its answer
/// reads with [`collateral_amount`].
#[must_use]
pub fn balance_request(network: Network, owner: Address) -> Query<Vec<u8>> {
    eth::call(&CallRequest::to(network.collateral().token, erc20::balance_of(owner)), BlockTag::Latest)
}

/// What the owner has let the contract take (`allowance`); its answer reads with
/// [`erc20::read_amount`].
#[must_use]
pub fn allowance_request(network: Network, owner: Address) -> Query<Vec<u8>> {
    eth::call(
        &CallRequest::to(network.collateral().token, erc20::allowance(owner, network.contract())),
        BlockTag::Latest,
    )
}

/// The master account an L1 address owns, by the contract (`addressToAccountIndex`); its answer
/// reads with [`zklighter::read_account_index`]. The venue's `accountsByL1Address` says the same.
#[must_use]
pub fn account_index_request(network: Network, owner: Address) -> Query<Vec<u8>> {
    eth::call(&CallRequest::to(network.contract(), zklighter::address_to_account_index(owner)), BlockTag::Latest)
}

/// A [`balance_request`] answer as the collateral, exactly.
pub fn collateral_amount(network: Network, answer: &[u8]) -> Result<Dec, RpcError> {
    let unreadable = |detail: String| RpcError::Unreadable { method: "eth_call", detail };
    let units = erc20::read_amount(answer).map_err(|error| unreadable(error.to_string()))?;
    units.to_dec(network.collateral().l1_decimals).map_err(|error| unreadable(error.to_string()))
}

/// Why a deposit did not happen, or is not known to have.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DepositError {
    #[error(transparent)]
    Bad(#[from] BadDeposit),
    /// The node is not on the network's L1: nothing was signed.
    #[error("the node is on chain {found}, not {expected}; nothing was sent")]
    WrongChain { expected: u64, found: u64 },
    /// The wallet holds less than the deposit: nothing was signed.
    #[error("the wallet holds {have} {symbol}, less than the {need} to deposit")]
    Short { have: Dec, need: Dec, symbol: &'static str },
    #[error(transparent)]
    Rpc(#[from] RpcError),
    #[error(transparent)]
    Wallet(#[from] WalletError),
    /// Mined, and reverted: the approval or the deposit did nothing but spend gas.
    #[error("transaction {0} reverted")]
    Reverted(B256),
    /// Sent, but no receipt came while waiting: it may still land, so it is never sent again.
    /// Read its receipt by this hash.
    #[error("transaction {0} was sent and is not yet mined; it may still land")]
    Unconfirmed(B256),
}

impl From<L1Error> for DepositError {
    fn from(error: L1Error) -> DepositError {
        match error {
            L1Error::Rpc(error) => DepositError::Rpc(error),
            L1Error::Wallet(error) => DepositError::Wallet(error),
            L1Error::WrongChain { expected, found } => DepositError::WrongChain { expected, found },
        }
    }
}

/// What a deposit of the collateral takes, read from the L1 before anything is signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepositPlan {
    pub network: Network,
    /// The address that pays: it holds the collateral and signs.
    pub owner: Address,
    /// The address whose account is credited, usually the owner.
    pub to: Address,
    /// The collateral's L1 units.
    pub amount: U256,
    pub balance: U256,
    pub allowance: U256,
    /// `approve(contract, amount)`, when the allowance is short of the amount.
    pub approve: Option<Call>,
    pub deposit: Call,
}

impl DepositPlan {
    /// The chain every call goes to: the network's L1.
    #[must_use]
    pub const fn chain_id(&self) -> u64 {
        self.network.l1_chain_id()
    }

    /// The calls to send, in order, each mined before the next: the approval when it is needed,
    /// then the deposit.
    #[must_use]
    pub fn calls(&self) -> Vec<Call> {
        self.approve.iter().cloned().chain([self.deposit.clone()]).collect()
    }
}

/// Checking: refused unless `rpc` is on the network's L1 and `owner` holds `amount` of the
/// collateral; the approval planned when the allowance is short. Three reads, nothing signed.
pub async fn plan_deposit<H: Http>(
    rpc: &Rpc<H>,
    network: Network,
    owner: Address,
    to: Address,
    amount: Dec,
) -> Result<DepositPlan, DepositError> {
    let units = collateral_units(network, amount)?;
    let deposit = deposit_call(network, to, amount)?;
    rpc.check_chain(network.l1_chain_id()).await?;
    let unreadable = |detail: String| RpcError::Unreadable { method: "eth_call", detail };
    let balance = erc20::read_amount(&rpc.run(balance_request(network, owner)).await?)
        .map_err(|error| unreadable(error.to_string()))?;
    let collateral = network.collateral();
    if balance < units {
        let have = balance.to_dec(collateral.l1_decimals).unwrap_or(Dec::ZERO);
        return Err(DepositError::Short { have, need: amount, symbol: collateral.symbol });
    }
    let allowance = erc20::read_amount(&rpc.run(allowance_request(network, owner)).await?)
        .map_err(|error| unreadable(error.to_string()))?;
    let approve = (allowance < units).then(|| approve_call(network, units));
    Ok(DepositPlan { network, owner, to, amount: units, balance, allowance, approve, deposit })
}

/// The receipts of a deposit made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deposited {
    /// The approval's, when one was needed.
    pub approval: Option<Receipt>,
    pub deposit: Receipt,
}

/// Deposits `amount` of the network's collateral from `wallet` into its own account on the
/// network, all four steps: checked ([`plan_deposit`]), approved for the amount when the allowance
/// is short and that approval mined, deposited, and the deposit mined. Each transaction takes the
/// node's nonce, gas estimate and fees ([`Rpc::prepare`]) and is sent once. `pause` waits between
/// receipt reads ([`Rpc::wait_for_receipt`]). Sent but not seen mined is
/// [`DepositError::Unconfirmed`], never sent again.
#[cfg(feature = "sign")]
pub async fn deposit<H: Http>(
    rpc: &Rpc<H>,
    wallet: &LocalWallet,
    network: Network,
    amount: Dec,
    pause: impl AsyncFn(u64),
) -> Result<Deposited, DepositError> {
    let owner = wallet.address();
    let plan = plan_deposit(rpc, network, owner, owner, amount).await?;
    let approval = match plan.approve {
        Some(call) => Some(mined(rpc, wallet, network, call, &pause).await?),
        None => None,
    };
    let deposit = mined(rpc, wallet, network, plan.deposit, &pause).await?;
    Ok(Deposited { approval, deposit })
}

/// `call` signed, sent once and waited for until mined and not reverted.
#[cfg(feature = "sign")]
async fn mined<H: Http>(
    rpc: &Rpc<H>,
    wallet: &LocalWallet,
    network: Network,
    call: Call,
    pause: &impl AsyncFn(u64),
) -> Result<Receipt, DepositError> {
    let raw = rpc.sign_call(wallet, network.l1_chain_id(), call).await?;
    let hash = tx_hash(&raw);
    match rpc.send_raw_transaction(&raw).await {
        Ok(_) => {}
        Err(error) if error.may_have_landed() => return Err(DepositError::Unconfirmed(hash)),
        Err(error) => return Err(error.into()),
    }
    match rpc.wait_for_receipt(hash, async |ms| pause(ms).await).await {
        Ok(Some(receipt)) if receipt.succeeded() => Ok(receipt),
        Ok(Some(_)) => Err(DepositError::Reverted(hash)),
        Ok(None) | Err(_) => Err(DepositError::Unconfirmed(hash)),
    }
}

#[cfg(test)]
#[path = "tests/deposit.rs"]
mod tests;
