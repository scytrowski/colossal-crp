use crate::CrpError;

const MAX_STRING_BYTES: u32 = 16 * 1024 * 1024;

pub(crate) struct PayloadReader<'a> {
    asset_name: &'a str,
    data: &'a [u8],
    offset: usize,
}

impl<'a> PayloadReader<'a> {
    pub(crate) fn new(asset_name: &'a str, data: &'a [u8]) -> Self {
        Self {
            asset_name,
            data,
            offset: 0,
        }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.data.len() - self.offset
    }

    pub(crate) fn offset(&self) -> usize {
        self.offset
    }

    pub(crate) fn invalid(&self, reason: impl Into<String>) -> CrpError {
        CrpError::InvalidAssetPayload {
            name: self.asset_name.to_owned(),
            offset: self.offset,
            reason: reason.into(),
        }
    }

    pub(crate) fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], CrpError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| self.invalid("payload offset overflow"))?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or_else(|| self.invalid(format!("truncated payload; need {count} bytes")))?;
        self.offset = end;
        Ok(bytes)
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8, CrpError> {
        Ok(self.read_bytes(1)?[0])
    }

    pub(crate) fn read_bool(&mut self) -> Result<bool, CrpError> {
        Ok(self.read_u8()? != 0)
    }

    pub(crate) fn read_i32_le(&mut self) -> Result<i32, CrpError> {
        let bytes = self.read_bytes(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(crate) fn read_u32_le(&mut self) -> Result<u32, CrpError> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(crate) fn read_u64_le(&mut self) -> Result<u64, CrpError> {
        let bytes = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    pub(crate) fn read_f32_le(&mut self) -> Result<f32, CrpError> {
        Ok(f32::from_bits(self.read_u32_le()?))
    }

    pub(crate) fn read_string(&mut self) -> Result<String, CrpError> {
        let byte_len = self.read_7bit_encoded_u32()?;
        if byte_len > MAX_STRING_BYTES {
            return Err(self.invalid("string exceeds the 16 MiB payload limit"));
        }
        let allocation_size = usize::try_from(byte_len)
            .map_err(|_| self.invalid("string length does not fit this platform"))?;
        let start = self.offset;
        let bytes = self.read_bytes(allocation_size)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| CrpError::InvalidAssetPayload {
            name: self.asset_name.to_owned(),
            offset: start,
            reason: "string is not valid UTF-8".to_owned(),
        })
    }

    pub(crate) fn read_7bit_encoded_u32(&mut self) -> Result<u32, CrpError> {
        let mut value = 0_u32;
        for shift in (0..35).step_by(7) {
            let byte = self.read_u8()?;
            if shift == 28 && byte > 0x0f {
                return Err(self.invalid("invalid 7-bit encoded string length"));
            }
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(self.invalid("unterminated 7-bit encoded string length"))
    }

    pub(crate) fn read_remaining(&mut self) -> &'a [u8] {
        let remaining = &self.data[self.offset..];
        self.offset = self.data.len();
        remaining
    }
}

pub(crate) fn write_bool(output: &mut Vec<u8>, value: bool) {
    output.push(u8::from(value));
}

pub(crate) fn write_i32_le(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_u32_le(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_u64_le(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_f32_le(output: &mut Vec<u8>, value: f32) {
    write_u32_le(output, value.to_bits());
}

pub(crate) fn write_string(output: &mut Vec<u8>, value: &str) -> Result<(), CrpError> {
    let byte_len = u32::try_from(value.len())
        .map_err(|_| CrpError::ArithmeticOverflow("asset string length"))?;
    write_7bit_encoded_u32(output, byte_len)?;
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn write_7bit_encoded_u32(output: &mut Vec<u8>, mut value: u32) -> Result<(), CrpError> {
    while value >= 0x80 {
        let low_bits = u8::try_from(value & 0x7f)
            .map_err(|_| CrpError::ArithmeticOverflow("asset string length encoding"))?;
        output.push(low_bits | 0x80);
        value >>= 7;
    }
    let final_byte = u8::try_from(value)
        .map_err(|_| CrpError::ArithmeticOverflow("asset string length encoding"))?;
    output.push(final_byte);
    Ok(())
}
