use crate::CrpError;
use crate::data_serializer::{DataSerializerStream, Reader, write_unique_string};

const ROOT_TYPE: &str =
    "SimulationMetaData, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null";
const MAX_MOD_OVERRIDES: u32 = 1_000_000;

/// One key and boolean value in `SimulationMetaData.m_modOverride`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModOverride {
    /// Mod identifier as stored in the game metadata.
    pub key: String,
    /// Raw DataSerializer boolean byte, retained for faithful rewriting.
    pub value: u8,
}

/// Typed view of a `SimulationMetaData` DataSerializer root object.
///
/// Fields whose `Option` is `None` were absent under the internal format
/// version's compatibility gates. Date/time values retain .NET ticks, and
/// boolean fields retain their raw bytes so decoding does not normalize them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationMetaData {
    /// Inner DataSerializer format version, not the outer package version.
    pub format_version: u32,
    /// Game-instance identifier, present from version 161.
    pub game_instance_identifier: Option<String>,
    /// Environment name.
    pub environment: Option<String>,
    /// Scenario asset identifier, present from version 292.
    pub scenario_asset: Option<String>,
    /// Scenario name, present from version 290.
    pub scenario_name: Option<String>,
    /// Raw scenario-won boolean byte, present from version 290.
    pub scenario_won: u8,
    /// Raw scenario-failed boolean byte, present from version 290.
    pub scenario_failed: u8,
    /// Starting .NET `DateTime` ticks and map name, present for versions
    /// 322–108999 and from 109001 onward.
    pub starting_datetime_ticks: Option<i64>,
    /// Map name under the same version gate as `starting_datetime_ticks`.
    pub map_name: Option<String>,
    /// Serialized mod override entries, present from version 298.
    pub mod_override: Vec<ModOverride>,
    /// Raw traffic inversion boolean byte; always present.
    pub invert_traffic: u8,
    /// Raw achievements-disabled boolean byte, present from version 192.
    pub disable_achievements: Option<u8>,
    /// Current .NET `DateTime` ticks, present from version 183.
    pub current_datetime_ticks: Option<i64>,
    /// Current day-hour IEEE-754 bits, present from version 212.
    pub current_day_hour_bits: Option<u32>,
    /// Map and game application versions, present from version 197.
    pub new_map_app_version: Option<u32>,
    /// Game application version under the same gate as `new_map_app_version`.
    pub new_game_app_version: Option<u32>,
    /// Save application version, present from version 191.
    pub save_app_version: Option<u32>,
    /// Data format version. Before version 191 the reader defaults this to
    /// `format_version` and no value is stored in the stream.
    pub data_format_version: u32,
}

impl SimulationMetaData {
    /// Decodes one `SimulationMetaData` root at the start of `bytes`.
    ///
    /// Returns the parsed value and the number of bytes consumed, allowing the
    /// following `SimulationManager+Data` stream in a combined `_Data` asset to
    /// remain untouched. Compatibility gates follow the game's
    /// `SimulationMetaData.Deserialize` method. A null mod-override key,
    /// impossible entry count, truncated field, or different root type returns
    /// an error.
    pub fn decode_from(bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let stream = DataSerializerStream::decode(bytes)?;
        if stream.root_type != ROOT_TYPE {
            return Err(typed_error(0, "root type is not SimulationMetaData"));
        }
        let mut reader = Reader::new(&stream.root_data);
        let version = stream.format_version;

        let game_instance_identifier = if version >= 161 {
            reader.read_unique_string()?
        } else {
            None
        };
        let environment = reader.read_unique_string()?;
        let scenario_asset = if version >= 292 {
            reader.read_unique_string()?
        } else {
            None
        };
        let (scenario_name, scenario_won, scenario_failed) = if version >= 290 {
            (
                reader.read_unique_string()?,
                reader.read_u8()?,
                reader.read_u8()?,
            )
        } else {
            (None, 0, 0)
        };
        let has_starting_datetime = (322..109_000).contains(&version) || version >= 109_001;
        let (starting_datetime_ticks, map_name) = if has_starting_datetime {
            (
                Some(read_i64_bits(&mut reader)?),
                reader.read_unique_string()?,
            )
        } else {
            (None, None)
        };
        let mut mod_override = Vec::new();
        if version >= 298 {
            let signed_count = reader.read_i32_be()?;
            if signed_count < 0 {
                return Err(reader.invalid(format!("negative mod override count {signed_count}")));
            }
            let count = u32::try_from(signed_count)
                .map_err(|_| reader.invalid("mod override count conversion failed"))?;
            if count > MAX_MOD_OVERRIDES {
                return Err(reader.invalid("mod override count exceeds the 1,000,000-entry limit"));
            }
            if usize::try_from(count).map_or(true, |count| count > reader.remaining() / 2) {
                return Err(reader.invalid("mod override count cannot fit in remaining bytes"));
            }
            let count = usize::try_from(count)
                .map_err(|_| reader.invalid("mod override count does not fit this platform"))?;
            mod_override.reserve(count);
            for _ in 0..count {
                let key = reader
                    .read_unique_string()?
                    .ok_or_else(|| reader.invalid("mod override key cannot be null"))?;
                let value = reader.read_u8()?;
                mod_override.push(ModOverride { key, value });
            }
        }
        let invert_traffic = reader.read_u8()?;
        let disable_achievements = if version >= 192 {
            Some(reader.read_u8()?)
        } else {
            None
        };
        let current_datetime_ticks = if version >= 183 {
            Some(read_i64_bits(&mut reader)?)
        } else {
            None
        };
        let current_day_hour_bits = if version >= 212 {
            Some(reader.read_u32_be()?)
        } else {
            None
        };
        let (new_map_app_version, new_game_app_version) = if version >= 197 {
            (Some(reader.read_u32_be()?), Some(reader.read_u32_be()?))
        } else {
            (None, None)
        };
        let (save_app_version, data_format_version) = if version >= 191 {
            (Some(reader.read_u32_be()?), reader.read_u32_be()?)
        } else {
            (None, version)
        };
        let consumed =
            stream
                .header_len
                .checked_add(reader.offset())
                .ok_or(CrpError::ArithmeticOverflow(
                    "SimulationMetaData stream length",
                ))?;

        Ok((
            Self {
                format_version: version,
                game_instance_identifier,
                environment,
                scenario_asset,
                scenario_name,
                scenario_won,
                scenario_failed,
                starting_datetime_ticks,
                map_name,
                mod_override,
                invert_traffic,
                disable_achievements,
                current_datetime_ticks,
                current_day_hour_bits,
                new_map_app_version,
                new_game_app_version,
                save_app_version,
                data_format_version,
            },
            consumed,
        ))
    }

