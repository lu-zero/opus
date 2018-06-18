//!
//! Silk Decoding
//!
//! See [section-4.2](https://tools.ietf.org/html/rfc6716#section-4.2)
//!

use codec::error::*;
use entropy::*;
use maths::*;
use packet::*;

use std::ops::Range;

#[derive(Debug)]
pub struct SilkInfo {
    bandwidth: Bandwidth,
    subframes: usize,
    sf_size: usize,
    f_size: usize,

    weight0: f32,
    weight1: f32,
    prev0: f32,
    prev1: f32,
}

#[derive(Debug)]
pub struct Silk {
    stereo: bool,
    stereo_out: bool,
    frames: usize,
    frame_len: usize,
    subframe_len: usize,
    info: SilkInfo,

    mid_frame: SilkFrame,
    side_frame: SilkFrame,
    // Todo use directly an AudioQueue ?
    left_outbuf: Vec<f32>,
    right_outbuf: Vec<f32>,
}

#[derive(Debug, Default)]
struct SubFrame {
    gain: f32,
    pitch_lag: i32,
    ltp_taps: [f32; 5],
}

const STAGE1: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[
        7, 9, 10, 11, 12, 22, 46, 54, 55, 56, 59, 82, 174, 197, 200, 201, 202, 210, 234, 244, 245,
        246, 247, 249, 256,
    ],
};

const STAGE2: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[85, 171, 256],
};

const STAGE3: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[51, 102, 154, 205, 256],
};

const MID_ONLY: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[192, 256],
};

const FRAME_TYPE_INACTIVE: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[26, 256],
};

const FRAME_TYPE_ACTIVE: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[24, 98, 246, 256],
};

const MSB_SUBFRAME_GAIN: &[&ICDFContext; 3] = &[
    &ICDFContext {
        total: 256,
        dist: &[32, 144, 212, 241, 253, 254, 255, 256],
    },
    &ICDFContext {
        total: 256,
        dist: &[2, 19, 64, 124, 186, 233, 252, 256],
    },
    &ICDFContext {
        total: 256,
        dist: &[1, 4, 30, 101, 195, 245, 254, 256],
    },
];

const LSB_SUBFRAME_GAIN: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[32, 64, 96, 128, 160, 192, 224, 256],
};

const DELTA_SUBFRAME_GAIN: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[
        6, 11, 22, 53, 185, 206, 214, 218, 221, 223, 225, 227, 228, 229, 230, 231, 232, 233, 234,
        235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252,
        253, 254, 255, 256,
    ],
};

const LSF_STAGE1_NB_MB: &[&ICDFContext] = &[
    &ICDFContext {
        total: 256,
        dist: &[
            44, 78, 108, 127, 148, 160, 171, 174, 177, 179, 195, 197, 199, 200, 205, 207, 208, 211,
            214, 215, 216, 218, 220, 222, 225, 226, 235, 244, 246, 253, 255, 256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            1, 11, 12, 20, 23, 31, 39, 53, 66, 80, 81, 95, 107, 120, 131, 142, 154, 165, 175, 185,
            196, 204, 213, 221, 228, 236, 237, 238, 244, 245, 251, 256,
        ],
    },
];

const LSF_STAGE1_WB: &[&ICDFContext] = &[
    &ICDFContext {
        total: 256,
        dist: &[
            31, 52, 55, 72, 73, 81, 98, 102, 103, 121, 137, 141, 143, 146, 147, 157, 158, 161, 177,
            188, 204, 206, 208, 211, 213, 224, 225, 229, 238, 246, 253, 256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            1, 5, 21, 26, 44, 55, 60, 74, 89, 90, 93, 105, 118, 132, 146, 152, 166, 178, 180, 186,
            187, 199, 211, 222, 232, 235, 245, 250, 251, 252, 253, 256,
        ],
    },
];

pub mod lsf_stage2_nb_mb {
    use entropy::*;

    const A: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 2, 3, 18, 242, 253, 254, 255, 256],
    };
    const B: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 2, 4, 38, 221, 253, 254, 255, 256],
    };
    const C: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 2, 6, 48, 197, 252, 254, 255, 256],
    };
    const D: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 2, 10, 62, 185, 246, 254, 255, 256],
    };
    const E: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 4, 20, 73, 174, 248, 254, 255, 256],
    };
    const F: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 4, 21, 76, 166, 239, 254, 255, 256],
    };
    const G: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 8, 32, 85, 159, 226, 252, 255, 256],
    };
    const H: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 2, 20, 83, 161, 219, 249, 255, 256],
    };

    pub const MAP: &[&[&ICDFContext]] = &[
        &[A, A, A, A, A, A, A, A, A, A],
        &[B, D, B, C, C, B, C, B, B, B],
        &[C, B, B, B, B, B, B, B, B, B],
        &[B, C, C, C, C, B, C, B, B, B],
        &[C, D, D, D, D, C, C, C, C, C],
        &[A, F, D, D, C, C, C, C, B, B],
        &[A, C, C, C, C, C, C, C, C, B],
        &[C, D, G, E, E, E, F, E, F, F],
        &[C, E, F, F, E, F, E, G, E, E],
        &[C, E, E, H, E, F, E, F, F, E],
        &[E, D, D, D, C, D, C, C, C, C],
        &[B, F, F, G, E, F, E, F, F, F],
        &[C, H, E, G, F, F, F, F, F, F],
        &[C, H, F, F, F, F, F, G, F, E],
        &[D, D, F, E, E, F, E, F, E, E],
        &[C, D, D, F, F, E, E, E, E, E],
        &[C, E, E, G, E, F, E, F, F, F],
        &[C, F, E, G, F, F, F, E, F, E],
        &[C, H, E, F, E, F, E, F, F, F],
        &[C, F, E, G, H, G, F, G, F, E],
        &[D, G, H, E, G, F, F, G, E, F],
        &[C, H, G, E, E, E, F, E, F, F],
        &[E, F, F, E, G, G, F, G, F, E],
        &[C, F, F, G, F, G, E, G, E, E],
        &[E, F, F, F, D, H, E, F, F, E],
        &[C, D, E, F, F, G, E, F, F, E],
        &[C, D, C, D, D, E, C, D, D, D],
        &[B, B, C, C, C, C, C, D, C, C],
        &[E, F, F, G, G, G, F, G, E, F],
        &[D, F, F, E, E, E, E, D, D, C],
        &[C, F, D, H, F, F, E, E, F, E],
        &[E, E, F, E, F, G, F, G, F, E],
    ];
}

use self::lsf_stage2_nb_mb::MAP as LSF_MAP_NB_MB;

pub mod lsf_stage2_wb {
    use entropy::*;
    const I: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 2, 3, 12, 244, 253, 254, 255, 256],
    };
    const J: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 2, 4, 32, 218, 253, 254, 255, 256],
    };
    const K: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 2, 5, 47, 199, 252, 254, 255, 256],
    };
    const L: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 2, 12, 61, 187, 252, 254, 255, 256],
    };
    const M: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 5, 24, 72, 172, 249, 254, 255, 256],
    };
    const N: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 2, 16, 70, 170, 242, 254, 255, 256],
    };
    const O: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 2, 17, 78, 165, 226, 251, 255, 256],
    };
    const P: &ICDFContext = &ICDFContext {
        total: 256,
        dist: &[1, 8, 29, 79, 156, 237, 254, 255, 256],
    };

    pub const MAP: &[&[&ICDFContext]] = &[
        &[I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I],
        &[K, L, L, L, L, L, K, K, K, K, K, J, J, J, I, L],
        &[K, N, N, L, P, M, M, N, K, N, M, N, N, M, L, L],
        &[I, K, J, K, K, J, J, J, J, J, I, I, I, I, I, J],
        &[I, O, N, M, O, M, P, N, M, M, M, N, N, M, M, L],
        &[I, L, N, N, M, L, L, N, L, L, L, L, L, L, K, M],
        &[I, I, I, I, I, I, I, I, I, I, I, I, I, I, I, I],
        &[I, K, O, L, P, K, N, L, M, N, N, M, L, L, K, L],
        &[I, O, K, O, O, M, N, M, O, N, M, M, N, L, L, L],
        &[K, J, I, I, I, I, I, I, I, I, I, I, I, I, I, I],
    ];
}

use self::lsf_stage2_wb::MAP as LSF_MAP_WB;

const LSF_STAGE2_EXTENSION: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[156, 216, 240, 249, 253, 255, 256],
};

const LSF_PRED_WEIGHT_NB_MB: &[&[u8]] = &[
    &[179, 138, 140, 148, 151, 149, 153, 151, 163],
    &[116, 67, 82, 59, 92, 72, 100, 89, 92],
];

const LSF_PRED_WEIGHT_WB: &[&[u8]] = &[
    &[
        175, 148, 160, 176, 178, 173, 174, 164, 177, 174, 196, 182, 198, 192, 182,
    ],
    &[
        68, 62, 66, 60, 72, 117, 85, 90, 118, 136, 151, 142, 160, 142, 155,
    ],
];

const LSF_PRED_WEIGHT_INDEX_NB_MB: &[&[usize]] = &[
    &[0, 1, 0, 0, 0, 0, 0, 0, 0],
    &[1, 0, 0, 0, 0, 0, 0, 0, 0],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0],
    &[1, 1, 1, 0, 0, 0, 0, 1, 0],
    &[0, 1, 0, 0, 0, 0, 0, 0, 0],
    &[0, 1, 0, 0, 0, 0, 0, 0, 0],
    &[1, 0, 1, 1, 0, 0, 0, 1, 0],
    &[0, 1, 1, 0, 0, 1, 1, 0, 0],
    &[0, 0, 1, 1, 0, 1, 0, 1, 1],
    &[0, 0, 1, 1, 0, 0, 1, 1, 1],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0],
    &[0, 1, 0, 1, 1, 1, 1, 1, 0],
    &[0, 1, 0, 1, 1, 1, 1, 1, 0],
    &[0, 1, 1, 1, 1, 1, 1, 1, 0],
    &[1, 0, 1, 1, 0, 1, 1, 1, 1],
    &[0, 1, 1, 1, 1, 1, 0, 1, 0],
    &[0, 0, 1, 1, 0, 1, 0, 1, 0],
    &[0, 0, 1, 1, 1, 0, 1, 1, 1],
    &[0, 1, 1, 0, 0, 1, 1, 1, 0],
    &[0, 0, 0, 1, 1, 1, 0, 1, 0],
    &[0, 1, 1, 0, 0, 1, 0, 1, 0],
    &[0, 1, 1, 0, 0, 0, 1, 1, 0],
    &[0, 0, 0, 0, 0, 1, 1, 1, 1],
    &[0, 0, 1, 1, 0, 0, 0, 1, 1],
    &[0, 0, 0, 1, 0, 1, 1, 1, 1],
    &[0, 1, 1, 1, 1, 1, 1, 1, 0],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0],
    &[0, 0, 1, 0, 1, 1, 0, 1, 0],
    &[1, 0, 0, 1, 0, 0, 0, 0, 0],
    &[0, 0, 0, 1, 1, 0, 1, 0, 1],
    &[1, 0, 1, 1, 0, 1, 1, 1, 1],
];

