//! Orders built from decimals: each of lighter-go's cases rebuilt from what a trader types, the
//! venue's defaults (28 days, 2% slippage), rounding only when asked, quote and whole-position
//! sizes, and the least size and value binding resting orders only.

use super::*;
use crate::trade::support::{case, d, order};
use crate::utils::Round;
use crate::utils::time::HOUR_MS;

/// The vectors' now.
const NOW: u64 = 1_790_000_000_000;
/// BTC on Lighter: 5 size decimals, 1 price decimal.
const BTC: Grid = Grid::new(5, 1);
/// ETH: 4 and 2.
const ETH: Grid = Grid::new(4, 2);

fn btc() -> MarketId {
    MarketId::of::<1>()
}

fn eth() -> MarketId {
    MarketId::of::<0>()
}

fn index(value: u64) -> ClientOrderIndex {
    ClientOrderIndex::new(value).unwrap()
}

fn vector_order(name: &str) -> Order {
    order(&case(name)["fields"])
}

#[test]
fn every_kind_builds_the_order_lighter_go_signed() {
    let built = [
        (
            "limit-buy-gtt",
            OrderIntent::limit(btc(), Side::Bid, d("0.01"), d("83453")).client_index(index(1_790_000_000_000)),
            BTC,
        ),
        (
            "limit-sell-post-only",
            OrderIntent::limit(eth(), Side::Ask, d("0.05"), d("3100")).post_only().client_index(index(7)),
            ETH,
        ),
        ("limit-ioc", OrderIntent::limit(eth(), Side::Bid, d("0.05"), d("3100")).ioc().client_index(index(8)), ETH),
        ("market-buy", OrderIntent::market(btc(), Side::Bid, d("0.0002"), d("83431.4")).client_index(index(9)), BTC),
        (
            "market-sell-reduce-only",
            OrderIntent::market(btc(), Side::Ask, d("0.0002"), d("83367.3")).reduce_only().client_index(index(10)),
            BTC,
        ),
        (
            "stop-loss",
            OrderIntent::stop_loss(btc(), Side::Ask, d("0.0002"), d("80000")).reduce_only().client_index(index(11)),
            BTC,
        ),
        (
            "stop-loss-limit",
            OrderIntent::stop_loss_limit(btc(), Side::Ask, d("0.0002"), d("80000"), d("79900"))
                .reduce_only()
                .client_index(index(12)),
            BTC,
        ),
        (
            "take-profit",
            OrderIntent::take_profit(btc(), Side::Ask, d("0.0002"), d("90000")).reduce_only().client_index(index(13)),
            BTC,
        ),
        (
            "take-profit-limit",
            OrderIntent::take_profit_limit(btc(), Side::Ask, d("0.0002"), d("90000"), d("90100"))
                .reduce_only()
                .client_index(index(14)),
            BTC,
        ),
        (
            "twap",
            OrderIntent::twap(btc(), Side::Bid, d("0.01"), d("90000"), NOW + HOUR_MS).client_index(index(15)),
            BTC,
        ),
        ("no-client-index", OrderIntent::limit(btc(), Side::Bid, d("0.01"), d("83453")), BTC),
        (
            "spot-market-2048",
            OrderIntent::limit(MarketId::of::<2048>(), Side::Bid, d("0.1"), d("2310")).client_index(index(16)),
            ETH,
        ),
        (
            "binary-option-4206",
            OrderIntent::limit(MarketId::of::<4206>(), Side::Bid, d("100"), d("51")).client_index(index(17)),
            Grid::new(4, 0),
        ),
        (
            "robinhood-limit",
            OrderIntent::limit(MarketId::of::<16>(), Side::Bid, d("1"), d("250.12")).client_index(index(1)),
            Grid::new(2, 2),
        ),
        (
            "sub-account-market",
            OrderIntent::market_at(eth(), Side::Bid, d("0.005"), d("3200")).client_index(index(22)),
            ETH,
        ),
    ];
    for (name, intent, grid) in built {
        assert_eq!(intent.build(grid, NOW), Ok(vector_order(name)), "{name}");
    }
}

#[test]
fn a_market_order_is_bounded_off_its_reference_toward_it_on_the_grid() {
    let buy = OrderIntent::market(btc(), Side::Bid, d("0.01"), d("65000")).build(BTC, NOW).unwrap();
    assert_eq!(
        (buy.price, buy.kind, buy.time_in_force, buy.expiry),
        (663_000, OrderKind::Market, TimeInForce::ImmediateOrCancel, 0)
    );
    let sell = OrderIntent::market(btc(), Side::Ask, d("0.01"), d("65000.05")).build(BTC, NOW).unwrap();
    assert_eq!(sell.price, 637_001, "63,700.049 rounds up: never more slippage than asked");
    let tight = OrderIntent::market(btc(), Side::Bid, d("0.01"), d("65000")).slippage_bps(50).build(BTC, NOW).unwrap();
    assert_eq!(tight.price, 653_250);
    let wide = OrderIntent::market(btc(), Side::Bid, d("0.01"), d("65000")).slippage_bps(1_001);
    assert_eq!(wide.build(BTC, NOW), Err(TradeError::Slippage(1_001)));
    let nothing = OrderIntent::market(btc(), Side::Bid, d("0.01"), Dec::ZERO);
    assert_eq!(nothing.build(BTC, NOW), Err(TradeError::NoPrice("a reference")));
}

