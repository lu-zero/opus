use std::ops::Range;

use entropy::*;
use maths::*;
use packet::*;


const SHORT_BLOCKSIZE: usize = 120;
const MAX_BANDS: usize = 21;
const MIN_PERIOD: usize = 15;

#[derive(Debug, Default)]
struct PostFilter {
    period: usize,
    period_new: usize,
    period_old: usize,

    gains: [f32; 3],
    gains_new: [f32; 3],
    gains_old: [f32; 3],
}

#[derive(Debug)]
struct CeltFrame {
    pf: PostFilter,
    energy: [f32; MAX_BANDS],
    prev_energy: [f32; MAX_BANDS],
    collapse_masks: [u8; MAX_BANDS],

    buf: Vec<f32>, // TODO: replace with an array once const-generics

    deemph_coeff: f32,
}

impl Default for CeltFrame {
    fn default() -> Self {
        CeltFrame {
            pf: Default::default(),
            energy: Default::default(),
            prev_energy: Default::default(),
            collapse_masks: Default::default(),

            buf: vec![0f32; 2048],

            deemph_coeff: 0f32,
        }
    }
}

#[derive(Debug)]
pub struct Celt {
    stereo: bool,
    stereo_pkt: bool,
    bits: usize,
    lm: usize, // aka duration in mdct blocks
    band: Range<usize>,
    frames: [CeltFrame; 2],

    fine_bits: [usize; MAX_BANDS],
    fine_priority: [usize; MAX_BANDS],
    pulses: [usize; MAX_BANDS],
    tf_change: [usize; MAX_BANDS],
}

const POSTFILTER_TAPS: &[&[f32]] = &[
    &[0.3066406250, 0.2170410156, 0.1296386719],
    &[0.4638671875, 0.2680664062, 0.0],
    &[0.7998046875, 0.1000976562, 0.0],
];

const TAPSET: &ICDFContext = &ICDFContext {
    total: 4,
    dist: &[2, 3, 4]
};

const ALPHA_COEF: &[f32] = &[
    29440.0/32768.0,    26112.0/32768.0,    21248.0/32768.0,    16384.0/32768.0
];

const BETA_COEF: &[f32] = &[
    1.0 - 30147.0/32768.0,    1.0 - 22282.0 / 32768.0,    1.0 - 12124.0/32768.0,     1.0 - 6554.0/32768.0
];

// TODO: make it a &[&[(u8, u8)]] if it makes no speed difference
const COARSE_ENERGY_INTRA: &[&[u8]] = &[
    // 120-samples
    &[ 24, 179,  48, 138,  54, 135,  54, 132,  53, 134,  56, 133,  55, 132,
             55, 132,  61, 114,  70,  96,  74,  88,  75,  88,  87,  74,  89,  66,
             91,  67, 100,  59, 108,  50, 120,  40, 122,  37,  97,  43,  78,  50
    ],
    // 240-samples
    &[ 23, 178,  54, 115,  63, 102,  66,  98,  69,  99,  74,  89,  71,  91,
             73,  91,  78,  89,  86,  80,  92,  66,  93,  64, 102,  59, 103,  60,
            104,  60, 117,  52, 123,  44, 138,  35, 133,  31,  97,  38,  77,  45
    ],
    // 480-samples
    &[ 21, 178,  59, 110,  71,  86,  75,  85,  84,  83,  91,  66,  88,  73,
             87,  72,  92,  75,  98,  72, 105,  58, 107,  54, 115,  52, 114,  55,
            112,  56, 129,  51, 132,  40, 150,  33, 140,  29,  98,  35,  77,  42
    ],
    // 960-samples
    &[ 22, 178,  63, 114,  74,  82,  84,  83,  92,  82, 103,  62,  96,  72,
             96,  67, 101,  73, 107,  72, 113,  55, 118,  52, 125,  52, 118,  52,
            117,  55, 135,  49, 137,  39, 157,  32, 145,  29,  97,  33,  77,  40
    ],
];

const COARSE_ENERGY_INTER: &[&[u8]] = &[
    // 120-samples
    &[ 72, 127,  65, 129,  66, 128,  65, 128,  64, 128,  62, 128,  64, 128,
             64, 128,  92,  78,  92,  79,  92,  78,  90,  79, 116,  41, 115,  40,
            114,  40, 132,  26, 132,  26, 145,  17, 161,  12, 176,  10, 177,  11
    ],
    // 240-samples
    &[ 83,  78,  84,  81,  88,  75,  86,  74,  87,  71,  90,  73,  93,  74,
             93,  74, 109,  40, 114,  36, 117,  34, 117,  34, 143,  17, 145,  18,
            146,  19, 162,  12, 165,  10, 178,   7, 189,   6, 190,   8, 177,   9
    ],
    // 480-samples
    &[ 61,  90,  93,  60, 105,  42, 107,  41, 110,  45, 116,  38, 113,  38,
            112,  38, 124,  26, 132,  27, 136,  19, 140,  20, 155,  14, 159,  16,
            158,  18, 170,  13, 177,  10, 187,   8, 192,   6, 175,   9, 159,  10
    ],
    // 960-samples
    &[ 42, 121,  96,  66, 108,  43, 111,  40, 117,  44, 123,  32, 120,  36,
            119,  33, 127,  33, 134,  34, 139,  21, 147,  23, 152,  20, 158,  25,
            154,  26, 166,  21, 173,  16, 184,  13, 184,  10, 150,  13, 139,  15
    ],
];

