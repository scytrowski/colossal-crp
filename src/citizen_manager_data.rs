use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    EncodedUIntArrayReader, read_encoded_byte_array, read_encoded_uint_array,
    read_encoded_ushort_array,
};

const CITIZEN_FLAG_COUNT: usize = 1_048_575;
const BYTE_CITIZEN_FLAGS_VERSION: u32 = 30;
const LEGACY_CITIZEN_COLUMNS_VERSION: u32 = 23;
const CITIZEN_HEALTH_VERSION: u32 = 65;
const CITIZEN_AGE_VERSION: u32 = 157;
const CITIZEN_AGE_USHORT_VERSION: u32 = 113_007;
const CITIZEN_FAMILY_VERSION: u32 = 190;
const CITIZEN_JOB_TITLE_VERSION: u32 = 112_007;
const CITIZEN_TOURIST_TYPE_VERSION: u32 = 117_002;
const CITIZEN_UNIT_FLAG_COUNT: usize = 524_287;
const CITIZEN_UNIT_MASK_VERSION: u32 = 69;
const CITIZEN_INSTANCE_FLAGS_VERSION: u32 = 44;
const FULL_CITIZEN_INSTANCE_FLAGS_VERSION: u32 = 204;
const LEGACY_CITIZEN_INSTANCE_FLAG_COUNT: usize = 32_767;
const FULL_CITIZEN_INSTANCE_FLAG_COUNT: usize = 65_535;

/// The decoded flag and sparse-column prefix of `CitizenManager+Data`.
///
/// The prefix ends before the fixed-capacity citizen-unit and citizen-instance
/// arrays. Those later fields remain in the serialized manager graph and are
/// not decoded here. The exact consumed source bytes are retained for unchanged
/// writing. Reader gates come from `CitizenManager+Data.Deserialize` in the
/// supplied Assembly-CSharp.dll; the local map corpus has no active citizens,
/// so nonzero values have synthetic test coverage only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitizenManagerDataPrefix {
    format_version: u32,
    citizen_flags: Vec<u32>,
    legacy_columns: Option<Vec<Vec<u8>>>,
    health: Option<Vec<u8>>,
    wellbeing: Option<Vec<u8>>,
    age: Option<Vec<u16>>,
    family: Option<Vec<u8>>,
    job_title: Option<Vec<u8>>,
    tourist_type: Option<Vec<u8>>,
    serialized_prefix: Vec<u8>,
}

