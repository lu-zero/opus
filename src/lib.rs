extern crate av_codec as codec;
extern crate av_data as data;

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

#[macro_use]
extern crate log;

extern crate integer_sqrt;

mod entropy;
mod maths;
mod packet;

mod celt;
mod silk;

pub mod decoder;
