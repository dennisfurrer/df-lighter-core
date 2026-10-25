//! The venue's websocket limits, as its rate limits page gives them, and how a refusal reads.

use super::*;

#[test]
fn the_venue_s_limits_are_the_docs() {
    assert_eq!((MAX_CONNECTIONS, MAX_SUBSCRIPTIONS_PER_CONNECTION, MAX_SUBSCRIPTIONS), (255, 500, 5_000));
    assert_eq!((MAX_ACCOUNTS, MAX_MESSAGES_PER_MINUTE, MAX_INFLIGHT_MESSAGES), (5_000, 200, 50));
    assert_eq!(Limits::default(), Limits { subscriptions: 500, accounts: 5_000, transactions: 50 });
}

#[test]
fn a_refusal_says_why() {
    assert_eq!(Refused::Batch(51).to_string(), "a batch carries 1 to 50 transactions, not 51");
    assert_eq!(
        Refused::NoToken("account_tx/1".to_owned()).to_string(),
        "`account_tx/1` needs the account's auth token, and the session has none"
    );
    assert_eq!(Refused::ReadOnly.to_string(), "the read-only socket takes no transactions");
}
