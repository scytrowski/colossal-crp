use crate::CrpError;
use crate::data_serializer::{Reader, write_nullable_unique_string, write_unique_string};
use crate::encoded_array::{
    read_encoded_byte_array, read_encoded_int_array, read_encoded_uint_array,
    read_encoded_ulong_array, read_encoded_ushort_array, write_encoded_byte_array,
    write_encoded_int_array, write_encoded_uint_array, write_encoded_uint_array_runs,
    write_encoded_ulong_array, write_encoded_ushort_array,
};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};
use std::ops::Range;

const BUILDING_COUNT: usize = 49_152;
const LEGACY_BUILDING_COUNT: usize = 32_768;
const FULL_BUILDING_COUNT_VERSION: u32 = 222;
const SECONDARY_BUILDING_FLAGS_VERSION: u32 = 112_030;
const TWO_BUILDING_PROBLEMS_VERSION: u32 = 115_000;
const SAVE_APP_PROBLEMS_VERSION: u32 = 146_809_088;
const MONUMENT_EFFECTS_VERSION: u32 = 110_016;
const FACULTY_BONUSES_VERSION: u32 = 112_001;
const RACE_TEAM_UNASSIGNED_VERSION: u32 = 120_002;
const BUILDING_EFFECT_TABLE_LENGTH: usize = 16;

/// Serialized flags for one one-based `Building` buffer position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingFlag {
    /// One-based building buffer index.
    pub index: u16,
    /// Primary building flags.
    pub flags: u32,
    /// Secondary building flags, zero before inner version 112030.
    pub flags2: u32,
}

/// Encoded building-flag arrays at the start of `BuildingManager+Data`.
///
/// Later prefab tables and building records remain outside this prefix. The
/// original encoded bytes are retained for exact writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingFlagsPrefix {
    format_version: u32,
    building_flags: Vec<BuildingFlag>,
    primary_flags_range: Range<usize>,
    secondary_flags_range: Option<Range<usize>>,
    serialized_prefix: Vec<u8>,
}

/// Editor for raw primary and secondary BuildingManager flag values.
///
/// Primary flags preserve each building slot's zero/nonzero activity state.
/// Secondary flags are available from inner version 112030 and do not select
/// active building records. Changed arrays are canonically re-encoded and can
/// resize the prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingFlagsEditor {
    original: BuildingFlagsPrefix,
    building_flags: Vec<BuildingFlag>,
}

/// Prefab index tables and names immediately after the BuildingManager counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingPrefabPrefix {
    format_version: u32,
    building_indices: Vec<u16>,
    guide_indices: Vec<u16>,
    milestone_indices: Vec<u16>,
    names: Vec<Option<String>>,
    serialized_building_indices: Vec<u8>,
    serialized_guide_indices: Vec<u8>,
    serialized_milestone_indices: Vec<u8>,
    serialized_name_count: Vec<u8>,
    serialized_name_entries: Vec<Vec<u8>>,
    serialized_prefix: Vec<u8>,
}

/// Editable existing BuildingManager prefab assignments and names.
///
/// The three assignment counts and the prefab-name count stay fixed. Changed
/// index arrays use canonical `EncodedArray.UShort` encoding; unchanged name
/// entries retain their original bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingPrefabEditor {
    format_version: u32,
    building_indices: Vec<u16>,
    guide_indices: Vec<u16>,
    milestone_indices: Vec<u16>,
    names: Vec<Option<String>>,
    original_building_indices: Vec<u16>,
    original_guide_indices: Vec<u16>,
    original_milestone_indices: Vec<u16>,
    original_names: Vec<Option<String>>,
    serialized_building_indices: Vec<u8>,
    serialized_guide_indices: Vec<u8>,
    serialized_milestone_indices: Vec<u8>,
    serialized_name_count: Vec<u8>,
    serialized_name_entries: Vec<Vec<u8>>,
    original_prefix: Vec<u8>,
}

/// Versioned object headers following the BuildingManager prefab table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingManagerObjectHeadersPrefix {
    format_version: u32,
    object_headers: Vec<NetObjectHeader>,
    trailing_problem_fields: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Opaque versioned problem state for one active building.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingProblemWords {
    /// One-based building buffer index for this problem state.
    pub index: u16,
    /// First serialized 64-bit word.
    pub first: u64,
    /// Second serialized 64-bit word.
    pub second: u64,
}

/// Per-building problem states from inner version 115000 onward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingProblemPrefix {
    format_version: u32,
    problems: Vec<BuildingProblemWords>,
    serialized_prefix: Vec<u8>,
}

/// Editable opaque problem-state words for active buildings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingProblemEditor {
    format_version: u32,
    active_indices: Vec<u16>,
    problems: Vec<BuildingProblemWords>,
}

/// One serialized monument or faculty effect factor and position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingEffectValue {
    /// Signed effect factor.
    pub factor: i32,
    /// IEEE-754 bit patterns for the x, y, and z position components.
    pub position_bits: [u32; 3],
}

/// The two fixed BuildingManager monument-effect tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingMonumentEffectsPrefix {
    format_version: u32,
    temporary: Vec<BuildingEffectValue>,
    final_values: Vec<BuildingEffectValue>,
    serialized_prefix: Vec<u8>,
}

/// Editable temporary and final monument-effect tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingMonumentEffectsEditor {
    format_version: u32,
    temporary: Vec<BuildingEffectValue>,
    final_values: Vec<BuildingEffectValue>,
}

/// The two fixed BuildingManager faculty-bonus tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingFacultyBonusesPrefix {
    format_version: u32,
    temporary: Vec<BuildingEffectValue>,
    final_values: Vec<BuildingEffectValue>,
    serialized_prefix: Vec<u8>,
}

/// Editable temporary and final faculty-bonus tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingFacultyBonusesEditor {
    format_version: u32,
    temporary: Vec<BuildingEffectValue>,
    final_values: Vec<BuildingEffectValue>,
}

/// Race-team-unassigned booleans for active buildings from inner version 120002.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingRaceTeamUnassignedPrefix {
    format_version: u32,
    unassigned: Vec<bool>,
    serialized_prefix: Vec<u8>,
}

/// Editable race-team-unassigned booleans that retain unchanged raw bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingRaceTeamUnassignedEditor {
    format_version: u32,
    unassigned: Vec<bool>,
    original_unassigned: Vec<bool>,
    original_bytes: Vec<u8>,
}

/// Race-team prefab indices and the `PrefabCollection<BuildingInfo>` name table.
/// The decoded body retains its original bytes for exact rewriting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingRaceTeamReferencesPrefix {
    format_version: u32,
    indices: Vec<u16>,
    prefab_names: Vec<Option<String>>,
    serialized_prefix: Vec<u8>,
}

/// Editable race-team indices and prefab names from the reference scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingRaceTeamReferencesEditor {
    format_version: u32,
    indices: Vec<u16>,
    prefab_names: Vec<Option<String>>,
}

/// Per-building buffer indices and byte fields following problem state.
///
/// The decoder supports inner versions 164 and newer and consumes only the
/// version-gated prefix represented here. Values remain paired with the active
/// building order from `BuildingFlagsPrefix`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingBufferFieldsPrefix {
    format_version: u32,
    net_node: Vec<u16>,
    sub_building: Vec<u16>,
    water_source: Vec<u16>,
    legacy_water_source_release: Vec<u16>,
    electricity_buffer: Vec<u16>,
    water_buffer: Vec<u16>,
    sewage_buffer: Vec<u16>,
    heating_buffer: Vec<u16>,
    garbage_buffer: Vec<u16>,
    crime_buffer: Vec<u16>,
    mail_buffer: Vec<u16>,
    custom_buffer1: Vec<u16>,
    custom_buffer2: Vec<u16>,
    build_water_height: Vec<u16>,
    production_rate: Vec<u8>,
    water_pollution: Vec<u8>,
    serialized_fields: Vec<Vec<u8>>,
    serialized_prefix: Vec<u8>,
}

/// Editable versioned network, utility, and byte fields for active buildings.
///
/// Each array keeps its original bytes until that field changes. Changed
/// arrays use canonical encoded-array blocks and may resize the prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingBufferFieldsEditor {
    original: BuildingBufferFieldsPrefix,
    net_node: Vec<u16>,
    sub_building: Vec<u16>,
    water_source: Vec<u16>,
    legacy_water_source_release: Vec<u16>,
    electricity_buffer: Vec<u16>,
    water_buffer: Vec<u16>,
    sewage_buffer: Vec<u16>,
    heating_buffer: Vec<u16>,
    garbage_buffer: Vec<u16>,
    crime_buffer: Vec<u16>,
    mail_buffer: Vec<u16>,
    custom_buffer1: Vec<u16>,
    custom_buffer2: Vec<u16>,
    build_water_height: Vec<u16>,
    production_rate: Vec<u8>,
    water_pollution: Vec<u8>,
}

/// Per-building byte state following utility buffers and pollution values.
///
/// The values are exposed as raw bytes because their game meanings have not
/// been fully mapped. The original encoded prefix is retained for exact writeback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingByteStatePrefix {
    format_version: u32,
    construct_state: Vec<u8>,
    fire_intensity: Vec<u8>,
    fire_damage: Vec<u8>,
    production_state: Vec<u8>,
    health: Vec<u8>,
    child_health: Vec<u8>,
    senior_health: Vec<u8>,
    happiness: Vec<u8>,
    citizen_count: Vec<u8>,
    temp_import: Vec<u8>,
    temp_export: Vec<u8>,
    final_import: Vec<u8>,
    final_export: Vec<u8>,
    serialized_fields: Vec<Vec<u8>>,
    serialized_prefix: Vec<u8>,
}

/// Editable raw per-building byte state from the `BuildingManager` arrays.
///
/// Active-building counts and version gates remain fixed. Unchanged arrays
/// preserve their original bytes; changed arrays use canonical byte encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingByteStateEditor {
    original: BuildingByteStatePrefix,
    construct_state: Vec<u8>,
    fire_intensity: Vec<u8>,
    fire_damage: Vec<u8>,
    production_state: Vec<u8>,
    health: Vec<u8>,
    child_health: Vec<u8>,
    senior_health: Vec<u8>,
    happiness: Vec<u8>,
    citizen_count: Vec<u8>,
    temp_import: Vec<u8>,
    temp_export: Vec<u8>,
    final_import: Vec<u8>,
    final_export: Vec<u8>,
}

/// A field in the shared, field-major BuildingManager timer array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingTimerField {
    /// Electricity-problem timer.
    Electricity,
    /// Heating-problem timer, present from inner version 231.
    Heating,
    /// Water-problem timer.
    Water,
    /// Worker-problem timer.
    Worker,
    /// Incoming-goods problem timer.
    Incoming,
    /// Outgoing-goods problem timer.
    Outgoing,
    /// Health-problem timer.
    Health,
    /// Death-problem timer.
    Death,
    /// Service-problem timer.
    Service,
    /// Tax-problem timer, present from inner version 172.
    Tax,
    /// Major-problem timer.
    Major,
}

/// Timer, level-up, width, and length arrays following BuildingManager byte state.
///
/// Timer values are stored field-major: the full active-building column for one
/// timer precedes the next timer column. Unchanged data writes its original
/// encoded bytes exactly; [`into_editor`](Self::into_editor) enables canonical
/// writing after edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingTimerGeometryPrefix {
    format_version: u32,
    active_building_count: usize,
    timer_fields: Vec<BuildingTimerField>,
    timer_values: Vec<u8>,
    level_up_progress: Vec<u8>,
    width: Vec<u8>,
    length: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Editable timer and building-dimension arrays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingTimerGeometryEditor {
    format_version: u32,
    active_building_count: usize,
    timer_fields: Vec<BuildingTimerField>,
    timer_values: Vec<u8>,
    level_up_progress: Vec<u8>,
    width: Vec<u8>,
    length: Vec<u8>,
}

/// Per-building education, population, hazard, and level arrays after geometry.
///
/// Values are retained as raw bytes. The game ignores the serialized level
/// column for inner versions 207 through 111004, so this API does not assign
/// semantic meaning to that legacy column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingPopulationPrefix {
    format_version: u32,
    active_building_count: usize,
    education1: Vec<u8>,
    education2: Vec<u8>,
    education3: Vec<u8>,
    children: Vec<u8>,
    teens: Vec<u8>,
    youngs: Vec<u8>,
    adults: Vec<u8>,
    seniors: Vec<u8>,
    fire_hazard: Vec<u8>,
    level: Vec<u8>,
    serialized_prefix: Vec<u8>,
}

/// Editable per-building education, population, hazard, and level arrays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingPopulationEditor {
    format_version: u32,
    active_building_count: usize,
    education1: Vec<u8>,
    education2: Vec<u8>,
    education3: Vec<u8>,
    children: Vec<u8>,
    teens: Vec<u8>,
    youngs: Vec<u8>,
    adults: Vec<u8>,
    seniors: Vec<u8>,
    fire_hazard: Vec<u8>,
    level: Vec<u8>,
}

/// Traffic, routing, cash, room, and employment columns following population.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingEconomicFieldsPrefix {
    format_version: u32,
    active_building_count: usize,
    cargo_traffic_rate: Vec<u32>,
    garbage_traffic_rate: Vec<u32>,
    access_segment: Vec<u16>,
    event_access_segment: Vec<u16>,
    cash_buffer: Vec<i32>,
    room_cost: Vec<u16>,
    room_cost_target: Vec<u16>,
    room_max: Vec<u16>,
    room_used: Vec<u16>,
    educated0_unemployed: Vec<u8>,
    educated1_unemployed: Vec<u8>,
    educated2_unemployed: Vec<u8>,
    educated3_unemployed: Vec<u8>,
    all_unemployed: Vec<u8>,
    event_route_index: Vec<u16>,
    serialized_prefix: Vec<u8>,
}

/// Editable traffic, routing, cash, room, and employment columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingEconomicFieldsEditor {
    format_version: u32,
    active_building_count: usize,
    cargo_traffic_rate: Vec<u32>,
    garbage_traffic_rate: Vec<u32>,
    access_segment: Vec<u16>,
    event_access_segment: Vec<u16>,
    cash_buffer: Vec<i32>,
    room_cost: Vec<u16>,
    room_cost_target: Vec<u16>,
    room_max: Vec<u16>,
    room_used: Vec<u16>,
    educated0_unemployed: Vec<u8>,
    educated1_unemployed: Vec<u8>,
    educated2_unemployed: Vec<u8>,
    educated3_unemployed: Vec<u8>,
    all_unemployed: Vec<u8>,
    event_route_index: Vec<u16>,
}

/// One keyed shared-string entry in a BuildingManager service-selection table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingServiceSelection {
    /// Service prefab index stored as a `u16`.
    pub service_index: u16,
    /// Shared string value; null values are preserved if present in the stream.
    pub value: Option<String>,
}

/// Versioned service-vehicle selection tables following economic fields.
///
/// The original table bytes are retained exactly. The shared-string table
/// snapshot includes known preceding manager tags and strings introduced here,
/// so a later decoder can continue from the correct shared-string indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingServiceSelectionPrefix {
    format_version: u32,
    vehicle_selection: Vec<BuildingServiceSelection>,
    additional_vehicle_selection: Vec<BuildingServiceSelection>,
    shared_strings: Vec<Option<String>>,
    serialized_prefix: Vec<u8>,
}

/// Directly serialized fields for one active BuildingManager entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildingInstanceRecord {
    /// Building prefab/build index.
    pub building_index: u32,
    /// IEEE-754 bit patterns for the x, y, and z coordinates.
    pub position_bits: [u32; 3],
    /// IEEE-754 bit pattern for the building angle.
    pub angle_bits: u32,
    /// Base height byte.
    pub base_height: u8,
    /// 24-bit citizen-unit index.
    pub citizen_units: u32,
}

/// Direct per-active-building records after service-selection tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingInstanceRecordsPrefix {
    format_version: u32,
    records: Vec<BuildingInstanceRecord>,
    serialized_prefix: Vec<u8>,
}

/// Editable direct per-active-building records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingInstanceRecordsEditor {
    format_version: u32,
    records: Vec<BuildingInstanceRecord>,
}

fn invalid_building_encoded_fields(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "BuildingManager encoded fields".to_owned(),
        offset,
        reason: reason.into(),
    }
}

fn read_ushort_field(
    reader: &mut Reader<'_>,
    count: usize,
    serialized_fields: &mut Vec<Vec<u8>>,
) -> Result<Vec<u16>, CrpError> {
    let start = reader.offset();
    let values = read_encoded_ushort_array(reader, count)?;
    serialized_fields.push(reader.consumed_bytes()[start..reader.offset()].to_vec());
    Ok(values)
}

