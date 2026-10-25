//! Orders in the shape the venue's docs give, every field read and every status told apart.

use super::*;

/// A resting take-profit-limit, as the account channels and reads write one.
const RESTING: &str = r#"{
    "order_index": 281476612587355, "client_order_index": 42, "order_id": "281476612587355",
    "client_order_id": "42", "market_index": 1, "market_symbol": "BTC", "market_kind": "perps",
    "owner_account_index": 100001, "initial_base_amount": "0.50000", "price": "90000.0", "nonce": 7,
    "remaining_base_amount": "0.20000", "is_ask": true, "base_size": 50000, "base_price": 900000,
    "filled_base_amount": "0.30000", "filled_quote_amount": "27000.000000", "side": "sell",
    "type": "take-profit-limit", "time_in_force": "good-till-time", "reduce_only": true,
    "trigger_price": "89000.0", "order_expiry": 1792918063399, "status": "open", "trigger_status": "na",
    "trigger_time": 0, "parent_order_index": 0, "parent_order_id": "", "to_trigger_order_id_0": "",
    "to_trigger_order_id_1": "", "to_cancel_order_id_0": "", "integrator_fee_collector_index": "",
    "integrator_taker_fee": "", "integrator_maker_fee": "", "order_flags": 0, "block_height": 198321831,
    "timestamp": 1773854156, "created_at": 1773854156654, "updated_at": 1773854156700,
    "transaction_time": 1773854156686065, "order_version": 0
}"#;

#[test]
fn an_order_reads_as_the_venue_writes_it() {
    let order: Order = serde_json::from_str(RESTING).unwrap();
    assert_eq!((order.market_index.get(), order.owner_account_index.get()), (1, 100_001));
    assert_eq!(
        (order.kind, order.time_in_force, order.side()),
        (OrderKind::TakeProfitLimit, Some(TimeInForce::GoodTillTime), Side::Ask)
    );
    assert_eq!((order.base_size, order.base_price, order.nonce), (50_000, 900_000, 7));
    assert_eq!((order.price.to_string(), order.trigger_price.to_string()), ("90000".to_owned(), "89000".to_owned()));
    assert_eq!(order.filled().to_string(), "0.3");
    assert_eq!((order.status, order.trigger_status), (OrderStatus::Open, TriggerStatus::Na));
    assert_eq!((order.timestamp, order.transaction_time), (1_773_854_156, 1_773_854_156_686_065));
    assert_eq!(order.index().map(OrderIndex::get), Some(281_476_612_587_355));
    assert_eq!(order.client_index().map(ClientOrderIndex::get), Some(42));
    assert!(matches!(order.reference(), Some(OrderRef::Venue(_))));
    assert!(order.reduce_only);
}

#[test]
fn an_order_with_unknown_words_and_empty_fields_still_reads() {
    let text = RESTING
        .replace(r#""time_in_force": "good-till-time""#, r#""time_in_force": "Unknown""#)
        .replace(r#""status": "open""#, r#""status": "canceled-something-new""#)
        .replace(r#""trigger_price": "89000.0""#, r#""trigger_price": """#)
        .replace(r#""order_expiry": 1792918063399"#, r#""order_expiry": -1"#)
        .replace(r#""client_order_index": 42"#, r#""client_order_index": 0"#)
        .replace(r#""filled_base_amount": "0.30000", "#, "");
    let order: Order = serde_json::from_str(&text).unwrap();
    assert_eq!((order.time_in_force, order.status), (None, OrderStatus::Other));
    assert_eq!((order.trigger_price, order.order_expiry, order.client_index()), (Dec::ZERO, 0, None));
    assert_eq!(order.filled().to_string(), "0.3", "from what remains");
}

#[test]
fn live_orders_are_in_progress_pending_or_open_and_the_rest_ended() {
    for (status, live, canceled) in [
        (OrderStatus::InProgress, true, false),
        (OrderStatus::Pending, true, false),
        (OrderStatus::Open, true, false),
        (OrderStatus::Filled, false, false),
        (OrderStatus::Canceled, false, true),
        (OrderStatus::CanceledSelfTrade, false, true),
        (OrderStatus::CanceledInvalidBalance, false, true),
        (OrderStatus::Other, false, false),
    ] {
        assert_eq!((status.is_live(), status.is_canceled()), (live, canceled), "{status:?}");
    }
    let read: OrderStatus = serde_json::from_str(r#""canceled-too-much-slippage""#).unwrap();
    assert_eq!(read, OrderStatus::CanceledTooMuchSlippage);
}