#[test]
fn a_worst_price_of_the_trader_s_own_is_held_within_ten_percent_of_a_reference() {
    let near = OrderIntent::market_at(btc(), Side::Bid, d("0.01"), d("71500")).reference(d("65000"));
    assert_eq!(near.build(BTC, NOW).unwrap().price, 715_000);
    let far = OrderIntent::market_at(btc(), Side::Bid, d("0.01"), d("80000")).reference(d("65000"));
    assert_eq!(far.build(BTC, NOW), Err(TradeError::TooFar { worst: d("80000"), reference: d("65000") }));
    let low = OrderIntent::market_at(btc(), Side::Ask, d("0.01"), d("58499.9")).reference(d("65000"));
    assert!(matches!(low.build(BTC, NOW), Err(TradeError::TooFar { .. })));
    let alone = OrderIntent::market_at(btc(), Side::Bid, d("0.01"), d("80000"));
    assert_eq!(alone.build(BTC, NOW).unwrap().price, 800_000, "no reference, nothing to hold it to");
}

#[test]
fn a_figure_finer_than_the_grid_is_refused_unless_a_fit_is_given() {
    let sized = OrderIntent::limit(btc(), Side::Bid, d("0.012345"), d("83453"));
    assert!(matches!(sized.build(BTC, NOW), Err(TradeError::Units { field: "BaseAmount", .. })));
    let fine = OrderIntent::limit(btc(), Side::Bid, d("0.012345"), d("83453.05"));
    let priced = OrderIntent::limit(btc(), Side::Bid, d("0.01"), d("83453.05"));
    assert!(matches!(priced.build(BTC, NOW), Err(TradeError::Units { field: "Price", .. })));

    let buy = fine.fit(Fit::Favour).build(BTC, NOW).unwrap();
    assert_eq!((buy.base_amount, buy.price), (1_234, 834_530), "a size cut down, a buy's price down");
    let sell =
        OrderIntent::limit(btc(), Side::Ask, d("0.012345"), d("83453.05")).fit(Fit::Favour).build(BTC, NOW).unwrap();
    assert_eq!((sell.base_amount, sell.price), (1_234, 834_531), "a sell's price up");
    let up = fine.fit(Fit::Round(Round::Up)).build(BTC, NOW).unwrap();
    assert_eq!((up.base_amount, up.price), (1_235, 834_531));
    let stop =
        OrderIntent::stop_loss(btc(), Side::Ask, d("0.01"), d("80000.06")).fit(Fit::Favour).build(BTC, NOW).unwrap();
    assert_eq!((stop.trigger_price, stop.price), (800_001, 784_001), "a trigger to the nearest, its bound off it");
    let nothing = OrderIntent::limit(btc(), Side::Bid, d("0.000001"), d("1")).fit(Fit::Favour);
    assert!(matches!(nothing.build(BTC, NOW), Err(TradeError::Units { field: "BaseAmount", .. })));
}

#[test]
fn a_resting_order_lapses_in_28_days_unless_it_says_when_inside_the_venue_s_window() {
    let resting = OrderIntent::limit(btc(), Side::Bid, d("0.01"), d("83453"));
    assert_eq!(resting.build(BTC, NOW).unwrap().expiry, NOW + 28 * 24 * HOUR_MS);
    assert_eq!(resting.expires_at(NOW + HOUR_MS).build(BTC, NOW).unwrap().expiry, NOW + HOUR_MS);
    for at in [NOW + 60_000, NOW + 31 * 24 * HOUR_MS, NOW - 1] {
        assert!(matches!(resting.expires_at(at).build(BTC, NOW), Err(TradeError::Expiry { .. })), "{at}");
    }
    let stop = OrderIntent::stop_loss(btc(), Side::Ask, d("0.01"), d("80000")).expires_at(NOW + HOUR_MS);
    assert_eq!(stop.build(BTC, NOW).unwrap().expiry, NOW + HOUR_MS);
    let ioc = OrderIntent::limit(btc(), Side::Bid, d("0.01"), d("83453")).ioc().expires_at(NOW + HOUR_MS);
    assert!(matches!(ioc.build(BTC, NOW), Err(TradeError::NotFor(_))), "an order that fills at once has none");
    let short = OrderIntent::twap(btc(), Side::Bid, d("0.01"), d("90000"), NOW + 60_000);
    assert!(matches!(short.build(BTC, NOW), Err(TradeError::Expiry { .. })));
    let long = OrderIntent::twap(btc(), Side::Bid, d("0.01"), d("90000"), NOW + 25 * HOUR_MS);
    assert!(matches!(long.build(BTC, NOW), Err(TradeError::Expiry { .. })));
}