fn read_byte_field(
    reader: &mut Reader<'_>,
    count: usize,
    serialized_fields: &mut Vec<Vec<u8>>,
) -> Result<Vec<u8>, CrpError> {
    let start = reader.offset();
    let values = read_encoded_byte_array(reader, count)?;
    serialized_fields.push(reader.consumed_bytes()[start..reader.offset()].to_vec());
    Ok(values)
}

fn write_ushort_field<'a>(
    output: &mut Vec<u8>,
    original_fields: &mut impl Iterator<Item = &'a Vec<u8>>,
    values: &[u16],
    original_values: &[u16],
) -> Result<(), CrpError> {
    let original_bytes = original_fields.next().ok_or_else(|| {
        invalid_building_encoded_fields(output.len(), "source field bytes are missing")
    })?;
    if values == original_values {
        output.extend_from_slice(original_bytes);
    } else {
        write_encoded_ushort_array(output, values);
    }
    Ok(())
}

fn write_byte_field<'a>(
    output: &mut Vec<u8>,
    original_fields: &mut impl Iterator<Item = &'a Vec<u8>>,
    values: &[u8],
    original_values: &[u8],
) -> Result<(), CrpError> {
    let original_bytes = original_fields.next().ok_or_else(|| {
        invalid_building_encoded_fields(output.len(), "source field bytes are missing")
    })?;
    if values == original_values {
        output.extend_from_slice(original_bytes);
    } else {
        write_encoded_byte_array(output, values);
    }
    Ok(())
}

impl BuildingBufferFieldsPrefix {
    /// Decodes versioned network, utility, and byte fields for active buildings.
    ///
    /// Returns the decoded prefix and bytes consumed. Truncated data or a
    /// mismatched flag-table version returns an error. Later BuildingManager
    /// fields remain unread and the original prefix bytes are retained.
    pub fn decode_prefix(
        format_version: u32,
        flags: &BuildingFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager buffer fields".to_owned(),
                offset: 0,
                reason: "unsupported or mismatched inner version".to_owned(),
            });
        }
        let count = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .count();
        let mut reader = Reader::new(bytes);
        let mut serialized_fields = Vec::new();
        let net_node = if format_version >= 33 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let sub_building = if format_version >= 208 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let water_source = if format_version >= 33 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let legacy_water_source_release = if (95..184).contains(&format_version) {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let electricity_buffer = if format_version >= 33 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let water_buffer = if format_version >= 33 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let sewage_buffer = if format_version >= 33 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let heating_buffer = if format_version >= 227 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let garbage_buffer = if format_version >= 33 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let crime_buffer = if format_version >= 33 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let mail_buffer = if format_version >= 111_003 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let custom_buffer1 = if format_version >= 33 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let custom_buffer2 = if format_version >= 66 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let build_water_height = if format_version >= 308 {
            read_ushort_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let production_rate = if format_version >= 33 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let water_pollution = if format_version >= 33 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                net_node,
                sub_building,
                water_source,
                legacy_water_source_release,
                electricity_buffer,
                water_buffer,
                sewage_buffer,
                heating_buffer,
                garbage_buffer,
                crime_buffer,
                mail_buffer,
                custom_buffer1,
                custom_buffer2,
                build_water_height,
                production_rate,
                water_pollution,
                serialized_fields,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns network-node indices for active buildings.
    pub fn net_node(&self) -> &[u16] {
        &self.net_node
    }
    /// Returns sub-building indices from version 208 onward.
    pub fn sub_building(&self) -> &[u16] {
        &self.sub_building
    }
    /// Returns water-source indices.
    pub fn water_source(&self) -> &[u16] {
        &self.water_source
    }
    /// Returns legacy water-source release values from versions 95 through 183.
    pub fn legacy_water_source_release(&self) -> &[u16] {
        &self.legacy_water_source_release
    }
    /// Returns electricity-buffer indices.
    pub fn electricity_buffer(&self) -> &[u16] {
        &self.electricity_buffer
    }
    /// Returns water-buffer indices.
    pub fn water_buffer(&self) -> &[u16] {
        &self.water_buffer
    }
    /// Returns sewage-buffer indices.
    pub fn sewage_buffer(&self) -> &[u16] {
        &self.sewage_buffer
    }
    /// Returns heating-buffer indices from version 227 onward.
    pub fn heating_buffer(&self) -> &[u16] {
        &self.heating_buffer
    }
    /// Returns garbage-buffer indices.
    pub fn garbage_buffer(&self) -> &[u16] {
        &self.garbage_buffer
    }
    /// Returns crime-buffer indices.
    pub fn crime_buffer(&self) -> &[u16] {
        &self.crime_buffer
    }
    /// Returns mail-buffer indices from version 111003 onward.
    pub fn mail_buffer(&self) -> &[u16] {
        &self.mail_buffer
    }
    /// Returns custom-buffer-1 indices.
    pub fn custom_buffer1(&self) -> &[u16] {
        &self.custom_buffer1
    }
    /// Returns custom-buffer-2 indices from version 66 onward.
    pub fn custom_buffer2(&self) -> &[u16] {
        &self.custom_buffer2
    }
    /// Returns build-water-height values from version 308 onward.
    pub fn build_water_height(&self) -> &[u16] {
        &self.build_water_height
    }
    /// Returns production-rate bytes.
    pub fn production_rate(&self) -> &[u8] {
        &self.production_rate
    }
    /// Returns water-pollution bytes.
    pub fn water_pollution(&self) -> &[u8] {
        &self.water_pollution
    }
    /// Writes the original encoded fields exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Creates an editor for values paired with the existing active-building order.
    pub fn into_editor(self) -> BuildingBufferFieldsEditor {
        BuildingBufferFieldsEditor {
            net_node: self.net_node.clone(),
            sub_building: self.sub_building.clone(),
            water_source: self.water_source.clone(),
            legacy_water_source_release: self.legacy_water_source_release.clone(),
            electricity_buffer: self.electricity_buffer.clone(),
            water_buffer: self.water_buffer.clone(),
            sewage_buffer: self.sewage_buffer.clone(),
            heating_buffer: self.heating_buffer.clone(),
            garbage_buffer: self.garbage_buffer.clone(),
            crime_buffer: self.crime_buffer.clone(),
            mail_buffer: self.mail_buffer.clone(),
            custom_buffer1: self.custom_buffer1.clone(),
            custom_buffer2: self.custom_buffer2.clone(),
            build_water_height: self.build_water_height.clone(),
            production_rate: self.production_rate.clone(),
            water_pollution: self.water_pollution.clone(),
            original: self,
        }
    }
}

impl BuildingBufferFieldsEditor {
    /// Returns the inner DataSerializer version associated with these fields.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns editable network-node indices in active-building order.
    pub fn net_node_mut(&mut self) -> &mut [u16] {
        &mut self.net_node
    }
    /// Returns editable sub-building indices from version 208 onward.
    pub fn sub_building_mut(&mut self) -> &mut [u16] {
        &mut self.sub_building
    }
    /// Returns editable water-source indices in active-building order.
    pub fn water_source_mut(&mut self) -> &mut [u16] {
        &mut self.water_source
    }
    /// Returns editable legacy water-source release values from versions 95–183.
    pub fn legacy_water_source_release_mut(&mut self) -> &mut [u16] {
        &mut self.legacy_water_source_release
    }
    /// Returns editable electricity-buffer indices in active-building order.
    pub fn electricity_buffer_mut(&mut self) -> &mut [u16] {
        &mut self.electricity_buffer
    }
    /// Returns editable water-buffer indices in active-building order.
    pub fn water_buffer_mut(&mut self) -> &mut [u16] {
        &mut self.water_buffer
    }
    /// Returns editable sewage-buffer indices in active-building order.
    pub fn sewage_buffer_mut(&mut self) -> &mut [u16] {
        &mut self.sewage_buffer
    }
    /// Returns editable heating-buffer indices from version 227 onward.
    pub fn heating_buffer_mut(&mut self) -> &mut [u16] {
        &mut self.heating_buffer
    }
    /// Returns editable garbage-buffer indices in active-building order.
    pub fn garbage_buffer_mut(&mut self) -> &mut [u16] {
        &mut self.garbage_buffer
    }
    /// Returns editable crime-buffer indices in active-building order.
    pub fn crime_buffer_mut(&mut self) -> &mut [u16] {
        &mut self.crime_buffer
    }
    /// Returns editable mail-buffer indices from version 111003 onward.
    pub fn mail_buffer_mut(&mut self) -> &mut [u16] {
        &mut self.mail_buffer
    }
    /// Returns editable custom-buffer-1 indices in active-building order.
    pub fn custom_buffer1_mut(&mut self) -> &mut [u16] {
        &mut self.custom_buffer1
    }
    /// Returns editable custom-buffer-2 indices from version 66 onward.
    pub fn custom_buffer2_mut(&mut self) -> &mut [u16] {
        &mut self.custom_buffer2
    }
    /// Returns editable build-water-height values from version 308 onward.
    pub fn build_water_height_mut(&mut self) -> &mut [u16] {
        &mut self.build_water_height
    }
    /// Returns editable production-rate bytes.
    pub fn production_rate_mut(&mut self) -> &mut [u8] {
        &mut self.production_rate
    }
    /// Returns editable water-pollution bytes.
    pub fn water_pollution_mut(&mut self) -> &mut [u8] {
        &mut self.water_pollution
    }

    /// Writes changed arrays canonically and preserves all other source bytes.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let version = self.format_version();
        if version < 164 {
            return Err(invalid_building_encoded_fields(
                0,
                "unsupported inner version",
            ));
        }
        let original = &self.original;
        if self.net_node == original.net_node
            && self.sub_building == original.sub_building
            && self.water_source == original.water_source
            && self.legacy_water_source_release == original.legacy_water_source_release
            && self.electricity_buffer == original.electricity_buffer
            && self.water_buffer == original.water_buffer
            && self.sewage_buffer == original.sewage_buffer
            && self.heating_buffer == original.heating_buffer
            && self.garbage_buffer == original.garbage_buffer
            && self.crime_buffer == original.crime_buffer
            && self.mail_buffer == original.mail_buffer
            && self.custom_buffer1 == original.custom_buffer1
            && self.custom_buffer2 == original.custom_buffer2
            && self.build_water_height == original.build_water_height
            && self.production_rate == original.production_rate
            && self.water_pollution == original.water_pollution
        {
            return Ok(original.serialized_prefix.clone());
        }

        let mut output = Vec::with_capacity(original.serialized_prefix.len());
        let mut original_fields = original.serialized_fields.iter();
        if version >= 33 {
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.net_node,
                &original.net_node,
            )?;
        }
        if version >= 208 {
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.sub_building,
                &original.sub_building,
            )?;
        }
        if version >= 33 {
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.water_source,
                &original.water_source,
            )?;
        }
        if (95..184).contains(&version) {
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.legacy_water_source_release,
                &original.legacy_water_source_release,
            )?;
        }
        if version >= 33 {
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.electricity_buffer,
                &original.electricity_buffer,
            )?;
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.water_buffer,
                &original.water_buffer,
            )?;
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.sewage_buffer,
                &original.sewage_buffer,
            )?;
        }
        if version >= 227 {
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.heating_buffer,
                &original.heating_buffer,
            )?;
        }
        if version >= 33 {
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.garbage_buffer,
                &original.garbage_buffer,
            )?;
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.crime_buffer,
                &original.crime_buffer,
            )?;
        }
        if version >= 111_003 {
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.mail_buffer,
                &original.mail_buffer,
            )?;
        }
        if version >= 33 {
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.custom_buffer1,
                &original.custom_buffer1,
            )?;
        }
        if version >= 66 {
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.custom_buffer2,
                &original.custom_buffer2,
            )?;
        }
        if version >= 308 {
            write_ushort_field(
                &mut output,
                &mut original_fields,
                &self.build_water_height,
                &original.build_water_height,
            )?;
        }
        if version >= 33 {
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.production_rate,
                &original.production_rate,
            )?;
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.water_pollution,
                &original.water_pollution,
            )?;
        }
        if original_fields.next().is_some() {
            return Err(invalid_building_encoded_fields(
                output.len(),
                "source contains more serialized fields than the version allows",
            ));
        }
        Ok(output)
    }

    pub(crate) fn original_prefix(&self) -> &[u8] {
        &self.original.serialized_prefix
    }
}

impl BuildingByteStatePrefix {
    /// Decodes the next versioned per-building byte arrays.
    ///
    /// Returns the decoded prefix and bytes consumed. Truncated data or a
    /// mismatched flag-table version returns an error; later BuildingManager
    /// arrays are left unread.
    pub fn decode_prefix(
        format_version: u32,
        flags: &BuildingFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager byte state".to_owned(),
                offset: 0,
                reason: "unsupported or mismatched inner version".to_owned(),
            });
        }
        let count = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .count();
        let mut reader = Reader::new(bytes);
        let mut serialized_fields = Vec::new();
        let construct_state = if format_version >= 33 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let fire_intensity = if format_version >= 35 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let fire_damage = if format_version >= 35 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let production_state = if format_version >= 81 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let health = if format_version >= 55 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let child_health = if format_version >= 113_004 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let senior_health = if format_version >= 113_004 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let happiness = if format_version >= 55 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let citizen_count = if format_version >= 55 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let temp_import = if format_version >= 72 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let temp_export = if format_version >= 72 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let final_import = if format_version >= 72 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let final_export = if format_version >= 72 {
            read_byte_field(&mut reader, count, &mut serialized_fields)?
        } else {
            Vec::new()
        };
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                construct_state,
                fire_intensity,
                fire_damage,
                production_state,
                health,
                child_health,
                senior_health,
                happiness,
                citizen_count,
                temp_import,
                temp_export,
                final_import,
                final_export,
                serialized_fields,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns raw construction-state bytes.
    pub fn construct_state(&self) -> &[u8] {
        &self.construct_state
    }
    /// Returns raw fire-intensity bytes from version 35 onward.
    pub fn fire_intensity(&self) -> &[u8] {
        &self.fire_intensity
    }
    /// Returns raw fire-damage bytes from version 35 onward.
    pub fn fire_damage(&self) -> &[u8] {
        &self.fire_damage
    }
    /// Returns raw production-state bytes from version 81 onward.
    pub fn production_state(&self) -> &[u8] {
        &self.production_state
    }
    /// Returns raw health bytes from version 55 onward.
    pub fn health(&self) -> &[u8] {
        &self.health
    }
    /// Returns raw child-health bytes from version 113004 onward.
    pub fn child_health(&self) -> &[u8] {
        &self.child_health
    }
    /// Returns raw senior-health bytes from version 113004 onward.
    pub fn senior_health(&self) -> &[u8] {
        &self.senior_health
    }
    /// Returns raw happiness bytes from version 55 onward.
    pub fn happiness(&self) -> &[u8] {
        &self.happiness
    }
    /// Returns raw citizen-count bytes from version 55 onward.
    pub fn citizen_count(&self) -> &[u8] {
        &self.citizen_count
    }
    /// Returns raw temporary-import bytes from version 72 onward.
    pub fn temp_import(&self) -> &[u8] {
        &self.temp_import
    }
    /// Returns raw temporary-export bytes from version 72 onward.
    pub fn temp_export(&self) -> &[u8] {
        &self.temp_export
    }
    /// Returns raw final-import bytes from version 72 onward.
    pub fn final_import(&self) -> &[u8] {
        &self.final_import
    }
    /// Returns raw final-export bytes from version 72 onward.
    pub fn final_export(&self) -> &[u8] {
        &self.final_export
    }
    /// Writes the original encoded fields exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Creates an editor for the existing active-building byte arrays.
    pub fn into_editor(self) -> BuildingByteStateEditor {
        BuildingByteStateEditor {
            construct_state: self.construct_state.clone(),
            fire_intensity: self.fire_intensity.clone(),
            fire_damage: self.fire_damage.clone(),
            production_state: self.production_state.clone(),
            health: self.health.clone(),
            child_health: self.child_health.clone(),
            senior_health: self.senior_health.clone(),
            happiness: self.happiness.clone(),
            citizen_count: self.citizen_count.clone(),
            temp_import: self.temp_import.clone(),
            temp_export: self.temp_export.clone(),
            final_import: self.final_import.clone(),
            final_export: self.final_export.clone(),
            original: self,
        }
    }
}

