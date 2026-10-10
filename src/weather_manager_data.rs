use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::read_encoded_ushort_array;

const WIND_CELL_COUNT: usize = 16_384;
const WIND_DELTA_VERSION: u32 = 117_013;
const LIGHTNING_VERSION: u32 = 277;
const LIGHTNING_RECORD_LENGTH: usize = 32;
const VERSION_276_COMPATIBILITY_LENGTH: usize = 36;

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
    total_heights: Vec<u16>,
    delta_heights: Option<Vec<u16>>,
    wind_direction: f32,
    target_wind_direction: f32,
    wind_direction_speed: f32,
    temperature: Option<[f32; 3]>,
    rain_and_fog: Option<[f32; 4]>,
    clouds: Option<[f32; 2]>,
    force_weather_on: Option<bool>,
    ground_wetness: Option<f32>,
    northern_lights_and_rainbow: Option<[f32; 4]>,
    version_276_compatibility: Option<Vec<u8>>,
    lightning_strikes: Option<Vec<WeatherLightningStrike>>,
    serialized_body: Vec<u8>,
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
        let self_heights = read_encoded_ushort_array(&mut reader, WIND_CELL_COUNT)?;
        let total_heights = read_encoded_ushort_array(&mut reader, WIND_CELL_COUNT)?;
        let delta_heights = if format_version >= WIND_DELTA_VERSION {
            Some(read_encoded_ushort_array(&mut reader, WIND_CELL_COUNT)?)
        } else {
            None
        };

        let wind_direction = reader.read_f32_be()?;
        let target_wind_direction = reader.read_f32_be()?;
        let wind_direction_speed = reader.read_f32_be()?;
        let temperature = if format_version >= 232 {
            Some([
                reader.read_f32_be()?,
                reader.read_f32_be()?,
                reader.read_f32_be()?,
            ])
        } else {
            None
        };
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
        let (clouds, force_weather_on) = if format_version >= 275 {
            let clouds = [reader.read_f32_be()?, reader.read_f32_be()?];
            let force_weather_on = reader.read_u8()? == 1;
            (Some(clouds), Some(force_weather_on))
        } else {
            (None, None)
        };
        let ground_wetness = if format_version >= 235 {
            Some(reader.read_f32_be()?)
        } else {
            None
        };
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
        let version_276_compatibility = if format_version == 276 {
            Some(
                reader
                    .read_bytes(VERSION_276_COMPATIBILITY_LENGTH)?
                    .to_vec(),
            )
        } else {
            None
        };
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

        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                self_heights,
                total_heights,
                delta_heights,
                wind_direction,
                target_wind_direction,
                wind_direction_speed,
                temperature,
                rain_and_fog,
                clouds,
                force_weather_on,
                ground_wetness,
                northern_lights_and_rainbow,
                version_276_compatibility,
                lightning_strikes,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
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
        [
            self.wind_direction,
            self.target_wind_direction,
            self.wind_direction_speed,
        ]
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
