use crate::CrpError;
use crate::data_serializer::Reader;

const TERRAIN_HEIGHT_COUNT: usize = 1_168_561;

/// Decoded height data from a queued `TerrainManager+Data` object.
///
/// Terrain grids have a fixed 1,168,561 entries in the inspected game DLL.
/// The original object body is retained and returned by [`encode`](Self::encode)
/// to preserve object references and exact encoded-array bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainManagerData {
    raw_heights: Vec<u16>,
    block_height_deltas: Vec<u16>,
    dirt_buffer: i32,
    serialized_body: Vec<u8>,
}

impl TerrainManagerData {
    /// Decodes one `TerrainManager+Data` object body and returns its byte length.
    ///
    /// The version gates match `TerrainManager+Data.Deserialize`: block-height
    /// deltas start at version 96, the dirt buffer at 243, and three object
    /// references at 244. Malformed or truncated encoded arrays return errors.
    pub fn decode_prefix(version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let raw_heights = read_encoded_ushort_array(&mut reader, TERRAIN_HEIGHT_COUNT)?;
        let mut block_height_deltas = vec![0; TERRAIN_HEIGHT_COUNT];
        if version >= 96 {
            let nonzero_count = raw_heights.iter().filter(|height| **height != 0).count();
            let sparse_deltas = read_encoded_ushort_array(&mut reader, nonzero_count)?;
            let mut delta_iter = sparse_deltas.into_iter();
            for (height, delta) in raw_heights.iter().zip(&mut block_height_deltas) {
                if *height != 0 {
                    *delta = delta_iter
                        .next()
                        .ok_or_else(|| reader.invalid("missing sparse block-height delta"))?;
                }
            }
        }
        let dirt_buffer = if version >= 243 {
            reader.read_i32_be()?
        } else {
            524_288
        };
        if version >= 244 {
            for _ in 0..3 {
                skip_object_reference(&mut reader)?;
            }
        }
        let consumed = reader.offset();
        Ok((
            Self {
                raw_heights,
                block_height_deltas,
                dirt_buffer,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the raw terrain heights as unsigned 16-bit values.
    pub fn raw_heights(&self) -> &[u16] {
        &self.raw_heights
    }

    /// Returns block-height deltas, with zero for positions whose raw height is zero.
    pub fn block_height_deltas(&self) -> &[u16] {
        &self.block_height_deltas
    }

    /// Returns the dirt buffer value, including its version default when absent.
    pub fn dirt_buffer(&self) -> i32 {
        self.dirt_buffer
    }

    /// Returns the original serialized object body unchanged.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }
}

pub(crate) fn read_encoded_ushort_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<u16>, CrpError> {
    let mut values = Vec::with_capacity(count);
    let mut previous_value = 0_u16;
    let mut previous_delta = 0_i16;
    let mut full_count = 0_usize;
    let mut close_count = 0_usize;
    let mut same_count = 0_usize;

    while values.len() < count {
        if same_count > 0 {
            previous_value = previous_value.wrapping_add(previous_delta as u16);
            same_count -= 1;
        } else if close_count > 0 {
            let delta_adjustment = reader.read_u8()? as i8;
            previous_delta = previous_delta.wrapping_add(i16::from(delta_adjustment));
            previous_value = previous_value.wrapping_add(previous_delta as u16);
            close_count -= 1;
        } else if full_count > 0 {
            let value = reader.read_u16_be()?;
            previous_delta = value.wrapping_sub(previous_value) as i16;
            previous_value = value;
            full_count -= 1;
        } else {
            let control = reader.read_u8()?;
            if control >= 0x80 {
                same_count = usize::from(control - 0x7f);
            } else if control >= 0x40 {
                close_count = usize::from(control - 0x3f);
            } else {
                full_count = usize::from(control) + 1;
            }
            continue;
        }
        values.push(previous_value);
    }
    Ok(values)
}

fn skip_object_reference(reader: &mut Reader<'_>) -> Result<(), CrpError> {
    let marker = reader.read_u8()?;
    match marker {
        0xff => Ok(()),
        0xfe => {
            let type_marker = reader.read_u8()?;
            if type_marker == 0xfe {
                reader
                    .read_unique_string()?
                    .ok_or_else(|| reader.invalid("object shared-type name cannot be null"))?;
            } else {
                reader.read_u8()?;
            }
            Ok(())
        }
        _ => {
            reader.read_u8()?;
            reader.read_u8()?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::read_encoded_ushort_array;
    use crate::data_serializer::Reader;

    #[test]
    fn ushort_array_decodes_literal_repeat_and_delta_adjustment_runs() {
        let literal_and_repeat = [0x00, 0x00, 0x05, 0x81];
        let mut reader = Reader::new(&literal_and_repeat);
        assert_eq!(
            read_encoded_ushort_array(&mut reader, 3).expect("valid encoded values"),
            [5, 10, 15]
        );

        let close_run = [0x00, 0x10, 0x00, 0x40, 0x01];
        let mut reader = Reader::new(&close_run);
        assert_eq!(
            read_encoded_ushort_array(&mut reader, 2).expect("valid adjusted delta"),
            [0x1000, 0x2001]
        );
    }

    #[test]
    fn ushort_array_rejects_truncated_literal_values() {
        let mut reader = Reader::new(&[0x01, 0x12]);
        assert!(read_encoded_ushort_array(&mut reader, 2).is_err());
    }
}
