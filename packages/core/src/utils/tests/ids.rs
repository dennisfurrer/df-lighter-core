//! Each id refuses what lighter-go refuses, reads from numbers and strings, and never from a float.

use super::*;

#[test]
fn each_id_holds_only_its_range() {
    assert!(MarketId::new(0).is_ok() && MarketId::new(254).is_ok() && MarketId::new(256).is_ok());
    assert_eq!(MarketId::new(255), Err(BadId::Market(255)), "255 is no market");
    assert!(MarketId::new(WIDTH_48).is_ok() && MarketId::new(WIDTH_48 + 1).is_err());
    assert!(AccountIndex::new(MAX_ACCOUNT_INDEX).is_ok() && AccountIndex::new(MAX_ACCOUNT_INDEX + 1).is_err());
    assert!(ApiKeyIndex::new(254).is_ok() && ApiKeyIndex::new(255).is_err());
    assert!(AssetId::new(0).is_err() && AssetId::new(62).is_ok() && AssetId::new(63).is_err());
    assert!(ClientOrderIndex::new(0).is_err() && ClientOrderIndex::new(WIDTH_48).is_ok());
    assert!(OrderIndex::new(WIDTH_48).is_err() && OrderIndex::new(MIN_ORDER_INDEX).is_ok());
    assert!(OrderIndex::new(MAX_ORDER_INDEX).is_ok() && OrderIndex::new(MAX_ORDER_INDEX + 1).is_err());
}

#[test]
fn masters_count_up_and_pools_count_down() {
    assert!(AccountIndex::of::<10>().is_master());
    assert!(!AccountIndex::LLP.is_master());
    assert_eq!(AccountIndex::LLP.get(), 281_474_976_710_654);
    assert!(!AccountIndex::of::<281_474_976_624_800>().is_master(), "the LIT staking pool");
    assert_eq!(AssetId::COLLATERAL.get(), 3);
    assert_eq!(MarketId::of::<1>().get(), 1);
}

#[test]
fn an_order_is_named_by_either_index_by_its_range() {
    assert_eq!(OrderRef::from_index(7), Ok(OrderRef::Client(ClientOrderIndex::of::<7>())));
    let venue = OrderRef::from_index(26_177_173_101_967_466).unwrap();
    assert!(matches!(venue, OrderRef::Venue(_)));
    assert_eq!(venue.index(), 26_177_173_101_967_466, "past 2^53, exactly");
    assert!(OrderRef::from_index(0).is_err());
    assert!(OrderRef::from_index(MAX_ORDER_INDEX + 1).is_err());
}

#[test]
fn ids_read_from_numbers_and_digits_but_never_floats() {
    assert_eq!(serde_json::from_str::<OrderIndex>("562953635421737").unwrap().get(), 562_953_635_421_737);
    assert_eq!(serde_json::from_str::<OrderIndex>("\"26177173101967466\"").unwrap().get(), 26_177_173_101_967_466);
    assert!(serde_json::from_str::<MarketId>("1.0").is_err());
    assert!(serde_json::from_str::<MarketId>("-1").is_err());
    assert!(serde_json::from_str::<MarketId>("255").is_err());
    assert!(serde_json::from_str::<ApiKeyIndex>("300").is_err());
    assert_eq!(serde_json::to_string(&AccountIndex::LLP).unwrap(), "281474976710654");
    assert_eq!("  42 ".parse::<MarketId>().unwrap().get(), 42);
    assert_eq!("x".parse::<MarketId>(), Err(BadId::NotANumber("x".to_owned())));
    assert_eq!((Route::Perps.code(), Route::Spot.code()), (0, 1));
}
