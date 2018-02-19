//!
//! Silk Decoding
//!
//! See [section-4.2](https://tools.ietf.org/html/rfc6716#section-4.2)
//!

use entropy::*;
use packet::*;
use maths::*;
use codec::error::*;

#[derive(Debug)]
pub struct SilkInfo {
    bandwidth: Bandwidth,
    subframes: usize,

    weight0: f32,
    weight1: f32,
}

#[derive(Debug)]
pub struct Silk {
    stereo: bool,
    frames: usize,
    frame_len: usize,
    subframe_len: usize,
    info: SilkInfo,

    mid_frame: SilkFrame,
    side_frame: SilkFrame,
}

#[derive(Debug, Default)]
struct SubFrame {
    gain: f32,
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
        175, 148, 160, 176, 178, 173, 174, 164, 177, 174, 196, 182, 198, 192, 182
    ],
    &[
        68, 62, 66, 60, 72, 117, 85, 90, 118, 136, 151, 142, 160, 142, 155
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
        7, 23, 38, 54, 69, 85, 100, 116, 131, 147, 162, 178, 193, 208, 223, 239
    ],
    &[
        13, 25, 41, 55, 69, 83, 98, 112, 127, 142, 157, 171, 187, 203, 220, 236
    ],
    &[
        15, 21, 34, 51, 61, 78, 92, 106, 126, 136, 152, 167, 185, 205, 225, 240
    ],
    &[
        10, 21, 36, 50, 63, 79, 95, 110, 126, 141, 157, 173, 189, 205, 221, 237
    ],
    &[
        17, 20, 37, 51, 59, 78, 89, 107, 123, 134, 150, 164, 184, 205, 224, 240
    ],
    &[
        10, 15, 32, 51, 67, 81, 96, 112, 129, 142, 158, 173, 189, 204, 220, 236
    ],
    &[
        8, 21, 37, 51, 65, 79, 98, 113, 126, 138, 155, 168, 179, 192, 209, 218
    ],
    &[
        12, 15, 34, 55, 63, 78, 87, 108, 118, 131, 148, 167, 185, 203, 219, 236
    ],
    &[
        16, 19, 32, 36, 56, 79, 91, 108, 118, 136, 154, 171, 186, 204, 220, 237
    ],
    &[
        11, 28, 43, 58, 74, 89, 105, 120, 135, 150, 165, 180, 196, 211, 226, 241
    ],
    &[
        6, 16, 33, 46, 60, 75, 92, 107, 123, 137, 156, 169, 185, 199, 214, 225
    ],
    &[
        11, 19, 30, 44, 57, 74, 89, 105, 121, 135, 152, 169, 186, 202, 218, 234
    ],
    &[
        12, 19, 29, 46, 57, 71, 88, 100, 120, 132, 148, 165, 182, 199, 216, 233
    ],
    &[
        17, 23, 35, 46, 56, 77, 92, 106, 123, 134, 152, 167, 185, 204, 222, 237
    ],
    &[
        14, 17, 45, 53, 63, 75, 89, 107, 115, 132, 151, 171, 188, 206, 221, 240
    ],
    &[
        9, 16, 29, 40, 56, 71, 88, 103, 119, 137, 154, 171, 189, 205, 222, 237
    ],
    &[
        16, 19, 36, 48, 57, 76, 87, 105, 118, 132, 150, 167, 185, 202, 218, 236
    ],
    &[
        12, 17, 29, 54, 71, 81, 94, 104, 126, 136, 149, 164, 182, 201, 221, 237
    ],
    &[
        15, 28, 47, 62, 79, 97, 115, 129, 142, 155, 168, 180, 194, 208, 223, 238
    ],
    &[
        8, 14, 30, 45, 62, 78, 94, 111, 127, 143, 159, 175, 192, 207, 223, 239
    ],
    &[
        17, 30, 49, 62, 79, 92, 107, 119, 132, 145, 160, 174, 190, 204, 220, 235
    ],
    &[
        14, 19, 36, 45, 61, 76, 91, 108, 121, 138, 154, 172, 189, 205, 222, 238
    ],
    &[
        12, 18, 31, 45, 60, 76, 91, 107, 123, 138, 154, 171, 187, 204, 221, 236
    ],
    &[
        13, 17, 31, 43, 53, 70, 83, 103, 114, 131, 149, 167, 185, 203, 220, 237
    ],
    &[
        17, 22, 35, 42, 58, 78, 93, 110, 125, 139, 155, 170, 188, 206, 224, 240
    ],
    &[
        8, 15, 34, 50, 67, 83, 99, 115, 131, 146, 162, 178, 193, 209, 224, 239
    ],
    &[
        13, 16, 41, 66, 73, 86, 95, 111, 128, 137, 150, 163, 183, 206, 225, 241
    ],
    &[
        17, 25, 37, 52, 63, 75, 92, 102, 119, 132, 144, 160, 175, 191, 212, 231
    ],
    &[
        19, 31, 49, 65, 83, 100, 117, 133, 147, 161, 174, 187, 200, 213, 227, 242
    ],
    &[
        18, 31, 52, 68, 88, 103, 117, 126, 138, 149, 163, 177, 192, 207, 223, 239
    ],
    &[
        16, 29, 47, 61, 76, 90, 106, 119, 133, 147, 161, 176, 193, 209, 224, 240
    ],
    &[
        15, 21, 35, 50, 61, 73, 86, 97, 110, 119, 129, 141, 175, 198, 218, 237
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

#[derive(Debug, PartialEq, Clone, Copy)]
enum FrameType {
    InactiveLow = 0b00,
    InactiveHigh = 0b01,
    UnvoicedLow = 0b10,
    UnvoicedHigh = 0b11,
    VoicedLow = 0b100,
    VoicedHigh = 0b101,
}

impl Default for FrameType {
    fn default() -> Self {
        FrameType::VoicedLow
    }
}

impl FrameType {
    #[inline(always)]
    fn voiced_index(&self) -> usize {
        *self as usize >> 2
    }
    #[inline(always)]
    fn signal_type_index(&self) -> usize {
        *self as usize >> 1
    }

    #[inline(always)]
    fn qoffset_type_index(&self) -> usize {
        *self as usize & 0b001
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

            if min_diff == 0 {
                return;
            }

            if k == 0 {
                nlsfs[0] = Self::MIN_SPACING[0];
            } else if k == Self::ORDER {
                nlsfs[Self::ORDER - 1] = 32760 - Self::MIN_SPACING[Self::ORDER];
            } else {
                let half_delta = (Self::MIN_SPACING[k] >> 1) as i16;
                let min_center = (Self::MIN_SPACING[..k].iter().sum::<i16>() + half_delta) as i32;
                let max_center =
                    (32760 - Self::MIN_SPACING[k + 1..].iter().sum::<i16>() - half_delta) as i32;
                let delta = nlsfs[k - 1] as i32 - nlsfs[k] as i32;
                let center = (delta >> 1) - (delta & 1);

                nlsfs[k - 1] = center.min(max_center).max(min_center) as i16 - half_delta;
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
            let (k, &maxabs) = a.iter()
                .enumerate()
                .rev()
                .max_by_key(|&(_i, v)| v.abs())
                .unwrap();

            let maxabs = (maxabs.abs() + (1 << 4)) >> 5;

            if maxabs > 32767 {
                let max = maxabs.max(163838);
                let start = 65470 - ((max - 32767) << 14) / ((max * (k as i32 + 1)) >> 2);
                let mut chirp = start;

                for v in a.iter_mut() {
                    *v = v.mul_round(chirp, 16);
                    chirp = (start * chirp + 32768) >> 16;
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

        println!("{:#?}", lsps);
        // TODO: fuse p and q as even/odd and zip it
        for (i, lsp) in lsps[2..].chunks(2).enumerate() {
            p[i + 2] = p[i] * 2 - lsp[0].mul_round(p[i + 1], 16);
            println!(
                "[{}] {} = {} * 2 - {} * {}",
                i + 2,
                p[i + 2],
                p[i],
                lsp[0],
                p[i + 1]
            );
            q[i + 2] = q[i] * 2 - lsp[1].mul_round(q[i + 1], 16);

            // TODO: benchmark let mut w = &p[j-2..j+1]
            // would be p[0..i+1].windows_mut(3).rev()
            for j in (2..i + 2).rev() {
                let v = p[j - 2] - lsp[0].mul_round(p[j - 1], 16);
                p[j] += v;
                println!(" [{}] {} = {} - {} * {}", j, v, p[j - 2], lsp[0], p[j - 1]);
                q[j] += q[j - 2] - lsp[1].mul_round(q[j - 1], 16);
            }

            p[1] -= lsp[0];
            q[1] -= lsp[1];
        }

        println!("{:#?}", p);
        println!("{:#?}", q);

        let mut a = vec![0; Self::ORDER];
        {
            let (a0, a1) = a.split_at_mut(Self::ORDER / 2);
            let it = a0.iter_mut().zip(a1.iter_mut().rev());
            let co = p.windows(2).zip(q.windows(2));
            for ((v0, v1), (pv, qv)) in it.zip(co) {
                let ps = pv[0] + pv[1];
                println!("{} = {} + {}", ps, pv[0], pv[1]);
                let qs = qv[1] - qv[0];
                //                println!("{} = {} + {}", qs, qv[0], qv[1]);
                *v0 = -ps - qs;
                *v1 = -ps + qs;
            }
        }

        println!("{:#?}", a);

        Self::range_limit(lpcs, &mut a);
    }
}

pub struct NB_MB;
pub struct WB;

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

#[derive(Debug, Default)]
pub struct SilkFrame {
    frame_type: FrameType,
    log_gain: isize,
    coded: bool,
    nlsfs: [i16; 16],
    lpc: [f32; 16],
    interpolated_lpc: [f32; 16],
}

impl SilkFrame {
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

    fn parse_lpc<B: Band>(&mut self, rd: &mut RangeDecoder, interpolate: bool) {
        /* TODO: use this once rust supports that
        let mut res = [0; B::ORDER];
        let mut lsfs_s2 = [0; B::ORDER];
        let mut nlsfs = [0; B::ORDER];
        */

        let idx = self.frame_type.voiced_index();
        let lsf_s1 = rd.decode_icdf(B::STAGE1[idx]);

        // TODO: store in the Band trait
        let (map, step, weight_map, weight_map_index, weights, codebooks) = (
            B::MAP[lsf_s1],
            B::STEP,
            B::PRED_WEIGHT,
            B::PRED_WEIGHT_INDEX[lsf_s1],
            B::WEIGHT[lsf_s1],
            B::CODEBOOK[lsf_s1],
        );
        /*
        if wb {
            println!("wb");
            (
                LSF_WB_MAP[lsf_s1],
                9830,
                LSF_PRED_WEIGHT_WB,
                LSF_PRED_WEIGHT_INDEX_WB[lsf_s1],
                LSF_WEIGHT_WB[lsf_s1],
                LSF_CODEBOOK_WB[lsf_s1],
                LSF_MIN_SPACING_WB,
            )
        } else {
            (
                LSF_NB_MB_MAP[lsf_s1],
                11796,
                LSF_PRED_WEIGHT_NB_MB,
                LSF_PRED_WEIGHT_INDEX_NB_MB[lsf_s1],
                LSF_WEIGHT_NB_MB[lsf_s1],
                LSF_CODEBOOK_NB_MB[lsf_s1],
                LSF_MIN_SPACING_NB_MB,
            )
        };*/

        /*
        for (mut lsf_s2, icdf) in lsfs_s2.iter_mut().zip(map) {
            let lsf = rd.decode_icdf(icdf) as isize - 4;
            *lsf_s2 = if lsf == -4 {
                lsf - rd.decode_icdf(LSF_STAGE2_EXTENSION) as isize
            } else if lsf == 4 {
                lsf + rd.decode_icdf(LSF_STAGE2_EXTENSION) as isize
            } else {
                lsf
            };
            println!("lsf2 {}", *lsf_s2);
        } */

        let lsfs_s2 = map.iter()
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

        println!("lsfs2_s2 {:?}", lsfs_s2);

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
                    let weight = weight_map[weight_map_index[i]][i] as i16;

                    (p * weight) >> 8
                } else {
                    0
                };

                prev = Some(res);

                res
            })
            .collect::<Vec<i16>>();

        println!("residuals {:#?}", residuals);

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

        println!("nlsf {:#?}", nlsfs);

        // Damage control
        B::stabilize(&mut nlsfs);

        if interpolate {
            let weight = rd.decode_icdf(LSF_INTERPOLATION_INDEX) as i16;
            if weight != 4 && self.coded {
                if weight != 0 {
                    let interpolated_nlsfs = nlsfs
                        .iter()
                        .zip(&self.nlsfs)
                        .map(|(&nlsf, &prev)| prev + ((nlsf - prev) * weight) >> 2);
                    B::lsf_to_lpc(&mut self.interpolated_lpc, interpolated_nlsfs);
                } else {
                    (&mut self.interpolated_lpc[..B::ORDER]).copy_from_slice(&self.lpc[..B::ORDER]);
                }
            }
        }

        (&mut self.nlsfs[..B::ORDER]).copy_from_slice(&nlsfs);

        B::lsf_to_lpc(&mut self.lpc, nlsfs);

        println!("lpc {:#?}", self.lpc);

        unreachable!();
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
                0 => FrameType::UnvoicedLow,
                1 => FrameType::UnvoicedHigh,
                2 => FrameType::VoicedLow,
                3 => FrameType::VoicedHigh,
                _ => unreachable!(),
            }
        } else {
            if rd.decode_icdf(FRAME_TYPE_INACTIVE) == 0 {
                FrameType::InactiveLow
            } else {
                FrameType::InactiveHigh
            }
        };

        println!("Type {:?}", self.frame_type);

        let mut sfs: [SubFrame; 4] = Default::default();

        for (i, mut sf) in &mut sfs[..info.subframes].iter_mut().enumerate() {
            let coded = i == 0 && (first || !self.coded);
            sf.gain = self.parse_subframe_gains(rd, coded);
            println!("subframe {} coded {} gain {}", i, coded, sf.gain);
        }

        if info.bandwidth > Bandwidth::Medium {
            self.parse_lpc::<WB>(rd, info.subframes == 4);
        } else {
            self.parse_lpc::<NB_MB>(rd, info.subframes == 4);
        }

        Ok(())
    }
}

