use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::read_encoded_uint_array;
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

const ZONE_BLOCK_COUNT: usize = 49_152;
const ZONE_FLAGS_OLD_COUNT: usize = 16_383;
const ZONE_FLAGS_MID_COUNT: usize = 32_767;
const ZONE_FLAGS_MID_VERSION: u32 = 205;
const ZONE_FLAGS_FULL_VERSION: u32 = 222;
const ZONE_DEMAND_VERSION: u32 = 74;
const ZONE_ACTUAL_DEMAND_VERSION: u32 = 168;
const ZONE_GOOD_AREA_VERSION: u32 = 194;
const ZONE_OBJECTS_VERSION: u32 = 130;
const ZONE_OPTIONS_VERSION: u32 = 134;
const ZONE_BLOCKS_VERSION: u32 = 160;
const ZONE_DEMAND_COUNT: usize = 3;
const GOOD_AREA_COUNT: usize = 8;
const INITIAL_OBJECT_COUNT: usize = 11;

/// One active `ZoneBlock` record from `ZoneManager+Data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneBlockRecord {
    /// One-based buffer index selected by the serialized flags.
    pub index: u16,
    /// Serialized build index.
    pub build_index: u32,
    /// Position x, y, and z as their original IEEE-754 bit patterns.
    pub position_bits: [u32; 3],
    /// Angle as its original IEEE-754 bit pattern.
    pub angle_bits: u32,
    /// `m_valid` bitfield.
    pub valid: u64,
    /// `m_shared` bitfield.
    pub shared: u64,
    /// `m_occupied1` bitfield.
    pub occupied1: u64,
    /// `m_occupied2` bitfield.
    pub occupied2: u64,
    /// `m_zone1` bitfield.
    pub zone1: u64,
    /// `m_zone2` bitfield.
    pub zone2: u64,
    /// Serialized segment index.
    pub segment: u16,
}

/// Known fields from `ZoneManager+Data` through its queued object references.
/// The original bytes are retained because the profiler end tag follows this
/// prefix and is resolved through DataSerializer's shared-string table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneManagerDataPrefix {
    format_version: u32,
    serialized_flag_count: usize,
    flags: Vec<u32>,
    blocks: Vec<ZoneBlockRecord>,
    demand: [i8; ZONE_DEMAND_COUNT],
    actual_demand: [i8; ZONE_DEMAND_COUNT],
    good_area_found: [i16; GOOD_AREA_COUNT],
    object_references: Vec<NetObjectHeader>,
    serialized_prefix: Vec<u8>,
}

