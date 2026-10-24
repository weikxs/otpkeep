//! CoreBluetooth on macOS (design/07-platform.md §5.1).
//!
//! Managers run on one serial dispatch queue; every call on a CoreBluetooth object happens in a closure on that
//! queue, and delegate callbacks arrive there too, so these objects are never used from two threads at once.
//! L2CAP streams move to a thread of their own with a run loop (see [`stream`]).

pub(crate) mod central;
pub(crate) mod peripheral;
pub(crate) mod stream;

use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2_core_bluetooth::{CBManagerState, CBUUID};
use objc2_foundation::{NSError, NSString};

use crate::BleError;

/// How long the system may take to report whether Bluetooth is usable.
pub(crate) const STATE_WAIT: Duration = Duration::from_secs(5);

/// An Objective-C object that is only used on the Bluetooth queue, or handed to one other thread once.
pub(crate) struct OnQueue<T: ?Sized>(pub(crate) Retained<T>);

impl<T: ?Sized> OnQueue<T> {
    /// The object; only to be used on the Bluetooth queue (or the one thread a stream was handed to).
    pub(crate) fn get(&self) -> &T {
        &self.0
    }
}

impl<T: objc2::Message> Clone for OnQueue<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

// SAFETY: every `OnQueue` value is only dereferenced inside closures run on the serial Bluetooth queue (or, for
// streams, on the single thread they were handed to), so the object is never used concurrently; moving the
// retained pointer between threads does not touch the object.
#[allow(unsafe_code)]
unsafe impl<T: ?Sized> Send for OnQueue<T> {}
// SAFETY: as above; shared references are only dereferenced on the queue.
#[allow(unsafe_code)]
unsafe impl<T: ?Sized> Sync for OnQueue<T> {}

/// The serial queue CoreBluetooth calls back on.
pub(crate) fn queue(label: &str) -> DispatchRetained<DispatchQueue> {
    DispatchQueue::new(label, None)
}

pub(crate) fn uuid(text: &str) -> Retained<CBUUID> {
    // SAFETY: a constant, well-formed UUID string.
    #[allow(unsafe_code)]
    unsafe {
        CBUUID::UUIDWithString(&NSString::from_str(text))
    }
}

/// What a manager's state means for us; `None` while the system is still deciding.
pub(crate) fn usable(state: CBManagerState) -> Option<Result<(), BleError>> {
    match state {
        CBManagerState::PoweredOn => Some(Ok(())),
        CBManagerState::PoweredOff => Some(Err(BleError::Off)),
        CBManagerState::Unauthorized => Some(Err(BleError::Denied)),
        CBManagerState::Unsupported => Some(Err(BleError::Unsupported)),
        _ => None,
    }
}

pub(crate) fn describe(error: Option<&NSError>) -> Option<String> {
    error.map(|e| e.localizedDescription().to_string())
}
