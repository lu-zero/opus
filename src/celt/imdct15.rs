use crate::complex::*;

#[derive(Debug)]
pub struct IMDCT15 {
    n: usize,
    len2: usize,
    len4: usize,

    tmp: Vec<Complex32>,
    exptab: Vec<Vec<Complex32>>,
    twiddle: Vec<Complex32>,
}

fn p2len(p2: usize) -> usize {
    15 * (1 << p2)
}

/* once num supports const fn
const fn fact(n: f64) -> Complex32 {
    let v = (n * 2f64 * Complex64::i() * PI / 5f64).exp();

    Complex32::new(v.re as f32, v.im as f32)
}
*/

const FACT: &[Complex32] = &[
    Complex32 {
        re: 0.30901699437494745,
        im: 0.95105651629515353,
    },
    Complex32 {
        re: -0.80901699437494734,
        im: 0.58778525229247325,
    },
];

/* Below the equivalent with less factors
fn m_c(out: &mut [Complex32], inp: Complex32) {
    out[0] = imp * FACT[0];
    out[1] = imp * FACT[1];
    out[2] = imp * FACT[1].conj();
    out[3] = imp * FACT[0].conj();
}
Once const fn and step_by are stabler reconsider the code
*/

#[inline]
fn mulc(a: Complex32, b: Complex32) -> (f32, f32, f32, f32) {
    (a.re * b.re, a.re * b.im, a.im * b.re, a.im * b.im)
}

#[inline]
fn m_c(inp: Complex32) -> [Complex32; 4] {
    let (rr0, ri0, ir0, ii0) = mulc(inp, FACT[0]);
    let (rr1, ri1, ir1, ii1) = mulc(inp, FACT[1]);
    [
        Complex32::new(rr0 - ii0, ir0 + ri0),
        Complex32::new(rr1 - ii1, ir1 + ri1),
        Complex32::new(rr1 + ii1, ir1 - ri1),
        Complex32::new(rr0 + ii0, ir0 - ri0),
    ]
}

use std::mem;

fn fft5(inp: &[Complex32], stride: usize) -> [Complex32; 5] {
    let z = [
        m_c(inp[1 * stride]),
        m_c(inp[2 * stride]),
        m_c(inp[3 * stride]),
        m_c(inp[4 * stride]),
    ];

    [
        inp[0] + inp[1 * stride] + inp[2 * stride] + inp[3 * stride] + inp[4 * stride],
        inp[0] + z[0][0] + z[1][1] + z[2][2] + z[3][3],
        inp[0] + z[0][1] + z[1][3] + z[2][0] + z[3][2],
        inp[0] + z[0][2] + z[1][0] + z[2][3] + z[3][1],
        inp[0] + z[0][3] + z[1][2] + z[2][1] + z[3][0],
    ]
}

impl IMDCT15 {
    fn new(n: usize) -> Self {
        use std::f32::consts::PI;
        let len2 = p2len(n);
        let len = len2 * 2;
        let len4 = len2 / 2;

        let mut tmp = Vec::with_capacity(len * 2);
        let twiddle = (len4..len2)
            .map(|i| {
                let v = 2f32 * PI * (i as f32 + 0.125) / len as f32;
                Complex32::new(v.cos(), v.sin())
            })
            .collect();

        let mut exptab: Vec<Vec<Complex32>> = (0..6)
            .map(|i| {
                let len = p2len(i);
                (0..len.max(19))
                    .map(|j| {
                        let v = 2f32 * PI * j as f32 / len as f32;
                        Complex32::new(v.cos(), v.sin())
                    })
                    .collect()
            })
            .collect();

        for i in 0..4 {
            let v = exptab[0][i];
            exptab[0].push(v);
        }

        tmp.resize(len * 2, Complex32::default());

        IMDCT15 {
            n,
            len2,
            len4,
            tmp,
            exptab,
            twiddle,
        }
    }

    fn fft15(&self, out: &mut [Complex32], inp: &[Complex32], stride: usize) {
        let exptab = &self.exptab[0];

        let tmp0 = fft5(&inp[..], stride * 3);
        let tmp1 = fft5(&inp[1 * stride..], stride * 3);
        let tmp2 = fft5(&inp[2 * stride..], stride * 3);

        for ((i, t0), (t1, t2)) in tmp0.iter().enumerate().zip(tmp1.iter().zip(tmp2.iter())) {
            let e1 = t1 * exptab[i];
            let e2 = t2 * exptab[2 * i];
            out[i] = t0 + e1 + e2;

            let e1 = t1 * exptab[i + 5];
            let e2 = t2 * exptab[2 * (i + 5)];
            out[i] = t0 + e1 + e2;

            let e1 = t1 * exptab[i + 10];
            let e2 = t2 * exptab[2 * i + 5];
            out[i] = t0 + e1 + e2;
        }
    }

    fn fft_calc(&self, n: usize, out: &mut [Complex32], inp: &[Complex32], stride: usize) {
        if n > 0 {
            let exptab = &self.exptab[n];
            let len2 = p2len(n);

            self.fft_calc(n - 1, &mut out[..], &inp, stride * 2);
            self.fft_calc(n - 1, &mut out[len2..], &inp[stride..], stride * 2);

            for i in 0..len2 {
                let e = out[i + len2] * exptab[i];
                let o = out[i];

                out[i + len2] = o + e;
                out[i] += e;
            }
        } else {
            self.fft15(out, inp, stride);
        }
    }

    // Assume out is aligned at least by 64
    pub fn imdct15_half(&mut self, out: &mut [f32], inp: &[f32], stride: usize, scale: f32) {
        let mut dst: Vec<Complex32> = unsafe {
            Vec::from_raw_parts(
                mem::transmute(out.as_mut_ptr()),
                out.len() / 2,
                out.len() / 2,
            )
        };
        let len8 = self.len4 / 2;
        let start = (self.len2 - 1) * stride;

        for (i, t) in self.tmp.iter_mut().enumerate() {
            let re = inp[start - 2 * stride * i];
            let im = inp[2 * stride * i];
            *t = Complex32::new(re, im) * self.twiddle[i];
        }

        self.fft_calc(self.n, &mut dst, &self.tmp, 1);

        for i in 0..len8 {
            let decr = len8 - i - 1;
            let incr = len8 + i;
            let re0im1 = Complex32::new(dst[decr].im, dst[decr].re)
                * Complex32::new(self.twiddle[decr].im, self.twiddle[decr].im);
            let re1im0 = Complex32::new(dst[incr].im, dst[incr].re)
                * Complex32::new(self.twiddle[incr].im, self.twiddle[incr].im);

            dst[decr] = Complex32::new(re0im1.re, re1im0.im).scale(scale);
            dst[incr] = Complex32::new(re1im0.re, re0im1.im).scale(scale);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn alloc() {
        let imdct = IMDCT15::new(0);

        println!("{:#?}", imdct);
    }

    #[test]
    fn fft5() {
        let a: Vec<Complex32> = (0..15)
            .map(|v| {
                let v = v as f32;
                Complex32::new(v, -v)
            })
            .collect();

        println!("{:#?}", a);

        let out = super::fft5(&a, 3);

        println!("{:#?}", out);

        let reference = [
            Complex {
                re: 30.0,
                im: -30.0,
            },
            Complex {
                re: -17.822865,
                im: -2.8228645,
            },
            Complex {
                re: -9.936897,
                im: 5.063103,
            },
            Complex {
                re: -5.063103,
                im: 9.936897,
            },
            Complex {
                re: 2.8228645,
                im: 17.822865,
            },
        ];
        assert_eq!(&out[..], &reference[..]);
    }
}
