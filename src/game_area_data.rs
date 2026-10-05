use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::read_encoded_byte_array;

const AREA_GRID_LENGTH: usize = 25;
const SAVED_CAMERA_VIEW_VERSION: u32 = 112_032;

/// Decoded fields of the first queued `GameAreaManager+Data` object.
///
/// The original object body is retained and returned by [`encode`](Self::encode),
/// so object references and any not-yet-modeled framing remain byte-exact.
#[derive(Debug, Clone, PartialEq)]
pub struct GameAreaManagerData {
    area_count: u8,
    start_tile: u8,
    area_grid: Vec<u8>,
    area_not_unlocked_reference: Vec<u8>,
    buildable_area: [f32; 4],
    saved_camera_view: SavedCameraView,
    serialized_body: Vec<u8>,
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
        let start_tile = if version >= 137 {
            reader.read_u8()?
        } else {
            12
        };
        let area_grid = read_encoded_byte_array(&mut reader, AREA_GRID_LENGTH)?;
        let reference_start = reader.offset();
        if version >= 87 {
            skip_object_reference(&mut reader)?;
        }
        let area_not_unlocked_reference = reader.consumed_bytes()[reference_start..].to_vec();
        let buildable_area = if version >= 199 {
            [
                reader.read_f32_be()?,
                reader.read_f32_be()?,
                reader.read_f32_be()?,
                reader.read_f32_be()?,
            ]
        } else {
            [-1.0; 4]
        };
        let saved_camera_view = if version >= SAVED_CAMERA_VIEW_VERSION {
            SavedCameraView {
                mode: reader.read_i16_be()?,
                position: [
                    reader.read_f32_be()?,
                    reader.read_f32_be()?,
                    reader.read_f32_be()?,
                ],
                size: reader.read_f32_be()?,
                angle: [reader.read_f32_be()?, reader.read_f32_be()?],
                height: reader.read_f32_be()?,
            }
        } else {
            SavedCameraView {
                mode: -1,
                position: [0.0; 3],
                size: 0.0,
                angle: [0.0; 2],
                height: 0.0,
            }
        };
        let consumed = reader.offset();
        Ok((
            Self {
                area_count,
                start_tile,
                area_grid,
                area_not_unlocked_reference,
                buildable_area,
                saved_camera_view,
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
