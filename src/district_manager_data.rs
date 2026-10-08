use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    read_encoded_bool_array, read_encoded_byte_array, read_encoded_uint_array,
    read_encoded_ulong_array, skip_encoded_ushort_array, write_encoded_bool_array,
    write_encoded_byte_array, write_encoded_ulong_array,
};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

const DISTRICT_COUNT: usize = 128;
const PARK_FLAGS_VERSION: u32 = 110_001;
const AIRLINE_NAME_VERSION: u32 = 114_006;
const AIRPORT_PANEL_VERSION: u32 = 114_007;
const POLICY_FLAGS_VERSION: u32 = 70;
const POLICY_EFFECTS_VERSION: u32 = 171;
const DISTRICT_EVENT_POLICIES_VERSION: u32 = 248;
const PARK_POLICIES_VERSION: u32 = 110_009;
const PARK_EVENT_POLICIES_VERSION: u32 = 112_015;
const PARK_SPECIALIZATION_POLICIES_VERSION: u32 = 115_012;
const DISTRICT_CRIME_RATE_VERSION: u32 = 203;
const DISTRICT_ELIGIBLE_AGE_VERSION: u32 = 112_031;
const DISTRICT_DETAILED_AGE_VERSION: u32 = 113_003;
const DISTRICT_DEAD_SENIORS_VERSION: u32 = 113_006;
const DISTRICT_VISITOR_DATA_VERSION: u32 = 71;
const DISTRICT_PRIVATE_LEVELS_VERSION: u32 = 117;
const DISTRICT_PRIVATE_HEALTH_VERSION: u32 = 142;
const DISTRICT_PRIVATE_BUILDING_COUNTS_VERSION: u32 = 143;
const DISTRICT_PRIVATE_ALIVE_COUNT_VERSION: u32 = 145;
const DISTRICT_PRIVATE_CRIME_RATE_VERSION: u32 = 181;
const DISTRICT_PRIVATE_COLLAPSED_COUNT_VERSION: u32 = 256;
const DISTRICT_PLAYER_DATA_VERSION: u32 = 144;
const DISTRICT_SPECIALIZATION_VERSION: u32 = 218;
const DISTRICT_ADDITIONAL_SPECIALIZATION_VERSION: u32 = 109_006;
const DISTRICT_BIRTH_DEATH_VERSION: u32 = 167;
const DISTRICT_EDUCATION_DATA_VERSION: u32 = 67;
const DISTRICT_EDUCATION_WORKER_VERSION: u32 = 140;
const DISTRICT_EDUCATION_AGE_VERSION: u32 = 113;
const DISTRICT_STUDENT_AGE_VERSION: u32 = 143;
const DISTRICT_PRODUCTION_VERSION: u32 = 92;
const DISTRICT_USAGE_VERSION: u32 = 239;
const DISTRICT_GROUND_VERSION: u32 = 97;
const DISTRICT_RESOURCE_VERSION: u32 = 118;
const DISTRICT_CONSUMPTION_EXPANSION_VERSION: u32 = 234;
const DISTRICT_RESOURCE_ALIVE_VERSION: u32 = 111_014;
const DISTRICT_RESOURCE_FIELDS_VERSION: u32 = 113_013;
const DISTRICT_TOURIST_VERSION: u32 = 119;
const DISTRICT_TOURIST_AVERAGE_VERSION: u32 = 122;
const DISTRICT_TOURIST_COUNT_VERSION: u32 = 117_011;
const DISTRICT_SUBCULTURE_VERSION: u32 = 207;
const DISTRICT_HOTEL_START_VERSION: u32 = 117_004;
const DISTRICT_HOTEL_END_VERSION: u32 = 117_009;
const DISTRICT_HOTEL_COLOR_VERSION: u32 = 117_007;
const DISTRICT_HOTEL_POPULARITY_VERSION: u32 = 117_008;
const DISTRICT_STYLE_VERSION: u32 = 217;
const DISTRICT_LIBRARY_VISITOR_VERSION: u32 = 112_006;
const DISTRICT_AIRLINE_VERSION: u32 = 114_003;
const POLICY_COUNT: usize = 32;
const POPULATION_DELTA_COUNT: usize = 16;
const PRIVATE_DATA_BASE_BYTES: usize = 21;
const PRIVATE_DATA_LEVEL_BYTES: usize = 20;
const PRIVATE_DATA_HEALTH_BYTES: usize = 5;
const PRIVATE_DATA_BUILDING_BYTES: usize = 18;
const PRIVATE_DATA_ALIVE_BYTES: usize = 6;
const PRIVATE_DATA_CRIME_BYTES: usize = 5;
const PRIVATE_DATA_COLLAPSED_BYTES: usize = 4;
const DISTRICT_AGE_PAIR_BYTES: usize = 6;
const DISTRICT_EDUCATION_BASE_BYTES: usize = 18;
const DISTRICT_EDUCATION_WORKER_BYTES: usize = 24;
const DISTRICT_PRODUCTION_BASE_BYTES: usize = 86;
const DISTRICT_CONSUMPTION_BASE_BYTES: usize = 72;
const DISTRICT_USAGE_BYTES: usize = 24;
const DISTRICT_GROUND_BYTES: usize = 15;
const DISTRICT_RESOURCE_BASE_BYTES: usize = 60;
const DISTRICT_TOURIST_BASE_BYTES: usize = 6;
const DISTRICT_SUBCULTURE_BYTES: usize = 11;
const DISTRICT_HOTEL_BASE_BYTES: usize = 12;
const TYPE_ZERO_PARK_FIRST_VERSION: u32 = 110_025;
const TYPE_ZERO_PARK_LAST_VERSION: u32 = 118_011;
const TYPE_ZERO_PARK_CAMPUS_VERSION: u32 = 114_005;
const TYPE_ZERO_PARK_PRODUCTION_VERSION: u32 = 111_009;
const TYPE_ZERO_PARK_WORK_STORAGE_VERSION: u32 = 111_011;
const TYPE_ZERO_PARK_WIDE_PRODUCTION_VERSION: u32 = 111_015;
const TYPE_ZERO_PARK_HAPPINESS_U16_VERSION: u32 = 115_011;
const TYPE_ZERO_PARK_HAPPINESS_U32_VERSION: u32 = 115_022;
const TYPE_ZERO_PARK_EARLY_BYTES: usize = 62;
const TYPE_ZERO_PARK_BASE_TYPE_OFFSET: usize = 42;
const PARK_WORK_STORAGE_DELTAS_BYTES: usize = 4;
const PARK_PRODUCTION_U32_BYTES: usize = 4;
const PARK_PRODUCTION_U64_BYTES: usize = 8;
const PARK_WORKER_COUNT_BYTES: usize = 2;
const PARK_VISITOR_BLOCK_BYTES: usize = 22;
const PARK_LAST_VISITOR_COUNT_BYTES: usize = 4;
const DISTRICT_GRID_CELLS: usize = 262_144;
const DISTRICT_GRID_PLANES: usize = 8;
const DISTRICT_GRID_VALUES: usize = DISTRICT_GRID_CELLS * DISTRICT_GRID_PLANES;
const INDUSTRY_RESOURCE_VERSION: u32 = 111_009;
const INDUSTRY_RESOURCE_TRADE_VERSION: u32 = 111_010;
const INDUSTRY_RESOURCE_RECORDS: usize = 13;
const INDUSTRY_RESOURCE_BASE_FIELDS: usize = 10;
const INDUSTRY_RESOURCE_TRADE_FIELDS: usize = 4;
const INDUSTRY_RESOURCE_FIELD_BYTES: usize = 4;
const INDUSTRY_RESOURCE_BASE_BYTES: usize =
    INDUSTRY_RESOURCE_BASE_FIELDS * INDUSTRY_RESOURCE_FIELD_BYTES;
const INDUSTRY_RESOURCE_EXPANDED_BYTES: usize = (INDUSTRY_RESOURCE_BASE_FIELDS
    + INDUSTRY_RESOURCE_TRADE_FIELDS)
    * INDUSTRY_RESOURCE_FIELD_BYTES;

/// The initial known fields from `DistrictManager+Data`.
///
/// The policy bool values are retained in serialized order: category first,
/// then policy bit 0–31, then district or park index 0–127. Active district
/// population and age fields are decoded, followed by the versioned district
/// private-data blocks. Original bytes are retained for exact unchanged writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictManagerFlagsPrefix {
    format_version: u32,
    airline_name_present: bool,
    airline_name: Option<String>,
    airport_panel_opened: Option<bool>,
    district_flags: Vec<u32>,
    park_flags_present: bool,
    park_flags: Vec<u32>,
    policy_bools: Option<Vec<bool>>,
    legacy_policy_bools: Option<Vec<bool>>,
    district_population_records: Vec<DistrictPopulationRecord>,
    serialized_prefix: Vec<u8>,
}

/// A supported park record from inner versions 110025–118011.
///
/// Modeled scalar, industry-resource, campus-summary, and campus-report fields
/// are interpreted. The exact record bytes are retained for unchanged writing,
/// including fields not yet modeled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictParkRecord {
    park_index: usize,
    park_type: u8,
    random_seed: u64,
    main_gate: u16,
    condition: u16,
    level: u8,
    level_offset: usize,
    visitor_data: Option<DistrictParkVisitorData>,
    gate_data: Option<DistrictParkGateData>,
    capacities: Option<DistrictParkCapacities>,
    day_night_count: Option<u8>,
    attractiveness: Option<DistrictParkAttractiveness>,
    area_factor: Option<u8>,
    work_storage_deltas: Option<DistrictParkWorkStorageDeltas>,
    production_data: Option<DistrictParkProductionData>,
    industry_resources: Option<Vec<DistrictParkIndustryResource>>,
    campus_data: Option<DistrictCampusData>,
    campus_data_offset: Option<usize>,
    campus_year_reports: Option<Vec<DistrictCampusYearReportData>>,
    campus_sports_data: Option<DistrictCampusSportsData>,
    post_level_offsets: ParkPostLevelFieldOffsets,
    industry_resource_offsets: Option<ParkIndustryResourceOffsets>,
    serialized: Vec<u8>,
}

/// Ticket and visitor fields stored in an active park record from inner version 110008.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictParkVisitorData {
    total_visitors: u32,
    temporary_ticket_income: u32,
    final_ticket_income: u32,
    ticket_price: u16,
    temporary_residents: u16,
    final_residents: u16,
    temporary_tourists: u16,
    final_tourists: u16,
    last_visitor_count: Option<u32>,
    field_offsets: ParkVisitorFieldOffsets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParkVisitorFieldOffsets {
    total_visitors: usize,
    temporary_ticket_income: usize,
    final_ticket_income: usize,
    ticket_price: usize,
    temporary_residents: usize,
    final_residents: usize,
    temporary_tourists: usize,
    final_tourists: usize,
    last_visitor_count: Option<usize>,
}

/// Random and temporary/final gate values stored from inner version 110012.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistrictParkGateData {
    /// Serialized random gate index.
    pub random_gate: u16,
    /// Temporary gate count.
    pub temporary_count: u8,
    /// Final gate count.
    pub final_count: u8,
}

/// Temporary and final capacities stored from inner version 110018.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistrictParkCapacities {
    /// Temporary visitor capacity.
    pub temporary_visitors: u16,
    /// Final visitor capacity.
    pub final_visitors: u16,
    /// Temporary main-building capacity.
    pub temporary_main: u16,
    /// Final main-building capacity.
    pub final_main: u16,
}

/// Temporary and final attractiveness accumulators stored from inner version 110020.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistrictParkAttractiveness {
    /// Temporary attractiveness accumulation.
    pub temporary: u16,
    /// Final attractiveness accumulation.
    pub final_value: u16,
}

/// Temporary and final work-efficiency and storage deltas from inner version 111011.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistrictParkWorkStorageDeltas {
    /// Temporary work-efficiency delta byte.
    pub temporary_work_efficiency: u8,
    /// Temporary storage delta byte.
    pub temporary_storage: u8,
    /// Final work-efficiency delta byte.
    pub final_work_efficiency: u8,
    /// Final storage delta byte.
    pub final_storage: u8,
}

/// Production and worker counters stored from inner version 111009.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistrictParkProductionData {
    /// Total production, serialized as `u32` before inner version 111015 and `u64` from it.
    pub total_production: u64,
    /// Temporary worker count.
    pub temporary_workers: u16,
    /// Final worker count.
    pub final_workers: u16,
}

/// The thirteen industry resource blocks in their DistrictPark wire order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistrictParkIndustryResourceKind {
    /// Grain resource block.
    Grain,
    /// Logs resource block.
    Logs,
    /// Ore resource block.
    Ore,
    /// Oil resource block.
    Oil,
    /// Animal-products resource block.
    AnimalProducts,
    /// Flour resource block.
    Flour,
    /// Paper resource block.
    Paper,
    /// Planed-timber resource block.
    PlanedTimber,
    /// Petroleum resource block.
    Petroleum,
    /// Plastics resource block.
    Plastics,
    /// Glass resource block.
    Glass,
    /// Metals resource block.
    Metals,
    /// Luxury-products resource block.
    LuxuryProducts,
}

impl DistrictParkIndustryResourceKind {
    fn index(self) -> usize {
        match self {
            Self::Grain => 0,
            Self::Logs => 1,
            Self::Ore => 2,
            Self::Oil => 3,
            Self::AnimalProducts => 4,
            Self::Flour => 5,
            Self::Paper => 6,
            Self::PlanedTimber => 7,
            Self::Petroleum => 8,
            Self::Plastics => 9,
            Self::Glass => 10,
            Self::Metals => 11,
            Self::LuxuryProducts => 12,
        }
    }
}

const INDUSTRY_RESOURCE_KINDS: [DistrictParkIndustryResourceKind; INDUSTRY_RESOURCE_RECORDS] = [
    DistrictParkIndustryResourceKind::Grain,
    DistrictParkIndustryResourceKind::Logs,
    DistrictParkIndustryResourceKind::Ore,
    DistrictParkIndustryResourceKind::Oil,
    DistrictParkIndustryResourceKind::AnimalProducts,
    DistrictParkIndustryResourceKind::Flour,
    DistrictParkIndustryResourceKind::Paper,
    DistrictParkIndustryResourceKind::PlanedTimber,
    DistrictParkIndustryResourceKind::Petroleum,
    DistrictParkIndustryResourceKind::Plastics,
    DistrictParkIndustryResourceKind::Glass,
    DistrictParkIndustryResourceKind::Metals,
    DistrictParkIndustryResourceKind::LuxuryProducts,
];

/// Four temporary and final import/export counters added in inner version 111010.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistrictParkIndustryTradeData {
    /// Temporary imported amount.
    pub temporary_import: u32,
    /// Temporary exported amount.
    pub temporary_export: u32,
    /// Final imported amount.
    pub final_import: u32,
    /// Final exported amount.
    pub final_export: u32,
}

/// One of the thirteen `DistrictAreaResourceData` records on an industry park.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistrictParkIndustryResource {
    /// Resource slot in the DLL-defined wire order.
    pub kind: DistrictParkIndustryResourceKind,
    /// Temporary consumption.
    pub temporary_consumption: u32,
    /// Temporary production.
    pub temporary_production: u32,
    /// Temporary buffer amount.
    pub temporary_buffer_amount: u32,
    /// Temporary incoming transfer.
    pub temporary_incoming_transfer: u32,
    /// Temporary buffer capacity.
    pub temporary_buffer_capacity: u32,
    /// Final consumption.
    pub final_consumption: u32,
    /// Final production.
    pub final_production: u32,
    /// Final buffer amount.
    pub final_buffer_amount: u32,
    /// Final incoming transfer.
    pub final_incoming_transfer: u32,
    /// Final buffer capacity.
    pub final_buffer_capacity: u32,
    /// Versioned import/export counters, absent before inner version 111010.
    pub trade_data: Option<DistrictParkIndustryTradeData>,
}

/// Core student and academic-work values in a campus park's versioned block.
///
/// The four academic-work integers are exposed in wire order because the
/// supplied DLL does not name their individual serialized meanings. The
/// block begins at inner version 112008 for campus parks and at 114005 for the
/// slot-0 compatibility record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistrictCampusData {
    /// Current student count.
    pub student_count: u16,
    /// Student capacity.
    pub student_capacity: u16,
    /// Campus-building attractiveness accumulator.
    pub campus_building_attractiveness: u16,
    /// Four `AcademicWorksData` signed 32-bit values in serialized order.
    pub academic_works: [i32; 4],
}

/// One serialized campus report, in its on-disk order.
///
/// The game exposes current, prior, and ledger reports through fields of
/// different owners. The reader keeps these entries in wire order and does
/// not infer a role for each entry where the sample layout is ambiguous.
/// Individual fields are `None` when omitted by the inner-version gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictCampusYearReportData {
    student_count: Option<i32>,
    campus_attractiveness: Option<i32>,
    reputation_level: Option<i8>,
    grant_type: Option<i8>,
    grant_work_index: Option<i16>,
    academic_works: Option<[i32; 4]>,
    total_prize_money: Option<u64>,
    tickets_income: Option<[u64; 5]>,
    winnings_income: Option<[u64; 5]>,
    matches_won: Option<[u8; 5]>,
    matches_lost: Option<[u8; 5]>,
    matches_cancelled: Option<[u8; 5]>,
    trophies: Option<[bool; 5]>,
    toga_party_count: Option<i8>,
    toga_party_seeds: Option<[i64; 3]>,
    milestone_field_present: bool,
    milestone_unlocked: Option<String>,
    total_works: Option<i16>,
    field_offsets: CampusYearReportFieldOffsets,
}

