use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_byte_array, write_encoded_byte_array};
use std::ops::Range;

const AREA_GRID_LENGTH: usize = 25;
const START_TILE_VERSION: u32 = 137;
const BUILDABLE_AREA_VERSION: u32 = 199;
const SAVED_CAMERA_VIEW_VERSION: u32 = 112_032;

/// Decoded fields of the first queued `GameAreaManager+Data` object.
///
/// The original object body is retained and returned by [`encode`](Self::encode),
/// so object references and any not-yet-modeled framing remain byte-exact.
#[derive(Debug, Clone, PartialEq)]
pub struct GameAreaManagerData {
    version: u32,
    area_count: u8,
    start_tile: u8,
    area_grid: Vec<u8>,
    area_not_unlocked_reference: Vec<u8>,
    buildable_area: [f32; 4],
    saved_camera_view: SavedCameraView,
    start_tile_range: Option<Range<usize>>,
    area_grid_range: Range<usize>,
    buildable_area_range: Option<Range<usize>>,
    saved_camera_view_range: Option<Range<usize>>,
    serialized_body: Vec<u8>,
}

/// Editor for the serialized tile count, start tile, area grid, and buildable
/// bounds. The opaque guide reference and saved camera bytes remain unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct GameAreaManagerEditor {
    original: GameAreaManagerData,
    area_count: u8,
    start_tile: u8,
    area_grid: Vec<u8>,
    buildable_area: [f32; 4],
    saved_camera_view: SavedCameraView,
}

/// Camera state serialized inline by `GameAreaManager+Data`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SavedCameraView {
    /// Camera mode; defaults to -1 before DataSerializer version 112032.
    pub mode: i16,
    /// World-space camera position.
    pub position: [f32; 3],
    /// Camera size.
    pub size: f32,
    /// Camera angle components.
    pub angle: [f32; 2],
    /// Camera height.
    pub height: f32,
}