const LSF_PRED_WEIGHT_INDEX_WB: &[&[usize]] = &[
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    &[0, 0, 1, 0, 0, 1, 1, 1, 0, 1, 1, 1, 1, 0, 0],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0],
    &[0, 1, 1, 0, 1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 0],
    &[0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0],
    &[0, 1, 1, 0, 0, 0, 1, 0, 1, 1, 1, 0, 1, 0, 1],
    &[0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 1],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    &[0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    &[0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0],
    &[0, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0, 0],
    &[0, 0, 0, 0, 1, 1, 0, 1, 0, 1, 1, 1, 1, 0, 0],
    &[0, 1, 0, 0, 0, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1],
    &[0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0],
    &[0, 1, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 1, 0, 0],
    &[0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 1, 1, 1, 0, 0],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    &[0, 1, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 1, 0],
    &[0, 0, 1, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 0],
    &[0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 1, 0],
    &[0, 0, 0, 1, 1, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    &[0, 1, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 1],
    &[0, 0, 0, 0, 0, 1, 0, 1, 1, 1, 1, 0, 1, 1, 1],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
    &[0, 0, 1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 0, 1, 0],
];

const LSF_CODEBOOK_NB_MB: &[&[u8]] = &[
    &[12, 35, 60, 83, 108, 132, 157, 180, 206, 228],
    &[15, 32, 55, 77, 101, 125, 151, 175, 201, 225],
    &[19, 42, 66, 89, 114, 137, 162, 184, 209, 230],
    &[12, 25, 50, 72, 97, 120, 147, 172, 200, 223],
    &[26, 44, 69, 90, 114, 135, 159, 180, 205, 225],
    &[13, 22, 53, 80, 106, 130, 156, 180, 205, 228],
    &[15, 25, 44, 64, 90, 115, 142, 168, 196, 222],
    &[19, 24, 62, 82, 100, 120, 145, 168, 190, 214],
    &[22, 31, 50, 79, 103, 120, 151, 170, 203, 227],
    &[21, 29, 45, 65, 106, 124, 150, 171, 196, 224],
    &[30, 49, 75, 97, 121, 142, 165, 186, 209, 229],
    &[19, 25, 52, 70, 93, 116, 143, 166, 192, 219],
    &[26, 34, 62, 75, 97, 118, 145, 167, 194, 217],
    &[25, 33, 56, 70, 91, 113, 143, 165, 196, 223],
    &[21, 34, 51, 72, 97, 117, 145, 171, 196, 222],
    &[20, 29, 50, 67, 90, 117, 144, 168, 197, 221],
    &[22, 31, 48, 66, 95, 117, 146, 168, 196, 222],
    &[24, 33, 51, 77, 116, 134, 158, 180, 200, 224],
    &[21, 28, 70, 87, 106, 124, 149, 170, 194, 217],
    &[26, 33, 53, 64, 83, 117, 152, 173, 204, 225],
    &[27, 34, 65, 95, 108, 129, 155, 174, 210, 225],
    &[20, 26, 72, 99, 113, 131, 154, 176, 200, 219],
    &[34, 43, 61, 78, 93, 114, 155, 177, 205, 229],
    &[23, 29, 54, 97, 124, 138, 163, 179, 209, 229],
    &[30, 38, 56, 89, 118, 129, 158, 178, 200, 231],
    &[21, 29, 49, 63, 85, 111, 142, 163, 193, 222],
    &[27, 48, 77, 103, 133, 158, 179, 196, 215, 232],
    &[29, 47, 74, 99, 124, 151, 176, 198, 220, 237],
    &[33, 42, 61, 76, 93, 121, 155, 174, 207, 225],
    &[29, 53, 87, 112, 136, 154, 170, 188, 208, 227],
    &[24, 30, 52, 84, 131, 150, 166, 186, 203, 229],
    &[37, 48, 64, 84, 104, 118, 156, 177, 201, 230],
];

const LSF_CODEBOOK_WB: &[&[u8]] = &[
    &[
        7, 23, 38, 54, 69, 85, 100, 116, 131, 147, 162, 178, 193, 208, 223, 239,
    ],
    &[
        13, 25, 41, 55, 69, 83, 98, 112, 127, 142, 157, 171, 187, 203, 220, 236,
    ],
    &[
        15, 21, 34, 51, 61, 78, 92, 106, 126, 136, 152, 167, 185, 205, 225, 240,
    ],
    &[
        10, 21, 36, 50, 63, 79, 95, 110, 126, 141, 157, 173, 189, 205, 221, 237,
    ],
    &[
        17, 20, 37, 51, 59, 78, 89, 107, 123, 134, 150, 164, 184, 205, 224, 240,
    ],
    &[
        10, 15, 32, 51, 67, 81, 96, 112, 129, 142, 158, 173, 189, 204, 220, 236,
    ],
    &[
        8, 21, 37, 51, 65, 79, 98, 113, 126, 138, 155, 168, 179, 192, 209, 218,
    ],
    &[
        12, 15, 34, 55, 63, 78, 87, 108, 118, 131, 148, 167, 185, 203, 219, 236,
    ],
    &[
        16, 19, 32, 36, 56, 79, 91, 108, 118, 136, 154, 171, 186, 204, 220, 237,
    ],
    &[
        11, 28, 43, 58, 74, 89, 105, 120, 135, 150, 165, 180, 196, 211, 226, 241,
    ],
    &[
        6, 16, 33, 46, 60, 75, 92, 107, 123, 137, 156, 169, 185, 199, 214, 225,
    ],
    &[
        11, 19, 30, 44, 57, 74, 89, 105, 121, 135, 152, 169, 186, 202, 218, 234,
    ],
    &[
        12, 19, 29, 46, 57, 71, 88, 100, 120, 132, 148, 165, 182, 199, 216, 233,
    ],
    &[
        17, 23, 35, 46, 56, 77, 92, 106, 123, 134, 152, 167, 185, 204, 222, 237,
    ],
    &[
        14, 17, 45, 53, 63, 75, 89, 107, 115, 132, 151, 171, 188, 206, 221, 240,
    ],
    &[
        9, 16, 29, 40, 56, 71, 88, 103, 119, 137, 154, 171, 189, 205, 222, 237,
    ],
    &[
        16, 19, 36, 48, 57, 76, 87, 105, 118, 132, 150, 167, 185, 202, 218, 236,
    ],
    &[
        12, 17, 29, 54, 71, 81, 94, 104, 126, 136, 149, 164, 182, 201, 221, 237,
    ],
    &[
        15, 28, 47, 62, 79, 97, 115, 129, 142, 155, 168, 180, 194, 208, 223, 238,
    ],
    &[
        8, 14, 30, 45, 62, 78, 94, 111, 127, 143, 159, 175, 192, 207, 223, 239,
    ],
    &[
        17, 30, 49, 62, 79, 92, 107, 119, 132, 145, 160, 174, 190, 204, 220, 235,
    ],
    &[
        14, 19, 36, 45, 61, 76, 91, 108, 121, 138, 154, 172, 189, 205, 222, 238,
    ],
    &[
        12, 18, 31, 45, 60, 76, 91, 107, 123, 138, 154, 171, 187, 204, 221, 236,
    ],
    &[
        13, 17, 31, 43, 53, 70, 83, 103, 114, 131, 149, 167, 185, 203, 220, 237,
    ],
    &[
        17, 22, 35, 42, 58, 78, 93, 110, 125, 139, 155, 170, 188, 206, 224, 240,
    ],
    &[
        8, 15, 34, 50, 67, 83, 99, 115, 131, 146, 162, 178, 193, 209, 224, 239,
    ],
    &[
        13, 16, 41, 66, 73, 86, 95, 111, 128, 137, 150, 163, 183, 206, 225, 241,
    ],
    &[
        17, 25, 37, 52, 63, 75, 92, 102, 119, 132, 144, 160, 175, 191, 212, 231,
    ],
    &[
        19, 31, 49, 65, 83, 100, 117, 133, 147, 161, 174, 187, 200, 213, 227, 242,
    ],
    &[
        18, 31, 52, 68, 88, 103, 117, 126, 138, 149, 163, 177, 192, 207, 223, 239,
    ],
    &[
        16, 29, 47, 61, 76, 90, 106, 119, 133, 147, 161, 176, 193, 209, 224, 240,
    ],
    &[
        15, 21, 35, 50, 61, 73, 86, 97, 110, 119, 129, 141, 175, 198, 218, 237,
    ],
];

/*
    for codebook in codebooks {
        let w: Vec<u32> = codebook.windows(3).map(|code| {
            let prev = code[0] as u32;
            let cur  = code[1] as u32;
            let next = code[2] as u32;

            let weight = (1024 / (cur - prev) + 1024 / (next - cur)) << 16;
            let i = (weight as usize).ilog();
            let f = (weight >> (i - 8)) & 127;
            let y = (if i & 1 != 0 { 32768 } else { 46214 }) >> ((32 - i) >> 1);
            y + ((213 * f * y) >> 16)
        }).collect();

        println!("&{:?},", w);
    }
*/

const LSF_WEIGHT_NB_MB: &[&[u16]] = &[
    &[2897, 2314, 2314, 2314, 2287, 2287, 2314, 2300, 2327, 2287],
    &[2888, 2580, 2394, 2367, 2314, 2274, 2274, 2274, 2274, 2194],
    &[2487, 2340, 2340, 2314, 2314, 2314, 2340, 2340, 2367, 2354],
    &[3216, 2766, 2340, 2340, 2314, 2274, 2221, 2207, 2261, 2194],
    &[2460, 2474, 2367, 2394, 2394, 2394, 2394, 2367, 2407, 2314],
    &[3479, 3056, 2127, 2207, 2274, 2274, 2274, 2287, 2314, 2261],
    &[3282, 3141, 2580, 2394, 2247, 2221, 2207, 2194, 2194, 2114],
    &[4096, 3845, 2221, 2620, 2620, 2407, 2314, 2394, 2367, 2074],
    &[3178, 3244, 2367, 2221, 2553, 2434, 2340, 2314, 2167, 2221],
    &[3338, 3488, 2726, 2194, 2261, 2460, 2354, 2367, 2207, 2101],
    &[2354, 2420, 2327, 2367, 2394, 2420, 2420, 2420, 2460, 2367],
    &[3779, 3629, 2434, 2527, 2367, 2274, 2274, 2300, 2207, 2048],
    &[3254, 3225, 2713, 2846, 2447, 2327, 2300, 2300, 2274, 2127],
    &[3263, 3300, 2753, 2806, 2447, 2261, 2261, 2247, 2127, 2101],
    &[2873, 2981, 2633, 2367, 2407, 2354, 2194, 2247, 2247, 2114],
    &[3225, 3197, 2633, 2580, 2274, 2181, 2247, 2221, 2221, 2141],
    &[3178, 3310, 2740, 2407, 2274, 2274, 2274, 2287, 2194, 2114],
    &[3141, 3272, 2460, 2061, 2287, 2500, 2367, 2487, 2434, 2181],
    &[3507, 3282, 2314, 2700, 2647, 2474, 2367, 2394, 2340, 2127],
    &[3423, 3535, 3038, 3056, 2300, 1950, 2221, 2274, 2274, 2274],
    &[3404, 3366, 2087, 2687, 2873, 2354, 2420, 2274, 2474, 2540],
    &[3760, 3488, 1950, 2660, 2897, 2527, 2394, 2367, 2460, 2261],
    &[3028, 3272, 2740, 2888, 2740, 2154, 2127, 2287, 2234, 2247],
    &[3695, 3657, 2025, 1969, 2660, 2700, 2580, 2500, 2327, 2367],
    &[3207, 3413, 2354, 2074, 2888, 2888, 2340, 2487, 2247, 2167],
    &[3338, 3366, 2846, 2780, 2327, 2154, 2274, 2287, 2114, 2061],
    &[2327, 2300, 2181, 2167, 2181, 2367, 2633, 2700, 2700, 2553],
    &[2407, 2434, 2221, 2261, 2221, 2221, 2340, 2420, 2607, 2700],
    &[3038, 3244, 2806, 2888, 2474, 2074, 2300, 2314, 2354, 2380],
    &[2221, 2154, 2127, 2287, 2500, 2793, 2793, 2620, 2580, 2367],
    &[3676, 3713, 2234, 1838, 2181, 2753, 2726, 2673, 2513, 2207],
    &[2793, 3160, 2726, 2553, 2846, 2513, 2181, 2394, 2221, 2181],
];

const LSF_WEIGHT_WB: &[&[u16]] = &[
    &[
        3657, 2925, 2925, 2925, 2925, 2925, 2925, 2925, 2925, 2925, 2925, 2925, 2963, 2963, 2925,
        2846,
    ],
    &[
        3216, 3085, 2972, 3056, 3056, 3010, 3010, 3010, 2963, 2963, 3010, 2972, 2888, 2846, 2846,
        2726,
    ],
    &[
        3920, 4014, 2981, 3207, 3207, 2934, 3056, 2846, 3122, 3244, 2925, 2846, 2620, 2553, 2780,
        2925,
    ],
    &[
        3516, 3197, 3010, 3103, 3019, 2888, 2925, 2925, 2925, 2925, 2888, 2888, 2888, 2888, 2888,
        2753,
    ],
    &[
        5054, 5054, 2934, 3573, 3385, 3056, 3085, 2793, 3160, 3160, 2972, 2846, 2513, 2540, 2753,
        2888,
    ],
    &[
        4428, 4149, 2700, 2753, 2972, 3010, 2925, 2846, 2981, 3019, 2925, 2925, 2925, 2925, 2888,
        2726,
    ],
    &[
        3620, 3019, 2972, 3056, 3056, 2873, 2806, 3056, 3216, 3047, 2981, 3291, 3291, 2981, 3310,
        2991,
    ],
    &[
        5227, 5014, 2540, 3338, 3526, 3385, 3197, 3094, 3376, 2981, 2700, 2647, 2687, 2793, 2846,
        2673,
    ],
    &[
        5081, 5174, 4615, 4428, 2460, 2897, 3047, 3207, 3169, 2687, 2740, 2888, 2846, 2793, 2846,
        2700,
    ],
    &[
        3122, 2888, 2963, 2925, 2925, 2925, 2925, 2963, 2963, 2963, 2963, 2925, 2925, 2963, 2963,
        2963,
    ],
    &[
        4202, 3207, 2981, 3103, 3010, 2888, 2888, 2925, 2972, 2873, 2916, 3019, 2972, 3010, 3197,
        2873,
    ],
    &[
        3760, 3760, 3244, 3103, 2981, 2888, 2925, 2888, 2972, 2934, 2793, 2793, 2846, 2888, 2888,
        2660,
    ],
    &[
        3854, 4014, 3207, 3122, 3244, 2934, 3047, 2963, 2963, 3085, 2846, 2793, 2793, 2793, 2793,
        2580,
    ],
    &[
        3845, 4080, 3357, 3516, 3094, 2740, 3010, 2934, 3122, 3085, 2846, 2846, 2647, 2647, 2846,
        2806,
    ],
    &[
        5147, 4894, 3225, 3845, 3441, 3169, 2897, 3413, 3451, 2700, 2580, 2673, 2740, 2846, 2806,
        2753,
    ],
    &[
        4109, 3789, 3291, 3160, 2925, 2888, 2888, 2925, 2793, 2740, 2793, 2740, 2793, 2846, 2888,
        2806,
    ],
    &[
        5081, 5054, 3047, 3545, 3244, 3056, 3085, 2944, 3103, 2897, 2740, 2740, 2740, 2846, 2793,
        2620,
    ],
    &[
        4309, 4309, 2860, 2527, 3207, 3376, 3376, 3075, 3075, 3376, 3056, 2846, 2647, 2580, 2726,
        2753,
    ],
    &[
        3056, 2916, 2806, 2888, 2740, 2687, 2897, 3103, 3150, 3150, 3216, 3169, 3056, 3010, 2963,
        2846,
    ],
    &[
        4375, 3882, 2925, 2888, 2846, 2888, 2846, 2846, 2888, 2888, 2888, 2846, 2888, 2925, 2888,
        2846,
    ],
    &[
        2981, 2916, 2916, 2981, 2981, 3056, 3122, 3216, 3150, 3056, 3010, 2972, 2972, 2972, 2925,
        2740,
    ],
    &[
        4229, 4149, 3310, 3347, 2925, 2963, 2888, 2981, 2981, 2846, 2793, 2740, 2846, 2846, 2846,
        2793,
    ],
    &[
        4080, 4014, 3103, 3010, 2925, 2925, 2925, 2888, 2925, 2925, 2846, 2846, 2846, 2793, 2888,
        2780,
    ],
    &[
        4615, 4575, 3169, 3441, 3207, 2981, 2897, 3038, 3122, 2740, 2687, 2687, 2687, 2740, 2793,
        2700,
    ],
    &[
        4149, 4269, 3789, 3657, 2726, 2780, 2888, 2888, 3010, 2972, 2925, 2846, 2687, 2687, 2793,
        2888,
    ],
    &[
        4215, 3554, 2753, 2846, 2846, 2888, 2888, 2888, 2925, 2925, 2888, 2925, 2925, 2925, 2963,
        2888,
    ],
    &[
        5174, 4921, 2261, 3432, 3789, 3479, 3347, 2846, 3310, 3479, 3150, 2897, 2460, 2487, 2753,
        2925,
    ],
    &[
        3451, 3685, 3122, 3197, 3357, 3047, 3207, 3207, 2981, 3216, 3085, 2925, 2925, 2687, 2540,
        2434,
    ],
    &[
        2981, 3010, 2793, 2793, 2740, 2793, 2846, 2972, 3056, 3103, 3150, 3150, 3150, 3103, 3010,
        3010,
    ],
    &[
        2944, 2873, 2687, 2726, 2780, 3010, 3432, 3545, 3357, 3244, 3056, 3010, 2963, 2925, 2888,
        2846,
    ],
    &[
        3019, 2944, 2897, 3010, 3010, 2972, 3019, 3103, 3056, 3056, 3010, 2888, 2846, 2925, 2925,
        2888,
    ],
    &[
        3920, 3967, 3010, 3197, 3357, 3216, 3291, 3291, 3479, 3704, 3441, 2726, 2181, 2460, 2580,
        2607,
    ],
];

const LSF_MIN_SPACING_NB_MB: &[i16] = &[250, 3, 6, 3, 3, 3, 4, 3, 3, 3, 461];

const LSF_MIN_SPACING_WB: &[i16] = &[100, 3, 40, 3, 3, 3, 5, 14, 14, 10, 11, 3, 8, 9, 7, 3, 347];

const LSF_INTERPOLATION_INDEX: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[13, 35, 64, 75, 256],
};