const MODEL_ENERGY_SMALL: &ICDFContext = &ICDFContext {
    total: 4,
    dist: &[2, 3, 4],
};

impl Celt {
    pub fn new(stereo: bool) -> Self {
        let frames = Default::default();
        Celt { stereo, stereo_pkt: false, bits: 0, lm: 0, frames, band: 0..MAX_BANDS,
                fine_bits: Default::default(),
                fine_priority: Default::default(),
                pulses: Default::default(),
                tf_change: Default::default(),
        }
    }

    pub fn setup(&mut self, pkt: &Packet) {
        self.stereo_pkt = pkt.stereo;
    }

    fn reset_gains(&mut self) {
       self.frames[0].pf.gains_new = [0.0; 3];
       self.frames[1].pf.gains_new = [0.0; 3];
    }

    fn parse_postfilter(&mut self, rd: &mut RangeDecoder) {
        if rd.decode_logp(1) {
            let octave = rd.decode_uniform(6);
            let period = (16 << octave) + rd.rawbits(4 + octave) - 1;
            let gain_bits = rd.rawbits(3) + 1;
            let gain = gain_bits as f32 * 0.09375;

            let tapset = if rd.available() >= 2 {
                rd.decode_icdf(TAPSET)
            } else {
                0
            };

            println!("postfilter: octave {}, period {}, gain {}, tapset {}",
                     octave, period, gain, tapset);
            let taps = POSTFILTER_TAPS[tapset];
            for frame in self.frames.iter_mut() {
                frame.pf.period_new = period.max(MIN_PERIOD);
                frame.pf.gains_new = [taps[0] * gain, taps[1] * gain, taps[2] * gain];
            }
        } else {
            println!("postfilter: no");
        }
    }

    fn decode_coarse_energy(&mut self, rd: &mut RangeDecoder, band: Range<usize>) {
        let (alpha, beta, model) = if rd.available() > 3 && rd.decode_logp(3) {
            (0f32, 1f32 - 4915f32 / 32768f32, COARSE_ENERGY_INTRA[self.lm])
        } else {
            (ALPHA_COEF[self.lm], 1f32 - BETA_COEF[self.lm], COARSE_ENERGY_INTER[self.lm])
        };

        self.frames.iter_mut().for_each(|f| {
            let mut energy = f.energy.iter_mut().enumerate();
            let mut prev = 0f32;

            energy.by_ref().take(band.start).for_each(|(_, en)| {
                *en = 0f32;
            });

            energy.by_ref().take(band.end - band.start).for_each(|(i, en)| {
                let available = rd.available();
                let value = if available >= 15 {
                    let k = i.min(20) << 1;
                    rd.decode_laplace((model[k] as usize) << 7, (model[k + 1] as isize) << 6)
                } else if available >= 1 {
                    let v = rd.decode_icdf(MODEL_ENERGY_SMALL) as isize;
                    (v >> 1) ^ - (v & 1)
                } else {
                    -1
                } as f32;

                *en = en.max(-9f32) * alpha + prev + value;
                prev += beta * value;
            });

            energy.by_ref().for_each(|(_, en)| {
                *en = 0.0;
            });
        });

        println!("{:#?}", &self.frames[0].energy[..]);
        println!("{:#?}", &self.frames[1].energy[..]);
    }

    fn decode_fine_energy(&mut self, rd: &mut RangeDecoder, band: Range<usize>) {
        self.frames.iter_mut().for_each(|f| {
            let energy = f.energy.iter_mut().enumerate();
        });
    }

    pub fn decode(&mut self, rd: &mut RangeDecoder, out_buf: &mut [f32], frame_duration: FrameDuration, band: Range<usize>) {
        assert!(band.end <= MAX_BANDS);

        let frame_size = frame_duration as usize;

        self.lm = (frame_size / SHORT_BLOCKSIZE).ilog() - 1;

        let silence = if rd.available() > 0 {
            rd.decode_logp(15)
        } else {
            true
        };

        println!("silence {}", silence);

        if silence {
            // Pretend we are at the end of the buffer
            rd.to_end();
        }

        self.reset_gains();
        if band.start == 0 && rd.available() >= 16 {
            self.parse_postfilter(rd);
        }

        let transient = if self.lm != 0 && rd.available() >= 3 {
            rd.decode_logp(3)
        } else {
            false
        };

        println!("duration {}, transient {}", self.lm, transient);

        let blocks = if transient { 1 << self.lm } else { 1 };
        let blocksize = frame_size / blocks;

        if !self.stereo_pkt {
            let (f0, f1) = self.frames.split_at_mut(1);

            f0[0].energy.iter_mut()
                .zip(f1[0].energy.iter())
                .for_each(|(e0, &e1)| {
                *e0 = e0.max(e1)
            });
        }

        self.frames.iter_mut().for_each(|f| f.collapse_masks.iter_mut().for_each(|c| *c = 0));

        self.decode_coarse_energy(rd, band);

    }
}


mod test {

}
