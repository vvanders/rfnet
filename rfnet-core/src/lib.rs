#[macro_use]
extern crate log;
extern crate reed_solomon;
extern crate byteorder;
extern crate hyper;
extern crate futures;
extern crate rand;

#[cfg(test)]
extern crate simple_logger;

mod kiss;
mod framed;
mod packet;
mod link;
mod node;
mod acked_packet;
mod send_block;
mod recv_block;
mod message;

#[cfg(test)]
mod send_recv_int;

const MAJOR_VER: u8 = 0;
const MINOR_VER: u8 = 1;