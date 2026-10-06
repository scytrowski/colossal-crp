use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_uint_array, read_encoded_ushort_array};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

const BUILDING_COUNT: usize = 49_152;
const LEGACY_BUILDING_COUNT: usize = 32_768;
const FULL_BUILDING_COUNT_VERSION: u32 = 222;
const SECONDARY_BUILDING_FLAGS_VERSION: u32 = 112_030;

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
            || (110_025..111_000).contains(&format_version),
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
            (111_002, 29),
            (111_008, 36),
            (112_000, 37),
            (112_017, 41),
            (113_011, 54),
            (113_014, 55),
            (116_017, 60),
            (117_006, 64),
            (121_033, 70),
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
