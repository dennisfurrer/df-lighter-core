//! Slots by range: what each holds, and the gaps between them refused.

use super::*;

#[test]
fn each_range_holds_one_kind_and_the_gaps_hold_none() {
    for (slot, kind) in [
        (0, MarketKind::Perps),
        (254, MarketKind::Perps),
        (1000, MarketKind::BinaryOptions),
        (2000, MarketKind::BinaryOptions),
        (2048, MarketKind::Spot),
        (4094, MarketKind::Spot),
    ] {
        assert_eq!(MarketSlot::new(slot).map(MarketSlot::kind), Ok(kind), "{slot}");
    }
    for gap in [255, 999, 2001, 2047, 4095, u16::MAX] {
        assert_eq!(MarketSlot::new(gap).unwrap_err().field, "MarketIndex", "{gap}");
    }
    assert_eq!(MarketSlot::of::<1500>().get(), 1500);
    assert!(PERPS_SLOTS.contains(&254) && BINARY_OPTIONS_SLOTS.contains(&1000) && SPOT_SLOTS.contains(&2048));
}

#[test]
fn create_and_settle_take_binary_options_slots_only() {
    assert_eq!(binary_options_slot(MarketSlot::of::<1500>()), Ok(()));
    assert!(binary_options_slot(MarketSlot::of::<1>()).is_err());
    assert!(binary_options_slot(MarketSlot::of::<2048>()).is_err());
}
