//! A transaction signed elsewhere, read back: lighter-go's own output passes, a swapped field, a
//! missing signature or another hash does not.

use super::*;
use crate::test_support::vector_envelope;
use crate::trade::orders::CreateOrder;
use crate::trade::support::{case, order};

#[test]
fn what_lighter_go_signed_reads_back_as_asked() {
    let case = case("limit-buy-gtt");
    let (chain_id, envelope) = vector_envelope(&case);
    let tx = CreateOrder::new(order(&case["fields"]));
    let info = case["out"]["tx_info"].as_str().unwrap();
    let hash = case["out"]["tx_hash"].as_str().unwrap();
    assert_eq!(read_back(&tx, chain_id, &envelope, info, hash), Ok(()));

    let swapped = info.replace("\"Price\":834530", "\"Price\":834531");
    assert_eq!(read_back(&tx, chain_id, &envelope, &swapped, hash).unwrap_err().field, "Price");
    let unsigned = info.replace("\"Sig\":\"", "\"Sig\":\"AA");
    assert_eq!(read_back(&tx, chain_id, &envelope, &unsigned, hash).unwrap_err().field, "Sig");
    let other = format!("{}0", &hash[..hash.len() - 1]);
    assert_eq!(read_back(&tx, chain_id, &envelope, info, &other).unwrap_err().field, "tx_hash");
    let elsewhere = read_back(&tx, 466_324, &envelope, info, hash);
    assert_eq!(elsewhere.unwrap_err().field, "tx_hash", "the same fields on the other network");
    let later = Envelope { nonce: envelope.nonce + 1, ..envelope };
    assert_eq!(read_back(&tx, chain_id, &later, info, hash).unwrap_err().field, "Nonce");
    assert_eq!(read_back(&tx, chain_id, &envelope, "[]", hash).unwrap_err().field, "tx_info");
    let missing = info.replace("\"ReduceOnly\":0,", "");
    assert_eq!(read_back(&tx, chain_id, &envelope, &missing, hash).unwrap_err().field, "ReduceOnly");
}
