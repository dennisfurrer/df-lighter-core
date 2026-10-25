//! `CreateOrder`, held to lighter-go: every placeable kind, the attributes, both networks, the
//! widest integers, and what it refuses.

use super::*;
use crate::trade::support::{attributes, case, cases, order, signed, signs_as_lighter_go};
use crate::utils::BadId;

fn order_of(name: &str) -> Order {
    order(&case(name)["fields"])
}

#[test]
fn every_order_lighter_go_signs_signs_the_same_here() {
    let mut names = Vec::new();
    for case in cases("CreateOrder").iter().filter(|case| signed(case)) {
        let tx = CreateOrder::new(order(&case["fields"])).with(attributes(case));
        signs_as_lighter_go(&tx, case);
        names.push(case["name"].as_str().unwrap().to_owned());
    }
    assert_eq!(
        names,
        [
            "limit-buy-gtt",
            "limit-sell-post-only",
            "limit-ioc",
            "market-buy",
            "market-sell-reduce-only",
            "stop-loss",
            "stop-loss-limit",
            "take-profit",
            "take-profit-limit",
            "twap",
            "no-client-index",
            "spot-market-2048",
            "binary-option-4206",
            "widest-integers",
            "integrator-fees",
            "integrator-tracking-only",
            "self-trade-modes",
            "skip-nonce",
            "robinhood-limit",
            "sub-account-market",
        ]
    );
}

#[test]
fn every_order_lighter_go_refuses_is_refused_before_signing() {
    let refused: Vec<String> = cases("CreateOrder")
        .iter()
        .filter(|case| !signed(case))
        .map(|case| case["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        refused,
        ["refused-market-255", "refused-zero-size", "refused-market-gtt", "refused-fees-and-self-trade"]
    );

    // 255 names no market: no order can even be made for it.
    let fields = &case("refused-market-255")["fields"];
    assert_eq!(MarketId::new(fields["MarketIndex"].as_u64().unwrap()), Err(BadId::Market(255)));

    let zero = CreateOrder::new(order_of("refused-zero-size"));
    assert_eq!(zero.check().unwrap_err().field, "BaseAmount");
    let market_gtt = CreateOrder::new(order_of("refused-market-gtt"));
    assert_eq!(market_gtt.check().unwrap_err().field, "TimeInForce");
    let both = case("refused-fees-and-self-trade");
    let fees_and_self_trade = CreateOrder::new(order(&both["fields"])).with(attributes(&both));
    assert_eq!(fees_and_self_trade.check().unwrap_err().field, "L2TxAttributes");
}

#[test]
fn each_kind_holds_its_time_in_force_trigger_and_expiry_to_lighter_go_s_rules() {
    let refuses = |order: Order, field: &str| {
        let error = order.check().unwrap_err();
        assert_eq!(error.field, field, "{order:?}: {error}");
    };
    let market = order_of("market-buy");
    order_of("market-buy").check().unwrap();
    refuses(Order { expiry: 1, ..market }, "OrderExpiry");
    refuses(Order { trigger_price: 1, ..market }, "TriggerPrice");
    refuses(Order { time_in_force: TimeInForce::PostOnly, ..market }, "TimeInForce");

    let resting = order_of("limit-buy-gtt");
    refuses(Order { expiry: 0, ..resting }, "OrderExpiry");
    refuses(Order { trigger_price: 5, ..resting }, "TriggerPrice");
    let ioc = order_of("limit-ioc");
    refuses(Order { expiry: 1_792_419_200_000, ..ioc }, "OrderExpiry");

    let stop = order_of("stop-loss");
    refuses(Order { time_in_force: TimeInForce::GoodTillTime, ..stop }, "TimeInForce");
    refuses(Order { trigger_price: 0, ..stop }, "TriggerPrice");
    refuses(Order { expiry: 0, ..stop }, "OrderExpiry");
    let stop_limit = order_of("stop-loss-limit");
    Order { time_in_force: TimeInForce::PostOnly, ..stop_limit }.check().unwrap();
    refuses(Order { trigger_price: 0, ..stop_limit }, "TriggerPrice");
    refuses(Order { expiry: 0, ..stop_limit }, "OrderExpiry");

    let twap = order_of("twap");
    refuses(Order { time_in_force: TimeInForce::ImmediateOrCancel, ..twap }, "TimeInForce");
    refuses(Order { trigger_price: 1, ..twap }, "TriggerPrice");
    refuses(Order { expiry: 0, ..twap }, "OrderExpiry");

    refuses(Order { kind: OrderKind::TwapSub, ..twap }, "Type");
    refuses(Order { kind: OrderKind::Liquidation, ..resting }, "Type");
}

#[test]
fn a_size_is_needed_unless_the_order_only_reduces_and_then_covers_the_position() {
    let market = order_of("market-sell-reduce-only");
    Order { base_amount: 0, ..market }.check().unwrap();
    let opening = Order { reduce_only: false, base_amount: 0, ..market };
    assert_eq!(opening.check().unwrap_err().field, "BaseAmount");
    let wide = Order { base_amount: 1 << 48, ..market };
    assert_eq!(wide.check().unwrap_err().field, "BaseAmount");
    assert_eq!(Order { price: 0, ..market }.check().unwrap_err().field, "Price");
    let far = Order { expiry: u64::MAX, ..order_of("limit-buy-gtt") };
    assert_eq!(far.check().unwrap_err().field, "OrderExpiry");
}

#[test]
fn an_order_reads_back_its_size_prices_and_whether_it_takes() {
    let grid = Grid::new(5, 1);
    let stop = order_of("stop-loss");
    assert_eq!(stop.size(grid).to_string(), "0.0002");
    assert_eq!(stop.limit(grid).to_string(), "78400");
    assert_eq!(stop.trigger(grid).map(|trigger| trigger.to_string()).as_deref(), Some("80000"));
    assert_eq!(order_of("limit-buy-gtt").trigger(grid), None);
    let takes: Vec<bool> = ["limit-buy-gtt", "limit-ioc", "market-buy", "stop-loss", "stop-loss-limit", "twap"]
        .into_iter()
        .map(|name| order_of(name).takes())
        .collect();
    assert_eq!(takes, [false, true, true, true, false, true]);
    let indexed = order_of("no-client-index").with_client_index(ClientOrderIndex::of::<9>());
    assert_eq!(indexed.client_index, Some(ClientOrderIndex::of::<9>()));
}
