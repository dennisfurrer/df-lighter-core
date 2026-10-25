//! Every recorded frame of both venues reads, each into its channel's type with the market or
//! account its channel names; the private channels read as the docs write them; what is not a
//! frame says so.

use super::*;
use crate::client::Network;
use crate::live::fixtures::{FUNDING, ORDER, POSITION, TRADE, answer, answers, every, lighter, robinhood};
use crate::live::frame::codes;
use crate::live::{BoStatus, NoteContent};
use crate::markets::TradeKind;
use crate::utils::AssetId;

fn data(text: &str) -> Data {
    match decode(text).unwrap() {
        Frame::Snapshot(data) | Frame::Update(data) => data,
        other => panic!("expected a channel's frame, got {other:?}"),
    }
}

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn market(id: u64) -> MarketId {
    MarketId::new(id).unwrap()
}

fn account(index: u64) -> AccountIndex {
    AccountIndex::new(index).unwrap()
}

/// Nothing either venue sent is unreadable or unknown: every channel and every control frame has
/// its type.
#[test]
fn every_recorded_frame_reads_into_its_type() {
    for network in Network::ALL {
        for line in every(network) {
            let frame = decode(line).unwrap_or_else(|bad| panic!("{network}: {bad}"));
            assert!(!matches!(frame, Frame::Other { .. }), "{network}: {line:.120}");
            assert!(!matches!(frame.data(), Some(Data::Other { .. })), "{network}: {line:.120}");
        }
    }
    for line in answers() {
        assert!(!matches!(decode(line).unwrap(), Frame::Other { .. }), "{line}");
    }
}

#[test]
fn a_book_reads_whole_on_subscribe_and_its_changes_after() {
    let Data::OrderBook(book) = data(lighter("subscribed/order_book", "order_book:1")) else { panic!() };
    assert_eq!((book.market, book.nonce, book.begin_nonce), (market(1), 25_652_761_941, 0));
    assert_eq!((book.asks.len(), book.bids.len()), (1658, 1543));
    assert_eq!(book.asks[0], Level { price: dec("80441.7"), size: dec("0.12293") });
    assert_eq!(book.bids[0], Level { price: dec("80441.6"), size: dec("0.0002") });
    assert_eq!(
        (book.offset, book.last_updated_at, book.timestamp),
        (37_354_955, 1_792_918_164_414_954, 1_792_918_164_485)
    );
    let Data::OrderBook(update) = data(lighter("update/order_book", "order_book:1")) else { panic!() };
    assert_eq!((update.begin_nonce, update.nonce), (25_652_761_941, 25_652_762_001), "it follows the snapshot");
    assert!(update.bids.iter().any(|level| level.size.is_zero()), "a level gone");
    assert!(matches!(decode(robinhood("subscribed/order_book", "")).unwrap(), Frame::Snapshot(Data::OrderBook(_))));
}

#[test]
fn a_ticker_reads_its_best_bid_and_offer() {
    let Data::Ticker(ticker) = data(lighter("subscribed/ticker", "")) else { panic!() };
    assert_eq!((ticker.market, ticker.symbol.as_str(), ticker.nonce), (market(1), "BTC", 25_652_761_995));
    assert_eq!(ticker.ask, Some(Level { price: dec("80441.7"), size: dec("0.12232") }));
    assert_eq!(ticker.bid.map(|bid| bid.price), Some(dec("80441.6")));
    assert_eq!(ticker.mid(), Some(dec("80441.65")));
    let empty = r#"{"channel":"ticker:2049","nonce":1,"ticker":{"s":"AAPL/USDG","a":{"price":"","size":""},"b":null},"timestamp":1,"type":"update/ticker"}"#;
    let Data::Ticker(ticker) = data(empty) else { panic!() };
    assert_eq!((ticker.ask, ticker.bid, ticker.mid()), (None, None, None), "a side with nothing resting");
}

