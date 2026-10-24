//! The opener's side: advertise the service, publish an L2CAP channel, serve the window information, and hand
//! every opened channel to the window.

use dispatch2::{DispatchQueue, DispatchRetained};
use keep_sync::lan::announce::{Announcement, window_info_bytes};
use keep_sync::transport::{Incoming, Via};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, Message, define_class, msg_send};
use objc2_core_bluetooth::{
    CBAdvertisementDataServiceUUIDsKey, CBAttributePermissions, CBCharacteristic,
    CBCharacteristicProperties, CBL2CAPChannel, CBL2CAPPSM, CBManagerState,
    CBMutableCharacteristic, CBMutableService, CBPeripheralManager, CBPeripheralManagerDelegate,
    CBService,
};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSError, NSString};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::{OnQueue, STATE_WAIT, describe, queue, stream, usable, uuid};
use crate::{BleError, SERVICE_UUID, WINDOW_INFO_UUID};

enum Event {
    State(CBManagerState),
    Published(Result<CBL2CAPPSM, String>),
    Added(Result<(), String>),
    Advertising(Result<(), String>),
    Opened(OnQueue<CBL2CAPChannel>),
}

struct Ivars {
    events: mpsc::UnboundedSender<Event>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class does not implement Drop.
    #[unsafe(super(NSObject))]
    #[name = "OTPKeepPeripheralDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl CBPeripheralManagerDelegate for Delegate {
        #[unsafe(method(peripheralManagerDidUpdateState:))]
        fn did_update_state(&self, manager: &CBPeripheralManager) {
            // SAFETY: called on the manager's queue with the manager itself.
            #[allow(unsafe_code)]
            let state = unsafe { manager.state() };
            let _ = self.ivars().events.send(Event::State(state));
        }

        #[unsafe(method(peripheralManager:didPublishL2CAPChannel:error:))]
        fn did_publish(
            &self,
            _manager: &CBPeripheralManager,
            psm: CBL2CAPPSM,
            error: Option<&NSError>,
        ) {
            let result = describe(error).map_or(Ok(psm), Err);
            let _ = self.ivars().events.send(Event::Published(result));
        }

        #[unsafe(method(peripheralManager:didAddService:error:))]
        fn did_add(
            &self,
            _manager: &CBPeripheralManager,
            _service: &CBService,
            error: Option<&NSError>,
        ) {
            let _ = self
                .ivars()
                .events
                .send(Event::Added(describe(error).map_or(Ok(()), Err)));
        }

        #[unsafe(method(peripheralManagerDidStartAdvertising:error:))]
        fn did_start(&self, _manager: &CBPeripheralManager, error: Option<&NSError>) {
            let _ = self
                .ivars()
                .events
                .send(Event::Advertising(describe(error).map_or(Ok(()), Err)));
        }

        #[unsafe(method(peripheralManager:didOpenL2CAPChannel:error:))]
        fn did_open(
            &self,
            _manager: &CBPeripheralManager,
            channel: Option<&CBL2CAPChannel>,
            error: Option<&NSError>,
        ) {
            match (channel, describe(error)) {
                (Some(channel), None) => {
                    let _ = self
                        .ivars()
                        .events
                        .send(Event::Opened(OnQueue(channel.retain())));
                }
                (_, problem) => tracing::warn!(?problem, "a Bluetooth channel did not open"),
            }
        }
    }
);

impl Delegate {
    fn new(events: mpsc::UnboundedSender<Event>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars { events });
        // SAFETY: NSObject's designated initialiser.
        #[allow(unsafe_code)]
        unsafe {
            msg_send![super(this), init]
        }
    }
}

/// An advertised window; stops advertising and closes its channel when dropped.
pub(crate) struct Peripheral {
    queue: DispatchRetained<DispatchQueue>,
    manager: OnQueue<CBPeripheralManager>,
    _delegate: OnQueue<Delegate>,
    psm: CBL2CAPPSM,
    forwarder: JoinHandle<()>,
}

