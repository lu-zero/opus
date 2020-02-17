//! Opus Packet parsing

use crate::codec::error::*;

#[derive(Debug, PartialEq, Clone)]
pub enum Code {
    Single,
    DoubleEqual,
    DoubleVary,
    Multiple,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Mode {
    SILK,
    CELT,
    HYBRID,
}

/// Bandwidth
///
/// See [section-2.1.3](https://tools.ietf.org/html/rfc6716#section-2.1.3)
#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, Clone, Copy)]
pub enum Bandwidth {
    Narrow = 8000,
    Medium = 12000,
    Wide = 16000,
    SuperWide = 24000,
    Full = 48000,
}

impl Bandwidth {
    pub fn celt_band(&self) -> usize {
        use self::Bandwidth::*;
        match self {
            Narrow => 13,
            Medium => 17,
            Wide => 17,
            SuperWide => 19,
            Full => 21,
        }
    }
}

/// Frame Duration
///
/// See [section-2.1.4](https://tools.ietf.org/html/rfc6716#section-2.1.4)
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FrameDuration {
    /// 2.5ms
    VeryShort = 120,
    /// 5ms
    Short = 240,
    /// 10ms
    Medium = 480,
    /// 20ms
    Standard = 960,
    /// 40ms,
    Long = 1920,
    /// 60ms
    VeryLong = 2880,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Packet<'a> {
    code: Code,
    vbr: bool,
    config: usize,
    pub stereo: bool,
    pub padding: usize,
    pub mode: Mode,
    pub bandwidth: Bandwidth,
    pub frame_duration: FrameDuration,
    pub frames: Vec<&'a [u8]>,
}

fn xiph_lacing_u16(buf: &[u8]) -> Result<(usize, usize)> {
    let mut v = buf[0] as usize;
    if v >= 252 {
        if buf.len() > 1 {
            v += 4 * buf[1] as usize;
            Ok((2, v))
        } else {
            Err(Error::InvalidData)
        }
    } else {
        Ok((1, v as usize))
    }
}

fn xiph_lacing_u32(buf: &[u8]) -> Result<(usize, usize)> {
    use std::u32;
    let mut v = 0;
    let mut o = 0;

    for b in buf {
        let b = *b as u32;
        v += b;
        o += 1;
        if b < 255 {
            break;
        } else {
            v -= 1;
        }

        if v > u32::MAX - 255 {
            return Err(Error::InvalidData);
        }
    }
    Ok((o, v as usize))
}

const MAX_FRAME_SIZE: usize = 1275;
const MAX_FRAMES: usize = 48;
const MAX_PACKET_DUR: usize = 5760;

impl<'a> Packet<'a> {
    fn single_packet(&mut self, buf: &'a [u8]) -> Result<()> {
        self.code = Code::Single;
        self.vbr = false;
        self.frames.push(buf);
        Ok(())
    }

    fn double_packet_es(&mut self, buf: &'a [u8]) -> Result<()> {
        self.code = Code::DoubleEqual;
        self.vbr = false;

        if buf.len() & 1 != 0 {
            return Err(Error::InvalidData);
        }

        for b in buf.chunks(buf.len()) {
            self.frames.push(b);
        }
        Ok(())
    }

    fn double_packet_va(&mut self, buf: &'a [u8]) -> Result<()> {
        self.code = Code::DoubleVary;
        self.vbr = true;

        let (off, len) = xiph_lacing_u16(buf)?;
        if len + off > buf.len() {
            return Err(Error::InvalidData);
        }

        let (b1, b2) = buf[off..].split_at(len);

        self.frames.push(b1);
        self.frames.push(b2);
        Ok(())
    }

