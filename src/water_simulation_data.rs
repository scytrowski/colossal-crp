use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_ushort_array, write_encoded_ushort_array};

const GRID_SIDE: usize = 1081;
const CELL_COUNT: usize = GRID_SIDE * GRID_SIDE;
const WATER_SOURCE_RECORD_LENGTH: usize = 43;
const WATER_WAVE_LEGACY_RECORD_LENGTH: usize = 23;
const WATER_WAVE_RECORD_LENGTH: usize = 24;

/// A water source in the version-184-and-newer layout.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterSource {
    /// Input position in world coordinates.
    pub input_position: [f32; 3],
    /// Output position in world coordinates.
    pub output_position: [f32; 3],
    /// Source kind stored by the game.
    pub source_type: u16,
    /// Target stored by the game.
    pub target: u16,
    /// Five 24-bit source parameters in serialized order.
    pub parameters: [u32; 5],
}

/// One `WaterWave` record in a version-259-or-newer WaterSimulation body.
///
/// Versions 259–260 encode `wave_type` as a boolean byte and can represent
/// only 0 or 1. Version 261 and newer encode it as a big-endian `u16`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaterWave {
    /// Wave kind stored by the game.
    pub wave_type: u16,
    /// Minimum X coordinate of the affected cell rectangle.
    pub min_x: u16,
    /// Minimum Z coordinate of the affected cell rectangle.
    pub min_z: u16,
    /// Maximum X coordinate of the affected cell rectangle.
    pub max_x: u16,
    /// Maximum Z coordinate of the affected cell rectangle.
    pub max_z: u16,
    /// Original X coordinate.
    pub origin_x: u16,
    /// Original Z coordinate.
    pub origin_z: u16,
    /// Signed X direction.
    pub direction_x: i16,
    /// Signed Z direction.
    pub direction_z: i16,
    /// Signed wave height delta.
    pub delta: i16,
    /// Wave duration.
    pub duration: u16,
    /// Current wave time.
    pub current_time: u16,
}

/// Decoded `WaterSimulation+Data` fields for inner serializer versions 184+.
///
/// The original body is retained for byte-exact writing. Unknown record values
/// and encoded-array run choices are never normalized on re-encoding.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterSimulationData {
    format_version: u32,
    heights: Vec<u16>,
    pollution: Vec<u16>,
    velocity_x: Vec<i16>,
    velocity_z: Vec<i16>,
    sources: Vec<WaterSource>,
    wave_count: u16,
    waves: Vec<WaterWave>,
    wave_records: Vec<u8>,
    sea_level: f32,
    pollution_rates: [i8; 2],
    frame_index: u32,
    serialized_body: Vec<u8>,
}

/// Editable WaterSimulation fields with an explicit canonical writer.
///
/// Create this by consuming a decoded [`WaterSimulationData`] with
/// [`into_editor`](WaterSimulationData::into_editor). Encoding writes fresh
/// arrays, sources, and tail fields. Unchanged wave records retain their
/// original bytes; edited waves are written from typed fields.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterSimulationEditor {
    data: WaterSimulationData,
    original_waves: Vec<WaterWave>,
}

impl WaterWave {
    fn read_from(version: u32, reader: &mut Reader<'_>) -> Result<Self, CrpError> {
        let wave_type = if version >= 261 {
            reader.read_u16_be()?
        } else {
            u16::from(reader.read_u8()? == 1)
        };
        Ok(Self {
            wave_type,
            min_x: reader.read_u16_be()?,
            min_z: reader.read_u16_be()?,
            max_x: reader.read_u16_be()?,
            max_z: reader.read_u16_be()?,
            origin_x: reader.read_u16_be()?,
            origin_z: reader.read_u16_be()?,
            direction_x: reader.read_i16_be()?,
            direction_z: reader.read_i16_be()?,
            delta: reader.read_i16_be()?,
            duration: reader.read_u16_be()?,
            current_time: reader.read_u16_be()?,
        })
    }

