use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_ushort_array, write_encoded_ushort_array};
use std::ops::Range;

const WIND_CELL_COUNT: usize = 16_384;
const WIND_DELTA_VERSION: u32 = 117_013;
const LIGHTNING_VERSION: u32 = 277;
const LIGHTNING_RECORD_LENGTH: usize = 32;
const MAX_LIGHTNING_STRIKE_COUNT: usize = u8::MAX as usize;
const VERSION_276_COMPATIBILITY_LENGTH: usize = 36;

#[derive(Debug, Clone, PartialEq, Eq)]
struct EncodedHeightPlaneSnapshot {
    range: Range<usize>,
    original_values: Vec<u16>,
}

/// One serialized `WeatherManager` lightning strike from inner version 277.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherLightningStrike {
    /// Simulation frame when the strike started.
    pub start_frame: u32,
    /// Strike position in world coordinates.
    pub position: [f32; 3],
    /// Strike rotation as a quaternion.
    pub rotation: [f32; 4],
}

/// Decoded reader fields from a queued `WeatherManager+Data` or `WindManager+Data` object.
///
/// The original body bytes are retained for exact unchanged writing. The wind
/// planes contain 16,384 cells each. Scalar fields are `None` when their
/// DataSerializer version gate omits them. The version-276 compatibility block
/// is retained as raw bytes because its meaning is not modeled.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherManagerData {
    format_version: u32,
    self_heights: Vec<u16>,
    self_heights_snapshot: EncodedHeightPlaneSnapshot,
    total_heights: Vec<u16>,
    total_heights_snapshot: EncodedHeightPlaneSnapshot,
    delta_heights: Option<Vec<u16>>,
    delta_heights_snapshot: Option<EncodedHeightPlaneSnapshot>,
    wind_directions: [f32; 3],
    temperature: Option<[f32; 3]>,
    rain_and_fog: Option<[f32; 4]>,
    clouds: Option<[f32; 2]>,
    force_weather_on: Option<bool>,
    ground_wetness: Option<f32>,
    northern_lights_and_rainbow: Option<[f32; 4]>,
    version_276_compatibility: Option<Vec<u8>>,
    lightning_strikes: Option<Vec<WeatherLightningStrike>>,
    wind_directions_range: Range<usize>,
    temperature_range: Option<Range<usize>>,
    rain_and_fog_range: Option<Range<usize>>,
    clouds_range: Option<Range<usize>>,
    force_weather_on_range: Option<Range<usize>>,
    ground_wetness_range: Option<Range<usize>>,
    northern_lights_and_rainbow_range: Option<Range<usize>>,
    lightning_strikes_count_offset: Option<usize>,
    lightning_strikes_range: Option<Range<usize>>,
    serialized_body: Vec<u8>,
}

/// Editor for WeatherManager height planes, scalar fields, and lightning records.
///
/// The version-276 compatibility block and all other unedited source bytes are
/// retained. From version 277, lightning records can be added or removed
/// within the serialized byte-count limit.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherManagerEditor {
    data: WeatherManagerData,
}

impl WeatherManagerData {
    /// Decodes WeatherManager reader fields and returns bytes consumed before
    /// its closing loading-profiler reference.
    ///
    /// `bytes` must start immediately after the `WeatherManager` or legacy
    /// `WindManager` opening profiler tag. Version gates follow
    /// `WeatherManager+Data.Deserialize` in the inspected Assembly-CSharp.dll.
    /// Truncated arrays, fields, or lightning records return an error.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let (self_heights, self_heights_snapshot) = read_height_plane(&mut reader)?;
        let (total_heights, total_heights_snapshot) = read_height_plane(&mut reader)?;
        let (delta_heights, delta_heights_snapshot) = if format_version >= WIND_DELTA_VERSION {
            let (values, snapshot) = read_height_plane(&mut reader)?;
            (Some(values), Some(snapshot))
        } else {
            (None, None)
        };

