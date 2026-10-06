use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_uint_array, read_encoded_ushort_array};

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
