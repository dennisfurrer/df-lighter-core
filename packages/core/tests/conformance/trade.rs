//! Orders on every market, each built from the Go fields lighter-go was handed: every placeable
//! kind and time in force, spot and binary options markets, the widest integers, integrator fees,
//! self-trade handling and a skipped nonce, on Lighter, on a sub-account and on Robinhood Chain;
//! OTO, OCO and OTOCO groups; cancels by either index, cancel-alls now, in one market, scheduled
//! and aborted; modifies with and without a version; leverage and isolated margin. What lighter-go
//! refused is refused here: slot 255, a zero size, a market order resting, fees with self-trade
//! handling, a sized child, a scheduled cancel-all in one market, a zero margin fraction.

use df_lighter_core::trade::{
    CancelAllOrders, CancelAllTime, CancelOrder, ClientOrderIndex, CreateGroupedOrders, CreateOrder, Grouping,
    MarginDirection, MarginMode, ModifyOrder, Order, OrderKind, OrderRef, Side, TimeInForce, UpdateLeverage,
    UpdateMargin,
};
use df_lighter_core::utils::BadId;

use crate::vector::{Fields, Refused, attributes, every};

const CREATE_ORDER: [&str; 24] = [
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
    "refused-market-255",
    "refused-zero-size",
    "refused-market-gtt",
    "refused-fees-and-self-trade",
];
const CREATE_GROUPED_ORDERS: [&str; 6] = [
    "oto-entry-and-stop",
    "otoco-entry-take-profit-stop",
    "otoco-limit-entry",
    "oco-take-profit-and-stop",
    "otoco-integrator",
    "refused-oto-sized-child",
];
const CANCEL_ORDER: [&str; 3] = ["cancel-by-client-index", "cancel-by-order-index", "cancel-order-index-above-2^53"];
const CANCEL_ALL_ORDERS: [&str; 5] = [
    "cancel-all-now",
    "cancel-all-one-market",
    "cancel-all-scheduled",
    "cancel-all-abort",
    "refused-cancel-one-market-scheduled",
];
const MODIFY_ORDER: [&str; 3] = ["modify-limit", "modify-trigger", "modify-with-version"];
const UPDATE_LEVERAGE: [&str; 3] = ["leverage-cross-10x", "leverage-isolated-3x", "refused-leverage-zero"];
const UPDATE_MARGIN: [&str; 3] = ["margin-add", "margin-remove", "margin-above-2^32"];

/// The transaction cases replayed here.
pub const CASES: [&[&str]; 7] = [
    &CREATE_ORDER,
    &CREATE_GROUPED_ORDERS,
    &CANCEL_ORDER,
    &CANCEL_ALL_ORDERS,
    &MODIFY_ORDER,
    &UPDATE_LEVERAGE,
    &UPDATE_MARGIN,
];

/// One order from lighter-go's `CreateOrderTxReq` fields; a zero client index is none.
fn order(fields: Fields) -> Result<Order, BadId> {
    let client = fields.int("ClientOrderIndex");
    Ok(Order {
        market: fields.market("MarketIndex")?,
        client_index: (client != 0).then(|| ClientOrderIndex::new(client)).transpose()?,
        base_amount: fields.int("BaseAmount"),
        price: fields.narrow("Price"),
        side: Side::from_is_ask(fields.flag("IsAsk")),
        kind: OrderKind::from_code(fields.narrow("Type")).unwrap(),
        time_in_force: TimeInForce::from_code(fields.narrow("TimeInForce")).unwrap(),
        reduce_only: fields.flag("ReduceOnly"),
        trigger_price: fields.narrow("TriggerPrice"),
        expiry: fields.int("OrderExpiry"),
    })
}

