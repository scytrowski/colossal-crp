use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    read_encoded_byte_array, read_encoded_uint_array, read_encoded_ulong_array,
    read_encoded_ushort_array,
};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

const BUILDING_COUNT: usize = 49_152;
const LEGACY_BUILDING_COUNT: usize = 32_768;
const FULL_BUILDING_COUNT_VERSION: u32 = 222;
const SECONDARY_BUILDING_FLAGS_VERSION: u32 = 112_030;
const TWO_BUILDING_PROBLEMS_VERSION: u32 = 115_000;
const SAVE_APP_PROBLEMS_VERSION: u32 = 146_809_088;

/// Serialized flags for one one-based `Building` buffer position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingFlag {
    /// One-based building buffer index.
    pub index: u16,
    /// Primary building flags.
    pub flags: u32,
    /// Secondary building flags, zero before inner version 112030.
    pub flags2: u32,
}

/// Encoded building-flag arrays at the start of `BuildingManager+Data`.
///
/// Later prefab tables and building records remain outside this prefix. The
/// original encoded bytes are retained for exact writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingFlagsPrefix {
    format_version: u32,
    building_flags: Vec<BuildingFlag>,
    serialized_prefix: Vec<u8>,
}

/// Prefab index tables and names immediately after the BuildingManager counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingPrefabPrefix {
    format_version: u32,
    building_indices: Vec<u16>,
    guide_indices: Vec<u16>,
    milestone_indices: Vec<u16>,
    names: Vec<Option<String>>,
    serialized_prefix: Vec<u8>,
}

/// Versioned object headers following the BuildingManager prefab table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingManagerObjectHeadersPrefix {
    format_version: u32,
    object_headers: Vec<NetObjectHeader>,
    trailing_problem_fields: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Opaque versioned problem state for one active building.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingProblemWords {
    /// One-based building buffer index for this problem state.
    pub index: u16,
    /// First serialized 64-bit word.
    pub first: u64,
    /// Second serialized 64-bit word.
    pub second: u64,
}

/// Per-building problem states from inner version 115000 onward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingProblemPrefix {
    format_version: u32,
    problems: Vec<BuildingProblemWords>,
    serialized_prefix: Vec<u8>,
}

/// Per-building buffer indices and byte fields following problem state.
///
/// The decoder supports inner versions 164 and newer and consumes only the
/// version-gated prefix represented here. Values remain paired with the active
/// building order from `BuildingFlagsPrefix`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingBufferFieldsPrefix {
    format_version: u32,
    net_node: Vec<u16>,
    sub_building: Vec<u16>,
    water_source: Vec<u16>,
    legacy_water_source_release: Vec<u16>,
    electricity_buffer: Vec<u16>,
    water_buffer: Vec<u16>,
    sewage_buffer: Vec<u16>,
    heating_buffer: Vec<u16>,
    garbage_buffer: Vec<u16>,
    crime_buffer: Vec<u16>,
    mail_buffer: Vec<u16>,
    custom_buffer1: Vec<u16>,
    custom_buffer2: Vec<u16>,
    build_water_height: Vec<u16>,
    production_rate: Vec<u8>,
    water_pollution: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Per-building byte state following utility buffers and pollution values.
///
/// The values are exposed as raw bytes because their game meanings have not
/// been fully mapped. The original encoded prefix is retained for exact writeback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingByteStatePrefix {
    format_version: u32,
    construct_state: Vec<u8>,
    fire_intensity: Vec<u8>,
    fire_damage: Vec<u8>,
    production_state: Vec<u8>,
    health: Vec<u8>,
    child_health: Vec<u8>,
    senior_health: Vec<u8>,
    happiness: Vec<u8>,
    citizen_count: Vec<u8>,
    temp_import: Vec<u8>,
    temp_export: Vec<u8>,
    final_import: Vec<u8>,
    final_export: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

