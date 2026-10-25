//! The venue's refusals, as the read-only socket answered them: each names its code, and a
//! subscription's its channel.

use super::*;
use crate::live::fixtures::{INVALID_CHANNEL, answer};
use crate::live::{Frame, decode};
use crate::utils::AccountIndex;

fn refusal(text: &str) -> Refusal {
    match decode(text).unwrap() {
        Frame::Error(refusal) => refusal,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_subscription_s_refusal_names_its_channel() {
    let again = refusal(answer("Already Subscribed"));
    assert_eq!((again.code, again.channel()), (codes::ALREADY_SUBSCRIBED, Some("height")));
    assert_eq!(again.subscription(), Some(Subscription::Height));
    let never = refusal(answer("Not Subscribed"));
    assert_eq!((never.code, never.subscription()), (codes::NOT_SUBSCRIBED, Some(Subscription::Height)));
    let token = refusal(answer("auth field is required"));
    assert_eq!(
        token.subscription(),
        Some(Subscription::AccountAllOrders { account: AccountIndex::new(10).unwrap() }),
        "{}",
        token.message
    );
    let unknown = refusal(INVALID_CHANNEL);
    assert_eq!((unknown.code, unknown.channel(), unknown.subscription()), (codes::INVALID_CHANNEL, None, None));
}

#[test]
fn a_refusal_reads_as_the_rest_api_s() {
    let refused = refusal(r#"{"error":{"code":21120,"message":"invalid signature"},"id":"3"}"#);
    assert_eq!(refused.to_api_error().code(), Some(21_120));
    assert_eq!(refused.id.as_deref(), Some("3"));
    assert_eq!(refusal(r#"{"error":"down for maintenance"}"#).message, "down for maintenance", "a bare message");
}
