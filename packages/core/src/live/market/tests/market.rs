//! The market frames' own arithmetic and names: a ticker's exact mid, a trade frame's rows
//! together, the resolutions a channel names.

use super::*;
use crate::live::fixtures::lighter;
use crate::live::{Data, Frame, decode};
use crate::utils::Dec;

#[test]
fn a_ticker_s_mid_is_exact() {
    let level = |price: &str| Some(Level { price: Dec::parse(price).unwrap(), size: Dec::ONE });
    let ticker = Ticker {
        market: MarketId::new(1).unwrap(),
        symbol: "BTC".to_owned(),
        ask: level("80441.7"),
        bid: level("80441.6"),
        nonce: 1,
        last_updated_at: 0,
        timestamp: 0,
    };
    assert_eq!(ticker.mid(), Dec::parse("80441.65").ok());
    assert_eq!(Ticker { ask: None, ..ticker }.mid(), None);
}

#[test]
fn a_trade_frame_gives_its_trades_then_its_liquidations() {
    let Ok(Frame::Snapshot(Data::Trades(trades))) = decode(lighter("subscribed/trade", "")) else { panic!() };
    let all: Vec<u64> = trades.all().map(|trade| trade.trade_id).collect();
    assert_eq!(all.first(), trades.trades.first().map(|trade| &trade.trade_id));
    assert_eq!(all.last(), trades.liquidations.last().map(|trade| &trade.trade_id));
}

#[test]
fn resolutions_are_named_as_the_channels_name_them() {
    for resolution in Resolution::ALL {
        assert_eq!(Resolution::parse(resolution.wire()), Some(resolution));
        assert_eq!(resolution.to_string(), resolution.wire());
    }
    assert_eq!(
        (Resolution::OneMinute.ms(), Resolution::FourHours.ms(), Resolution::OneDay.ms()),
        (60_000, 14_400_000, 86_400_000)
    );
    assert_eq!(Resolution::parse("2m"), None);
    assert_eq!(
        (UpDownResolution::parse("1s"), UpDownResolution::parse("30s")),
        (Some(UpDownResolution::Second), Some(UpDownResolution::ThirtySeconds))
    );
    assert_eq!(UpDownResolution::ThirtySeconds.to_string(), "30s");
}
