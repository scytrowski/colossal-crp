use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_ushort_array, write_encoded_ushort_array};
use std::ops::Range;

const TERRAIN_HEIGHT_COUNT: usize = 1_168_561;

/// Decoded height data from a queued `TerrainManager+Data` object.
///
/// Terrain grids have a fixed 1,168,561 entries in the inspected game DLL.
/// The original object body is retained and returned by [`encode`](Self::encode)
/// to preserve object references and exact encoded-array bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainManagerData {
    version: u32,
    raw_heights: Vec<u16>,
    block_height_deltas: Vec<u16>,
    dirt_buffer: i32,
    object_references: Vec<Vec<u8>>,
    raw_heights_range: Range<usize>,
    block_height_deltas_range: Option<Range<usize>>,
    dirt_buffer_range: Option<Range<usize>>,
    serialized_body: Vec<u8>,
}

/// Editor for the terrain height arrays and dirt-buffer value.
///
/// Setting a raw height to zero clears its sparse block-height delta. Changes
/// to either encoded array can resize the body; queued object references remain
/// unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainManagerEditor {
    original: TerrainManagerData,
    raw_heights: Vec<u16>,
    block_height_deltas: Vec<u16>,
    dirt_buffer: i32,
}

impl TerrainManagerData {
    /// Decodes one `TerrainManager+Data` object body and returns its byte length.
    ///
    /// The version gates match `TerrainManager+Data.Deserialize`: block-height
    /// deltas start at version 96, the dirt buffer at 243, and three object
    /// references at 244. Malformed or truncated encoded arrays return errors.
    pub fn decode_prefix(version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let raw_heights_start = reader.offset();
        let raw_heights = read_encoded_ushort_array(&mut reader, TERRAIN_HEIGHT_COUNT)?;
        let raw_heights_range = raw_heights_start..reader.offset();
        let mut block_height_deltas = vec![0; TERRAIN_HEIGHT_COUNT];
        let block_height_deltas_range = if version >= 96 {
            let nonzero_count = raw_heights.iter().filter(|height| **height != 0).count();
            let start = reader.offset();
            let sparse_deltas = read_encoded_ushort_array(&mut reader, nonzero_count)?;
            let mut delta_iter = sparse_deltas.into_iter();
            for (height, delta) in raw_heights.iter().zip(&mut block_height_deltas) {
                if *height != 0 {
                    *delta = delta_iter
                        .next()
                        .ok_or_else(|| reader.invalid("missing sparse block-height delta"))?;
                }
            }
            Some(start..reader.offset())
        } else {
            None
        };
        let (dirt_buffer, dirt_buffer_range) = if version >= 243 {
            let start = reader.offset();
            (reader.read_i32_be()?, Some(start..reader.offset()))
        } else {
            (524_288, None)
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
                version,
                raw_heights,
                block_height_deltas,
                dirt_buffer,
                object_references,
                raw_heights_range,
                block_height_deltas_range,
                dirt_buffer_range,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.version
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

    /// Creates an editor for terrain values represented in the source body.
    pub fn into_editor(self) -> TerrainManagerEditor {
        TerrainManagerEditor {
            raw_heights: self.raw_heights.clone(),
            block_height_deltas: self.block_height_deltas.clone(),
            dirt_buffer: self.dirt_buffer,
            original: self,
        }
    }

    /// Returns the original serialized object body unchanged.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }
}

impl TerrainManagerEditor {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.original.version
    }

    /// Returns the current raw terrain heights.
    pub fn raw_heights(&self) -> &[u16] {
        &self.raw_heights
    }

    /// Returns current sparse block-height differences, with zero at raw-height
    /// cells whose value is zero.
    pub fn block_height_deltas(&self) -> &[u16] {
        &self.block_height_deltas
    }

    /// Returns the current dirt-buffer value, including its version default.
    pub fn dirt_buffer(&self) -> i32 {
        self.dirt_buffer
    }

    /// Sets one raw terrain height by zero-based grid cell.
    ///
    /// Setting a cell to zero clears its sparse block-height delta. Invalid cell
    /// indexes return an error.
    pub fn set_raw_height(&mut self, cell: usize, value: u16) -> Result<(), CrpError> {
        let height = self
            .raw_heights
            .get_mut(cell)
            .ok_or_else(|| invalid_terrain(cell, "raw-height cell is outside the terrain grid"))?;
        *height = value;
        if value == 0 {
            self.block_height_deltas[cell] = 0;
        }
        Ok(())
    }

    /// Sets one serialized block-height difference from inner version 96.
    ///
    /// The cell must have a nonzero raw height because only those deltas are
    /// stored. Earlier versions have no serialized delta array.
    pub fn set_block_height_delta(&mut self, cell: usize, value: u16) -> Result<(), CrpError> {
        if self.original.version < 96 {
            return Err(invalid_terrain(
                cell,
                "block-height deltas are unavailable before inner version 96",
            ));
        }
        let raw_height = self.raw_heights.get(cell).ok_or_else(|| {
            invalid_terrain(cell, "block-height cell is outside the terrain grid")
        })?;
        if *raw_height == 0 {
            return Err(invalid_terrain(
                cell,
                "block-height delta requires a nonzero raw height",
            ));
        }
        self.block_height_deltas[cell] = value;
        Ok(())
    }

    /// Sets the stored dirt-buffer value from inner version 243.
    pub fn set_dirt_buffer(&mut self, value: i32) -> Result<(), CrpError> {
        if self.original.dirt_buffer_range.is_none() {
            return Err(invalid_terrain(
                0,
                "dirt buffer is unavailable before inner version 243",
            ));
        }
        self.dirt_buffer = value;
        Ok(())
    }

    /// Writes changed arrays and scalar fields while retaining unchanged source
    /// bytes, including all three queued object-reference headers.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.raw_heights.len() != self.original.raw_heights.len()
            || self.block_height_deltas.len() != self.original.block_height_deltas.len()
        {
            return Err(invalid_terrain(0, "terrain grid length changed"));
        }
        let mut bytes = self.original.serialized_body.clone();
        let mut replacements = Vec::new();
        if self.raw_heights != self.original.raw_heights {
            let mut encoded = Vec::new();
            write_encoded_ushort_array(&mut encoded, &self.raw_heights);
            replacements.push((self.original.raw_heights_range.clone(), encoded));
        }

        if self.original.version >= 96 {
            let current_sparse_deltas = sparse_deltas(&self.raw_heights, &self.block_height_deltas);
            let original_sparse_deltas = sparse_deltas(
                &self.original.raw_heights,
                &self.original.block_height_deltas,
            );
            if current_sparse_deltas != original_sparse_deltas {
                let range = self
                    .original
                    .block_height_deltas_range
                    .as_ref()
                    .ok_or_else(|| invalid_terrain(0, "sparse block-height array is missing"))?;
                let mut encoded = Vec::new();
                write_encoded_ushort_array(&mut encoded, &current_sparse_deltas);
                replacements.push((range.clone(), encoded));
            }
        } else if self.block_height_deltas != self.original.block_height_deltas {
            return Err(invalid_terrain(
                0,
                "block-height deltas cannot change before inner version 96",
            ));
        }

        if self.dirt_buffer != self.original.dirt_buffer {
            let range =
                self.original.dirt_buffer_range.as_ref().ok_or_else(|| {
                    invalid_terrain(0, "dirt buffer is unavailable in this version")
                })?;
            replacements.push((range.clone(), self.dirt_buffer.to_be_bytes().to_vec()));
        }

        replacements.sort_by_key(|(range, _)| {
            (std::cmp::Reverse(range.start), std::cmp::Reverse(range.end))
        });
        let mut previous_start = bytes.len();
        for (range, replacement) in replacements {
            if range.end > previous_start {
                return Err(invalid_terrain(range.start, "terrain field ranges overlap"));
            }
            validate_range(&bytes, &range, range.len(), "edited terrain field")?;
            bytes.splice(range.clone(), replacement);
            previous_start = range.start;
        }
        Ok(bytes)
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original.serialized_body
    }
}

