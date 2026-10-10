use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    read_encoded_byte_array, read_encoded_int_array, read_encoded_uint_array,
    read_encoded_ulong_array, read_encoded_ushort_array, write_encoded_byte_array,
    write_encoded_uint_array, write_encoded_ulong_array, write_encoded_ushort_array,
};

const LANE_COUNT: usize = 262_144;
const NODE_COUNT: usize = 32_768;
const SEGMENT_COUNT: usize = 36_864;
const LEGACY_SEGMENT_COUNT: usize = 32_768;
const TWO_NODE_FLAGS_VERSION: u32 = 115_018;
const FULL_NODE_COUNT_VERSION: u32 = 202;
const FULL_SEGMENT_COUNT_VERSION: u32 = 222;
const TWO_SEGMENT_FLAGS_VERSION: u32 = 114_012;
const TWO_PROBLEMS_VERSION: u32 = 115_000;
const SEGMENT_PROBLEMS_VERSION: u32 = 226;
const INT_TRAFFIC_LIGHT_VERSION: u32 = 115_017;
const FIRE_COVERAGE_VERSION: u32 = 185;
const WETNESS_VERSION: u32 = 224;
const SEGMENT_CONDITION_VERSION: u32 = 229;
const SEGMENT_NOISE_VERSION: u32 = 109_007;
const SEGMENT_ATTRACTIVENESS_VERSION: u32 = 121_030;
const SEGMENT_NAME_SEED_VERSION: u32 = 312;
const SEGMENT_PATH_OVERRIDE_VERSION: u32 = 113_015;
const SEGMENT_TREE_VERSION: u32 = 114_002;
const SEGMENT_FENCE_VERSION: u32 = 120_003;
const SEGMENT_EVENT_ROUTE_VERSION: u32 = 121_005;
const ADJUSTED_SEGMENT_WORDS: usize = 576;
const NODE_EVENT_ROUTE_VERSION: u32 = 121_007;
const MAX_NODE_SEGMENTS: usize = 8;
const SAVE_APP_PROBLEMS_VERSION: u32 = (1 << 27) + (3 << 22) + (1 << 13) + (1 << 8);

/// One active lane's serialized flags, link, and reservation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetLane {
    /// One-based index in the game's fixed lane buffer.
    pub index: u32,
    /// Raw lane flags from the encoded array.
    pub flags: u16,
    /// Next lane ID, stored as a 24-bit integer.
    pub next_lane: u32,
    /// Previous reserved position; present from inner version 163.
    pub reserved_prev: u8,
    /// Next reserved position; present from inner version 163.
    pub reserved_next: u8,
    /// Reservation frame; present from inner version 163.
    pub reserved_frame: u8,
    /// Last reservation ID; present from inner version 206.
    pub last_reserve_id: u16,
}

/// The lane section of `NetManager+Data`, before node flags.
///
/// This is a partial manager body. Later fields remain in the opaque manager
/// graph and are not claimed to be decoded by this prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetLanePrefix {
    format_version: u32,
    lane_flags: Vec<u16>,
    active_lanes: Vec<NetLane>,
    reservation_prefix: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Editor for existing lane reservation state.
///
/// It keeps lane flags and next-lane links byte-exact. If a reservation value
/// changes, the reservation arrays are re-encoded canonically and may change
/// this prefix's byte length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetLaneEditor {
    format_version: u32,
    active_count: usize,
    reservation_prefix: Vec<u8>,
    reserved_prev: Vec<u8>,
    reserved_next: Vec<u8>,
    reserved_frame: Vec<u8>,
    last_reserve_ids: Option<Vec<u16>>,
    original_reserved_prev: Vec<u8>,
    original_reserved_next: Vec<u8>,
    original_reserved_frame: Vec<u8>,
    original_last_reserve_ids: Option<Vec<u16>>,
    original_prefix: Vec<u8>,
}

/// Serialized flags for one one-based `NetNode` buffer position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeFlag {
    /// One-based node buffer index.
    pub index: u16,
    /// Primary node flags.
    pub flags: u32,
    /// Secondary node flags, zero before inner version 115018.
    pub flags2: u32,
}

/// The encoded node-flag array immediately after NetManager's lane section.
///
/// Later fields can be decoded by separate prefixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeFlagsPrefix {
    format_version: u32,
    node_flags: Vec<NetNodeFlag>,
    serialized_prefix: Vec<u8>,
}

/// Problem bits for one node with nonzero primary flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeProblem {
    /// One-based node buffer index.
    pub index: u16,
    /// First 64 problem bits after the game's legacy bit relocation.
    pub problems1: u64,
    /// Second 64 problem bits, zero before inner version 115000.
    pub problems2: u64,
}

/// Version-dependent node problem array following the node flags.
///
/// Later fields can be decoded by separate prefixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeProblemsPrefix {
    format_version: u32,
    save_app_version: Option<u32>,
    problems: Vec<NetNodeProblem>,
    serialized_prefix: Vec<u8>,
}

/// Wait time and two counters for one active network node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeCounter {
    /// One-based node buffer index.
    pub index: u16,
    /// Maximum wait time for the node.
    pub max_wait_time: u8,
    /// Temporary counter value.
    pub temp_counter: u16,
    /// Final counter value.
    pub final_counter: u16,
}

/// Sparse node counters following the problem array in `NetManager+Data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeCountersPrefix {
    format_version: u32,
    counters: Vec<NetNodeCounter>,
    serialized_prefix: Vec<u8>,
}

/// Fields stored for one node with nonzero primary flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeRecord {
    /// One-based node buffer index.
    pub index: u16,
    /// Exact IEEE 754 bits of the X, Y, and Z coordinates.
    pub position_bits: [u32; 3],
    /// Segment IDs in their serialized order, with at most eight entries.
    pub segments: Vec<u16>,
    /// Build index stored as a big-endian `u32`.
    pub build_index: u32,
    /// Associated building ID.
    pub building: u16,
    /// Next building-node ID.
    pub next_building_node: u16,
    /// Lane ID stored in 24 bits.
    pub lane: u32,
    /// Position along the lane.
    pub lane_offset: u8,
    /// Node elevation.
    pub elevation: u8,
    /// Height offset.
    pub height_offset: u8,
    /// Connection count.
    pub connect_count: u8,
    /// Event route ID, zero before inner version 121007.
    pub event_route: u16,
}

/// Variable-length records following NetManager's sparse node counters.
///
/// The segment arrays and remaining NetManager fields follow this prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeRecordsPrefix {
    format_version: u32,
    records: Vec<NetNodeRecord>,
    serialized_prefix: Vec<u8>,
}

/// Serialized flags for one one-based `NetSegment` buffer position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentFlag {
    /// One-based segment buffer index.
    pub index: u16,
    /// Primary segment flags.
    pub flags: u32,
    /// Secondary segment flags, zero before inner version 114012.
    pub flags2: u32,
}

/// Encoded segment-flag arrays following the individual node records.
///
/// Later segment problem arrays and records remain in the opaque graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentFlagsPrefix {
    format_version: u32,
    segment_flags: Vec<NetSegmentFlag>,
    serialized_prefix: Vec<u8>,
}

/// Editor for the versioned primary and secondary NetManager segment flags.
///
/// The zero/nonzero pattern of primary flags must remain unchanged because
/// later segment arrays contain one record per active segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentFlagsEditor {
    format_version: u32,
    primary_flags: Vec<u32>,
    secondary_flags: Option<Vec<u32>>,
    original_primary_flags: Vec<u32>,
    original_secondary_flags: Option<Vec<u32>>,
    original_prefix: Vec<u8>,
}

/// Problem bits for one segment with nonzero primary flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentProblem {
    /// One-based segment buffer index.
    pub index: u16,
    /// First 64 problem bits.
    pub problems1: u64,
    /// Second 64 problem bits, zero before inner version 115000.
    pub problems2: u64,
}

/// Version-dependent segment problem array following the segment flags.
///
/// Later segment fields remain in the opaque manager graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentProblemsPrefix {
    format_version: u32,
    problems: Vec<NetSegmentProblem>,
    source_segment_flags: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Editor for versioned problem-bit arrays attached to active segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentProblemsEditor {
    format_version: u32,
    problems1: Vec<u64>,
    problems2: Option<Vec<u64>>,
    original_problems1: Vec<u64>,
    original_problems2: Option<Vec<u64>>,
    original_segment_flags: Vec<u8>,
    original_prefix: Vec<u8>,
}

/// Traffic-light state for one segment with nonzero primary flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentTrafficLight {
    /// One-based segment buffer index.
    pub index: u16,
    /// First traffic-light state value.
    pub state0: i32,
    /// Second traffic-light state value.
    pub state1: i32,
}