    /// Encodes the modeled fields using the DataSerializer version gates.
    ///
    /// Fields absent in that version are omitted. Fields required by a gate
    /// must be `Some`; otherwise encoding returns an error. The emitted root
    /// type and field sequence match the examined game DLL.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut root_data = Vec::new();
        let version = self.format_version;
        if version >= 161 {
            write_optional_string(&mut root_data, self.game_instance_identifier.as_deref())?;
        }
        write_optional_string(&mut root_data, self.environment.as_deref())?;
        if version >= 292 {
            write_optional_string(&mut root_data, self.scenario_asset.as_deref())?;
        }
        if version >= 290 {
            write_optional_string(&mut root_data, self.scenario_name.as_deref())?;
            root_data.extend_from_slice(&[self.scenario_won, self.scenario_failed]);
        }
        if has_starting_datetime(version) {
            let ticks = self
                .starting_datetime_ticks
                .ok_or_else(|| typed_error(root_data.len(), "starting date/time is required"))?;
            write_i64_bits(&mut root_data, ticks);
            write_optional_string(&mut root_data, self.map_name.as_deref())?;
        }
        if version >= 298 {
            let count = u32::try_from(self.mod_override.len())
                .map_err(|_| CrpError::ArithmeticOverflow("mod override count"))?;
            if count > MAX_MOD_OVERRIDES {
                return Err(typed_error(
                    root_data.len(),
                    "too many mod override entries",
                ));
            }
            root_data.extend_from_slice(&count.to_be_bytes());
            for entry in &self.mod_override {
                write_unique_string(&mut root_data, &entry.key)?;
                root_data.push(entry.value);
            }
        }
        root_data.push(self.invert_traffic);
        if version >= 192 {
            root_data.push(self.disable_achievements.ok_or_else(|| {
                typed_error(root_data.len(), "disable-achievements value is required")
            })?);
        }
        if version >= 183 {
            let ticks = self
                .current_datetime_ticks
                .ok_or_else(|| typed_error(root_data.len(), "current date/time is required"))?;
            write_i64_bits(&mut root_data, ticks);
        }
        if version >= 212 {
            root_data.extend_from_slice(
                &self
                    .current_day_hour_bits
                    .ok_or_else(|| {
                        typed_error(root_data.len(), "current day-hour value is required")
                    })?
                    .to_be_bytes(),
            );
        }
        if version >= 197 {
            root_data.extend_from_slice(
                &self
                    .new_map_app_version
                    .ok_or_else(|| typed_error(root_data.len(), "map app version is required"))?
                    .to_be_bytes(),
            );
            root_data.extend_from_slice(
                &self
                    .new_game_app_version
                    .ok_or_else(|| typed_error(root_data.len(), "game app version is required"))?
                    .to_be_bytes(),
            );
        }
        if version >= 191 {
            root_data.extend_from_slice(
                &self
                    .save_app_version
                    .ok_or_else(|| typed_error(root_data.len(), "save app version is required"))?
                    .to_be_bytes(),
            );
            root_data.extend_from_slice(&self.data_format_version.to_be_bytes());
        }
        Ok(DataSerializerStream::from_parts(version, ROOT_TYPE.to_owned(), root_data)?.encode())
    }
}

fn has_starting_datetime(version: u32) -> bool {
    (322..109_000).contains(&version) || version >= 109_001
}

fn read_i64_bits(reader: &mut Reader<'_>) -> Result<i64, CrpError> {
    let bytes = reader.read_bytes(8)?;
    Ok(i64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn write_i64_bits(output: &mut Vec<u8>, value: i64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn write_optional_string(output: &mut Vec<u8>, value: Option<&str>) -> Result<(), CrpError> {
    match value {
        Some(value) => write_unique_string(output, value),
        None => {
            output.push(0xff);
            Ok(())
        }
    }
}

fn typed_error(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "SimulationMetaData".to_owned(),
        offset,
        reason: reason.into(),
    }
}