    fn multiple_packet(&mut self, buf: &'a [u8]) -> Result<()> {
        self.code = Code::Multiple;
        self.vbr = (buf[0] >> 7) & 0x01 == 1;

        let count = (buf[0] & 0x3f) as usize;
        let padding = (buf[0] >> 6) & 0x01 == 1;

        if count == 0 || count > MAX_FRAMES {
            return Err(Error::InvalidData);
        }

        let buf = if padding {
            let (off, pad) = xiph_lacing_u32(&buf[1..])?;
            self.padding = pad;
            &buf[1 + off .. buf.len() - pad]
        } else {
            &buf[1 ..]
        };

        if self.vbr {
            let mut b = buf;
            println!("count {} padding {}", count, self.padding);
            let mut lens = Vec::with_capacity(count - 1);
            for i in 0..count - 1 {
                let (off, len) = xiph_lacing_u16(b)?;
                println!("packet {} {}", i, len);
                b = &b[off..];
                lens.push(len);
            }
            for len in lens.iter() {
                let (b1, rem) = b.split_at(*len);
                self.frames.push(b1);
                b = rem;
            }
        } else {
            let len = buf.len() / count;
            if len * count != buf.len() || len > MAX_FRAME_SIZE {
                return Err(Error::InvalidData);
            }

            for b in buf.chunks(len) {
                self.frames.push(b);
            }
        }
        Ok(())
    }

    pub fn from_slice(buf: &'a [u8]) -> Result<Self> {
        let mut p = Packet {
            code: Code::Single,
            stereo: false,
            vbr: false,
            config: 0,
            padding: 0,
            frame_duration: FrameDuration::Standard,
            mode: Mode::HYBRID,
            bandwidth: Bandwidth::Wide,
            frames: Vec::new(),
        };

        if buf.len() < 1 {
            unimplemented!();
        }

        let code = buf[0] & 0x3;
        let config = (buf[0] >> 3) & 0x1f;
        p.stereo = (buf[0] >> 2) & 0x01 == 1;

        if code >= 2 && buf.len() < 1 {
            unimplemented!();
        }

        let buf = &buf[1..];

        println!("code {} config {}", code, config);

        // TODO support self delimited
        match code {
            0 => {
                p.single_packet(&buf)?;
            },
            1 => {
                p.double_packet_es(&buf)?;
            },
            2 => {
                p.double_packet_va(&buf)?;
            },
            3 => {
                p.multiple_packet(&buf)?;
            }
            _ => unimplemented!()
        }

        match config {
            c @ 0 ..= 11 => {
                p.mode = Mode::SILK;
                match c {
                    0 ..= 3 => {
                        p.bandwidth = Bandwidth::Narrow;
                    },
                    4 ..= 7 => {
                        p.bandwidth = Bandwidth::Medium;
                    },
                    8 ..= 11 => {
                        p.bandwidth = Bandwidth::Wide;
                    },
                    _ => unreachable!(),
                }
                match c & 0b11 {
                    0 => p.frame_duration = FrameDuration::Medium,
                    1 => p.frame_duration = FrameDuration::Standard,
                    2 => p.frame_duration = FrameDuration::Long,
                    3 => p.frame_duration = FrameDuration::VeryLong,
                    _ => unreachable!(),
                }
            },
            c @ 12 ..= 15 => {
                p.mode = Mode::HYBRID;
                match c {
                    12 ..= 13 => {
                        p.bandwidth = Bandwidth::SuperWide;
                    },
                    14 ..= 15 => {
                        p.bandwidth = Bandwidth::Full;
                    },
                    _ => unreachable!(),
                }
                match c & 0b1 {
                    0 => p.frame_duration = FrameDuration::Medium,
                    1 => p.frame_duration = FrameDuration::Standard,
                    _ => unreachable!()
                }
            },
            c @ 16 ..= 31 => {
                p.mode = Mode::CELT;
                match c {
                    16 ..= 19 => {
                        p.bandwidth = Bandwidth::Narrow;
                    },
                    20 ..= 23 => {
                        p.bandwidth = Bandwidth::Wide;
                    },
                    24 ..= 27 => {
                        p.bandwidth = Bandwidth::SuperWide;
                    }
                    28 ..= 31 => {
                        p.bandwidth = Bandwidth::Full;
                    },
                    _ => unreachable!(),
                }
                match c & 0b11 {
                    0 => p.frame_duration = FrameDuration::VeryShort,
                    1 => p.frame_duration = FrameDuration::Short,
                    2 => p.frame_duration = FrameDuration::Medium,
                    3 => p.frame_duration = FrameDuration::Standard,
                    _ => unreachable!(),
                }
            },
            _ => unreachable!(),
        }

        Ok(p)
    }
}
