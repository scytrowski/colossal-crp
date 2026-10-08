use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::read_encoded_ushort_array;

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
    object_references: Vec<Vec<u8>>,
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
        let mut object_references = Vec::new();
        if version >= 244 {
            object_references.reserve(3);
            for _ in 0..3 {
                let start = reader.offset();
                skip_object_reference(&mut reader)?;
                object_references.push(reader.consumed_bytes()[start..].to_vec());
            }
        }
        let consumed = reader.offset();
        Ok((
            Self {
                raw_heights,
                block_height_deltas,
                dirt_buffer,
                object_references,
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

    /// Returns the three version-244+ object references in serialized order.
    /// Each entry contains its exact DataSerializer header bytes.
    pub fn object_references(&self) -> &[Vec<u8>] {
        &self.object_references
    }

    /// Returns the original serialized object body unchanged.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }
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
    use super::{TERRAIN_HEIGHT_COUNT, TerrainManagerData};
    use crate::data_serializer::write_unique_string;
    use crate::encoded_array::write_encoded_ushort_array;

    #[test]
    fn terrain_prefix_retains_versioned_object_reference_headers() {
        let mut bytes = Vec::new();
        write_encoded_ushort_array(&mut bytes, &vec![0; TERRAIN_HEIGHT_COUNT]);
        bytes.extend_from_slice(&0_i32.to_be_bytes());
        bytes.push(0xff);
        bytes.extend_from_slice(&[0xfe, 0xfe]);
        write_unique_string(&mut bytes, "TerrainToolGuide").unwrap();
        bytes.extend_from_slice(&[0, 0, 7]);
        let original = bytes.clone();

        let (terrain, consumed) = TerrainManagerData::decode_prefix(244, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(
            terrain.object_references(),
            [
                vec![0xff],
                {
                    let mut reference = vec![0xfe, 0xfe];
                    write_unique_string(&mut reference, "TerrainToolGuide").unwrap();
                    reference
                },
                vec![0, 0, 7],
            ]
        );
        assert_eq!(terrain.encode(), original);

        let (legacy, legacy_consumed) = TerrainManagerData::decode_prefix(243, &bytes).unwrap();
        assert!(legacy.object_references().is_empty());
        assert!(legacy_consumed < bytes.len());
    }
}