impl BuildingBufferFieldsPrefix {
    /// Decodes versioned network, utility, and byte fields for active buildings.
    ///
    /// Returns the decoded prefix and bytes consumed. Truncated data or a
    /// mismatched flag-table version returns an error. Later BuildingManager
    /// fields remain unread and the original prefix bytes are retained.
    pub fn decode_prefix(
        format_version: u32,
        flags: &BuildingFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager buffer fields".to_owned(),
                offset: 0,
                reason: "unsupported or mismatched inner version".to_owned(),
            });
        }
        let count = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .count();
        let mut reader = Reader::new(bytes);
        let net_node = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let sub_building = if format_version >= 208 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let water_source = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let legacy_water_source_release = if (95..184).contains(&format_version) {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let electricity_buffer = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let water_buffer = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let sewage_buffer = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let heating_buffer = if format_version >= 227 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let garbage_buffer = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let crime_buffer = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let mail_buffer = if format_version >= 111_003 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let custom_buffer1 = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let custom_buffer2 = if format_version >= 66 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let build_water_height = if format_version >= 308 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let production_rate = if format_version >= 33 {
            read_encoded_byte_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let water_pollution = if format_version >= 33 {
            read_encoded_byte_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                net_node,
                sub_building,
                water_source,
                legacy_water_source_release,
                electricity_buffer,
                water_buffer,
                sewage_buffer,
                heating_buffer,
                garbage_buffer,
                crime_buffer,
                mail_buffer,
                custom_buffer1,
                custom_buffer2,
                build_water_height,
                production_rate,
                water_pollution,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns network-node indices for active buildings.
    pub fn net_node(&self) -> &[u16] {
        &self.net_node
    }
    /// Returns sub-building indices from version 208 onward.
    pub fn sub_building(&self) -> &[u16] {
        &self.sub_building
    }
    /// Returns water-source indices.
    pub fn water_source(&self) -> &[u16] {
        &self.water_source
    }
    /// Returns legacy water-source release values from versions 95 through 183.
    pub fn legacy_water_source_release(&self) -> &[u16] {
        &self.legacy_water_source_release
    }
    /// Returns electricity-buffer indices.
    pub fn electricity_buffer(&self) -> &[u16] {
        &self.electricity_buffer
    }
    /// Returns water-buffer indices.
    pub fn water_buffer(&self) -> &[u16] {
        &self.water_buffer
    }
    /// Returns sewage-buffer indices.
    pub fn sewage_buffer(&self) -> &[u16] {
        &self.sewage_buffer
    }
    /// Returns heating-buffer indices from version 227 onward.
    pub fn heating_buffer(&self) -> &[u16] {
        &self.heating_buffer
    }
    /// Returns garbage-buffer indices.
    pub fn garbage_buffer(&self) -> &[u16] {
        &self.garbage_buffer
    }
    /// Returns crime-buffer indices.
    pub fn crime_buffer(&self) -> &[u16] {
        &self.crime_buffer
    }
    /// Returns mail-buffer indices from version 111003 onward.
    pub fn mail_buffer(&self) -> &[u16] {
        &self.mail_buffer
    }
    /// Returns custom-buffer-1 indices.
    pub fn custom_buffer1(&self) -> &[u16] {
        &self.custom_buffer1
    }
    /// Returns custom-buffer-2 indices from version 66 onward.
    pub fn custom_buffer2(&self) -> &[u16] {
        &self.custom_buffer2
    }
    /// Returns build-water-height values from version 308 onward.
    pub fn build_water_height(&self) -> &[u16] {
        &self.build_water_height
    }
    /// Returns production-rate bytes.
    pub fn production_rate(&self) -> &[u8] {
        &self.production_rate
    }
    /// Returns water-pollution bytes.
    pub fn water_pollution(&self) -> &[u8] {
        &self.water_pollution
    }
    /// Writes the original encoded fields exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl BuildingByteStatePrefix {
    /// Decodes the next versioned per-building byte arrays.
    ///
    /// Returns the decoded prefix and bytes consumed. Truncated data or a
    /// mismatched flag-table version returns an error; later BuildingManager
    /// arrays are left unread.
    pub fn decode_prefix(
        format_version: u32,
        flags: &BuildingFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager byte state".to_owned(),
                offset: 0,
                reason: "unsupported or mismatched inner version".to_owned(),
            });
        }
        let count = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .count();
        let mut reader = Reader::new(bytes);
        let mut read = |since| {
            if format_version >= since {
                read_encoded_byte_array(&mut reader, count)
            } else {
                Ok(Vec::new())
            }
        };
        let construct_state = read(33)?;
        let fire_intensity = read(35)?;
        let fire_damage = read(35)?;
        let production_state = read(81)?;
        let health = read(55)?;
        let child_health = read(113_004)?;
        let senior_health = read(113_004)?;
        let happiness = read(55)?;
        let citizen_count = read(55)?;
        let temp_import = read(72)?;
        let temp_export = read(72)?;
        let final_import = read(72)?;
        let final_export = read(72)?;
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                construct_state,
                fire_intensity,
                fire_damage,
                production_state,
                health,
                child_health,
                senior_health,
                happiness,
                citizen_count,
                temp_import,
                temp_export,
                final_import,
                final_export,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns raw construction-state bytes.
    pub fn construct_state(&self) -> &[u8] {
        &self.construct_state
    }
    /// Returns raw fire-intensity bytes from version 35 onward.
    pub fn fire_intensity(&self) -> &[u8] {
        &self.fire_intensity
    }
    /// Returns raw fire-damage bytes from version 35 onward.
    pub fn fire_damage(&self) -> &[u8] {
        &self.fire_damage
    }
    /// Returns raw production-state bytes from version 81 onward.
    pub fn production_state(&self) -> &[u8] {
        &self.production_state
    }
    /// Returns raw health bytes from version 55 onward.
    pub fn health(&self) -> &[u8] {
        &self.health
    }
    /// Returns raw child-health bytes from version 113004 onward.
    pub fn child_health(&self) -> &[u8] {
        &self.child_health
    }
    /// Returns raw senior-health bytes from version 113004 onward.
    pub fn senior_health(&self) -> &[u8] {
        &self.senior_health
    }
    /// Returns raw happiness bytes from version 55 onward.
    pub fn happiness(&self) -> &[u8] {
        &self.happiness
    }
    /// Returns raw citizen-count bytes from version 55 onward.
    pub fn citizen_count(&self) -> &[u8] {
        &self.citizen_count
    }
    /// Returns raw temporary-import bytes from version 72 onward.
    pub fn temp_import(&self) -> &[u8] {
        &self.temp_import
    }
    /// Returns raw temporary-export bytes from version 72 onward.
    pub fn temp_export(&self) -> &[u8] {
        &self.temp_export
    }
    /// Returns raw final-import bytes from version 72 onward.
    pub fn final_import(&self) -> &[u8] {
        &self.final_import
    }
    /// Returns raw final-export bytes from version 72 onward.
    pub fn final_export(&self) -> &[u8] {
        &self.final_export
    }
    /// Writes the original encoded fields exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl BuildingProblemPrefix {
    /// Decodes versioned problem state for each active building.
    ///
    /// From inner version 115000 the game serializes two encoded 64-bit words.
    /// Versions 221–114999 use one 64-bit or 32-bit value depending on the
    /// saved-application version; earlier supported versions use one 32-bit
    /// value. A save-app version is required between inner versions 221 and
    /// 114999. The original encoded bytes are retained for exact writing.
    pub fn decode_prefix(
        format_version: u32,
        save_app_version: Option<u32>,
        flags: &BuildingFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager problem words".to_owned(),
                offset: 0,
                reason: "unsupported or mismatched inner version".to_owned(),
            });
        }
        if (221..TWO_BUILDING_PROBLEMS_VERSION).contains(&format_version)
            && save_app_version.is_none()
        {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager problem words".to_owned(),
                offset: 0,
                reason: "save-app version is required for the legacy problem encoding".to_owned(),
            });
        }
        let active_indices = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .map(|building| building.index)
            .collect::<Vec<_>>();
        let problem_count = active_indices.len();
        let mut reader = Reader::new(bytes);
        let mut problems = Vec::with_capacity(problem_count);
        if format_version >= TWO_BUILDING_PROBLEMS_VERSION {
            let word_count = problem_count
                .checked_mul(2)
                .ok_or(CrpError::ArithmeticOverflow(
                    "BuildingManager problem word count",
                ))?;
            let words = read_encoded_ulong_array(&mut reader, word_count)?;
            for (index, pair) in active_indices.into_iter().zip(words.chunks_exact(2)) {
                problems.push(BuildingProblemWords {
                    index,
                    first: pair[0],
                    second: pair[1],
                });
            }
        } else if format_version >= 221
            && save_app_version.is_some_and(|version| version >= SAVE_APP_PROBLEMS_VERSION)
        {
            let words = read_encoded_ulong_array(&mut reader, problem_count)?;
            for (index, first) in active_indices.into_iter().zip(words) {
                problems.push(BuildingProblemWords {
                    index,
                    first,
                    second: 0,
                });
            }
        } else {
            let words = read_encoded_uint_array(&mut reader, problem_count)?;
            for (index, raw) in active_indices.into_iter().zip(words) {
                let raw = u64::from(raw);
                let first = ((raw & 0xc000_0000) << 32) | (raw & 0x3fff_ffff);
                problems.push(BuildingProblemWords {
                    index,
                    first,
                    second: 0,
                });
            }
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                problems,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns one opaque word pair for each active building, in buffer order.
    pub fn problems(&self) -> &[BuildingProblemWords] {
        &self.problems
    }

    /// Writes the original encoded array exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

fn fixed_object_header_count(format_version: u32) -> usize {
    let mut count = 3 * usize::from(format_version >= 82)
        + usize::from(format_version >= 198)
        + 3 * usize::from(format_version >= 87)
        + 2 * usize::from(format_version >= 156)
        + usize::from(format_version >= 166)
        + usize::from(format_version >= 160)
        + 2 * usize::from(format_version >= 188)
        + 6 * usize::from(format_version >= 237)
        + 5 * usize::from(format_version >= 295)
        + usize::from(format_version >= 304)
        + usize::from(format_version >= 309)
        + 3 * usize::from(format_version >= 110_022)
        + 7 * usize::from(format_version >= 111_008)
        + usize::from(format_version >= 112_000)
        + 4 * usize::from(format_version >= 112_017)
        + 13 * usize::from(format_version >= 113_011)
        + usize::from(format_version >= 113_014)
        + 5 * usize::from(format_version >= 116_017)
        + 4 * usize::from(format_version >= 117_006)
        + 6 * usize::from(format_version >= 121_033);
    count += usize::from(
        (109_012..110_000).contains(&format_version)
            || (110_025..111_000).contains(&format_version)
            || format_version >= 111_002,
    );
    count
}

impl BuildingManagerObjectHeadersPrefix {
    /// Decodes queued object headers and aggregate problem fields.
    /// The returned prefix ends before later per-building serialized arrays.
    pub fn decode_prefix(
        format_version: u32,
        prefabs: &BuildingPrefabPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || prefabs.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager object headers".to_owned(),
                offset: 0,
                reason: "unsupported or mismatched inner version".to_owned(),
            });
        }
        let fixed_count = fixed_object_header_count(format_version);
        let header_count = prefabs
            .guide_indices()
            .len()
            .checked_add(prefabs.milestone_indices().len())
            .and_then(|count| count.checked_add(fixed_count))
            .ok_or(CrpError::ArithmeticOverflow(
                "BuildingManager object-header count",
            ))?;
        let mut reader = Reader::new(bytes);
        let mut object_headers = Vec::with_capacity(header_count);
        for _ in 0..header_count {
            object_headers.push(read_net_object_header(&mut reader)?);
        }
        let problem_field_bytes = if format_version >= 115_000 {
            4 * 8
        } else if format_version >= 134 {
            2 * 4
        } else {
            0
        };
        let trailing_problem_fields = reader.read_bytes(problem_field_bytes)?.to_vec();
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                object_headers,
                trailing_problem_fields,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns headers in guide, milestone, and fixed field order.
    pub fn object_headers(&self) -> &[NetObjectHeader] {
        &self.object_headers
    }

    /// Returns the original serialized building-problem fields after headers.
    pub fn trailing_problem_fields(&self) -> &[u8] {
        &self.trailing_problem_fields
    }

    /// Writes the original headers and trailing fields exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::fixed_object_header_count;

    #[test]
    fn building_object_header_count_tracks_version_gates() {
        for (version, expected) in [
            (164, 9),
            (166, 10),
            (188, 12),
            (198, 13),
            (237, 19),
            (295, 24),
            (304, 25),
            (309, 26),
            (110_022, 29),
            (110_025, 30),
            (111_000, 29),
            (111_001, 29),
            (111_002, 30),
            (111_008, 37),
            (112_000, 38),
            (112_017, 42),
            (112_026, 42),
            (112_027, 42),
            (113_011, 55),
            (113_014, 56),
            (116_017, 61),
            (117_006, 65),
            (121_033, 71),
        ] {
            assert_eq!(
                fixed_object_header_count(version),
                expected,
                "version {version}"
            );
        }
    }
}

