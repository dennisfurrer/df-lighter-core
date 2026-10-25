//! The ZkLighter contract on each network's L1 ([`Network::contract`](crate::client::Network::contract)):
//! the calldata of every call this crate makes, and its answers read back. Every call is the same
//! on both networks.
//!
//! - Deposits: `deposit` credits an L1 address's account with an asset on a route, the amount in
//!   the token's L1 decimals (6 for USDC and USDG, 18 for ETH); an ERC-20 is approved to the
//!   contract first, ETH is sent as the transaction's value. The first deposit makes the account.
//!   A plain token transfer to the contract is not a deposit and strands the funds.
//! - Priority transactions, the venue's way out when its API is down: `withdraw` (a secure
//!   withdrawal), `changePubKey`, `cancelAllOrders`, a reduce-only IOC `createOrder`,
//!   `burnShares`. Each names the account by index and is taken only from its owner.
//! - Claims and reads: `withdrawPendingBalance` claims a secure withdrawal the venue has not
//!   claimed for the owner; `addressToAccountIndex`, `getPendingBalance`, `tokenToAssetIndex`,
//!   `desertMode` and `lastAccountIndex` read the contract's state through `eth_call`.

use crate::client::evm::abi::{self, AbiError, Kind, Token};
use crate::client::evm::u256::{B256, U256};
use crate::utils::sign::PublicKey;
use crate::utils::{AccountIndex, Address, ApiKeyIndex, AssetId, MarketId, OrderKind, Route};

/// `deposit(address _to, uint16 _assetIndex, uint8 _routeType, uint256 _amount)`.
pub const DEPOSIT: [u8; 4] = [0x8a, 0x85, 0x70, 0x83];
/// `depositBatch(uint64[], address[], uint48[])`.
pub const DEPOSIT_BATCH: [u8; 4] = [0xea, 0x21, 0x02, 0xe4];
/// `withdraw(uint48 accountIndex, uint16 assetIndex, uint8 routeType, uint64 amount)`.
pub const WITHDRAW: [u8; 4] = [0xd2, 0x01, 0x91, 0xbd];
/// `changePubKey(uint48 accountIndex, uint8 apiKeyIndex, bytes pubKey)`.
pub const CHANGE_PUB_KEY: [u8; 4] = [0x17, 0x01, 0x0c, 0x68];
/// `cancelAllOrders(uint48 accountIndex)`.
pub const CANCEL_ALL_ORDERS: [u8; 4] = [0xa4, 0xb6, 0xf7, 0x56];
/// `createOrder(uint48 accountIndex, uint16 marketIndex, uint48 baseAmount, uint32 price, uint8
/// isAsk, uint8 orderType)`.
pub const CREATE_ORDER: [u8; 4] = [0x3c, 0x40, 0xc6, 0x76];
/// `burnShares(uint48 accountIndex, uint48 publicPoolIndex, uint64 shareAmount)`.
pub const BURN_SHARES: [u8; 4] = [0xae, 0x15, 0x0b, 0x39];
/// `withdrawPendingBalance(address owner, uint16 assetIndex, uint128 amount)`.
pub const WITHDRAW_PENDING_BALANCE: [u8; 4] = [0x2f, 0x25, 0x80, 0x7e];
/// `addressToAccountIndex(address) returns (uint48)`.
pub const ADDRESS_TO_ACCOUNT_INDEX: [u8; 4] = [0xab, 0xf6, 0xa0, 0x38];
/// `getPendingBalance(address, uint16) returns (uint128)`.
pub const GET_PENDING_BALANCE: [u8; 4] = [0xd1, 0xcb, 0xc6, 0x4f];
/// `tokenToAssetIndex(address) returns (uint16)`.
pub const TOKEN_TO_ASSET_INDEX: [u8; 4] = [0x89, 0x9c, 0xfa, 0x29];
/// `desertMode() returns (bool)`.
pub const DESERT_MODE: [u8; 4] = [0x02, 0xcf, 0xb5, 0x63];
/// `lastAccountIndex()`.
pub const LAST_ACCOUNT_INDEX: [u8; 4] = [0xfa, 0xe2, 0xe0, 0x46];

/// Every function above by its canonical signature, which its selector is the hash of.
pub const SIGNATURES: [(&str, [u8; 4]); 13] = [
    ("deposit(address,uint16,uint8,uint256)", DEPOSIT),
    ("depositBatch(uint64[],address[],uint48[])", DEPOSIT_BATCH),
    ("withdraw(uint48,uint16,uint8,uint64)", WITHDRAW),
    ("changePubKey(uint48,uint8,bytes)", CHANGE_PUB_KEY),
    ("cancelAllOrders(uint48)", CANCEL_ALL_ORDERS),
    ("createOrder(uint48,uint16,uint48,uint32,uint8,uint8)", CREATE_ORDER),
    ("burnShares(uint48,uint48,uint64)", BURN_SHARES),
    ("withdrawPendingBalance(address,uint16,uint128)", WITHDRAW_PENDING_BALANCE),
    ("addressToAccountIndex(address)", ADDRESS_TO_ACCOUNT_INDEX),
    ("getPendingBalance(address,uint16)", GET_PENDING_BALANCE),
    ("tokenToAssetIndex(address)", TOKEN_TO_ASSET_INDEX),
    ("desertMode()", DESERT_MODE),
    ("lastAccountIndex()", LAST_ACCOUNT_INDEX),
];