impl BuildingByteStateEditor {
    /// Returns the inner DataSerializer version associated with these arrays.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns editable raw construction-state bytes.
    pub fn construct_state_mut(&mut self) -> &mut [u8] {
        &mut self.construct_state
    }
    /// Returns editable raw fire-intensity bytes from version 35 onward.
    pub fn fire_intensity_mut(&mut self) -> &mut [u8] {
        &mut self.fire_intensity
    }
    /// Returns editable raw fire-damage bytes from version 35 onward.
    pub fn fire_damage_mut(&mut self) -> &mut [u8] {
        &mut self.fire_damage
    }
    /// Returns editable raw production-state bytes from version 81 onward.
    pub fn production_state_mut(&mut self) -> &mut [u8] {
        &mut self.production_state
    }
    /// Returns editable raw health bytes from version 55 onward.
    pub fn health_mut(&mut self) -> &mut [u8] {
        &mut self.health
    }
    /// Returns editable raw child-health bytes from version 113004 onward.
    pub fn child_health_mut(&mut self) -> &mut [u8] {
        &mut self.child_health
    }
    /// Returns editable raw senior-health bytes from version 113004 onward.
    pub fn senior_health_mut(&mut self) -> &mut [u8] {
        &mut self.senior_health
    }
    /// Returns editable raw happiness bytes from version 55 onward.
    pub fn happiness_mut(&mut self) -> &mut [u8] {
        &mut self.happiness
    }
    /// Returns editable raw citizen-count bytes from version 55 onward.
    pub fn citizen_count_mut(&mut self) -> &mut [u8] {
        &mut self.citizen_count
    }
    /// Returns editable raw temporary-import bytes from version 72 onward.
    pub fn temp_import_mut(&mut self) -> &mut [u8] {
        &mut self.temp_import
    }
    /// Returns editable raw temporary-export bytes from version 72 onward.
    pub fn temp_export_mut(&mut self) -> &mut [u8] {
        &mut self.temp_export
    }
    /// Returns editable raw final-import bytes from version 72 onward.
    pub fn final_import_mut(&mut self) -> &mut [u8] {
        &mut self.final_import
    }
    /// Returns editable raw final-export bytes from version 72 onward.
    pub fn final_export_mut(&mut self) -> &mut [u8] {
        &mut self.final_export
    }

    /// Writes changed byte arrays canonically and preserves unedited source bytes.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let version = self.format_version();
        if version < 164 {
            return Err(invalid_building_encoded_fields(
                0,
                "unsupported inner version",
            ));
        }
        let original = &self.original;
        if self.construct_state == original.construct_state
            && self.fire_intensity == original.fire_intensity
            && self.fire_damage == original.fire_damage
            && self.production_state == original.production_state
            && self.health == original.health
            && self.child_health == original.child_health
            && self.senior_health == original.senior_health
            && self.happiness == original.happiness
            && self.citizen_count == original.citizen_count
            && self.temp_import == original.temp_import
            && self.temp_export == original.temp_export
            && self.final_import == original.final_import
            && self.final_export == original.final_export
        {
            return Ok(original.serialized_prefix.clone());
        }

        let mut output = Vec::with_capacity(original.serialized_prefix.len());
        let mut original_fields = original.serialized_fields.iter();
        if version >= 33 {
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.construct_state,
                &original.construct_state,
            )?;
        }
        if version >= 35 {
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.fire_intensity,
                &original.fire_intensity,
            )?;
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.fire_damage,
                &original.fire_damage,
            )?;
        }
        if version >= 81 {
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.production_state,
                &original.production_state,
            )?;
        }
        if version >= 55 {
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.health,
                &original.health,
            )?;
        }
        if version >= 113_004 {
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.child_health,
                &original.child_health,
            )?;
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.senior_health,
                &original.senior_health,
            )?;
        }
        if version >= 55 {
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.happiness,
                &original.happiness,
            )?;
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.citizen_count,
                &original.citizen_count,
            )?;
        }
        if version >= 72 {
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.temp_import,
                &original.temp_import,
            )?;
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.temp_export,
                &original.temp_export,
            )?;
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.final_import,
                &original.final_import,
            )?;
            write_byte_field(
                &mut output,
                &mut original_fields,
                &self.final_export,
                &original.final_export,
            )?;
        }
        if original_fields.next().is_some() {
            return Err(invalid_building_encoded_fields(
                output.len(),
                "source contains more byte-state fields than the version allows",
            ));
        }
        Ok(output)
    }

    pub(crate) fn original_prefix(&self) -> &[u8] {
        &self.original.serialized_prefix
    }
}

impl BuildingTimerGeometryPrefix {
    /// Decodes the timer and dimension arrays after the BuildingManager byte state.
    ///
    /// Supports inner versions 164 and newer. Returns the number of bytes used;
    /// truncated arrays and mismatched versions return errors.
    pub fn decode_prefix(
        format_version: u32,
        flags: &BuildingFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager timer and geometry".to_owned(),
                offset: 0,
                reason: "unsupported or mismatched inner version".to_owned(),
            });
        }
        let active_building_count = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .count();
        let timer_fields = timer_fields_for_version(format_version);
        let timer_count = active_building_count
            .checked_mul(timer_fields.len())
            .ok_or(CrpError::ArithmeticOverflow(
                "BuildingManager timer value count",
            ))?;
        let mut reader = Reader::new(bytes);
        let timer_values = read_encoded_byte_array(&mut reader, timer_count)?;
        let level_up_progress = if format_version >= 170 {
            read_encoded_byte_array(&mut reader, active_building_count)?
        } else {
            Vec::new()
        };
        let width = read_encoded_byte_array(&mut reader, active_building_count)?;
        let length = read_encoded_byte_array(&mut reader, active_building_count)?;
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                active_building_count,
                timer_fields,
                timer_values,
                level_up_progress,
                width,
                length,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the number of buildings with nonzero primary flags.
    pub fn active_building_count(&self) -> usize {
        self.active_building_count
    }

    /// Returns timer columns in their serialized field-major order.
    pub fn timer_fields(&self) -> &[BuildingTimerField] {
        &self.timer_fields
    }

    /// Returns one timer column, or `None` when the field is absent in this version.
    pub fn timer(&self, field: BuildingTimerField) -> Option<&[u8]> {
        let index = self
            .timer_fields
            .iter()
            .position(|candidate| *candidate == field)?;
        let start = index.checked_mul(self.active_building_count)?;
        let end = start.checked_add(self.active_building_count)?;
        self.timer_values.get(start..end)
    }

    /// Returns level-up progress for active buildings from inner version 170.
    pub fn level_up_progress(&self) -> &[u8] {
        &self.level_up_progress
    }

    /// Returns building widths for active buildings.
    pub fn width(&self) -> &[u8] {
        &self.width
    }

    /// Returns building lengths for active buildings.
    pub fn length(&self) -> &[u8] {
        &self.length
    }

    /// Writes the original encoded fields exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    pub(crate) fn encoded_len(&self) -> usize {
        self.serialized_prefix.len()
    }

    /// Creates an editor; changed arrays will use canonical literal encoding.
    pub fn into_editor(self) -> BuildingTimerGeometryEditor {
        BuildingTimerGeometryEditor {
            format_version: self.format_version,
            active_building_count: self.active_building_count,
            timer_fields: self.timer_fields,
            timer_values: self.timer_values,
            level_up_progress: self.level_up_progress,
            width: self.width,
            length: self.length,
        }
    }
}

impl BuildingTimerGeometryEditor {
    /// Returns the inner DataSerializer version associated with these arrays.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the number of active buildings represented by these arrays.
    pub fn active_building_count(&self) -> usize {
        self.active_building_count
    }

    /// Returns one editable timer column, or `None` when absent in this version.
    pub fn timer_mut(&mut self, field: BuildingTimerField) -> Option<&mut [u8]> {
        let index = self
            .timer_fields
            .iter()
            .position(|candidate| *candidate == field)?;
        let start = index.checked_mul(self.active_building_count)?;
        let end = start.checked_add(self.active_building_count)?;
        self.timer_values.get_mut(start..end)
    }

    /// Returns editable level-up progress from inner version 170 onward.
    pub fn level_up_progress_mut(&mut self) -> &mut [u8] {
        &mut self.level_up_progress
    }

    /// Returns editable building widths.
    pub fn width_mut(&mut self) -> &mut [u8] {
        &mut self.width
    }

    /// Returns editable building lengths.
    pub fn length_mut(&mut self) -> &mut [u8] {
        &mut self.length
    }

    /// Encodes timer columns and dimensions with valid canonical byte-array controls.
    ///
    /// The game writes timer columns through one shared array in field-major
    /// order. Edited output uses literal runs of at most 128 bytes and may not
    /// match the original game's control-block grouping.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.format_version < 164
            || self.timer_fields != timer_fields_for_version(self.format_version)
        {
            return Err(invalid_timer("invalid timer version or fields"));
        }
        let expected_timer_count = self
            .active_building_count
            .checked_mul(self.timer_fields.len())
            .ok_or(CrpError::ArithmeticOverflow(
                "BuildingManager timer value count",
            ))?;
        if self.timer_values.len() != expected_timer_count
            || self.level_up_progress.len()
                != if self.format_version >= 170 {
                    self.active_building_count
                } else {
                    0
                }
            || self.width.len() != self.active_building_count
            || self.length.len() != self.active_building_count
        {
            return Err(invalid_timer(
                "timer, level, width, or length count is invalid",
            ));
        }
        let mut output = Vec::new();
        write_encoded_byte_array(&mut output, &self.timer_values);
        write_encoded_byte_array(&mut output, &self.level_up_progress);
        write_encoded_byte_array(&mut output, &self.width);
        write_encoded_byte_array(&mut output, &self.length);
        Ok(output)
    }
}

impl BuildingPopulationPrefix {
    /// Decodes population-related arrays after timer and geometry state.
    ///
    /// Supports inner versions 164 and newer. The original encoded bytes are
    /// retained for exact writeback; truncated arrays or mismatched flag-table
    /// versions return an error.
    pub fn decode_prefix(
        format_version: u32,
        flags: &BuildingFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || flags.format_version() != format_version {
            return Err(invalid_population(
                "unsupported or mismatched inner version",
            ));
        }
        let active_building_count = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .count();
        let mut reader = Reader::new(bytes);
        let education1 = read_encoded_byte_array(&mut reader, active_building_count)?;
        let education2 = read_encoded_byte_array(&mut reader, active_building_count)?;
        let education3 = read_encoded_byte_array(&mut reader, active_building_count)?;
        let children = if format_version >= 113_005 {
            read_encoded_byte_array(&mut reader, active_building_count)?
        } else {
            Vec::new()
        };
        let teens = read_encoded_byte_array(&mut reader, active_building_count)?;
        let youngs = read_encoded_byte_array(&mut reader, active_building_count)?;
        let adults = read_encoded_byte_array(&mut reader, active_building_count)?;
        let seniors = read_encoded_byte_array(&mut reader, active_building_count)?;
        let fire_hazard = if format_version >= 175 {
            read_encoded_byte_array(&mut reader, active_building_count)?
        } else {
            Vec::new()
        };
        // Before 111005 the game consumes and discards this column from 207 onward.
        let level = if format_version >= 207 {
            read_encoded_byte_array(&mut reader, active_building_count)?
        } else {
            Vec::new()
        };
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                active_building_count,
                education1,
                education2,
                education3,
                children,
                teens,
                youngs,
                adults,
                seniors,
                fire_hazard,
                level,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns the number of buildings with nonzero primary flags.
    pub fn active_building_count(&self) -> usize {
        self.active_building_count
    }
    /// Returns education-level-1 values from version 98 onward.
    pub fn education1(&self) -> &[u8] {
        &self.education1
    }
    /// Returns education-level-2 values from version 98 onward.
    pub fn education2(&self) -> &[u8] {
        &self.education2
    }
    /// Returns education-level-3 values from version 98 onward.
    pub fn education3(&self) -> &[u8] {
        &self.education3
    }
    /// Returns child counts from version 113005 onward.
    pub fn children(&self) -> &[u8] {
        &self.children
    }
    /// Returns teen counts from version 98 onward.
    pub fn teens(&self) -> &[u8] {
        &self.teens
    }
    /// Returns young-adult counts from version 98 onward.
    pub fn youngs(&self) -> &[u8] {
        &self.youngs
    }
    /// Returns adult counts from version 98 onward.
    pub fn adults(&self) -> &[u8] {
        &self.adults
    }
    /// Returns senior counts from version 98 onward.
    pub fn seniors(&self) -> &[u8] {
        &self.seniors
    }
    /// Returns raw fire-hazard values from version 175 onward.
    pub fn fire_hazard(&self) -> &[u8] {
        &self.fire_hazard
    }
    /// Returns raw serialized level bytes from version 207 onward.
    ///
    /// The game ignores these values before version 111005; versions below 207
    /// encode the legacy level in flag bits instead and have no separate column.
    pub fn level(&self) -> &[u8] {
        &self.level
    }
    /// Writes the original encoded fields exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Creates an editor; changed arrays will use canonical literal encoding.
    pub fn into_editor(self) -> BuildingPopulationEditor {
        BuildingPopulationEditor {
            format_version: self.format_version,
            active_building_count: self.active_building_count,
            education1: self.education1,
            education2: self.education2,
            education3: self.education3,
            children: self.children,
            teens: self.teens,
            youngs: self.youngs,
            adults: self.adults,
            seniors: self.seniors,
            fire_hazard: self.fire_hazard,
            level: self.level,
        }
    }
}

impl BuildingPopulationEditor {
    /// Returns the inner DataSerializer version associated with these arrays.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns the number of active buildings represented by these arrays.
    pub fn active_building_count(&self) -> usize {
        self.active_building_count
    }
    /// Returns editable education-level-1 values.
    pub fn education1_mut(&mut self) -> &mut [u8] {
        &mut self.education1
    }
    /// Returns editable education-level-2 values.
    pub fn education2_mut(&mut self) -> &mut [u8] {
        &mut self.education2
    }
    /// Returns editable education-level-3 values.
    pub fn education3_mut(&mut self) -> &mut [u8] {
        &mut self.education3
    }
    /// Returns editable child counts, empty before version 113005.
    pub fn children_mut(&mut self) -> &mut [u8] {
        &mut self.children
    }
    /// Returns editable teen counts.
    pub fn teens_mut(&mut self) -> &mut [u8] {
        &mut self.teens
    }
    /// Returns editable young-adult counts.
    pub fn youngs_mut(&mut self) -> &mut [u8] {
        &mut self.youngs
    }
    /// Returns editable adult counts.
    pub fn adults_mut(&mut self) -> &mut [u8] {
        &mut self.adults
    }
    /// Returns editable senior counts.
    pub fn seniors_mut(&mut self) -> &mut [u8] {
        &mut self.seniors
    }
    /// Returns editable fire-hazard values, empty before version 175.
    pub fn fire_hazard_mut(&mut self) -> &mut [u8] {
        &mut self.fire_hazard
    }
    /// Returns editable raw level bytes, empty before version 207.
    pub fn level_mut(&mut self) -> &mut [u8] {
        &mut self.level
    }

    /// Encodes the version-gated arrays with canonical literal controls.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.format_version < 164 {
            return Err(invalid_population("unsupported inner version"));
        }
        let count = self.active_building_count;
        let has_children = self.format_version >= 113_005;
        let has_fire_hazard = self.format_version >= 175;
        let has_level = self.format_version >= 207;
        if [
            self.education1.len(),
            self.education2.len(),
            self.education3.len(),
            self.teens.len(),
            self.youngs.len(),
            self.adults.len(),
            self.seniors.len(),
        ]
        .iter()
        .any(|length| *length != count)
            || self.children.len() != if has_children { count } else { 0 }
            || self.fire_hazard.len() != if has_fire_hazard { count } else { 0 }
            || self.level.len() != if has_level { count } else { 0 }
        {
            return Err(invalid_population(
                "one or more array counts do not match version and active buildings",
            ));
        }
        let mut output = Vec::new();
        for array in [&self.education1, &self.education2, &self.education3] {
            write_encoded_byte_array(&mut output, array);
        }
        if has_children {
            write_encoded_byte_array(&mut output, &self.children);
        }
        for array in [&self.teens, &self.youngs, &self.adults, &self.seniors] {
            write_encoded_byte_array(&mut output, array);
        }
        if has_fire_hazard {
            write_encoded_byte_array(&mut output, &self.fire_hazard);
        }
        if has_level {
            write_encoded_byte_array(&mut output, &self.level);
        }
        Ok(output)
    }
}

