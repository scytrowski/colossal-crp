use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    EncodedUIntArrayReader, read_encoded_byte_array, read_encoded_ushort_array,
    write_encoded_byte_array, write_encoded_ushort_array,
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
const VEHICLE_TRAFFIC_TOTALS_VERSION: u32 = 315;
const VEHICLE_COUNTS_VERSION: u32 = 116_013;
const PARKED_VEHICLE_RECORD_LEN: usize = 31;

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

    /// Returns the number of active parked vehicles, absent before version 115.
    pub fn active_parked_vehicle_count(&self) -> Option<usize> {
        self.parked_vehicle_flags
            .as_deref()
            .map(|flags| flags.iter().filter(|flags| **flags != 0).count())
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
                active_parked_vehicle_count: self.active_parked_vehicle_count(),
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
    active_parked_vehicle_count: Option<usize>,
    gate_indices: Option<Vec<u8>>,
    race_teams: Option<Vec<u16>>,
    race_teammates: Option<Vec<u8>>,
    racer_indices: Option<Vec<u8>>,
    water_sources: Option<Vec<u16>>,
    serialized_prefix: Vec<u8>,
}

/// Editor for version-gated sparse columns of existing active vehicles.
///
/// It retains the active vehicle count and field gates. Setters return errors
/// when a field is absent at the source version or an index is out of range.
/// An unchanged editor retains the original encoded bytes; changing any value
/// canonically re-encodes the complete sparse section while retaining the
/// other decoded values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleManagerSparseFieldsEditor {
    original: VehicleManagerSparseFieldsPrefix,
    gate_indices: Option<Vec<u8>>,
    race_teams: Option<Vec<u16>>,
    race_teammates: Option<Vec<u8>>,
    racer_indices: Option<Vec<u8>>,
    water_sources: Option<Vec<u16>>,
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

    /// Returns the active parked-vehicle count, absent before version 115.
    pub fn active_parked_vehicle_count(&self) -> Option<usize> {
        self.active_parked_vehicle_count
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

    /// Creates an editor for the existing sparse active-vehicle columns.
    ///
    /// The active count and version gates stay fixed. Encoding an unchanged
    /// editor returns the original bytes; edits re-encode all sparse columns.
    pub fn into_editor(self) -> VehicleManagerSparseFieldsEditor {
        VehicleManagerSparseFieldsEditor {
            gate_indices: self.gate_indices.clone(),
            race_teams: self.race_teams.clone(),
            race_teammates: self.race_teammates.clone(),
            racer_indices: self.racer_indices.clone(),
            water_sources: self.water_sources.clone(),
            original: self,
        }
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
                active_parked_vehicle_count: self.active_parked_vehicle_count,
                records,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }
}

impl VehicleManagerSparseFieldsEditor {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns the number of existing active vehicle entries.
    pub fn active_vehicle_count(&self) -> usize {
        self.original.active_vehicle_count
    }

    /// Returns current gate indexes from inner version 182.
    pub fn gate_indices(&self) -> Option<&[u8]> {
        self.gate_indices.as_deref()
    }

    /// Changes the gate index for one existing active vehicle (version 182+).
    pub fn set_gate_index(&mut self, index: usize, value: u8) -> Result<(), CrpError> {
        set_sparse_value(&mut self.gate_indices, index, value, "gate index")
    }

    /// Returns current race-team indexes from inner version 121008.
    pub fn race_teams(&self) -> Option<&[u16]> {
        self.race_teams.as_deref()
    }

    /// Changes the race-team index for one existing active vehicle (version 121008+).
    pub fn set_race_team(&mut self, index: usize, value: u16) -> Result<(), CrpError> {
        set_sparse_value(&mut self.race_teams, index, value, "race-team index")
    }

    /// Returns current race-teammate values from inner version 121012.
    pub fn race_teammates(&self) -> Option<&[u8]> {
        self.race_teammates.as_deref()
    }

    /// Changes the race-teammate value for one existing active vehicle (version 121012+).
    pub fn set_race_teammate(&mut self, index: usize, value: u8) -> Result<(), CrpError> {
        set_sparse_value(&mut self.race_teammates, index, value, "race teammate")
    }

    /// Returns current racer indexes from inner version 121012.
    pub fn racer_indices(&self) -> Option<&[u8]> {
        self.racer_indices.as_deref()
    }

    /// Changes the racer index for one existing active vehicle (version 121012+).
    pub fn set_racer_index(&mut self, index: usize, value: u8) -> Result<(), CrpError> {
        set_sparse_value(&mut self.racer_indices, index, value, "racer index")
    }

    /// Returns current water-source indexes from inner version 273.
    pub fn water_sources(&self) -> Option<&[u16]> {
        self.water_sources.as_deref()
    }

    /// Changes the water-source index for one existing active vehicle (version 273+).
    pub fn set_water_source(&mut self, index: usize, value: u16) -> Result<(), CrpError> {
        set_sparse_value(&mut self.water_sources, index, value, "water-source index")
    }

    /// Writes all sparse columns in their versioned field order.
    ///
    /// If no values changed, the source encoding is returned byte-for-byte.
    /// Otherwise all sparse columns are encoded canonically and the byte
    /// length can change.
    pub fn encode(&self) -> Vec<u8> {
        let unchanged = self.gate_indices == self.original.gate_indices
            && self.race_teams == self.original.race_teams
            && self.race_teammates == self.original.race_teammates
            && self.racer_indices == self.original.racer_indices
            && self.water_sources == self.original.water_sources;
        if unchanged {
            return self.original.serialized_prefix.clone();
        }

        let mut bytes = Vec::new();
        if let Some(values) = &self.gate_indices {
            write_encoded_byte_array(&mut bytes, values);
        }
        if let Some(values) = &self.race_teams {
            write_encoded_ushort_array(&mut bytes, values);
        }
        if let Some(values) = &self.race_teammates {
            write_encoded_byte_array(&mut bytes, values);
        }
        if let Some(values) = &self.racer_indices {
            write_encoded_byte_array(&mut bytes, values);
        }
        if let Some(values) = &self.water_sources {
            write_encoded_ushort_array(&mut bytes, values);
        }
        bytes
    }

    pub(crate) fn original_bytes(&self) -> &[u8] {
        &self.original.serialized_prefix
    }
}

