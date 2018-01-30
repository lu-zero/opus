//!
//! Silk Decoding
//!
//! See [section-4.2](https://tools.ietf.org/html/rfc6716#section-4.2)
//!

use entropy::*;
use packet::*;
use codec::error::*;

#[derive(Debug, Default)]
pub struct SilkFrame {
    frame_type: FrameType,
    log_gain: isize,
    coded: bool,
}

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

const LSF_STAGE1: &[&ICDFContext] = &[
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

use self::lsf_stage2_nb_mb::MAP as LSF_NB_MB_MAP;

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

use self::lsf_stage2_wb::MAP as LSF_WB_MAP;

const LSF_STAGE2_EXTENSION: &ICDFContext = &ICDFContext {
    total: 256,
    dist: &[156, 216, 240, 249, 253, 255, 256],
};

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

    fn parse_lpc(&mut self, rd: &mut RangeDecoder, wb: bool) -> usize {
        let mut res = [0; 16];
        let mut lsfs_s2 = [0isize; 16];

        let idx = self.frame_type.voiced_index() | ((wb as usize) << 1);
        let lsf_s1 = rd.decode_icdf(LSF_STAGE1[idx]);

        let (map, step, weight_map_val, weight_map_index) = if wb {
            (LSF_WB_MAP[lsf_s1],
             9830,
             LSF_PRED_MAP_VALUES_WB,
             LSF_PRED_MAP_INDEXES_WB[lsf_s1])
        } else {
            (LSF_NB_MB_MAP[lsf_s1],
            11796,
            LSF_PRED_MAP_VALUES_NB_MB,
            LSF_PRED_MAP_INDEXES_NB_MB[lsf_s1])
        };

        for (mut lsf_s2, icdf) in lsfs_s2.iter_mut().zip(map) {
            let lsf = rd.decode_icdf(icdf) as isize - 4;
            *lsf_s2 = if lsf == -4 {
                lsf - rd.decode_icdf(LSF_STAGE2_EXTENSION) as isize
            } else if lsf == 4 {
                lsf + rd.decode_icdf(LSF_STAGE2_EXTENSION) as isize
            } else {
                lsf
            };
        }

        let dequant_step = |lsf_s2: isize| -> isize {
            let fix = if lsf_s2 < 0 {
                102
            } else if lsf_s2 > 0 {
                -102
            } else {
                0
            };

            ((lsf_s2 * 1024 + fix) * step) >> 16
        };

        let mut prev = None;
        for (i, (mut res, lsf_s2)) in res.iter_mut().zip(lsfs_s2.iter()).enumerate().rev() {
            let r = dequant_step(*lsf_s2);

            *res = r + if let Some(p) = prev {
                let weight = weight_map_val[weight_map_index[i]][i];

                (p * weight) >> 8
            } else {
                0
            };

            prev = Some(*res);
        }

        0
    }

    fn parse(
        &mut self,
        rd: &mut RangeDecoder,
        info: &SilkInfo,
        vad: bool,
        first: bool,
    ) -> Result<()> {
        self.frame_type = if vad {
            if rd.decode_icdf(FRAME_TYPE_INACTIVE) == 0 {
                FrameType::InactiveLow
            } else {
                FrameType::InactiveHigh
            }
        } else {
            match rd.decode_icdf(FRAME_TYPE_ACTIVE) {
                0 => FrameType::UnvoicedLow,
                1 => FrameType::UnvoicedHigh,
                2 => FrameType::VoicedLow,
                3 => FrameType::VoicedHigh,
                _ => unreachable!(),
            }
        };

        println!("Type {:?}", self.frame_type);

        let mut sfs: [SubFrame; 4] = Default::default();

        for (i, mut sf) in &mut sfs[..info.subframes].iter_mut().enumerate() {
            let coded = i == 0 && (first || !self.coded);
            sf.gain = self.parse_subframe_gains(rd, coded);
        }

        self.parse_lpc(rd, info.bandwidth > Bandwidth::Medium);

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
        self.info.bandwidth = pkt.bandwidth.max(Bandwidth::Wide);
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
        println!("{:?}", self);
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

            self.mid_frame.parse(rd, &self.info, mid_vad[i], i == 0)?;

            if self.stereo && !midonly {
                self.side_frame.parse(rd, &self.info, side_vad[i], i == 0)?;
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
}