const LSF_ORDERING_NB_MB: &[u8] = &[0, 9, 6, 3, 4, 5, 8, 1, 2, 7];
const LSF_ORDERING_WB: &[u8] = &[0, 15, 8, 7, 4, 11, 12, 3, 2, 13, 10, 5, 6, 9, 14, 1];

const COSINE: &[i16] = &[
    4096, 4095, 4091, 4085, 4076, 4065, 4052, 4036, 4017, 3997, 3973, 3948, 3920, 3889, 3857, 3822,
    3784, 3745, 3703, 3659, 3613, 3564, 3513, 3461, 3406, 3349, 3290, 3229, 3166, 3102, 3035, 2967,
    2896, 2824, 2751, 2676, 2599, 2520, 2440, 2359, 2276, 2191, 2106, 2019, 1931, 1842, 1751, 1660,
    1568, 1474, 1380, 1285, 1189, 1093, 995, 897, 799, 700, 601, 501, 401, 301, 201, 101, 0, -101,
    -201, -301, -401, -501, -601, -700, -799, -897, -995, -1093, -1189, -1285, -1380, -1474, -1568,
    -1660, -1751, -1842, -1931, -2019, -2106, -2191, -2276, -2359, -2440, -2520, -2599, -2676,
    -2751, -2824, -2896, -2967, -3035, -3102, -3166, -3229, -3290, -3349, -3406, -3461, -3513,
    -3564, -3613, -3659, -3703, -3745, -3784, -3822, -3857, -3889, -3920, -3948, -3973, -3997,
    -4017, -4036, -4052, -4065, -4076, -4085, -4091, -4095, -4096,
];

const PITCH_DELTA: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[
        46, 48, 50, 53, 57, 63, 73, 88, 114, 152, 182, 204, 219, 229, 236, 242, 246, 250, 252, 254,
        256,
    ],
};

#[derive(Default, Debug, PartialEq, Clone, Copy)]
struct FrameType {
    active: bool,
    voiced: bool,
    high: bool,
}

/*
    InactiveLow  = 0b000,
    InactiveHigh = 0b001,
    UnvoicedLow  = 0b010,
    UnvoicedHigh = 0b011,
    VoicedLow    = 0b100,
    VoicedHigh   = 0b101,
*/

/*
impl Default for FrameType {
    fn default() -> Self {
        FrameType::VoicedLow
    }
}
*/

impl FrameType {
    #[inline(always)]
    fn voiced_index(&self) -> usize {
        self.voiced as usize
    }
    #[inline(always)]
    fn signal_type_index(&self) -> usize {
        (self.voiced as usize) + (self.active as usize)
    }
    #[inline(always)]
    fn qoffset_type_index(&self) -> usize {
        self.high as usize
    }
}

trait Log2Lin: Copy {
    fn log2lin(self) -> usize;
}

impl Log2Lin for isize {
    fn log2lin(self) -> usize {
        let i = 1 << (self >> 7);
        let f = self & 127;

        (i + ((-174 * f * (128 - f) >> 16) + f) * (i >> 7)) as usize
    }
}

trait ExMath: Into<i64> + Copy {
    fn mul_shift<I: Into<i64>>(self, other: I, bits: usize) -> i32 {
        let a: i64 = self.into();
        let b: i64 = other.into();

        ((a * b) >> bits) as i32
    }

    fn mul_round<I: Into<i64>>(self, other: I, bits: u64) -> i32 {
        let a: i64 = self.into();
        let b: i64 = other.into();

        (((a * b) + (1 << (bits - 1))) >> bits) as i32
    }
}

impl ExMath for i32 {}

// TODO: refactor once
pub trait Band {
    const ORDER: usize;
    const STEP: i32;

    const STAGE1: &'static [&'static ICDFContext];
    const MAP: &'static [&'static [&'static ICDFContext]];
    const PRED_WEIGHT: &'static [&'static [u8]];
    const PRED_WEIGHT_INDEX: &'static [&'static [usize]];
    const WEIGHT: &'static [&'static [u16]];
    const CODEBOOK: &'static [&'static [u8]];
    const MIN_SPACING: &'static [i16];
    const ORDERING: &'static [u8];

    // TODO: write a proper test for it
    fn stabilize(nlsfs: &mut [i16]) {
        for _ in 0..20 {
            let mut k = 0;
            let mut min_diff = 0;

            for (i, &spacing) in Self::MIN_SPACING.iter().enumerate() {
                let low = if i == 0 { 0 } else { nlsfs[i - 1] } as i32;
                let high = if i == Self::ORDER {
                    32768
                } else {
                    nlsfs[i] as i32
                };
                let diff = high - low - spacing as i32;

                if diff < min_diff {
                    min_diff = diff;
                    k = i;
                }
            }

            //            println!("min_diff {} k {}", min_diff, k);

            if min_diff == 0 {
                return;
            }

            if k == 0 {
                nlsfs[0] = Self::MIN_SPACING[0];
            } else if k == Self::ORDER {
                nlsfs[Self::ORDER - 1] = (32768 - Self::MIN_SPACING[Self::ORDER] as i32) as i16;
            } else {
                /*                println!("min_delta {:#?} {} {}", Self::MIN_SPACING,
                         Self::MIN_SPACING[..k].iter().sum::<i16>(),
                         32768 - Self::MIN_SPACING[k + 1..].iter().sum::<i16>() as i32); */
                let half_delta = Self::MIN_SPACING[k] as i32 >> 1;
                let min_center = Self::MIN_SPACING[..k].iter().sum::<i16>() as i32 + half_delta;
                let max_center =
                    32768 - Self::MIN_SPACING[k + 1..].iter().sum::<i16>() as i32 - half_delta;
                let delta = nlsfs[k - 1] as i32 + nlsfs[k] as i32;
                let center = (delta >> 1) + (delta & 1);

                //                println!("{} {} {} {} {}", delta, half_delta, min_center, max_center, center);

                nlsfs[k - 1] = (center.max(min_center).min(max_center) - half_delta) as i16;
                nlsfs[k] = nlsfs[k - 1] + Self::MIN_SPACING[k];
            }
        }

        nlsfs.sort_unstable();

        let mut prev = 0;
        for (nlsf, &spacing) in nlsfs.iter_mut().zip(Self::MIN_SPACING) {
            let v = prev + spacing;
            if *nlsf < v {
                *nlsf = v;
            }
            prev = *nlsf;
        }

        let mut next = 32768;
        for (nlsf, &spacing) in nlsfs.iter_mut().zip(&Self::MIN_SPACING[1..]).rev() {
            let v = next - spacing as i32;
            if *nlsf as i32 > v {
                *nlsf = v as i16;
            }
            next = *nlsf as i32;
        }
    }

    fn is_stable(lpcs: &[i16]) -> bool {
        let mut dc_resp = 0;
        let mut even = vec![0; Self::ORDER];
        let mut odd = vec![0; Self::ORDER];
        let mut invgain = 1 << 30;

        for (c, &lpc) in even.iter_mut().zip(lpcs.iter()) {
            let l = lpc as i32;
            dc_resp += l;
            *c = l * 4096;
        }

        if dc_resp > 4096 {
            return false;
        }

        let mut k = Self::ORDER - 1;
        let mut a = even[k];

        loop {
            if a.abs() > 16773022 {
                return false;
            }

            let rc = -a * 128;
            let div = (1 << 30) - rc.mul_shift(rc, 32);

            invgain = invgain.mul_shift(div, 32) << 2;

            if k == 0 {
                return invgain >= 107374;
            }

            let b1 = div.ilog();
            let b2 = b1 - 16;
            let inv = ((1 << 29) - 1) / (div >> (b2 + 1));
            let err = (1 << 29) - (div << (15 - b2)).mul_shift(inv, 16);
            let gain = (inv << 16) + (err * inv >> 13);

            let (prev, cur) = if k & 1 != 0 {
                (&mut even, &mut odd)
            } else {
                (&mut odd, &mut even)
            };

            for j in 0..k {
                let v = prev[j] - prev[k - j - 1].mul_shift(rc, 31);
                cur[j] = v.mul_shift(gain, b1 as usize);
            }

            k -= 1;

            a = cur[k];
        }
    }

    fn range_limit(lpcs: &mut [f32], a: &mut [i32]) {
        let mut lpc = vec![0; Self::ORDER];
        let mut deadline = true;
        for _ in 0..10 {
            // max_by() returns the last maximum the spec requires
            // the first.
            let (k, &maxabs) = a
                .iter()
                .enumerate()
                .rev()
                .max_by_key(|&(_i, v)| v.abs())
                .unwrap();

            let maxabs = ((maxabs.abs() + (1 << 4)) >> 5) as u32;

            if maxabs > 32767 {
                let max = maxabs.max(163838);
                let start = 65470 - ((max - 32767) << 14) / ((max * (k as u32 + 1)) >> 2);
                let mut chirp = start;

                for v in a.iter_mut() {
                    *v = v.mul_round(chirp, 16);
                    chirp = ((start as u32 * chirp as u32 + 32768) >> 16) as u32;
                }
            } else {
                deadline = false;
                break;
            }
        }

        if deadline {
            for (v, l) in a.iter_mut().zip(lpc.iter_mut()) {
                let v16 = ((*v + 16) >> 5)
                    .min(i16::max_value() as i32)
                    .max(i16::min_value() as i32);
                *l = v16 as i16;
                *v = v16 << 5;
            }
        } else {
            for (&v, l) in a.iter().zip(lpc.iter_mut()) {
                *l = ((v + 16) >> 5) as i16;
            }
        }

        for i in 1..16 + 1 {
            if Self::is_stable(&lpc) {
                break;
            }
            let start = 65536u32 - (1 << i);
            let mut chirp = start;

            for (v, l) in a.iter_mut().zip(lpc.iter_mut()) {
                *v = v.mul_round(chirp, 16);
                *l = ((*v + (1 << 4)) >> 5) as i16;

                chirp = (start * chirp + 32768) >> 16;
            }
        }

        for (d, &l) in lpcs.iter_mut().zip(lpc.iter()) {
            *d = (l as f32) / 4096f32;
        }
    }

    fn lsf_to_lpc<'a, I>(lpcs: &'a mut [f32], nlsfs: I)
    where
        I: IntoIterator<Item = i16>,
    {
        let mut lsps = vec![0; Self::ORDER];
        let mut p = vec![0; Self::ORDER / 2 + 1];
        let mut q = vec![0; Self::ORDER / 2 + 1];

        for (&ord, nlsf) in Self::ORDERING.iter().zip(nlsfs) {
            let idx = (nlsf >> 8) as usize;
            let off = (nlsf & 255) as i32;

            let cos = COSINE[idx] as i32;
            let next_cos = COSINE[idx + 1] as i32;

            lsps[ord as usize] = (cos * 256 + (next_cos - cos) * off + 4) >> 3;
        }

        p[0] = 65536;
        q[0] = 65536;
        p[1] = -lsps[0];
        q[1] = -lsps[1];

        // println!("{:#?}", lsps);
        // TODO: fuse p and q as even/odd and zip it
        for (i, lsp) in lsps[2..].chunks(2).enumerate() {
            p[i + 2] = p[i] * 2 - lsp[0].mul_round(p[i + 1], 16);
            /* println!(
                "[{}] {} = {} * 2 - {} * {}",
                i + 2,
                p[i + 2],
                p[i],
                lsp[0],
                p[i + 1]
            ); */
            q[i + 2] = q[i] * 2 - lsp[1].mul_round(q[i + 1], 16);

            // TODO: benchmark let mut w = &p[j-2..j+1]
            // would be p[0..i+1].windows_mut(3).rev()
            for j in (2..i + 2).rev() {
                let v = p[j - 2] - lsp[0].mul_round(p[j - 1], 16);
                p[j] += v;
                // println!(" [{}] {} = {} - {} * {}", j, v, p[j - 2], lsp[0], p[j - 1]);
                q[j] += q[j - 2] - lsp[1].mul_round(q[j - 1], 16);
            }

            p[1] -= lsp[0];
            q[1] -= lsp[1];
        }

        // println!("{:#?}", p);
        // println!("{:#?}", q);

        let mut a = vec![0; Self::ORDER];
        {
            let (a0, a1) = a.split_at_mut(Self::ORDER / 2);
            let it = a0.iter_mut().zip(a1.iter_mut().rev());
            let co = p.windows(2).zip(q.windows(2));
            for ((v0, v1), (pv, qv)) in it.zip(co) {
                let ps = pv[0] + pv[1];
                // println!("{} = {} + {}", ps, pv[0], pv[1]);
                let qs = qv[1] - qv[0];
                //                println!("{} = {} + {}", qs, qv[0], qv[1]);
                *v0 = -ps - qs;
                *v1 = -ps + qs;
            }
        }

        // println!("{:#?}", a);

        Self::range_limit(lpcs, &mut a);
    }
}