fn set_sparse_value<T: Copy>(
    values: &mut Option<Vec<T>>,
    index: usize,
    value: T,
    field: &'static str,
) -> Result<(), CrpError> {
    let Some(values) = values.as_mut() else {
        return Err(invalid(
            index,
            format!("VehicleManager {field} is not serialized at this version"),
        ));
    };
    let slot = values.get_mut(index).ok_or_else(|| {
        invalid(
            index,
            format!("VehicleManager {field} index {index} is out of range"),
        )
    })?;
    *slot = value;
    Ok(())
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
    active_parked_vehicle_count: Option<usize>,
    records: Vec<VehicleLastFrameRecord>,
    serialized_prefix: Vec<u8>,
}

/// Editor for fixed-width active-vehicle motion and target-position vectors.
///
/// Record counts and version-gated fields remain fixed. Float values are
/// exposed as IEEE-754 bit patterns so unchanged NaNs and signed zero retain
/// their original representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleLastFrameEditor {
    original: VehicleLastFrameDataPrefix,
    records: Vec<VehicleLastFrameRecord>,
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

    /// Creates an editor for the existing active-vehicle vector records.
    pub fn into_editor(self) -> VehicleLastFrameEditor {
        VehicleLastFrameEditor {
            records: self.records.clone(),
            original: self,
        }
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
                active_parked_vehicle_count: self.active_parked_vehicle_count,
                records,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }
}

impl VehicleLastFrameEditor {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns the current active-vehicle vector records.
    pub fn records(&self) -> &[VehicleLastFrameRecord] {
        &self.records
    }

    /// Changes an existing vehicle's velocity component bit patterns
    /// (inner version 47 and newer).
    pub fn set_velocity_bits(&mut self, index: usize, value: [u32; 3]) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        let velocity = record.velocity.as_mut().ok_or_else(|| {
            invalid(
                index,
                "vehicle velocity is not serialized before version 47",
            )
        })?;
        *velocity = value;
        Ok(())
    }

    /// Changes an existing vehicle's position component bit patterns.
    pub fn set_position_bits(&mut self, index: usize, value: [u32; 3]) -> Result<(), CrpError> {
        self.record_mut(index)?.position = value;
        Ok(())
    }

    /// Changes an existing vehicle's rotation component bit patterns
    /// (inner version 78 and newer).
    pub fn set_rotation_bits(&mut self, index: usize, value: [u32; 4]) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        let rotation = record.rotation.as_mut().ok_or_else(|| {
            invalid(
                index,
                "vehicle rotation is not serialized before version 78",
            )
        })?;
        *rotation = value;
        Ok(())
    }

    /// Changes an existing vehicle's angle-velocity component bit patterns
    /// (inner version 129 and newer).
    pub fn set_angle_velocity_bits(
        &mut self,
        index: usize,
        value: [u32; 3],
    ) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        let angle_velocity = record.angle_velocity.as_mut().ok_or_else(|| {
            invalid(
                index,
                "vehicle angle velocity is not serialized before version 129",
            )
        })?;
        *angle_velocity = value;
        Ok(())
    }

    /// Changes `m_targetPos0` using the source version's stored vector width.
    pub fn set_target_position0(
        &mut self,
        index: usize,
        value: VehicleTargetPosition0,
    ) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        let width_matches = matches!(
            (record.target_position0, value),
            (
                VehicleTargetPosition0::DerivedFromPosition,
                VehicleTargetPosition0::DerivedFromPosition
            ) | (
                VehicleTargetPosition0::Vector3(_),
                VehicleTargetPosition0::Vector3(_)
            ) | (
                VehicleTargetPosition0::Vector4(_),
                VehicleTargetPosition0::Vector4(_)
            )
        );
        if !width_matches {
            return Err(invalid(
                index,
                "target position 0 width does not match the source version",
            ));
        }
        record.target_position0 = value;
        Ok(())
    }

    /// Changes target positions 1–3 (inner version 90 and newer).
    pub fn set_target_positions1_to_3(
        &mut self,
        index: usize,
        value: [[u32; 4]; 3],
    ) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        let targets = record.target_positions1_to_3.as_mut().ok_or_else(|| {
            invalid(
                index,
                "target positions 1–3 are not serialized before version 90",
            )
        })?;
        *targets = value;
        Ok(())
    }

    /// Writes the same fixed-width record count and versioned field layout.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.original.serialized_prefix.len());
        for record in &self.records {
            if let Some(velocity) = record.velocity {
                append_vector_bits(&mut bytes, &velocity);
            }
            append_vector_bits(&mut bytes, &record.position);
            if let Some(rotation) = record.rotation {
                append_vector_bits(&mut bytes, &rotation);
            }
            if let Some(angle_velocity) = record.angle_velocity {
                append_vector_bits(&mut bytes, &angle_velocity);
            }
            match record.target_position0 {
                VehicleTargetPosition0::DerivedFromPosition => {}
                VehicleTargetPosition0::Vector3(value) => append_vector_bits(&mut bytes, &value),
                VehicleTargetPosition0::Vector4(value) => append_vector_bits(&mut bytes, &value),
            }
            if let Some(targets) = record.target_positions1_to_3 {
                for target in targets {
                    append_vector_bits(&mut bytes, &target);
                }
            }
        }
        bytes
    }

    pub(crate) fn original_bytes(&self) -> &[u8] {
        &self.original.serialized_prefix
    }

    fn record_mut(&mut self, index: usize) -> Result<&mut VehicleLastFrameRecord, CrpError> {
        self.records.get_mut(index).ok_or_else(|| {
            invalid(
                index,
                format!("vehicle record index {index} is out of range"),
            )
        })
    }
}