        let wind_directions_start = reader.offset();
        let wind_directions = [
            reader.read_f32_be()?,
            reader.read_f32_be()?,
            reader.read_f32_be()?,
        ];
        let wind_directions_range = wind_directions_start..reader.offset();
        let temperature_start = reader.offset();
        let temperature = if format_version >= 232 {
            Some([
                reader.read_f32_be()?,
                reader.read_f32_be()?,
                reader.read_f32_be()?,
            ])
        } else {
            None
        };
        let temperature_range =
            (format_version >= 232).then_some(temperature_start..reader.offset());
        let rain_and_fog_start = reader.offset();
        let rain_and_fog = if format_version >= 233 {
            Some([
                reader.read_f32_be()?,
                reader.read_f32_be()?,
                reader.read_f32_be()?,
                reader.read_f32_be()?,
            ])
        } else {
            None
        };
        let rain_and_fog_range =
            (format_version >= 233).then_some(rain_and_fog_start..reader.offset());
        let clouds_start = reader.offset();
        let (clouds, force_weather_on) = if format_version >= 275 {
            let clouds = [reader.read_f32_be()?, reader.read_f32_be()?];
            let force_weather_on = reader.read_u8()? == 1;
            (Some(clouds), Some(force_weather_on))
        } else {
            (None, None)
        };
        let clouds_range = if format_version >= 275 {
            let end = clouds_start
                .checked_add(8)
                .ok_or(CrpError::ArithmeticOverflow("WeatherManager cloud range"))?;
            Some(clouds_start..end)
        } else {
            None
        };
        let force_weather_on_range = if format_version >= 275 {
            let start = clouds_start
                .checked_add(8)
                .ok_or(CrpError::ArithmeticOverflow(
                    "WeatherManager force-weather offset",
                ))?;
            let end = start.checked_add(1).ok_or(CrpError::ArithmeticOverflow(
                "WeatherManager force-weather range",
            ))?;
            Some(start..end)
        } else {
            None
        };
        let ground_wetness_start = reader.offset();
        let ground_wetness = if format_version >= 235 {
            Some(reader.read_f32_be()?)
        } else {
            None
        };
        let ground_wetness_range =
            (format_version >= 235).then_some(ground_wetness_start..reader.offset());
        let northern_lights_start = reader.offset();
        let northern_lights_and_rainbow = if format_version >= 241 {
            Some([
                reader.read_f32_be()?,
                reader.read_f32_be()?,
                reader.read_f32_be()?,
                reader.read_f32_be()?,
            ])
        } else {
            None
        };
        let northern_lights_and_rainbow_range =
            (format_version >= 241).then_some(northern_lights_start..reader.offset());
        let version_276_compatibility = if format_version == 276 {
            Some(
                reader
                    .read_bytes(VERSION_276_COMPATIBILITY_LENGTH)?
                    .to_vec(),
            )
        } else {
            None
        };
        let lightning_strikes_start = reader.offset();
        let lightning_strikes_count_offset =
            (format_version >= LIGHTNING_VERSION).then_some(lightning_strikes_start);
        let lightning_strikes = if format_version >= LIGHTNING_VERSION {
            let count = usize::from(reader.read_u8()?);
            if count > reader.remaining() / LIGHTNING_RECORD_LENGTH {
                return Err(
                    reader.invalid("lightning records exceed remaining WeatherManager bytes")
                );
            }
            let mut strikes = Vec::with_capacity(count);
            for _ in 0..count {
                strikes.push(WeatherLightningStrike {
                    start_frame: reader.read_u32_be()?,
                    position: [
                        reader.read_f32_be()?,
                        reader.read_f32_be()?,
                        reader.read_f32_be()?,
                    ],
                    rotation: [
                        reader.read_f32_be()?,
                        reader.read_f32_be()?,
                        reader.read_f32_be()?,
                        reader.read_f32_be()?,
                    ],
                });
            }
            Some(strikes)
        } else {
            None
        };
        let lightning_strikes_range = if format_version >= LIGHTNING_VERSION {
            let start =
                lightning_strikes_start
                    .checked_add(1)
                    .ok_or(CrpError::ArithmeticOverflow(
                        "WeatherManager lightning offset",
                    ))?;
            Some(start..reader.offset())
        } else {
            None
        };

        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                self_heights,
                self_heights_snapshot,
                total_heights,
                total_heights_snapshot,
                delta_heights,
                delta_heights_snapshot,
                wind_directions,
                temperature,
                rain_and_fog,
                clouds,
                force_weather_on,
                ground_wetness,
                northern_lights_and_rainbow,
                version_276_compatibility,
                lightning_strikes,
                wind_directions_range,
                temperature_range,
                rain_and_fog_range,
                clouds_range,
                force_weather_on_range,
                ground_wetness_range,
                northern_lights_and_rainbow_range,
                lightning_strikes_count_offset,
                lightning_strikes_range,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Creates an editor for the height planes and scalar fields present in
    /// this source version.
    pub fn into_editor(self) -> WeatherManagerEditor {
        WeatherManagerEditor { data: self }
    }

    /// Returns the 16,384 self-height cells.
    pub fn self_heights(&self) -> &[u16] {
        &self.self_heights
    }

    /// Returns the 16,384 total-height cells.
    pub fn total_heights(&self) -> &[u16] {
        &self.total_heights
    }

    /// Returns the version-117013-and-newer delta-height cells.
    pub fn delta_heights(&self) -> Option<&[u16]> {
        self.delta_heights.as_deref()
    }

    /// Returns wind direction, target direction, and direction speed.
    pub fn wind_directions(&self) -> [f32; 3] {
        self.wind_directions
    }

    /// Returns current temperature, target temperature, and temperature speed.
    pub fn temperature(&self) -> Option<[f32; 3]> {
        self.temperature
    }

    /// Returns current and target rain followed by current and target fog.
    pub fn rain_and_fog(&self) -> Option<[f32; 4]> {
        self.rain_and_fog
    }

    /// Returns current and target cloud values.
    pub fn clouds(&self) -> Option<[f32; 2]> {
        self.clouds
    }