trait PitchLag {
    const LOW_PART: &'static ICDFContext;

    const MIN_LAG: u16;
    const MAX_LAG: u16;

    const SCALE: u16;

    const OFFSET: &'static [&'static [&'static [i8]]];
    const CONTOUR: &'static [&'static ICDFContext];
}

pub struct NB_MB;
pub struct WB;
pub struct MB;
pub struct NB;

impl Band for NB_MB {
    const ORDER: usize = 10;
    const STEP: i32 = 11796;

    const STAGE1: &'static [&'static ICDFContext] = LSF_STAGE1_NB_MB;
    const MAP: &'static [&'static [&'static ICDFContext]] = LSF_MAP_NB_MB;
    const PRED_WEIGHT: &'static [&'static [u8]] = LSF_PRED_WEIGHT_NB_MB;
    const PRED_WEIGHT_INDEX: &'static [&'static [usize]] = LSF_PRED_WEIGHT_INDEX_NB_MB;
    const WEIGHT: &'static [&'static [u16]] = LSF_WEIGHT_NB_MB;
    const CODEBOOK: &'static [&'static [u8]] = LSF_CODEBOOK_NB_MB;
    const MIN_SPACING: &'static [i16] = LSF_MIN_SPACING_NB_MB;
    const ORDERING: &'static [u8] = LSF_ORDERING_NB_MB;
}

impl Band for WB {
    const ORDER: usize = 16;
    const STEP: i32 = 9830;

    const STAGE1: &'static [&'static ICDFContext] = LSF_STAGE1_WB;
    const MAP: &'static [&'static [&'static ICDFContext]] = LSF_MAP_WB;
    const PRED_WEIGHT: &'static [&'static [u8]] = LSF_PRED_WEIGHT_WB;
    const PRED_WEIGHT_INDEX: &'static [&'static [usize]] = LSF_PRED_WEIGHT_INDEX_WB;
    const WEIGHT: &'static [&'static [u16]] = LSF_WEIGHT_WB;
    const CODEBOOK: &'static [&'static [u8]] = LSF_CODEBOOK_WB;
    const MIN_SPACING: &'static [i16] = LSF_MIN_SPACING_WB;
    const ORDERING: &'static [u8] = LSF_ORDERING_WB;
}

const PITCH_HIGH_PART: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[
        3, 6, 12, 23, 44, 74, 106, 125, 136, 146, 158, 171, 184, 196, 207, 216, 224, 231, 237, 241,
        243, 245, 247, 248, 249, 250, 251, 252, 253, 254, 255, 256,
    ],
};

const PITCH_OFFSET_NB: &[&[&[i8]]] = &[
    &[&[0, 0], &[1, 0], &[0, 1]],
    &[
        &[0, 0, 0, 0],
        &[2, 1, 0, -1],
        &[-1, 0, 1, 2],
        &[-1, 0, 0, 1],
        &[-1, 0, 0, 0],
        &[0, 0, 0, 1],
        &[0, 0, 1, 1],
        &[1, 1, 0, 0],
        &[1, 0, 0, 0],
        &[0, 0, 0, -1],
        &[1, 0, 0, -1],
    ],
];

const PITCH_CONTOUR_NB: &[&ICDFContext] = &[
    &ICDFContext {
        total: 256,
        dist: &[143, 193, 256],
    },
    &ICDFContext {
        total: 256,
        dist: &[68, 80, 101, 118, 137, 159, 189, 213, 230, 246, 256],
    },
];

const PITCH_OFFSET_MB_WB: &[&[&[i8]]] = &[
    &[
        &[0, 0],
        &[0, 1],
        &[1, 0],
        &[-1, 1],
        &[1, -1],
        &[-1, 2],
        &[2, -1],
        &[-2, 2],
        &[2, -2],
        &[-2, 3],
        &[3, -2],
        &[-3, 3],
    ],
    &[
        &[0, 0, 0, 0],
        &[0, 0, 1, 1],
        &[1, 1, 0, 0],
        &[-1, 0, 0, 0],
        &[0, 0, 0, 1],
        &[1, 0, 0, 0],
        &[-1, 0, 0, 1],
        &[0, 0, 0, -1],
        &[-1, 0, 1, 2],
        &[1, 0, 0, -1],
        &[-2, -1, 1, 2],
        &[2, 1, 0, -1],
        &[-2, 0, 0, 2],
        &[-2, 0, 1, 3],
        &[2, 1, -1, -2],
        &[-3, -1, 1, 3],
        &[2, 0, 0, -2],
        &[3, 1, 0, -2],
        &[-3, -1, 2, 4],
        &[-4, -1, 1, 4],
        &[3, 1, -1, -3],
        &[-4, -1, 2, 5],
        &[4, 2, -1, -3],
        &[4, 1, -1, -4],
        &[-5, -1, 2, 6],
        &[5, 2, -1, -4],
        &[-6, -2, 2, 6],
        &[-5, -2, 2, 5],
        &[6, 2, -1, -5],
        &[-7, -2, 3, 8],
        &[6, 2, -2, -6],
        &[5, 2, -2, -5],
        &[8, 3, -2, -7],
        &[-9, -3, 3, 9],
    ],
];

const PITCH_CONTOUR_MB_WB: &[&ICDFContext] = &[
    &ICDFContext {
        total: 256,
        dist: &[91, 137, 176, 195, 209, 221, 229, 236, 242, 247, 252, 256],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            33, 55, 73, 89, 104, 118, 132, 145, 158, 168, 177, 186, 194, 200, 206, 212, 217, 221,
            225, 229, 232, 235, 238, 240, 242, 244, 246, 248, 250, 252, 253, 254, 255, 256,
        ],
    },
];

impl PitchLag for NB {
    const LOW_PART: &'static ICDFContext = &ICDFContext {
        total: 256,
        dist: &[64, 128, 192, 256],
    };

    const MIN_LAG: u16 = 16;
    const MAX_LAG: u16 = 144;

    const SCALE: u16 = 4;

    const OFFSET: &'static [&'static [&'static [i8]]] = PITCH_OFFSET_NB;
    const CONTOUR: &'static [&'static ICDFContext] = PITCH_CONTOUR_NB;
}

impl PitchLag for MB {
    const LOW_PART: &'static ICDFContext = &ICDFContext {
        total: 256,
        dist: &[43, 85, 128, 171, 213, 256],
    };

    const MIN_LAG: u16 = 24;
    const MAX_LAG: u16 = 216;

    const SCALE: u16 = 6;

    const OFFSET: &'static [&'static [&'static [i8]]] = PITCH_OFFSET_MB_WB;
    const CONTOUR: &'static [&'static ICDFContext] = PITCH_CONTOUR_MB_WB;
}

impl PitchLag for WB {
    const LOW_PART: &'static ICDFContext = &ICDFContext {
        total: 256,
        dist: &[32, 64, 96, 128, 160, 192, 224, 256],
    };

    const MIN_LAG: u16 = 32;
    const MAX_LAG: u16 = 288;

    const SCALE: u16 = 8;

    const OFFSET: &'static [&'static [&'static [i8]]] = PITCH_OFFSET_MB_WB;
    const CONTOUR: &'static [&'static ICDFContext] = PITCH_CONTOUR_MB_WB;
}

const LTP_PERIODICITY: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[77, 157, 256],
};

const LTP_FILTER: &[&ICDFContext] = &[
    &ICDFContext {
        total: 256,
        dist: &[185, 200, 213, 226, 235, 244, 250, 256],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            57, 91, 112, 132, 147, 160, 172, 185, 195, 205, 214, 224, 233, 241, 248, 256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            15, 31, 45, 57, 69, 81, 92, 103, 114, 124, 133, 142, 151, 160, 168, 176, 184, 192, 199,
            206, 212, 218, 223, 227, 232, 236, 240, 244, 247, 251, 254, 256,
        ],
    },
];

const LTP_TAPS: &[&[&[i8]]] = &[
    &[
        &[4, 6, 24, 7, 5],
        &[0, 0, 2, 0, 0],
        &[12, 28, 41, 13, -4],
        &[-9, 15, 42, 25, 14],
        &[1, -2, 62, 41, -9],
        &[-10, 37, 65, -4, 3],
        &[-6, 4, 66, 7, -8],
        &[16, 14, 38, -3, 33],
    ],
    &[
        &[13, 22, 39, 23, 12],
        &[-1, 36, 64, 27, -6],
        &[-7, 10, 55, 43, 17],
        &[1, 1, 8, 1, 1],
        &[6, -11, 74, 53, -9],
        &[-12, 55, 76, -12, 8],
        &[-3, 3, 93, 27, -4],
        &[26, 39, 59, 3, -8],
        &[2, 0, 77, 11, 9],
        &[-8, 22, 44, -6, 7],
        &[40, 9, 26, 3, 9],
        &[-7, 20, 101, -7, 4],
        &[3, -8, 42, 26, 0],
        &[-15, 33, 68, 2, 23],
        &[-2, 55, 46, -2, 15],
        &[3, -1, 21, 16, 41],
    ],
    &[
        &[-6, 27, 61, 39, 5],
        &[-11, 42, 88, 4, 1],
        &[-2, 60, 65, 6, -4],
        &[-1, -5, 73, 56, 1],
        &[-9, 19, 94, 29, -9],
        &[0, 12, 99, 6, 4],
        &[8, -19, 102, 46, -13],
        &[3, 2, 13, 3, 2],
        &[9, -21, 84, 72, -18],
        &[-11, 46, 104, -22, 8],
        &[18, 38, 48, 23, 0],
        &[-16, 70, 83, -21, 11],
        &[5, -11, 117, 22, -8],
        &[-6, 23, 117, -12, 3],
        &[3, -8, 95, 28, 4],
        &[-10, 15, 77, 60, -15],
        &[-1, 4, 124, 2, -4],
        &[3, 38, 84, 24, -25],
        &[2, 13, 42, 13, 31],
        &[21, -4, 56, 46, -1],
        &[-1, 35, 79, -13, 19],
        &[-7, 65, 88, -9, -14],
        &[20, 4, 81, 49, -29],
        &[20, 0, 75, 3, -17],
        &[5, -9, 44, 92, -8],
        &[1, -3, 22, 69, 31],
        &[-6, 95, 41, -12, 5],
        &[39, 67, 16, -4, 1],
        &[0, -6, 120, 55, -36],
        &[-13, 44, 122, 4, -24],
        &[81, 5, 11, 3, 7],
        &[2, 0, 9, 10, 88],
    ],
];

const LTP_SCALE: &[u16] = &[15565, 12288, 8192];

const LTP_SCALE_INDEX: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[128, 192, 256],
};

const LTP_ORDER: usize = 5;
const RES_HISTORY: usize = 288 + LTP_ORDER / 2;
const LPC_HISTORY: usize = 322;

const LCG_SEED: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[64, 128, 192, 256],
};

trait ShellBlock {
    const SHELL_BLOCKS: &'static [u8];
}

impl ShellBlock for NB {
    const SHELL_BLOCKS: &'static [u8] = &[5, 10];
}

impl ShellBlock for MB {
    const SHELL_BLOCKS: &'static [u8] = &[8, 15];
}

impl ShellBlock for WB {
    const SHELL_BLOCKS: &'static [u8] = &[10, 20];
}

const EXC_RATE: &[&ICDFContext] = &[
    &ICDFContext {
        total: 256,
        dist: &[15, 66, 78, 124, 169, 182, 215, 242, 256],
    },
    &ICDFContext {
        total: 256,
        dist: &[33, 63, 99, 116, 150, 199, 217, 238, 256],
    },
];

const PULSE_COUNT: &[&ICDFContext] = &[
    &ICDFContext {
        total: 256,
        dist: &[
            131, 205, 230, 238, 241, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255,
            256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            58, 151, 211, 234, 241, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255,
            256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            43, 94, 140, 173, 197, 213, 224, 232, 238, 241, 244, 247, 249, 250, 251, 253, 254, 256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            17, 69, 140, 197, 228, 240, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255, 256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            6, 27, 68, 121, 170, 205, 226, 237, 243, 246, 248, 250, 251, 252, 253, 254, 255, 256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            7, 21, 43, 71, 100, 128, 153, 173, 190, 203, 214, 223, 230, 235, 239, 243, 246, 256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            2, 7, 21, 50, 92, 138, 179, 210, 229, 240, 246, 249, 251, 252, 253, 254, 255, 256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            1, 3, 7, 17, 36, 65, 100, 137, 171, 199, 219, 233, 241, 246, 250, 252, 254, 256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            1, 3, 5, 10, 19, 33, 53, 77, 104, 132, 158, 181, 201, 216, 227, 235, 241, 256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            1, 2, 3, 9, 36, 94, 150, 189, 214, 228, 238, 244, 247, 250, 252, 253, 254, 256,
        ],
    },
    &ICDFContext {
        total: 256,
        dist: &[
            2, 3, 9, 36, 94, 150, 189, 214, 228, 238, 244, 247, 250, 252, 253, 254, 256, 256,
        ],
    },
];

