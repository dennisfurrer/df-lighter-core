//! The account's order reads. Each needs a token (the venue refuses them without one), so the
//! orders here are shaped from the docs' `Order` schema, not recorded; what is checked is what goes
//! out and what an order implies.

use super::*;
use crate::account::history::orders::OrderStatus;
use crate::test_support::{Recorded, block_on};

/// A documented order; `kind`, `status` and prices filled in per test.
fn order(kind: &str, status: &str, price: &str, trigger: &str) -> String {
    format!(
        r#"{{"order_index":281476612587355,"client_order_index":42,"order_id":"281476612587355",
        "market_index":1,"owner_account_index":100001,"initial_base_amount":"0.50000","price":"{price}",
        "nonce":7,"remaining_base_amount":"0.20000","is_ask":true,"base_size":50000,"base_price":900000,
        "filled_base_amount":"0.30000","filled_quote_amount":"27000.000000","type":"{kind}",
        "time_in_force":"good-till-time","reduce_only":true,"trigger_price":"{trigger}","order_expiry":1792918063399,
        "status":"{status}","trigger_status":"na","trigger_time":0,"parent_order_index":0,"block_height":198321831,
        "timestamp":1773854156,"created_at":{created},"updated_at":1773854156700,"transaction_time":1773854156686065}}"#,
        created = if kind == "limit" { 0 } else { 1_773_854_156_654_u64 },
    )
}

fn page(orders: &[String], cursor: &str) -> String {
    format!(r#"{{"code":200,"orders":[{}],"next_cursor":"{cursor}"}}"#, orders.join(","))
}

fn account() -> AccountIndex {
    AccountIndex::new(100_001).unwrap()
}

#[test]
fn working_orders_are_asked_with_the_token_by_market_and_kind() {
    let transport = Recorded::answering(&page(&[order("limit", "open", "90000.0", "0")], ""));
    let orders = block_on(active_orders(&transport, account(), None, &[MarketKind::Perps, MarketKind::Spot])).unwrap();
    assert_eq!(orders.len(), 1);
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("accountActiveOrders", true));
    assert_eq!(call.fields, [("account_index", "100001".to_owned()), ("market_type", "perp,spot".to_owned())]);
    let one_market = active_orders_call(account(), Some(MarketId::new(1).unwrap()), &[]);
    assert_eq!(one_market.fields, [("account_index", "100001".to_owned()), ("market_id", "1".to_owned())]);
}

#[test]
fn ended_orders_come_a_page_at_a_time_with_the_cursor_carried() {
    let transport = Recorded::answering(&page(&[order("limit", "filled", "90000.0", "0")], "next-page"));
    let query = InactiveQuery { side: Some(Side::Ask), limit: 250, ..InactiveQuery::new(account()) };
    let first = block_on(inactive_orders(&transport, &query)).unwrap();
    assert_eq!((first.orders[0].status, first.next()), (OrderStatus::Filled, Some("next-page")));
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("accountInactiveOrders", true));
    assert_eq!(
        call.fields,
        [("account_index", "100001".to_owned()), ("ask_filter", "1".to_owned()), ("limit", "100".to_owned())]
    );
    let next = query.after(&first).unwrap();
    assert_eq!(inactive_orders_call(&next).value("cursor"), Some("next-page"));
    let last = OrderPage { orders: Vec::new(), next_cursor: Some(String::new()) };
    assert_eq!(query.after(&last), None, "an empty cursor ends it");
}

#[test]
fn orders_by_client_index_go_twenty_to_a_request() {
    let transport = Recorded::answering(&page(&[order("limit", "open", "90000.0", "0")], ""));
    let indexes: Vec<ClientOrderIndex> = (1..=25).map(|index| ClientOrderIndex::new(index).unwrap()).collect();
    let orders = block_on(orders_by_client_index(&transport, None, &indexes)).unwrap();
    assert_eq!(orders.len(), 2, "one answer a request");
    let sent = transport.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].value("client_order_indexes").unwrap().split(',').count(), MOST_CLIENT_INDEXES);
    assert_eq!(sent[1].fields, [("client_order_indexes", "21,22,23,24,25".to_owned())]);
    assert!(sent.iter().all(|call| call.path == "accountOrders" && call.auth));
    let empty = Recorded::answering("{}");
    assert_eq!(block_on(orders_by_client_index(&empty, Some(account()), &[])), Ok(Vec::new()));
    assert!(empty.sent().is_empty(), "nothing asked for nothing");
}

#[test]
fn an_order_shows_its_resting_price_its_trigger_and_when_it_was_placed() {
    let read = |text: String| serde_json::from_str::<Order>(&text).unwrap();
    let limit = read(order("limit", "open", "90000.0", "0"));
    assert_eq!((limit.limit_price().map(|price| price.to_string()), limit.trigger()), (Some("90000".to_owned()), None));
    assert_eq!(limit.placed_ms(), 1_773_854_156_000, "no created_at: the second it was sequenced");
    assert!(!limit.is_trigger());

    let stop = read(order("stop-loss", "pending", "78400.0", "80000.0"));
    assert_eq!(stop.limit_price(), None, "a market trigger's price is only a bound");
    assert_eq!(stop.trigger().map(|price| price.to_string()).as_deref(), Some("80000"));
    assert_eq!((stop.placed_ms(), stop.remaining().to_string()), (1_773_854_156_654, "0.2".to_owned()));

    let take_profit_limit = read(order("take-profit-limit", "pending", "89000.0", "89500.0"));
    assert_eq!(take_profit_limit.limit_price().map(|price| price.to_string()).as_deref(), Some("89000"));
    assert!(take_profit_limit.trigger().is_some() && take_profit_limit.status.is_live());
    assert_eq!(kinds_filter(&[]), None);
}