    /// Returns the version-275-and-newer force-weather flag.
    pub fn force_weather_on(&self) -> Option<bool> {
        self.force_weather_on
    }

    /// Returns the version-235-and-newer ground wetness value.
    pub fn ground_wetness(&self) -> Option<f32> {
        self.ground_wetness
    }

    /// Returns current and target northern-light and rainbow values.
    pub fn northern_lights_and_rainbow(&self) -> Option<[f32; 4]> {
        self.northern_lights_and_rainbow
    }

    /// Returns the unidentified raw 36-byte block present only at version 276.
    pub fn version_276_compatibility(&self) -> Option<&[u8]> {
        self.version_276_compatibility.as_deref()
    }

    /// Returns lightning records from version 277 onward.
    pub fn lightning_strikes(&self) -> Option<&[WeatherLightningStrike]> {
        self.lightning_strikes.as_deref()
    }

    /// Returns the original decoded fields unchanged, without canonicalizing
    /// the encoded wind planes or any unmodeled trailing bytes.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }
}

impl WeatherManagerEditor {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.data.format_version
    }

    /// Returns mutable access to the 16,384 self-height cells.
    pub fn self_heights_mut(&mut self) -> &mut [u16] {
        &mut self.data.self_heights
    }

    /// Returns mutable access to the 16,384 total-height cells.
    pub fn total_heights_mut(&mut self) -> &mut [u16] {
        &mut self.data.total_heights
    }

    /// Returns mutable access to the version-117013-and-newer delta-height
    /// cells.
    pub fn delta_heights_mut(&mut self) -> Option<&mut [u16]> {
        self.data.delta_heights.as_deref_mut()
    }

    /// Returns mutable current wind direction, target direction, and speed.
    pub fn wind_directions_mut(&mut self) -> &mut [f32; 3] {
        &mut self.data.wind_directions
    }

    /// Returns mutable current, target, and speed temperature values when present.
    pub fn temperature_mut(&mut self) -> Option<&mut [f32; 3]> {
        self.data.temperature.as_mut()
    }

    /// Returns mutable current/target rain and fog values when present.
    pub fn rain_and_fog_mut(&mut self) -> Option<&mut [f32; 4]> {
        self.data.rain_and_fog.as_mut()
    }

    /// Returns mutable cloud values when present.
    pub fn clouds_mut(&mut self) -> Option<&mut [f32; 2]> {
        self.data.clouds.as_mut()
    }

    /// Returns the version-275-and-newer force-weather flag when present.
    pub fn force_weather_on_mut(&mut self) -> Option<&mut bool> {
        self.data.force_weather_on.as_mut()
    }

    /// Returns mutable ground wetness when present.
    pub fn ground_wetness_mut(&mut self) -> Option<&mut f32> {
        self.data.ground_wetness.as_mut()
    }

    /// Returns mutable northern-light and rainbow values when present.
    pub fn northern_lights_and_rainbow_mut(&mut self) -> Option<&mut [f32; 4]> {
        self.data.northern_lights_and_rainbow.as_mut()
    }

    /// Returns mutable lightning records without changing their count.
    pub fn lightning_strikes_mut(&mut self) -> Option<&mut [WeatherLightningStrike]> {
        self.data.lightning_strikes.as_deref_mut()
    }

    /// Adds an existing-format lightning record from inner version 277 onward.
    /// Returns an error before version 277 or when the byte-sized count is full.
    pub fn add_lightning_strike(&mut self, strike: WeatherLightningStrike) -> Result<(), CrpError> {
        let count_offset = self.data.lightning_strikes_count_offset.ok_or_else(|| {
            invalid(
                0,
                "lightning records are not serialized before inner version 277",
            )
        })?;
        let strikes = self.data.lightning_strikes.as_mut().ok_or_else(|| {
            invalid(
                count_offset,
                "WeatherManager lightning record storage is missing",
            )
        })?;
        if strikes.len() >= MAX_LIGHTNING_STRIKE_COUNT {
            return Err(invalid(
                count_offset,
                "WeatherManager lightning count exceeds the byte-sized limit of 255",
            ));
        }
        strikes.push(strike);
        Ok(())
    }

    /// Removes and returns one lightning record by zero-based serialized index.
    /// Returns an error before version 277 or for an out-of-range index.
    pub fn remove_lightning_strike(
        &mut self,
        index: usize,
    ) -> Result<WeatherLightningStrike, CrpError> {
        let count_offset = self.data.lightning_strikes_count_offset.ok_or_else(|| {
            invalid(
                0,
                "lightning records are not serialized before inner version 277",
            )
        })?;
        let strikes = self.data.lightning_strikes.as_mut().ok_or_else(|| {
            invalid(
                count_offset,
                "WeatherManager lightning record storage is missing",
            )
        })?;
        if index >= strikes.len() {
            return Err(invalid(
                count_offset,
                format!(
                    "WeatherManager lightning index {index} is outside the {} serialized records",
                    strikes.len()
                ),
            ));
        }
        Ok(strikes.remove(index))
    }

    /// Writes edited height planes and scalar fields into the original body
    /// while retaining all other bytes. Changed encoded planes can resize the
    /// body. Returns an error if a captured field range or plane length is
    /// inconsistent.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut bytes = self.data.serialized_body.clone();
        write_f32_range(
            &mut bytes,
            &self.data.wind_directions_range,
            &self.data.wind_directions,
            "wind directions",
        )?;
        encode_optional_f32_range(
            &mut bytes,
            self.data.temperature_range.as_ref(),
            self.data
                .temperature
                .as_ref()
                .map(|values| values.as_slice()),
            "temperature",
        )?;
        encode_optional_f32_range(
            &mut bytes,
            self.data.rain_and_fog_range.as_ref(),
            self.data
                .rain_and_fog
                .as_ref()
                .map(|values| values.as_slice()),
            "rain and fog",
        )?;
        encode_optional_f32_range(
            &mut bytes,
            self.data.clouds_range.as_ref(),
            self.data.clouds.as_ref().map(|values| values.as_slice()),
            "clouds",
        )?;
        encode_force_weather_flag(
            &mut bytes,
            self.data.force_weather_on_range.as_ref(),
            self.data.force_weather_on,
        )?;
        encode_optional_f32_range(
            &mut bytes,
            self.data.ground_wetness_range.as_ref(),
            self.data.ground_wetness.as_ref().map(std::slice::from_ref),
            "ground wetness",
        )?;
        encode_optional_f32_range(
            &mut bytes,
            self.data.northern_lights_and_rainbow_range.as_ref(),
            self.data
                .northern_lights_and_rainbow
                .as_ref()
                .map(|values| values.as_slice()),
            "northern lights and rainbow",
        )?;
        let lightning_replacement = encode_lightning_strikes(
            &mut bytes,
            self.data.lightning_strikes_count_offset,
            self.data.lightning_strikes_range.as_ref(),
            self.data.lightning_strikes.as_deref(),
        )?;
        let mut replacements = Vec::new();
        if let Some(replacement) = lightning_replacement {
            replacements.push(replacement);
        }
        if let Some(replacement) = changed_height_plane(
            &self.data.self_heights,
            &self.data.self_heights_snapshot,
            bytes.len(),
            "self-height plane",
        )? {
            replacements.push(replacement);
        }
        if let Some(replacement) = changed_height_plane(
            &self.data.total_heights,
            &self.data.total_heights_snapshot,
            bytes.len(),
            "total-height plane",
        )? {
            replacements.push(replacement);
        }
        match (
            self.data.delta_heights.as_deref(),
            self.data.delta_heights_snapshot.as_ref(),
        ) {
            (Some(values), Some(snapshot)) => {
                if let Some(replacement) =
                    changed_height_plane(values, snapshot, bytes.len(), "delta-height plane")?
                {
                    replacements.push(replacement);
                }
            }
            (None, None) => {}
            _ => {
                return Err(invalid(
                    0,
                    "WeatherManager delta-height values and version gate disagree",
                ));
            }
        }
        replacements.sort_by_key(|(range, _)| range.start);
        for (range, encoded) in replacements.into_iter().rev() {
            bytes.splice(range, encoded);
        }
        Ok(bytes)
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.data.serialized_body
    }
}

