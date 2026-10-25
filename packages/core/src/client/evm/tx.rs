//! EIP-1559 transactions, type 2, on Ethereum and on Robinhood Chain: an approval, a deposit, a
//! priority transaction on the ZkLighter contract.
//!
//! ```text
//! unsigned = 0x02 || rlp([chainId, nonce, maxPriorityFeePerGas, maxFeePerGas,
//!                         gas, to, value, data, accessList])
//! signed   = 0x02 || rlp([... the same nine, yParity, r, s])
//! ```
//!
//! Without the `sign` feature the unsigned transaction, its signing hash and the raw bytes from a
//! signature made elsewhere (a browser wallet's) still build.

use serde::Deserialize;

use crate::account::keys::{L1Signature, WalletError};
#[cfg(feature = "sign")]
use crate::account::keys::{LocalWallet, recover_digest};
use crate::client::evm::rlp;
use crate::client::evm::u256::{B256, U256};
use crate::utils::Address;
use crate::utils::sign::keccak256;

/// The type byte that leads an EIP-1559 transaction, before and after signing.
pub const EIP1559: u8 = 0x02;

/// Gas asked for over the node's estimate, in percent, so a busier block does not run the
/// transaction out. Unused gas is not charged.
pub const GAS_HEADROOM: u64 = 130;

/// The fee cap over the current base fee: room for two blocks of the largest rise.
const BASE_FEE_TIMES: u64 = 2;

/// A contract and the storage slots the transaction declares it will touch.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessItem {
    pub address: Address,
    pub storage_keys: Vec<B256>,
}

/// The fields of a type 2 transaction, in the order they are encoded. Every amount is in wei of
/// the chain's coin; `data` is the calldata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Eip1559 {
    pub chain_id: u64,
    pub nonce: u64,
    pub max_priority_fee: U256,
    pub max_fee: U256,
    pub gas: u64,
    /// `None` creates a contract from `data`; nothing here does.
    pub to: Option<Address>,
    pub value: U256,
    pub data: Vec<u8>,
    pub access_list: Vec<AccessItem>,
}

impl Eip1559 {
    fn fields(&self) -> Vec<Vec<u8>> {
        let to = match &self.to {
            Some(to) => rlp::bytes(to.bytes()),
            None => rlp::bytes(&[]),
        };
        let access: Vec<Vec<u8>> = self
            .access_list
            .iter()
            .map(|item| {
                let keys: Vec<Vec<u8>> = item.storage_keys.iter().map(|key| rlp::bytes(&key.0)).collect();
                rlp::list(&[rlp::bytes(item.address.bytes()), rlp::list(&keys)])
            })
            .collect();
        vec![
            rlp::uint(u128::from(self.chain_id)),
            rlp::uint(u128::from(self.nonce)),
            rlp::u256(self.max_priority_fee),
            rlp::u256(self.max_fee),
            rlp::uint(u128::from(self.gas)),
            to,
            rlp::u256(self.value),
            rlp::bytes(&self.data),
            rlp::list(&access),
        ]
    }

    /// `0x02 || rlp(fields)`: what the signature commits to.
    #[must_use]
    pub fn unsigned(&self) -> Vec<u8> {
        typed(&self.fields())
    }

    #[must_use]
    pub fn signing_hash(&self) -> B256 {
        B256(keccak256(&self.unsigned()))
    }

    /// `0x02 || rlp(fields, yParity, r, s)`: the bytes a node is sent. The signature is over
    /// [`Eip1559::signing_hash`], from a key here or a signer elsewhere; its `v` must be 27 or 28.
    pub fn signed(&self, signature: &L1Signature) -> Result<Vec<u8>, WalletError> {
        let parity = match signature.v() {
            27 => 0u8,
            28 => 1u8,
            other => return Err(WalletError::BadV(other)),
        };
        let mut fields = self.fields();
        fields.push(rlp::uint(u128::from(parity)));
        fields.push(rlp::word(signature.r()));
        fields.push(rlp::word(signature.s()));
        Ok(typed(&fields))
    }

    /// Signed with the owner's key held in memory: RFC 6979, low-s.
    #[cfg(feature = "sign")]
    pub fn sign(&self, wallet: &LocalWallet) -> Result<Vec<u8>, WalletError> {
        self.signed(&wallet.sign_digest(&self.signing_hash().0)?)
    }

    /// The address that signed it, to check a wallet's signature before sending.
    #[cfg(feature = "sign")]
    pub fn signer(&self, signature: &L1Signature) -> Result<Address, WalletError> {
        recover_digest(&self.signing_hash().0, signature)
    }

    /// The most the gas can cost: every unit at the fee cap.
    #[must_use]
    pub fn gas_ceiling(&self) -> U256 {
        self.max_fee.saturating_mul(U256::from_u64(self.gas))
    }

    /// The most the sender can spend in wei: the gas ceiling and the value.
    #[must_use]
    pub fn most_spent(&self) -> Option<U256> {
        self.max_fee.checked_mul(U256::from_u64(self.gas))?.checked_add(self.value)
    }
}

fn typed(fields: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![EIP1559];
    out.extend(rlp::list(fields));
    out
}

/// The hash a node gives a signed transaction: `keccak256` of its raw bytes.
#[must_use]
pub fn tx_hash(raw: &[u8]) -> B256 {
    B256(keccak256(raw))
}

/// What a transaction does: where it goes, the value it carries, its calldata. A browser wallet
/// sends it as it is (`eth_sendTransaction` with `to`, `value` and `data`); here it becomes an
/// [`Eip1559`] with [`Call::transaction`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Call {
    /// `None` creates a contract from `data`; nothing here does.
    pub to: Option<Address>,
    /// Wei of the chain's coin: zero but for a deposit of ETH itself.
    pub value: U256,
    pub data: Vec<u8>,
}

impl Call {
    /// `data` sent to a contract, with no value.
    #[must_use]
    pub fn to(to: Address, data: Vec<u8>) -> Call {
        Call { to: Some(to), value: U256::ZERO, data }
    }

    /// The transaction that makes this call on `chain_id`.
    #[must_use]
    pub fn transaction(self, chain_id: u64, nonce: u64, gas: u64, fees: Fees) -> Eip1559 {
        Eip1559 {
            chain_id,
            nonce,
            max_priority_fee: fees.max_priority_fee,
            max_fee: fees.max_fee,
            gas,
            to: self.to,
            value: self.value,
            data: self.data,
            access_list: Vec::new(),
        }
    }
}

/// The two fees a type 2 transaction names, in wei per unit of gas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fees {
    pub max_priority_fee: U256,
    pub max_fee: U256,
}

impl Fees {
    /// The tip, and a cap of twice the base fee plus the tip: what a block or two of rising fees
    /// can take. Saturates rather than wraps.
    #[must_use]
    pub fn over_base(base: U256, tip: U256) -> Fees {
        let max_fee = base.saturating_mul(U256::from_u64(BASE_FEE_TIMES)).saturating_add(tip);
        Fees { max_priority_fee: tip, max_fee }
    }
}

/// The node's gas estimate with [`GAS_HEADROOM`].
#[must_use]
pub fn with_headroom(gas: u64) -> u64 {
    let wide = u128::from(gas) * u128::from(GAS_HEADROOM) / 100;
    u64::try_from(wide).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "tests/tx.rs"]
mod tests;
