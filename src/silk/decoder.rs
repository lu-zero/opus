//!
//! Silk Decoding
//!
//! See [section-4.2](https://tools.ietf.org/html/rfc6716#section-4.2)
//!

use crate::codec::error::*;
use crate::entropy::*;
use crate::maths::*;
use crate::packet::*;

use crate::silk::tables::*;

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
    pub left_outbuf: Vec<f32>,
    pub right_outbuf: Vec<f32>,
}

#[derive(Debug, Default)]
struct SubFrame {
    gain: f32,
    pitch_lag: i32,
    ltp_taps: [f32; 5],
}

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
                        // let idx = (((avail - 1 + 5) * (avail - 1)) >> 1) as usize;
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
        // println!("bandwidth {:?} {}", info.bandwidth, info.bandwidth > Bandwidth::Medium);
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
            /* println!(
                "history {:.6} output {:.6}",
                self.lpc_history[i], self.output[i]
            ); */
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

    pub fn flush(&mut self) {
        self.mid_frame.flush();
        self.side_frame.flush();

        self.info.prev0 = 0.0;
        self.info.prev1 = 0.0;
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

        // println!("rem");

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
                // println!("unmix");
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

/*        println!("stereo {} out {}", self.stereo, self.stereo_out);
        println!(
            "right: {:#?}",
            &self.right_outbuf[..self.frames * self.info.f_size]
        );
        println!(
            "left: {:#?}",
            &self.left_outbuf[..self.frames * self.info.f_size]
        );
*/
        Ok(0)
    }
}