impl Peripheral {
    pub(crate) async fn open(
        announcement: &Announcement,
        connections: mpsc::Sender<Incoming>,
    ) -> Result<Self, BleError> {
        let (events, mut received) = mpsc::unbounded_channel();
        let delegate = Delegate::new(events);
        let queue = queue("otpkeep.ble.peripheral");
        // SAFETY: the manager calls back on `queue`; the delegate lives as long as the manager (held below).
        #[allow(unsafe_code)]
        let manager = unsafe {
            CBPeripheralManager::initWithDelegate_queue(
                CBPeripheralManager::alloc(),
                Some(ProtocolObject::from_ref(&*delegate)),
                Some(&queue),
            )
        };
        let manager = OnQueue(manager);
        let mut opened = Self {
            queue,
            manager,
            _delegate: OnQueue(delegate),
            psm: 0,
            forwarder: tokio::spawn(async {}),
        };
        wait_until_usable(&mut received).await?;
        opened.on_queue(|manager| {
            // SAFETY: on the manager's queue.
            #[allow(unsafe_code)]
            unsafe {
                manager.publishL2CAPChannelWithEncryption(false);
            }
        });
        opened.psm = match next(&mut received).await? {
            Event::Published(Ok(psm)) => psm,
            Event::Published(Err(problem)) => return Err(BleError::Failed(problem)),
            _ => return Err(BleError::Failed("unexpected event".to_owned())),
        };
        let info = window_info_bytes(announcement, opened.psm);
        opened.on_queue(move |manager| add_service(manager, &info));
        match next(&mut received).await? {
            Event::Added(Ok(())) => {}
            Event::Added(Err(problem)) => return Err(BleError::Failed(problem)),
            _ => return Err(BleError::Failed("unexpected event".to_owned())),
        }
        opened.on_queue(start_advertising);
        match next(&mut received).await? {
            Event::Advertising(Ok(())) => {}
            Event::Advertising(Err(problem)) => return Err(BleError::Failed(problem)),
            _ => return Err(BleError::Failed("unexpected event".to_owned())),
        }
        opened.forwarder = tokio::spawn(async move {
            while let Some(event) = received.recv().await {
                if let Event::Opened(channel) = event {
                    let link = stream::bridge(channel);
                    if connections
                        .send(Incoming {
                            link,
                            via: Via::Bluetooth,
                        })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
        tracing::info!("Bluetooth window open");
        Ok(opened)
    }

    /// Runs `work` with the manager on its queue.
    fn on_queue(&self, work: impl FnOnce(&CBPeripheralManager) + Send + 'static) {
        let manager = self.manager.clone();
        self.queue.exec_sync(move || work(manager.get()));
    }
}

impl Drop for Peripheral {
    fn drop(&mut self) {
        self.forwarder.abort();
        let psm = self.psm;
        self.on_queue(move |manager| {
            // SAFETY: on the manager's queue.
            #[allow(unsafe_code)]
            unsafe {
                manager.stopAdvertising();
                manager.removeAllServices();
                if psm != 0 {
                    manager.unpublishL2CAPChannel(psm);
                }
            }
        });
        tracing::info!("Bluetooth window closed");
    }
}

fn add_service(manager: &CBPeripheralManager, info: &[u8]) {
    let value = NSData::with_bytes(info);
    // SAFETY: plain object construction and a call on the manager's queue.
    #[allow(unsafe_code)]
    unsafe {
        let characteristic = CBMutableCharacteristic::initWithType_properties_value_permissions(
            CBMutableCharacteristic::alloc(),
            &uuid(WINDOW_INFO_UUID),
            CBCharacteristicProperties::Read,
            Some(&value),
            CBAttributePermissions::Readable,
        );
        let service = CBMutableService::initWithType_primary(
            CBMutableService::alloc(),
            &uuid(SERVICE_UUID),
            true,
        );
        let characteristic: Retained<CBCharacteristic> = Retained::into_super(characteristic);
        service.setCharacteristics(Some(&NSArray::from_retained_slice(&[characteristic])));
        manager.addService(&service);
    }
}

fn start_advertising(manager: &CBPeripheralManager) {
    let services: Retained<NSArray<objc2_core_bluetooth::CBUUID>> =
        NSArray::from_retained_slice(&[uuid(SERVICE_UUID)]);
    let services: Retained<AnyObject> = Retained::into_super(Retained::into_super(services));
    // SAFETY: the key is CoreBluetooth's constant; the call runs on the manager's queue. No local name is set.
    #[allow(unsafe_code)]
    unsafe {
        let data: Retained<NSDictionary<NSString, AnyObject>> =
            NSDictionary::from_retained_objects(&[CBAdvertisementDataServiceUUIDsKey], &[services]);
        manager.startAdvertising(Some(&data));
    }
}

async fn next(received: &mut mpsc::UnboundedReceiver<Event>) -> Result<Event, BleError> {
    match tokio::time::timeout(STATE_WAIT, received.recv()).await {
        Ok(Some(event)) => Ok(event),
        _ => Err(BleError::Failed("Bluetooth did not respond".to_owned())),
    }
}

async fn wait_until_usable(received: &mut mpsc::UnboundedReceiver<Event>) -> Result<(), BleError> {
    loop {
        if let Event::State(state) = next(received).await.map_err(|_| BleError::Unsupported)?
            && let Some(result) = usable(state)
        {
            return result;
        }
    }
}