/// The `all` channel's map and one market's object read alike; prices left blank read as none;
/// volumes are exact as written.
#[test]
fn market_figures_read_from_one_market_or_all() {
    let Data::MarketStats(one) = data(lighter("subscribed/market_stats", "")) else { panic!() };
    let btc = &one[0];
    assert_eq!((one.len(), btc.market_id, btc.symbol.as_str()), (1, market(1), "BTC"));
    assert_eq!(
        (btc.mark_price, btc.mid_price, btc.open_interest_usd),
        (dec("80439.4"), Some(dec("80441.7")), Some(dec("335034131.01394")))
    );
    assert_eq!(btc.daily_quote_token_volume.as_str(), "593384341.834102");
    assert_eq!(btc.funding_timestamp, 1_792_915_200_002);
    let Data::MarketStats(all) = data(robinhood("subscribed/market_stats", "market_stats:all")) else { panic!() };
    assert!(all.len() > 20, "every perp: {}", all.len());
    assert_eq!(all[0].symbol, "ETH");
    let Data::SpotMarketStats(spot) = data(robinhood("subscribed/spot_market_stats", "")) else { panic!() };
    let eth = spot.iter().find(|each| each.market_id == market(2048)).unwrap();
    assert_eq!((eth.symbol.as_str(), eth.mid_price, eth.best_bid_price), ("ETH/USDG", None, Some(dec("0.88"))));
    let Data::SpotMarketStats(one) = data(lighter("subscribed/spot_market_stats", "")) else { panic!() };
    assert_eq!((one.len(), one[0].daily_price_change.as_str()), (1, "-0.8427836911802793"));
}

#[test]
fn binary_options_read_one_or_all_with_their_status() {
    let Data::BoMarketStats(all) = data(lighter("subscribed/bo_market_stats", "bo_market_stats:all")) else { panic!() };
    let settling = all.iter().find(|each| each.market_id == market(5210)).unwrap();
    assert_eq!((settling.status, settling.outcome, settling.mid_price), (BoStatus::InSettlement, 2, None));
    assert!(all.iter().any(|each| each.status == BoStatus::Active));
    let Data::BoMarketStats(one) = data(answer("bo_market_stats:5218")) else { panic!() };
    assert_eq!((one.len(), one[0].market_id, one[0].status), (1, market(5218), BoStatus::Active));
    assert_eq!((one[0].best_bid_price, one[0].open_interest), (Some(dec("0.01")), Some(dec("52.4046"))));
}

/// Trades are the rows every read writes, liquidations apart; the snapshot is the last fifty of
/// each.
#[test]
fn trades_read_as_the_shared_rows() {
    let Data::Trades(snapshot) = data(lighter("subscribed/trade", "")) else { panic!() };
    assert_eq!((snapshot.market, snapshot.trades.len(), snapshot.liquidations.len()), (market(1), 50, 50));
    assert!(snapshot.liquidations.iter().all(|trade| trade.kind == TradeKind::Liquidation));
    assert_eq!(snapshot.all().count(), 100);
    let Data::Trades(update) = data(lighter("update/trade", "")) else { panic!() };
    let first = &update.trades[0];
    assert_eq!((update.nonce, first.trade_id, first.price), (25_652_762_377, 36_388_690_000, dec("80440.9")));
    assert_eq!(
        (first.ask_account_id, first.bid_account_id, first.is_maker_ask),
        (account(734_724), account(27_927), false)
    );
    let Data::Trades(robinhood) = data(robinhood("update/trade", "")) else { panic!() };
    assert_eq!(robinhood.trades[0].market_symbol, "BTC");
}