#[test]
fn a_time_in_force_or_slippage_the_kind_does_not_take_is_refused() {
    let market = OrderIntent::market(btc(), Side::Bid, d("0.01"), d("65000"));
    assert!(matches!(market.post_only().build(BTC, NOW), Err(TradeError::NotFor(_))));
    assert!(market.ioc().build(BTC, NOW).is_ok());
    let stop = OrderIntent::stop_loss(btc(), Side::Ask, d("0.01"), d("80000"));
    assert!(matches!(stop.good_till_time().build(BTC, NOW), Err(TradeError::NotFor(_))));
    let stop_limit = OrderIntent::stop_loss_limit(btc(), Side::Ask, d("0.01"), d("80000"), d("79900"));
    assert_eq!(stop_limit.post_only().build(BTC, NOW).unwrap().time_in_force, TimeInForce::PostOnly);
    assert!(matches!(stop_limit.slippage_bps(100).build(BTC, NOW), Err(TradeError::NotFor(_))));
    let twap = OrderIntent::twap(btc(), Side::Bid, d("0.01"), d("90000"), NOW + HOUR_MS);
    assert!(matches!(twap.ioc().build(BTC, NOW), Err(TradeError::NotFor(_))));
    let limit = OrderIntent::limit(btc(), Side::Bid, d("0.01"), d("83453"));
    assert!(matches!(limit.slippage_bps(100).build(BTC, NOW), Err(TradeError::NotFor(_))));
    let wide_stop = stop.slippage_bps(500).build(BTC, NOW).unwrap();
    assert_eq!(wide_stop.price, 760_000, "5% under the trigger");
}

#[test]
fn a_quote_size_is_priced_at_the_order_s_price_else_its_trigger_else_the_reference_and_cut_down() {
    let limit = OrderIntent::limit(btc(), Side::Bid, Size::Quote(d("100")), d("65000")).build(BTC, NOW).unwrap();
    assert_eq!(limit.base_amount, 153, "100 / 65,000 = 0.0015384..., cut to 0.00153");
    let market = OrderIntent::market(btc(), Side::Bid, Size::Quote(d("100")), d("50000")).build(BTC, NOW).unwrap();
    assert_eq!(market.base_amount, 200, "sized at the reference, not the bound");
    let stop = OrderIntent::stop_loss(btc(), Side::Ask, Size::Quote(d("100")), d("40000")).build(BTC, NOW).unwrap();
    assert_eq!(stop.base_amount, 250);
    let dust = OrderIntent::limit(btc(), Side::Bid, Size::Quote(d("0.0001")), d("65000"));
    assert!(matches!(dust.build(BTC, NOW), Err(TradeError::Units { field: "BaseAmount", .. })));
}

#[test]
fn the_whole_position_is_a_size_for_a_reduce_only_order_alone() {
    let closing = OrderIntent::take_profit(btc(), Side::Ask, Size::Position, d("90000")).reduce_only();
    assert_eq!(closing.build(BTC, NOW).unwrap().base_amount, 0);
    let opening = OrderIntent::take_profit(btc(), Side::Ask, Size::Position, d("90000"));
    assert!(matches!(opening.build(BTC, NOW), Err(TradeError::NotFor(_))));
}

#[test]
fn the_least_size_and_value_bind_resting_orders_only() {
    let small = OrderIntent::limit(btc(), Side::Bid, d("0.0001"), d("65000")).minimums(d("0.0002"), d("10"));
    assert_eq!(small.build(BTC, NOW), Err(TradeError::BelowLeastSize { size: d("0.0001"), least: d("0.0002") }));
    let cheap = OrderIntent::limit(btc(), Side::Bid, d("0.0002"), d("40000")).minimums(d("0.0002"), d("10"));
    assert_eq!(cheap.build(BTC, NOW), Err(TradeError::BelowLeastValue { value: d("8"), least: d("10") }));
    assert!(cheap.ioc().build(BTC, NOW).is_ok(), "an immediate-or-cancel limit takes");
    let dust =
        OrderIntent::market(btc(), Side::Ask, d("0.0001"), d("65000")).reduce_only().minimums(d("0.0002"), d("10"));
    assert!(dust.build(BTC, NOW).is_ok(), "closing a dust position");
}
