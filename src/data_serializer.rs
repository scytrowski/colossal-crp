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
    pub(crate) format_version: u32,
    /// Assembly-qualified type name of the root object.
    pub(crate) root_type: String,
    /// Opaque serialized fields following the root object's type header.
    pub(crate) root_data: Vec<u8>,
    /// Byte length through the root type header.
    pub(crate) header_len: usize,
    bytes: Vec<u8>,
}

impl DataSerializerStream {
    /// Returns this stream's inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the assembly-qualified type name of the root object.
    pub fn root_type(&self) -> &str {
        &self.root_type
    }

    /// Returns the opaque bytes following the root type header.
    pub fn root_data(&self) -> &[u8] {
        &self.root_data
    }

    /// Returns the byte length through the root type header.
    pub fn header_len(&self) -> usize {
        self.header_len
    }

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
        let root_data = bytes[reader.offset..].to_vec();
        Ok(Self {
            format_version,
            root_type,
            root_data,
            header_len: reader.offset,
            bytes: bytes.to_vec(),
        })
    }

    /// Builds a stream with a new root object and opaque serialized root data.
    ///
    /// The root type is emitted as a new shared type. Its name uses the
    /// DataSerializer compact or wide unique-string form, and `root_data` is
    /// copied without interpretation.
    pub fn from_parts(
        format_version: u32,
        root_type: String,
        root_data: Vec<u8>,
    ) -> Result<Self, CrpError> {
        let mut bytes = format_version.to_be_bytes().to_vec();
        bytes.extend_from_slice(&[NEW_OBJECT_MARKER, NEW_OBJECT_MARKER]);
        write_unique_string(&mut bytes, &root_type)?;
        let header_len = bytes.len();
        bytes.extend_from_slice(&root_data);
        Ok(Self {
            format_version,
            root_type,
            root_data,
            header_len,
            bytes,
        })
    }

    /// Returns the original stream bytes for lossless rewriting.
    pub fn encode(&self) -> Vec<u8> {
        self.bytes.clone()
    }
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(crate) fn invalid(&self, reason: impl Into<String>) -> CrpError {
        CrpError::InvalidAssetPayload {
            name: "DataSerializer stream".to_owned(),
            offset: self.offset,
            reason: reason.into(),
        }
    }

    pub(crate) fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], CrpError> {
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

    pub(crate) fn read_u8(&mut self) -> Result<u8, CrpError> {
        Ok(self.read_bytes(1)?[0])
    }

    pub(crate) fn read_u32_be(&mut self) -> Result<u32, CrpError> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(crate) fn read_u16_be(&mut self) -> Result<u16, CrpError> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    pub(crate) fn read_i16_be(&mut self) -> Result<i16, CrpError> {
        Ok(self.read_u16_be()? as i16)
    }

    pub(crate) fn read_f32_be(&mut self) -> Result<f32, CrpError> {
        Ok(f32::from_bits(self.read_u32_be()?))
    }

    pub(crate) fn read_i32_be(&mut self) -> Result<i32, CrpError> {
        let bytes = self.read_bytes(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(crate) fn read_unique_string(&mut self) -> Result<Option<String>, CrpError> {
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
            let mut code_units = Vec::with_capacity(char_count);
            for pair in deltas.chunks_exact(2) {
                let delta = u16::from_be_bytes([pair[0], pair[1]]);
                previous = previous.wrapping_add(delta);
                code_units.push(previous);
            }
            String::from_utf16(&code_units)
                .map(Some)
                .map_err(|_| self.invalid("wide string contains invalid UTF-16"))
        }
    }

    pub(crate) fn offset(&self) -> usize {
        self.offset
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    pub(crate) fn consumed_bytes(&self) -> &'a [u8] {
        &self.bytes[..self.offset]
    }

    pub(crate) fn remaining_bytes(&self) -> &'a [u8] {
        &self.bytes[self.offset..]
    }
}

pub(crate) fn write_unique_string(output: &mut Vec<u8>, value: &str) -> Result<(), CrpError> {
    let compact = value.chars().all(|character| u32::from(character) < 256);
    let units = if compact {
        value
            .chars()
            .map(|character| u32::from(character))
            .collect::<Vec<_>>()
    } else {
        value.encode_utf16().map(u32::from).collect::<Vec<_>>()
    };
    if units.len() > MAX_STRING_CHARS {
        return Err(CrpError::InvalidAssetPayload {
            name: "DataSerializer stream".to_owned(),
            offset: output.len(),
            reason: "unique string exceeds the 16 Mi character limit".to_owned(),
        });
    }
    let length = u32::try_from(units.len())
        .map_err(|_| CrpError::ArithmeticOverflow("DataSerializer string length"))?;
    let encoded_length = if compact {
        length | COMPACT_STRING_FLAG
    } else {
        length
    };
    if encoded_length > 0xff_ffff {
        return Err(CrpError::ArithmeticOverflow(
            "DataSerializer 24-bit string length",
        ));
    }
    output.extend_from_slice(&[
        (encoded_length >> 16) as u8,
        (encoded_length >> 8) as u8,
        encoded_length as u8,
    ]);
    if compact {
        let mut previous = 128_u32;
        for unit in units {
            output.push(unit.wrapping_sub(previous) as u8);
            previous = unit;
        }
    } else {
        let mut previous = 128_u32;
        for unit in units {
            let delta = unit.wrapping_sub(previous) as u16;
            output.extend_from_slice(&delta.to_be_bytes());
            previous = unit;
        }
    }
    Ok(())
}

pub(crate) fn write_u24_be(output: &mut Vec<u8>, value: u32) -> Result<(), CrpError> {
    if value >= 0xff_0000 {
        return Err(CrpError::ArithmeticOverflow("DataSerializer 24-bit count"));
    }
    output.extend_from_slice(&[
        u8::try_from(value >> 16)
            .map_err(|_| CrpError::ArithmeticOverflow("DataSerializer 24-bit count"))?,
        u8::try_from((value >> 8) & 0xff)
            .map_err(|_| CrpError::ArithmeticOverflow("DataSerializer 24-bit count"))?,
        u8::try_from(value & 0xff)
            .map_err(|_| CrpError::ArithmeticOverflow("DataSerializer 24-bit count"))?,
    ]);
    Ok(())
}
