use framed::*;
use node::{Node, ClientEvent, ClientState};
use link::{Link, HttpProvider};
use send_block::RetryConfig;
use message::{RESTMethod, ResponseMessage, ResponseType};
use request_response::RequestResponse;
use kiss;

use hyper;

use std::io;

fn cycle_node<F,R,W>(node: &mut Node, send_channel: &mut F, recv_channel: &mut F, request_reader: &mut R, request_size: usize, response_writer: &mut W) -> bool
        where F: FramedWrite + FramedRead<Vec<u8>>, R: io::Read, W: io::Write {
    let mut resp_complete = false;
    let mut connect = false;
    let mut start_req = false;
    let mut recv_frame = vec!();
    {
        let mut event_handler = |e| {
            match e {
                ClientEvent::ConnectionFailed => assert!(false),
                ClientEvent::ResponseComplete => resp_complete = true,
                ClientEvent::StateChange(ClientState::Listening, ClientState::Idle) => connect = true,
                ClientEvent::Connected => start_req = true,
                _ => {}
            }
        };

        if let Ok(Some(frame)) = recv_channel.read_frame(&mut recv_frame) {
            node.on_data(frame, send_channel, response_writer, request_reader, &mut event_handler).unwrap();
        }
        node.tick(100, send_channel, &mut event_handler).unwrap();
    }

    if connect {
        node.connect(send_channel, |_| {}).unwrap();
    }

    if start_req {
        node.start_request(request_reader, request_size, send_channel, |_| {}).unwrap();
    }

    if resp_complete {
        node.disconnect(send_channel, |_| {}).unwrap();
    }

    resp_complete
}

fn cycle_link<F>(link: &mut Link, send_channel: &mut F, recv_channel: &mut F) where F: FramedWrite + FramedRead<Vec<u8>> {
    struct MockHttp {
    }

    impl HttpProvider for MockHttp {
        fn request(&mut self, _request: hyper::Request) -> Result<hyper::Response, hyper::Error> {
            let response = hyper::Response::new()
                .with_status(hyper::StatusCode::Ok)
                .with_body("Response");

            Ok(response)
        }
    }

    let mut http = MockHttp {};

    let mut recv_frame = vec!();
    if let Ok(Some(frame)) = send_channel.read_frame(&mut recv_frame) {
        link.recv_data(frame, recv_channel, &mut http).unwrap();
    }

    link.elapsed(100, recv_channel).unwrap();
}

fn cycle_data<S,R>(mut drop_send_fn: S, mut drop_recv_fn: R)
        where S: FnMut(usize, usize, &mut Vec<u8>), R: FnMut(usize, usize, &mut Vec<u8>) {
    let mut node = Node::new("KI7EST".to_string(), None, RetryConfig::default(1200));

    let link_config = ::link::LinkConfig {
        link_width: 32,
        fec: true,
        retry_enabled: true,
        retry: RetryConfig::default(1200),
        broadcast_rate: Some(100)
    };
    let mut link = Link::new("KI7EST", link_config);

    let mut rr = RequestResponse::new();
    rr.new_request(
        (1,0),
        "KI7EST@rfnet.net",
        0,
        RESTMethod::GET,
        "http://www.rfnet.net/test", 
        "", 
        "BODY",
        &[0;64]).unwrap();

    let mut send_idx = 0;
    let mut recv_idx = 0;

    let mut response_complete = false;
    {
        let mut send_chan = KISSFramed::new(LoopbackIo::new(), 0);
        let mut recv_chan = KISSFramed::new(LoopbackIo::new(), 0);


        for i in 0..4096 {
            let request_size = rr.request.get_data().len();
            response_complete |= cycle_node(&mut node, &mut send_chan, &mut recv_chan, &mut rr.request, request_size, &mut rr.response);

            if recv_chan.get_tnc_mut().buffer_mut().len() > 0 {
                send_idx += 1;

                drop_send_fn(i, send_idx, recv_chan.get_tnc_mut().buffer_mut());
            }

            cycle_link(&mut link, &mut send_chan, &mut recv_chan);

            if send_chan.get_tnc_mut().buffer_mut().len() > 0 {
                recv_idx += 1;

                drop_recv_fn(i, recv_idx, send_chan.get_tnc_mut().buffer_mut());
            }

            if response_complete {
                trace!("Send/Recv complete, returning");
                break
            }
        }
    }

    let response = ResponseMessage {
        resp_type: ResponseType::REST {
            code: 200,
            body: "Response"
        }
    };

    assert!(response_complete);
    assert_eq!(rr.response.decode().unwrap(), response);
}

#[test]
fn send_recv_full() {
    cycle_data(|_idx, _sidx, _data| {}, |_idx, _ridx, _data| {});
}

#[test]
fn send_send_alt() {
    cycle_data(
        |_idx, sidx, data|
            if sidx % 2 == 1 {
                data.clear();
            },
        |_idx, _ridx, _data| {});
}

#[test]
fn send_recv_alt() {
    cycle_data(
        |_idx, _sidx, _data| {},
        |_idx, ridx, data| 
            if ridx % 2 == 1 {
                data.clear();
            });
}

#[test]
fn send_flip() {
    cycle_data(|_idx, sidx, data| {
        let mut decode_data = vec!();
        kiss::decode(data.iter().cloned(), &mut decode_data).unwrap();

        let idx = sidx % decode_data.len();
        decode_data[idx] = !decode_data[idx];

        data.clear();
        kiss::encode(io::Cursor::new(&decode_data[..]), data, 0).unwrap();
    }, |_idx, _ridx, _data| {});
}

#[test]
fn recv_flip() {
    cycle_data(
        |_idx, _sidx, _data| {},
        |_idx, ridx, data| {
            let mut decode_data = vec!();
            kiss::decode(data.iter().cloned(), &mut decode_data).unwrap();

            let idx = ridx % decode_data.len();
            decode_data[idx] = !decode_data[idx];

            data.clear();
            kiss::encode(io::Cursor::new(&decode_data[..]), data, 0).unwrap();
        });
}