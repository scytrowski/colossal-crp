use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    read_encoded_byte_array, read_encoded_uint_array, read_encoded_ushort_array,
};

const LANE_COUNT: usize = 262_144;
const NODE_COUNT: usize = 32_768;
const TWO_NODE_FLAGS_VERSION: u32 = 115_018;
const FULL_NODE_COUNT_VERSION: u32 = 202;

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
/// This is a partial manager body. Node and segment fields remain in the
/// opaque manager graph and are not claimed to be decoded here.
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
/// Later node problem arrays and individual records remain opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNodeFlagsPrefix {
    format_version: u32,
    node_flags: Vec<NetNodeFlag>,
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

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "NetManager+Data lane prefix".to_owned(),
        offset,
        reason: reason.into(),
    }
}
