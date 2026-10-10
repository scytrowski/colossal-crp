use std::ops::Range;

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
const CITIZEN_INSTANCE_VELOCITY_VERSION: u32 = 46;
const CITIZEN_INSTANCE_TARGET_POSITION_VERSION: u32 = 47;
const CITIZEN_INSTANCE_WAIT_COUNTER_VERSION: u32 = 86;
const CITIZEN_INSTANCE_TARGET_POSITION_VECTOR4_VERSION: u32 = 195;
const CITIZEN_INSTANCE_TARGET_POSITION_VECTOR3_END_VERSION: u32 =
    CITIZEN_INSTANCE_TARGET_POSITION_VECTOR4_VERSION - 1;
const CITIZEN_INSTANCE_TARGET_DIRECTION_VERSION: u32 = 200;
const CITIZEN_INSTANCE_TARGET_SEED_VERSION: u32 = 201;
const CITIZEN_INSTANCE_COLOR_VERSION: u32 = 247;
const CITIZEN_INSTANCE_PERFORMER_VERSION: u32 = 121_028;
const CITIZEN_INSTANCE_COMPATIBILITY_END_VERSION: u32 = 121_029;
const CITIZEN_INSTANCE_SOURCE_BUILDING_VERSION: u32 = 180;
const CITIZEN_INSTANCE_COLOR_FLAG: u32 = 1 << 24;

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

/// The serialized representation of an active instance's target position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CitizenInstanceTargetPosition {
    /// Before version 47, the game uses the decoded position as its target.
    FollowsPosition,
    /// Versions 47–194 serialize three IEEE-754 components.
    Vector3([f32; 3]),
    /// Version 195 and later serialize four IEEE-754 components.
    Vector4([f32; 4]),
}

/// One active `CitizenInstance` record decoded in manager order.
#[derive(Debug, Clone, PartialEq)]
pub struct CitizenInstanceRecord {
    index: u32,
    flags: u32,
    prefab_index: u16,
    velocity: Option<[f32; 3]>,
    position: [f32; 3],
    target_position: CitizenInstanceTargetPosition,
    target_direction: Option<[f32; 2]>,
    color_rgb: Option<[u8; 3]>,
    citizen: u32,
    path: Option<u32>,
    path_position_index: Option<u8>,
    last_path_offset: Option<u8>,
    wait_counter: Option<u8>,
    target_seed: Option<u8>,
    performer_index: Option<u8>,
    racer_index: Option<u8>,
    compatibility_value: Option<u16>,
    source_building: Option<u16>,
    target_building: u16,
    position_offset: usize,
    target_position_offset: Option<usize>,
    original_record_start: usize,
    original_record_end: usize,
}

impl CitizenInstanceRecord {
    /// Returns the one-based index in the fixed CitizenInstance buffer.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Returns the serialized active-slot flags.
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the index into the BuildingInfo prefab table.
    pub fn prefab_index(&self) -> u16 {
        self.prefab_index
    }

    /// Returns velocity when the version 46 field is serialized.
    pub fn velocity(&self) -> Option<[f32; 3]> {
        self.velocity
    }

    /// Returns the serialized position vector.
    pub fn position(&self) -> [f32; 3] {
        self.position
    }

    /// Returns the versioned target-position representation.
    pub fn target_position(&self) -> CitizenInstanceTargetPosition {
        self.target_position
    }

    /// Returns the target direction from version 200.
    pub fn target_direction(&self) -> Option<[f32; 2]> {
        self.target_direction
    }

    /// Returns the conditional RGB bytes from version 247.
    ///
    /// The field is present only when flag bit 24 is set. The game sets alpha
    /// to 255 when reading these three bytes.
    pub fn color_rgb(&self) -> Option<[u8; 3]> {
        self.color_rgb
    }

    /// Returns the 24-bit citizen reference.
    pub fn citizen(&self) -> u32 {
        self.citizen
    }

    /// Returns the path reference and position fields from version 46.
    pub fn path(&self) -> Option<u32> {
        self.path
    }

    /// Returns the path-position index from version 46.
    pub fn path_position_index(&self) -> Option<u8> {
        self.path_position_index
    }

    /// Returns the last path offset from version 46.
    pub fn last_path_offset(&self) -> Option<u8> {
        self.last_path_offset
    }

    /// Returns the wait counter from version 86.
    pub fn wait_counter(&self) -> Option<u8> {
        self.wait_counter
    }

    /// Returns the target seed from version 201.
    pub fn target_seed(&self) -> Option<u8> {
        self.target_seed
    }

    /// Returns performer and racer indices from version 121028.
    pub fn performer_index(&self) -> Option<u8> {
        self.performer_index
    }