impl BuildingEconomicFieldsPrefix {
    /// Decodes version-gated traffic, room, employment, and routing arrays.
    pub fn decode_prefix(
        format_version: u32,
        flags: &BuildingFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || flags.format_version() != format_version {
            return Err(invalid_economic("unsupported or mismatched inner version"));
        }
        let active_building_count = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .count();
        let mut reader = Reader::new(bytes);
        let cargo_traffic_rate = if format_version >= 115_004 {
            read_encoded_uint_array(&mut reader, active_building_count)?
        } else {
            Vec::new()
        };
        let garbage_traffic_rate = if format_version >= 115_022 {
            read_encoded_uint_array(&mut reader, active_building_count)?
        } else {
            Vec::new()
        };
        let access_segment = if format_version >= 115_013 {
            read_encoded_ushort_array(&mut reader, active_building_count)?
        } else {
            Vec::new()
        };
        let event_access_segment = if format_version >= 121_017 {
            read_encoded_ushort_array(&mut reader, active_building_count)?
        } else {
            Vec::new()
        };
        let cash_buffer = if format_version >= 116_006 {
            read_encoded_int_array(&mut reader, active_building_count)?
        } else if format_version >= 116_001 {
            read_encoded_ushort_array(&mut reader, active_building_count)?
                .into_iter()
                .map(i32::from)
                .collect()
        } else {
            Vec::new()
        };
        let (room_cost, room_cost_target, room_max, room_used) = if format_version >= 117_005 {
            (
                read_encoded_ushort_array(&mut reader, active_building_count)?,
                read_encoded_ushort_array(&mut reader, active_building_count)?,
                read_encoded_ushort_array(&mut reader, active_building_count)?,
                read_encoded_ushort_array(&mut reader, active_building_count)?,
            )
        } else {
            (Vec::new(), Vec::new(), Vec::new(), Vec::new())
        };
        let (
            educated0_unemployed,
            educated1_unemployed,
            educated2_unemployed,
            educated3_unemployed,
            all_unemployed,
        ) = if format_version >= 120_001 {
            (
                read_encoded_byte_array(&mut reader, active_building_count)?,
                read_encoded_byte_array(&mut reader, active_building_count)?,
                read_encoded_byte_array(&mut reader, active_building_count)?,
                read_encoded_byte_array(&mut reader, active_building_count)?,
                read_encoded_byte_array(&mut reader, active_building_count)?,
            )
        } else {
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new())
        };
        let event_route_index = if format_version >= 121_005 {
            read_encoded_ushort_array(&mut reader, active_building_count)?
        } else {
            Vec::new()
        };
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                active_building_count,
                cargo_traffic_rate,
                garbage_traffic_rate,
                access_segment,
                event_access_segment,
                cash_buffer,
                room_cost,
                room_cost_target,
                room_max,
                room_used,
                educated0_unemployed,
                educated1_unemployed,
                educated2_unemployed,
                educated3_unemployed,
                all_unemployed,
                event_route_index,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns the active-building count represented by these arrays.
    pub fn active_building_count(&self) -> usize {
        self.active_building_count
    }
    /// Returns cargo traffic values, empty before version 115004.
    pub fn cargo_traffic_rate(&self) -> &[u32] {
        &self.cargo_traffic_rate
    }
    /// Returns garbage traffic values, empty before version 115022.
    pub fn garbage_traffic_rate(&self) -> &[u32] {
        &self.garbage_traffic_rate
    }
    /// Returns access segment indices, empty before version 115013.
    pub fn access_segment(&self) -> &[u16] {
        &self.access_segment
    }
    /// Returns event access segment indices, empty before version 121017.
    pub fn event_access_segment(&self) -> &[u16] {
        &self.event_access_segment
    }
    /// Returns cash buffer values, empty before version 116001.
    pub fn cash_buffer(&self) -> &[i32] {
        &self.cash_buffer
    }
    /// Returns room-cost values, empty before version 117005.
    pub fn room_cost(&self) -> &[u16] {
        &self.room_cost
    }
    /// Returns target room-cost values, empty before version 117005.
    pub fn room_cost_target(&self) -> &[u16] {
        &self.room_cost_target
    }
    /// Returns maximum room values, empty before version 117005.
    pub fn room_max(&self) -> &[u16] {
        &self.room_max
    }
    /// Returns used room values, empty before version 117005.
    pub fn room_used(&self) -> &[u16] {
        &self.room_used
    }
    /// Returns unemployed counts for education level zero, empty before 120001.
    pub fn educated0_unemployed(&self) -> &[u8] {
        &self.educated0_unemployed
    }
    /// Returns unemployed counts for education level one, empty before 120001.
    pub fn educated1_unemployed(&self) -> &[u8] {
        &self.educated1_unemployed
    }
    /// Returns unemployed counts for education level two, empty before 120001.
    pub fn educated2_unemployed(&self) -> &[u8] {
        &self.educated2_unemployed
    }
    /// Returns unemployed counts for education level three, empty before 120001.
    pub fn educated3_unemployed(&self) -> &[u8] {
        &self.educated3_unemployed
    }
    /// Returns total unemployed counts, empty before 120001.
    pub fn all_unemployed(&self) -> &[u8] {
        &self.all_unemployed
    }
    /// Returns event route indices, empty before version 121005.
    pub fn event_route_index(&self) -> &[u16] {
        &self.event_route_index
    }
    /// Writes the original encoded fields exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Creates an editor; changed fields use valid canonical array encoding.
    pub fn into_editor(self) -> BuildingEconomicFieldsEditor {
        BuildingEconomicFieldsEditor {
            format_version: self.format_version,
            active_building_count: self.active_building_count,
            cargo_traffic_rate: self.cargo_traffic_rate,
            garbage_traffic_rate: self.garbage_traffic_rate,
            access_segment: self.access_segment,
            event_access_segment: self.event_access_segment,
            cash_buffer: self.cash_buffer,
            room_cost: self.room_cost,
            room_cost_target: self.room_cost_target,
            room_max: self.room_max,
            room_used: self.room_used,
            educated0_unemployed: self.educated0_unemployed,
            educated1_unemployed: self.educated1_unemployed,
            educated2_unemployed: self.educated2_unemployed,
            educated3_unemployed: self.educated3_unemployed,
            all_unemployed: self.all_unemployed,
            event_route_index: self.event_route_index,
        }
    }
}

impl BuildingEconomicFieldsEditor {
    /// Returns the inner DataSerializer version associated with these fields.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns the active-building count represented by these fields.
    pub fn active_building_count(&self) -> usize {
        self.active_building_count
    }
    /// Returns editable cargo traffic values.
    pub fn cargo_traffic_rate_mut(&mut self) -> &mut [u32] {
        &mut self.cargo_traffic_rate
    }
    /// Returns editable garbage traffic values.
    pub fn garbage_traffic_rate_mut(&mut self) -> &mut [u32] {
        &mut self.garbage_traffic_rate
    }
    /// Returns editable access segment indices.
    pub fn access_segment_mut(&mut self) -> &mut [u16] {
        &mut self.access_segment
    }
    /// Returns editable event access segment indices.
    pub fn event_access_segment_mut(&mut self) -> &mut [u16] {
        &mut self.event_access_segment
    }
    /// Returns editable cash buffer values.
    pub fn cash_buffer_mut(&mut self) -> &mut [i32] {
        &mut self.cash_buffer
    }
    /// Returns editable room-cost values.
    pub fn room_cost_mut(&mut self) -> &mut [u16] {
        &mut self.room_cost
    }
    /// Returns editable target room-cost values.
    pub fn room_cost_target_mut(&mut self) -> &mut [u16] {
        &mut self.room_cost_target
    }
    /// Returns editable maximum room values.
    pub fn room_max_mut(&mut self) -> &mut [u16] {
        &mut self.room_max
    }
    /// Returns editable used room values.
    pub fn room_used_mut(&mut self) -> &mut [u16] {
        &mut self.room_used
    }
    /// Returns editable unemployed counts for education level zero.
    pub fn educated0_unemployed_mut(&mut self) -> &mut [u8] {
        &mut self.educated0_unemployed
    }
    /// Returns editable unemployed counts for education level one.
    pub fn educated1_unemployed_mut(&mut self) -> &mut [u8] {
        &mut self.educated1_unemployed
    }
    /// Returns editable unemployed counts for education level two.
    pub fn educated2_unemployed_mut(&mut self) -> &mut [u8] {
        &mut self.educated2_unemployed
    }
    /// Returns editable unemployed counts for education level three.
    pub fn educated3_unemployed_mut(&mut self) -> &mut [u8] {
        &mut self.educated3_unemployed
    }
    /// Returns editable total unemployed counts.
    pub fn all_unemployed_mut(&mut self) -> &mut [u8] {
        &mut self.all_unemployed
    }
    /// Returns editable event route indices.
    pub fn event_route_index_mut(&mut self) -> &mut [u16] {
        &mut self.event_route_index
    }

    /// Encodes the version-gated arrays and checks each active-building count.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.format_version < 164 {
            return Err(invalid_economic("unsupported inner version"));
        }
        let count = self.active_building_count;
        let gate_counts = [
            (
                self.format_version >= 115_004,
                self.cargo_traffic_rate.len(),
            ),
            (
                self.format_version >= 115_022,
                self.garbage_traffic_rate.len(),
            ),
            (self.format_version >= 115_013, self.access_segment.len()),
            (
                self.format_version >= 121_017,
                self.event_access_segment.len(),
            ),
            (self.format_version >= 116_001, self.cash_buffer.len()),
            (self.format_version >= 117_005, self.room_cost.len()),
            (self.format_version >= 117_005, self.room_cost_target.len()),
            (self.format_version >= 117_005, self.room_max.len()),
            (self.format_version >= 117_005, self.room_used.len()),
            (
                self.format_version >= 120_001,
                self.educated0_unemployed.len(),
            ),
            (
                self.format_version >= 120_001,
                self.educated1_unemployed.len(),
            ),
            (
                self.format_version >= 120_001,
                self.educated2_unemployed.len(),
            ),
            (
                self.format_version >= 120_001,
                self.educated3_unemployed.len(),
            ),
            (self.format_version >= 120_001, self.all_unemployed.len()),
            (self.format_version >= 121_005, self.event_route_index.len()),
        ];
        if gate_counts
            .iter()
            .any(|(present, actual)| *actual != if *present { count } else { 0 })
        {
            return Err(invalid_economic(
                "one or more array counts do not match version and active buildings",
            ));
        }
        if self.format_version < 116_006
            && self.format_version >= 116_001
            && self
                .cash_buffer
                .iter()
                .any(|value| !(0..=i32::from(u16::MAX)).contains(value))
        {
            return Err(invalid_economic(
                "legacy cash buffer value does not fit in u16",
            ));
        }
        let mut output = Vec::new();
        if self.format_version >= 115_004 {
            write_encoded_uint_array(&mut output, &self.cargo_traffic_rate);
        }
        if self.format_version >= 115_022 {
            write_encoded_uint_array(&mut output, &self.garbage_traffic_rate);
        }
        if self.format_version >= 115_013 {
            write_encoded_ushort_array(&mut output, &self.access_segment);
        }
        if self.format_version >= 121_017 {
            write_encoded_ushort_array(&mut output, &self.event_access_segment);
        }
        if self.format_version >= 116_006 {
            write_encoded_int_array(&mut output, &self.cash_buffer);
        } else if self.format_version >= 116_001 {
            let values = self
                .cash_buffer
                .iter()
                .map(|value| *value as u16)
                .collect::<Vec<_>>();
            write_encoded_ushort_array(&mut output, &values);
        }
        if self.format_version >= 117_005 {
            for values in [
                &self.room_cost,
                &self.room_cost_target,
                &self.room_max,
                &self.room_used,
            ] {
                write_encoded_ushort_array(&mut output, values);
            }
        }
        if self.format_version >= 120_001 {
            for values in [
                &self.educated0_unemployed,
                &self.educated1_unemployed,
                &self.educated2_unemployed,
                &self.educated3_unemployed,
                &self.all_unemployed,
            ] {
                write_encoded_byte_array(&mut output, values);
            }
        }
        if self.format_version >= 121_005 {
            write_encoded_ushort_array(&mut output, &self.event_route_index);
        }
        Ok(output)
    }
}

impl BuildingServiceSelectionPrefix {
    /// Decodes the service-selection tables using the stream's shared-string state.
    ///
    /// `shared_strings` must contain every shared string read earlier in this
    /// DataSerializer stream, in original order. The returned prefix includes
    /// the updated table snapshot and original bytes for exact writeback.
    pub fn decode_prefix(
        format_version: u32,
        bytes: &[u8],
        mut shared_strings: Vec<Option<String>>,
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(invalid_service_selection("unsupported inner version"));
        }
        let mut reader = Reader::new(bytes);
        let vehicle_selection = if format_version >= 114_001 {
            read_service_selection_table(&mut reader, &mut shared_strings)?
        } else {
            Vec::new()
        };
        let additional_vehicle_selection = if format_version >= 116_020 {
            read_service_selection_table(&mut reader, &mut shared_strings)?
        } else {
            Vec::new()
        };
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                vehicle_selection,
                additional_vehicle_selection,
                shared_strings,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns the primary vehicle-selection entries in serialized order.
    pub fn vehicle_selection(&self) -> &[BuildingServiceSelection] {
        &self.vehicle_selection
    }
    /// Returns additional vehicle-selection entries from version 116020 onward.
    pub fn additional_vehicle_selection(&self) -> &[BuildingServiceSelection] {
        &self.additional_vehicle_selection
    }
    /// Returns shared strings known after these tables, including prior entries.
    pub fn shared_strings(&self) -> &[Option<String>] {
        &self.shared_strings
    }
    /// Writes the original encoded table bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl BuildingInstanceRecordsPrefix {
    /// Decodes fixed-layout records for every building with nonzero primary flags.
    ///
    /// The supported 164+ stream layout uses `u32`, three float bit patterns,
    /// one byte, and a 24-bit citizen-unit index per active building.
    pub fn decode_prefix(
        format_version: u32,
        flags: &BuildingFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || flags.format_version() != format_version {
            return Err(invalid_building_records(
                "unsupported or mismatched inner version",
            ));
        }
        let count = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .count();
        let expected_bytes = count.checked_mul(24).ok_or(CrpError::ArithmeticOverflow(
            "BuildingManager instance-record byte count",
        ))?;
        let mut reader = Reader::new(bytes);
        if expected_bytes > reader.remaining() {
            return Err(reader.invalid("truncated BuildingManager instance records"));
        }
        let mut records = Vec::with_capacity(count);
        for _ in 0..count {
            records.push(BuildingInstanceRecord {
                building_index: reader.read_u32_be()?,
                position_bits: [
                    reader.read_u32_be()?,
                    reader.read_u32_be()?,
                    reader.read_u32_be()?,
                ],
                angle_bits: reader.read_u32_be()?,
                base_height: reader.read_u8()?,
                citizen_units: reader.read_u24_be()?,
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
    /// Returns records in active primary-flag order.
    pub fn records(&self) -> &[BuildingInstanceRecord] {
        &self.records
    }
    /// Writes original record bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
    /// Creates an editor for record fields.
    pub fn into_editor(self) -> BuildingInstanceRecordsEditor {
        BuildingInstanceRecordsEditor {
            format_version: self.format_version,
            records: self.records,
        }
    }
}

impl BuildingInstanceRecordsEditor {
    /// Returns the inner DataSerializer version associated with these records.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns editable records in active primary-flag order.
    pub fn records_mut(&mut self) -> &mut [BuildingInstanceRecord] {
        &mut self.records
    }
    /// Returns the number of editable active-building records.
    pub fn record_count(&self) -> usize {
        self.records.len()
    }
    /// Encodes records in the fixed 24-byte layout used by inner version 164+.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.format_version < 164 {
            return Err(invalid_building_records("unsupported inner version"));
        }
        if self
            .records
            .iter()
            .any(|record| record.citizen_units > 0x00ff_ffff)
        {
            return Err(invalid_building_records(
                "citizen-unit index does not fit in 24 bits",
            ));
        }
        let capacity = self
            .records
            .len()
            .checked_mul(24)
            .ok_or(CrpError::ArithmeticOverflow(
                "BuildingManager instance-record byte count",
            ))?;
        let mut output = Vec::with_capacity(capacity);
        for record in &self.records {
            output.extend_from_slice(&record.building_index.to_be_bytes());
            for position in record.position_bits {
                output.extend_from_slice(&position.to_be_bytes());
            }
            output.extend_from_slice(&record.angle_bits.to_be_bytes());
            output.push(record.base_height);
            output.push((record.citizen_units >> 16) as u8);
            output.push((record.citizen_units >> 8) as u8);
            output.push(record.citizen_units as u8);
        }
        Ok(output)
    }
}

fn invalid_building_records(reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "BuildingManager instance records".to_owned(),
        offset: 0,
        reason: reason.into(),
    }
}

fn read_service_selection_table(
    reader: &mut Reader<'_>,
    shared_strings: &mut Vec<Option<String>>,
) -> Result<Vec<BuildingServiceSelection>, CrpError> {
    let count = reader.read_i16_be()?;
    if count < 0 {
        return Err(reader.invalid(format!("negative service-selection count {count}")));
    }
    let count = usize::try_from(count)
        .map_err(|_| reader.invalid("service-selection count does not fit this platform"))?;
    if count > reader.remaining() / 3 {
        return Err(reader.invalid("service-selection count cannot fit in remaining bytes"));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let service_index = reader.read_u16_be()?;
        let value = reader.read_shared_string(shared_strings)?;
        entries.push(BuildingServiceSelection {
            service_index,
            value,
        });
    }
    Ok(entries)
}

fn invalid_service_selection(reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "BuildingManager service-selection tables".to_owned(),
        offset: 0,
        reason: reason.into(),
    }
}

