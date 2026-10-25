//! Deposits: the calldata, the plan read from the L1, and the whole journey against a node that
//! answers from a queue. The transactions it signs are go-ethereum's byte for byte
//! (`vectors/lighter_go.json`: `approve-tx-robinhood`, `deposit-tx-robinhood`,
//! `deposit-tx-ethereum`), given the node's nonce, fees and gas those vectors were made with.

use super::*;
use crate::client::Response;
use crate::client::TransportError;
use crate::test_support::{Wire, block_on, ok};
use crate::utils::hex;

fn owner() -> Address {
    "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".parse().unwrap()
}

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn result(value: &str) -> Result<Response, TransportError> {
    ok(&format!(r#"{{"jsonrpc":"2.0","id":1,"result":{value}}}"#))
}

fn word(value: u64) -> Result<Response, TransportError> {
    result(&format!("\"{}\"", hex::encode_prefixed(&U256::from(value).to_be_bytes())))
}

#[test]
fn a_deposit_is_the_contract_s_call_never_a_token_transfer() {
    let call = deposit_call(Network::Lighter, owner(), dec("25.5")).unwrap();
    assert_eq!(call.to, Some(Network::Lighter.contract()));
    assert_eq!(call.value, U256::ZERO);
    assert_eq!(
        hex::encode_prefixed(&call.data),
        "0x8a857083000000000000000000000000f39fd6e51aad88f6f4ce6ab8827279cfffb92266000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001851960"
    );
    let approve = approve_call(Network::Robinhood, U256::from(25_500_000u64));
    assert_eq!(approve.to, Some(Network::Robinhood.collateral().token));
    assert_eq!(
        hex::encode_prefixed(&approve.data),
        "0x095ea7b300000000000000000000000094bab9693ba2f6358507effcbd372b0660afff9d0000000000000000000000000000000000000000000000000000000001851960"
    );
    assert_eq!(&approve_max_call(Network::Lighter).data[36..], &[0xff; 32]);
    // ETH goes as the value; an ERC-20 does not.
    let eth = asset_deposit_call(Network::Lighter, owner(), AssetId::ETH, Route::Spot, None, 18, dec("0.05")).unwrap();
    assert_eq!(eth.value, U256::from(50_000_000_000_000_000u64));
    let link: Address = "0x514910771AF9Ca656af840dff83E8264EcF986CA".parse().unwrap();
    let token =
        asset_deposit_call(Network::Lighter, owner(), AssetId::of::<5>(), Route::Spot, Some(link), 18, dec("1"))
            .unwrap();
    assert_eq!(token.value, U256::ZERO);
    for (amount, refused) in [
        ("0.5", BadDeposit::BelowMinimum { amount: dec("0.5"), minimum: Dec::ONE, symbol: "USDC" }),
        ("1.0000001", BadDeposit::TooFine { value: dec("1.0000001"), decimals: 6 }),
        ("-1", BadDeposit::NotPositive(dec("-1"))),
        ("0", BadDeposit::NotPositive(Dec::ZERO)),
    ] {
        assert_eq!(deposit_call(Network::Lighter, owner(), dec(amount)), Err(refused), "{amount}");
    }
}

#[test]
fn the_plan_reads_the_chain_the_balance_and_the_allowance() {
    let rpc = Rpc::l1(Wire::answering([result(r#""0x1237""#), word(100_000_000), word(0)]), Network::Robinhood);
    let plan = block_on(plan_deposit(&rpc, Network::Robinhood, owner(), owner(), dec("25.5"))).unwrap();
    assert_eq!(
        (plan.chain_id(), plan.amount, plan.balance, plan.allowance),
        (4663, U256::from(25_500_000u64), U256::from(100_000_000u64), U256::ZERO)
    );
    let calls = plan.calls();
    assert_eq!(calls.len(), 2, "approve, then deposit");
    assert_eq!(calls[0], approve_call(Network::Robinhood, U256::from(25_500_000u64)));
    assert_eq!(calls[1], deposit_call(Network::Robinhood, owner(), dec("25.5")).unwrap());
    let bodies = rpc.http().bodies();
    assert!(
        bodies[1].1.contains(r#""to":"0x5fc5360d0400a0fd4f2af552add042d716f1d168","data":"0x70a08231"#),
        "balanceOf on USDG"
    );
    assert!(bodies[2].1.contains(r#""data":"0xdd62ed3e"#), "allowance");

    let enough = Rpc::l1(
        Wire::answering([result(r#""0x1""#), word(25_500_000), result(&format!("\"0x{}\"", "f".repeat(64)))]),
        Network::Lighter,
    );
    let plan = block_on(plan_deposit(&enough, Network::Lighter, owner(), owner(), dec("25.5"))).unwrap();
    assert_eq!(plan.calls().len(), 1, "an allowance that covers it needs no approval");

    let wrong = Rpc::l1(Wire::answering([result(r#""0x1""#)]), Network::Robinhood);
    let refused = block_on(plan_deposit(&wrong, Network::Robinhood, owner(), owner(), dec("25.5")));
    assert_eq!(refused, Err(DepositError::WrongChain { expected: 4663, found: 1 }));
    let short = Rpc::l1(Wire::answering([result(r#""0x1""#), word(25_000_000)]), Network::Lighter);
    let refused = block_on(plan_deposit(&short, Network::Lighter, owner(), owner(), dec("25.5")));
    assert_eq!(refused, Err(DepositError::Short { have: dec("25"), need: dec("25.5"), symbol: "USDC" }));
    assert_eq!(short.http().sent().len(), 2, "nothing more is asked");

    assert_eq!(collateral_amount(Network::Lighter, &U256::from(25_500_000u64).to_be_bytes()), Ok(dec("25.5")));
    let ten = include_str!("../../../fixtures/lighter/eth_call_addressToAccountIndex.json");
    let answer = account_index_request(Network::Lighter, owner()).answer(ten).unwrap();
    assert_eq!(zklighter::read_account_index(&answer).unwrap().map(|index| index.get()), Some(10));
}

#[cfg(feature = "sign")]
mod journey {
    use super::*;
    use crate::test_support::{vector, vector_wallet_key};

    fn receipt(hash: &str, status: u8) -> Result<Response, TransportError> {
        result(&format!(
            r#"{{"transactionHash":"{hash}","blockNumber":"0x10","from":"0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266","to":null,"gasUsed":"0x13e5d","cumulativeGasUsed":"0x13e5d","status":"0x{status}","logs":[],"type":"0x2"}}"#
        ))
    }

    /// What a node answers to send one transaction: its nonce, the fees and gas the vectors were
    /// made with (a tip of 1 gwei, a cap of 30 gwei, 120,000 gas), and its hash.
    fn sending(nonce: u64, hash: &str) -> Vec<Result<Response, TransportError>> {
        vec![
            result(&format!("\"{nonce:#x}\"")),
            result(r#"{"oldestBlock":"0x10","baseFeePerGas":["0x1","0x360447100"],"gasUsedRatio":[0.5]}"#),
            result(r#""0x3b9aca00""#),
            result(r#""0x16894""#),
            result(&format!("\"{hash}\"")),
        ]
    }

    fn raw_sent(wire: &Wire) -> Vec<String> {
        wire.bodies()
            .into_iter()
            .filter_map(|(_, body)| {
                let value: serde_json::Value = serde_json::from_str(&body).unwrap();
                (value["method"] == "eth_sendRawTransaction").then(|| value["params"][0].as_str().unwrap().to_owned())
            })
            .collect()
    }

    #[test]
    fn a_short_allowance_is_approved_mined_and_then_the_deposit_is_made() {
        let approve = vector("approve-tx-robinhood");
        let deposit_tx = vector("deposit-tx-robinhood");
        let approve_hash = approve["tx"]["hash"].as_str().unwrap();
        let deposit_hash = deposit_tx["tx"]["hash"].as_str().unwrap();
        let mut answers = vec![result(r#""0x1237""#), word(100_000_000), word(0)];
        answers.extend(sending(0, approve_hash));
        answers.extend([result("null"), receipt(approve_hash, 1)]);
        answers.extend(sending(1, deposit_hash));
        answers.push(receipt(deposit_hash, 1));
        let rpc = Rpc::l1(Wire::answering(answers), Network::Robinhood);
        let wallet = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
        let paused = std::cell::Cell::new(0);
        let done =
            block_on(deposit(&rpc, &wallet, Network::Robinhood, dec("25.5"), async |_| paused.set(paused.get() + 1)))
                .unwrap();
        assert_eq!(done.approval.map(|receipt| receipt.transaction_hash.to_hex()).as_deref(), Some(approve_hash));
        assert_eq!(done.deposit.transaction_hash.to_hex(), deposit_hash);
        assert_eq!(paused.get(), 1, "one pause while the approval was not yet mined");
        let raws = raw_sent(rpc.http());
        assert_eq!(raws, [approve["tx"]["raw"].as_str().unwrap(), deposit_tx["tx"]["raw"].as_str().unwrap()]);
    }

    #[test]
    fn with_the_allowance_in_place_only_the_deposit_is_sent() {
        let deposit_tx = vector("deposit-tx-ethereum");
        let hash = deposit_tx["tx"]["hash"].as_str().unwrap();
        let mut answers = vec![result(r#""0x1""#), word(30_000_000), word(25_500_000)];
        answers.extend(sending(7, hash));
        answers.push(receipt(hash, 1));
        let rpc = Rpc::l1(Wire::answering(answers), Network::Lighter);
        let wallet = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
        let done = block_on(deposit(&rpc, &wallet, Network::Lighter, dec("25.5"), async |_| {})).unwrap();
        assert_eq!(done.approval, None);
        assert_eq!(raw_sent(rpc.http()), [deposit_tx["tx"]["raw"].as_str().unwrap()]);
    }

    #[test]
    fn a_revert_or_an_answer_that_never_came_is_told_apart_and_nothing_is_sent_again() {
        let deposit_tx = vector("deposit-tx-ethereum");
        let hash = deposit_tx["tx"]["hash"].as_str().unwrap();
        let wallet = LocalWallet::from_hex(&vector_wallet_key()).unwrap();
        let mut answers = vec![result(r#""0x1""#), word(30_000_000), word(25_500_000)];
        answers.extend(sending(7, hash));
        answers.push(receipt(hash, 0));
        let rpc = Rpc::l1(Wire::answering(answers), Network::Lighter);
        let reverted = block_on(deposit(&rpc, &wallet, Network::Lighter, dec("25.5"), async |_| {}));
        assert_eq!(reverted, Err(DepositError::Reverted(hash.parse().unwrap())));

        let mut answers = vec![result(r#""0x1""#), word(30_000_000), word(25_500_000)];
        let mut send = sending(7, hash);
        send.pop();
        answers.extend(send);
        let rpc = Rpc::l1(Wire::answering(answers), Network::Lighter);
        let lost = block_on(deposit(&rpc, &wallet, Network::Lighter, dec("25.5"), async |_| {}));
        assert_eq!(lost, Err(DepositError::Unconfirmed(hash.parse().unwrap())), "no answer: it may have landed");
        assert_eq!(raw_sent(rpc.http()).len(), 1, "sent once");
    }
}