    /// Returns racer index from version 121028.
    pub fn racer_index(&self) -> Option<u8> {
        self.racer_index
    }

    /// Returns the extra `u16` consumed only at version 121028.
    ///
    /// Its meaning is not established. The original bytes are retained even
    /// though the supplied game's `Deserialize` method discards the value.
    pub fn compatibility_value(&self) -> Option<u16> {
        self.compatibility_value
    }

    /// Returns the source-building index from version 180.
    pub fn source_building(&self) -> Option<u16> {
        self.source_building
    }

    /// Returns the target-building index.
    pub fn target_building(&self) -> u16 {
        self.target_building
    }
}

/// The flags and BuildingInfo prefab scope preceding active citizen instances.
///
/// This prefix stops before per-instance position, path, citizen, and building
/// fields. The exact source bytes are retained for unchanged writing. Active
/// instance records are decoded by [`CitizenManagerData`].
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

const CITIZEN_MANAGER_END_REFERENCE_LEN: usize = 2;
const OLDEST_RESIDENTS_VERSION: u32 = 169;
const FULLY_EDUCATED_RESIDENTS_VERSION: u32 = 179;

/// The decoded CitizenManager prefixes, instance records, and tail counters.
///
/// Citizen flags and sparse columns, version 69+ unit records, and the
/// versioned instance flags/prefab scope and active records are decoded. The
/// full instance byte section, two-byte profiler reference, and counters are
/// retained. `encode` reproduces the complete bounded manager body exactly
/// until a field is explicitly edited.
#[derive(Debug, Clone, PartialEq)]
pub struct CitizenManagerData {
    format_version: u32,
    citizen_prefix: CitizenManagerDataPrefix,
    unit_prefix: CitizenUnitDataPrefix,
    instance_prefix: CitizenInstanceDataPrefix,
    instance_fields: Range<usize>,
    active_instances: Vec<CitizenInstanceRecord>,
    instance_field_suffix: Range<usize>,
    oldest_original_residents: Option<[i16; 2]>,
    fully_educated_original_residents: Option<i32>,
    end_profiler_reference: [u8; 2],
    original_body: Vec<u8>,
}