fn invalid_economic(reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "BuildingManager economic fields".to_owned(),
        offset: 0,
        reason: reason.into(),
    }
}

fn invalid_population(reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "BuildingManager population fields".to_owned(),
        offset: 0,
        reason: reason.into(),
    }
}

fn timer_fields_for_version(format_version: u32) -> Vec<BuildingTimerField> {
    let mut fields = vec![BuildingTimerField::Electricity];
    if format_version >= 231 {
        fields.push(BuildingTimerField::Heating);
    }
    fields.extend([
        BuildingTimerField::Water,
        BuildingTimerField::Worker,
        BuildingTimerField::Incoming,
        BuildingTimerField::Outgoing,
        BuildingTimerField::Health,
        BuildingTimerField::Death,
        BuildingTimerField::Service,
    ]);
    if format_version >= 172 {
        fields.push(BuildingTimerField::Tax);
    }
    fields.push(BuildingTimerField::Major);
    fields
}

fn invalid_timer(reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "BuildingManager timer and geometry".to_owned(),
        offset: 0,
        reason: reason.into(),
    }
}

impl BuildingProblemPrefix {
    /// Decodes versioned problem state for each active building.
    ///
    /// From inner version 115000 the game serializes two encoded 64-bit words.
    /// Versions 221–114999 use one 64-bit or 32-bit value depending on the
    /// saved-application version; earlier supported versions use one 32-bit
    /// value. A save-app version is required between inner versions 221 and
    /// 114999. The original encoded bytes are retained for exact writing.
    pub fn decode_prefix(
        format_version: u32,
        save_app_version: Option<u32>,
        flags: &BuildingFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || flags.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager problem words".to_owned(),
                offset: 0,
                reason: "unsupported or mismatched inner version".to_owned(),
            });
        }
        if (221..TWO_BUILDING_PROBLEMS_VERSION).contains(&format_version)
            && save_app_version.is_none()
        {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager problem words".to_owned(),
                offset: 0,
                reason: "save-app version is required for the legacy problem encoding".to_owned(),
            });
        }
        let active_indices = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .map(|building| building.index)
            .collect::<Vec<_>>();
        let problem_count = active_indices.len();
        let mut reader = Reader::new(bytes);
        let mut problems = Vec::with_capacity(problem_count);
        if format_version >= TWO_BUILDING_PROBLEMS_VERSION {
            let word_count = problem_count
                .checked_mul(2)
                .ok_or(CrpError::ArithmeticOverflow(
                    "BuildingManager problem word count",
                ))?;
            let words = read_encoded_ulong_array(&mut reader, word_count)?;
            for (index, pair) in active_indices.into_iter().zip(words.chunks_exact(2)) {
                problems.push(BuildingProblemWords {
                    index,
                    first: pair[0],
                    second: pair[1],
                });
            }
        } else if format_version >= 221
            && save_app_version.is_some_and(|version| version >= SAVE_APP_PROBLEMS_VERSION)
        {
            let words = read_encoded_ulong_array(&mut reader, problem_count)?;
            for (index, first) in active_indices.into_iter().zip(words) {
                problems.push(BuildingProblemWords {
                    index,
                    first,
                    second: 0,
                });
            }
        } else {
            let words = read_encoded_uint_array(&mut reader, problem_count)?;
            for (index, raw) in active_indices.into_iter().zip(words) {
                let raw = u64::from(raw);
                let first = ((raw & 0xc000_0000) << 32) | (raw & 0x3fff_ffff);
                problems.push(BuildingProblemWords {
                    index,
                    first,
                    second: 0,
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

    /// Returns one opaque word pair for each active building, in buffer order.
    pub fn problems(&self) -> &[BuildingProblemWords] {
        &self.problems
    }

    /// Writes the original encoded array exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
    /// Creates an editor for the decoded problem words.
    pub fn into_editor(self) -> BuildingProblemEditor {
        BuildingProblemEditor {
            format_version: self.format_version,
            active_indices: self.problems.iter().map(|problem| problem.index).collect(),
            problems: self.problems,
        }
    }
}

impl BuildingProblemEditor {
    /// Returns the inner DataSerializer version associated with these words.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns editable opaque problem words in active-building order.
    /// Changing a record's index is rejected during encoding because flags
    /// determine the index-to-record mapping.
    pub fn problems_mut(&mut self) -> &mut [BuildingProblemWords] {
        &mut self.problems
    }

    /// Returns the number of active-building problem records.
    pub fn problem_count(&self) -> usize {
        self.problems.len()
    }

    /// Encodes words using the version and saved-application gates from the game.
    /// Legacy 32-bit values are accepted only when the widened representation
    /// contains no bits that the old format cannot preserve.
    pub fn encode(&self, save_app_version: Option<u32>) -> Result<Vec<u8>, CrpError> {
        if self.format_version < 164 {
            return Err(invalid_building_problems("unsupported inner version"));
        }
        if self.problems.len() != self.active_indices.len()
            || self
                .problems
                .iter()
                .zip(&self.active_indices)
                .any(|(problem, index)| problem.index != *index)
        {
            return Err(invalid_building_problems(
                "problem record indices do not match the active-building order",
            ));
        }
        let mut output = Vec::new();
        if self.format_version >= TWO_BUILDING_PROBLEMS_VERSION {
            let value_count =
                self.problems
                    .len()
                    .checked_mul(2)
                    .ok_or(CrpError::ArithmeticOverflow(
                        "BuildingManager problem word count",
                    ))?;
            let mut values = Vec::with_capacity(value_count);
            for problem in &self.problems {
                values.push(problem.first);
                values.push(problem.second);
            }
            write_encoded_ulong_array(&mut output, &values);
        } else if (221..TWO_BUILDING_PROBLEMS_VERSION).contains(&self.format_version)
            && save_app_version.is_none()
        {
            return Err(invalid_building_problems(
                "save-app version is required for the legacy problem encoding",
            ));
        } else if (221..TWO_BUILDING_PROBLEMS_VERSION).contains(&self.format_version)
            && save_app_version.is_some_and(|version| version >= SAVE_APP_PROBLEMS_VERSION)
        {
            if self.problems.iter().any(|problem| problem.second != 0) {
                return Err(invalid_building_problems(
                    "legacy single-word problem encoding cannot store the second word",
                ));
            }
            write_encoded_ulong_array(
                &mut output,
                &self
                    .problems
                    .iter()
                    .map(|problem| problem.first)
                    .collect::<Vec<_>>(),
            );
        } else {
            const LEGACY_WORD_MASK: u64 = 0xc000_0000_3fff_ffff;
            let mut values = Vec::with_capacity(self.problems.len());
            for problem in &self.problems {
                if problem.second != 0 || problem.first & !LEGACY_WORD_MASK != 0 {
                    return Err(invalid_building_problems(
                        "problem word cannot be represented by the legacy 32-bit encoding",
                    ));
                }
                let value = ((problem.first >> 32) as u32 & 0xc000_0000)
                    | (problem.first as u32 & 0x3fff_ffff);
                values.push(value);
            }
            write_encoded_uint_array(&mut output, &values);
        }
        Ok(output)
    }
}

fn invalid_building_problems(reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "BuildingManager problem words".to_owned(),
        offset: 0,
        reason: reason.into(),
    }
}

impl BuildingMonumentEffectsPrefix {
    /// Decodes both sixteen-entry monument-effect tables from inner version 110016.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let (temporary, final_values, serialized_prefix, consumed) = read_effect_tables(
            format_version,
            MONUMENT_EFFECTS_VERSION,
            "BuildingManager monument effects",
            bytes,
        )?;
        Ok((
            Self {
                format_version,
                temporary,
                final_values,
                serialized_prefix,
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns the temporary monument-effect entries in serialized order.
    pub fn temporary(&self) -> &[BuildingEffectValue] {
        &self.temporary
    }
    /// Returns the final monument-effect entries in serialized order.
    pub fn final_values(&self) -> &[BuildingEffectValue] {
        &self.final_values
    }
    /// Writes the original encoded tables exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
    /// Creates an editor for both tables.
    pub fn into_editor(self) -> BuildingMonumentEffectsEditor {
        BuildingMonumentEffectsEditor {
            format_version: self.format_version,
            temporary: self.temporary,
            final_values: self.final_values,
        }
    }
}

impl BuildingMonumentEffectsEditor {
    /// Returns the inner DataSerializer version associated with these tables.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns the temporary monument-effect entries.
    pub fn temporary(&self) -> &[BuildingEffectValue] {
        &self.temporary
    }
    /// Returns the editable temporary monument-effect entries.
    pub fn temporary_mut(&mut self) -> &mut [BuildingEffectValue] {
        &mut self.temporary
    }
    /// Returns the editable final monument-effect entries.
    pub fn final_values_mut(&mut self) -> &mut [BuildingEffectValue] {
        &mut self.final_values
    }
    /// Returns the final monument-effect entries.
    pub fn final_values(&self) -> &[BuildingEffectValue] {
        &self.final_values
    }
    /// Encodes both fixed tables in the game's field order.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        write_effect_tables(
            self.format_version,
            MONUMENT_EFFECTS_VERSION,
            "BuildingManager monument effects",
            &self.temporary,
            &self.final_values,
        )
    }
}

impl BuildingFacultyBonusesPrefix {
    /// Decodes both sixteen-entry faculty-bonus tables from inner version 112001.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let (temporary, final_values, serialized_prefix, consumed) = read_effect_tables(
            format_version,
            FACULTY_BONUSES_VERSION,
            "BuildingManager faculty bonuses",
            bytes,
        )?;
        Ok((
            Self {
                format_version,
                temporary,
                final_values,
                serialized_prefix,
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns the temporary faculty-bonus entries in serialized order.
    pub fn temporary(&self) -> &[BuildingEffectValue] {
        &self.temporary
    }
    /// Returns the final faculty-bonus entries in serialized order.
    pub fn final_values(&self) -> &[BuildingEffectValue] {
        &self.final_values
    }
    /// Writes the original encoded tables exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
    /// Creates an editor for both tables.
    pub fn into_editor(self) -> BuildingFacultyBonusesEditor {
        BuildingFacultyBonusesEditor {
            format_version: self.format_version,
            temporary: self.temporary,
            final_values: self.final_values,
        }
    }
}

impl BuildingFacultyBonusesEditor {
    /// Returns the inner DataSerializer version associated with these tables.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns the temporary faculty-bonus entries.
    pub fn temporary(&self) -> &[BuildingEffectValue] {
        &self.temporary
    }
    /// Returns the editable temporary faculty-bonus entries.
    pub fn temporary_mut(&mut self) -> &mut [BuildingEffectValue] {
        &mut self.temporary
    }
    /// Returns the editable final faculty-bonus entries.
    pub fn final_values_mut(&mut self) -> &mut [BuildingEffectValue] {
        &mut self.final_values
    }
    /// Returns the final faculty-bonus entries.
    pub fn final_values(&self) -> &[BuildingEffectValue] {
        &self.final_values
    }
    /// Encodes both fixed tables in the game's field order.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        write_effect_tables(
            self.format_version,
            FACULTY_BONUSES_VERSION,
            "BuildingManager faculty bonuses",
            &self.temporary,
            &self.final_values,
        )
    }
}

fn read_effect_tables(
    format_version: u32,
    since_version: u32,
    name: &str,
    bytes: &[u8],
) -> Result<
    (
        Vec<BuildingEffectValue>,
        Vec<BuildingEffectValue>,
        Vec<u8>,
        usize,
    ),
    CrpError,
> {
    if format_version < 164 {
        return Err(invalid_building_effects(name, "unsupported inner version"));
    }
    let expected_count = if format_version >= since_version {
        BUILDING_EFFECT_TABLE_LENGTH
    } else {
        0
    };
    let expected_bytes = expected_count
        .checked_mul(2)
        .and_then(|count| count.checked_mul(16))
        .ok_or(CrpError::ArithmeticOverflow(
            "BuildingManager effect table size",
        ))?;
    if bytes.len() < expected_bytes {
        return Err(invalid_building_effects(name, "truncated effect tables"));
    }
    let mut reader = Reader::new(bytes);
    let temporary = read_effect_table(&mut reader, expected_count)?;
    let final_values = read_effect_table(&mut reader, expected_count)?;
    let consumed = reader.offset();
    Ok((
        temporary,
        final_values,
        bytes[..consumed].to_vec(),
        consumed,
    ))
}

fn read_effect_table(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<BuildingEffectValue>, CrpError> {
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(BuildingEffectValue {
            factor: reader.read_i32_be()?,
            position_bits: [
                reader.read_u32_be()?,
                reader.read_u32_be()?,
                reader.read_u32_be()?,
            ],
        });
    }
    Ok(values)
}

fn write_effect_tables(
    format_version: u32,
    since_version: u32,
    name: &str,
    temporary: &[BuildingEffectValue],
    final_values: &[BuildingEffectValue],
) -> Result<Vec<u8>, CrpError> {
    if format_version < 164 {
        return Err(invalid_building_effects(name, "unsupported inner version"));
    }
    let expected_count = if format_version >= since_version {
        BUILDING_EFFECT_TABLE_LENGTH
    } else {
        0
    };
    if temporary.len() != expected_count || final_values.len() != expected_count {
        return Err(invalid_building_effects(
            name,
            "effect-table entry count does not match the inner version",
        ));
    }
    let mut output = Vec::with_capacity(expected_count * 32);
    for value in temporary.iter().chain(final_values) {
        output.extend_from_slice(&value.factor.to_be_bytes());
        for position in value.position_bits {
            output.extend_from_slice(&position.to_be_bytes());
        }
    }
    Ok(output)
}

fn invalid_building_effects(name: &str, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: name.to_owned(),
        offset: 0,
        reason: reason.into(),
    }
}

impl BuildingRaceTeamUnassignedPrefix {
    /// Decodes one DataSerializer bool per active building from version 120002.
    /// The game treats only byte `01` as true; original bytes are retained.
    pub fn decode_prefix(
        format_version: u32,
        flags: &BuildingFlagsPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || flags.format_version() != format_version {
            return Err(invalid_building_race_team_unassigned(
                "unsupported or mismatched inner version",
            ));
        }
        let count = if format_version >= RACE_TEAM_UNASSIGNED_VERSION {
            flags
                .building_flags()
                .iter()
                .filter(|building| building.flags != 0)
                .count()
        } else {
            0
        };
        if bytes.len() < count {
            return Err(invalid_building_race_team_unassigned(
                "truncated race-team-unassigned bytes",
            ));
        }
        let original = &bytes[..count];
        // The game writes `m_raceTeamIndex == 0xFFFF`, meaning no team is
        // assigned. A false value is followed by an encoded prefab index.
        let unassigned = original.iter().map(|byte| *byte == 1).collect();
        Ok((
            Self {
                format_version,
                unassigned,
                serialized_prefix: original.to_vec(),
            },
            count,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns `true` for buildings whose race-team index is `0xFFFF`.
    /// Such buildings have no prefab reference in the following reference scope.
    pub fn unassigned(&self) -> &[bool] {
        &self.unassigned
    }
    /// Writes the original bool bytes exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
    /// Creates an editor that preserves unchanged raw bool bytes.
    pub fn into_editor(self) -> BuildingRaceTeamUnassignedEditor {
        BuildingRaceTeamUnassignedEditor {
            format_version: self.format_version,
            original_unassigned: self.unassigned.clone(),
            unassigned: self.unassigned,
            original_bytes: self.serialized_prefix,
        }
    }
}

impl BuildingRaceTeamUnassignedEditor {
    /// Returns the inner DataSerializer version associated with these flags.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }
    /// Returns editable unassigned flags in active-building order.
    pub fn unassigned_mut(&mut self) -> &mut [bool] {
        &mut self.unassigned
    }
    /// Returns current unassigned flags in active-building order.
    pub fn unassigned(&self) -> &[bool] {
        &self.unassigned
    }
    /// Encodes changed bools as `00` or `01` and retains unchanged raw bytes.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.format_version < RACE_TEAM_UNASSIGNED_VERSION
            && (!self.unassigned.is_empty() || !self.original_bytes.is_empty())
        {
            return Err(invalid_building_race_team_unassigned(
                "race-team-unassigned bytes are absent before inner version 120002",
            ));
        }
        if self.unassigned.len() != self.original_unassigned.len()
            || self.unassigned.len() != self.original_bytes.len()
        {
            return Err(invalid_building_race_team_unassigned(
                "race-team-unassigned count changed",
            ));
        }
        Ok(self
            .unassigned
            .iter()
            .zip(&self.original_unassigned)
            .zip(&self.original_bytes)
            .map(|((current, original), raw)| {
                if current == original {
                    *raw
                } else if *current {
                    1
                } else {
                    0
                }
            })
            .collect())
    }
}

impl BuildingRaceTeamReferencesPrefix {
    /// Decodes the reference scope after the version-120002 unassigned flags.
    /// One encoded UShort index is read for each active building marked
    /// assigned, followed by the prefab-name count and unique-string table.
    /// Returns the exact number of bytes consumed from `bytes`.
    pub fn decode_after_unassigned(
        format_version: u32,
        unassigned: &BuildingRaceTeamUnassignedPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < RACE_TEAM_UNASSIGNED_VERSION
            || unassigned.format_version() != format_version
        {
            return Err(invalid_building_race_team_references(
                "race-team reference scope requires matching inner version 120002 or newer",
            ));
        }

        let reference_count = unassigned
            .unassigned()
            .iter()
            .filter(|is_unassigned| !**is_unassigned)
            .count();
        let mut reader = Reader::new(bytes);
        let indices = read_encoded_ushort_array(&mut reader, reference_count)?;
        let name_count = usize::from(reader.read_u16_be()?);
        if name_count > reader.remaining() {
            return Err(reader.invalid("race-team prefab-name table cannot fit in remaining bytes"));
        }
        let mut prefab_names = Vec::with_capacity(name_count);
        for _ in 0..name_count {
            prefab_names.push(reader.read_unique_string()?);
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                indices,
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

    /// Returns prefab-table indices in assigned active-building order.
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }

    /// Returns the ordered prefab names; null entries are retained.
    pub fn prefab_names(&self) -> &[Option<String>] {
        &self.prefab_names
    }

    /// Writes the original reference scope byte for byte.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Creates an editor for the indices and names in this scope.
    pub fn into_editor(self) -> BuildingRaceTeamReferencesEditor {
        BuildingRaceTeamReferencesEditor {
            format_version: self.format_version,
            indices: self.indices,
            prefab_names: self.prefab_names,
        }
    }
}

impl BuildingRaceTeamReferencesEditor {
    /// Returns the inner DataSerializer version associated with this scope.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns mutable indices in assigned active-building order.
    pub fn indices_mut(&mut self) -> &mut Vec<u16> {
        &mut self.indices
    }

    /// Returns indices in assigned active-building order.
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }

    /// Returns mutable names indexed by the prefab-reference values.
    pub fn prefab_names_mut(&mut self) -> &mut Vec<Option<String>> {
        &mut self.prefab_names
    }

    /// Encodes a canonical UShort index array and prefab-name table.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.format_version < RACE_TEAM_UNASSIGNED_VERSION {
            return Err(invalid_building_race_team_references(
                "race-team reference scope is absent before inner version 120002",
            ));
        }
        let name_count = u16::try_from(self.prefab_names.len()).map_err(|_| {
            invalid_building_race_team_references("prefab-name count exceeds the on-disk u16")
        })?;
        let mut output = Vec::new();
        write_encoded_ushort_array(&mut output, &self.indices);
        output.extend_from_slice(&name_count.to_be_bytes());
        for name in &self.prefab_names {
            if let Some(name) = name {
                write_unique_string(&mut output, name)?;
            } else {
                output.push(0xff);
            }
        }
        Ok(output)
    }
}

fn invalid_building_race_team_unassigned(reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "BuildingManager race-team unassigned flags".to_owned(),
        offset: 0,
        reason: reason.into(),
    }
}

