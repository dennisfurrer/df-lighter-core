//! Whole accounts as both venues wrote them, recorded: account 10 on Lighter (flat, with assets,
//! pool shares and an approved integrator) and the LLP on Robinhood Chain (open longs and shorts,
//! unified, a pool; trimmed to five positions and three daily returns). Every field reads, and the
//! values an app shows come out exact.

use super::*;
use crate::test_support::{Recorded, block_on};
use crate::utils::{MarginMode, Side};

const ACCOUNT_10: &str = include_str!("../../../../fixtures/lighter/account_10.json");
const LLP: &str = include_str!("../../../../fixtures/robinhood/account_llp.json");

fn only(text: &str) -> Account {
    let page: AccountPage = serde_json::from_str(text).unwrap();
    assert_eq!(page.total, 1);
    page.accounts.into_iter().next().unwrap()
}

fn market(id: u64) -> MarketId {
    MarketId::new(id).unwrap()
}

#[test]
fn a_recorded_lighter_account_reads_whole() {
    let account = only(ACCOUNT_10);
    assert_eq!((account.index.get(), account.kind, account.status), (10, AccountKind::Master, 0));
    assert_eq!(account.l1_address.to_hex(), "0x478686468057d9873e8718083d40d2927b12f91a");
    assert_eq!(
        (account.trading_mode, account.is_unified(), account.is_master()),
        (Some(TradingMode::Classic), false, true)
    );
    assert_eq!(
        (account.collateral.to_string(), account.free_collateral().to_string()),
        ("1945.003244".into(), "1945.003244".into())
    );
    assert_eq!((account.positions.len(), account.open_positions().count()), (48, 0));
    assert_eq!((account.bo_positions.len(), account.pending_unlocks.len()), (0, 0));
    assert_eq!((account.metadata.color.as_str(), account.pool_info.as_ref()), ("", None));
    assert_eq!((account.referral_points_percentage, account.can_rfq_market_ids.len()), (None, 0));
    assert!(account.can_invite && account.can_rfq && account.agent_enabled);
    assert_eq!(account.transaction_time, 1_789_404_078_899_134);

    let btc = account.position(market(1)).unwrap();
    assert_eq!((btc.symbol.as_str(), btc.margin_mode, btc.is_open()), ("BTC", MarginMode::Isolated, false));
    assert_eq!(account.open_position(market(1)), None, "a flat row is no open position");
    assert_eq!(account.asset(AssetId::LIT).map(|lit| lit.balance.to_string()).as_deref(), Some("524.21838586"));
    assert_eq!(account.spot_assets().count(), 2);
    assert_eq!(account.pool_share(AccountIndex::LLP).map(|share| share.shares), Some(1));
    let integrator = account.integrator(AccountIndex::new(281_474_976_496_476).unwrap()).unwrap();
    assert_eq!(integrator.name, "Lighter");

    let balance = account.perps_balance();
    assert_eq!(
        (balance.total, balance.available, balance.unrealized_pnl, balance.margin_used),
        (account.collateral, account.collateral, Dec::ZERO, Dec::ZERO)
    );
}

#[test]
fn a_recorded_robinhood_pool_reads_with_its_open_positions_and_terms() {
    let account = only(LLP);
    assert_eq!((account.index, account.kind, account.status), (AccountIndex::LLP, AccountKind::Pool(3), 1));
    assert!(account.l1_address.is_zero() && account.is_unified() && !account.is_master());
    assert_eq!(account.approved_integrators.len(), 0, "an absent list is empty");
    assert_eq!(account.referral_points_percentage.map(|share| share.to_string()).as_deref(), Some("10"));

    let open: Vec<&Position> = account.open_positions().collect();
    assert_eq!(open.len(), 5);
    let hype = account.open_position(market(2)).unwrap();
    assert_eq!(
        (hype.symbol.as_str(), hype.side(), hype.signed_size().to_string()),
        ("HYPE", Some(Side::Ask), "-529.212".to_owned())
    );
    assert_eq!(hype.liquidation().map(|price| price.to_string()).as_deref(), Some("3795.404887785106"));
    assert_eq!(account.open_position(market(0)).unwrap().liquidation(), None, "0 is none");

    let usdg = account.asset(AssetId::COLLATERAL).unwrap();
    assert!(usdg.backs_perps() && usdg.symbol == "USDG");
    assert_eq!(account.spot_assets().count(), 0, "a pool holds nothing in spot");

    let balance = account.perps_balance();
    assert_eq!(balance.total.to_string(), "2168866.8535329993");
    assert_eq!(balance.unrealized_pnl.to_string(), "44413.8987769993");
    assert_eq!(balance.margin_used.to_string(), "417017.0123599993");
    assert_eq!(account.cross_initial_margin_requirement.to_string(), "417017.01236");

    let pool = account.pool_info.unwrap();
    assert_eq!(
        (pool.status, pool.total_shares, pool.operator_shares),
        (crate::earn::PoolStatus::Active, 1_021_267_925, 100_000)
    );
    assert_eq!(pool.annual_percentage_yield.as_str(), "176.09640232288734");
}

#[test]
fn an_account_is_read_by_index_in_public_and_none_when_the_venue_knows_none() {
    let transport = Recorded::answering(ACCOUNT_10);
    let ten = block_on(account(&transport, AccountIndex::new(10).unwrap())).unwrap().unwrap();
    assert_eq!(ten.index.get(), 10);
    let call = transport.only();
    assert_eq!((call.path, call.auth), ("account", false));
    assert_eq!(call.fields, [("by", "index".to_owned()), ("value", "10".to_owned())]);

    let none = Recorded::answering(include_str!("../../../../fixtures/lighter/accountsByL1Address_none.json"));
    assert_eq!(block_on(account(&none, AccountIndex::new(99).unwrap())), Ok(None));
}

#[test]
fn an_owner_s_accounts_are_asked_by_address_with_only_the_active_rows() {
    let owner: Address = "0x478686468057D9873E8718083D40d2927b12F91A".parse().unwrap();
    let call = account_call(owner.into(), true, Some("c1"));
    assert_eq!(
        call.fields,
        [
            ("by", "l1_address".to_owned()),
            ("value", "0x478686468057d9873e8718083d40d2927b12f91a".to_owned()),
            ("active_only", "true".to_owned()),
            ("cursor", "c1".to_owned()),
        ]
    );
    let transport = Recorded::answering(r#"{"code":21100,"message":"account not found"}"#);
    let page = block_on(accounts(&transport, Lookup::L1Address(owner), false, None)).unwrap();
    assert_eq!((page.accounts.len(), page.total, page.next_cursor), (0, 0, None));
}

#[test]
fn kinds_and_binary_options_positions_read_as_documented() {
    assert_eq!(
        [0, 1, 2, 3].map(AccountKind::from_code),
        [AccountKind::Master, AccountKind::SubAccount, AccountKind::Pool(2), AccountKind::Pool(3)]
    );
    assert_eq!(AccountKind::Pool(3).code(), 3);
    let position: BoPosition = serde_json::from_str(
        r#"{"market_id":4206,"sign":-1,"size":"100.0000","entry_quote":"51.000000","avg_entry_price":"0.51"}"#,
    )
    .unwrap();
    assert_eq!(
        (position.market_id.get(), position.is_up(), position.size.to_string()),
        (4206, false, "100".to_owned())
    );
    let unlock: PendingUnlock =
        serde_json::from_str(r#"{"unlock_timestamp":1790000000000,"asset_index":2,"amount":"12.5"}"#).unwrap();
    assert_eq!((unlock.asset, unlock.amount.to_string()), (AssetId::LIT, "12.5".to_owned()));
}
