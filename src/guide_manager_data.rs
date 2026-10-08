use crate::CrpError;
use crate::data_serializer::Reader;
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

/// The meaning of one versioned reference in `GuideManager+Data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideManagerReferenceField {
    /// Entry `i` in `m_serviceNotUsed`.
    ServiceNotUsed(usize),
    /// Entry `i` in `m_serviceNeeded`.
    ServiceNeeded(usize),
    /// One of the four camera guide references, in serialized order.
    Camera(usize),
    /// `m_speedNotUsed`.
    SpeedNotUsed,
    /// `m_speedNotUsed2`.
    SpeedNotUsed2,
    /// `m_infoviewNotUsed`.
    InfoviewNotUsed,
    /// `m_notEnoughMoney`.
    NotEnoughMoney,
    /// `m_deathCareNeeded`.
    DeathCareNeeded,
    /// `m_bulldozerNotUsed`.
    BulldozerNotUsed,
    /// `m_worldInfoNotUsed`.
    WorldInfoNotUsed,
    /// `m_renameNotUsed`.
    RenameNotUsed,
    /// `m_zonedBuildingInfo`.
    ZonedBuildingInfo,
    /// `m_teamColorNotUsed`.
    TeamColorNotUsed,
    /// `m_localOnlyStation`.
    LocalOnlyStation,
    /// `m_routeButton`.
    RouteButton,
    /// `m_undergroundBulldozer`.
    UndergroundBulldozer,
    /// `m_roadNamesOpen` in its disjoint version ranges.
    RoadNamesOpen,
    /// `m_festivalColorNotUsed` in its disjoint version ranges.
    FestivalColorNotUsed,
    /// `m_ticketPricesNotUsed` in its disjoint version ranges.
    TicketPricesNotUsed,
}

/// One field and its DataSerializer object-reference header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideManagerReference {
    field: GuideManagerReferenceField,
    header: NetObjectHeader,
}

impl GuideManagerReference {
    /// Returns the field represented by this header.
    pub fn field(&self) -> GuideManagerReferenceField {
        self.field
    }

    /// Returns the parsed header without resolving shared-type or object IDs.
    ///
    /// Resolving those IDs requires replaying every preceding queued object.
    pub fn header(&self) -> &NetObjectHeader {
        &self.header
    }
}

/// Versioned `GuideManager+Data` reference fields.
///
/// The decoder retains the exact serialized fields and profiler end index for
/// unchanged writing. It does not resolve referenced object types or locate
/// their queued bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideManagerReferences {
    format_version: u32,
    references: Vec<GuideManagerReference>,
    serialized_body: Vec<u8>,
}

impl GuideManagerReferences {
    /// Returns the inner DataSerializer version used for the field gates.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns reference headers in the order written by `GuideManager+Data`.
    pub fn references(&self) -> &[GuideManagerReference] {
        &self.references
    }

    /// Returns the body fields and profiler end index byte for byte.
    ///
    /// This is the body after the manager's profiler begin tag; it does not
    /// include that begin tag.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }

    pub(crate) fn decode(
        format_version: u32,
        expected_profile_index: u16,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        let fields = reference_fields(format_version);
        let mut reader = Reader::new(bytes);
        let mut references = Vec::with_capacity(fields.len());
        for field in fields {
            references.push(GuideManagerReference {
                field,
                header: read_net_object_header(&mut reader)?,
            });
        }
        let profile_end = reader.read_u16_be()?;
        if profile_end != expected_profile_index {
            return Err(reader.invalid(format!(
                "GuideManager end tag refers to profiler string {profile_end}, expected {expected_profile_index}"
            )));
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                references,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }
}

fn reference_fields(format_version: u32) -> Vec<GuideManagerReferenceField> {
    let service_count = if format_version >= 120_002 {
        20
    } else if format_version >= 117_001 {
        19
    } else if format_version >= 115_005 {
        18
    } else if format_version >= 113_000 {
        17
    } else if format_version >= 112_019 {
        16
    } else if format_version >= 112_010 {
        15
    } else if format_version >= 112_002 {
        14
    } else if format_version >= 111_006 {
        13
    } else if format_version >= 82 {
        12
    } else {
        0
    };
    let mut fields = Vec::new();
    for index in 0..service_count {
        fields.push(GuideManagerReferenceField::ServiceNotUsed(index));
        fields.push(GuideManagerReferenceField::ServiceNeeded(index));
    }
    if format_version >= 84 {
        fields.extend((0..4).map(GuideManagerReferenceField::Camera));
    }
    if format_version >= 94 {
        fields.push(GuideManagerReferenceField::SpeedNotUsed);
    }
    if format_version >= 174 {
        fields.push(GuideManagerReferenceField::SpeedNotUsed2);
    }
    if format_version >= 107 {
        fields.push(GuideManagerReferenceField::InfoviewNotUsed);
    }
    if format_version >= 114 {
        fields.push(GuideManagerReferenceField::NotEnoughMoney);
    }
    if format_version >= 134 {
        fields.push(GuideManagerReferenceField::DeathCareNeeded);
        fields.push(GuideManagerReferenceField::BulldozerNotUsed);
    }
    if format_version >= 156 {
        fields.push(GuideManagerReferenceField::WorldInfoNotUsed);
    }
    if format_version >= 160 {
        fields.push(GuideManagerReferenceField::RenameNotUsed);
    }
    if format_version >= 198 {
        fields.push(GuideManagerReferenceField::ZonedBuildingInfo);
    }
    if format_version >= 253 {
        fields.push(GuideManagerReferenceField::TeamColorNotUsed);
    }
    if format_version >= 316 {
        fields.push(GuideManagerReferenceField::LocalOnlyStation);
    }
    if format_version >= 318 {
        fields.push(GuideManagerReferenceField::RouteButton);
        fields.push(GuideManagerReferenceField::UndergroundBulldozer);
    }
    if road_names_enabled(format_version) {
        fields.push(GuideManagerReferenceField::RoadNamesOpen);
    }
    if festival_and_ticket_enabled(format_version) {
        fields.push(GuideManagerReferenceField::FestivalColorNotUsed);
        fields.push(GuideManagerReferenceField::TicketPricesNotUsed);
    }
    fields
}

fn road_names_enabled(format_version: u32) -> bool {
    (108_009..109_000).contains(&format_version) || format_version >= 109_009
}

fn festival_and_ticket_enabled(format_version: u32) -> bool {
    (108_011..109_000).contains(&format_version) || format_version >= 109_011
}

#[cfg(test)]
mod tests {
    use super::{GuideManagerReferenceField, GuideManagerReferences, reference_fields};
    use crate::net_manager_data::NetObjectHeader;

