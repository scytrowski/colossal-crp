use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::read_encoded_byte_array;
use std::ops::Range;

const PATH_UNIT_COUNT: usize = 262_144;
const MAX_POSITIONS: usize = 12;
const USHORT_FLAGS_VERSION: u32 = 121_011;

/// One path position stored in an active `PathUnit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPosition {
    /// Network segment ID.
    pub segment: u16,
    /// Position along the segment.
    pub offset: u8,
    /// Lane index.
    pub lane: u8,
}

/// Fields serialized for a path unit whose simulation flags are nonzero.
#[derive(Debug, Clone, PartialEq)]
pub struct PathUnit {
    /// One-based index in the game's fixed path-unit buffer.
    pub index: u32,
    /// Raw simulation flags from the encoded array.
    pub simulation_flags: u8,
    /// Pathfinding state flags.
    pub path_find_flags: u8,
    /// Lane type flags.
    pub lane_types: u8,
    /// Vehicle type flags, widened from `u8` or `u16` in older versions.
    pub vehicle_types: u32,
    /// Index of the next path unit, stored as a 24-bit integer.
    pub next_path_unit: u32,
    /// Build index associated with the path.
    pub build_index: u32,
    /// Path length in game units.
    pub length: f32,
    /// Speed, defaulting to 100 in versions that omit the byte.
    pub speed: u8,
    /// At most twelve segment/lane positions.
    pub positions: Vec<PathPosition>,
}

/// The fifth queued manager body, excluding loading-profiler tags.
///
/// Its original bytes are retained for exact writing. Semantic path edits are
/// Speed edits are available only in versions that serialize the speed byte;
/// they preserve all other record fields and the original encoded flag array.
#[derive(Debug, Clone, PartialEq)]
pub struct PathManagerData {
    format_version: u32,
    simulation_flags: Vec<u8>,
    active_units: Vec<PathUnit>,
    speed_ranges: Option<Vec<Range<usize>>>,
    serialized_body: Vec<u8>,
}

/// In-place editor for serialized speeds of existing active path units.
///
/// It changes only the existing one-byte speed fields. Path flags, path-unit
/// links, positions, counts, and all other original bytes remain unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct PathManagerEditor {
    format_version: u32,
    speeds: Option<Vec<u8>>,
    speed_ranges: Vec<Range<usize>>,
    original_body: Vec<u8>,
}

impl PathManagerData {
    /// Decodes an inner-version-164+ body below version 121011 and its length.
    ///
    /// These versions store 262143 flags through `EncodedArray.Byte`. Later
    /// versions changed the flag array to `EncodedArray.UShort` and return an
    /// unsupported-version error here. Truncation and invalid position counts
    /// return errors without indexing beyond the input.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if !(164..USHORT_FLAGS_VERSION).contains(&format_version) {
            return Err(invalid(
                0,
                format!("unsupported PathManager inner version {format_version}"),
            ));
        }
        let mut reader = Reader::new(bytes);
        let simulation_flags = read_encoded_byte_array(&mut reader, PATH_UNIT_COUNT - 1)?;
        let mut active_units = Vec::new();
        let has_serialized_speed =
            (110_026..111_000).contains(&format_version) || format_version >= 111_012;
        let mut speed_ranges = has_serialized_speed.then(Vec::new);
        for (offset, &flags) in simulation_flags.iter().enumerate() {
            if flags == 0 {
                continue;
            }
            let index = u32::try_from(offset + 1)
                .map_err(|_| CrpError::ArithmeticOverflow("PathUnit index"))?;
            let path_find_flags = reader.read_u8()?;
            let lane_types = reader.read_u8()?;
            let vehicle_types = if format_version >= 113_001 {
                reader.read_u32_be()?
            } else if format_version >= 311 {
                u32::from(reader.read_u16_be()?)
            } else {
                u32::from(reader.read_u8()?)
            };
            let position_count = usize::from(reader.read_u8()?);
            if position_count > MAX_POSITIONS {
                return Err(
                    reader.invalid(format!("PathUnit {index} has more than twelve positions"))
                );
            }
            let next_bytes = reader.read_bytes(3)?;
            let next_path_unit = (u32::from(next_bytes[0]) << 16)
                | (u32::from(next_bytes[1]) << 8)
                | u32::from(next_bytes[2]);
            let build_index = reader.read_u32_be()?;
            let length = reader.read_f32_be()?;
            let speed = if has_serialized_speed {
                let start = reader.offset();
                let speed = reader.read_u8()?;
                if let Some(ranges) = &mut speed_ranges {
                    ranges.push(start..reader.offset());
                }
                speed
            } else {
                100
            };
            let mut positions = Vec::with_capacity(position_count);
            for _ in 0..position_count {
                positions.push(PathPosition {
                    segment: reader.read_u16_be()?,
                    offset: reader.read_u8()?,
                    lane: reader.read_u8()?,
                });
            }
            active_units.push(PathUnit {
                index,
                simulation_flags: flags,
                path_find_flags,
                lane_types,
                vehicle_types,
                next_path_unit,
                build_index,
                length,
                speed,
                positions,
            });
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                simulation_flags,
                active_units,
                speed_ranges,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns all one-based path-unit flags in buffer order.
    pub fn simulation_flags(&self) -> &[u8] {
        &self.simulation_flags
    }

    /// Returns the stored units whose simulation flags are nonzero.
    pub fn active_units(&self) -> &[PathUnit] {
        &self.active_units
    }

    /// Writes the original body byte for byte, including encoded-array runs.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }

    /// Consumes the decoded body to expose its serialized speed bytes.
    ///
    /// Returns `None` when this inner version omits the speed field. For
    /// versions with the field, the slice is in active path-unit order and
    /// can be edited without changing body length or any other path data.
    pub fn into_editor(self) -> PathManagerEditor {
        PathManagerEditor {
            format_version: self.format_version,
            speeds: self
                .speed_ranges
                .as_ref()
                .map(|_| self.active_units.iter().map(|unit| unit.speed).collect()),
            speed_ranges: self.speed_ranges.unwrap_or_default(),
            original_body: self.serialized_body,
        }
    }
}

impl PathManagerEditor {
    /// Returns the inner DataSerializer version associated with this body.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns mutable speed bytes in active path-unit order.
    ///
    /// Returns `None` for versions that omit serialized speed. A present but
    /// empty slice means the version stores speed and this body has no active
    /// path units.
    pub fn speeds_mut(&mut self) -> Option<&mut [u8]> {
        self.speeds.as_deref_mut()
    }

    /// Encodes the original body with only edited speed bytes replaced.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut output = self.original_body.clone();
        match &self.speeds {
            Some(speeds) if speeds.len() != self.speed_ranges.len() => {
                return Err(invalid(
                    0,
                    "edited speed count does not match active path-unit records",
                ));
            }
            Some(speeds) => {
                for (speed, range) in speeds.iter().zip(&self.speed_ranges) {
                    if range.len() != 1 || output.get(range.clone()).is_none() {
                        return Err(invalid(
                            range.start,
                            "serialized PathUnit speed range is invalid",
                        ));
                    }
                    output[range.start] = *speed;
                }
            }
            None if !self.speed_ranges.is_empty() => {
                return Err(invalid(
                    0,
                    "speed ranges exist for a version without serialized speed",
                ));
            }
            None => {}
        }
        Ok(output)
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original_body
    }
}

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "PathManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}
