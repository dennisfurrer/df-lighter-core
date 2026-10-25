//! The [`Client`](df_lighter_core::Client) against both venues: warming its connection, the meter
//! after a few reads, a refusal read by the venue's code, and a private read that never leaves
//! without a token.

use df_lighter_core::Network;
use df_lighter_core::account::history::{PnlQuery, Resolution, pnl};
use df_lighter_core::account::holdings::accounts;
use df_lighter_core::account::{Lookup, account};
use df_lighter_core::client::http::codes;
use df_lighter_core::client::http::limit::{STANDARD_BUDGET, STANDARD_FLOOR};
use df_lighter_core::client::native::clock;
use df_lighter_core::client::{ApiError, TransportError};
use df_lighter_core::markets::{MetricKind, Period, exchange_metrics, recent_trades, system_config};
use df_lighter_core::utils::time::DAY_MS;
use df_lighter_core::utils::{AccountIndex, MarketId};

use crate::tap::{Tapped, reader, same_shape};

/// 2^47 - 1: no account sits there on either venue.
const NOBODY: AccountIndex = AccountIndex::of::<140_737_488_355_327>();

#[tokio::test]
#[ignore = "live"]
async fn warming_reads_the_venue_s_status_and_counts_as_one_request() {
    for network in Network::ALL {
        let client = reader(network);
        client.warm().await;
        let status = client.tap().last();
        assert_eq!(status["status"], 200, "{network}: {status}");
        assert!(status["timestamp"].as_u64().is_some_and(|at| at.abs_diff(clock() / 1_000) < 600), "{status}");
        let spent = client.spent();
        let kinds: Vec<_> = spent.kinds.iter().map(|kind| (kind.name.as_str(), kind.count, kind.weight)).collect();
        assert_eq!(kinds, [("/", 1, STANDARD_FLOOR)], "{network}");
        assert_eq!((spent.minute, spent.refused, spent.cooldown_ms), (STANDARD_FLOOR, 0, 0), "{network}");
    }
}

/// A Standard request costs the floor, or its weight when heavier: the tape weighs 600.
#[tokio::test]
#[ignore = "live"]
async fn the_meter_counts_each_read_at_its_cost_heaviest_first() {
    for network in Network::ALL {
        let client = reader(network);
        system_config(&client).await.unwrap();
        recent_trades(&client, MarketId::of::<1>(), 1).await.unwrap();
        exchange_metrics(&client, Period::Week, MetricKind::Volume, None).await.unwrap();
        system_config(&client).await.unwrap();
        let spent = client.spent();
        let kinds: Vec<_> = spent.kinds.iter().map(|kind| (kind.name.as_str(), kind.count, kind.weight)).collect();
        let floor = STANDARD_FLOOR;
        assert_eq!(kinds, [("systemConfig", 2, 2 * floor), ("recentTrades", 1, 600), ("exchangeMetrics", 1, floor)]);
        assert_eq!((spent.budget, spent.minute, spent.refused), (STANDARD_BUDGET, 3 * floor + 600, 0), "{network}");
        assert_eq!(client.tap().count(), 4);
    }
}

/// An index no account has is the venue's 29404, sent with an HTTP 400, however it is asked. A
/// read of the account's own, with no token to carry, is refused before it goes out.
#[tokio::test]
#[ignore = "live"]
async fn a_refusal_reads_by_its_code_and_a_private_read_never_leaves_without_a_token() {
    for network in Network::ALL {
        let client = reader(network);
        let refused = account(&client, NOBODY).await.unwrap_err();
        same_shape(&client, "lighter/account_none.json");
        assert_eq!(refused.code(), Some(codes::NOT_FOUND), "{network}: {refused:?}");
        assert!(!refused.is_rate_limited() && !refused.may_have_landed(), "{refused:?}");
        let refused = accounts(&client, Lookup::Index(NOBODY), true, None).await.unwrap_err();
        assert_eq!(refused.code(), Some(codes::NOT_FOUND), "{network}: {refused:?}");

        let sent = client.tap().count();
        let now = clock();
        let query = PnlQuery {
            account: AccountIndex::of::<10>(),
            resolution: Resolution::Day,
            from_ms: now - DAY_MS,
            to_ms: now,
            count: 1,
            ignore_transfers: false,
        };
        let error = pnl(&client, &query).await.unwrap_err();
        assert!(matches!(error, ApiError::Transport(TransportError::NoToken(_))), "{error:?}");
        assert_eq!(client.tap().count(), sent, "{network}: nothing went out");
    }
}
