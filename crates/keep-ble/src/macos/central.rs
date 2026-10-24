//! The joiner's side: scan for the service, connect, read each window's information, and open an L2CAP
//! channel when the session starts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dispatch2::{DispatchQueue, DispatchRetained};
use keep_sync::lan::announce::{FoundWindow, from_window_info};
use keep_sync::transport::{Connecting, Connector, Route, Via};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, Message, define_class, msg_send};
use objc2_core_bluetooth::{
    CBCentralManager, CBCentralManagerDelegate, CBL2CAPChannel, CBL2CAPPSM, CBManagerState,
    CBPeripheral, CBPeripheralDelegate, CBService,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSNumber, NSString};
use tokio::sync::{mpsc, oneshot};

use super::{OnQueue, STATE_WAIT, describe, queue, stream, usable, uuid};
use crate::{BleError, SERVICE_UUID, WINDOW_INFO_UUID};

/// How long connecting, discovering and reading one device may take.
const PER_DEVICE: Duration = Duration::from_secs(8);

enum Event {
    State(CBManagerState),
    Discovered(String, OnQueue<CBPeripheral>),
    Connected(String),
    Services(String),
    Characteristics(String, OnQueue<CBService>),
    Value(String, Vec<u8>),
    Lost(String),
}

type Waiters = Mutex<HashMap<String, oneshot::Sender<Result<OnQueue<CBL2CAPChannel>, String>>>>;

struct Ivars {
    events: mpsc::UnboundedSender<Event>,
    channels: Arc<Waiters>,
}

fn id_of(peripheral: &CBPeripheral) -> String {
    // SAFETY: reading the identifier of a peripheral CoreBluetooth handed to us.
    #[allow(unsafe_code)]
    unsafe {
        peripheral.identifier().UUIDString().to_string()
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class does not implement Drop.
    #[unsafe(super(NSObject))]
    #[name = "OTPKeepCentralDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl CBCentralManagerDelegate for Delegate {
        #[unsafe(method(centralManagerDidUpdateState:))]
        fn did_update_state(&self, central: &CBCentralManager) {
            // SAFETY: called on the manager's queue with the manager itself.
            #[allow(unsafe_code)]
            let state = unsafe { central.state() };
            let _ = self.ivars().events.send(Event::State(state));
        }

        #[unsafe(method(centralManager:didDiscoverPeripheral:advertisementData:RSSI:))]
        fn did_discover(
            &self,
            _central: &CBCentralManager,
            peripheral: &CBPeripheral,
            _data: &NSDictionary<NSString, AnyObject>,
            _rssi: &NSNumber,
        ) {
            let _ = self.ivars().events.send(Event::Discovered(
                id_of(peripheral),
                OnQueue(peripheral.retain()),
            ));
        }

        #[unsafe(method(centralManager:didConnectPeripheral:))]
        fn did_connect(&self, _central: &CBCentralManager, peripheral: &CBPeripheral) {
            let _ = self
                .ivars()
                .events
                .send(Event::Connected(id_of(peripheral)));
        }

        #[unsafe(method(centralManager:didFailToConnectPeripheral:error:))]
        fn did_fail(
            &self,
            _central: &CBCentralManager,
            peripheral: &CBPeripheral,
            _error: Option<&NSError>,
        ) {
            let _ = self.ivars().events.send(Event::Lost(id_of(peripheral)));
        }

        #[unsafe(method(centralManager:didDisconnectPeripheral:error:))]
        fn did_disconnect(
            &self,
            _central: &CBCentralManager,
            peripheral: &CBPeripheral,
            _error: Option<&NSError>,
        ) {
            let _ = self.ivars().events.send(Event::Lost(id_of(peripheral)));
        }
    }

    unsafe impl CBPeripheralDelegate for Delegate {
        #[unsafe(method(peripheral:didDiscoverServices:))]
        fn did_discover_services(&self, peripheral: &CBPeripheral, error: Option<&NSError>) {
            let event = match describe(error) {
                None => Event::Services(id_of(peripheral)),
                Some(_) => Event::Lost(id_of(peripheral)),
            };
            let _ = self.ivars().events.send(event);
        }

        #[unsafe(method(peripheral:didDiscoverCharacteristicsForService:error:))]
        fn did_discover_characteristics(
            &self,
            peripheral: &CBPeripheral,
            service: &CBService,
            error: Option<&NSError>,
        ) {
            let event = match describe(error) {
                None => Event::Characteristics(id_of(peripheral), OnQueue(service.retain())),
                Some(_) => Event::Lost(id_of(peripheral)),
            };
            let _ = self.ivars().events.send(event);
        }

        #[unsafe(method(peripheral:didUpdateValueForCharacteristic:error:))]
        fn did_update_value(
            &self,
            peripheral: &CBPeripheral,
            characteristic: &objc2_core_bluetooth::CBCharacteristic,
            error: Option<&NSError>,
        ) {
            // SAFETY: reading the value CoreBluetooth just delivered, on its queue.
            #[allow(unsafe_code)]
            let value = unsafe { characteristic.value() };
            let event = match (value, describe(error)) {
                (Some(value), None) => Event::Value(id_of(peripheral), value.to_vec()),
                _ => Event::Lost(id_of(peripheral)),
            };
            let _ = self.ivars().events.send(event);
        }

        #[unsafe(method(peripheral:didOpenL2CAPChannel:error:))]
        fn did_open(
            &self,
            peripheral: &CBPeripheral,
            channel: Option<&CBL2CAPChannel>,
            error: Option<&NSError>,
        ) {
            let waiter = self
                .ivars()
                .channels
                .lock()
                .ok()
                .and_then(|mut w| w.remove(&id_of(peripheral)));
            if let Some(waiter) = waiter {
                let result = match (channel, describe(error)) {
                    (Some(channel), None) => Ok(OnQueue(channel.retain())),
                    (_, problem) => Err(problem.unwrap_or_else(|| "no channel".to_owned())),
                };
                let _ = waiter.send(result);
            }
        }
    }
);

