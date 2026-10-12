use crate::CrpError;
use crate::data_serializer::{
    Reader, write_nullable_unique_string, write_u24_be, write_unique_string,
};
use crate::encoded_array::{
    read_encoded_byte_array, read_encoded_int_array, read_encoded_uint_array,
    read_encoded_ulong_array, read_encoded_ushort_array, write_encoded_byte_array,
    write_encoded_int_array, write_encoded_uint_array, write_encoded_ulong_array,
    write_encoded_ushort_array,
};

const LANE_COUNT: usize = 262_144;
const NODE_COUNT: usize = 32_768;
const SEGMENT_COUNT: usize = 36_864;
const LEGACY_SEGMENT_COUNT: usize = 32_768;
const LEGACY_NODE_FLAG_COUNT: usize = 16_383;
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

/// Editor for primary and version-gated secondary NetManager node flags.
///
/// The primary zero/nonzero pattern stays fixed because later node data is
/// stored only for active nodes. Secondary flags do not select node records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeFlagsEditor {
    format_version: u32,
    primary_flags: Vec<u32>,
    secondary_flags: Option<Vec<u32>>,
    original_primary_flags: Vec<u32>,
    original_secondary_flags: Option<Vec<u32>>,
    original_prefix: Vec<u8>,
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
    source_node_flags: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Editor for problem-bit arrays belonging to active NetManager nodes.
///
/// The version and save-app gates select whether edited values use a legacy
/// UInt, one ULong, or interleaved ULong pairs. The source node-flag bytes and
/// active-node order remain fixed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeProblemsEditor {
    format_version: u32,
    save_app_version: Option<u32>,
    problems1: Vec<u64>,
    problems2: Option<Vec<u64>>,
    original_problems1: Vec<u64>,
    original_problems2: Option<Vec<u64>>,
    original_node_flags: Vec<u8>,
    original_prefix: Vec<u8>,
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
    source_node_flags: Vec<u8>,
    serialized_wait_times: Vec<u8>,
    serialized_counter_values: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Editor for wait times and temporary/final counters on active nodes.
///
/// The active-node order and count are fixed by the source node flags. Each
/// changed encoded array is rewritten independently; unchanged arrays retain
/// their original bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeCountersEditor {
    format_version: u32,
    counters: Vec<NetNodeCounter>,
    original_counters: Vec<NetNodeCounter>,
    source_node_flags: Vec<u8>,
    original_wait_times: Vec<u8>,
    original_counter_values: Vec<u8>,
    original_prefix: Vec<u8>,
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
    source_node_flags: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Editor for variable-length records belonging to active nodes.
///
/// The active-node order is fixed by the source node flags. A changed segment
/// list can change the encoded prefix length; following NetManager fields are
/// preserved by the manager-level replacement method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeRecordsEditor {
    format_version: u32,
    records: Vec<NetNodeRecord>,
    original_records: Vec<NetNodeRecord>,
    source_node_flags: Vec<u8>,
    original_prefix: Vec<u8>,
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
    source_segment_flags: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Editor for paired traffic-light values belonging to active segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentTrafficLightsEditor {
    format_version: u32,
    state0: Vec<i32>,
    state1: Vec<i32>,
    original_state0: Vec<i32>,
    original_state1: Vec<i32>,
    original_segment_flags: Vec<u8>,
    original_prefix: Vec<u8>,
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
    source_segment_flags: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Editor for fire coverage and wetness belonging to active segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentCoverageEditor {
    format_version: u32,
    fire_coverage: Vec<u8>,
    wetness: Vec<u8>,
    original_fire_coverage: Vec<u8>,
    original_wetness: Vec<u8>,
    original_segment_flags: Vec<u8>,
    original_prefix: Vec<u8>,
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
    source_segment_flags: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Editor for condition, noise, and attractiveness values on active segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentEnvironmentEditor {
    format_version: u32,
    condition: Vec<u8>,
    noise_buffer: Vec<u16>,
    noise_density: Vec<u8>,
    attractiveness: Vec<u8>,
    original_condition: Vec<u8>,
    original_noise_buffer: Vec<u16>,
    original_noise_density: Vec<u8>,
    original_attractiveness: Vec<u8>,
    original_segment_flags: Vec<u8>,
    original_prefix: Vec<u8>,
}

/// Fixed fields stored for one segment with nonzero primary flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentRecord {
    /// Derived one-based segment buffer index; it is not serialized.
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
    source_segment_flags: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Editor for fixed records belonging to active segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSegmentRecordsEditor {
    format_version: u32,
    records: Vec<NetSegmentRecord>,
    original_records: Vec<NetSegmentRecord>,
    original_segment_flags: Vec<u8>,
    original_prefix: Vec<u8>,
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
    source_node_flags: Vec<u8>,
    source_segment_flags: Vec<u8>,
    source_guide_counts: Vec<u8>,
    serialized_indices: Vec<u8>,
    serialized_name_count: Vec<u8>,
    serialized_name_entries: Vec<Vec<u8>>,
    serialized_prefix: Vec<u8>,
}

/// Editor for NetManager's primary network-info prefab index and name table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetInfoPrefabEditor {
    format_version: u32,
    node_info_indices: Vec<u16>,
    segment_info_indices: Vec<u16>,
    guide_indices: Vec<u16>,
    milestone_indices: Vec<u16>,
    names: Vec<Option<String>>,
    original_node_info_indices: Vec<u16>,
    original_segment_info_indices: Vec<u16>,
    original_guide_indices: Vec<u16>,
    original_milestone_indices: Vec<u16>,
    original_names: Vec<Option<String>>,
    source_node_flags: Vec<u8>,
    source_segment_flags: Vec<u8>,
    source_guide_counts: Vec<u8>,
    serialized_indices: Vec<u8>,
    serialized_name_count: Vec<u8>,
    serialized_name_entries: Vec<Vec<u8>>,
    original_prefix: Vec<u8>,
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
    source_segment_flags: Vec<u8>,
    source_tree_presence: Vec<u8>,
    serialized_indices: Vec<u8>,
    serialized_name_count: Vec<u8>,
    serialized_name_entries: Vec<Vec<u8>>,
    serialized_prefix: Vec<u8>,
}

/// Editor for NetManager's optional tree-prefab index and name table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetTreePrefabEditor {
    format_version: u32,
    indices: Vec<u16>,
    names: Vec<Option<String>>,
    original_indices: Vec<u16>,
    original_names: Vec<Option<String>>,
    source_segment_flags: Vec<u8>,
    source_tree_presence: Vec<u8>,
    serialized_indices: Vec<u8>,
    serialized_name_count: Vec<u8>,
    serialized_name_entries: Vec<Vec<u8>>,
    original_prefix: Vec<u8>,
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
    source_segment_flags: Vec<u8>,
    source_fence_presence: Vec<[u8; 2]>,
    serialized_left_indices: Vec<u8>,
    serialized_left_name_count: Vec<u8>,
    serialized_left_name_entries: Vec<Vec<u8>>,
    serialized_right_indices: Vec<u8>,
    serialized_right_name_count: Vec<u8>,
    serialized_right_name_entries: Vec<Vec<u8>>,
    serialized_prefix: Vec<u8>,
}

/// Editor for NetManager's optional left and right fence-prefab tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetFencePrefabEditor {
    format_version: u32,
    left_indices: Vec<u16>,
    left_names: Vec<Option<String>>,
    right_indices: Vec<u16>,
    right_names: Vec<Option<String>>,
    original_left_indices: Vec<u16>,
    original_left_names: Vec<Option<String>>,
    original_right_indices: Vec<u16>,
    original_right_names: Vec<Option<String>>,
    source_segment_flags: Vec<u8>,
    source_fence_presence: Vec<[u8; 2]>,
    serialized_left_indices: Vec<u8>,
    serialized_left_name_count: Vec<u8>,
    serialized_left_name_entries: Vec<Vec<u8>>,
    serialized_right_indices: Vec<u8>,
    serialized_right_name_count: Vec<u8>,
    serialized_right_name_entries: Vec<Vec<u8>>,
    original_prefix: Vec<u8>,
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

impl NetObjectHeader {
    /// Decodes one object header and returns the number of bytes consumed.
    ///
    /// Any following bytes are left for the caller. This resolves only the
    /// header form; object IDs and shared-type indexes are not checked against
    /// a surrounding DataSerializer table.
    pub fn decode_from(bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let header = read_net_object_header(&mut reader)?;
        Ok((header, reader.offset()))
    }

    /// Encodes this header using the canonical DataSerializer string encoding.
    ///
    /// A decoded `NewType` does not retain whether its type name used compact
    /// or wide string encoding, so this method deliberately canonicalizes that
    /// string. Existing-body writers should preserve their original bytes when
    /// no semantic edit is intended. Reserved marker prefixes and IDs are
    /// rejected because they cannot be represented as this header variant.
    pub fn encode_canonical(&self) -> Result<Vec<u8>, CrpError> {
        let mut bytes = Vec::new();
        match self {
            Self::Null => bytes.push(0xff),
            Self::NewType(type_name) => {
                bytes.extend_from_slice(&[0xfe, 0xfe]);
                write_unique_string(&mut bytes, type_name)?;
            }
            Self::NewSharedType(index) => {
                if index.to_be_bytes()[0] >= 0xfe {
                    return Err(invalid_object_header(
                        "shared-type index uses a reserved marker prefix",
                    ));
                }
                bytes.push(0xfe);
                bytes.extend_from_slice(&index.to_be_bytes());
            }
            Self::Existing(object_id) => {
                if *object_id >= 0xfe_0000 {
                    return Err(invalid_object_header(
                        "existing-object ID uses a reserved marker prefix",
                    ));
                }
                write_u24_be(&mut bytes, *object_id)?;
            }
        }
        Ok(bytes)
    }
}

