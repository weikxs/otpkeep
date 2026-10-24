//! An L2CAP channel as an async byte stream.
//!
//! The channel's `NSInputStream` / `NSOutputStream` deliver events only through a run loop that is running.
//! Each channel gets a thread that schedules both streams on its own run loop and moves bytes when the streams
//! say so: it reads only after "bytes available" and writes only after "space available", once per event, as
//! the stream may block otherwise (a stream may report bytes available when it is not sure). A Tokio task joins
//! that thread to one end of an in-memory pipe, whose other end the session reads and writes. The channel object
//! is held by the thread for as long as the streams are used: the channel closes when it is released.

use std::cell::Cell;
use std::ptr::NonNull;
use std::sync::mpsc as std_mpsc;

use keep_sync::transport::BoxLink;
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_core_bluetooth::CBL2CAPChannel;
use objc2_foundation::{
    NSDate, NSDefaultRunLoopMode, NSInputStream, NSOutputStream, NSRunLoop, NSStream,
    NSStreamDelegate, NSStreamEvent,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use super::OnQueue;

/// Bytes held in the pipe between the session and the channel.
const PIPE: usize = 64 * 1024;
const CHUNK: usize = 4096;
/// How long one turn of the run loop may wait for an event.
const TURN: f64 = 0.01;

/// What a stream's delegate saw since the last turn.
#[derive(Default)]
struct Flags {
    ready: Cell<bool>,
    ended: Cell<bool>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class does not implement Drop.
    #[unsafe(super(NSObject))]
    #[name = "OTPKeepStreamEvents"]
    #[ivars = Flags]
    struct Events;

    unsafe impl NSObjectProtocol for Events {}

    unsafe impl NSStreamDelegate for Events {
        #[unsafe(method(stream:handleEvent:))]
        fn handle(&self, _stream: &NSStream, event: NSStreamEvent) {
            let flags = self.ivars();
            if event.contains(NSStreamEvent::HasBytesAvailable)
                || event.contains(NSStreamEvent::HasSpaceAvailable)
            {
                flags.ready.set(true);
            }
            if event.contains(NSStreamEvent::EndEncountered)
                || event.contains(NSStreamEvent::ErrorOccurred)
            {
                flags.ended.set(true);
            }
        }
    }
);

impl Events {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(Flags::default());
        // SAFETY: NSObject's designated initialiser.
        #[allow(unsafe_code)]
        unsafe {
            msg_send![super(this), init]
        }
    }
}

/// Streams to bridge, with whatever keeps them open (the channel).
pub(crate) struct Streams {
    pub(crate) input: OnQueue<NSInputStream>,
    pub(crate) output: OnQueue<NSOutputStream>,
    pub(crate) _channel: Option<OnQueue<CBL2CAPChannel>>,
}

/// Turns an opened channel into a byte stream. Call from within a Tokio runtime.
pub(crate) fn bridge(channel: OnQueue<CBL2CAPChannel>) -> BoxLink {
    // SAFETY: reading the streams of a channel CoreBluetooth just opened; they are only used by the new thread.
    #[allow(unsafe_code)]
    let streams = unsafe { (channel.get().inputStream(), channel.get().outputStream()) };
    let (Some(input), Some(output)) = streams else {
        // A link that is already closed.
        let (link, _) = tokio::io::duplex(1);
        return Box::new(link);
    };
    bridge_streams(Streams {
        input: OnQueue(input),
        output: OnQueue(output),
        _channel: Some(channel),
    })
}

/// Joins two streams to an async byte stream. Call from within a Tokio runtime.
pub(crate) fn bridge_streams(streams: Streams) -> BoxLink {
    let (session_end, bridge_end) = tokio::io::duplex(PIPE);
    let (to_radio, outgoing) = std_mpsc::channel::<Vec<u8>>();
    let (from_radio, mut incoming) = mpsc::unbounded_channel::<Vec<u8>>();
    let spawned = std::thread::Builder::new()
        .name("otpkeep-l2cap".to_owned())
        .spawn(move || pump(&streams, &outgoing, &from_radio));
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
        // The session stopped writing (or the radio closed): the thread finishes writing and closes, while what
        // the radio already received is still delivered.
        drop(to_radio);
        while let Some(bytes) = incoming.recv().await {
            if write_half.write_all(&bytes).await.is_err() {
                break;
            }
        }
        let _ = write_half.shutdown().await;
    });
    Box::new(session_end)
}

/// Runs on the channel's thread until either side closes.
fn pump(
    streams: &Streams,
    outgoing: &std_mpsc::Receiver<Vec<u8>>,
    incoming: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let (input, output) = (streams.input.get(), streams.output.get());
    let (readable, writable) = (Events::new(), Events::new());
    let run_loop = NSRunLoop::currentRunLoop();
    // SAFETY: Foundation's constant run loop mode, initialised before any code runs.
    #[allow(unsafe_code)]
    let mode = unsafe { NSDefaultRunLoopMode };
    // SAFETY: both streams were handed to this thread alone: their delegates and run loop are set here, before
    // they are opened, and they are only used on this thread. The delegates outlive the streams' use below.
    #[allow(unsafe_code)]
    unsafe {
        input.setDelegate(Some(ProtocolObject::from_ref(&*readable)));
        output.setDelegate(Some(ProtocolObject::from_ref(&*writable)));
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
        let (read_flags, write_flags) = (readable.ivars(), writable.ivars());
        if read_flags.ready.take() && !read_once(input, &mut buffer, incoming) {
            break;
        }
        if read_flags.ended.get() || write_flags.ended.get() {
            break;
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
        if !pending.is_empty() && write_flags.ready.get() {
            // One write per "space available"; the next write waits for the next event.
            write_flags.ready.set(false);
            if !write_once(output, &mut pending) {
                break;
            }
        }
        if !session_open && pending.is_empty() {
            break;
        }
    }
    // SAFETY: still on this thread; the delegates are removed before they are released.
    #[allow(unsafe_code)]
    unsafe {
        input.setDelegate(None);
        output.setDelegate(None);
    }
    input.close();
    output.close();
}

/// Reads what the stream announced; returns `false` at the end of the stream.
fn read_once(
    input: &NSInputStream,
    buffer: &mut [u8],
    incoming: &mpsc::UnboundedSender<Vec<u8>>,
) -> bool {
    let Some(target) = NonNull::new(buffer.as_mut_ptr()) else {
        return false;
    };
    // SAFETY: `target` points to `buffer`, which holds `buffer.len()` writable bytes.
    #[allow(unsafe_code)]
    let read = unsafe { input.read_maxLength(target, buffer.len()) };
    match usize::try_from(read) {
        Ok(0) | Err(_) => false,
        Ok(n) => incoming.send(buffer[..n].to_vec()).is_ok(),
    }
}

/// Writes as much as the stream takes; returns `false` when it fails.
fn write_once(output: &NSOutputStream, pending: &mut Vec<u8>) -> bool {
    let Some(source) = NonNull::new(pending.as_mut_ptr()) else {
        return false;
    };
    // SAFETY: `source` points to `pending`, which holds `pending.len()` readable bytes.
    #[allow(unsafe_code)]
    let written = unsafe { output.write_maxLength(source, pending.len()) };
    match usize::try_from(written) {
        Ok(n) if n > 0 => {
            pending.drain(..n);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests;