#[test]
fn candles_read_with_their_market_and_resolution() {
    let Data::Candles(candles) = data(lighter("subscribed/candle", "")) else { panic!() };
    assert_eq!(
        (candles.market, candles.resolution, candles.timestamp),
        (market(1), Resolution::OneMinute, 1_792_918_164_498)
    );
    let candle = &candles.candles[0];
    assert_eq!(
        (candle.open_time, candle.open.as_ref().map(Figure::as_str), candle.volume.as_ref().map(Figure::as_str)),
        (1_792_918_140_000, Some("80473.1"), Some("1.376149999999998"))
    );
    let quote = candle.quote_volume.as_ref().map(Figure::as_str);
    assert_eq!((quote, candle.last_trade_id), (Some("110707.29037900003"), 36_388_689_014));
    let Data::MarkPriceCandles(marks) = data(lighter("subscribed/mark_price_candle", "")) else { panic!() };
    assert_eq!((marks.candles[0].open.as_str(), marks.candles[0].samples), ("80471", 2532));
    let Data::UpDownCandles(series) = data(lighter("subscribed/up_down_candle", "")) else { panic!() };
    assert_eq!(
        (series.symbol.as_str(), series.duration.as_str(), series.resolution),
        ("btc", "15m", UpDownResolution::Second)
    );
    assert_eq!(series.candles.len(), 300, "the last 300");
    assert_eq!(series.candles[0].open, dec("80395.147736182522118144"), "eighteen places, exact");
    assert_eq!(series.candles[0].provider, "chainlink");
}

#[test]
fn the_height_reads() {
    assert_eq!(
        data(lighter("subscribed/height", "")),
        Data::Height(Height { height: 365_159_557, timestamp: 1_792_918_165_000 })
    );
}

/// `account_all`, `account_all_positions`, `account_all_trades` and `user_stats` are public,
/// recorded for account 10.
#[test]
fn an_account_s_public_channels_read() {
    let Data::AccountAll(all) = data(lighter("subscribed/account_all", "")) else { panic!() };
    assert_eq!((all.account, all.positions.len(), all.assets.len(), all.shares.len()), (account(10), 48, 2, 2));
    assert_eq!(all.assets[1].asset_id, AssetId::COLLATERAL);
    assert_eq!(all.assets[1].margin_balance, dec("1945.003244157056"));
    assert_eq!(all.positions[0].symbol, "ETH");
    assert!(all.trades.is_empty() && all.fundings.is_empty() && all.bo_positions.is_empty());
    let volumes = all.volumes.unwrap();
    assert_eq!((volumes.total.as_str(), volumes.total_trades), ("2935377.596694", Some(2476)));
    let Data::AccountAllPositions(positions) = data(lighter("subscribed/account_all_positions", "")) else { panic!() };
    assert_eq!(
        (positions.account, positions.positions.len(), positions.transaction_time),
        (account(10), 48, 1_789_404_078_899_134)
    );
    let Data::AccountAllTrades(trades) = data(lighter("subscribed/account_all_trades", "")) else { panic!() };
    assert_eq!((trades.trades.len(), trades.volumes.map(|volumes| volumes.daily_trades)), (0, Some(None)));
    let Data::UserStats(stats) = data(lighter("subscribed/user_stats", "")) else { panic!() };
    assert_eq!((stats.account, stats.stats.collateral, stats.trading_mode), (account(10), dec("1945.003244"), 0));
    assert_eq!(stats.cross.map(|cross| cross.available_balance), Some(dec("1945.003244")));
}

/// An update's changes come as maps by market, a position, one market's trades; `assets` may be
/// `null`.
#[test]
fn an_account_update_reads_what_changed() {
    let text = format!(
        r#"{{"account":10,"assets":null,"channel":"account_all:10","positions":{{"1":{POSITION}}},
        "trades":{{"0":[{TRADE}]}},"funding_histories":{{"0":[{FUNDING}]}},"shares":[],
        "transaction_time":1789404078899135,"type":"update/account_all"}}"#
    );
    let Data::AccountAll(all) = data(&text) else { panic!() };
    assert!(all.assets.is_empty() && all.volumes.is_none());
    assert_eq!(
        (all.positions[0].market_id, all.trades[0].trade_id, all.fundings[0].funding_id),
        (market(1), 16_164_557_907, 2001)
    );
}