fn sparse_deltas(raw_heights: &[u16], deltas: &[u16]) -> Vec<u16> {
    raw_heights
        .iter()
        .zip(deltas)
        .filter_map(|(height, delta)| (*height != 0).then_some(*delta))
        .collect()
}

fn validate_range(
    source: &[u8],
    range: &Range<usize>,
    expected_len: usize,
    field: &str,
) -> Result<(), CrpError> {
    if range.start > range.end || range.end > source.len() || range.len() != expected_len {
        return Err(invalid_terrain(
            range.start,
            format!("{field} byte range does not match its source data"),
        ));
    }
    Ok(())
}

fn invalid_terrain(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "TerrainManager+Data".to_owned(),
        offset,
        reason: reason.into(),
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

    fn empty_body(version: u32) -> Vec<u8> {
        let mut bytes = vec![0xff; 9_129];
        bytes.push(0xb0); // 46 remaining zero heights.
        if version >= 243 {
            bytes.extend_from_slice(&524_288_i32.to_be_bytes());
        }
        if version >= 244 {
            bytes.extend_from_slice(&[0xff; 3]);
        }
        bytes
    }

    #[test]
    fn terrain_prefix_retains_versioned_object_reference_headers() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0xff; 9_129]);
        bytes.push(0xb0);
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

    #[test]
    fn terrain_editor_inserts_height_and_sparse_delta_with_exact_bytes() {
        let bytes = empty_body(244);
        let (terrain, used) = TerrainManagerData::decode_prefix(244, &bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(terrain.raw_heights().len(), TERRAIN_HEIGHT_COUNT);
        assert!(terrain.raw_heights().iter().all(|height| *height == 0));

        let mut editor = terrain.into_editor();
        editor.set_raw_height(0, 2).unwrap();
        editor.set_block_height_delta(0, 0x1234).unwrap();
        editor.set_dirt_buffer(99).unwrap();

        let mut expected = vec![2, 0, 2, 0, 0, 0, 0];
        expected.extend_from_slice(&[0xff; 9_129]);
        expected.push(0xad); // 46 final repeated zero heights.
        expected.extend_from_slice(&[0, 0x12, 0x34]); // One sparse UShort delta.
        expected.extend_from_slice(&99_i32.to_be_bytes());
        expected.extend_from_slice(&[0xff; 3]); // The three source object references.
        assert_eq!(editor.encode().unwrap(), expected);

        let (updated, used) = TerrainManagerData::decode_prefix(244, &expected).unwrap();
        assert_eq!(used, expected.len());
        assert_eq!(updated.raw_heights()[0], 2);
        assert_eq!(updated.block_height_deltas()[0], 0x1234);
        assert_eq!(updated.dirt_buffer(), 99);
        assert_eq!(updated.object_references().len(), 3);
        assert!(
            updated
                .object_references()
                .iter()
                .all(|reference| reference == &[0xff])
        );
    }

    #[test]
    fn terrain_editor_checks_legacy_gates_and_cell_bounds() {
        let (terrain, _) = TerrainManagerData::decode_prefix(95, &empty_body(95)).unwrap();
        let mut editor = terrain.into_editor();
        assert!(editor.set_block_height_delta(0, 1).is_err());
        assert!(editor.set_dirt_buffer(12).is_err());
        assert!(editor.set_raw_height(TERRAIN_HEIGHT_COUNT, 1).is_err());
        editor.set_raw_height(7, 10).unwrap();
        let encoded = editor.encode().unwrap();
        let (updated, _) = TerrainManagerData::decode_prefix(95, &encoded).unwrap();
        assert_eq!(updated.raw_heights()[7], 10);
        assert_eq!(updated.block_height_deltas()[7], 0);
        assert_eq!(updated.dirt_buffer(), 524_288);
    }
}