    fn write_to(&self, version: u32, output: &mut Vec<u8>) -> Result<(), CrpError> {
        if version >= 261 {
            output.extend_from_slice(&self.wave_type.to_be_bytes());
        } else if self.wave_type <= 1 {
            output.push(self.wave_type as u8);
        } else {
            return Err(invalid(
                output.len(),
                "wave type exceeds the version-259 boolean layout",
            ));
        }
        for coordinate in [
            self.min_x,
            self.min_z,
            self.max_x,
            self.max_z,
            self.origin_x,
            self.origin_z,
        ] {
            output.extend_from_slice(&coordinate.to_be_bytes());
        }
        for signed in [self.direction_x, self.direction_z, self.delta] {
            output.extend_from_slice(&signed.to_be_bytes());
        }
        output.extend_from_slice(&self.duration.to_be_bytes());
        output.extend_from_slice(&self.current_time.to_be_bytes());
        Ok(())
    }
}

impl WaterSimulationData {
    /// Decodes a WaterSimulation body and returns its byte length.
    ///
    /// The stream must start after the `WaterSimulation` profiler begin tag.
    /// Versions before 184 are rejected because their source record layout has
    /// not been implemented. Malformed arrays and truncated records return an error.
    pub fn decode_prefix(version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if version < 184 {
            return Err(invalid(
                0,
                "WaterSource layout before version 184 is unsupported",
            ));
        }
        let mut reader = Reader::new(bytes);
        let heights = read_encoded_ushort_array(&mut reader, CELL_COUNT)?;
        let wet_count = heights.iter().filter(|height| **height != 0).count();
        let wet_pollution = read_encoded_ushort_array(&mut reader, wet_count)?;
        let mut pollution = vec![0_u16; CELL_COUNT];
        let mut wet_iter = wet_pollution.into_iter();
        for (height, value) in heights.iter().zip(&mut pollution) {
            if *height != 0 {
                *value = wet_iter
                    .next()
                    .ok_or_else(|| reader.invalid("missing pollution value for wet cell"))?;
            }
        }

        let x_count = heights
            .iter()
            .enumerate()
            .filter(|(index, height)| {
                **height != 0 || (*index % GRID_SIDE < GRID_SIDE - 1 && heights[*index + 1] != 0)
            })
            .count();
        let packed_x = read_encoded_ushort_array(&mut reader, x_count)?;
        let mut velocity_x = vec![0_i16; CELL_COUNT];
        let mut x_iter = packed_x.into_iter();
        for (index, value) in velocity_x.iter_mut().enumerate() {
            if heights[index] != 0 || (index % GRID_SIDE < GRID_SIDE - 1 && heights[index + 1] != 0)
            {
                *value = x_iter
                    .next()
                    .ok_or_else(|| reader.invalid("missing horizontal velocity"))?
                    as i16;
            }
        }

        let z_count = heights
            .iter()
            .enumerate()
            .filter(|(index, height)| {
                **height != 0
                    || (*index / GRID_SIDE < GRID_SIDE - 1 && heights[*index + GRID_SIDE] != 0)
            })
            .count();
        let packed_z = read_encoded_ushort_array(&mut reader, z_count)?;
        let mut velocity_z = vec![0_i16; CELL_COUNT];
        let mut z_iter = packed_z.into_iter();
        for (index, value) in velocity_z.iter_mut().enumerate() {
            if heights[index] != 0
                || (index / GRID_SIDE < GRID_SIDE - 1 && heights[index + GRID_SIDE] != 0)
            {
                *value = z_iter
                    .next()
                    .ok_or_else(|| reader.invalid("missing vertical velocity"))?
                    as i16;
            }
        }

        let source_count = usize::from(reader.read_u16_be()?);
        if source_count > reader.remaining() / WATER_SOURCE_RECORD_LENGTH {
            return Err(reader.invalid("water source records exceed remaining bytes"));
        }
        let mut sources = Vec::with_capacity(source_count);
        for _ in 0..source_count {
            let input_position = [
                reader.read_f32_be()?,
                reader.read_f32_be()?,
                reader.read_f32_be()?,
            ];
            let output_position = [
                reader.read_f32_be()?,
                reader.read_f32_be()?,
                reader.read_f32_be()?,
            ];
            let source_type = reader.read_u16_be()?;
            let target = reader.read_u16_be()?;
            let mut parameters = [0_u32; 5];
            for parameter in &mut parameters {
                let bytes = reader.read_bytes(3)?;
                *parameter =
                    (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
            }
            sources.push(WaterSource {
                input_position,
                output_position,
                source_type,
                target,
                parameters,
            });
        }
        let (wave_count, waves, wave_records) = if version >= 259 {
            let count = reader.read_u16_be()?;
            let record_length = if version >= 261 {
                WATER_WAVE_RECORD_LENGTH
            } else {
                WATER_WAVE_LEGACY_RECORD_LENGTH
            };
            let length = usize::from(count)
                .checked_mul(record_length)
                .ok_or(CrpError::ArithmeticOverflow("water wave table length"))?;
            if length > reader.remaining() {
                return Err(reader.invalid("water wave records exceed remaining bytes"));
            }
            let start = reader.offset();
            let mut waves = Vec::with_capacity(usize::from(count));
            for _ in 0..count {
                waves.push(WaterWave::read_from(version, &mut reader)?);
            }
            let records = reader.consumed_bytes()[start..reader.offset()].to_vec();
            (count, waves, records)
        } else {
            (0, Vec::new(), Vec::new())
        };
        let sea_level = if version >= 131 {
            reader.read_f32_be()?
        } else {
            0.0
        };
        let pollution_rates = if version >= 147 {
            [reader.read_u8()? as i8, reader.read_u8()? as i8]
        } else {
            [0, 0]
        };
        let frame_index = if version >= 173 {
            reader.read_u32_be()?
        } else {
            0
        };
        let consumed = reader.offset();
        Ok((
            Self {
                format_version: version,
                heights,
                pollution,
                velocity_x,
                velocity_z,
                sources,
                wave_count,
                waves,
                wave_records,
                sea_level,
                pollution_rates,
                frame_index,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the row-major 1081-by-1081 water heights.
    pub fn heights(&self) -> &[u16] {
        &self.heights
    }

    /// Returns pollution values, with zero in dry cells.
    pub fn pollution(&self) -> &[u16] {
        &self.pollution
    }

    /// Returns signed horizontal velocities, with zero at omitted cells.
    pub fn velocity_x(&self) -> &[i16] {
        &self.velocity_x
    }

    /// Returns signed vertical velocities, with zero at omitted cells.
    pub fn velocity_z(&self) -> &[i16] {
        &self.velocity_z
    }

    /// Returns the decoded water source records.
    pub fn sources(&self) -> &[WaterSource] {
        &self.sources
    }

    /// Returns the number of decoded water wave records.
    pub fn wave_count(&self) -> u16 {
        self.wave_count
    }

    /// Returns typed wave records in their original order.
    pub fn waves(&self) -> &[WaterWave] {
        &self.waves
    }

    /// Returns opaque wave records in their original on-disk order.
    pub fn wave_records(&self) -> &[u8] {
        &self.wave_records
    }

    /// Returns the saved sea level.
    pub fn sea_level(&self) -> f32 {
        self.sea_level
    }

    /// Returns the two signed pollution disposal rates.
    pub fn pollution_rates(&self) -> [i8; 2] {
        self.pollution_rates
    }

    /// Returns the saved water simulation frame index.
    pub fn frame_index(&self) -> u32 {
        self.frame_index
    }

    /// Returns the original body without changing array controls or unknown records.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }

    /// Consumes this decoded body for editing without copying its large grids.
    ///
    /// Use [`WaterSimulationEditor::encode`] to write changed fields. The
    /// original array bytes are discarded; opaque wave records remain stored.
    /// Edited writes always use canonical array controls.
    pub fn into_editor(mut self) -> WaterSimulationEditor {
        self.serialized_body = Vec::new();
        WaterSimulationEditor {
            original_waves: self.waves.clone(),
            data: self,
        }
    }

    pub(crate) fn encoded_len(&self) -> usize {
        self.serialized_body.len()
    }
}

impl WaterSimulationEditor {
    /// Returns the inner DataSerializer version associated with this body.
    pub fn format_version(&self) -> u32 {
        self.data.format_version
    }

    /// Returns the editable fixed-size height grid.
    pub fn heights_mut(&mut self) -> &mut [u16] {
        &mut self.data.heights
    }

    /// Returns the editable pollution grid.
    ///
    /// Values at dry cells must remain zero or [`encode`](Self::encode) fails.
    pub fn pollution_mut(&mut self) -> &mut [u16] {
        &mut self.data.pollution
    }

    /// Returns editable signed horizontal velocities.
    ///
    /// Values at cells outside the height/neighbor mask must remain zero.
    pub fn velocity_x_mut(&mut self) -> &mut [i16] {
        &mut self.data.velocity_x
    }

    /// Returns editable signed vertical velocities.
    ///
    /// Values at cells outside the height/neighbor mask must remain zero.
    pub fn velocity_z_mut(&mut self) -> &mut [i16] {
        &mut self.data.velocity_z
    }

    /// Returns the editable source list; at most 65,535 sources can be written.
    pub fn sources_mut(&mut self) -> &mut Vec<WaterSource> {
        &mut self.data.sources
    }

    /// Returns the editable wave list for inner versions 259 and newer.
    ///
    /// The writer preserves original wave bytes when this list is unchanged.
    /// For versions 259–260, every edited `wave_type` must be 0 or 1.
    pub fn waves_mut(&mut self) -> &mut Vec<WaterWave> {
        &mut self.data.waves
    }

    /// Sets the saved sea level.
    pub fn set_sea_level(&mut self, sea_level: f32) {
        self.data.sea_level = sea_level;
    }

    /// Sets the two signed pollution disposal rates.
    pub fn set_pollution_rates(&mut self, rates: [i8; 2]) {
        self.data.pollution_rates = rates;
    }

    /// Sets the saved water simulation frame index.
    pub fn set_frame_index(&mut self, frame_index: u32) {
        self.data.frame_index = frame_index;
    }

    /// Encodes changed fields for inner versions 184 and newer.
    ///
    /// Array blocks are regenerated using literal and repeated-delta controls,
    /// so bytes can differ from the game's original run grouping. Original
    /// wave bytes are preserved when unchanged and re-encoded when edited.
    /// Returns an error if counts or 24-bit parameters overflow, an old-version
    /// wave type is unrepresentable, or a value would be dropped by a sparse mask.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let data = &self.data;
        if data.format_version < 184 {
            return Err(invalid(
                0,
                "WaterSource layout before version 184 is unsupported",
            ));
        }
        if data.heights.len() != CELL_COUNT
            || data.pollution.len() != CELL_COUNT
            || data.velocity_x.len() != CELL_COUNT
            || data.velocity_z.len() != CELL_COUNT
        {
            return Err(invalid(
                0,
                "WaterSimulation grids must have 1,168,561 cells",
            ));
        }
        let source_count = u16::try_from(data.sources.len())
            .map_err(|_| invalid(0, "water source count exceeds 65,535"))?;
        let mut output = Vec::new();
        write_encoded_ushort_array(&mut output, &data.heights);

        let mut wet_pollution = Vec::new();
        let mut packed_x = Vec::new();
        let mut packed_z = Vec::new();
        for index in 0..CELL_COUNT {
            let wet = data.heights[index] != 0;
            if wet {
                wet_pollution.push(data.pollution[index]);
            } else if data.pollution[index] != 0 {
                return Err(invalid(
                    index,
                    "nonzero pollution in dry WaterSimulation cell",
                ));
            }
            let has_x = wet || (index % GRID_SIDE < GRID_SIDE - 1 && data.heights[index + 1] != 0);
            if has_x {
                packed_x.push(data.velocity_x[index] as u16);
            } else if data.velocity_x[index] != 0 {
                return Err(invalid(
                    index,
                    "horizontal velocity is outside the sparse mask",
                ));
            }
            let has_z =
                wet || (index / GRID_SIDE < GRID_SIDE - 1 && data.heights[index + GRID_SIDE] != 0);
            if has_z {
                packed_z.push(data.velocity_z[index] as u16);
            } else if data.velocity_z[index] != 0 {
                return Err(invalid(
                    index,
                    "vertical velocity is outside the sparse mask",
                ));
            }
        }
        write_encoded_ushort_array(&mut output, &wet_pollution);
        write_encoded_ushort_array(&mut output, &packed_x);
        write_encoded_ushort_array(&mut output, &packed_z);

        output.extend_from_slice(&source_count.to_be_bytes());
        for (index, source) in data.sources.iter().enumerate() {
            for coordinate in source
                .input_position
                .into_iter()
                .chain(source.output_position)
            {
                output.extend_from_slice(&coordinate.to_bits().to_be_bytes());
            }
            output.extend_from_slice(&source.source_type.to_be_bytes());
            output.extend_from_slice(&source.target.to_be_bytes());
            for parameter in source.parameters {
                if parameter > 0x00ff_ffff {
                    return Err(invalid(index, "water source parameter exceeds 24 bits"));
                }
                output.extend_from_slice(&parameter.to_be_bytes()[1..]);
            }
        }

        if data.format_version >= 259 {
            let wave_count = u16::try_from(data.waves.len())
                .map_err(|_| invalid(output.len(), "water wave count exceeds 65,535"))?;
            let record_length = if data.format_version >= 261 {
                WATER_WAVE_RECORD_LENGTH
            } else {
                WATER_WAVE_LEGACY_RECORD_LENGTH
            };
            let expected_length = usize::from(data.wave_count)
                .checked_mul(record_length)
                .ok_or(CrpError::ArithmeticOverflow("water wave table length"))?;
            output.extend_from_slice(&wave_count.to_be_bytes());
            if data.waves == self.original_waves && wave_count == data.wave_count {
                if data.wave_records.len() != expected_length {
                    return Err(invalid(
                        output.len(),
                        "wave record length does not match count",
                    ));
                }
                output.extend_from_slice(&data.wave_records);
            } else {
                for wave in &data.waves {
                    wave.write_to(data.format_version, &mut output)?;
                }
            }
        } else if !data.waves.is_empty() {
            return Err(invalid(
                output.len(),
                "water waves are unavailable before version 259",
            ));
        }
        output.extend_from_slice(&data.sea_level.to_bits().to_be_bytes());
        output.extend(data.pollution_rates.map(|rate| rate as u8));
        output.extend_from_slice(&data.frame_index.to_be_bytes());
        Ok(output)
    }
}

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "WaterSimulation+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{CELL_COUNT, WaterSimulationData, WaterWave};
    use crate::data_serializer::Reader;
    use crate::encoded_array::write_encoded_ushort_array;

    fn zero_run(output: &mut Vec<u8>, mut remaining: usize) {
        while remaining > 0 {
            let count = remaining.min(128);
            output.push(0x7f + u8::try_from(count).unwrap());
            remaining -= count;
        }
    }

    fn sample_body(wave_record_length: Option<usize>) -> Vec<u8> {
        let mut bytes = Vec::new();
        zero_run(&mut bytes, CELL_COUNT);
        // No pollution values are stored when the height grid is dry.
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        for value in [1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            bytes.extend_from_slice(&value.to_bits().to_be_bytes());
        }
        bytes.extend_from_slice(&7_u16.to_be_bytes());
        bytes.extend_from_slice(&8_u16.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 1, 0, 0, 2, 0, 0, 3, 0, 0, 4, 0, 0, 5]);
        if let Some(record_length) = wave_record_length {
            bytes.extend_from_slice(&1_u16.to_be_bytes());
            bytes.extend(std::iter::repeat_n(0_u8, record_length));
        }
        bytes.extend_from_slice(&38.0_f32.to_bits().to_be_bytes());
        bytes.extend_from_slice(&[1, 2]);
        bytes.extend_from_slice(&123_u32.to_be_bytes());
        bytes
    }

    #[test]
    fn water_body_decodes_independent_fields_and_preserves_bytes() {
        for (version, wave_record_length) in [(194, None), (259, Some(23)), (261, Some(24))] {
            let bytes = sample_body(wave_record_length);
            let (water, consumed) = WaterSimulationData::decode_prefix(version, &bytes).unwrap();
            assert_eq!(consumed, bytes.len());
            assert_eq!(water.heights(), &[0_u16; CELL_COUNT]);
            assert_eq!(water.pollution(), &[0_u16; CELL_COUNT]);
            assert_eq!(water.velocity_x(), &[0_i16; CELL_COUNT]);
            assert_eq!(water.velocity_z(), &[0_i16; CELL_COUNT]);
            assert_eq!(water.sources().len(), 1);
            assert_eq!(water.sources()[0].input_position, [1.0, 2.0, 3.0]);
            assert_eq!(water.sources()[0].output_position, [4.0, 5.0, 6.0]);
            assert_eq!(water.sources()[0].source_type, 7);
            assert_eq!(water.sources()[0].target, 8);
            assert_eq!(water.sources()[0].parameters, [1, 2, 3, 4, 5]);
            assert_eq!(water.wave_count(), u16::from(wave_record_length.is_some()));
            assert_eq!(
                water.waves().len(),
                usize::from(wave_record_length.is_some())
            );
            assert_eq!(water.sea_level(), 38.0);
            assert_eq!(water.pollution_rates(), [1, 2]);
            assert_eq!(water.frame_index(), 123);
            assert_eq!(water.encode(), bytes);
        }
    }

    #[test]
    fn water_body_rejects_truncation_and_unsupported_source_layout() {
        let bytes = sample_body(Some(24));
        assert!(WaterSimulationData::decode_prefix(261, &bytes[..bytes.len() - 1]).is_err());
        assert!(WaterSimulationData::decode_prefix(183, &bytes).is_err());
        for (version, record_length) in [(259, 23), (261, 24)] {
            let bytes = sample_body(Some(record_length));
            let wave_start = bytes.len() - 10 - record_length;
            assert!(
                WaterSimulationData::decode_prefix(
                    version,
                    &bytes[..wave_start + record_length - 1]
                )
                .is_err()
            );
        }
    }

    #[test]
    fn game_produced_desert_water_body_has_expected_fields() {
        let bytes = include_bytes!("../tests/fixtures/water-simulation-body-v116020-desert.bin");
        let (water, consumed) = WaterSimulationData::decode_prefix(116_020, bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(water.heights().len(), CELL_COUNT);
        assert_eq!(
            water.heights().iter().filter(|value| **value != 0).count(),
            13_810
        );
        assert_eq!(water.sources().len(), 29);
        assert_eq!(
            water.sources()[0].input_position.map(f32::to_bits),
            [0x43e9_9c91, 0x4394_eb76, 0x44a9_13c8]
        );
        assert_eq!(water.sources()[0].parameters, [0, 0, 32_767, 32_767, 83]);
        assert_eq!(water.wave_count(), 0);
        assert_eq!(water.sea_level(), 35.0);
        assert_eq!(water.pollution_rates(), [1, 1]);
        assert_eq!(water.frame_index(), 3_420_902);
        assert_eq!(water.encode(), bytes);
    }

    #[test]
    fn water_sparse_velocity_selection_respects_row_boundaries() {
        let mut bytes = Vec::new();
        // One wet cell at the first column of row 1. Its dry neighbor above
        // has a Z value; the preceding row's last cell has no X value.
        zero_run(&mut bytes, 1081);
        bytes.extend_from_slice(&[0x02, 0, 1, 0, 0, 0, 0]);
        zero_run(&mut bytes, CELL_COUNT - 1084);
        bytes.extend_from_slice(&[0x00, 0, 9]); // pollution at the wet cell
        bytes.extend_from_slice(&[0x00, 0, 3]); // X at the wet cell only
        bytes.extend_from_slice(&[0x01, 0, 4, 0, 5]); // Z above and at wet cell
        bytes.extend_from_slice(&0_u16.to_be_bytes()); // no sources
        bytes.extend_from_slice(&0_f32.to_bits().to_be_bytes());
        bytes.extend_from_slice(&[1, 1]);
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        let (water, consumed) = WaterSimulationData::decode_prefix(194, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(water.heights()[1081], 1);
        assert_eq!(water.pollution()[1081], 9);
        assert_eq!(water.velocity_x()[1080], 0);
        assert_eq!(water.velocity_x()[1081], 3);
        assert_eq!(water.velocity_z()[0], 4);
        assert_eq!(water.velocity_z()[1081], 5);
    }

    #[test]
    fn canonical_array_writer_emits_known_literal_and_repeat_controls() {
        let mut bytes = Vec::new();
        write_encoded_ushort_array(&mut bytes, &[5, 10, 15, 20]);
        assert_eq!(bytes, [0x00, 0x00, 0x05, 0x82]);

        bytes.clear();
        write_encoded_ushort_array(&mut bytes, &[0, 0, 0, 0]);
        assert_eq!(bytes, [0x83]);

        bytes.clear();
        write_encoded_ushort_array(&mut bytes, &[0xfffe, 0xfffc, 0xfffa, 0xfff8]);
        assert_eq!(bytes, [0x00, 0xff, 0xfe, 0x82]);
    }

    #[test]
    fn edited_water_reencodes_fields_and_preserves_wave_records() {
        let mut original = sample_body(Some(23));
        let wave_start = original.len() - 10 - 23;
        original[wave_start] = 0xa5;
        let (water, _) = WaterSimulationData::decode_prefix(259, &original).unwrap();
        let mut editor = water.into_editor();
        editor.heights_mut()[0] = 1;
        editor.pollution_mut()[0] = 9;
        editor.velocity_x_mut()[0] = -2;
        editor.velocity_z_mut()[0] = 3;
        editor.sources_mut()[0].parameters[4] = 0x00ab_cdef;
        editor.set_sea_level(41.5);
        editor.set_pollution_rates([-1, 2]);
        editor.set_frame_index(456);
        let encoded = editor.encode().unwrap();
        assert_eq!(
            &encoded[encoded.len() - 10 - 23..encoded.len() - 10][..1],
            &[0xa5]
        );
        assert_eq!(
            &encoded[encoded.len() - 10..],
            &[0x42, 0x26, 0x00, 0x00, 0xff, 0x02, 0, 0, 1, 0xc8]
        );
        let (decoded, consumed) = WaterSimulationData::decode_prefix(259, &encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded.heights()[0], 1);
        assert_eq!(decoded.pollution()[0], 9);
        assert_eq!(decoded.velocity_x()[0], -2);
        assert_eq!(decoded.velocity_z()[0], 3);
        assert_eq!(decoded.sources()[0].parameters[4], 0x00ab_cdef);
        assert_eq!(decoded.wave_count(), 1);
        assert_eq!(decoded.waves()[0].wave_type, 0);
        assert_eq!(decoded.sea_level(), 41.5);
        assert_eq!(decoded.pollution_rates(), [-1, 2]);
        assert_eq!(decoded.frame_index(), 456);
    }

    #[test]
    fn edited_water_rejects_values_that_sparse_fields_cannot_store() {
        let bytes = sample_body(None);
        let (water, _) = WaterSimulationData::decode_prefix(194, &bytes).unwrap();
        let mut editor = water.into_editor();
        editor.pollution_mut()[0] = 1;
        assert!(editor.encode().is_err());
        editor.pollution_mut()[0] = 0;
        editor.velocity_x_mut()[0] = 1;
        assert!(editor.encode().is_err());
        editor.velocity_x_mut()[0] = 0;
        editor.sources_mut()[0].parameters[0] = 0x0100_0000;
        assert!(editor.encode().is_err());
        editor.sources_mut()[0].parameters[0] = 0;
        let source = editor.sources_mut()[0].clone();
        editor.sources_mut().resize(65_536, source);
        assert!(editor.encode().is_err());
    }

    fn known_wave() -> WaterWave {
        WaterWave {
            wave_type: 0x1234,
            min_x: 1,
            min_z: 2,
            max_x: 3,
            max_z: 4,
            origin_x: 5,
            origin_z: 6,
            direction_x: -1,
            direction_z: -2,
            delta: -3,
            duration: 7,
            current_time: 8,
        }
    }

    #[test]
    fn water_wave_uses_independent_expected_bytes_for_both_version_gates() {
        let suffix = [
            0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0xff, 0xff, 0xff, 0xfe, 0xff, 0xfd, 0, 7, 0, 8,
        ];
        let wave = known_wave();
        let mut bytes = Vec::new();
        wave.write_to(261, &mut bytes).unwrap();
        assert_eq!(&bytes[..2], &[0x12, 0x34]);
        assert_eq!(&bytes[2..], &suffix);
        let decoded = WaterWave::read_from(261, &mut Reader::new(&bytes)).unwrap();
        assert_eq!(decoded, wave);

        let mut old_wave = wave;
        old_wave.wave_type = 1;
        bytes.clear();
        old_wave.write_to(259, &mut bytes).unwrap();
        assert_eq!(bytes[0], 1);
        assert_eq!(&bytes[1..], &suffix);
        let decoded = WaterWave::read_from(259, &mut Reader::new(&bytes)).unwrap();
        assert_eq!(decoded, old_wave);
        old_wave.wave_type = 2;
        assert!(old_wave.write_to(259, &mut Vec::new()).is_err());
        old_wave.wave_type = 0;
        bytes.clear();
        old_wave.write_to(259, &mut bytes).unwrap();
        assert_eq!(bytes[0], 0);
    }

    #[test]
    fn edited_wave_fields_are_written_without_changing_other_water_fields() {
        for (version, record_length, wave_type) in [(259, 23, 1), (261, 24, 0x1234)] {
            let original = sample_body(Some(record_length));
            let (water, _) = WaterSimulationData::decode_prefix(version, &original).unwrap();
            let mut editor = water.into_editor();
            editor.waves_mut()[0] = WaterWave {
                wave_type,
                ..known_wave()
            };
            let encoded = editor.encode().unwrap();
            let record_start = encoded.len() - 10 - record_length;
            assert_eq!(encoded[record_start], if version >= 261 { 0x12 } else { 1 });
            let (decoded, length) = WaterSimulationData::decode_prefix(version, &encoded).unwrap();
            assert_eq!(length, encoded.len());
            assert_eq!(decoded.waves()[0], editor.waves_mut()[0]);
            assert_eq!(decoded.sources().len(), 1);
            assert_eq!(decoded.sea_level(), 38.0);
        }
    }

    #[test]
    fn waves_cannot_be_added_before_the_wave_table_version() {
        let (water, _) = WaterSimulationData::decode_prefix(258, &sample_body(None)).unwrap();
        let mut editor = water.into_editor();
        editor.waves_mut().push(known_wave());
        assert!(editor.encode().is_err());

        let (water, _) = WaterSimulationData::decode_prefix(259, &sample_body(Some(23))).unwrap();
        let mut editor = water.into_editor();
        editor.waves_mut()[0].wave_type = 2;
        assert!(editor.encode().is_err());

        let (water, _) = WaterSimulationData::decode_prefix(261, &sample_body(Some(24))).unwrap();
        let mut editor = water.into_editor();
        editor.waves_mut().resize(65_536, known_wave());
        assert!(editor.encode().is_err());
    }

    #[test]
    fn legacy_wave_bool_is_true_only_for_byte_one() {
        let mut bytes = sample_body(Some(23));
        let wave_start = bytes.len() - 10 - 23;
        bytes[wave_start] = 2;
        let (water, _) = WaterSimulationData::decode_prefix(259, &bytes).unwrap();
        assert_eq!(water.waves()[0].wave_type, 0);
        assert_eq!(water.encode(), bytes);
        let editor = water.into_editor();
        let canonical = editor.encode().unwrap();
        let canonical_wave_start = canonical.len() - 10 - 23;
        assert_eq!(canonical[canonical_wave_start], 2);
    }
}
