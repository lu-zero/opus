use entropy::RangeDecoder;

struct Frame<'a> {
    r: RangeDecoder<'a>,
    buf: &'a [u8],
}

impl<'a> Frame<'a> {
    fn from_slice(buf: &'a [u8]) -> Self {
        let r = RangeDecoder::new(buf);
        Frame {
            r: r,
            buf: buf,
        }
    }
}
