//! Registering an API key at a slot: `ChangePubKey`, signed by the new key itself and by the
//! owner's wallet over "Register Lighter Account". The venue takes it at the slot's own nonce, so
//! a fresh slot starts at whatever `nextNonce` says, and the registration replaces whatever key
//! sat there before.

use crate::utils::hex;
use crate::utils::json::Json;
use crate::utils::sign::PublicKey;
use crate::utils::sign::goldilocks::Fp;
use crate::utils::sign::tx::{Envelope, Tx, TxType, byte_string, hex16};

/// `ChangePubKey`: `public_key` takes the envelope's slot of the envelope's account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangePubKey {
    pub public_key: PublicKey,
}

impl ChangePubKey {
    #[must_use]
    pub const fn new(public_key: PublicKey) -> ChangePubKey {
        ChangePubKey { public_key }
    }
}

impl Tx for ChangePubKey {
    fn tx_type(&self) -> TxType {
        TxType::ChangePubKey
    }

    fn hash_fields(&self) -> Vec<Fp> {
        self.public_key.limbs().0.to_vec()
    }

    fn info_fields(&self) -> Vec<(&'static str, Json)> {
        vec![("PubKey", byte_string(self.public_key.bytes())), ("L1Sig", Json::String(String::new()))]
    }

    /// Always asked: no key registers without the owner.
    fn l1_message(&self, envelope: &Envelope, _chain_id: u32) -> Option<String> {
        Some(format!(
            "Register Lighter Account\n\npubkey: 0x{}\nnonce: {}\naccount index: {}\napi key index: {}\nOnly sign this message for a trusted client!",
            hex::encode(self.public_key.bytes()),
            hex16(envelope.nonce),
            hex16(envelope.account.get()),
            hex16(u64::from(envelope.api_key.get())),
        ))
    }
}

#[cfg(test)]
#[cfg(feature = "sign")]
#[path = "tests/register.rs"]
mod tests;
