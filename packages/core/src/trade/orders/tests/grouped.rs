//! Grouped orders, held to lighter-go: OTO, OCO and OTOCO, a limit entry, an integrator on the
//! whole group, a bracket built from an entry and two triggers, and the rules a group is refused
//! by.

use super::*;
use crate::trade::support::{attributes, case, cases, d, order, signed, signs_as_lighter_go};
use crate::utils::{ClientOrderIndex, MarketId, Side, TimeInForce};

const NOW: u64 = 1_790_000_000_000;
const BTC: Grid = Grid::new(5, 1);

/// A case's group, exactly as its fields read.
fn group(name: &str) -> CreateGroupedOrders {
    let case = case(name);
    let fields = &case["fields"];
    CreateGroupedOrders {
        grouping: Grouping::from_code(u8::try_from(fields["GroupingType"].as_u64().unwrap()).unwrap()).unwrap(),
        orders: fields["Orders"].as_array().unwrap().iter().map(order).collect(),
        attributes: attributes(&case),
    }
}

#[test]
fn every_group_lighter_go_signs_signs_the_same_here() {
    let mut names = Vec::new();
    for case in cases("CreateGroupedOrders").iter().filter(|case| signed(case)) {
        let name = case["name"].as_str().unwrap();
        signs_as_lighter_go(&group(name), case);
        names.push(name.to_owned());
    }
    assert_eq!(
        names,
        [
            "oto-entry-and-stop",
            "otoco-entry-take-profit-stop",
            "otoco-limit-entry",
            "oco-take-profit-and-stop",
            "otoco-integrator"
        ]
    );
}

#[test]
fn a_child_with_a_size_of_its_own_is_refused_as_lighter_go_refuses_it() {
    let sized = group("refused-oto-sized-child");
    assert!(!signed(&case("refused-oto-sized-child")));
    assert_eq!(sized.check().unwrap_err().field, "BaseAmount");
    let [entry, child] = [sized.orders[0], sized.orders[1]];
    let cleared = CreateGroupedOrders::oto(entry, child);
    assert_eq!(cleared.orders[1].base_amount, 0, "oto clears it");
    signs_as_lighter_go(&cleared, &case("oto-entry-and-stop"));
}

#[test]
fn the_constructors_build_what_lighter_go_signed() {
    let otoco = group("otoco-entry-take-profit-stop");
    let [entry, take_profit, stop_loss] = [otoco.orders[0], otoco.orders[1], otoco.orders[2]];
    assert_eq!(CreateGroupedOrders::otoco(entry, take_profit, stop_loss), otoco);
    let oco = group("oco-take-profit-and-stop");
    assert_eq!(CreateGroupedOrders::oco(oco.orders[0], oco.orders[1]), oco);
    let integrated = group("otoco-integrator");
    assert_eq!(CreateGroupedOrders::otoco(entry, take_profit, stop_loss).with(integrated.attributes), integrated);
}

#[test]
fn a_bracket_is_an_entry_with_its_take_profit_and_stop_two_percent_past_their_triggers() {
    let entry = group("otoco-entry-take-profit-stop").orders[0];
    let mut bracket =
        CreateGroupedOrders::bracket(entry, Some(d("90000")), Some(d("80000")), BTC, NOW, Fit::Exact).unwrap();
    bracket.orders[1].client_index = Some(ClientOrderIndex::of::<32>());
    bracket.orders[2].client_index = Some(ClientOrderIndex::of::<31>());
    assert_eq!(bracket, group("otoco-entry-take-profit-stop"));

    let mut stop_only = CreateGroupedOrders::bracket(entry, None, Some(d("80000")), BTC, NOW, Fit::Exact).unwrap();
    stop_only.orders[1].client_index = Some(ClientOrderIndex::of::<31>());
    assert_eq!(stop_only, group("oto-entry-and-stop"));

    let resting = group("otoco-limit-entry").orders[0];
    let later = CreateGroupedOrders::bracket(resting, Some(d("90000")), Some(d("80000")), BTC, NOW + 5_000, Fit::Exact)
        .unwrap();
    assert!(later.orders.iter().all(|order| order.expiry == resting.expiry), "children lapse with a resting entry");

    let closing = Order { reduce_only: true, ..entry };
    assert!(matches!(
        CreateGroupedOrders::bracket(closing, Some(d("90000")), None, BTC, NOW, Fit::Exact),
        Err(TradeError::NotFor(_))
    ));
    assert!(CreateGroupedOrders::bracket(entry, None, None, BTC, NOW, Fit::Exact).is_err());
    let fine = CreateGroupedOrders::bracket(entry, Some(d("90000.06")), None, BTC, NOW, Fit::Exact);
    assert!(matches!(fine, Err(TradeError::Units { field: "TriggerPrice", .. })));
    let placed = CreateGroupedOrders::bracket(entry, Some(d("90000.06")), None, BTC, NOW, Fit::Favour).unwrap();
    assert_eq!(placed.orders[1].trigger_price, 900_001);
}

