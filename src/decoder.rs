use crate::codec::decoder::*;
use crate::codec::error::*;
use crate::data::packet::Packet as AVPacket;
use crate::data::frame::ArcFrame;

use crate::packet::*;

use crate::entropy::*;
use crate::silk::Silk;
use crate::celt::Celt;

struct Des {
    descr: Descr,
}

struct Dec {
    extradata: Option<Vec<u8>>,
    silk: Option<Silk>,
    celt: Option<Celt>,
}

impl Dec {
    fn new() -> Self {
        Dec { extradata: None, silk: None, celt: None }
    }
}

impl Descriptor for Des {
    fn create(&self) -> Box<dyn Decoder> {
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
            let silk = self.silk.as_mut().unwrap();
            let celt = self.celt.as_mut().unwrap();
            let pkt = Packet::from_slice(pkt.data.as_slice())?;

            println!("{:?}", pkt);

            // Configure the CELT and the SILK decoder with the
            // frame-invariant, per-packet information
            if pkt.mode != Mode::CELT {
                silk.setup(&pkt);
            }

            if pkt.mode == Mode::CELT {
                celt.setup(&pkt);
            }

            if pkt.mode == Mode::HYBRID {
//                unimplemented!();
            }

            // Decode the frames
            //
            // If a silk or a hybrid frame is preset, decode the silk part first
            for frame in pkt.frames {
                let mut rd = RangeDecoder::new(frame);
                // println!("Decoding {:?}", frame);

                if pkt.mode != Mode::CELT {
                    silk.decode(&mut rd)?;
                } else {
                    silk.flush();
                }

                let size = frame.len();
                let consumed = rd.tell();
                let redundancy = if pkt.mode == Mode::HYBRID && consumed + 37 <= size * 8 {
                    rd.decode_logp(12)
                } else if pkt.mode == Mode::SILK && consumed + 17 <= size * 8 {
                    true
                } else {
                    false
                };

                println!("consumed {} redundancy {}", consumed, redundancy);

                if redundancy {
                    let redundancy_pos = rd.decode_logp(1);

                    let redundancy_size = if pkt.mode == Mode::HYBRID {
                        rd.decode_uniform(256) + 2
                    } else {
                        size - (consumed + 7) / 8
                    };

                    if redundancy_size >= size {
                        return Err(Error::InvalidData);
                    }

                    let _size = size - redundancy_size;

                    println!("redundancy pos {} size {}", redundancy_pos, redundancy_size);

                    if redundancy_pos {
                        // decode_redundancy
                        // celt.flush()
                    }
                }

                if pkt.mode != Mode::SILK {
                    let mut out_buf = [0f32; 1024]; // TODO
                    let range = if pkt.mode == Mode::HYBRID {
                        17
                    } else {
                        0
                    } .. pkt.bandwidth.celt_band();

                    celt.decode(&mut rd, &mut out_buf, pkt.frame_duration, range)

                }
            }


            Ok(())
        }
        fn receive_frame(&mut self) -> Result<ArcFrame> {
            // self.pending.pop_front().ok_or(ErrorKind::MoreDataNeeded.into())
            //
            unimplemented!()
        }
        fn configure(&mut self) -> Result<()> {
            use crate::bitstream::byteread::get_i16l;

            let channels;
            let _sample_rate = 48000;
            let mut gain_db = 0;
            let mut streams = 1;
            let mut coupled_streams = 0;
            let mut mapping : &[u8] = &[0u8, 1u8];
            let mut channel_map = false;

            if let Some(ref extradata) = self.extradata {
                channels = *extradata.get(9).unwrap_or(&2) as usize;

                if extradata.len() >= OPUS_HEAD_SIZE {
                    gain_db = get_i16l(&extradata[16..=17]);
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
                        return Err(Error::ConfigurationInvalid);
                    }
                    if channels > 1 {
                        coupled_streams = 1;
                    }
                }
            } else {
                return Err(Error::ConfigurationIncomplete);
            }

            if channels > 2 {
                unimplemented!() // TODO: Support properly channel mapping
            } else {
                // println!("channels {}", channels);
                self.silk = Some(Silk::new(channels > 1));
                self.celt = Some(Celt::new(channels > 1));
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

pub const OPUS_DESCR: &dyn Descriptor = &Des {
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
    use crate::format::demuxer::Context;
    use crate::format::demuxer::Event;
    use crate::format::buffer::*;
    use std::fs::File;
    use std::path::PathBuf;

    use interpolate_name::interpolate_test;

    fn parse_packet(sample: &PathBuf) {
        let mut ctx = Context::new(Box::new(MkvDemuxer::new()),
                                   Box::new(AccReader::new(File::open(sample).unwrap())));
        let _ = ctx.read_headers().unwrap();

        let mut d = Dec::new();

        d.set_extradata(ctx.info.streams[0].get_extradata().unwrap());
        let _ = d.configure();

        for _ in 0..10 {
            if let Ok(ev) = ctx.read_event() {
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

    #[interpolate_test(n01, 1)]
    #[interpolate_test(n02, 2)]
    #[interpolate_test(n03, 3)]
    #[interpolate_test(n04, 4)]
    #[interpolate_test(n05, 5)]
    #[interpolate_test(n06, 6)]
    #[interpolate_test(n07, 7)]
    #[interpolate_test(n08, 8)]
    #[interpolate_test(n09, 9)]
    #[interpolate_test(n10, 10)]
    #[interpolate_test(n11, 11)]
    #[interpolate_test(n12, 12)]
    fn send_packet(index: usize) {
        let p = env!("CARGO_MANIFEST_DIR");
        let mut d = PathBuf::from(p);
        d.push("assets");
        d.push(format!("testvector{:02}.mka", index));
        println!("path {:?}", d);
        parse_packet(&d);
    }
}
