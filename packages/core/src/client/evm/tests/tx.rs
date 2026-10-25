//! Transactions held to go-ethereum 1.15.6's `types.SignTx` (`vectors/lighter_go.json`, `evm`): a
//! deposit on Ethereum, an approval and a deposit on Robinhood Chain, each signed by Hardhat's
//! first account and rebuilt here byte for byte.

use super::*;
use crate::utils::hex;

#[cfg(feature = "sign")]
fn from_args(args: &serde_json::Value) -> Eip1559 {
    let number = |at: usize| -> U256 { args[at].as_str().unwrap().parse().unwrap() };
    Eip1559 {
        chain_id: args[0].as_u64().unwrap(),
        nonce: args[1].as_u64().unwrap(),
        max_priority_fee: number(2),
        max_fee: number(3),
        gas: args[4].as_u64().unwrap(),
        to: Some(args[5].as_str().unwrap().parse().unwrap()),
        value: number(6),
        data: hex::decode(args[7].as_str().unwrap()).unwrap(),
        access_list: Vec::new(),
    }
}

#[cfg(feature = "sign")]
#[test]
fn every_transaction_signs_to_go_ethereum_s_bytes_and_hash() {
    use crate::account::keys::LocalWallet;
    use crate::test_support::{vector, vector_wallet_key};

    let wallet = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
    for name in ["deposit-tx-ethereum", "approve-tx-robinhood", "deposit-tx-robinhood"] {
        let case = vector(name);
        let tx = from_args(&case["args"]);
        assert_eq!(hex::encode_prefixed(&tx.data), case["calldata"].as_str().unwrap(), "{name}: calldata");
        let raw = tx.sign(&wallet).unwrap();
        assert_eq!(hex::encode_prefixed(&raw), case["tx"]["raw"].as_str().unwrap(), "{name}: raw");
        assert_eq!(tx_hash(&raw).to_hex(), case["tx"]["hash"].as_str().unwrap(), "{name}: hash");
        // The same bytes from a signature made elsewhere, and it recovers to the wallet.
        let signature = wallet.sign_digest(&tx.signing_hash().0).unwrap();
        assert_eq!(tx.signed(&signature).unwrap(), raw, "{name}: signed from a signature");
        assert_eq!(tx.signer(&signature).unwrap(), wallet.address(), "{name}: signer");
    }
}

#[test]
fn a_signature_without_a_recovery_bit_is_refused() {
    let tx = Eip1559 { chain_id: 1, gas: 21_000, ..Eip1559::default() };
    let odd = L1Signature::from_parts([1; 32], [1; 32], 29);
    assert_eq!(tx.signed(&odd), Err(WalletError::BadV(29)));
}

#[test]
fn a_call_becomes_the_transaction_its_fees_and_gas_bounded_by_integers() {
    let contract: Address = "0x3B4D794a66304F130a4Db8F2551B0070dfCf5ca7".parse().unwrap();
    let fees = Fees::over_base(U256::from(14_500_000_000u64), U256::from(1_000_000_000u64));
    assert_eq!(fees.max_fee, U256::from(30_000_000_000u64), "twice the base fee and the tip");
    let tx = Call::to(contract, vec![0x8a, 0x85, 0x70, 0x83]).transaction(1, 7, 120_000, fees);
    assert_eq!((tx.chain_id, tx.nonce, tx.gas, tx.to), (1, 7, 120_000, Some(contract)));
    assert_eq!((tx.max_priority_fee, tx.value, tx.access_list.len()), (U256::from(1_000_000_000u64), U256::ZERO, 0));
    assert_eq!(&tx.unsigned()[..1], &[EIP1559]);

    assert_eq!(Fees::over_base(U256::MAX, U256::ONE).max_fee, U256::MAX, "it saturates, never wraps");
    assert_eq!((with_headroom(92_308), with_headroom(u64::MAX)), (120_000, u64::MAX), "thirty percent over");
    let tx = Eip1559 { max_fee: U256::from(20_000_000u64), gas: 60_000, ..Eip1559::default() };
    assert_eq!(tx.gas_ceiling(), U256::from(1_200_000_000_000u64), "every unit of gas at the cap");
    let with_value = Eip1559 { value: U256::from(5u8), ..tx };
    assert_eq!(with_value.most_spent(), Some(U256::from(1_200_000_000_005u64)));
    assert_eq!(Eip1559 { value: U256::MAX, ..with_value }.most_spent(), None);
}