fn invalid_building_race_team_references(reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "BuildingManager race-team references".to_owned(),
        offset: 0,
        reason: reason.into(),
    }
}

fn fixed_object_header_count(format_version: u32) -> usize {
    let mut count = 3 * usize::from(format_version >= 82)
        + usize::from(format_version >= 198)
        + 3 * usize::from(format_version >= 87)
        + 2 * usize::from(format_version >= 156)
        + usize::from(format_version >= 166)
        + usize::from(format_version >= 160)
        + 2 * usize::from(format_version >= 188)
        + 6 * usize::from(format_version >= 237)
        + 5 * usize::from(format_version >= 295)
        + usize::from(format_version >= 304)
        + usize::from(format_version >= 309)
        + 3 * usize::from(format_version >= 110_022)
        + 7 * usize::from(format_version >= 111_008)
        + usize::from(format_version >= 112_000)
        + 4 * usize::from(format_version >= 112_017)
        + 13 * usize::from(format_version >= 113_011)
        + usize::from(format_version >= 113_014)
        + 5 * usize::from(format_version >= 116_017)
        + 4 * usize::from(format_version >= 117_006)
        + 6 * usize::from(format_version >= 121_033);
    count += usize::from(
        (109_012..110_000).contains(&format_version)
            || (110_025..111_000).contains(&format_version)
            || format_version >= 111_002,
    );
    count
}

