//! Poseidon2 over Goldilocks, width 12, rate 8, x^7 S-box, 8 full and 22 partial rounds: the
//! permutation Lighter hashes transactions, auth tokens and Schnorr challenges with
//! (elliottech/poseidon_crypto v0.0.15, `hash/poseidon2_goldilocks_plonky2`). The round constants
//! are that package's; the internal diagonal is Plonky3's.

use crate::utils::sign::fp5::Fp5;
use crate::utils::sign::goldilocks::Fp;

const WIDTH: usize = 12;
const RATE: usize = 8;
const ROUNDS_F_HALF: usize = 4;
const ROUNDS_P: usize = 22;

const EXTERNAL_CONSTANTS: [[u64; WIDTH]; 2 * ROUNDS_F_HALF] = [
    [
        15492826721047263190,
        11728330187201910315,
        8836021247773420868,
        16777404051263952451,
        5510875212538051896,
        6173089941271892285,
        2927757366422211339,
        10340958981325008808,
        8541987352684552425,
        9739599543776434497,
        15073950188101532019,
        12084856431752384512,
    ],
    [
        4584713381960671270,
        8807052963476652830,
        54136601502601741,
        4872702333905478703,
        5551030319979516287,
        12889366755535460989,
        16329242193178844328,
        412018088475211848,
        10505784623379650541,
        9758812378619434837,
        7421979329386275117,
        375240370024755551,
    ],
    [
        3331431125640721931,
        15684937309956309981,
        578521833432107983,
        14379242000670861838,
        17922409828154900976,
        8153494278429192257,
        15904673920630731971,
        11217863998460634216,
        3301540195510742136,
        9937973023749922003,
        3059102938155026419,
        1895288289490976132,
    ],
    [
        5580912693628927540,
        10064804080494788323,
        9582481583369602410,
        10186259561546797986,
        247426333829703916,
        13193193905461376067,
        6386232593701758044,
        17954717245501896472,
        1531720443376282699,
        2455761864255501970,
        11234429217864304495,
        4746959618548874102,
    ],
    [
        13571697342473846203,
        17477857865056504753,
        15963032953523553760,
        16033593225279635898,
        14252634232868282405,
        8219748254835277737,
        7459165569491914711,
        15855939513193752003,
        16788866461340278896,
        7102224659693946577,
        3024718005636976471,
        13695468978618890430,
    ],
    [
        8214202050877825436,
        2670727992739346204,
        16259532062589659211,
        11869922396257088411,
        3179482916972760137,
        13525476046633427808,
        3217337278042947412,
        14494689598654046340,
        15837379330312175383,
        8029037639801151344,
        2153456285263517937,
        8301106462311849241,
    ],
    [
        13294194396455217955,
        17394768489610594315,
        12847609130464867455,
        14015739446356528640,
        5879251655839607853,
        9747000124977436185,
        8950393546890284269,
        10765765936405694368,
        14695323910334139959,
        16366254691123000864,
        15292774414889043182,
        10910394433429313384,
    ],
    [
        17253424460214596184,
        3442854447664030446,
        3005570425335613727,
        10859158614900201063,
        9763230642109343539,
        6647722546511515039,
        909012944955815706,
        18101204076790399111,
        11588128829349125809,
        15863878496612806566,
        5201119062417750399,
        176665553780565743,
    ],
];

const INTERNAL_CONSTANTS: [u64; ROUNDS_P] = [
    11921381764981422944,
    10318423381711320787,
    8291411502347000766,
    229948027109387563,
    9152521390190983261,
    7129306032690285515,
    15395989607365232011,
    8641397269074305925,
    17256848792241043600,
    6046475228902245682,
    12041608676381094092,
    12785542378683951657,
    14546032085337914034,
    3304199118235116851,
    16499627707072547655,
    10386478025625759321,
    13475579315436919170,
    16042710511297532028,
    1411266850385657080,
    9024840976168649958,
    14047056970978379368,
    838728605080212101,
];

const MATRIX_DIAG_12: [u64; WIDTH] = [
    0xc3b6c08e23ba9300,
    0xd84b5de94a324fb6,
    0x0d0c371c5b35b84f,
    0x7964f570e7188037,
    0x5daf18bbd996604b,
    0x6743bc47b9595257,
    0x5528b9362c59bb70,
    0xac45e25b7127b68b,
    0xa2077d7dfbb606b5,
    0xf3faac6faee378ae,
    0x0c6388b51545e883,
    0xd27dbb6944917b60,
];