fn read_height_plane(
    reader: &mut Reader<'_>,
) -> Result<(Vec<u16>, EncodedHeightPlaneSnapshot), CrpError> {
    let start = reader.offset();
    let values = read_encoded_ushort_array(reader, WIND_CELL_COUNT)?;
    let end = reader.offset();
    Ok((
        values.clone(),
        EncodedHeightPlaneSnapshot {
            range: start..end,
            original_values: values,
        },
    ))
}

fn changed_height_plane(
    values: &[u16],
    snapshot: &EncodedHeightPlaneSnapshot,
    source_body_len: usize,
    field_name: &str,
) -> Result<Option<(Range<usize>, Vec<u8>)>, CrpError> {
    if values.len() != WIND_CELL_COUNT || snapshot.original_values.len() != WIND_CELL_COUNT {
        return Err(invalid(
            snapshot.range.start,
            format!("WeatherManager {field_name} has an invalid cell count"),
        ));
    }
    if snapshot.range.start > snapshot.range.end || snapshot.range.end > source_body_len {
        return Err(invalid(
            snapshot.range.start,
            format!("WeatherManager {field_name} source range exceeds the body"),
        ));
    }
    if values == snapshot.original_values {
        return Ok(None);
    }
    let mut encoded = Vec::new();
    write_encoded_ushort_array(&mut encoded, values);
    Ok(Some((snapshot.range.clone(), encoded)))
}

fn encode_optional_f32_range(
    output: &mut [u8],
    range: Option<&Range<usize>>,
    values: Option<&[f32]>,
    field_name: &str,
) -> Result<(), CrpError> {
    match (range, values) {
        (Some(range), Some(values)) => write_f32_range(output, range, values, field_name),
        (None, None) => Ok(()),
        _ => Err(invalid(
            0,
            format!("WeatherManager {field_name} values and source range disagree"),
        )),
    }
}

