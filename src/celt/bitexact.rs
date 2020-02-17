/// Bit-exact functions

use crate::maths::*;

// TODO: make sure llvm does the right thing
#[inline(always)]
pub fn mul16(a: i16, b: i16) -> i32 {
    a as i32 * b as i32
}

#[inline(always)]
pub fn frac_mul16(a: i16, b: i16) -> i16 {
    let v = mul16(a, b);

    ((16384 + v as i32) >> 15) as i16
}

#[inline(always)]
pub fn cos(x: i16) -> i16 {
    let v = ((mul16(x, x) + 4096) >> 13) as i16;

    1 + (32767 - v) + frac_mul16(v, -7651 + frac_mul16(v, 8277 + frac_mul16(-626, v)))
}

#[inline(always)]
pub fn log2tan(isin: i32, icos: i32) -> i32 {
    let ls = isin.ilog();
    let lc = icos.ilog();

    let isin = (isin << (15 - ls)) as i16;
    let icos = (icos << (15 - lc)) as i16;

    let s = (ls << 11) + frac_mul16(isin, frac_mul16(isin, -2597) + 7932) as i32;
    let c = (lc << 11) + frac_mul16(icos, frac_mul16(icos, -2597) + 7932) as i32;

    s - c
}
