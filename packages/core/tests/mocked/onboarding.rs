//! Onboarding the way atlas walks it: an account found by its owner, a key generated here, a
//! slot that leaves Lighter's own apps alone, one `personal_sign` to register it, then trading
//! on it, and its auth token on the account's own reads.

use df_lighter_core::account::{ApiKey, keys};
use df_lighter_core::client::{Call, Network, Transport};
use df_lighter_core::utils::{AccountIndex, ApiKeyIndex};
use df_lighter_core::{Client, SendError};

use crate::client::Cancel;
use crate::support::venue::Venue;
use crate::support::{ACCOUNT, clock, owner, owner_address, secret};

/// The venue's own web app holds slots 0 and 1 of the account, as on a fresh account.
fn fresh() -> Venue {
    Venue::lighter().account(ACCOUNT, owner_address()).key(ACCOUNT, 0, secret("b").public_key(), 12).key(
        ACCOUNT,
        1,
        secret("b").public_key(),
        3,
    )
}

#[tokio::test]
async fn a_new_key_is_registered_by_its_owner_then_trades() {
    let account = AccountIndex::new(ACCOUNT).unwrap();
    let reader = Client::reader(fresh(), Network::Lighter, clock);
    let listed = keys::api_keys(&reader, account).await.unwrap();
    let slot = keys::pick_slot(Network::Lighter, &listed, None).unwrap();
    assert_eq!(slot.get(), 4, "the lowest slot past the venue's own");

    let key = ApiKey::generate(account, slot).unwrap();
    let client = Client::new(fresh(), Network::Lighter, clock, key);
    assert!(!client.is_registered().await.unwrap());
    let registered = client.register(&owner()).await.unwrap();
    assert_eq!(registered.tx_hash.len(), 80);
    assert!(client.is_registered().await.unwrap(), "the venue lists the key at slot 4");
    assert_eq!(client.http().registered(ACCOUNT, 4), Some((client.key().public_key(), 1)));

    client.send(&Cancel(7)).await.unwrap();
    client.send(&Cancel(8)).await.unwrap();
    let taken = client.http().taken();
    let nonces: Vec<u64> = taken.iter().map(|tx| tx.number("Nonce")).collect();
    assert_eq!(nonces, [0, 1, 2], "the registration took the slot's first nonce, the cancels the next");
    let asked = client.http().paths().iter().filter(|path| path.contains("nextNonce")).count();
    assert_eq!(asked, 1, "asked once, then counted");
}

/// The key signs the account's own reads too: one token for seven hours, carried by each.
#[tokio::test]
async fn the_key_s_token_reads_the_account_s_own_orders() {
    let venue =
        fresh().key(ACCOUNT, 4, secret("a").public_key(), 0).text("accountActiveOrders", r#"{"code":200,"orders":[]}"#);
    let key = ApiKey::new(AccountIndex::new(ACCOUNT).unwrap(), ApiKeyIndex::new(4).unwrap(), secret("a"));
    let client = Client::new(venue, Network::Lighter, clock, key);
    for _ in 0..2 {
        client.call(Call::get("accountActiveOrders").field("account_index", ACCOUNT).authed()).await.unwrap();
    }
    let tokens: Vec<String> =
        client.http().sent().iter().map(|request| request.header("Authorization").unwrap().to_owned()).collect();
    assert_eq!(tokens[0], tokens[1], "one token, kept");
    assert!(tokens[0].starts_with(&format!("{}:{ACCOUNT}:4:", clock() / 1_000 + 7 * 3_600)), "{}", tokens[0]);
}

/// A slot that holds someone else's key is never taken over, and the venue's own are refused
/// even when asked for.
#[tokio::test]
async fn the_apps_slots_and_other_keys_are_left_alone() {
    let account = AccountIndex::new(ACCOUNT).unwrap();
    let venue = fresh().key(ACCOUNT, 4, secret("b").public_key(), 9);
    let reader = Client::reader(venue, Network::Lighter, clock);
    let listed = keys::api_keys(&reader, account).await.unwrap();
    let mine = secret("a").public_key();
    let slot = keys::pick_slot(Network::Lighter, &listed, Some((ApiKeyIndex::new(4).unwrap(), &mine))).unwrap();
    assert_eq!(slot.get(), 5);

    let key = ApiKey::new(account, ApiKeyIndex::new(1).unwrap(), secret("a"));
    let client = Client::new(fresh(), Network::Lighter, clock, key);
    assert!(matches!(client.register(&owner()).await, Err(SendError::Sign(_))));
    assert!(client.http().sent().is_empty(), "nothing went out");
}
