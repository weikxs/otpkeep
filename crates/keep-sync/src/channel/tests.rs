use keep_vault::Platform;
use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

use super::code::{CodeCheck, CodeError, Secret, confirm_code};
use super::hello::Hello;
use super::noise::{Channel, ChannelError, Purpose, accept_noise, connect_noise};
use super::{MAX_MESSAGE, PROTOCOL_VERSION};
use crate::identity::DeviceIdentity;

const GROUP: [u8; 16] = [0x42; 16];
const WINDOW: [u8; 16] = [0x17; 16];

fn identity(n: u8) -> DeviceIdentity {
    DeviceIdentity::from_seeds(&[n; 32], &[n + 100; 32], &GROUP)
}

fn hello(device: &DeviceIdentity, name: &str) -> Hello {
    Hello {
        device: device.device_id(),
        dx_cert: Some(device.dx_cert()),
        platform: Platform::MacOs,
        lan_name: name.to_owned(),
        versions: vec![PROTOCOL_VERSION],
    }
}

type Pipe = tokio::io::DuplexStream;

/// Opener (device 1) and joiner (device 2) after the Noise handshake.
async fn connected(purpose: Purpose) -> ((Channel<Pipe>, Hello), (Channel<Pipe>, Hello)) {
    let (a, b) = duplex(1 << 16);
    let (opener, joiner) = (identity(1), identity(2));
    let opener_hello = hello(&opener, "Study Mac");
    let open = accept_noise(a, purpose, &WINDOW, opener.exchange_secret(), &opener_hello);
    let joiner_hello = hello(&joiner, "Phone");
    let join = connect_noise(b, purpose, &WINDOW, joiner.exchange_secret(), &joiner_hello);
    let (open, join) = tokio::join!(open, join);
    (open.unwrap(), join.unwrap())
}

#[tokio::test]
async fn both_sides_learn_each_other_and_the_keys_they_proved() {
    let ((opener, joiner_hello), (joiner, opener_hello)) = connected(Purpose::Sync).await;

    assert_eq!(joiner_hello.device, identity(2).device_id());
    assert_eq!(joiner_hello.lan_name, "Phone");
    assert_eq!(opener_hello.device, identity(1).device_id());
    assert_eq!(opener.remote_static(), identity(2).dx_public());
    assert_eq!(joiner.remote_static(), identity(1).dx_public());
    assert_eq!(opener.handshake_hash(), joiner.handshake_hash());
}

#[tokio::test]
async fn messages_of_any_size_up_to_the_limit_arrive_intact() {
    let ((mut opener, _), (mut joiner, _)) = connected(Purpose::Sync).await;
    let big: Vec<u8> = (0..300_000u32).map(|n| (n % 251) as u8).collect();

    let (sent, received) = tokio::join!(opener.send(&big), joiner.recv());
    sent.unwrap();
    assert_eq!(received.unwrap().as_slice(), big.as_slice());
    let (sent, received) = tokio::join!(joiner.send(b""), opener.recv());
    sent.unwrap();
    assert!(received.unwrap().is_empty());
    assert!(matches!(
        opener.send(&vec![0; MAX_MESSAGE + 1]).await,
        Err(ChannelError::TooLarge)
    ));
}

#[tokio::test]
async fn a_connection_for_another_window_fails_the_handshake() {
    let (a, b) = duplex(1 << 16);
    let (opener, joiner) = (identity(1), identity(2));
    let opener_hello = hello(&opener, "A");
    let open = accept_noise(
        a,
        Purpose::Sync,
        &WINDOW,
        opener.exchange_secret(),
        &opener_hello,
    );
    let joiner_hello = hello(&joiner, "B");
    let join = connect_noise(
        b,
        Purpose::Sync,
        &[0x18; 16],
        joiner.exchange_secret(),
        &joiner_hello,
    );

    let (open, join) = tokio::join!(open, join);

    assert!(open.is_err() || join.is_err());
}