fn write_f32_range(
    output: &mut [u8],
    range: &Range<usize>,
    values: &[f32],
    field_name: &str,
) -> Result<(), CrpError> {
    let expected_len = values
        .len()
        .checked_mul(4)
        .ok_or(CrpError::ArithmeticOverflow(
            "WeatherManager float range length",
        ))?;
    if range.len() != expected_len {
        return Err(invalid(
            range.start,
            format!("WeatherManager {field_name} source range has an invalid length"),
        ));
    }
    let destination = output.get_mut(range.clone()).ok_or_else(|| {
        invalid(
            range.start,
            format!("WeatherManager {field_name} range exceeds source body"),
        )
    })?;
    for (chunk, value) in destination.chunks_exact_mut(4).zip(values) {
        let encoded = value.to_bits().to_be_bytes();
        if chunk != encoded {
            chunk.copy_from_slice(&encoded);
        }
    }
    Ok(())
}

fn encode_force_weather_flag(
    output: &mut [u8],
    range: Option<&Range<usize>>,
    value: Option<bool>,
) -> Result<(), CrpError> {
    match (range, value) {
        (Some(range), Some(value)) if range.len() == 1 => {
            let byte = output.get_mut(range.clone()).ok_or_else(|| {
                invalid(
                    range.start,
                    "WeatherManager force-weather range exceeds source body",
                )
            })?;
            if value != (byte[0] == 1) {
                byte[0] = u8::from(value);
            }
            Ok(())
        }
        (None, None) => Ok(()),
        (Some(range), _) => Err(invalid(
            range.start,
            "WeatherManager force-weather value and source range disagree",
        )),
        _ => Err(invalid(
            0,
            "WeatherManager force-weather value and source range disagree",
        )),
    }
}

