//! The [`Client`] against the fake venue: the nonce through every kind of answer, the turn one
//! client's transactions take, and what a lost answer means.

use df_lighter_core::account::ApiKey;
use df_lighter_core::client::{ApiError, Network};
use df_lighter_core::utils::sign::tx::number;
use df_lighter_core::utils::sign::{Fp, Tx, TxType};
use df_lighter_core::utils::{AccountIndex, ApiKeyIndex, Json};
use df_lighter_core::{Client, SendError};

use crate::support::venue::Venue;
use crate::support::{ACCOUNT, SLOT, clock, secret};

/// A cancel of order `index` on market 1, the lightest transaction there is: written here so
/// the client is held apart from the trade module.
pub struct Cancel(pub u64);

impl Tx for Cancel {
    fn tx_type(&self) -> TxType {
        TxType::CancelOrder
    }

    fn hash_fields(&self) -> Vec<Fp> {
        vec![Fp::new(1), Fp::new(self.0)]
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![("MarketIndex", number(1)), ("Index", number(self.0))]
    }
}

/// The vectors' key at its slot of the account, registered on `venue` at nonce 40.
pub fn trader(venue: Venue) -> Client<Venue, ApiKey> {
    let key = ApiKey::new(AccountIndex::new(ACCOUNT).unwrap(), ApiKeyIndex::new(SLOT).unwrap(), secret("a"));
    Client::new(venue.key(ACCOUNT, SLOT, secret("a").public_key(), 40), Network::Lighter, clock, key)
}

fn nonces(client: &Client<Venue, ApiKey>) -> Vec<u64> {
    client.http().taken().iter().map(|tx| tx.number("Nonce")).collect()
}

/// A refusal leaves the nonce for the next transaction; a transaction the sequencer dropped
/// leaves the venue a step behind, and the next one is refused for its nonce, asked again and
/// sent once more.
#[tokio::test]
async fn the_nonce_follows_the_venue_through_refusals_and_drops() {
    let venue = Venue::lighter().refuse(21_739, "not enough margin");
    let client = trader(venue);
    let refused = client.send(&Cancel(1)).await.unwrap_err();
    assert_eq!(refused, SendError::Api(ApiError::Refused { code: 21_739, message: "not enough margin".to_owned() }));
    client.send(&Cancel(2)).await.unwrap();
    assert_eq!(nonces(&client), [40], "the refused one did not count");

    let client = trader(Venue::lighter().drop_next());
    client.send(&Cancel(1)).await.unwrap();
    client.send(&Cancel(2)).await.unwrap();
    assert_eq!(nonces(&client), [40, 40], "41 was refused, the count asked again, and 40 signed anew");
    let asked = client.http().paths().iter().filter(|path| path.contains("nextNonce")).count();
    assert_eq!(asked, 2);
}

/// An answer that never came may have landed: nothing is sent again, and the next transaction
/// asks the venue for the count.
#[tokio::test]
async fn a_lost_answer_is_never_resent_and_the_count_is_asked_again() {
    let client = trader(Venue::lighter().unreachable("sendTx"));
    let lost = client.send(&Cancel(1)).await.unwrap_err();
    assert!(matches!(&lost, SendError::Api(error) if error.may_have_landed()), "{lost}");
    assert_eq!(nonces(&client), [40], "it was taken");
    client.send(&Cancel(2)).await.unwrap();
    assert_eq!(nonces(&client), [40, 41]);
    let asked = client.http().paths().iter().filter(|path| path.contains("nextNonce")).count();
    assert_eq!(asked, 2);
}

/// Transactions sent at once from one client go out in turn, each at the next nonce.
#[tokio::test]
async fn sends_at_once_take_their_turn() {
    let client = trader(Venue::lighter());
    let (one, two, three) = tokio::join!(client.send(&Cancel(1)), client.send(&Cancel(2)), client.send(&Cancel(3)));
    for sent in [one, two, three] {
        sent.unwrap();
    }
    assert_eq!(nonces(&client), [40, 41, 42]);
}

/// A batch is checked transaction by transaction, at consecutive nonces.
#[tokio::test]
async fn a_batch_goes_out_at_consecutive_nonces() {
    let client = trader(Venue::lighter());
    let txs: [&dyn Tx; 3] = [&Cancel(1), &Cancel(2), &Cancel(3)];
    let sent = client.send_batch(&txs).await.unwrap();
    let taken = client.http().taken();
    assert_eq!(sent.tx_hashes, taken.iter().map(|tx| tx.hash.clone()).collect::<Vec<_>>());
    assert_eq!(nonces(&client), [40, 41, 42]);
    assert_eq!(client.next_nonce().await, Ok(43));
}