impl ZoneManagerDataPrefix {
    /// Decodes the known ZoneManager prefix for inner versions 164 and newer.
    /// Returns the byte length through its 13 queued object references, before
    /// the loading-profiler end tag. Truncated fields return an error.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(invalid_zone_manager(
                0,
                "ZoneManager prefix requires inner version 164 or newer",
            ));
        }
        let serialized_flag_count = if format_version >= ZONE_FLAGS_FULL_VERSION {
            ZONE_BLOCK_COUNT - 1
        } else if format_version >= ZONE_FLAGS_MID_VERSION {
            ZONE_FLAGS_MID_COUNT
        } else {
            ZONE_FLAGS_OLD_COUNT
        };
        let mut reader = Reader::new(bytes);
        let mut flags = read_encoded_uint_array(&mut reader, serialized_flag_count)?;
        flags.resize(ZONE_BLOCK_COUNT - 1, 0);

        let active_count = flags.iter().filter(|flag| **flag != 0).count();
        let mut blocks = Vec::with_capacity(active_count);
        for (index, flag) in flags.iter().enumerate() {
            if *flag == 0 {
                continue;
            }
            let block_index = u16::try_from(index + 1)
                .map_err(|_| CrpError::ArithmeticOverflow("ZoneBlock buffer index"))?;
            let build_index = reader.read_u32_be()?;
            let position_bits = [
                reader.read_u32_be()?,
                reader.read_u32_be()?,
                reader.read_u32_be()?,
            ];
            let angle_bits = reader.read_u32_be()?;
            let valid = reader.read_u64_be()?;
            let shared = reader.read_u64_be()?;
            let occupied1 = reader.read_u64_be()?;
            let occupied2 = reader.read_u64_be()?;
            let zone1 = reader.read_u64_be()?;
            let zone2 = reader.read_u64_be()?;
            let segment = reader.read_u16_be()?;
            blocks.push(ZoneBlockRecord {
                index: block_index,
                build_index,
                position_bits,
                angle_bits,
                valid,
                shared,
                occupied1,
                occupied2,
                zone1,
                zone2,
                segment,
            });
        }

        let demand = if format_version >= ZONE_DEMAND_VERSION {
            [
                reader.read_u8()? as i8,
                reader.read_u8()? as i8,
                reader.read_u8()? as i8,
            ]
        } else {
            [50; ZONE_DEMAND_COUNT]
        };
        let actual_demand = if format_version >= ZONE_ACTUAL_DEMAND_VERSION {
            [
                reader.read_u8()? as i8,
                reader.read_u8()? as i8,
                reader.read_u8()? as i8,
            ]
        } else {
            demand
        };
        let good_area_found = if format_version >= ZONE_GOOD_AREA_VERSION {
            let mut values = [0_i16; GOOD_AREA_COUNT];
            for value in &mut values {
                *value = reader.read_i16_be()?;
            }
            values
        } else {
            [0; GOOD_AREA_COUNT]
        };

        let mut object_references = Vec::with_capacity(INITIAL_OBJECT_COUNT + 2);
        if format_version >= ZONE_OBJECTS_VERSION {
            for _ in 0..INITIAL_OBJECT_COUNT {
                object_references.push(read_net_object_header(&mut reader)?);
            }
        } else {
            object_references.extend((0..INITIAL_OBJECT_COUNT).map(|_| NetObjectHeader::Null));
        }
        object_references.push(if format_version >= ZONE_OPTIONS_VERSION {
            read_net_object_header(&mut reader)?
        } else {
            NetObjectHeader::Null
        });
        object_references.push(if format_version >= ZONE_BLOCKS_VERSION {
            read_net_object_header(&mut reader)?
        } else {
            NetObjectHeader::Null
        });

        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                serialized_flag_count,
                flags,
                blocks,
                demand,
                actual_demand,
                good_area_found,
                object_references,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the number of flag values physically stored for this version.
    pub fn serialized_flag_count(&self) -> usize {
        self.serialized_flag_count
    }

    /// Returns all 49,151 one-based flags, zero-filling version-omitted slots.
    pub fn flags(&self) -> &[u32] {
        &self.flags
    }

    /// Returns direct records for blocks with nonzero flags.
    pub fn blocks(&self) -> &[ZoneBlockRecord] {
        &self.blocks
    }

    /// Returns residential, commercial, and workplace demand.
    pub fn demand(&self) -> [i8; ZONE_DEMAND_COUNT] {
        self.demand
    }

    /// Returns actual residential, commercial, and workplace demand.
    pub fn actual_demand(&self) -> [i8; ZONE_DEMAND_COUNT] {
        self.actual_demand
    }

    /// Returns the eight good-area values.
    pub fn good_area_found(&self) -> &[i16; GOOD_AREA_COUNT] {
        &self.good_area_found
    }

    /// Returns references in serialized order: 8 unused zones, 3 demand
    /// wrappers, options, then zones. New-type names and indices are retained.
    pub fn object_references(&self) -> &[NetObjectHeader] {
        &self.object_references
    }

    /// Writes the original known prefix exactly, including encoded array runs.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