fn append_vector_bits<const N: usize>(bytes: &mut Vec<u8>, values: &[u32; N]) {
    for value in values {
        bytes.extend_from_slice(&value.to_be_bytes());
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
    active_parked_vehicle_count: Option<usize>,
    records: Vec<VehicleInstanceFields>,
    serialized_prefix: Vec<u8>,
}

/// Editor for fixed-width active-vehicle scalar and reference fields.
///
/// The record count and version-gated fields remain fixed. Citizen-unit
/// references are checked against their serialized 24-bit width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleInstanceFieldsEditor {
    original: VehicleInstanceFieldsPrefix,
    records: Vec<VehicleInstanceFields>,
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

    /// Creates an editor for the existing active-vehicle scalar fields.
    pub fn into_editor(self) -> VehicleInstanceFieldsEditor {
        VehicleInstanceFieldsEditor {
            records: self.records.clone(),
            original: self,
        }
    }

    /// Decodes serialized parked-vehicle records after the active vehicles.
    ///
    /// Versions before 115 have no parked-record section. From version 115,
    /// each active parked vehicle stores a position, rotation, and 24-bit
    /// owner-citizen reference. The returned byte count ends before manager
    /// counters or the closing profiler reference.
    pub fn decode_parked_vehicle_records(
        &self,
        bytes: &[u8],
    ) -> Result<(VehicleParkedRecordsPrefix, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let records = if let Some(count) = self.active_parked_vehicle_count {
            let mut records = Vec::with_capacity(count);
            for _ in 0..count {
                records.push(VehicleParkedInstanceRecord {
                    position: read_vector::<3>(&mut reader)?,
                    rotation: read_vector::<4>(&mut reader)?,
                    owner_citizen: reader.read_u24_be()?,
                });
            }
            Some(records)
        } else {
            None
        };
        let consumed = reader.offset();
        Ok((
            VehicleParkedRecordsPrefix {
                format_version: self.format_version,
                records,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }
}

impl VehicleInstanceFieldsEditor {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns the current fields, in active-vehicle order.
    pub fn records(&self) -> &[VehicleInstanceFields] {
        &self.records
    }

    /// Changes the source-building index.
    pub fn set_source_building(&mut self, index: usize, value: u16) -> Result<(), CrpError> {
        self.record_mut(index)?.source_building = value;
        Ok(())
    }

    /// Changes the target-building index.
    pub fn set_target_building(&mut self, index: usize, value: u16) -> Result<(), CrpError> {
        self.record_mut(index)?.target_building = value;
        Ok(())
    }

    /// Changes the transport-line index from version 52.
    pub fn set_transport_line(&mut self, index: usize, value: u16) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        let field = record.transport_line.as_mut().ok_or_else(|| {
            invalid(
                index,
                "transport-line field is not serialized before version 52",
            )
        })?;
        *field = value;
        Ok(())
    }

    /// Changes the transfer size.
    pub fn set_transfer_size(&mut self, index: usize, value: u16) -> Result<(), CrpError> {
        self.record_mut(index)?.transfer_size = value;
        Ok(())
    }

    /// Changes the transfer-type byte.
    pub fn set_transfer_type(&mut self, index: usize, value: u8) -> Result<(), CrpError> {
        self.record_mut(index)?.transfer_type = value;
        Ok(())
    }

    /// Changes the wait-counter byte.
    pub fn set_wait_counter(&mut self, index: usize, value: u8) -> Result<(), CrpError> {
        self.record_mut(index)?.wait_counter = value;
        Ok(())
    }

    /// Changes the block-counter byte from version 99.
    pub fn set_block_counter(&mut self, index: usize, value: u8) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        let field = record
            .block_counter
            .as_mut()
            .ok_or_else(|| invalid(index, "block counter is not serialized before version 99"))?;
        *field = value;
        Ok(())
    }

    /// Changes the 24-bit citizen-unit reference from version 32.
    pub fn set_citizen_unit(&mut self, index: usize, value: u32) -> Result<(), CrpError> {
        if value > 0x00ff_ffff {
            return Err(invalid(
                index,
                "vehicle citizen-unit reference does not fit u24",
            ));
        }
        let record = self.record_mut(index)?;
        let field = record.citizen_unit.as_mut().ok_or_else(|| {
            invalid(
                index,
                "citizen-unit reference is not serialized before version 32",
            )
        })?;
        *field = value;
        Ok(())
    }

    /// Changes the path-unit reference from version 47.
    pub fn set_path_unit(&mut self, index: usize, value: u16) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        let field = record.path_unit.as_mut().ok_or_else(|| {
            invalid(
                index,
                "path-unit reference is not serialized before version 47",
            )
        })?;
        *field = value;
        Ok(())
    }

    /// Changes both path-position bytes from version 47.
    pub fn set_path_position(&mut self, index: usize, value: [u8; 2]) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        let field = record
            .path_position
            .as_mut()
            .ok_or_else(|| invalid(index, "path position is not serialized before version 47"))?;
        *field = value;
        Ok(())
    }

    /// Changes the trailing-vehicle index from version 58.
    pub fn set_trailing_vehicle(&mut self, index: usize, value: u16) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        let field = record.trailing_vehicle.as_mut().ok_or_else(|| {
            invalid(
                index,
                "trailing-vehicle field is not serialized before version 58",
            )
        })?;
        *field = value;
        Ok(())
    }

    /// Changes the cargo-parent index from version 104.
    pub fn set_cargo_parent(&mut self, index: usize, value: u16) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        let field = record.cargo_parent.as_mut().ok_or_else(|| {
            invalid(
                index,
                "cargo-parent field is not serialized before version 104",
            )
        })?;
        *field = value;
        Ok(())
    }

    /// Changes the custom value from version 113012.
    pub fn set_custom_value(&mut self, index: usize, value: u16) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        let field = record.custom_value.as_mut().ok_or_else(|| {
            invalid(
                index,
                "custom value is not serialized before version 113012",
            )
        })?;
        *field = value;
        Ok(())
    }

    /// Writes the same fixed-width record count and version-gated field layout.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.original.serialized_prefix.len());
        for record in &self.records {
            bytes.extend_from_slice(&record.source_building.to_be_bytes());
            bytes.extend_from_slice(&record.target_building.to_be_bytes());
            if let Some(value) = record.transport_line {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend_from_slice(&record.transfer_size.to_be_bytes());
            bytes.push(record.transfer_type);
            bytes.push(record.wait_counter);
            if let Some(value) = record.block_counter {
                bytes.push(value);
            }
            if let Some(value) = record.citizen_unit {
                bytes.push((value >> 16) as u8);
                bytes.push((value >> 8) as u8);
                bytes.push(value as u8);
            }
            if let Some(value) = record.path_unit {
                bytes.extend_from_slice(&value.to_be_bytes());
                bytes.extend_from_slice(&record.path_position.unwrap_or_default());
            }
            if let Some(value) = record.trailing_vehicle {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            if let Some(value) = record.cargo_parent {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            if let Some(value) = record.custom_value {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
        bytes
    }

    pub(crate) fn original_bytes(&self) -> &[u8] {
        &self.original.serialized_prefix
    }

    fn record_mut(&mut self, index: usize) -> Result<&mut VehicleInstanceFields, CrpError> {
        self.records.get_mut(index).ok_or_else(|| {
            invalid(
                index,
                format!("vehicle record index {index} is out of range"),
            )
        })
    }
}

/// Position, rotation, and owner reference for one active parked vehicle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VehicleParkedInstanceRecord {
    position: [u32; 3],
    rotation: [u32; 4],
    owner_citizen: u32,
}

impl VehicleParkedInstanceRecord {
    /// Returns position component bit patterns.
    pub fn position(&self) -> [u32; 3] {
        self.position
    }

    /// Returns quaternion component bit patterns.
    pub fn rotation(&self) -> [u32; 4] {
        self.rotation
    }

    /// Returns the 24-bit owner-citizen reference.
    pub fn owner_citizen(&self) -> u32 {
        self.owner_citizen
    }
}

/// Decoded parked-vehicle records after active vehicle records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleParkedRecordsPrefix {
    format_version: u32,
    records: Option<Vec<VehicleParkedInstanceRecord>>,
    serialized_prefix: Vec<u8>,
}

/// Editor for position, rotation, and owner references of existing parked
/// vehicles. The record count and parked-vehicle slots remain fixed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleParkedRecordsEditor {
    original: VehicleParkedRecordsPrefix,
    records: Option<Vec<VehicleParkedInstanceRecord>>,
}

