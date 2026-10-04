use crate::CrpError;

const NULL_MARKER: u8 = 0xff;
const NEW_OBJECT_MARKER: u8 = 0xfe;
const COMPACT_STRING_FLAG: u32 = 0x80_0000;
const MAX_STRING_CHARS: usize = 16 * 1024 * 1024;

/// Framing information for one isolated DataSerializer stream.
///
/// The root object's fields remain opaque and are preserved byte-for-byte.
/// `decode` requires a slice containing exactly one stream, including its
/// format version; it does not locate boundaries inside a combined `_Data`
/// payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSerializerStream {
    /// The inner DataSerializer format version, independent of package version.
    pub format_version: u32,
    /// Assembly-qualified type name of the root object.
    pub root_type: String,
    bytes: Vec<u8>,
}

impl DataSerializerStream {
    /// Reads the version and new-root type header of one isolated stream.
    ///
    /// The remaining object graph is retained verbatim because manager and
    /// metadata field schemas are not yet fully decoded. Truncated headers,
    /// malformed strings, null roots, and root references are rejected.
    pub fn decode(bytes: &[u8]) -> Result<Self, CrpError> {
        let mut reader = Reader::new(bytes);
        let format_version = reader.read_u32_be()?;
        if reader.read_u8()? != NEW_OBJECT_MARKER {
            return Err(reader.invalid("expected a new root object marker (0xFE)"));
        }
        if reader.read_u8()? != NEW_OBJECT_MARKER {
            return Err(reader.invalid("expected a new shared-type marker (0xFE)"));
        }
        let root_type = reader
            .read_unique_string()?
            .ok_or_else(|| reader.invalid("root object's shared type name cannot be null"))?;
        Ok(Self {
            format_version,
            root_type,
            bytes: bytes.to_vec(),
        })
    }

    /// Returns the original stream bytes for lossless rewriting.
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn invalid(&self, reason: impl Into<String>) -> CrpError {
        CrpError::InvalidAssetPayload {
            name: "DataSerializer stream".to_owned(),
            offset: self.offset,
            reason: reason.into(),
        }
    }

    fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], CrpError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| self.invalid("stream offset overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| self.invalid(format!("truncated stream; need {count} bytes")))?;
        self.offset = end;
        Ok(bytes)
    }

    fn read_u8(&mut self) -> Result<u8, CrpError> {
        Ok(self.read_bytes(1)?[0])
    }

    fn read_u32_be(&mut self) -> Result<u32, CrpError> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_unique_string(&mut self) -> Result<Option<String>, CrpError> {
        let first = self.read_u8()?;
        if first == NULL_MARKER {
            return Ok(None);
        }
        let second = self.read_u8()?;
        let third = self.read_u8()?;
        let encoded_length = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);
        let compact = encoded_length & COMPACT_STRING_FLAG != 0;
        let char_count = usize::try_from(encoded_length & !COMPACT_STRING_FLAG)
            .map_err(|_| self.invalid("string length does not fit this platform"))?;
        if char_count > MAX_STRING_CHARS {
            return Err(self.invalid("unique string exceeds the 16 Mi character limit"));
        }
        if compact {
            let deltas = self.read_bytes(char_count)?;
            let mut previous = 128_u8;
            let mut value = String::with_capacity(char_count);
            for delta in deltas {
                previous = previous.wrapping_add(*delta);
                value.push(char::from(previous));
            }
            Ok(Some(value))
        } else {
            let byte_count = char_count
                .checked_mul(2)
                .ok_or_else(|| self.invalid("wide string byte length overflow"))?;
            let deltas = self.read_bytes(byte_count)?;
            let mut previous = 128_u16;
            let mut value = String::with_capacity(char_count);
            for pair in deltas.chunks_exact(2) {
                let delta = u16::from_be_bytes([pair[0], pair[1]]);
                previous = previous.wrapping_add(delta);
                let character = char::from_u32(u32::from(previous)).ok_or_else(|| {
                    self.invalid("wide string contains an invalid UTF-16 code unit")
                })?;
                value.push(character);
            }
            Ok(Some(value))
        }
    }
}