fn call(selector: [u8; 4], words: &[[u8; 32]]) -> Vec<u8> {
    let mut out = selector.to_vec();
    for word in words {
        out.extend_from_slice(word);
    }
    out
}

fn word(value: impl Into<U256>) -> [u8; 32] {
    value.into().to_be_bytes()
}

fn word_of(address: Address) -> [u8; 32] {
    B256::from_address(address).0
}

/// `deposit(to, asset, route, amount)`: `amount` of `asset` in the token's L1 units, credited to
/// `to`'s account on `route`. Only the collateral goes to perps; any listed asset goes to spot.
#[must_use]
pub fn deposit(to: Address, asset: AssetId, route: Route, amount: U256) -> Vec<u8> {
    call(DEPOSIT, &[word_of(to), word(asset.get()), word(route.code()), amount.to_be_bytes()])
}

/// `depositBatch(amounts, to, accounts)`: the docs say only "deposit to multiple addresses"; the
/// types are a `uint64` amount (the collateral's L1 units), an address and a `uint48`, which in
/// this contract is always an account index, per deposit. The three lists must be as long.
pub fn deposit_batch(amounts: &[u64], to: &[Address], accounts: &[AccountIndex]) -> Result<Vec<u8>, AbiError> {
    if to.len() != amounts.len() || accounts.len() != amounts.len() {
        let got = if to.len() != amounts.len() { to.len() } else { accounts.len() };
        return Err(AbiError::Count { wants: amounts.len(), got });
    }
    let kinds = [
        Kind::Array(Box::new(Kind::Uint(64))),
        Kind::Array(Box::new(Kind::Address)),
        Kind::Array(Box::new(Kind::Uint(48))),
    ];
    let tokens = [
        Token::Array(amounts.iter().map(|amount| Token::uint(*amount)).collect()),
        Token::Array(to.iter().map(|address| Token::address(*address)).collect()),
        Token::Array(accounts.iter().map(|account| Token::uint(account.get())).collect()),
    ];
    let mut out = DEPOSIT_BATCH.to_vec();
    out.extend(abi::encode(&kinds, &tokens)?);
    Ok(out)
}

/// `withdraw(account, asset, route, amount)`: a secure withdrawal as a priority transaction, to
/// the account's owner on the L1, `amount` in the asset's venue decimals as the L2 withdrawal
/// counts it. Taken only from the owner's address.
#[must_use]
pub fn withdraw(account: AccountIndex, asset: AssetId, route: Route, amount: u64) -> Vec<u8> {
    call(WITHDRAW, &[word(account.get()), word(asset.get()), word(route.code()), word(amount)])
}

/// `changePubKey(account, slot, key)`: registers an API key at `slot` from the L1, with no
/// signature but the transaction's own.
#[must_use]
pub fn change_pub_key(account: AccountIndex, slot: ApiKeyIndex, key: &PublicKey) -> Vec<u8> {
    let bytes = key.bytes();
    let mut tail = [0u8; 64];
    tail[..bytes.len()].copy_from_slice(bytes);
    let mut first = [0u8; 32];
    let mut second = [0u8; 32];
    first.copy_from_slice(&tail[..32]);
    second.copy_from_slice(&tail[32..]);
    // Two words of head before the offset's own: the bytes start at 0x60.
    call(CHANGE_PUB_KEY, &[word(account.get()), word(slot.get()), word(96u8), word(bytes.len() as u64), first, second])
}

/// `cancelAllOrders(account)`.
#[must_use]
pub fn cancel_all_orders(account: AccountIndex) -> Vec<u8> {
    call(CANCEL_ALL_ORDERS, &[word(account.get())])
}

/// `createOrder(account, market, base_amount, price, is_ask, kind)`: a reduce-only IOC order
/// from the L1, `base_amount` and `price` the market's integers as an L2 order signs them. The
/// contract counts a market in 16 bits and a size in 48: a wider one is refused.
pub fn create_order(
    account: AccountIndex,
    market: MarketId,
    base_amount: u64,
    price: u32,
    is_ask: bool,
    kind: OrderKind,
) -> Result<Vec<u8>, AbiError> {
    let market_word = u16::try_from(market.get())
        .map_err(|_| AbiError::Mismatch { kind: "uint16".to_owned(), token: format!("market {market}") })?;
    if base_amount >= 1 << 48 {
        return Err(AbiError::Mismatch { kind: "uint48".to_owned(), token: format!("the integer {base_amount}") });
    }
    Ok(call(
        CREATE_ORDER,
        &[
            word(account.get()),
            word(market_word),
            word(base_amount),
            word(price),
            word(u8::from(is_ask)),
            word(kind.code()),
        ],
    ))
}