impl BuildingPrefabPrefix {
    /// Decodes prefab indices for active buildings, guides, and milestones.
    ///
    /// `bytes` begins immediately after the two big-endian `u16` guide and
    /// milestone counts. `flags` supplies the active building positions.
    pub fn decode_prefix(
        format_version: u32,
        flags: &BuildingFlagsPrefix,
        guide_count: u16,
        milestone_count: u16,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || format_version != flags.format_version() {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager prefab indices".to_owned(),
                offset: 0,
                reason: "unsupported or mismatched inner version".to_owned(),
            });
        }
        let mut reader = Reader::new(bytes);
        let building_count = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .count();
        let building_indices = read_encoded_ushort_array(&mut reader, building_count)?;
        let guide_indices = read_encoded_ushort_array(&mut reader, usize::from(guide_count))?;
        let milestone_indices =
            read_encoded_ushort_array(&mut reader, usize::from(milestone_count))?;
        let name_count = usize::from(reader.read_u16_be()?);
        let mut names = Vec::with_capacity(name_count);
        for _ in 0..name_count {
            names.push(reader.read_unique_string()?);
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                building_indices,
                guide_indices,
                milestone_indices,
                names,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns indices corresponding to nonzero primary building flags.
    pub fn building_indices(&self) -> &[u16] {
        &self.building_indices
    }

    /// Returns prefab indices for guide entries.
    pub fn guide_indices(&self) -> &[u16] {
        &self.guide_indices
    }

    /// Returns prefab indices for milestone entries.
    pub fn milestone_indices(&self) -> &[u16] {
        &self.milestone_indices
    }

    /// Returns the prefab collection names, preserving null entries.
    pub fn names(&self) -> &[Option<String>] {
        &self.names
    }

    /// Writes the original encoded tables and names exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl BuildingFlagsPrefix {
    /// Decodes building flags for inner versions 164 and newer.
    ///
    /// Versions before 222 read 32,767 primary positions; later versions read
    /// 49,151. A second full-length array begins at 112030. Omitted positions
    /// are zero-filled. Truncated or malformed encoded arrays return an error.
    /// The returned length ends before guide and milestone counts.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager+Data flags".to_owned(),
                offset: 0,
                reason: "unsupported inner version before 164".to_owned(),
            });
        }
        let mut reader = Reader::new(bytes);
        let primary_count = if format_version < FULL_BUILDING_COUNT_VERSION {
            LEGACY_BUILDING_COUNT - 1
        } else {
            BUILDING_COUNT - 1
        };
        let primary = read_encoded_uint_array(&mut reader, primary_count)?;
        let secondary = if format_version >= SECONDARY_BUILDING_FLAGS_VERSION {
            Some(read_encoded_uint_array(&mut reader, BUILDING_COUNT - 1)?)
        } else {
            None
        };
        let mut building_flags = Vec::with_capacity(BUILDING_COUNT - 1);
        for index in 0..BUILDING_COUNT - 1 {
            building_flags.push(BuildingFlag {
                index: u16::try_from(index + 1)
                    .map_err(|_| CrpError::ArithmeticOverflow("building buffer index"))?,
                flags: primary.get(index).copied().unwrap_or(0),
                flags2: secondary
                    .as_ref()
                    .and_then(|flags| flags.get(index))
                    .copied()
                    .unwrap_or(0),
            });
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                building_flags,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns flags for one-based building buffer positions 1 through 49151.
    pub fn building_flags(&self) -> &[BuildingFlag] {
        &self.building_flags
    }

    /// Writes the original encoded arrays exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}
