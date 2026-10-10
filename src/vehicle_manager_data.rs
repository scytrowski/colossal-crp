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
/// The original consumed bytes are retained. This type does not interpret or
/// rewrite the per-vehicle records that follow these columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleManagerSparseFieldsPrefix {
    format_version: u32,
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
    use super::VehicleManagerDataPrefix;

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
}