/// `burnShares(account, pool, shares)`: shares of a public pool, the LLP among them, burnt from
/// the L1.
#[must_use]
pub fn burn_shares(account: AccountIndex, pool: AccountIndex, shares: u64) -> Vec<u8> {
    call(BURN_SHARES, &[word(account.get()), word(pool.get()), word(shares)])
}

/// `withdrawPendingBalance(owner, asset, amount)`: claims a secure withdrawal the venue has not
/// claimed for `owner`, `amount` as [`get_pending_balance`] answers it.
#[must_use]
pub fn withdraw_pending_balance(owner: Address, asset: AssetId, amount: u128) -> Vec<u8> {
    call(WITHDRAW_PENDING_BALANCE, &[word_of(owner), word(asset.get()), word(amount)])
}

/// `addressToAccountIndex(owner)`; its answer reads with [`read_account_index`].
#[must_use]
pub fn address_to_account_index(owner: Address) -> Vec<u8> {
    call(ADDRESS_TO_ACCOUNT_INDEX, &[word_of(owner)])
}

/// `getPendingBalance(owner, asset)`; its answer reads with [`read_pending_balance`].
#[must_use]
pub fn get_pending_balance(owner: Address, asset: AssetId) -> Vec<u8> {
    call(GET_PENDING_BALANCE, &[word_of(owner), word(asset.get())])
}

/// `tokenToAssetIndex(token)`; its answer reads with [`read_asset`].
#[must_use]
pub fn token_to_asset_index(token: Address) -> Vec<u8> {
    call(TOKEN_TO_ASSET_INDEX, &[word_of(token)])
}

/// `desertMode()`; its answer reads with [`read_desert_mode`].
#[must_use]
pub fn desert_mode() -> Vec<u8> {
    DESERT_MODE.to_vec()
}

/// `lastAccountIndex()`; its answer reads with [`read_last_account_index`].
#[must_use]
pub fn last_account_index() -> Vec<u8> {
    LAST_ACCOUNT_INDEX.to_vec()
}

fn one(kind: Kind, answer: &[u8]) -> Result<Token, AbiError> {
    abi::decode(&[kind], answer)?.into_iter().next().ok_or(AbiError::Short(answer.len()))
}

fn whole(kind: Kind, answer: &[u8]) -> Result<U256, AbiError> {
    one(kind, answer)?.as_u256().ok_or(AbiError::Short(answer.len()))
}

/// An index the contract answered: an account index, or refused.
fn account_of(value: U256) -> Result<AccountIndex, AbiError> {
    value
        .to_u64()
        .and_then(|index| AccountIndex::new(index).ok())
        .ok_or_else(|| AbiError::Misfit { at: 0, kind: "an account index".to_owned() })
}

/// The master account an address owns; `None` before its first deposit (the contract answers
/// 0, the treasury's index, for an address it does not know).
pub fn read_account_index(answer: &[u8]) -> Result<Option<AccountIndex>, AbiError> {
    let value = whole(Kind::Uint(48), answer)?;
    match value.is_zero() {
        true => Ok(None),
        false => account_of(value).map(Some),
    }
}

/// What a secure withdrawal left waiting for `owner` to claim, in the units the contract counts
/// the asset in.
pub fn read_pending_balance(answer: &[u8]) -> Result<u128, AbiError> {
    whole(Kind::Uint(128), answer)?.to_u128().ok_or(AbiError::Short(answer.len()))
}

/// The asset an ERC-20 is listed as; `None` for a token the contract does not take.
pub fn read_asset(answer: &[u8]) -> Result<Option<AssetId>, AbiError> {
    let index = whole(Kind::Uint(16), answer)?.to_u64().and_then(|index| u16::try_from(index).ok());
    Ok(index.and_then(|index| AssetId::new(index).ok()))
}

/// Whether desert mode is on: the venue has stopped and only exits from the L1 remain.
pub fn read_desert_mode(answer: &[u8]) -> Result<bool, AbiError> {
    one(Kind::Bool, answer)?.as_bool().ok_or(AbiError::Short(answer.len()))
}

/// The highest master account index given out so far.
pub fn read_last_account_index(answer: &[u8]) -> Result<AccountIndex, AbiError> {
    account_of(whole(Kind::Uint(48), answer)?)
}

#[cfg(test)]
#[path = "tests/zklighter.rs"]
mod tests;
