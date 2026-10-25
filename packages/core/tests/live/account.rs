//! Every public account read, from both venues: an account whole by its index and by its owner,
//! the accounts an address owns, an account's API keys and next nonce, its PnL chart, the
//! leaderboards, an integrator's stats and a transaction. Nothing that needs a token; each answer
//! is held against its recording.

use df_lighter_core::Network::{self, Lighter, Robinhood};
use df_lighter_core::account::history::{
    Board, PnlChart, PnlQuery, RankBy, Resolution, Window, partner_stats, pnl_call, pnl_leaderboard,
    points_leaderboard, tx, tx_by_sequence,
};
use df_lighter_core::account::holdings::accounts;
use df_lighter_core::account::{
    AccountKind, Lookup, account, accounts_by_l1_address, api_key, api_keys, master_account, next_nonce,
};
use df_lighter_core::client::native::clock;
use df_lighter_core::client::{Call, read};
use df_lighter_core::utils::sign::TxType;
use df_lighter_core::utils::time::DAY_MS;
use df_lighter_core::utils::{AccountIndex, Address, ApiKeyIndex};

use crate::tap::{owner, reader, recording, same_shape, unused};

const TEN: AccountIndex = AccountIndex::of::<10>();

/// A BTC market order on Lighter, the one `tx.json` recorded.
const ORDER: &str = "b5b63eaa396afe74c0bc3e812f5c05ad4f55b3b80e66c4284dea26f5351ed311320924c1e8514aaa";

#[tokio::test]
#[ignore = "live"]
async fn account_10_and_each_venue_s_llp_read_whole() {
    let client = reader(Lighter);
    let ten = account(&client, TEN).await.unwrap().unwrap();
    same_shape(&client, "lighter/account_10.json");
    assert_eq!((ten.index, ten.kind, ten.l1_address), (TEN, AccountKind::Master, owner()));
    let page = accounts(&client, Lookup::L1Address(owner()), false, None).await.unwrap();
    same_shape(&client, "lighter/account_10.json");
    assert!(
        page.accounts.iter().any(|one| one.index == TEN) && page.accounts.iter().all(|one| one.l1_address == owner())
    );

    for (network, recorded) in [(Lighter, "account_281474976710654.json"), (Robinhood, "account_llp.json")] {
        let client = reader(network);
        let llp = account(&client, AccountIndex::LLP).await.unwrap().unwrap();
        same_shape(&client, &recording(network, recorded));
        assert_eq!((llp.index, llp.kind, llp.l1_address), (AccountIndex::LLP, AccountKind::Pool(3), Address::ZERO));
    }
}

#[tokio::test]
#[ignore = "live"]
async fn an_owner_s_accounts_read_by_address_and_an_unused_address_owns_none() {
    let client = reader(Lighter);
    let owned = accounts_by_l1_address(&client, owner(), None).await.unwrap();
    same_shape(&client, "lighter/accountsByL1Address.json");
    assert_eq!((owned.l1_address, owned.master().map(|master| master.index)), (Some(owner()), Some(TEN)));
    assert!(owned.sub_accounts().all(|sub| sub.kind == AccountKind::SubAccount && sub.l1_address == owner()));
    assert_eq!(master_account(&client, owner()).await.unwrap().map(|master| master.index), Some(TEN));
    same_shape(&client, "lighter/accountsByL1Address.json");

    for network in Network::ALL {
        let client = reader(network);
        let none = accounts_by_l1_address(&client, unused(), None).await.unwrap();
        same_shape(&client, &recording(network, "accountsByL1Address_none.json"));
        assert!(none.is_empty(), "{network}: {none:?}");
        assert_eq!(master_account(&client, unused()).await, Ok(None), "{network}");
        same_shape(&client, &recording(network, "accountsByL1Address_none.json"));
    }
}

#[tokio::test]
#[ignore = "live"]
async fn account_10_s_keys_and_a_slot_s_next_nonce_read() {
    let client = reader(Lighter);
    let slots = api_keys(&client, TEN).await.unwrap();
    same_shape(&client, "lighter/apikeys_10.json");
    assert!(!slots.is_empty() && slots.iter().all(|slot| slot.account == TEN), "{slots:?}");
    assert!(slots.windows(2).all(|pair| pair[0].slot < pair[1].slot), "by slot");

    let slot = ApiKeyIndex::of::<4>();
    let next = next_nonce(&client, TEN, slot).await.unwrap();
    same_shape(&client, "lighter/nextNonce_10.json");
    let held = api_key(&client, TEN, slot).await.unwrap();
    same_shape(&client, "lighter/apikeys_10.json");
    assert!(held.as_ref().is_none_or(|held| held.slot == slot && held.nonce <= next), "{held:?} at {next}");
}

/// The crate asks for the chart with the token, as the docs list it among the private reads; the
/// venue answers an account's chart to anyone, so it is asked here without one.
#[tokio::test]
#[ignore = "live"]
async fn account_10_s_pnl_chart_reads_without_a_token() {
    let client = reader(Lighter);
    let now = clock();
    let query = PnlQuery {
        account: TEN,
        resolution: Resolution::Day,
        from_ms: now - 30 * DAY_MS,
        to_ms: now,
        count: 30,
        ignore_transfers: true,
    };
    let chart: PnlChart = read(&client, Call { auth: false, ..pnl_call(&query) }).await.unwrap();
    same_shape(&client, "lighter/pnl_10.json");
    assert_eq!(chart.resolution, "1d");
    assert!(!chart.pnl.is_empty() && chart.pnl.windows(2).all(|pair| pair[0].timestamp < pair[1].timestamp));
}

#[tokio::test]
#[ignore = "live"]
async fn the_leaderboards_an_integrator_s_stats_and_a_transaction_read() {
    let client = reader(Lighter);
    let board = pnl_leaderboard(&client, Window::Day, RankBy::Pnl, 3, 0).await.unwrap();
    same_shape(&client, "lighter/pnlLeaderboard_24h.json");
    assert!(board.entries.len() <= 3 && board.entries.windows(2).all(|pair| pair[0].rank < pair[1].rank));

    let stats = partner_stats(&client, AccountIndex::of::<713_888>(), None, None).await.unwrap();
    same_shape(&client, "lighter/partnerStats.json");
    assert!(!stats.total_fees_earned.is_negative() && !stats.total_volume.is_negative(), "{stats:?}");

    let order = tx(&client, ORDER).await.unwrap();
    same_shape(&client, "lighter/tx.json");
    assert_eq!((order.hash.as_str(), order.kind(), order.nonce), (ORDER, Some(TxType::CreateOrder), 459));
    // Executed when recorded (2), the venue's "pending, final state" (3) a day on.
    let settled = order.status.is_executed();
    assert!(settled && order.executed_at > 0 && order.info_json().is_ok(), "{order:?}");
    let again = tx_by_sequence(&client, order.sequence_index).await.unwrap();
    same_shape(&client, "lighter/tx.json");
    assert_eq!(again.hash, order.hash);

    let client = reader(Robinhood);
    let weekly = points_leaderboard(&client, &Board::Weekly, None).await.unwrap();
    same_shape(&client, "robinhood/leaderboard_weekly.json");
    assert!(weekly.iter().all(|entry| !entry.l1_address.is_zero()), "{weekly:?}");
}
