use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    read_encoded_byte_array, read_encoded_int_array, read_encoded_uint_array,
    read_encoded_ulong_array, read_encoded_ushort_array, write_encoded_byte_array,
    write_encoded_int_array, write_encoded_uint_array, write_encoded_ulong_array,
    write_encoded_ushort_array,
};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

const BUILDING_COUNT: usize = 49_152;
const LEGACY_BUILDING_COUNT: usize = 32_768;
const FULL_BUILDING_COUNT_VERSION: u32 = 222;
const SECONDARY_BUILDING_FLAGS_VERSION: u32 = 112_030;
const TWO_BUILDING_PROBLEMS_VERSION: u32 = 115_000;
const SAVE_APP_PROBLEMS_VERSION: u32 = 146_809_088;

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
    serialized_prefix: Vec<u8>,
}

/// Prefab index tables and names immediately after the BuildingManager counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingPrefabPrefix {
    format_version: u32,
    building_indices: Vec<u16>,
    guide_indices: Vec<u16>,
    milestone_indices: Vec<u16>,
    names: Vec<Option<String>>,
    serialized_prefix: Vec<u8>,
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
    serialized_prefix: Vec<u8>,
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
    serialized_prefix: Vec<u8>,
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
        let net_node = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let sub_building = if format_version >= 208 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let water_source = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let legacy_water_source_release = if (95..184).contains(&format_version) {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let electricity_buffer = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let water_buffer = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let sewage_buffer = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let heating_buffer = if format_version >= 227 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let garbage_buffer = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let crime_buffer = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let mail_buffer = if format_version >= 111_003 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let custom_buffer1 = if format_version >= 33 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let custom_buffer2 = if format_version >= 66 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let build_water_height = if format_version >= 308 {
            read_encoded_ushort_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let production_rate = if format_version >= 33 {
            read_encoded_byte_array(&mut reader, count)?
        } else {
            Vec::new()
        };
        let water_pollution = if format_version >= 33 {
            read_encoded_byte_array(&mut reader, count)?
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
        let mut read = |since| {
            if format_version >= since {
                read_encoded_byte_array(&mut reader, count)
            } else {
                Ok(Vec::new())
            }
        };
        let construct_state = read(33)?;
        let fire_intensity = read(35)?;
        let fire_damage = read(35)?;
        let production_state = read(81)?;
        let health = read(55)?;
        let child_health = read(113_004)?;
        let senior_health = read(113_004)?;
        let happiness = read(55)?;
        let citizen_count = read(55)?;
        let temp_import = read(72)?;
        let temp_export = read(72)?;
        let final_import = read(72)?;
        let final_export = read(72)?;
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
        BuildingEconomicFieldsPrefix, BuildingFlag, BuildingFlagsPrefix,
        BuildingInstanceRecordsPrefix, BuildingPopulationEditor, BuildingPopulationPrefix,
        BuildingServiceSelectionPrefix, BuildingTimerField, BuildingTimerGeometryEditor,
        fixed_object_header_count, timer_fields_for_version,
    };

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
        let building_indices = read_encoded_ushort_array(&mut reader, building_count)?;
        let guide_indices = read_encoded_ushort_array(&mut reader, usize::from(guide_count))?;
        let milestone_indices =
            read_encoded_ushort_array(&mut reader, usize::from(milestone_count))?;
        let name_count = usize::from(reader.read_u16_be()?);
        let mut names = Vec::with_capacity(name_count);
        for _ in 0..name_count {
            names.push(reader.read_unique_string()?);
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                building_indices,
                guide_indices,
                milestone_indices,
                names,
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
        let primary = read_encoded_uint_array(&mut reader, primary_count)?;
        let secondary = if format_version >= SECONDARY_BUILDING_FLAGS_VERSION {
            Some(read_encoded_uint_array(&mut reader, BUILDING_COUNT - 1)?)
        } else {
            None
        };
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

    /// Writes the original encoded arrays exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}