/// The docs' structures for what needs the token: orders by market, one market's account (its
/// position one object or `null`), transactions, balances missing their margin fields, average
/// entry prices, a pool, notifications, quotes, points.
#[test]
fn the_private_channels_read_as_the_docs_write_them() {
    let text = format!(
        r#"{{"channel":"account_all_orders:10","orders":{{"1":[{ORDER}]}},"type":"update/account_all_orders"}}"#
    );
    let Data::AccountAllOrders(all) = data(&text) else { panic!() };
    assert_eq!((all.account, all.orders[0].order_index), (account(10), 281_476_612_587_355));

    let text = format!(
        r#"{{"account":10,"channel":"account_orders:1","nonce":9,"orders":{{"1":[{ORDER}]}},"type":"subscribed/account_orders"}}"#
    );
    let Data::AccountOrders(one) = data(&text) else { panic!() };
    assert_eq!((one.market, one.account, one.nonce, one.orders.len()), (market(1), account(10), 9, 1));

    let text = format!(
        r#"{{"account":10,"assets":null,"channel":"account_market:1:10","funding_history":null,"orders":[{ORDER}],
        "position":{POSITION},"bo_position":null,"trades":[{TRADE}],"type":"subscribed/account_market"}}"#
    );
    let Data::AccountMarket(one) = data(&text) else { panic!() };
    assert_eq!(
        (one.market, one.account, one.position.as_ref().map(|position| position.sign)),
        (market(1), account(10), Some(1))
    );
    assert!(
        one.assets.is_empty() && one.fundings.is_empty() && one.bo_position.is_none() && one.transaction_time.is_none()
    );

    let tx = r#"{"hash":"0xabc123456789def","type":15,"info":"{\"AccountIndex\":1,\"Nonce\":1234}","event_info":"{\"a\":1}",
        "status":2,"transaction_index":10,"l1_address":"0x123abc456def789","account_index":101,"nonce":12345,
        "expire_at":1700000000000,"block_height":1500000,"queued_at":1699999990000,"executed_at":1700000000005,
        "sequence_index":5678,"parent_hash":"0xparenthash123456","api_key_index":2,"transaction_time":1700000000005000}"#;
    let Data::AccountTx(txs) =
        data(&format!(r#"{{"channel":"account_tx:101","txs":[{tx}],"type":"update/account_tx"}}"#))
    else {
        panic!()
    };
    assert_eq!((txs.account, txs.txs[0].tx_type, txs.txs[0].nonce), (account(101), 15, 12_345));
    assert_eq!(txs.txs[0].info, r#"{"AccountIndex":1,"Nonce":1234}"#);

    let text = r#"{"assets":{"1":{"symbol":"ETH","asset_id":1,"balance":"7.1072","locked_balance":"0.0000"},
        "3":{"symbol":"USDC","asset_id":3,"balance":"6343.581906","locked_balance":"297.000000"}},
        "channel":"account_all_assets:1234","timestamp":1791407336316,"transaction_time":1791406944316056,
        "type":"update/account_all_assets"}"#;
    let Data::AccountAllAssets(assets) = data(text) else { panic!() };
    assert_eq!((assets.account, assets.assets.len()), (account(1234), 2));
    assert_eq!(
        (assets.assets[1].free(), assets.assets[1].margin_balance, assets.assets[1].backs_perps()),
        (dec("6046.581906"), Dec::ZERO, false)
    );

    let text = r#"{"avg_entry_prices":{"1":{"asset_id":1,"avg_entry_price":"1850.45","asset_size":"0.01234567",
        "last_trade_id":13472591098}},"channel":"account_spot_avg_entry_prices:1234","timestamp":1773158679717,
        "type":"subscribed/account_spot_avg_entry_prices"}"#;
    let Data::AvgEntryPrices(prices) = data(text) else { panic!() };
    assert_eq!((prices.prices[0].asset_id, prices.prices[0].avg_entry_price), (AssetId::ETH, dec("1850.45")));

    let text = format!(
        r#"{{"channel":"pool_data:7","account":7,"trades":{{"0":[{TRADE}]}},"orders":{{"1":[{ORDER}]}},
        "positions":{{"1":[{POSITION}]}},"shares":[],"funding_histories":{{}},"type":"subscribed/pool_data"}}"#
    );
    let Data::PoolData(pool) = data(&text) else { panic!() };
    assert_eq!((pool.account, pool.trades.len(), pool.orders.len(), pool.positions.len()), (account(7), 1, 1, 1));

    let text = r#"{"channel":"pool_info:7","pool_info":{"status":0,"operator_fee":"10","min_operator_share_rate":"5",
        "total_shares":1000,"operator_shares":100,"annual_percentage_yield":30.733724772746875,"sharpe_ratio":1.5,
        "daily_returns":[{"timestamp":1773100000,"daily_return":0.0012}],"share_prices":[{"timestamp":1773100000,
        "share_price":1.0123}],"strategies":[{"collateral":"1000.5"},null]},"type":"subscribed/pool_info"}"#;
    let Data::PoolInfo { account: pool, info } = data(text) else { panic!() };
    assert_eq!(
        (pool, info.annual_percentage_yield.as_str(), info.strategies[0].as_ref().map(|strategy| strategy.collateral)),
        (account(7), "30.733724772746875", Some(dec("1000.5")))
    );
    assert_eq!(info.strategies[1], None, "a slot no strategy fills, as Robinhood Chain writes it");
}

