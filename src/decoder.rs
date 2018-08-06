use codec::decoder::*;
use codec::error::*;
use data::packet::Packet as AVPacket;
use data::frame::ArcFrame;

use packet::*;

use entropy::*;
use silk::Silk;

struct Des {
    descr: Descr,
}

struct Dec {
    extradata: Option<Vec<u8>>,
    silk: Option<Silk>,
}

impl Dec {
    fn new() -> Self {
        Dec { extradata: None, silk: None }
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
            let silk = self.silk.as_mut().unwrap();
            let pkt = Packet::from_slice(pkt.data.as_slice())?;

            println!("{:?}", pkt);

            // Configure the CELT and the SILK decoder with the
            // frame-invariant, per-packet information
            if pkt.mode != Mode::CELT {
                silk.setup(&pkt);
            }

/*            if pkt.mode != Mode::SILK {
                celt.setup(&pkt);
            }
*/
            if pkt.mode == Mode::HYBRID {
                unimplemented!();
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

                println!("size {} consumed {} redundancy {}", size, consumed, redundancy);

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

                    let size = size - redundancy_size;

                    println!("redundancy pos {} size {}", redundancy_pos, redundancy_size);

                    if redundancy_pos {
                        // decode_redundancy
                        // celt.flush()
                    }
                }

                if pkt.mode != Mode::SILK {

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
    use format::demuxer::Demuxer;
    use format::demuxer::Event;
    use format::buffer::*;
    use std::fs::File;
    use std::path::PathBuf;

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

    #[test]
    fn send_packet() {
        let p = env!("CARGO_MANIFEST_DIR");
        let mut d = PathBuf::from(p);
        d.push("assets/_");
        for i in /*1..12*/ 8..9 {
            let filename = format!("testvector{:02}.mka", i);
            d.set_file_name(filename);
            println!("path {:?}", d);
            parse_packet(&d);
        }
    }
}