impl Delegate {
    fn new(events: mpsc::UnboundedSender<Event>, channels: Arc<Waiters>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars { events, channels });
        // SAFETY: NSObject's designated initialiser.
        #[allow(unsafe_code)]
        unsafe {
            msg_send![super(this), init]
        }
    }
}

/// The central manager and the devices it found, kept while any found window may still be joined.
struct Central {
    queue: DispatchRetained<DispatchQueue>,
    manager: OnQueue<CBCentralManager>,
    delegate: OnQueue<Delegate>,
    channels: Arc<Waiters>,
    peripherals: Mutex<Vec<OnQueue<CBPeripheral>>>,
}

impl Central {
    fn on_queue(&self, work: impl FnOnce(&CBCentralManager, &Delegate) + Send + 'static) {
        let (manager, delegate) = (self.manager.clone(), self.delegate.clone());
        self.queue
            .exec_sync(move || work(manager.get(), delegate.get()));
    }
}

impl Drop for Central {
    fn drop(&mut self) {
        let peripherals: Vec<OnQueue<CBPeripheral>> = self
            .peripherals
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default();
        self.on_queue(move |manager, _| {
            // SAFETY: on the manager's queue, with peripherals it reported.
            #[allow(unsafe_code)]
            unsafe {
                manager.stopScan();
                for peripheral in &peripherals {
                    manager.cancelPeripheralConnection(peripheral.get());
                }
            }
        });
    }
}

/// Opens an L2CAP channel to one found window.
struct Window {
    central: Arc<Central>,
    peripheral: OnQueue<CBPeripheral>,
    psm: CBL2CAPPSM,
}

impl Connector for Window {
    fn connect(&self) -> Connecting<'_> {
        Box::pin(async move {
            let (sender, opened) = oneshot::channel();
            let id = id_of(self.peripheral.get());
            if let Ok(mut waiters) = self.central.channels.lock() {
                waiters.insert(id, sender);
            }
            let (peripheral, psm) = (self.peripheral.clone(), self.psm);
            self.central.on_queue(move |_, _| {
                // SAFETY: on the manager's queue, with a peripheral it reported and that is connected.
                #[allow(unsafe_code)]
                unsafe {
                    peripheral.get().openL2CAPChannel(psm);
                }
            });
            match tokio::time::timeout(PER_DEVICE, opened).await {
                Ok(Ok(Ok(channel))) => Ok(stream::bridge(channel)),
                Ok(Ok(Err(problem))) => Err(std::io::Error::other(problem)),
                _ => Err(std::io::Error::other("the Bluetooth channel did not open")),
            }
        })
    }
}

pub(crate) async fn browse(
    limit: Duration,
    settle: Duration,
) -> Result<Vec<FoundWindow>, BleError> {
    let (events, mut received) = mpsc::unbounded_channel();
    let channels: Arc<Waiters> = Arc::default();
    let delegate = Delegate::new(events, Arc::clone(&channels));
    let queue = queue("otpkeep.ble.central");
    // SAFETY: the manager calls back on `queue`; the delegate is held as long as the manager.
    #[allow(unsafe_code)]
    let manager = unsafe {
        CBCentralManager::initWithDelegate_queue(
            CBCentralManager::alloc(),
            Some(ProtocolObject::from_ref(&*delegate)),
            Some(&queue),
        )
    };
    let central = Arc::new(Central {
        queue,
        manager: OnQueue(manager),
        delegate: OnQueue(delegate),
        channels,
        peripherals: Mutex::new(Vec::new()),
    });
    wait_until_usable(&mut received).await?;
    central.on_queue(|manager, _| {
        let services = NSArray::from_retained_slice(&[uuid(SERVICE_UUID)]);
        // SAFETY: on the manager's queue.
        #[allow(unsafe_code)]
        unsafe {
            manager.scanForPeripheralsWithServices_options(Some(&services), None);
        }
    });
    let mut search = Search {
        central: Arc::clone(&central),
        found: Vec::new(),
        known: HashMap::new(),
    };
    let deadline = tokio::time::Instant::now() + limit;
    let mut settled: Option<tokio::time::Instant> = None;
    loop {
        let until = settled.map_or(deadline, |s| s.min(deadline));
        let Ok(Some(event)) = tokio::time::timeout_at(until, received.recv()).await else {
            break;
        };
        if search.handle(event) {
            settled.get_or_insert(tokio::time::Instant::now() + settle);
        }
    }
    central.on_queue(|manager, _| {
        // SAFETY: on the manager's queue.
        #[allow(unsafe_code)]
        unsafe {
            manager.stopScan();
        }
    });
    Ok(search.found)
}