#[test]
fn notifications_read_by_kind_and_keep_what_they_do_not_know() {
    let text = r#"{"channel":"notification:12345","notifs":[
        {"id":"notif_123","created_at":"2024-01-15T10:30:00Z","updated_at":"2024-01-15T10:30:00Z","kind":"liquidation",
         "account_index":12345,"content":{"id":"notif_123","is_ask":false,"usdc_amount":"1500.50","size":"0.500000",
         "market_index":1,"price":"3000.00","timestamp":1705312200,"avg_price":"3000.00"},"ack":false,"acked_at":null},
        {"id":"notif_124","created_at":"2024-01-15T11:00:00Z","updated_at":"2024-01-15T11:00:00Z","kind":"deleverage",
         "account_index":12345,"content":{"id":"notif_124","usdc_amount":"500.25","size":"0.200000","market_index":1,
         "settlement_price":"2501.25","timestamp":1705314000},"ack":false,"acked_at":null},
        {"id":"notif_125","created_at":"","updated_at":"","kind":"announcement","account_index":12345,
         "content":{"title":"Hello","content":"New markets","created_at":1705314000},"ack":true,"acked_at":"2024-01-15T12:00:00Z"},
        {"id":"notif_126","created_at":"","updated_at":"","kind":"something_new","account_index":12345,
         "content":{"x":1},"ack":false,"acked_at":null}],"type":"update/notification"}"#;
    let Data::Notifications(notes) = data(text) else { panic!() };
    assert_eq!(notes.account, account(12_345));
    let [liquidation, deleverage, announcement, other] = notes.notes.as_slice() else { panic!() };
    assert!(matches!(&liquidation.content, NoteContent::Liquidation(note) if note.usdc_amount == dec("1500.5")));
    assert!(matches!(&deleverage.content, NoteContent::Deleverage(note) if note.settlement_price == dec("2501.25")));
    assert!(matches!(&announcement.content, NoteContent::Announcement(note) if note.title == "Hello"));
    assert_eq!(other.content, NoteContent::Other(r#"{"x":1}"#.to_owned()));
    assert_eq!(announcement.acked_at.as_deref(), Some("2024-01-15T12:00:00Z"));
}

#[test]
fn quotes_and_points_read() {
    let text = r#"{"channel":"rfq","rfqs":[{"id":1234,"account_index":704885,"market_index":1,"direction":-1,
        "base_amount":"100","quote_amount":"","status":"opened","metadata":{},"responses":[],
        "created_at":1780077339117,"updated_at":1780077339117}],"type":"update/rfq"}"#;
    let Data::Rfqs(rfqs) = data(text) else { panic!() };
    assert_eq!(
        (rfqs[0].id, rfqs[0].direction, rfqs[0].base_amount, rfqs[0].quote_amount),
        (1234, -1, Some(dec("100")), None)
    );
    assert_eq!((rfqs[0].metadata.as_str(), rfqs[0].responses.as_str()), ("{}", "[]"));
    let text = r#"{"type":"update/live_points","channel":"live_points:1234","total_live_points":1234.56}"#;
    let Data::LivePoints(points) = data(text) else { panic!() };
    assert_eq!((points.account, points.total.as_str()), (account(1234), "1234.56"));
}