impl CitizenManagerData {
    /// Decodes the known CitizenManager sections from one bounded manager body.
    ///
    /// Supports the version 69+ unit-reference layout and decodes the
    /// version-gated active CitizenInstance fields. Truncated prefixes,
    /// records, or tails return errors. Any bytes after the last known record
    /// remain available through [`Self::unparsed_instance_suffix`].
    pub fn decode(format_version: u32, bytes: &[u8]) -> Result<Self, CrpError> {
        let (citizen_prefix, citizen_len) =
            CitizenManagerDataPrefix::decode_prefix(format_version, bytes)?;
        let unit_bytes = bytes
            .get(citizen_len..)
            .ok_or_else(|| invalid(citizen_len, "CitizenManager citizen prefix exceeds body"))?;
        let (unit_prefix, unit_len) =
            CitizenUnitDataPrefix::decode_prefix(format_version, unit_bytes)?;
        let instance_start =
            citizen_len
                .checked_add(unit_len)
                .ok_or(CrpError::ArithmeticOverflow(
                    "CitizenManager instance start",
                ))?;
        let instance_bytes = bytes
            .get(instance_start..)
            .ok_or_else(|| invalid(instance_start, "CitizenManager unit prefix exceeds body"))?;
        let (instance_prefix, instance_prefix_len) =
            CitizenInstanceDataPrefix::decode_prefix(format_version, instance_bytes)?;
        let instance_fields_start =
            instance_start
                .checked_add(instance_prefix_len)
                .ok_or(CrpError::ArithmeticOverflow(
                    "CitizenManager instance fields start",
                ))?;

        let oldest_counter_len: usize = if format_version >= OLDEST_RESIDENTS_VERSION {
            4
        } else {
            0
        };
        let educated_counter_len: usize = if format_version >= FULLY_EDUCATED_RESIDENTS_VERSION {
            4
        } else {
            0
        };
        let trailing_counter_len = oldest_counter_len.checked_add(educated_counter_len).ok_or(
            CrpError::ArithmeticOverflow("CitizenManager counter length"),
        )?;
        let end_reference_start = bytes
            .len()
            .checked_sub(CITIZEN_MANAGER_END_REFERENCE_LEN)
            .ok_or_else(|| {
                invalid(
                    bytes.len(),
                    "CitizenManager closing profiler reference is truncated",
                )
            })?;
        let counter_start = end_reference_start
            .checked_sub(trailing_counter_len)
            .ok_or_else(|| {
                invalid(
                    end_reference_start,
                    "CitizenManager trailing counters are truncated",
                )
            })?;
        if instance_fields_start > counter_start {
            return Err(invalid(
                instance_fields_start,
                "CitizenManager prefixes overlap the trailing counters",
            ));
        }

        let instance_fields = bytes
            .get(instance_fields_start..counter_start)
            .ok_or_else(|| {
                invalid(
                    instance_fields_start,
                    "CitizenManager instance field range is invalid",
                )
            })?;
        let mut instance_reader = Reader::new(instance_fields);
        let mut active_instances = Vec::with_capacity(instance_prefix.active_instances().len());
        for slot in instance_prefix.active_instances() {
            let record_start = instance_reader.offset();
            let mut record =
                decode_citizen_instance_record(format_version, *slot, &mut instance_reader)?;
            record.original_record_start = instance_fields_start.checked_add(record_start).ok_or(
                CrpError::ArithmeticOverflow("CitizenManager instance record start"),
            )?;
            record.original_record_end = instance_fields_start
                .checked_add(instance_reader.offset())
                .ok_or(CrpError::ArithmeticOverflow(
                    "CitizenManager instance record end",
                ))?;
            record.position_offset = instance_fields_start
                .checked_add(record.position_offset)
                .ok_or(CrpError::ArithmeticOverflow(
                    "CitizenManager instance position offset",
                ))?;
            record.target_position_offset = record
                .target_position_offset
                .map(|offset| {
                    instance_fields_start
                        .checked_add(offset)
                        .ok_or(CrpError::ArithmeticOverflow(
                            "CitizenManager instance target-position offset",
                        ))
                })
                .transpose()?;
            active_instances.push(record);
        }
        let instance_field_suffix_start = instance_fields_start
            .checked_add(instance_reader.offset())
            .ok_or(CrpError::ArithmeticOverflow(
                "CitizenManager unparsed instance suffix",
            ))?;

        let mut tail_reader = Reader::new(
            bytes
                .get(counter_start..end_reference_start)
                .ok_or_else(|| invalid(counter_start, "CitizenManager counter range is invalid"))?,
        );
        let oldest_original_residents = if format_version >= OLDEST_RESIDENTS_VERSION {
            Some([tail_reader.read_i16_be()?, tail_reader.read_i16_be()?])
        } else {
            None
        };
        let fully_educated_original_residents =
            if format_version >= FULLY_EDUCATED_RESIDENTS_VERSION {
                Some(tail_reader.read_i32_be()?)
            } else {
                None
            };
        if tail_reader.remaining() != 0 {
            return Err(invalid(
                counter_start,
                "CitizenManager counter width is inconsistent",
            ));
        }
        let end_profiler_reference: [u8; 2] = bytes
            .get(end_reference_start..)
            .ok_or_else(|| {
                invalid(
                    end_reference_start,
                    "CitizenManager profiler reference is truncated",
                )
            })?
            .try_into()
            .map_err(|_| {
                invalid(
                    end_reference_start,
                    "CitizenManager profiler reference width is not two bytes",
                )
            })?;

        Ok(Self {
            format_version,
            citizen_prefix,
            unit_prefix,
            instance_prefix,
            instance_fields: instance_fields_start..counter_start,
            active_instances,
            instance_field_suffix: instance_field_suffix_start..counter_start,
            oldest_original_residents,
            fully_educated_original_residents,
            end_profiler_reference,
            original_body: bytes.to_vec(),
        })
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns decoded citizen flags and sparse columns.
    pub fn citizens(&self) -> &CitizenManagerDataPrefix {
        &self.citizen_prefix
    }

    /// Returns decoded version 69+ citizen-unit data.
    pub fn units(&self) -> &CitizenUnitDataPrefix {
        &self.unit_prefix
    }

    /// Returns the decoded instance-flag and prefab scope prefix.
    pub fn instance_prefix(&self) -> &CitizenInstanceDataPrefix {
        &self.instance_prefix
    }

    /// Returns the complete serialized byte section for active records.
    ///
    /// Known records are also exposed through [`Self::active_instances`]. The
    /// section includes any edits made through this type's setters.
    pub fn opaque_instance_fields(&self) -> &[u8] {
        &self.original_body[self.instance_fields.clone()]
    }

    /// Returns the active CitizenInstance records in ascending slot order.
    pub fn active_instances(&self) -> &[CitizenInstanceRecord] {
        &self.active_instances
    }

    /// Returns the serialized bytes for an active record by its zero-based
    /// position in [`Self::active_instances`], including edits made through
    /// this type's setters.
    pub fn instance_record_bytes(&self, record_index: usize) -> Option<&[u8]> {
        let record = self.active_instances.get(record_index)?;
        self.original_body
            .get(record.original_record_start..record.original_record_end)
    }

    /// Replaces one active instance's serialized position without changing
    /// the record width or any neighboring bytes.
    pub fn set_instance_position(
        &mut self,
        record_index: usize,
        position: [f32; 3],
    ) -> Result<(), CrpError> {
        let record = self
            .active_instances
            .get(record_index)
            .ok_or_else(|| invalid(0, "CitizenInstance record index is out of range"))?;
        let offset = record.position_offset;
        write_f32_vector(&mut self.original_body, offset, &position)?;
        self.active_instances[record_index].position = position;
        Ok(())
    }

    /// Replaces one active instance's serialized target position.
    ///
    /// Versions 47–194 require [`CitizenInstanceTargetPosition::Vector3`];
    /// version 195 and later require `Vector4`. Before version 47 the target
    /// follows the position and has no separate serialized field.
    pub fn set_instance_target_position(
        &mut self,
        record_index: usize,
        target_position: CitizenInstanceTargetPosition,
    ) -> Result<(), CrpError> {
        let record = self
            .active_instances
            .get(record_index)
            .ok_or_else(|| invalid(0, "CitizenInstance record index is out of range"))?;
        let offset = record.target_position_offset.ok_or_else(|| {
            invalid(
                record.original_record_start,
                "CitizenInstance target position is not serialized before version 47",
            )
        })?;
        match (self.format_version, target_position) {
            (
                CITIZEN_INSTANCE_TARGET_POSITION_VERSION
                    ..=CITIZEN_INSTANCE_TARGET_POSITION_VECTOR3_END_VERSION,
                CitizenInstanceTargetPosition::Vector3(values),
            ) => {
                write_f32_vector(&mut self.original_body, offset, &values)?;
            }
            (
                CITIZEN_INSTANCE_TARGET_POSITION_VECTOR4_VERSION..,
                CitizenInstanceTargetPosition::Vector4(values),
            ) => {
                write_f32_vector(&mut self.original_body, offset, &values)?;
            }
            _ => {
                return Err(invalid(
                    record.original_record_start,
                    "CitizenInstance target-position width does not match the inner version",
                ));
            }
        }
        self.active_instances[record_index].target_position = target_position;
        Ok(())
    }

    /// Returns any bytes after the decoded active records for forward-compatible preservation.
    pub fn unparsed_instance_suffix(&self) -> &[u8] {
        &self.original_body[self.instance_field_suffix.clone()]
    }

    /// Returns the two original-resident counters from version 169.
    pub fn oldest_original_residents(&self) -> Option<[i16; 2]> {
        self.oldest_original_residents
    }

    /// Returns the fully-educated original-resident count from version 179.
    pub fn fully_educated_original_residents(&self) -> Option<i32> {
        self.fully_educated_original_residents
    }

    /// Returns the original two-byte closing profiler reference.
    pub fn end_profiler_reference(&self) -> [u8; 2] {
        self.end_profiler_reference
    }

    /// Reproduces the retained manager body, including any explicit field edits.
    pub fn encode(&self) -> Vec<u8> {
        self.original_body.clone()
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original_body
    }

    pub(crate) fn instance_fields_range(&self) -> Range<usize> {
        self.instance_fields.clone()
    }
}

fn decode_citizen_instance_record(
    format_version: u32,
    slot: CitizenInstanceSlot,
    reader: &mut Reader<'_>,
) -> Result<CitizenInstanceRecord, CrpError> {
    let velocity = if format_version >= CITIZEN_INSTANCE_VELOCITY_VERSION {
        Some(read_vector3(reader)?)
    } else {
        None
    };
    let position_offset = reader.offset();
    let position = read_vector3(reader)?;
    let target_position_offset = if format_version >= CITIZEN_INSTANCE_TARGET_POSITION_VERSION {
        Some(reader.offset())
    } else {
        None
    };
    let target_position = if format_version >= CITIZEN_INSTANCE_TARGET_POSITION_VECTOR4_VERSION {
        CitizenInstanceTargetPosition::Vector4(read_vector4(reader)?)
    } else if format_version >= CITIZEN_INSTANCE_TARGET_POSITION_VERSION {
        CitizenInstanceTargetPosition::Vector3(read_vector3(reader)?)
    } else {
        CitizenInstanceTargetPosition::FollowsPosition
    };
    let target_direction = if format_version >= CITIZEN_INSTANCE_TARGET_DIRECTION_VERSION {
        Some(read_vector2(reader)?)
    } else {
        None
    };
    let color_rgb = if format_version >= CITIZEN_INSTANCE_COLOR_VERSION
        && slot.flags() & CITIZEN_INSTANCE_COLOR_FLAG != 0
    {
        Some([reader.read_u8()?, reader.read_u8()?, reader.read_u8()?])
    } else {
        None
    };
    let citizen = reader.read_u24_be()?;
    let (path, path_position_index, last_path_offset) =
        if format_version >= CITIZEN_INSTANCE_VELOCITY_VERSION {
            (
                Some(reader.read_u24_be()?),
                Some(reader.read_u8()?),
                Some(reader.read_u8()?),
            )
        } else {
            (None, None, None)
        };
    let wait_counter = if format_version >= CITIZEN_INSTANCE_WAIT_COUNTER_VERSION {
        Some(reader.read_u8()?)
    } else {
        None
    };
    let target_seed = if format_version >= CITIZEN_INSTANCE_TARGET_SEED_VERSION {
        Some(reader.read_u8()?)
    } else {
        None
    };
    let (performer_index, racer_index, compatibility_value) = if format_version
        >= CITIZEN_INSTANCE_PERFORMER_VERSION
    {
        let performer_index = reader.read_u8()?;
        let racer_index = reader.read_u8()?;
        let compatibility_value = if format_version < CITIZEN_INSTANCE_COMPATIBILITY_END_VERSION {
            Some(reader.read_u16_be()?)
        } else {
            None
        };
        (
            Some(performer_index),
            Some(racer_index),
            compatibility_value,
        )
    } else {
        (None, None, None)
    };
    let source_building = if format_version >= CITIZEN_INSTANCE_SOURCE_BUILDING_VERSION {
        Some(reader.read_u16_be()?)
    } else {
        None
    };
    let target_building = reader.read_u16_be()?;

    Ok(CitizenInstanceRecord {
        index: slot.index(),
        flags: slot.flags(),
        prefab_index: slot.prefab_index(),
        velocity,
        position,
        target_position,
        target_direction,
        color_rgb,
        citizen,
        path,
        path_position_index,
        last_path_offset,
        wait_counter,
        target_seed,
        performer_index,
        racer_index,
        compatibility_value,
        source_building,
        target_building,
        position_offset,
        target_position_offset,
        original_record_start: 0,
        original_record_end: 0,
    })
}

fn write_f32_vector<const N: usize>(
    bytes: &mut [u8],
    offset: usize,
    values: &[f32; N],
) -> Result<(), CrpError> {
    let width = N
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or(CrpError::ArithmeticOverflow("CitizenInstance vector width"))?;
    let end = offset
        .checked_add(width)
        .ok_or(CrpError::ArithmeticOverflow("CitizenInstance vector end"))?;
    let output = bytes
        .get_mut(offset..end)
        .ok_or_else(|| invalid(offset, "CitizenInstance vector exceeds manager body"))?;
    for (chunk, value) in output.chunks_exact_mut(4).zip(values) {
        chunk.copy_from_slice(&value.to_bits().to_be_bytes());
    }
    Ok(())
}

fn read_vector2(reader: &mut Reader<'_>) -> Result<[f32; 2], CrpError> {
    Ok([reader.read_f32_be()?, reader.read_f32_be()?])
}

fn read_vector3(reader: &mut Reader<'_>) -> Result<[f32; 3], CrpError> {
    Ok([
        reader.read_f32_be()?,
        reader.read_f32_be()?,
        reader.read_f32_be()?,
    ])
}

fn read_vector4(reader: &mut Reader<'_>) -> Result<[f32; 4], CrpError> {
    Ok([
        reader.read_f32_be()?,
        reader.read_f32_be()?,
        reader.read_f32_be()?,
        reader.read_f32_be()?,
    ])
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
    use super::{
        CitizenInstanceDataPrefix, CitizenInstanceSlot, CitizenInstanceTargetPosition,
        CitizenManagerData, CitizenManagerDataPrefix, CitizenUnitDataPrefix,
        decode_citizen_instance_record,
    };
    use crate::data_serializer::Reader;

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

    fn append_zero_ushort_array(bytes: &mut Vec<u8>, count: usize) {
        for _ in 0..count / 128 {
            bytes.push(0xff);
        }
        let remainder = count % 128;
        if remainder > 0 {
            bytes.push((0x7f + remainder) as u8);
        }
    }

    fn empty_manager_body(format_version: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        append_zero_uint_array(&mut bytes, 1_048_575);
        append_zero_ushort_array(&mut bytes, 524_287);
        if format_version >= 44 {
            let flag_count = if format_version >= 204 {
                65_535
            } else {
                32_767
            };
            append_zero_uint_array(&mut bytes, flag_count);
            bytes.extend_from_slice(&[0, 0]); // empty BuildingInfo prefab names
        }
        if format_version >= 169 {
            bytes.extend_from_slice(&(-2_i16).to_be_bytes());
            bytes.extend_from_slice(&0x0102_i16.to_be_bytes());
        }
        if format_version >= 179 {
            bytes.extend_from_slice(&(-100_i32).to_be_bytes());
        }
        bytes.extend_from_slice(&[0x12, 0x34]);
        bytes
    }

    fn active_instance_record_bytes(format_version: u32, flags: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        if format_version >= 46 {
            append_vector(&mut bytes, &[1.0, 2.0, 3.0]); // velocity
        }
        append_vector(&mut bytes, &[4.0, 5.0, 6.0]); // position
        if format_version >= 195 {
            append_vector(&mut bytes, &[7.0, 8.0, 9.0, 10.0]);
        } else if format_version >= 47 {
            append_vector(&mut bytes, &[7.0, 8.0, 9.0]);
        }
        if format_version >= 200 {
            append_vector(&mut bytes, &[11.0, 12.0]); // target direction
        }
        if format_version >= 247 && flags & (1 << 24) != 0 {
            bytes.extend_from_slice(&[0xaa, 0xbb, 0xcc]);
        }
        bytes.extend_from_slice(&[0x12, 0x34, 0x56]); // citizen
        if format_version >= 46 {
            bytes.extend_from_slice(&[0x23, 0x45, 0x67]); // path
            bytes.extend_from_slice(&[0x34, 0x45]); // path position and offset
        }
        if format_version >= 86 {
            bytes.push(0x56); // wait counter
        }
        if format_version >= 201 {
            bytes.push(0x67); // target seed
        }
        if format_version >= 121_028 {
            bytes.extend_from_slice(&[0x78, 0x89]); // performer and racer
            if format_version < 121_029 {
                bytes.extend_from_slice(&[0xab, 0xcd]); // compatibility value
            }
        }
        if format_version >= 180 {
            bytes.extend_from_slice(&[0xcd, 0xef]); // source building
        }
        bytes.extend_from_slice(&[0x13, 0x57]); // target building
        bytes
    }

    fn single_active_instance_body_194() -> Vec<u8> {
        let mut bytes = Vec::new();
        append_zero_uint_array(&mut bytes, 1_048_575);
        append_zero_ushort_array(&mut bytes, 524_287);

        // Encode one active slot followed by zero flags in the legacy range.
        bytes.push(63);
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend(std::iter::repeat_n(0, 63 * 4));
        append_zero_uint_array(&mut bytes, 32_767 - 64);
        bytes.extend_from_slice(&ushort_array(&[0])); // prefab index
        bytes.extend_from_slice(&[0, 0]); // empty prefab-name table

        append_vector(&mut bytes, &[1.0, 2.0, 3.0]); // velocity
        append_vector(&mut bytes, &[4.0, 5.0, 6.0]); // position
        append_vector(&mut bytes, &[7.0, 8.0, 9.0]); // version-194 target position
        bytes.extend_from_slice(&[0, 0, 0]); // citizen reference
        bytes.extend_from_slice(&[0, 0, 0, 0, 0]); // path and offsets
        bytes.push(0); // wait counter
        bytes.extend_from_slice(&[0; 4]); // source and target buildings
        bytes.extend_from_slice(&[0; 8]); // versioned trailing counters
        bytes.extend_from_slice(&[0x12, 0x34]); // closing profiler reference
        bytes
    }

    fn append_vector(bytes: &mut Vec<u8>, values: &[f32]) {
        for value in values {
            bytes.extend_from_slice(&value.to_be_bytes());
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

    #[test]
    fn manager_tail_counters_follow_169_and_179_gates() {
        let oldest_residents = [-2, 0x0102];
        for (version, expected_oldest, expected_educated) in [
            (168, None, None),
            (169, Some(oldest_residents), None),
            (178, Some(oldest_residents), None),
            (179, Some(oldest_residents), Some(-100)),
        ] {
            let bytes = empty_manager_body(version);
            let decoded = CitizenManagerData::decode(version, &bytes)
                .unwrap_or_else(|error| panic!("version {version}: {error}"));
            assert_eq!(decoded.oldest_original_residents(), expected_oldest);
            assert_eq!(
                decoded.fully_educated_original_residents(),
                expected_educated
            );
            assert!(decoded.opaque_instance_fields().is_empty());
            assert_eq!(decoded.end_profiler_reference(), [0x12, 0x34]);
            assert_eq!(decoded.encode(), bytes);
        }
    }

    #[test]
    fn edits_instance_position_and_versioned_target_without_touching_other_bytes() {
        let bytes = single_active_instance_body_194();
        let decoded = CitizenManagerData::decode(194, &bytes).expect("version-194 manager");
        assert_eq!(decoded.active_instances().len(), 1);
        assert_eq!(
            decoded.active_instances()[0].target_position(),
            CitizenInstanceTargetPosition::Vector3([7.0, 8.0, 9.0])
        );

        let position = [-10.5_f32, 20.25, 30.0];
        let target = CitizenInstanceTargetPosition::Vector3([40.5_f32, -50.25, 60.0]);
        let record = &decoded.active_instances()[0];
        let position_offset = record.position_offset;
        let target_offset = record.target_position_offset.expect("serialized target");
        let mut expected = bytes.clone();
        for (chunk, value) in expected[position_offset..position_offset + 12]
            .chunks_exact_mut(4)
            .zip(position)
        {
            chunk.copy_from_slice(&value.to_bits().to_be_bytes());
        }
        if let CitizenInstanceTargetPosition::Vector3(values) = target {
            for (chunk, value) in expected[target_offset..target_offset + 12]
                .chunks_exact_mut(4)
                .zip(values)
            {
                chunk.copy_from_slice(&value.to_bits().to_be_bytes());
            }
        }

        let mut edited = decoded;
        edited
            .set_instance_position(0, position)
            .expect("edit position");
        edited
            .set_instance_target_position(0, target)
            .expect("edit version-194 target");
        assert_eq!(edited.active_instances()[0].position(), position);
        assert_eq!(edited.active_instances()[0].target_position(), target);
        assert_eq!(edited.encode(), expected);
        assert!(edited.set_instance_position(1, [0.0; 3]).is_err());
        assert!(
            edited
                .set_instance_target_position(0, CitizenInstanceTargetPosition::Vector4([0.0; 4]))
                .is_err()
        );
        assert_eq!(edited.encode(), expected);
    }

    #[test]
    fn manager_rejects_truncated_counter_and_profiler_tail() {
        let bytes = empty_manager_body(179);
        for missing in 1..=10 {
            let truncated = &bytes[..bytes.len() - missing];
            assert!(
                CitizenManagerData::decode(179, truncated).is_err(),
                "accepted body missing {missing} tail bytes"
            );
        }
    }

    #[test]
    fn desert_map_fixture_decodes_active_instances_and_preserves_record_bytes() {
        let fixture = include_bytes!("../tests/fixtures/citizen-manager-v116020-desert.bin");
        let decoded = CitizenManagerData::decode(116_020, fixture).expect("game-produced body");
        assert_eq!(decoded.active_instances().len(), 72);
        assert!(decoded.unparsed_instance_suffix().is_empty());
        assert_eq!(decoded.encode(), fixture);

        let first = &decoded.active_instances()[0];
        assert_eq!(first.index(), 449);
        assert_eq!(first.flags(), 17);
        assert_eq!(first.prefab_index(), 53);
        assert_eq!(
            first.velocity(),
            Some([
                f32::from_bits(0xbf2a_3801),
                f32::from_bits(0xbda1_b000),
                f32::from_bits(0x3ff1_702c),
            ])
        );
        assert_eq!(
            first.position(),
            [
                f32::from_bits(0xc4d5_1612),
                f32::from_bits(0x43a0_535e),
                f32::from_bits(0xc50c_dfcf),
            ]
        );
        assert_eq!(
            first.target_position(),
            CitizenInstanceTargetPosition::Vector4([
                f32::from_bits(0xc4dd_f7c7),
                f32::from_bits(0x43a0_4e51),
                f32::from_bits(0xc500_46c8),
                0.0,
            ])
        );
        assert_eq!(first.target_direction(), Some([0.0, 0.0]));
        assert_eq!(first.citizen(), 0);
        assert_eq!(first.path(), Some(0));
        assert_eq!(first.path_position_index(), Some(0));
        assert_eq!(first.last_path_offset(), Some(0));
        assert_eq!(first.wait_counter(), Some(0));
        assert_eq!(first.target_seed(), Some(0));
        assert_eq!(first.source_building(), Some(0));
        assert_eq!(first.target_building(), 32_152);
        assert_eq!(
            decoded.instance_record_bytes(0).unwrap(),
            [
                0xbf, 0x2a, 0x38, 0x01, 0xbd, 0xa1, 0xb0, 0x00, 0x3f, 0xf1, 0x70, 0x2c, 0xc4, 0xd5,
                0x16, 0x12, 0x43, 0xa0, 0x53, 0x5e, 0xc5, 0x0c, 0xdf, 0xcf, 0xc4, 0xdd, 0xf7, 0xc7,
                0x43, 0xa0, 0x4e, 0x51, 0xc5, 0x00, 0x46, 0xc8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x7d, 0x98,
            ]
        );

        let changed_target = [12.5_f32, -34.25, 56.0, 1.0];
        let record = &decoded.active_instances()[0];
        let target_offset = record.target_position_offset.expect("serialized Vector4");
        let mut expected = fixture.to_vec();
        for (chunk, value) in expected[target_offset..target_offset + 16]
            .chunks_exact_mut(4)
            .zip(changed_target)
        {
            chunk.copy_from_slice(&value.to_bits().to_be_bytes());
        }
        let mut edited = decoded;
        edited
            .set_instance_target_position(0, CitizenInstanceTargetPosition::Vector4(changed_target))
            .expect("edit game-produced target position");
        assert_eq!(edited.encode(), expected);
    }

    #[test]
    fn active_instance_fields_follow_reader_gates_and_field_order() {
        let colored = 1 << 24;
        for (version, flags, expected_len) in [
            (44, 0, 17),
            (45, 0, 17),
            (46, 0, 34),
            (47, 0, 46),
            (85, 0, 46),
            (86, 0, 47),
            (179, 0, 47),
            (194, 0, 49),
            (195, 0, 53),
            (199, 0, 53),
            (200, 0, 61),
            (201, 0, 62),
            (246, 0, 62),
            (247, 0, 62),
            (247, colored, 65),
            (121_027, colored, 65),
            (121_028, colored, 69),
            (121_029, colored, 67),
        ] {
            let bytes = active_instance_record_bytes(version, flags);
            let mut reader = Reader::new(&bytes);
            let slot = CitizenInstanceSlot {
                index: 3,
                flags,
                prefab_index: 2,
            };
            let record = decode_citizen_instance_record(version, slot, &mut reader)
                .unwrap_or_else(|error| panic!("version {version}: {error}"));
            assert_eq!(reader.offset(), expected_len, "version {version}");
            assert_eq!(reader.remaining(), 0, "version {version}");
            assert_eq!(record.position(), [4.0, 5.0, 6.0]);
            assert_eq!(record.citizen(), 0x123456);
            assert_eq!(record.target_building(), 0x1357);
            assert_eq!(
                record.color_rgb(),
                (flags & colored != 0).then_some([0xaa, 0xbb, 0xcc])
            );
            if version >= 46 {
                assert_eq!(record.velocity(), Some([1.0, 2.0, 3.0]));
                assert_eq!(record.path(), Some(0x234567));
                assert_eq!(record.path_position_index(), Some(0x34));
                assert_eq!(record.last_path_offset(), Some(0x45));
            } else {
                assert_eq!(record.velocity(), None);
                assert_eq!(record.path(), None);
            }
            if version < 47 {
                assert_eq!(
                    record.target_position(),
                    CitizenInstanceTargetPosition::FollowsPosition
                );
            } else if version < 195 {
                assert_eq!(
                    record.target_position(),
                    CitizenInstanceTargetPosition::Vector3([7.0, 8.0, 9.0])
                );
            } else {
                assert_eq!(
                    record.target_position(),
                    CitizenInstanceTargetPosition::Vector4([7.0, 8.0, 9.0, 10.0])
                );
            }
            assert_eq!(
                record.target_direction(),
                (version >= 200).then_some([11.0, 12.0])
            );
            assert_eq!(record.wait_counter(), (version >= 86).then_some(0x56));
            assert_eq!(record.target_seed(), (version >= 201).then_some(0x67));
            assert_eq!(
                record.performer_index(),
                (version >= 121_028).then_some(0x78)
            );
            assert_eq!(record.racer_index(), (version >= 121_028).then_some(0x89));
            assert_eq!(
                record.compatibility_value(),
                (version == 121_028).then_some(0xabcd)
            );
            assert_eq!(record.source_building(), (version >= 180).then_some(0xcdef));
        }
    }

    #[test]
    fn active_instance_records_reject_truncated_field_bytes() {
        let flags = 1 << 24;
        let bytes = active_instance_record_bytes(121_029, flags);
        let slot = CitizenInstanceSlot {
            index: 1,
            flags,
            prefab_index: 0,
        };
        for end in 0..bytes.len() {
            assert!(
                decode_citizen_instance_record(121_029, slot, &mut Reader::new(&bytes[..end]))
                    .is_err()
            );
        }
    }
}
