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
const LEGACY_POLICY_BOOL_VERSION: u32 = 43;
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
const POLICY_COUNT: usize = 32;
const POPULATION_DELTA_COUNT: usize = 16;
const PRIVATE_DATA_BASE_BYTES: usize = 21;
const PRIVATE_DATA_LEVEL_BYTES: usize = 20;
const PRIVATE_DATA_HEALTH_BYTES: usize = 5;
const PRIVATE_DATA_BUILDING_BYTES: usize = 18;
const PRIVATE_DATA_ALIVE_BYTES: usize = 6;
const PRIVATE_DATA_CRIME_BYTES: usize = 5;
const PRIVATE_DATA_COLLAPSED_BYTES: usize = 4;

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
}

/// A named pair of temporary and final age-group counts in a district.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistrictAgePopulation {
    category: DistrictAgeCategory,
    temporary: u32,
    final_count: u32,
}

impl DistrictAgePopulation {
    /// Returns the age or health category represented by this pair.
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
}

impl DistrictManagerFlagsPrefix {
    /// Decodes the opening fields and both known policy bool streams.
    ///
    /// This prefix supports inner versions 164 and newer. It stops before the
    /// player and specialization data that follow the district private-data blocks.
    /// Truncated or malformed fields return an error.
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
        let legacy_bool_count =
            DISTRICT_COUNT
                .checked_mul(POLICY_COUNT)
                .ok_or(CrpError::ArithmeticOverflow(
                    "DistrictManager legacy policy bool count",
                ))?;
        let legacy_policy_bools = if format_version >= LEGACY_POLICY_BOOL_VERSION {
            Some(read_encoded_bool_array(&mut reader, legacy_bool_count)?)
        } else {
            None
        };
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
            let mut private_data = Vec::with_capacity(5);
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

    /// Returns a later 32-by-128 bool table that `Deserialize` reads and discards.
    pub fn legacy_policy_bools(&self) -> Option<&[bool]> {
        self.legacy_policy_bools.as_deref()
    }

    /// Returns decoded population fields for districts with nonzero flags.
    pub fn district_population_records(&self) -> &[DistrictPopulationRecord] {
        &self.district_population_records
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
    use crate::encoded_array::{write_encoded_bool_array, write_encoded_uint_array};

    fn zero_flags(output: &mut Vec<u8>) {
        write_encoded_uint_array(output, &[0; 128]);
    }

    fn zero_policy_bools(output: &mut Vec<u8>, version: u32) {
        let count = district_policy_bool_count(version).unwrap();
        if count != 0 {
            write_encoded_bool_array(output, &vec![false; count]);
        }
        if version >= super::LEGACY_POLICY_BOOL_VERSION {
            write_encoded_bool_array(output, &vec![false; POLICY_COUNT * 128]);
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
        write_encoded_bool_array(&mut bytes, &vec![false; POLICY_COUNT * 128]);

        let (prefix, used) = DistrictManagerFlagsPrefix::decode_prefix(version, &bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(prefix.policy_bools().unwrap(), policies);
        assert_eq!(
            prefix.legacy_policy_bools().unwrap().len(),
            POLICY_COUNT * 128
        );
        assert_eq!(prefix.encode(), bytes);
    }

    #[test]
    fn active_district_population_record_obeys_crime_rate_gate() {
        for (version, crime_rate, expected_age_count) in [
            (194, None, 5),
            (203, Some(41), 5),
            (112_031, Some(41), 8),
            (113_003, Some(41), 13),
            (113_006, Some(41), 14),
        ] {
            let mut bytes = Vec::new();
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
            ];
            let private_size = super::district_private_data_size(version);
            for (index, _) in private_kinds.iter().enumerate() {
                bytes.extend(std::iter::repeat_n(index as u8 + 1, private_size));
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
            assert_eq!(prefix.encode(), bytes);

            let mut truncated = bytes.clone();
            truncated.pop();
            assert!(DistrictManagerFlagsPrefix::decode_prefix(version, &truncated).is_err());
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
}