impl CitizenManagerDataPrefix {
    /// Decodes citizen flags and the versioned sparse citizen columns at the
    /// start of the manager body, returning the byte count before unit data.
    ///
    /// Versions below 30 use byte flags; version 30 and later use unsigned
    /// 32-bit flags. Sparse values are stored in ascending citizen-index order
    /// for each nonzero flag. Truncated or malformed arrays return an error.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        Self::decode_prefix_with_count(format_version, CITIZEN_FLAG_COUNT, bytes)
    }

    fn decode_prefix_with_count(
        format_version: u32,
        flag_count: usize,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let citizen_flags = if format_version < BYTE_CITIZEN_FLAGS_VERSION {
            read_encoded_byte_array(&mut reader, flag_count)?
                .into_iter()
                .map(u32::from)
                .collect()
        } else {
            EncodedUIntArrayReader::default().read_array(&mut reader, flag_count)?
        };
        let active_count = citizen_flags.iter().filter(|flags| **flags != 0).count();

        let legacy_columns = if format_version < LEGACY_CITIZEN_COLUMNS_VERSION {
            Some(vec![read_encoded_byte_array(&mut reader, active_count)?])
        } else if format_version < BYTE_CITIZEN_FLAGS_VERSION {
            let mut columns = Vec::with_capacity(4);
            for _ in 0..4 {
                columns.push(read_encoded_byte_array(&mut reader, active_count)?);
            }
            Some(columns)
        } else {
            None
        };

        let (health, wellbeing) = if format_version >= CITIZEN_HEALTH_VERSION {
            (
                Some(read_encoded_byte_array(&mut reader, active_count)?),
                Some(read_encoded_byte_array(&mut reader, active_count)?),
            )
        } else {
            (None, None)
        };
        let age = if format_version >= CITIZEN_AGE_USHORT_VERSION {
            Some(read_encoded_ushort_array(&mut reader, active_count)?)
        } else if format_version >= CITIZEN_AGE_VERSION {
            Some(
                read_encoded_byte_array(&mut reader, active_count)?
                    .into_iter()
                    .map(u16::from)
                    .collect(),
            )
        } else {
            None
        };
        let family = if format_version >= CITIZEN_FAMILY_VERSION {
            Some(read_encoded_byte_array(&mut reader, active_count)?)
        } else {
            None
        };
        let job_title = if format_version >= CITIZEN_JOB_TITLE_VERSION {
            Some(read_encoded_byte_array(&mut reader, active_count)?)
        } else {
            None
        };
        let tourist_type = if format_version >= CITIZEN_TOURIST_TYPE_VERSION {
            Some(read_encoded_byte_array(&mut reader, active_count)?)
        } else {
            None
        };

        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                citizen_flags,
                legacy_columns,
                health,
                wellbeing,
                age,
                family,
                job_title,
                tourist_type,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version used for the reader gates.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns flags for citizen slots 1 through 1,048,575.
    ///
    /// Slots with a nonzero value have sparse-column entries. The vector's
    /// index is the serialized array index minus one.
    pub fn citizen_flags(&self) -> &[u32] {
        &self.citizen_flags
    }

    /// Returns one column before version 23 or four columns in versions 23–29.
    /// Their game meanings are not established by the inspected reader.
    pub fn legacy_columns(&self) -> Option<&[Vec<u8>]> {
        self.legacy_columns.as_deref()
    }

    /// Returns the sparse health values from version 65.
    pub fn health(&self) -> Option<&[u8]> {
        self.health.as_deref()
    }

    /// Returns the sparse wellbeing values from version 65.
    pub fn wellbeing(&self) -> Option<&[u8]> {
        self.wellbeing.as_deref()
    }

    /// Returns sparse age values from version 157, widened to `u16` for a
    /// consistent API. The serialized width changes at version 113007.
    pub fn age(&self) -> Option<&[u16]> {
        self.age.as_deref()
    }

    /// Returns the sparse family values from version 190.
    pub fn family(&self) -> Option<&[u8]> {
        self.family.as_deref()
    }

    /// Returns sparse job-title indices from version 112007.
    pub fn job_title(&self) -> Option<&[u8]> {
        self.job_title.as_deref()
    }

    /// Returns sparse tourist types from version 117002.
    pub fn tourist_type(&self) -> Option<&[u8]> {
        self.tourist_type.as_deref()
    }

    /// Returns the exact source bytes consumed by this prefix decoder.
    pub fn original_prefix(&self) -> &[u8] {
        &self.serialized_prefix
    }

    /// Reproduces this prefix unchanged. Sparse values are not editable yet.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

/// One active `CitizenUnit` and its six optional 24-bit references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitizenUnitRecord {
    index: u32,
    flags: u16,
    reference_mask: u8,
    citizens: [Option<u32>; 5],
    next_unit: Option<u32>,
    goods: u16,
}

impl CitizenUnitRecord {
    /// Returns the one-based index in the fixed citizen-unit buffer.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Returns the serialized unit flags.
    pub fn flags(&self) -> u16 {
        self.flags
    }

    /// Returns the raw mask that selects the serialized references.
    pub fn reference_mask(&self) -> u8 {
        self.reference_mask
    }

    /// Returns the five optional citizen references in serialized slot order.
    pub fn citizens(&self) -> &[Option<u32>; 5] {
        &self.citizens
    }

    /// Returns the optional next-unit reference.
    pub fn next_unit(&self) -> Option<u32> {
        self.next_unit
    }

    /// Returns the goods value from the trailing sparse UShort column.
    pub fn goods(&self) -> u16 {
        self.goods
    }
}

/// The version 69+ active-unit records in `CitizenManager+Data`.
///
/// Unit flags select active slots. Each active slot has a one-byte reference
/// mask followed by zero to six 24-bit references, then a sparse goods value.
/// The exact source bytes are retained for unchanged writing. The supplied
/// map corpus contains no active units; nonzero records have synthetic
/// expected-byte tests only. Pre-69 reference selection from the flag bits is
/// not exposed by this reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitizenUnitDataPrefix {
    format_version: u32,
    unit_flags: Vec<u16>,
    active_units: Vec<CitizenUnitRecord>,
    serialized_prefix: Vec<u8>,
}