/// Sparse traffic-light state array following segment problems.
///
/// Later coverage and segment records remain in the opaque manager graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentTrafficLightsPrefix {
    format_version: u32,
    states: Vec<NetSegmentTrafficLight>,
    serialized_prefix: Vec<u8>,
}

/// Environmental byte fields for one segment with nonzero primary flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentCoverage {
    /// One-based segment buffer index.
    pub index: u16,
    /// Fire coverage; zero before inner version 185.
    pub fire_coverage: u8,
    /// Wetness; zero before inner version 224.
    pub wetness: u8,
}

/// Sparse fire-coverage and wetness arrays following traffic-light state.
///
/// Later segment fields remain in the opaque manager graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentCoveragePrefix {
    format_version: u32,
    coverage: Vec<NetSegmentCoverage>,
    serialized_prefix: Vec<u8>,
}

/// Later environmental fields for one segment with nonzero primary flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentEnvironment {
    /// One-based segment buffer index.
    pub index: u16,
    /// Condition; zero before inner version 229.
    pub condition: u8,
    /// Noise buffer; zero before inner version 109007.
    pub noise_buffer: u16,
    /// Noise density; zero before inner version 109007.
    pub noise_density: u8,
    /// Attractiveness; zero before inner version 121030.
    pub attractiveness: u8,
}

/// Sparse condition, noise, and attractiveness arrays after wetness.
///
/// The original encoded bytes are retained for exact rewriting. Individual
/// segment records follow this prefix and remain outside its scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentEnvironmentPrefix {
    format_version: u32,
    environment: Vec<NetSegmentEnvironment>,
    serialized_prefix: Vec<u8>,
}

/// Fixed fields stored for one segment with nonzero primary flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentRecord {
    /// One-based segment buffer index.
    pub index: u16,
    /// Build and modified indices, stored as big-endian `u32` values.
    pub build_index: u32,
    pub modified_index: u32,
    /// First lane and path IDs, each stored in 24 bits.
    pub lanes: u32,
    pub path: u32,
    /// Exact IEEE 754 bits of the start and end direction vectors.
    pub start_direction_bits: [u32; 3],
    pub end_direction_bits: [u32; 3],
    /// Start and end node IDs.
    pub start_node: u16,
    pub end_node: u16,
    /// Block IDs in start-left, start-right, end-left, end-right order.
    pub blocks: [u16; 4],
    pub traffic_buffer: u16,
    pub traffic_density: u8,
    /// Zero before inner version 312.
    pub name_seed: u16,
    /// Raw bool byte, zero before inner version 113015.
    pub override_path_find_direction_limit: u8,
    /// Raw bool byte, zero before inner version 114002.
    pub tree_present: u8,
    /// Raw bool bytes, zero before inner version 120003.
    pub fence_present: [u8; 2],
    /// Zero before inner version 121005.
    pub event_route: u16,
    pub next_event_route_segment: u16,
}

/// Individual active-segment records after the sparse environment arrays.
///
/// The original bytes are retained for exact rewriting. Later NetManager
/// tables are outside this prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentRecordsPrefix {
    format_version: u32,
    records: Vec<NetSegmentRecord>,
    serialized_prefix: Vec<u8>,
}

/// Counts of saved network guide and milestone prefab references.
///
/// These two big-endian `u16` values follow the individual segment records
/// from inner version 128. Later prefab-reference tables remain opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetGuideCountsPrefix {
    format_version: u32,
    guide_count: u16,
    milestone_count: u16,
    serialized_prefix: Vec<u8>,
}

/// The first prefab-reference table after NetManager's guide counts.
///
/// Indices align with active node and segment buffers; guide and milestone
/// indices align with their preceding counts. Names may be null. Original
/// encoded bytes are retained for exact writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetInfoPrefabPrefix {
    format_version: u32,
    node_info_indices: Vec<u16>,
    segment_info_indices: Vec<u16>,
    guide_indices: Vec<u16>,
    milestone_indices: Vec<u16>,
    names: Vec<Option<String>>,
    serialized_prefix: Vec<u8>,
}

/// Optional tree-prefab indices and names after the network-info table.
///
/// Indices align with active segments whose on-disk tree-present bool is one.
/// Earlier versions contain no bytes for this table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetTreePrefabPrefix {
    format_version: u32,
    indices: Vec<u16>,
    names: Vec<Option<String>>,
    serialized_prefix: Vec<u8>,
}

/// Left and right fence prefab tables after the tree-prefab table.
///
/// Each index list aligns with active segments whose corresponding on-disk
/// fence-present bool is one. The original bytes are retained for writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetFencePrefabPrefix {
    format_version: u32,
    left_indices: Vec<u16>,
    left_names: Vec<Option<String>>,
    right_indices: Vec<u16>,
    right_names: Vec<Option<String>>,
    serialized_prefix: Vec<u8>,
}

/// One DataSerializer object header in the NetManager tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetObjectHeader {
    /// Null object marker (`FF`).
    Null,
    /// New object with an inline assembly-qualified type name.
    NewType(String),
    /// New object referring to an earlier shared type by 16-bit index.
    NewSharedType(u16),
    /// Existing object referring to an earlier object by 24-bit index.
    Existing(u32),
}

/// Guide object headers and final scalar fields of `NetManager+Data`.
///
/// Newly declared object bodies are queued elsewhere in DataSerializer and
/// remain opaque. This prefix retains every header and scalar byte exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetManagerTailPrefix {
    format_version: u32,
    object_headers: Vec<NetObjectHeader>,
    last_max_wetness: u8,
    current_max_wetness: u8,
    name_modified_count: u32,
    adjusted_segments: Vec<u64>,
    profile_end_shared_string_index: Option<u16>,
    serialized_prefix: Vec<u8>,
}

/// One queued `NetNodeInstanceGuide` body decoded according to its version gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeInstanceGuideBody {
    format_version: u32,
    save_app_version: Option<u32>,
    next_repeat: u32,
    activation_frame: u32,
    activation_count: u8,
    display_count: u8,
    disabled: bool,
    problems: u64,
    build_index: u32,
    node_id: u16,
    serialized_body: Vec<u8>,
}

/// Editable fields for a queued `NetNodeInstanceGuide` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeInstanceGuideEditor {
    original: NetNodeInstanceGuideBody,
    save_app_version: Option<u32>,
    next_repeat: u32,
    activation_frame: u32,
    activation_count: u8,
    display_count: u8,
    disabled: bool,
    problems: u64,
    build_index: u32,
    node_id: u16,
}

