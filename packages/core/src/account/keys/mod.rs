//! Who may sign for an account: the API keys that sign every transaction, the auth tokens their
//! reads carry, the owner's Ethereum wallet that registers them, and registering one.

pub mod api_key;
pub mod register;
pub mod slots;
pub mod wallet;

pub use api_key::{ApiKey, AuthToken, MAX_TOKEN_S, TOKEN_S};
pub use register::ChangePubKey;
pub use slots::{Slot, api_key, api_keys, api_keys_call, next_nonce, next_nonce_call, pick_slot};
pub use wallet::{L1Signature, Wallet, WalletError};
#[cfg(feature = "sign")]
pub use wallet::{LocalWallet, address_of_public_key, recover, recover_digest, verify};
