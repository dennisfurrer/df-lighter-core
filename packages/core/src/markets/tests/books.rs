//! Both networks' listings and details, recorded on 2026-10-25: every row read, each market found
//! by id and by symbol, and the rules a market's settings make.

use super::*;
use crate::test_support::{Recorded, block_on};

const LIGHTER_BOOKS: &str = include_str!("../../../fixtures/lighter/orderBooks.json");
const ROBINHOOD_BOOKS: &str = include_str!("../../../fixtures/robinhood/orderBooks.json");
const LIGHTER_DETAILS: &str = include_str!("../../../fixtures/lighter/orderBookDetails.json");
const ROBINHOOD_DETAILS: &str = include_str!("../../../fixtures/robinhood/orderBookDetails.json");

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn id(value: u64) -> MarketId {
    MarketId::new(value).unwrap()
}

fn markets(text: &str) -> Markets {
    block_on(order_book_details(&Recorded::answering(text), MarketFilter::All)).unwrap()
}

#[test]
fn every_listed_market_reads_on_both_networks() {
    let transport = Recorded::answering(LIGHTER_BOOKS);
    let lighter = block_on(order_books(&transport, MarketFilter::All)).unwrap();
    let call = transport.only();
    assert_eq!((call.url_path(), call.auth), ("/api/v1/orderBooks".to_owned(), false));
    assert_eq!(call.fields, [("filter", "all".to_owned())]);
    assert_eq!(lighter.len(), 246);
    let count = |rows: &[MarketListing], kind| rows.iter().filter(|row| row.market_type == Some(kind)).count();
    assert_eq!((count(&lighter, MarketKind::Perps), count(&lighter, MarketKind::Spot)), (235, 11));
    let tao = &lighter[0];
    assert_eq!((tao.symbol.as_str(), tao.market_id, tao.status), ("TAO", id(13), MarketStatus::Active));
    assert_eq!((tao.base_asset_id, tao.quote_asset_id), (None, None), "a perp has no assets of its own");
    assert_eq!((tao.min_base_amount, tao.min_quote_amount), (dec("0.025"), dec("10")));
    assert_eq!((tao.grid().size_decimals(), tao.grid().price_decimals()), (3, 3));
    assert_eq!((tao.liquidation_fee, tao.created_at), (dec("1"), 1_737_098_461_338));
    assert_eq!((tao.open_interest_usd, tao.outcome), (Some(dec("1269102.585186")), Outcome::Undetermined));
    assert_eq!(tao.market_deployer.as_deref(), Some("lighter"));
    assert_eq!(lighter.iter().filter(|row| row.status == MarketStatus::Inactive).count(), 21);

    let robinhood = block_on(order_books(&Recorded::answering(ROBINHOOD_BOOKS), MarketFilter::All)).unwrap();
    assert_eq!((count(&robinhood, MarketKind::Perps), count(&robinhood, MarketKind::Spot)), (63, 27));
    let slv = robinhood.iter().find(|row| row.symbol == "SLV/USDG").unwrap();
    assert_eq!(
        (slv.market_id, slv.base_asset_id.map(AssetId::get), slv.quote_asset_id),
        (id(2067), Some(22), Some(AssetId::COLLATERAL))
    );
    assert!(robinhood.iter().all(|row| row.open_interest_usd.is_none() && row.market_deployer.is_none()));
}

#[test]
fn one_listing_is_asked_by_its_id() {
    let transport = Recorded::answering(LIGHTER_BOOKS);
    let tao = block_on(listing(&transport, id(13))).unwrap().unwrap();
    assert_eq!(tao.symbol, "TAO");
    assert_eq!(transport.only().fields, [("market_id", "13".to_owned())]);
    assert_eq!(block_on(listing(&transport, id(9_999))).unwrap(), None);
}