fn invalid_zone_manager(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "ZoneManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::ZoneManagerDataPrefix;
    use crate::encoded_array::write_encoded_uint_array;
    use crate::net_manager_data::NetObjectHeader;

    fn zero_flags(format_version: u32) -> Vec<u8> {
        let count = if format_version >= 222 {
            49_151
        } else if format_version >= 205 {
            32_767
        } else {
            16_383
        };
        let mut bytes = Vec::new();
        write_encoded_uint_array(&mut bytes, &vec![0; count]);
        bytes
    }

    fn null_references(bytes: &mut Vec<u8>, format_version: u32) {
        let count = if format_version >= 134 {
            13
        } else if format_version >= 130 {
            11
        } else {
            0
        };
        bytes.extend(vec![0xff; count]);
    }

    #[test]
    fn zone_manager_decodes_versioned_flag_counts_and_demand_gates() {
        for (version, flag_count) in [(164, 16_383), (205, 32_767), (222, 49_151)] {
            let mut bytes = zero_flags(version);
            if version >= 74 {
                bytes.extend_from_slice(&[0xff, 2, 3]);
            }
            if version >= 168 {
                bytes.extend_from_slice(&[4, 5, 6]);
            }
            if version >= 194 {
                for value in [-4_i16, -3, -2, -1, 0, 1, 2, 3] {
                    bytes.extend_from_slice(&value.to_be_bytes());
                }
            }
            null_references(&mut bytes, version);

            let (zone, used) = ZoneManagerDataPrefix::decode_prefix(version, &bytes)
                .expect("ZoneManager prefix decodes");
            assert_eq!(zone.serialized_flag_count(), flag_count);
            assert_eq!(zone.flags().len(), 49_151);
            assert!(zone.flags().iter().all(|flag| *flag == 0));
            assert!(zone.blocks().is_empty());
            assert_eq!(zone.demand(), [-1, 2, 3]);
            assert_eq!(
                zone.actual_demand(),
                if version >= 168 {
                    [4, 5, 6]
                } else {
                    [-1, 2, 3]
                }
            );
            assert_eq!(
                zone.good_area_found(),
                if version >= 194 {
                    &[-4, -3, -2, -1, 0, 1, 2, 3]
                } else {
                    &[0; 8]
                }
            );
            assert_eq!(used, bytes.len());
            assert_eq!(zone.encode(), bytes);
            assert_eq!(zone.object_references().len(), 13);
        }
    }

    #[test]
    fn zone_manager_reads_active_records_in_flag_order_and_rejects_truncation() {
        let mut flags = vec![0; 16_383];
        flags[0] = 1;
        flags[2] = 7;
        let mut bytes = Vec::new();
        write_encoded_uint_array(&mut bytes, &flags);
        for block in [0_u32, 1] {
            bytes.extend_from_slice(&(0x1000 + block).to_be_bytes());
            for bits in [0x3f80_0000_u32 + block, 0xc020_0000, 0x4040_0000] {
                bytes.extend_from_slice(&bits.to_be_bytes());
            }
            bytes.extend_from_slice(&0x3f00_0000_u32.to_be_bytes());
            for value in 1_u64..=6 {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend_from_slice(&(0x20 + block as u16).to_be_bytes());
        }
        bytes.extend_from_slice(&[0xff, 2, 3]);
        null_references(&mut bytes, 164);

        let (zone, used) = ZoneManagerDataPrefix::decode_prefix(164, &bytes)
            .expect("active ZoneBlock records decode");
        assert_eq!(used, bytes.len());
        assert_eq!(zone.flags()[0], 1);
        assert_eq!(zone.flags()[2], 7);
        assert_eq!(zone.blocks().len(), 2);
        assert_eq!(zone.blocks()[0].index, 1);
        assert_eq!(zone.blocks()[1].index, 3);
        assert_eq!(zone.blocks()[0].build_index, 0x1000);
        assert_eq!(zone.blocks()[1].position_bits[0], 0x3f80_0001);
        assert_eq!(zone.blocks()[1].zone2, 6);
        assert_eq!(zone.blocks()[1].segment, 0x21);
        assert!(
            zone.object_references()
                .iter()
                .all(|header| *header == NetObjectHeader::Null)
        );
        assert!(ZoneManagerDataPrefix::decode_prefix(164, &bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn zone_manager_rejects_unsupported_and_truncated_input() {
        assert!(ZoneManagerDataPrefix::decode_prefix(163, &[]).is_err());
        assert!(ZoneManagerDataPrefix::decode_prefix(164, &[]).is_err());
    }
}