impl VehicleParkedRecordsPrefix {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns active parked records, absent before version 115.
    pub fn records(&self) -> Option<&[VehicleParkedInstanceRecord]> {
        self.records.as_deref()
    }

    /// Writes the original parked-record bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Creates an editor for the parked records represented in this prefix.
    pub fn into_editor(self) -> VehicleParkedRecordsEditor {
        VehicleParkedRecordsEditor {
            records: self.records.clone(),
            original: self,
        }
    }

    /// Decodes versioned counters following the parked-record array.
    ///
    /// Traffic totals appear from version 315; temporary and final vehicle
    /// counts appear from version 116013. The returned byte count ends before
    /// the two-byte manager-closing profiler reference.
    pub fn decode_tail(
        &self,
        bytes: &[u8],
    ) -> Result<(VehicleManagerTailDataPrefix, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let traffic_flow_totals = if self.format_version >= VEHICLE_TRAFFIC_TOTALS_VERSION {
            Some([reader.read_u32_be()?, reader.read_u32_be()?])
        } else {
            None
        };
        let traffic_flow_byte = if self.format_version >= VEHICLE_TRAFFIC_TOTALS_VERSION {
            Some(reader.read_u8()?)
        } else {
            None
        };
        let temporary_vehicle_count = if self.format_version >= VEHICLE_COUNTS_VERSION {
            Some(reader.read_u32_be()?)
        } else {
            None
        };
        let final_vehicle_count = if self.format_version >= VEHICLE_COUNTS_VERSION {
            Some(reader.read_u32_be()?)
        } else {
            None
        };
        let consumed = reader.offset();
        Ok((
            VehicleManagerTailDataPrefix {
                format_version: self.format_version,
                traffic_flow_totals,
                traffic_flow_byte,
                temporary_vehicle_count,
                final_vehicle_count,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }
}

impl VehicleParkedRecordsEditor {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns current parked records, absent before version 115.
    pub fn records(&self) -> Option<&[VehicleParkedInstanceRecord]> {
        self.records.as_deref()
    }

    /// Changes one parked vehicle's position component bit patterns.
    pub fn set_position_bits(&mut self, index: usize, position: [u32; 3]) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        record.position = position;
        Ok(())
    }

    /// Changes one parked vehicle's quaternion component bit patterns.
    pub fn set_rotation_bits(&mut self, index: usize, rotation: [u32; 4]) -> Result<(), CrpError> {
        let record = self.record_mut(index)?;
        record.rotation = rotation;
        Ok(())
    }

    /// Changes one parked vehicle's 24-bit owner-citizen reference.
    pub fn set_owner_citizen(&mut self, index: usize, owner: u32) -> Result<(), CrpError> {
        if owner > 0x00ff_ffff {
            return Err(invalid(
                index,
                "parked-vehicle owner reference does not fit u24",
            ));
        }
        let record = self.record_mut(index)?;
        record.owner_citizen = owner;
        Ok(())
    }

    /// Writes edited records in the game's fixed-width field order while
    /// preserving the original bytes when no record changed.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.records == self.original.records {
            return Ok(self.original.serialized_prefix.clone());
        }
        let records = self.records.as_ref().ok_or_else(|| {
            invalid(
                0,
                "parked-vehicle records are unavailable before inner version 115",
            )
        })?;
        let original_records = self.original.records.as_ref().ok_or_else(|| {
            invalid(
                0,
                "parked-vehicle records are unavailable before inner version 115",
            )
        })?;
        if records.len() != original_records.len() {
            return Err(invalid(0, "parked-vehicle record count cannot change"));
        }

        let mut bytes = Vec::with_capacity(records.len() * PARKED_VEHICLE_RECORD_LEN);
        for (index, record) in records.iter().enumerate() {
            if record.owner_citizen > 0x00ff_ffff {
                return Err(invalid(
                    index,
                    "parked-vehicle owner reference does not fit u24",
                ));
            }
            for value in record.position {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            for value in record.rotation {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.push((record.owner_citizen >> 16) as u8);
            bytes.push((record.owner_citizen >> 8) as u8);
            bytes.push(record.owner_citizen as u8);
        }
        if bytes.len() != self.original.serialized_prefix.len() {
            return Err(invalid(
                0,
                "parked-vehicle records changed their serialized width",
            ));
        }
        Ok(bytes)
    }

    pub(crate) fn original_bytes(&self) -> &[u8] {
        &self.original.serialized_prefix
    }

    fn record_mut(&mut self, index: usize) -> Result<&mut VehicleParkedInstanceRecord, CrpError> {
        self.records
            .as_mut()
            .ok_or_else(|| {
                invalid(
                    index,
                    "parked-vehicle records are unavailable before inner version 115",
                )
            })?
            .get_mut(index)
            .ok_or_else(|| invalid(index, "parked-vehicle record index is out of bounds"))
    }
}

/// Versioned VehicleManager counters after parked-vehicle records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleManagerTailDataPrefix {
    format_version: u32,
    traffic_flow_totals: Option<[u32; 2]>,
    traffic_flow_byte: Option<u8>,
    temporary_vehicle_count: Option<u32>,
    final_vehicle_count: Option<u32>,
    serialized_prefix: Vec<u8>,
}

impl VehicleManagerTailDataPrefix {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the two serialized traffic-flow totals from version 315.
    pub fn traffic_flow_totals(&self) -> Option<[u32; 2]> {
        self.traffic_flow_totals
    }

