//! The few pieces of the protobuf wire format the migration payload needs: varints and length-delimited
//! fields, plus skipping anything else.

/// Wire types.
pub(super) const VARINT: u8 = 0;
pub(super) const FIXED64: u8 = 1;
pub(super) const LENGTH_DELIMITED: u8 = 2;
pub(super) const FIXED32: u8 = 5;

const MAX_VARINT_BYTES: usize = 10;

/// The data ended early or is not well formed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Malformed;

/// Reads fields from a protobuf message.
pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

/// One field's value.
pub(super) enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Other,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    /// The next field number and value, or `None` at the end.
    pub(super) fn next_field(&mut self) -> Result<Option<(u64, Value<'a>)>, Malformed> {
        if self.at == self.bytes.len() {
            return Ok(None);
        }
        let key = self.varint()?;
        let field = key >> 3;
        let wire = u8::try_from(key & 0x07).map_err(|_| Malformed)?;
        let value = match wire {
            VARINT => Value::Varint(self.varint()?),
            LENGTH_DELIMITED => {
                let len = usize::try_from(self.varint()?).map_err(|_| Malformed)?;
                Value::Bytes(self.take(len)?)
            }
            FIXED64 => {
                self.take(8)?;
                Value::Other
            }
            FIXED32 => {
                self.take(4)?;
                Value::Other
            }
            _ => return Err(Malformed),
        };
        if field == 0 {
            return Err(Malformed);
        }
        Ok(Some((field, value)))
    }

    fn varint(&mut self) -> Result<u64, Malformed> {
        let mut value: u64 = 0;
        for i in 0..MAX_VARINT_BYTES {
            let byte = *self.bytes.get(self.at).ok_or(Malformed)?;
            self.at += 1;
            value |= u64::from(byte & 0x7f) << (7 * i);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(Malformed)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], Malformed> {
        let end = self.at.checked_add(len).ok_or(Malformed)?;
        let slice = self.bytes.get(self.at..end).ok_or(Malformed)?;
        self.at = end;
        Ok(slice)
    }
}

/// Appends a varint field.
pub(super) fn put_varint_field(out: &mut Vec<u8>, field: u64, value: u64) {
    put_varint(out, field << 3 | u64::from(VARINT));
    put_varint(out, value);
}

/// Appends a length-delimited field.
pub(super) fn put_bytes_field(out: &mut Vec<u8>, field: u64, bytes: &[u8]) {
    put_varint(out, field << 3 | u64::from(LENGTH_DELIMITED));
    put_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        // The low seven bits plus the continuation bit fit in a byte.
        #[allow(clippy::cast_possible_truncation)]
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    #[allow(clippy::cast_possible_truncation)]
    out.push(value as u8);
}