/// The socket's own frames, as the read-only socket answered them.
#[test]
fn the_socket_s_own_frames_read() {
    assert_eq!(
        decode(answer("connected")).unwrap(),
        Frame::Connected { session_id: "ef82e0e4-40e2-47fb-95fa-79bdacf29d1b".to_owned() }
    );
    assert_eq!(decode(answer("unsubscribed")).unwrap(), Frame::Unsubscribed { channel: "height".to_owned() });
    assert_eq!(decode(r#"{"type":"pong"}"#).unwrap(), Frame::Pong);
    assert_eq!(decode(r#"{"type":"ping"}"#).unwrap(), Frame::Ping);
    let Frame::Error(refused) = decode(answer("Already Subscribed")).unwrap() else { panic!() };
    assert_eq!((refused.code, refused.id), (codes::ALREADY_SUBSCRIBED, None));
    let Frame::Error(refused) = decode(answer("auth field is required")).unwrap() else { panic!() };
    assert_eq!(refused.code, codes::INVALID_PARAM);
}

/// A transaction's answer names the id it was sent with, at the top level (lighter-ts-sdk 1.1.0:
/// `{"code":200,"id":..,"tx_hash":..}`, a batch's hashes in a list, a refusal as an error with
/// the id).
#[test]
fn a_transaction_s_answer_reads_by_its_id() {
    let taken = r#"{"code":200,"id":"7","tx_hash":"b5b63eaa","predicted_execution_time_ms":1792918662523}"#;
    let Frame::Sent { id, sent } = decode(taken).unwrap() else { panic!() };
    assert_eq!(
        (id.as_str(), sent.tx_hash.as_str(), sent.predicted_execution_time_ms),
        ("7", "b5b63eaa", Some(1_792_918_662_523))
    );
    let batch = r#"{"code":200,"id":"8","tx_hash":["aa","bb"],"volume_quota_remaining":12}"#;
    let Frame::SentBatch { id, sent } = decode(batch).unwrap() else { panic!() };
    assert_eq!((id.as_str(), sent.tx_hashes.len(), sent.volume_quota_remaining), ("8", 2, Some(12)));
    let refused = r#"{"error":{"code":21120,"message":"invalid signature"},"id":"9"}"#;
    let frame = decode(refused).unwrap();
    assert_eq!(frame.answers(), Some("9"));
    assert!(matches!(frame, Frame::Error(Refusal { code: 21_120, .. })));
    let coded = r#"{"code":21104,"message":"invalid nonce","id":10}"#;
    assert_eq!(decode(coded).unwrap().answers(), Some("10"), "an id written as a number");
}

#[test]
fn what_is_not_a_frame_says_so_and_what_is_unknown_is_kept() {
    let bad = decode("Websocket connection established.").unwrap_err();
    assert_eq!(bad.channel, None);
    assert!(bad.to_string().starts_with("not a frame"), "{bad}");
    assert!(decode(r#"{"channel":"x"}"#).is_err(), "no type");
    let bad =
        decode(r#"{"channel":"order_book:1","order_book":{"asks":"no"},"type":"update/order_book"}"#).unwrap_err();
    assert_eq!(bad.channel.as_deref(), Some("order_book:1"));
    let shutdown = r#"{"type":"shutdown","close_in_ms":5000}"#;
    assert_eq!(decode(shutdown).unwrap(), Frame::Other { kind: "shutdown".to_owned(), text: shutdown.to_owned() });
    let unknown = r#"{"channel":"new_channel:1","x":1,"type":"update/new_channel"}"#;
    assert_eq!(data(unknown), Data::Other { channel: "new_channel:1".to_owned(), text: unknown.to_owned() });
}
