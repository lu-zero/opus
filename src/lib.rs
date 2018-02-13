extern crate av_data as data;
extern crate av_codec as codec;

#[macro_use]
extern crate av_bitstream as bitstream;

#[cfg(test)]
extern crate matroska;

#[cfg(test)]
extern crate av_format as format;

mod entropy;
mod packet;
mod silk;
mod maths;
pub mod decoder;

