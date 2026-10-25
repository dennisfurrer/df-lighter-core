//! Pools as both venues list them (recorded 2026-10-25): the LLP and three operated pools on
//! Lighter, a frozen one and an empty one, the staking pool, Robinhood Chain's LLP; the LLP's
//! detail as `account` answers it (trimmed to two positions and three days of record); and what a
//! share is worth.

use super::*;
use crate::test_support::{Recorded, block_on};

const ALL: &str = include_str!("../../../../fixtures/lighter/publicPoolsMetadata_all.json");
const USER: &str = include_str!("../../../../fixtures/lighter/publicPoolsMetadata_user.json");
const STAKE: &str = include_str!("../../../../fixtures/lighter/publicPoolsMetadata_stake.json");
const LLP: &str = include_str!("../../../../fixtures/lighter/llp.json");
const RH_ALL: &str = include_str!("../../../../fixtures/robinhood/publicPoolsMetadata_all.json");
const LLP_ACCOUNT: &str = include_str!("../../../../fixtures/lighter/account_281474976710654.json");
const ACCOUNT_10: &str = include_str!("../../../../fixtures/lighter/account_10.json");
const NO_ACCOUNT: &str = include_str!("../../../../fixtures/lighter/account_none.json");
const FEE: &str = include_str!("../../../../fixtures/lighter/publicPoolsMetadata_fee.json");

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn index(value: u64) -> AccountIndex {
    AccountIndex::new(value).unwrap()
}

#[test]
fn a_page_lists_pools_from_the_llp_down_with_their_yield_as_written() {
    let transport = Recorded::answering(ALL);
    let page = PoolsPage::first(PoolFilter::All, 4);
    let pools = block_on(public_pools(&transport, &page)).unwrap();
    let call = transport.only();
    assert_eq!((call.url_path(), call.auth), ("/api/v1/publicPoolsMetadata".to_owned(), false));
    assert_eq!(
        call.fields,
        [("filter", "all".to_owned()), ("index", "281474976710655".to_owned()), ("limit", "4".to_owned())]
    );
    assert_eq!(pools.len(), 4);
    let llp = &pools[0];
    assert_eq!(
        (llp.pool, llp.kind(), llp.name.as_str()),
        (AccountIndex::LLP, PoolKind::Protocol, "Lighter Liquidity Provider (LLP)")
    );
    assert_eq!(llp.annual_percentage_yield.as_str(), "14.525245980602785");
    assert_eq!((llp.status, llp.operator_fee, llp.total_shares), (PoolStatus::Active, Dec::ZERO, 26_574_874_858));
    assert!(llp.l1_address.is_zero() && llp.master_account_index.get() == 1 && llp.account_share.is_none());
    assert_eq!(llp.holding(AssetId::COLLATERAL).map(|usdc| usdc.balance), Some(dec("10137934.801017762172")));
    assert_eq!(llp.share_price(), Some(dec("0.003552999685539589")));
    let eth_long = &pools[1];
    assert_eq!(
        (eth_long.kind(), eth_long.name.as_str(), eth_long.created_at),
        (PoolKind::Operated, "ETH 3x long", 1_737_101_233)
    );
    assert_eq!(eth_long.annual_percentage_yield.to_dec(), Some(dec("-65.84572054698785")));

    let next = page.after(pools[3].pool);
    assert_eq!(public_pools_call(&next).value("index"), Some("281474976710651"), "the last pool is the next cursor");
}

#[test]
fn a_holder_s_page_is_its_own_read() {
    let page = PoolsPage::first(PoolFilter::Account, 200).holder(index(10));
    let call = public_pools_call(&page);
    assert!(call.auth);
    assert_eq!(call.value("filter"), Some("account_index"));
    assert_eq!((call.value("limit"), call.value("account_index")), (Some("100"), Some("10")), "a page is at most 100");
    let plain = public_pools_call(&PoolsPage::first(PoolFilter::Any, 0));
    assert_eq!((plain.value("filter"), plain.value("limit")), (None, Some("1")));
}

#[test]
fn one_pool_is_asked_from_one_above_and_found_only_when_it_answers() {
    let transport = Recorded::answering(LLP);
    let llp = block_on(public_pool(&transport, AccountIndex::LLP, PoolFilter::Any)).unwrap().unwrap();
    assert_eq!(llp.total_asset_value, dec("94418596.495627"));
    assert_eq!(transport.only().fields, [("index", "281474976710655".to_owned()), ("limit", "1".to_owned())]);

    let transport = Recorded::answering(LLP);
    let other = block_on(public_pool(&transport, index(281_474_976_710_000), PoolFilter::Any)).unwrap();
    assert_eq!(other, None, "the page starts at the next pool down, which is not the one asked");
    assert_eq!(public_pool_call(AccountIndex::LLP, PoolFilter::Stake).value("filter"), Some("stake"));
}

#[test]
fn a_share_is_the_pool_s_value_over_its_shares_cut() {
    let llp: PublicPools = serde_json::from_str(LLP).unwrap();
    let llp = &llp.public_pools[0];
    assert_eq!(llp.share_price(), Some(dec("0.003552525232376643")));
    assert_eq!(llp.shares_for(dec("100")), Some(28_148));
    assert_eq!(llp.shares_for(dec("0.003")), Some(0), "less than a share buys none");
    assert_eq!(llp.shares_for(dec("-1")), None);
    assert_eq!(llp.value_of(5_492), Some(dec("19.510468")));
    assert_eq!(llp.value_of(0), Some(Dec::ZERO));
}