const PULSE_LOCATION: &[&[&ICDFContext]] = &[
    &[
        &ICDFContext {
            total: 256,
            dist: &[126, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[56, 198, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[25, 126, 230, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[12, 72, 180, 244, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[7, 42, 126, 213, 250, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[4, 24, 83, 169, 232, 253, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[3, 15, 53, 125, 200, 242, 254, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[2, 10, 35, 89, 162, 221, 248, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[2, 7, 24, 63, 126, 191, 233, 251, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 5, 17, 45, 94, 157, 211, 241, 252, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 5, 13, 33, 70, 125, 182, 223, 245, 253, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 4, 11, 26, 54, 98, 151, 199, 232, 248, 254, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 3, 9, 21, 42, 77, 124, 172, 212, 237, 249, 254, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[
                1, 2, 6, 16, 33, 60, 97, 144, 187, 220, 241, 250, 254, 255, 256,
            ],
        },
        &ICDFContext {
            total: 256,
            dist: &[
                1, 2, 3, 11, 25, 47, 80, 120, 163, 201, 229, 245, 253, 254, 255, 256,
            ],
        },
        &ICDFContext {
            total: 256,
            dist: &[
                1, 2, 3, 4, 17, 35, 62, 98, 139, 180, 214, 238, 252, 253, 254, 255, 256,
            ],
        },
    ],
    &[
        &ICDFContext {
            total: 256,
            dist: &[127, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[53, 202, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[22, 127, 233, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[11, 72, 183, 246, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[6, 41, 127, 215, 251, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[4, 24, 83, 170, 232, 253, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[3, 16, 56, 127, 200, 241, 254, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[3, 12, 39, 92, 162, 218, 246, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[3, 11, 30, 67, 124, 185, 229, 249, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[3, 10, 25, 53, 97, 151, 200, 233, 250, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 8, 21, 43, 77, 123, 171, 209, 237, 251, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 2, 13, 35, 62, 97, 139, 186, 219, 244, 254, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 2, 8, 22, 48, 85, 128, 171, 208, 234, 248, 254, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[
                1, 2, 6, 16, 36, 67, 107, 149, 189, 220, 240, 250, 254, 255, 256,
            ],
        },
        &ICDFContext {
            total: 256,
            dist: &[
                1, 2, 5, 13, 29, 55, 90, 128, 166, 201, 227, 243, 251, 254, 255, 256,
            ],
        },
        &ICDFContext {
            total: 256,
            dist: &[
                1, 2, 4, 10, 22, 43, 73, 109, 147, 183, 213, 234, 246, 252, 254, 255, 256,
            ],
        },
    ],
    &[
        &ICDFContext {
            total: 256,
            dist: &[127, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[49, 206, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[20, 127, 236, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[11, 71, 184, 246, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[7, 43, 127, 214, 250, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[6, 30, 87, 169, 229, 252, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[5, 23, 62, 126, 194, 236, 252, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[6, 20, 49, 96, 157, 209, 239, 253, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 16, 39, 74, 125, 175, 215, 245, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 2, 23, 55, 97, 149, 195, 236, 254, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 7, 23, 50, 86, 128, 170, 206, 233, 249, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 6, 18, 39, 70, 108, 148, 186, 217, 238, 250, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 4, 13, 30, 56, 90, 128, 166, 200, 226, 243, 252, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[
                1, 4, 11, 25, 47, 76, 110, 146, 180, 209, 231, 245, 252, 255, 256,
            ],
        },
        &ICDFContext {
            total: 256,
            dist: &[
                1, 3, 8, 19, 37, 62, 93, 128, 163, 194, 219, 237, 248, 253, 255, 256,
            ],
        },
        &ICDFContext {
            total: 256,
            dist: &[
                1, 2, 6, 15, 30, 51, 79, 111, 145, 177, 205, 226, 241, 250, 254, 255, 256,
            ],
        },
    ],
    &[
        &ICDFContext {
            total: 256,
            dist: &[128, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[42, 214, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[21, 128, 235, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[12, 72, 184, 245, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[8, 42, 128, 214, 249, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[8, 31, 86, 176, 231, 251, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[5, 20, 58, 130, 202, 238, 253, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[6, 18, 45, 97, 174, 221, 241, 251, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[6, 25, 53, 88, 128, 168, 203, 231, 250, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[4, 18, 40, 71, 108, 148, 185, 216, 238, 252, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[3, 13, 31, 57, 90, 128, 166, 199, 225, 243, 253, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[2, 10, 23, 44, 73, 109, 147, 183, 212, 233, 246, 254, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[1, 6, 16, 33, 58, 90, 128, 166, 198, 223, 240, 250, 255, 256],
        },
        &ICDFContext {
            total: 256,
            dist: &[
                1, 5, 12, 25, 46, 75, 110, 146, 181, 210, 231, 244, 251, 255, 256,
            ],
        },
        &ICDFContext {
            total: 256,
            dist: &[
                1, 3, 8, 18, 35, 60, 92, 128, 164, 196, 221, 238, 248, 253, 255, 256,
            ],
        },
        &ICDFContext {
            total: 256,
            dist: &[
                1, 3, 7, 14, 27, 48, 76, 110, 146, 180, 208, 229, 242, 249, 253, 255, 256,
            ],
        },
    ],
];

const EXC_LSB: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[136, 256],
};

const EXC_SIGN: &[&[&[&ICDFContext]]] = &[
    &[
        // Inactive
        &[
            // Low offset
            &ICDFContext {
                total: 256,
                dist: &[2, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[207, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[189, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[179, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[174, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[163, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[157, 256],
            },
        ],
        &[
            // High offset
            &ICDFContext {
                total: 256,
                dist: &[58, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[245, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[238, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[232, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[225, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[220, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[211, 256],
            },
        ],
    ],
    &[
        // Unvoiced
        &[
            // Low offset
            &ICDFContext {
                total: 256,
                dist: &[1, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[210, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[190, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[178, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[169, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[162, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[152, 256],
            },
        ],
        &[
            // High offset
            &ICDFContext {
                total: 256,
                dist: &[48, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[242, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[235, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[224, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[214, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[205, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[190, 256],
            },
        ],
    ],
    &[
        // Voiced
        &[
            // Low offset
            &ICDFContext {
                total: 256,
                dist: &[1, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[162, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[152, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[147, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[144, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[141, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[138, 256],
            },
        ],
        &[
            // High offset
            &ICDFContext {
                total: 256,
                dist: &[8, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[203, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[187, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[176, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[168, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[161, 256],
            },
            &ICDFContext {
                total: 256,
                dist: &[154, 256],
            },
        ],
    ],
];

const QUANT_OFFSET: &[&[i32]] = &[
    &[25, 60], // Inactive or Unvoiced
    &[8, 25],  // Voiced
];

#[derive(Debug, Default)]
pub struct SilkFrame {
    frame_type: FrameType,
    log_gain: isize,
    coded: bool,
    prev_voiced: bool,
    nlsfs: [i16; 16],
    lpc: [f32; 16],
    interpolated_lpc: [f32; 16],
    interpolated: bool,
    interp_factor4: bool,
    previous_lag: i32,

    /* arrays are second class citizens
    output: [f32; LPC_HISTORY],
    lpc_history: [f32; LPC_HISTORY],
    */
    output: Vec<f32>,
    lpc_history: Vec<f32>,
}

impl SilkFrame {
    fn new() -> Self {
        let mut f = SilkFrame::default();

        f.output.resize(2 * LPC_HISTORY, 0f32);
        f.lpc_history.resize(2 * LPC_HISTORY, 0f32);

        f
    }

    fn parse_subframe_gains(&mut self, rd: &mut RangeDecoder, coded: bool) -> f32 {
        self.log_gain = if coded {
            let idx = self.frame_type.signal_type_index();
            let msb = rd.decode_icdf(MSB_SUBFRAME_GAIN[idx]) as isize;
            let lsb = rd.decode_icdf(LSB_SUBFRAME_GAIN) as isize;
            ((msb << 3) | lsb).max(self.log_gain - 16)
        } else {
            let delta = rd.decode_icdf(DELTA_SUBFRAME_GAIN) as isize;

            (delta * 2 - 16)
                .max(self.log_gain + delta - 4)
                .max(0)
                .min(63)
        };

        let log_gain = (self.log_gain * 0x1D1C71 >> 16) + 2090;

        log_gain.log2lin() as f32 / 65536.0f32
    }

    // TODO: once collect to slice is available rework to avoid allocations.
    fn parse_lpc<B: Band>(&mut self, rd: &mut RangeDecoder, interpolate: bool) {
        let idx = self.frame_type.voiced_index();
        let lsf_s1 = rd.decode_icdf(B::STAGE1[idx]);

        // TODO: directly reference the tables
        let (map, step, weight_map, weight_map_index, weights, codebooks) = (
            B::MAP[lsf_s1],
            B::STEP,
            B::PRED_WEIGHT,
            B::PRED_WEIGHT_INDEX[lsf_s1],
            B::WEIGHT[lsf_s1],
            B::CODEBOOK[lsf_s1],
        );

        let lsfs_s2 = map
            .iter()
            .map(|icdf| {
                let lsf = rd.decode_icdf(icdf) as i8 - 4;
                if lsf == -4 {
                    lsf - rd.decode_icdf(LSF_STAGE2_EXTENSION) as i8
                } else if lsf == 4 {
                    lsf + rd.decode_icdf(LSF_STAGE2_EXTENSION) as i8
                } else {
                    lsf
                }
            })
            .collect::<Vec<i8>>();

        // println!("lsfs2_s2 {:?}", lsfs_s2);

        let dequant_step = |lsf_s2: i16| -> i16 {
            let fix = if lsf_s2 < 0 {
                102
            } else if lsf_s2 > 0 {
                -102
            } else {
                0
            };

            (((lsf_s2 as i32 * 1024 + fix) * step) >> 16) as i16
        };

        let mut prev = None;

        // TODO: reverse codebooks and weights to avoid the collect?
        let residuals = lsfs_s2
            .iter()
            .enumerate()
            .rev()
            .map(|(i, lsf_s2)| {
                let ds = dequant_step(*lsf_s2 as i16);

                let res = ds + if let Some(p) = prev {
                    let weight = weight_map[weight_map_index[i]][i] as i32;
                    ((p as i32 * weight) >> 8) as i16
                } else {
                    0
                };

                prev = Some(res);

                res
            })
            .collect::<Vec<i16>>();

        // println!("residuals {:#?}", residuals);

        let mut nlsfs = residuals
            .iter()
            .rev()
            .zip(codebooks)
            .zip(weights)
            .map(|((&r, &c), &w)| {
                let nlsf = ((c as i32) << 7) + ((r as i32) << 14) / (w as i32);

                nlsf.max(0).min(1 << 15) as i16
            })
            .collect::<Vec<i16>>();

        // println!("nlsf {:#?}", nlsfs);

        // Damage control
        B::stabilize(&mut nlsfs);

        // println!("nlsf {:#?}", nlsfs);

        self.interpolated = false;
        self.interp_factor4 = if interpolate {
            let weight = rd.decode_icdf(LSF_INTERPOLATION_INDEX) as i16;
            // println!("w {} coded {}", weight, self.coded);
            if weight != 4 && self.coded {
                self.interpolated = true;
                if weight != 0 {
                    let interpolated_nlsfs = nlsfs
                        .iter()
                        .zip(&self.nlsfs)
                        .map(|(&nlsf, &prev)| prev + ((nlsf - prev) * weight >> 2));
                    B::lsf_to_lpc(&mut self.interpolated_lpc, interpolated_nlsfs);
                } else {
                    (&mut self.interpolated_lpc[..B::ORDER]).copy_from_slice(&self.lpc[..B::ORDER]);
                }
                false
            } else {
                true
            }
        } else {
            true
        };

        (&mut self.nlsfs[..B::ORDER]).copy_from_slice(&nlsfs);

        B::lsf_to_lpc(&mut self.lpc, nlsfs);

        //        println!("lpc {:#.6?}", &self.lpc[..B::ORDER]);
        //        println!("interpolated_lpc {:#.6?}", &self.interpolated_lpc[..B::ORDER]);
    }

    fn parse_pitch_lags<P: PitchLag>(
        &mut self,
        rd: &mut RangeDecoder,
        subframes: &mut [SubFrame],
        absolute: bool,
    ) {
        // println!("pitch_lags abs {}", absolute);
        let parse_absolute_lag = |rd: &mut RangeDecoder| {
            let high = rd.decode_icdf(PITCH_HIGH_PART) as i32;
            let low = rd.decode_icdf(P::LOW_PART) as i32;

            high * P::SCALE as i32 + low + P::MIN_LAG as i32
        };

        let lag = if !absolute {
            let delta = rd.decode_icdf(PITCH_DELTA) as i32;
            if delta != 0 {
                self.previous_lag + delta - 9
            } else {
                parse_absolute_lag(rd)
            }
        } else {
            parse_absolute_lag(rd)
        };

        // println!("lag {}", lag);

        self.previous_lag = lag;

        let offsets = if subframes.len() == 2 {
            let idx = rd.decode_icdf(P::CONTOUR[0]);
            P::OFFSET[0][idx]
        } else {
            let idx = rd.decode_icdf(P::CONTOUR[1]);
            P::OFFSET[1][idx]
        };

        for (sf, &off) in subframes.iter_mut().zip(offsets.iter()) {
            sf.pitch_lag = (lag + off as i32)
                .min(P::MAX_LAG as i32)
                .max(P::MIN_LAG as i32);
        }
    }

    fn parse_ltp_filter_coeff(&mut self, rd: &mut RangeDecoder, subframes: &mut [SubFrame]) {
        let idx_period = rd.decode_icdf(LTP_PERIODICITY);

        for sf in subframes.iter_mut() {
            let idx_filter = rd.decode_icdf(LTP_FILTER[idx_period]);
            let filter_taps = LTP_TAPS[idx_period][idx_filter];
            for (tap_f32, &tap_i8) in sf.ltp_taps.iter_mut().zip(filter_taps.iter()) {
                *tap_f32 = tap_i8 as f32 / 128f32;
            }
            //            println!("ltp_taps {:.6?}", &sf.ltp_taps);
        }
    }

    fn parse_excitation<S: ShellBlock>(
        &mut self,
        rd: &mut RangeDecoder,
        residuals: &mut [f32],
        long_frame: bool,
    ) {
        let shell_blocks = S::SHELL_BLOCKS[long_frame as usize] as usize;
        let pulsecount: &mut [u8] = &mut [0u8; 20][..shell_blocks];
        let lsbcount: &mut [u8] = &mut [0u8; 20][..shell_blocks];
        let excitation: &mut [i32] = &mut [0i32; 320][..shell_blocks * 16];
        let mut seed = rd.decode_icdf(LCG_SEED) as u32;
        let voiced_index = self.frame_type.voiced_index();
        let ratelevel = rd.decode_icdf(EXC_RATE[voiced_index]);
        // println!("ratelevel {} voiced_index {}", ratelevel, voiced_index);
        // println!("seed {} shell {}", seed, shell_blocks);
        for (pc, lsb) in pulsecount.iter_mut().zip(lsbcount.iter_mut()) {
            let mut p = rd.decode_icdf(PULSE_COUNT[ratelevel]);
            //            println!("p {}", p);
            if p == 17 {
                let mut l = 0;
                while p == 17 && {
                    l += 1;
                    l
                } != 10
                {
                    p = rd.decode_icdf(PULSE_COUNT[9]);
                }
                if l == 10 {
                    p = rd.decode_icdf(PULSE_COUNT[10]);
                }
                *lsb = l as u8;
            }
            //            println!("fp {}", p);
            *pc = p as u8;
        }
        //        println!("lsb {:#?}", lsbcount);
        for (&p, loc) in pulsecount.iter().zip(excitation.chunks_mut(16)) {
            if p == 0 {
                for ex in loc.iter_mut() {
                    *ex = 0;
                }
            } else {
                fn split_loc(rd: &mut RangeDecoder, level: usize, avail: i32) -> [i32; 2] {
                    if avail == 0 {
                        [0, 0]
                    } else {
                        let idx = (((avail - 1 + 5) * (avail - 1)) >> 1) as usize;
                        //                        println!("level {} total {} index {}",level, avail, idx);
                        let left =
                            rd.decode_icdf(PULSE_LOCATION[level][(avail - 1) as usize]) as i32;
                        let right = avail - left;

                        //                        println!("{} {}", left, right);

                        [left as i32, right as i32]
                    }
                }

                let dist = split_loc(rd, 0, p as i32);
                for (lv1, &avail) in loc.chunks_mut(8).zip(dist.iter()) {
                    let dist = split_loc(rd, 1, avail);
                    for (lv2, &avail) in lv1.chunks_mut(4).zip(dist.iter()) {
                        let dist = split_loc(rd, 2, avail);
                        for (lv3, &avail) in lv2.chunks_mut(2).zip(dist.iter()) {
                            let dist = split_loc(rd, 3, avail);

                            lv3.copy_from_slice(&dist);
                        }
                    }
                }
            }
        }

        //        println!("excitation {:#?}", excitation);

        for (&bits, loc) in lsbcount.iter().zip(excitation.chunks_mut(16)) {
            for l in loc.iter_mut() {
                for _ in 0..bits {
                    *l = (*l << 1) | (rd.decode_icdf(EXC_LSB) as i32);
                }
            }
        }

        //        println!("lsb excitation {:#?}", excitation);

        for (&p, loc) in pulsecount.iter().zip(excitation.chunks_mut(16)) {
            for l in loc.iter_mut() {
                if *l != 0 {
                    let signal_type = self.frame_type.signal_type_index();
                    let qoffset_type = self.frame_type.qoffset_type_index();
                    let pulse = p.min(6) as usize;

                    let sign = rd.decode_icdf(EXC_SIGN[signal_type][qoffset_type][pulse]);

                    if sign == 0 {
                        *l *= -1;
                    }
                }
            }
        }

        for (&l, r) in excitation.iter().zip(residuals.iter_mut()) {
            let voiced = self.frame_type.voiced_index();
            let qoffset = self.frame_type.qoffset_type_index();
            let ex1 = l * 256 | QUANT_OFFSET[voiced][qoffset];
            let mut ex = ex1 - 20 * l.signum();
            //            println!("res {} val {} {}", ex1, l, ex);

            seed = seed.wrapping_mul(196314165).wrapping_add(907633515);
            // println!("seed {}",  seed);
            if (seed & 0x80000000) != 0 {
                ex *= -1;
            }
            seed = seed.wrapping_add(l as u32);

            *r = (ex as f32) / 8388608.0f32;
            //            println!("res {:.6}", r);
        }
    }

    fn flush(&mut self) {
        if self.coded {
            //            println!("flushing");

            self.log_gain = 0;
            self.coded = false;
            self.prev_voiced = false;
            self.nlsfs = [0; 16];
            self.lpc = [0f32; 16];
            self.interpolated_lpc = [0f32; 16];
            self.interpolated = false;
            self.interp_factor4 = false;
            self.previous_lag = 0;

            self.output.clear();
            self.lpc_history.clear();

            self.output.resize(2 * LPC_HISTORY, 0f32);
            self.lpc_history.resize(2 * LPC_HISTORY, 0f32);
        }
    }

    fn parse(
        &mut self,
        rd: &mut RangeDecoder,
        info: &SilkInfo,
        vad: bool,
        first: bool,
    ) -> Result<()> {
        self.frame_type = if vad {
            match rd.decode_icdf(FRAME_TYPE_ACTIVE) {
                0 => FrameType {
                    active: true,
                    voiced: false,
                    high: false,
                }, // UnvoicedLow,
                1 => FrameType {
                    active: true,
                    voiced: false,
                    high: true,
                }, // UnvoicedHigh,
                2 => FrameType {
                    active: true,
                    voiced: true,
                    high: false,
                }, // VoicedLow,
                3 => FrameType {
                    active: true,
                    voiced: true,
                    high: true,
                }, // VoicedHigh,
                _ => unreachable!(),
            }
        } else {
            if rd.decode_icdf(FRAME_TYPE_INACTIVE) == 0 {
                FrameType {
                    active: false,
                    voiced: false,
                    high: false,
                } // InactiveLow
            } else {
                FrameType {
                    active: false,
                    voiced: false,
                    high: true,
                } // InactiveHigh
            }
        };

        //        println!("Type {:?}", self.frame_type);

        let mut sfs: [SubFrame; 4] = Default::default();
        let mut residuals = [0f32; LPC_HISTORY + RES_HISTORY];

        for (i, mut sf) in &mut sfs[..info.subframes].iter_mut().enumerate() {
            let coded = i == 0 && (first || !self.coded);
            sf.gain = self.parse_subframe_gains(rd, coded);
            //            println!("subframe {} coded {} gain {:.6}", i, coded, sf.gain);
        }

        // TODO: monomorphize over long/short frames?
        let long_frame = info.subframes == 4;

        // TODO: move the WB/NB_MB up
        let order = if info.bandwidth > Bandwidth::Medium {
            self.parse_lpc::<WB>(rd, long_frame);
            WB::ORDER
        } else {
            self.parse_lpc::<NB_MB>(rd, long_frame);
            NB_MB::ORDER
        };

        if self.frame_type.voiced {
            let absolute = first || !self.prev_voiced;
            match info.bandwidth {
                Bandwidth::Narrow => {
                    self.parse_pitch_lags::<NB>(rd, &mut sfs[..info.subframes], absolute);
                }
                Bandwidth::Medium => {
                    self.parse_pitch_lags::<MB>(rd, &mut sfs[..info.subframes], absolute);
                }
                _ => {
                    self.parse_pitch_lags::<WB>(rd, &mut sfs[..info.subframes], absolute);
                }
            }

            self.parse_ltp_filter_coeff(rd, &mut sfs[..info.subframes]);
        }

        let ltpscale = if self.frame_type.voiced && first {
            LTP_SCALE[rd.decode_icdf(LTP_SCALE_INDEX)] as f32
        } else {
            15565 as f32
        } / 16384f32;

        //        println!("ltpscale {:.6}", ltpscale);

        match info.bandwidth {
            Bandwidth::Narrow => {
                self.parse_excitation::<NB>(rd, &mut residuals[RES_HISTORY..], long_frame);
            }
            Bandwidth::Medium => {
                self.parse_excitation::<MB>(rd, &mut residuals[RES_HISTORY..], long_frame);
            }
            _ => {
                self.parse_excitation::<WB>(rd, &mut residuals[RES_HISTORY..], long_frame);
            }
        }

        // println!("residuals {:?}", &residuals);

        // if self.mono_only { return Ok(()) }
        for i in 0..info.subframes {
            let sf = &sfs[i];
            // TODO: assemble an iterator outside
            let lpc_coeff = if i < 2 && self.interpolated {
                &self.interpolated_lpc[..order]
            } else {
                &self.lpc[..order]
            };

            //            println!("lpc coef {} {}", i, self.interpolated);

            if self.frame_type.voiced {
                let before = (sf.pitch_lag as usize) + LTP_ORDER / 2;
                let (end, scale) = if i < 2 || self.interp_factor4 {
                    (i * info.sf_size, ltpscale)
                } else {
                    ((i - 2) * info.sf_size, 1f32)
                };

                if before > end {
                    // re-white residuals
                    let start = LPC_HISTORY + i * info.sf_size - before;
                    let stop = LPC_HISTORY + i * info.sf_size - end;

                    let start_res = RES_HISTORY + i * info.sf_size - before;
                    let stop_res = RES_HISTORY + i * info.sf_size - end;

                    let previous_w = self.output[start - order..stop].windows(order);
                    let iter = self.output[start..stop]
                        .iter()
                        .zip(residuals[start_res..stop_res].iter_mut());

                    /*                    println!("previous_w {} {} {} {} {} {} {}",
                             start,
                             stop,
                             - (sf.pitch_lag as isize) - LTP_ORDER as isize / 2,
                             info.sf_size,
                             LPC_HISTORY,
                             i,
                             order);
*/
                    for ((&o, r), p_w) in iter.zip(previous_w) {
                        let mut sum = o;

                        // println!("{:.6?}", p_w);
                        for (&c, &p) in lpc_coeff.iter().zip(p_w.iter().rev()) {
                            //                            println!("rewhite {:.6} {:.6} {:.6}", sum, c, p);
                            sum -= c * p;
                        }

                        *r = sum.max(-1f32).min(1f32) * scale / sf.gain;
                        //                        println!("res {:.6} <- {:.6} {:.6}", *r, scale, sf.gain);
                    }
                }

                if end != 0 {
                    // first and third subframe
                    let start = RES_HISTORY + i * info.sf_size - end;
                    let stop = RES_HISTORY + i * info.sf_size;
                    let rescale = sfs[i - 1].gain / sfs[i].gain;

                    //                    println!("rescaling {} {} {}", start, stop, rescale);

                    for r in residuals[start..stop].iter_mut() {
                        *r *= rescale;
                    }
                }
                {
                    let start = RES_HISTORY + i * info.sf_size;
                    let stop = start + info.sf_size;

                    //                    println!("before {:#.6?}", &residuals[..]);

                    for i in start..stop {
                        let mut sum = residuals[i];

                        for o in 0..LTP_ORDER {
                            let idx = i - (sf.pitch_lag as usize) + LTP_ORDER / 2 - o;
                            //                            println!("ord {} idx {} -> {:.6} * {:.8}", o, idx, sf.ltp_taps[o], residuals[idx]);
                            sum += sf.ltp_taps[o] * residuals[idx];
                        }

                        residuals[i] = sum;
                        //                        println!("residuals {:.6}", sum);
                    }
                }
            }

            // TODO: Use chunks_mut
            let start_lpc = LPC_HISTORY + i * info.sf_size;
            let stop_lpc = LPC_HISTORY + (i + 1) * info.sf_size;
            let range_res = RES_HISTORY + i * info.sf_size..RES_HISTORY + (i + 1) * info.sf_size;

            // println!("range {:?} {}", range_res, i);

            let res = &residuals[range_res];

            let output = &mut self.output[start_lpc..stop_lpc];
            let lpc = &mut self.lpc_history[start_lpc - order..stop_lpc];

            for j in 0..info.sf_size {
                let mut sum = res[j] * sf.gain;
                for k in 0..order {
                    //                    println!("sum {:.6} coeff {:.6} lpc {:.6}", sum, lpc_coeff[k], lpc[j + order - k - 1]);
                    sum += lpc_coeff[k] * lpc[j + order - k - 1];
                }
                lpc[j + order] = sum;
                output[j] = sum.max(-1f32).min(1f32);
                //                println!("lpc {:.6} dst {:.6}", lpc[j + order], output[j]);
            }
        }

        self.prev_voiced = self.frame_type.voiced;

        //        println!("flength {}", info.f_size);

        for i in 0..LPC_HISTORY {
            self.lpc_history[i] = self.lpc_history[i + info.f_size];
            self.output[i] = self.output[i + info.f_size];
            println!(
                "history {:.6} output {:.6}",
                self.lpc_history[i], self.output[i]
            );
        }

        self.coded = true;

        Ok(())
    }
}

impl Silk {
    pub fn new(stereo_out: bool) -> Self {
        Silk {
            stereo: true,
            stereo_out: stereo_out,
            frames: 0,
            frame_len: 0,
            subframe_len: 0,

            info: SilkInfo {
                subframes: 0,
                sf_size: 0,
                f_size: 0,
                bandwidth: Bandwidth::Full,

                weight0: 0f32,
                weight1: 0f32,
                prev0: 0f32,
                prev1: 0f32,
            },

            mid_frame: SilkFrame::new(),
            side_frame: SilkFrame::new(),
            left_outbuf: vec![0f32; 960],
            right_outbuf: vec![0f32; 960],
        }
    }

    pub fn setup(&mut self, pkt: &Packet) {
        match pkt.frame_duration {
            FrameDuration::Medium => {
                self.frames = 1;
                self.info.subframes = 2;
            }
            FrameDuration::Standard => {
                self.frames = 1;
                self.info.subframes = 4;
            }
            FrameDuration::Long => {
                self.frames = 2;
                self.info.subframes = 4;
            }
            FrameDuration::VeryLong => {
                self.frames = 3;
                self.info.subframes = 4;
            }
            _ => unreachable!(),
        }
        self.stereo = pkt.stereo;
        self.info.bandwidth = pkt.bandwidth.min(Bandwidth::Wide);
        self.info.sf_size = match self.info.bandwidth {
            Bandwidth::Narrow => 40,
            Bandwidth::Medium => 60,
            Bandwidth::Wide => 80,
            _ => unreachable!(),
        };
        self.info.f_size = self.info.sf_size * self.info.subframes;

        // TODO: avoid the memset
        self.left_outbuf
            .resize(self.info.f_size * self.frames, 0f32);
        self.right_outbuf
            .resize(self.info.f_size * self.frames, 0f32);
    }

    pub fn parse_stereo_weight(&mut self, rd: &mut RangeDecoder, vad: bool) -> bool {
        let w_q13 = [
            -13732, -10050, -8266, -7526, -6500, -5000, -2950, -820, 820, 2950, 5000, 6500, 7526,
            8266, 10050, 13732,
        ];
        let n = rd.decode_icdf(STAGE1);
        let i0 = rd.decode_icdf(STAGE2) + 3 * (n / 5);
        let i1 = rd.decode_icdf(STAGE3) * 2 + 1;
        let i2 = rd.decode_icdf(STAGE2) + 3 * (n % 5);
        let i3 = rd.decode_icdf(STAGE3) * 2 + 1;

        let weight = |idx, scale| {
            let w = w_q13[idx];
            let w1 = w_q13[idx + 1];

            w + (((w1 - w) * 6554) >> 16) as isize * scale as isize
        };

        let w0 = weight(i0, i1);
        let w1 = weight(i2, i3);

        self.info.weight0 = (w0 - w1) as f32 / 8192f32;
        self.info.weight1 = w1 as f32 / 8192f32;

        // println!("{:?}", self);

        if vad {
            false
        } else {
            rd.decode_icdf(MID_ONLY) != 0
        }
    }

    fn unmix_ms(&mut self, range: Range<usize>) {
        let in_start = LPC_HISTORY - self.info.f_size;
        let in_range = in_start + self.info.f_size;
        let w0 = self.info.weight0;
        let w1 = self.info.weight1;
        let w0p = self.info.prev0;
        let w1p = self.info.prev1;
        let n1 = match self.info.bandwidth {
            Bandwidth::Narrow => 64,
            Bandwidth::Medium => 96,
            _ => 128,
        };
        let w0d = (w0 - w0p) / (n1 as f32);
        let w1d = (w1 - w1p) / (n1 as f32);

        let left = self.left_outbuf[range.clone()].iter_mut();
        let right = self.right_outbuf[range].iter_mut();
        let mid = &self.mid_frame.output[in_start - 2..in_range];
        let side = &self.side_frame.output[in_start - 1..in_range - 1];

        let out = left.zip(right);
        let inb = mid.windows(3).zip(side);

        let mut iter = out.zip(inb);

        for (i, ((l, r), (m, s))) in iter.by_ref().enumerate().take(n1) {
            let interp0 = w0p + i as f32 * w0d;
            let interp1 = w1p + i as f32 * w1d;
            let p0 = 0.25 * (m[0] + 2.0 * m[1] + m[2]);
            let si0 = s + interp0 * p0;

            *r = ((1.0 + interp1) * m[1] + si0).min(1.0).max(-1.0);
            *l = ((1.0 - interp1) * m[1] - si0).min(1.0).max(-1.0);
            // println!("{:#.6} {:#.6}", r, l);
        }

        println!("rem");

        for ((l, r), (m, s)) in iter {
            let p0 = 0.25 * (m[0] + 2.0 * m[1] + m[2]);
            let si0 = s + w0 * p0;

            *r = ((1.0 + w1) * m[1] + si0).min(1.0).max(-1.0);
            *l = ((1.0 - w1) * m[1] - si0).min(1.0).max(-1.0);
            // println!("{:#.6} {:#.6}", r, l);
        }

        self.info.prev0 = self.info.weight0;
        self.info.prev1 = self.info.weight1;
    }

    pub fn decode(&mut self, rd: &mut RangeDecoder) -> Result<usize> {
        let mut mid_vad = [false; 3];
        let mut side_vad = [false; 3];
        fn lp(rd: &mut RangeDecoder, vad: &mut [bool]) -> Result<()> {
            for v in vad {
                *v = rd.decode_logp(1);
            }
            if rd.decode_logp(1) {
                return Err(Error::Unsupported("LBRR frames".to_owned()));
            } else {
                Ok(())
            }
        }

        lp(rd, &mut mid_vad[..self.frames])?;

        if self.stereo {
            lp(rd, &mut side_vad[..self.frames])?;
        }
        //        println!("{:?} {:?}", mid_vad, side_vad);
        for i in 0..self.frames {
            let first = i == 0;
            let midonly = if self.stereo {
                self.parse_stereo_weight(rd, side_vad[i])
            } else {
                false
            };
            //            println!("{} midonly {} stereo {}", i, midonly, self.stereo);
            self.mid_frame.parse(rd, &self.info, mid_vad[i], first)?;

            if self.stereo && !midonly {
                self.side_frame.parse(rd, &self.info, side_vad[i], first)?;
            }

            if midonly {
                self.side_frame.flush();
            }
            let out_range = i * self.info.f_size..(i + 1) * self.info.f_size;
            if self.stereo && self.stereo_out {
                println!("unmix");
                self.unmix_ms(out_range);
            } else {
                let in_start = LPC_HISTORY - self.info.f_size - 2;
                let in_range = in_start..in_start + self.info.f_size;
                let inbuf = &self.mid_frame.output[in_range];

                if self.stereo_out {
                    self.left_outbuf[out_range.clone()].copy_from_slice(inbuf);
                }
                self.right_outbuf[out_range].copy_from_slice(inbuf);
            }
        }

        println!("stereo {} out {}", self.stereo, self.stereo_out);
        println!(
            "right: {:#?}",
            &self.right_outbuf[..self.frames * self.info.f_size]
        );
        println!(
            "left: {:#?}",
            &self.left_outbuf[..self.frames * self.info.f_size]
        );

        Ok(0)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn decode(in_slice: &[u8], stereo_out: bool,
              right_outbuf: &[f32], left_outbuf: &[f32]) {
        let p = Packet::from_slice(in_slice).unwrap();

        let mut silk = Silk::new(stereo_out);

        silk.setup(&p);

        for frame in p.frames {
            let mut rd = RangeDecoder::new(frame);

            let _ = silk.decode(&mut rd);
        }

        assert_eq!(&silk.right_outbuf[..], &right_outbuf[..]);
        assert_eq!(&silk.left_outbuf[..], &left_outbuf[..]);
    }

    #[test]
    // First Packet of testvector02
    fn decode_midonly_to_stereo() {
        let in_slice = &[
            24, 0, 117, 35, 193, 30, 132, 212, 10, 126, 208, 7, 81, 52, 218, 159, 252, 5, 41, 239,
            159, 65, 1, 87, 181, 124, 31, 132, 62, 64,
        ];

        let outbuf = vec![
            0.0,
            0.0,
            0.000018067658,
            0.000040303756,
            0.00006217384,
            0.000050676164,
            0.00006882091,
            0.00006251221,
            0.000079100166,
            0.000071366696,
            0.00008358848,
            0.00010954219,
            0.00009314428,
            0.00007471895,
            0.00009187053,
            0.00011543312,
            0.00009968806,
            0.00007717536,
            0.00005688308,
            0.00006992291,
            0.00009155123,
            0.00007225159,
            0.000082283856,
            0.00010369863,
            0.000089072724,
            0.00007039197,
            0.000051477527,
            0.000065270964,
            0.000048930804,
            0.00005775347,
            0.000078105826,
            0.000093486946,
            0.00011695236,
            0.000104911516,
            0.000087843735,
            0.000072130475,
            0.0000893232,
            0.00007565466,
            0.00004729777,
            0.000023753006,
            -0.000003481034,
            0.000007222726,
            -0.00003455881,
            -0.00009271239,
            -0.000066918095,
            -0.000027454684,
            0.000010518399,
            -0.0000254706,
            0.0000049614614,
            -0.00039022753,
            -0.0005224956,
            -0.00047128735,
            -0.00056358863,
            -0.0006580248,
            -0.00068203855,
            -0.00066704734,
            -0.0006456452,
            -0.0005996575,
            -0.0006334914,
            -0.0006084688,
            -0.00062359835,
            -0.00058508565,
            -0.0005228264,
            -0.0004765002,
            -0.0004977702,
            -0.0005371424,
            -0.0005641401,
            -0.0006018564,
            -0.00056627224,
            -0.00060242246,
            -0.0006653947,
            -0.0006363526,
            -0.00067228265,
            -0.00072607934,
            -0.00077472016,
            -0.0007508323,
            -0.00070951204,
            -0.0006736619,
            -0.00070547353,
            -0.00067490665,
            -0.0006116076,
            -0.0005548779,
            -0.0005106908,
            -0.00051649136,
            -0.0004828797,
            -0.00043634523,
            -0.0003942762,
            -0.00039607857,
            -0.0003701741,
            -0.000371713,
            -0.0003464717,
            -0.00034844707,
            -0.0003288207,
            -0.0003371921,
            -0.0003200362,
            -0.0002849602,
            -0.00030087636,
            -0.00027824516,
            -0.00024563857,
            -0.00025836847,
            -0.0002770026,
            -0.00029516904,
            -0.00031652366,
            -0.00029966095,
            -0.0002755761,
            -0.00029857265,
            -0.00028539647,
            -0.00029825774,
            -0.0003229264,
            -0.00034267516,
            -0.00036900686,
            -0.00035476664,
            -0.00033501693,
            -0.0003578332,
            -0.0003442865,
            -0.00035875358,
            -0.00034144567,
            -0.00035179761,
            -0.00037886848,
            -0.00035795642,
            -0.00033369192,
            -0.00035021192,
            -0.00036565852,
            -0.00037517678,
            -0.00036457967,
            -0.00034890795,
            -0.00033583154,
            -0.0003450666,
            -0.0003578865,
            -0.00034270264,
            -0.00034698966,
            -0.00035916705,
            -0.0003708265,
            -0.00036255966,
            -0.0003707815,
            -0.00038588603,
            -0.00039805335,
            -0.00041352137,
            -0.00042772965,
            -0.0004430421,
            -0.00043711794,
            -0.0004272484,
            -0.00041760248,
            -0.0004265278,
            -0.00043894202,
            -0.0004476532,
            -0.00043577352,
            -0.00044231408,
            -0.00043519543,
            -0.0004426892,
            -0.00045720986,
            -0.0004666732,
            -0.00045662167,
            -0.00044183253,
            -0.00043033285,
            -0.00041554883,
            -0.0003991444,
            -0.0004021518,
            -0.0003859062,
            -0.000363729,
            -0.00036794055,
            -0.00035335377,
            -0.00030704716,
            -0.00032831624,
            -0.00031874186,
            -0.00030945305,
            -0.00035139953,
            -0.00027755776,
            -0.00031040065,
            -0.00031451246,
            -0.00031291708,
            -0.00031822757,
            -0.00036744564,
            -0.00030704128,
            -0.00034542562,
            -0.00036296426,
            -0.00036483022,
            -0.00038423264,
            -0.0003941805,
            -0.0003664376,
            -0.00036076238,
            -0.00035452793,
            -0.0003323944,
            -0.00036015123,
            -0.00035394664,
            -0.00035182483,
            -0.00035670222,
            -0.0003617367,
            -0.0003228092,
            -0.000355942,
            -0.0003593549,
            -0.00035908085,
            -0.0003688061,
            -0.00037436158,
            -0.0003863271,
            -0.0003966799,
            -0.00041026337,
            -0.00042153866,
            -0.00043842307,
            -0.00040728808,
            -0.0003582299,
            -0.00037332508,
            -0.00039398205,
            -0.000338237,
            -0.00031311178,
            -0.00028689075,
            -0.00025412143,
            -0.00021913888,
            -0.0002228038,
            -0.00015388099,
            -0.00016105262,
            -0.00009726519,
            -0.00010000902,
            -0.00008408187,
            -0.00006209169,
            -0.000052660682,
            0.0000067388846,
            0.000029670395,
            0.00006542809,
            0.000053659922,
            0.00007506335,
            0.000088867186,
            0.00009480247,
            0.00014607885,
            0.000117823256,
            0.00012417675,
            0.00017139876,
            0.00014227604,
            0.00014138046,
            0.00018449393,
            0.00019616325,
            0.00021080494,
            0.00023250596,
            0.00025279768,
            0.00027799938,
            0.00030531277,
            0.0002840034,
            0.00033773758,
            0.00040656223,
            0.0003562422,
            0.00036952534,
            0.0004232817,
            0.0003956121,
            0.00044202147,
            0.00045867288,
            0.00042758355,
            0.0004768431,
            0.0004921706,
            0.00045787738,
            0.0005025254,
            0.0005157768,
            0.0005241799,
            0.00054180715,
            0.0005563786,
            0.00052613893,
            0.00052395667,
            0.0005599942,
            0.0005590905,
            0.00056636054,
            0.0005729376,
            0.00058212475,
            0.00059331453,
            0.000557559,
            0.00054652675,
            0.0005760184,
            0.0006173439,
            0.00054389605,
            0.0005313912,
            0.00051632815,
            0.0005339546,
            0.00047883834,
            0.00049019617,
            0.00043511493,
            0.00044641914,
            0.0004367338,
            0.00041501003,
            0.000364374,
            0.000331152,
            0.00034666515,
            0.00027492447,
            0.00023972207,
            0.00024224953,
            0.00021756259,
            0.00019639167,
            0.00017851594,
            0.00016501207,
            0.00015369013,
            0.00014687527,
            0.00009163152,
            0.00011311626,
            0.000106089705,
            0.00009617381,
            0.00009553661,
            0.000048387086,
            0.00007897997,
            0.00007687673,
            0.00003113523,
            0.00006030392,
            0.000016340247,
            -0.0000020786729,
            -0.000022103666,
            -0.000048141155,
            -0.00012122062,
            -0.00008128,
            -0.00014473175,
            -0.00013882222,
            -0.00019146306,
            -0.00018190368,
            -0.00018643036,
            -0.0002891976,
            -0.0002361408,
            -0.00029843312,
            -0.00027830145,
            -0.0002879903,
            -0.0002934076,
            -0.00033351593,
            -0.0003498932,
            -0.0003612237,
            -0.00034400827,
            -0.00038435328,
            -0.0003618736,
            -0.0003991937,
            -0.00038086757,
            -0.00040051353,
            -0.00043208967,
            -0.00044373644,
            -0.000417068,
            -0.00038973696,
            -0.00041766578,
            -0.0003853322,
            -0.00038022987,
            -0.00039688247,
            -0.00035251363,
            -0.00037676597,
            -0.0003814074,
            -0.0003698009,
            -0.00033389402,
            -0.00039195863,
            -0.00037793597,
            -0.00039117408,
            -0.00036094044,
            -0.00035158306,
            -0.00032902358,
            -0.0003512097,
            -0.00036278958,
            -0.00031850336,
            -0.0002997689,
            -0.00034405876,
            -0.00029620476,
            -0.00031462678,
            -0.0002742068,
            -0.00031360218,
            -0.0002839408,
            -0.00024859587,
            -0.00025631723,
            -0.0002085692,
            -0.00023808342,
            -0.00019352697,
            -0.000202344,
            -0.000202121,
            -0.0002053329,
            -0.00015825967,
            -0.00016356443,
            -0.00016973377,
            -0.00016889903,
            -0.00014837584,
            -0.00014787487,
            -0.00010911938,
            -0.00008238437,
            -0.00010556641,
            -0.000057263976,
            -0.000036379584,
            -0.00005322733,
            0.0000005010479,
            0.000029535115,
            0.000059450067,
            0.000065109016,
            0.000052581832,
            0.00013207388,
            0.00012951999,
            0.00017732712,
            0.00016627478,
            0.00020370472,
            0.00020061409,
            0.00019981223,
            0.0002452158,
            0.00027158554,
            0.00027266165,
            0.00025877624,
            0.00024592614,
            0.00027438614,
            0.00031528424,
            0.00033861955,
            0.0003117534,
            0.0003384213,
            0.00035331098,
            0.00037478213,
            0.000358395,
            0.00033471006,
            0.00036768438,
            0.00034856432,
            0.00033497758,
            0.00035534197,
            0.00036381785,
            0.00037925263,
            0.00039519335,
            0.0003906512,
            0.0003972133,
            0.0004193461,
            0.00038130264,
            0.00036231562,
            0.00043677297,
            0.00039331318,
            0.00037891854,
            0.00040104223,
            0.00038769856,
            0.00036665238,
            0.00039447664,
            0.0003355762,
            0.000308798,
            0.00034601463,
            0.00028193608,
            0.00025882598,
            0.00023086509,
            0.00017186649,
            0.00018958538,
            0.00023993287,
            0.00020597127,
            0.00020340554,
            0.0002397749,
            0.00015634668,
            0.00018520016,
            0.00014973764,
            0.00010392851,
            0.00010673383,
            0.000077002434,
            0.00007891109,
            0.00004642789,
            0.0000057877733,
            -0.0000031385862,
            -0.000038078528,
            -0.000033914253,
            -0.000033344833,
            -0.00003447271,
            -0.000037464928,
            -0.000036620615,
            -0.00008619548,
            -0.00002655501,
            -0.000018066954,
            -0.00005559528,
            -0.00006443009,
            -0.000053738033,
            -0.000081289276,
            -0.000038029775,
            -0.000021225887,
            -0.000066999884,
            -0.00006602613,
            -0.000050787115,
            -0.000040469444,
            -0.000048672224,
            -0.000020870457,
            -0.000060901293,
            -0.00006710121,
            -0.00007888975,
            -0.00011023259,
            -0.00015817984,
            -0.00011895697,
            -0.00015202575,
            -0.00017670613,
            -0.00019538797,
            -0.00020414621,
            -0.00018765386,
            -0.00021895839,
            -0.0002552592,
            -0.00028966588,
            -0.00030726718,
            -0.00028667695,
            -0.00027067948,
            -0.00030915916,
            -0.00029481357,
        ];

        decode(in_slice, true, &outbuf, &outbuf);
    }

    #[test]
    // First Packet of testvector08
    fn decode_unmix() {
        let in_slice = &[12, 9, 178, 70, 140, 148, 202, 129, 225, 86, 64, 234, 160];
        let right = vec![
    0.0,
    0.00002701117,
    0.000048303198,
    0.00006884026,
    0.0000385869,
    0.00007594532,
    0.00004979196,
    0.000080403195,
    0.00005132884,
    0.00007735115,
    0.000104849874,
    0.00006291426,
    0.000046256202,
    0.00007947949,
    0.000102889244,
    0.00005986393,
    0.000037757258,
    0.000017585764,
    0.0000493664,
    0.00007017855,
    0.00002405231,
    0.000056863744,
    0.00008294645,
    0.00004725504,
    0.000025951815,
    0.0000054149305,
    0.00003965142,
    -0.0000016703914,
    0.000028146218,
    0.000050180864,
    0.000065732085,
    0.000095747004,
    0.00005945634,
    0.000043198073,
    0.000023384904,
    0.00006248026,
    0.000019645982,
    -0.0000141328255,
    -0.000034440323,
    -0.00006514615,
    -0.000027445385,
    -0.00008529465,
    -0.00012651064,
    -0.000074555246,
    -0.000044602937,
    -0.000016317026,
    -0.000070441514,
    -0.0000066870252,
    -0.0002150266,
    -0.0003797998,
    -0.00027074805,
    -0.00035152008,
    -0.00041664584,
    -0.00046383071,
    -0.0004131211,
    -0.0004703398,
    -0.00041907397,
    -0.00028594965,
    -0.0002836588,
    -0.00031874585,
    -0.00023711874,
    -0.000267677,
    -0.0002049809,
    -0.00024235052,
    -0.00018757087,
    -0.00023073469,
    -0.0001721245,
    -0.0002092402,
    -0.00016225711,
    -0.0001987486,
    -0.0001499964,
    -0.00018773323,
    -0.00014108633,
    -0.00008909872,
    -0.00006227259,
    -0.00010247962,
    -0.00013381199,
    -0.000057028294,
    -0.00011289501,
    -0.0001481295,
    -0.00017926776,
    -0.00013800371,
    -0.00017888175,
    -0.00014718024,
    -0.000115891744,
    -0.00009865972,
    -0.00012588227,
    -0.00014533648,
    -0.00015823983,
    -0.0001169622,
    -0.00015542419,
    -0.00012230722,
    -0.00009141508,
    -0.000068696536,
    -0.00003434902,
    -0.000003405161,
    -0.000034427063,
    0.000013745997,
    -0.000013917833,
    0.00002517773,
    -0.000009594136,
    0.000027176982,
    -0.000006826173,
    0.000025878671,
    0.00006104425,
    0.0000761183,
    0.000043645923,
    0.000016591166,
    0.00007020828,
    0.000024037368,
    0.00006161385,
    0.000019607238,
    0.00005222777,
    0.000085446474,
    0.000035877536,
    0.00008139619,
    0.00010465333,
    0.0001321316,
    0.0000911244,
    0.00006948422,
    0.000051440846,
    0.000025125784,
    0.000058851714,
    0.00006679643,
    0.00007917189,
    0.000097577795,
    0.00017788081,
    0.00013449864,
    0.000113731345,
    0.00010559635,
    0.00013815417,
    0.000102875856,
    0.000069948874,
    0.00004959022,
    0.000017785953,
    0.000051436153,
    0.00006287675,
    0.000073632094,
    0.00009297575,
    0.00006125957,
    0.000045362824,
    0.000028204115,
    0.000005843962,
    -0.000021768203,
    -0.00004984023,
    -0.00007661332,
    -0.00010647597,
    -0.00007628251,
    -0.00006148941,
    -0.000046500918,
    -0.00007924863,
    -0.000096379714,
    -0.00010777723,
    -0.0001297998,
    -0.00009537318,
    -0.00007924525,
    -0.000119052376,
    -0.00008138536,
    -0.000050684197,
    -0.000086116626
];
        let left = vec![
    0.0,
    0.000026856527,
    0.000047767004,
    0.0000675792,
    0.000037950907,
    0.00007361174,
    0.00004842155,
    0.00007700742,
    0.000049432798,
    0.00007357977,
    0.00009864361,
    0.000059388796,
    0.000043718006,
    0.0000738314,
    0.000094481584,
    0.0000552968,
    0.000034567776,
    0.000016641621,
    0.000044509583,
    0.000062219304,
    0.000022642611,
    0.00005048268,
    0.00007241323,
    0.00004179756,
    0.00002272755,
    0.000005652104,
    0.000032961525,
    -0.00000012799654,
    0.000023937448,
    0.000042557844,
    0.00005587733,
    0.00007916408,
    0.000050148134,
    0.000035875913,
    0.000020642283,
    0.00004952278,
    0.000016265893,
    -0.000011167162,
    -0.000028078062,
    -0.00005071203,
    -0.000024359058,
    -0.00006738367,
    -0.0000975618,
    -0.00005918362,
    -0.00003496505,
    -0.000014939646,
    -0.000051263658,
    -0.000012917168,
    -0.00016304075,
    -0.00028056273,
    -0.00021020102,
    -0.0002637392,
    -0.0003106788,
    -0.00034155956,
    -0.00030850744,
    -0.00034197696,
    -0.00030341704,
    -0.00021197842,
    -0.00020607849,
    -0.00022497708,
    -0.00017298502,
    -0.00018689412,
    -0.00014804589,
    -0.00016692035,
    -0.00013425323,
    -0.00015746107,
    -0.00012338205,
    -0.00014308868,
    -0.000116070645,
    -0.00013572138,
    -0.0001076234,
    -0.0001280017,
    -0.00009819423,
    -0.00006310858,
    -0.00004586245,
    -0.00007114727,
    -0.00008956536,
    -0.00004445191,
    -0.0000781147,
    -0.00010312785,
    -0.00012252324,
    -0.00009911802,
    -0.00012225151,
    -0.000102652375,
    -0.00008138929,
    -0.000070431,
    -0.000087570676,
    -0.00010111334,
    -0.00010850407,
    -0.00008436676,
    -0.00010592205,
    -0.000085360196,
    -0.000064060914,
    -0.000047518108,
    -0.000024122884,
    -0.000004544596,
    -0.00002127729,
    0.000006974824,
    -0.000007423361,
    0.000015014924,
    -0.0000042315482,
    0.000016521928,
    -0.000002417499,
    0.00001817611,
    0.00004183271,
    0.000051386287,
    0.000030667325,
    0.000014401053,
    0.000045520395,
    0.00001964373,
    0.000040245184,
    0.00001629926,
    0.000036452147,
    0.000056711666,
    0.00002836532,
    0.000056059343,
    0.00007309567,
    0.000089739275,
    0.00006421899,
    0.000048612987,
    0.000035638717,
    0.000019659641,
    0.000040180777,
    0.000046746434,
    0.00005549869,
    0.00007020909,
    0.00011972043,
    0.0000945622,
    0.00007984316,
    0.00007508902,
    0.00009397969,
    0.000071814095,
    0.00004924415,
    0.000034244353,
    0.000014733809,
    0.00003513578,
    0.00004383111,
    0.000051638355,
    0.00006304898,
    0.000043266093,
    0.000031609117,
    0.00001948501,
    0.0000038841035,
    -0.000015206173,
    -0.0000347261,
    -0.000053513744,
    -0.00007213981,
    -0.0000537265,
    -0.00004293768,
    -0.00003414388,
    -0.000054758813,
    -0.00006702926,
    -0.00007552501,
    -0.00008854988,
    -0.000067192654,
    -0.000057248515,
    -0.000080323196,
    -0.000057019606,
    -0.000037682505,
    -0.000057456866
];
        decode(in_slice, true, &right, &left);
    }

    #[test]
    fn lsf_to_lpc() {
        let lsf = vec![
            321i16, 2471, 5904, 9856, 12928, 16000, 19328, 22400, 25728, 28800,
        ];
        let mut lpc = [0.0; 10];

        let reference = [
            1.2307129,
            -0.30419922,
            0.24829102,
            -0.14990234,
            0.10522461,
            -0.13671875,
            0.031982422,
            -0.0871582,
            0.06933594,
            -0.011230469,
        ];

        NB_MB::lsf_to_lpc(&mut lpc, lsf);

        assert_eq!(lpc, reference);
    }

}
