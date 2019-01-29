extern crate av_data as data;
extern crate av_codec as codec;

#[macro_use]
extern crate av_bitstream as bitstream;

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

#[cfg(test)]
extern crate matroska;

#[cfg(test)]
extern crate av_format as format;

#[cfg(test)]
extern crate interpolate_name;

extern crate num_complex as complex;

extern crate pretty_env_logger;
#[macro_use] extern crate log;

extern crate integer_sqrt;

mod entropy;
mod packet;
mod maths;

mod silk;
mod celt;

pub mod decoder;