#[test]
fn details_read_every_perp_and_spot_pair_on_both_networks() {
    let transport = Recorded::answering(LIGHTER_DETAILS);
    let lighter = block_on(order_book_details(&transport, MarketFilter::Perps)).unwrap();
    let call = transport.only();
    assert_eq!(
        (call.url_path(), call.fields),
        ("/api/v1/orderBookDetails".to_owned(), vec![("filter", "perp".to_owned())])
    );
    assert_eq!((lighter.perps.len(), lighter.spot.len(), lighter.len()), (240, 12, 252));
    assert!(lighter.perps.iter().all(MarketDetail::is_perp) && lighter.spot.iter().all(MarketDetail::is_spot));

    let btc = lighter.get(id(1)).unwrap();
    assert_eq!((btc.symbol.as_str(), btc.grid().size_decimals(), btc.grid().price_decimals()), ("BTC", 5, 1));
    assert_eq!((btc.size_step(), btc.tick()), (dec("0.00001"), dec("0.1")));
    assert_eq!((btc.mark_price, btc.index_price.is_some()), (Some(dec("80465.4")), true));
    assert_eq!((btc.last_price(), btc.price()), (Some(dec("80469.4")), Some(dec("80469.4"))));
    assert_eq!(btc.daily_change(), Some(dec("-0.00159432049688")), "-0.159432...% as a fraction");
    assert_eq!((btc.daily_volume(), btc.open_interest_base()), (Some(dec("592900034.886")), Some(dec("2082.74744"))));
    assert_eq!(btc.daily_quote_token_volume.as_str(), "592900034.886242", "the float as written");
    assert_eq!((btc.open_interest_usd, btc.max_leverage()), (Some(dec("335178211.717152")), Some(50)));
    assert_eq!((btc.min_initial_margin_fraction, btc.funding_premium_multiplier), (Some(200), Some(100)));
    let config = btc.market_config.as_ref().unwrap();
    assert_eq!((config.insurance_fund(), config.rfq_enabled), (None, true), "2^48 - 1 is no account");
    let dram = lighter.by_symbol("DRAM").and_then(|dram| dram.market_config.as_ref()).unwrap();
    assert_eq!(dram.insurance_fund(), Some(AccountIndex::LLP));
    assert!(btc.is_listed() && !btc.is_close_only() && !btc.is_isolated_only());

    let pair = lighter.by_symbol("ETH/USDC").unwrap();
    assert_eq!(
        (pair.market_id, pair.base_asset_id, pair.quote_asset_id),
        (id(2048), Some(AssetId::ETH), Some(AssetId::COLLATERAL))
    );
    assert_eq!((pair.mark_price, pair.market_config.as_ref(), pair.max_leverage()), (None, None, None));
    assert_eq!(pair.open_interest.as_str(), "0.000000", "a spot pair writes a string");

    let robinhood = markets(ROBINHOOD_DETAILS);
    assert_eq!((robinhood.perps.len(), robinhood.spot.len()), (63, 27));
    let btc = robinhood.get(id(1)).unwrap();
    assert_eq!((btc.mark_price, btc.open_interest_usd, btc.open_interest_limit), (Some(dec("80455.7")), None, None));
    let config = btc.market_config.as_ref().unwrap();
    assert_eq!((config.insurance_fund_account_index, config.insurance_fund()), ((1 << 48) - 1, None));
    let apple = robinhood.by_symbol("AAPL/USDG").unwrap();
    assert_eq!((apple.market_id, apple.multiplier), (id(2049), dec("1.000566080061092436")));
    assert_eq!((apple.size_decimals, apple.price_decimals, apple.last_price()), (4, 2, Some(dec("347.68"))));
}