impl CitizenUnitDataPrefix {
    /// Decodes the fixed unit-flag array, active reference records, and goods
    /// column, returning the consumed byte count before instance data.
    ///
    /// Supports inner versions 69 and later. Earlier versions select
    /// references directly from bits in the flags and are rejected because
    /// that historical mapping is not modeled here. Truncated arrays and
    /// references return errors.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        Self::decode_prefix_with_count(format_version, CITIZEN_UNIT_FLAG_COUNT, bytes)
    }

    fn decode_prefix_with_count(
        format_version: u32,
        flag_count: usize,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < CITIZEN_UNIT_MASK_VERSION {
            return Err(invalid(
                0,
                format!(
                    "unsupported pre-69 CitizenUnit reference layout at version {format_version}"
                ),
            ));
        }

        let mut reader = Reader::new(bytes);
        let unit_flags = read_encoded_ushort_array(&mut reader, flag_count)?;
        let mut active_units = Vec::new();
        for (offset, flags) in unit_flags.iter().copied().enumerate() {
            if flags == 0 {
                continue;
            }
            let index = u32::try_from(offset + 1)
                .map_err(|_| CrpError::ArithmeticOverflow("CitizenUnit index"))?;
            let reference_mask = reader.read_u8()?;
            let mut citizens = [None; 5];
            for (reference_index, citizen) in citizens.iter_mut().enumerate() {
                if reference_mask & (1 << reference_index) != 0 {
                    *citizen = Some(reader.read_u24_be()?);
                }
            }
            let next_unit = if reference_mask & (1 << 5) != 0 {
                Some(reader.read_u24_be()?)
            } else {
                None
            };
            active_units.push(CitizenUnitRecord {
                index,
                flags,
                reference_mask,
                citizens,
                next_unit,
                goods: 0,
            });
        }

        let goods = read_encoded_ushort_array(&mut reader, active_units.len())?;
        for (unit, goods) in active_units.iter_mut().zip(goods) {
            unit.goods = goods;
        }

        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                unit_flags,
                active_units,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns all 524,287 serialized flags for one-based unit slots.
    pub fn unit_flags(&self) -> &[u16] {
        &self.unit_flags
    }

    /// Returns active units in ascending buffer-index order.
    pub fn active_units(&self) -> &[CitizenUnitRecord] {
        &self.active_units
    }

    /// Returns the exact source bytes consumed by this prefix decoder.
    pub fn original_prefix(&self) -> &[u8] {
        &self.serialized_prefix
    }

    /// Reproduces this prefix unchanged. Unit records are not editable yet.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

/// One active CitizenManager instance and its BuildingInfo prefab-table index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CitizenInstanceSlot {
    index: u32,
    flags: u32,
    prefab_index: u16,
}

impl CitizenInstanceSlot {
    /// Returns the one-based index in the citizen-instance buffer.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Returns the serialized instance flags.
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the index into [`CitizenInstanceDataPrefix::prefab_names`].
    pub fn prefab_index(&self) -> u16 {
        self.prefab_index
    }
}

/// The flags and BuildingInfo prefab scope preceding active citizen instances.
///
/// This prefix stops before per-instance position, path, citizen, and building
/// fields. The exact source bytes are retained for unchanged writing. The
/// local map corpus contains active instances, but the Rust decoder's active
/// slot and prefab association tests use synthetic expected bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitizenInstanceDataPrefix {
    format_version: u32,
    instance_flags: Option<Vec<u32>>,
    active_instances: Vec<CitizenInstanceSlot>,
    prefab_names: Option<Vec<Option<String>>>,
    serialized_prefix: Vec<u8>,
}