    #[test]
    fn reference_fields_follow_independent_version_gates() {
        for (version, expected_count) in [
            (81, 0),
            (82, 24),
            (111_005, 44),
            (111_006, 46),
            (112_002, 48),
            (108_009, 42),
            (109_000, 41),
            (109_009, 42),
            (109_010, 42),
            (109_011, 44),
            (112_004, 48),
            (120_002, 60),
        ] {
            assert_eq!(
                reference_fields(version).len(),
                expected_count,
                "v{version}"
            );
        }

        assert_eq!(
            reference_fields(112_004).first(),
            Some(&GuideManagerReferenceField::ServiceNotUsed(0))
        );
        assert_eq!(
            reference_fields(112_004).get(1),
            Some(&GuideManagerReferenceField::ServiceNeeded(0))
        );
        assert_eq!(
            reference_fields(112_004).get(47),
            Some(&GuideManagerReferenceField::TicketPricesNotUsed)
        );
        assert!(!reference_fields(109_000).contains(&GuideManagerReferenceField::RoadNamesOpen));
        assert!(
            !reference_fields(109_010).contains(&GuideManagerReferenceField::FestivalColorNotUsed)
        );
    }

    #[test]
    fn decode_retains_exact_headers_and_checks_profiler_end_index() {
        let mut bytes = Vec::new();
        for _ in 0..48 {
            bytes.extend_from_slice(&[0xfe, 0x00, 0x24]);
        }
        bytes.extend_from_slice(&[0x00, 0x1d]);

        let (guide, consumed) = GuideManagerReferences::decode(112_004, 0x1d, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(guide.references().len(), 48);
        assert_eq!(
            guide.references()[0].field(),
            GuideManagerReferenceField::ServiceNotUsed(0)
        );
        assert_eq!(
            guide.references()[1].field(),
            GuideManagerReferenceField::ServiceNeeded(0)
        );
        assert_eq!(
            guide.references()[0].header(),
            &NetObjectHeader::NewSharedType(0x24)
        );
        assert_eq!(guide.encode(), bytes);
        assert!(GuideManagerReferences::decode(112_004, 0x1e, &bytes).is_err());
        assert!(GuideManagerReferences::decode(112_004, 0x1d, &bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn wolf_creek_manager_reference_fixture_has_independently_known_type_counts() {
        const FIXTURE: &[u8] =
            include_bytes!("../tests/fixtures/guide-manager-references-v112004-wolf-creek.bin");
        let (guide, consumed) = GuideManagerReferences::decode(112_004, 0x1d, FIXTURE).unwrap();
        assert_eq!(consumed, FIXTURE.len());
        assert_eq!(guide.references().len(), 48);
        assert!(
            guide.references()[..28]
                .iter()
                .all(|reference| reference.header() == &NetObjectHeader::NewSharedType(32))
        );
        assert_eq!(
            guide
                .references()
                .iter()
                .filter(|reference| reference.header() == &NetObjectHeader::NewSharedType(32))
                .count(),
            28
        );
        assert_eq!(
            guide
                .references()
                .iter()
                .filter(|reference| reference.header() == &NetObjectHeader::NewSharedType(30))
                .count(),
            19
        );
        assert_eq!(
            guide
                .references()
                .iter()
                .filter(|reference| reference.header() == &NetObjectHeader::NewSharedType(36))
                .count(),
            1
        );
        assert_eq!(guide.encode(), FIXTURE);
    }
}
