#[macro_use]
extern crate log;
extern crate reed_solomon;
extern crate byteorder;
extern crate hyper;
extern crate futures;
extern crate rand;
extern crate rust_sodium;
extern crate base64;

#[cfg(test)]
extern crate simple_logger;

mod kiss;
pub mod framed;
mod packet;
pub mod link;
pub mod node;
mod send_block;
mod recv_block;
pub mod message;
pub mod request_response;

#[cfg(test)]
mod send_recv_int;
#[cfg(test)]
mod node_link_int;

const MAJOR_VER: u8 = 0;
const MINOR_VER: u8 = 1;

pub use link::{ Link, LinkConfig, HttpProvider };
pub use node::{ Node, RemoteLinkConfig };
pub use send_block::RetryConfig;