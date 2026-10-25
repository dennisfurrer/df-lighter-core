//! The ERC-20 calls a deposit needs: `approve` (the ZkLighter contract as the spender),
//! `allowance` and `balanceOf`, and their answers.

use crate::client::evm::abi::{self, AbiError, Kind};
use crate::client::evm::u256::{B256, U256};
use crate::utils::Address;

/// `approve(address,uint256)`.
pub const APPROVE: [u8; 4] = [0x09, 0x5e, 0xa7, 0xb3];
/// `allowance(address,address)`.
pub const ALLOWANCE: [u8; 4] = [0xdd, 0x62, 0xed, 0x3e];
/// `balanceOf(address)`.
pub const BALANCE_OF: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];

fn call(selector: [u8; 4], words: &[[u8; 32]]) -> Vec<u8> {
    let mut out = selector.to_vec();
    for word in words {
        out.extend_from_slice(word);
    }
    out
}

/// `approve(spender, amount)`, the amount in the token's own units; [`U256::MAX`] for an allowance
/// that never runs out, as the venues' own apps set.
#[must_use]
pub fn approve(spender: Address, amount: U256) -> Vec<u8> {
    call(APPROVE, &[B256::from_address(spender).0, amount.to_be_bytes()])
}

/// `allowance(owner, spender)`; its answer reads with [`read_amount`].
#[must_use]
pub fn allowance(owner: Address, spender: Address) -> Vec<u8> {
    call(ALLOWANCE, &[B256::from_address(owner).0, B256::from_address(spender).0])
}

/// `balanceOf(owner)`; its answer reads with [`read_amount`].
#[must_use]
pub fn balance_of(owner: Address) -> Vec<u8> {
    call(BALANCE_OF, &[B256::from_address(owner).0])
}

/// A `uint256` answer: a balance or an allowance, in the token's own units.
pub fn read_amount(answer: &[u8]) -> Result<U256, AbiError> {
    abi::decode(&[Kind::Uint(256)], answer)?
        .into_iter()
        .next()
        .and_then(|token| token.as_u256())
        .ok_or(AbiError::Short(answer.len()))
}

#[cfg(test)]
#[path = "tests/erc20.rs"]
mod tests;