    /// Returns the trailing traffic-flow byte from version 315.
    pub fn traffic_flow_byte(&self) -> Option<u8> {
        self.traffic_flow_byte
    }

    /// Returns the temporary vehicle count from version 116013.
    pub fn temporary_vehicle_count(&self) -> Option<u32> {
        self.temporary_vehicle_count
    }

    /// Returns the final vehicle count from version 116013.
    pub fn final_vehicle_count(&self) -> Option<u32> {
        self.final_vehicle_count
    }

    /// Writes the original counter bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

/// Fully decoded known `VehicleManager+Data` sections through its counters.
///
/// The original bounded manager body, including its closing profiler
/// reference, remains available for exact rewriting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleManagerData {
    prefix: VehicleManagerDataPrefix,
    sparse_columns: VehicleManagerSparseFieldsPrefix,
    last_frame_records: VehicleLastFrameDataPrefix,
    instance_fields: VehicleInstanceFieldsPrefix,
    parked_records: VehicleParkedRecordsPrefix,
    tail: VehicleManagerTailDataPrefix,
    serialized_body: Vec<u8>,
}

impl VehicleManagerData {
    /// Decodes the known fields in one bounded `VehicleManager+Data` body.
    ///
    /// The body must end with the two-byte closing profiler reference. The
    /// returned value preserves the complete body, including that reference.
    pub(crate) fn decode_body(format_version: u32, bytes: &[u8]) -> Result<Self, CrpError> {
        let (prefix, mut offset) = VehicleManagerDataPrefix::decode_prefix(format_version, bytes)?;
        let (sparse_columns, consumed) = prefix.decode_sparse_columns(remaining(bytes, offset)?)?;
        offset = checked_end(offset, consumed, "VehicleManager sparse columns")?;

        let (last_frame_records, consumed) =
            sparse_columns.decode_last_frame_records(remaining(bytes, offset)?)?;
        offset = checked_end(offset, consumed, "VehicleManager last-frame records")?;

        let (instance_fields, consumed) =
            last_frame_records.decode_scalar_fields(remaining(bytes, offset)?)?;
        offset = checked_end(offset, consumed, "VehicleManager instance fields")?;

        let (parked_records, consumed) =
            instance_fields.decode_parked_vehicle_records(remaining(bytes, offset)?)?;
        offset = checked_end(offset, consumed, "VehicleManager parked records")?;

        let (tail, consumed) = parked_records.decode_tail(remaining(bytes, offset)?)?;
        offset = checked_end(offset, consumed, "VehicleManager counters")?;

        let closing_reference_end = checked_end(offset, 2, "VehicleManager closing reference")?;
        if closing_reference_end != bytes.len()
            || bytes.get(offset).is_none_or(|marker| *marker >= 0xfe)
        {
            return Err(invalid(
                offset,
                "VehicleManager counters do not end before its two-byte closing profiler reference",
            ));
        }

        Ok(Self {
            prefix,
            sparse_columns,
            last_frame_records,
            instance_fields,
            parked_records,
            tail,
            serialized_body: bytes.to_vec(),
        })
    }

    /// Returns the initial flags and prefab collection.
    pub fn prefix(&self) -> &VehicleManagerDataPrefix {
        &self.prefix
    }

    /// Returns the version-gated sparse active-vehicle columns.
    pub fn sparse_columns(&self) -> &VehicleManagerSparseFieldsPrefix {
        &self.sparse_columns
    }

    /// Returns active-vehicle last-frame vectors and target positions.
    pub fn last_frame_records(&self) -> &VehicleLastFrameDataPrefix {
        &self.last_frame_records
    }

    /// Returns remaining active-vehicle scalar and reference fields.
    pub fn instance_fields(&self) -> &VehicleInstanceFieldsPrefix {
        &self.instance_fields
    }

    /// Returns active parked-vehicle records when present.
    pub fn parked_records(&self) -> &VehicleParkedRecordsPrefix {
        &self.parked_records
    }

    /// Returns the version-gated manager counters.
    pub fn tail(&self) -> &VehicleManagerTailDataPrefix {
        &self.tail
    }