#[test]
fn a_frozen_pool_takes_no_deposits_and_an_empty_one_has_no_price() {
    let pools: PublicPools = serde_json::from_str(USER).unwrap();
    let pools = pools.public_pools;
    let frozen = pools.iter().find(|pool| pool.pool == index(281_474_976_710_645)).unwrap();
    assert_eq!(frozen.status, PoolStatus::Frozen);
    assert!(!frozen.takes_deposits());
    let empty = pools.iter().find(|pool| pool.total_shares == 0).unwrap();
    assert_eq!((empty.share_price(), empty.shares_for(dec("1")), empty.value_of(1)), (None, None, None));
    let drained = pools.iter().find(|pool| pool.total_asset_value.is_zero() && pool.total_shares > 0).unwrap();
    assert_eq!(
        (drained.share_price(), drained.shares_for(dec("1"))),
        (Some(Dec::ZERO), None),
        "value gone, shares left"
    );
}

#[test]
fn the_staking_pool_counts_its_shares_in_lit() {
    let pools: PublicPools = serde_json::from_str(STAKE).unwrap();
    let staking = &pools.public_pools[0];
    assert_eq!((staking.pool.get(), staking.kind()), (281_474_976_624_800, PoolKind::Staking));
    assert_eq!((staking.total_asset_value, staking.share_price()), (Dec::ZERO, Some(Dec::ZERO)), "no collateral");
    assert_eq!(staking.lit_per_share(), Some(dec("0.00001052408346139")));
    assert_eq!(staking.shares_for_lit(dec("100")), Some(9_502_015));
    assert_eq!(staking.lit_of(965), Some(dec("0.01015574")));
}

#[test]
fn robinhood_chain_s_llp_sits_at_the_same_index_in_usdg() {
    let pools: PublicPools = serde_json::from_str(RH_ALL).unwrap();
    let llp = &pools.public_pools[0];
    assert_eq!((llp.pool, llp.kind(), llp.name.as_str()), (AccountIndex::LLP, PoolKind::Protocol, ""));
    assert_eq!(llp.holding(AssetId::COLLATERAL).map(|usdg| usdg.symbol.as_str()), Some("USDG"));
    assert_eq!(llp.annual_percentage_yield.as_str(), "176.09640232288734");
}

#[test]
fn the_llp_s_detail_reads_from_its_account() {
    let transport = Recorded::answering(LLP_ACCOUNT);
    let detail = block_on(pool(&transport, AccountIndex::LLP)).unwrap().unwrap();
    let call = transport.only();
    assert_eq!(call.url_path(), "/api/v1/account");
    assert_eq!(call.fields, [("by", "index".to_owned()), ("value", "281474976710654".to_owned())]);
    assert_eq!((detail.pool, detail.kind()), (AccountIndex::LLP, PoolKind::Protocol));
    assert!(detail.description.starts_with("LLP is a community-owned"));
    let info = &detail.info;
    assert_eq!((info.status, info.total_shares, info.operator_shares), (PoolStatus::Active, 26_574_517_748, 1_000_000));
    assert_eq!((info.operator_fee, info.min_operator_share_rate), (Dec::ZERO, Dec::ZERO));
    assert_eq!(
        info.share_prices.last().map(|day| (day.timestamp, day.share_price.as_str())),
        Some((1_792_872_000, "0.003640112021854272"))
    );
    assert_eq!(info.daily_returns.len(), 3);
    assert_eq!(
        (info.strategies.len(), info.strategies[1].as_ref().map(|slot| slot.collateral)),
        (8, Some(dec("10137934.801017")))
    );
    assert_eq!((detail.positions.len(), detail.positions[0].symbol.as_str()), (2, "ETH"));
    assert_eq!(detail.share_price(), Some(dec("0.003553009868323841")));
}

#[test]
fn an_account_without_a_pool_or_no_account_at_all_is_no_pool() {
    let transport = Recorded::answering(ACCOUNT_10);
    assert_eq!(block_on(pool(&transport, index(10))), Ok(None));
    let transport = Recorded::answering(NO_ACCOUNT);
    assert_eq!(block_on(pool(&transport, index(140_737_488_355_327))), Ok(None), "29404: not found");
    assert_eq!(block_on(pool_shares(&transport, index(140_737_488_355_327))), Ok(Vec::new()));
}

#[test]
fn an_account_s_shares_name_each_pool() {
    let transport = Recorded::answering(ACCOUNT_10);
    let shares = block_on(pool_shares(&transport, index(10))).unwrap();
    assert_eq!(
        shares.iter().map(|share| (share.pool.get(), share.shares)).collect::<Vec<_>>(),
        [(281_474_976_624_800, 965), (AccountIndex::LLP.get(), 1)]
    );
    assert_eq!(block_on(shares_in(&transport, index(10), AccountIndex::LLP)), Ok(1));
    assert_eq!(block_on(shares_in(&transport, index(10), index(281_474_976_710_600))), Ok(0));
}

/// Asked for the staking pool without `filter=stake`, the venue answered the next pool down: a
/// frozen, empty one charging 5%.
#[test]
fn an_operator_fee_reads_in_percent_and_an_unlisted_pool_is_not_answered_for() {
    let transport = Recorded::answering(FEE);
    let staking = index(281_474_976_624_800);
    assert_eq!(block_on(public_pool(&transport, staking, PoolFilter::Any)), Ok(None));
    let pools: PublicPools = serde_json::from_str(FEE).unwrap();
    let pool = &pools.public_pools[0];
    assert_eq!((pool.name.as_str(), pool.operator_fee, pool.status), ("KUY Strategy", dec("5"), PoolStatus::Frozen));
    assert_eq!((pool.annual_percentage_yield.as_str(), pool.assets.len()), ("0", 0));
}
