//! Settling Up and refunding, held to lighter-go, and a refund that names a price refused.

use super::*;

#[cfg(feature = "sign")]
#[test]
fn an_up_settlement_and_a_refund_sign_as_lighter_go_does() {
    use crate::earn::support::{field, fields, matches};

    for name in ["operator-settle-up", "operator-refund"] {
        let fields = fields(name);
        let slot = MarketSlot::new(u16::try_from(field(&fields, "MarketSlot")).unwrap()).unwrap();
        let market = MarketId::new(field(&fields, "MarketIndex")).unwrap();
        let tx = SettleMarket {
            slot,
            market,
            price: u32::try_from(field(&fields, "SettlementPrice")).unwrap(),
            refund: field(&fields, "IsRefund") == 1,
        };
        let made = match name {
            "operator-settle-up" => SettleMarket::up(slot, market, 100),
            _ => SettleMarket::refund(slot, market),
        };
        assert_eq!(tx, made, "{name}");
        matches(name, &tx);
    }
}

#[test]
fn a_refund_names_no_price_and_a_settlement_names_a_binary_options_slot() {
    let slot = MarketSlot::of::<1500>();
    let market = MarketId::of::<4206>();
    assert_eq!(SettleMarket::down(slot, market).price, 0);
    assert_eq!(SettleMarket::at(slot, market, 37).check(), Ok(()), "continuous books settle anywhere");
    let priced = SettleMarket { price: 50, ..SettleMarket::refund(slot, market) };
    assert_eq!(priced.check().unwrap_err().field, "SettlementPrice");
    let perp = SettleMarket::down(MarketSlot::of::<1>(), market);
    assert_eq!(perp.check().unwrap_err().field, "MarketIndex");
}