fn invalid_object_header(reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "DataSerializer object header".to_owned(),
        offset: 0,
        reason: reason.into(),
    }
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
    max_wetness_offset: Option<usize>,
    name_modified_count: u32,
    name_modified_count_offset: Option<usize>,
    adjusted_segments: Vec<u64>,
    adjusted_segment_count_offset: Option<usize>,
    adjusted_segment_array_offset: Option<usize>,
    adjusted_segment_array_end: Option<usize>,
    profile_end_shared_string_index: Option<u16>,
    serialized_prefix: Vec<u8>,
}

/// Editor for final NetManager counters and adjusted-segment words.
///
/// It leaves object headers and the profiler end tag byte-exact. Edited
/// adjusted-segment data is rewritten with literal `EncodedArray.ULong`
/// blocks, so its serialized length can change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetManagerTailEditor {
    format_version: u32,
    last_max_wetness: u8,
    current_max_wetness: u8,
    max_wetness_offset: Option<usize>,
    name_modified_count: u32,
    name_modified_count_offset: Option<usize>,
    adjusted_segments: Vec<u64>,
    original_adjusted_segments: Vec<u64>,
    adjusted_segment_count_offset: Option<usize>,
    adjusted_segment_array_offset: Option<usize>,
    adjusted_segment_array_end: Option<usize>,
    original_prefix: Vec<u8>,
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
    disabled_raw: u8,
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
            LEGACY_NODE_FLAG_COUNT
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

    /// Consumes the prefix to edit its serialized node-flag arrays.
    pub fn into_editor(self) -> NetNodeFlagsEditor {
        let primary_count = if self.format_version < FULL_NODE_COUNT_VERSION {
            LEGACY_NODE_FLAG_COUNT
        } else {
            NODE_COUNT - 1
        };
        let primary_flags = self.node_flags[..primary_count]
            .iter()
            .map(|node| node.flags)
            .collect::<Vec<_>>();
        let secondary_flags = (self.format_version >= TWO_NODE_FLAGS_VERSION).then(|| {
            self.node_flags
                .iter()
                .map(|node| node.flags2)
                .collect::<Vec<_>>()
        });
        NetNodeFlagsEditor {
            format_version: self.format_version,
            original_primary_flags: primary_flags.clone(),
            original_secondary_flags: secondary_flags.clone(),
            primary_flags,
            secondary_flags,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetNodeFlagsEditor {
    /// Returns the inner DataSerializer version associated with this prefix.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns primary flags for positions serialized by this version.
    pub fn primary_flags(&self) -> &[u32] {
        &self.primary_flags
    }

    /// Changes raw primary flag bits while preserving the node's active state.
    /// Index zero represents serialized node slot one.
    pub fn set_primary_flag_bits(&mut self, index: usize, value: u32) -> Result<(), CrpError> {
        let current = *self
            .primary_flags
            .get(index)
            .ok_or_else(|| invalid(index, "NetManager primary node-flag index is out of range"))?;
        if (current == 0) != (value == 0) {
            return Err(invalid(
                index,
                "NetManager primary node-flag edits must preserve the active-node pattern",
            ));
        }
        self.primary_flags[index] = value;
        Ok(())
    }

    /// Returns secondary flags from inner version 115018 onward.
    pub fn secondary_flags(&self) -> Option<&[u32]> {
        self.secondary_flags.as_deref()
    }

    /// Changes a secondary flag value from inner version 115018 onward.
    /// These values do not determine the active-node record count.
    pub fn set_secondary_flag_bits(&mut self, index: usize, value: u32) -> Result<(), CrpError> {
        let flags = self.secondary_flags.as_mut().ok_or_else(|| {
            invalid(
                0,
                "NetManager secondary node flags are absent before inner version 115018",
            )
        })?;
        let flag = flags.get_mut(index).ok_or_else(|| {
            invalid(
                index,
                "NetManager secondary node-flag index is out of range",
            )
        })?;
        *flag = value;
        Ok(())
    }

    /// Encodes changed flag arrays while retaining all source bytes when unchanged.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let primary_count = if self.format_version < FULL_NODE_COUNT_VERSION {
            LEGACY_NODE_FLAG_COUNT
        } else {
            NODE_COUNT - 1
        };
        let secondary_count =
            (self.format_version >= TWO_NODE_FLAGS_VERSION).then_some(NODE_COUNT - 1);
        if self.primary_flags.len() != primary_count
            || self.secondary_flags.as_ref().map(Vec::len) != secondary_count
        {
            return Err(invalid(
                0,
                "edited node-flag counts do not match the version",
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
                "primary node-flag edits must preserve the active-node pattern",
            ));
        }
        if self.primary_flags == self.original_primary_flags
            && self.secondary_flags == self.original_secondary_flags
        {
            return Ok(self.original_prefix.clone());
        }

        if let Some(secondary_flags) = &self.secondary_flags {
            let mut interleaved = Vec::with_capacity(
                self.primary_flags
                    .len()
                    .checked_mul(2)
                    .ok_or(CrpError::ArithmeticOverflow("NetManager node flag count"))?,
            );
            for (primary, secondary) in self.primary_flags.iter().zip(secondary_flags) {
                interleaved.push(*primary);
                interleaved.push(*secondary);
            }
            let mut output = Vec::new();
            write_encoded_uint_array(&mut output, &interleaved);
            Ok(output)
        } else {
            let mut output = Vec::new();
            write_encoded_uint_array(&mut output, &self.primary_flags);
            Ok(output)
        }
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original_prefix
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
                source_node_flags: node_flags.encode(),
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

    /// Consumes the prefix to edit the serialized problem-bit arrays.
    pub fn into_editor(self) -> NetNodeProblemsEditor {
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
        NetNodeProblemsEditor {
            format_version: self.format_version,
            save_app_version: self.save_app_version,
            original_problems1: problems1.clone(),
            original_problems2: problems2.clone(),
            problems1,
            problems2,
            original_node_flags: self.source_node_flags,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetNodeProblemsEditor {
    /// Returns the inner DataSerializer version associated with this prefix.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the saved application version used for the legacy problem gate.
    pub fn save_app_version(&self) -> Option<u32> {
        self.save_app_version
    }

    /// Returns mutable first problem-bit values for active nodes in buffer order.
    pub fn problems1_mut(&mut self) -> &mut [u64] {
        &mut self.problems1
    }

    /// Returns mutable second problem-bit values from inner version 115000 onward.
    pub fn problems2_mut(&mut self) -> Option<&mut [u64]> {
        self.problems2.as_deref_mut()
    }

    /// Encodes edited values using the source version's node-problem layout.
    ///
    /// Unchanged values preserve their original bytes. Legacy UInt values can
    /// represent only the relocated low 30 bits and high two bits of the
    /// exposed 64-bit value; values outside that mask are rejected.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.problems1.len() != self.original_problems1.len()
            || self.problems2.as_ref().map(Vec::len)
                != self.original_problems2.as_ref().map(Vec::len)
        {
            return Err(invalid(
                0,
                "edited node problem counts do not match the source",
            ));
        }
        if self.problems1 == self.original_problems1 && self.problems2 == self.original_problems2 {
            return Ok(self.original_prefix.clone());
        }
        if self.format_version < 164 {
            return Err(invalid(
                0,
                "node problem values require inner version 164 or newer",
            ));
        }
        if (self.format_version >= TWO_PROBLEMS_VERSION) != self.problems2.is_some() {
            return Err(invalid(
                0,
                "second node problem values do not match this NetManager version",
            ));
        }

        let mut output = Vec::new();
        if self.format_version >= TWO_PROBLEMS_VERSION {
            let problems2 = self.problems2.as_ref().ok_or_else(|| {
                invalid(0, "second node problem values are absent from this version")
            })?;
            let value_count =
                self.problems1
                    .len()
                    .checked_mul(2)
                    .ok_or(CrpError::ArithmeticOverflow(
                        "NetManager node problem count",
                    ))?;
            let mut interleaved = Vec::with_capacity(value_count);
            for (&first, &second) in self.problems1.iter().zip(problems2) {
                interleaved.push(first);
                interleaved.push(second);
            }
            write_encoded_ulong_array(&mut output, &interleaved);
        } else if self.format_version >= 221
            && self
                .save_app_version
                .is_some_and(|version| version >= SAVE_APP_PROBLEMS_VERSION)
        {
            write_encoded_ulong_array(&mut output, &self.problems1);
        } else {
            const LEGACY_PROBLEM_MASK: u64 = 0xc000_0000_3fff_ffff;
            let mut values = Vec::with_capacity(self.problems1.len());
            for (index, &problem) in self.problems1.iter().enumerate() {
                if problem & !LEGACY_PROBLEM_MASK != 0 {
                    return Err(invalid(
                        index,
                        "edited node problem cannot be represented by the legacy UInt layout",
                    ));
                }
                let value = ((problem & 0xc000_0000_0000_0000) >> 32) | (problem & 0x3fff_ffff);
                values.push(u32::try_from(value).map_err(|_| {
                    CrpError::ArithmeticOverflow("NetManager legacy node problem value")
                })?);
            }
            write_encoded_uint_array(&mut output, &values);
        }
        Ok(output)
    }

    pub(crate) fn original_node_flags(&self) -> &[u8] {
        &self.original_node_flags
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original_prefix
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
        let wait_start = reader.offset();
        let max_wait_times = read_encoded_byte_array(&mut reader, active_count)?;
        let wait_end = reader.offset();
        let counter_start = reader.offset();
        let counter_count = active_count
            .checked_mul(2)
            .ok_or(CrpError::ArithmeticOverflow("NetManager counter count"))?;
        let counter_values = read_encoded_ushort_array(&mut reader, counter_count)?;
        let counter_end = reader.offset();
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
                source_node_flags: node_flags.encode(),
                serialized_wait_times: bytes[wait_start..wait_end].to_vec(),
                serialized_counter_values: bytes[counter_start..counter_end].to_vec(),
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

    /// Consumes the prefix to edit counters on active nodes.
    pub fn into_editor(self) -> NetNodeCountersEditor {
        NetNodeCountersEditor {
            format_version: self.format_version,
            original_counters: self.counters.clone(),
            counters: self.counters,
            source_node_flags: self.source_node_flags,
            original_wait_times: self.serialized_wait_times,
            original_counter_values: self.serialized_counter_values,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetNodeCountersEditor {
    /// Returns the inner DataSerializer version associated with these counters.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns counters in ascending active-node buffer order.
    pub fn counters(&self) -> &[NetNodeCounter] {
        &self.counters
    }

    /// Returns mutable counters in ascending active-node buffer order.
    pub fn counters_mut(&mut self) -> &mut [NetNodeCounter] {
        &mut self.counters
    }

    /// Encodes edited counter arrays while preserving unchanged source arrays.
    ///
    /// The active-node count and indexes are derived from the source flags and
    /// cannot be changed.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.counters.len() != self.original_counters.len() {
            return Err(invalid_node_counter(
                0,
                "edited node-counter count does not match the source",
            ));
        }
        if self.counters == self.original_counters {
            return Ok(self.original_prefix.clone());
        }

        let mut wait_times = Vec::with_capacity(self.counters.len());
        let counter_capacity = self
            .counters
            .len()
            .checked_mul(2)
            .ok_or(CrpError::ArithmeticOverflow("NetManager counter count"))?;
        let mut counter_values = Vec::with_capacity(counter_capacity);
        for (counter, original) in self.counters.iter().zip(&self.original_counters) {
            if counter.index != original.index {
                return Err(invalid_node_counter(
                    wait_times.len(),
                    "node counter index is derived from flags and cannot be changed",
                ));
            }
            wait_times.push(counter.max_wait_time);
            counter_values.push(counter.temp_counter);
            counter_values.push(counter.final_counter);
        }

        let mut output = Vec::with_capacity(self.original_prefix.len());
        if wait_times
            .iter()
            .zip(&self.original_counters)
            .any(|(wait, original)| *wait != original.max_wait_time)
        {
            write_encoded_byte_array(&mut output, &wait_times);
        } else {
            output.extend_from_slice(&self.original_wait_times);
        }
        let counter_values_changed =
            self.counters
                .iter()
                .zip(&self.original_counters)
                .any(|(counter, original)| {
                    counter.temp_counter != original.temp_counter
                        || counter.final_counter != original.final_counter
                });
        if counter_values_changed {
            write_encoded_ushort_array(&mut output, &counter_values);
        } else {
            output.extend_from_slice(&self.original_counter_values);
        }
        Ok(output)
    }

    pub(crate) fn source_node_flags(&self) -> &[u8] {
        &self.source_node_flags
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original_prefix
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
                source_node_flags: node_flags.encode(),
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

    /// Consumes the prefix to edit active node records.
    pub fn into_editor(self) -> NetNodeRecordsEditor {
        NetNodeRecordsEditor {
            format_version: self.format_version,
            original_records: self.records.clone(),
            records: self.records,
            source_node_flags: self.source_node_flags,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetNodeRecordsEditor {
    /// Returns the inner DataSerializer version associated with these records.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns records in ascending active-node buffer order.
    pub fn records(&self) -> &[NetNodeRecord] {
        &self.records
    }

    /// Returns mutable records in ascending active-node buffer order.
    pub fn records_mut(&mut self) -> &mut [NetNodeRecord] {
        &mut self.records
    }

    /// Encodes edited records with their version-specific fields and byte order.
    ///
    /// Unchanged records retain their source bytes. The active-node count and
    /// indexes cannot change, each segment list is limited to eight entries,
    /// and lane IDs must fit their 24-bit field.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.records.len() != self.original_records.len() {
            return Err(invalid_node_record(
                0,
                "edited node-record count does not match the source",
            ));
        }
        if self.records == self.original_records {
            return Ok(self.original_prefix.clone());
        }

        let mut output = Vec::with_capacity(self.original_prefix.len());
        for (record, original) in self.records.iter().zip(&self.original_records) {
            if record.index != original.index {
                return Err(invalid_node_record(
                    output.len(),
                    "node record index is derived from flags and cannot be changed",
                ));
            }
            if record.segments.len() > MAX_NODE_SEGMENTS {
                return Err(invalid_node_record(
                    output.len() + 12,
                    format!(
                        "node {} has {} segments; maximum is {MAX_NODE_SEGMENTS}",
                        record.index,
                        record.segments.len()
                    ),
                ));
            }
            if self.format_version < NODE_EVENT_ROUTE_VERSION
                && record.event_route != original.event_route
            {
                return Err(invalid_node_record(
                    output.len(),
                    "event-route field is not serialized before inner version 121007",
                ));
            }
            if record.lane > 0x00ff_ffff {
                return Err(invalid_node_record(
                    output.len(),
                    "node lane ID exceeds 24-bit storage",
                ));
            }

            for coordinate_bits in record.position_bits {
                output.extend_from_slice(&coordinate_bits.to_be_bytes());
            }
            output.push(u8::try_from(record.segments.len()).map_err(|_| {
                invalid_node_record(
                    output.len(),
                    "node segment count does not fit its byte field",
                )
            })?);
            for segment in &record.segments {
                output.extend_from_slice(&segment.to_be_bytes());
            }
            output.extend_from_slice(&record.build_index.to_be_bytes());
            output.extend_from_slice(&record.building.to_be_bytes());
            output.extend_from_slice(&record.next_building_node.to_be_bytes());
            output.extend_from_slice(&[
                ((record.lane >> 16) & 0xff) as u8,
                ((record.lane >> 8) & 0xff) as u8,
                (record.lane & 0xff) as u8,
                record.lane_offset,
                record.elevation,
                record.height_offset,
                record.connect_count,
            ]);
            if self.format_version >= NODE_EVENT_ROUTE_VERSION {
                output.extend_from_slice(&record.event_route.to_be_bytes());
            }
        }
        Ok(output)
    }

    pub(crate) fn source_node_flags(&self) -> &[u8] {
        &self.source_node_flags
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original_prefix
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

    /// Returns traffic-light states for segments with nonzero primary flags.
    pub fn states(&self) -> &[NetSegmentTrafficLight] {
        &self.states
    }

    /// Writes the original encoded state array exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Consumes the prefix to edit active segments' paired state values.
    pub fn into_editor(self) -> NetSegmentTrafficLightsEditor {
        let state0 = self
            .states
            .iter()
            .map(|state| state.state0)
            .collect::<Vec<_>>();
        let state1 = self
            .states
            .iter()
            .map(|state| state.state1)
            .collect::<Vec<_>>();
        NetSegmentTrafficLightsEditor {
            format_version: self.format_version,
            original_state0: state0.clone(),
            original_state1: state1.clone(),
            state0,
            state1,
            original_segment_flags: self.source_segment_flags,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetSegmentTrafficLightsEditor {
    /// Returns the inner DataSerializer version associated with this prefix.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns first traffic-light values in active-segment order.
    pub fn state0_mut(&mut self) -> &mut [i32] {
        &mut self.state0
    }

    /// Returns second traffic-light values in active-segment order.
    pub fn state1_mut(&mut self) -> &mut [i32] {
        &mut self.state1
    }

    /// Encodes paired state values using the version-specific array type.
    ///
    /// Unchanged values preserve their original bytes. Versions before 115017
    /// store bytes and reject edited values outside 0–255; later versions use
    /// signed 32-bit values.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.state0.len() != self.original_state0.len()
            || self.state1.len() != self.original_state1.len()
            || self.state0.len() != self.state1.len()
        {
            return Err(invalid(
                0,
                "edited traffic-light state counts do not match the source",
            ));
        }
        if self.state0 == self.original_state0 && self.state1 == self.original_state1 {
            return Ok(self.original_prefix.clone());
        }

        if self.format_version >= INT_TRAFFIC_LIGHT_VERSION {
            let value_count =
                self.state0
                    .len()
                    .checked_mul(2)
                    .ok_or(CrpError::ArithmeticOverflow(
                        "NetManager traffic-light state count",
                    ))?;
            let mut interleaved = Vec::with_capacity(value_count);
            for (&state0, &state1) in self.state0.iter().zip(&self.state1) {
                interleaved.push(state0);
                interleaved.push(state1);
            }
            let mut output = Vec::new();
            write_encoded_int_array(&mut output, &interleaved);
            return Ok(output);
        }

        let value_count = self
            .state0
            .len()
            .checked_mul(2)
            .ok_or(CrpError::ArithmeticOverflow(
                "NetManager traffic-light state count",
            ))?;
        let mut interleaved = Vec::with_capacity(value_count);
        for (&state0, &state1) in self.state0.iter().zip(&self.state1) {
            interleaved.push(
                u8::try_from(state0).map_err(|_| {
                    invalid(0, "byte traffic-light state must be in the range 0–255")
                })?,
            );
            interleaved.push(
                u8::try_from(state1).map_err(|_| {
                    invalid(0, "byte traffic-light state must be in the range 0–255")
                })?,
            );
        }
        let mut output = Vec::new();
        write_encoded_byte_array(&mut output, &interleaved);
        Ok(output)
    }

    pub(crate) fn original_segment_flags(&self) -> &[u8] {
        &self.original_segment_flags
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original_prefix
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

    /// Returns coverage fields for segments with nonzero primary flags.
    pub fn coverage(&self) -> &[NetSegmentCoverage] {
        &self.coverage
    }

    /// Writes the original encoded arrays exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Consumes the prefix to edit active segments' coverage values.
    pub fn into_editor(self) -> NetSegmentCoverageEditor {
        let fire_coverage = self
            .coverage
            .iter()
            .map(|item| item.fire_coverage)
            .collect::<Vec<_>>();
        let wetness = self
            .coverage
            .iter()
            .map(|item| item.wetness)
            .collect::<Vec<_>>();
        NetSegmentCoverageEditor {
            format_version: self.format_version,
            original_fire_coverage: fire_coverage.clone(),
            original_wetness: wetness.clone(),
            fire_coverage,
            wetness,
            original_segment_flags: self.source_segment_flags,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetSegmentCoverageEditor {
    /// Returns the inner DataSerializer version associated with this prefix.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns fire-coverage values in active-segment order.
    pub fn fire_coverage_mut(&mut self) -> &mut [u8] {
        &mut self.fire_coverage
    }

    /// Returns wetness values in active-segment order.
    pub fn wetness_mut(&mut self) -> &mut [u8] {
        &mut self.wetness
    }

    /// Encodes the versioned Byte arrays, preserving the source if unchanged.
    ///
    /// Fire coverage is serialized from inner version 185 and wetness from
    /// version 224. Values synthesized before their field's version gate
    /// cannot be changed.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.fire_coverage.len() != self.original_fire_coverage.len()
            || self.wetness.len() != self.original_wetness.len()
            || self.fire_coverage.len() != self.wetness.len()
        {
            return Err(invalid(
                0,
                "edited segment coverage counts do not match the source",
            ));
        }
        if self.fire_coverage == self.original_fire_coverage
            && self.wetness == self.original_wetness
        {
            return Ok(self.original_prefix.clone());
        }
        if self.format_version < FIRE_COVERAGE_VERSION
            && self.fire_coverage != self.original_fire_coverage
        {
            return Err(invalid(
                0,
                "fire coverage is not serialized before inner version 185",
            ));
        }
        if self.format_version < WETNESS_VERSION && self.wetness != self.original_wetness {
            return Err(invalid(
                0,
                "wetness is not serialized before inner version 224",
            ));
        }

        let mut output = Vec::new();
        if self.format_version >= FIRE_COVERAGE_VERSION {
            write_encoded_byte_array(&mut output, &self.fire_coverage);
        }
        if self.format_version >= WETNESS_VERSION {
            write_encoded_byte_array(&mut output, &self.wetness);
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

    /// Returns fields for segments with nonzero primary flags.
    pub fn environment(&self) -> &[NetSegmentEnvironment] {
        &self.environment
    }

    /// Writes the original encoded arrays exactly, preserving unknown run grouping.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Consumes the prefix to edit active segments' environmental values.
    pub fn into_editor(self) -> NetSegmentEnvironmentEditor {
        let condition = self
            .environment
            .iter()
            .map(|item| item.condition)
            .collect::<Vec<_>>();
        let noise_buffer = self
            .environment
            .iter()
            .map(|item| item.noise_buffer)
            .collect::<Vec<_>>();
        let noise_density = self
            .environment
            .iter()
            .map(|item| item.noise_density)
            .collect::<Vec<_>>();
        let attractiveness = self
            .environment
            .iter()
            .map(|item| item.attractiveness)
            .collect::<Vec<_>>();
        NetSegmentEnvironmentEditor {
            format_version: self.format_version,
            original_condition: condition.clone(),
            original_noise_buffer: noise_buffer.clone(),
            original_noise_density: noise_density.clone(),
            original_attractiveness: attractiveness.clone(),
            condition,
            noise_buffer,
            noise_density,
            attractiveness,
            original_segment_flags: self.source_segment_flags,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetSegmentEnvironmentEditor {
    /// Returns the inner DataSerializer version associated with this prefix.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns condition values in active-segment order.
    pub fn condition_mut(&mut self) -> &mut [u8] {
        &mut self.condition
    }

    /// Returns noise-buffer values in active-segment order.
    pub fn noise_buffer_mut(&mut self) -> &mut [u16] {
        &mut self.noise_buffer
    }

    /// Returns noise-density values in active-segment order.
    pub fn noise_density_mut(&mut self) -> &mut [u8] {
        &mut self.noise_density
    }

    /// Returns attractiveness values in active-segment order.
    pub fn attractiveness_mut(&mut self) -> &mut [u8] {
        &mut self.attractiveness
    }

    /// Encodes arrays in their version-gated serialized order.
    ///
    /// Condition starts at inner version 229, the noise arrays at 109007,
    /// and attractiveness at 121030. Fields synthesized before their gates
    /// cannot be changed.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let count = self.original_condition.len();
        if self.condition.len() != count
            || self.noise_buffer.len() != count
            || self.noise_density.len() != count
            || self.attractiveness.len() != count
        {
            return Err(invalid(
                0,
                "edited segment environment counts do not match the source",
            ));
        }
        if self.condition == self.original_condition
            && self.noise_buffer == self.original_noise_buffer
            && self.noise_density == self.original_noise_density
            && self.attractiveness == self.original_attractiveness
        {
            return Ok(self.original_prefix.clone());
        }
        if self.format_version < SEGMENT_CONDITION_VERSION
            && self.condition != self.original_condition
        {
            return Err(invalid(
                0,
                "segment condition is not serialized before inner version 229",
            ));
        }
        if self.format_version < SEGMENT_NOISE_VERSION
            && (self.noise_buffer != self.original_noise_buffer
                || self.noise_density != self.original_noise_density)
        {
            return Err(invalid(
                0,
                "segment noise is not serialized before inner version 109007",
            ));
        }
        if self.format_version < SEGMENT_ATTRACTIVENESS_VERSION
            && self.attractiveness != self.original_attractiveness
        {
            return Err(invalid(
                0,
                "segment attractiveness is not serialized before inner version 121030",
            ));
        }

        let mut output = Vec::new();
        if self.format_version >= SEGMENT_CONDITION_VERSION {
            write_encoded_byte_array(&mut output, &self.condition);
        }
        if self.format_version >= SEGMENT_NOISE_VERSION {
            write_encoded_ushort_array(&mut output, &self.noise_buffer);
            write_encoded_byte_array(&mut output, &self.noise_density);
        }
        if self.format_version >= SEGMENT_ATTRACTIVENESS_VERSION {
            write_encoded_byte_array(&mut output, &self.attractiveness);
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

    /// Returns one record per segment with nonzero primary flags.
    pub fn records(&self) -> &[NetSegmentRecord] {
        &self.records
    }

    pub(crate) fn source_segment_flags(&self) -> &[u8] {
        &self.source_segment_flags
    }

    /// Writes the original record bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl NetSegmentRecordsPrefix {
    /// Consumes the prefix to edit records for active segments.
    pub fn into_editor(self) -> NetSegmentRecordsEditor {
        NetSegmentRecordsEditor {
            format_version: self.format_version,
            original_records: self.records.clone(),
            records: self.records,
            original_segment_flags: self.source_segment_flags,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetSegmentRecordsEditor {
    /// Returns the inner DataSerializer version associated with these records.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns records in ascending active-segment buffer order.
    pub fn records_mut(&mut self) -> &mut [NetSegmentRecord] {
        &mut self.records
    }

    /// Encodes fixed records with the version-specific fields and byte order.
    ///
    /// Unchanged records retain their source bytes. Optional fields are only
    /// writable from their respective inner-version gates; 24-bit lane and
    /// path IDs are range-checked.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.records.len() != self.original_records.len() {
            return Err(invalid_record(
                0,
                "edited segment record count does not match the source",
            ));
        }
        if self.records == self.original_records {
            return Ok(self.original_prefix.clone());
        }

        let mut output = Vec::with_capacity(self.original_prefix.len());
        for (record, original) in self.records.iter().zip(&self.original_records) {
            if record.index != original.index {
                return Err(invalid_record(
                    output.len(),
                    "segment record index is derived from flags and cannot be changed",
                ));
            }
            if self.format_version < SEGMENT_NAME_SEED_VERSION
                && record.name_seed != original.name_seed
            {
                return Err(invalid_record(
                    output.len(),
                    "segment name seed is not serialized before inner version 312",
                ));
            }
            if self.format_version < SEGMENT_PATH_OVERRIDE_VERSION
                && record.override_path_find_direction_limit
                    != original.override_path_find_direction_limit
            {
                return Err(invalid_record(
                    output.len(),
                    "path-direction override is not serialized before inner version 113015",
                ));
            }
            if self.format_version < SEGMENT_TREE_VERSION
                && record.tree_present != original.tree_present
            {
                return Err(invalid_record(
                    output.len(),
                    "tree-presence flag is not serialized before inner version 114002",
                ));
            }
            if self.format_version < SEGMENT_FENCE_VERSION
                && record.fence_present != original.fence_present
            {
                return Err(invalid_record(
                    output.len(),
                    "fence-presence flags are not serialized before inner version 120003",
                ));
            }
            if self.format_version < SEGMENT_EVENT_ROUTE_VERSION
                && (record.event_route != original.event_route
                    || record.next_event_route_segment != original.next_event_route_segment)
            {
                return Err(invalid_record(
                    output.len(),
                    "event-route fields are not serialized before inner version 121005",
                ));
            }

            output.extend_from_slice(&record.build_index.to_be_bytes());
            output.extend_from_slice(&record.modified_index.to_be_bytes());
            write_record_u24(&mut output, record.lanes)?;
            write_record_u24(&mut output, record.path)?;
            for bits in record
                .start_direction_bits
                .iter()
                .chain(&record.end_direction_bits)
            {
                output.extend_from_slice(&bits.to_be_bytes());
            }
            output.extend_from_slice(&record.start_node.to_be_bytes());
            output.extend_from_slice(&record.end_node.to_be_bytes());
            for block in record.blocks {
                output.extend_from_slice(&block.to_be_bytes());
            }
            output.extend_from_slice(&record.traffic_buffer.to_be_bytes());
            output.push(record.traffic_density);
            if self.format_version >= SEGMENT_NAME_SEED_VERSION {
                output.extend_from_slice(&record.name_seed.to_be_bytes());
            }
            if self.format_version >= SEGMENT_PATH_OVERRIDE_VERSION {
                output.push(record.override_path_find_direction_limit);
            }
            if self.format_version >= SEGMENT_TREE_VERSION {
                output.push(record.tree_present);
            }
            if self.format_version >= SEGMENT_FENCE_VERSION {
                output.extend_from_slice(&record.fence_present);
            }
            if self.format_version >= SEGMENT_EVENT_ROUTE_VERSION {
                output.extend_from_slice(&record.event_route.to_be_bytes());
                output.extend_from_slice(&record.next_event_route_segment.to_be_bytes());
            }
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
        let indices_start = reader.offset();
        let indices = read_encoded_ushort_array(&mut reader, total_count)?;
        let serialized_indices = reader.consumed_bytes()[indices_start..reader.offset()].to_vec();
        let mut index_iter = indices.into_iter();
        let node_info_indices = index_iter.by_ref().take(node_count).collect();
        let segment_info_indices = index_iter.by_ref().take(segment_count).collect();
        let guide_indices = index_iter.by_ref().take(guide_count).collect();
        let milestone_indices = index_iter.take(milestone_count).collect();
        let (names, serialized_name_count, serialized_name_entries) =
            read_prefab_names_with_raw(&mut reader)?;
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                node_info_indices,
                segment_info_indices,
                guide_indices,
                milestone_indices,
                names,
                source_node_flags: node_flags.encode(),
                source_segment_flags: segment_flags.encode(),
                source_guide_counts: guide_counts.encode(),
                serialized_indices,
                serialized_name_count,
                serialized_name_entries,
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

    /// Consumes the prefix to edit its prefab indices and names.
    pub fn into_editor(self) -> NetInfoPrefabEditor {
        NetInfoPrefabEditor {
            format_version: self.format_version,
            original_node_info_indices: self.node_info_indices.clone(),
            original_segment_info_indices: self.segment_info_indices.clone(),
            original_guide_indices: self.guide_indices.clone(),
            original_milestone_indices: self.milestone_indices.clone(),
            original_names: self.names.clone(),
            node_info_indices: self.node_info_indices,
            segment_info_indices: self.segment_info_indices,
            guide_indices: self.guide_indices,
            milestone_indices: self.milestone_indices,
            names: self.names,
            source_node_flags: self.source_node_flags,
            source_segment_flags: self.source_segment_flags,
            source_guide_counts: self.source_guide_counts,
            serialized_indices: self.serialized_indices,
            serialized_name_count: self.serialized_name_count,
            serialized_name_entries: self.serialized_name_entries,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetInfoPrefabEditor {
    /// Returns the inner DataSerializer version associated with this table.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns mutable network-info indices for active nodes.
    pub fn node_info_indices_mut(&mut self) -> &mut [u16] {
        &mut self.node_info_indices
    }

    /// Returns mutable network-info indices for active segments.
    pub fn segment_info_indices_mut(&mut self) -> &mut [u16] {
        &mut self.segment_info_indices
    }

    /// Returns mutable network-info indices for guide objects.
    pub fn guide_indices_mut(&mut self) -> &mut [u16] {
        &mut self.guide_indices
    }

    /// Returns mutable network-info indices for milestone objects.
    pub fn milestone_indices_mut(&mut self) -> &mut [u16] {
        &mut self.milestone_indices
    }

    /// Returns mutable names in original table order.
    pub fn names_mut(&mut self) -> &mut [Option<String>] {
        &mut self.names
    }

    /// Encodes edited indices while retaining unchanged name-entry bytes.
    ///
    /// The index array is rewritten canonically only if an index changes.
    /// Changed names use the crate's unique-string writer; unchanged names
    /// retain their exact source bytes, including compact/full encoding.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.node_info_indices.len() != self.original_node_info_indices.len()
            || self.segment_info_indices.len() != self.original_segment_info_indices.len()
            || self.guide_indices.len() != self.original_guide_indices.len()
            || self.milestone_indices.len() != self.original_milestone_indices.len()
            || self.names.len() != self.original_names.len()
            || self.names.len() != self.serialized_name_entries.len()
        {
            return Err(invalid(
                0,
                "edited network-info prefab counts do not match the source",
            ));
        }
        if self.node_info_indices == self.original_node_info_indices
            && self.segment_info_indices == self.original_segment_info_indices
            && self.guide_indices == self.original_guide_indices
            && self.milestone_indices == self.original_milestone_indices
            && self.names == self.original_names
        {
            return Ok(self.original_prefix.clone());
        }

        let indices_changed = self.node_info_indices != self.original_node_info_indices
            || self.segment_info_indices != self.original_segment_info_indices
            || self.guide_indices != self.original_guide_indices
            || self.milestone_indices != self.original_milestone_indices;
        let mut output = Vec::with_capacity(self.original_prefix.len());
        if indices_changed {
            let mut indices = Vec::new();
            indices.extend_from_slice(&self.node_info_indices);
            indices.extend_from_slice(&self.segment_info_indices);
            indices.extend_from_slice(&self.guide_indices);
            indices.extend_from_slice(&self.milestone_indices);
            write_encoded_ushort_array(&mut output, &indices);
        } else {
            output.extend_from_slice(&self.serialized_indices);
        }
        output.extend_from_slice(&self.serialized_name_count);
        for ((name, original_name), raw_entry) in self
            .names
            .iter()
            .zip(&self.original_names)
            .zip(&self.serialized_name_entries)
        {
            if name == original_name {
                output.extend_from_slice(raw_entry);
            } else {
                write_nullable_unique_string(&mut output, name.as_deref())?;
            }
        }
        Ok(output)
    }

    pub(crate) fn source_node_flags(&self) -> &[u8] {
        &self.source_node_flags
    }

    pub(crate) fn source_segment_flags(&self) -> &[u8] {
        &self.source_segment_flags
    }

    pub(crate) fn source_guide_counts(&self) -> &[u8] {
        &self.source_guide_counts
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original_prefix
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
        let mut serialized_indices = Vec::new();
        let mut serialized_name_count = Vec::new();
        let mut serialized_name_entries = Vec::new();
        let (indices, names) = if format_version >= SEGMENT_TREE_VERSION {
            let count = segment_records
                .records()
                .iter()
                .filter(|record| record.tree_present == 1)
                .count();
            let indices_start = reader.offset();
            let indices = read_encoded_ushort_array(&mut reader, count)?;
            serialized_indices = reader.consumed_bytes()[indices_start..reader.offset()].to_vec();
            let (names, count_bytes, name_entries) = read_prefab_names_with_raw(&mut reader)?;
            serialized_name_count = count_bytes;
            serialized_name_entries = name_entries;
            (indices, names)
        } else {
            (Vec::new(), Vec::new())
        };
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                indices,
                names,
                source_segment_flags: segment_records.source_segment_flags.clone(),
                source_tree_presence: segment_records
                    .records()
                    .iter()
                    .map(|record| record.tree_present)
                    .collect(),
                serialized_indices,
                serialized_name_count,
                serialized_name_entries,
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

    /// Consumes the prefix to edit its tree-prefab indices and names.
    pub fn into_editor(self) -> NetTreePrefabEditor {
        NetTreePrefabEditor {
            format_version: self.format_version,
            original_indices: self.indices.clone(),
            original_names: self.names.clone(),
            indices: self.indices,
            names: self.names,
            source_segment_flags: self.source_segment_flags,
            source_tree_presence: self.source_tree_presence,
            serialized_indices: self.serialized_indices,
            serialized_name_count: self.serialized_name_count,
            serialized_name_entries: self.serialized_name_entries,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetTreePrefabEditor {
    /// Returns the inner DataSerializer version associated with this table.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns mutable tree-prefab indices in active-segment order.
    pub fn indices_mut(&mut self) -> &mut [u16] {
        &mut self.indices
    }

    /// Returns mutable names in original table order.
    pub fn names_mut(&mut self) -> &mut [Option<String>] {
        &mut self.names
    }

    /// Encodes edited indices and names while preserving unchanged name bytes.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.indices.len() != self.original_indices.len()
            || self.names.len() != self.original_names.len()
            || self.names.len() != self.serialized_name_entries.len()
        {
            return Err(invalid(
                0,
                "edited tree-prefab counts do not match the source",
            ));
        }
        if self.indices == self.original_indices && self.names == self.original_names {
            return Ok(self.original_prefix.clone());
        }

        let mut output = Vec::with_capacity(self.original_prefix.len());
        if self.indices != self.original_indices {
            write_encoded_ushort_array(&mut output, &self.indices);
        } else {
            output.extend_from_slice(&self.serialized_indices);
        }
        output.extend_from_slice(&self.serialized_name_count);
        for ((name, original_name), raw_entry) in self
            .names
            .iter()
            .zip(&self.original_names)
            .zip(&self.serialized_name_entries)
        {
            if name == original_name {
                output.extend_from_slice(raw_entry);
            } else {
                write_nullable_unique_string(&mut output, name.as_deref())?;
            }
        }
        Ok(output)
    }

    pub(crate) fn source_tree_presence(&self) -> &[u8] {
        &self.source_tree_presence
    }

    pub(crate) fn source_segment_flags(&self) -> &[u8] {
        &self.source_segment_flags
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original_prefix
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
        let mut serialized_left_indices = Vec::new();
        let mut serialized_left_name_count = Vec::new();
        let mut serialized_left_name_entries = Vec::new();
        let mut serialized_right_indices = Vec::new();
        let mut serialized_right_name_count = Vec::new();
        let mut serialized_right_name_entries = Vec::new();
        let (left_indices, left_names, right_indices, right_names) = if format_version
            >= SEGMENT_FENCE_VERSION
        {
            let left_count = segment_records
                .records()
                .iter()
                .filter(|record| record.fence_present[0] == 1)
                .count();
            let left_start = reader.offset();
            let left_indices = read_encoded_ushort_array(&mut reader, left_count)?;
            serialized_left_indices = reader.consumed_bytes()[left_start..reader.offset()].to_vec();
            let (left_names, left_count_bytes, left_entries) =
                read_prefab_names_with_raw(&mut reader)?;
            serialized_left_name_count = left_count_bytes;
            serialized_left_name_entries = left_entries;

            let right_count = segment_records
                .records()
                .iter()
                .filter(|record| record.fence_present[1] == 1)
                .count();
            let right_start = reader.offset();
            let right_indices = read_encoded_ushort_array(&mut reader, right_count)?;
            serialized_right_indices =
                reader.consumed_bytes()[right_start..reader.offset()].to_vec();
            let (right_names, right_count_bytes, right_entries) =
                read_prefab_names_with_raw(&mut reader)?;
            serialized_right_name_count = right_count_bytes;
            serialized_right_name_entries = right_entries;
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
                source_segment_flags: segment_records.source_segment_flags.clone(),
                source_fence_presence: segment_records
                    .records()
                    .iter()
                    .map(|record| record.fence_present)
                    .collect(),
                serialized_left_indices,
                serialized_left_name_count,
                serialized_left_name_entries,
                serialized_right_indices,
                serialized_right_name_count,
                serialized_right_name_entries,
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

    /// Consumes the prefix to edit the left and right fence tables.
    pub fn into_editor(self) -> NetFencePrefabEditor {
        NetFencePrefabEditor {
            format_version: self.format_version,
            original_left_indices: self.left_indices.clone(),
            original_left_names: self.left_names.clone(),
            original_right_indices: self.right_indices.clone(),
            original_right_names: self.right_names.clone(),
            left_indices: self.left_indices,
            left_names: self.left_names,
            right_indices: self.right_indices,
            right_names: self.right_names,
            source_segment_flags: self.source_segment_flags,
            source_fence_presence: self.source_fence_presence,
            serialized_left_indices: self.serialized_left_indices,
            serialized_left_name_count: self.serialized_left_name_count,
            serialized_left_name_entries: self.serialized_left_name_entries,
            serialized_right_indices: self.serialized_right_indices,
            serialized_right_name_count: self.serialized_right_name_count,
            serialized_right_name_entries: self.serialized_right_name_entries,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetFencePrefabEditor {
    /// Returns the inner DataSerializer version associated with these tables.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns mutable left-fence prefab indices in segment order.
    pub fn left_indices_mut(&mut self) -> &mut [u16] {
        &mut self.left_indices
    }

    /// Returns mutable left-fence names in original table order.
    pub fn left_names_mut(&mut self) -> &mut [Option<String>] {
        &mut self.left_names
    }

    /// Returns mutable right-fence prefab indices in segment order.
    pub fn right_indices_mut(&mut self) -> &mut [u16] {
        &mut self.right_indices
    }

    /// Returns mutable right-fence names in original table order.
    pub fn right_names_mut(&mut self) -> &mut [Option<String>] {
        &mut self.right_names
    }

    /// Encodes both fence tables and preserves unchanged name-entry bytes.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.left_indices.len() != self.original_left_indices.len()
            || self.left_names.len() != self.original_left_names.len()
            || self.left_names.len() != self.serialized_left_name_entries.len()
            || self.right_indices.len() != self.original_right_indices.len()
            || self.right_names.len() != self.original_right_names.len()
            || self.right_names.len() != self.serialized_right_name_entries.len()
        {
            return Err(invalid(
                0,
                "edited fence-prefab counts do not match the source",
            ));
        }
        if self.left_indices == self.original_left_indices
            && self.left_names == self.original_left_names
            && self.right_indices == self.original_right_indices
            && self.right_names == self.original_right_names
        {
            return Ok(self.original_prefix.clone());
        }

        let mut output = Vec::with_capacity(self.original_prefix.len());
        if self.left_indices == self.original_left_indices {
            output.extend_from_slice(&self.serialized_left_indices);
        } else {
            write_encoded_ushort_array(&mut output, &self.left_indices);
        }
        output.extend_from_slice(&self.serialized_left_name_count);
        for ((name, original_name), raw_entry) in self
            .left_names
            .iter()
            .zip(&self.original_left_names)
            .zip(&self.serialized_left_name_entries)
        {
            if name == original_name {
                output.extend_from_slice(raw_entry);
            } else {
                write_nullable_unique_string(&mut output, name.as_deref())?;
            }
        }

        if self.right_indices == self.original_right_indices {
            output.extend_from_slice(&self.serialized_right_indices);
        } else {
            write_encoded_ushort_array(&mut output, &self.right_indices);
        }
        output.extend_from_slice(&self.serialized_right_name_count);
        for ((name, original_name), raw_entry) in self
            .right_names
            .iter()
            .zip(&self.original_right_names)
            .zip(&self.serialized_right_name_entries)
        {
            if name == original_name {
                output.extend_from_slice(raw_entry);
            } else {
                write_nullable_unique_string(&mut output, name.as_deref())?;
            }
        }
        Ok(output)
    }

    pub(crate) fn source_segment_flags(&self) -> &[u8] {
        &self.source_segment_flags
    }

    pub(crate) fn source_fence_presence(&self) -> &[[u8; 2]] {
        &self.source_fence_presence
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original_prefix
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
        let max_wetness_offset = if format_version >= 237 {
            let offset = reader.offset();
            Some(offset)
        } else {
            None
        };
        let (last_max_wetness, current_max_wetness) = if max_wetness_offset.is_some() {
            (reader.read_u8()?, reader.read_u8()?)
        } else {
            (0, 0)
        };
        let name_modified_count_offset = if format_version >= 319 {
            Some(reader.offset())
        } else {
            None
        };
        let name_modified_count = if format_version >= 319 {
            read_segment_u24(&mut reader)?
        } else {
            0
        };
        let adjusted_segment_count_offset = if format_version >= 321 {
            Some(reader.offset())
        } else {
            None
        };
        let adjusted_segments_count = if format_version >= 321 {
            let count = usize::try_from(read_segment_u24(&mut reader)?)
                .map_err(|_| reader.invalid("adjusted-segment count does not fit platform"))?;
            if count > ADJUSTED_SEGMENT_WORDS {
                return Err(reader.invalid("adjusted-segment count exceeds 576 words"));
            }
            Some(count)
        } else {
            None
        };
        let adjusted_segment_array_offset = adjusted_segments_count.map(|_| reader.offset());
        let adjusted_segments = if let Some(count) = adjusted_segments_count {
            read_encoded_ulong_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let adjusted_segment_array_end = adjusted_segment_array_offset.map(|_| reader.offset());
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
                max_wetness_offset,
                name_modified_count,
                name_modified_count_offset,
                adjusted_segments,
                adjusted_segment_count_offset,
                adjusted_segment_array_offset,
                adjusted_segment_array_end,
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

    /// Consumes the tail prefix to edit its fixed-width counters.
    pub fn into_editor(self) -> NetManagerTailEditor {
        NetManagerTailEditor {
            format_version: self.format_version,
            last_max_wetness: self.last_max_wetness,
            current_max_wetness: self.current_max_wetness,
            max_wetness_offset: self.max_wetness_offset,
            name_modified_count: self.name_modified_count,
            name_modified_count_offset: self.name_modified_count_offset,
            adjusted_segments: self.adjusted_segments.clone(),
            original_adjusted_segments: self.adjusted_segments,
            adjusted_segment_count_offset: self.adjusted_segment_count_offset,
            adjusted_segment_array_offset: self.adjusted_segment_array_offset,
            adjusted_segment_array_end: self.adjusted_segment_array_end,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl NetManagerTailEditor {
    /// Returns the inner DataSerializer version associated with this editor.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Sets the final wetness counters, serialized from inner version 237.
    pub fn set_max_wetness(&mut self, last: u8, current: u8) -> Result<(), CrpError> {
        if self.max_wetness_offset.is_none() {
            return Err(invalid(
                0,
                "NetManager wetness counters are not serialized before inner version 237",
            ));
        }
        self.last_max_wetness = last;
        self.current_max_wetness = current;
        Ok(())
    }

    /// Sets the 24-bit name-modification count, serialized from inner version 319.
    pub fn set_name_modified_count(&mut self, count: u32) -> Result<(), CrpError> {
        let Some(offset) = self.name_modified_count_offset else {
            return Err(invalid(
                0,
                "NetManager name-modification count is not serialized before inner version 319",
            ));
        };
        if count > 0x00ff_ffff {
            return Err(invalid(
                offset,
                "NetManager name-modification count exceeds its 24-bit field",
            ));
        }
        self.name_modified_count = count;
        Ok(())
    }

    /// Returns mutable adjusted-segment words in their serialized order.
    ///
    /// The field is serialized from inner version 321. Encoding rejects more
    /// than 576 words, matching the game's allocated array length.
    pub fn adjusted_segments_mut(&mut self) -> &mut Vec<u64> {
        &mut self.adjusted_segments
    }

    /// Encodes edits while retaining all unchanged source bytes.
    ///
    /// A changed adjusted-segment array uses literal `EncodedArray.ULong`
    /// blocks and can change this prefix's length. An unchanged array retains
    /// its original encoded representation.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut output = self.original_prefix.clone();
        if let Some(offset) = self.max_wetness_offset {
            let end = offset
                .checked_add(2)
                .ok_or(CrpError::ArithmeticOverflow("NetManager wetness field end"))?;
            let field = output.get_mut(offset..end).ok_or_else(|| {
                invalid(
                    offset,
                    "NetManager wetness field is outside the source prefix",
                )
            })?;
            field.copy_from_slice(&[self.last_max_wetness, self.current_max_wetness]);
        }
        if let Some(offset) = self.name_modified_count_offset {
            let end = offset.checked_add(3).ok_or(CrpError::ArithmeticOverflow(
                "NetManager name-modification field end",
            ))?;
            let field = output.get_mut(offset..end).ok_or_else(|| {
                invalid(
                    offset,
                    "NetManager name-modification field is outside the source prefix",
                )
            })?;
            field.copy_from_slice(&[
                (self.name_modified_count >> 16) as u8,
                (self.name_modified_count >> 8) as u8,
                self.name_modified_count as u8,
            ]);
        }
        if self.adjusted_segments != self.original_adjusted_segments {
            let (Some(count_offset), Some(array_offset), Some(array_end)) = (
                self.adjusted_segment_count_offset,
                self.adjusted_segment_array_offset,
                self.adjusted_segment_array_end,
            ) else {
                return Err(invalid(
                    0,
                    "adjusted-segment array is not serialized before inner version 321",
                ));
            };
            if self.adjusted_segments.len() > ADJUSTED_SEGMENT_WORDS {
                return Err(invalid(
                    count_offset,
                    "adjusted-segment count exceeds 576 words",
                ));
            }
            let count_end = count_offset
                .checked_add(3)
                .ok_or(CrpError::ArithmeticOverflow(
                    "NetManager adjusted-segment count end",
                ))?;
            if count_end > array_offset || array_offset > array_end {
                return Err(invalid(
                    count_offset,
                    "NetManager adjusted-segment offsets are inconsistent",
                ));
            }
            let suffix = output.get(array_end..).ok_or_else(|| {
                invalid(
                    array_end,
                    "adjusted-segment array end is outside the source prefix",
                )
            })?;
            let mut rebuilt = output
                .get(..array_offset)
                .ok_or_else(|| {
                    invalid(
                        array_offset,
                        "adjusted-segment array start is outside the source prefix",
                    )
                })?
                .to_vec();
            let count_bytes = rebuilt.get_mut(count_offset..count_end).ok_or_else(|| {
                invalid(
                    count_offset,
                    "adjusted-segment count is outside the source prefix",
                )
            })?;
            let count = u32::try_from(self.adjusted_segments.len())
                .map_err(|_| CrpError::ArithmeticOverflow("NetManager adjusted-segment count"))?;
            count_bytes.copy_from_slice(&[(count >> 16) as u8, (count >> 8) as u8, count as u8]);
            write_encoded_ulong_array(&mut rebuilt, &self.adjusted_segments);
            rebuilt.extend_from_slice(suffix);
            output = rebuilt;
        }
        Ok(output)
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original_prefix
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
        let disabled_raw = reader.read_u8()?;
        let disabled = disabled_raw == 1;
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
                disabled_raw,
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
        output.push(if self.disabled == self.original.disabled {
            self.original.disabled_raw
        } else {
            u8::from(self.disabled)
        });
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

fn read_prefab_names_with_raw(
    reader: &mut Reader<'_>,
) -> Result<(Vec<Option<String>>, Vec<u8>, Vec<Vec<u8>>), CrpError> {
    let count_start = reader.offset();
    let name_count = usize::from(reader.read_u16_be()?);
    let serialized_count = reader.consumed_bytes()[count_start..reader.offset()].to_vec();
    let mut names = Vec::with_capacity(name_count);
    let mut serialized_entries = Vec::with_capacity(name_count);
    for _ in 0..name_count {
        let entry_start = reader.offset();
        names.push(reader.read_unique_string()?);
        serialized_entries.push(reader.consumed_bytes()[entry_start..reader.offset()].to_vec());
    }
    Ok((names, serialized_count, serialized_entries))
}

fn read_segment_u24(reader: &mut Reader<'_>) -> Result<u32, CrpError> {
    let bytes = reader.read_bytes(3)?;
    Ok((u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]))
}

fn write_record_u24(output: &mut Vec<u8>, value: u32) -> Result<(), CrpError> {
    if value > 0x00ff_ffff {
        return Err(invalid_record(
            output.len(),
            "segment lane or path value exceeds 24-bit storage",
        ));
    }
    output.extend_from_slice(&[
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    ]);
    Ok(())
}

fn invalid_record(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "NetManager+Data segment records".to_owned(),
        offset,
        reason: reason.into(),
    }
}

fn invalid_node_record(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "NetManager+Data node records".to_owned(),
        offset,
        reason: reason.into(),
    }
}

fn invalid_node_counter(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "NetManager+Data node counters".to_owned(),
        offset,
        reason: reason.into(),
    }
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

        let mut other_field_edit = guide.clone().into_editor();
        *other_field_edit.next_repeat_mut() = 0x1122_3344;
        let mut expected_other_field_edit = bytes;
        expected_other_field_edit[..4].copy_from_slice(&0x1122_3344_u32.to_be_bytes());
        assert_eq!(
            other_field_edit
                .encode()
                .expect("guide field edit preserves unchanged raw bool"),
            expected_other_field_edit
        );

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

#[cfg(test)]
mod net_object_header_tests {
    use super::NetObjectHeader;

    #[test]
    fn object_header_variants_match_independent_expected_bytes() {
        let cases = [
            (NetObjectHeader::Null, vec![0xff]),
            (NetObjectHeader::Existing(0x12_3456), vec![0x12, 0x34, 0x56]),
            (
                NetObjectHeader::NewSharedType(0x1234),
                vec![0xfe, 0x12, 0x34],
            ),
            (
                NetObjectHeader::NewType("A".to_owned()),
                vec![0xfe, 0xfe, 0x80, 0x00, 0x01, 0xc1],
            ),
            (
                NetObjectHeader::NewType("Ā".to_owned()),
                vec![0xfe, 0xfe, 0x00, 0x00, 0x01, 0x00, 0x80],
            ),
        ];

        for (header, expected) in cases {
            let encoded = header
                .encode_canonical()
                .expect("valid object header encodes");
            assert_eq!(encoded, expected);
            let (decoded, consumed) =
                NetObjectHeader::decode_from(&expected).expect("expected bytes decode");
            assert_eq!(decoded, header);
            assert_eq!(consumed, expected.len());
        }
    }

    #[test]
    fn object_header_decoder_leaves_following_bytes_and_checks_truncation() {
        let (header, consumed) = NetObjectHeader::decode_from(&[0xff, 0x42])
            .expect("null header decodes before trailing field");
        assert_eq!(header, NetObjectHeader::Null);
        assert_eq!(consumed, 1);

        for truncated in [&[][..], &[0xfe][..], &[0xfe, 0x12][..], &[0x01, 0x02][..]] {
            assert!(NetObjectHeader::decode_from(truncated).is_err());
        }
        assert!(NetObjectHeader::decode_from(&[0xfe, 0xff]).is_err());
    }

    #[test]
    fn object_header_writer_rejects_reserved_marker_ranges() {
        assert!(
            NetObjectHeader::Existing(0xfe_0000)
                .encode_canonical()
                .is_err()
        );
        assert!(
            NetObjectHeader::Existing(u32::MAX)
                .encode_canonical()
                .is_err()
        );
        assert!(
            NetObjectHeader::NewSharedType(0xfe00)
                .encode_canonical()
                .is_err()
        );
        assert!(
            NetObjectHeader::NewSharedType(u16::MAX)
                .encode_canonical()
                .is_err()
        );

        assert_eq!(
            NetObjectHeader::Existing(0xfd_ffff)
                .encode_canonical()
                .expect("largest non-reserved object ID encodes"),
            [0xfd, 0xff, 0xff]
        );
        assert_eq!(
            NetObjectHeader::NewSharedType(0xfdff)
                .encode_canonical()
                .expect("largest non-reserved shared-type index encodes"),
            [0xfe, 0xfd, 0xff]
        );
    }
}

#[cfg(test)]
mod net_node_flag_editor_tests {
    use super::{FULL_NODE_COUNT_VERSION, LEGACY_NODE_FLAG_COUNT, NODE_COUNT, NetNodeFlagsPrefix};
    use crate::CrpError;

    fn expected_uint_literals(values: &[u32]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for chunk in values.chunks(64) {
            bytes.push((chunk.len() - 1) as u8);
            for value in chunk {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
        bytes
    }

    #[test]
    fn node_flag_editor_obeys_version_gates_and_primary_activity() {
        let mut legacy_flags = vec![0; LEGACY_NODE_FLAG_COUNT];
        legacy_flags[0] = 1;
        legacy_flags[LEGACY_NODE_FLAG_COUNT - 1] = 2;
        let legacy_source = expected_uint_literals(&legacy_flags);
        let (legacy, consumed) = NetNodeFlagsPrefix::decode_prefix(164, &legacy_source).unwrap();
        assert_eq!(consumed, legacy_source.len());
        assert_eq!(legacy.node_flags().len(), NODE_COUNT - 1);
        assert_eq!(legacy.node_flags()[LEGACY_NODE_FLAG_COUNT].flags, 0);
        let mut editor = legacy.into_editor();
        assert!(editor.secondary_flags().is_none());
        assert!(matches!(
            editor.set_secondary_flag_bits(0, 1),
            Err(CrpError::InvalidAssetPayload { .. })
        ));
        assert!(editor.set_primary_flag_bits(0, 0).is_err());
        assert!(
            editor
                .set_primary_flag_bits(LEGACY_NODE_FLAG_COUNT, 1)
                .is_err()
        );
        editor.set_primary_flag_bits(0, 3).unwrap();
        editor
            .set_primary_flag_bits(LEGACY_NODE_FLAG_COUNT - 1, 4)
            .unwrap();
        legacy_flags[0] = 3;
        legacy_flags[LEGACY_NODE_FLAG_COUNT - 1] = 4;
        let expected = expected_uint_literals(&legacy_flags);
        assert_eq!(editor.encode().unwrap(), expected);

        let full_flags = vec![0; NODE_COUNT - 1];
        let full_source = expected_uint_literals(&full_flags);
        let (full, consumed) =
            NetNodeFlagsPrefix::decode_prefix(FULL_NODE_COUNT_VERSION, &full_source).unwrap();
        assert_eq!(consumed, full_source.len());
        let full_editor = full.into_editor();
        assert_eq!(full_editor.primary_flags().len(), NODE_COUNT - 1);
        assert!(full_editor.secondary_flags().is_none());
        assert_eq!(full_editor.encode().unwrap(), full_source);

        let mut primary_flags = vec![0; NODE_COUNT - 1];
        primary_flags[0] = 1;
        let mut secondary_flags = vec![0; NODE_COUNT - 1];
        let mut interleaved = Vec::with_capacity((NODE_COUNT - 1) * 2);
        for (primary, secondary) in primary_flags.iter().zip(&secondary_flags) {
            interleaved.extend_from_slice(&[*primary, *secondary]);
        }
        let source = expected_uint_literals(&interleaved);
        let (paired, consumed) = NetNodeFlagsPrefix::decode_prefix(115_018, &source).unwrap();
        assert_eq!(consumed, source.len());
        let mut editor = paired.into_editor();
        assert_eq!(editor.secondary_flags().unwrap().len(), NODE_COUNT - 1);
        assert!(editor.set_primary_flag_bits(1, 1).is_err());
        editor.set_primary_flag_bits(0, 3).unwrap();
        editor.set_secondary_flag_bits(0, 7).unwrap();
        primary_flags[0] = 3;
        secondary_flags[0] = 7;
        interleaved.clear();
        for (primary, secondary) in primary_flags.iter().zip(&secondary_flags) {
            interleaved.extend_from_slice(&[*primary, *secondary]);
        }
        assert_eq!(
            editor.encode().unwrap(),
            expected_uint_literals(&interleaved)
        );
    }
}

#[cfg(test)]
mod net_node_problem_editor_tests {
    use super::{
        LEGACY_NODE_FLAG_COUNT, NODE_COUNT, NetNodeFlagsPrefix, NetNodeProblemsPrefix,
        SAVE_APP_PROBLEMS_VERSION, TWO_NODE_FLAGS_VERSION, TWO_PROBLEMS_VERSION,
    };

    fn expected_uint_literals(values: &[u32]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for chunk in values.chunks(64) {
            bytes.push((chunk.len() - 1) as u8);
            for value in chunk {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
        bytes
    }

    fn expected_ulong_literals(values: &[u64]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for chunk in values.chunks(64) {
            bytes.push((chunk.len() - 1) as u8);
            for value in chunk {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
        bytes
    }

    fn node_flags(format_version: u32, active_indexes: &[usize]) -> NetNodeFlagsPrefix {
        let primary_count = if format_version < 202 {
            LEGACY_NODE_FLAG_COUNT
        } else {
            NODE_COUNT - 1
        };
        let mut primary = vec![0; primary_count];
        for index in active_indexes {
            primary[*index] = 1;
        }
        let encoded = if format_version >= TWO_NODE_FLAGS_VERSION {
            let mut paired = Vec::with_capacity(primary.len() * 2);
            for value in primary {
                paired.extend_from_slice(&[value, 0]);
            }
            expected_uint_literals(&paired)
        } else {
            expected_uint_literals(&primary)
        };
        NetNodeFlagsPrefix::decode_prefix(format_version, &encoded)
            .unwrap()
            .0
    }

    #[test]
    fn legacy_node_problem_editor_reverses_bit_relocation_and_rejects_unrepresentable_bits() {
        let flags = node_flags(164, &[0, 2]);
        let original_values = [0xc000_0005, 0x8000_0007];
        let source = expected_uint_literals(&original_values);
        let (prefix, consumed) =
            NetNodeProblemsPrefix::decode_prefix(164, None, &flags, &source).unwrap();
        assert_eq!(consumed, source.len());
        assert_eq!(prefix.problems()[0].problems1, 0xc000_0000_0000_0005);
        assert_eq!(prefix.problems()[1].problems1, 0x8000_0000_0000_0007);
        assert_eq!(prefix.encode(), source);

        let mut editor = prefix.into_editor();
        editor.problems1_mut()[0] = 0x4000_0000_0000_0009;
        editor.problems1_mut()[1] = 10;
        assert_eq!(
            editor.encode().unwrap(),
            expected_uint_literals(&[0x4000_0009, 10])
        );
        editor.problems1_mut()[0] = 1 << 40;
        assert!(editor.encode().is_err());
    }

    #[test]
    fn node_problem_editor_obeys_application_and_format_gates() {
        let flags = node_flags(221, &[0]);
        let old_source = expected_uint_literals(&[0xc000_0001]);
        let (old_prefix, _) = NetNodeProblemsPrefix::decode_prefix(
            221,
            Some(SAVE_APP_PROBLEMS_VERSION - 1),
            &flags,
            &old_source,
        )
        .unwrap();
        let mut old_editor = old_prefix.into_editor();
        old_editor.problems1_mut()[0] = 0x4000_0000_0000_0002;
        assert_eq!(
            old_editor.encode().unwrap(),
            expected_uint_literals(&[0x4000_0002])
        );

        let wide_source = expected_ulong_literals(&[7]);
        let (wide_prefix, _) = NetNodeProblemsPrefix::decode_prefix(
            221,
            Some(SAVE_APP_PROBLEMS_VERSION),
            &flags,
            &wide_source,
        )
        .unwrap();
        let mut wide_editor = wide_prefix.into_editor();
        wide_editor.problems1_mut()[0] = 0x1122_3344_5566_7788;
        assert_eq!(
            wide_editor.encode().unwrap(),
            expected_ulong_literals(&[0x1122_3344_5566_7788])
        );
        assert!(NetNodeProblemsPrefix::decode_prefix(221, None, &flags, &old_source).is_err());

        let modern_flags = node_flags(TWO_PROBLEMS_VERSION, &[0, 2]);
        let modern_source = expected_ulong_literals(&[1, 2, 3, 4]);
        let (modern_prefix, _) = NetNodeProblemsPrefix::decode_prefix(
            TWO_PROBLEMS_VERSION,
            None,
            &modern_flags,
            &modern_source,
        )
        .unwrap();
        let mut modern_editor = modern_prefix.into_editor();
        modern_editor.problems1_mut()[0] = 5;
        modern_editor.problems2_mut().unwrap()[0] = 6;
        assert_eq!(
            modern_editor.encode().unwrap(),
            expected_ulong_literals(&[5, 6, 3, 4])
        );
        assert!(modern_editor.problems2_mut().is_some());
    }
}