impl Silk {
    pub fn new() -> Self {
        Silk {
            stereo: true,
            frames: 0,
            frame_len: 0,
            subframe_len: 0,

            info: SilkInfo {
                subframes: 0,
                bandwidth: Bandwidth::Full,

                weight0: 0f32,
                weight1: 0f32,
            },

            mid_frame: Default::default(),
            side_frame: Default::default(),
        }
    }

    pub fn setup(&mut self, pkt: &Packet) {
        self.stereo = pkt.stereo;
        self.info.bandwidth = pkt.bandwidth.min(Bandwidth::Wide);
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

        println!("{:?}", self);

        if vad {
            false
        } else {
            rd.decode_icdf(MID_ONLY) != 0
        }
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

        println!("{:?} {:?}", mid_vad, side_vad);
        for i in 0..self.frames {
            let coded = i == 0;
            let midonly = if self.stereo {
                self.parse_stereo_weight(rd, side_vad[i])
            } else {
                false
            };

            self.mid_frame.parse(rd, &self.info, mid_vad[i], coded)?;

            if self.stereo && !midonly {
                self.side_frame.parse(rd, &self.info, side_vad[i], coded)?;
            }
        }

        Ok(0)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn decode() {
        let mut p = Packet::from_slice(&[
            12, 9, 178, 70, 140, 148, 202, 129, 225, 86, 64, 234, 160
        ]).unwrap();

        let mut silk = Silk::new();

        silk.setup(&p);

        for frame in p.frames {
            let mut rd = RangeDecoder::new(frame);

            silk.decode(&mut rd);
        }
    }

    #[test]
    fn lsf_to_lpc() {
        let lsf = vec![
            321i16, 2471, 5904, 9856, 12928, 16000, 19328, 22400, 25728, 28800
        ];
        let mut lpc = [0.0; 10];

        let mut reference = [
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
