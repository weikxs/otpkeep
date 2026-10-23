use super::{DeviceTag, EntryId, Hlc};

fn hlc(wall_ms: u64, logical: u32, device: u8) -> Hlc {
    Hlc {
        wall_ms,
        logical,
        device: DeviceTag([device; 8]),
    }
}

#[test]
fn timestamps_order_by_wall_clock_then_counter_then_device() {
    assert!(hlc(1, 9, 9) < hlc(2, 0, 0));
    assert!(hlc(1, 0, 9) < hlc(1, 1, 0));
    assert!(hlc(1, 1, 0) < hlc(1, 1, 1));
    assert_eq!(hlc(5, 5, 5), hlc(5, 5, 5));
}

#[test]
fn random_entry_ids_are_unique_version_4_uuids() {
    let first = EntryId::random();
    let second = EntryId::random();

    assert_ne!(first, second);
    // RFC 9562: version 4 in the high nibble of byte 6, variant 10xx in byte 8.
    assert_eq!(first.0[6] >> 4, 4);
    assert_eq!(first.0[8] >> 6, 0b10);
}

#[test]
fn entry_ids_print_as_hyphenated_uuids() {
    let id = EntryId([0xab; 16]);

    assert_eq!(id.to_string(), "abababab-abab-abab-abab-abababababab");
}
