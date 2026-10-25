//! Whole journeys through the public [`Client`](df_lighter_core::Client) against a fake venue
//! that answers from recordings and checks every transaction the way Lighter's API does
//! ([`support::venue::Venue`]). Each test is one thing a user does, end to end; the units it
//! passes through have their own tests next to their code.

// A test crate: clippy's allowance for tests covers `#[test]` functions only, not their helpers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../support/mod.rs"]
mod support;

mod client;
mod onboarding;
mod trade;
mod transfer;
mod venue;
