//! A freeze, held to lighter-go, and the terms each kind of book does or does not take.

use super::*;

fn freeze() -> UpdateMarket {
    UpdateMarket {
        slot: MarketSlot::of::<1500>(),
        market: MarketId::of::<4206>(),
        start_ms: 1_790_000_000_000,
        end_ms: 1_790_000_900_000,
        taker_fee: 10_000,
        maker_fee: 0,
        liquidation_fee: 0,
        min_base_amount: 50_000,
        min_quote_amount: 1,
        order_quote_limit: (1 << 48) - 1,
        open_interest_limit: 1 << 50,
        frozen: true,
    }
}

#[cfg(feature = "sign")]
#[test]
fn a_freeze_signs_as_lighter_go_does() {
    use crate::earn::support::{field, fields, matches};

    let go = fields("operator-freeze-market");
    let number = |name: &str| field(&go, name);
    let small = |name: &str| u32::try_from(number(name)).unwrap();
    let tx = UpdateMarket {
        slot: MarketSlot::new(u16::try_from(number("MarketSlot")).unwrap()).unwrap(),
        market: MarketId::new(number("MarketIndex")).unwrap(),
        start_ms: number("StartTimestampMs"),
        end_ms: number("EndTimestampMs"),
        taker_fee: small("TakerFee"),
        maker_fee: small("MakerFee"),
        liquidation_fee: small("LiquidationFee"),
        min_base_amount: number("MinBaseAmount"),
        min_quote_amount: number("MinQuoteAmount"),
        order_quote_limit: number("OrderQuoteLimit"),
        open_interest_limit: number("OpenInterestLimit"),
        frozen: number("IsFrozen") == 1,
    };
    assert_eq!(tx, freeze());
    assert_eq!(tx, freeze().with_frozen(false).with_frozen(true));
    matches("operator-freeze-market", &tx);
}

#[test]
fn each_kind_of_book_takes_only_its_own_terms() {
    assert_eq!(freeze().check(), Ok(()));
    let field = |update: UpdateMarket| update.check().unwrap_err().field;
    assert_eq!(field(UpdateMarket { end_ms: 1_790_000_000_000, ..freeze() }), "StartTimestamp");
    assert_eq!(field(UpdateMarket { liquidation_fee: 1, ..freeze() }), "LiquidationFee");

    let perp = UpdateMarket {
        slot: MarketSlot::of::<1>(),
        market: MarketId::of::<1>(),
        start_ms: 0,
        end_ms: 0,
        liquidation_fee: MAX_LIQUIDATION_FEE,
        frozen: false,
        ..freeze()
    };
    assert_eq!(perp.check(), Ok(()), "a perp book takes a liquidation fee up to 1%");
    assert_eq!(field(UpdateMarket { liquidation_fee: MAX_LIQUIDATION_FEE + 1, ..perp }), "LiquidationFee");
    assert_eq!(field(UpdateMarket { end_ms: 1, ..perp }), "StartTimestamp");
    assert_eq!(field(UpdateMarket { frozen: true, ..perp }), "IsFrozen");

    let spot = UpdateMarket { slot: MarketSlot::of::<2048>(), liquidation_fee: 0, open_interest_limit: 0, ..perp };
    assert_eq!(spot.check(), Ok(()), "a spot book carries no open interest limit");
    assert_eq!(field(UpdateMarket { open_interest_limit: 1 << 50, ..spot }), "OpenInterestLimit");
    assert_eq!(field(UpdateMarket { liquidation_fee: 1, ..spot }), "LiquidationFee");
    assert_eq!(field(UpdateMarket { maker_fee: 1_000_001, ..spot }), "TakerFee");
}
