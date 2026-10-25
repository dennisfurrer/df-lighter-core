//! Keccak-256, the Ethereum hash: pad byte `0x01`, not SHA3's `0x06`.
//!
//! The sponge and padding are written here. The permutation is borrowed from
//! the `keccak` crate, which uses the ARMv8.2 SHA3 instructions where a native
//! build has them and its portable code everywhere else, wasm32 included. Two 64-byte inputs
//! hash in one pass with [`keccak256_of_two`], on both of the ARMv8.2 path's lanes.

/// The rate of Keccak-`f[1600]` at 256-bit capacity: 136 bytes per permute.
const RATE: usize = 136;

#[must_use]
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    Keccak256::new().chain(data).finalize()
}

#[derive(Clone)]
pub struct Keccak256 {
    state: [u64; 25],
    block: [u8; RATE],
    filled: usize,
}

impl Keccak256 {
    #[must_use]
    pub const fn new() -> Self {
        Keccak256 { state: [0u64; 25], block: [0u8; RATE], filled: 0 }
    }

    pub fn update(&mut self, data: &[u8]) {
        let mut rest = data;
        while !rest.is_empty() {
            let take = (RATE - self.filled).min(rest.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&rest[..take]);
            self.filled += take;
            rest = &rest[take..];
            if self.filled == RATE {
                self.absorb();
                self.filled = 0;
            }
        }
    }

    #[must_use]
    pub fn chain(mut self, data: &[u8]) -> Self {
        self.update(data);
        self
    }

    #[must_use]
    pub fn finalize(mut self) -> [u8; 32] {
        self.block[self.filled..].fill(0);
        self.block[self.filled] = 0x01;
        self.block[RATE - 1] |= 0x80;
        self.absorb();

        let mut digest = [0u8; 32];
        for (bytes, lane) in digest.chunks_exact_mut(8).zip(self.state.iter()) {
            bytes.copy_from_slice(&lane.to_le_bytes());
        }
        digest
    }

    fn absorb(&mut self) {
        for (lane, bytes) in self.state.iter_mut().zip(self.block.chunks_exact(8)) {
            let mut word = [0u8; 8];
            word.copy_from_slice(bytes);
            *lane ^= u64::from_le_bytes(word);
        }
        permute(&mut self.state);
    }
}

impl Default for Keccak256 {
    fn default() -> Self {
        Keccak256::new()
    }
}

impl std::fmt::Debug for Keccak256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Keccak256")
    }
}

fn permute(state: &mut [u64; 25]) {
    backend().with_f1600(|f| f(state));
}

/// Whether the permutation runs on the ARMv8.2 SHA3 instructions here, found the way the
/// `keccak` crate finds them.
#[must_use]
pub fn hardware() -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        std::arch::is_aarch64_feature_detected!("sha3")
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        false
    }
}

/// The backend, found once: the search for it is not free, and a hash is called millions of
/// times a second by a key search.
fn backend() -> &'static keccak::Keccak {
    static BACKEND: std::sync::OnceLock<keccak::Keccak> = std::sync::OnceLock::new();
    BACKEND.get_or_init(keccak::Keccak::new)
}

/// Keccak-256 of two 64-byte inputs, such as two public keys, in one pass. The ARMv8.2 path
/// works on two lanes at once, so the second digest is nearly free; elsewhere it is two passes.
#[must_use]
pub fn keccak256_of_two(a: &[u8; 64], b: &[u8; 64]) -> ([u8; 32], [u8; 32]) {
    let mut left = one_block(a);
    let mut right = one_block(b);
    permute_pair(&mut left, &mut right);
    (squeezed(&left), squeezed(&right))
}

/// A 64-byte input absorbed and padded: it fits one block, so the state is the input itself.
fn one_block(input: &[u8; 64]) -> [u64; 25] {
    let mut state = [0u64; 25];
    let (words, _) = input.as_chunks::<8>();
    for (lane, word) in state.iter_mut().zip(words) {
        *lane = u64::from_le_bytes(*word);
    }
    state[8] ^= 0x01;
    state[(RATE - 1) / 8] ^= 0x80 << 56;
    state
}

fn squeezed(state: &[u64; 25]) -> [u8; 32] {
    let mut digest = [0u8; 32];
    for (bytes, lane) in digest.chunks_exact_mut(8).zip(state.iter()) {
        bytes.copy_from_slice(&lane.to_le_bytes());
    }
    digest
}

fn permute_pair(a: &mut [u64; 25], b: &mut [u64; 25]) {
    struct Pair<'a>(&'a mut [u64; 25], &'a mut [u64; 25]);

    impl keccak::BackendClosure for Pair<'_> {
        fn call_once<B: keccak::Backend>(self) {
            let mut states = keccak::ParState1600::<B>::default();
            if states.len() < 2 {
                let permute = B::get_p1600::<{ keccak::F1600_ROUNDS }>();
                permute(self.0);
                permute(self.1);
                return;
            }
            states[0] = *self.0;
            states[1] = *self.1;
            B::get_par_f1600()(&mut states);
            *self.0 = states[0];
            *self.1 = states[1];
        }
    }

    backend().with_backend(Pair(a, b));
}

#[cfg(test)]
#[path = "tests/keccak.rs"]
mod tests;