#[tokio::test]
async fn a_pairing_connection_is_not_a_sync_connection() {
    let (a, b) = duplex(1 << 16);
    let (opener, joiner) = (identity(1), identity(2));
    let opener_hello = hello(&opener, "A");
    let open = accept_noise(
        a,
        Purpose::Sync,
        &WINDOW,
        opener.exchange_secret(),
        &opener_hello,
    );
    let joiner_hello = hello(&joiner, "B");
    let join = connect_noise(
        b,
        Purpose::Pair,
        &WINDOW,
        joiner.exchange_secret(),
        &joiner_hello,
    );

    let (open, join) = tokio::join!(open, join);

    assert!(open.is_err() || join.is_err());
}

#[tokio::test]
async fn a_tampered_frame_is_rejected() {
    let (a, mut wire) = duplex(1 << 16);
    let (b, mut wire_b) = duplex(1 << 16);
    let (opener, joiner) = (identity(1), identity(2));
    // Relay everything, flipping one bit in the first data frame from the joiner.
    let relay = tokio::spawn(async move {
        let mut frames = 0;
        loop {
            tokio::select! {
                read = read_frame(&mut wire_b) => {
                    let Some(mut frame) = read else { break };
                    frames += 1;
                    if frames == 3 { let last = frame.len() - 1; frame[last] ^= 1; }
                    write_frame(&mut wire, &frame).await;
                }
                read = read_frame(&mut wire) => {
                    let Some(frame) = read else { break };
                    write_frame(&mut wire_b, &frame).await;
                }
            }
        }
    });
    let opener_hello = hello(&opener, "A");
    let open = accept_noise(
        a,
        Purpose::Sync,
        &WINDOW,
        opener.exchange_secret(),
        &opener_hello,
    );
    let joiner_hello = hello(&joiner, "B");
    let join = connect_noise(
        b,
        Purpose::Sync,
        &WINDOW,
        joiner.exchange_secret(),
        &joiner_hello,
    );
    let (open, join) = tokio::join!(open, join);
    let (mut opener, _) = open.unwrap();
    let (mut joiner, _) = join.unwrap();

    let (sent, received) = tokio::join!(joiner.send(b"hello"), opener.recv());
    sent.unwrap();
    assert!(received.is_err());
    relay.abort();
}

async fn read_frame(stream: &mut Pipe) -> Option<Vec<u8>> {
    let len = stream.read_u16().await.ok()?;
    let mut frame = vec![0; usize::from(len)];
    stream.read_exact(&mut frame).await.ok()?;
    Some(frame)
}

async fn write_frame(stream: &mut Pipe, frame: &[u8]) {
    stream
        .write_u16(u16::try_from(frame.len()).unwrap())
        .await
        .unwrap();
    stream.write_all(frame).await.unwrap();
}

#[tokio::test]
async fn the_same_code_on_both_sides_is_confirmed_and_both_see_the_same_check_code() {
    let ((mut opener, _), (mut joiner, _)) = connected(Purpose::Pair).await;
    let secret = Secret::PairCode("12345678".to_owned().into());

    let (o, j) = tokio::join!(
        confirm_code(&mut opener, CodeCheck::Opener(&secret)),
        confirm_code(&mut joiner, CodeCheck::Joiner(&secret))
    );
    let (o, j) = (o.unwrap(), j.unwrap());

    assert_eq!(o.sas, j.sas);
    assert_eq!(o.sas.len(), 6);
    assert!(o.sas.chars().all(|c| c.is_ascii_digit()));
}

#[tokio::test]
async fn a_wrong_code_is_reported_to_both_sides_as_a_wrong_code() {
    let ((mut opener, _), (mut joiner, _)) = connected(Purpose::Sync).await;

    let (right, typed) = (
        Secret::SyncCode("123456".to_owned().into()),
        Secret::SyncCode("123457".to_owned().into()),
    );

    let (o, j) = tokio::join!(
        confirm_code(&mut opener, CodeCheck::Opener(&right)),
        confirm_code(&mut joiner, CodeCheck::Joiner(&typed))
    );

    assert_eq!(o.err(), Some(CodeError::Wrong));
    assert_eq!(j.err(), Some(CodeError::Wrong));
}