fn encode_lightning_strikes(
    output: &mut [u8],
    count_offset: Option<usize>,
    range: Option<&Range<usize>>,
    strikes: Option<&[WeatherLightningStrike]>,
) -> Result<Option<(Range<usize>, Vec<u8>)>, CrpError> {
    match (count_offset, range, strikes) {
        (Some(count_offset), Some(range), Some(strikes)) => {
            if strikes.len() > MAX_LIGHTNING_STRIKE_COUNT {
                return Err(invalid(
                    count_offset,
                    "WeatherManager lightning count exceeds the byte-sized limit of 255",
                ));
            }
            let count_end = count_offset
                .checked_add(1)
                .ok_or(CrpError::ArithmeticOverflow(
                    "WeatherManager lightning count end",
                ))?;
            if range.start != count_end
                || range.start > range.end
                || range.end > output.len()
                || range.len() % LIGHTNING_RECORD_LENGTH != 0
            {
                return Err(invalid(
                    count_offset,
                    "WeatherManager lightning count and source record range are inconsistent",
                ));
            }
            let count = u8::try_from(strikes.len()).map_err(|_| {
                invalid(
                    count_offset,
                    "WeatherManager lightning count does not fit its serialized byte",
                )
            })?;
            let count_byte = output.get_mut(count_offset).ok_or_else(|| {
                invalid(
                    count_offset,
                    "WeatherManager lightning count exceeds source body",
                )
            })?;
            *count_byte = count;

            let encoded_len = strikes.len().checked_mul(LIGHTNING_RECORD_LENGTH).ok_or(
                CrpError::ArithmeticOverflow("WeatherManager lightning range length"),
            )?;
            let mut encoded = Vec::with_capacity(encoded_len);
            for strike in strikes {
                encoded.extend_from_slice(&strike.start_frame.to_be_bytes());
                for value in strike.position.into_iter().chain(strike.rotation) {
                    encoded.extend_from_slice(&value.to_bits().to_be_bytes());
                }
            }
            if output.get(range.clone()) == Some(encoded.as_slice()) {
                Ok(None)
            } else {
                Ok(Some((range.clone(), encoded)))
            }
        }
        (None, None, None) => Ok(None),
        (_, Some(range), _) => Err(invalid(
            range.start,
            "WeatherManager lightning records and source range disagree",
        )),
        (Some(offset), None, _) => Err(invalid(
            offset,
            "WeatherManager lightning records and source range disagree",
        )),
        (None, None, Some(_)) => Err(invalid(
            0,
            "WeatherManager lightning records and source range disagree",
        )),
    }
}

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "WeatherManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{WIND_CELL_COUNT, WeatherManagerData};
    use crate::encoded_array::write_encoded_ushort_array;

    fn zero_plane(output: &mut Vec<u8>) {
        output.resize(output.len() + WIND_CELL_COUNT / 128, 0xff);
    }

    fn append_floats(output: &mut Vec<u8>, values: &[f32]) {
        for value in values {
            output.extend_from_slice(&value.to_bits().to_be_bytes());
        }
    }

    fn expected_ushort_plane_with_first_nonzero(value: u16) -> Vec<u8> {
        let mut bytes = vec![2]; // Three initial literals seed the delta state.
        bytes.extend_from_slice(&value.to_be_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend(std::iter::repeat_n(0xff, 127));
        bytes.push(0xfc); // The remaining 125 zero values.
        bytes
    }

    fn expected_ushort_plane_with_last_nonzero(value: u16) -> Vec<u8> {
        let mut bytes = vec![0xff; (WIND_CELL_COUNT - 1) / 128];
        bytes.push(0xfe); // 127 zero values before the final literal.
        bytes.push(0);
        bytes.extend_from_slice(&value.to_be_bytes());
        bytes
    }

    fn base_body(format_version: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        zero_plane(&mut bytes);
        zero_plane(&mut bytes);
        if format_version >= 117_013 {
            zero_plane(&mut bytes);
        }
        append_floats(&mut bytes, &[1.0, 2.0, 3.0]);
        if format_version >= 232 {
            append_floats(&mut bytes, &[4.0, 5.0, 6.0]);
        }
        if format_version >= 233 {
            append_floats(&mut bytes, &[7.0, 8.0, 9.0, 10.0]);
        }
        if format_version >= 275 {
            append_floats(&mut bytes, &[11.0, 12.0]);
            bytes.push(1);
        }
        if format_version >= 235 {
            append_floats(&mut bytes, &[13.0]);
        }
        if format_version >= 241 {
            append_floats(&mut bytes, &[14.0, 15.0, 16.0, 17.0]);
        }
        if format_version == 276 {
            bytes.extend(0_u8..36);
        }
        if format_version >= 277 {
            bytes.push(1);
            bytes.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
            append_floats(&mut bytes, &[18.0, 19.0, 20.0, 0.1, 0.2, 0.3, 0.4]);
        }
        bytes
    }

    #[test]
    fn weather_fields_follow_independent_reader_version_gates() {
        for version in [
            231, 232, 233, 234, 235, 240, 241, 274, 275, 276, 277, 117_012, 117_013,
        ] {
            let bytes = base_body(version);
            let (weather, consumed) = WeatherManagerData::decode_prefix(version, &bytes).unwrap();
            assert_eq!(consumed, bytes.len(), "version {version}");
            assert_eq!(weather.format_version(), version);
            assert_eq!(weather.self_heights().len(), WIND_CELL_COUNT);
            assert_eq!(weather.total_heights().len(), WIND_CELL_COUNT);
            assert_eq!(
                weather.delta_heights().map(|heights| heights.len()),
                (version >= 117_013).then_some(WIND_CELL_COUNT)
            );
            assert_eq!(weather.wind_directions(), [1.0, 2.0, 3.0]);
            assert_eq!(
                weather.temperature(),
                (version >= 232).then_some([4.0, 5.0, 6.0])
            );
            assert_eq!(
                weather.rain_and_fog(),
                (version >= 233).then_some([7.0, 8.0, 9.0, 10.0])
            );
            assert_eq!(weather.clouds(), (version >= 275).then_some([11.0, 12.0]));
            assert_eq!(weather.force_weather_on(), (version >= 275).then_some(true));
            assert_eq!(weather.ground_wetness(), (version >= 235).then_some(13.0));
            assert_eq!(
                weather.northern_lights_and_rainbow(),
                (version >= 241).then_some([14.0, 15.0, 16.0, 17.0])
            );
            if version == 276 {
                let expected = (0_u8..36).collect::<Vec<_>>();
                assert_eq!(
                    weather.version_276_compatibility(),
                    Some(expected.as_slice())
                );
            } else {
                assert_eq!(weather.version_276_compatibility(), None);
            }
            assert_eq!(
                weather.lightning_strikes().map(|strikes| strikes.len()),
                (version >= 277).then_some(1)
            );
            assert_eq!(weather.encode(), bytes);
        }

        let mut delta_body = base_body(117_013);
        let remainder = delta_body.split_off(3 * (WIND_CELL_COUNT / 128));
        delta_body.clear();
        let mut nonzero_self_heights = vec![0; WIND_CELL_COUNT];
        nonzero_self_heights[0] = 0x1234;
        write_encoded_ushort_array(&mut delta_body, &nonzero_self_heights);
        zero_plane(&mut delta_body);
        zero_plane(&mut delta_body);
        delta_body.extend_from_slice(&remainder);
        let (weather, _) = WeatherManagerData::decode_prefix(117_013, &delta_body).unwrap();
        assert_eq!(weather.self_heights().first(), Some(&0x1234));
        assert_eq!(
            weather.delta_heights().map(<[_]>::len),
            Some(WIND_CELL_COUNT)
        );
    }

    #[test]
    fn weather_reader_preserves_version_276_bytes_and_bool_wire_value() {
        let mut bytes = base_body(276);
        let bool_offset = 256 + 3 * 4 + 3 * 4 + 4 * 4 + 2 * 4;
        bytes[bool_offset] = 0x80;
        let (weather, consumed) = WeatherManagerData::decode_prefix(276, &bytes).unwrap();
        assert!(!weather.force_weather_on().unwrap());
        assert_eq!(
            weather.version_276_compatibility().unwrap(),
            &(0_u8..36).collect::<Vec<_>>()
        );
        assert_eq!(weather.encode(), bytes[..consumed]);

        let mut editor = weather.into_editor();
        assert_eq!(editor.encode().unwrap(), bytes[..consumed]);
        *editor.force_weather_on_mut().unwrap() = true;
        let mut expected = bytes[..consumed].to_vec();
        expected[bool_offset] = 1;
        assert_eq!(editor.encode().unwrap(), expected);
        *editor.force_weather_on_mut().unwrap() = false;
        expected[bool_offset] = 0x80;
        assert_eq!(editor.encode().unwrap(), expected);
    }

    #[test]
    fn weather_editor_reencodes_changed_height_planes_and_preserves_other_fields() {
        let bytes = base_body(117_013);
        let (weather, _) = WeatherManagerData::decode_prefix(117_013, &bytes).unwrap();
        let mut editor = weather.into_editor();
        editor.self_heights_mut()[0] = 0x1234;
        editor.total_heights_mut()[WIND_CELL_COUNT - 1] = 0x5678;
        editor.delta_heights_mut().unwrap()[0] = 0x9abc;

        let mut expected = bytes.clone();
        let mut replacements = vec![
            (
                editor.data.self_heights_snapshot.range.clone(),
                expected_ushort_plane_with_first_nonzero(0x1234),
            ),
            (
                editor.data.total_heights_snapshot.range.clone(),
                expected_ushort_plane_with_last_nonzero(0x5678),
            ),
            (
                editor
                    .data
                    .delta_heights_snapshot
                    .as_ref()
                    .unwrap()
                    .range
                    .clone(),
                expected_ushort_plane_with_first_nonzero(0x9abc),
            ),
        ];
        replacements.sort_by_key(|(range, _)| range.start);
        for (range, encoded) in replacements.into_iter().rev() {
            expected.splice(range, encoded);
        }
        let encoded = editor.encode().unwrap();
        assert_eq!(encoded, expected);
        assert_ne!(encoded.len(), bytes.len());
        let (decoded, consumed) = WeatherManagerData::decode_prefix(117_013, &encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded.self_heights()[0], 0x1234);
        assert_eq!(decoded.total_heights()[WIND_CELL_COUNT - 1], 0x5678);
        assert_eq!(decoded.delta_heights().unwrap()[0], 0x9abc);
        assert_eq!(decoded.wind_directions(), [1.0, 2.0, 3.0]);

        let older_bytes = base_body(117_012);
        let (older, _) = WeatherManagerData::decode_prefix(117_012, &older_bytes).unwrap();
        let mut older_editor = older.into_editor();
        assert!(older_editor.delta_heights_mut().is_none());
        assert_eq!(older_editor.encode().unwrap(), older_bytes);
    }

    #[test]
    fn weather_editor_patches_scalars_and_existing_lightning_records() {
        let bytes = base_body(277);
        let (weather, consumed) = WeatherManagerData::decode_prefix(277, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        let mut editor = weather.into_editor();
        editor
            .wind_directions_mut()
            .copy_from_slice(&[21.0, 22.0, 23.0]);
        editor
            .temperature_mut()
            .unwrap()
            .copy_from_slice(&[24.0, 25.0, 26.0]);
        editor
            .rain_and_fog_mut()
            .unwrap()
            .copy_from_slice(&[27.0, 28.0, 29.0, 30.0]);
        editor.clouds_mut().unwrap().copy_from_slice(&[31.0, 32.0]);
        *editor.force_weather_on_mut().unwrap() = false;
        *editor.ground_wetness_mut().unwrap() = 33.0;
        editor
            .northern_lights_and_rainbow_mut()
            .unwrap()
            .copy_from_slice(&[34.0, 35.0, 36.0, 37.0]);
        let strike = &mut editor.lightning_strikes_mut().unwrap()[0];
        strike.start_frame = 0x0a0b_0c0d;
        strike.position = [38.0, 39.0, 40.0];
        strike.rotation = [0.5, 0.6, 0.7, 0.8];

        let mut expected = bytes.clone();
        let mut scalar_bytes = Vec::new();
        append_floats(&mut scalar_bytes, &[21.0, 22.0, 23.0]);
        append_floats(&mut scalar_bytes, &[24.0, 25.0, 26.0]);
        append_floats(&mut scalar_bytes, &[27.0, 28.0, 29.0, 30.0]);
        append_floats(&mut scalar_bytes, &[31.0, 32.0]);
        scalar_bytes.push(0);
        append_floats(&mut scalar_bytes, &[33.0]);
        append_floats(&mut scalar_bytes, &[34.0, 35.0, 36.0, 37.0]);
        expected[256..325].copy_from_slice(&scalar_bytes);
        let strike_start = 326;
        expected[strike_start..strike_start + 4].copy_from_slice(&0x0a0b_0c0d_u32.to_be_bytes());
        let mut strike_fields = Vec::new();
        append_floats(&mut strike_fields, &[38.0, 39.0, 40.0, 0.5, 0.6, 0.7, 0.8]);
        expected[strike_start + 4..strike_start + 32].copy_from_slice(&strike_fields);
        assert_eq!(editor.encode().unwrap(), expected);

        let (decoded, decoded_len) = WeatherManagerData::decode_prefix(277, &expected).unwrap();
        assert_eq!(decoded_len, expected.len());
        assert_eq!(decoded.wind_directions(), [21.0, 22.0, 23.0]);
        assert_eq!(
            decoded.lightning_strikes().unwrap()[0].start_frame,
            0x0a0b_0c0d
        );
        assert_eq!(decoded.encode(), expected);
    }

    #[test]
    fn weather_editor_adds_and_removes_lightning_records_with_expected_count_bytes() {
        let bytes = base_body(277);
        let (weather, consumed) = WeatherManagerData::decode_prefix(277, &bytes).unwrap();
        let original_strike = weather.lightning_strikes().unwrap()[0];
        let mut editor = weather.into_editor();
        let added_strike = super::WeatherLightningStrike {
            start_frame: 0xaabb_ccdd,
            position: [21.0, 22.0, 23.0],
            rotation: [0.1, 0.2, 0.3, 0.4],
        };
        editor.add_lightning_strike(added_strike).unwrap();
        assert_eq!(editor.lightning_strikes_mut().unwrap().len(), 2);

        let count_offset = consumed - 33;
        let mut expected = bytes.clone();
        expected[count_offset] = 2;
        expected.extend_from_slice(&added_strike.start_frame.to_be_bytes());
        append_floats(&mut expected, &[21.0, 22.0, 23.0, 0.1, 0.2, 0.3, 0.4]);
        assert_eq!(editor.encode().unwrap(), expected);
        let (decoded, decoded_len) = WeatherManagerData::decode_prefix(277, &expected).unwrap();
        assert_eq!(decoded_len, expected.len());
        assert_eq!(decoded.lightning_strikes().unwrap().len(), 2);
        assert_eq!(
            decoded.lightning_strikes().unwrap()[1].start_frame,
            added_strike.start_frame
        );

        assert_eq!(editor.remove_lightning_strike(0).unwrap(), original_strike);
        let mut expected_one = bytes[..count_offset + 1].to_vec();
        expected_one[count_offset] = 1;
        expected_one.extend_from_slice(&added_strike.start_frame.to_be_bytes());
        append_floats(&mut expected_one, &[21.0, 22.0, 23.0, 0.1, 0.2, 0.3, 0.4]);
        assert_eq!(editor.encode().unwrap(), expected_one);
        assert!(editor.remove_lightning_strike(1).is_err());
        editor.remove_lightning_strike(0).unwrap();
        let mut expected_empty = bytes[..count_offset + 1].to_vec();
        expected_empty[count_offset] = 0;
        assert_eq!(editor.encode().unwrap(), expected_empty);

        let (weather, _) = WeatherManagerData::decode_prefix(277, &bytes).unwrap();
        let mut full_editor = weather.into_editor();
        for _ in 1..u8::MAX {
            full_editor.add_lightning_strike(added_strike).unwrap();
        }
        assert_eq!(full_editor.lightning_strikes_mut().unwrap().len(), 255);
        assert!(full_editor.add_lightning_strike(added_strike).is_err());
    }

    #[test]
    fn weather_editor_keeps_version_276_compatibility_bytes_opaque() {
        let bytes = base_body(276);
        let (weather, consumed) = WeatherManagerData::decode_prefix(276, &bytes).unwrap();
        let mut editor = weather.into_editor();
        let strike = super::WeatherLightningStrike {
            start_frame: 0,
            position: [0.0; 3],
            rotation: [0.0; 4],
        };
        assert!(editor.add_lightning_strike(strike).is_err());
        assert!(editor.remove_lightning_strike(0).is_err());
        *editor.ground_wetness_mut().unwrap() = 42.0;
        let mut expected = bytes[..consumed].to_vec();
        expected[305..309].copy_from_slice(&42.0_f32.to_bits().to_be_bytes());
        assert_eq!(editor.encode().unwrap(), expected);
        assert_eq!(&expected[325..361], &bytes[325..361]);
    }

    #[test]
    fn weather_reader_rejects_truncated_arrays_fields_and_lightning_records() {
        let mut truncated_array = vec![0xff; WIND_CELL_COUNT / 128 - 1];
        truncated_array.extend(vec![0xff; WIND_CELL_COUNT / 128]);
        assert!(WeatherManagerData::decode_prefix(231, &truncated_array).is_err());

        let mut truncated_fields = base_body(232);
        truncated_fields.pop();
        assert!(WeatherManagerData::decode_prefix(232, &truncated_fields).is_err());

        let mut truncated_compatibility = base_body(276);
        truncated_compatibility.pop();
        assert!(WeatherManagerData::decode_prefix(276, &truncated_compatibility).is_err());

        let mut truncated_lightning = base_body(277);
        truncated_lightning.pop();
        assert!(WeatherManagerData::decode_prefix(277, &truncated_lightning).is_err());
    }
}
