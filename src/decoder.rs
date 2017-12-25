use codec::decoder::*;
use codec::error::*;
use data::packet::Packet as AVPacket;
use data::frame::ArcFrame;

use packet::*;

struct Des {
    descr: Descr,
}

struct Dec {
    extradata: Option<Vec<u8>>,
    /*
    sample_rate: usize,
    channels: usize,
    streams: usize,
    coupled_streams: usize,
    mapping: Vec<u8>,
    gain: usize,
    */
}

impl Dec {
    fn new() -> Self {
        Dec { extradata: None }
    }
}

impl Descriptor for Des {
    fn create(&self) -> Box<Decoder> {
        Box::new(Dec::new())
    }

    fn describe<'a>(&'a self) -> &'a Descr {
        &self.descr
    }
}

const OPUS_HEAD_SIZE: usize = 19;

impl Decoder for Dec {
        fn set_extradata(&mut self, extra: &[u8]) {
            self.extradata = Some(Vec::from(extra));
        }
        fn send_packet(&mut self, pkt: &AVPacket) -> Result<()> {
            let pkt = Packet::from_slice(pkt.data.as_slice())?;

            println!("{:?}", pkt);

            Ok(())
        }
        fn receive_frame(&mut self) -> Result<ArcFrame> {
            // self.pending.pop_front().ok_or(ErrorKind::MoreDataNeeded.into())
            //
            unimplemented!()
        }
        fn configure(&mut self) -> Result<()> {
            use bitstream::byteread::get_i16l;

            let channels;
            let sample_rate = 48000;
            let mut gain_db = 0;
            let mut streams = 1;
            let mut coupled_streams = 0;
            let mut mapping : &[u8] = &[0u8, 1u8];
            let mut channel_map = false;

            if let Some(ref extradata) = self.extradata {
                channels = *extradata.get(9).unwrap_or(&2) as usize;

                if extradata.len() >= OPUS_HEAD_SIZE {
                    gain_db = get_i16l(&extradata[16..17]);
                    channel_map = extradata[18] != 0;
                }
                if extradata.len() >= OPUS_HEAD_SIZE + 2 + channels {
                    streams = extradata[OPUS_HEAD_SIZE] as usize;
                    coupled_streams = extradata[OPUS_HEAD_SIZE + 1] as usize;
                    if streams + coupled_streams != channels {
                        unimplemented!()
                    }
                    mapping = &extradata[OPUS_HEAD_SIZE + 2 ..]
                } else {
                    if channels > 2 || channel_map {
                        return Err(ErrorKind::InvalidConfiguration.into());
                    }
                    if channels > 1 {
                        coupled_streams = 1;
                    }
                }
            } else {
                return Err(ErrorKind::ConfigurationIncomplete.into());
            }

            if channels > 2 {
                unimplemented!() // TODO: Support properly channel mapping
            } else {
                // self.info.map = ChannelMap::default_map(channels);
            }

//            sample_rate, channels, streams, coupled_streams, mapping

            Ok(())
        }

        fn flush(&mut self) -> Result<()> {
            // self.dec.as_mut().unwrap().reset();
            unimplemented!()
        }
    }

pub const OPUS_DESCR: &Descriptor = &Des {
    descr: Descr {
        codec: "opus",
        name: "opus",
        desc: "pure-rust opus decoder",
        mime: "audio/OPUS",
    },
};

#[cfg(test)]
mod test {
    use super::*;
    use matroska::demuxer::*;
    use format::demuxer::Context;
    use format::demuxer::Event;
    use format::buffer::*;
    use std::io::Cursor;

    static TV01 : &[u8] = include_bytes!("../assets/testvector01.mka");

    #[test]
    fn parse_packet() {
        let mut ctx = Context::new(Box::new(MkvDemuxer::new()),
                                   Box::new(AccReader::new(Cursor::new(TV01))));
        let _ = ctx.read_headers().unwrap();

        let mut d = Dec::new();

        d.set_extradata(ctx.info.streams[0].get_extradata().unwrap());
        d.configure();


        while let Ok(ev) = ctx.read_event() {
            match ev {
                Event::NewPacket(p) => {
                    println!("{:?}", p);
                    d.send_packet(&p).unwrap();
                },
                _ => unreachable!(),
            }
        }
    }
}
