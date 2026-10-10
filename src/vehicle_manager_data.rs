use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    EncodedUIntArrayReader, read_encoded_byte_array, read_encoded_ushort_array,
};

const VEHICLE_FLAG_COUNT: usize = 16_383;
const PARKED_FLAG_COUNT_LEGACY: usize = 16_383;
const PARKED_FLAG_COUNT_FULL: usize = 32_767;
const PREFAB_COLLECTION_VERSION: u32 = 30;
const PARKED_VEHICLES_VERSION: u32 = 115;
const PARKED_FLAGS_FULL_VERSION: u32 = 205;
const SECOND_VEHICLE_FLAGS_VERSION: u32 = 274;
const VEHICLE_GATE_INDEX_VERSION: u32 = 182;
const VEHICLE_RACE_TEAM_VERSION: u32 = 121_008;
const VEHICLE_RACE_COLUMNS_VERSION: u32 = 121_012;
const VEHICLE_WATER_SOURCE_VERSION: u32 = 273;
const VEHICLE_CITIZEN_UNIT_VERSION: u32 = 32;
const VEHICLE_PATH_VERSION: u32 = 47;
const VEHICLE_TRANSPORT_LINE_VERSION: u32 = 52;
const VEHICLE_TRAILING_VEHICLE_VERSION: u32 = 58;
const VEHICLE_BLOCK_COUNTER_VERSION: u32 = 99;
const VEHICLE_CARGO_PARENT_VERSION: u32 = 104;
const VEHICLE_CUSTOM_VALUE_VERSION: u32 = 113_012;

/// The serialized flag and prefab-table prefix of `VehicleManager+Data`.
///
/// This decoder stops before the active-vehicle sparse columns. It preserves
/// the exact consumed bytes, including fields that are only exposed as raw
/// flag values. It does not claim to decode or rewrite active vehicle records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleManagerDataPrefix {
    format_version: u32,
    vehicle_flags: Vec<u32>,
    vehicle_flags2: Option<Vec<u32>>,
    parked_vehicle_flags: Option<Vec<u16>>,
    vehicle_prefab_indices: Option<Vec<u16>>,
    parked_vehicle_prefab_indices: Option<Vec<u16>>,
    prefab_names: Option<Vec<Option<String>>>,
    serialized_prefix: Vec<u8>,
}

impl VehicleManagerDataPrefix {
    /// Decodes the known flag and prefab-table fields at the start of the
    /// manager body and returns the consumed byte count before sparse columns.
    ///
    /// Inner version 274 adds the second vehicle flag array; version 115 adds
    /// parked-vehicle flags and prefab indices; version 205 expands the
    /// serialized parked-flag range; version 30 adds the prefab collection.
    /// Truncated or malformed fields return an error.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let mut vehicle_flag_scope = EncodedUIntArrayReader::default();
        let vehicle_flags = vehicle_flag_scope.read_array(&mut reader, VEHICLE_FLAG_COUNT)?;
        let vehicle_flags2 = if format_version >= SECOND_VEHICLE_FLAGS_VERSION {
            Some(vehicle_flag_scope.read_array(&mut reader, VEHICLE_FLAG_COUNT)?)
        } else {
            None
        };

        let parked_vehicle_flags = if format_version >= PARKED_VEHICLES_VERSION {
            let count = if format_version >= PARKED_FLAGS_FULL_VERSION {
                PARKED_FLAG_COUNT_FULL
            } else {
                PARKED_FLAG_COUNT_LEGACY
            };
            Some(read_encoded_ushort_array(&mut reader, count)?)
        } else {
            None
        };

        let (vehicle_prefab_indices, parked_vehicle_prefab_indices, prefab_names) =
            if format_version >= PREFAB_COLLECTION_VERSION {
                let active_vehicle_count =
                    vehicle_flags.iter().filter(|flags| **flags != 0).count();
                let active_parked_count = parked_vehicle_flags
                    .as_deref()
                    .map(|flags| flags.iter().filter(|flags| **flags != 0).count())
                    .unwrap_or(0);
                let total_count = active_vehicle_count
                    .checked_add(active_parked_count)
                    .ok_or(CrpError::ArithmeticOverflow(
                        "VehicleManager prefab index count",
                    ))?;
                let indices = read_encoded_ushort_array(&mut reader, total_count)?;
                let (vehicle_indices, parked_indices) = indices.split_at(active_vehicle_count);
                let names = read_prefab_names(&mut reader)?;
                (
                    Some(vehicle_indices.to_vec()),
                    Some(parked_indices.to_vec()),
                    Some(names),
                )
            } else {
                (None, None, None)
            };

        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                vehicle_flags,
                vehicle_flags2,
                parked_vehicle_flags,
                vehicle_prefab_indices,
                parked_vehicle_prefab_indices,
                prefab_names,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns all serialized vehicle flags for one-based vehicle slots.
    pub fn vehicle_flags(&self) -> &[u32] {
        &self.vehicle_flags
    }

