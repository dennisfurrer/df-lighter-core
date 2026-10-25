//! A binary options operator's book, built from the Go fields lighter-go was handed: an Up/Down
//! market created on slot 1500, frozen, settled Up and refunded.

use df_lighter_core::deploy::{CreateMarket, MarketSlot, SettleMarket, Settlement, UpdateMarket};

use crate::vector::{Fields, every};

const CREATE_MARKET: [&str; 1] = ["operator-create-market"];
const UPDATE_MARKET: [&str; 1] = ["operator-freeze-market"];
const SETTLE_MARKET: [&str; 2] = ["operator-settle-up", "operator-refund"];

/// The transaction cases replayed here.
pub const CASES: [&[&str]; 3] = [&CREATE_MARKET, &UPDATE_MARKET, &SETTLE_MARKET];

fn slot(fields: Fields) -> MarketSlot {
    MarketSlot::new(fields.narrow("MarketSlot")).unwrap()
}

#[test]
fn a_book_is_created_as_lighter_go_signs_it() {
    every("BinaryOptionsOperatorCreateMarket", &CREATE_MARKET, &[], |case| {
        let fields = case.fields();
        let settlement = [Settlement::Discrete, Settlement::Continuous]
            .into_iter()
            .find(|settlement| u64::from(settlement.code()) == fields.int("SettlementType"))
            .unwrap();
        Ok(CreateMarket {
            slot: slot(fields),
            start_ms: fields.int("StartTimestampMs"),
            end_ms: fields.int("EndTimestampMs"),
            settlement_cap: fields.narrow("SettlementCap"),
            settlement,
            default_price: fields.narrow("DefaultPrice"),
            taker_fee: fields.narrow("TakerFee"),
            maker_fee: fields.narrow("MakerFee"),
            min_base_amount: fields.int("MinBaseAmount"),
            min_quote_amount: fields.int("MinQuoteAmount"),
            order_quote_limit: fields.int("OrderQuoteLimit"),
            open_interest_limit: fields.int("OpenInterestLimit"),
            frozen: fields.flag("IsFrozen"),
            symbol: fields.text("Symbol").to_owned(),
            size_decimals: fields.narrow("SizeDecimals"),
            price_decimals: fields.narrow("PriceDecimals"),
        })
    });
}

#[test]
fn a_book_is_frozen_as_lighter_go_signs_it() {
    every("BinaryOptionsOperatorUpdateMarket", &UPDATE_MARKET, &[], |case| {
        let fields = case.fields();
        Ok(UpdateMarket {
            slot: slot(fields),
            market: fields.market("MarketIndex")?,
            start_ms: fields.int("StartTimestampMs"),
            end_ms: fields.int("EndTimestampMs"),
            taker_fee: fields.narrow("TakerFee"),
            maker_fee: fields.narrow("MakerFee"),
            liquidation_fee: fields.narrow("LiquidationFee"),
            min_base_amount: fields.int("MinBaseAmount"),
            min_quote_amount: fields.int("MinQuoteAmount"),
            order_quote_limit: fields.int("OrderQuoteLimit"),
            open_interest_limit: fields.int("OpenInterestLimit"),
            frozen: fields.flag("IsFrozen"),
        })
    });
}

#[test]
fn a_book_is_settled_up_and_refunded_as_lighter_go_signs_it() {
    every("BinaryOptionsOperatorSettleMarket", &SETTLE_MARKET, &[], |case| {
        let fields = case.fields();
        Ok(SettleMarket {
            slot: slot(fields),
            market: fields.market("MarketIndex")?,
            price: fields.narrow("SettlementPrice"),
            refund: fields.flag("IsRefund"),
        })
    });
}