/// Remaining fixed campus, staffing, and varsity values in the campus block.
///
/// Coach-hire ticks are the raw signed 64-bit values from `EncodedArray.Long`.
/// The game converts them through `f32` and clamps them to `DateTime` limits
/// before constructing its in-memory values, so this type avoids that lossy
/// conversion. The editor can explicitly rewrite these arrays using canonical
/// encodings while preserving the values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictCampusSportsData {
    is_main_campus: bool,
    grant_type: u8,
    coach_count: u8,
    cheerleading_budget: i16,
    academic_staff_count: Option<u8>,
    academic_staff_accumulation_bits: Option<u32>,
    coach_hire_time_ticks: [i64; 25],
    win_probability: u8,
    varsity_identity_index: i16,
    varsity_color: [u8; 3],
    lifetime_trophies: Option<i16>,
    static_varsity_attractiveness_modifier: Option<i16>,
    dynamic_varsity_attractiveness_modifier: Option<i16>,
    sports: Option<i16>,
    away_matches_done: Option<[bool; 5]>,
    campus_building_count: Option<i16>,
    field_offsets: CampusSportsFieldOffsets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct CampusSportsFieldOffsets {
    is_main_campus: Option<usize>,
    grant_type: Option<usize>,
    coach_count: Option<usize>,
    cheerleading_budget: Option<usize>,
    academic_staff_count: Option<usize>,
    academic_staff_accumulation: Option<usize>,
    coach_hire_time_array: Option<SerializedByteRange>,
    win_probability: Option<usize>,
    varsity_identity_index: Option<usize>,
    varsity_color_array: Option<SerializedByteRange>,
    lifetime_trophies: Option<usize>,
    static_varsity_attractiveness_modifier: Option<usize>,
    dynamic_varsity_attractiveness_modifier: Option<usize>,
    sports: Option<usize>,
    away_matches_array: Option<SerializedByteRange>,
    campus_building_count: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SerializedByteRange {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct CampusYearReportFieldOffsets {
    student_count: Option<usize>,
    campus_attractiveness: Option<usize>,
    reputation_level: Option<usize>,
    grant_type: Option<usize>,
    grant_work_index: Option<usize>,
    academic_works: Option<usize>,
    total_prize_money: Option<usize>,
    toga_party_count: Option<usize>,
    toga_party_seeds: Option<usize>,
    total_works: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParkPostLevelFieldOffsets {
    gates: Option<usize>,
    capacities: Option<usize>,
    day_night_count: Option<usize>,
    attractiveness: Option<usize>,
    area_factor: Option<usize>,
    work_storage_deltas: Option<usize>,
    production: Option<ParkProductionFieldOffsets>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParkProductionFieldOffsets {
    total_production: usize,
    total_production_bytes: usize,
    temporary_workers: usize,
    final_workers: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParkIndustryResourceOffsets {
    first_record: usize,
    record_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DecodedParkPostLevelData {
    gates: Option<DistrictParkGateData>,
    capacities: Option<DistrictParkCapacities>,
    day_night_count: Option<u8>,
    attractiveness: Option<DistrictParkAttractiveness>,
    area_factor: Option<u8>,
    work_storage_deltas: Option<DistrictParkWorkStorageDeltas>,
    production_data: Option<DistrictParkProductionData>,
    offsets: ParkPostLevelFieldOffsets,
}

/// Editor for modeled scalar fields of one active DistrictManager park.
///
/// All unmodeled fields remain byte-for-byte intact. Modeled scalar, visitor,
/// production, industry-resource, campus-summary, and fixed-width campus-report
/// fields can be changed. The three modeled campus sports arrays can also be
/// explicitly re-encoded, which may change this record's length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictParkEditor {
    original_serialized: Vec<u8>,
    record: DistrictParkRecord,
}

/// Edits fixed-width fields in one campus report without re-encoding its
/// variable-length arrays or changing the containing park record's length.
#[derive(Debug)]
pub struct DistrictCampusYearReportEditor<'a> {
    serialized: &'a mut Vec<u8>,
    report: &'a mut DistrictCampusYearReportData,
}

/// Edits fixed-width campus sports fields and rewrites its modeled arrays.
#[derive(Debug)]
pub struct DistrictCampusSportsEditor<'a> {
    serialized: &'a mut Vec<u8>,
    data: &'a mut DistrictCampusSportsData,
}

/// The decoded district and optional park grid planes from `DistrictManager+Data`.
///
/// Each grid contains eight 262,144-cell byte planes in district1–4, alpha1–4
/// order. The original encoded bytes are retained for exact unchanged writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictManagerGridData {
    district_values: Vec<u8>,
    park_values: Option<Vec<u8>>,
    district_encoded_len: usize,
    serialized: Vec<u8>,
}

/// Edits district or park grid planes while retaining the other encoded stream.
///
/// A changed grid is encoded as literal `EncodedArray.Byte` blocks. Its run
/// grouping may therefore differ from the game-produced bytes.
#[derive(Debug, Clone)]
pub struct DistrictManagerGridEditor {
    original: DistrictManagerGridData,
    district_dirty: bool,
    park_dirty: bool,
}

/// Versioned object headers and scalar fields after the DistrictManager grids.
///
/// Queued object bodies remain opaque. The exact header and profiler-tag bytes
/// are retained for unchanged writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictManagerTailData {
    object_headers: Vec<NetObjectHeader>,
    next_policy_message_frames: [u32; 2],
    district_policies_set: Option<u32>,
    serialized: Vec<u8>,
}

impl DistrictManagerTailData {
    /// Decodes the tail for inner versions 164 and newer.
    ///
    /// The returned byte count includes the DistrictManager profiler end tag.
    /// This follows the examined DLL layout. Truncated headers, scalars, or
    /// tags return an error.
    pub fn decode(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(invalid_district_manager(
                0,
                "DistrictManager tail requires inner version 164 or newer",
            ));
        }
        let mut reader = Reader::new(bytes);
        let mut object_headers = Vec::new();
        for _ in 0..district_tail_object_header_count(format_version) {
            object_headers.push(read_net_object_header(&mut reader)?);
        }
        let next_policy_message_frames = [reader.read_u32_be()?, reader.read_u32_be()?];
        let district_policies_set = if format_version >= 178 {
            Some(reader.read_u32_be()?)
        } else {
            None
        };
        match reader.read_u8()? {
            0xff => {
                return Err(invalid_district_manager(
                    reader.offset(),
                    "DistrictManager profiler end tag is null",
                ));
            }
            0xfe => {
                let name = reader.read_unique_string()?;
                if name.as_deref() != Some("DistrictManager") {
                    return Err(invalid_district_manager(
                        reader.offset(),
                        "wrong DistrictManager profiler end tag",
                    ));
                }
            }
            _high => {
                reader.read_u8()?; // Existing shared-string index.
            }
        }
        let consumed = reader.offset();
        Ok((
            Self {
                object_headers,
                next_policy_message_frames,
                district_policies_set,
                serialized: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the queued object headers in serialized order.
    pub fn object_headers(&self) -> &[NetObjectHeader] {
        &self.object_headers
    }

    /// Returns the two saved policy-message frame counters.
    pub fn next_policy_message_frames(&self) -> [u32; 2] {
        self.next_policy_message_frames
    }

    /// Returns the saved district-policy bit set from inner version 178.
    pub fn district_policies_set(&self) -> Option<u32> {
        self.district_policies_set
    }

    /// Writes the original bytes exactly, including unknown object references.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized.clone()
    }
}

fn district_tail_object_header_count(format_version: u32) -> usize {
    2 + 2 * usize::from(format_version >= 110_022)
        + 2 * usize::from(format_version >= 111_008)
        + 8 * usize::from(format_version >= 112_016)
        + 2 * usize::from(format_version >= 114_009)
        + 2 * usize::from(format_version >= 115_024)
        + usize::from(format_version >= 116_016)
}

impl DistrictManagerGridData {
    /// Decodes the grid streams and returns the number of consumed bytes.
    ///
    /// Inner versions 164 and newer are supported. The park grid is serialized
    /// from version 110001. Truncated streams return an error. The byte count
    /// ends before the following manager objects.
    pub fn decode(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(invalid_district_manager(
                0,
                "DistrictManager grids require inner version 164 or newer",
            ));
        }
        let mut reader = Reader::new(bytes);
        let district_values =
            read_encoded_byte_array(&mut reader, DISTRICT_GRID_VALUES).map_err(|error| {
                invalid_district_manager(reader.offset(), format!("district grid: {error}"))
            })?;
        let district_encoded_len = reader.offset();
        let park_values = if format_version >= PARK_FLAGS_VERSION {
            Some(
                read_encoded_byte_array(&mut reader, DISTRICT_GRID_VALUES).map_err(|error| {
                    invalid_district_manager(reader.offset(), format!("park grid: {error}"))
                })?,
            )
        } else {
            None
        };
        let consumed = reader.offset();
        Ok((
            Self {
                district_values,
                park_values,
                district_encoded_len,
                serialized: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns one district plane by index 0–7, or `None` for an invalid index.
    pub fn district_plane(&self, index: usize) -> Option<&[u8]> {
        grid_plane(&self.district_values, index)
    }

    /// Returns one park plane by index 0–7, or `None` when absent or invalid.
    pub fn park_plane(&self, index: usize) -> Option<&[u8]> {
        self.park_values
            .as_deref()
            .and_then(|values| grid_plane(values, index))
    }

    /// Returns whether the park grid is serialized in this version.
    pub fn park_grid_present(&self) -> bool {
        self.park_values.is_some()
    }

    /// Writes the exact encoded grid bytes without normalizing their runs.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized.clone()
    }

    pub(crate) fn original_bytes(&self) -> &[u8] {
        &self.serialized
    }

    /// Converts the decoded grids into an editor for explicit plane changes.
    pub fn into_editor(self) -> DistrictManagerGridEditor {
        DistrictManagerGridEditor {
            original: self,
            district_dirty: false,
            park_dirty: false,
        }
    }
}

impl DistrictManagerGridEditor {
    /// Replaces one district plane (index 0–7) of exactly 262,144 bytes.
    pub fn replace_district_plane(&mut self, index: usize, values: &[u8]) -> Result<(), CrpError> {
        replace_grid_plane(&mut self.original.district_values, index, values)?;
        self.district_dirty = true;
        Ok(())
    }

    /// Replaces one park plane (index 0–7) of exactly 262,144 bytes.
    ///
    /// Returns an error when this inner version has no serialized park grid.
    pub fn replace_park_plane(&mut self, index: usize, values: &[u8]) -> Result<(), CrpError> {
        let park = self.original.park_values.as_mut().ok_or_else(|| {
            invalid_district_manager(0, "park grid is absent in this inner version")
        })?;
        replace_grid_plane(park, index, values)?;
        self.park_dirty = true;
        Ok(())
    }

    /// Returns a district plane, or `None` for an invalid index.
    pub fn district_plane(&self, index: usize) -> Option<&[u8]> {
        self.original.district_plane(index)
    }

    /// Returns a park plane, or `None` when absent or invalid.
    pub fn park_plane(&self, index: usize) -> Option<&[u8]> {
        self.original.park_plane(index)
    }

    /// Writes edited grids, preserving each unchanged grid stream byte for byte.
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::new();
        if self.district_dirty {
            write_encoded_byte_array(&mut output, &self.original.district_values);
        } else {
            output
                .extend_from_slice(&self.original.serialized[..self.original.district_encoded_len]);
        }
        if self.park_dirty {
            if let Some(park) = &self.original.park_values {
                write_encoded_byte_array(&mut output, park);
            }
        } else {
            output
                .extend_from_slice(&self.original.serialized[self.original.district_encoded_len..]);
        }
        output
    }

    pub(crate) fn original_bytes(&self) -> &[u8] {
        &self.original.serialized
    }
}

fn replace_grid_plane(grid: &mut [u8], index: usize, values: &[u8]) -> Result<(), CrpError> {
    if index >= DISTRICT_GRID_PLANES {
        return Err(invalid_district_manager(
            0,
            format!("grid plane index {index} is outside 0–7"),
        ));
    }
    if values.len() != DISTRICT_GRID_CELLS {
        return Err(invalid_district_manager(
            0,
            format!(
                "grid plane has {} bytes; expected {DISTRICT_GRID_CELLS}",
                values.len()
            ),
        ));
    }
    let start = index * DISTRICT_GRID_CELLS;
    grid[start..start + DISTRICT_GRID_CELLS].copy_from_slice(values);
    Ok(())
}

fn grid_plane(values: &[u8], index: usize) -> Option<&[u8]> {
    if index >= DISTRICT_GRID_PLANES {
        return None;
    }
    let start = index * DISTRICT_GRID_CELLS;
    values.get(start..start + DISTRICT_GRID_CELLS)
}

impl DistrictParkRecord {
    /// Returns the index in the 128-entry park buffer.
    pub fn park_index(&self) -> usize {
        self.park_index
    }

    /// Returns the serialized park type (0–15 in the current reader).
    pub fn park_type(&self) -> u8 {
        self.park_type
    }

    /// Returns the serialized random seed.
    pub fn random_seed(&self) -> u64 {
        self.random_seed
    }

    /// Returns the main-gate building index.
    pub fn main_gate(&self) -> u16 {
        self.main_gate
    }

    /// Returns the serialized park condition.
    pub fn condition(&self) -> u16 {
        self.condition
    }

    /// Returns the park level.
    pub fn level(&self) -> u8 {
        self.level
    }

    /// Returns ticket and visitor counters when serialized by this version.
    pub fn visitor_data(&self) -> Option<&DistrictParkVisitorData> {
        self.visitor_data.as_ref()
    }

    /// Returns gate values when serialized by this version.
    pub fn gate_data(&self) -> Option<DistrictParkGateData> {
        self.gate_data
    }

    /// Returns visitor and main-building capacities when serialized by this version.
    pub fn capacities(&self) -> Option<DistrictParkCapacities> {
        self.capacities
    }

    /// Returns the day/night counter when serialized by this version.
    pub fn day_night_count(&self) -> Option<u8> {
        self.day_night_count
    }

    /// Returns attractiveness accumulators when serialized by this version.
    pub fn attractiveness(&self) -> Option<DistrictParkAttractiveness> {
        self.attractiveness
    }

    /// Returns the park area factor when serialized by this version.
    pub fn area_factor(&self) -> Option<u8> {
        self.area_factor
    }

    /// Returns work and storage deltas when serialized by this version.
    pub fn work_storage_deltas(&self) -> Option<DistrictParkWorkStorageDeltas> {
        self.work_storage_deltas
    }

    /// Returns production and worker counters when serialized by this version.
    pub fn production_data(&self) -> Option<DistrictParkProductionData> {
        self.production_data
    }

    /// Returns the thirteen industry resource records in DLL wire order.
    ///
    /// This is present for park types 5–9 from inner version 111009.
    pub fn industry_resources(&self) -> Option<&[DistrictParkIndustryResource]> {
        self.industry_resources.as_deref()
    }

    /// Returns the leading student and academic-work values when this record
    /// contains a campus or slot-0 compatibility block.
    pub fn campus_data(&self) -> Option<DistrictCampusData> {
        self.campus_data
    }

    /// Returns the campus reports in serialized order when the record has a
    /// campus block. Their current/prior/ledger roles are not inferred.
    pub fn campus_year_reports(&self) -> Option<&[DistrictCampusYearReportData]> {
        self.campus_year_reports.as_deref()
    }

    /// Returns remaining campus and varsity values when a campus block exists.
    pub fn campus_sports_data(&self) -> Option<&DistrictCampusSportsData> {
        self.campus_sports_data.as_ref()
    }

    /// Creates an editor that preserves all unmodeled bytes in this record.
    pub fn into_editor(self) -> DistrictParkEditor {
        DistrictParkEditor {
            original_serialized: self.serialized.clone(),
            record: self,
        }
    }

    /// Writes the exact record bytes, including fields not yet modeled.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized.clone()
    }
}

impl DistrictCampusYearReportData {
    /// Returns student count when serialized by this report version.
    pub fn student_count(&self) -> Option<i32> {
        self.student_count
    }

    /// Returns campus attractiveness when serialized by this report version.
    pub fn campus_attractiveness(&self) -> Option<i32> {
        self.campus_attractiveness
    }

    /// Returns reputation level when serialized by this report version.
    pub fn reputation_level(&self) -> Option<i8> {
        self.reputation_level
    }

    /// Returns grant type when serialized by this report version.
    pub fn grant_type(&self) -> Option<i8> {
        self.grant_type
    }

    /// Returns grant work index when serialized by this report version.
    pub fn grant_work_index(&self) -> Option<i16> {
        self.grant_work_index
    }

    /// Returns four academic-work values in their serialized order.
    pub fn academic_works(&self) -> Option<[i32; 4]> {
        self.academic_works
    }

    /// Returns total prize money.
    pub fn total_prize_money(&self) -> Option<u64> {
        self.total_prize_money
    }

    /// Returns five ticket-income values in their serialized order.
    pub fn tickets_income(&self) -> Option<[u64; 5]> {
        self.tickets_income
    }

    /// Returns five winnings-income values in their serialized order.
    pub fn winnings_income(&self) -> Option<[u64; 5]> {
        self.winnings_income
    }

    /// Returns five match-win values in their serialized order.
    pub fn matches_won(&self) -> Option<[u8; 5]> {
        self.matches_won
    }

    /// Returns five match-loss values in their serialized order.
    pub fn matches_lost(&self) -> Option<[u8; 5]> {
        self.matches_lost
    }

    /// Returns five cancelled-match values in their serialized order.
    pub fn matches_cancelled(&self) -> Option<[u8; 5]> {
        self.matches_cancelled
    }

    /// Returns trophy flags in their serialized order.
    pub fn trophies(&self) -> Option<[bool; 5]> {
        self.trophies
    }

    /// Returns the toga-party count from inner version 112017.
    pub fn toga_party_count(&self) -> Option<i8> {
        self.toga_party_count
    }

    /// Returns the three toga-party seeds from inner version 112017.
    pub fn toga_party_seeds(&self) -> Option<[i64; 3]> {
        self.toga_party_seeds
    }

    /// Returns true if the milestone string field is serialized.
    pub fn has_milestone_field(&self) -> bool {
        self.milestone_field_present
    }

    /// Returns the decoded milestone string, or `None` for a null or absent value.
    /// Use [`Self::has_milestone_field`] to distinguish a serialized null from
    /// a field omitted by the inner-version gate.
    pub fn milestone_unlocked(&self) -> Option<&str> {
        self.milestone_unlocked.as_deref()
    }

    /// Returns total works from inner version 112026.
    pub fn total_works(&self) -> Option<i16> {
        self.total_works
    }
}

impl DistrictCampusSportsData {
    /// Returns the campus-state value; `ReadBool` is true only for raw byte `1`.
    pub fn is_main_campus(&self) -> bool {
        self.is_main_campus
    }

    /// Returns the campus grant type.
    pub fn grant_type(&self) -> u8 {
        self.grant_type
    }

    /// Returns the coach count.
    pub fn coach_count(&self) -> u8 {
        self.coach_count
    }

    /// Returns the signed cheerleading budget.
    pub fn cheerleading_budget(&self) -> i16 {
        self.cheerleading_budget
    }

    /// Returns academic staff count from inner version 112025.
    pub fn academic_staff_count(&self) -> Option<u8> {
        self.academic_staff_count
    }

    /// Returns academic staff accumulation from inner version 112025.
    pub fn academic_staff_accumulation(&self) -> Option<f32> {
        self.academic_staff_accumulation_bits.map(f32::from_bits)
    }

    /// Returns all 25 raw signed coach-hire time values in serialized order.
    pub fn coach_hire_time_ticks(&self) -> &[i64; 25] {
        &self.coach_hire_time_ticks
    }

    /// Returns varsity win probability.
    pub fn win_probability(&self) -> u8 {
        self.win_probability
    }

    /// Returns the signed varsity identity index.
    pub fn varsity_identity_index(&self) -> i16 {
        self.varsity_identity_index
    }

    /// Returns varsity color as red, green, and blue bytes.
    pub fn varsity_color(&self) -> [u8; 3] {
        self.varsity_color
    }

    /// Returns lifetime trophies from inner version 112009.
    pub fn lifetime_trophies(&self) -> Option<i16> {
        self.lifetime_trophies
    }

    /// Returns the static varsity attractiveness modifier from version 112011.
    pub fn static_varsity_attractiveness_modifier(&self) -> Option<i16> {
        self.static_varsity_attractiveness_modifier
    }

    /// Returns the dynamic varsity attractiveness modifier from version 112012.
    pub fn dynamic_varsity_attractiveness_modifier(&self) -> Option<i16> {
        self.dynamic_varsity_attractiveness_modifier
    }

    /// Returns the sports field from inner version 112013.
    pub fn sports(&self) -> Option<i16> {
        self.sports
    }

    /// Returns away-match flags from inner version 112014.
    pub fn away_matches_done(&self) -> Option<[bool; 5]> {
        self.away_matches_done
    }

    /// Returns campus-building count from inner version 112024.
    pub fn campus_building_count(&self) -> Option<i16> {
        self.campus_building_count
    }
}

impl DistrictParkVisitorData {
    /// Returns the total visitor count.
    pub fn total_visitors(&self) -> u32 {
        self.total_visitors
    }

    /// Returns temporary ticket income.
    pub fn temporary_ticket_income(&self) -> u32 {
        self.temporary_ticket_income
    }

    /// Returns final ticket income.
    pub fn final_ticket_income(&self) -> u32 {
        self.final_ticket_income
    }

    /// Returns the ticket price.
    pub fn ticket_price(&self) -> u16 {
        self.ticket_price
    }

    /// Returns the temporary resident count.
    pub fn temporary_residents(&self) -> u16 {
        self.temporary_residents
    }

    /// Returns the final resident count.
    pub fn final_residents(&self) -> u16 {
        self.final_residents
    }

    /// Returns the temporary tourist count.
    pub fn temporary_tourists(&self) -> u16 {
        self.temporary_tourists
    }

    /// Returns the final tourist count.
    pub fn final_tourists(&self) -> u16 {
        self.final_tourists
    }

    /// Returns the last visitor count from inner version 110014.
    pub fn last_visitor_count(&self) -> Option<u32> {
        self.last_visitor_count
    }
}

impl DistrictParkEditor {
    /// Returns the index in the 128-entry park buffer.
    pub fn park_index(&self) -> usize {
        self.record.park_index
    }

    /// Returns the current random seed.
    pub fn random_seed(&self) -> u64 {
        self.record.random_seed
    }

    /// Replaces the random seed in its big-endian on-disk field.
    pub fn set_random_seed(&mut self, value: u64) -> Result<(), CrpError> {
        self.write_field(0, &value.to_be_bytes(), "park random seed")?;
        self.record.random_seed = value;
        Ok(())
    }

    /// Returns the current main-gate building index.
    pub fn main_gate(&self) -> u16 {
        self.record.main_gate
    }

    /// Replaces the main-gate building index in big-endian form.
    pub fn set_main_gate(&mut self, value: u16) -> Result<(), CrpError> {
        self.write_field(8, &value.to_be_bytes(), "park main gate")?;
        self.record.main_gate = value;
        Ok(())
    }

    /// Returns the current park condition.
    pub fn condition(&self) -> u16 {
        self.record.condition
    }

    /// Replaces the park condition in big-endian form.
    pub fn set_condition(&mut self, value: u16) -> Result<(), CrpError> {
        self.write_field(10, &value.to_be_bytes(), "park condition")?;
        self.record.condition = value;
        Ok(())
    }

    /// Returns the current park level.
    pub fn level(&self) -> u8 {
        self.record.level
    }

    /// Replaces the park level byte.
    pub fn set_level(&mut self, value: u8) -> Result<(), CrpError> {
        self.write_field(self.record.level_offset, &[value], "park level")?;
        self.record.level = value;
        Ok(())
    }

    /// Returns ticket and visitor fields when serialized by this version.
    pub fn visitor_data(&self) -> Option<&DistrictParkVisitorData> {
        self.record.visitor_data()
    }

    /// Returns gate values when serialized by this version.
    pub fn gate_data(&self) -> Option<DistrictParkGateData> {
        self.record.gate_data()
    }

    /// Replaces the random gate and temporary/final gate counts.
    pub fn set_gate_data(&mut self, value: DistrictParkGateData) -> Result<(), CrpError> {
        let offset = self
            .record
            .post_level_offsets
            .gates
            .ok_or_else(|| invalid_district_manager(0, "park gate fields are absent"))?;
        self.write_field(offset, &value.random_gate.to_be_bytes(), "random gate")?;
        self.write_field(offset + 2, &[value.temporary_count], "temporary gate count")?;
        self.write_field(offset + 3, &[value.final_count], "final gate count")?;
        self.record.gate_data = Some(value);
        Ok(())
    }

    /// Returns visitor and main-building capacities when serialized by this version.
    pub fn capacities(&self) -> Option<DistrictParkCapacities> {
        self.record.capacities()
    }

    /// Replaces all four temporary/final visitor and main-building capacities.
    pub fn set_capacities(&mut self, value: DistrictParkCapacities) -> Result<(), CrpError> {
        let offset = self
            .record
            .post_level_offsets
            .capacities
            .ok_or_else(|| invalid_district_manager(0, "park capacities are absent"))?;
        for (field_offset, field_value, name) in [
            (
                offset,
                value.temporary_visitors,
                "temporary visitor capacity",
            ),
            (offset + 2, value.final_visitors, "final visitor capacity"),
            (offset + 4, value.temporary_main, "temporary main capacity"),
            (offset + 6, value.final_main, "final main capacity"),
        ] {
            self.write_field(field_offset, &field_value.to_be_bytes(), name)?;
        }
        self.record.capacities = Some(value);
        Ok(())
    }

    /// Returns the day/night counter when serialized by this version.
    pub fn day_night_count(&self) -> Option<u8> {
        self.record.day_night_count()
    }

    /// Replaces the day/night counter.
    pub fn set_day_night_count(&mut self, value: u8) -> Result<(), CrpError> {
        let offset = self
            .record
            .post_level_offsets
            .day_night_count
            .ok_or_else(|| invalid_district_manager(0, "day/night field is absent"))?;
        self.write_field(offset, &[value], "day/night counter")?;
        self.record.day_night_count = Some(value);
        Ok(())
    }

    /// Returns attractiveness accumulators when serialized by this version.
    pub fn attractiveness(&self) -> Option<DistrictParkAttractiveness> {
        self.record.attractiveness()
    }

    /// Replaces temporary and final attractiveness accumulators.
    pub fn set_attractiveness(
        &mut self,
        value: DistrictParkAttractiveness,
    ) -> Result<(), CrpError> {
        let offset = self
            .record
            .post_level_offsets
            .attractiveness
            .ok_or_else(|| invalid_district_manager(0, "park attractiveness fields are absent"))?;
        self.write_field(
            offset,
            &value.temporary.to_be_bytes(),
            "temporary attractiveness",
        )?;
        self.write_field(
            offset + 2,
            &value.final_value.to_be_bytes(),
            "final attractiveness",
        )?;
        self.record.attractiveness = Some(value);
        Ok(())
    }

    /// Returns the park area factor when serialized by this version.
    pub fn area_factor(&self) -> Option<u8> {
        self.record.area_factor()
    }

    /// Replaces the park area factor.
    pub fn set_area_factor(&mut self, value: u8) -> Result<(), CrpError> {
        let offset = self
            .record
            .post_level_offsets
            .area_factor
            .ok_or_else(|| invalid_district_manager(0, "park area factor is absent"))?;
        self.write_field(offset, &[value], "park area factor")?;
        self.record.area_factor = Some(value);
        Ok(())
    }

    /// Returns work and storage deltas when serialized by this version.
    pub fn work_storage_deltas(&self) -> Option<DistrictParkWorkStorageDeltas> {
        self.record.work_storage_deltas()
    }

    /// Replaces temporary/final work-efficiency and storage delta bytes.
    pub fn set_work_storage_deltas(
        &mut self,
        value: DistrictParkWorkStorageDeltas,
    ) -> Result<(), CrpError> {
        let offset = self
            .record
            .post_level_offsets
            .work_storage_deltas
            .ok_or_else(|| invalid_district_manager(0, "park work/storage fields are absent"))?;
        self.write_field(
            offset,
            &[
                value.temporary_work_efficiency,
                value.temporary_storage,
                value.final_work_efficiency,
                value.final_storage,
            ],
            "park work/storage deltas",
        )?;
        self.record.work_storage_deltas = Some(value);
        Ok(())
    }

    /// Returns production and worker counters when serialized by this version.
    pub fn production_data(&self) -> Option<DistrictParkProductionData> {
        self.record.production_data()
    }

    /// Replaces total production and temporary/final worker counters.
    ///
    /// Production must fit in `u32` before inner version 111015.
    pub fn set_production_data(
        &mut self,
        value: DistrictParkProductionData,
    ) -> Result<(), CrpError> {
        let offsets = self
            .record
            .post_level_offsets
            .production
            .ok_or_else(|| invalid_district_manager(0, "park production fields are absent"))?;
        if offsets.total_production_bytes == PARK_PRODUCTION_U32_BYTES {
            let production = u32::try_from(value.total_production).map_err(|_| {
                invalid_district_manager(
                    offsets.total_production,
                    "park production does not fit the u32 field in this inner version",
                )
            })?;
            self.write_field(
                offsets.total_production,
                &production.to_be_bytes(),
                "park total production",
            )?;
        } else {
            self.write_field(
                offsets.total_production,
                &value.total_production.to_be_bytes(),
                "park total production",
            )?;
        }
        self.write_field(
            offsets.temporary_workers,
            &value.temporary_workers.to_be_bytes(),
            "temporary park workers",
        )?;
        self.write_field(
            offsets.final_workers,
            &value.final_workers.to_be_bytes(),
            "final park workers",
        )?;
        self.record.production_data = Some(value);
        Ok(())
    }

    /// Returns industry resources in the fixed DLL wire order when this park has them.
    pub fn industry_resources(&self) -> Option<&[DistrictParkIndustryResource]> {
        self.record.industry_resources()
    }

    /// Replaces one industry resource record without changing the surrounding park bytes.
    ///
    /// Returns an error when this park has no industry resources or when the trade-data
    /// presence does not match the park's inner-version gate.
    pub fn set_industry_resource(
        &mut self,
        value: DistrictParkIndustryResource,
    ) -> Result<(), CrpError> {
        let offsets = self
            .record
            .industry_resource_offsets
            .ok_or_else(|| invalid_district_manager(0, "industry resource fields are absent"))?;
        let index = value.kind.index();
        if self
            .record
            .industry_resources
            .as_ref()
            .and_then(|resources| resources.get(index))
            .is_none()
        {
            return Err(invalid_district_manager(
                0,
                "industry resource kind is outside this park's resource table",
            ));
        }
        let has_trade_data = offsets.record_bytes == INDUSTRY_RESOURCE_EXPANDED_BYTES;
        let encoded = encode_industry_resource(&value, has_trade_data)?;
        let resource_offset = index
            .checked_mul(offsets.record_bytes)
            .and_then(|relative| offsets.first_record.checked_add(relative))
            .ok_or(CrpError::ArithmeticOverflow(
                "industry resource record offset",
            ))?;
        self.write_field(resource_offset, &encoded, "industry resource record")?;
        let resources =
            self.record.industry_resources.as_mut().ok_or_else(|| {
                invalid_district_manager(0, "industry resource fields are absent")
            })?;
        resources[index] = value;
        Ok(())
    }

    /// Replaces the total visitor count.
    pub fn set_total_visitors(&mut self, value: u32) -> Result<(), CrpError> {
        let offset = self.visitor_offsets()?.total_visitors;
        self.write_field(offset, &value.to_be_bytes(), "total visitor count")?;
        self.visitor_data_mut()?.total_visitors = value;
        Ok(())
    }

    /// Replaces temporary ticket income.
    pub fn set_temporary_ticket_income(&mut self, value: u32) -> Result<(), CrpError> {
        let offset = self.visitor_offsets()?.temporary_ticket_income;
        self.write_field(offset, &value.to_be_bytes(), "temporary ticket income")?;
        self.visitor_data_mut()?.temporary_ticket_income = value;
        Ok(())
    }

    /// Replaces final ticket income.
    pub fn set_final_ticket_income(&mut self, value: u32) -> Result<(), CrpError> {
        let offset = self.visitor_offsets()?.final_ticket_income;
        self.write_field(offset, &value.to_be_bytes(), "final ticket income")?;
        self.visitor_data_mut()?.final_ticket_income = value;
        Ok(())
    }

    /// Replaces the ticket price.
    pub fn set_ticket_price(&mut self, value: u16) -> Result<(), CrpError> {
        let offset = self.visitor_offsets()?.ticket_price;
        self.write_field(offset, &value.to_be_bytes(), "ticket price")?;
        self.visitor_data_mut()?.ticket_price = value;
        Ok(())
    }

    /// Replaces the temporary resident count.
    pub fn set_temporary_residents(&mut self, value: u16) -> Result<(), CrpError> {
        let offset = self.visitor_offsets()?.temporary_residents;
        self.write_field(offset, &value.to_be_bytes(), "temporary resident count")?;
        self.visitor_data_mut()?.temporary_residents = value;
        Ok(())
    }

    /// Replaces the final resident count.
    pub fn set_final_residents(&mut self, value: u16) -> Result<(), CrpError> {
        let offset = self.visitor_offsets()?.final_residents;
        self.write_field(offset, &value.to_be_bytes(), "final resident count")?;
        self.visitor_data_mut()?.final_residents = value;
        Ok(())
    }

    /// Replaces the temporary tourist count.
    pub fn set_temporary_tourists(&mut self, value: u16) -> Result<(), CrpError> {
        let offset = self.visitor_offsets()?.temporary_tourists;
        self.write_field(offset, &value.to_be_bytes(), "temporary tourist count")?;
        self.visitor_data_mut()?.temporary_tourists = value;
        Ok(())
    }

    /// Replaces the final tourist count.
    pub fn set_final_tourists(&mut self, value: u16) -> Result<(), CrpError> {
        let offset = self.visitor_offsets()?.final_tourists;
        self.write_field(offset, &value.to_be_bytes(), "final tourist count")?;
        self.visitor_data_mut()?.final_tourists = value;
        Ok(())
    }

    /// Replaces the last visitor count from inner version 110014.
    pub fn set_last_visitor_count(&mut self, value: u32) -> Result<(), CrpError> {
        let offset = self
            .visitor_offsets()?
            .last_visitor_count
            .ok_or_else(|| invalid_district_manager(0, "last visitor count is absent"))?;
        self.write_field(offset, &value.to_be_bytes(), "last visitor count")?;
        self.visitor_data_mut()?.last_visitor_count = Some(value);
        Ok(())
    }

    /// Returns the leading student and academic-work values when present.
    pub fn campus_data(&self) -> Option<DistrictCampusData> {
        self.record.campus_data()
    }

    /// Replaces the fixed 22-byte campus summary while preserving the rest of
    /// the versioned campus block exactly.
    pub fn set_campus_data(&mut self, value: DistrictCampusData) -> Result<(), CrpError> {
        let offset = self
            .record
            .campus_data_offset
            .ok_or_else(|| invalid_district_manager(0, "campus fields are absent"))?;
        let mut bytes = [0_u8; 22];
        bytes[0..2].copy_from_slice(&value.student_count.to_be_bytes());
        bytes[2..4].copy_from_slice(&value.student_capacity.to_be_bytes());
        bytes[4..6].copy_from_slice(&value.campus_building_attractiveness.to_be_bytes());
        for (index, item) in value.academic_works.iter().enumerate() {
            let start = 6 + index * 4;
            bytes[start..start + 4].copy_from_slice(&item.to_be_bytes());
        }
        self.write_field(offset, &bytes, "campus student and academic-work fields")?;
        self.record.campus_data = Some(value);
        Ok(())
    }

    /// Returns campus reports in serialized order when present.
    pub fn campus_year_reports(&self) -> Option<&[DistrictCampusYearReportData]> {
        self.record.campus_year_reports()
    }

    /// Returns an editor for one campus report in serialized order.
    pub fn campus_year_report_mut(
        &mut self,
        index: usize,
    ) -> Result<DistrictCampusYearReportEditor<'_>, CrpError> {
        let DistrictParkRecord {
            serialized,
            campus_year_reports,
            ..
        } = &mut self.record;
        let reports = campus_year_reports
            .as_mut()
            .ok_or_else(|| invalid_district_manager(0, "campus reports are absent"))?;
        let report = reports.get_mut(index).ok_or_else(|| {
            invalid_district_manager(0, format!("campus report index {index} is out of range"))
        })?;
        Ok(DistrictCampusYearReportEditor { serialized, report })
    }

    /// Returns remaining campus and varsity values when present.
    pub fn campus_sports_data(&self) -> Option<&DistrictCampusSportsData> {
        self.record.campus_sports_data()
    }

    /// Returns an editor for campus and varsity fields.
    ///
    /// Editing an encoded array replaces only that array and may change the
    /// serialized park-record length.
    pub fn campus_sports_mut(&mut self) -> Result<DistrictCampusSportsEditor<'_>, CrpError> {
        let DistrictParkRecord {
            serialized,
            campus_sports_data,
            ..
        } = &mut self.record;
        let data = campus_sports_data
            .as_mut()
            .ok_or_else(|| invalid_district_manager(0, "campus sports fields are absent"))?;
        Ok(DistrictCampusSportsEditor { serialized, data })
    }

    /// Writes the edited record while retaining every unmodeled byte.
    pub fn encode(&self) -> Vec<u8> {
        self.record.serialized.clone()
    }

    pub(crate) fn original_bytes(&self) -> &[u8] {
        &self.original_serialized
    }

    fn write_field(&mut self, offset: usize, value: &[u8], field: &str) -> Result<(), CrpError> {
        let end = offset
            .checked_add(value.len())
            .ok_or(CrpError::ArithmeticOverflow(
                "DistrictManager park field end",
            ))?;
        let destination = self.record.serialized.get_mut(offset..end).ok_or_else(|| {
            invalid_district_manager(offset, format!("{field} is outside the park record"))
        })?;
        destination.copy_from_slice(value);
        Ok(())
    }

    fn visitor_offsets(&self) -> Result<ParkVisitorFieldOffsets, CrpError> {
        self.record
            .visitor_data
            .as_ref()
            .map(|data| data.field_offsets)
            .ok_or_else(|| invalid_district_manager(0, "visitor fields are absent"))
    }

    fn visitor_data_mut(&mut self) -> Result<&mut DistrictParkVisitorData, CrpError> {
        self.record
            .visitor_data
            .as_mut()
            .ok_or_else(|| invalid_district_manager(0, "visitor fields are absent"))
    }
}

impl DistrictCampusYearReportEditor<'_> {
    /// Returns the current decoded report values.
    pub fn report(&self) -> &DistrictCampusYearReportData {
        self.report
    }

    /// Replaces student count.
    pub fn set_student_count(&mut self, value: i32) -> Result<(), CrpError> {
        self.write_field(
            self.report.field_offsets.student_count,
            &value.to_be_bytes(),
            "campus report student count",
        )?;
        self.report.student_count = Some(value);
        Ok(())
    }

    /// Replaces campus attractiveness.
    pub fn set_campus_attractiveness(&mut self, value: i32) -> Result<(), CrpError> {
        self.write_field(
            self.report.field_offsets.campus_attractiveness,
            &value.to_be_bytes(),
            "campus report attractiveness",
        )?;
        self.report.campus_attractiveness = Some(value);
        Ok(())
    }

    /// Replaces the report's signed reputation level.
    pub fn set_reputation_level(&mut self, value: i8) -> Result<(), CrpError> {
        self.write_field(
            self.report.field_offsets.reputation_level,
            &[value as u8],
            "campus report reputation level",
        )?;
        self.report.reputation_level = Some(value);
        Ok(())
    }

    /// Replaces the report's signed grant type.
    pub fn set_grant_type(&mut self, value: i8) -> Result<(), CrpError> {
        self.write_field(
            self.report.field_offsets.grant_type,
            &[value as u8],
            "campus report grant type",
        )?;
        self.report.grant_type = Some(value);
        Ok(())
    }

    /// Replaces grant work index.
    pub fn set_grant_work_index(&mut self, value: i16) -> Result<(), CrpError> {
        self.write_field(
            self.report.field_offsets.grant_work_index,
            &value.to_be_bytes(),
            "campus report grant work index",
        )?;
        self.report.grant_work_index = Some(value);
        Ok(())
    }

    /// Replaces the four academic-work values.
    pub fn set_academic_works(&mut self, value: [i32; 4]) -> Result<(), CrpError> {
        let mut bytes = [0_u8; 16];
        for (index, item) in value.iter().enumerate() {
            let start = index * 4;
            bytes[start..start + 4].copy_from_slice(&item.to_be_bytes());
        }
        self.write_field(
            self.report.field_offsets.academic_works,
            &bytes,
            "campus report academic works",
        )?;
        self.report.academic_works = Some(value);
        Ok(())
    }

    /// Replaces total prize money.
    pub fn set_total_prize_money(&mut self, value: u64) -> Result<(), CrpError> {
        self.write_field(
            self.report.field_offsets.total_prize_money,
            &value.to_be_bytes(),
            "campus report total prize money",
        )?;
        self.report.total_prize_money = Some(value);
        Ok(())
    }

    /// Replaces the signed toga-party count from inner version 112017.
    pub fn set_toga_party_count(&mut self, value: i8) -> Result<(), CrpError> {
        self.write_field(
            self.report.field_offsets.toga_party_count,
            &[value as u8],
            "campus report toga-party count",
        )?;
        self.report.toga_party_count = Some(value);
        Ok(())
    }

    /// Replaces the three signed toga-party seeds from inner version 112017.
    pub fn set_toga_party_seeds(&mut self, value: [i64; 3]) -> Result<(), CrpError> {
        let mut bytes = [0_u8; 24];
        for (index, item) in value.iter().enumerate() {
            let start = index * 8;
            bytes[start..start + 8].copy_from_slice(&item.to_be_bytes());
        }
        self.write_field(
            self.report.field_offsets.toga_party_seeds,
            &bytes,
            "campus report toga-party seeds",
        )?;
        self.report.toga_party_seeds = Some(value);
        Ok(())
    }

    /// Replaces total works from inner version 112026.
    pub fn set_total_works(&mut self, value: i16) -> Result<(), CrpError> {
        self.write_field(
            self.report.field_offsets.total_works,
            &value.to_be_bytes(),
            "campus report total works",
        )?;
        self.report.total_works = Some(value);
        Ok(())
    }

    fn write_field(
        &mut self,
        offset: Option<usize>,
        value: &[u8],
        field: &str,
    ) -> Result<(), CrpError> {
        let offset =
            offset.ok_or_else(|| invalid_district_manager(0, format!("{field} is absent")))?;
        let end = offset
            .checked_add(value.len())
            .ok_or(CrpError::ArithmeticOverflow(
                "DistrictManager campus report field end",
            ))?;
        let destination = self.serialized.get_mut(offset..end).ok_or_else(|| {
            invalid_district_manager(offset, format!("{field} is outside the park record"))
        })?;
        destination.copy_from_slice(value);
        Ok(())
    }
}