impl NetLanePrefix {
    /// Decodes lane flags, links, and reservations for inner versions 164+.
    /// Returns bytes consumed, excluding the loading-profiler opening tag.
    /// Truncated encoded arrays or link lists return errors.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(invalid(
                0,
                "NetManager lane prefix requires inner version 164 or newer",
            ));
        }
        let mut reader = Reader::new(bytes);
        let lane_flags = read_encoded_ushort_array(&mut reader, LANE_COUNT - 1)?;
        let active_count = lane_flags.iter().filter(|flags| **flags != 0).count();
        if active_count > reader.remaining() / 3 {
            return Err(reader.invalid("NetManager lane links cannot fit in remaining bytes"));
        }
        let mut active_lanes = Vec::with_capacity(active_count);
        for (offset, &flags) in lane_flags.iter().enumerate() {
            if flags == 0 {
                continue;
            }
            let bytes = reader.read_bytes(3)?;
            let next_lane =
                (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
            active_lanes.push(NetLane {
                index: u32::try_from(offset + 1)
                    .map_err(|_| CrpError::ArithmeticOverflow("NetLane index"))?,
                flags,
                next_lane,
                reserved_prev: 0,
                reserved_next: 0,
                reserved_frame: 0,
                last_reserve_id: 0,
            });
        }
        let reservation_start = reader.offset();
        for field in 0..3 {
            let values = read_encoded_byte_array(&mut reader, active_count)?;
            for (lane, value) in active_lanes.iter_mut().zip(values) {
                match field {
                    0 => lane.reserved_prev = value,
                    1 => lane.reserved_next = value,
                    _ => lane.reserved_frame = value,
                }
            }
        }
        if format_version >= 206 {
            let values = read_encoded_ushort_array(&mut reader, active_count)?;
            for (lane, value) in active_lanes.iter_mut().zip(values) {
                lane.last_reserve_id = value;
            }
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                lane_flags,
                active_lanes,
                reservation_prefix: bytes[..reservation_start].to_vec(),
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns lane flags in one-based buffer order.
    pub fn lane_flags(&self) -> &[u16] {
        &self.lane_flags
    }

    /// Returns nonzero-flag lanes and their next-lane links in buffer order.
    pub fn active_lanes(&self) -> &[NetLane] {
        &self.active_lanes
    }

    /// Writes the original prefix bytes without changing encoded-array runs.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Consumes the prefix to edit reservation values for active lanes.
    pub fn into_editor(self) -> NetLaneEditor {
        let reserved_prev = self
            .active_lanes
            .iter()
            .map(|lane| lane.reserved_prev)
            .collect::<Vec<_>>();
        let reserved_next = self
            .active_lanes
            .iter()
            .map(|lane| lane.reserved_next)
            .collect::<Vec<_>>();
        let reserved_frame = self
            .active_lanes
            .iter()
            .map(|lane| lane.reserved_frame)
            .collect::<Vec<_>>();
        let last_reserve_ids = (self.format_version >= 206).then(|| {
            self.active_lanes
                .iter()
                .map(|lane| lane.last_reserve_id)
                .collect::<Vec<_>>()
        });
        NetLaneEditor {
            format_version: self.format_version,
            active_count: self.active_lanes.len(),
            reservation_prefix: self.reservation_prefix,
            original_reserved_prev: reserved_prev.clone(),
            original_reserved_next: reserved_next.clone(),
            original_reserved_frame: reserved_frame.clone(),
            original_last_reserve_ids: last_reserve_ids.clone(),
            reserved_prev,
            reserved_next,
            reserved_frame,
            last_reserve_ids,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetLaneEditor {
    /// Returns the inner DataSerializer version associated with this prefix.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns mutable previous-reservation values in active-lane order.
    pub fn reserved_prev_mut(&mut self) -> &mut [u8] {
        &mut self.reserved_prev
    }

    /// Returns mutable next-reservation values in active-lane order.
    pub fn reserved_next_mut(&mut self) -> &mut [u8] {
        &mut self.reserved_next
    }

    /// Returns mutable reservation-frame values in active-lane order.
    pub fn reserved_frame_mut(&mut self) -> &mut [u8] {
        &mut self.reserved_frame
    }

    /// Returns mutable last-reservation IDs in active-lane order from version 206.
    ///
    /// Returns `None` for earlier versions, which omit this array.
    pub fn last_reserve_ids_mut(&mut self) -> Option<&mut [u16]> {
        self.last_reserve_ids.as_deref_mut()
    }

    /// Encodes the original lane prefix with edited reservation arrays.
    ///
    /// If no reservation value changed, the original encoded bytes are
    /// returned exactly. After an edit, all present reservation arrays use
    /// canonical array controls; flags and next-lane links remain byte-exact.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        self.validate_column_lengths()?;
        if self.reserved_prev == self.original_reserved_prev
            && self.reserved_next == self.original_reserved_next
            && self.reserved_frame == self.original_reserved_frame
            && self.last_reserve_ids == self.original_last_reserve_ids
        {
            return Ok(self.original_prefix.clone());
        }

        let mut output = self.reservation_prefix.clone();
        write_encoded_byte_array(&mut output, &self.reserved_prev);
        write_encoded_byte_array(&mut output, &self.reserved_next);
        write_encoded_byte_array(&mut output, &self.reserved_frame);
        if let Some(last_reserve_ids) = &self.last_reserve_ids {
            write_encoded_ushort_array(&mut output, last_reserve_ids);
        }
        Ok(output)
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original_prefix
    }

    fn validate_column_lengths(&self) -> Result<(), CrpError> {
        if self.reserved_prev.len() != self.active_count
            || self.reserved_next.len() != self.active_count
            || self.reserved_frame.len() != self.active_count
            || self
                .last_reserve_ids
                .as_ref()
                .is_some_and(|values| values.len() != self.active_count)
        {
            return Err(invalid(
                0,
                "edited reservation count does not match active lanes",
            ));
        }
        if (self.format_version >= 206) != self.last_reserve_ids.is_some() {
            return Err(invalid(
                0,
                "last-reservation IDs do not match this NetManager version",
            ));
        }
        Ok(())
    }
}

impl NetNodeFlagsPrefix {
    /// Decodes node flags for inner versions 164 and newer and returns bytes used.
    /// Versions below 202 serialize only indices 1–16383; later indices are
    /// filled with zero. Versions 115018+ serialize two flags per node.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(invalid(
                0,
                "NetManager node flags require inner version 164 or newer",
            ));
        }
        let mut reader = Reader::new(bytes);
        let node_count = if format_version < FULL_NODE_COUNT_VERSION {
            16_383
        } else {
            NODE_COUNT - 1
        };
        let paired = format_version >= TWO_NODE_FLAGS_VERSION;
        let encoded_count = if paired {
            node_count
                .checked_mul(2)
                .ok_or(CrpError::ArithmeticOverflow("NetManager node flag count"))?
        } else {
            node_count
        };
        let encoded = read_encoded_uint_array(&mut reader, encoded_count)?;
        let mut node_flags = Vec::with_capacity(NODE_COUNT - 1);
        for index in 0..node_count {
            let (flags, flags2) = if paired {
                (encoded[index * 2], encoded[index * 2 + 1])
            } else {
                (encoded[index], 0)
            };
            node_flags.push(NetNodeFlag {
                index: u16::try_from(index + 1)
                    .map_err(|_| CrpError::ArithmeticOverflow("NetNode index"))?,
                flags,
                flags2,
            });
        }
        for index in node_count..NODE_COUNT - 1 {
            node_flags.push(NetNodeFlag {
                index: u16::try_from(index + 1)
                    .map_err(|_| CrpError::ArithmeticOverflow("NetNode index"))?,
                flags: 0,
                flags2: 0,
            });
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                node_flags,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns flags for all 32,767 one-based node positions.
    pub fn node_flags(&self) -> &[NetNodeFlag] {
        &self.node_flags
    }

    /// Writes the original encoded node-flag bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetNodeProblemsPrefix {
    /// Decodes one or two problem values for each active node.
    ///
    /// From inner version 115000, the stream contains two `EncodedArray.ULong`
    /// values per active node. Earlier version 221+ streams use one ULong only
    /// when `save_app_version` is at least 1.3.0.1.1.0; otherwise one UInt
    /// value is relocated into the legacy 64-bit problem representation.
    /// Pass the first type-52 root's stored save-app version for this gate.
    pub fn decode_prefix(
        format_version: u32,
        save_app_version: Option<u32>,
        node_flags: &NetNodeFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || node_flags.format_version() != format_version {
            return Err(invalid(
                0,
                "NetManager problem version does not match node flags",
            ));
        }
        if (221..TWO_PROBLEMS_VERSION).contains(&format_version) && save_app_version.is_none() {
            return Err(invalid(
                0,
                "save-app version is required for NetManager problems",
            ));
        }
        let active_indices = node_flags
            .node_flags()
            .iter()
            .filter(|node| node.flags != 0)
            .map(|node| node.index)
            .collect::<Vec<_>>();
        let active_count = active_indices.len();
        let mut reader = Reader::new(bytes);
        let mut problems = Vec::with_capacity(active_count);
        if format_version >= TWO_PROBLEMS_VERSION {
            let count = active_count
                .checked_mul(2)
                .ok_or(CrpError::ArithmeticOverflow("NetManager problem count"))?;
            let values = read_encoded_ulong_array(&mut reader, count)?;
            for (index, pair) in active_indices.into_iter().zip(values.chunks_exact(2)) {
                problems.push(NetNodeProblem {
                    index,
                    problems1: pair[0],
                    problems2: pair[1],
                });
            }
        } else if format_version >= 221
            && save_app_version.is_some_and(|version| version >= SAVE_APP_PROBLEMS_VERSION)
        {
            let values = read_encoded_ulong_array(&mut reader, active_count)?;
            for (index, value) in active_indices.into_iter().zip(values) {
                problems.push(NetNodeProblem {
                    index,
                    problems1: value,
                    problems2: 0,
                });
            }
        } else {
            let values = read_encoded_uint_array(&mut reader, active_count)?;
            for (index, value) in active_indices.into_iter().zip(values) {
                let value = u64::from(value);
                // The older reader moves bits 30–31 to problem bits 62–63.
                let relocated = ((value & 0xc000_0000) << 32) | (value & 0x3fff_ffff);
                problems.push(NetNodeProblem {
                    index,
                    problems1: relocated,
                    problems2: 0,
                });
            }
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                save_app_version,
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

    /// Returns the save-app version supplied for the legacy format gate.
    pub fn save_app_version(&self) -> Option<u32> {
        self.save_app_version
    }

    /// Returns problem bits for nodes with nonzero primary flags.
    pub fn problems(&self) -> &[NetNodeProblem] {
        &self.problems
    }

    /// Writes the original encoded problem array exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetNodeCountersPrefix {
    /// Decodes one byte and two UShort values for each active node.
    ///
    /// This crate supports inner versions 164+, which include both arrays.
    /// The returned byte length ends before individual node records.
    pub fn decode_prefix(
        format_version: u32,
        node_flags: &NetNodeFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || node_flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data node counters".to_owned(),
                offset: 0,
                reason: "counter version does not match node flags".to_owned(),
            });
        }
        let active_indices = node_flags
            .node_flags()
            .iter()
            .filter(|node| node.flags != 0)
            .map(|node| node.index)
            .collect::<Vec<_>>();
        let active_count = active_indices.len();
        let mut reader = Reader::new(bytes);
        let max_wait_times = read_encoded_byte_array(&mut reader, active_count)?;
        let counter_count = active_count
            .checked_mul(2)
            .ok_or(CrpError::ArithmeticOverflow("NetManager counter count"))?;
        let counter_values = read_encoded_ushort_array(&mut reader, counter_count)?;
        let mut counters = Vec::with_capacity(active_count);
        for ((index, max_wait_time), pair) in active_indices
            .into_iter()
            .zip(max_wait_times)
            .zip(counter_values.chunks_exact(2))
        {
            counters.push(NetNodeCounter {
                index,
                max_wait_time,
                temp_counter: pair[0],
                final_counter: pair[1],
            });
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                counters,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns counters for nodes with nonzero primary flags.
    pub fn counters(&self) -> &[NetNodeCounter] {
        &self.counters
    }

    /// Writes the original encoded counter-array bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetNodeRecordsPrefix {
    /// Decodes one variable-length record for each active node, in index order.
    ///
    /// Supports inner versions 164 and newer. A count above eight segments,
    /// truncated fields, or mismatched flag version returns an error. The
    /// consumed length ends before the segment-flag array.
    pub fn decode_prefix(
        format_version: u32,
        node_flags: &NetNodeFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || node_flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data node records".to_owned(),
                offset: 0,
                reason: "record version does not match node flags".to_owned(),
            });
        }
        let mut reader = Reader::new(bytes);
        let mut records = Vec::new();
        for node in node_flags
            .node_flags()
            .iter()
            .filter(|node| node.flags != 0)
        {
            let position_bits = [
                reader.read_u32_be()?,
                reader.read_u32_be()?,
                reader.read_u32_be()?,
            ];
            let segment_count = usize::from(reader.read_u8()?);
            if segment_count > MAX_NODE_SEGMENTS {
                return Err(reader.invalid(format!(
                    "node {} has {segment_count} segments; maximum is {MAX_NODE_SEGMENTS}",
                    node.index
                )));
            }
            let mut segments = Vec::with_capacity(segment_count);
            for _ in 0..segment_count {
                segments.push(reader.read_u16_be()?);
            }
            let build_index = reader.read_u32_be()?;
            let building = reader.read_u16_be()?;
            let next_building_node = reader.read_u16_be()?;
            let lane_bytes = reader.read_bytes(3)?;
            let lane = (u32::from(lane_bytes[0]) << 16)
                | (u32::from(lane_bytes[1]) << 8)
                | u32::from(lane_bytes[2]);
            let lane_offset = reader.read_u8()?;
            let elevation = reader.read_u8()?;
            let height_offset = reader.read_u8()?;
            let connect_count = reader.read_u8()?;
            let event_route = if format_version >= NODE_EVENT_ROUTE_VERSION {
                reader.read_u16_be()?
            } else {
                0
            };
            records.push(NetNodeRecord {
                index: node.index,
                position_bits,
                segments,
                build_index,
                building,
                next_building_node,
                lane,
                lane_offset,
                elevation,
                height_offset,
                connect_count,
                event_route,
            });
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                records,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns records for nodes with nonzero primary flags.
    pub fn records(&self) -> &[NetNodeRecord] {
        &self.records
    }

    /// Writes the original node-record bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetSegmentFlagsPrefix {
    /// Decodes primary flags and the version-gated secondary flag array.
    ///
    /// Inner versions before 222 store only indices 1–32767 in the primary
    /// array. Secondary flags are stored in a separate full-length array from
    /// version 114012. Returns bytes consumed before segment problems.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data segment flags".to_owned(),
                offset: 0,
                reason: "segment flags require inner version 164 or newer".to_owned(),
            });
        }
        let mut reader = Reader::new(bytes);
        let primary_count = if format_version < FULL_SEGMENT_COUNT_VERSION {
            LEGACY_SEGMENT_COUNT - 1
        } else {
            SEGMENT_COUNT - 1
        };
        let primary = read_encoded_uint_array(&mut reader, primary_count)?;
        let secondary = if format_version >= TWO_SEGMENT_FLAGS_VERSION {
            Some(read_encoded_uint_array(&mut reader, SEGMENT_COUNT - 1)?)
        } else {
            None
        };
        let mut segment_flags = Vec::with_capacity(SEGMENT_COUNT - 1);
        for index in 0..SEGMENT_COUNT - 1 {
            segment_flags.push(NetSegmentFlag {
                index: u16::try_from(index + 1)
                    .map_err(|_| CrpError::ArithmeticOverflow("NetSegment index"))?,
                flags: primary.get(index).copied().unwrap_or(0),
                flags2: secondary
                    .as_ref()
                    .and_then(|values| values.get(index).copied())
                    .unwrap_or(0),
            });
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                segment_flags,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns flags for all 36,863 one-based segment positions.
    pub fn segment_flags(&self) -> &[NetSegmentFlag] {
        &self.segment_flags
    }

    /// Writes the original encoded flag arrays exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Consumes the prefix to edit its serialized flag arrays.
    pub fn into_editor(self) -> NetSegmentFlagsEditor {
        let primary_count = if self.format_version < FULL_SEGMENT_COUNT_VERSION {
            LEGACY_SEGMENT_COUNT - 1
        } else {
            SEGMENT_COUNT - 1
        };
        let primary_flags = self.segment_flags[..primary_count]
            .iter()
            .map(|segment| segment.flags)
            .collect::<Vec<_>>();
        let secondary_flags = (self.format_version >= TWO_SEGMENT_FLAGS_VERSION).then(|| {
            self.segment_flags
                .iter()
                .map(|segment| segment.flags2)
                .collect::<Vec<_>>()
        });
        NetSegmentFlagsEditor {
            format_version: self.format_version,
            original_primary_flags: primary_flags.clone(),
            original_secondary_flags: secondary_flags.clone(),
            primary_flags,
            secondary_flags,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetSegmentFlagsEditor {
    /// Returns the inner DataSerializer version associated with this prefix.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns primary flags for positions serialized by this version.
    ///
    /// Keep each entry's zero/nonzero state unchanged; `encode` rejects edits
    /// that would change the sparse record counts in later NetManager fields.
    pub fn primary_flags_mut(&mut self) -> &mut [u32] {
        &mut self.primary_flags
    }

    /// Returns secondary flags from inner version 114012 onward.
    pub fn secondary_flags_mut(&mut self) -> Option<&mut [u32]> {
        self.secondary_flags.as_deref_mut()
    }

    /// Encodes the original flag arrays with any edits applied.
    ///
    /// Unchanged values preserve the original encoded bytes exactly. Changed
    /// arrays use canonical literal blocks. Primary flags may change bits but
    /// must retain each segment's original zero/nonzero state.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let primary_count = if self.format_version < FULL_SEGMENT_COUNT_VERSION {
            LEGACY_SEGMENT_COUNT - 1
        } else {
            SEGMENT_COUNT - 1
        };
        let secondary_count =
            (self.format_version >= TWO_SEGMENT_FLAGS_VERSION).then_some(SEGMENT_COUNT - 1);
        if self.primary_flags.len() != primary_count
            || self.secondary_flags.as_ref().map(Vec::len) != secondary_count
        {
            return Err(invalid(
                0,
                "edited segment-flag counts do not match the version",
            ));
        }
        if self
            .primary_flags
            .iter()
            .zip(&self.original_primary_flags)
            .any(|(edited, original)| (edited == &0) != (original == &0))
        {
            return Err(invalid(
                0,
                "primary segment edits must preserve the active-segment pattern",
            ));
        }
        if self.primary_flags == self.original_primary_flags
            && self.secondary_flags == self.original_secondary_flags
        {
            return Ok(self.original_prefix.clone());
        }

        let mut output = Vec::new();
        write_encoded_uint_array(&mut output, &self.primary_flags);
        if let Some(secondary_flags) = &self.secondary_flags {
            write_encoded_uint_array(&mut output, secondary_flags);
        }
        Ok(output)
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original_prefix
    }
}

impl NetSegmentProblemsPrefix {
    /// Decodes problem bits for segments with nonzero primary flags.
    ///
    /// Before inner version 226 no problem array is stored and all values are
    /// zero. Versions 226–114999 store one ULong per active segment; versions
    /// 115000 and newer store two interleaved ULong values. The returned length
    /// ends before later segment fields.
    pub fn decode_prefix(
        format_version: u32,
        segment_flags: &NetSegmentFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || segment_flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data segment problems".to_owned(),
                offset: 0,
                reason: "problem version does not match segment flags".to_owned(),
            });
        }
        let active_indices = segment_flags
            .segment_flags()
            .iter()
            .filter(|segment| segment.flags != 0)
            .map(|segment| segment.index)
            .collect::<Vec<_>>();
        let active_count = active_indices.len();
        let mut reader = Reader::new(bytes);
        let mut problems = Vec::with_capacity(active_count);
        if format_version >= TWO_PROBLEMS_VERSION {
            let value_count = active_count
                .checked_mul(2)
                .ok_or(CrpError::ArithmeticOverflow(
                    "NetManager segment problem count",
                ))?;
            let values = read_encoded_ulong_array(&mut reader, value_count)?;
            for (index, pair) in active_indices.into_iter().zip(values.chunks_exact(2)) {
                problems.push(NetSegmentProblem {
                    index,
                    problems1: pair[0],
                    problems2: pair[1],
                });
            }
        } else if format_version >= SEGMENT_PROBLEMS_VERSION {
            let values = read_encoded_ulong_array(&mut reader, active_count)?;
            for (index, value) in active_indices.into_iter().zip(values) {
                problems.push(NetSegmentProblem {
                    index,
                    problems1: value,
                    problems2: 0,
                });
            }
        } else {
            for index in active_indices {
                problems.push(NetSegmentProblem {
                    index,
                    problems1: 0,
                    problems2: 0,
                });
            }
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                problems,
                source_segment_flags: segment_flags.encode(),
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns problem bits for segments with nonzero primary flags.
    pub fn problems(&self) -> &[NetSegmentProblem] {
        &self.problems
    }

    /// Writes the original encoded problem-array bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Consumes the prefix to edit its stored problem-bit arrays.
    pub fn into_editor(self) -> NetSegmentProblemsEditor {
        let problems1 = self
            .problems
            .iter()
            .map(|problem| problem.problems1)
            .collect::<Vec<_>>();
        let problems2 = (self.format_version >= TWO_PROBLEMS_VERSION).then(|| {
            self.problems
                .iter()
                .map(|problem| problem.problems2)
                .collect::<Vec<_>>()
        });
        NetSegmentProblemsEditor {
            format_version: self.format_version,
            original_problems1: problems1.clone(),
            original_problems2: problems2.clone(),
            problems1,
            problems2,
            original_segment_flags: self.source_segment_flags,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetSegmentProblemsEditor {
    /// Returns the inner DataSerializer version associated with this prefix.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns first problem-bit values for active segments in buffer order.
    pub fn problems1_mut(&mut self) -> &mut [u64] {
        &mut self.problems1
    }

    /// Returns second problem-bit values from inner version 115000 onward.
    pub fn problems2_mut(&mut self) -> Option<&mut [u64]> {
        self.problems2.as_deref_mut()
    }

    /// Encodes the original segment problem arrays with edited values.
    ///
    /// Unchanged values preserve their original bytes. Versions before 226 do
    /// not store problem arrays, so changing their synthesized zeroes is an
    /// error. Versions 226–114999 store one value per active segment; later
    /// versions store interleaved pairs.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.problems1.len() != self.original_problems1.len()
            || self.problems2.as_ref().map(Vec::len)
                != self.original_problems2.as_ref().map(Vec::len)
        {
            return Err(invalid(
                0,
                "edited segment problem counts do not match the source",
            ));
        }
        if self.problems1 == self.original_problems1 && self.problems2 == self.original_problems2 {
            return Ok(self.original_prefix.clone());
        }
        if self.format_version < SEGMENT_PROBLEMS_VERSION {
            return Err(invalid(
                0,
                "segment problem values are synthesized before inner version 226",
            ));
        }
        if (self.format_version >= TWO_PROBLEMS_VERSION) != self.problems2.is_some() {
            return Err(invalid(
                0,
                "second segment problem values do not match this NetManager version",
            ));
        }

        let mut output = Vec::new();
        if let Some(problems2) = &self.problems2 {
            let value_count =
                self.problems1
                    .len()
                    .checked_mul(2)
                    .ok_or(CrpError::ArithmeticOverflow(
                        "NetManager segment problem count",
                    ))?;
            let mut interleaved = Vec::with_capacity(value_count);
            for (&first, &second) in self.problems1.iter().zip(problems2) {
                interleaved.push(first);
                interleaved.push(second);
            }
            write_encoded_ulong_array(&mut output, &interleaved);
        } else {
            write_encoded_ulong_array(&mut output, &self.problems1);
        }
        Ok(output)
    }

    pub(crate) fn original_segment_flags(&self) -> &[u8] {
        &self.original_segment_flags
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original_prefix
    }
}

impl NetSegmentTrafficLightsPrefix {
    /// Decodes two traffic-light values for each active segment.
    ///
    /// Inner versions 164–115016 store byte values. Versions 115017 and newer
    /// store signed 32-bit values through `EncodedArray.Int`. The returned
    /// length ends before the fire-coverage array.
    pub fn decode_prefix(
        format_version: u32,
        segment_flags: &NetSegmentFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || segment_flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data traffic-light state".to_owned(),
                offset: 0,
                reason: "state version does not match segment flags".to_owned(),
            });
        }
        let active_indices = segment_flags
            .segment_flags()
            .iter()
            .filter(|segment| segment.flags != 0)
            .map(|segment| segment.index)
            .collect::<Vec<_>>();
        let value_count =
            active_indices
                .len()
                .checked_mul(2)
                .ok_or(CrpError::ArithmeticOverflow(
                    "NetManager traffic-light state count",
                ))?;
        let mut reader = Reader::new(bytes);
        let values = if format_version >= INT_TRAFFIC_LIGHT_VERSION {
            read_encoded_int_array(&mut reader, value_count)?
        } else {
            read_encoded_byte_array(&mut reader, value_count)?
                .into_iter()
                .map(i32::from)
                .collect()
        };
        let mut states = Vec::with_capacity(active_indices.len());
        for (index, pair) in active_indices.into_iter().zip(values.chunks_exact(2)) {
            states.push(NetSegmentTrafficLight {
                index,
                state0: pair[0],
                state1: pair[1],
            });
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                states,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns traffic-light states for segments with nonzero primary flags.
    pub fn states(&self) -> &[NetSegmentTrafficLight] {
        &self.states
    }

    /// Writes the original encoded state array exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetSegmentCoveragePrefix {
    /// Decodes sparse fire-coverage and wetness byte arrays.
    ///
    /// Fire coverage begins at inner version 185 and wetness at 224. Earlier
    /// versions supply zeroes without consuming bytes. The returned length
    /// ends before the condition array.
    pub fn decode_prefix(
        format_version: u32,
        segment_flags: &NetSegmentFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || segment_flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data segment coverage".to_owned(),
                offset: 0,
                reason: "coverage version does not match segment flags".to_owned(),
            });
        }
        let active_indices = segment_flags
            .segment_flags()
            .iter()
            .filter(|segment| segment.flags != 0)
            .map(|segment| segment.index)
            .collect::<Vec<_>>();
        let count = active_indices.len();
        let mut reader = Reader::new(bytes);
        let fire_coverage = if format_version >= FIRE_COVERAGE_VERSION {
            read_encoded_byte_array(&mut reader, count)?
        } else {
            vec![0; count]
        };
        let wetness = if format_version >= WETNESS_VERSION {
            read_encoded_byte_array(&mut reader, count)?
        } else {
            vec![0; count]
        };
        let mut coverage = Vec::with_capacity(count);
        for ((index, fire_coverage), wetness) in
            active_indices.into_iter().zip(fire_coverage).zip(wetness)
        {
            coverage.push(NetSegmentCoverage {
                index,
                fire_coverage,
                wetness,
            });
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                coverage,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns coverage fields for segments with nonzero primary flags.
    pub fn coverage(&self) -> &[NetSegmentCoverage] {
        &self.coverage
    }

    /// Writes the original encoded arrays exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetSegmentEnvironmentPrefix {
    /// Decodes the four sparse arrays in serialized field order.
    ///
    /// Omitted fields are zero-filled without consuming bytes. Returns an
    /// error on version mismatch or malformed encoded arrays. The consumed
    /// length ends before the individual segment records.
    pub fn decode_prefix(
        format_version: u32,
        segment_flags: &NetSegmentFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || segment_flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data segment environment".to_owned(),
                offset: 0,
                reason: "environment version does not match segment flags".to_owned(),
            });
        }
        let active_indices = segment_flags
            .segment_flags()
            .iter()
            .filter(|segment| segment.flags != 0)
            .map(|segment| segment.index)
            .collect::<Vec<_>>();
        let count = active_indices.len();
        let mut reader = Reader::new(bytes);
        let condition = if format_version >= SEGMENT_CONDITION_VERSION {
            read_encoded_byte_array(&mut reader, count)?
        } else {
            vec![0; count]
        };
        let noise_buffer = if format_version >= SEGMENT_NOISE_VERSION {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            vec![0; count]
        };
        let noise_density = if format_version >= SEGMENT_NOISE_VERSION {
            read_encoded_byte_array(&mut reader, count)?
        } else {
            vec![0; count]
        };
        let attractiveness = if format_version >= SEGMENT_ATTRACTIVENESS_VERSION {
            read_encoded_byte_array(&mut reader, count)?
        } else {
            vec![0; count]
        };
        let mut environment = Vec::with_capacity(count);
        for (((index, condition), noise_buffer), (noise_density, attractiveness)) in active_indices
            .into_iter()
            .zip(condition)
            .zip(noise_buffer)
            .zip(noise_density.into_iter().zip(attractiveness))
        {
            environment.push(NetSegmentEnvironment {
                index,
                condition,
                noise_buffer,
                noise_density,
                attractiveness,
            });
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                environment,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns fields for segments with nonzero primary flags.
    pub fn environment(&self) -> &[NetSegmentEnvironment] {
        &self.environment
    }

    /// Writes the original encoded arrays exactly, preserving unknown run grouping.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetSegmentRecordsPrefix {
    /// Decodes fixed records for active segments in inner versions 164+.
    ///
    /// Rejects a mismatched flag version or truncated record. The returned
    /// length ends before the following guide-count tables.
    pub fn decode_prefix(
        format_version: u32,
        segment_flags: &NetSegmentFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || segment_flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data segment records".to_owned(),
                offset: 0,
                reason: "record version does not match segment flags".to_owned(),
            });
        }
        let mut reader = Reader::new(bytes);
        let mut records = Vec::new();
        for segment in segment_flags
            .segment_flags()
            .iter()
            .filter(|segment| segment.flags != 0)
        {
            let build_index = reader.read_u32_be()?;
            let modified_index = reader.read_u32_be()?;
            let lanes = read_segment_u24(&mut reader)?;
            let path = read_segment_u24(&mut reader)?;
            let start_direction_bits = [
                reader.read_u32_be()?,
                reader.read_u32_be()?,
                reader.read_u32_be()?,
            ];
            let end_direction_bits = [
                reader.read_u32_be()?,
                reader.read_u32_be()?,
                reader.read_u32_be()?,
            ];
            let start_node = reader.read_u16_be()?;
            let end_node = reader.read_u16_be()?;
            let blocks = [
                reader.read_u16_be()?,
                reader.read_u16_be()?,
                reader.read_u16_be()?,
                reader.read_u16_be()?,
            ];
            let traffic_buffer = reader.read_u16_be()?;
            let traffic_density = reader.read_u8()?;
            let name_seed = if format_version >= SEGMENT_NAME_SEED_VERSION {
                reader.read_u16_be()?
            } else {
                0
            };
            let override_path_find_direction_limit =
                if format_version >= SEGMENT_PATH_OVERRIDE_VERSION {
                    reader.read_u8()?
                } else {
                    0
                };
            let tree_present = if format_version >= SEGMENT_TREE_VERSION {
                reader.read_u8()?
            } else {
                0
            };
            let fence_present = if format_version >= SEGMENT_FENCE_VERSION {
                [reader.read_u8()?, reader.read_u8()?]
            } else {
                [0; 2]
            };
            let (event_route, next_event_route_segment) =
                if format_version >= SEGMENT_EVENT_ROUTE_VERSION {
                    (reader.read_u16_be()?, reader.read_u16_be()?)
                } else {
                    (0, 0)
                };
            records.push(NetSegmentRecord {
                index: segment.index,
                build_index,
                modified_index,
                lanes,
                path,
                start_direction_bits,
                end_direction_bits,
                start_node,
                end_node,
                blocks,
                traffic_buffer,
                traffic_density,
                name_seed,
                override_path_find_direction_limit,
                tree_present,
                fence_present,
                event_route,
                next_event_route_segment,
            });
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                records,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns one record per segment with nonzero primary flags.
    pub fn records(&self) -> &[NetSegmentRecord] {
        &self.records
    }

    /// Writes the original record bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetGuideCountsPrefix {
    /// Decodes the two prefab-reference counts in inner versions 164+.
    /// Returns an error on unsupported versions or truncation and stops before
    /// the prefab-reference tables.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data guide counts".to_owned(),
                offset: 0,
                reason: "unsupported inner version before 164".to_owned(),
            });
        }
        let mut reader = Reader::new(bytes);
        let guide_count = reader.read_u16_be()?;
        let milestone_count = reader.read_u16_be()?;
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                guide_count,
                milestone_count,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the count of `m_netNotUsedGuides` references.
    pub fn guide_count(&self) -> u16 {
        self.guide_count
    }

    /// Returns the count of `m_UnlockMilestones` references.
    pub fn milestone_count(&self) -> u16 {
        self.milestone_count
    }

    /// Writes the two original count values exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetInfoPrefabPrefix {
    /// Decodes the network-info UShort stream and its prefab-name table.
    ///
    /// Supports inner versions 164+. Rejects mismatched versions, count
    /// overflow, malformed encoded arrays, and invalid names. The returned
    /// length ends before the tree-prefab table in version 114002+.
    pub fn decode_prefix(
        format_version: u32,
        node_flags: &NetNodeFlagsPrefix,
        segment_flags: &NetSegmentFlagsPrefix,
        guide_counts: &NetGuideCountsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164
            || node_flags.format_version() != format_version
            || segment_flags.format_version() != format_version
            || guide_counts.format_version() != format_version
        {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data info prefab table".to_owned(),
                offset: 0,
                reason: "prefab-table version does not match flags and counts".to_owned(),
            });
        }
        let node_count = node_flags
            .node_flags()
            .iter()
            .filter(|node| node.flags != 0)
            .count();
        let segment_count = segment_flags
            .segment_flags()
            .iter()
            .filter(|segment| segment.flags != 0)
            .count();
        let guide_count = usize::from(guide_counts.guide_count());
        let milestone_count = usize::from(guide_counts.milestone_count());
        let total_count = node_count
            .checked_add(segment_count)
            .and_then(|count| count.checked_add(guide_count))
            .and_then(|count| count.checked_add(milestone_count))
            .ok_or(CrpError::ArithmeticOverflow(
                "NetManager info prefab index count",
            ))?;
        let mut reader = Reader::new(bytes);
        let indices = read_encoded_ushort_array(&mut reader, total_count)?;
        let mut index_iter = indices.into_iter();
        let node_info_indices = index_iter.by_ref().take(node_count).collect();
        let segment_info_indices = index_iter.by_ref().take(segment_count).collect();
        let guide_indices = index_iter.by_ref().take(guide_count).collect();
        let milestone_indices = index_iter.take(milestone_count).collect();
        let names = read_prefab_names(&mut reader)?;
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                node_info_indices,
                segment_info_indices,
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

    /// Returns prefab indices for active nodes, in buffer order.
    pub fn node_info_indices(&self) -> &[u16] {
        &self.node_info_indices
    }

    /// Returns prefab indices for active segments, in buffer order.
    pub fn segment_info_indices(&self) -> &[u16] {
        &self.segment_info_indices
    }

    /// Returns serialized guide prefab indices.
    pub fn guide_indices(&self) -> &[u16] {
        &self.guide_indices
    }

    /// Returns serialized milestone prefab indices.
    pub fn milestone_indices(&self) -> &[u16] {
        &self.milestone_indices
    }

    /// Returns the prefab names in their original table order.
    pub fn names(&self) -> &[Option<String>] {
        &self.names
    }

    /// Writes the original encoded index and name-table bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetTreePrefabPrefix {
    /// Decodes the tree-prefab UShort stream and name table from version 114002.
    /// Earlier supported versions return empty fields and consume no bytes.
    /// Malformed arrays and strings return an error. The returned length ends
    /// before left-fence prefabs when those are present.
    pub fn decode_prefix(
        format_version: u32,
        segment_records: &NetSegmentRecordsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || segment_records.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data tree prefab table".to_owned(),
                offset: 0,
                reason: "tree-prefab version does not match segment records".to_owned(),
            });
        }
        let mut reader = Reader::new(bytes);
        let (indices, names) = if format_version >= SEGMENT_TREE_VERSION {
            let count = segment_records
                .records()
                .iter()
                .filter(|record| record.tree_present == 1)
                .count();
            (
                read_encoded_ushort_array(&mut reader, count)?,
                read_prefab_names(&mut reader)?,
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                indices,
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

    /// Returns tree prefab indices in segment-buffer order.
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }

    /// Returns the tree-prefab names in original order.
    pub fn names(&self) -> &[Option<String>] {
        &self.names
    }

    /// Writes the original encoded tree-prefab bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetFencePrefabPrefix {
    /// Decodes separate left and right fence tables from inner version 120003.
    /// Earlier supported versions consume no bytes. Returns an error on a
    /// mismatched version, malformed encoded array, or invalid string. The
    /// returned length ends before guide and milestone object references.
    pub fn decode_prefix(
        format_version: u32,
        segment_records: &NetSegmentRecordsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || segment_records.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data fence prefab tables".to_owned(),
                offset: 0,
                reason: "fence-prefab version does not match segment records".to_owned(),
            });
        }
        let mut reader = Reader::new(bytes);
        let (left_indices, left_names, right_indices, right_names) =
            if format_version >= SEGMENT_FENCE_VERSION {
                let left_count = segment_records
                    .records()
                    .iter()
                    .filter(|record| record.fence_present[0] == 1)
                    .count();
                let left_indices = read_encoded_ushort_array(&mut reader, left_count)?;
                let left_names = read_prefab_names(&mut reader)?;
                let right_count = segment_records
                    .records()
                    .iter()
                    .filter(|record| record.fence_present[1] == 1)
                    .count();
                let right_indices = read_encoded_ushort_array(&mut reader, right_count)?;
                let right_names = read_prefab_names(&mut reader)?;
                (left_indices, left_names, right_indices, right_names)
            } else {
                (Vec::new(), Vec::new(), Vec::new(), Vec::new())
            };
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                left_indices,
                left_names,
                right_indices,
                right_names,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns left-fence prefab indices in segment-buffer order.
    pub fn left_indices(&self) -> &[u16] {
        &self.left_indices
    }

    /// Returns left-fence prefab names in original table order.
    pub fn left_names(&self) -> &[Option<String>] {
        &self.left_names
    }

    /// Returns right-fence prefab indices in segment-buffer order.
    pub fn right_indices(&self) -> &[u16] {
        &self.right_indices
    }

    /// Returns right-fence prefab names in original table order.
    pub fn right_names(&self) -> &[Option<String>] {
        &self.right_names
    }

    /// Writes the original left and right table bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetManagerTailPrefix {
    /// Decodes object headers, versioned scalars, and the profiler end tag.
    ///
    /// Supports inner versions 164+. The object type names and original bytes
    /// are preserved; queued object bodies are not decoded. Invalid headers,
    /// truncated fields, and adjusted-segment counts above the game's 576-word
    /// allocation return errors. The returned length ends at the next queued
    /// manager-body boundary.
    pub fn decode_prefix(
        format_version: u32,
        guide_counts: &NetGuideCountsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || guide_counts.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "NetManager+Data tail".to_owned(),
                offset: 0,
                reason: "tail version does not match guide counts".to_owned(),
            });
        }
        let mut reader = Reader::new(bytes);
        let fixed_header_count = 8
            + usize::from(format_version >= 166)
            + 3 * usize::from(format_version >= 236)
            + usize::from(format_version >= 237)
            + 2 * usize::from(format_version >= 295)
            + 3 * usize::from(format_version >= 318);
        let header_count = usize::from(guide_counts.guide_count())
            .checked_add(usize::from(guide_counts.milestone_count()))
            .and_then(|count| count.checked_add(fixed_header_count))
            .ok_or(CrpError::ArithmeticOverflow(
                "NetManager object-header count",
            ))?;
        let mut object_headers = Vec::new();
        for _ in 0..header_count {
            object_headers.push(read_net_object_header(&mut reader)?);
        }
        let (last_max_wetness, current_max_wetness) = if format_version >= 237 {
            (reader.read_u8()?, reader.read_u8()?)
        } else {
            (0, 0)
        };
        let name_modified_count = if format_version >= 319 {
            read_segment_u24(&mut reader)?
        } else {
            0
        };
        let adjusted_segments = if format_version >= 321 {
            let count = usize::try_from(read_segment_u24(&mut reader)?)
                .map_err(|_| reader.invalid("adjusted-segment count does not fit platform"))?;
            if count > ADJUSTED_SEGMENT_WORDS {
                return Err(reader.invalid("adjusted-segment count exceeds 576 words"));
            }
            read_encoded_ulong_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let profile_marker = reader.read_u8()?;
        let profile_end_shared_string_index = match profile_marker {
            0xff => return Err(reader.invalid("NetManager profiler end tag cannot be null")),
            0xfe => {
                let name = reader.read_unique_string()?.ok_or_else(|| {
                    reader.invalid("NetManager profiler end string cannot be null")
                })?;
                if name != "NetManager" {
                    return Err(reader.invalid("wrong NetManager profiler end tag"));
                }
                None
            }
            high => Some(u16::from_be_bytes([high, reader.read_u8()?])),
        };
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                object_headers,
                last_max_wetness,
                current_max_wetness,
                name_modified_count,
                adjusted_segments,
                profile_end_shared_string_index,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns object headers in guide, milestone, and fixed-field order.
    pub fn object_headers(&self) -> &[NetObjectHeader] {
        &self.object_headers
    }

    /// Returns the last and current maximum wetness values, zero before 237.
    pub fn max_wetness(&self) -> (u8, u8) {
        (self.last_max_wetness, self.current_max_wetness)
    }

    /// Returns the 24-bit name-modification count, zero before 319.
    pub fn name_modified_count(&self) -> u32 {
        self.name_modified_count
    }

    /// Returns serialized adjusted-segment words from version 321 onward.
    pub fn adjusted_segments(&self) -> &[u64] {
        &self.adjusted_segments
    }

    /// Returns the profiler end's shared-string index, if it used one.
    pub fn profile_end_shared_string_index(&self) -> Option<u16> {
        self.profile_end_shared_string_index
    }

    /// Writes the original object headers, scalars, and profiler tag exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

pub(crate) fn read_net_object_header(reader: &mut Reader<'_>) -> Result<NetObjectHeader, CrpError> {
    match reader.read_u8()? {
        0xff => Ok(NetObjectHeader::Null),
        0xfe => match reader.read_u8()? {
            0xff => Err(reader.invalid("new object has a null shared type")),
            0xfe => {
                let name = reader
                    .read_unique_string()?
                    .ok_or_else(|| reader.invalid("new object type name cannot be null"))?;
                Ok(NetObjectHeader::NewType(name))
            }
            high => Ok(NetObjectHeader::NewSharedType(u16::from_be_bytes([
                high,
                reader.read_u8()?,
            ]))),
        },
        high => {
            let suffix = reader.read_bytes(2)?;
            Ok(NetObjectHeader::Existing(
                (u32::from(high) << 16) | (u32::from(suffix[0]) << 8) | u32::from(suffix[1]),
            ))
        }
    }
}

impl NetNodeInstanceGuideBody {
    /// Decodes one guide body for inner versions 164 and newer.
    /// Versions 221+ require the save application version because it selects
    /// the 32-bit or 64-bit problem-field representation.
    pub fn decode(
        format_version: u32,
        save_app_version: Option<u32>,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(invalid_guide(
                0,
                "NetNodeInstanceGuide requires inner version 164 or newer",
            ));
        }
        if format_version >= 221 && save_app_version.is_none() {
            return Err(invalid_guide(
                0,
                "save application version is required for the guide problem-field gate",
            ));
        }
        let mut reader = Reader::new(bytes);
        let next_repeat = reader.read_u32_be()?;
        let activation_frame = reader.read_u32_be()?;
        let activation_count = reader.read_u8()?;
        let display_count = reader.read_u8()?;
        let disabled = reader.read_u8()? == 1;
        let wide_problems = format_version >= 221
            && save_app_version.is_some_and(|version| version >= SAVE_APP_PROBLEMS_VERSION);
        let problems = if wide_problems {
            reader.read_u64_be()?
        } else {
            legacy_guide_problems_to_u64(reader.read_u32_be()?)
        };
        let build_index = reader.read_u32_be()?;
        let node_id = reader.read_u16_be()?;
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                save_app_version,
                next_repeat,
                activation_frame,
                activation_count,
                display_count,
                disabled,
                problems,
                build_index,
                node_id,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the next-repeat frame.
    pub fn next_repeat(&self) -> u32 {
        self.next_repeat
    }

    /// Returns the activation frame.
    pub fn activation_frame(&self) -> u32 {
        self.activation_frame
    }

    /// Returns the number of activations.
    pub fn activation_count(&self) -> u8 {
        self.activation_count
    }

    /// Returns the display count.
    pub fn display_count(&self) -> u8 {
        self.display_count
    }

    /// Returns whether the guide is disabled; only raw byte `01` decodes true.
    pub fn disabled(&self) -> bool {
        self.disabled
    }

    /// Returns the normalized 64-bit problem value.
    pub fn problems(&self) -> u64 {
        self.problems
    }

    /// Returns the guide's build index.
    pub fn build_index(&self) -> u32 {
        self.build_index
    }

    /// Returns the guide's node ID.
    pub fn node_id(&self) -> u16 {
        self.node_id
    }

    /// Returns the original serialized body byte for byte.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }

    /// Creates an editor that preserves original bytes when no field changes.
    pub fn into_editor(self) -> NetNodeInstanceGuideEditor {
        NetNodeInstanceGuideEditor {
            next_repeat: self.next_repeat,
            activation_frame: self.activation_frame,
            activation_count: self.activation_count,
            display_count: self.display_count,
            disabled: self.disabled,
            problems: self.problems,
            build_index: self.build_index,
            node_id: self.node_id,
            save_app_version: self.save_app_version,
            original: self,
        }
    }
}

impl NetNodeInstanceGuideEditor {
    /// Returns mutable access to the next-repeat frame.
    pub fn next_repeat_mut(&mut self) -> &mut u32 {
        &mut self.next_repeat
    }

    /// Returns mutable access to the activation frame.
    pub fn activation_frame_mut(&mut self) -> &mut u32 {
        &mut self.activation_frame
    }

    /// Returns mutable access to the activation count.
    pub fn activation_count_mut(&mut self) -> &mut u8 {
        &mut self.activation_count
    }

    /// Returns mutable access to the display count.
    pub fn display_count_mut(&mut self) -> &mut u8 {
        &mut self.display_count
    }

    /// Returns mutable access to the disabled flag.
    pub fn disabled_mut(&mut self) -> &mut bool {
        &mut self.disabled
    }

    /// Returns mutable access to the normalized problem value.
    pub fn problems_mut(&mut self) -> &mut u64 {
        &mut self.problems
    }

    /// Returns mutable access to the build index.
    pub fn build_index_mut(&mut self) -> &mut u32 {
        &mut self.build_index
    }

    /// Returns mutable access to the node ID.
    pub fn node_id_mut(&mut self) -> &mut u16 {
        &mut self.node_id
    }

    /// Encodes using the compatibility layout selected by the reader gates.
    /// Unchanged fields retain their original bytes, including unusual bool bytes.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.next_repeat == self.original.next_repeat
            && self.activation_frame == self.original.activation_frame
            && self.activation_count == self.original.activation_count
            && self.display_count == self.original.display_count
            && self.disabled == self.original.disabled
            && self.problems == self.original.problems
            && self.build_index == self.original.build_index
            && self.node_id == self.original.node_id
        {
            return Ok(self.original.encode());
        }
        let version = self.original.format_version;
        if version >= 221 && self.save_app_version.is_none() {
            return Err(invalid_guide(
                0,
                "save application version is required for the guide problem-field gate",
            ));
        }
        let wide_problems = version >= 221
            && self
                .save_app_version
                .is_some_and(|value| value >= SAVE_APP_PROBLEMS_VERSION);
        let mut output = Vec::with_capacity(if wide_problems { 25 } else { 21 });
        output.extend_from_slice(&self.next_repeat.to_be_bytes());
        output.extend_from_slice(&self.activation_frame.to_be_bytes());
        output.push(self.activation_count);
        output.push(self.display_count);
        output.push(u8::from(self.disabled));
        if wide_problems {
            output.extend_from_slice(&self.problems.to_be_bytes());
        } else {
            output.extend_from_slice(&legacy_guide_problems_from_u64(self.problems)?.to_be_bytes());
        }
        output.extend_from_slice(&self.build_index.to_be_bytes());
        output.extend_from_slice(&self.node_id.to_be_bytes());
        Ok(output)
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original.serialized_body
    }

    pub(crate) fn matches_version_gates(
        &self,
        format_version: u32,
        save_app_version: Option<u32>,
    ) -> bool {
        self.original.format_version == format_version && self.save_app_version == save_app_version
    }
}