impl BuildingManagerObjectHeadersPrefix {
    /// Decodes queued object headers and aggregate problem fields.
    /// The returned prefix ends before later per-building serialized arrays.
    pub fn decode_prefix(
        format_version: u32,
        prefabs: &BuildingPrefabPrefix,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || prefabs.format_version() != format_version {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager object headers".to_owned(),
                offset: 0,
                reason: "unsupported or mismatched inner version".to_owned(),
            });
        }
        let fixed_count = fixed_object_header_count(format_version);
        let header_count = prefabs
            .guide_indices()
            .len()
            .checked_add(prefabs.milestone_indices().len())
            .and_then(|count| count.checked_add(fixed_count))
            .ok_or(CrpError::ArithmeticOverflow(
                "BuildingManager object-header count",
            ))?;
        let mut reader = Reader::new(bytes);
        let mut object_headers = Vec::with_capacity(header_count);
        for _ in 0..header_count {
            object_headers.push(read_net_object_header(&mut reader)?);
        }
        let problem_field_bytes = if format_version >= 115_000 {
            4 * 8
        } else if format_version >= 134 {
            2 * 4
        } else {
            0
        };
        let trailing_problem_fields = reader.read_bytes(problem_field_bytes)?.to_vec();
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                object_headers,
                trailing_problem_fields,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns headers in guide, milestone, and fixed field order.
    pub fn object_headers(&self) -> &[NetObjectHeader] {
        &self.object_headers
    }

    /// Returns the original serialized building-problem fields after headers.
    pub fn trailing_problem_fields(&self) -> &[u8] {
        &self.trailing_problem_fields
    }

    /// Writes the original headers and trailing fields exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BuildingBufferFieldsPrefix, BuildingEconomicFieldsPrefix, BuildingFacultyBonusesPrefix,
        BuildingFlag, BuildingFlagsPrefix, BuildingInstanceRecordsPrefix,
        BuildingMonumentEffectsPrefix, BuildingPopulationEditor, BuildingPopulationPrefix,
        BuildingPrefabPrefix, BuildingRaceTeamReferencesPrefix, BuildingRaceTeamUnassignedPrefix,
        BuildingServiceSelectionPrefix, BuildingTimerField, BuildingTimerGeometryEditor,
        fixed_object_header_count, timer_fields_for_version,
    };

    #[test]
    fn building_flag_editor_preserves_activity_and_respects_version_gates() {
        let mut legacy_values = vec![0_u32; 32_767];
        legacy_values[32_766] = 1;
        let mut legacy_bytes = Vec::new();
        crate::encoded_array::write_encoded_uint_array(&mut legacy_bytes, &legacy_values);
        let (legacy, consumed) = BuildingFlagsPrefix::decode_prefix(164, &legacy_bytes)
            .expect("legacy primary flags decode");
        assert_eq!(consumed, legacy_bytes.len());
        let mut editor = legacy.into_editor();
        assert!(editor.set_primary_flag_bits(32_766, 0).is_err());
        assert!(editor.set_primary_flag_bits(32_767, 1).is_err());
        assert!(editor.set_secondary_flag_bits(0, 1).is_err());
        editor.set_primary_flag_bits(32_766, 3).unwrap();

        let mut expected = vec![0xff; 255]; // 32,640 repeated zero values.
        expected.extend_from_slice(&[0xfd, 0, 0, 0, 0, 3]); // 126 zeros, then one literal value.
        assert_eq!(editor.encode().unwrap(), expected);
        let (updated, _) =
            BuildingFlagsPrefix::decode_prefix(164, &expected).expect("edited legacy flags decode");
        assert_eq!(updated.building_flags()[32_766].flags, 3);
        assert_eq!(updated.building_flags()[32_767].flags, 0);

        let mut modern_primary = vec![0_u32; 49_151];
        modern_primary[49_150] = 1;
        let mut modern_bytes = Vec::new();
        crate::encoded_array::write_encoded_uint_array(&mut modern_bytes, &modern_primary);
        crate::encoded_array::write_encoded_uint_array_runs(&mut modern_bytes, &vec![0; 49_151]);
        let (modern, consumed) = BuildingFlagsPrefix::decode_prefix(112_030, &modern_bytes)
            .expect("version-112030 flag arrays decode");
        assert_eq!(consumed, modern_bytes.len());
        let mut editor = modern.into_editor();
        editor.set_primary_flag_bits(49_150, 5).unwrap();
        editor.set_secondary_flag_bits(0, 7).unwrap();
        let encoded = editor.encode().unwrap();
        let (updated, consumed) = BuildingFlagsPrefix::decode_prefix(112_030, &encoded)
            .expect("edited secondary flags decode");
        assert_eq!(consumed, encoded.len());
        assert_eq!(updated.building_flags()[49_150].flags, 5);
        assert_eq!(updated.building_flags()[0].flags2, 7);

        let mut before_secondary_gate = Vec::new();
        crate::encoded_array::write_encoded_uint_array_runs(
            &mut before_secondary_gate,
            &vec![0; 49_151],
        );
        let (legacy, _) = BuildingFlagsPrefix::decode_prefix(112_029, &before_secondary_gate)
            .expect("pre-secondary flags decode");
        assert!(legacy.into_editor().set_secondary_flag_bits(0, 1).is_err());
    }

    fn expected_effect_table_bytes() -> Vec<u8> {
        let mut bytes = Vec::with_capacity(512);
        for index in 0..32_i32 {
            bytes.extend_from_slice(&(index - 16).to_be_bytes());
            for bits in [
                0x3f80_0000_u32.wrapping_add(index as u32),
                0xc000_0000_u32.wrapping_add(index as u32),
                0x4040_0000_u32.wrapping_add(index as u32),
            ] {
                bytes.extend_from_slice(&bits.to_be_bytes());
            }
        }
        bytes
    }

    #[test]
    fn building_effect_tables_follow_version_gates_and_field_order() {
        let bytes = expected_effect_table_bytes();
        let (monuments, used) = BuildingMonumentEffectsPrefix::decode_prefix(110_016, &bytes)
            .expect("monument tables decode");
        assert_eq!(used, 512);
        assert_eq!(monuments.encode(), bytes);
        assert_eq!(monuments.temporary().len(), 16);
        assert_eq!(monuments.final_values().len(), 16);
        assert_eq!(monuments.temporary()[0].factor, -16);
        assert_eq!(
            monuments.temporary()[0].position_bits,
            [0x3f80_0000, 0xc000_0000, 0x4040_0000]
        );
        assert_eq!(monuments.final_values()[0].factor, 0);
        let mut editor = monuments.into_editor();
        editor.temporary_mut()[0].factor = 17;
        let edited = editor.encode().expect("monument tables encode");
        assert_eq!(&edited[..4], &[0, 0, 0, 17]);
        assert_eq!(&edited[4..16], &bytes[4..16]);
        assert_eq!(edited.len(), 512);

        let (before_monument_gate, used) =
            BuildingMonumentEffectsPrefix::decode_prefix(110_015, &[])
                .expect("pre-gate monument tables are absent");
        assert_eq!(used, 0);
        assert!(before_monument_gate.temporary().is_empty());
        assert!(BuildingMonumentEffectsPrefix::decode_prefix(110_016, &bytes[..511]).is_err());

        let (before_faculty_gate, used) = BuildingFacultyBonusesPrefix::decode_prefix(112_000, &[])
            .expect("pre-gate faculty tables are absent");
        assert_eq!(used, 0);
        assert!(before_faculty_gate.final_values().is_empty());
        let (faculty, used) = BuildingFacultyBonusesPrefix::decode_prefix(112_001, &bytes)
            .expect("faculty tables decode");
        assert_eq!(used, 512);
        assert_eq!(faculty.encode(), bytes);
        assert_eq!(faculty.temporary()[15].factor, -1);
        assert_eq!(faculty.final_values()[15].factor, 15);
    }

    #[test]
    fn building_race_team_unassigned_uses_one_bool_per_active_building() {
        let flags = BuildingFlagsPrefix {
            format_version: 120_002,
            building_flags: vec![
                BuildingFlag {
                    index: 1,
                    flags: 1,
                    flags2: 0,
                },
                BuildingFlag {
                    index: 2,
                    flags: 0,
                    flags2: 0,
                },
                BuildingFlag {
                    index: 3,
                    flags: 2,
                    flags2: 0,
                },
            ],
            primary_flags_range: 0..0,
            secondary_flags_range: None,
            serialized_prefix: Vec::new(),
        };
        let (prefix, used) =
            BuildingRaceTeamUnassignedPrefix::decode_prefix(120_002, &flags, &[1, 2])
                .expect("race-team bools decode");
        assert_eq!(used, 2);
        assert_eq!(prefix.unassigned(), [true, false]);
        assert_eq!(prefix.encode(), [1, 2]);
        let mut editor = prefix.into_editor();
        editor.unassigned_mut()[0] = false;
        assert_eq!(editor.encode().expect("unassigned flags encode"), [0, 2]);
        editor.unassigned_mut()[1] = true;
        assert_eq!(editor.encode().expect("unassigned flags encode"), [0, 1]);
        assert!(BuildingRaceTeamUnassignedPrefix::decode_prefix(120_002, &flags, &[1]).is_err());

        let legacy_flags = BuildingFlagsPrefix {
            format_version: 120_001,
            building_flags: flags.building_flags.clone(),
            primary_flags_range: 0..0,
            secondary_flags_range: None,
            serialized_prefix: Vec::new(),
        };
        let (legacy, used) =
            BuildingRaceTeamUnassignedPrefix::decode_prefix(120_001, &legacy_flags, &[])
                .expect("pre-gate race-team flags are absent");
        assert_eq!(used, 0);
        assert!(legacy.unassigned().is_empty());
    }

    #[test]
    fn building_race_team_reference_scope_decodes_indices_and_names() {
        let flags = BuildingFlagsPrefix {
            format_version: 120_002,
            building_flags: vec![
                BuildingFlag {
                    index: 1,
                    flags: 1,
                    flags2: 0,
                },
                BuildingFlag {
                    index: 2,
                    flags: 1,
                    flags2: 0,
                },
                BuildingFlag {
                    index: 3,
                    flags: 1,
                    flags2: 0,
                },
                BuildingFlag {
                    index: 4,
                    flags: 1,
                    flags2: 0,
                },
            ],
            primary_flags_range: 0..0,
            secondary_flags_range: None,
            serialized_prefix: Vec::new(),
        };
        let (unassigned, _) =
            BuildingRaceTeamUnassignedPrefix::decode_prefix(120_002, &flags, &[1, 0, 2, 1])
                .expect("unassigned flags decode");
        let bytes = [
            1, 0, 0, 0, 2, 0, 3, // two EncodedArray.UShort indices, then three names
            0x80, 0, 4, 0xf4, 0xf1, 0xfc, 0x0c, // "team"
            0x80, 0, 5, 0xef, 0x05, 0xf4, 0xfd, 0x0d, // "other"
            0xff, // null prefab name
        ];

        let (references, used) =
            BuildingRaceTeamReferencesPrefix::decode_after_unassigned(120_002, &unassigned, &bytes)
                .expect("reference scope decodes");
        assert_eq!(references.indices(), [0, 2]);
        assert_eq!(
            references.prefab_names(),
            [Some("team".to_owned()), Some("other".to_owned()), None]
        );
        assert_eq!(used, bytes.len());
        assert_eq!(references.encode(), bytes);
        let editor = references.clone().into_editor();
        assert_eq!(editor.encode().expect("reference scope encodes"), bytes);
        let mut edited = editor;
        edited.indices_mut()[0] = 1;
        edited.prefab_names_mut()[0] = Some("new team".to_owned());
        let edited_bytes = edited.encode().expect("edited references encode");
        let (decoded_edit, edited_used) =
            BuildingRaceTeamReferencesPrefix::decode_after_unassigned(
                120_002,
                &unassigned,
                &edited_bytes,
            )
            .expect("edited references decode");
        assert_eq!(edited_used, edited_bytes.len());
        assert_eq!(decoded_edit.indices(), [1, 2]);
        assert_eq!(decoded_edit.prefab_names()[0].as_deref(), Some("new team"));
        let mut too_many_names = decoded_edit.into_editor();
        *too_many_names.prefab_names_mut() = vec![None; usize::from(u16::MAX) + 1];
        assert!(too_many_names.encode().is_err());
        assert!(
            BuildingRaceTeamReferencesPrefix::decode_after_unassigned(
                120_002,
                &unassigned,
                &bytes[..bytes.len() - 1],
            )
            .is_err()
        );
    }

    #[test]
    fn building_prefab_editor_reencodes_changed_fields_and_retains_other_bytes() {
        let flags = BuildingFlagsPrefix {
            format_version: 116_020,
            building_flags: (1..=3)
                .map(|index| BuildingFlag {
                    index,
                    flags: 1,
                    flags2: 0,
                })
                .collect(),
            primary_flags_range: 0..0,
            secondary_flags_range: None,
            serialized_prefix: Vec::new(),
        };
        let bytes = [
            2, 0, 10, 0, 20, 0, 30, // Three building-prefab indexes.
            0, 0, 7, // One guide-prefab index.
            0, 0, 9, // One milestone-prefab index.
            0, 3,    // Three names.
            0xff, // Null.
            0x80, 0, 1, 0xc1, // Compact "A".
            0, 0, 1, 3, 0x29, // Wide "Ω".
        ];
        let (prefix, consumed) = BuildingPrefabPrefix::decode_prefix(116_020, &flags, 1, 1, &bytes)
            .expect("building prefab table decodes");
        assert_eq!(consumed, bytes.len());
        assert_eq!(prefix.building_indices(), [10, 20, 30]);
        assert_eq!(prefix.guide_indices(), [7]);
        assert_eq!(prefix.milestone_indices(), [9]);
        assert_eq!(
            prefix.names(),
            [None, Some("A".to_owned()), Some("Ω".to_owned())]
        );

        let mut editor = prefix.into_editor();
        editor.building_indices_mut().fill(0);
        editor.names_mut()[1] = Some("BB".to_owned());
        let edited = editor.encode().expect("building prefab edits encode");
        let expected = [
            0x82, // Three repeated zero deltas.
            0, 0, 7, // Unchanged guide index.
            0, 0, 9, // Unchanged milestone index.
            0, 3,    // Unchanged name count.
            0xff, // Unchanged null entry.
            0x80, 0, 2, 0xc2, 0, // Compact "BB".
            0, 0, 1, 3, 0x29, // Unchanged wide "Ω" entry.
        ];
        assert_eq!(edited, expected);
        let (decoded, decoded_len) =
            BuildingPrefabPrefix::decode_prefix(116_020, &flags, 1, 1, &edited)
                .expect("edited building prefab table decodes");
        assert_eq!(decoded_len, edited.len());
        assert_eq!(decoded.building_indices(), [0, 0, 0]);
        assert_eq!(decoded.guide_indices(), [7]);
        assert_eq!(decoded.milestone_indices(), [9]);
        assert_eq!(
            decoded.names(),
            [None, Some("BB".to_owned()), Some("Ω".to_owned())]
        );
    }

    #[test]
    fn building_buffer_fields_editor_resizes_only_the_changed_array() {
        let flags = BuildingFlagsPrefix {
            format_version: 116_020,
            building_flags: (1..=3)
                .map(|index| BuildingFlag {
                    index,
                    flags: 1,
                    flags2: 0,
                })
                .collect(),
            primary_flags_range: 0..0,
            secondary_flags_range: None,
            serialized_prefix: Vec::new(),
        };
        let mut bytes = Vec::new();
        for start in 10..23 {
            crate::encoded_array::write_encoded_ushort_array(
                &mut bytes,
                &[start, start + 1, start + 2],
            );
        }
        crate::encoded_array::write_encoded_byte_array(&mut bytes, &[24, 25, 26]);
        crate::encoded_array::write_encoded_byte_array(&mut bytes, &[27, 28, 29]);

        let (prefix, consumed) = BuildingBufferFieldsPrefix::decode_prefix(116_020, &flags, &bytes)
            .expect("versioned buffer fields decode");
        assert_eq!(consumed, bytes.len());
        assert_eq!(prefix.net_node(), [10, 11, 12]);
        assert_eq!(prefix.sub_building(), [11, 12, 13]);
        assert_eq!(prefix.mail_buffer(), [19, 20, 21]);
        assert_eq!(prefix.production_rate(), [24, 25, 26]);
        assert_eq!(prefix.water_pollution(), [27, 28, 29]);

        let mut editor = prefix.into_editor();
        editor.net_node_mut().fill(0);
        let edited = editor.encode().expect("buffer-field edit encodes");
        let expected = [&[0x82][..], &bytes[7..]].concat();
        assert_eq!(edited, expected);
        let (decoded, decoded_len) =
            BuildingBufferFieldsPrefix::decode_prefix(116_020, &flags, &edited)
                .expect("edited buffer fields decode");
        assert_eq!(decoded_len, edited.len());
        assert_eq!(decoded.net_node(), [0, 0, 0]);
        assert_eq!(decoded.sub_building(), [11, 12, 13]);
        assert_eq!(decoded.water_pollution(), [27, 28, 29]);
    }

    #[test]
    fn building_instance_records_decode_and_encode_exact_big_endian_fields() {
        let flags = BuildingFlagsPrefix {
            format_version: 164,
            building_flags: vec![
                BuildingFlag {
                    index: 1,
                    flags: 1,
                    flags2: 0,
                },
                BuildingFlag {
                    index: 2,
                    flags: 0,
                    flags2: 0,
                },
                BuildingFlag {
                    index: 3,
                    flags: 2,
                    flags2: 0,
                },
            ],
            primary_flags_range: 0..0,
            secondary_flags_range: None,
            serialized_prefix: Vec::new(),
        };
        let bytes = [
            0x00, 0x00, 0x08, 0x55, 0x3f, 0x80, 0x00, 0x00, 0xc0, 0x20, 0x00, 0x00, 0x40, 0x48,
            0x00, 0x00, 0x3f, 0x00, 0x00, 0x00, 0x07, 0x12, 0x34, 0x56, 0x00, 0x00, 0x09, 0xaa,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0xbf, 0x80,
            0x00, 0x00, 0x08, 0xab, 0xcd, 0xef,
        ];
        let (decoded, consumed) = BuildingInstanceRecordsPrefix::decode_prefix(164, &flags, &bytes)
            .expect("valid records decode");
        assert_eq!(consumed, 48);
        assert_eq!(decoded.encode(), bytes);
        assert_eq!(decoded.records()[0].building_index, 0x855);
        assert_eq!(
            decoded.records()[0].position_bits,
            [0x3f80_0000, 0xc020_0000, 0x4048_0000]
        );
        assert_eq!(decoded.records()[0].angle_bits, 0x3f00_0000);
        assert_eq!(decoded.records()[0].base_height, 7);
        assert_eq!(decoded.records()[0].citizen_units, 0x123456);
        assert_eq!(decoded.records()[1].building_index, 0x9aa);
        assert_eq!(decoded.records()[1].citizen_units, 0xabcdef);

        let mut editor = decoded.into_editor();
        editor.records_mut()[0].angle_bits = 0x4000_0000;
        let edited = editor.encode().expect("edited records encode");
        assert_eq!(&edited[16..20], &[0x40, 0x00, 0x00, 0x00]);
        assert_eq!(edited.len(), 48);
    }

    #[test]
    fn building_instance_records_reject_truncation_and_oversized_u24() {
        let flags = BuildingFlagsPrefix {
            format_version: 164,
            building_flags: vec![BuildingFlag {
                index: 1,
                flags: 1,
                flags2: 0,
            }],
            primary_flags_range: 0..0,
            secondary_flags_range: None,
            serialized_prefix: Vec::new(),
        };
        assert!(BuildingInstanceRecordsPrefix::decode_prefix(164, &flags, &[0; 23]).is_err());
        let (decoded, _) = BuildingInstanceRecordsPrefix::decode_prefix(164, &flags, &[0; 24])
            .expect("zero record decodes");
        let mut editor = decoded.into_editor();
        editor.records_mut()[0].citizen_units = 0x0100_0000;
        assert!(editor.encode().is_err());
    }

    #[test]
    fn building_object_header_count_tracks_version_gates() {
        for (version, expected) in [
            (164, 9),
            (166, 10),
            (188, 12),
            (198, 13),
            (237, 19),
            (295, 24),
            (304, 25),
            (309, 26),
            (110_022, 29),
            (110_025, 30),
            (111_000, 29),
            (111_001, 29),
            (111_002, 30),
            (111_008, 37),
            (112_000, 38),
            (112_017, 42),
            (112_026, 42),
            (112_027, 42),
            (113_011, 55),
            (113_014, 56),
            (116_017, 61),
            (117_006, 65),
            (121_033, 71),
        ] {
            assert_eq!(
                fixed_object_header_count(version),
                expected,
                "version {version}"
            );
        }
    }

    #[test]
    fn building_timer_fields_follow_version_gates_and_il_order() {
        let legacy = timer_fields_for_version(164);
        assert_eq!(legacy.len(), 9);
        assert_eq!(legacy[0], BuildingTimerField::Electricity);
        assert_eq!(legacy[1], BuildingTimerField::Water);
        assert_eq!(legacy[8], BuildingTimerField::Major);

        let tax = timer_fields_for_version(172);
        assert_eq!(tax.len(), 10);
        assert_eq!(tax[8], BuildingTimerField::Tax);
        assert_eq!(tax[9], BuildingTimerField::Major);

        let heating = timer_fields_for_version(231);
        assert_eq!(heating.len(), 11);
        assert_eq!(heating[1], BuildingTimerField::Heating);
        assert_eq!(heating[2], BuildingTimerField::Water);
    }

    #[test]
    fn building_timer_editor_writes_field_major_literal_bytes() {
        let editor = BuildingTimerGeometryEditor {
            format_version: 231,
            active_building_count: 2,
            timer_fields: timer_fields_for_version(231),
            timer_values: (0..22).collect(),
            level_up_progress: vec![30, 31],
            width: vec![2, 3],
            length: vec![4, 5],
        };
        let mut expected = vec![21];
        expected.extend(0..22);
        expected.extend([1, 30, 31, 1, 2, 3, 1, 4, 5]);
        assert_eq!(editor.encode().expect("valid timer editor"), expected);

        let invalid = BuildingTimerGeometryEditor {
            width: vec![2],
            ..editor
        };
        assert!(invalid.encode().is_err());
    }

    #[test]
    fn building_population_prefix_obeys_version_gates_and_preserves_legacy_level() {
        let flags = BuildingFlagsPrefix {
            format_version: 207,
            building_flags: vec![
                BuildingFlag {
                    index: 1,
                    flags: 1,
                    flags2: 0,
                },
                BuildingFlag {
                    index: 2,
                    flags: 0,
                    flags2: 0,
                },
                BuildingFlag {
                    index: 3,
                    flags: 1,
                    flags2: 0,
                },
            ],
            primary_flags_range: 0..0,
            secondary_flags_range: None,
            serialized_prefix: Vec::new(),
        };
        // Nine one-value literal columns: education x3, age groups x4,
        // fire hazard, and the legacy level bytes discarded by the game.
        let encoded = [
            1, 11, 12, 1, 21, 22, 1, 31, 32, 1, 41, 42, 1, 51, 52, 1, 61, 62, 1, 71, 72, 1, 81, 82,
            1, 91, 92,
        ];
        let (prefix, used) = BuildingPopulationPrefix::decode_prefix(207, &flags, &encoded)
            .expect("valid version 207 population fields");
        assert_eq!(used, encoded.len());
        assert_eq!(prefix.active_building_count(), 2);
        assert_eq!(prefix.education1(), [11, 12]);
        assert_eq!(prefix.seniors(), [71, 72]);
        assert_eq!(prefix.fire_hazard(), [81, 82]);
        assert_eq!(prefix.level(), [91, 92]);
        assert_eq!(prefix.encode(), encoded);

        let mut editor = prefix.into_editor();
        editor.level_mut()[1] = 93;
        let rewritten = editor.encode().expect("valid editor");
        let (updated, _) = BuildingPopulationPrefix::decode_prefix(207, &flags, &rewritten)
            .expect("rewritten population fields");
        assert_eq!(updated.level(), [91, 93]);
        assert!(updated.children().is_empty());
    }

    #[test]
    fn building_population_prefix_includes_children_at_gate() {
        let flags = BuildingFlagsPrefix {
            format_version: 113_005,
            building_flags: vec![BuildingFlag {
                index: 1,
                flags: 1,
                flags2: 0,
            }],
            primary_flags_range: 0..0,
            secondary_flags_range: None,
            serialized_prefix: Vec::new(),
        };
        // Ten one-value literal columns, with children between education
        // and teen fields as written by BuildingManager+Data.Serialize.
        let encoded = [0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0, 8, 0, 9, 0, 10];
        let (prefix, used) = BuildingPopulationPrefix::decode_prefix(113_005, &flags, &encoded)
            .expect("valid version 113005 population fields");
        assert_eq!(used, encoded.len());
        assert_eq!(prefix.children(), [4]);
        assert_eq!(prefix.teens(), [5]);
        assert_eq!(prefix.fire_hazard(), [9]);
        assert_eq!(prefix.level(), [10]);
        assert_eq!(prefix.encode(), encoded);

        let invalid = BuildingPopulationEditor {
            format_version: 113_005,
            active_building_count: 1,
            education1: vec![1],
            education2: vec![2],
            education3: vec![3],
            children: Vec::new(),
            teens: vec![5],
            youngs: vec![6],
            adults: vec![7],
            seniors: vec![8],
            fire_hazard: vec![9],
            level: vec![10],
        };
        assert!(invalid.encode().is_err());
    }

    #[test]
    fn building_economic_fields_follow_version_gates_and_round_trip() {
        let flags = BuildingFlagsPrefix {
            format_version: 121_017,
            building_flags: vec![BuildingFlag {
                index: 1,
                flags: 1,
                flags2: 0,
            }],
            primary_flags_range: 0..0,
            secondary_flags_range: None,
            serialized_prefix: Vec::new(),
        };
        let encoded = [
            0, 0, 0, 0, 7, // cargo traffic UInt
            0, 0, 0, 0, 8, // garbage traffic UInt
            0, 0, 9, // access segment UShort
            0, 0, 10, // event access segment UShort
            0, 0xff, 0xff, 0xff, 0xf6, // cash buffer Int (-10)
            0, 0, 11, 0, 0, 12, 0, 0, 13, 0, 0, 14, // room columns
            0, 15, 0, 16, 0, 17, 0, 18, 0, 19, // unemployment columns
            0, 0, 20, // event route UShort
        ];
        let (prefix, used) = BuildingEconomicFieldsPrefix::decode_prefix(121_017, &flags, &encoded)
            .expect("valid economic fields");
        assert_eq!(used, encoded.len());
        assert_eq!(prefix.cargo_traffic_rate(), [7]);
        assert_eq!(prefix.garbage_traffic_rate(), [8]);
        assert_eq!(prefix.access_segment(), [9]);
        assert_eq!(prefix.event_access_segment(), [10]);
        assert_eq!(prefix.cash_buffer(), [-10]);
        assert_eq!(prefix.room_max(), [13]);
        assert_eq!(prefix.all_unemployed(), [19]);
        assert_eq!(prefix.event_route_index(), [20]);
        assert_eq!(prefix.encode(), encoded);

        let mut editor = prefix.into_editor();
        editor.cash_buffer_mut()[0] = -11;
        editor.event_route_index_mut()[0] = 21;
        let rewritten = editor.encode().expect("valid edited economic fields");
        let (updated, _) = BuildingEconomicFieldsPrefix::decode_prefix(121_017, &flags, &rewritten)
            .expect("rewritten economic fields");
        assert_eq!(updated.cash_buffer(), [-11]);
        assert_eq!(updated.event_route_index(), [21]);
    }

    #[test]
    fn building_cash_buffer_preserves_legacy_ushort_gate() {
        let flags = BuildingFlagsPrefix {
            format_version: 116_001,
            building_flags: vec![BuildingFlag {
                index: 1,
                flags: 1,
                flags2: 0,
            }],
            primary_flags_range: 0..0,
            secondary_flags_range: None,
            serialized_prefix: Vec::new(),
        };
        let encoded = [
            0, 0, 0, 0, 1, // cargo traffic
            0, 0, 0, 0, 2, // garbage traffic
            0, 0, 3, // access segment
            0, 0, 4, // legacy UShort cash buffer
        ];
        let (prefix, used) = BuildingEconomicFieldsPrefix::decode_prefix(116_001, &flags, &encoded)
            .expect("valid legacy economic fields");
        assert_eq!(used, encoded.len());
        assert_eq!(prefix.cash_buffer(), [4]);
        assert!(prefix.room_cost().is_empty());
        assert_eq!(prefix.encode(), encoded);

        let mut editor = prefix.into_editor();
        editor.cash_buffer_mut()[0] = i32::from(u16::MAX) + 1;
        assert!(editor.encode().is_err());
    }

    #[test]
    fn building_service_selection_tables_share_strings_and_preserve_bytes() {
        let shared = [
            "SimulationManager",
            "GameAreaManager",
            "TerrainManager",
            "WaterSimulation",
            "InstanceManager",
            "PathManager",
            "NetManager",
            "BuildingManager",
        ]
        .into_iter()
        .map(|value| Some(value.to_owned()))
        .collect();
        let encoded = [
            0, 2, 0, 5, 0xfe, 0x80, 0, 1, 0xd0, 0, 6, 0, 8, 0, 1, 0, 7, 0, 8,
        ];
        let (prefix, used) =
            BuildingServiceSelectionPrefix::decode_prefix(116_020, &encoded, shared)
                .expect("valid service selection tables");
        assert_eq!(used, encoded.len());
        assert_eq!(prefix.vehicle_selection().len(), 2);
        assert_eq!(prefix.vehicle_selection()[0].service_index, 5);
        assert_eq!(prefix.vehicle_selection()[0].value.as_deref(), Some("P"));
        assert_eq!(prefix.vehicle_selection()[1].value.as_deref(), Some("P"));
        assert_eq!(prefix.additional_vehicle_selection()[0].service_index, 7);
        assert_eq!(
            prefix.additional_vehicle_selection()[0].value.as_deref(),
            Some("P")
        );
        assert_eq!(prefix.shared_strings().len(), 9);
        assert_eq!(prefix.encode(), encoded);
    }

    #[test]
    fn building_service_selection_rejects_negative_counts_and_bad_indices() {
        let shared = vec![Some("BuildingManager".to_owned())];
        assert!(
            BuildingServiceSelectionPrefix::decode_prefix(114_001, &[0xff, 0xff], shared.clone(),)
                .is_err()
        );
        assert!(
            BuildingServiceSelectionPrefix::decode_prefix(114_001, &[0, 1, 0, 1, 0, 2], shared,)
                .is_err()
        );
    }
}

