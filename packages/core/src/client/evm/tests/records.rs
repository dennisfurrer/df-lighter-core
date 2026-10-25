//! Receipts and fee history as an Ethereum node writes them: quantities exact, a reverted
//! transaction told apart from a successful one, the gas used ratio kept as written.

use super::*;

const RECEIPT: &str = r#"{
  "blockHash":"0x3c1d3f5a4f0c0f7e1f0c1d3f5a4f0c0f7e1f0c1d3f5a4f0c0f7e1f0c1d3f5a4f",
  "blockNumber":"0x1626b3a",
  "contractAddress":null,
  "cumulativeGasUsed":"0xa7c1d2",
  "effectiveGasPrice":"0x1dcd6500",
  "from":"0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266",
  "gasUsed":"0x13e5d",
  "logs":[{"address":"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48","topics":["0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"],"data":"0x0000000000000000000000000000000000000000000000000000000001851960","logIndex":"0x5","removed":false}],
  "logsBloom":"0x00",
  "status":"0x1",
  "to":"0x3b4d794a66304f130a4db8f2551b0070dfcf5ca7",
  "transactionHash":"0x6936ceea7f4827630678f71e669a592b2965d06411e35d7df26880fd22abb098",
  "transactionIndex":"0x2a",
  "type":"0x2"
}"#;

#[test]
fn a_receipt_reads_with_its_status_gas_and_logs() {
    let receipt: Receipt = serde_json::from_str(RECEIPT).unwrap();
    assert!(receipt.succeeded());
    assert_eq!(
        (receipt.block_number, receipt.gas_used, receipt.transaction_index),
        (Some(23_227_194), 81_501, Some(42))
    );
    assert_eq!(receipt.effective_gas_price, Some(U256::from(500_000_000u64)));
    assert_eq!(receipt.logs[0].data.len(), 32);
    assert_eq!((receipt.logs[0].log_index, receipt.logs[0].removed), (Some(5), false));
    let reverted: Receipt = serde_json::from_str(&RECEIPT.replace(r#""status":"0x1""#, r#""status":"0x0""#)).unwrap();
    assert!(!reverted.succeeded());
    assert!(serde_json::from_str::<Receipt>(&RECEIPT.replace(r#""status":"0x1""#, r#""status":"0x2""#)).is_err());
}

#[test]
fn fee_history_gives_the_next_block_s_base_fee() {
    let history: FeeHistory = serde_json::from_str(
        r#"{"oldestBlock":"0x1626b3a","baseFeePerGas":["0x1dcd6500","0x1e8ea1ab"],"gasUsedRatio":[0.5212964666666667],"reward":[["0x5f5e100"]]}"#,
    )
    .unwrap();
    assert_eq!(history.next_base_fee(), Some(U256::from(512_663_979u64)));
    assert_eq!(history.gas_used_ratio[0].as_str(), "0.5212964666666667");
    assert_eq!(history.reward, [[U256::from(100_000_000u64)]]);
    let empty: FeeHistory = serde_json::from_str(r#"{"oldestBlock":"0x0","baseFeePerGas":[]}"#).unwrap();
    assert_eq!(empty.next_base_fee(), None);
}
