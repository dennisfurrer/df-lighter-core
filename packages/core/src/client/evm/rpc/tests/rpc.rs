//! Requests as a node takes them, answers as Ethereum's and Robinhood Chain's public nodes gave
//! them (`fixtures/*/eth_*.json`), refusals by their code, and [`Rpc`] over a wire that answers
//! from a queue.

use super::*;
use crate::client::evm::tx::Fees;
use crate::client::evm::u256::B256;
use crate::client::{Network, Response, TransportError};
use crate::test_support::{Wire, block_on, ok, status};

fn owner() -> Address {
    "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266".parse().unwrap()
}

fn result(value: &str) -> Result<Response, TransportError> {
    ok(&format!(r#"{{"jsonrpc":"2.0","id":1,"result":{value}}}"#))
}

#[test]
fn a_query_writes_its_request_as_a_node_takes_it() {
    let contract = Network::Lighter.contract();
    let call = CallRequest {
        from: Some(owner()),
        value: Some(U256::from(5u8)),
        gas: Some(21_000),
        ..CallRequest::to(contract, vec![0xab])
    };
    assert_eq!(
        eth::call(&call, BlockTag::Latest).body(),
        r#"{"jsonrpc":"2.0","id":1,"method":"eth_call","params":[{"from":"0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266","to":"0x3b4d794a66304f130a4db8f2551b0070dfcf5ca7","gas":"0x5208","value":"0x5","data":"0xab"},"latest"]}"#
    );
    assert_eq!(
        eth::transaction_count(owner(), BlockTag::Pending).body(),
        r#"{"jsonrpc":"2.0","id":1,"method":"eth_getTransactionCount","params":["0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266","pending"]}"#
    );
    assert_eq!(
        eth::fee_history(5, BlockTag::Number(255), &[50]).body(),
        r#"{"jsonrpc":"2.0","id":1,"method":"eth_feeHistory","params":["0x5","0xff",[50]]}"#
    );
    assert!(eth::send_raw_transaction(&[0x02, 0xf8]).body().contains(r#""params":["0x02f8"]"#));
    for tag in [BlockTag::Safe, BlockTag::Finalized] {
        assert!(eth::balance(owner(), tag).body().contains(&format!("\"{tag}\"")));
    }
    assert_eq!(eth::gas_price().method(), "eth_gasPrice");
    let empty = CallRequest::of(None, &crate::client::evm::tx::Call::to(owner(), Vec::new()));
    assert_eq!(
        eth::estimate_gas(&empty).body(),
        r#"{"jsonrpc":"2.0","id":1,"method":"eth_estimateGas","params":[{"to":"0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"}]}"#
    );
}

#[test]
fn each_l1_answers_its_own_chain_id() {
    let lighter = include_str!("../../../../../fixtures/lighter/eth_chainId.json");
    let robinhood = include_str!("../../../../../fixtures/robinhood/eth_chainId.json");
    assert_eq!(eth::chain_id().answer(lighter), Ok(Network::Lighter.l1_chain_id()));
    assert_eq!(eth::chain_id().answer(robinhood), Ok(Network::Robinhood.l1_chain_id()));
}

#[test]
fn a_node_error_says_its_kind_and_a_revert_its_reason() {
    let node = |code: i64, message: &str| NodeError { code, message: message.to_owned(), data: None };
    for (code, message, kind) in [
        (-32700, "parse error", ErrorKind::Parse),
        (-32600, "invalid request", ErrorKind::InvalidRequest),
        (-32601, "Method not found", ErrorKind::MethodNotFound),
        (-32602, "invalid params", ErrorKind::InvalidParams),
        (-32603, "internal error", ErrorKind::Internal),
        (3, "execution reverted", ErrorKind::Reverted),
        (-32000, "execution reverted: no", ErrorKind::Reverted),
        (-32000, "insufficient funds for gas * price + value", ErrorKind::Server),
        (-32100, "out of range", ErrorKind::Other),
    ] {
        assert_eq!(node(code, message).kind(), kind, "{code} {message}");
    }
    assert_eq!(node(3, "execution reverted").to_string(), "execution reverted (code 3)");
    let with_data = NodeError { data: Some("0xdead".to_owned()), ..node(3, "execution reverted") };
    assert_eq!(with_data.to_string(), "execution reverted (code 3): 0xdead");
    // Panic(0x11), an overflow.
    let panic = "0x4e487b710000000000000000000000000000000000000000000000000000000000000011";
    let answer =
        format!(r#"{{"jsonrpc":"2.0","id":1,"error":{{"code":3,"message":"execution reverted","data":"{panic}"}}}}"#);
    let RpcError::Node { error, .. } =
        eth::call(&CallRequest::default(), BlockTag::Latest).answer(&answer).unwrap_err()
    else {
        panic!()
    };
    assert_eq!(error.revert(), Some(Revert::Panic(U256::from(0x11u8))));
    let short = [0x08, 0xc3, 0x79, 0xa0, 0x00];
    for data in [&short[..], &[0xde, 0xad, 0xbe, 0xef, 0x01], &[]] {
        assert_eq!(read_revert(data), Revert::Custom(data.to_vec()));
    }
    assert!(matches!(eth::chain_id().answer("<html>"), Err(RpcError::Unreadable { method: "eth_chainId", .. })));
    assert!(matches!(eth::chain_id().answer(r#"{"jsonrpc":"2.0","id":1}"#), Err(RpcError::Unreadable { .. })));
    assert!(matches!(
        eth::chain_id().answer(r#"{"jsonrpc":"2.0","id":1,"result":3}"#),
        Err(RpcError::Unreadable { .. })
    ));
    assert!(RpcError::Unreadable { method: "eth_sendRawTransaction", detail: String::new() }.may_have_landed());
    assert!(RpcError::Transport(TransportError::Unreachable("down".to_owned())).may_have_landed());
    assert!(
        !RpcError::Node { method: "eth_sendRawTransaction", error: node(-32000, "nonce too low") }.may_have_landed()
    );
}

#[test]
fn the_rpc_posts_json_to_its_node_and_reads_what_comes_back() {
    let rpc = Rpc::l1(
        Wire::answering([
            ok(include_str!("../../../../../fixtures/robinhood/eth_chainId.json")),
            status(429, "slow down"),
            status(502, "bad gateway"),
            status(400, r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"invalid params"}}"#),
        ]),
        Network::Robinhood,
    );
    assert_eq!(rpc.url(), "https://rpc.mainnet.chain.robinhood.com");
    assert_eq!(block_on(rpc.chain_id()), Ok(4663));
    assert!(matches!(block_on(rpc.gas_price()), Err(RpcError::Transport(TransportError::RateLimited { .. }))));
    assert!(matches!(block_on(rpc.gas_price()), Err(RpcError::Transport(TransportError::Status { status: 502, .. }))));
    let Err(RpcError::Node { error, .. }) = block_on(rpc.gas_price()) else { panic!() };
    assert_eq!(error.kind(), ErrorKind::InvalidParams, "a refusal reads whatever the status");
    let sent = rpc.http().sent();
    assert_eq!(sent[0].url, "https://rpc.mainnet.chain.robinhood.com");
    assert_eq!(sent[0].header("content-type"), Some("application/json"));
    assert_eq!(
        String::from_utf8_lossy(&sent[0].body),
        r#"{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}"#
    );
}

#[test]
fn a_transaction_is_prepared_from_the_node_s_nonce_fees_and_estimate() {
    let rpc = Rpc::new(
        Wire::answering([
            result(r#""0x7""#),
            result(r#"{"oldestBlock":"0x10","baseFeePerGas":["0x3","0x360447100"],"gasUsedRatio":[0.5],"reward":[]}"#),
            result(r#""0x3b9aca00""#),
            result(r#""0x16894""#),
            result(r#""0x1""#),
        ]),
        "https://node.example",
    );
    let call = crate::client::evm::tx::Call::to(Network::Lighter.contract(), vec![0x8a]);
    let tx = block_on(rpc.prepare(1, owner(), call)).unwrap();
    assert_eq!((tx.chain_id, tx.nonce, tx.gas), (1, 7, 120_000), "92308 gas and thirty percent");
    let fees = Fees::over_base(U256::from(14_500_000_000u64), U256::from(1_000_000_000u64));
    assert_eq!((tx.max_priority_fee, tx.max_fee), (fees.max_priority_fee, fees.max_fee));
    let methods: Vec<String> = rpc
        .http()
        .bodies()
        .into_iter()
        .map(|(_, body)| {
            serde_json::from_str::<serde_json::Value>(&body).unwrap()["method"].as_str().unwrap().to_owned()
        })
        .collect();
    assert_eq!(methods, ["eth_getTransactionCount", "eth_feeHistory", "eth_maxPriorityFeePerGas", "eth_estimateGas"]);
    assert_eq!(block_on(rpc.check_chain(4663)), Err(L1Error::WrongChain { expected: 4663, found: 1 }));
}

#[test]
fn a_receipt_is_waited_for_with_the_caller_s_pause() {
    let hash = B256([0x11; 32]);
    let receipt = format!(
        r#"{{"transactionHash":"{hash}","blockNumber":"0x10","from":"0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266","to":"0x3b4d794a66304f130a4db8f2551b0070dfcf5ca7","gasUsed":"0x5208","cumulativeGasUsed":"0x5208","effectiveGasPrice":"0x3b9aca00","status":"0x1","logs":[],"type":"0x2"}}"#
    );
    let rpc = Rpc::new(Wire::answering([result("null"), result("null"), result(&receipt)]), "https://node.example");
    let paused = std::cell::RefCell::new(Vec::new());
    let mined = block_on(rpc.wait_for_receipt(hash, async |ms| paused.borrow_mut().push(ms))).unwrap().unwrap();
    assert!(mined.succeeded());
    assert_eq!((mined.gas_used, mined.kind), (21_000, Some(2)));
    assert_eq!(*paused.borrow(), [RECEIPT_POLL_MS, RECEIPT_POLL_MS], "a pause between reads, none before the first");
    let never = Rpc::new(Wire::always(r#"{"jsonrpc":"2.0","id":1,"result":null}"#), "https://node.example");
    assert_eq!(block_on(never.wait_for_receipt(hash, async |_| {})), Ok(None));
    assert_eq!(never.http().sent().len(), usize::try_from(RECEIPT_TRIES).unwrap());
}
