use crate::CrpError;
use crate::data_serializer::Reader;

const WIDE_INFO_VIEWS_VERSION: u32 = 110_003;

/// The visited information-view bitmask stored by `InfoManager+Data`.
///
/// The original field bytes are retained for exact unchanged writing. The
/// reader uses four bytes before inner version 110003 and eight bytes from that
/// version onward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoManagerData {
    format_version: u32,
    visited_info_views: u64,
    original_bytes: Vec<u8>,
    serialized_field: Vec<u8>,
}

impl InfoManagerData {
    /// Decodes the version-sized visited-view mask at the start of the manager
    /// body and returns the number of bytes consumed.
    pub fn decode(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let visited_info_views = if format_version < WIDE_INFO_VIEWS_VERSION {
            u64::from(reader.read_u32_be()?)
        } else {
            reader.read_u64_be()?
        };
        let consumed = reader.offset();
        let serialized_field = bytes[..consumed].to_vec();

        Ok((
            Self {
                format_version,
                visited_info_views,
                original_bytes: serialized_field.clone(),
                serialized_field,
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version used to decode this field.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the visited information-view bitmask, widened to `u64` for all
    /// supported versions.
    pub fn visited_info_views(&self) -> u64 {
        self.visited_info_views
    }

    /// Replaces the mask using the width selected by the reader's version gate.
    /// Versions below 110003 reject values that do not fit in 32 bits.
    pub fn set_visited_info_views(&mut self, value: u64) -> Result<(), CrpError> {
        let serialized_field = if self.format_version < WIDE_INFO_VIEWS_VERSION {
            u32::try_from(value)
                .map_err(|_| invalid_info_manager(0, "visited-view mask does not fit u32"))?
                .to_be_bytes()
                .to_vec()
        } else {
            value.to_be_bytes().to_vec()
        };
        self.visited_info_views = value;
        self.serialized_field = serialized_field;
        Ok(())
    }

    /// Returns the original mask bytes unchanged.
    pub fn encode_unchanged(&self) -> Vec<u8> {
        self.original_bytes.clone()
    }

    /// Returns the current mask encoded at the original version-selected width.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_field.clone()
    }

    /// Returns the serialized width of the mask.
    pub fn encoded_len(&self) -> usize {
        self.serialized_field.len()
    }

    pub(crate) fn original_bytes(&self) -> &[u8] {
        &self.original_bytes
    }
}

fn invalid_info_manager(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "InfoManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::InfoManagerData;

    #[test]
    fn visited_view_mask_uses_the_reader_version_gate_and_expected_bytes() {
        let legacy_bytes = [0x12, 0x34, 0x56, 0x78, 0xaa];
        let (mut legacy, consumed) = InfoManagerData::decode(110_002, &legacy_bytes).unwrap();
        assert_eq!(consumed, 4);
        assert_eq!(legacy.visited_info_views(), 0x1234_5678);
        assert_eq!(legacy.encoded_len(), 4);
        assert_eq!(legacy.encode_unchanged(), [0x12, 0x34, 0x56, 0x78]);
        assert_eq!(legacy.encode(), [0x12, 0x34, 0x56, 0x78]);

        legacy.set_visited_info_views(0xa1b2_c3d4).unwrap();
        assert_eq!(legacy.encode(), [0xa1, 0xb2, 0xc3, 0xd4]);
        assert_eq!(legacy.encode_unchanged(), [0x12, 0x34, 0x56, 0x78]);

        let current_bytes = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
        let (mut current, consumed) = InfoManagerData::decode(110_003, &current_bytes).unwrap();
        assert_eq!(consumed, 8);
        assert_eq!(current.visited_info_views(), 0x0123_4567_89ab_cdef);
        current
            .set_visited_info_views(0xfedc_ba98_7654_3210)
            .unwrap();
        assert_eq!(
            current.encode(),
            [0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10]
        );
    }

    #[test]
    fn legacy_visited_view_mask_rejects_values_that_do_not_fit() {
        let (mut data, _) = InfoManagerData::decode(110_002, &[0, 0, 0, 1]).unwrap();
        assert!(data.set_visited_info_views(0x1_0000_0000).is_err());
        assert_eq!(data.visited_info_views(), 1);
        assert_eq!(data.encode(), [0, 0, 0, 1]);
    }

    #[test]
    fn visited_view_mask_rejects_truncated_fields() {
        assert!(InfoManagerData::decode(110_002, &[0, 0, 0]).is_err());
        assert!(InfoManagerData::decode(110_003, &[0, 0, 0, 0, 0, 0, 0]).is_err());
    }
}