#[test]
fn a_market_is_found_by_id_or_symbol_and_its_kind_is_never_its_id() {
    let lighter = markets(LIGHTER_DETAILS);
    let robinhood = markets(ROBINHOOD_DETAILS);
    let stonk = lighter.get(id(4095)).unwrap();
    assert_eq!((stonk.symbol.as_str(), stonk.is_perp()), ("STONK", true), "a perp at or past 2048");
    assert_eq!(lighter.by_symbol("UNI").map(|uni| uni.market_id), Some(id(30)));
    assert_eq!(robinhood.by_symbol("UNI").map(|uni| uni.market_id), Some(id(4095)), "each network's own ids");
    assert_eq!(lighter.by_symbol("weeth/usdc").map(|pair| pair.symbol.as_str()), Some("weETH/USDC"));
    assert_eq!(
        (lighter.get(id(0)).map(|eth| eth.symbol.as_str()), robinhood.get(id(0)).map(|eth| eth.symbol.as_str())),
        (Some("ETH"), Some("ETH"))
    );
    assert_eq!((lighter.get(id(5212)), lighter.by_symbol("DOGE/USDC")), (None, None), "no binary option, no such pair");

    let pepe = lighter.by_symbol("1000PEPE").unwrap();
    assert_eq!(pepe.underlying(), ("PEPE", dec("1000")));
    assert_eq!((pepe.size_step(), pepe.tick()), (dec("1"), dec("0.000001")));
}

#[test]
fn a_market_s_settings_say_whether_and_how_it_trades() {
    let lighter = markets(LIGHTER_DETAILS);
    let aztec = lighter.by_symbol("AZTEC/USDC").unwrap();
    assert_eq!(aztec.status, MarketStatus::Inactive);
    assert!(!aztec.is_listed() && aztec.is_close_only(), "inactive takes no new exposure");
    let avgo = lighter.by_symbol("AVGO").unwrap();
    assert!(avgo.is_listed() && avgo.force_reduce_only() && avgo.is_close_only());
    let h100 = lighter.by_symbol("H100").unwrap();
    assert!(h100.is_isolated_only() && !h100.is_close_only());
    assert_eq!(h100.max_leverage(), Some(10));
    assert_eq!(lighter.by_symbol("MKR").unwrap().max_leverage(), None, "no margin fraction, no leverage");
    assert_eq!(lighter.by_symbol("UNI").unwrap().max_leverage(), Some(15), "666 is 15x, rounded down");
    assert_eq!(lighter.by_symbol("AVGO").unwrap().last_price(), None, "no trade yet");
    assert_eq!(lighter.by_symbol("AVGO").unwrap().price(), Some(dec("322.29")), "the mark then");
    assert_eq!(lighter.listed().count(), lighter.iter().filter(|detail| detail.is_active()).count());
}

#[test]
fn a_null_spot_list_is_none_and_another_answer_replaces_by_id() {
    let empty: Markets =
        serde_json::from_str(r#"{"code":200,"order_book_details":[],"spot_order_book_details":null}"#).unwrap();
    assert!(empty.is_empty());
    let mut lighter = markets(LIGHTER_DETAILS);
    let mut btc = lighter.get(id(1)).unwrap().clone();
    btc.status = MarketStatus::Inactive;
    lighter.extend(Markets { perps: vec![btc], spot: Vec::new() });
    assert_eq!((lighter.len(), lighter.get(id(1)).map(|btc| btc.status)), (252, Some(MarketStatus::Inactive)));
}

#[test]
fn one_market_s_details_are_asked_by_its_id() {
    let transport = Recorded::answering(ROBINHOOD_DETAILS);
    let apple = block_on(market_detail(&transport, id(2049))).unwrap().unwrap();
    assert_eq!(apple.symbol, "AAPL/USDG");
    assert_eq!(transport.only().fields, [("market_id", "2049".to_owned())]);
    assert_eq!(block_on(market_detail(&transport, id(9_999))).unwrap(), None);
    assert_eq!(order_books_call(MarketFilter::Spot).fields, [("filter", "spot".to_owned())]);
    assert_eq!(listing_call(id(5212)).fields, [("market_id", "5212".to_owned())]);
}

#[test]
fn a_symbol_of_a_thousand_names_a_thousand_units() {
    for (symbol, base, units) in [
        ("1000PEPE", "PEPE", "1000"),
        ("1000BONK", "BONK", "1000"),
        ("0G", "0G", "1"),
        ("2Z", "2Z", "1"),
        ("1000", "1000", "1"),
        ("1000X1", "1000X1", "1"),
        ("BTC", "BTC", "1"),
    ] {
        assert_eq!(underlying(symbol), (base, dec(units)), "{symbol}");
    }
}