impl DistrictCampusSportsEditor<'_> {
    /// Returns the current decoded campus and varsity values.
    pub fn data(&self) -> &DistrictCampusSportsData {
        self.data
    }

    /// Replaces the campus-state bool, writing canonical byte `0` or `1`.
    pub fn set_is_main_campus(&mut self, value: bool) -> Result<(), CrpError> {
        self.write_field(
            self.data.field_offsets.is_main_campus,
            &[u8::from(value)],
            "is-main-campus flag",
        )?;
        self.data.is_main_campus = value;
        Ok(())
    }

    /// Replaces the campus grant type.
    pub fn set_grant_type(&mut self, value: u8) -> Result<(), CrpError> {
        self.write_field(
            self.data.field_offsets.grant_type,
            &[value],
            "campus grant type",
        )?;
        self.data.grant_type = value;
        Ok(())
    }

    /// Replaces the coach count.
    pub fn set_coach_count(&mut self, value: u8) -> Result<(), CrpError> {
        self.write_field(
            self.data.field_offsets.coach_count,
            &[value],
            "campus coach count",
        )?;
        self.data.coach_count = value;
        Ok(())
    }

    /// Replaces the signed cheerleading budget.
    pub fn set_cheerleading_budget(&mut self, value: i16) -> Result<(), CrpError> {
        self.write_field(
            self.data.field_offsets.cheerleading_budget,
            &value.to_be_bytes(),
            "cheerleading budget",
        )?;
        self.data.cheerleading_budget = value;
        Ok(())
    }

    /// Replaces academic staff count from inner version 112025.
    pub fn set_academic_staff_count(&mut self, value: u8) -> Result<(), CrpError> {
        self.write_field(
            self.data.field_offsets.academic_staff_count,
            &[value],
            "academic staff count",
        )?;
        self.data.academic_staff_count = Some(value);
        Ok(())
    }

    /// Replaces academic staff accumulation from inner version 112025.
    pub fn set_academic_staff_accumulation(&mut self, value: f32) -> Result<(), CrpError> {
        let bits = value.to_bits();
        self.write_field(
            self.data.field_offsets.academic_staff_accumulation,
            &bits.to_be_bytes(),
            "academic staff accumulation",
        )?;
        self.data.academic_staff_accumulation_bits = Some(bits);
        Ok(())
    }

    /// Replaces all 25 raw signed coach-hire tick values.
    ///
    /// The values are written as `EncodedArray.Long`; this preserves their
    /// integer bits without the game's lossy `f32` and `DateTime` conversion.
    pub fn set_coach_hire_time_ticks(&mut self, values: [i64; 25]) -> Result<(), CrpError> {
        let encoded_values = values.map(|value| value as u64);
        let mut encoded = Vec::new();
        write_encoded_ulong_array(&mut encoded, &encoded_values);
        self.replace_encoded_array(
            self.data.field_offsets.coach_hire_time_array,
            encoded,
            "coach-hire-time array",
        )?;
        self.data.coach_hire_time_ticks = values;
        Ok(())
    }

    /// Replaces varsity win probability.
    pub fn set_win_probability(&mut self, value: u8) -> Result<(), CrpError> {
        self.write_field(
            self.data.field_offsets.win_probability,
            &[value],
            "varsity win probability",
        )?;
        self.data.win_probability = value;
        Ok(())
    }

    /// Replaces the signed varsity identity index.
    pub fn set_varsity_identity_index(&mut self, value: i16) -> Result<(), CrpError> {
        self.write_field(
            self.data.field_offsets.varsity_identity_index,
            &value.to_be_bytes(),
            "varsity identity index",
        )?;
        self.data.varsity_identity_index = value;
        Ok(())
    }

    /// Replaces the three varsity color bytes in red, green, blue order.
    pub fn set_varsity_color(&mut self, value: [u8; 3]) -> Result<(), CrpError> {
        let mut encoded = Vec::new();
        write_encoded_byte_array(&mut encoded, &value);
        self.replace_encoded_array(
            self.data.field_offsets.varsity_color_array,
            encoded,
            "varsity color array",
        )?;
        self.data.varsity_color = value;
        Ok(())
    }

    /// Replaces lifetime trophies from inner version 112009.
    pub fn set_lifetime_trophies(&mut self, value: i16) -> Result<(), CrpError> {
        self.write_field(
            self.data.field_offsets.lifetime_trophies,
            &value.to_be_bytes(),
            "lifetime trophies",
        )?;
        self.data.lifetime_trophies = Some(value);
        Ok(())
    }

    /// Replaces the static varsity attractiveness modifier from version 112011.
    pub fn set_static_varsity_attractiveness_modifier(
        &mut self,
        value: i16,
    ) -> Result<(), CrpError> {
        self.write_field(
            self.data
                .field_offsets
                .static_varsity_attractiveness_modifier,
            &value.to_be_bytes(),
            "static varsity attractiveness modifier",
        )?;
        self.data.static_varsity_attractiveness_modifier = Some(value);
        Ok(())
    }

    /// Replaces the dynamic varsity attractiveness modifier from version 112012.
    pub fn set_dynamic_varsity_attractiveness_modifier(
        &mut self,
        value: i16,
    ) -> Result<(), CrpError> {
        self.write_field(
            self.data
                .field_offsets
                .dynamic_varsity_attractiveness_modifier,
            &value.to_be_bytes(),
            "dynamic varsity attractiveness modifier",
        )?;
        self.data.dynamic_varsity_attractiveness_modifier = Some(value);
        Ok(())
    }

    /// Replaces the sports field from inner version 112013.
    pub fn set_sports(&mut self, value: i16) -> Result<(), CrpError> {
        self.write_field(
            self.data.field_offsets.sports,
            &value.to_be_bytes(),
            "varsity sports field",
        )?;
        self.data.sports = Some(value);
        Ok(())
    }

    /// Replaces the five away-match flags from inner version 112014.
    pub fn set_away_matches_done(&mut self, value: [bool; 5]) -> Result<(), CrpError> {
        let mut encoded = Vec::new();
        write_encoded_bool_array(&mut encoded, &value);
        self.replace_encoded_array(
            self.data.field_offsets.away_matches_array,
            encoded,
            "away-match array",
        )?;
        self.data.away_matches_done = Some(value);
        Ok(())
    }

    /// Replaces campus-building count from inner version 112024.
    pub fn set_campus_building_count(&mut self, value: i16) -> Result<(), CrpError> {
        self.write_field(
            self.data.field_offsets.campus_building_count,
            &value.to_be_bytes(),
            "campus building count",
        )?;
        self.data.campus_building_count = Some(value);
        Ok(())
    }

    fn write_field(
        &mut self,
        offset: Option<usize>,
        value: &[u8],
        field: &str,
    ) -> Result<(), CrpError> {
        let offset =
            offset.ok_or_else(|| invalid_district_manager(0, format!("{field} is absent")))?;
        let end = offset
            .checked_add(value.len())
            .ok_or(CrpError::ArithmeticOverflow(
                "DistrictManager campus sports field end",
            ))?;
        let destination = self.serialized.get_mut(offset..end).ok_or_else(|| {
            invalid_district_manager(offset, format!("{field} is outside the park record"))
        })?;
        destination.copy_from_slice(value);
        Ok(())
    }

    fn replace_encoded_array(
        &mut self,
        range: Option<SerializedByteRange>,
        replacement: Vec<u8>,
        field: &str,
    ) -> Result<(), CrpError> {
        let range =
            range.ok_or_else(|| invalid_district_manager(0, format!("{field} is absent")))?;
        if range.start > range.end || range.end > self.serialized.len() {
            return Err(invalid_district_manager(
                range.start,
                format!("{field} range is outside the park record"),
            ));
        }
        let old_len = range.end - range.start;
        let new_len = self
            .serialized
            .len()
            .checked_sub(old_len)
            .and_then(|length| length.checked_add(replacement.len()))
            .ok_or(CrpError::ArithmeticOverflow(
                "DistrictManager campus array replacement length",
            ))?;
        let replacement_end =
            range
                .start
                .checked_add(replacement.len())
                .ok_or(CrpError::ArithmeticOverflow(
                    "DistrictManager campus array replacement end",
                ))?;
        let field_offsets = self
            .data
            .field_offsets
            .shift_after_array_replacement(range, replacement_end)?;
        if replacement_end > new_len {
            return Err(CrpError::ArithmeticOverflow(
                "DistrictManager campus array replacement result length",
            ));
        }
        self.serialized.splice(range.start..range.end, replacement);
        self.data.field_offsets = field_offsets;
        Ok(())
    }
}

impl CampusSportsFieldOffsets {
    fn shift_after_array_replacement(
        mut self,
        replaced: SerializedByteRange,
        replacement_end: usize,
    ) -> Result<Self, CrpError> {
        self.is_main_campus = shift_field_offset(self.is_main_campus, replaced, replacement_end)?;
        self.grant_type = shift_field_offset(self.grant_type, replaced, replacement_end)?;
        self.coach_count = shift_field_offset(self.coach_count, replaced, replacement_end)?;
        self.cheerleading_budget =
            shift_field_offset(self.cheerleading_budget, replaced, replacement_end)?;
        self.academic_staff_count =
            shift_field_offset(self.academic_staff_count, replaced, replacement_end)?;
        self.academic_staff_accumulation =
            shift_field_offset(self.academic_staff_accumulation, replaced, replacement_end)?;
        self.win_probability = shift_field_offset(self.win_probability, replaced, replacement_end)?;
        self.varsity_identity_index =
            shift_field_offset(self.varsity_identity_index, replaced, replacement_end)?;
        self.lifetime_trophies =
            shift_field_offset(self.lifetime_trophies, replaced, replacement_end)?;
        self.static_varsity_attractiveness_modifier = shift_field_offset(
            self.static_varsity_attractiveness_modifier,
            replaced,
            replacement_end,
        )?;
        self.dynamic_varsity_attractiveness_modifier = shift_field_offset(
            self.dynamic_varsity_attractiveness_modifier,
            replaced,
            replacement_end,
        )?;
        self.sports = shift_field_offset(self.sports, replaced, replacement_end)?;
        self.campus_building_count =
            shift_field_offset(self.campus_building_count, replaced, replacement_end)?;
        self.coach_hire_time_array =
            shift_array_range(self.coach_hire_time_array, replaced, replacement_end)?;
        self.varsity_color_array =
            shift_array_range(self.varsity_color_array, replaced, replacement_end)?;
        self.away_matches_array =
            shift_array_range(self.away_matches_array, replaced, replacement_end)?;
        Ok(self)
    }
}

fn shift_field_offset(
    offset: Option<usize>,
    replaced: SerializedByteRange,
    replacement_end: usize,
) -> Result<Option<usize>, CrpError> {
    offset
        .map(|offset| {
            if offset >= replaced.end {
                replacement_end.checked_add(offset - replaced.end).ok_or(
                    CrpError::ArithmeticOverflow("DistrictManager campus field offset adjustment"),
                )
            } else if offset > replaced.start {
                Err(invalid_district_manager(
                    offset,
                    "campus field offset overlaps an encoded array",
                ))
            } else {
                Ok(offset)
            }
        })
        .transpose()
}

fn shift_array_range(
    range: Option<SerializedByteRange>,
    replaced: SerializedByteRange,
    replacement_end: usize,
) -> Result<Option<SerializedByteRange>, CrpError> {
    range
        .map(|range| {
            if range == replaced {
                Ok(SerializedByteRange {
                    start: replaced.start,
                    end: replacement_end,
                })
            } else if range.end <= replaced.start {
                Ok(range)
            } else if range.start >= replaced.end {
                let tail_start = replacement_end
                    .checked_add(range.start - replaced.end)
                    .ok_or(CrpError::ArithmeticOverflow(
                        "DistrictManager campus array offset adjustment",
                    ))?;
                let tail_end = replacement_end
                    .checked_add(range.end - replaced.end)
                    .ok_or(CrpError::ArithmeticOverflow(
                        "DistrictManager campus array end adjustment",
                    ))?;
                Ok(SerializedByteRange {
                    start: tail_start,
                    end: tail_end,
                })
            } else {
                Err(invalid_district_manager(
                    range.start,
                    "campus encoded-array ranges overlap",
                ))
            }
        })
        .transpose()
}

/// Population counters retained from one active district's data record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictPopulationRecord {
    district_index: usize,
    random_seed: u64,
    temporary_population: u32,
    final_population: u32,
    population_deltas: [i16; POPULATION_DELTA_COUNT],
    final_happiness: u8,
    final_crime_rate: Option<u8>,
    age_populations: Vec<DistrictAgePopulation>,
    last_average_lifespan: Option<i8>,
    private_data: Vec<DistrictPrivateDataBlock>,
    specializations: Vec<DistrictSpecializationBlock>,
    demographic_data: Vec<DistrictDemographicBlock>,
    statistics: Vec<DistrictStatisticsBlock>,
    tourism: Vec<DistrictTourismBlock>,
    subcultures: Vec<DistrictSubCultureBlock>,
    hotel_data: Option<DistrictHotelBlock>,
    district_style_present: Option<bool>,
    district_style_name: Option<String>,
    library_visitor_data: Option<DistrictAgePopulation>,
    airline: Option<i8>,
}

/// An opaque fixed-size production, consumption, or resource data block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictStatisticsBlock {
    kind: DistrictStatisticsKind,
    bytes: Vec<u8>,
}

impl DistrictStatisticsBlock {
    /// Returns which statistics field this block represents.
    pub fn kind(&self) -> DistrictStatisticsKind {
        self.kind
    }

    /// Returns the exact serialized bytes for this currently opaque block.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A district statistics block in its wire-order position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistrictStatisticsKind {
    Production,
    PlayerConsumption,
    ResidentialConsumption,
    CommercialConsumption,
    IndustrialConsumption,
    OfficeConsumption,
    Usage,
    Ground,
    ImportResources,
    ExportResources,
}

/// One of the three district tourist metric records in wire order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistrictTourismKind {
    Tourist1,
    Tourist2,
    Tourist3,
}

/// An opaque district tourist metric record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictTourismBlock {
    kind: DistrictTourismKind,
    bytes: Vec<u8>,
}

impl DistrictTourismBlock {
    /// Returns which tourist metric record this block represents.
    pub fn kind(&self) -> DistrictTourismKind {
        self.kind
    }

    /// Returns the exact serialized bytes for this currently opaque block.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// One of the district subculture records in wire order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistrictSubCultureKind {
    Hippie,
    Hipster,
    Redneck,
    Gangsta,
}

/// An opaque district subculture metric record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictSubCultureBlock {
    kind: DistrictSubCultureKind,
    bytes: Vec<u8>,
}

/// Opaque legacy hotel statistics read from versions 117004–117008.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictHotelBlock {
    bytes: Vec<u8>,
}

impl DistrictHotelBlock {
    /// Returns the exact serialized bytes for this currently opaque block.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl DistrictSubCultureBlock {
    /// Returns which subculture this block represents.
    pub fn kind(&self) -> DistrictSubCultureKind {
        self.kind
    }

    /// Returns the exact serialized bytes for this currently opaque block.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A version-bounded birth, education, or student counter block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictDemographicBlock {
    kind: DistrictDemographicKind,
    bytes: Vec<u8>,
}

impl DistrictDemographicBlock {
    /// Returns the demographic category represented by this block.
    pub fn kind(&self) -> DistrictDemographicKind {
        self.kind
    }

    /// Returns the exact serialized bytes for this currently opaque block.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A named district birth, death, education, or student-data block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistrictDemographicKind {
    Births,
    Deaths,
    Educated0,
    Educated1,
    Educated2,
    Educated3,
    Education1,
    Education2,
    Education3,
    Student1,
    Student2,
    Student3,
}

/// An opaque, fixed-size specialization counter block for one district.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictSpecializationBlock {
    kind: DistrictSpecializationKind,
    bytes: Vec<u8>,
}

impl DistrictSpecializationBlock {
    /// Returns which specialization this block represents.
    pub fn kind(&self) -> DistrictSpecializationKind {
        self.kind
    }

    /// Returns the exact serialized bytes for this block.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A district specialization with a serialized `DistrictSpecializationData` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistrictSpecializationKind {
    Farming,
    Forestry,
    Ore,
    Oil,
    Leisure,
    Tourist,
    Organic,
    SelfSufficient,
    HighTech,
}

/// A raw, version-bounded `DistrictPrivateData` block for one district.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictPrivateDataBlock {
    kind: DistrictPrivateDataKind,
    bytes: Vec<u8>,
}

impl DistrictPrivateDataBlock {
    /// Returns which district category this block belongs to.
    pub fn kind(&self) -> DistrictPrivateDataKind {
        self.kind
    }

    /// Returns the exact serialized bytes for this currently opaque block.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// The district category for a private population and building-statistics block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistrictPrivateDataKind {
    Residential,
    Commercial,
    Visitor,
    Industrial,
    Office,
    Player,
}

/// A named pair of temporary and final district counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistrictAgePopulation {
    category: DistrictAgeCategory,
    temporary: u32,
    final_count: u32,
}

impl DistrictAgePopulation {
    /// Returns the age, health, or visitor category represented by this pair.
    pub fn category(&self) -> DistrictAgeCategory {
        self.category
    }

    /// Returns the temporary and final counts.
    pub fn counts(&self) -> (u32, u32) {
        (self.temporary, self.final_count)
    }
}

/// The serialized category associated with one `DistrictAgeData` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistrictAgeCategory {
    Children,
    Teens,
    Young,
    Adults,
    Seniors,
    ChildHealth,
    ChildSickness,
    SeniorHealth,
    SeniorSickness,
    AgeAtDeath,
    DeadSeniors,
    ElementaryEligible,
    HighschoolEligible,
    UniversityEligible,
    LibraryVisitors,
}

impl DistrictPopulationRecord {
    /// Returns the district buffer index associated with this record.
    pub fn district_index(&self) -> usize {
        self.district_index
    }

    /// Returns the saved district randomizer seed.
    pub fn random_seed(&self) -> u64 {
        self.random_seed
    }

    /// Returns the temporary and final population counts.
    pub fn population_counts(&self) -> (u32, u32) {
        (self.temporary_population, self.final_population)
    }

    /// Returns the sixteen saved population deltas.
    pub fn population_deltas(&self) -> &[i16] {
        &self.population_deltas
    }

    /// Returns the saved final happiness value.
    pub fn final_happiness(&self) -> u8 {
        self.final_happiness
    }

    /// Returns the final crime-rate value from inner version 203 onward.
    pub fn final_crime_rate(&self) -> Option<u8> {
        self.final_crime_rate
    }

    /// Returns the age and eligibility counts in their serialized order.
    pub fn age_populations(&self) -> &[DistrictAgePopulation] {
        &self.age_populations
    }

    /// Returns the district's saved average lifespan from inner version 113006.
    pub fn last_average_lifespan(&self) -> Option<i8> {
        self.last_average_lifespan
    }

    /// Returns the exact private-data blocks in serialized category order.
    pub fn private_data(&self) -> &[DistrictPrivateDataBlock] {
        &self.private_data
    }

    /// Returns the specialization blocks in serialized order.
    pub fn specializations(&self) -> &[DistrictSpecializationBlock] {
        &self.specializations
    }

    /// Returns the birth, education, and student blocks in serialized order.
    pub fn demographic_data(&self) -> &[DistrictDemographicBlock] {
        &self.demographic_data
    }

    /// Returns the production, consumption, ground, and resource blocks in wire order.
    pub fn statistics(&self) -> &[DistrictStatisticsBlock] {
        &self.statistics
    }

    /// Returns the tourist metric records in wire order.
    pub fn tourism(&self) -> &[DistrictTourismBlock] {
        &self.tourism
    }

    /// Returns the subculture metric records in wire order.
    pub fn subcultures(&self) -> &[DistrictSubCultureBlock] {
        &self.subcultures
    }

    /// Returns the legacy hotel statistics block, present only in versions 117004–117008.
    pub fn hotel_data(&self) -> Option<&DistrictHotelBlock> {
        self.hotel_data.as_ref()
    }

    /// Returns whether the versioned district-style field is present and enabled.
    pub fn district_style_present(&self) -> Option<bool> {
        self.district_style_present
    }

    /// Returns the district-style name when the style field contains a name.
    pub fn district_style_name(&self) -> Option<&str> {
        self.district_style_name.as_deref()
    }

    /// Returns the library visitor age counters, when serialized.
    pub fn library_visitor_data(&self) -> Option<&DistrictAgePopulation> {
        self.library_visitor_data.as_ref()
    }

    /// Returns the signed airline value, when serialized.
    pub fn airline(&self) -> Option<i8> {
        self.airline
    }
}