impl BuildingPrefabPrefix {
    /// Decodes prefab indices for active buildings, guides, and milestones.
    ///
    /// `bytes` begins immediately after the two big-endian `u16` guide and
    /// milestone counts. `flags` supplies the active building positions.
    pub fn decode_prefix(
        format_version: u32,
        flags: &BuildingFlagsPrefix,
        guide_count: u16,
        milestone_count: u16,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        if format_version < 164 || format_version != flags.format_version() {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager prefab indices".to_owned(),
                offset: 0,
                reason: "unsupported or mismatched inner version".to_owned(),
            });
        }
        let mut reader = Reader::new(bytes);
        let building_count = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .count();
        let building_indices_start = reader.offset();
        let building_indices = read_encoded_ushort_array(&mut reader, building_count)?;
        let serialized_building_indices =
            reader.consumed_bytes()[building_indices_start..reader.offset()].to_vec();
        let guide_indices_start = reader.offset();
        let guide_indices = read_encoded_ushort_array(&mut reader, usize::from(guide_count))?;
        let serialized_guide_indices =
            reader.consumed_bytes()[guide_indices_start..reader.offset()].to_vec();
        let milestone_indices_start = reader.offset();
        let milestone_indices =
            read_encoded_ushort_array(&mut reader, usize::from(milestone_count))?;
        let serialized_milestone_indices =
            reader.consumed_bytes()[milestone_indices_start..reader.offset()].to_vec();
        let name_count_start = reader.offset();
        let name_count = usize::from(reader.read_u16_be()?);
        let serialized_name_count =
            reader.consumed_bytes()[name_count_start..reader.offset()].to_vec();
        let mut names = Vec::with_capacity(name_count);
        let mut serialized_name_entries = Vec::with_capacity(name_count);
        for _ in 0..name_count {
            let name_start = reader.offset();
            names.push(reader.read_unique_string()?);
            serialized_name_entries
                .push(reader.consumed_bytes()[name_start..reader.offset()].to_vec());
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                building_indices,
                guide_indices,
                milestone_indices,
                names,
                serialized_building_indices,
                serialized_guide_indices,
                serialized_milestone_indices,
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

    /// Returns indices corresponding to nonzero primary building flags.
    pub fn building_indices(&self) -> &[u16] {
        &self.building_indices
    }

    /// Returns prefab indices for guide entries.
    pub fn guide_indices(&self) -> &[u16] {
        &self.guide_indices
    }

    /// Returns prefab indices for milestone entries.
    pub fn milestone_indices(&self) -> &[u16] {
        &self.milestone_indices
    }

    /// Returns the prefab collection names, preserving null entries.
    pub fn names(&self) -> &[Option<String>] {
        &self.names
    }

    /// Writes the original encoded tables and names exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl BuildingPrefabPrefix {
    /// Creates an editor for existing assignment entries and prefab names.
    pub fn into_editor(self) -> BuildingPrefabEditor {
        BuildingPrefabEditor {
            format_version: self.format_version,
            original_building_indices: self.building_indices.clone(),
            original_guide_indices: self.guide_indices.clone(),
            original_milestone_indices: self.milestone_indices.clone(),
            original_names: self.names.clone(),
            building_indices: self.building_indices,
            guide_indices: self.guide_indices,
            milestone_indices: self.milestone_indices,
            names: self.names,
            serialized_building_indices: self.serialized_building_indices,
            serialized_guide_indices: self.serialized_guide_indices,
            serialized_milestone_indices: self.serialized_milestone_indices,
            serialized_name_count: self.serialized_name_count,
            serialized_name_entries: self.serialized_name_entries,
            original_prefix: self.serialized_prefix,
        }
    }
}

impl BuildingPrefabEditor {
    /// Returns the inner DataSerializer version associated with these tables.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns editable prefab indexes for active buildings.
    pub fn building_indices_mut(&mut self) -> &mut [u16] {
        &mut self.building_indices
    }

    /// Returns editable prefab indexes for guide entries.
    pub fn guide_indices_mut(&mut self) -> &mut [u16] {
        &mut self.guide_indices
    }

    /// Returns editable prefab indexes for milestone entries.
    pub fn milestone_indices_mut(&mut self) -> &mut [u16] {
        &mut self.milestone_indices
    }

    /// Returns editable prefab names in serialized table order.
    pub fn names_mut(&mut self) -> &mut [Option<String>] {
        &mut self.names
    }

    /// Encodes changed index arrays and names while retaining unchanged bytes.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.format_version < 164
            || self.building_indices.len() != self.original_building_indices.len()
            || self.guide_indices.len() != self.original_guide_indices.len()
            || self.milestone_indices.len() != self.original_milestone_indices.len()
            || self.names.len() != self.original_names.len()
            || self.names.len() != self.serialized_name_entries.len()
            || self.serialized_name_count.len() != 2
        {
            return Err(invalid_building_prefabs(
                "edited assignment or prefab-name counts do not match the source",
            ));
        }
        if self.building_indices == self.original_building_indices
            && self.guide_indices == self.original_guide_indices
            && self.milestone_indices == self.original_milestone_indices
            && self.names == self.original_names
        {
            return Ok(self.original_prefix.clone());
        }

        let mut output = Vec::with_capacity(self.original_prefix.len());
        if self.building_indices == self.original_building_indices {
            output.extend_from_slice(&self.serialized_building_indices);
        } else {
            write_encoded_ushort_array(&mut output, &self.building_indices);
        }
        if self.guide_indices == self.original_guide_indices {
            output.extend_from_slice(&self.serialized_guide_indices);
        } else {
            write_encoded_ushort_array(&mut output, &self.guide_indices);
        }
        if self.milestone_indices == self.original_milestone_indices {
            output.extend_from_slice(&self.serialized_milestone_indices);
        } else {
            write_encoded_ushort_array(&mut output, &self.milestone_indices);
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

    pub(crate) fn original_prefix(&self) -> &[u8] {
        &self.original_prefix
    }
}

fn invalid_building_prefabs(reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "BuildingManager prefab table".to_owned(),
        offset: 0,
        reason: reason.into(),
    }
}

impl BuildingFlagsPrefix {
    /// Decodes building flags for inner versions 164 and newer.
    ///
    /// Versions before 222 read 32,767 primary positions; later versions read
    /// 49,151. A second full-length array begins at 112030. Omitted positions
    /// are zero-filled. Truncated or malformed encoded arrays return an error.
    /// The returned length ends before guide and milestone counts.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(CrpError::InvalidAssetPayload {
                name: "BuildingManager+Data flags".to_owned(),
                offset: 0,
                reason: "unsupported inner version before 164".to_owned(),
            });
        }
        let mut reader = Reader::new(bytes);
        let primary_count = if format_version < FULL_BUILDING_COUNT_VERSION {
            LEGACY_BUILDING_COUNT - 1
        } else {
            BUILDING_COUNT - 1
        };
        let primary_start = reader.offset();
        let primary = read_encoded_uint_array(&mut reader, primary_count)?;
        let primary_flags_range = primary_start..reader.offset();
        let secondary_start = reader.offset();
        let secondary = if format_version >= SECONDARY_BUILDING_FLAGS_VERSION {
            Some(read_encoded_uint_array(&mut reader, BUILDING_COUNT - 1)?)
        } else {
            None
        };
        let secondary_flags_range = secondary.as_ref().map(|_| secondary_start..reader.offset());
        let mut building_flags = Vec::with_capacity(BUILDING_COUNT - 1);
        for index in 0..BUILDING_COUNT - 1 {
            building_flags.push(BuildingFlag {
                index: u16::try_from(index + 1)
                    .map_err(|_| CrpError::ArithmeticOverflow("building buffer index"))?,
                flags: primary.get(index).copied().unwrap_or(0),
                flags2: secondary
                    .as_ref()
                    .and_then(|flags| flags.get(index))
                    .copied()
                    .unwrap_or(0),
            });
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                building_flags,
                primary_flags_range,
                secondary_flags_range,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns flags for one-based building buffer positions 1 through 49151.
    pub fn building_flags(&self) -> &[BuildingFlag] {
        &self.building_flags
    }

    /// Creates an editor for raw primary and version-112030 secondary flags.
    pub fn into_editor(self) -> BuildingFlagsEditor {
        BuildingFlagsEditor {
            building_flags: self.building_flags.clone(),
            original: self,
        }
    }

    /// Writes the original encoded arrays exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl BuildingFlagsEditor {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns decoded entries in ascending slot order; each entry has a
    /// one-based buffer index.
    pub fn building_flags(&self) -> &[BuildingFlag] {
        &self.building_flags
    }

    /// Replaces primary flag bits for one serialized building slot.
    ///
    /// The zero/nonzero state must remain unchanged because it selects the
    /// number and ordering of following active-building fields. Slots omitted
    /// by the pre-version-222 layout return an error.
    pub fn set_primary_flag_bits(&mut self, index: usize, value: u32) -> Result<(), CrpError> {
        let serialized_count = if self.original.format_version < FULL_BUILDING_COUNT_VERSION {
            LEGACY_BUILDING_COUNT - 1
        } else {
            BUILDING_COUNT - 1
        };
        if index >= serialized_count {
            return Err(invalid_building_flags(
                index,
                "primary flag slot is not serialized by this inner version",
            ));
        }
        let flag = self
            .building_flags
            .get_mut(index)
            .ok_or_else(|| invalid_building_flags(index, "primary flag index is out of range"))?;
        if (flag.flags == 0) != (value == 0) {
            return Err(invalid_building_flags(
                index,
                "primary flag edits must preserve the slot's active state",
            ));
        }
        flag.flags = value;
        Ok(())
    }

    /// Replaces secondary flag bits for one slot from inner version 112030.
    ///
    /// Secondary flags do not determine the active-building count. Earlier
    /// versions return an error because this array is absent from their bytes.
    pub fn set_secondary_flag_bits(&mut self, index: usize, value: u32) -> Result<(), CrpError> {
        if self.original.format_version < SECONDARY_BUILDING_FLAGS_VERSION {
            return Err(invalid_building_flags(
                index,
                "secondary flag array is absent before inner version 112030",
            ));
        }
        let flag = self
            .building_flags
            .get_mut(index)
            .ok_or_else(|| invalid_building_flags(index, "secondary flag index is out of range"))?;
        flag.flags2 = value;
        Ok(())
    }

    /// Re-encodes changed flag arrays and preserves all other source bytes.
    ///
    /// Changed arrays use the canonical repeat-delta encoding and may resize
    /// this prefix. Primary activity and all building slot indexes stay fixed.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.building_flags.len() != self.original.building_flags.len() {
            return Err(invalid_building_flags(0, "building flag count changed"));
        }
        let primary_count = if self.original.format_version < FULL_BUILDING_COUNT_VERSION {
            LEGACY_BUILDING_COUNT - 1
        } else {
            BUILDING_COUNT - 1
        };
        let mut primary = Vec::with_capacity(primary_count);
        for (index, (edited, original)) in self
            .building_flags
            .iter()
            .zip(&self.original.building_flags)
            .enumerate()
        {
            if edited.index != original.index {
                return Err(invalid_building_flags(
                    index,
                    "building buffer index changed",
                ));
            }
            if (edited.flags == 0) != (original.flags == 0) {
                return Err(invalid_building_flags(
                    index,
                    "primary flag edits must preserve the slot's active state",
                ));
            }
            if index < primary_count {
                primary.push(edited.flags);
            } else if edited.flags != 0 {
                return Err(invalid_building_flags(
                    index,
                    "primary flag slot is not serialized by this inner version",
                ));
            }
            if self.original.secondary_flags_range.is_none() && edited.flags2 != 0 {
                return Err(invalid_building_flags(
                    index,
                    "secondary flag array is absent before inner version 112030",
                ));
            }
        }

        let mut bytes = self.original.serialized_prefix.clone();
        let original_primary = self
            .original
            .building_flags
            .iter()
            .take(primary_count)
            .map(|flag| flag.flags);
        if let Some(range) = &self.original.secondary_flags_range {
            let secondary: Vec<_> = self.building_flags.iter().map(|flag| flag.flags2).collect();
            let original_secondary = self.original.building_flags.iter().map(|flag| flag.flags2);
            if !secondary.iter().copied().eq(original_secondary) {
                validate_building_flag_range(&bytes, range)?;
                let mut encoded = Vec::new();
                write_encoded_uint_array_runs(&mut encoded, &secondary);
                bytes.splice(range.clone(), encoded);
            }
        }

        if !primary.iter().copied().eq(original_primary) {
            validate_building_flag_range(&bytes, &self.original.primary_flags_range)?;
            let mut encoded = Vec::new();
            write_encoded_uint_array_runs(&mut encoded, &primary);
            bytes.splice(self.original.primary_flags_range.clone(), encoded);
        }
        Ok(bytes)
    }

    pub(crate) fn original_prefix(&self) -> &[u8] {
        &self.original.serialized_prefix
    }
}

fn validate_building_flag_range(bytes: &[u8], range: &Range<usize>) -> Result<(), CrpError> {
    if range.start > range.end || range.end > bytes.len() {
        return Err(invalid_building_flags(
            range.start,
            "encoded flag range exceeds the original prefix",
        ));
    }
    Ok(())
}

fn invalid_building_flags(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "BuildingManager+Data flags".to_owned(),
        offset,
        reason: reason.into(),
    }
}