impl GameAreaManagerData {
    /// Decodes one `GameAreaManager+Data` object body and returns its byte length.
    ///
    /// `bytes` must begin at object ID 1's body in the queued manager graph.
    /// The game allocates a fixed 25-byte area grid. The object-reference field
    /// is preserved in its original form; fields after this object remain opaque.
    pub fn decode_prefix(version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let area_count = reader.read_u8()?;
        let (start_tile, start_tile_range) = if version >= START_TILE_VERSION {
            let start = reader.offset();
            (reader.read_u8()?, Some(start..reader.offset()))
        } else {
            (12, None)
        };
        let area_grid_start = reader.offset();
        let area_grid = read_encoded_byte_array(&mut reader, AREA_GRID_LENGTH)?;
        let area_grid_range = area_grid_start..reader.offset();
        let reference_start = reader.offset();
        if version >= 87 {
            skip_object_reference(&mut reader)?;
        }
        let area_not_unlocked_reference = reader.consumed_bytes()[reference_start..].to_vec();
        let (buildable_area, buildable_area_range) = if version >= BUILDABLE_AREA_VERSION {
            let start = reader.offset();
            let values = [
                reader.read_f32_be()?,
                reader.read_f32_be()?,
                reader.read_f32_be()?,
                reader.read_f32_be()?,
            ];
            (values, Some(start..reader.offset()))
        } else {
            ([-1.0; 4], None)
        };
        let (saved_camera_view, saved_camera_view_range) = if version >= SAVED_CAMERA_VIEW_VERSION {
            let start = reader.offset();
            let view = SavedCameraView {
                mode: reader.read_i16_be()?,
                position: [
                    reader.read_f32_be()?,
                    reader.read_f32_be()?,
                    reader.read_f32_be()?,
                ],
                size: reader.read_f32_be()?,
                angle: [reader.read_f32_be()?, reader.read_f32_be()?],
                height: reader.read_f32_be()?,
            };
            (view, Some(start..reader.offset()))
        } else {
            (
                SavedCameraView {
                    mode: -1,
                    position: [0.0; 3],
                    size: 0.0,
                    angle: [0.0; 2],
                    height: 0.0,
                },
                None,
            )
        };
        let consumed = reader.offset();
        Ok((
            Self {
                version,
                area_count,
                start_tile,
                area_grid,
                area_not_unlocked_reference,
                buildable_area,
                saved_camera_view,
                start_tile_range,
                area_grid_range,
                buildable_area_range,
                saved_camera_view_range,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the number of unlocked map tiles.
    pub fn area_count(&self) -> u8 {
        self.area_count
    }

    /// Returns the start tile index.
    pub fn start_tile(&self) -> u8 {
        self.start_tile
    }

    /// Returns the fixed 5-by-5 area grid.
    pub fn area_grid(&self) -> &[u8] {
        &self.area_grid
    }

    /// Returns the raw DataSerializer reference bytes for `m_areaNotUnlocked`.
    pub fn area_not_unlocked_reference(&self) -> &[u8] {
        &self.area_not_unlocked_reference
    }

    /// Returns the four buildable-area limits.
    pub fn buildable_area(&self) -> [f32; 4] {
        self.buildable_area
    }

    /// Returns the saved camera state, including version defaults.
    pub fn saved_camera_view(&self) -> SavedCameraView {
        self.saved_camera_view
    }

    /// Creates an editor for the tile count, start tile, area grid, and
    /// buildable bounds present in this version.
    pub fn into_editor(self) -> GameAreaManagerEditor {
        GameAreaManagerEditor {
            area_count: self.area_count,
            start_tile: self.start_tile,
            area_grid: self.area_grid.clone(),
            buildable_area: self.buildable_area,
            saved_camera_view: self.saved_camera_view,
            original: self,
        }
    }

    /// Returns the original serialized object body unchanged.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }
}

impl GameAreaManagerEditor {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.original.version
    }

    /// Sets the number of unlocked map tiles.
    pub fn set_area_count(&mut self, value: u8) {
        self.area_count = value;
    }

    /// Returns mutable access to the serialized start tile when the version
    /// includes that field (inner version 137 and newer).
    pub fn start_tile_mut(&mut self) -> Option<&mut u8> {
        self.original.start_tile_range.as_ref()?;
        Some(&mut self.start_tile)
    }

    /// Returns the fixed 5-by-5 area grid for editing.
    pub fn area_grid_mut(&mut self) -> &mut [u8] {
        &mut self.area_grid
    }

    /// Returns mutable buildable-area limits from inner version 199 onward.
    pub fn buildable_area_mut(&mut self) -> Option<&mut [f32; 4]> {
        self.original.buildable_area_range.as_ref()?;
        Some(&mut self.buildable_area)
    }

    /// Returns mutable saved-camera fields from inner version 112032 onward.
    pub fn saved_camera_view_mut(&mut self) -> Option<&mut SavedCameraView> {
        self.original.saved_camera_view_range.as_ref()?;
        Some(&mut self.saved_camera_view)
    }

    /// Writes changed values using the known field layout while preserving
    /// the opaque reference and saved camera bytes exactly.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.area_grid.len() != AREA_GRID_LENGTH {
            return Err(invalid_game_area(
                self.original.area_grid_range.start,
                format!("area grid must contain exactly {AREA_GRID_LENGTH} cells"),
            ));
        }

        let mut bytes = self.original.serialized_body.clone();
        if bytes.is_empty() {
            return Err(invalid_game_area(0, "source object body is empty"));
        }
        bytes[0] = self.area_count;

        let mut replacements = Vec::new();
        if self.start_tile != self.original.start_tile {
            let range = self.original.start_tile_range.as_ref().ok_or_else(|| {
                invalid_game_area(1, "start tile is unavailable before inner version 137")
            })?;
            replacements.push((range.clone(), vec![self.start_tile]));
        }
        if self.area_grid != self.original.area_grid {
            let mut encoded = Vec::new();
            write_encoded_byte_array(&mut encoded, &self.area_grid);
            replacements.push((self.original.area_grid_range.clone(), encoded));
        }
        if self
            .buildable_area
            .iter()
            .zip(self.original.buildable_area)
            .any(|(current, original)| current.to_bits() != original.to_bits())
        {
            let range = self.original.buildable_area_range.as_ref().ok_or_else(|| {
                invalid_game_area(0, "buildable area is unavailable before inner version 199")
            })?;
            let mut encoded = Vec::with_capacity(16);
            for value in self.buildable_area {
                encoded.extend_from_slice(&value.to_bits().to_be_bytes());
            }
            replacements.push((range.clone(), encoded));
        }
        if !saved_camera_view_bits_equal(self.saved_camera_view, self.original.saved_camera_view) {
            let range = self
                .original
                .saved_camera_view_range
                .as_ref()
                .ok_or_else(|| {
                    invalid_game_area(
                        0,
                        "saved camera view is unavailable before inner version 112032",
                    )
                })?;
            let mut encoded = Vec::with_capacity(30);
            encoded.extend_from_slice(&self.saved_camera_view.mode.to_be_bytes());
            for value in [
                self.saved_camera_view.position[0],
                self.saved_camera_view.position[1],
                self.saved_camera_view.position[2],
                self.saved_camera_view.size,
                self.saved_camera_view.angle[0],
                self.saved_camera_view.angle[1],
                self.saved_camera_view.height,
            ] {
                encoded.extend_from_slice(&value.to_bits().to_be_bytes());
            }
            replacements.push((range.clone(), encoded));
        }

        replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        for (range, replacement) in replacements {
            if range.start > range.end || range.end > bytes.len() {
                return Err(invalid_game_area(
                    range.start,
                    "source field range is outside the object body",
                ));
            }
            bytes.splice(range, replacement);
        }
        Ok(bytes)
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original.serialized_body
    }
}

fn invalid_game_area(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "GameAreaManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

fn saved_camera_view_bits_equal(left: SavedCameraView, right: SavedCameraView) -> bool {
    left.mode == right.mode
        && left
            .position
            .iter()
            .zip(right.position)
            .all(|(left, right)| left.to_bits() == right.to_bits())
        && left.size.to_bits() == right.size.to_bits()
        && left
            .angle
            .iter()
            .zip(right.angle)
            .all(|(left, right)| left.to_bits() == right.to_bits())
        && left.height.to_bits() == right.height.to_bits()
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
    use super::GameAreaManagerData;

    #[test]
    fn editor_updates_fields_and_preserves_unmodeled_bytes() {
        let source = include_bytes!("../tests/fixtures/game-area-manager-body-v112004.bin");
        let (data, _) = GameAreaManagerData::decode_prefix(112_004, source).unwrap();
        let grid_start = data.area_grid_range.start;
        let grid_end = data.area_grid_range.end;
        let bounds_start = data.buildable_area_range.as_ref().unwrap().start;
        let mut editor = data.into_editor();
        editor.set_area_count(3);
        *editor.start_tile_mut().unwrap() = 4;
        editor.area_grid_mut()[24] = 7;
        editor
            .buildable_area_mut()
            .unwrap()
            .copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        let encoded = editor.encode().unwrap();

        let mut expected = source[..grid_start].to_vec();
        expected[0] = 3;
        expected[1] = 4;
        expected.push(24);
        expected.extend_from_slice(&[0; 24]);
        expected.push(7);
        expected.extend_from_slice(&source[grid_end..bounds_start]);
        for value in [1.0_f32, 2.0, 3.0, 4.0] {
            expected.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        assert_eq!(encoded, expected);
        assert_eq!(encoded.len(), source.len() - (grid_end - grid_start) + 26);
    }

    #[test]
    fn editor_preserves_nan_payload_bits_when_bounds_are_unchanged() {
        let mut source = vec![0, 12, 24];
        source.extend([0; 25]);
        source.push(0xff);
        for _ in 0..4 {
            source.extend_from_slice(&[0x7f, 0xc1, 0x23, 0x45]);
        }
        let (data, _) = GameAreaManagerData::decode_prefix(199, &source).unwrap();
        let editor = data.into_editor();
        assert_eq!(editor.encode().unwrap(), source);
    }

    #[test]
    fn editor_preserves_saved_camera_bytes_after_resizing_the_grid() {
        let mut source = vec![5, 7, 0x98, 0xff];
        let bounds = [0.1_f32, 0.2, 0.3, 0.4];
        for value in bounds {
            source.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        let mut camera = vec![0x01, 0x02];
        for value in [1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0] {
            camera.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        source.extend_from_slice(&camera);

        let (data, consumed) = GameAreaManagerData::decode_prefix(112_032, &source).unwrap();
        assert_eq!(consumed, source.len());
        let view = data.saved_camera_view();
        assert_eq!(view.mode, 0x0102);
        assert_eq!(view.position, [1.0, 2.0, 3.0]);
        assert_eq!(view.size, 4.0);
        assert_eq!(view.angle, [5.0, 6.0]);
        assert_eq!(view.height, 7.0);

        let mut editor = data.into_editor();
        editor.set_area_count(6);
        editor.area_grid_mut()[0] = 1;
        let encoded = editor.encode().unwrap();
        assert!(encoded.ends_with(&camera));
        assert_eq!(encoded.len(), source.len() + 25);
    }

    #[test]
    fn editor_writes_saved_camera_fields_in_expected_big_endian_order() {
        let mut source = vec![5, 7, 0x98, 0xff];
        source.extend([0; 16]);
        let camera_start = source.len();
        source.extend_from_slice(&0x0102_i16.to_be_bytes());
        for value in [1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0] {
            source.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        let (data, _) = GameAreaManagerData::decode_prefix(112_032, &source).unwrap();
        let mut editor = data.into_editor();
        let camera = editor.saved_camera_view_mut().unwrap();
        camera.mode = -2;
        camera.position = [8.5, -9.25, 1.5];
        camera.size = 2.5;
        camera.angle = [0.5, -0.75];
        camera.height = 100.0;

        let encoded = editor.encode().unwrap();
        let mut expected = source[..camera_start].to_vec();
        expected.extend_from_slice(&(-2_i16).to_be_bytes());
        for value in [8.5_f32, -9.25, 1.5, 2.5, 0.5, -0.75, 100.0] {
            expected.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        assert_eq!(encoded, expected);
        let (decoded, _) = GameAreaManagerData::decode_prefix(112_032, &encoded).unwrap();
        assert_eq!(decoded.saved_camera_view().mode, -2);
        assert_eq!(decoded.saved_camera_view().position, [8.5, -9.25, 1.5]);
        assert_eq!(decoded.saved_camera_view().size, 2.5);
        assert_eq!(decoded.saved_camera_view().angle, [0.5, -0.75]);
        assert_eq!(decoded.saved_camera_view().height, 100.0);
    }

    #[test]
    fn editor_keeps_legacy_fields_absent() {
        let mut source = vec![7, 24];
        source.extend([0; 25]);
        let (data, _) = GameAreaManagerData::decode_prefix(86, &source).unwrap();
        let mut editor = data.into_editor();
        assert!(editor.start_tile_mut().is_none());
        assert!(editor.buildable_area_mut().is_none());
        assert!(editor.saved_camera_view_mut().is_none());
        editor.set_area_count(8);
        let mut expected = source;
        expected[0] = 8;
        assert_eq!(editor.encode().unwrap(), expected);
    }
}
