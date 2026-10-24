use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use super::{HEADER, MIN_PAYLOAD, Packet, PacketSink, Sending, gatt_link};

/// A radio that carries packets to the other side through a small queue, so senders must wait.
struct Radio {
    to_peer: mpsc::Sender<Vec<u8>>,
    /// Drop the packet with this index (to test loss detection).
    drop_at: Option<usize>,
    sent: usize,
    payload_max: usize,
}

impl PacketSink for Radio {
    fn send(&mut self, packet: Vec<u8>) -> Sending<'_> {
        Box::pin(async move {
            assert!(
                packet.len() <= self.payload_max + HEADER,
                "a packet larger than the MTU"
            );
            let index = self.sent;
            self.sent += 1;
            if self.drop_at == Some(index) {
                return Ok(());
            }
            self.to_peer
                .send(packet)
                .await
                .map_err(|_| std::io::Error::other("radio closed"))
        })
    }
}

fn pair(
    payload_max: usize,
    drop_at: Option<usize>,
) -> (keep_sync::transport::BoxLink, keep_sync::transport::BoxLink) {
    let (a_out, b_in) = mpsc::channel(4);
    let (b_out, a_in) = mpsc::channel(4);
    let a = gatt_link(
        Radio {
            to_peer: a_out,
            drop_at,
            sent: 0,
            payload_max,
        },
        a_in,
        payload_max,
    );
    let b = gatt_link(
        Radio {
            to_peer: b_out,
            drop_at: None,
            sent: 0,
            payload_max,
        },
        b_in,
        payload_max,
    );
    (a, b)
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|n| u8::try_from(n % 251).unwrap().wrapping_add(seed))
        .collect()
}

#[test]
fn packets_survive_encoding_and_malformed_ones_are_refused() {
    let data = Packet::Data {
        seq: 255,
        bytes: vec![1, 2, 3],
    };

    assert_eq!(Packet::decode(&data.encode()), Some(data));
    assert_eq!(
        Packet::decode(&Packet::Close { seq: 7 }.encode()),
        Some(Packet::Close { seq: 7 })
    );
    assert_eq!(Packet::decode(&[1, 0]), None, "data without bytes");
    assert_eq!(Packet::decode(&[9, 0, 1]), None);
    assert_eq!(Packet::decode(&[]), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bytes_cross_both_ways_in_small_packets_with_back_pressure() {
    let (a, b) = pair(MIN_PAYLOAD, None);
    let (to_b, to_a) = (pattern(100_000, 1), pattern(80_000, 2));
    let exchange = |link: keep_sync::transport::BoxLink, send: Vec<u8>, expect: usize| async move {
        let (mut reader, mut writer) = tokio::io::split(link);
        let write = async move { writer.write_all(&send).await.unwrap() };
        let read = async move {
            let mut got = vec![0u8; expect];
            reader.read_exact(&mut got).await.unwrap();
            got
        };
        tokio::join!(write, read).1
    };

    let (at_a, at_b) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(
            exchange(a, to_b.clone(), to_a.len()),
            exchange(b, to_a.clone(), to_b.len())
        )
    })
    .await
    .unwrap();

    assert_eq!(at_b, to_b);
    assert_eq!(at_a, to_a);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_reaches_the_other_side_after_the_last_bytes() {
    let (mut a, mut b) = pair(180, None);

    a.write_all(b"goodbye").await.unwrap();
    drop(a);
    let mut got = Vec::new();
    b.read_to_end(&mut got).await.unwrap();

    assert_eq!(got, b"goodbye");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_packet_ends_the_stream_instead_of_damaging_it() {
    let (mut a, mut b) = pair(MIN_PAYLOAD, Some(3));
    let sent = pattern(2_000, 5);

    a.write_all(&sent).await.unwrap();
    let mut got = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(10), b.read_to_end(&mut got))
        .await
        .unwrap();

    assert!(got.len() < sent.len());
    assert_eq!(
        got,
        sent[..got.len()],
        "what arrived is an unchanged prefix"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_whole_noise_session_runs_over_gatt_packets() {
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
        platform: keep_vault::Platform::Windows,
        lan_name: "Test".to_owned(),
        versions: vec![PROTOCOL_VERSION],
    };
    let (opener_hello, joiner_hello) = (hello(&opener), hello(&joiner));
    let (a, b) = pair(244, None);

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
    let big = pattern(120_000, 9);
    let (sent, received) = tokio::join!(join.send(&big), open.recv());

    sent.unwrap();
    assert_eq!(received.unwrap().as_slice(), big.as_slice());
}
