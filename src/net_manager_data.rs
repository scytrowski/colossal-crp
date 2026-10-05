use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    read_encoded_byte_array, read_encoded_uint_array, read_encoded_ulong_array,
    read_encoded_ushort_array,
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
    serialized_prefix: Vec<u8>,
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
    serialized_prefix: Vec<u8>,
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
}

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "NetManager+Data lane prefix".to_owned(),
        offset,
        reason: reason.into(),
    }
}
