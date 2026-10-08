use crate::CrpError;
use crate::data_serializer::Reader;
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

/// Inline fields of `StatisticsManager+Data`.
///
/// The manager stores a start frame and a reference to a separately queued
/// `StatisticArray`. Its original body bytes, including the profiler end
/// index, are retained for exact writing. The object-reference header is not
/// resolved against the DataSerializer object table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticsManagerData {
    format_version: u32,
    start_frame: u32,
    statistic_array: NetObjectHeader,
    serialized_body: Vec<u8>,
}

impl StatisticsManagerData {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the serialized start frame.
    pub fn start_frame(&self) -> u32 {
        self.start_frame
    }

    /// Returns the raw `StatisticArray` object-reference header.
    pub fn statistic_array_header(&self) -> &NetObjectHeader {
        &self.statistic_array
    }

    /// Returns the original fields and profiler end index byte for byte.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }

    pub(crate) fn decode(
        format_version: u32,
        expected_profile_index: u16,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let start_frame = reader.read_u32_be()?;
        let statistic_array = read_net_object_header(&mut reader)?;
        let profile_end = reader.read_u16_be()?;
        if profile_end != expected_profile_index {
            return Err(reader.invalid(format!(
                "StatisticsManager end tag refers to profiler string {profile_end}, expected {expected_profile_index}"
            )));
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                start_frame,
                statistic_array,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }
}

/// The queued object-reference table stored in a `StatisticArray` body.
///
/// `StatisticArray.Serialize` writes a big-endian `u16` element count followed
/// by that many DataSerializer references. The concrete statistic objects are
/// queued separately and remain opaque here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticArrayReferences {
    references: Vec<NetObjectHeader>,
    serialized_body: Vec<u8>,
}

impl StatisticArrayReferences {
    /// Decodes one `StatisticArray` body and returns its consumed byte count.
    ///
    /// The reference headers are not resolved, and bytes after the body remain
    /// with the caller. Truncated or malformed headers return an error.
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let count = usize::from(reader.read_u16_be()?);
        if count > reader.remaining() {
            return Err(reader.invalid(format!(
                "StatisticArray declares {count} references but has only {} bytes remaining",
                reader.remaining()
            )));
        }
        let mut references = Vec::with_capacity(count);
        for _ in 0..count {
            references.push(read_net_object_header(&mut reader)?);
        }
        let consumed = reader.offset();
        Ok((
            Self {
                references,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns statistic references in their original array order.
    pub fn references(&self) -> &[NetObjectHeader] {
        &self.references
    }

    /// Returns the original count and reference headers byte for byte.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{StatisticArrayReferences, StatisticsManagerData};
    use crate::data_serializer::write_unique_string;
    use crate::net_manager_data::NetObjectHeader;

    #[test]
    fn statistic_array_uses_u16_count_and_preserves_reference_headers() {
        let bytes = [0x00, 0x02, 0xff, 0x00, 0x00, 0x01, 0xaa];
        let (array, consumed) = StatisticArrayReferences::decode(&bytes).unwrap();
        assert_eq!(consumed, 6);
        assert_eq!(
            array.references(),
            &[NetObjectHeader::Null, NetObjectHeader::Existing(1)]
        );
        assert_eq!(array.encode(), bytes[..6]);
        assert_eq!(bytes[consumed], 0xaa);
        assert!(StatisticArrayReferences::decode(&[0x00, 0x01]).is_err());
    }

    #[test]
    fn statistic_array_wolf_creek_fixture_has_sixty_headers() {
        const FIXTURE: &[u8] =
            include_bytes!("../tests/fixtures/statistic-array-references-v112004-wolf-creek.bin");
        let (array, consumed) = StatisticArrayReferences::decode(FIXTURE).unwrap();
        assert_eq!(consumed, FIXTURE.len());
        assert_eq!(array.references().len(), 60);
        assert_eq!(array.references()[0], NetObjectHeader::NewSharedType(38));
        assert_eq!(array.encode(), FIXTURE);
    }

    #[test]
    fn statistics_manager_decodes_start_frame_reference_and_profiler_end() {
        let mut bytes = 0x0102_0304_u32.to_be_bytes().to_vec();
        bytes.extend_from_slice(&[0xfe, 0xfe]);
        write_unique_string(&mut bytes, "StatisticArray").unwrap();
        bytes.extend_from_slice(&[0x00, 0x07]);

        let (manager, consumed) = StatisticsManagerData::decode(112_004, 7, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(manager.format_version(), 112_004);
        assert_eq!(manager.start_frame(), 0x0102_0304);
        assert_eq!(
            manager.statistic_array_header(),
            &NetObjectHeader::NewType("StatisticArray".to_owned())
        );
        assert_eq!(manager.encode(), bytes);
        assert!(StatisticsManagerData::decode(112_004, 8, &bytes).is_err());
        assert!(StatisticsManagerData::decode(112_004, 7, &bytes[..bytes.len() - 1]).is_err());
    }
}
