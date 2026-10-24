//! An L2CAP channel as an async byte stream.
//!
//! The channel's `NSInputStream` / `NSOutputStream` need a run loop. Each channel gets a thread that schedules
//! both streams on its own run loop and moves bytes; a Tokio task joins that thread to one end of an in-memory
//! pipe, whose other end the session reads and writes.

use std::ptr::NonNull;
use std::sync::mpsc as std_mpsc;

use keep_sync::transport::BoxLink;
use objc2_core_bluetooth::CBL2CAPChannel;
use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSRunLoop, NSStreamStatus};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use super::OnQueue;

/// Bytes held in the pipe between the session and the channel.
const PIPE: usize = 64 * 1024;
const CHUNK: usize = 4096;
/// How long one turn of the run loop may wait.
const TURN: f64 = 0.01;

/// Turns an opened channel into a byte stream. Call from within a Tokio runtime.
pub(crate) fn bridge(channel: OnQueue<CBL2CAPChannel>) -> BoxLink {
    let (session_end, bridge_end) = tokio::io::duplex(PIPE);
    let (to_radio, outgoing) = std_mpsc::channel::<Vec<u8>>();
    let (from_radio, mut incoming) = mpsc::unbounded_channel::<Vec<u8>>();
    let spawned = std::thread::Builder::new()
        .name("otpkeep-l2cap".to_owned())
        .spawn(move || pump(&channel, &outgoing, &from_radio));
    if spawned.is_err() {
        tracing::warn!("could not start the Bluetooth stream thread");
    }
    let (mut read_half, mut write_half) = tokio::io::split(bridge_end);
    tokio::spawn(async move {
        let mut buffer = vec![0u8; CHUNK];
        loop {
            tokio::select! {
                read = read_half.read(&mut buffer) => match read {
                    Ok(0) | Err(_) => break,
                    Ok(n) => if to_radio.send(buffer[..n].to_vec()).is_err() { break },
                },
                received = incoming.recv() => match received {
                    Some(bytes) => if write_half.write_all(&bytes).await.is_err() { break },
                    None => break,
                },
            }
        }
        let _ = write_half.shutdown().await;
    });
    Box::new(session_end)
}

/// Runs on the channel's thread until either side closes.
fn pump(
    channel: &OnQueue<CBL2CAPChannel>,
    outgoing: &std_mpsc::Receiver<Vec<u8>>,
    incoming: &mpsc::UnboundedSender<Vec<u8>>,
) {
    // SAFETY: the channel was handed to this thread alone; its streams are only used here from now on.
    #[allow(unsafe_code)]
    let (Some(input), Some(output)) = (unsafe { channel.0.inputStream() }, unsafe {
        channel.0.outputStream()
    }) else {
        return;
    };
    let run_loop = NSRunLoop::currentRunLoop();
    // SAFETY: Foundation's constant run loop mode, initialised before any code runs.
    #[allow(unsafe_code)]
    let mode = unsafe { NSDefaultRunLoopMode };
    // SAFETY: both streams are scheduled on this thread's run loop and only used on this thread.
    #[allow(unsafe_code)]
    unsafe {
        input.scheduleInRunLoop_forMode(&run_loop, mode);
        output.scheduleInRunLoop_forMode(&run_loop, mode);
    }
    input.open();
    output.open();
    let mut pending: Vec<u8> = Vec::new();
    let mut buffer = vec![0u8; CHUNK];
    let mut session_open = true;
    loop {
        run_loop.runMode_beforeDate(mode, &NSDate::dateWithTimeIntervalSinceNow(TURN));
        if matches!(
            input.streamStatus(),
            NSStreamStatus::Error | NSStreamStatus::AtEnd | NSStreamStatus::Closed
        ) || matches!(
            output.streamStatus(),
            NSStreamStatus::Error | NSStreamStatus::Closed
        ) {
            break;
        }
        while input.hasBytesAvailable() {
            let Some(target) = NonNull::new(buffer.as_mut_ptr()) else {
                break;
            };
            // SAFETY: `target` points to `buffer`, which holds `CHUNK` writable bytes.
            #[allow(unsafe_code)]
            let read = unsafe { input.read_maxLength(target, CHUNK) };
            let Ok(read) = usize::try_from(read) else {
                break;
            };
            if read == 0 || incoming.send(buffer[..read].to_vec()).is_err() {
                break;
            }
        }
        loop {
            match outgoing.try_recv() {
                Ok(bytes) => pending.extend_from_slice(&bytes),
                Err(std_mpsc::TryRecvError::Empty) => break,
                Err(std_mpsc::TryRecvError::Disconnected) => {
                    session_open = false;
                    break;
                }
            }
        }
        while !pending.is_empty() && output.hasSpaceAvailable() {
            let Some(source) = NonNull::new(pending.as_mut_ptr()) else {
                break;
            };
            // SAFETY: `source` points to `pending`, which holds `pending.len()` readable bytes.
            #[allow(unsafe_code)]
            let written = unsafe { output.write_maxLength(source, pending.len()) };
            match usize::try_from(written) {
                Ok(n) if n > 0 => {
                    pending.drain(..n);
                }
                _ => break,
            }
        }
        if !session_open && pending.is_empty() {
            break;
        }
    }
    input.close();
    output.close();
}
