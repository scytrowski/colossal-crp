use crate::CrpError;
use crate::data_serializer::Reader;
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

const FIRST_CURRENT_LAYOUT_VERSION: u32 = 127;
const EVENT_POLICY_VERSION: u32 = 248;
const SCENARIO_TRIGGER_VERSION: u32 = 279;
const PARK_POLICY_VERSION: u32 = 110_009;
const SCENARIO_GOALS_GUIDE_VERSION: u32 = 295;

/// A versioned milestone-reference array written by `UnlockManager+Data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnlockManagerMilestoneKind {
    /// Progression milestones.
    Progression,
    /// Feature milestones.
    Feature,
    /// Area milestones.
    Area,
    /// Zone milestones.
    Zone,
    /// Service milestones.
    Service,
    /// Subservice milestones.
    Subservice,
    /// Policy-type milestones.
    PolicyType,
    /// Industry-policy milestones.
    IndustryPolicy,
    /// Service-policy milestones.
    ServicePolicy,
    /// Taxation-policy milestones.
    TaxationPolicy,
    /// City-planning-policy milestones.
    CityPlanningPolicy,
    /// Special-policy milestones.
    SpecialPolicy,
    /// Info-mode milestones.
    InfoMode,
    /// Achievement milestones.
    Achievement,
    /// Event-policy milestones.
    EventPolicy,
    /// Park-policy milestones.
    ParkPolicy,
}

/// One milestone array and its unresolved DataSerializer object references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlockManagerMilestoneArray {
    kind: UnlockManagerMilestoneKind,
    references: Vec<NetObjectHeader>,
}

impl UnlockManagerMilestoneArray {
    /// Returns the semantic array represented by these references.
    pub fn kind(&self) -> UnlockManagerMilestoneKind {
        self.kind
    }

    /// Returns the reference headers in their serialized order.
    pub fn references(&self) -> &[NetObjectHeader] {
        &self.references
    }
}

/// Version-gated milestone references stored in `UnlockManager+Data`.
///
/// The decoder preserves the serialized body for exact unchanged writing. It
/// supports the current reader layout from DataSerializer version 127, except
/// for the historical scenario-condition layout in versions 262–278 and
/// non-empty inline scenario-trigger data from version 279. Object references
/// remain unresolved because locating their queued bodies requires replaying
/// the DataSerializer object list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlockManagerReferences {
    format_version: u32,
    arrays: Vec<UnlockManagerMilestoneArray>,
    scenario_trigger_count: Option<u32>,
    scenario_trigger_references: Vec<NetObjectHeader>,
    milestones_not_used: Option<NetObjectHeader>,
    scenario_goals_not_used: Option<NetObjectHeader>,
    profile_end_shared_string_index: u16,
    serialized_body: Vec<u8>,
}

impl UnlockManagerReferences {
    /// Returns the DataSerializer version used for the field gates.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns version-present milestone arrays in serialized order.
    pub fn arrays(&self) -> &[UnlockManagerMilestoneArray] {
        &self.arrays
    }

    /// Returns the scenario-trigger count for version 279 and newer.
    pub fn scenario_trigger_count(&self) -> Option<u32> {
        self.scenario_trigger_count
    }

    /// Returns unresolved scenario-trigger milestone references.
    pub fn scenario_trigger_references(&self) -> &[NetObjectHeader] {
        &self.scenario_trigger_references
    }

    /// Returns the milestones-not-used guide reference when present.
    pub fn milestones_not_used(&self) -> Option<&NetObjectHeader> {
        self.milestones_not_used.as_ref()
    }

    /// Returns the scenario-goals-not-used guide reference when present.
    pub fn scenario_goals_not_used(&self) -> Option<&NetObjectHeader> {
        self.scenario_goals_not_used.as_ref()
    }

    /// Returns the stored profiler end index without resolving the string table.
    pub fn profile_end_shared_string_index(&self) -> u16 {
        self.profile_end_shared_string_index
    }

    /// Returns the decoded fields and profiler index byte for byte.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }

