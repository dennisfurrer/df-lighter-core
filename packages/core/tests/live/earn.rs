//! Public pools, LIT staking and LIT fee credits, read from both venues and held against their
//! recordings: pages of pools from the LLP down, the LLP listed and as an account, Lighter's
//! staking pool and a holder's shares in it, and the lease terms. Robinhood Chain runs the LLP and
//! no staking pool.

use df_lighter_core::Network::{self, Lighter, Robinhood};
use df_lighter_core::earn::pools::shares_in;
use df_lighter_core::earn::{
    LIGHTER_STAKING_POOL, PoolFilter, PoolKind, PoolsPage, lease_options, pending_unlocks, pool, pool_shares,
    public_pool, public_pools, staked_shares, staking_pool,
};
use df_lighter_core::utils::{AccountIndex, AssetId, Dec};

use crate::tap::{Tapped, reader, recording, same_shape};

const TEN: AccountIndex = AccountIndex::of::<10>();

#[tokio::test]
#[ignore = "live"]
async fn pools_list_from_the_llp_down_highest_first() {
    for network in Network::ALL {
        let client = reader(network);
        let pools = public_pools(&client, &PoolsPage::first(PoolFilter::All, 4)).await.unwrap();
        same_shape(&client, &recording(network, "publicPoolsMetadata_all.json"));
        let llp = pools.first().unwrap();
        assert_eq!((llp.pool, llp.kind()), (AccountIndex::LLP, PoolKind::Protocol), "{network}");
        assert!(llp.takes_deposits() && llp.share_price().is_some_and(|price| price > Dec::ZERO), "{llp:?}");
        let collateral = llp.holding(AssetId::COLLATERAL).map(|held| held.symbol.as_str());
        assert_eq!(collateral, Some(network.collateral().symbol), "{network}");
        assert!(pools.len() <= 4 && pools.windows(2).all(|pair| pair[0].pool > pair[1].pool), "{network}");
    }

    let client = reader(Lighter);
    let page = PoolsPage::first(PoolFilter::User, 5);
    let operated = public_pools(&client, &page).await.unwrap();
    same_shape(&client, "lighter/publicPoolsMetadata_user.json");
    assert!(operated.iter().all(|pool| pool.kind() == PoolKind::Operated), "{operated:?}");
    let next = public_pools(&client, &page.after(operated.last().unwrap().pool)).await.unwrap();
    same_shape(&client, "lighter/publicPoolsMetadata_user.json");
    assert!(next.first().is_none_or(|first| first.pool < operated.last().unwrap().pool), "the page goes on");
}

#[tokio::test]
#[ignore = "live"]
async fn the_llp_reads_as_a_listed_pool_and_as_an_account() {
    for (network, listed, whole) in [
        (Lighter, "llp.json", "account_281474976710654.json"),
        (Robinhood, "publicPoolsMetadata_all.json", "account_llp.json"),
    ] {
        let client = reader(network);
        let llp = public_pool(&client, AccountIndex::LLP, PoolFilter::Any).await.unwrap().unwrap();
        same_shape(&client, &recording(network, listed));
        assert!(llp.total_shares > 0 && llp.total_asset_value > Dec::ZERO, "{network}: {llp:?}");
        let detail = pool(&client, AccountIndex::LLP).await.unwrap().unwrap();
        same_shape(&client, &recording(network, whole));
        assert_eq!((detail.pool, detail.kind()), (AccountIndex::LLP, PoolKind::Protocol), "{network}");
        assert!(detail.info.total_shares > 0 && detail.share_price().is_some(), "{network}: {:?}", detail.info);
    }
}

#[tokio::test]
#[ignore = "live"]
async fn lighter_s_staking_pool_and_a_holder_s_shares_read_and_robinhood_chain_has_none() {
    let client = reader(Lighter);
    let staking = staking_pool(&client, Lighter).await.unwrap().unwrap();
    same_shape(&client, "lighter/publicPoolsMetadata_stake.json");
    assert_eq!((staking.pool, staking.kind()), (LIGHTER_STAKING_POOL, PoolKind::Staking));
    assert!(staking.lit_per_share().is_some_and(|lit| lit > Dec::ZERO), "{staking:?}");
    let unfiltered = public_pool(&client, LIGHTER_STAKING_POOL, PoolFilter::Any).await.unwrap();
    same_shape(&client, "lighter/publicPoolsMetadata_fee.json");
    assert_eq!(unfiltered, None, "without the filter the page starts at the next pool down");

    let staked = staked_shares(&client, Lighter, TEN).await.unwrap();
    same_shape(&client, "lighter/account_10.json");
    let held = pool_shares(&client, TEN).await.unwrap();
    same_shape(&client, "lighter/account_10.json");
    let shares = |pool: AccountIndex| held.iter().find(|share| share.pool == pool).map_or(0, |share| share.shares);
    assert_eq!(shares(LIGHTER_STAKING_POOL), staked);
    let in_llp = shares_in(&client, TEN, AccountIndex::LLP).await.unwrap();
    same_shape(&client, "lighter/account_10.json");
    assert_eq!(shares(AccountIndex::LLP), in_llp);
    pending_unlocks(&client, TEN).await.unwrap();
    same_shape(&client, "lighter/account_10.json");

    let client = reader(Robinhood);
    assert_eq!(staking_pool(&client, Robinhood).await, Ok(None));
    assert_eq!(staked_shares(&client, Robinhood, TEN).await, Ok(0));
    assert_eq!(client.tap().count(), 0, "not asked");
    let none = public_pools(&client, &PoolsPage::first(PoolFilter::Stake, 10)).await.unwrap();
    same_shape(&client, "robinhood/publicPoolsMetadata_stake.json");
    assert_eq!(none, []);
}

#[tokio::test]
#[ignore = "live"]
async fn lease_terms_read_on_both_networks() {
    for network in Network::ALL {
        let client = reader(network);
        let terms = lease_options(&client).await.unwrap();
        same_shape(&client, &recording(network, "leaseOptions.json"));
        assert!(!terms.options.is_empty(), "{network}");
        assert!(terms.options.iter().all(|option| option.duration_days > 0 && option.annual_rate.to_dec().is_some()));
    }
}