    /// Returns the version-274-and-newer second vehicle flag array.
    pub fn vehicle_flags2(&self) -> Option<&[u32]> {
        self.vehicle_flags2.as_deref()
    }

    /// Returns serialized parked-vehicle flags, absent before version 115.
    pub fn parked_vehicle_flags(&self) -> Option<&[u16]> {
        self.parked_vehicle_flags.as_deref()
    }

    /// Returns prefab indices in active-vehicle buffer order, absent before version 30.
    pub fn vehicle_prefab_indices(&self) -> Option<&[u16]> {
        self.vehicle_prefab_indices.as_deref()
    }

    /// Returns prefab indices in active parked-vehicle buffer order from version 115.
    pub fn parked_vehicle_prefab_indices(&self) -> Option<&[u16]> {
        self.parked_vehicle_prefab_indices.as_deref()
    }

    /// Returns the prefab-name table, preserving null names, from version 30.
    pub fn prefab_names(&self) -> Option<&[Option<String>]> {
        self.prefab_names.as_deref()
    }

    /// Writes the original known prefix exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Decodes the sparse active-vehicle columns that follow this prefix.
    ///
    /// The returned byte count ends before the per-vehicle records. Fields are
    /// present at their game-reader version gates, even when there are no
    /// active vehicles. Truncated columns return an error.
    pub fn decode_sparse_columns(
        &self,
        bytes: &[u8],
    ) -> Result<(VehicleManagerSparseFieldsPrefix, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let active_count = self
            .vehicle_flags
            .iter()
            .filter(|flags| **flags != 0)
            .count();

        let gate_indices = if self.format_version >= VEHICLE_GATE_INDEX_VERSION {
            Some(read_encoded_byte_array(&mut reader, active_count)?)
        } else {
            None
        };
        let race_teams = if self.format_version >= VEHICLE_RACE_TEAM_VERSION {
            Some(read_encoded_ushort_array(&mut reader, active_count)?)
        } else {
            None
        };
        let race_teammates = if self.format_version >= VEHICLE_RACE_COLUMNS_VERSION {
            Some(read_encoded_byte_array(&mut reader, active_count)?)
        } else {
            None
        };
        let racer_indices = if self.format_version >= VEHICLE_RACE_COLUMNS_VERSION {
            Some(read_encoded_byte_array(&mut reader, active_count)?)
        } else {
            None
        };
        let water_sources = if self.format_version >= VEHICLE_WATER_SOURCE_VERSION {
            Some(read_encoded_ushort_array(&mut reader, active_count)?)
        } else {
            None
        };

        let consumed = reader.offset();
        Ok((
            VehicleManagerSparseFieldsPrefix {
                format_version: self.format_version,
                active_vehicle_count: active_count,
                gate_indices,
                race_teams,
                race_teammates,
                racer_indices,
                water_sources,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }
}

/// Known sparse active-vehicle columns between the prefab table and vehicle records.
///
/// The original column bytes are retained. A separate method decodes the
/// versioned last-frame vectors; later scalar and reference fields stay opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleManagerSparseFieldsPrefix {
    format_version: u32,
    active_vehicle_count: usize,
    gate_indices: Option<Vec<u8>>,
    race_teams: Option<Vec<u16>>,
    race_teammates: Option<Vec<u8>>,
    racer_indices: Option<Vec<u8>>,
    water_sources: Option<Vec<u16>>,
    serialized_prefix: Vec<u8>,
}

impl VehicleManagerSparseFieldsPrefix {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the number of active vehicle records selected by `m_flags`.
    pub fn active_vehicle_count(&self) -> usize {
        self.active_vehicle_count
    }

    /// Returns gate indices from inner version 182.
    pub fn gate_indices(&self) -> Option<&[u8]> {
        self.gate_indices.as_deref()
    }

    /// Returns race-team indices from inner version 121008.
    pub fn race_teams(&self) -> Option<&[u16]> {
        self.race_teams.as_deref()
    }

    /// Returns race-teammate columns from inner version 121012.
    pub fn race_teammates(&self) -> Option<&[u8]> {
        self.race_teammates.as_deref()
    }

    /// Returns racer indices from inner version 121012.
    pub fn racer_indices(&self) -> Option<&[u8]> {
        self.racer_indices.as_deref()
    }

    /// Returns water-source indices from inner version 273.
    pub fn water_sources(&self) -> Option<&[u16]> {
        self.water_sources.as_deref()
    }

    /// Writes the original known sparse-column bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Decodes the last-frame vectors and target positions for each active vehicle.
    ///
    /// Returns the consumed byte count before the remaining per-vehicle scalar
    /// and reference fields. The decoded vectors retain their IEEE-754 bit
    /// patterns, and the original bytes are preserved for exact rewriting.
    pub fn decode_last_frame_records(
        &self,
        bytes: &[u8],
    ) -> Result<(VehicleLastFrameDataPrefix, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let mut records = Vec::with_capacity(self.active_vehicle_count);
        for _ in 0..self.active_vehicle_count {
            let velocity = if self.format_version >= 47 {
                Some(read_vector::<3>(&mut reader)?)
            } else {
                None
            };
            let position = read_vector::<3>(&mut reader)?;
            let rotation = if self.format_version >= 78 {
                Some(read_vector::<4>(&mut reader)?)
            } else {
                None
            };
            let angle_velocity = if self.format_version >= 129 {
                Some(read_vector::<3>(&mut reader)?)
            } else {
                None
            };
            let target_position0 = if self.format_version >= 68 {
                VehicleTargetPosition0::Vector4(read_vector::<4>(&mut reader)?)
            } else if self.format_version >= 47 {
                VehicleTargetPosition0::Vector3(read_vector::<3>(&mut reader)?)
            } else {
                VehicleTargetPosition0::DerivedFromPosition
            };
            let target_positions1_to_3 = if self.format_version >= 90 {
                Some([
                    read_vector::<4>(&mut reader)?,
                    read_vector::<4>(&mut reader)?,
                    read_vector::<4>(&mut reader)?,
                ])
            } else {
                None
            };
            records.push(VehicleLastFrameRecord {
                velocity,
                position,
                rotation,
                angle_velocity,
                target_position0,
                target_positions1_to_3,
            });
        }

        let consumed = reader.offset();
        Ok((
            VehicleLastFrameDataPrefix {
                format_version: self.format_version,
                records,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }
}

/// The serialized representation of `m_targetPos0` for one vehicle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleTargetPosition0 {
    /// Before version 47, the game derives this value from the vehicle position.
    DerivedFromPosition,
    /// Versions 47–67 store three float components.
    Vector3([u32; 3]),
    /// Version 68 and newer store four float components.
    Vector4([u32; 4]),
}

/// Last-frame vectors and target positions for one active vehicle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleLastFrameRecord {
    velocity: Option<[u32; 3]>,
    position: [u32; 3],
    rotation: Option<[u32; 4]>,
    angle_velocity: Option<[u32; 3]>,
    target_position0: VehicleTargetPosition0,
    target_positions1_to_3: Option<[[u32; 4]; 3]>,
}

impl VehicleLastFrameRecord {
    /// Returns velocity component bit patterns from version 47.
    pub fn velocity(&self) -> Option<[u32; 3]> {
        self.velocity
    }

    /// Returns the position component bit patterns.
    pub fn position(&self) -> [u32; 3] {
        self.position
    }

    /// Returns rotation component bit patterns from version 78.
    pub fn rotation(&self) -> Option<[u32; 4]> {
        self.rotation
    }

    /// Returns angle-velocity component bit patterns from version 129.
    pub fn angle_velocity(&self) -> Option<[u32; 3]> {
        self.angle_velocity
    }

    /// Returns the versioned first target position.
    pub fn target_position0(&self) -> VehicleTargetPosition0 {
        self.target_position0
    }

    /// Returns serialized target positions 1–3 from version 90.
    ///
    /// Before version 90, the game copies target position 0 into these slots.
    pub fn target_positions1_to_3(&self) -> Option<[[u32; 4]; 3]> {
        self.target_positions1_to_3
    }
}

/// Decoded last-frame records at the start of active vehicle data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleLastFrameDataPrefix {
    format_version: u32,
    records: Vec<VehicleLastFrameRecord>,
    serialized_prefix: Vec<u8>,
}

impl VehicleLastFrameDataPrefix {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns one last-frame record per active vehicle, in flag order.
    pub fn records(&self) -> &[VehicleLastFrameRecord] {
        &self.records
    }

    /// Writes the original last-frame bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Decodes the remaining scalar and reference fields of each active vehicle.
    ///
    /// Returns the consumed byte count before parked-vehicle records or later
    /// manager counters. Version-gated fields absent from old saves are exposed
    /// as `None`; truncated records return an error.
    pub fn decode_scalar_fields(
        &self,
        bytes: &[u8],
    ) -> Result<(VehicleInstanceFieldsPrefix, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let mut records = Vec::with_capacity(self.records.len());
        for _ in &self.records {
            let source_building = reader.read_u16_be()?;
            let target_building = reader.read_u16_be()?;
            let transport_line = if self.format_version >= VEHICLE_TRANSPORT_LINE_VERSION {
                Some(reader.read_u16_be()?)
            } else {
                None
            };
            let transfer_size = reader.read_u16_be()?;
            let transfer_type = reader.read_u8()?;
            let wait_counter = reader.read_u8()?;
            let block_counter = if self.format_version >= VEHICLE_BLOCK_COUNTER_VERSION {
                Some(reader.read_u8()?)
            } else {
                None
            };
            let citizen_unit = if self.format_version >= VEHICLE_CITIZEN_UNIT_VERSION {
                Some(reader.read_u24_be()?)
            } else {
                None
            };
            let (path_unit, path_position) = if self.format_version >= VEHICLE_PATH_VERSION {
                (
                    Some(reader.read_u16_be()?),
                    Some([reader.read_u8()?, reader.read_u8()?]),
                )
            } else {
                (None, None)
            };
            let trailing_vehicle = if self.format_version >= VEHICLE_TRAILING_VEHICLE_VERSION {
                Some(reader.read_u16_be()?)
            } else {
                None
            };
            let cargo_parent = if self.format_version >= VEHICLE_CARGO_PARENT_VERSION {
                Some(reader.read_u16_be()?)
            } else {
                None
            };
            let custom_value = if self.format_version >= VEHICLE_CUSTOM_VALUE_VERSION {
                Some(reader.read_u16_be()?)
            } else {
                None
            };
            records.push(VehicleInstanceFields {
                source_building,
                target_building,
                transport_line,
                transfer_size,
                transfer_type,
                wait_counter,
                block_counter,
                citizen_unit,
                path_unit,
                path_position,
                trailing_vehicle,
                cargo_parent,
                custom_value,
            });
        }

        let consumed = reader.offset();
        Ok((
            VehicleInstanceFieldsPrefix {
                format_version: self.format_version,
                records,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }
}

/// Remaining scalar and reference values from one active vehicle record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleInstanceFields {
    source_building: u16,
    target_building: u16,
    transport_line: Option<u16>,
    transfer_size: u16,
    transfer_type: u8,
    wait_counter: u8,
    block_counter: Option<u8>,
    citizen_unit: Option<u32>,
    path_unit: Option<u16>,
    path_position: Option<[u8; 2]>,
    trailing_vehicle: Option<u16>,
    cargo_parent: Option<u16>,
    custom_value: Option<u16>,
}

impl VehicleInstanceFields {
    /// Returns the source-building index.
    pub fn source_building(&self) -> u16 {
        self.source_building
    }

    /// Returns the target-building index.
    pub fn target_building(&self) -> u16 {
        self.target_building
    }

    /// Returns the transport-line index from version 52.
    pub fn transport_line(&self) -> Option<u16> {
        self.transport_line
    }

    /// Returns the transfer size.
    pub fn transfer_size(&self) -> u16 {
        self.transfer_size
    }

    /// Returns the transfer type byte.
    pub fn transfer_type(&self) -> u8 {
        self.transfer_type
    }

    /// Returns the wait counter byte.
    pub fn wait_counter(&self) -> u8 {
        self.wait_counter
    }

    /// Returns the block counter from version 99.
    pub fn block_counter(&self) -> Option<u8> {
        self.block_counter
    }

    /// Returns the 24-bit citizen-unit reference from version 32.
    pub fn citizen_unit(&self) -> Option<u32> {
        self.citizen_unit
    }

    /// Returns the path-unit reference from version 47.
    pub fn path_unit(&self) -> Option<u16> {
        self.path_unit
    }

    /// Returns the two path-position bytes from version 47.
    pub fn path_position(&self) -> Option<[u8; 2]> {
        self.path_position
    }

    /// Returns the trailing-vehicle index from version 58.
    pub fn trailing_vehicle(&self) -> Option<u16> {
        self.trailing_vehicle
    }

    /// Returns the cargo-parent index from version 104.
    pub fn cargo_parent(&self) -> Option<u16> {
        self.cargo_parent
    }

    /// Returns the custom value from version 113012.
    pub fn custom_value(&self) -> Option<u16> {
        self.custom_value
    }
}

/// Decoded remaining fields for the active VehicleManager records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleInstanceFieldsPrefix {
    format_version: u32,
    records: Vec<VehicleInstanceFields>,
    serialized_prefix: Vec<u8>,
}

impl VehicleInstanceFieldsPrefix {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns one field record per active vehicle, in flag order.
    pub fn records(&self) -> &[VehicleInstanceFields] {
        &self.records
    }

    /// Writes the original scalar/reference bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

fn read_vector<const N: usize>(reader: &mut Reader<'_>) -> Result<[u32; N], CrpError> {
    let mut values = [0_u32; N];
    for value in &mut values {
        *value = reader.read_u32_be()?;
    }
    Ok(values)
}

fn read_prefab_names(reader: &mut Reader<'_>) -> Result<Vec<Option<String>>, CrpError> {
    let count = usize::from(reader.read_u16_be()?);
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        names.push(reader.read_unique_string()?);
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::{
        VehicleLastFrameDataPrefix, VehicleLastFrameRecord, VehicleManagerDataPrefix,
        VehicleManagerSparseFieldsPrefix, VehicleTargetPosition0,
    };

    fn append_zero_uint_array(bytes: &mut Vec<u8>, count: usize) {
        for _ in 0..count / 128 {
            bytes.push(0xff);
        }
        let remainder = count % 128;
        if remainder != 0 {
            bytes.push((0x7f + remainder) as u8);
        }
    }

    fn append_zero_ushort_array(bytes: &mut Vec<u8>, count: usize) {
        for _ in 0..count / 128 {
            bytes.push(0xff);
        }
        let remainder = count % 128;
        if remainder != 0 {
            bytes.push((0x7f + remainder) as u8);
        }
    }

    fn empty_prefix(version: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        append_zero_uint_array(&mut bytes, 16_383);
        if version >= 274 {
            append_zero_uint_array(&mut bytes, 16_383);
        }
        if version >= 115 {
            append_zero_ushort_array(&mut bytes, if version >= 205 { 32_767 } else { 16_383 });
        }
        if version >= 30 {
            bytes.extend_from_slice(&[0, 0]);
        }
        bytes
    }

    fn prefix_with_active_slots(
        format_version: u32,
        active_count: usize,
    ) -> VehicleManagerDataPrefix {
        let mut flags = vec![0; 16_383];
        flags[..active_count].fill(1);
        VehicleManagerDataPrefix {
            format_version,
            vehicle_flags: flags,
            vehicle_flags2: None,
            parked_vehicle_flags: None,
            vehicle_prefab_indices: None,
            parked_vehicle_prefab_indices: None,
            prefab_names: None,
            serialized_prefix: Vec::new(),
        }
    }

    fn sparse_fields_with_count(
        format_version: u32,
        active_vehicle_count: usize,
    ) -> VehicleManagerSparseFieldsPrefix {
        VehicleManagerSparseFieldsPrefix {
            format_version,
            active_vehicle_count,
            gate_indices: None,
            race_teams: None,
            race_teammates: None,
            racer_indices: None,
            water_sources: None,
            serialized_prefix: Vec::new(),
        }
    }

    fn last_frame_prefix_with_count(
        format_version: u32,
        record_count: usize,
    ) -> VehicleLastFrameDataPrefix {
        let empty_record = VehicleLastFrameRecord {
            velocity: None,
            position: [0; 3],
            rotation: None,
            angle_velocity: None,
            target_position0: VehicleTargetPosition0::DerivedFromPosition,
            target_positions1_to_3: None,
        };
        VehicleLastFrameDataPrefix {
            format_version,
            records: std::iter::repeat_n(empty_record, record_count).collect(),
            serialized_prefix: Vec::new(),
        }
    }

    fn words_as_bytes(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_be_bytes()).collect()
    }

    fn scalar_record_for_version(version: u32) -> Vec<u8> {
        let mut bytes = vec![0x01, 0x02, 0x03, 0x04]; // source and target buildings
        if version >= 52 {
            bytes.extend_from_slice(&[0x05, 0x06]); // transport line
        }
        bytes.extend_from_slice(&[0x07, 0x08, 0x09, 0x0a]); // transfer size, type, wait
        if version >= 99 {
            bytes.push(0x0b); // block counter
        }
        if version >= 32 {
            bytes.extend_from_slice(&[0x0c, 0x0d, 0x0e]); // citizen unit
        }
        if version >= 47 {
            bytes.extend_from_slice(&[0x0f, 0x10, 0x11, 0x12]); // path unit and positions
        }
        if version >= 58 {
            bytes.extend_from_slice(&[0x13, 0x14]); // trailing vehicle
        }
        if version >= 104 {
            bytes.extend_from_slice(&[0x15, 0x16]); // cargo parent
        }
        if version >= 113_012 {
            bytes.extend_from_slice(&[0x17, 0x18]); // custom value
        }
        bytes
    }

    #[test]
    fn follows_vehicle_manager_flag_and_prefab_version_gates() {
        for (version, has_flags2, parked_count, has_prefabs) in [
            (29, false, None, false),
            (30, false, None, true),
            (114, false, None, true),
            (115, false, Some(16_383), true),
            (204, false, Some(16_383), true),
            (205, false, Some(32_767), true),
            (273, false, Some(32_767), true),
            (274, true, Some(32_767), true),
        ] {
            let bytes = empty_prefix(version);
            let (decoded, consumed) = VehicleManagerDataPrefix::decode_prefix(version, &bytes)
                .expect("versioned empty prefix is valid");
            assert_eq!(consumed, bytes.len(), "version {version}");
            assert_eq!(decoded.encode(), bytes, "version {version}");
            assert_eq!(decoded.vehicle_flags().len(), 16_383);
            assert_eq!(decoded.vehicle_flags2().is_some(), has_flags2);
            assert_eq!(
                decoded.parked_vehicle_flags().map(<[u16]>::len),
                parked_count,
                "version {version}"
            );
            assert_eq!(decoded.prefab_names().is_some(), has_prefabs);
        }
    }

    #[test]
    fn decodes_active_vehicle_and_parked_prefab_assignments() {
        let mut bytes = Vec::new();

        // Vehicle flags have one active slot at the beginning.
        bytes.extend_from_slice(&[0, 0, 0, 0, 1]);
        bytes.extend_from_slice(&[0x40, 0xff, 0xfe]); // next value is zero
        bytes.extend_from_slice(&[0x40, 0, 1]); // reset delta, value remains zero
        append_zero_uint_array(&mut bytes, 16_380);

        // One active parked slot, then zero flags through the v205 range.
        bytes.extend_from_slice(&[0, 0, 1]);
        bytes.extend_from_slice(&[0x40, 0xfe]);
        bytes.extend_from_slice(&[0x40, 1]);
        append_zero_ushort_array(&mut bytes, 32_764);

        // Two prefab indices: vehicle index 1 and parked vehicle index 0.
        bytes.extend_from_slice(&[1, 0, 1, 0, 0]);
        bytes.extend_from_slice(&[0, 2]); // two names
        bytes.push(0xff); // null name
        bytes.extend_from_slice(&[0x80, 0, 1, 0xc1]); // compact "A"

        let (decoded, consumed) = VehicleManagerDataPrefix::decode_prefix(205, &bytes)
            .expect("valid vehicle prefab prefix");
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.vehicle_flags()[0], 1);
        assert_eq!(
            decoded
                .vehicle_flags()
                .iter()
                .filter(|flags| **flags != 0)
                .count(),
            1
        );
        assert_eq!(
            decoded
                .parked_vehicle_flags()
                .unwrap()
                .iter()
                .filter(|flags| **flags != 0)
                .count(),
            1
        );
        assert_eq!(decoded.vehicle_prefab_indices(), Some([1].as_slice()));
        assert_eq!(
            decoded.parked_vehicle_prefab_indices(),
            Some([0].as_slice())
        );
        assert_eq!(
            decoded.prefab_names(),
            Some([None, Some("A".to_owned())].as_slice())
        );
        assert_eq!(decoded.encode(), bytes);
    }

    #[test]
    fn carries_uint_delta_state_between_vehicle_flag_arrays() {
        let mut bytes = Vec::new();
        append_zero_uint_array(&mut bytes, 16_382);
        bytes.extend_from_slice(&[0, 0, 0, 0, 1]); // last m_flags value is one
        bytes.push(0x80); // repeats the prior delta, producing two in m_flags2
        let mut remaining = 16_382;
        while remaining > 0 {
            let count = remaining.min(64);
            bytes.push((count - 1) as u8);
            bytes.extend(std::iter::repeat_n(0_u8, count * 4));
            remaining -= count;
        }
        append_zero_ushort_array(&mut bytes, 32_767);
        bytes.push(0x80); // one active vehicle's prefab index is zero
        bytes.extend_from_slice(&[0, 0]); // empty prefab-name table

        let (decoded, _) =
            VehicleManagerDataPrefix::decode_prefix(274, &bytes).expect("valid v274 flags");
        assert_eq!(decoded.vehicle_flags()[16_382], 1);
        assert_eq!(decoded.vehicle_flags2().unwrap()[0], 2);
    }

    #[test]
    fn rejects_truncated_vehicle_flag_parked_flag_and_name_fields() {
        assert!(VehicleManagerDataPrefix::decode_prefix(30, &[]).is_err());

        let mut before_parked_flags = Vec::new();
        append_zero_uint_array(&mut before_parked_flags, 16_383);
        assert!(VehicleManagerDataPrefix::decode_prefix(115, &before_parked_flags).is_err());

        let mut before_names = Vec::new();
        append_zero_uint_array(&mut before_names, 16_383);
        before_names.extend_from_slice(&[0, 2]);
        assert!(VehicleManagerDataPrefix::decode_prefix(30, &before_names).is_err());
    }

    #[test]
    fn sparse_vehicle_columns_follow_independent_version_gates() {
        let legacy = prefix_with_active_slots(181, 2);
        let (decoded, consumed) = legacy.decode_sparse_columns(&[0xaa]).unwrap();
        assert_eq!(consumed, 0);
        assert_eq!(decoded.gate_indices(), None);
        assert_eq!(decoded.water_sources(), None);

        let version_273 = prefix_with_active_slots(273, 2);
        let bytes = [1, 7, 8, 1, 0, 9, 0, 10];
        let (decoded, consumed) = version_273.decode_sparse_columns(&bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.gate_indices(), Some([7, 8].as_slice()));
        assert_eq!(decoded.race_teams(), None);
        assert_eq!(decoded.race_teammates(), None);
        assert_eq!(decoded.racer_indices(), None);
        assert_eq!(decoded.water_sources(), Some([9, 10].as_slice()));
        assert_eq!(decoded.encode(), bytes);

        let latest = prefix_with_active_slots(121_012, 2);
        let bytes = [
            1, 7, 8, // gate indices
            1, 0, 2, 0, 3, // race teams
            1, 4, 5, // race teammates
            1, 6, 7, // racer indices
            1, 0, 8, 0, 9, // water sources
        ];
        let (decoded, consumed) = latest.decode_sparse_columns(&bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.race_teams(), Some([2, 3].as_slice()));
        assert_eq!(decoded.race_teammates(), Some([4, 5].as_slice()));
        assert_eq!(decoded.racer_indices(), Some([6, 7].as_slice()));
        assert_eq!(decoded.water_sources(), Some([8, 9].as_slice()));
        assert_eq!(decoded.encode(), bytes);
    }

    #[test]
    fn sparse_vehicle_columns_reject_truncated_arrays() {
        let version_182 = prefix_with_active_slots(182, 2);
        assert!(version_182.decode_sparse_columns(&[1, 7]).is_err());

        let latest = prefix_with_active_slots(121_012, 2);
        let truncated_race_teammate = [1, 7, 8, 1, 0, 2, 0, 3, 1, 4];
        assert!(
            latest
                .decode_sparse_columns(&truncated_race_teammate)
                .is_err()
        );
    }

    #[test]
    fn last_frame_records_follow_version_gates_and_preserve_float_bits() {
        for (version, word_count, has_velocity, has_rotation, has_angle, targets_1_to_3) in [
            (46, 3, false, false, false, false),
            (47, 9, true, false, false, false),
            (67, 9, true, false, false, false),
            (68, 10, true, false, false, false),
            (77, 10, true, false, false, false),
            (78, 14, true, true, false, false),
            (89, 14, true, true, false, false),
            (90, 26, true, true, false, true),
            (128, 26, true, true, false, true),
            (129, 29, true, true, true, true),
        ] {
            let fields = sparse_fields_with_count(version, 1);
            let expected_words: Vec<u32> = (1..=word_count as u32).collect();
            let bytes = words_as_bytes(&expected_words);
            let (decoded, consumed) = fields
                .decode_last_frame_records(&bytes)
                .expect("versioned last-frame record is complete");
            assert_eq!(consumed, bytes.len(), "version {version}");
            assert_eq!(decoded.encode(), bytes, "version {version}");
            assert_eq!(decoded.records().len(), 1, "version {version}");
            let record = &decoded.records()[0];
            assert_eq!(
                record.velocity().is_some(),
                has_velocity,
                "version {version}"
            );
            assert_eq!(
                record.rotation().is_some(),
                has_rotation,
                "version {version}"
            );
            assert_eq!(
                record.angle_velocity().is_some(),
                has_angle,
                "version {version}"
            );
            assert_eq!(
                record.target_positions1_to_3().is_some(),
                targets_1_to_3,
                "version {version}"
            );
            match (version, record.target_position0()) {
                (..=46, VehicleTargetPosition0::DerivedFromPosition) => {}
                (47..=67, VehicleTargetPosition0::Vector3(_)) => {}
                (68.., VehicleTargetPosition0::Vector4(_)) => {}
                _ => panic!("target position 0 does not match version {version}"),
            }
        }

        let fields = sparse_fields_with_count(129, 2);
        let words: Vec<u32> = (1..=58).collect();
        let bytes = words_as_bytes(&words);
        let (decoded, consumed) = fields
            .decode_last_frame_records(&bytes)
            .expect("two current last-frame records are complete");
        assert_eq!(consumed, 2 * 29 * 4);
        assert_eq!(decoded.records().len(), 2);
        assert_eq!(decoded.records()[0].velocity(), Some([1, 2, 3]));
        assert_eq!(decoded.records()[0].position(), [4, 5, 6]);
        assert_eq!(decoded.records()[0].rotation(), Some([7, 8, 9, 10]));
        assert_eq!(decoded.records()[0].angle_velocity(), Some([11, 12, 13]));
        assert_eq!(
            decoded.records()[0].target_position0(),
            VehicleTargetPosition0::Vector4([14, 15, 16, 17])
        );
        assert_eq!(
            decoded.records()[0].target_positions1_to_3(),
            Some([[18, 19, 20, 21], [22, 23, 24, 25], [26, 27, 28, 29]])
        );
        assert_eq!(decoded.records()[1].velocity(), Some([30, 31, 32]));
        assert_eq!(decoded.records()[1].position(), [33, 34, 35]);
        assert_eq!(decoded.encode(), bytes);
    }

    #[test]
    fn last_frame_records_reject_truncation_and_allow_no_active_vehicles() {
        let fields = sparse_fields_with_count(129, 1);
        assert!(fields.decode_last_frame_records(&[0; 115]).is_err());

        let empty = sparse_fields_with_count(129, 0);
        let (decoded, consumed) = empty.decode_last_frame_records(&[0xaa, 0xbb]).unwrap();
        assert_eq!(decoded.records(), []);
        assert_eq!(consumed, 0);
        assert!(decoded.encode().is_empty());
    }

    #[test]
    fn vehicle_scalar_fields_follow_version_gates_and_big_endian_order() {
        for (version, byte_count) in [
            (31, 8),
            (32, 11),
            (47, 15),
            (52, 17),
            (58, 19),
            (99, 20),
            (104, 22),
            (113_012, 24),
        ] {
            let frames = last_frame_prefix_with_count(version, 1);
            let bytes = scalar_record_for_version(version);
            assert_eq!(bytes.len(), byte_count, "fixture size at version {version}");
            let (decoded, consumed) = frames
                .decode_scalar_fields(&bytes[..byte_count])
                .expect("versioned scalar fields are complete");
            assert_eq!(consumed, byte_count, "version {version}");
            assert_eq!(decoded.encode(), bytes[..byte_count], "version {version}");
            assert_eq!(decoded.records().len(), 1, "version {version}");
            let record = &decoded.records()[0];
            assert_eq!(record.source_building(), 0x0102);
            assert_eq!(record.target_building(), 0x0304);
            assert_eq!(record.transport_line().is_some(), version >= 52);
            assert_eq!(record.transfer_size(), 0x0708);
            assert_eq!(record.transfer_type(), 0x09);
            assert_eq!(record.wait_counter(), 0x0a);
            assert_eq!(record.block_counter().is_some(), version >= 99);
            assert_eq!(record.citizen_unit().is_some(), version >= 32);
            assert_eq!(record.path_unit().is_some(), version >= 47);
            assert_eq!(record.trailing_vehicle().is_some(), version >= 58);
            assert_eq!(record.cargo_parent().is_some(), version >= 104);
            assert_eq!(record.custom_value().is_some(), version >= 113_012);
        }

        let frames = last_frame_prefix_with_count(113_012, 2);
        let mut bytes = scalar_record_for_version(113_012);
        let mut second_record = scalar_record_for_version(113_012);
        second_record[..2].copy_from_slice(&[0x21, 0x22]);
        bytes.extend_from_slice(&second_record);
        let (decoded, consumed) = frames
            .decode_scalar_fields(&bytes)
            .expect("two current scalar records are complete");
        assert_eq!(consumed, 48);
        assert_eq!(decoded.records().len(), 2);
        assert_eq!(decoded.records()[0].citizen_unit(), Some(0x0c0d0e));
        assert_eq!(decoded.records()[0].path_position(), Some([0x11, 0x12]));
        assert_eq!(decoded.records()[0].custom_value(), Some(0x1718));
        assert_eq!(decoded.records()[1].source_building(), 0x2122);
        assert_eq!(decoded.encode(), bytes);
    }

    #[test]
    fn vehicle_scalar_fields_reject_truncation_and_handle_empty_records() {
        let one_current = last_frame_prefix_with_count(113_012, 1);
        let record = scalar_record_for_version(113_012);
        assert!(one_current.decode_scalar_fields(&record[..23]).is_err());

        let empty = last_frame_prefix_with_count(113_012, 0);
        let (decoded, consumed) = empty.decode_scalar_fields(&[0xaa, 0xbb]).unwrap();
        assert_eq!(decoded.records(), []);
        assert_eq!(consumed, 0);
        assert_eq!(decoded.encode(), Vec::<u8>::new());
    }
}