impl DistrictManagerFlagsPrefix {
    /// Decodes the opening fields and both known policy bool streams.
    ///
    /// This prefix supports inner versions 164 and newer. It decodes district
    /// population, age, private, specialization, demographic, statistics,
    /// tourism, hotel, style, library-visitor, and airline fields, then stops
    /// before the park records. Opaque blocks retain their exact serialized
    /// bytes. Truncated or malformed fields return an error.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(invalid_district_manager(
                0,
                "DistrictManager prefix requires inner version 164 or newer",
            ));
        }

        let mut reader = Reader::new(bytes);
        let airline_name_present = format_version >= AIRLINE_NAME_VERSION;
        let airline_name = if airline_name_present {
            reader.read_unique_string()?
        } else {
            None
        };
        let airport_panel_opened = if format_version >= AIRPORT_PANEL_VERSION {
            Some(reader.read_u8()? == 1)
        } else {
            None
        };
        let district_flags = read_encoded_uint_array(&mut reader, DISTRICT_COUNT)?;
        let park_flags_present = format_version >= PARK_FLAGS_VERSION;
        let park_flags = if park_flags_present {
            read_encoded_uint_array(&mut reader, DISTRICT_COUNT)?
        } else {
            vec![0; DISTRICT_COUNT]
        };
        let policy_bool_count = district_policy_bool_count(format_version)?;
        let policy_bools = if policy_bool_count == 0 {
            None
        } else {
            Some(read_encoded_bool_array(&mut reader, policy_bool_count)?)
        };
        // The discarded 32 x 128 bool table is read only for versions 43..70.
        // This decoder supports 164+, so that compatibility branch is unreachable.
        let legacy_policy_bools = None;
        let mut district_population_records = Vec::new();
        for (district_index, flags) in district_flags.iter().copied().enumerate() {
            if flags == 0 {
                continue;
            }
            let random_seed = reader.read_u64_be()?;
            let temporary_population = reader.read_u24_be()?;
            let final_population = reader.read_u24_be()?;
            let mut population_deltas = [0_i16; POPULATION_DELTA_COUNT];
            for delta in &mut population_deltas {
                *delta = reader.read_i16_be()?;
            }
            let final_happiness = reader.read_u8()?;
            let final_crime_rate = if format_version >= DISTRICT_CRIME_RATE_VERSION {
                Some(reader.read_u8()?)
            } else {
                None
            };
            let mut age_populations = Vec::new();
            for category in [
                DistrictAgeCategory::Children,
                DistrictAgeCategory::Teens,
                DistrictAgeCategory::Young,
                DistrictAgeCategory::Adults,
                DistrictAgeCategory::Seniors,
            ] {
                age_populations.push(read_district_age_population(&mut reader, category)?);
            }
            if format_version >= DISTRICT_DETAILED_AGE_VERSION {
                for category in [
                    DistrictAgeCategory::ChildHealth,
                    DistrictAgeCategory::ChildSickness,
                    DistrictAgeCategory::SeniorHealth,
                    DistrictAgeCategory::SeniorSickness,
                    DistrictAgeCategory::AgeAtDeath,
                ] {
                    age_populations.push(read_district_age_population(&mut reader, category)?);
                }
            }
            let last_average_lifespan = if format_version >= DISTRICT_DEAD_SENIORS_VERSION {
                age_populations.push(read_district_age_population(
                    &mut reader,
                    DistrictAgeCategory::DeadSeniors,
                )?);
                Some(reader.read_u8()? as i8)
            } else {
                None
            };
            if format_version >= DISTRICT_ELIGIBLE_AGE_VERSION {
                for category in [
                    DistrictAgeCategory::ElementaryEligible,
                    DistrictAgeCategory::HighschoolEligible,
                    DistrictAgeCategory::UniversityEligible,
                ] {
                    age_populations.push(read_district_age_population(&mut reader, category)?);
                }
            }
            let mut private_data = Vec::with_capacity(6);
            for kind in [
                DistrictPrivateDataKind::Residential,
                DistrictPrivateDataKind::Commercial,
            ] {
                private_data.push(read_district_private_data(
                    &mut reader,
                    format_version,
                    kind,
                )?);
            }
            if format_version >= DISTRICT_VISITOR_DATA_VERSION {
                private_data.push(read_district_private_data(
                    &mut reader,
                    format_version,
                    DistrictPrivateDataKind::Visitor,
                )?);
            }
            for kind in [
                DistrictPrivateDataKind::Industrial,
                DistrictPrivateDataKind::Office,
            ] {
                private_data.push(read_district_private_data(
                    &mut reader,
                    format_version,
                    kind,
                )?);
            }
            if format_version >= DISTRICT_PLAYER_DATA_VERSION {
                private_data.push(read_district_private_data(
                    &mut reader,
                    format_version,
                    DistrictPrivateDataKind::Player,
                )?);
            }
            let mut specializations = Vec::new();
            if format_version >= DISTRICT_SPECIALIZATION_VERSION {
                for kind in [
                    DistrictSpecializationKind::Farming,
                    DistrictSpecializationKind::Forestry,
                    DistrictSpecializationKind::Ore,
                    DistrictSpecializationKind::Oil,
                    DistrictSpecializationKind::Leisure,
                    DistrictSpecializationKind::Tourist,
                ] {
                    specializations.push(read_district_specialization(&mut reader, kind)?);
                }
            }
            if format_version >= DISTRICT_ADDITIONAL_SPECIALIZATION_VERSION {
                for kind in [
                    DistrictSpecializationKind::Organic,
                    DistrictSpecializationKind::SelfSufficient,
                    DistrictSpecializationKind::HighTech,
                ] {
                    specializations.push(read_district_specialization(&mut reader, kind)?);
                }
            }
            let mut demographic_data = Vec::new();
            if format_version >= DISTRICT_BIRTH_DEATH_VERSION {
                for kind in [
                    DistrictDemographicKind::Births,
                    DistrictDemographicKind::Deaths,
                ] {
                    demographic_data.push(read_district_demographic(
                        &mut reader,
                        kind,
                        DISTRICT_AGE_PAIR_BYTES,
                    )?);
                }
            }
            if format_version >= DISTRICT_EDUCATION_DATA_VERSION {
                let education_block_bytes = if format_version >= DISTRICT_EDUCATION_WORKER_VERSION {
                    DISTRICT_EDUCATION_WORKER_BYTES
                } else {
                    DISTRICT_EDUCATION_BASE_BYTES
                };
                for kind in [
                    DistrictDemographicKind::Educated0,
                    DistrictDemographicKind::Educated1,
                    DistrictDemographicKind::Educated2,
                    DistrictDemographicKind::Educated3,
                ] {
                    demographic_data.push(read_district_demographic(
                        &mut reader,
                        kind,
                        education_block_bytes,
                    )?);
                }
            }
            if format_version >= DISTRICT_EDUCATION_AGE_VERSION {
                for kind in [
                    DistrictDemographicKind::Education1,
                    DistrictDemographicKind::Education2,
                    DistrictDemographicKind::Education3,
                ] {
                    demographic_data.push(read_district_demographic(
                        &mut reader,
                        kind,
                        DISTRICT_AGE_PAIR_BYTES,
                    )?);
                }
            }
            if format_version >= DISTRICT_STUDENT_AGE_VERSION {
                for kind in [
                    DistrictDemographicKind::Student1,
                    DistrictDemographicKind::Student2,
                    DistrictDemographicKind::Student3,
                ] {
                    demographic_data.push(read_district_demographic(
                        &mut reader,
                        kind,
                        DISTRICT_AGE_PAIR_BYTES,
                    )?);
                }
            }
            let mut statistics = Vec::new();
            if format_version >= DISTRICT_PRODUCTION_VERSION {
                statistics.push(read_district_statistics(
                    &mut reader,
                    DistrictStatisticsKind::Production,
                    district_production_data_size(format_version),
                )?);
                for kind in [
                    DistrictStatisticsKind::PlayerConsumption,
                    DistrictStatisticsKind::ResidentialConsumption,
                    DistrictStatisticsKind::CommercialConsumption,
                    DistrictStatisticsKind::IndustrialConsumption,
                    DistrictStatisticsKind::OfficeConsumption,
                ] {
                    statistics.push(read_district_statistics(
                        &mut reader,
                        kind,
                        district_consumption_data_size(format_version),
                    )?);
                }
            }
            if format_version >= DISTRICT_USAGE_VERSION {
                statistics.push(read_district_statistics(
                    &mut reader,
                    DistrictStatisticsKind::Usage,
                    DISTRICT_USAGE_BYTES,
                )?);
            }
            if format_version >= DISTRICT_GROUND_VERSION {
                statistics.push(read_district_statistics(
                    &mut reader,
                    DistrictStatisticsKind::Ground,
                    DISTRICT_GROUND_BYTES,
                )?);
            }
            if format_version >= DISTRICT_RESOURCE_VERSION {
                let resource_bytes = district_resource_data_size(format_version);
                statistics.push(read_district_statistics(
                    &mut reader,
                    DistrictStatisticsKind::ImportResources,
                    resource_bytes,
                )?);
                statistics.push(read_district_statistics(
                    &mut reader,
                    DistrictStatisticsKind::ExportResources,
                    resource_bytes,
                )?);
            }
            let mut tourism = Vec::new();
            if format_version >= DISTRICT_TOURIST_VERSION {
                let tourist_bytes = district_tourist_data_size(format_version);
                for kind in [
                    DistrictTourismKind::Tourist1,
                    DistrictTourismKind::Tourist2,
                    DistrictTourismKind::Tourist3,
                ] {
                    tourism.push(read_district_tourism(&mut reader, kind, tourist_bytes)?);
                }
            }
            let mut subcultures = Vec::new();
            if format_version >= DISTRICT_SUBCULTURE_VERSION {
                for kind in [
                    DistrictSubCultureKind::Hippie,
                    DistrictSubCultureKind::Hipster,
                    DistrictSubCultureKind::Redneck,
                    DistrictSubCultureKind::Gangsta,
                ] {
                    subcultures.push(read_district_subculture(
                        &mut reader,
                        kind,
                        DISTRICT_SUBCULTURE_BYTES,
                    )?);
                }
            }
            let hotel_data = if (DISTRICT_HOTEL_START_VERSION..DISTRICT_HOTEL_END_VERSION)
                .contains(&format_version)
            {
                Some(DistrictHotelBlock {
                    bytes: reader
                        .read_bytes(district_hotel_data_size(format_version))?
                        .to_vec(),
                })
            } else {
                None
            };
            let district_style_present = if format_version >= DISTRICT_STYLE_VERSION {
                Some(reader.read_u8()? == 1)
            } else {
                None
            };
            let district_style_name = if district_style_present == Some(true) {
                reader.read_unique_string()?
            } else {
                None
            };
            let library_visitor_data = if format_version >= DISTRICT_LIBRARY_VISITOR_VERSION {
                Some(read_district_age_population(
                    &mut reader,
                    DistrictAgeCategory::LibraryVisitors,
                )?)
            } else {
                None
            };
            let airline = if format_version >= DISTRICT_AIRLINE_VERSION {
                Some(reader.read_u8()? as i8)
            } else {
                None
            };
            district_population_records.push(DistrictPopulationRecord {
                district_index,
                random_seed,
                temporary_population,
                final_population,
                population_deltas,
                final_happiness,
                final_crime_rate,
                age_populations,
                last_average_lifespan,
                private_data,
                specializations,
                demographic_data,
                statistics,
                tourism,
                subcultures,
                hotel_data,
                district_style_present,
                district_style_name,
                library_visitor_data,
                airline,
            });
        }
        let consumed = reader.offset();

        Ok((
            Self {
                format_version,
                airline_name_present,
                airline_name,
                airport_panel_opened,
                district_flags,
                park_flags_present,
                park_flags,
                policy_bools,
                legacy_policy_bools,
                district_population_records,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns whether the airline-name field is present in this version.
    pub fn airline_name_present(&self) -> bool {
        self.airline_name_present
    }

    /// Returns the serialized airline name, or `None` for a null/omitted value.
    pub fn airline_name(&self) -> Option<&str> {
        self.airline_name.as_deref()
    }

    /// Returns the airport-panel flag, absent before version 114007.
    pub fn airport_panel_opened(&self) -> Option<bool> {
        self.airport_panel_opened
    }

    /// Returns the 128 district flags.
    pub fn district_flags(&self) -> &[u32] {
        &self.district_flags
    }

    /// Returns whether park flags are serialized in this version.
    pub fn park_flags_present(&self) -> bool {
        self.park_flags_present
    }

    /// Returns all 128 park flags, zero-filled before their version gate.
    pub fn park_flags(&self) -> &[u32] {
        &self.park_flags
    }

    /// Returns the versioned packed district and park policy bools in wire order.
    ///
    /// Categories are district policies, district effects, district event
    /// policies, park event/policy/effect tables, then park specialization
    /// policies. Each category contains policy indices 0–31 across entries
    /// 0–127 and is included only at its version gate.
    pub fn policy_bools(&self) -> Option<&[bool]> {
        self.policy_bools.as_deref()
    }

    /// Returns the discarded 32-by-128 bool table for versions 43–69.
    ///
    /// This decoder supports version 164 and newer, so this is always `None`.
    pub fn legacy_policy_bools(&self) -> Option<&[bool]> {
        self.legacy_policy_bools.as_deref()
    }

    /// Returns decoded population fields for districts with nonzero flags.
    pub fn district_population_records(&self) -> &[DistrictPopulationRecord] {
        &self.district_population_records
    }

    /// Decodes park types 0–15 in buffer order from the bytes after this prefix.
    ///
    /// Active records are supported for inner versions 110025–118011.
    /// An empty park buffer is supported from inner version 164 onward because
    /// it contains no record bytes. Type-0 slot-0 records at inner versions
    /// 117013 and 118011 use the ten-report layout observed in the available
    /// MP23/MP27 maps; other supported records follow the examined DLL's
    /// thirteen-report layout from version 112020. Other active versions or
    /// park types return an error. Types 5–9 include thirteen versioned
    /// industry resource records. Their fields are decoded while the complete
    /// record bytes remain available for exact unchanged writing. The returned
    /// byte count ends before the grid.
    pub fn decode_park_records(
        &self,
        bytes: &[u8],
    ) -> Result<(Vec<DistrictParkRecord>, usize), CrpError> {
        self.decode_park_records_with_type_limit(bytes, 15)
    }

    /// Decodes only type-0 park records, retaining the previous strict scope.
    ///
    /// Empty park buffers are accepted from inner version 164 onward.
    pub fn decode_type_zero_park_records(
        &self,
        bytes: &[u8],
    ) -> Result<(Vec<DistrictParkRecord>, usize), CrpError> {
        self.decode_park_records_with_type_limit(bytes, 0)
    }

    fn decode_park_records_with_type_limit(
        &self,
        bytes: &[u8],
        maximum_type: u8,
    ) -> Result<(Vec<DistrictParkRecord>, usize), CrpError> {
        let use_ten_reports = matches!(self.format_version, 117_013 | 118_011);
        self.decode_park_records_with_ten_reports(bytes, maximum_type, use_ten_reports)
    }

    pub(crate) fn decode_park_records_with_ten_reports(
        &self,
        bytes: &[u8],
        maximum_type: u8,
        use_ten_reports: bool,
    ) -> Result<(Vec<DistrictParkRecord>, usize), CrpError> {
        if self.park_flags.iter().all(|flags| *flags == 0) {
            return Ok((Vec::new(), 0));
        }
        if !(TYPE_ZERO_PARK_FIRST_VERSION..=TYPE_ZERO_PARK_LAST_VERSION)
            .contains(&self.format_version)
        {
            return Err(invalid_district_manager(
                0,
                format!(
                    "park record layout is unsupported at inner version {}",
                    self.format_version
                ),
            ));
        }

        let mut records = Vec::new();
        let mut offset = 0_usize;
        for (park_index, flags) in self.park_flags.iter().copied().enumerate() {
            if flags == 0 {
                continue;
            }
            let mut reader = Reader::new(bytes.get(offset..).ok_or_else(|| {
                invalid_district_manager(offset, format!("park {park_index} record is truncated"))
            })?);
            let happiness_bytes = park_happiness_bytes(self.format_version);
            let fixed_bytes = park_scalar_bytes(self.format_version);
            let type_offset = TYPE_ZERO_PARK_BASE_TYPE_OFFSET + happiness_bytes;
            let fixed = reader.read_bytes(fixed_bytes).map_err(|error| {
                invalid_district_manager(offset, format!("park {park_index} record: {error}"))
            })?;
            let park_type = fixed[type_offset];
            if park_type > maximum_type {
                return Err(invalid_district_manager(
                    offset + type_offset,
                    format!("park {park_index} has unsupported type {park_type}"),
                ));
            }
            let random_seed = u64::from_be_bytes(fixed[..8].try_into().map_err(|_| {
                invalid_district_manager(offset, "invalid park random seed length")
            })?);
            let main_gate = u16::from_be_bytes([fixed[8], fixed[9]]);
            let condition = u16::from_be_bytes([fixed[10], fixed[11]]);
            let level = fixed[type_offset + 1];
            let visitor_data = decode_park_visitor_data(self.format_version, type_offset, fixed)?;
            let post_level_data =
                decode_park_post_level_data(self.format_version, type_offset, fixed)?;
            let (industry_resources, industry_resource_offsets) = if (5..=9).contains(&park_type)
                && self.format_version >= INDUSTRY_RESOURCE_VERSION
            {
                // DistrictPark.IsIndustryType is true for types 5–9. The
                // industry branch writes thirteen DistrictAreaResourceData
                // records before the campus and airport branches.
                let (resources, resource_offsets) = decode_park_industry_resources(
                    &mut reader,
                    self.format_version,
                    offset,
                    park_index,
                )?;
                (Some(resources), Some(resource_offsets))
            } else {
                (None, None)
            };
            let (campus_data, campus_data_offset, campus_year_reports, campus_sports_data) =
                if self.format_version >= 112_008
                    && ((10..=13).contains(&park_type)
                        || (self.format_version >= TYPE_ZERO_PARK_CAMPUS_VERSION
                            && park_index == 0))
                {
                    // The supplied MP23/MP27 type-0 slot-0 records contain ten
                    // report records in total. The examined DLL reads thirteen.
                    let observed_ten_reports = use_ten_reports && park_index == 0 && park_type == 0;
                    let campus_data_offset = reader.offset();
                    let (campus_data, campus_year_reports, campus_sports_data) =
                        read_campus(&mut reader, self.format_version, observed_ten_reports)
                            .map_err(|error| {
                                invalid_district_manager(
                                    offset + reader.offset(),
                                    format!("park {park_index} campus block: {error}"),
                                )
                            })?;
                    (
                        Some(campus_data),
                        Some(campus_data_offset),
                        Some(campus_year_reports),
                        Some(campus_sports_data),
                    )
                } else {
                    (None, None, None, None)
                };
            if park_type == 14 {
                reader
                    .read_bytes(airport_park_bytes(self.format_version))
                    .map_err(|error| {
                        invalid_district_manager(
                            offset + reader.offset(),
                            format!("park {park_index} airport fields: {error}"),
                        )
                    })?;
            }
            if park_type == 15 {
                skip_pedestrian_zone(
                    &mut reader,
                    self.format_version,
                    fixed[type_offset + 4],
                    fixed[type_offset + 5],
                )
                .map_err(|error| {
                    invalid_district_manager(
                        offset + reader.offset(),
                        format!("park {park_index} pedestrian zone: {error}"),
                    )
                })?;
            }
            let end = offset
                .checked_add(reader.offset())
                .ok_or(CrpError::ArithmeticOverflow(
                    "DistrictManager park record end",
                ))?;
            records.push(DistrictParkRecord {
                park_index,
                park_type,
                random_seed,
                main_gate,
                condition,
                level,
                level_offset: type_offset + 1,
                visitor_data,
                gate_data: post_level_data.gates,
                capacities: post_level_data.capacities,
                day_night_count: post_level_data.day_night_count,
                attractiveness: post_level_data.attractiveness,
                area_factor: post_level_data.area_factor,
                work_storage_deltas: post_level_data.work_storage_deltas,
                production_data: post_level_data.production_data,
                industry_resources,
                campus_data,
                campus_data_offset,
                campus_year_reports,
                campus_sports_data,
                post_level_offsets: post_level_data.offsets,
                industry_resource_offsets,
                serialized: bytes[offset..end].to_vec(),
            });
            offset = end;
        }
        Ok((records, offset))
    }

    /// Writes the original known prefix exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

fn park_happiness_bytes(format_version: u32) -> usize {
    if format_version >= TYPE_ZERO_PARK_HAPPINESS_U32_VERSION {
        16 // Four u32 accumulation counters.
    } else if format_version >= TYPE_ZERO_PARK_HAPPINESS_U16_VERSION {
        4 // Two u16 accumulation counters.
    } else {
        0
    }
}

fn park_scalar_bytes(format_version: u32) -> usize {
    let mut bytes = TYPE_ZERO_PARK_EARLY_BYTES + park_happiness_bytes(format_version);
    if format_version >= TYPE_ZERO_PARK_WORK_STORAGE_VERSION {
        bytes += 4;
    }
    if format_version >= TYPE_ZERO_PARK_WIDE_PRODUCTION_VERSION {
        bytes += 12; // u64 production and two u16 worker counts.
    } else if format_version >= TYPE_ZERO_PARK_PRODUCTION_VERSION {
        bytes += 8; // u32 production and two u16 worker counts.
    }
    bytes
}

fn park_visitor_field_offsets(
    format_version: u32,
    type_offset: usize,
) -> Option<ParkVisitorFieldOffsets> {
    if format_version < 110_008 {
        return None;
    }
    let block_start = type_offset.checked_sub(PARK_VISITOR_BLOCK_BYTES)?;
    let last_visitor_count = (format_version >= 110_014)
        .then(|| block_start.checked_sub(PARK_LAST_VISITOR_COUNT_BYTES))
        .flatten();
    Some(ParkVisitorFieldOffsets {
        total_visitors: block_start,
        temporary_ticket_income: block_start + 4,
        final_ticket_income: block_start + 8,
        ticket_price: block_start + 12,
        temporary_residents: block_start + 14,
        final_residents: block_start + 16,
        temporary_tourists: block_start + 18,
        final_tourists: block_start + 20,
        last_visitor_count,
    })
}

fn park_post_level_field_offsets(
    format_version: u32,
    type_offset: usize,
) -> Result<ParkPostLevelFieldOffsets, CrpError> {
    let mut cursor = type_offset
        .checked_add(2)
        .ok_or(CrpError::ArithmeticOverflow("park type and level end"))?;
    let mut take = |gate: u32, size: usize| -> Result<Option<usize>, CrpError> {
        if format_version < gate {
            return Ok(None);
        }
        let offset = cursor;
        cursor = cursor
            .checked_add(size)
            .ok_or(CrpError::ArithmeticOverflow("park scalar field end"))?;
        Ok(Some(offset))
    };
    let gates = take(110_012, 4)?;
    let capacities = take(110_018, 8)?;
    let day_night_count = take(110_019, 1)?;
    let attractiveness = take(110_020, 4)?;
    let area_factor = take(110_024, 1)?;
    let work_storage_deltas = take(
        TYPE_ZERO_PARK_WORK_STORAGE_VERSION,
        PARK_WORK_STORAGE_DELTAS_BYTES,
    )?;
    let production = if format_version >= TYPE_ZERO_PARK_PRODUCTION_VERSION {
        let total_production_bytes = if format_version >= TYPE_ZERO_PARK_WIDE_PRODUCTION_VERSION {
            PARK_PRODUCTION_U64_BYTES
        } else {
            PARK_PRODUCTION_U32_BYTES
        };
        let total_production = cursor;
        let temporary_workers = total_production
            .checked_add(total_production_bytes)
            .ok_or(CrpError::ArithmeticOverflow("park production field end"))?;
        let final_workers = temporary_workers
            .checked_add(PARK_WORKER_COUNT_BYTES)
            .ok_or(CrpError::ArithmeticOverflow("park worker field end"))?;
        let _production_end = final_workers
            .checked_add(PARK_WORKER_COUNT_BYTES)
            .ok_or(CrpError::ArithmeticOverflow("park worker field end"))?;
        Some(ParkProductionFieldOffsets {
            total_production,
            total_production_bytes,
            temporary_workers,
            final_workers,
        })
    } else {
        None
    };
    Ok(ParkPostLevelFieldOffsets {
        gates,
        capacities,
        day_night_count,
        attractiveness,
        area_factor,
        work_storage_deltas,
        production,
    })
}

fn read_park_u16(bytes: &[u8], offset: usize) -> Result<u16, CrpError> {
    let mut reader = Reader::new(bytes.get(offset..).ok_or_else(|| {
        invalid_district_manager(offset, "park u16 field offset is outside the record")
    })?);
    reader
        .read_u16_be()
        .map_err(|error| invalid_district_manager(offset, format!("park u16 field: {error}")))
}

fn read_park_u8(bytes: &[u8], offset: usize) -> Result<u8, CrpError> {
    let mut reader = Reader::new(bytes.get(offset..).ok_or_else(|| {
        invalid_district_manager(offset, "park scalar field offset is outside the record")
    })?);
    reader
        .read_u8()
        .map_err(|error| invalid_district_manager(offset, format!("park scalar field: {error}")))
}

fn read_park_u32(bytes: &[u8], offset: usize) -> Result<u32, CrpError> {
    let mut reader = Reader::new(bytes.get(offset..).ok_or_else(|| {
        invalid_district_manager(offset, "park u32 field offset is outside the record")
    })?);
    reader
        .read_u32_be()
        .map_err(|error| invalid_district_manager(offset, format!("park u32 field: {error}")))
}

fn read_park_u64(bytes: &[u8], offset: usize) -> Result<u64, CrpError> {
    let mut reader = Reader::new(bytes.get(offset..).ok_or_else(|| {
        invalid_district_manager(offset, "park production field offset is outside the record")
    })?);
    reader.read_u64_be().map_err(|error| {
        invalid_district_manager(offset, format!("park production field: {error}"))
    })
}

fn decode_park_visitor_data(
    format_version: u32,
    type_offset: usize,
    record: &[u8],
) -> Result<Option<DistrictParkVisitorData>, CrpError> {
    let Some(field_offsets) = park_visitor_field_offsets(format_version, type_offset) else {
        return Ok(None);
    };
    let last_visitor_count = field_offsets
        .last_visitor_count
        .map(|offset| read_park_u32(record, offset))
        .transpose()?;
    Ok(Some(DistrictParkVisitorData {
        total_visitors: read_park_u32(record, field_offsets.total_visitors)?,
        temporary_ticket_income: read_park_u32(record, field_offsets.temporary_ticket_income)?,
        final_ticket_income: read_park_u32(record, field_offsets.final_ticket_income)?,
        ticket_price: read_park_u16(record, field_offsets.ticket_price)?,
        temporary_residents: read_park_u16(record, field_offsets.temporary_residents)?,
        final_residents: read_park_u16(record, field_offsets.final_residents)?,
        temporary_tourists: read_park_u16(record, field_offsets.temporary_tourists)?,
        final_tourists: read_park_u16(record, field_offsets.final_tourists)?,
        last_visitor_count,
        field_offsets,
    }))
}

fn decode_park_post_level_data(
    format_version: u32,
    type_offset: usize,
    record: &[u8],
) -> Result<DecodedParkPostLevelData, CrpError> {
    let offsets = park_post_level_field_offsets(format_version, type_offset)?;
    let gates = offsets
        .gates
        .map(|offset| -> Result<DistrictParkGateData, CrpError> {
            Ok(DistrictParkGateData {
                random_gate: read_park_u16(record, offset)?,
                temporary_count: read_park_u8(record, offset + 2)?,
                final_count: read_park_u8(record, offset + 3)?,
            })
        })
        .transpose()?;
    let capacities = offsets
        .capacities
        .map(|offset| -> Result<DistrictParkCapacities, CrpError> {
            Ok(DistrictParkCapacities {
                temporary_visitors: read_park_u16(record, offset)?,
                final_visitors: read_park_u16(record, offset + 2)?,
                temporary_main: read_park_u16(record, offset + 4)?,
                final_main: read_park_u16(record, offset + 6)?,
            })
        })
        .transpose()?;
    let day_night_count = offsets
        .day_night_count
        .map(|offset| read_park_u8(record, offset))
        .transpose()?;
    let attractiveness = offsets
        .attractiveness
        .map(|offset| -> Result<DistrictParkAttractiveness, CrpError> {
            Ok(DistrictParkAttractiveness {
                temporary: read_park_u16(record, offset)?,
                final_value: read_park_u16(record, offset + 2)?,
            })
        })
        .transpose()?;
    let area_factor = offsets
        .area_factor
        .map(|offset| read_park_u8(record, offset))
        .transpose()?;
    let work_storage_deltas = offsets
        .work_storage_deltas
        .map(
            |offset| -> Result<DistrictParkWorkStorageDeltas, CrpError> {
                Ok(DistrictParkWorkStorageDeltas {
                    temporary_work_efficiency: read_park_u8(record, offset)?,
                    temporary_storage: read_park_u8(record, offset + 1)?,
                    final_work_efficiency: read_park_u8(record, offset + 2)?,
                    final_storage: read_park_u8(record, offset + 3)?,
                })
            },
        )
        .transpose()?;
    let production_data = offsets
        .production
        .map(
            |production| -> Result<DistrictParkProductionData, CrpError> {
                let total_production =
                    if production.total_production_bytes == PARK_PRODUCTION_U32_BYTES {
                        u64::from(read_park_u32(record, production.total_production)?)
                    } else {
                        read_park_u64(record, production.total_production)?
                    };
                Ok(DistrictParkProductionData {
                    total_production,
                    temporary_workers: read_park_u16(record, production.temporary_workers)?,
                    final_workers: read_park_u16(record, production.final_workers)?,
                })
            },
        )
        .transpose()?;
    Ok(DecodedParkPostLevelData {
        gates,
        capacities,
        day_night_count,
        attractiveness,
        area_factor,
        work_storage_deltas,
        production_data,
        offsets,
    })
}

fn decode_park_industry_resources(
    reader: &mut Reader<'_>,
    format_version: u32,
    park_offset: usize,
    park_index: usize,
) -> Result<
    (
        Vec<DistrictParkIndustryResource>,
        ParkIndustryResourceOffsets,
    ),
    CrpError,
> {
    let record_bytes = if format_version >= INDUSTRY_RESOURCE_TRADE_VERSION {
        INDUSTRY_RESOURCE_EXPANDED_BYTES
    } else {
        INDUSTRY_RESOURCE_BASE_BYTES
    };
    let block_bytes =
        INDUSTRY_RESOURCE_RECORDS
            .checked_mul(record_bytes)
            .ok_or(CrpError::ArithmeticOverflow(
                "park industry resource block size",
            ))?;
    let first_record = reader.offset();
    let block = match reader.read_bytes(block_bytes) {
        Ok(block) => block,
        Err(error) => {
            let error_offset = park_offset
                .checked_add(reader.offset())
                .ok_or(CrpError::ArithmeticOverflow("park industry error offset"))?;
            return Err(invalid_district_manager(
                error_offset,
                format!("park {park_index} industry resources: {error}"),
            ));
        }
    };
    let mut resource_reader = Reader::new(block);
    let mut resources = Vec::with_capacity(INDUSTRY_RESOURCE_RECORDS);
    for kind in INDUSTRY_RESOURCE_KINDS {
        let mut read_field = |field: &str| -> Result<u32, CrpError> {
            let field_offset = park_offset
                .checked_add(first_record)
                .and_then(|offset| offset.checked_add(resource_reader.offset()))
                .ok_or(CrpError::ArithmeticOverflow("park industry field offset"))?;
            resource_reader.read_u32_be().map_err(|error| {
                invalid_district_manager(
                    field_offset,
                    format!("park {park_index} industry {kind:?} {field}: {error}"),
                )
            })
        };
        let temporary_consumption = read_field("temporary consumption")?;
        let temporary_production = read_field("temporary production")?;
        let temporary_buffer_amount = read_field("temporary buffer amount")?;
        let temporary_incoming_transfer = read_field("temporary incoming transfer")?;
        let temporary_buffer_capacity = read_field("temporary buffer capacity")?;
        let final_consumption = read_field("final consumption")?;
        let final_production = read_field("final production")?;
        let final_buffer_amount = read_field("final buffer amount")?;
        let final_incoming_transfer = read_field("final incoming transfer")?;
        let final_buffer_capacity = read_field("final buffer capacity")?;
        let trade_data = if format_version >= INDUSTRY_RESOURCE_TRADE_VERSION {
            Some(DistrictParkIndustryTradeData {
                temporary_import: read_field("temporary import")?,
                temporary_export: read_field("temporary export")?,
                final_import: read_field("final import")?,
                final_export: read_field("final export")?,
            })
        } else {
            None
        };
        resources.push(DistrictParkIndustryResource {
            kind,
            temporary_consumption,
            temporary_production,
            temporary_buffer_amount,
            temporary_incoming_transfer,
            temporary_buffer_capacity,
            final_consumption,
            final_production,
            final_buffer_amount,
            final_incoming_transfer,
            final_buffer_capacity,
            trade_data,
        });
    }
    Ok((
        resources,
        ParkIndustryResourceOffsets {
            first_record,
            record_bytes,
        },
    ))
}

fn encode_industry_resource(
    resource: &DistrictParkIndustryResource,
    has_trade_data: bool,
) -> Result<Vec<u8>, CrpError> {
    if has_trade_data != resource.trade_data.is_some() {
        return Err(invalid_district_manager(
            0,
            if has_trade_data {
                "industry trade counters are required from inner version 111010"
            } else {
                "industry trade counters are absent before inner version 111010"
            },
        ));
    }
    let mut bytes = Vec::with_capacity(if has_trade_data {
        INDUSTRY_RESOURCE_EXPANDED_BYTES
    } else {
        INDUSTRY_RESOURCE_BASE_BYTES
    });
    for value in [
        resource.temporary_consumption,
        resource.temporary_production,
        resource.temporary_buffer_amount,
        resource.temporary_incoming_transfer,
        resource.temporary_buffer_capacity,
        resource.final_consumption,
        resource.final_production,
        resource.final_buffer_amount,
        resource.final_incoming_transfer,
        resource.final_buffer_capacity,
    ] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    if let Some(trade_data) = resource.trade_data {
        for value in [
            trade_data.temporary_import,
            trade_data.temporary_export,
            trade_data.final_import,
            trade_data.final_export,
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    }
    let expected_bytes = if has_trade_data {
        INDUSTRY_RESOURCE_EXPANDED_BYTES
    } else {
        INDUSTRY_RESOURCE_BASE_BYTES
    };
    if bytes.len() != expected_bytes {
        return Err(invalid_district_manager(
            0,
            "encoded industry resource has an unexpected byte length",
        ));
    }
    Ok(bytes)
}

fn airport_park_bytes(format_version: u32) -> usize {
    usize::from(format_version >= 114_000) * 4
        + usize::from(format_version >= 114_004) * 8
        + usize::from(format_version >= 114_008) * 3
        + usize::from(format_version >= 114_010) * 8
        + usize::from(format_version >= 114_011) * 8
}

fn read_district_age_population(
    reader: &mut Reader<'_>,
    category: DistrictAgeCategory,
) -> Result<DistrictAgePopulation, CrpError> {
    Ok(DistrictAgePopulation {
        category,
        temporary: reader.read_u24_be()?,
        final_count: reader.read_u24_be()?,
    })
}

fn read_campus(
    reader: &mut Reader<'_>,
    format_version: u32,
    observed_ten_reports: bool,
) -> Result<
    (
        DistrictCampusData,
        Vec<DistrictCampusYearReportData>,
        DistrictCampusSportsData,
    ),
    CrpError,
> {
    // DistrictManager+Data.Deserialize reads this for campus types from 112008
    // and for slot 0 of any type from 114005.
    let student_count = reader.read_u16_be()?;
    let student_capacity = reader.read_u16_be()?;
    let campus_building_attractiveness = reader.read_u16_be()?;
    let mut academic_works = [0_i32; 4];
    for value in &mut academic_works {
        *value = reader.read_i32_be()?;
    }
    let report_count =
        2 + usize::from(format_version >= 112_009) + if format_version >= 112_020 { 10 } else { 0 };
    let report_count = if observed_ten_reports {
        10
    } else {
        report_count
    };
    let mut reports = Vec::with_capacity(report_count);
    for _ in 0..report_count {
        reports.push(read_year_report(reader, format_version)?);
    }
    let mut sports_field_offsets = CampusSportsFieldOffsets::default();
    sports_field_offsets.is_main_campus = Some(reader.offset());
    let is_main_campus = reader.read_u8()? == 1;
    sports_field_offsets.grant_type = Some(reader.offset());
    let grant_type = reader.read_u8()?;
    sports_field_offsets.coach_count = Some(reader.offset());
    let coach_count = reader.read_u8()?;
    sports_field_offsets.cheerleading_budget = Some(reader.offset());
    let cheerleading_budget = reader.read_i16_be()?;
    let (academic_staff_count, academic_staff_accumulation_bits) = if format_version >= 112_025 {
        sports_field_offsets.academic_staff_count = Some(reader.offset());
        let staff_count = reader.read_u8()?;
        sports_field_offsets.academic_staff_accumulation = Some(reader.offset());
        let accumulation = reader.read_f32_be()?.to_bits();
        (Some(staff_count), Some(accumulation))
    } else {
        (None, None)
    };
    // EncodedArray.Long has the ULong control layout and 64-bit wire width.
    let coach_hire_time_start = reader.offset();
    let coach_hire_times = read_encoded_ulong_array(reader, 25)?;
    sports_field_offsets.coach_hire_time_array = Some(SerializedByteRange {
        start: coach_hire_time_start,
        end: reader.offset(),
    });
    let mut coach_hire_time_ticks = [0_i64; 25];
    for (target, value) in coach_hire_time_ticks.iter_mut().zip(coach_hire_times) {
        *target = value as i64;
    }
    sports_field_offsets.win_probability = Some(reader.offset());
    let win_probability = reader.read_u8()?;
    sports_field_offsets.varsity_identity_index = Some(reader.offset());
    let varsity_identity_index = reader.read_i16_be()?;
    let varsity_color_start = reader.offset();
    let varsity_color: [u8; 3] = read_encoded_byte_array(reader, 3)?
        .try_into()
        .map_err(|_| reader.invalid("encoded varsity color did not contain three values"))?;
    sports_field_offsets.varsity_color_array = Some(SerializedByteRange {
        start: varsity_color_start,
        end: reader.offset(),
    });
    let lifetime_trophies = if format_version >= 112_009 {
        sports_field_offsets.lifetime_trophies = Some(reader.offset());
        Some(reader.read_i16_be()?)
    } else {
        None
    };
    let static_varsity_attractiveness_modifier = if format_version >= 112_011 {
        sports_field_offsets.static_varsity_attractiveness_modifier = Some(reader.offset());
        Some(reader.read_i16_be()?)
    } else {
        None
    };
    let dynamic_varsity_attractiveness_modifier = if format_version >= 112_012 {
        sports_field_offsets.dynamic_varsity_attractiveness_modifier = Some(reader.offset());
        Some(reader.read_i16_be()?)
    } else {
        None
    };
    let sports = if format_version >= 112_013 {
        sports_field_offsets.sports = Some(reader.offset());
        Some(reader.read_i16_be()?)
    } else {
        None
    };
    let away_matches_done = if format_version >= 112_014 {
        let away_matches_start = reader.offset();
        let values = read_encoded_bool_array(reader, 5)?
            .try_into()
            .map_err(|_| reader.invalid("encoded away-match array did not contain five values"))?;
        sports_field_offsets.away_matches_array = Some(SerializedByteRange {
            start: away_matches_start,
            end: reader.offset(),
        });
        Some(values)
    } else {
        None
    };
    let campus_building_count = if format_version >= 112_024 {
        sports_field_offsets.campus_building_count = Some(reader.offset());
        Some(reader.read_i16_be()?)
    } else {
        None
    };
    let sports_data = DistrictCampusSportsData {
        is_main_campus,
        grant_type,
        coach_count,
        cheerleading_budget,
        academic_staff_count,
        academic_staff_accumulation_bits,
        coach_hire_time_ticks,
        win_probability,
        varsity_identity_index,
        varsity_color,
        lifetime_trophies,
        static_varsity_attractiveness_modifier,
        dynamic_varsity_attractiveness_modifier,
        sports,
        away_matches_done,
        campus_building_count,
        field_offsets: sports_field_offsets,
    };
    Ok((
        DistrictCampusData {
            student_count,
            student_capacity,
            campus_building_attractiveness,
            academic_works,
        },
        reports,
        sports_data,
    ))
}

fn skip_pedestrian_zone(
    reader: &mut Reader<'_>,
    format_version: u32,
    temp_gate_count: u8,
    final_gate_count: u8,
) -> Result<(), CrpError> {
    let mut group_count = 0_usize;
    if format_version >= 115_003 {
        let count = reader.read_i32_be()?;
        group_count = usize::try_from(count).map_err(|_| {
            reader.invalid(format!("negative pedestrian material group count {count}"))
        })?;
        // Both tables need at least one reason byte and one i32 per group.
        if group_count > reader.remaining_bytes().len() / 10 {
            return Err(reader.invalid("pedestrian material group count exceeds remaining bytes"));
        }
        for _ in 0..2 {
            for _ in 0..group_count {
                reader.read_u8()?; // Reason, retained in the raw record.
                let count = reader.read_i32_be()?;
                let item_count = usize::try_from(count).map_err(|_| {
                    reader.invalid(format!("negative pedestrian material item count {count}"))
                })?;
                skip_encoded_ushort_array(reader, item_count)?;
            }
        }
    }
    if format_version >= 115_013 {
        skip_encoded_ushort_array(reader, usize::from(final_gate_count))?;
        skip_encoded_ushort_array(reader, usize::from(temp_gate_count))?;
        let count = reader.read_u8()? as i8;
        let count = usize::try_from(count)
            .map_err(|_| reader.invalid(format!("negative random service-point count {count}")))?;
        reader.read_bytes(count * 10)?; // i64 vehicle category and u16 amount.
    }
    if format_version >= 115_007 {
        let bytes_per_private = if format_version >= 115_015 { 46 } else { 42 };
        reader.read_bytes(4 * bytes_per_private)?;
    }
    if format_version >= 115_015 {
        reader.read_bytes(2)?; // Zone focus and dirty flag.
    }
    if format_version >= 115_016 {
        if format_version < 115_021 {
            reader.read_bytes(16)?; // Discarded legacy goods counters.
        }
        reader.read_bytes(8)?; // Temporary and final goods sold.
    }
    if format_version >= 115_021 {
        // IL 0x2DC8 compares against the earlier 115003 group count, not 22.
        let byte_count = group_count
            .checked_mul(17)
            .ok_or(CrpError::ArithmeticOverflow(
                "pedestrian reason record bytes",
            ))?;
        reader.read_bytes(byte_count)?;
    }
    if format_version >= 115_023 {
        reader.read_bytes(2)?; // Cargo and garbage service-point flags.
    }
    Ok(())
}

fn read_year_report(
    reader: &mut Reader<'_>,
    format_version: u32,
) -> Result<DistrictCampusYearReportData, CrpError> {
    let mut field_offsets = CampusYearReportFieldOffsets::default();
    let mut student_count = None;
    let mut campus_attractiveness = None;
    let mut reputation_level = None;
    let mut grant_type = None;
    let mut grant_work_index = None;
    let mut academic_works = None;
    let mut total_prize_money = None;
    let mut tickets_income = None;
    let mut winnings_income = None;
    let mut matches_won = None;
    let mut matches_lost = None;
    let mut matches_cancelled = None;
    let mut trophies = None;
    if format_version >= 112_014 {
        field_offsets.student_count = Some(reader.offset());
        student_count = Some(reader.read_i32_be()?);
        field_offsets.campus_attractiveness = Some(reader.offset());
        campus_attractiveness = Some(reader.read_i32_be()?);
        field_offsets.reputation_level = Some(reader.offset());
        reputation_level = Some(reader.read_u8()? as i8);
        field_offsets.grant_type = Some(reader.offset());
        grant_type = Some(reader.read_u8()? as i8);
        field_offsets.grant_work_index = Some(reader.offset());
        grant_work_index = Some(reader.read_i16_be()?);
        field_offsets.academic_works = Some(reader.offset());
        let mut works = [0_i32; 4];
        for value in &mut works {
            *value = reader.read_i32_be()?;
        }
        academic_works = Some(works);
        field_offsets.total_prize_money = Some(reader.offset());
        total_prize_money = Some(reader.read_u64_be()?);
        tickets_income = Some(read_campus_report_ulong_array(reader, "ticket income")?);
        winnings_income = Some(read_campus_report_ulong_array(reader, "winnings income")?);
        matches_won = Some(read_campus_report_byte_array(reader, "matches won")?);
        matches_lost = Some(read_campus_report_byte_array(reader, "matches lost")?);
        matches_cancelled = Some(read_campus_report_byte_array(reader, "matches cancelled")?);
        trophies = Some(read_campus_report_bool_array(reader)?);
    }
    let mut toga_party_count = None;
    let mut toga_party_seeds = None;
    if format_version >= 112_017 {
        field_offsets.toga_party_count = Some(reader.offset());
        toga_party_count = Some(reader.read_u8()? as i8);
        field_offsets.toga_party_seeds = Some(reader.offset());
        let mut seeds = [0_i64; 3];
        for value in &mut seeds {
            *value = reader.read_u64_be()? as i64;
        }
        toga_party_seeds = Some(seeds);
    }
    let milestone_field_present = format_version >= 112_023;
    let milestone_unlocked = if milestone_field_present {
        reader.read_unique_string()?
    } else {
        None
    };
    let total_works = if format_version >= 112_026 {
        field_offsets.total_works = Some(reader.offset());
        Some(reader.read_i16_be()?)
    } else {
        None
    };
    Ok(DistrictCampusYearReportData {
        student_count,
        campus_attractiveness,
        reputation_level,
        grant_type,
        grant_work_index,
        academic_works,
        total_prize_money,
        tickets_income,
        winnings_income,
        matches_won,
        matches_lost,
        matches_cancelled,
        trophies,
        toga_party_count,
        toga_party_seeds,
        milestone_field_present,
        milestone_unlocked,
        total_works,
        field_offsets,
    })
}

fn read_campus_report_ulong_array(
    reader: &mut Reader<'_>,
    field: &str,
) -> Result<[u64; 5], CrpError> {
    read_encoded_ulong_array(reader, 5)?
        .try_into()
        .map_err(|_| {
            reader.invalid(format!(
                "encoded campus report {field} did not contain five values"
            ))
        })
}

fn read_campus_report_byte_array(
    reader: &mut Reader<'_>,
    field: &str,
) -> Result<[u8; 5], CrpError> {
    read_encoded_byte_array(reader, 5)?.try_into().map_err(|_| {
        reader.invalid(format!(
            "encoded campus report {field} did not contain five values"
        ))
    })
}

fn read_campus_report_bool_array(reader: &mut Reader<'_>) -> Result<[bool; 5], CrpError> {
    read_encoded_bool_array(reader, 5)?
        .try_into()
        .map_err(|_| reader.invalid("encoded campus report trophies did not contain five values"))
}

fn read_district_private_data(
    reader: &mut Reader<'_>,
    format_version: u32,
    kind: DistrictPrivateDataKind,
) -> Result<DistrictPrivateDataBlock, CrpError> {
    let bytes = reader
        .read_bytes(district_private_data_size(format_version))?
        .to_vec();
    Ok(DistrictPrivateDataBlock { kind, bytes })
}

fn read_district_specialization(
    reader: &mut Reader<'_>,
    kind: DistrictSpecializationKind,
) -> Result<DistrictSpecializationBlock, CrpError> {
    const SPECIALIZATION_DATA_BYTES: usize = 8 * 3;
    let bytes = reader.read_bytes(SPECIALIZATION_DATA_BYTES)?.to_vec();
    Ok(DistrictSpecializationBlock { kind, bytes })
}

fn read_district_demographic(
    reader: &mut Reader<'_>,
    kind: DistrictDemographicKind,
    byte_count: usize,
) -> Result<DistrictDemographicBlock, CrpError> {
    let bytes = reader.read_bytes(byte_count)?.to_vec();
    Ok(DistrictDemographicBlock { kind, bytes })
}

fn read_district_statistics(
    reader: &mut Reader<'_>,
    kind: DistrictStatisticsKind,
    byte_count: usize,
) -> Result<DistrictStatisticsBlock, CrpError> {
    let bytes = reader.read_bytes(byte_count)?.to_vec();
    Ok(DistrictStatisticsBlock { kind, bytes })
}

fn read_district_tourism(
    reader: &mut Reader<'_>,
    kind: DistrictTourismKind,
    byte_count: usize,
) -> Result<DistrictTourismBlock, CrpError> {
    let bytes = reader.read_bytes(byte_count)?.to_vec();
    Ok(DistrictTourismBlock { kind, bytes })
}

fn read_district_subculture(
    reader: &mut Reader<'_>,
    kind: DistrictSubCultureKind,
    byte_count: usize,
) -> Result<DistrictSubCultureBlock, CrpError> {
    let bytes = reader.read_bytes(byte_count)?.to_vec();
    Ok(DistrictSubCultureBlock { kind, bytes })
}

fn district_production_data_size(format_version: u32) -> usize {
    let mut size = DISTRICT_PRODUCTION_BASE_BYTES;
    for (gate, added_bytes) in [
        (215, 12),
        (216, 8),
        (219, 4),
        (226, 16),
        (228, 6),
        (240, 8),
        (307, 28),
        (109_010, 12),
        (112_006, 6),
        (116_008, 24),
        (116_011, 90),
    ] {
        if format_version >= gate {
            size += added_bytes;
        }
    }
    size
}

fn district_consumption_data_size(format_version: u32) -> usize {
    DISTRICT_CONSUMPTION_BASE_BYTES
        + if format_version >= DISTRICT_CONSUMPTION_EXPANSION_VERSION {
            6
        } else {
            0
        }
}

fn district_resource_data_size(format_version: u32) -> usize {
    DISTRICT_RESOURCE_BASE_BYTES
        + if format_version >= DISTRICT_RESOURCE_ALIVE_VERSION {
            12
        } else {
            0
        }
        + if format_version >= DISTRICT_RESOURCE_FIELDS_VERSION {
            12
        } else {
            0
        }
}

fn district_tourist_data_size(format_version: u32) -> usize {
    DISTRICT_TOURIST_BASE_BYTES
        + if format_version >= DISTRICT_TOURIST_AVERAGE_VERSION {
            3
        } else {
            0
        }
        + if format_version >= DISTRICT_TOURIST_COUNT_VERSION {
            6
        } else {
            0
        }
}

fn district_hotel_data_size(format_version: u32) -> usize {
    DISTRICT_HOTEL_BASE_BYTES
        + if format_version >= DISTRICT_HOTEL_COLOR_VERSION {
            4
        } else {
            0
        }
        + if format_version >= DISTRICT_HOTEL_POPULARITY_VERSION {
            9
        } else {
            0
        }
}

fn district_private_data_size(format_version: u32) -> usize {
    let mut size = PRIVATE_DATA_BASE_BYTES;
    if format_version >= DISTRICT_PRIVATE_LEVELS_VERSION {
        size += PRIVATE_DATA_LEVEL_BYTES;
    }
    if format_version >= DISTRICT_PRIVATE_HEALTH_VERSION {
        size += PRIVATE_DATA_HEALTH_BYTES;
    }
    if format_version >= DISTRICT_PRIVATE_BUILDING_COUNTS_VERSION {
        size += PRIVATE_DATA_BUILDING_BYTES;
    }
    if format_version >= DISTRICT_PRIVATE_ALIVE_COUNT_VERSION {
        size += PRIVATE_DATA_ALIVE_BYTES;
    }
    if format_version >= DISTRICT_PRIVATE_CRIME_RATE_VERSION {
        size += PRIVATE_DATA_CRIME_BYTES;
    }
    if format_version >= DISTRICT_PRIVATE_COLLAPSED_COUNT_VERSION {
        size += PRIVATE_DATA_COLLAPSED_BYTES;
    }
    size
}

fn district_policy_bool_count(format_version: u32) -> Result<usize, CrpError> {
    if format_version < POLICY_FLAGS_VERSION {
        return Ok(0);
    }
    let mut categories = 5_usize;
    if format_version >= POLICY_EFFECTS_VERSION {
        categories += 5;
    }
    if format_version >= DISTRICT_EVENT_POLICIES_VERSION {
        categories += 2;
    }
    if format_version >= PARK_POLICIES_VERSION {
        categories += 2;
    }
    if format_version >= PARK_EVENT_POLICIES_VERSION {
        categories += 1;
    }
    if format_version >= PARK_SPECIALIZATION_POLICIES_VERSION {
        categories += 1;
    }
    categories
        .checked_mul(POLICY_COUNT)
        .and_then(|count| count.checked_mul(DISTRICT_COUNT))
        .ok_or(CrpError::ArithmeticOverflow(
            "DistrictManager policy bool count",
        ))
}

fn invalid_district_manager(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "DistrictManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AIRLINE_NAME_VERSION, AIRPORT_PANEL_VERSION, DISTRICT_COUNT, DISTRICT_GRID_CELLS,
        DISTRICT_GRID_VALUES, DistrictCampusData, DistrictManagerFlagsPrefix,
        DistrictManagerGridData, DistrictManagerTailData, DistrictParkAttractiveness,
        DistrictParkCapacities, DistrictParkGateData, DistrictParkIndustryResource,
        DistrictParkIndustryResourceKind, DistrictParkIndustryTradeData,
        DistrictParkProductionData, DistrictParkWorkStorageDeltas, PARK_FLAGS_VERSION,
        POLICY_COUNT, district_policy_bool_count,
    };
    use crate::data_serializer::{Reader, write_unique_string};
    use crate::encoded_array::{
        read_encoded_byte_array, write_encoded_bool_array, write_encoded_uint_array,
    };

    fn zero_flags(output: &mut Vec<u8>) {
        write_encoded_uint_array(output, &[0; 128]);
    }

    fn zero_policy_bools(output: &mut Vec<u8>, version: u32) {
        let count = district_policy_bool_count(version).unwrap();
        if count != 0 {
            write_encoded_bool_array(output, &vec![false; count]);
        }
    }

    fn zero_campus_with_reports(version: u32, report_count: usize) -> Vec<u8> {
        let mut bytes = vec![0; 22]; // Student fields and academic works.
        for _ in 0..report_count {
            if version >= 112_014 {
                bytes.extend_from_slice(&[0; 36]);
                bytes.extend_from_slice(&[0x84; 2]); // Two five-value ULong arrays.
                bytes.extend_from_slice(&[0x84; 3]); // Three five-value Byte arrays.
                bytes.push(0x80); // Five false bools in one packed byte.
            }
            if version >= 112_017 {
                bytes.extend_from_slice(&[0; 25]);
            }
            if version >= 112_023 {
                write_unique_string(&mut bytes, "").unwrap();
            }
            if version >= 112_026 {
                bytes.extend_from_slice(&[0; 2]);
            }
        }
        bytes.extend_from_slice(&[0; 5]);
        if version >= 112_025 {
            bytes.extend_from_slice(&[0; 5]);
        }
        bytes.push(0x98); // 25 zero Long values.
        bytes.extend_from_slice(&[0; 3]);
        bytes.push(0x82); // Three zero color bytes.
        for gate in [112_009, 112_011, 112_012, 112_013] {
            if version >= gate {
                bytes.extend_from_slice(&[0; 2]);
            }
        }
        if version >= 112_014 {
            bytes.push(0x80); // Five false bools.
        }
        if version >= 112_024 {
            bytes.extend_from_slice(&[0; 2]);
        }
        bytes
    }

    fn zero_campus(version: u32) -> Vec<u8> {
        let report_count =
            2 + usize::from(version >= 112_009) + if version >= 112_020 { 10 } else { 0 };
        zero_campus_with_reports(version, report_count)
    }

    fn pedestrian_body(version: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        if version >= 115_003 {
            bytes.extend_from_slice(&1_i32.to_be_bytes());
            for _ in 0..2 {
                bytes.push(250); // Unknown reason must still be consumed.
                bytes.extend_from_slice(&2_i32.to_be_bytes());
                bytes.extend_from_slice(&[0x01, 0x00, 0x11, 0x00, 0x22]);
            }
        }
        if version >= 115_013 {
            bytes.extend_from_slice(&[0x00, 0x00, 0x11]); // One final gate value.
            bytes.extend_from_slice(&[0x01, 0x00, 0x22, 0x00, 0x33]); // Two temporary.
            bytes.push(1);
            bytes.extend_from_slice(&[0x5a; 10]);
        }
        if version >= 115_007 {
            bytes.extend(vec![0x5a; 4 * if version >= 115_015 { 46 } else { 42 }]);
        }
        if version >= 115_015 {
            bytes.extend_from_slice(&[0, 1]);
        }
        if version >= 115_016 {
            if version < 115_021 {
                bytes.extend_from_slice(&[0x5a; 16]);
            }
            bytes.extend_from_slice(&[0x5a; 8]);
        }
        if version >= 115_021 {
            bytes.extend_from_slice(&[0x5a; 17]); // One reason record, not 22.
        }
        if version >= 115_023 {
            bytes.extend_from_slice(&[0, 1]);
        }
        bytes
    }

    #[test]
    fn district_and_park_flags_follow_version_gate() {
        for (version, park_flags_present) in [(110_000, false), (110_001, true)] {
            let mut bytes = Vec::new();
            zero_flags(&mut bytes);
            if park_flags_present {
                zero_flags(&mut bytes);
            }
            zero_policy_bools(&mut bytes, version);
            let (prefix, used) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &bytes).unwrap();
            assert_eq!(used, bytes.len());
            assert_eq!(prefix.district_flags(), &[0; 128]);
            assert_eq!(prefix.park_flags_present(), park_flags_present);
            assert_eq!(prefix.park_flags(), &[0; 128]);
            assert_eq!(prefix.encode(), bytes);
        }
    }

    #[test]
    fn airline_fields_follow_their_independent_version_gates() {
        for (version, name_present, panel_flag) in [
            (114_005, false, None),
            (114_006, true, None),
            (114_007, true, Some(true)),
        ] {
            let mut bytes = Vec::new();
            if name_present {
                write_unique_string(&mut bytes, "SkyLink").unwrap();
            }
            if let Some(value) = panel_flag {
                bytes.push(u8::from(value));
            }
            zero_flags(&mut bytes);
            write_encoded_uint_array(&mut bytes, &[0; 128]);
            zero_policy_bools(&mut bytes, version);

            let (prefix, used) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &bytes).unwrap();
            assert_eq!(used, bytes.len());
            assert_eq!(prefix.airline_name_present(), name_present);
            assert_eq!(prefix.airline_name(), name_present.then_some("SkyLink"));
            assert_eq!(prefix.airport_panel_opened(), panel_flag);
            assert_eq!(prefix.encode(), bytes);
        }
    }

    #[test]
    fn district_prefix_rejects_unsupported_and_truncated_data() {
        assert!(DistrictManagerFlagsPrefix::decode_prefix(163, &[]).is_err());
        assert!(DistrictManagerFlagsPrefix::decode_prefix(194, &[0; 8]).is_err());
    }

    #[test]
    fn policy_bool_count_follows_district_and_park_version_gates() {
        for (version, categories) in [
            (69, 0),
            (70, 5),
            (171, 10),
            (248, 12),
            (110_009, 14),
            (112_015, 15),
            (115_012, 16),
        ] {
            assert_eq!(
                district_policy_bool_count(version).unwrap(),
                categories * POLICY_COUNT * 128
            );
        }
    }

    #[test]
    fn policy_bool_stream_preserves_field_major_bit_order() {
        let version = 171;
        let mut bytes = Vec::new();
        zero_flags(&mut bytes);
        let mut policies = vec![false; district_policy_bool_count(version).unwrap()];
        let selected = POLICY_COUNT * 128 + 3 * 128 + 5;
        policies[selected] = true;
        write_encoded_bool_array(&mut bytes, &policies);

        let (prefix, used) = DistrictManagerFlagsPrefix::decode_prefix(version, &bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(prefix.policy_bools().unwrap(), policies);
        assert_eq!(prefix.legacy_policy_bools(), None);
        assert_eq!(prefix.encode(), bytes);
    }

    #[test]
    fn game_produced_prefix_stops_before_park_record() {
        let bytes =
            include_bytes!("../tests/fixtures/district-manager-prefix-v112004-wolf-creek.bin");
        let (prefix, used) = DistrictManagerFlagsPrefix::decode_prefix(112_004, bytes).unwrap();

        // The game IL consumes 78 bytes for this version's non-industry park
        // record, followed by the first district-grid byte plane.
        assert_eq!(used, 1809);
        assert_eq!(prefix.park_flags()[0], 1);
        assert_eq!(prefix.legacy_policy_bools(), None);
        assert_eq!(bytes[used + 42], 0); // Park type.
        let mut grid = crate::data_serializer::Reader::new(&bytes[used + 78..]);
        assert_eq!(
            read_encoded_byte_array(&mut grid, 262_144).unwrap(),
            vec![0; 262_144]
        );
        assert_eq!(grid.offset(), 2048);
        assert_eq!(prefix.encode(), bytes[..used]);

        let (parks, park_bytes) = prefix
            .decode_type_zero_park_records(&bytes[used..])
            .unwrap();
        assert_eq!(park_bytes, 78);
        assert_eq!(parks.len(), 1);
        assert_eq!(parks[0].park_index(), 0);
        assert_eq!(parks[0].random_seed(), 0);
        assert_eq!(parks[0].main_gate(), 0);
        assert_eq!(parks[0].condition(), 0);
        assert_eq!(parks[0].level(), 0);
        assert_eq!(parks[0].encode(), bytes[used..used + park_bytes]);
    }

    #[test]
    fn type_zero_park_records_use_big_endian_fields_and_flag_order() {
        let mut prefix_bytes = Vec::new();
        zero_flags(&mut prefix_bytes);
        let mut park_flags = [0_u32; DISTRICT_COUNT];
        park_flags[0] = 1;
        park_flags[5] = 1;
        write_encoded_uint_array(&mut prefix_bytes, &park_flags);
        zero_policy_bools(&mut prefix_bytes, 112_004);
        let (prefix, used) =
            DistrictManagerFlagsPrefix::decode_prefix(112_004, &prefix_bytes).unwrap();
        assert_eq!(used, prefix_bytes.len());

        let mut first = [0_u8; 78];
        first[..12].copy_from_slice(&[
            1, 2, 3, 4, 5, 6, 7, 8, // Random seed.
            0x12, 0x34, // Main gate.
            0xab, 0xcd, // Condition.
        ]);
        first[43] = 4;
        first[77] = 0x5a;
        let mut second = [0_u8; 78];
        second[43] = 2;
        let bytes = [first.as_slice(), second.as_slice(), &[0xff]].concat();
        let (records, consumed) = prefix.decode_type_zero_park_records(&bytes).unwrap();
        assert_eq!(consumed, 156);
        assert_eq!(
            records
                .iter()
                .map(|record| record.park_index())
                .collect::<Vec<_>>(),
            [0, 5]
        );
        assert_eq!(records[0].random_seed(), 0x0102_0304_0506_0708);
        assert_eq!(records[0].main_gate(), 0x1234);
        assert_eq!(records[0].condition(), 0xabcd);
        assert_eq!(records[0].level(), 4);
        assert_eq!(records[1].level(), 2);
        assert_eq!(records[0].encode(), first);
        assert_eq!(records[1].encode(), second);
        assert_eq!(bytes[consumed], 0xff);

        assert!(prefix.decode_type_zero_park_records(&bytes[..155]).is_err());
        let mut unknown_type = bytes.clone();
        unknown_type[42] = 15;
        assert!(prefix.decode_type_zero_park_records(&unknown_type).is_err());
    }

    #[test]
    fn park_types_one_through_nine_follow_il_resource_branch() {
        for (version, park_type, resource_bytes) in [
            (110_025, 4, 0),
            (110_025, 5, 0),
            (111_009, 5, 13 * 40),
            (111_010, 9, 13 * 56),
            (112_004, 1, 0),
        ] {
            let mut prefix_bytes = Vec::new();
            zero_flags(&mut prefix_bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[5] = 1;
            write_encoded_uint_array(&mut prefix_bytes, &park_flags);
            zero_policy_bools(&mut prefix_bytes, version);
            let (prefix, _) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();
            let scalar_bytes = super::park_scalar_bytes(version);
            let mut record = vec![0; scalar_bytes + resource_bytes];
            record[42] = park_type;
            record[43] = 3;
            if resource_bytes != 0 {
                record[scalar_bytes] = 0x12;
                record[scalar_bytes + resource_bytes - 1] = 0x34;
            }
            let mut bytes = record.clone();
            bytes.push(0xff); // First district-grid control after the park.
            let (parks, used) = prefix.decode_park_records(&bytes).unwrap();
            assert_eq!(used, record.len());
            assert_eq!(parks.len(), 1);
            assert_eq!(parks[0].park_index(), 5);
            assert_eq!(parks[0].park_type(), park_type);
            assert_eq!(parks[0].level(), 3);
            assert_eq!(parks[0].industry_resources().is_some(), resource_bytes != 0);
            if version < super::INDUSTRY_RESOURCE_VERSION && park_type == 5 {
                let mut editor = parks[0].clone().into_editor();
                assert!(
                    editor
                        .set_industry_resource(DistrictParkIndustryResource {
                            kind: DistrictParkIndustryResourceKind::Grain,
                            temporary_consumption: 0,
                            temporary_production: 0,
                            temporary_buffer_amount: 0,
                            temporary_incoming_transfer: 0,
                            temporary_buffer_capacity: 0,
                            final_consumption: 0,
                            final_production: 0,
                            final_buffer_amount: 0,
                            final_incoming_transfer: 0,
                            final_buffer_capacity: 0,
                            trade_data: None,
                        })
                        .is_err()
                );
            }
            if let Some(resources) = parks[0].industry_resources() {
                assert_eq!(resources.len(), super::INDUSTRY_RESOURCE_RECORDS);
            }
            assert_eq!(parks[0].encode(), record);
            assert_eq!(bytes[used], 0xff);
            assert!(prefix.decode_type_zero_park_records(&bytes).is_err());
            if resource_bytes != 0 {
                assert!(
                    prefix
                        .decode_park_records(&record[..record.len() - 1])
                        .is_err()
                );
            }
            record[42] = 16;
            assert!(prefix.decode_park_records(&record).is_err());
        }
    }

    #[test]
    fn industry_resource_records_decode_and_edit_each_versioned_counter() {
        let kinds = [
            DistrictParkIndustryResourceKind::Grain,
            DistrictParkIndustryResourceKind::Logs,
            DistrictParkIndustryResourceKind::Ore,
            DistrictParkIndustryResourceKind::Oil,
            DistrictParkIndustryResourceKind::AnimalProducts,
            DistrictParkIndustryResourceKind::Flour,
            DistrictParkIndustryResourceKind::Paper,
            DistrictParkIndustryResourceKind::PlanedTimber,
            DistrictParkIndustryResourceKind::Petroleum,
            DistrictParkIndustryResourceKind::Plastics,
            DistrictParkIndustryResourceKind::Glass,
            DistrictParkIndustryResourceKind::Metals,
            DistrictParkIndustryResourceKind::LuxuryProducts,
        ];
        for (version, record_bytes, field_count) in [(111_009, 40, 10), (111_010, 56, 14)] {
            let mut prefix_bytes = Vec::new();
            zero_flags(&mut prefix_bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[5] = 1;
            write_encoded_uint_array(&mut prefix_bytes, &park_flags);
            zero_policy_bools(&mut prefix_bytes, version);
            let (prefix, _) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();

            let scalar_bytes = super::park_scalar_bytes(version);
            let mut record = vec![0; scalar_bytes];
            record[42] = 5;
            let expected_value = |resource: usize, field: usize| {
                0x1020_3000_u32 + (resource as u32 * 0x100) + field as u32
            };
            for resource in 0..kinds.len() {
                for field in 0..field_count {
                    record.extend_from_slice(&expected_value(resource, field).to_be_bytes());
                }
            }

            let (parks, consumed) = prefix.decode_park_records(&record).unwrap();
            assert_eq!(consumed, record.len());
            let resources = parks[0].industry_resources().unwrap();
            assert_eq!(resources.len(), kinds.len());
            for (index, resource) in resources.iter().enumerate() {
                assert_eq!(resource.kind, kinds[index]);
                assert_eq!(resource.temporary_consumption, expected_value(index, 0));
                assert_eq!(resource.temporary_production, expected_value(index, 1));
                assert_eq!(resource.temporary_buffer_amount, expected_value(index, 2));
                assert_eq!(
                    resource.temporary_incoming_transfer,
                    expected_value(index, 3)
                );
                assert_eq!(resource.temporary_buffer_capacity, expected_value(index, 4));
                assert_eq!(resource.final_consumption, expected_value(index, 5));
                assert_eq!(resource.final_production, expected_value(index, 6));
                assert_eq!(resource.final_buffer_amount, expected_value(index, 7));
                assert_eq!(resource.final_incoming_transfer, expected_value(index, 8));
                assert_eq!(resource.final_buffer_capacity, expected_value(index, 9));
                assert_eq!(
                    resource.trade_data,
                    (version >= 111_010).then_some(DistrictParkIndustryTradeData {
                        temporary_import: expected_value(index, 10),
                        temporary_export: expected_value(index, 11),
                        final_import: expected_value(index, 12),
                        final_export: expected_value(index, 13),
                    })
                );
            }

            let replacement = DistrictParkIndustryResource {
                kind: DistrictParkIndustryResourceKind::PlanedTimber,
                temporary_consumption: 0xa001_0001,
                temporary_production: 0xa002_0002,
                temporary_buffer_amount: 0xa003_0003,
                temporary_incoming_transfer: 0xa004_0004,
                temporary_buffer_capacity: 0xa005_0005,
                final_consumption: 0xa006_0006,
                final_production: 0xa007_0007,
                final_buffer_amount: 0xa008_0008,
                final_incoming_transfer: 0xa009_0009,
                final_buffer_capacity: 0xa00a_000a,
                trade_data: (version >= 111_010).then_some(DistrictParkIndustryTradeData {
                    temporary_import: 0xa00b_000b,
                    temporary_export: 0xa00c_000c,
                    final_import: 0xa00d_000d,
                    final_export: 0xa00e_000e,
                }),
            };
            let mut editor = parks[0].clone().into_editor();
            editor.set_industry_resource(replacement).unwrap();
            assert_eq!(editor.industry_resources().unwrap()[7], replacement);

            let mut expected = record.clone();
            let edited_record_offset = scalar_bytes + 7 * record_bytes;
            for (field, value) in [
                replacement.temporary_consumption,
                replacement.temporary_production,
                replacement.temporary_buffer_amount,
                replacement.temporary_incoming_transfer,
                replacement.temporary_buffer_capacity,
                replacement.final_consumption,
                replacement.final_production,
                replacement.final_buffer_amount,
                replacement.final_incoming_transfer,
                replacement.final_buffer_capacity,
            ]
            .into_iter()
            .enumerate()
            {
                let start = edited_record_offset + field * 4;
                expected[start..start + 4].copy_from_slice(&value.to_be_bytes());
            }
            if let Some(trade_data) = replacement.trade_data {
                for (field, value) in [
                    trade_data.temporary_import,
                    trade_data.temporary_export,
                    trade_data.final_import,
                    trade_data.final_export,
                ]
                .into_iter()
                .enumerate()
                {
                    let start = edited_record_offset + (10 + field) * 4;
                    expected[start..start + 4].copy_from_slice(&value.to_be_bytes());
                }
            }
            assert_eq!(editor.encode(), expected);

            let mismatched_trade_data = DistrictParkIndustryResource {
                trade_data: if version < 111_010 {
                    Some(DistrictParkIndustryTradeData {
                        temporary_import: 1,
                        temporary_export: 2,
                        final_import: 3,
                        final_export: 4,
                    })
                } else {
                    None
                },
                ..replacement
            };
            assert!(editor.set_industry_resource(mismatched_trade_data).is_err());
            assert_eq!(editor.encode(), expected);
        }
    }

    #[test]
    fn industry_resources_precede_slot_zero_campus_compatibility_block() {
        let version = 114_008;
        let mut prefix_bytes = Vec::new();
        write_unique_string(&mut prefix_bytes, "").unwrap();
        prefix_bytes.push(0);
        zero_flags(&mut prefix_bytes);
        let mut park_flags = [0_u32; DISTRICT_COUNT];
        park_flags[0] = 1;
        write_encoded_uint_array(&mut prefix_bytes, &park_flags);
        zero_policy_bools(&mut prefix_bytes, version);
        let (prefix, _) =
            DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();

        let mut bytes = vec![0; super::park_scalar_bytes(version)];
        bytes[42] = 5;
        bytes.extend(vec![0xa5; 13 * 56]);
        bytes.extend_from_slice(&zero_campus(version));
        let expected_len = bytes.len();
        bytes.push(0xff);
        let (records, used) = prefix.decode_park_records(&bytes).unwrap();
        assert_eq!(used, expected_len);
        assert_eq!(records[0].park_type(), 5);
        let resources = records[0].industry_resources().unwrap();
        assert_eq!(resources.len(), 13);
        assert_eq!(resources[0].kind, DistrictParkIndustryResourceKind::Grain);
        assert_eq!(
            resources[12].kind,
            DistrictParkIndustryResourceKind::LuxuryProducts
        );
        assert_eq!(resources[0].temporary_consumption, 0xa5a5_a5a5);
        assert_eq!(
            resources[12].trade_data,
            Some(DistrictParkIndustryTradeData {
                temporary_import: 0xa5a5_a5a5,
                temporary_export: 0xa5a5_a5a5,
                final_import: 0xa5a5_a5a5,
                final_export: 0xa5a5_a5a5,
            })
        );
        assert_eq!(records[0].encode(), bytes[..used]);
        assert_eq!(bytes[used], 0xff);
        assert!(
            prefix
                .decode_park_records(&bytes[..expected_len - 1])
                .is_err()
        );
    }

    #[test]
    fn airport_park_fields_follow_each_version_gate() {
        for (version, airport_bytes) in [
            (114_000, 4),
            (114_003, 4),
            (114_004, 12),
            (114_007, 12),
            (114_008, 15),
            (114_009, 15),
            (114_010, 23),
            (114_011, 31),
        ] {
            assert_eq!(super::airport_park_bytes(version), airport_bytes);
            let mut prefix_bytes = Vec::new();
            if version >= AIRLINE_NAME_VERSION {
                write_unique_string(&mut prefix_bytes, "").unwrap();
            }
            if version >= AIRPORT_PANEL_VERSION {
                prefix_bytes.push(0);
            }
            zero_flags(&mut prefix_bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[5] = 1;
            write_encoded_uint_array(&mut prefix_bytes, &park_flags);
            zero_policy_bools(&mut prefix_bytes, version);
            let (prefix, _) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();
            let mut record = vec![0; super::park_scalar_bytes(version)];
            record[42] = 14;
            record.extend(vec![0x5a; airport_bytes]);
            let mut bytes = record.clone();
            bytes.push(0xff);
            let (parks, used) = prefix.decode_park_records(&bytes).unwrap();
            assert_eq!(used, record.len());
            assert_eq!(parks[0].park_type(), 14);
            assert_eq!(parks[0].encode(), record);
            assert_eq!(bytes[used], 0xff);
            assert!(
                prefix
                    .decode_park_records(&record[..record.len() - 1])
                    .is_err()
            );
            for unsupported in [16] {
                record[42] = unsupported;
                assert!(prefix.decode_park_records(&record).is_err());
            }
        }
    }

    #[test]
    fn airport_fields_follow_slot_zero_campus_compatibility_block() {
        let version = 114_011;
        let mut prefix_bytes = Vec::new();
        write_unique_string(&mut prefix_bytes, "").unwrap();
        prefix_bytes.push(0);
        zero_flags(&mut prefix_bytes);
        let mut park_flags = [0_u32; DISTRICT_COUNT];
        park_flags[0] = 1;
        write_encoded_uint_array(&mut prefix_bytes, &park_flags);
        zero_policy_bools(&mut prefix_bytes, version);
        let (prefix, _) =
            DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();
        let mut record = vec![0; super::park_scalar_bytes(version)];
        record[42] = 14;
        record.extend_from_slice(&zero_campus(version));
        record.extend([0x5a; 31]);
        let (parks, used) = prefix.decode_park_records(&record).unwrap();
        assert_eq!(used, record.len());
        assert_eq!(parks[0].encode(), record);
        assert!(
            prefix
                .decode_park_records(&record[..record.len() - 1])
                .is_err()
        );
    }

    #[test]
    fn campus_park_records_follow_versioned_report_and_sports_gates() {
        for (version, park_type, expected_campus_bytes) in [
            (112_007, 10, 0),
            (112_008, 10, 32),
            (112_009, 11, 34),
            (112_011, 12, 36),
            (112_012, 13, 38),
            (112_013, 10, 40),
            (112_014, 11, 167),
            (112_017, 11, 242),
            (112_020, 12, 912),
            (112_023, 12, 951),
            (112_024, 13, 953),
            (112_025, 10, 958),
            (112_026, 11, 984),
            (114_005, 12, 984),
            (118_011, 10, 984),
        ] {
            let mut prefix_bytes = Vec::new();
            if version >= AIRLINE_NAME_VERSION {
                write_unique_string(&mut prefix_bytes, "").unwrap();
            }
            if version >= AIRPORT_PANEL_VERSION {
                prefix_bytes.push(0);
            }
            zero_flags(&mut prefix_bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[5] = 1;
            write_encoded_uint_array(&mut prefix_bytes, &park_flags);
            zero_policy_bools(&mut prefix_bytes, version);
            let (prefix, _) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();
            let campus = if version >= 112_008 {
                zero_campus(version)
            } else {
                Vec::new()
            };
            assert_eq!(campus.len(), expected_campus_bytes);
            let mut record = vec![0; super::park_scalar_bytes(version)];
            record[42 + super::park_happiness_bytes(version)] = park_type;
            record.extend_from_slice(&campus);
            let mut bytes = record.clone();
            bytes.push(0xff);
            let (parks, used) = prefix.decode_park_records(&bytes).unwrap();
            assert_eq!(used, record.len());
            assert_eq!(parks[0].park_type(), park_type);
            assert_eq!(parks[0].encode(), record);
            assert_eq!(
                parks[0].campus_data(),
                (version >= 112_008).then_some(DistrictCampusData {
                    student_count: 0,
                    student_capacity: 0,
                    campus_building_attractiveness: 0,
                    academic_works: [0; 4],
                })
            );
            match parks[0].campus_year_reports() {
                Some(reports) => {
                    let expected_report_count = 2
                        + usize::from(version >= 112_009)
                        + if version >= 112_020 { 10 } else { 0 };
                    assert_eq!(reports.len(), expected_report_count);
                    assert_eq!(reports[0].student_count().is_some(), version >= 112_014);
                    assert_eq!(reports[0].toga_party_count().is_some(), version >= 112_017);
                    assert_eq!(reports[0].has_milestone_field(), version >= 112_023);
                    assert_eq!(reports[0].total_works().is_some(), version >= 112_026);
                }
                None => assert!(version < 112_008),
            }
            match parks[0].campus_sports_data() {
                Some(sports) => {
                    assert!(!sports.is_main_campus());
                    assert_eq!(sports.grant_type(), 0);
                    assert_eq!(sports.coach_count(), 0);
                    assert_eq!(sports.cheerleading_budget(), 0);
                    assert_eq!(sports.academic_staff_count().is_some(), version >= 112_025);
                    assert_eq!(
                        sports.academic_staff_accumulation().is_some(),
                        version >= 112_025
                    );
                    assert_eq!(sports.coach_hire_time_ticks(), &[0; 25]);
                    assert_eq!(sports.win_probability(), 0);
                    assert_eq!(sports.varsity_identity_index(), 0);
                    assert_eq!(sports.varsity_color(), [0; 3]);
                    assert_eq!(sports.lifetime_trophies().is_some(), version >= 112_009);
                    assert_eq!(
                        sports.static_varsity_attractiveness_modifier().is_some(),
                        version >= 112_011
                    );
                    assert_eq!(
                        sports.dynamic_varsity_attractiveness_modifier().is_some(),
                        version >= 112_012
                    );
                    assert_eq!(sports.sports().is_some(), version >= 112_013);
                    assert_eq!(sports.away_matches_done().is_some(), version >= 112_014);
                    assert_eq!(sports.campus_building_count().is_some(), version >= 112_024);
                }
                None => assert!(version < 112_008),
            }
            if version == 112_014 {
                let mut editor = parks[0].clone().into_editor();
                let original = editor.encode();
                {
                    let mut report_editor = editor.campus_year_report_mut(0).unwrap();
                    assert!(report_editor.set_toga_party_count(1).is_err());
                    assert!(report_editor.set_total_works(1).is_err());
                }
                {
                    let mut sports_editor = editor.campus_sports_mut().unwrap();
                    assert!(sports_editor.set_academic_staff_count(1).is_err());
                    assert!(sports_editor.set_campus_building_count(1).is_err());
                }
                assert_eq!(editor.encode(), original);
            }
            if version == 112_013 {
                let mut editor = parks[0].clone().into_editor();
                let original = editor.encode();
                assert!(
                    editor
                        .campus_sports_mut()
                        .unwrap()
                        .set_away_matches_done([true; 5])
                        .is_err()
                );
                assert_eq!(editor.encode(), original);
            }
            assert_eq!(bytes[used], 0xff);
            if expected_campus_bytes != 0 {
                assert!(
                    prefix
                        .decode_park_records(&record[..record.len() - 1])
                        .is_err()
                );
            }
            assert!(prefix.decode_type_zero_park_records(&record).is_err());
        }
    }

    #[test]
    fn campus_summary_can_be_edited_without_rewriting_the_rest_of_the_record() {
        let version = 112_026;
        let mut prefix_bytes = Vec::new();
        if version >= AIRLINE_NAME_VERSION {
            write_unique_string(&mut prefix_bytes, "").unwrap();
        }
        if version >= AIRPORT_PANEL_VERSION {
            prefix_bytes.push(0);
        }
        zero_flags(&mut prefix_bytes);
        let mut park_flags = [0_u32; DISTRICT_COUNT];
        park_flags[5] = 1;
        write_encoded_uint_array(&mut prefix_bytes, &park_flags);
        zero_policy_bools(&mut prefix_bytes, version);
        let (prefix, _) =
            DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();

        let initial = DistrictCampusData {
            student_count: 0x1234,
            student_capacity: 0x5678,
            campus_building_attractiveness: 0x9abc,
            academic_works: [0x0102_0304, -2, 0x1122_3344, i32::MIN],
        };
        let mut campus = zero_campus(version);
        campus[0..2].copy_from_slice(&initial.student_count.to_be_bytes());
        campus[2..4].copy_from_slice(&initial.student_capacity.to_be_bytes());
        campus[4..6].copy_from_slice(&initial.campus_building_attractiveness.to_be_bytes());
        for (index, value) in initial.academic_works.iter().enumerate() {
            let start = 6 + index * 4;
            campus[start..start + 4].copy_from_slice(&value.to_be_bytes());
        }
        let mut record = vec![0; super::park_scalar_bytes(version)];
        record[42 + super::park_happiness_bytes(version)] = 10;
        record.extend_from_slice(&campus);

        let (parks, consumed) = prefix.decode_park_records(&record).unwrap();
        assert_eq!(consumed, record.len());
        assert_eq!(parks[0].campus_data(), Some(initial));
        assert_eq!(parks[0].encode(), record);

        let replacement = DistrictCampusData {
            student_count: 7,
            student_capacity: 900,
            campus_building_attractiveness: 123,
            academic_works: [-1, 2, i32::MAX, 0x5566_7788],
        };
        let mut editor = parks.into_iter().next().unwrap().into_editor();
        assert_eq!(editor.campus_data(), Some(initial));
        editor.set_campus_data(replacement).unwrap();
        let edited = editor.encode();
        let campus_offset = super::park_scalar_bytes(version);
        let expected_summary = {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&replacement.student_count.to_be_bytes());
            bytes.extend_from_slice(&replacement.student_capacity.to_be_bytes());
            bytes.extend_from_slice(&replacement.campus_building_attractiveness.to_be_bytes());
            for value in replacement.academic_works {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes
        };
        assert_eq!(
            &edited[campus_offset..campus_offset + expected_summary.len()],
            expected_summary
        );
        assert_eq!(
            &edited[campus_offset + expected_summary.len()..],
            &record[campus_offset + expected_summary.len()..]
        );
        assert_eq!(editor.campus_data(), Some(replacement));
    }

    #[test]
    fn campus_reports_decode_versioned_fields_and_edit_fixed_width_values() {
        let version = 112_026;
        let mut prefix_bytes = Vec::new();
        if version >= AIRLINE_NAME_VERSION {
            write_unique_string(&mut prefix_bytes, "").unwrap();
        }
        if version >= AIRPORT_PANEL_VERSION {
            prefix_bytes.push(0);
        }
        zero_flags(&mut prefix_bytes);
        let mut park_flags = [0_u32; DISTRICT_COUNT];
        park_flags[5] = 1;
        write_encoded_uint_array(&mut prefix_bytes, &park_flags);
        zero_policy_bools(&mut prefix_bytes, version);
        let (prefix, _) =
            DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();

        let mut campus = zero_campus(version);
        let report_start = 22;
        campus[report_start..report_start + 4].copy_from_slice(&(-7_i32).to_be_bytes());
        campus[report_start + 4..report_start + 8].copy_from_slice(&0x1020_3040_i32.to_be_bytes());
        campus[report_start + 8] = 0xfe; // Signed reputation level -2.
        campus[report_start + 9] = 0xfd; // Signed grant type -3.
        campus[report_start + 10..report_start + 12].copy_from_slice(&(-9_i16).to_be_bytes());
        let academic_works = [1, -2, 3, i32::MAX];
        for (index, value) in academic_works.iter().enumerate() {
            let start = report_start + 12 + index * 4;
            campus[start..start + 4].copy_from_slice(&value.to_be_bytes());
        }
        campus[report_start + 28..report_start + 36]
            .copy_from_slice(&0x0102_0304_0506_0708_u64.to_be_bytes());

        let mut ticket_income = vec![0x00];
        ticket_income.extend_from_slice(&10_u64.to_be_bytes());
        ticket_income.push(0x83); // Four repeats of the +10 delta.
        campus.splice(report_start + 36..report_start + 37, ticket_income);
        let matches_won_offset = report_start + 36 + 10 + 1;
        campus.splice(
            matches_won_offset..matches_won_offset + 1,
            [0x04, 1, 2, 3, 4, 5],
        );
        let trophies_offset = report_start + 36 + 10 + 1 + 6 + 1 + 1;
        campus.splice(trophies_offset..trophies_offset + 1, [0x00, 0x15]);

        let toga_offset = report_start + 57;
        campus[toga_offset] = 0xff; // Signed count -1.
        for (index, value) in [1_i64, -2, i64::MIN].iter().enumerate() {
            let start = toga_offset + 1 + index * 8;
            campus[start..start + 8].copy_from_slice(&value.to_be_bytes());
        }
        let total_works_offset = report_start + 85;
        campus[total_works_offset..total_works_offset + 2]
            .copy_from_slice(&(-11_i16).to_be_bytes());

        let mut record = vec![0; super::park_scalar_bytes(version)];
        record[42 + super::park_happiness_bytes(version)] = 10;
        record.extend_from_slice(&campus);
        let (parks, consumed) = prefix.decode_park_records(&record).unwrap();
        assert_eq!(consumed, record.len());
        let reports = parks[0].campus_year_reports().unwrap();
        assert_eq!(reports.len(), 13);
        let report = &reports[0];
        assert_eq!(report.student_count(), Some(-7));
        assert_eq!(report.campus_attractiveness(), Some(0x1020_3040));
        assert_eq!(report.reputation_level(), Some(-2));
        assert_eq!(report.grant_type(), Some(-3));
        assert_eq!(report.grant_work_index(), Some(-9));
        assert_eq!(report.academic_works(), Some(academic_works));
        assert_eq!(report.total_prize_money(), Some(0x0102_0304_0506_0708));
        assert_eq!(report.tickets_income(), Some([10, 20, 30, 40, 50]));
        assert_eq!(report.winnings_income(), Some([0; 5]));
        assert_eq!(report.matches_won(), Some([1, 2, 3, 4, 5]));
        assert_eq!(report.matches_lost(), Some([0; 5]));
        assert_eq!(report.matches_cancelled(), Some([0; 5]));
        assert_eq!(report.trophies(), Some([true, false, true, false, true]));
        assert_eq!(report.toga_party_count(), Some(-1));
        assert_eq!(report.toga_party_seeds(), Some([1, -2, i64::MIN]));
        assert!(report.has_milestone_field());
        assert_eq!(report.milestone_unlocked(), Some(""));
        assert_eq!(report.total_works(), Some(-11));

        let mut editor = parks.into_iter().next().unwrap().into_editor();
        let new_works = [-8, 9, i32::MIN, 10];
        let new_seeds = [i64::MAX, 0, -1];
        {
            let mut report_editor = editor.campus_year_report_mut(0).unwrap();
            report_editor.set_student_count(123).unwrap();
            report_editor.set_academic_works(new_works).unwrap();
            report_editor.set_toga_party_seeds(new_seeds).unwrap();
            report_editor.set_total_works(77).unwrap();
            assert_eq!(report_editor.report().student_count(), Some(123));
            assert_eq!(report_editor.report().academic_works(), Some(new_works));
            assert_eq!(report_editor.report().toga_party_seeds(), Some(new_seeds));
            assert_eq!(report_editor.report().total_works(), Some(77));
        }
        let mut expected = record;
        let first_report_offset = super::park_scalar_bytes(version) + report_start;
        expected[first_report_offset..first_report_offset + 4]
            .copy_from_slice(&123_i32.to_be_bytes());
        for (index, value) in new_works.iter().enumerate() {
            let start = first_report_offset + 12 + index * 4;
            expected[start..start + 4].copy_from_slice(&value.to_be_bytes());
        }
        for (index, value) in new_seeds.iter().enumerate() {
            let start = first_report_offset + 58 + index * 8;
            expected[start..start + 8].copy_from_slice(&value.to_be_bytes());
        }
        let total_works_start = first_report_offset + 85;
        expected[total_works_start..total_works_start + 2].copy_from_slice(&77_i16.to_be_bytes());
        assert_eq!(editor.encode(), expected);
    }

    #[test]
    fn campus_sports_fields_decode_versioned_values_and_edit_in_place() {
        let version = 112_026;
        let mut prefix_bytes = Vec::new();
        if version >= AIRLINE_NAME_VERSION {
            write_unique_string(&mut prefix_bytes, "").unwrap();
        }
        if version >= AIRPORT_PANEL_VERSION {
            prefix_bytes.push(0);
        }
        zero_flags(&mut prefix_bytes);
        let mut park_flags = [0_u32; DISTRICT_COUNT];
        park_flags[5] = 1;
        write_encoded_uint_array(&mut prefix_bytes, &park_flags);
        zero_policy_bools(&mut prefix_bytes, version);
        let (prefix, _) =
            DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();

        let mut campus = zero_campus(version);
        let sports_offset = 22 + 13 * 72;
        campus[sports_offset] = 1; // ReadBool is true only for byte one.
        campus[sports_offset + 1] = 0xa1;
        campus[sports_offset + 2] = 7;
        campus[sports_offset + 3..sports_offset + 5].copy_from_slice(&(-123_i16).to_be_bytes());
        campus[sports_offset + 5] = 8;
        campus[sports_offset + 6..sports_offset + 10]
            .copy_from_slice(&1.5_f32.to_bits().to_be_bytes());
        campus[sports_offset + 11] = 72;
        campus[sports_offset + 12..sports_offset + 14].copy_from_slice(&(-9_i16).to_be_bytes());
        campus[sports_offset + 15..sports_offset + 17].copy_from_slice(&10_i16.to_be_bytes());
        campus[sports_offset + 17..sports_offset + 19].copy_from_slice(&(-11_i16).to_be_bytes());
        campus[sports_offset + 19..sports_offset + 21].copy_from_slice(&12_i16.to_be_bytes());
        campus[sports_offset + 21..sports_offset + 23].copy_from_slice(&(-13_i16).to_be_bytes());
        campus[sports_offset + 24..sports_offset + 26].copy_from_slice(&14_i16.to_be_bytes());
        let mut encoded_coach_times = vec![1]; // Two literal values, then 23 equal deltas.
        encoded_coach_times.extend_from_slice(&(-7_i64).to_be_bytes());
        encoded_coach_times.extend_from_slice(&(-7_i64).to_be_bytes());
        encoded_coach_times.push(0x96);
        campus.splice(sports_offset + 10..sports_offset + 11, encoded_coach_times);

        let mut record = vec![0; super::park_scalar_bytes(version)];
        record[42 + super::park_happiness_bytes(version)] = 10;
        record.extend_from_slice(&campus);
        let (parks, consumed) = prefix.decode_park_records(&record).unwrap();
        assert_eq!(consumed, record.len());
        let sports = parks[0].campus_sports_data().unwrap();
        assert!(sports.is_main_campus());
        assert_eq!(sports.grant_type(), 0xa1);
        assert_eq!(sports.coach_count(), 7);
        assert_eq!(sports.cheerleading_budget(), -123);
        assert_eq!(sports.academic_staff_count(), Some(8));
        assert_eq!(sports.academic_staff_accumulation(), Some(1.5));
        assert_eq!(sports.coach_hire_time_ticks(), &[-7; 25]);
        assert_eq!(sports.win_probability(), 72);
        assert_eq!(sports.varsity_identity_index(), -9);
        assert_eq!(sports.varsity_color(), [0; 3]);
        assert_eq!(sports.lifetime_trophies(), Some(10));
        assert_eq!(sports.static_varsity_attractiveness_modifier(), Some(-11));
        assert_eq!(sports.dynamic_varsity_attractiveness_modifier(), Some(12));
        assert_eq!(sports.sports(), Some(-13));
        assert_eq!(sports.away_matches_done(), Some([false; 5]));
        assert_eq!(sports.campus_building_count(), Some(14));

        let mut fixed_field_editor = parks[0].clone().into_editor();
        fixed_field_editor
            .campus_sports_mut()
            .unwrap()
            .set_grant_type(0x5a)
            .unwrap();
        let mut fixed_field_expected = record.clone();
        fixed_field_expected[super::park_scalar_bytes(version) + sports_offset + 1] = 0x5a;
        assert_eq!(fixed_field_editor.encode(), fixed_field_expected);

        let mut editor = parks.into_iter().next().unwrap().into_editor();
        let new_staff_accumulation = -2.25_f32;
        let new_coach_times = [5_i64; 25];
        let new_varsity_color = [0x12, 0x34, 0x56];
        let new_away_matches = [true, false, true, false, true];
        {
            let mut campus_editor = editor.campus_sports_mut().unwrap();
            campus_editor
                .set_coach_hire_time_ticks(new_coach_times)
                .unwrap();
            campus_editor.set_varsity_color(new_varsity_color).unwrap();
            campus_editor
                .set_away_matches_done(new_away_matches)
                .unwrap();
            campus_editor.set_is_main_campus(false).unwrap();
            campus_editor.set_grant_type(9).unwrap();
            campus_editor.set_coach_count(10).unwrap();
            campus_editor.set_cheerleading_budget(-101).unwrap();
            campus_editor.set_academic_staff_count(11).unwrap();
            campus_editor
                .set_academic_staff_accumulation(new_staff_accumulation)
                .unwrap();
            campus_editor.set_win_probability(88).unwrap();
            campus_editor.set_varsity_identity_index(222).unwrap();
            campus_editor.set_lifetime_trophies(33).unwrap();
            campus_editor
                .set_static_varsity_attractiveness_modifier(44)
                .unwrap();
            campus_editor
                .set_dynamic_varsity_attractiveness_modifier(55)
                .unwrap();
            campus_editor.set_sports(66).unwrap();
            campus_editor.set_campus_building_count(77).unwrap();
            assert!(!campus_editor.data().is_main_campus());
            assert_eq!(
                campus_editor.data().academic_staff_accumulation(),
                Some(new_staff_accumulation)
            );
            assert_eq!(
                campus_editor.data().coach_hire_time_ticks(),
                &new_coach_times
            );
            assert_eq!(campus_editor.data().varsity_color(), new_varsity_color);
            assert_eq!(
                campus_editor.data().away_matches_done(),
                Some(new_away_matches)
            );
        }

        let mut expected = record;
        let start = super::park_scalar_bytes(version) + sports_offset;
        let original_coach_array_growth = 17;
        let coach_array_growth = original_coach_array_growth + 183;
        let color_array_growth = 3;
        let away_array_growth = 1;
        let mut encoded_coach_times = vec![0x18];
        for _ in 0..25 {
            encoded_coach_times.extend_from_slice(&5_u64.to_be_bytes());
        }
        expected.splice(start + 10..start + 28, encoded_coach_times);
        expected.splice(
            start + 14 + coach_array_growth..start + 15 + coach_array_growth,
            [0x02, 0x12, 0x34, 0x56],
        );
        expected.splice(
            start + 23 + coach_array_growth + color_array_growth
                ..start + 24 + coach_array_growth + color_array_growth,
            [0x00, 0x15],
        );
        expected[start] = 0;
        expected[start + 1] = 9;
        expected[start + 2] = 10;
        expected[start + 3..start + 5].copy_from_slice(&(-101_i16).to_be_bytes());
        expected[start + 5] = 11;
        expected[start + 6..start + 10]
            .copy_from_slice(&new_staff_accumulation.to_bits().to_be_bytes());
        expected[start + 11 + coach_array_growth] = 88;
        expected[start + 12 + coach_array_growth..start + 14 + coach_array_growth]
            .copy_from_slice(&222_i16.to_be_bytes());
        expected[start + 15 + coach_array_growth + color_array_growth
            ..start + 17 + coach_array_growth + color_array_growth]
            .copy_from_slice(&33_i16.to_be_bytes());
        expected[start + 17 + coach_array_growth + color_array_growth
            ..start + 19 + coach_array_growth + color_array_growth]
            .copy_from_slice(&44_i16.to_be_bytes());
        expected[start + 19 + coach_array_growth + color_array_growth
            ..start + 21 + coach_array_growth + color_array_growth]
            .copy_from_slice(&55_i16.to_be_bytes());
        expected[start + 21 + coach_array_growth + color_array_growth
            ..start + 23 + coach_array_growth + color_array_growth]
            .copy_from_slice(&66_i16.to_be_bytes());
        expected[start + 24 + coach_array_growth + color_array_growth + away_array_growth
            ..start + 26 + coach_array_growth + color_array_growth + away_array_growth]
            .copy_from_slice(&77_i16.to_be_bytes());
        assert_eq!(editor.encode(), expected);
    }

    #[test]
    fn pedestrian_park_records_follow_list_and_private_data_gates() {
        for (version, expected_body_bytes) in [
            (115_002, 0),
            (115_003, 24),
            (115_007, 192),
            (115_013, 211),
            (115_015, 229),
            (115_016, 253),
            (115_020, 253),
            (115_021, 254),
            (115_023, 256),
        ] {
            let mut prefix_bytes = Vec::new();
            write_unique_string(&mut prefix_bytes, "").unwrap();
            prefix_bytes.push(0);
            zero_flags(&mut prefix_bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[5] = 1;
            write_encoded_uint_array(&mut prefix_bytes, &park_flags);
            zero_policy_bools(&mut prefix_bytes, version);
            let (prefix, _) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();
            let body = pedestrian_body(version);
            assert_eq!(body.len(), expected_body_bytes);
            let scalar_bytes = super::park_scalar_bytes(version);
            let type_offset = 42 + super::park_happiness_bytes(version);
            let mut record = vec![0; scalar_bytes];
            record[type_offset] = 15;
            if version >= 115_013 {
                record[type_offset + 4] = 2; // Temporary gate count.
                record[type_offset + 5] = 1; // Final gate count.
            }
            record.extend_from_slice(&body);
            let mut bytes = record.clone();
            bytes.push(0xff);
            let (parks, used) = prefix.decode_park_records(&bytes).unwrap();
            assert_eq!(used, record.len());
            assert_eq!(parks[0].park_type(), 15);
            assert_eq!(parks[0].encode(), record);
            assert_eq!(bytes[used], 0xff);
            if expected_body_bytes != 0 {
                assert!(
                    prefix
                        .decode_park_records(&record[..record.len() - 1])
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn pedestrian_park_rejects_negative_and_unbounded_counts() {
        let version = 115_023;
        let mut prefix_bytes = Vec::new();
        write_unique_string(&mut prefix_bytes, "").unwrap();
        prefix_bytes.push(0);
        zero_flags(&mut prefix_bytes);
        let mut park_flags = [0_u32; DISTRICT_COUNT];
        park_flags[5] = 1;
        write_encoded_uint_array(&mut prefix_bytes, &park_flags);
        zero_policy_bools(&mut prefix_bytes, version);
        let (prefix, _) =
            DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();
        let scalar_bytes = super::park_scalar_bytes(version);
        let mut record = vec![0; scalar_bytes];
        record[58] = 15;
        record[62] = 2;
        record[63] = 1;
        record.extend_from_slice(&pedestrian_body(version));
        assert!(prefix.decode_park_records(&record).is_ok());

        for (offset, value) in [
            (scalar_bytes, (-1_i32).to_be_bytes()),
            (scalar_bytes, i32::MAX.to_be_bytes()),
            (scalar_bytes + 5, (-1_i32).to_be_bytes()),
        ] {
            let mut malformed = record.clone();
            malformed[offset..offset + 4].copy_from_slice(&value);
            assert!(prefix.decode_park_records(&malformed).is_err());
        }
        let mut malformed = record;
        malformed[scalar_bytes + 32] = 0xff; // Signed random service-point count.
        assert!(prefix.decode_park_records(&malformed).is_err());
    }

    #[test]
    fn type_zero_park_record_version_boundaries_are_explicit() {
        for (version, expected_bytes) in [
            (110_025, 62),
            (111_008, 62),
            (111_009, 70),
            (111_010, 70),
            (111_011, 74),
            (111_014, 74),
            (111_015, 78),
            (115_010, 78),
            (115_011, 82),
            (115_021, 82),
            (115_022, 94),
        ] {
            assert_eq!(super::park_scalar_bytes(version), expected_bytes);
        }
        for (version, supported) in [
            (110_024, false),
            (110_025, true),
            (111_008, true),
            (111_009, true),
            (111_010, true),
            (111_011, true),
            (111_014, true),
            (111_015, true),
            (114_004, true),
            (114_005, true),
            (114_011, true),
            (115_010, true),
            (115_011, true),
            (115_021, true),
            (115_022, true),
            (118_011, true),
            (118_012, false),
        ] {
            let mut bytes = Vec::new();
            if version >= 114_006 {
                write_unique_string(&mut bytes, "").unwrap();
            }
            if version >= 114_007 {
                bytes.push(0);
            }
            zero_flags(&mut bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[0] = 1;
            write_encoded_uint_array(&mut bytes, &park_flags);
            zero_policy_bools(&mut bytes, version);
            let (prefix, _) = DistrictManagerFlagsPrefix::decode_prefix(version, &bytes).unwrap();
            let happiness_bytes = super::park_happiness_bytes(version);
            let scalar_bytes = super::park_scalar_bytes(version);
            let mut record = vec![0; 16];
            record.extend(vec![0; happiness_bytes]);
            record.extend(vec![0; scalar_bytes - happiness_bytes - 16]);
            if version >= 114_005 {
                let campus = if version == 118_011 {
                    zero_campus_with_reports(version, 10)
                } else {
                    zero_campus(version)
                };
                record.extend_from_slice(&campus);
            }
            let result = prefix.decode_type_zero_park_records(&record);
            if supported {
                let (records, consumed) = result.unwrap();
                assert_eq!(consumed, record.len());
                assert_eq!(records[0].encode(), record);
            } else {
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn empty_park_buffer_has_no_record_bytes_across_version_gates() {
        for version in [164, 110_000, 110_001, 110_024, 118_012] {
            let mut prefix_bytes = Vec::new();
            if version >= AIRLINE_NAME_VERSION {
                write_unique_string(&mut prefix_bytes, "").unwrap();
            }
            if version >= AIRPORT_PANEL_VERSION {
                prefix_bytes.push(0);
            }
            zero_flags(&mut prefix_bytes);
            if version >= PARK_FLAGS_VERSION {
                zero_flags(&mut prefix_bytes);
            }
            zero_policy_bools(&mut prefix_bytes, version);
            let (prefix, used) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();
            assert_eq!(used, prefix_bytes.len());
            let grid = vec![0xff; DISTRICT_GRID_VALUES / 128];
            let (parks, park_used) = prefix.decode_type_zero_park_records(&grid).unwrap();
            assert!(parks.is_empty());
            assert_eq!(park_used, 0);
            let mut streams = grid.clone();
            if version >= PARK_FLAGS_VERSION {
                streams.extend_from_slice(&grid);
            }
            let (grids, grid_used) = DistrictManagerGridData::decode(version, &streams).unwrap();
            assert_eq!(grid_used, streams.len());
            assert_eq!(grids.park_grid_present(), version >= PARK_FLAGS_VERSION);
        }
    }

    #[test]
    fn park_happiness_fields_shift_type_and_level_at_both_gates() {
        for (version, happiness_bytes) in [(115_011, 4), (115_022, 16)] {
            let mut prefix_bytes = Vec::new();
            write_unique_string(&mut prefix_bytes, "").unwrap();
            prefix_bytes.push(0);
            zero_flags(&mut prefix_bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[0] = 1;
            write_encoded_uint_array(&mut prefix_bytes, &park_flags);
            zero_policy_bools(&mut prefix_bytes, version);
            let (prefix, _) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();

            let mut record = vec![0; 16];
            record.extend(vec![0xa5; happiness_bytes]);
            record.extend(vec![0; 78 - 16]);
            let type_offset = 42 + happiness_bytes;
            record[type_offset + 1] = 3;
            record.extend_from_slice(&zero_campus(version));
            let (records, consumed) = prefix.decode_type_zero_park_records(&record).unwrap();
            assert_eq!(consumed, record.len());
            assert_eq!(records[0].level(), 3);
            assert_eq!(records[0].encode(), record);
            let mut editor = records[0].clone().into_editor();
            editor.set_level(9).unwrap();
            let mut edited = record.clone();
            edited[type_offset + 1] = 9;
            assert_eq!(editor.encode(), edited);

            record[type_offset] = 15;
            assert!(prefix.decode_type_zero_park_records(&record).is_err());
            assert!(
                prefix
                    .decode_type_zero_park_records(&record[..type_offset])
                    .is_err()
            );
        }
    }

    #[test]
    fn park_ticket_and_visitor_fields_follow_versioned_offsets() {
        assert!(super::park_visitor_field_offsets(110_007, 42).is_none());
        assert_eq!(
            super::park_visitor_field_offsets(110_008, 42)
                .unwrap()
                .last_visitor_count,
            None
        );
        assert_eq!(
            super::park_visitor_field_offsets(110_014, 42)
                .unwrap()
                .last_visitor_count,
            Some(16)
        );

        for (version, happiness_bytes) in [(110_025, 0), (115_011, 4), (115_022, 16)] {
            let mut prefix_bytes = Vec::new();
            if version >= 114_006 {
                write_unique_string(&mut prefix_bytes, "").unwrap();
            }
            if version >= 114_007 {
                prefix_bytes.push(0);
            }
            zero_flags(&mut prefix_bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[5] = 1;
            write_encoded_uint_array(&mut prefix_bytes, &park_flags);
            zero_policy_bools(&mut prefix_bytes, version);
            let (prefix, _) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();

            let type_offset = super::TYPE_ZERO_PARK_BASE_TYPE_OFFSET + happiness_bytes;
            let offsets = super::park_visitor_field_offsets(version, type_offset).unwrap();
            let (visitor_start, last_visitor_count) = match version {
                110_025 => (20, Some(16)),
                115_011 => (24, Some(20)),
                115_022 => (36, Some(32)),
                _ => unreachable!("test version is enumerated above"),
            };
            assert_eq!(
                offsets,
                super::ParkVisitorFieldOffsets {
                    total_visitors: visitor_start,
                    temporary_ticket_income: visitor_start + 4,
                    final_ticket_income: visitor_start + 8,
                    ticket_price: visitor_start + 12,
                    temporary_residents: visitor_start + 14,
                    final_residents: visitor_start + 16,
                    temporary_tourists: visitor_start + 18,
                    final_tourists: visitor_start + 20,
                    last_visitor_count,
                }
            );
            let mut record = vec![0; super::park_scalar_bytes(version)];
            record[type_offset + 1] = 3;
            for (offset, value) in [
                (
                    offsets.total_visitors,
                    0x0102_0304_u32.to_be_bytes().to_vec(),
                ),
                (
                    offsets.temporary_ticket_income,
                    0x1112_1314_u32.to_be_bytes().to_vec(),
                ),
                (
                    offsets.final_ticket_income,
                    0x2122_2324_u32.to_be_bytes().to_vec(),
                ),
                (offsets.ticket_price, 0x3132_u16.to_be_bytes().to_vec()),
                (
                    offsets.temporary_residents,
                    0x4142_u16.to_be_bytes().to_vec(),
                ),
                (offsets.final_residents, 0x5152_u16.to_be_bytes().to_vec()),
                (
                    offsets.temporary_tourists,
                    0x6162_u16.to_be_bytes().to_vec(),
                ),
                (offsets.final_tourists, 0x7172_u16.to_be_bytes().to_vec()),
            ] {
                record[offset..offset + value.len()].copy_from_slice(&value);
            }
            if let Some(offset) = offsets.last_visitor_count {
                record[offset..offset + 4].copy_from_slice(&0x3132_3334_u32.to_be_bytes());
            }

            let (records, used) = prefix.decode_type_zero_park_records(&record).unwrap();
            assert_eq!(used, record.len());
            let visitor_data = records[0].visitor_data().unwrap();
            assert_eq!(visitor_data.total_visitors(), 0x0102_0304);
            assert_eq!(visitor_data.temporary_ticket_income(), 0x1112_1314);
            assert_eq!(visitor_data.final_ticket_income(), 0x2122_2324);
            assert_eq!(visitor_data.ticket_price(), 0x3132);
            assert_eq!(visitor_data.temporary_residents(), 0x4142);
            assert_eq!(visitor_data.final_residents(), 0x5152);
            assert_eq!(visitor_data.temporary_tourists(), 0x6162);
            assert_eq!(visitor_data.final_tourists(), 0x7172);
            assert_eq!(
                visitor_data.last_visitor_count(),
                offsets.last_visitor_count.map(|_| 0x3132_3334)
            );

            let mut editor = records[0].clone().into_editor();
            editor.set_total_visitors(0xa1a2_a3a4).unwrap();
            editor.set_temporary_ticket_income(0xb1b2_b3b4).unwrap();
            editor.set_final_ticket_income(0xc1c2_c3c4).unwrap();
            editor.set_ticket_price(0xd1d2).unwrap();
            editor.set_temporary_residents(0xe1e2).unwrap();
            editor.set_final_residents(0xf1f2).unwrap();
            editor.set_temporary_tourists(0x0304).unwrap();
            editor.set_final_tourists(0x0506).unwrap();
            if offsets.last_visitor_count.is_some() {
                editor.set_last_visitor_count(0x0708_090a).unwrap();
            } else {
                assert!(editor.set_last_visitor_count(1).is_err());
            }
            let mut expected = record;
            for (offset, value) in [
                (
                    offsets.total_visitors,
                    0xa1a2_a3a4_u32.to_be_bytes().to_vec(),
                ),
                (
                    offsets.temporary_ticket_income,
                    0xb1b2_b3b4_u32.to_be_bytes().to_vec(),
                ),
                (
                    offsets.final_ticket_income,
                    0xc1c2_c3c4_u32.to_be_bytes().to_vec(),
                ),
                (offsets.ticket_price, 0xd1d2_u16.to_be_bytes().to_vec()),
                (
                    offsets.temporary_residents,
                    0xe1e2_u16.to_be_bytes().to_vec(),
                ),
                (offsets.final_residents, 0xf1f2_u16.to_be_bytes().to_vec()),
                (
                    offsets.temporary_tourists,
                    0x0304_u16.to_be_bytes().to_vec(),
                ),
                (offsets.final_tourists, 0x0506_u16.to_be_bytes().to_vec()),
            ] {
                expected[offset..offset + value.len()].copy_from_slice(&value);
            }
            if let Some(offset) = offsets.last_visitor_count {
                expected[offset..offset + 4].copy_from_slice(&0x0708_090a_u32.to_be_bytes());
            }
            assert_eq!(editor.encode(), expected);
        }
    }

    #[test]
    fn park_gate_capacity_and_attractiveness_fields_follow_version_gates() {
        for (version, happiness_bytes, expected_offsets) in [
            (110_025, 0, (44, 48, 56, 57, 61)),
            (115_011, 4, (48, 52, 60, 61, 65)),
            (115_022, 16, (60, 64, 72, 73, 77)),
        ] {
            let type_offset = super::TYPE_ZERO_PARK_BASE_TYPE_OFFSET + happiness_bytes;
            let offsets = super::park_post_level_field_offsets(version, type_offset).unwrap();
            assert_eq!(
                offsets,
                super::ParkPostLevelFieldOffsets {
                    gates: Some(expected_offsets.0),
                    capacities: Some(expected_offsets.1),
                    day_night_count: Some(expected_offsets.2),
                    attractiveness: Some(expected_offsets.3),
                    area_factor: Some(expected_offsets.4),
                    work_storage_deltas: match version {
                        110_025 => None,
                        115_011 => Some(66),
                        115_022 => Some(78),
                        _ => unreachable!("test version is enumerated above"),
                    },
                    production: match version {
                        110_025 => None,
                        115_011 => Some(super::ParkProductionFieldOffsets {
                            total_production: 70,
                            total_production_bytes: 8,
                            temporary_workers: 78,
                            final_workers: 80,
                        }),
                        115_022 => Some(super::ParkProductionFieldOffsets {
                            total_production: 82,
                            total_production_bytes: 8,
                            temporary_workers: 90,
                            final_workers: 92,
                        }),
                        _ => unreachable!("test version is enumerated above"),
                    },
                }
            );
            let mut record = vec![0; super::park_scalar_bytes(version)];
            record[type_offset + 1] = 3;
            let gate = expected_offsets.0;
            record[gate..gate + 2].copy_from_slice(&0x8182_u16.to_be_bytes());
            record[gate + 2] = 0x83;
            record[gate + 3] = 0x84;
            let capacity = expected_offsets.1;
            for (index, value) in [0x9192_u16, 0xa1a2, 0xb1b2, 0xc1c2].into_iter().enumerate() {
                let offset = capacity + index * 2;
                record[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
            }
            record[expected_offsets.2] = 0xd3;
            record[expected_offsets.3..expected_offsets.3 + 2]
                .copy_from_slice(&0xe1e2_u16.to_be_bytes());
            record[expected_offsets.3 + 2..expected_offsets.3 + 4]
                .copy_from_slice(&0xf1f2_u16.to_be_bytes());
            record[expected_offsets.4] = 0x03;

            let mut prefix_bytes = Vec::new();
            if version >= 114_006 {
                write_unique_string(&mut prefix_bytes, "").unwrap();
            }
            if version >= 114_007 {
                prefix_bytes.push(0);
            }
            zero_flags(&mut prefix_bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[5] = 1;
            write_encoded_uint_array(&mut prefix_bytes, &park_flags);
            zero_policy_bools(&mut prefix_bytes, version);
            let (prefix, _) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();
            let (records, consumed) = prefix.decode_type_zero_park_records(&record).unwrap();
            assert_eq!(consumed, record.len());
            assert_eq!(
                records[0].gate_data(),
                Some(DistrictParkGateData {
                    random_gate: 0x8182,
                    temporary_count: 0x83,
                    final_count: 0x84,
                })
            );
            assert_eq!(
                records[0].capacities(),
                Some(DistrictParkCapacities {
                    temporary_visitors: 0x9192,
                    final_visitors: 0xa1a2,
                    temporary_main: 0xb1b2,
                    final_main: 0xc1c2,
                })
            );
            assert_eq!(records[0].day_night_count(), Some(0xd3));
            assert_eq!(
                records[0].attractiveness(),
                Some(DistrictParkAttractiveness {
                    temporary: 0xe1e2,
                    final_value: 0xf1f2,
                })
            );
            assert_eq!(records[0].area_factor(), Some(0x03));
            if version == 110_025 {
                assert_eq!(records[0].work_storage_deltas(), None);
                assert_eq!(records[0].production_data(), None);
                let mut editor = records[0].clone().into_editor();
                assert!(
                    editor
                        .set_work_storage_deltas(DistrictParkWorkStorageDeltas {
                            temporary_work_efficiency: 1,
                            temporary_storage: 2,
                            final_work_efficiency: 3,
                            final_storage: 4,
                        })
                        .is_err()
                );
                assert!(
                    editor
                        .set_production_data(DistrictParkProductionData {
                            total_production: 1,
                            temporary_workers: 2,
                            final_workers: 3,
                        })
                        .is_err()
                );
            }

            let mut editor = records[0].clone().into_editor();
            editor
                .set_gate_data(DistrictParkGateData {
                    random_gate: 0x1122,
                    temporary_count: 0x33,
                    final_count: 0x44,
                })
                .unwrap();
            editor
                .set_capacities(DistrictParkCapacities {
                    temporary_visitors: 0x5566,
                    final_visitors: 0x7788,
                    temporary_main: 0x99aa,
                    final_main: 0xbbcc,
                })
                .unwrap();
            editor.set_day_night_count(0xdd).unwrap();
            editor
                .set_attractiveness(DistrictParkAttractiveness {
                    temporary: 0x1234,
                    final_value: 0x5678,
                })
                .unwrap();
            editor.set_area_factor(0xef).unwrap();

            let mut expected = record;
            expected[gate..gate + 2].copy_from_slice(&0x1122_u16.to_be_bytes());
            expected[gate + 2] = 0x33;
            expected[gate + 3] = 0x44;
            for (index, value) in [0x5566_u16, 0x7788, 0x99aa, 0xbbcc].into_iter().enumerate() {
                let offset = capacity + index * 2;
                expected[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
            }
            expected[expected_offsets.2] = 0xdd;
            expected[expected_offsets.3..expected_offsets.3 + 2]
                .copy_from_slice(&0x1234_u16.to_be_bytes());
            expected[expected_offsets.3 + 2..expected_offsets.3 + 4]
                .copy_from_slice(&0x5678_u16.to_be_bytes());
            expected[expected_offsets.4] = 0xef;
            assert_eq!(editor.encode(), expected);
        }
    }

    #[test]
    fn park_work_storage_production_and_worker_fields_follow_version_gates() {
        for (
            version,
            happiness_bytes,
            work_offset,
            production_offset,
            production_bytes,
            record_len,
        ) in [
            (111_009, 0, None, 62, 4, 70),
            (111_011, 0, Some(62), 66, 4, 74),
            (111_015, 0, Some(62), 66, 8, 78),
            (115_011, 4, Some(66), 70, 8, 82),
            (115_022, 16, Some(78), 82, 8, 94),
        ] {
            let type_offset = super::TYPE_ZERO_PARK_BASE_TYPE_OFFSET + happiness_bytes;
            let expected_offsets = super::ParkPostLevelFieldOffsets {
                gates: Some(44 + happiness_bytes),
                capacities: Some(48 + happiness_bytes),
                day_night_count: Some(56 + happiness_bytes),
                attractiveness: Some(57 + happiness_bytes),
                area_factor: Some(61 + happiness_bytes),
                work_storage_deltas: work_offset,
                production: Some(super::ParkProductionFieldOffsets {
                    total_production: production_offset,
                    total_production_bytes: production_bytes,
                    temporary_workers: production_offset + production_bytes,
                    final_workers: production_offset + production_bytes + 2,
                }),
            };
            assert_eq!(
                super::park_post_level_field_offsets(version, type_offset).unwrap(),
                expected_offsets
            );
            assert_eq!(super::park_scalar_bytes(version), record_len);

            let mut prefix_bytes = Vec::new();
            if version >= 114_006 {
                write_unique_string(&mut prefix_bytes, "").unwrap();
            }
            if version >= 114_007 {
                prefix_bytes.push(0);
            }
            zero_flags(&mut prefix_bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[5] = 1;
            write_encoded_uint_array(&mut prefix_bytes, &park_flags);
            zero_policy_bools(&mut prefix_bytes, version);
            let (prefix, _) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();

            let mut record = vec![0; record_len];
            let initial_production = if production_bytes == 4 {
                record[production_offset..production_offset + 4]
                    .copy_from_slice(&0x0102_0304_u32.to_be_bytes());
                0x0102_0304_u64
            } else {
                record[production_offset..production_offset + 8]
                    .copy_from_slice(&0x0102_0304_0506_0708_u64.to_be_bytes());
                0x0102_0304_0506_0708_u64
            };
            let temporary_workers = production_offset + production_bytes;
            record[temporary_workers..temporary_workers + 2]
                .copy_from_slice(&0x1112_u16.to_be_bytes());
            let final_workers = temporary_workers + 2;
            record[final_workers..final_workers + 2].copy_from_slice(&0x2122_u16.to_be_bytes());
            if let Some(offset) = work_offset {
                record[offset..offset + 4].copy_from_slice(&[0x31, 0x42, 0x53, 0x64]);
            }

            assert!(
                prefix
                    .decode_type_zero_park_records(&record[..record_len - 1])
                    .is_err()
            );
            let (records, consumed) = prefix.decode_type_zero_park_records(&record).unwrap();
            assert_eq!(consumed, record_len);
            assert_eq!(
                records[0].work_storage_deltas().is_some(),
                work_offset.is_some()
            );
            assert_eq!(
                records[0].production_data(),
                Some(DistrictParkProductionData {
                    total_production: initial_production,
                    temporary_workers: 0x1112,
                    final_workers: 0x2122,
                })
            );
            if work_offset.is_some() {
                assert_eq!(
                    records[0].work_storage_deltas(),
                    Some(DistrictParkWorkStorageDeltas {
                        temporary_work_efficiency: 0x31,
                        temporary_storage: 0x42,
                        final_work_efficiency: 0x53,
                        final_storage: 0x64,
                    })
                );
            }

            let replacement_production = if production_bytes == 4 {
                0xa1b2_c3d4_u64
            } else {
                0xa1a2_a3a4_a5a6_a7a8_u64
            };
            let mut editor = records[0].clone().into_editor();
            editor
                .set_production_data(DistrictParkProductionData {
                    total_production: replacement_production,
                    temporary_workers: 0x3132,
                    final_workers: 0x4142,
                })
                .unwrap();
            if work_offset.is_some() {
                editor
                    .set_work_storage_deltas(DistrictParkWorkStorageDeltas {
                        temporary_work_efficiency: 0xd1,
                        temporary_storage: 0xd2,
                        final_work_efficiency: 0xd3,
                        final_storage: 0xd4,
                    })
                    .unwrap();
            } else {
                assert!(
                    editor
                        .set_work_storage_deltas(DistrictParkWorkStorageDeltas {
                            temporary_work_efficiency: 1,
                            temporary_storage: 2,
                            final_work_efficiency: 3,
                            final_storage: 4,
                        })
                        .is_err()
                );
            }

            let mut expected = record;
            if production_bytes == 4 {
                expected[production_offset..production_offset + 4]
                    .copy_from_slice(&(replacement_production as u32).to_be_bytes());
            } else {
                expected[production_offset..production_offset + 8]
                    .copy_from_slice(&replacement_production.to_be_bytes());
            }
            expected[temporary_workers..temporary_workers + 2]
                .copy_from_slice(&0x3132_u16.to_be_bytes());
            expected[final_workers..final_workers + 2].copy_from_slice(&0x4142_u16.to_be_bytes());
            if let Some(offset) = work_offset {
                expected[offset..offset + 4].copy_from_slice(&[0xd1, 0xd2, 0xd3, 0xd4]);
            }
            assert_eq!(editor.encode(), expected);

            if production_bytes == 4 {
                assert!(
                    editor
                        .set_production_data(DistrictParkProductionData {
                            total_production: u64::from(u32::MAX) + 1,
                            temporary_workers: 1,
                            final_workers: 2,
                        })
                        .is_err()
                );
                assert_eq!(editor.encode(), expected);
            }
        }
    }

    #[test]
    fn slot_zero_campus_rejects_truncation_and_preserves_wire_bytes() {
        let version = 114_008;
        let mut prefix_bytes = Vec::new();
        write_unique_string(&mut prefix_bytes, "").unwrap();
        prefix_bytes.push(0);
        zero_flags(&mut prefix_bytes);
        let mut park_flags = [0_u32; DISTRICT_COUNT];
        park_flags[0] = 1;
        write_encoded_uint_array(&mut prefix_bytes, &park_flags);
        zero_policy_bools(&mut prefix_bytes, version);
        let (prefix, _) =
            DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();

        let mut park = vec![0; 78];
        park.extend_from_slice(&zero_campus(version));
        let (records, used) = prefix.decode_type_zero_park_records(&park).unwrap();
        assert_eq!(used, park.len());
        assert_eq!(records[0].encode(), park);
        assert!(
            prefix
                .decode_type_zero_park_records(&park[..park.len() - 1])
                .is_err()
        );
        park[42] = 15;
        assert!(prefix.decode_type_zero_park_records(&park).is_err());
    }

    #[test]
    fn game_produced_park_records_stop_before_grid_across_version_gates() {
        for (version, park_bytes, visitor_start, last_visitor_offset, bytes) in [
            (
                110_025,
                62,
                20,
                16,
                &include_bytes!("../tests/fixtures/district-park-v110025-windfield.bin")[..],
            ),
            (
                111_013,
                74,
                20,
                16,
                &include_bytes!("../tests/fixtures/district-park-v111013-marble-canyon.bin")[..],
            ),
            (
                114_011,
                1039,
                20,
                16,
                &include_bytes!("../tests/fixtures/district-park-v114011-ccp-map1.bin")[..],
            ),
            (
                115_014,
                1043,
                24,
                20,
                &include_bytes!("../tests/fixtures/district-park-v115014-cape-apple.bin")[..],
            ),
            (
                116_019,
                1055,
                36,
                32,
                &include_bytes!("../tests/fixtures/district-park-v116019-boreal-map1.bin")[..],
            ),
        ] {
            let mut prefix_bytes = Vec::new();
            if version >= 114_006 {
                write_unique_string(&mut prefix_bytes, "").unwrap();
            }
            if version >= 114_007 {
                prefix_bytes.push(0);
            }
            zero_flags(&mut prefix_bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[0] = 1;
            write_encoded_uint_array(&mut prefix_bytes, &park_flags);
            zero_policy_bools(&mut prefix_bytes, version);
            let (prefix, _) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();

            let (records, used) = prefix.decode_type_zero_park_records(bytes).unwrap();
            assert_eq!(used, park_bytes);
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].park_index(), 0);
            assert_eq!(records[0].encode(), bytes[..used]);
            if version == 115_014 {
                let reports = records[0].campus_year_reports().unwrap();
                assert_eq!(reports.len(), 13);
                assert_eq!(reports[0].student_count(), Some(0));
                assert_eq!(reports[0].tickets_income(), Some([0; 5]));
                assert_eq!(reports[0].matches_won(), Some([0; 5]));
                assert_eq!(reports[0].trophies(), Some([false; 5]));
                assert!(reports[0].has_milestone_field());
                assert_eq!(reports[0].milestone_unlocked(), None);
                assert_eq!(reports[0].total_works(), Some(0));
            }
            let visitor_data = records[0].visitor_data().unwrap();
            assert_eq!(
                visitor_data.total_visitors(),
                u32::from_be_bytes(bytes[visitor_start..visitor_start + 4].try_into().unwrap())
            );
            assert_eq!(
                visitor_data.temporary_ticket_income(),
                u32::from_be_bytes(
                    bytes[visitor_start + 4..visitor_start + 8]
                        .try_into()
                        .unwrap()
                )
            );
            assert_eq!(
                visitor_data.final_ticket_income(),
                u32::from_be_bytes(
                    bytes[visitor_start + 8..visitor_start + 12]
                        .try_into()
                        .unwrap()
                )
            );
            assert_eq!(
                visitor_data.ticket_price(),
                u16::from_be_bytes(
                    bytes[visitor_start + 12..visitor_start + 14]
                        .try_into()
                        .unwrap()
                )
            );
            assert_eq!(
                visitor_data.last_visitor_count(),
                Some(u32::from_be_bytes(
                    bytes[last_visitor_offset..last_visitor_offset + 4]
                        .try_into()
                        .unwrap()
                ))
            );
            let mut grid = crate::data_serializer::Reader::new(&bytes[used..]);
            assert_eq!(
                read_encoded_byte_array(&mut grid, 262_144).unwrap(),
                vec![0; 262_144]
            );
            assert_eq!(grid.offset(), 2048);
            assert_eq!(used + grid.offset(), bytes.len());
        }
    }

    #[test]
    fn grid_streams_obey_park_version_gate_and_preserve_exact_controls() {
        // EncodedArray.Byte control 0xff repeats a zero delta 128 times.
        let zero_grid = vec![0xff; 8 * DISTRICT_GRID_CELLS / 128];
        for (version, park_grid_present) in [(110_000, false), (110_001, true)] {
            let mut bytes = zero_grid.clone();
            if park_grid_present {
                bytes.extend_from_slice(&zero_grid);
            }
            bytes.push(0xfe); // First byte after the grid streams.
            let (grids, used) = DistrictManagerGridData::decode(version, &bytes).unwrap();
            assert_eq!(used, bytes.len() - 1);
            assert_eq!(grids.park_grid_present(), park_grid_present);
            assert_eq!(
                grids.district_plane(0).unwrap(),
                vec![0; DISTRICT_GRID_CELLS]
            );
            assert_eq!(
                grids.district_plane(7).unwrap(),
                vec![0; DISTRICT_GRID_CELLS]
            );
            assert_eq!(grids.park_plane(7).is_some(), park_grid_present);
            assert!(grids.district_plane(8).is_none());
            assert!(grids.park_plane(8).is_none());
            assert_eq!(grids.encode(), bytes[..used]);
            assert_eq!(bytes[used], 0xfe);
            assert!(DistrictManagerGridData::decode(version, &bytes[..used - 1]).is_err());
        }
        assert!(DistrictManagerGridData::decode(163, &[]).is_err());
    }

    #[test]
    fn district_tail_headers_follow_version_gates_and_preserve_wire_bytes() {
        for (version, header_count) in [
            (164, 2),
            (110_021, 2),
            (110_022, 4),
            (111_007, 4),
            (111_008, 6),
            (112_015, 6),
            (112_016, 14),
            (114_008, 14),
            (114_009, 16),
            (115_023, 16),
            (115_024, 18),
            (116_015, 18),
            (116_016, 19),
        ] {
            assert_eq!(
                super::district_tail_object_header_count(version),
                header_count
            );
            let mut bytes = vec![0xff; header_count];
            bytes.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
            bytes.extend_from_slice(&0xaabb_ccdd_u32.to_be_bytes());
            if version >= 178 {
                bytes.extend_from_slice(&0x1020_3040_u32.to_be_bytes());
            }
            bytes.push(0xfe);
            write_unique_string(&mut bytes, "DistrictManager").unwrap();
            let used = bytes.len();
            bytes.push(0xaa);
            let (tail, consumed) = DistrictManagerTailData::decode(version, &bytes).unwrap();
            assert_eq!(consumed, used);
            assert_eq!(tail.object_headers().len(), header_count);
            assert!(
                tail.object_headers()
                    .iter()
                    .all(|header| matches!(header, crate::NetObjectHeader::Null))
            );
            assert_eq!(
                tail.next_policy_message_frames(),
                [0x0102_0304, 0xaabb_ccdd]
            );
            assert_eq!(
                tail.district_policies_set(),
                (version >= 178).then_some(0x1020_3040)
            );
            assert_eq!(tail.encode(), bytes[..used]);
            assert_eq!(bytes[used], 0xaa);
            assert!(DistrictManagerTailData::decode(version, &bytes[..used - 1]).is_err());
            let mut wrong = bytes[..used].to_vec();
            wrong[header_count] = 0xff; // Truncates a frame, changing the tail boundary.
            wrong.truncate(header_count + 7);
            assert!(DistrictManagerTailData::decode(version, &wrong).is_err());
        }
        assert!(DistrictManagerTailData::decode(163, &[]).is_err());
    }

    #[test]
    fn game_tail_fixtures_confirm_standard_headers_across_versions() {
        for (version, fixture) in [
            (
                116_020,
                &include_bytes!("../tests/fixtures/district-tail-v116020-desert.bin")[..],
            ),
            (
                117_013,
                &include_bytes!("../tests/fixtures/district-tail-v117013-mp23-map01.bin")[..],
            ),
            (
                118_011,
                &include_bytes!("../tests/fixtures/district-tail-v118011-mp27-map04.bin")[..],
            ),
        ] {
            let (tail, used) = DistrictManagerTailData::decode(version, fixture).unwrap();
            assert_eq!(used, 71);
            assert_eq!(tail.object_headers().len(), 19);
            assert_eq!(tail.next_policy_message_frames(), [0, 0]);
            assert_eq!(tail.district_policies_set(), Some(0));
            assert_eq!(tail.encode(), fixture);
        }
    }

    #[test]
    fn game_excerpts_confirm_following_manager_boundaries() {
        for (version, tail, next, expected_name) in [
            (
                194,
                &include_bytes!("../tests/fixtures/district-tail-v194-co.bin")[..],
                &include_bytes!("../tests/fixtures/district-next-v194-info.bin")[..],
                "InfoManager",
            ),
            (
                116_020,
                &include_bytes!("../tests/fixtures/district-tail-v116020-desert.bin")[..],
                &include_bytes!("../tests/fixtures/district-next-v116020-event.bin")[..],
                "EventManager",
            ),
            (
                117_013,
                &include_bytes!("../tests/fixtures/district-tail-v117013-mp23-map01.bin")[..],
                &include_bytes!("../tests/fixtures/district-next-v117013-event.bin")[..],
                "EventManager",
            ),
            (
                118_011,
                &include_bytes!("../tests/fixtures/district-tail-v118011-mp27-map04.bin")[..],
                &include_bytes!("../tests/fixtures/district-next-v118011-event.bin")[..],
                "EventManager",
            ),
        ] {
            let mut combined = tail.to_vec();
            combined.extend_from_slice(next);
            let (decoded, used) = DistrictManagerTailData::decode(version, &combined).unwrap();
            assert_eq!(used, tail.len());
            assert_eq!(decoded.encode(), tail);
            let mut reader = Reader::new(&combined[used..]);
            let actual_name = reader.read_shared_string(&mut Vec::new()).unwrap();
            assert_eq!(actual_name.as_deref(), Some(expected_name));
            assert_eq!(reader.offset(), next.len());
        }
    }

    #[test]
    fn mp23_and_mp27_park_records_stop_before_both_grids_and_dll_tail() {
        for (version, park, grids, tail, next) in [
            (
                117_013,
                &include_bytes!("../tests/fixtures/district-park-v117013-mp23-map01.bin")[..],
                &include_bytes!("../tests/fixtures/district-grids-v117013-mp23-map01.bin")[..],
                &include_bytes!("../tests/fixtures/district-tail-v117013-mp23-map01.bin")[..],
                &include_bytes!("../tests/fixtures/district-next-v117013-event.bin")[..],
            ),
            (
                118_011,
                &include_bytes!("../tests/fixtures/district-park-v118011-mp27-map04.bin")[..],
                &include_bytes!("../tests/fixtures/district-grids-v118011-mp27-map04.bin")[..],
                &include_bytes!("../tests/fixtures/district-tail-v118011-mp27-map04.bin")[..],
                &include_bytes!("../tests/fixtures/district-next-v118011-event.bin")[..],
            ),
        ] {
            let mut prefix_bytes = Vec::new();
            write_unique_string(&mut prefix_bytes, "").unwrap();
            prefix_bytes.push(0); // Airport panel state.
            zero_flags(&mut prefix_bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[0] = 1;
            write_encoded_uint_array(&mut prefix_bytes, &park_flags);
            zero_policy_bools(&mut prefix_bytes, version);
            let (prefix, _) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &prefix_bytes).unwrap();

            let mut following = park.to_vec();
            following.extend_from_slice(grids);
            following.extend_from_slice(tail);
            following.extend_from_slice(next);
            let (records, park_used) = prefix.decode_park_records(&following).unwrap();
            assert_eq!(park_used, 1_088);
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].encode(), park);
            assert!(prefix.decode_park_records(&park[..park.len() - 1]).is_err());

            let (decoded_grids, grid_used) =
                DistrictManagerGridData::decode(version, &following[park_used..]).unwrap();
            assert_eq!(grid_used, 32_798);
            assert_eq!(decoded_grids.encode(), grids);
            let (decoded_tail, tail_used) =
                DistrictManagerTailData::decode(version, &following[park_used + grid_used..])
                    .unwrap();
            assert_eq!(tail_used, 71);
            assert_eq!(decoded_tail.encode(), tail);
            let mut next_reader = Reader::new(&following[park_used + grid_used + tail_used..]);
            assert_eq!(
                next_reader
                    .read_shared_string(&mut Vec::new())
                    .unwrap()
                    .as_deref(),
                Some("EventManager")
            );
            assert_eq!(next_reader.offset(), next.len());
        }
    }

    #[test]
    fn grid_editor_writes_exact_literals_and_preserves_the_other_stream() {
        let zero_grid = vec![0xff; DISTRICT_GRID_VALUES / 128];
        let mut bytes = zero_grid.clone();
        bytes.extend_from_slice(&zero_grid);
        let (grids, used) = DistrictManagerGridData::decode(110_001, &bytes).unwrap();
        assert_eq!(used, bytes.len());
        let mut editor = grids.into_editor();
        assert_eq!(editor.encode(), bytes);
        let mut plane = vec![0; DISTRICT_GRID_CELLS];
        for (value, byte) in plane.iter_mut().take(128).enumerate() {
            *byte = value as u8;
        }
        assert!(editor.replace_district_plane(8, &plane).is_err());
        assert!(editor.replace_district_plane(0, &plane[..128]).is_err());
        assert_eq!(editor.encode(), bytes);
        editor.replace_district_plane(0, &plane).unwrap();
        assert_eq!(editor.district_plane(0).unwrap(), plane);

        let mut expected = Vec::new();
        for block in 0..DISTRICT_GRID_VALUES / 128 {
            expected.push(0x7f);
            if block == 0 {
                expected.extend(0_u8..128);
            } else {
                expected.extend([0; 128]);
            }
        }
        expected.extend_from_slice(&zero_grid);
        assert_eq!(editor.encode(), expected);
        let (decoded, consumed) = DistrictManagerGridData::decode(110_001, &expected).unwrap();
        assert_eq!(consumed, expected.len());
        assert_eq!(decoded.district_plane(0).unwrap(), plane);
        assert_eq!(decoded.park_plane(0).unwrap(), vec![0; DISTRICT_GRID_CELLS]);

        let (grids, _) = DistrictManagerGridData::decode(110_001, &bytes).unwrap();
        let mut park_editor = grids.into_editor();
        park_editor.replace_park_plane(7, &plane).unwrap();
        let encoded_park = park_editor.encode();
        assert_eq!(&encoded_park[..zero_grid.len()], zero_grid);
        let mut expected_park = zero_grid.clone();
        for block in 0..DISTRICT_GRID_VALUES / 128 {
            expected_park.push(0x7f);
            if block == 7 * DISTRICT_GRID_CELLS / 128 {
                expected_park.extend(0_u8..128);
            } else {
                expected_park.extend([0; 128]);
            }
        }
        assert_eq!(encoded_park, expected_park);
        let (decoded, _) = DistrictManagerGridData::decode(110_001, &encoded_park).unwrap();
        assert_eq!(decoded.park_plane(7).unwrap(), plane);

        let (old_grids, _) = DistrictManagerGridData::decode(110_000, &zero_grid).unwrap();
        assert!(
            old_grids
                .into_editor()
                .replace_park_plane(0, &plane)
                .is_err()
        );
    }

    #[test]
    fn game_produced_grid_streams_have_expected_default_planes() {
        for (version, bytes, expected_len) in [
            (
                112_004,
                &include_bytes!("../tests/fixtures/district-grids-v112004-wolf-creek.bin")[..],
                32_798,
            ),
            (
                117_013,
                &include_bytes!("../tests/fixtures/district-grids-v117013-mp23-map04.bin")[..],
                32_798,
            ),
        ] {
            let (grids, used) = DistrictManagerGridData::decode(version, bytes).unwrap();
            assert_eq!(used, expected_len);
            assert_eq!(grids.encode(), bytes);
            for (index, default_value) in [0, 1, 2, 3, 255, 0, 0, 0].into_iter().enumerate() {
                let district = grids.district_plane(index).unwrap();
                let park = grids.park_plane(index).unwrap();
                assert_eq!(district.len(), DISTRICT_GRID_CELLS);
                assert_eq!(park.len(), DISTRICT_GRID_CELLS);
                assert!(district.iter().all(|value| *value == default_value));
                assert!(park.iter().all(|value| *value == default_value));
            }
        }
    }

    #[test]
    fn edited_game_grid_retains_untouched_park_stream() {
        let bytes = include_bytes!("../tests/fixtures/district-grids-v117013-mp23-map04.bin");
        let (grids, _) = DistrictManagerGridData::decode(117_013, bytes).unwrap();
        let original_park = &bytes[grids.district_encoded_len..];
        let mut editor = grids.into_editor();
        let mut plane = editor.district_plane(0).unwrap().to_vec();
        plane[0] = 42;
        editor.replace_district_plane(0, &plane).unwrap();
        let encoded = editor.encode();
        let (decoded, used) = DistrictManagerGridData::decode(117_013, &encoded).unwrap();
        assert_eq!(used, encoded.len());
        assert_eq!(decoded.district_plane(0).unwrap(), plane);
        assert_eq!(&encoded[decoded.district_encoded_len..], original_park);
        assert!(
            decoded
                .park_plane(7)
                .unwrap()
                .iter()
                .all(|value| *value == 0)
        );
    }

    #[test]
    fn active_district_population_record_obeys_versioned_field_layout() {
        for (version, crime_rate, expected_age_count) in [
            (164, None, 5),
            (194, None, 5),
            (203, Some(41), 5),
            (206, Some(41), 5),
            (207, Some(41), 5),
            (234, Some(41), 5),
            (237, Some(41), 5),
            (239, Some(41), 5),
            (256, Some(41), 5),
            (307, Some(41), 5),
            (109_006, Some(41), 5),
            (109_010, Some(41), 5),
            (111_014, Some(41), 5),
            (112_031, Some(41), 8),
            (113_003, Some(41), 13),
            (113_006, Some(41), 14),
            (113_013, Some(41), 14),
            (116_011, Some(41), 14),
            (117_004, Some(41), 14),
            (117_010, Some(41), 14),
            (117_011, Some(41), 14),
            (117_012, Some(41), 14),
            (117_007, Some(41), 14),
            (117_008, Some(41), 14),
            (117_009, Some(41), 14),
        ] {
            let mut bytes = Vec::new();
            if version >= super::AIRLINE_NAME_VERSION {
                write_unique_string(&mut bytes, "SkyLink").unwrap();
            }
            if version >= super::AIRPORT_PANEL_VERSION {
                bytes.push(0);
            }
            let mut flags = [0_u32; DISTRICT_COUNT];
            flags[17] = 1;
            write_encoded_uint_array(&mut bytes, &flags);
            if version >= super::PARK_FLAGS_VERSION {
                write_encoded_uint_array(&mut bytes, &[0; DISTRICT_COUNT]);
            }
            zero_policy_bools(&mut bytes, version);

            bytes.extend_from_slice(&0x0102_0304_0506_0708_u64.to_be_bytes());
            bytes.extend_from_slice(&[0x01, 0x02, 0x03]);
            bytes.extend_from_slice(&[0x04, 0x05, 0x06]);
            for delta in 0_i16..16 {
                bytes.extend_from_slice(&(delta - 8).to_be_bytes());
            }
            bytes.push(77);
            if let Some(value) = crime_rate {
                bytes.push(value);
            }
            for index in 0..5 {
                append_age_pair(&mut bytes, index as u32 + 1);
            }
            if version >= super::DISTRICT_DETAILED_AGE_VERSION {
                for index in 5..10 {
                    append_age_pair(&mut bytes, index as u32 + 1);
                }
            }
            if version >= super::DISTRICT_DEAD_SENIORS_VERSION {
                append_age_pair(&mut bytes, 11);
                bytes.push(0xF9);
            }
            if version >= super::DISTRICT_ELIGIBLE_AGE_VERSION {
                for index in 11..14 {
                    append_age_pair(&mut bytes, index as u32 + 1);
                }
            }
            let private_kinds = [
                super::DistrictPrivateDataKind::Residential,
                super::DistrictPrivateDataKind::Commercial,
                super::DistrictPrivateDataKind::Visitor,
                super::DistrictPrivateDataKind::Industrial,
                super::DistrictPrivateDataKind::Office,
                super::DistrictPrivateDataKind::Player,
            ];
            let private_size = super::district_private_data_size(version);
            for (index, _) in private_kinds.iter().enumerate() {
                bytes.extend(std::iter::repeat_n(index as u8 + 1, private_size));
            }
            let specialization_kinds = expected_specialization_kinds(version);
            for (index, _) in specialization_kinds.iter().enumerate() {
                bytes.extend(std::iter::repeat_n(index as u8 + 10, 24));
            }
            let demographic_blocks = expected_demographic_blocks(version);
            for (index, (_, byte_count)) in demographic_blocks.iter().enumerate() {
                bytes.extend(std::iter::repeat_n(index as u8 + 20, *byte_count));
            }
            let statistics_blocks = expected_statistics_blocks(version);
            for (index, (_, byte_count)) in statistics_blocks.iter().enumerate() {
                bytes.extend(std::iter::repeat_n(index as u8 + 30, *byte_count));
            }
            let tourism_blocks = expected_tourism_blocks(version);
            for (index, (_, byte_count)) in tourism_blocks.iter().enumerate() {
                bytes.extend(std::iter::repeat_n(index as u8 + 40, *byte_count));
            }
            let subculture_blocks = expected_subculture_blocks(version);
            for (index, (_, byte_count)) in subculture_blocks.iter().enumerate() {
                bytes.extend(std::iter::repeat_n(index as u8 + 50, *byte_count));
            }
            let hotel_size = super::district_hotel_data_size(version);
            let hotel_present = (super::DISTRICT_HOTEL_START_VERSION
                ..super::DISTRICT_HOTEL_END_VERSION)
                .contains(&version);
            if hotel_present {
                bytes.extend(std::iter::repeat_n(60, hotel_size));
            }
            let style_present =
                (version >= super::DISTRICT_STYLE_VERSION).then_some(version >= 239);
            let mut style_string_offset = None;
            if let Some(present) = style_present {
                bytes.push(u8::from(present));
                if present {
                    style_string_offset = Some(bytes.len());
                    write_unique_string(&mut bytes, "District Style").unwrap();
                }
            }
            if version >= super::DISTRICT_LIBRARY_VISITOR_VERSION {
                append_age_pair(&mut bytes, 41);
            }
            let airline = (version >= super::DISTRICT_AIRLINE_VERSION).then_some(-3);
            if let Some(value) = airline {
                bytes.push(value as u8);
            }

            let (prefix, used) =
                DistrictManagerFlagsPrefix::decode_prefix(version, &bytes).unwrap();
            assert_eq!(used, bytes.len());
            assert_eq!(prefix.district_population_records().len(), 1);
            let record = &prefix.district_population_records()[0];
            assert_eq!(record.district_index(), 17);
            assert_eq!(record.random_seed(), 0x0102_0304_0506_0708);
            assert_eq!(record.population_counts(), (0x010203, 0x040506));
            assert_eq!(record.population_deltas(), &(-8_i16..8).collect::<Vec<_>>());
            assert_eq!(record.final_happiness(), 77);
            assert_eq!(record.final_crime_rate(), crime_rate);
            assert_eq!(record.age_populations().len(), expected_age_count);
            assert_eq!(
                record
                    .age_populations()
                    .iter()
                    .map(|age| age.category())
                    .collect::<Vec<_>>(),
                expected_age_categories(version)
            );
            assert_eq!(
                record.last_average_lifespan(),
                (version >= super::DISTRICT_DEAD_SENIORS_VERSION).then_some(-7)
            );
            assert_eq!(record.private_data().len(), private_kinds.len());
            for ((block, expected_kind), index) in
                record.private_data().iter().zip(private_kinds).zip(0_u8..)
            {
                assert_eq!(block.kind(), expected_kind);
                assert_eq!(block.as_bytes(), vec![index + 1; private_size]);
            }
            assert_eq!(record.specializations().len(), specialization_kinds.len());
            for ((block, expected_kind), index) in record
                .specializations()
                .iter()
                .zip(specialization_kinds)
                .zip(0_u8..)
            {
                assert_eq!(block.kind(), expected_kind);
                assert_eq!(block.as_bytes(), vec![index + 10; 24]);
            }
            assert_eq!(record.demographic_data().len(), demographic_blocks.len());
            for ((block, (expected_kind, byte_count)), index) in record
                .demographic_data()
                .iter()
                .zip(demographic_blocks)
                .zip(0_u8..)
            {
                assert_eq!(block.kind(), expected_kind);
                assert_eq!(block.as_bytes(), vec![index + 20; byte_count]);
            }
            assert_eq!(record.statistics().len(), statistics_blocks.len());
            for ((block, (expected_kind, byte_count)), index) in record
                .statistics()
                .iter()
                .zip(statistics_blocks)
                .zip(0_u8..)
            {
                assert_eq!(block.kind(), expected_kind);
                assert_eq!(block.as_bytes(), vec![index + 30; byte_count]);
            }
            assert_eq!(record.tourism().len(), tourism_blocks.len());
            for ((block, (expected_kind, byte_count)), index) in
                record.tourism().iter().zip(tourism_blocks).zip(0_u8..)
            {
                assert_eq!(block.kind(), expected_kind);
                assert_eq!(block.as_bytes(), vec![index + 40; byte_count]);
            }
            assert_eq!(record.subcultures().len(), subculture_blocks.len());
            for ((block, (expected_kind, byte_count)), index) in record
                .subcultures()
                .iter()
                .zip(subculture_blocks)
                .zip(0_u8..)
            {
                assert_eq!(block.kind(), expected_kind);
                assert_eq!(block.as_bytes(), vec![index + 50; byte_count]);
            }
            assert_eq!(
                record.hotel_data().map(|block| block.as_bytes()),
                hotel_present.then(|| vec![60; hotel_size]).as_deref()
            );
            assert_eq!(record.district_style_present(), style_present);
            assert_eq!(
                record.district_style_name(),
                (style_present == Some(true)).then_some("District Style")
            );
            assert_eq!(
                record
                    .library_visitor_data()
                    .map(|age| (age.category(), age.counts())),
                (version >= super::DISTRICT_LIBRARY_VISITOR_VERSION)
                    .then_some((super::DistrictAgeCategory::LibraryVisitors, (41, 141)))
            );
            assert_eq!(record.airline(), airline);
            assert_eq!(prefix.encode(), bytes);

            let mut truncated = bytes.clone();
            truncated.pop();
            assert!(DistrictManagerFlagsPrefix::decode_prefix(version, &truncated).is_err());
            if let Some(string_offset) = style_string_offset {
                let mut truncated_style = bytes.clone();
                truncated_style.truncate(string_offset + 3);
                assert!(
                    DistrictManagerFlagsPrefix::decode_prefix(version, &truncated_style).is_err()
                );
            }
        }
    }

    fn append_age_pair(output: &mut Vec<u8>, value: u32) {
        for count in [value, value + 100] {
            output.extend_from_slice(&[(count >> 16) as u8, (count >> 8) as u8, count as u8]);
        }
    }

    fn expected_age_categories(version: u32) -> Vec<super::DistrictAgeCategory> {
        use super::DistrictAgeCategory as Category;

        let mut categories = vec![
            Category::Children,
            Category::Teens,
            Category::Young,
            Category::Adults,
            Category::Seniors,
        ];
        if version >= super::DISTRICT_DETAILED_AGE_VERSION {
            categories.extend([
                Category::ChildHealth,
                Category::ChildSickness,
                Category::SeniorHealth,
                Category::SeniorSickness,
                Category::AgeAtDeath,
            ]);
        }
        if version >= super::DISTRICT_DEAD_SENIORS_VERSION {
            categories.push(Category::DeadSeniors);
        }
        if version >= super::DISTRICT_ELIGIBLE_AGE_VERSION {
            categories.extend([
                Category::ElementaryEligible,
                Category::HighschoolEligible,
                Category::UniversityEligible,
            ]);
        }
        categories
    }

    fn expected_statistics_blocks(version: u32) -> Vec<(super::DistrictStatisticsKind, usize)> {
        use super::DistrictStatisticsKind as Kind;

        let mut blocks = Vec::new();
        if version >= super::DISTRICT_PRODUCTION_VERSION {
            blocks.push((
                Kind::Production,
                super::district_production_data_size(version),
            ));
            let consumption_size = super::district_consumption_data_size(version);
            blocks.extend(
                [
                    Kind::PlayerConsumption,
                    Kind::ResidentialConsumption,
                    Kind::CommercialConsumption,
                    Kind::IndustrialConsumption,
                    Kind::OfficeConsumption,
                ]
                .map(|kind| (kind, consumption_size)),
            );
        }
        if version >= super::DISTRICT_USAGE_VERSION {
            blocks.push((Kind::Usage, super::DISTRICT_USAGE_BYTES));
        }
        if version >= super::DISTRICT_GROUND_VERSION {
            blocks.push((Kind::Ground, super::DISTRICT_GROUND_BYTES));
        }
        if version >= super::DISTRICT_RESOURCE_VERSION {
            let resource_size = super::district_resource_data_size(version);
            blocks.push((Kind::ImportResources, resource_size));
            blocks.push((Kind::ExportResources, resource_size));
        }
        blocks
    }

    fn expected_tourism_blocks(version: u32) -> Vec<(super::DistrictTourismKind, usize)> {
        use super::DistrictTourismKind as Kind;

        if version < super::DISTRICT_TOURIST_VERSION {
            return Vec::new();
        }
        let byte_count = super::district_tourist_data_size(version);
        [Kind::Tourist1, Kind::Tourist2, Kind::Tourist3]
            .map(|kind| (kind, byte_count))
            .to_vec()
    }

    fn expected_subculture_blocks(version: u32) -> Vec<(super::DistrictSubCultureKind, usize)> {
        use super::DistrictSubCultureKind as Kind;

        if version < super::DISTRICT_SUBCULTURE_VERSION {
            return Vec::new();
        }
        [Kind::Hippie, Kind::Hipster, Kind::Redneck, Kind::Gangsta]
            .map(|kind| (kind, super::DISTRICT_SUBCULTURE_BYTES))
            .to_vec()
    }

    fn expected_specialization_kinds(version: u32) -> Vec<super::DistrictSpecializationKind> {
        use super::DistrictSpecializationKind as Kind;

        let mut kinds = if version >= super::DISTRICT_SPECIALIZATION_VERSION {
            vec![
                Kind::Farming,
                Kind::Forestry,
                Kind::Ore,
                Kind::Oil,
                Kind::Leisure,
                Kind::Tourist,
            ]
        } else {
            Vec::new()
        };
        if version >= super::DISTRICT_ADDITIONAL_SPECIALIZATION_VERSION {
            kinds.extend([Kind::Organic, Kind::SelfSufficient, Kind::HighTech]);
        }
        kinds
    }

    fn expected_demographic_blocks(version: u32) -> Vec<(super::DistrictDemographicKind, usize)> {
        use super::DistrictDemographicKind as Kind;

        let mut blocks = Vec::new();
        if version >= super::DISTRICT_BIRTH_DEATH_VERSION {
            blocks.extend([(Kind::Births, 6), (Kind::Deaths, 6)]);
        }
        if version >= super::DISTRICT_EDUCATION_DATA_VERSION {
            let bytes = if version >= super::DISTRICT_EDUCATION_WORKER_VERSION {
                24
            } else {
                18
            };
            blocks.extend([
                (Kind::Educated0, bytes),
                (Kind::Educated1, bytes),
                (Kind::Educated2, bytes),
                (Kind::Educated3, bytes),
            ]);
        }
        if version >= super::DISTRICT_EDUCATION_AGE_VERSION {
            blocks.extend([
                (Kind::Education1, 6),
                (Kind::Education2, 6),
                (Kind::Education3, 6),
            ]);
        }
        if version >= super::DISTRICT_STUDENT_AGE_VERSION {
            blocks.extend([
                (Kind::Student1, 6),
                (Kind::Student2, 6),
                (Kind::Student3, 6),
            ]);
        }
        blocks
    }
}