#[tokio::test]
async fn the_pairing_secret_from_the_qr_code_works_like_the_typed_code() {
    let ((mut opener, _), (mut joiner, _)) = connected(Purpose::Pair).await;
    let qr = Secret::PairSecret([9; 16]);
    let typed = Secret::PairCode("12345678".to_owned().into());

    let (o, j) = tokio::join!(
        confirm_code(
            &mut opener,
            CodeCheck::PairOpener {
                secret: &[9; 16],
                code: "12345678"
            }
        ),
        confirm_code(&mut joiner, CodeCheck::Joiner(&qr))
    );
    assert!(o.is_ok() && j.is_ok());
    let ((mut opener, _), (mut joiner, _)) = connected(Purpose::Pair).await;
    let (o, j) = tokio::join!(
        confirm_code(
            &mut opener,
            CodeCheck::PairOpener {
                secret: &[9; 16],
                code: "12345678"
            }
        ),
        confirm_code(&mut joiner, CodeCheck::Joiner(&typed))
    );
    assert!(o.is_ok() && j.is_ok());
}

#[tokio::test]
async fn a_relay_between_two_channels_cannot_pass_the_code_check() {
    // The attacker runs one Noise channel with each side and forwards the code-check messages unchanged.
    let ((mut opener, _), (mut to_opener, _)) = connected(Purpose::Sync).await;
    let ((mut from_joiner, _), (mut joiner, _)) = connected(Purpose::Sync).await;
    let code = Secret::SyncCode("246810".to_owned().into());
    let relay = async {
        // joiner → opener: code-check start; opener → joiner: reply; joiner → opener: confirmation.
        let first = from_joiner.recv().await.unwrap();
        to_opener.send(&first).await.unwrap();
        let reply = to_opener.recv().await.unwrap();
        from_joiner.send(&reply).await.unwrap();
        if let Ok(confirm) = from_joiner.recv().await {
            let _ = to_opener.send(&confirm).await;
        }
        if let Ok(answer) = to_opener.recv().await {
            let _ = from_joiner.send(&answer).await;
        }
    };

    let (o, j, ()) = tokio::join!(
        confirm_code(&mut opener, CodeCheck::Opener(&code)),
        confirm_code(&mut joiner, CodeCheck::Joiner(&code)),
        relay
    );

    assert_eq!(o.err(), Some(CodeError::Wrong));
    assert!(j.is_err());
}

#[tokio::test]
async fn the_versions_both_support_decide_the_protocol() {
    assert_eq!(super::hello::common_version(&[1, 2], &[1]), Some(1));
    assert_eq!(super::hello::common_version(&[1, 3, 2], &[2, 3]), Some(3));
    assert_eq!(super::hello::common_version(&[2], &[1]), None);
}

#[tokio::test]
async fn a_busy_window_says_so_before_any_handshake() {
    let (a, b) = duplex(1 << 16);
    let device = identity(2);
    let joiner_hello = hello(&device, "B");

    let (refusal, outcome) = tokio::join!(
        super::noise::refuse_busy(a),
        connect_noise(
            b,
            Purpose::Sync,
            &WINDOW,
            device.exchange_secret(),
            &joiner_hello
        )
    );

    refusal.unwrap();
    assert!(matches!(outcome, Err(ChannelError::Busy)));
}

#[tokio::test]
async fn a_silent_peer_times_out_when_an_idle_limit_is_set() {
    let ((mut opener, _), (_joiner, _)) = connected(Purpose::Sync).await;
    opener.set_idle_timeout(Some(std::time::Duration::from_millis(50)));

    assert!(matches!(opener.recv().await, Err(ChannelError::TimedOut)));
}