    /// Writes the original bounded manager body exactly, including its closing
    /// profiler reference.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }
}

fn remaining(bytes: &[u8], offset: usize) -> Result<&[u8], CrpError> {
    bytes
        .get(offset..)
        .ok_or_else(|| invalid(offset, "VehicleManager field offset is out of bounds"))
}

fn checked_end(offset: usize, consumed: usize, context: &'static str) -> Result<usize, CrpError> {
    offset
        .checked_add(consumed)
        .ok_or(CrpError::ArithmeticOverflow(context))
}

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "VehicleManager+Data".to_owned(),
        offset,
        reason: reason.into(),
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
        VehicleInstanceFieldsPrefix, VehicleLastFrameDataPrefix, VehicleLastFrameRecord,
        VehicleManagerData, VehicleManagerDataPrefix, VehicleManagerSparseFieldsPrefix,
        VehicleParkedRecordsPrefix, VehicleTargetPosition0,
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
            active_parked_vehicle_count: None,
            gate_indices: None,
            race_teams: None,
            race_teammates: None,
            racer_indices: None,
            water_sources: None,
            serialized_prefix: Vec::new(),
        }
    }

    #[test]
    fn vehicle_sparse_fields_editor_rewrites_versioned_columns_canonically() {
        let prefix = prefix_with_active_slots(121_012, 4);
        let source = [
            0x03, 1, 2, 3, 4, // gate indexes
            0x03, 0, 4, 0, 3, 0, 2, 0, 1, // race-team indexes
            0x03, 4, 3, 2, 1, // race teammates
            0x03, 4, 3, 2, 1, // racer indexes
            0x03, 0, 4, 0, 3, 0, 2, 0, 1, // water-source indexes
        ];
        let (decoded, consumed) = prefix.decode_sparse_columns(&source).unwrap();
        assert_eq!(consumed, source.len());
        assert_eq!(decoded.active_vehicle_count(), 4);
        assert_eq!(decoded.gate_indices(), Some([1, 2, 3, 4].as_slice()));
        assert_eq!(decoded.race_teams(), Some([4, 3, 2, 1].as_slice()));
        assert_eq!(decoded.race_teammates(), Some([4, 3, 2, 1].as_slice()));
        assert_eq!(decoded.racer_indices(), Some([4, 3, 2, 1].as_slice()));
        assert_eq!(decoded.water_sources(), Some([4, 3, 2, 1].as_slice()));

        let mut editor = decoded.into_editor();
        assert_eq!(editor.encode(), source);
        editor.set_gate_index(0, 9).unwrap();
        for (index, value) in [1, 2, 3, 4].into_iter().enumerate() {
            editor.set_race_team(index, value).unwrap();
        }
        for (index, value) in [5, 6, 7, 8].into_iter().enumerate() {
            editor.set_race_teammate(index, value).unwrap();
        }
        for (index, value) in [9, 10, 11, 12].into_iter().enumerate() {
            editor.set_racer_index(index, value).unwrap();
        }
        for (index, value) in [10, 20, 30, 40].into_iter().enumerate() {
            editor.set_water_source(index, value).unwrap();
        }

        let expected = [
            0x03, 9, 2, 3, 4, // gate indexes
            0x00, 0, 1, 0x82, // one race-team literal and three repeated deltas
            0x03, 5, 6, 7, 8, // race teammates
            0x03, 9, 10, 11, 12, // racer indexes
            0x00, 0, 10, 0x82, // one water-source literal and three repeated deltas
        ];
        assert_eq!(editor.encode(), expected);
        assert!(editor.encode().len() < source.len());
        assert!(editor.set_gate_index(4, 1).is_err());
        assert_eq!(editor.gate_indices(), Some([9, 2, 3, 4].as_slice()));
    }

    #[test]
    fn vehicle_sparse_fields_editor_rejects_fields_before_version_gates() {
        let prefix = prefix_with_active_slots(181, 1);
        let (decoded, consumed) = prefix.decode_sparse_columns(&[]).unwrap();
        assert_eq!(consumed, 0);
        let mut editor = decoded.into_editor();
        assert!(editor.set_gate_index(0, 1).is_err());
        assert!(editor.set_race_team(0, 1).is_err());
        assert!(editor.set_race_teammate(0, 1).is_err());
        assert!(editor.set_racer_index(0, 1).is_err());
        assert!(editor.set_water_source(0, 1).is_err());
        assert_eq!(editor.encode(), []);
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
            active_parked_vehicle_count: None,
            records: std::iter::repeat_n(empty_record, record_count).collect(),
            serialized_prefix: Vec::new(),
        }
    }

    fn instance_fields_prefix(
        format_version: u32,
        active_parked_vehicle_count: Option<usize>,
    ) -> VehicleInstanceFieldsPrefix {
        VehicleInstanceFieldsPrefix {
            format_version,
            active_parked_vehicle_count,
            records: Vec::new(),
            serialized_prefix: Vec::new(),
        }
    }

    fn parked_records_prefix(format_version: u32) -> VehicleParkedRecordsPrefix {
        VehicleParkedRecordsPrefix {
            format_version,
            records: Some(Vec::new()),
            serialized_prefix: Vec::new(),
        }
    }

    #[test]
    fn parked_vehicle_editor_writes_fixed_big_endian_records_and_checks_u24() {
        let mut source = Vec::new();
        for bits in [0x3f80_0000_u32, 0xc020_0000, 0x42f6_e979] {
            source.extend_from_slice(&bits.to_be_bytes());
        }
        for bits in [0x3f00_0000_u32, 0x3f80_0000, 0xbf00_0000, 0x3e80_0000] {
            source.extend_from_slice(&bits.to_be_bytes());
        }
        source.extend_from_slice(&[0x01, 0x23, 0x45]);

        let fields = instance_fields_prefix(115, Some(1));
        let (records, consumed) = fields.decode_parked_vehicle_records(&source).unwrap();
        assert_eq!(consumed, 31);
        assert_eq!(
            records.records().unwrap()[0].position(),
            [0x3f80_0000, 0xc020_0000, 0x42f6_e979,]
        );
        assert_eq!(records.records().unwrap()[0].owner_citizen(), 0x0123_45);

        let mut editor = records.into_editor();
        editor
            .set_position_bits(0, [0x3f00_0000, 0xbf80_0000, 0x4120_0000])
            .unwrap();
        editor
            .set_rotation_bits(0, [0x3e80_0000, 0x3f40_0000, 0xbf40_0000, 0x3f80_0000])
            .unwrap();
        editor.set_owner_citizen(0, 0x00ab_cdef).unwrap();

        let mut expected = Vec::new();
        for bits in [0x3f00_0000_u32, 0xbf80_0000, 0x4120_0000] {
            expected.extend_from_slice(&bits.to_be_bytes());
        }
        for bits in [0x3e80_0000_u32, 0x3f40_0000, 0xbf40_0000, 0x3f80_0000] {
            expected.extend_from_slice(&bits.to_be_bytes());
        }
        expected.extend_from_slice(&[0xab, 0xcd, 0xef]);
        assert_eq!(editor.encode().unwrap(), expected);
        assert!(editor.set_owner_citizen(0, 0x0100_0000).is_err());
        assert!(editor.set_owner_citizen(1, 0).is_err());

        let absent = instance_fields_prefix(114, None);
        let (legacy, _) = absent.decode_parked_vehicle_records(&[]).unwrap();
        let mut legacy_editor = legacy.into_editor();
        assert!(legacy_editor.records().is_none());
        assert!(legacy_editor.set_position_bits(0, [0; 3]).is_err());
        assert_eq!(legacy_editor.encode().unwrap(), []);
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

    fn parked_record(position: [u32; 3], rotation: [u32; 4], owner: u32) -> Vec<u8> {
        let mut bytes = words_as_bytes(&position);
        bytes.extend_from_slice(&words_as_bytes(&rotation));
        bytes.extend_from_slice(&owner.to_be_bytes()[1..]);
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
        assert_eq!(decoded.active_parked_vehicle_count(), Some(1));
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
    fn last_frame_editor_patches_versioned_vectors_and_preserves_raw_float_bits() {
        let fields = sparse_fields_with_count(129, 1);
        let source = words_as_bytes(&(1..=29).collect::<Vec<_>>());
        let (decoded, consumed) = fields.decode_last_frame_records(&source).unwrap();
        assert_eq!(consumed, source.len());
        let mut editor = decoded.into_editor();
        assert_eq!(editor.encode(), source);
        editor
            .set_velocity_bits(0, [0x7fc0_1234, 0x8000_0000, 0xff80_0000])
            .unwrap();
        editor
            .set_position_bits(0, [0x3f80_0000, 0xc020_0000, 0x42f6_e979])
            .unwrap();
        editor
            .set_rotation_bits(0, [0, 0x3f80_0000, 0xbf00_0000, 0x7fc0_5678])
            .unwrap();
        editor
            .set_angle_velocity_bits(0, [0x3f00_0000, 0xbf40_0000, 0x3e80_0000])
            .unwrap();
        editor
            .set_target_position0(
                0,
                VehicleTargetPosition0::Vector4([0x4100_0000, 0xc120_0000, 0x4270_0000, 1]),
            )
            .unwrap();
        editor
            .set_target_positions1_to_3(0, [[31, 32, 33, 34], [35, 36, 37, 38], [39, 40, 41, 42]])
            .unwrap();

        let mut expected = source.clone();
        let replacements = [
            (0, [0x7fc0_1234, 0x8000_0000, 0xff80_0000].as_slice()),
            (3, [0x3f80_0000, 0xc020_0000, 0x42f6_e979].as_slice()),
            (6, [0, 0x3f80_0000, 0xbf00_0000, 0x7fc0_5678].as_slice()),
            (10, [0x3f00_0000, 0xbf40_0000, 0x3e80_0000].as_slice()),
            (13, [0x4100_0000, 0xc120_0000, 0x4270_0000, 1].as_slice()),
        ];
        for (word_offset, values) in replacements {
            expected[word_offset * 4..(word_offset + values.len()) * 4]
                .copy_from_slice(&words_as_bytes(values));
        }
        expected[17 * 4..29 * 4].copy_from_slice(&words_as_bytes(&[
            31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42,
        ]));
        assert_eq!(editor.encode(), expected);
        assert_eq!(editor.encode().len(), source.len());

        let (decoded, consumed) = fields.decode_last_frame_records(&expected).unwrap();
        assert_eq!(consumed, expected.len());
        assert_eq!(
            decoded.records()[0].velocity(),
            Some([0x7fc0_1234, 0x8000_0000, 0xff80_0000])
        );
        assert_eq!(
            decoded.records()[0].position(),
            [0x3f80_0000, 0xc020_0000, 0x42f6_e979]
        );
        assert_eq!(decoded.encode(), expected);
    }

    #[test]
    fn last_frame_editor_rejects_absent_fields_width_changes_and_bad_indexes() {
        let fields = sparse_fields_with_count(46, 1);
        let source = words_as_bytes(&[1, 2, 3]);
        let (decoded, _) = fields.decode_last_frame_records(&source).unwrap();
        let mut editor = decoded.into_editor();
        assert!(editor.set_velocity_bits(0, [1, 2, 3]).is_err());
        assert!(editor.set_rotation_bits(0, [1, 2, 3, 4]).is_err());
        assert!(editor.set_angle_velocity_bits(0, [1, 2, 3]).is_err());
        assert!(editor.set_target_positions1_to_3(0, [[0; 4]; 3]).is_err());
        assert!(
            editor
                .set_target_position0(0, VehicleTargetPosition0::Vector3([1, 2, 3]))
                .is_err()
        );
        assert!(editor.set_position_bits(1, [1, 2, 3]).is_err());
        editor
            .set_target_position0(0, VehicleTargetPosition0::DerivedFromPosition)
            .unwrap();
        editor
            .set_position_bits(0, [0x1234, 0x5678, 0x9abc])
            .unwrap();
        assert_eq!(editor.encode(), words_as_bytes(&[0x1234, 0x5678, 0x9abc]));
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
    fn vehicle_instance_fields_editor_writes_exact_versioned_bytes() {
        let source = scalar_record_for_version(113_012);
        let frames = last_frame_prefix_with_count(113_012, 1);
        let (decoded, consumed) = frames.decode_scalar_fields(&source).unwrap();
        assert_eq!(consumed, 24);
        let mut editor = decoded.into_editor();
        assert_eq!(editor.encode(), source);
        editor.set_source_building(0, 0xaabb).unwrap();
        editor.set_target_building(0, 0xccdd).unwrap();
        editor.set_transport_line(0, 0x1122).unwrap();
        editor.set_transfer_size(0, 0x3344).unwrap();
        editor.set_transfer_type(0, 0x55).unwrap();
        editor.set_wait_counter(0, 0x66).unwrap();
        editor.set_block_counter(0, 0x77).unwrap();
        editor.set_citizen_unit(0, 0x8899aa).unwrap();
        editor.set_path_unit(0, 0xbbcc).unwrap();
        editor.set_path_position(0, [0xdd, 0xee]).unwrap();
        editor.set_trailing_vehicle(0, 0x1234).unwrap();
        editor.set_cargo_parent(0, 0x5678).unwrap();
        editor.set_custom_value(0, 0x9abc).unwrap();

        let expected = [
            0xaa, 0xbb, 0xcc, 0xdd, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa,
            0xbb, 0xcc, 0xdd, 0xee, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc,
        ];
        assert_eq!(editor.encode(), expected);
        assert_eq!(editor.encode().len(), source.len());
        assert!(editor.set_citizen_unit(0, 0x0100_0000).is_err());
        assert!(editor.set_source_building(1, 0).is_err());
        assert_eq!(editor.encode(), expected);

        let (decoded, consumed) = frames.decode_scalar_fields(&expected).unwrap();
        assert_eq!(consumed, expected.len());
        assert_eq!(decoded.records()[0].source_building(), 0xaabb);
        assert_eq!(decoded.records()[0].transport_line(), Some(0x1122));
        assert_eq!(decoded.records()[0].citizen_unit(), Some(0x8899aa));
        assert_eq!(decoded.records()[0].path_position(), Some([0xdd, 0xee]));
        assert_eq!(decoded.records()[0].custom_value(), Some(0x9abc));
    }

    #[test]
    fn vehicle_instance_fields_editor_rejects_fields_before_their_gates() {
        let source = scalar_record_for_version(31);
        let frames = last_frame_prefix_with_count(31, 1);
        let (decoded, consumed) = frames.decode_scalar_fields(&source).unwrap();
        assert_eq!(consumed, source.len());
        let mut editor = decoded.into_editor();
        assert!(editor.set_transport_line(0, 1).is_err());
        assert!(editor.set_citizen_unit(0, 1).is_err());
        assert!(editor.set_path_unit(0, 1).is_err());
        assert!(editor.set_path_position(0, [1, 2]).is_err());
        assert!(editor.set_trailing_vehicle(0, 1).is_err());
        assert!(editor.set_block_counter(0, 1).is_err());
        assert!(editor.set_cargo_parent(0, 1).is_err());
        assert!(editor.set_custom_value(0, 1).is_err());
        editor.set_source_building(0, 0x1234).unwrap();
        assert_eq!(
            editor.encode(),
            [0x12, 0x34, 0x03, 0x04, 0x07, 0x08, 0x09, 0x0a]
        );
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

    #[test]
    fn parked_vehicle_records_follow_active_parked_flags_and_preserve_bits() {
        let mut prefix = prefix_with_active_slots(205, 0);
        let mut parked_flags = vec![0; 32_767];
        parked_flags[2] = 1;
        parked_flags[19] = 4;
        prefix.parked_vehicle_flags = Some(parked_flags);
        let (columns, _) = prefix
            .decode_sparse_columns(&[])
            .expect("empty sparse columns with no active vehicles");
        assert_eq!(columns.active_parked_vehicle_count(), Some(2));
        let (frames, _) = columns
            .decode_last_frame_records(&[])
            .expect("there are no active vehicle records");
        let (fields, _) = frames
            .decode_scalar_fields(&[])
            .expect("there are no active scalar records");

        let first = parked_record(
            [0x3f80_0000, 0xc020_0000, 0x42a0_0000],
            [0, 0, 0, 0x3f80_0000],
            0x12_3456,
        );
        let second = parked_record(
            [0x4120_0000, 0x41a0_0000, 0x41f0_0000],
            [0x3f00_0000, 0, 0, 0x3f80_0000],
            0xab_cdef,
        );
        let mut bytes = first;
        bytes.extend_from_slice(&second);
        let (decoded, consumed) = fields
            .decode_parked_vehicle_records(&bytes)
            .expect("two parked records are complete");
        assert_eq!(decoded.records().unwrap().len(), 2);
        assert_eq!(consumed, 62);
        assert_eq!(
            decoded.records().unwrap()[0].position(),
            [0x3f80_0000, 0xc020_0000, 0x42a0_0000]
        );
        assert_eq!(
            decoded.records().unwrap()[0].rotation(),
            [0, 0, 0, 0x3f80_0000]
        );
        assert_eq!(decoded.records().unwrap()[0].owner_citizen(), 0x12_3456);
        assert_eq!(decoded.records().unwrap()[1].owner_citizen(), 0xab_cdef);
        assert_eq!(decoded.encode(), bytes);
    }

    #[test]
    fn parked_vehicle_records_respect_version_gate_and_truncation() {
        let old = instance_fields_prefix(114, None);
        let (decoded, consumed) = old
            .decode_parked_vehicle_records(&[0xaa])
            .expect("old versions have no parked record bytes");
        assert_eq!(decoded.records(), None);
        assert_eq!(consumed, 0);

        let empty = instance_fields_prefix(115, Some(0));
        let (decoded, consumed) = empty
            .decode_parked_vehicle_records(&[])
            .expect("version 115 has an empty active parked set");
        assert_eq!(decoded.records(), Some([].as_slice()));
        assert_eq!(consumed, 0);

        let one = instance_fields_prefix(115, Some(1));
        assert!(one.decode_parked_vehicle_records(&[0; 30]).is_err());
    }

    #[test]
    fn vehicle_manager_tail_counters_follow_version_gates() {
        for (version, expected_byte_count, has_flow, has_vehicle_counts) in [
            (314, 0, false, false),
            (315, 9, true, false),
            (116_012, 9, true, false),
            (116_013, 17, true, true),
        ] {
            let parked = parked_records_prefix(version);
            let bytes = [
                0, 0, 0, 1, // first traffic total
                0, 0, 0, 2, // second traffic total
                3, // final traffic byte
                0, 0, 0, 4, // temporary vehicle count
                0, 0, 0, 5, // final vehicle count
            ];
            let (decoded, consumed) = parked
                .decode_tail(&bytes)
                .expect("versioned manager tail is complete");
            assert_eq!(consumed, expected_byte_count, "version {version}");
            assert_eq!(decoded.traffic_flow_totals().is_some(), has_flow);
            assert_eq!(decoded.traffic_flow_byte().is_some(), has_flow);
            assert_eq!(
                decoded.temporary_vehicle_count().is_some(),
                has_vehicle_counts
            );
            assert_eq!(decoded.final_vehicle_count().is_some(), has_vehicle_counts);
            if has_flow {
                assert_eq!(decoded.traffic_flow_totals(), Some([1, 2]));
                assert_eq!(decoded.traffic_flow_byte(), Some(3));
            }
            if has_vehicle_counts {
                assert_eq!(decoded.temporary_vehicle_count(), Some(4));
                assert_eq!(decoded.final_vehicle_count(), Some(5));
            }
            assert_eq!(decoded.encode(), bytes[..expected_byte_count]);
        }

        assert!(parked_records_prefix(315).decode_tail(&[0; 8]).is_err());
        assert!(
            parked_records_prefix(116_013)
                .decode_tail(&[0; 16])
                .is_err()
        );
    }

    #[test]
    fn full_vehicle_manager_body_decodes_latest_counters_and_preserves_bytes() {
        let mut bytes = empty_prefix(116_013);
        bytes.extend_from_slice(&[
            0, 0, 0, 1, // first traffic total
            0, 0, 0, 2, // second traffic total
            3, // final traffic byte
            0, 0, 0, 4, // temporary vehicle count
            0, 0, 0, 5, // final vehicle count
            0, 0, // closing profiler reference
        ]);

        let decoded = VehicleManagerData::decode_body(116_013, &bytes)
            .expect("complete latest-version VehicleManager body");
        assert_eq!(decoded.prefix().active_parked_vehicle_count(), Some(0));
        assert_eq!(decoded.sparse_columns().active_vehicle_count(), 0);
        assert_eq!(decoded.last_frame_records().records(), []);
        assert_eq!(decoded.instance_fields().records(), []);
        assert_eq!(decoded.parked_records().records(), Some([].as_slice()));
        assert_eq!(decoded.tail().traffic_flow_totals(), Some([1, 2]));
        assert_eq!(decoded.tail().traffic_flow_byte(), Some(3));
        assert_eq!(decoded.tail().temporary_vehicle_count(), Some(4));
        assert_eq!(decoded.tail().final_vehicle_count(), Some(5));
        assert_eq!(decoded.encode(), bytes);

        let mut malformed = bytes.clone();
        let closing_marker_offset = malformed.len() - 2;
        malformed[closing_marker_offset] = 0xfe;
        assert!(VehicleManagerData::decode_body(116_013, &malformed).is_err());
        assert!(VehicleManagerData::decode_body(116_013, &bytes[..bytes.len() - 1]).is_err());
    }
}