#[test]
fn every_order_signs_as_lighter_go_signs_it_and_what_it_refuses_is_refused() {
    let refused = [
        ("refused-market-255", Refused::Id(BadId::Market(255))),
        ("refused-zero-size", Refused::Field("BaseAmount")),
        ("refused-market-gtt", Refused::Field("TimeInForce")),
        ("refused-fees-and-self-trade", Refused::Field("L2TxAttributes")),
    ];
    every("CreateOrder", &CREATE_ORDER, &refused, |case| {
        Ok(CreateOrder::new(order(case.fields())?).with(attributes(case)?))
    });
}

#[test]
fn every_group_signs_as_lighter_go_signs_it_and_a_sized_child_is_refused() {
    let refused = [("refused-oto-sized-child", Refused::Field("BaseAmount"))];
    every("CreateGroupedOrders", &CREATE_GROUPED_ORDERS, &refused, |case| {
        let fields = case.fields();
        Ok(CreateGroupedOrders {
            grouping: Grouping::from_code(fields.narrow("GroupingType")).unwrap(),
            orders: fields.list("Orders").into_iter().map(order).collect::<Result<_, _>>()?,
            attributes: attributes(case)?,
        })
    });
}

#[test]
fn every_cancel_signs_as_lighter_go_signs_it_by_either_index() {
    every("CancelOrder", &CANCEL_ORDER, &[], |case| {
        let fields = case.fields();
        let order = OrderRef::from_index(fields.int("Index"))?;
        Ok(CancelOrder::new(fields.market("MarketIndex")?, order).with(attributes(case)?))
    });
}

#[test]
fn every_cancel_all_signs_as_lighter_go_signs_it_and_one_market_is_never_scheduled() {
    let refused = [("refused-cancel-one-market-scheduled", Refused::Field("TimeInForce"))];
    every("CancelAllOrders", &CANCEL_ALL_ORDERS, &refused, |case| {
        let fields = case.fields();
        let (code, time) = (fields.int("TimeInForce"), fields.int("Time"));
        let when = match (code, time) {
            (0, 0) => CancelAllTime::Now,
            (1, at) => CancelAllTime::At(at),
            (2, 0) => CancelAllTime::Abort,
            other => panic!("{}: no cancel-all is {other:?}", case.name()),
        };
        Ok(CancelAllOrders { when, attributes: attributes(case)? })
    });
}

#[test]
fn every_modify_signs_as_lighter_go_signs_it_with_and_without_a_version() {
    every("ModifyOrder", &MODIFY_ORDER, &[], |case| {
        let fields = case.fields();
        Ok(ModifyOrder {
            market: fields.market("MarketIndex")?,
            order: OrderRef::from_index(fields.int("Index"))?,
            base_amount: fields.int("BaseAmount"),
            price: fields.narrow("Price"),
            trigger_price: fields.narrow("TriggerPrice"),
            attributes: attributes(case)?,
        })
    });
}

#[test]
fn every_leverage_update_signs_as_lighter_go_signs_it_and_a_zero_fraction_is_refused() {
    let refused = [("refused-leverage-zero", Refused::Field("InitialMarginFraction"))];
    every("UpdateLeverage", &UPDATE_LEVERAGE, &refused, |case| {
        let fields = case.fields();
        Ok(UpdateLeverage {
            market: fields.market("MarketIndex")?,
            initial_margin_fraction: fields.narrow("InitialMarginFraction"),
            mode: MarginMode::from_code(fields.narrow("MarginMode")).unwrap(),
            attributes: attributes(case)?,
        })
    });
}

#[test]
fn every_margin_update_signs_as_lighter_go_signs_it_past_2_to_the_32() {
    every("UpdateMargin", &UPDATE_MARGIN, &[], |case| {
        let fields = case.fields();
        let direction = match fields.int("Direction") {
            0 => MarginDirection::Remove,
            1 => MarginDirection::Add,
            other => panic!("no direction {other}"),
        };
        Ok(UpdateMargin {
            market: fields.market("MarketIndex")?,
            amount: fields.int("USDCAmount"),
            direction,
            attributes: attributes(case)?,
        })
    });
}
