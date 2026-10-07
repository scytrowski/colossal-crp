use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_bool_array, read_encoded_uint_array};

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
const TYPE_ZERO_PARK_FIRST_VERSION: u32 = 111_015;
const TYPE_ZERO_PARK_LAST_VERSION: u32 = 114_004;
const TYPE_ZERO_PARK_BYTES: usize = 78;
const TYPE_ZERO_PARK_TYPE_OFFSET: usize = 42;
const TYPE_ZERO_PARK_LEVEL_OFFSET: usize = 43;

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

/// A park type 0 record from inner versions 111015–114004.
///
/// Only the leading scalar fields are interpreted. The exact record bytes are
/// retained for unchanged writing, including fields not yet modeled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictParkRecord {
    park_index: usize,
    random_seed: u64,
    main_gate: u16,
    condition: u16,
    level: u8,
    serialized: [u8; TYPE_ZERO_PARK_BYTES],
}

impl DistrictParkRecord {
    /// Returns the index in the 128-entry park buffer.
    pub fn park_index(&self) -> usize {
        self.park_index
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

    /// Writes the exact record bytes, including fields not yet modeled.
    pub fn encode(&self) -> [u8; TYPE_ZERO_PARK_BYTES] {
        self.serialized
    }
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

    /// Decodes park type 0 records in buffer order from the bytes after this prefix.
    ///
    /// This fixed layout is supported for inner versions 111015–114004 only.
    /// Other versions or park types return an error. Each record preserves its
    /// original bytes for exact unchanged writing. The returned byte count
    /// ends before the district grid.
    pub fn decode_type_zero_park_records(
        &self,
        bytes: &[u8],
    ) -> Result<(Vec<DistrictParkRecord>, usize), CrpError> {
        if !(TYPE_ZERO_PARK_FIRST_VERSION..=TYPE_ZERO_PARK_LAST_VERSION)
            .contains(&self.format_version)
        {
            return Err(invalid_district_manager(
                0,
                format!(
                    "park type 0 record layout is unsupported at inner version {}",
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
            let end =
                offset
                    .checked_add(TYPE_ZERO_PARK_BYTES)
                    .ok_or(CrpError::ArithmeticOverflow(
                        "DistrictManager park record end",
                    ))?;
            let serialized: [u8; TYPE_ZERO_PARK_BYTES] = bytes
                .get(offset..end)
                .ok_or_else(|| {
                    invalid_district_manager(
                        offset,
                        format!("park {park_index} record is truncated"),
                    )
                })?
                .try_into()
                .map_err(|_| invalid_district_manager(offset, "invalid park record length"))?;
            let park_type = serialized[TYPE_ZERO_PARK_TYPE_OFFSET];
            if park_type != 0 {
                return Err(invalid_district_manager(
                    offset + TYPE_ZERO_PARK_TYPE_OFFSET,
                    format!("park {park_index} has unsupported type {park_type}"),
                ));
            }
            let mut reader = Reader::new(&serialized);
            let random_seed = reader.read_u64_be()?;
            let main_gate = reader.read_u16_be()?;
            let condition = reader.read_u16_be()?;
            records.push(DistrictParkRecord {
                park_index,
                random_seed,
                main_gate,
                condition,
                level: serialized[TYPE_ZERO_PARK_LEVEL_OFFSET],
                serialized,
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
        DISTRICT_COUNT, DistrictManagerFlagsPrefix, POLICY_COUNT, district_policy_bool_count,
    };
    use crate::data_serializer::write_unique_string;
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
    fn type_zero_park_record_version_boundaries_are_explicit() {
        for (version, supported) in [
            (111_014, false),
            (111_015, true),
            (114_004, true),
            (114_005, false),
        ] {
            let mut bytes = Vec::new();
            zero_flags(&mut bytes);
            let mut park_flags = [0_u32; DISTRICT_COUNT];
            park_flags[0] = 1;
            write_encoded_uint_array(&mut bytes, &park_flags);
            zero_policy_bools(&mut bytes, version);
            let (prefix, _) = DistrictManagerFlagsPrefix::decode_prefix(version, &bytes).unwrap();
            let result = prefix.decode_type_zero_park_records(&[0; 78]);
            if supported {
                assert_eq!(result.unwrap().1, 78);
            } else {
                assert!(result.is_err());
            }
        }
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
