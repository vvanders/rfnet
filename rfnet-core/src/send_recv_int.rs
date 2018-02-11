use send_block::*;
use recv_block::*;
use kiss;
use packet;
use framed::{FramedWrite, KISSFramedWrite};

use std::io;

fn cycle_recv<'a,W,F>(recv: &'a mut RecvBlock<W>, send_channel: &mut F, recv_channel: &mut Vec<u8>) -> Option<RecvResult<'a>>
    where W: io::Write, F: FramedWrite {

    recv.tick(50, send_channel).unwrap();

    if recv_channel.len() > 0 {
        let mut data = vec!();
        let decoded = kiss::decode(recv_channel.iter().cloned(), &mut data);

        if let Some(decoded) = decoded {
            let packet = packet::decode(&mut data[..], true).unwrap();

            let res = Some(recv.on_packet(&packet, send_channel).unwrap());

            recv_channel.drain(0..decoded.bytes_read);

            res
        } else {
            None
        }
    } else {
        None
    }
}

fn cycle_send<'a,W,F>(send: &'a mut SendBlock<W>, send_channel: &mut Vec<u8>, recv_channel: &mut F) -> Option<SendResult<'a>>
    where W: io::Read, F: FramedWrite {

    send.tick(50, recv_channel).unwrap();

    if send_channel.len() > 0 {
        let mut data = vec!();
        let decoded = kiss::decode(send_channel.iter().cloned(), &mut data);

        if let Some(decoded) = decoded {
            let packet = packet::decode(&mut data[..], true).unwrap();

            let res = Some(send.on_packet(&packet.0, recv_channel).unwrap());

            send_channel.drain(0..decoded.bytes_read);

            res
        } else {
            None
        }
    } else {
        None
    }
}

#[test]
fn send_recv_full() {
    let (send,recv) = cycle_data(|idx, sidx| false, |idx, ridx| false);
}

#[test]
fn send_send_alt() {
    use simple_logger;
    simple_logger::init();

    let (send,recv) = cycle_data(|idx, sidx| false, |idx, ridx| false);
    let (send_alt,recv_alt) = cycle_data(|idx, sidx| sidx % 2 == 1, |idx, ridx| false);

    assert_eq!(send.packets_sent*2, send_alt.packets_sent);
    assert_eq!(recv.acks_sent*2, recv_alt.acks_sent);
    assert_eq!(recv_alt.packets_received, send_alt.packets_sent/2);
}

fn cycle_data<S,R>(drop_send_fn: S, drop_recv_fn: R) -> (SendStats, RecvStats)
        where S: Fn(usize, usize) -> bool, R: Fn(usize, usize) -> bool {
    let payload = (0..4096).map(|v| v as u8).collect::<Vec<u8>>();
    let mut received = vec!();

    let session_id = 1000;
    let link_width = 64;
    let fec = Some(0);
    let retry = RetryConfig {
        delay_ms: 0,
        bps: 1200,
        bps_scale: 1.0,
        retry_attempts: 5
    };

    let stats = {
        let mut send_chan = vec!();
        let mut recv_chan = vec!();

        let mut send = SendBlock::new(io::Cursor::new(&payload[..]), payload.len(), session_id, link_width, fec, retry);
        let mut recv = RecvBlock::new(session_id, fec.is_some(), &mut received);

        send.send(&mut KISSFramedWrite::new(&mut recv_chan, 0)).unwrap();

        let mut send_idx = 0;
        let mut recv_idx = 0;

        let mut send_complete = false;
        let mut recv_complete = false;
        for i in 0..4096 {
            match cycle_send(&mut send, &mut send_chan, &mut KISSFramedWrite::new(&mut recv_chan, 0)) {
                Some(SendResult::CompleteNoResponse) => send_complete = true,
                _ => {}
            }

            if recv_chan.len() > 0 {
                send_idx += 1;

                if drop_send_fn(i, send_idx) {
                    recv_chan.clear();
                }
            }

            match cycle_recv(&mut recv, &mut KISSFramedWrite::new(&mut send_chan, 0), &mut recv_chan) {
                Some(RecvResult::Complete) => recv_complete = true,
                Some(RecvResult::CompleteSendResponse) => {
                    recv.send_response(false, &mut KISSFramedWrite::new(&mut send_chan, 0)).unwrap();
                },
                Some(RecvResult::Status(_)) => recv_idx += 1,
                _ => {}
            }

            if send_chan.len() > 0 {
                recv_idx += 1;

                if drop_recv_fn(i, recv_idx) {
                    send_chan.clear();
                }
            }

            if send_complete && recv_complete {
                trace!("Send/Recv complete, returning");
                break
            }
        }

        assert!(send_complete);
        assert!(recv_complete);

        (send.get_stats().clone(), recv.get_stats().clone())
    };

    assert_eq!(payload, received);

    stats
}