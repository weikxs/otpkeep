//! The bridge on real Foundation streams: two bound stream pairs stand in for the two directions of an L2CAP
//! channel, so the run loop, delegate events, reads, writes and closing are exercised on this machine.

use std::time::Duration;

use keep_sync::transport::BoxLink;
use objc2::rc::Retained;
use objc2_foundation::{NSInputStream, NSOutputStream, NSStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{OnQueue, Streams, bridge_streams};

/// A small buffer, so writes fill it and must wait for "space available".
const BOUND_BUFFER: usize = 1024;

fn bound_pair() -> (Retained<NSInputStream>, Retained<NSOutputStream>) {
    let (mut input, mut output) = (None, None);
    NSStream::getBoundStreamsWithBufferSize_inputStream_outputStream(
        BOUND_BUFFER,
        Some(&mut input),
        Some(&mut output),
    );
    (input.unwrap(), output.unwrap())
}

/// Two links joined like the two ends of one channel.
fn channel() -> (BoxLink, BoxLink) {
    let (a_to_b_in, a_to_b_out) = bound_pair();
    let (b_to_a_in, b_to_a_out) = bound_pair();
    let a = bridge_streams(Streams {
        input: OnQueue(b_to_a_in),
        output: OnQueue(a_to_b_out),
        _channel: None,
    });
    let b = bridge_streams(Streams {
        input: OnQueue(a_to_b_in),
        output: OnQueue(b_to_a_out),
        _channel: None,
    });
    (a, b)
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|n| {
            u8::try_from(n % 251)
                .unwrap()
                .wrapping_mul(31)
                .wrapping_add(seed)
        })
        .collect()
}

/// Writes `send` and reads `expect_len` bytes at the same time.
async fn exchange(link: BoxLink, send: Vec<u8>, expect_len: usize) -> Vec<u8> {
    let (mut reader, mut writer) = tokio::io::split(link);
    let write = async move { writer.write_all(&send).await.unwrap() };
    let read = async move {
        let mut got = vec![0u8; expect_len];
        reader.read_exact(&mut got).await.unwrap();
        got
    };
    tokio::join!(write, read).1
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bytes_cross_in_both_directions_at_once_intact() {
    let (a, b) = channel();
    let (to_b, to_a) = (pattern(300_000, 1), pattern(200_000, 2));

    let both = async {
        tokio::join!(
            exchange(a, to_b.clone(), to_a.len()),
            exchange(b, to_a.clone(), to_b.len())
        )
    };
    let (at_a, at_b) = tokio::time::timeout(Duration::from_secs(30), both)
        .await
        .unwrap();

    assert_eq!(at_b, to_b);
    assert_eq!(at_a, to_a);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_one_end_reaches_the_other_after_the_last_bytes() {
    let (mut a, mut b) = channel();

    a.write_all(b"last words").await.unwrap();
    drop(a);
    let mut got = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), b.read_to_end(&mut got))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(got, b"last words");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_whole_noise_session_runs_over_the_bridge() {
    use keep_sync::channel::PROTOCOL_VERSION;
    use keep_sync::channel::hello::Hello;
    use keep_sync::channel::noise::{Purpose, accept_noise, connect_noise};
    use keep_sync::identity::DeviceIdentity;

    let group = [7u8; 16];
    let (opener, joiner) = (
        DeviceIdentity::from_seeds(&[1; 32], &[2; 32], &group),
        DeviceIdentity::from_seeds(&[3; 32], &[4; 32], &group),
    );
    let hello = |d: &DeviceIdentity| Hello {
        device: d.device_id(),
        dx_cert: Some(d.dx_cert()),
        platform: keep_vault::Platform::MacOs,
        lan_name: "Test".to_owned(),
        versions: vec![PROTOCOL_VERSION],
    };
    let (opener_hello, joiner_hello) = (hello(&opener), hello(&joiner));
    let (a, b) = channel();

    let (open, join) = tokio::join!(
        accept_noise(
            a,
            Purpose::Sync,
            &[9; 16],
            opener.exchange_secret(),
            &opener_hello
        ),
        connect_noise(
            b,
            Purpose::Sync,
            &[9; 16],
            joiner.exchange_secret(),
            &joiner_hello
        )
    );
    let (mut open, mut join) = (open.unwrap().0, join.unwrap().0);
    let big = pattern(150_000, 9);
    let (sent, received) = tokio::join!(join.send(&big), open.recv());

    sent.unwrap();
    assert_eq!(received.unwrap().as_slice(), big.as_slice());
}
