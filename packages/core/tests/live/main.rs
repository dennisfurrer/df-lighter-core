//! Against both venues, read only, run by hand with `just live`: every public REST read the crate
//! exposes on Lighter and on Robinhood Chain, and the reads a deposit makes on each L1. Nothing is
//! posted to a venue, nothing is signed, and no token is used; every request waits its turn
//! ([`tap::GAP`]).
//!
//! Each read must decode into its type, and its answer must have the shape of the recording the
//! unit and mocked suites read ([`shape::drift`]), so a field the venue has added, or a value that
//! changed kind, shows up here before it reaches a user.

// A test crate: clippy's allowance for tests covers `#[test]` functions only, not their helpers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../support/mod.rs"]
mod support;

mod shape;
mod tap;

mod account;
mod client;
mod earn;
mod markets;
mod transfer;