#[test]
fn a_group_is_refused_by_lighter_go_s_rules() {
    let otoco = group("otoco-entry-take-profit-stop");
    let oco = group("oco-take-profit-and-stop");
    let oto = group("oto-entry-and-stop");
    let refuses = |group: &CreateGroupedOrders, field: &str| {
        let error = group.check().unwrap_err();
        assert_eq!(error.field, field, "{group:?}: {error}");
    };
    let with = |group: &CreateGroupedOrders, at: usize, change: &dyn Fn(Order) -> Order| {
        let mut changed = group.clone();
        changed.orders[at] = change(changed.orders[at]);
        changed
    };

    refuses(&CreateGroupedOrders { orders: Vec::new(), ..oto.clone() }, "Orders");
    let four = CreateGroupedOrders { orders: [otoco.orders.clone(), vec![otoco.orders[0]]].concat(), ..otoco.clone() };
    refuses(&four, "Orders");
    refuses(&CreateGroupedOrders { grouping: Grouping::Oto, ..otoco.clone() }, "Orders");
    refuses(&CreateGroupedOrders { grouping: Grouping::Otoco, ..oto.clone() }, "Orders");
    refuses(&with(&otoco, 2, &|order| Order { market: MarketId::of::<2>(), ..order }), "MarketIndex");
    refuses(
        &with(&otoco, 2, &|order| Order { client_index: Some(ClientOrderIndex::of::<32>()), ..order }),
        "ClientOrderIndex",
    );
    refuses(&with(&oto, 0, &|order| Order { price: 0, ..order }), "Price");

    refuses(&with(&oto, 1, &|order| Order { side: Side::Bid, ..order }), "IsAsk");
    refuses(&with(&oto, 1, &|order| Order { kind: OrderKind::Limit, trigger_price: 0, ..order }), "Type");
    refuses(&with(&oto, 0, &|order| Order { kind: OrderKind::Twap, ..order }), "Type");
    refuses(&with(&oto, 0, &|order| Order { expiry: 1, ..order }), "OrderExpiry");
    let resting = group("otoco-limit-entry");
    refuses(&with(&resting, 0, &|order| Order { expiry: order.expiry + 1, ..order }), "OrderExpiry");
    refuses(&with(&otoco, 2, &|order| Order { expiry: order.expiry + 1, ..order }), "OrderExpiry");
    refuses(&with(&otoco, 2, &|order| Order { kind: OrderKind::TakeProfit, ..order }), "Type");
    refuses(&with(&otoco, 1, &|order| Order { time_in_force: TimeInForce::GoodTillTime, ..order }), "TimeInForce");

    refuses(&with(&oco, 1, &|order| Order { base_amount: 999, ..order }), "BaseAmount");
    refuses(&with(&oco, 1, &|order| Order { side: Side::Bid, ..order }), "IsAsk");
    refuses(&with(&oco, 1, &|order| Order { reduce_only: false, ..order }), "ReduceOnly");
    refuses(&with(&oco, 1, &|order| Order { expiry: order.expiry + 1, ..order }), "OrderExpiry");
    let both_unsized =
        with(&with(&oco, 0, &|order| Order { base_amount: 0, ..order }), 1, &|order| Order { base_amount: 0, ..order });
    both_unsized.check().unwrap();

    let fees_and_self_trade = otoco.clone().with(
        Attributes::NONE
            .integrator(crate::utils::AccountIndex::of::<713_888>(), 5, 0)
            .self_trade(crate::utils::sign::SelfTrade::ExpireTaker, crate::utils::sign::SelfTradeScope::Account),
    );
    refuses(&fees_and_self_trade, "L2TxAttributes");
}
