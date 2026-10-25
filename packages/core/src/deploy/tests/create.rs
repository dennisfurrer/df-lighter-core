//! A binary options book: held to lighter-go, its multipliers derived as lighter-go derives them,
//! and every term `Validate` (and the request's symbol check) refuses, refused.

use super::*;

/// An Up/Down book as Lighter lists its own: two price decimals and a 1.00 payout, four size
/// decimals and a five-share minimum.
fn up_down() -> CreateMarket {
    CreateMarket {
        slot: MarketSlot::of::<1500>(),
        start_ms: 1_790_007_200_000,
        end_ms: 1_790_008_100_000,
        settlement_cap: 100,
        settlement: Settlement::Discrete,
        default_price: 50,
        taker_fee: 10_000,
        maker_fee: 0,
        min_base_amount: 50_000,
        min_quote_amount: 1,
        order_quote_limit: MAX_ORDER_QUOTE_AMOUNT,
        open_interest_limit: 1 << 50,
        frozen: false,
        symbol: "ETH-UPDOWN-15M-1790007200".to_owned(),
        size_decimals: 4,
        price_decimals: 2,
    }
}

#[cfg(feature = "sign")]
#[test]
fn a_new_book_signs_as_lighter_go_does() {
    use crate::earn::support::{field, fields, matches};

    let go = fields("operator-create-market");
    let number = |name: &str| field(&go, name);
    let small = |name: &str| u32::try_from(number(name)).unwrap();
    let tx = CreateMarket {
        slot: MarketSlot::new(u16::try_from(number("MarketSlot")).unwrap()).unwrap(),
        start_ms: number("StartTimestampMs"),
        end_ms: number("EndTimestampMs"),
        settlement_cap: small("SettlementCap"),
        settlement: match number("SettlementType") {
            0 => Settlement::Discrete,
            _ => Settlement::Continuous,
        },
        default_price: small("DefaultPrice"),
        taker_fee: small("TakerFee"),
        maker_fee: small("MakerFee"),
        min_base_amount: number("MinBaseAmount"),
        min_quote_amount: number("MinQuoteAmount"),
        order_quote_limit: number("OrderQuoteLimit"),
        open_interest_limit: number("OpenInterestLimit"),
        frozen: number("IsFrozen") == 1,
        symbol: go["Symbol"].as_str().unwrap().to_owned(),
        size_decimals: u8::try_from(number("SizeDecimals")).unwrap(),
        price_decimals: u8::try_from(number("PriceDecimals")).unwrap(),
    };
    assert_eq!(tx, up_down());
    matches("operator-create-market", &tx);
}

#[test]
fn the_multipliers_follow_the_decimals_and_the_payout() {
    let book = up_down();
    assert_eq!((book.quote_extension_multiplier(), book.size_extension_multiplier()), (1_000_000, Some(100_000_000)));
    let fine = CreateMarket { size_decimals: 6, price_decimals: 0, ..up_down() };
    assert_eq!(fine.quote_extension_multiplier(), 1_000_000);
    let too_fine = CreateMarket { size_decimals: 13, ..up_down() };
    assert_eq!(too_fine.quote_extension_multiplier(), 0, "past twelve places there is none");
    let bytes = book.symbol_bytes();
    assert_eq!((&bytes[..3], bytes[25], bytes[31]), (&b"ETH"[..], 0, 0), "zero-padded");
}

#[test]
fn every_term_lighter_go_refuses_is_refused() {
    assert_eq!(up_down().check(), Ok(()));
    let field = |book: CreateMarket| book.check().unwrap_err().field;
    assert_eq!(field(CreateMarket { symbol: "X".repeat(32), ..up_down() }), "Symbol");
    assert_eq!(CreateMarket { symbol: "X".repeat(31), ..up_down() }.check(), Ok(()));
    assert_eq!(field(CreateMarket { slot: MarketSlot::of::<1>(), ..up_down() }), "MarketIndex");
    assert_eq!(field(CreateMarket { end_ms: 1_790_007_200_000, ..up_down() }), "StartTimestamp");
    assert_eq!(field(CreateMarket { end_ms: 1 << 48, ..up_down() }), "StartTimestamp");
    assert_eq!(field(CreateMarket { size_decimals: 5, ..up_down() }), "QuoteExtensionMultiplier", "seven places");
    assert_eq!(field(CreateMarket { settlement_cap: 0, ..up_down() }), "SizeExtensionMultiplier");
    assert_eq!(
        field(CreateMarket { settlement_cap: u32::MAX, size_decimals: 0, price_decimals: 0, ..up_down() }),
        "SizeExtensionMultiplier",
        "past 2^56"
    );
    assert_eq!(field(CreateMarket { default_price: 101, ..up_down() }), "DefaultPrice");
    assert_eq!(field(CreateMarket { taker_fee: 1_000_001, ..up_down() }), "TakerFee");
    assert_eq!(field(CreateMarket { min_base_amount: 0, ..up_down() }), "MinBaseAmount");
    assert_eq!(field(CreateMarket { min_quote_amount: 0, ..up_down() }), "MinQuoteAmount");
    assert_eq!(field(CreateMarket { order_quote_limit: 0, ..up_down() }), "OrderQuoteLimit");
    assert_eq!(field(CreateMarket { open_interest_limit: 1, ..up_down() }), "OpenInterestLimit");
    assert_eq!(field(CreateMarket { open_interest_limit: MAX_OPEN_INTEREST + 1, ..up_down() }), "OpenInterestLimit");
    let continuous = CreateMarket { settlement: Settlement::Continuous, frozen: true, ..up_down() };
    assert_eq!(continuous.check(), Ok(()));
}