fn legacy_guide_problems_to_u64(value: u32) -> u64 {
    (u64::from(value & 0xc000_0000) << 32) | u64::from(value & 0x3fff_ffff)
}

fn legacy_guide_problems_from_u64(value: u64) -> Result<u32, CrpError> {
    const LEGACY_MASK: u64 = 0xc000_0000_3fff_ffff;
    if value & !LEGACY_MASK != 0 {
        return Err(invalid_guide(
            11,
            "problem value does not fit the legacy 32-bit representation",
        ));
    }
    Ok((value as u32 & 0x3fff_ffff) | ((value >> 32) as u32 & 0xc000_0000))
}

fn invalid_guide(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "NetNodeInstanceGuide".to_owned(),
        offset,
        reason: reason.into(),
    }
}

fn read_prefab_names(reader: &mut Reader<'_>) -> Result<Vec<Option<String>>, CrpError> {
    let name_count = usize::from(reader.read_u16_be()?);
    let mut names = Vec::with_capacity(name_count);
    for _ in 0..name_count {
        names.push(reader.read_unique_string()?);
    }
    Ok(names)
}

fn read_segment_u24(reader: &mut Reader<'_>) -> Result<u32, CrpError> {
    let bytes = reader.read_bytes(3)?;
    Ok((u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]))
}

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "NetManager+Data lane prefix".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod guide_tests {
    use super::NetNodeInstanceGuideBody;

    #[test]
    fn net_node_instance_guide_decodes_legacy_problem_bits_and_preserves_raw_bool() {
        let bytes = [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 2, // base trigger fields; 02 is false
            0xc0, 0, 0, 5, // legacy problem word
            0x10, 0x20, 0x30, 0x40, 0x50, 0x60, // build index and node ID
        ];
        let (guide, used) =
            NetNodeInstanceGuideBody::decode(220, None, &bytes).expect("legacy guide body decodes");
        assert_eq!(used, bytes.len());
        assert_eq!(guide.next_repeat(), 0x0102_0304);
        assert_eq!(guide.activation_frame(), 0x0506_0708);
        assert_eq!(guide.activation_count(), 9);
        assert_eq!(guide.display_count(), 10);
        assert!(!guide.disabled());
        assert_eq!(guide.problems(), 0xc000_0000_0000_0005);
        assert_eq!(guide.build_index(), 0x1020_3040);
        assert_eq!(guide.node_id(), 0x5060);
        assert_eq!(guide.encode(), bytes);
        assert!(NetNodeInstanceGuideBody::decode(220, None, &bytes[..bytes.len() - 1]).is_err());

        let mut unchanged = guide.clone().into_editor();
        assert!(unchanged.matches_version_gates(220, None));
        assert!(!unchanged.matches_version_gates(221, None));
        assert_eq!(unchanged.encode().expect("no-op guide write"), bytes);
        *unchanged.disabled_mut() = true;
        let mut edited_bytes = bytes;
        edited_bytes[10] = 1;
        assert_eq!(
            unchanged.encode().expect("edited guide write"),
            edited_bytes
        );

        let mut invalid = guide.into_editor();
        *invalid.problems_mut() = 1 << 31;
        assert!(invalid.encode().is_err());
    }

    #[test]
    fn net_node_instance_guide_problem_field_uses_app_version_gate() {
        let mut legacy = vec![0; 11];
        legacy.extend_from_slice(&0x8000_0006_u32.to_be_bytes());
        legacy.extend_from_slice(&[0, 0, 0, 7, 0, 8]);
        let (old, used) = NetNodeInstanceGuideBody::decode(
            221,
            Some(super::SAVE_APP_PROBLEMS_VERSION - 1),
            &legacy,
        )
        .expect("old application keeps the u32 problem field");
        assert_eq!(used, 21);
        assert_eq!(old.problems(), 0x8000_0000_0000_0006);
        assert!(NetNodeInstanceGuideBody::decode(221, None, &legacy).is_err());

        let mut wide = vec![0; 11];
        wide.extend_from_slice(&0xfedc_ba98_7654_3210_u64.to_be_bytes());
        wide.extend_from_slice(&[0, 0, 0, 7, 0, 8]);
        let (current, used) =
            NetNodeInstanceGuideBody::decode(221, Some(super::SAVE_APP_PROBLEMS_VERSION), &wide)
                .expect("new application uses the u64 problem field");
        assert_eq!(used, 25);
        assert_eq!(current.problems(), 0xfedc_ba98_7654_3210);
        assert_eq!(current.encode(), wide);

        let mut editor = current.into_editor();
        assert!(editor.matches_version_gates(221, Some(super::SAVE_APP_PROBLEMS_VERSION)));
        assert!(!editor.matches_version_gates(221, Some(super::SAVE_APP_PROBLEMS_VERSION - 1)));
        *editor.problems_mut() = 0x0123_4567_89ab_cdef;
        let encoded = editor.encode().expect("wide problem edits encode");
        assert_eq!(&encoded[11..19], &0x0123_4567_89ab_cdef_u64.to_be_bytes());
    }
}