type State = [Fp; WIDTH];

/// x^7.
fn sbox(x: Fp) -> Fp {
    let cube = x.square().mul(x);
    cube.square().mul(x)
}

/// circ(2, 3, 1, 1) on each group of four, then each lane plus the sum of its column.
fn external_linear_layer(state: &mut State) {
    let mut wide = [0u128; WIDTH];
    for (slot, lane) in wide.iter_mut().zip(state.iter()) {
        *slot = u128::from(lane.value());
    }
    for group in wide.chunks_exact_mut(4) {
        let (x0, x1, x2, x3) = (group[0], group[1], group[2], group[3]);
        let t01 = x0 + x1;
        let t23 = x2 + x3;
        let t0123 = t01 + t23;
        group[0] = t0123 + t01 + x1;
        group[1] = t0123 + x1 + 2 * x2;
        group[2] = t0123 + t23 + x3;
        group[3] = t0123 + x3 + 2 * x0;
    }
    let sums: [u128; 4] = std::array::from_fn(|column| wide[column] + wide[column + 4] + wide[column + 8]);
    for (at, lane) in state.iter_mut().enumerate() {
        *lane = Fp::from_u128(wide[at] + sums[at % 4]);
    }
}

/// Every lane becomes the state's sum plus its diagonal multiple.
fn internal_linear_layer(state: &mut State) {
    let sum = Fp::from_u128(state.iter().map(|lane| u128::from(lane.value())).sum());
    for (lane, diagonal) in state.iter_mut().zip(MATRIX_DIAG_12) {
        *lane = sum.mul_add(*lane, Fp::new(diagonal));
    }
}

fn full_rounds(state: &mut State, rounds: std::ops::Range<usize>) {
    for round in rounds {
        for (lane, constant) in state.iter_mut().zip(EXTERNAL_CONSTANTS[round]) {
            *lane = sbox(lane.add(Fp::new(constant)));
        }
        external_linear_layer(state);
    }
}

/// The permutation, in place.
pub fn permute(state: &mut State) {
    external_linear_layer(state);
    full_rounds(state, 0..ROUNDS_F_HALF);
    for constant in INTERNAL_CONSTANTS {
        state[0] = sbox(state[0].add(Fp::new(constant)));
        internal_linear_layer(state);
    }
    full_rounds(state, ROUNDS_F_HALF..2 * ROUNDS_F_HALF);
}

/// The sponge with no padding, the rate overwritten on each absorb (`HashNToMNoPad`).
#[must_use]
pub fn hash_n_to_m_no_pad(input: &[Fp], outputs: usize) -> Vec<Fp> {
    let mut state = [Fp::ZERO; WIDTH];
    for chunk in input.chunks(RATE) {
        state[..chunk.len()].copy_from_slice(chunk);
        permute(&mut state);
    }
    let mut out = Vec::with_capacity(outputs);
    loop {
        for lane in &state[..RATE] {
            out.push(*lane);
            if out.len() == outputs {
                return out;
            }
        }
        permute(&mut state);
    }
}

/// `HashToQuinticExtension`: the first five outputs as one GF(p^5) element.
#[must_use]
pub fn hash_to_fp5(input: &[Fp]) -> Fp5 {
    let out = hash_n_to_m_no_pad(input, 5);
    let mut limbs = [Fp::ZERO; 5];
    limbs.copy_from_slice(&out[..5]);
    Fp5(limbs)
}

/// `HashNoPad`: four outputs, what a grouped order's leg hashes to.
#[must_use]
pub fn hash_no_pad(input: &[Fp]) -> [Fp; 4] {
    let out = hash_n_to_m_no_pad(input, 4);
    [out[0], out[1], out[2], out[3]]
}

/// `HashTwoToOne`: two four-element hashes folded into one.
#[must_use]
pub fn hash_two_to_one(left: [Fp; 4], right: [Fp; 4]) -> [Fp; 4] {
    let mut both = [Fp::ZERO; 8];
    both[..4].copy_from_slice(&left);
    both[4..].copy_from_slice(&right);
    hash_no_pad(&both)
}

#[cfg(test)]
#[path = "tests/poseidon2.rs"]
mod tests;