    pub(crate) fn decode(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < FIRST_CURRENT_LAYOUT_VERSION {
            return Err(invalid(
                0,
                format!(
                    "UnlockManager version {format_version} predates the supported milestone-array layout"
                ),
            ));
        }
        if (262..SCENARIO_TRIGGER_VERSION).contains(&format_version) {
            return Err(invalid(
                0,
                format!(
                    "UnlockManager scenario-condition layout is unsupported at version {format_version}"
                ),
            ));
        }

        let mut reader = Reader::new(bytes);
        let mut arrays = Vec::new();
        for kind in milestone_kinds(format_version) {
            let count = usize::from(reader.read_u16_be()?);
            if count > reader.remaining() {
                return Err(reader.invalid(format!(
                    "UnlockManager {kind:?} count {count} cannot fit in remaining bytes"
                )));
            }
            let mut references = Vec::with_capacity(count);
            for _ in 0..count {
                references.push(read_net_object_header(&mut reader)?);
            }
            arrays.push(UnlockManagerMilestoneArray { kind, references });
        }

        let (scenario_trigger_count, scenario_trigger_references) = if format_version
            >= SCENARIO_TRIGGER_VERSION
        {
            let count = reader.read_i32_be()?;
            if count < 0 {
                return Err(reader.invalid("scenario-trigger count is negative"));
            }
            let reference_count = usize::from(reader.read_u16_be()?);
            if reference_count > reader.remaining() {
                return Err(reader.invalid(format!(
                        "scenario-trigger reference count {reference_count} cannot fit in remaining bytes"
                    )));
            }
            let mut references = Vec::with_capacity(reference_count);
            for _ in 0..reference_count {
                references.push(read_net_object_header(&mut reader)?);
            }
            if count != 0 || reference_count != 0 {
                return Err(reader
                    .invalid("non-empty scenario-trigger data uses an unsupported inline layout"));
            }
            (Some(0), references)
        } else {
            (None, Vec::new())
        };

        let milestones_not_used = if format_version >= 134 {
            Some(read_net_object_header(&mut reader)?)
        } else {
            None
        };
        let scenario_goals_not_used = if format_version >= SCENARIO_GOALS_GUIDE_VERSION {
            Some(read_net_object_header(&mut reader)?)
        } else {
            None
        };
        let profile_end_shared_string_index = reader.read_u16_be()?;
        let consumed = reader.offset();

        Ok((
            Self {
                format_version,
                arrays,
                scenario_trigger_count,
                scenario_trigger_references,
                milestones_not_used,
                scenario_goals_not_used,
                profile_end_shared_string_index,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }
}

fn milestone_kinds(format_version: u32) -> Vec<UnlockManagerMilestoneKind> {
    use UnlockManagerMilestoneKind as Kind;

    let mut kinds = vec![
        Kind::Progression,
        Kind::Feature,
        Kind::Area,
        Kind::Zone,
        Kind::Service,
        Kind::Subservice,
        Kind::PolicyType,
        Kind::IndustryPolicy,
        Kind::ServicePolicy,
        Kind::TaxationPolicy,
        Kind::CityPlanningPolicy,
        Kind::SpecialPolicy,
        Kind::InfoMode,
    ];
    if format_version >= 177 {
        kinds.push(Kind::Achievement);
    }
    if format_version >= EVENT_POLICY_VERSION {
        kinds.push(Kind::EventPolicy);
    }
    if format_version >= PARK_POLICY_VERSION {
        kinds.push(Kind::ParkPolicy);
    }
    kinds
}

fn invalid(offset: usize, message: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "UnlockManager+Data".to_owned(),
        offset,
        reason: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{UnlockManagerMilestoneKind as Kind, UnlockManagerReferences};
    use crate::net_manager_data::NetObjectHeader;

    fn empty_unlock_body(version: u32, array_count: usize, with_trigger_fields: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        for _ in 0..array_count {
            bytes.extend_from_slice(&0_u16.to_be_bytes());
        }
        if with_trigger_fields {
            bytes.extend_from_slice(&0_i32.to_be_bytes());
            bytes.extend_from_slice(&0_u16.to_be_bytes());
        }
        if version >= 134 {
            bytes.push(0xff);
        }
        if version >= 295 {
            bytes.push(0xff);
        }
        bytes.extend_from_slice(&0x1234_u16.to_be_bytes());
        bytes
    }

    #[test]
    fn milestone_arrays_follow_version_gates_and_preserve_raw_references() {
        let bytes = empty_unlock_body(248, 15, false);
        let (record, consumed) = UnlockManagerReferences::decode(248, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(record.arrays().len(), 15);
        assert_eq!(record.arrays()[0].kind(), Kind::Progression);
        assert_eq!(record.arrays()[12].kind(), Kind::InfoMode);
        assert_eq!(record.arrays()[13].kind(), Kind::Achievement);
        assert_eq!(record.arrays()[14].kind(), Kind::EventPolicy);
        assert!(
            record
                .arrays()
                .iter()
                .all(|array| array.references().is_empty())
        );
        assert_eq!(record.milestones_not_used(), Some(&NetObjectHeader::Null));
        assert_eq!(record.scenario_goals_not_used(), None);
        assert_eq!(record.profile_end_shared_string_index(), 0x1234);
        assert_eq!(record.encode(), bytes);
    }

    #[test]
    fn scenario_and_park_gates_are_explicit() {
        let before_event_policy = empty_unlock_body(247, 14, false);
        let (record, consumed) =
            UnlockManagerReferences::decode(247, &before_event_policy).unwrap();
        assert_eq!(consumed, before_event_policy.len());
        assert_eq!(record.arrays().len(), 14);
        assert_eq!(record.arrays().last().unwrap().kind(), Kind::Achievement);

        let scenario = empty_unlock_body(295, 15, true);
        let (record, consumed) = UnlockManagerReferences::decode(295, &scenario).unwrap();
        assert_eq!(consumed, scenario.len());
        assert_eq!(record.scenario_trigger_count(), Some(0));
        assert!(record.scenario_trigger_references().is_empty());
        assert_eq!(
            record.scenario_goals_not_used(),
            Some(&NetObjectHeader::Null)
        );

        let before_park_policy = empty_unlock_body(110_008, 15, true);
        let (record, consumed) =
            UnlockManagerReferences::decode(110_008, &before_park_policy).unwrap();
        assert_eq!(consumed, before_park_policy.len());
        assert_eq!(record.arrays().len(), 15);
        assert_eq!(record.arrays().last().unwrap().kind(), Kind::EventPolicy);

        let park = empty_unlock_body(110_009, 16, true);
        let (record, consumed) = UnlockManagerReferences::decode(110_009, &park).unwrap();
        assert_eq!(consumed, park.len());
        assert_eq!(record.arrays().last().unwrap().kind(), Kind::ParkPolicy);
    }

    #[test]
    fn nonempty_milestone_reference_headers_are_retained() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.push(0xff);
        for _ in 1..15 {
            bytes.extend_from_slice(&0_u16.to_be_bytes());
        }
        bytes.push(0xff);
        bytes.extend_from_slice(&0x1234_u16.to_be_bytes());

        let (record, consumed) = UnlockManagerReferences::decode(248, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(record.arrays()[0].references(), &[NetObjectHeader::Null]);
        assert_eq!(record.encode(), bytes);
    }

    #[test]
    fn wolf_creek_fixture_preserves_guide_headers_and_end_index() {
        const FIXTURE: &[u8] =
            include_bytes!("../tests/fixtures/unlock-manager-v112004-wolf-creek.bin");

        let (record, consumed) = UnlockManagerReferences::decode(112_004, FIXTURE).unwrap();
        assert_eq!(consumed, FIXTURE.len());
        assert_eq!(record.arrays().len(), 16);
        assert!(
            record
                .arrays()
                .iter()
                .all(|array| array.references().is_empty())
        );
        assert_eq!(record.scenario_trigger_count(), Some(0));
        assert!(record.scenario_trigger_references().is_empty());
        assert_eq!(
            record.milestones_not_used(),
            Some(&NetObjectHeader::NewSharedType(30))
        );
        assert_eq!(
            record.scenario_goals_not_used(),
            Some(&NetObjectHeader::NewSharedType(30))
        );
        assert_eq!(record.profile_end_shared_string_index(), 27);
        assert_eq!(record.encode(), FIXTURE);
    }

    #[test]
    fn unsupported_historical_and_active_scenario_layouts_are_errors() {
        assert!(UnlockManagerReferences::decode(126, &[]).is_err());
        assert!(UnlockManagerReferences::decode(270, &[]).is_err());

        let mut active = empty_unlock_body(295, 15, false);
        active[15 * 2..15 * 2 + 4].copy_from_slice(&1_u32.to_be_bytes());
        assert!(UnlockManagerReferences::decode(295, &active).is_err());

        let mut negative = empty_unlock_body(295, 15, false);
        negative[15 * 2..15 * 2 + 4].copy_from_slice(&(-1_i32).to_be_bytes());
        assert!(UnlockManagerReferences::decode(295, &negative).is_err());
        assert!(UnlockManagerReferences::decode(248, &[0; 2]).is_err());
    }
}