impl CitizenInstanceDataPrefix {
    /// Decodes the versioned instance-flag array and BuildingInfo prefab table,
    /// returning the consumed byte count before active instance fields.
    ///
    /// Before inner version 44 these fields are absent. Versions 44–203 read
    /// 32,767 flags; version 204 and later read 65,535. Truncated arrays,
    /// indices, or names return errors.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let flag_count = if format_version < CITIZEN_INSTANCE_FLAGS_VERSION {
            None
        } else if format_version < FULL_CITIZEN_INSTANCE_FLAGS_VERSION {
            Some(LEGACY_CITIZEN_INSTANCE_FLAG_COUNT)
        } else {
            Some(FULL_CITIZEN_INSTANCE_FLAG_COUNT)
        };
        Self::decode_prefix_with_count(format_version, flag_count, bytes)
    }

    fn decode_prefix_with_count(
        format_version: u32,
        flag_count: Option<usize>,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let Some(flag_count) = flag_count else {
            return Ok((
                Self {
                    format_version,
                    instance_flags: None,
                    active_instances: Vec::new(),
                    prefab_names: None,
                    serialized_prefix: Vec::new(),
                },
                0,
            ));
        };

        let instance_flags = read_encoded_uint_array(&mut reader, flag_count)?;
        let active_count = instance_flags.iter().filter(|flags| **flags != 0).count();
        let prefab_indices = read_encoded_ushort_array(&mut reader, active_count)?;
        let name_count = usize::from(reader.read_u16_be()?);
        let mut prefab_names = Vec::with_capacity(name_count);
        for _ in 0..name_count {
            prefab_names.push(reader.read_unique_string()?);
        }

        let mut active_instances = Vec::with_capacity(active_count);
        let mut prefab_indices = prefab_indices.into_iter();
        for (offset, flags) in instance_flags.iter().copied().enumerate() {
            if flags == 0 {
                continue;
            }
            let index = u32::try_from(offset + 1)
                .map_err(|_| CrpError::ArithmeticOverflow("CitizenInstance index"))?;
            let prefab_index = prefab_indices.next().ok_or_else(|| {
                invalid(reader.offset(), "CitizenInstance prefab index is missing")
            })?;
            active_instances.push(CitizenInstanceSlot {
                index,
                flags,
                prefab_index,
            });
        }

        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                instance_flags: Some(instance_flags),
                active_instances,
                prefab_names: Some(prefab_names),
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the version-sized flag array, absent before version 44.
    pub fn instance_flags(&self) -> Option<&[u32]> {
        self.instance_flags.as_deref()
    }

    /// Returns active slots in ascending buffer-index order.
    pub fn active_instances(&self) -> &[CitizenInstanceSlot] {
        &self.active_instances
    }

    /// Returns the BuildingInfo prefab names, preserving null entries.
    pub fn prefab_names(&self) -> Option<&[Option<String>]> {
        self.prefab_names.as_deref()
    }

    /// Returns the exact source bytes consumed by this prefix decoder.
    pub fn original_prefix(&self) -> &[u8] {
        &self.serialized_prefix
    }

    /// Reproduces this prefix unchanged. Active instance fields are not edited.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "CitizenManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{CitizenInstanceDataPrefix, CitizenManagerDataPrefix, CitizenUnitDataPrefix};

    fn byte_array(values: &[u8]) -> Vec<u8> {
        let mut bytes = vec![(values.len() - 1) as u8];
        bytes.extend_from_slice(values);
        bytes
    }

    fn uint_array(values: &[u32]) -> Vec<u8> {
        let mut bytes = vec![(values.len() - 1) as u8];
        for value in values {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes
    }

    fn ushort_array(values: &[u16]) -> Vec<u8> {
        let mut bytes = vec![(values.len() - 1) as u8];
        for value in values {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes
    }

    fn append_zero_uint_array(bytes: &mut Vec<u8>, count: usize) {
        for _ in 0..count / 128 {
            bytes.push(0xff);
        }
        let remainder = count % 128;
        if remainder > 0 {
            bytes.push((0x7f + remainder) as u8);
        }
    }

    #[test]
    fn decodes_flag_width_and_legacy_sparse_column_gates() {
        let old_flags = [2, 1, 0, 0];
        let mut v22 = old_flags.to_vec();
        v22.extend_from_slice(&byte_array(&[0x55]));
        v22.push(0xaa); // first byte after the decoded prefix
        let (old, consumed) = CitizenManagerDataPrefix::decode_prefix_with_count(22, 3, &v22)
            .expect("v22 citizen prefix");
        assert_eq!(old.citizen_flags(), [1, 0, 0]);
        assert_eq!(old.legacy_columns(), Some([vec![0x55]].as_slice()));
        assert_eq!(consumed, v22.len() - 1);
        assert_eq!(old.encode(), v22[..consumed]);

        let mut v23 = old_flags.to_vec();
        for value in [0x11, 0x22, 0x33, 0x44] {
            v23.extend_from_slice(&byte_array(&[value]));
        }
        let (middle, middle_consumed) =
            CitizenManagerDataPrefix::decode_prefix_with_count(23, 3, &v23)
                .expect("v23 citizen prefix");
        assert_eq!(
            middle.legacy_columns(),
            Some([vec![0x11], vec![0x22], vec![0x33], vec![0x44]].as_slice())
        );
        assert_eq!(middle_consumed, v23.len());

        let v30 = uint_array(&[1, 0, 0]);
        let (modern, modern_consumed) =
            CitizenManagerDataPrefix::decode_prefix_with_count(30, 3, &v30)
                .expect("v30 citizen prefix");
        assert_eq!(modern.citizen_flags(), [1, 0, 0]);
        assert_eq!(modern.legacy_columns(), None);
        assert_eq!(modern_consumed, v30.len());
    }

    #[test]
    fn decodes_active_citizen_columns_across_age_and_later_gates() {
        for (version, age_bytes, expected_age, has_tourist_type) in [
            (113_006, vec![0, 0x7f], vec![127], false),
            (113_007, vec![0, 0x01, 0x23], vec![0x0123], false),
            (117_002, vec![0, 0, 0x7f], vec![127], true),
        ] {
            let mut bytes = uint_array(&[1, 0, 0]);
            bytes.extend_from_slice(&byte_array(&[0x21])); // health
            bytes.extend_from_slice(&byte_array(&[0x32])); // wellbeing
            bytes.extend_from_slice(&age_bytes);
            bytes.extend_from_slice(&byte_array(&[0x43])); // family
            bytes.extend_from_slice(&byte_array(&[0x54])); // job title
            if has_tourist_type {
                bytes.extend_from_slice(&byte_array(&[0x65]));
            }
            bytes.extend_from_slice(&[0xfe, 0xed]); // next manager data

            let (decoded, consumed) =
                CitizenManagerDataPrefix::decode_prefix_with_count(version, 3, &bytes)
                    .expect("active citizen prefix");
            assert_eq!(decoded.citizen_flags(), [1, 0, 0]);
            assert_eq!(decoded.health(), Some([0x21].as_slice()));
            assert_eq!(decoded.wellbeing(), Some([0x32].as_slice()));
            assert_eq!(decoded.age(), Some(expected_age.as_slice()));
            assert_eq!(decoded.family(), Some([0x43].as_slice()));
            assert_eq!(decoded.job_title(), Some([0x54].as_slice()));
            assert_eq!(decoded.tourist_type().is_some(), has_tourist_type);
            assert_eq!(decoded.encode(), bytes[..consumed]);
            assert_eq!(bytes[consumed..], [0xfe, 0xed]);
        }
    }

    #[test]
    fn rejects_truncated_flag_and_sparse_columns() {
        assert!(CitizenManagerDataPrefix::decode_prefix_with_count(30, 3, &[0, 0, 0]).is_err());
        let mut missing_sparse_value = uint_array(&[1, 0, 0]);
        missing_sparse_value.extend_from_slice(&[0]); // health array advertises one byte
        assert!(
            CitizenManagerDataPrefix::decode_prefix_with_count(65, 3, &missing_sparse_value)
                .is_err()
        );
    }

    #[test]
    fn decodes_masked_citizen_unit_references_and_goods() {
        let mut bytes = ushort_array(&[1, 0, 2]);
        bytes.extend_from_slice(&[
            0b0011_1111, // slot 1: all six references are present
            0x12,
            0x34,
            0x56,
            0x23,
            0x45,
            0x67,
            0x34,
            0x56,
            0x78,
            0x45,
            0x67,
            0x89,
            0x56,
            0x78,
            0x9a,
            0xab,
            0xcd,
            0xef,
            0b0000_0010,
            0x01,
            0x02,
            0x03, // slot 3
        ]);
        bytes.extend_from_slice(&ushort_array(&[0x1111, 0x2222]));
        bytes.extend_from_slice(&[0xfe, 0xed]);

        let (decoded, consumed) = CitizenUnitDataPrefix::decode_prefix_with_count(69, 3, &bytes)
            .expect("masked CitizenUnit records");
        assert_eq!(decoded.unit_flags(), [1, 0, 2]);
        assert_eq!(decoded.active_units().len(), 2);
        let first = &decoded.active_units()[0];
        assert_eq!(first.index(), 1);
        assert_eq!(first.flags(), 1);
        assert_eq!(first.reference_mask(), 0b0011_1111);
        assert_eq!(
            first.citizens(),
            &[
                Some(0x123456),
                Some(0x234567),
                Some(0x345678),
                Some(0x456789),
                Some(0x56789a),
            ]
        );
        assert_eq!(first.next_unit(), Some(0xabcdef));
        assert_eq!(first.goods(), 0x1111);
        let second = &decoded.active_units()[1];
        assert_eq!(second.index(), 3);
        assert_eq!(second.citizens(), &[None, Some(0x010203), None, None, None]);
        assert_eq!(second.next_unit(), None);
        assert_eq!(second.goods(), 0x2222);
        assert_eq!(decoded.encode(), bytes[..consumed]);
        assert_eq!(bytes[consumed..], [0xfe, 0xed]);
    }

    #[test]
    fn unit_records_validate_version_masks_and_goods_boundaries() {
        assert!(CitizenUnitDataPrefix::decode_prefix_with_count(68, 1, &[0x40]).is_err());
        assert!(CitizenUnitDataPrefix::decode_prefix_with_count(69, 1, &[0, 1]).is_err());
        assert!(
            CitizenUnitDataPrefix::decode_prefix_with_count(69, 1, &[0, 1, 0b0000_0001]).is_err()
        );

        let mut missing_goods = ushort_array(&[1]);
        missing_goods.push(0); // active record with empty mask
        assert!(CitizenUnitDataPrefix::decode_prefix_with_count(69, 1, &missing_goods).is_err());

        let empty = [0x82]; // three zero flags, no active records, no goods bytes
        let (decoded, consumed) = CitizenUnitDataPrefix::decode_prefix_with_count(69, 3, &empty)
            .expect("empty CitizenUnit prefix");
        assert!(decoded.active_units().is_empty());
        assert_eq!(consumed, empty.len());
    }

    #[test]
    fn instance_flag_and_prefab_scope_obey_version_gates() {
        let before =
            CitizenInstanceDataPrefix::decode_prefix(43, &[0xaa]).expect("pre-44 instance layout");
        assert_eq!(before.0.instance_flags(), None);
        assert_eq!(before.0.prefab_names(), None);
        assert_eq!(before.1, 0);

        for (version, expected_count) in [(203, 32_767), (204, 65_535)] {
            let mut bytes = Vec::new();
            append_zero_uint_array(&mut bytes, expected_count);
            bytes.extend_from_slice(&[0, 0]); // no BuildingInfo prefab names
            let (decoded, consumed) = CitizenInstanceDataPrefix::decode_prefix(version, &bytes)
                .expect("empty instance and prefab tables");
            assert_eq!(decoded.instance_flags().unwrap().len(), expected_count);
            assert!(decoded.active_instances().is_empty());
            assert_eq!(decoded.prefab_names(), Some([].as_slice()));
            assert_eq!(decoded.encode(), bytes);
            assert_eq!(consumed, bytes.len());
        }
    }

    #[test]
    fn active_instance_prefab_indices_follow_flag_order() {
        let mut bytes = uint_array(&[0x41, 0, 0x07]);
        bytes.extend_from_slice(&ushort_array(&[1, 0]));
        bytes.extend_from_slice(&[0, 2, 0xff, 0x80, 0, 1, 0xc1]);
        bytes.extend_from_slice(&[0xaa, 0xbb]);

        let (decoded, consumed) =
            CitizenInstanceDataPrefix::decode_prefix_with_count(44, Some(3), &bytes)
                .expect("active instance prefab table");
        assert_eq!(decoded.instance_flags(), Some([0x41, 0, 0x07].as_slice()));
        assert_eq!(decoded.active_instances().len(), 2);
        assert_eq!(decoded.active_instances()[0].index(), 1);
        assert_eq!(decoded.active_instances()[0].flags(), 0x41);
        assert_eq!(decoded.active_instances()[0].prefab_index(), 1);
        assert_eq!(decoded.active_instances()[1].index(), 3);
        assert_eq!(decoded.active_instances()[1].flags(), 0x07);
        assert_eq!(decoded.active_instances()[1].prefab_index(), 0);
        assert_eq!(
            decoded.prefab_names(),
            Some([None, Some("A".to_owned())].as_slice())
        );
        assert_eq!(decoded.encode(), bytes[..consumed]);
        assert_eq!(bytes[consumed..], [0xaa, 0xbb]);
    }

    #[test]
    fn instance_prefix_rejects_truncated_flags_indices_and_names() {
        assert!(CitizenInstanceDataPrefix::decode_prefix_with_count(44, Some(3), &[0, 0]).is_err());

        let active_flags = uint_array(&[1, 0, 0]);
        let mut missing_index = active_flags.clone();
        missing_index.extend_from_slice(&[0, 0]); // name table only; active index is missing
        assert!(
            CitizenInstanceDataPrefix::decode_prefix_with_count(44, Some(3), &missing_index)
                .is_err()
        );

        let mut missing_name = active_flags;
        missing_name.extend_from_slice(&[0, 0, 1, 0, 1]); // index 1 and one name advertised
        assert!(
            CitizenInstanceDataPrefix::decode_prefix_with_count(44, Some(3), &missing_name)
                .is_err()
        );
    }
}