/// The state of one search.
struct Search {
    central: Arc<Central>,
    found: Vec<FoundWindow>,
    known: HashMap<String, OnQueue<CBPeripheral>>,
}

impl Search {
    /// Takes one step for `event`; returns whether a window was found.
    fn handle(&mut self, event: Event) -> bool {
        match event {
            Event::Discovered(id, peripheral) if !self.known.contains_key(&id) => {
                self.known.insert(id, peripheral.clone());
                if let Ok(mut list) = self.central.peripherals.lock() {
                    list.push(peripheral.clone());
                }
                self.central.on_queue(move |manager, _| {
                    // SAFETY: on the manager's queue.
                    #[allow(unsafe_code)]
                    unsafe {
                        manager.connectPeripheral_options(peripheral.get(), None);
                    }
                });
            }
            Event::Connected(id) => {
                if let Some(peripheral) = self.known.get(&id).cloned() {
                    self.central.on_queue(move |_, delegate| {
                        let services = NSArray::from_retained_slice(&[uuid(SERVICE_UUID)]);
                        // SAFETY: on the manager's queue; the delegate outlives the peripheral's use.
                        #[allow(unsafe_code)]
                        unsafe {
                            peripheral
                                .get()
                                .setDelegate(Some(ProtocolObject::from_ref(delegate)));
                            peripheral.get().discoverServices(Some(&services));
                        }
                    });
                }
            }
            Event::Services(id) => {
                if let Some(peripheral) = self.known.get(&id).cloned() {
                    self.central
                        .on_queue(move |_, _| discover_info(peripheral.get()));
                }
            }
            Event::Characteristics(id, service) => {
                if let Some(peripheral) = self.known.get(&id).cloned() {
                    self.central
                        .on_queue(move |_, _| read_info(peripheral.get(), service.get()));
                }
            }
            Event::Value(id, bytes) => {
                if let (Ok((announcement, psm)), Some(peripheral)) =
                    (from_window_info(&bytes), self.known.get(&id))
                {
                    let window = Window {
                        central: Arc::clone(&self.central),
                        peripheral: peripheral.clone(),
                        psm,
                    };
                    self.found.push(FoundWindow {
                        announcement,
                        route: Route::Direct {
                            via: Via::Bluetooth,
                            connector: Arc::new(window),
                        },
                    });
                    return true;
                }
            }
            Event::Lost(id) => {
                self.known.remove(&id);
            }
            Event::Discovered(..) | Event::State(_) => {}
        }
        false
    }
}

fn discover_info(peripheral: &CBPeripheral) {
    // SAFETY: on the manager's queue, with a connected peripheral.
    #[allow(unsafe_code)]
    unsafe {
        let Some(services) = peripheral.services() else {
            return;
        };
        let wanted = NSArray::from_retained_slice(&[uuid(WINDOW_INFO_UUID)]);
        for service in &services {
            if service.UUID() == uuid(SERVICE_UUID) {
                peripheral.discoverCharacteristics_forService(Some(&wanted), &service);
            }
        }
    }
}

fn read_info(peripheral: &CBPeripheral, service: &CBService) {
    // SAFETY: on the manager's queue, with a connected peripheral and one of its services.
    #[allow(unsafe_code)]
    unsafe {
        let Some(characteristics) = service.characteristics() else {
            return;
        };
        for characteristic in &characteristics {
            if characteristic.UUID() == uuid(WINDOW_INFO_UUID) {
                peripheral.readValueForCharacteristic(&characteristic);
            }
        }
    }
}

async fn wait_until_usable(received: &mut mpsc::UnboundedReceiver<Event>) -> Result<(), BleError> {
    loop {
        match tokio::time::timeout(STATE_WAIT, received.recv()).await {
            Ok(Some(Event::State(state))) => {
                if let Some(result) = usable(state) {
                    return result;
                }
            }
            Ok(Some(_)) => {}
            _ => return Err(BleError::Unsupported),
        }
    }
}
