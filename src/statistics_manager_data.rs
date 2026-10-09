use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    read_encoded_int_array, read_encoded_ulong_array, write_encoded_int_array,
    write_encoded_ulong_array,
};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

const STATISTIC_HISTORY_LENGTH: usize = 256;
const STATISTIC_END_FRAME_VERSION: u32 = 189;

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
    profile_end_shared_string_index: u16,
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

    /// Returns the profiler end index stored by this manager body.
    ///
    /// The index is retained as serialized. Resolving it requires the full
    /// DataSerializer shared-string table, which this manager-level decoder
    /// does not reconstruct.
    pub fn profile_end_shared_string_index(&self) -> u16 {
        self.profile_end_shared_string_index
    }

    /// Returns the original fields and profiler end index byte for byte.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }

    /// Creates an editor for the start frame while preserving the array
    /// reference and unresolved profiler end index.
    pub fn into_editor(self) -> StatisticsManagerEditor {
        StatisticsManagerEditor::from(self)
    }

    pub(crate) fn decode(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let start_frame = reader.read_u32_be()?;
        let statistic_array = read_net_object_header(&mut reader)?;
        let profile_end = reader.read_u16_be()?;
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                start_frame,
                statistic_array,
                profile_end_shared_string_index: profile_end,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }
}

/// Editable `StatisticsManager+Data` start-frame value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticsManagerEditor {
    original: StatisticsManagerData,
    start_frame: u32,
}

impl From<StatisticsManagerData> for StatisticsManagerEditor {
    fn from(original: StatisticsManagerData) -> Self {
        Self {
            start_frame: original.start_frame,
            original,
        }
    }
}

impl StatisticsManagerEditor {
    /// Returns the inner DataSerializer version used for this body.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns mutable access to the statistics start frame.
    pub fn start_frame_mut(&mut self) -> &mut u32 {
        &mut self.start_frame
    }

    /// Encodes the edited frame and preserves every other body byte.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = self.original.serialized_body.clone();
        bytes[..4].copy_from_slice(&self.start_frame.to_be_bytes());
        bytes
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original.serialized_body
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

/// One queued `StatisticInt32` body.
///
/// The history contains exactly 256 `EncodedArray.Int` values. Before inner
/// version 189, the reader does not consume an end frame; `end_frame` is
/// therefore `None` for those streams. Original body bytes are retained for
/// unchanged writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticInt32Data {
    format_version: u32,
    start_frame: u32,
    end_frame: Option<u32>,
    temporary_value: i32,
    temporary_value_2: i32,
    latest_value: i32,
    total_value: i32,
    sample_count: u16,
    shift: u8,
    index: u8,
    history: Vec<i32>,
    serialized_body: Vec<u8>,
}

/// Editable values for one `StatisticInt32` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticInt32Editor {
    original: StatisticInt32Data,
    start_frame: u32,
    end_frame: Option<u32>,
    temporary_value: i32,
    temporary_value_2: i32,
    latest_value: i32,
    total_value: i32,
    sample_count: u16,
    shift: u8,
    index: u8,
    history: Vec<i32>,
}

impl StatisticInt32Data {
    /// Decodes one statistic body and returns the number of bytes consumed.
    pub fn decode(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let start_frame = reader.read_u32_be()?;
        let end_frame = if format_version >= STATISTIC_END_FRAME_VERSION {
            Some(reader.read_u32_be()?)
        } else {
            None
        };
        let temporary_value = reader.read_u32_be()? as i32;
        let temporary_value_2 = reader.read_u32_be()? as i32;
        let latest_value = reader.read_u32_be()? as i32;
        let total_value = reader.read_u32_be()? as i32;
        let sample_count = reader.read_u16_be()?;
        let shift = reader.read_u8()?;
        let index = reader.read_u8()?;
        let history = read_encoded_int_array(&mut reader, STATISTIC_HISTORY_LENGTH)?;
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                start_frame,
                end_frame,
                temporary_value,
                temporary_value_2,
                latest_value,
                total_value,
                sample_count,
                shift,
                index,
                history,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the statistic's start frame.
    pub fn start_frame(&self) -> u32 {
        self.start_frame
    }

    /// Returns the serialized end frame, absent from reader layouts before v189.
    pub fn end_frame(&self) -> Option<u32> {
        self.end_frame
    }

    /// Returns the first temporary signed value.
    pub fn temporary_value(&self) -> i32 {
        self.temporary_value
    }

    /// Returns the second temporary signed value.
    pub fn temporary_value_2(&self) -> i32 {
        self.temporary_value_2
    }

    /// Returns the latest signed value.
    pub fn latest_value(&self) -> i32 {
        self.latest_value
    }

    /// Returns the accumulated signed total.
    pub fn total_value(&self) -> i32 {
        self.total_value
    }

    /// Returns the sample count.
    pub fn sample_count(&self) -> u16 {
        self.sample_count
    }

    /// Returns the statistic's shift value.
    pub fn shift(&self) -> u8 {
        self.shift
    }

    /// Returns the statistic's index value.
    pub fn index(&self) -> u8 {
        self.index
    }

    /// Returns the decoded 256-value history.
    pub fn history(&self) -> &[i32] {
        &self.history
    }

    /// Returns the original statistic body byte for byte.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }

    /// Creates an editor for the decoded values.
    pub fn into_editor(self) -> StatisticInt32Editor {
        StatisticInt32Editor::from(self)
    }
}

impl From<StatisticInt32Data> for StatisticInt32Editor {
    fn from(original: StatisticInt32Data) -> Self {
        Self {
            start_frame: original.start_frame,
            end_frame: original.end_frame,
            temporary_value: original.temporary_value,
            temporary_value_2: original.temporary_value_2,
            latest_value: original.latest_value,
            total_value: original.total_value,
            sample_count: original.sample_count,
            shift: original.shift,
            index: original.index,
            history: original.history.clone(),
            original,
        }
    }
}

impl StatisticInt32Editor {
    /// Returns mutable access to the start frame.
    pub fn start_frame_mut(&mut self) -> &mut u32 {
        &mut self.start_frame
    }

    /// Returns mutable access to the serialized end frame when present.
    pub fn end_frame_mut(&mut self) -> Option<&mut u32> {
        self.end_frame.as_mut()
    }

    /// Returns mutable access to the first temporary signed value.
    pub fn temporary_value_mut(&mut self) -> &mut i32 {
        &mut self.temporary_value
    }

    /// Returns mutable access to the second temporary signed value.
    pub fn temporary_value_2_mut(&mut self) -> &mut i32 {
        &mut self.temporary_value_2
    }

    /// Returns mutable access to the latest signed value.
    pub fn latest_value_mut(&mut self) -> &mut i32 {
        &mut self.latest_value
    }

    /// Returns mutable access to the accumulated signed total.
    pub fn total_value_mut(&mut self) -> &mut i32 {
        &mut self.total_value
    }

    /// Returns mutable access to the sample count.
    pub fn sample_count_mut(&mut self) -> &mut u16 {
        &mut self.sample_count
    }

    /// Returns mutable access to the shift value.
    pub fn shift_mut(&mut self) -> &mut u8 {
        &mut self.shift
    }

    /// Returns mutable access to the index value.
    pub fn index_mut(&mut self) -> &mut u8 {
        &mut self.index
    }

    /// Returns mutable access to the 256-value history.
    pub fn history_mut(&mut self) -> &mut [i32] {
        &mut self.history
    }

    /// Encodes the body, preserving original history bytes when unchanged.
    ///
    /// Editing the history uses the canonical `EncodedArray.Int` writer.
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.original.serialized_body.len());
        output.extend_from_slice(&self.start_frame.to_be_bytes());
        if let Some(end_frame) = self.end_frame {
            output.extend_from_slice(&end_frame.to_be_bytes());
        }
        output.extend_from_slice(&self.temporary_value.to_be_bytes());
        output.extend_from_slice(&self.temporary_value_2.to_be_bytes());
        output.extend_from_slice(&self.latest_value.to_be_bytes());
        output.extend_from_slice(&self.total_value.to_be_bytes());
        output.extend_from_slice(&self.sample_count.to_be_bytes());
        output.extend_from_slice(&[self.shift, self.index]);

        if self.history == self.original.history {
            let history_offset = statistic_history_offset(self.original.format_version, 4);
            output.extend_from_slice(&self.original.serialized_body[history_offset..]);
        } else {
            write_encoded_int_array(&mut output, &self.history);
        }
        output
    }
}

/// One queued `StatisticInt64` body.
///
/// The history contains exactly 256 `EncodedArray.Long` values. Before inner
/// version 189, the reader does not consume an end frame; `end_frame` is
/// therefore `None` for those streams. Original body bytes are retained for
/// unchanged writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticInt64Data {
    format_version: u32,
    start_frame: u32,
    end_frame: Option<u32>,
    temporary_value: i64,
    temporary_value_2: i64,
    latest_value: i64,
    total_value: i64,
    sample_count: u16,
    shift: u8,
    index: u8,
    history: Vec<i64>,
    serialized_body: Vec<u8>,
}

/// Editable values for one `StatisticInt64` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticInt64Editor {
    original: StatisticInt64Data,
    start_frame: u32,
    end_frame: Option<u32>,
    temporary_value: i64,
    temporary_value_2: i64,
    latest_value: i64,
    total_value: i64,
    sample_count: u16,
    shift: u8,
    index: u8,
    history: Vec<i64>,
}

impl StatisticInt64Data {
    /// Decodes one statistic body and returns the number of bytes consumed.
    pub fn decode(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let start_frame = reader.read_u32_be()?;
        let end_frame = if format_version >= STATISTIC_END_FRAME_VERSION {
            Some(reader.read_u32_be()?)
        } else {
            None
        };
        let temporary_value = reader.read_u64_be()? as i64;
        let temporary_value_2 = reader.read_u64_be()? as i64;
        let latest_value = reader.read_u64_be()? as i64;
        let total_value = reader.read_u64_be()? as i64;
        let sample_count = reader.read_u16_be()?;
        let shift = reader.read_u8()?;
        let index = reader.read_u8()?;
        let history = read_encoded_ulong_array(&mut reader, STATISTIC_HISTORY_LENGTH)?
            .into_iter()
            .map(|bits| bits as i64)
            .collect();
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                start_frame,
                end_frame,
                temporary_value,
                temporary_value_2,
                latest_value,
                total_value,
                sample_count,
                shift,
                index,
                history,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the statistic's start frame.
    pub fn start_frame(&self) -> u32 {
        self.start_frame
    }

    /// Returns the serialized end frame, absent from reader layouts before v189.
    pub fn end_frame(&self) -> Option<u32> {
        self.end_frame
    }

    /// Returns the first temporary signed value.
    pub fn temporary_value(&self) -> i64 {
        self.temporary_value
    }

    /// Returns the second temporary signed value.
    pub fn temporary_value_2(&self) -> i64 {
        self.temporary_value_2
    }

    /// Returns the latest signed value.
    pub fn latest_value(&self) -> i64 {
        self.latest_value
    }

    /// Returns the accumulated signed total.
    pub fn total_value(&self) -> i64 {
        self.total_value
    }

    /// Returns the sample count.
    pub fn sample_count(&self) -> u16 {
        self.sample_count
    }

    /// Returns the statistic's shift value.
    pub fn shift(&self) -> u8 {
        self.shift
    }

    /// Returns the statistic's index value.
    pub fn index(&self) -> u8 {
        self.index
    }

    /// Returns the decoded 256-value history.
    pub fn history(&self) -> &[i64] {
        &self.history
    }

    /// Returns the original statistic body byte for byte.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }

    /// Creates an editor for the decoded values.
    pub fn into_editor(self) -> StatisticInt64Editor {
        StatisticInt64Editor::from(self)
    }
}

impl From<StatisticInt64Data> for StatisticInt64Editor {
    fn from(original: StatisticInt64Data) -> Self {
        Self {
            start_frame: original.start_frame,
            end_frame: original.end_frame,
            temporary_value: original.temporary_value,
            temporary_value_2: original.temporary_value_2,
            latest_value: original.latest_value,
            total_value: original.total_value,
            sample_count: original.sample_count,
            shift: original.shift,
            index: original.index,
            history: original.history.clone(),
            original,
        }
    }
}

impl StatisticInt64Editor {
    /// Returns mutable access to the start frame.
    pub fn start_frame_mut(&mut self) -> &mut u32 {
        &mut self.start_frame
    }

    /// Returns mutable access to the serialized end frame when present.
    pub fn end_frame_mut(&mut self) -> Option<&mut u32> {
        self.end_frame.as_mut()
    }

    /// Returns mutable access to the first temporary signed value.
    pub fn temporary_value_mut(&mut self) -> &mut i64 {
        &mut self.temporary_value
    }

    /// Returns mutable access to the second temporary signed value.
    pub fn temporary_value_2_mut(&mut self) -> &mut i64 {
        &mut self.temporary_value_2
    }

    /// Returns mutable access to the latest signed value.
    pub fn latest_value_mut(&mut self) -> &mut i64 {
        &mut self.latest_value
    }

    /// Returns mutable access to the accumulated signed total.
    pub fn total_value_mut(&mut self) -> &mut i64 {
        &mut self.total_value
    }

    /// Returns mutable access to the sample count.
    pub fn sample_count_mut(&mut self) -> &mut u16 {
        &mut self.sample_count
    }

    /// Returns mutable access to the shift value.
    pub fn shift_mut(&mut self) -> &mut u8 {
        &mut self.shift
    }

    /// Returns mutable access to the index value.
    pub fn index_mut(&mut self) -> &mut u8 {
        &mut self.index
    }

    /// Returns mutable access to the 256-value history.
    pub fn history_mut(&mut self) -> &mut [i64] {
        &mut self.history
    }

    /// Encodes the body, preserving original history bytes when unchanged.
    ///
    /// Editing the history uses the canonical `EncodedArray.Long` writer.
    pub fn encode(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.original.serialized_body.len());
        output.extend_from_slice(&self.start_frame.to_be_bytes());
        if let Some(end_frame) = self.end_frame {
            output.extend_from_slice(&end_frame.to_be_bytes());
        }
        output.extend_from_slice(&self.temporary_value.to_be_bytes());
        output.extend_from_slice(&self.temporary_value_2.to_be_bytes());
        output.extend_from_slice(&self.latest_value.to_be_bytes());
        output.extend_from_slice(&self.total_value.to_be_bytes());
        output.extend_from_slice(&self.sample_count.to_be_bytes());
        output.extend_from_slice(&[self.shift, self.index]);

        if self.history == self.original.history {
            let history_offset = statistic_history_offset(self.original.format_version, 8);
            output.extend_from_slice(&self.original.serialized_body[history_offset..]);
        } else {
            let history = self
                .history
                .iter()
                .map(|value| *value as u64)
                .collect::<Vec<_>>();
            write_encoded_ulong_array(&mut output, &history);
        }
        output
    }
}

fn statistic_history_offset(format_version: u32, value_width: usize) -> usize {
    8 + usize::from(format_version >= STATISTIC_END_FRAME_VERSION) * 4 + value_width * 4
}

#[cfg(test)]
mod tests {
    use super::{
        StatisticArrayReferences, StatisticInt32Data, StatisticInt64Data, StatisticsManagerData,
    };
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
        assert!(matches!(
            &array.references()[3],
            NetObjectHeader::NewType(type_name)
                if type_name.starts_with("StatisticInt64, Assembly-CSharp")
        ));
        assert!(matches!(
            &array.references()[4],
            NetObjectHeader::NewType(type_name)
                if type_name.starts_with("StatisticInt32, Assembly-CSharp")
        ));
        assert_eq!(array.encode(), FIXTURE);
    }

    #[test]
    fn wolf_creek_statistic_child_bodies_match_game_produced_records() {
        const ARRAY: &[u8] =
            include_bytes!("../tests/fixtures/statistic-array-nested-v112004-wolf-creek.bin");
        const INT32: &[u8] =
            include_bytes!("../tests/fixtures/statistic-int32-v112004-wolf-creek.bin");
        const INT64: &[u8] =
            include_bytes!("../tests/fixtures/statistic-int64-v112004-wolf-creek.bin");

        let (array, array_len) = StatisticArrayReferences::decode(ARRAY).unwrap();
        assert_eq!(array_len, ARRAY.len());
        assert_eq!(array.references().len(), 3);
        assert!(
            array
                .references()
                .iter()
                .all(|reference| *reference == NetObjectHeader::NewSharedType(40))
        );
        assert_eq!(array.encode(), ARRAY);

        let (statistic, consumed) = StatisticInt32Data::decode(112_004, INT32).unwrap();
        assert_eq!(consumed, INT32.len());
        assert_eq!(statistic.start_frame(), 4_243_412_992);
        assert_eq!(statistic.end_frame(), Some(1_920_255));
        assert_eq!(statistic.sample_count(), 64_282);
        assert_eq!(statistic.temporary_value(), -1_920_000);
        assert_eq!(statistic.temporary_value_2(), 3_072_255);
        assert_eq!(statistic.latest_value(), -3_071_745);
        assert_eq!(statistic.total_value(), -34_215_681);
        assert_eq!(statistic.history().len(), 256);
        assert_eq!(statistic.history()[0], 3);
        assert_eq!(statistic.encode(), INT32);

        let (statistic, consumed) = StatisticInt64Data::decode(112_004, INT64).unwrap();
        assert_eq!(consumed, INT64.len());
        assert_eq!(statistic.start_frame(), 0);
        assert_eq!(statistic.end_frame(), Some(0));
        assert_eq!(statistic.sample_count(), 40);
        assert_eq!(statistic.temporary_value(), 261_632);
        assert_eq!(statistic.temporary_value_2(), 2_953_798_581_661_739_008);
        assert_eq!(statistic.latest_value(), 359_725_196_296_333_566);
        assert_eq!(statistic.total_value(), 11_538_275_692_971_262);
        assert_eq!(statistic.history().len(), 256);
        assert_eq!(statistic.history()[0], -144_070_116_686_430_208);
        assert_eq!(statistic.encode(), INT64);
    }

    #[test]
    fn statistics_manager_decodes_start_frame_reference_and_profiler_end() {
        let mut bytes = 0x0102_0304_u32.to_be_bytes().to_vec();
        bytes.extend_from_slice(&[0xfe, 0xfe]);
        write_unique_string(&mut bytes, "StatisticArray").unwrap();
        bytes.extend_from_slice(&[0x00, 0x07]);

        let (manager, consumed) = StatisticsManagerData::decode(112_004, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(manager.format_version(), 112_004);
        assert_eq!(manager.start_frame(), 0x0102_0304);
        assert_eq!(
            manager.statistic_array_header(),
            &NetObjectHeader::NewType("StatisticArray".to_owned())
        );
        assert_eq!(manager.profile_end_shared_string_index(), 7);
        assert_eq!(manager.encode(), bytes);
        *bytes.last_mut().unwrap() = 8;
        let (manager, consumed) = StatisticsManagerData::decode(112_004, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(manager.profile_end_shared_string_index(), 8);
        assert!(StatisticsManagerData::decode(112_004, &bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn statistic_int32_fields_and_history_follow_v189_gate() {
        let mut current = Vec::new();
        current.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
        current.extend_from_slice(&0x1122_3344_u32.to_be_bytes());
        current.extend_from_slice(&(-1_i32).to_be_bytes());
        current.extend_from_slice(&0x1234_5678_i32.to_be_bytes());
        current.extend_from_slice(&(-2_i32).to_be_bytes());
        current.extend_from_slice(&0x0102_0304_i32.to_be_bytes());
        current.extend_from_slice(&0x0506_u16.to_be_bytes());
        current.extend_from_slice(&[7, 8, 0xff, 0xff]);
        current.push(0xaa);

        let (statistic, consumed) = StatisticInt32Data::decode(189, &current).unwrap();
        assert_eq!(consumed, current.len() - 1);
        assert_eq!(statistic.start_frame(), 0x0102_0304);
        assert_eq!(statistic.end_frame(), Some(0x1122_3344));
        assert_eq!(statistic.temporary_value(), -1);
        assert_eq!(statistic.temporary_value_2(), 0x1234_5678);
        assert_eq!(statistic.latest_value(), -2);
        assert_eq!(statistic.total_value(), 0x0102_0304);
        assert_eq!(statistic.sample_count(), 0x0506);
        assert_eq!(statistic.shift(), 7);
        assert_eq!(statistic.index(), 8);
        assert_eq!(statistic.history(), &[0; 256]);
        assert_eq!(statistic.encode(), current[..consumed]);

        let mut legacy = Vec::new();
        legacy.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
        legacy.extend_from_slice(&(-1_i32).to_be_bytes());
        legacy.extend_from_slice(&0x1234_5678_i32.to_be_bytes());
        legacy.extend_from_slice(&(-2_i32).to_be_bytes());
        legacy.extend_from_slice(&0x0102_0304_i32.to_be_bytes());
        legacy.extend_from_slice(&0x0506_u16.to_be_bytes());
        legacy.extend_from_slice(&[7, 8, 0xff, 0xff]);
        let (statistic, consumed) = StatisticInt32Data::decode(188, &legacy).unwrap();
        assert_eq!(consumed, legacy.len());
        assert_eq!(statistic.end_frame(), None);
        assert_eq!(statistic.temporary_value(), -1);
        assert_eq!(statistic.encode(), legacy);
        let mut legacy_editor = statistic.into_editor();
        assert!(legacy_editor.end_frame_mut().is_none());
        *legacy_editor.total_value_mut() = -12;
        let mut expected_legacy = legacy.clone();
        expected_legacy[16..20].copy_from_slice(&(-12_i32).to_be_bytes());
        assert_eq!(legacy_editor.encode(), expected_legacy);
        assert!(StatisticInt32Data::decode(189, &current[..current.len() - 2]).is_err());
    }

    #[test]
    fn statistic_int32_editor_preserves_or_canonicalizes_history_explicitly() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&2_u32.to_be_bytes());
        bytes.extend_from_slice(&3_i32.to_be_bytes());
        bytes.extend_from_slice(&4_i32.to_be_bytes());
        bytes.extend_from_slice(&5_i32.to_be_bytes());
        bytes.extend_from_slice(&6_i32.to_be_bytes());
        bytes.extend_from_slice(&7_u16.to_be_bytes());
        bytes.extend_from_slice(&[8, 9, 0xff, 0xff]);

        let (statistic, consumed) = StatisticInt32Data::decode(189, &bytes).unwrap();
        let mut editor = statistic.into_editor();
        *editor.latest_value_mut() = -10;
        let scalar_edit = editor.encode();
        let mut expected = bytes[..consumed].to_vec();
        expected[16..20].copy_from_slice(&(-10_i32).to_be_bytes());
        assert_eq!(scalar_edit, expected);

        editor.history_mut()[0] = -3;
        let history_edit = editor.encode();
        assert_eq!(&history_edit[28..33], &[0x3f, 0xff, 0xff, 0xff, 0xfd]);
        assert_eq!(history_edit.len(), 28 + 4 * (1 + 64 * 4));
        let (decoded, decoded_len) = StatisticInt32Data::decode(189, &history_edit).unwrap();
        assert_eq!(decoded_len, history_edit.len());
        assert_eq!(decoded.latest_value(), -10);
        assert_eq!(decoded.history()[0], -3);
        assert_eq!(&decoded.history()[1..], &[0; 255]);
    }

    #[test]
    fn statistic_int64_decodes_signed_values_and_256_value_history() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
        bytes.extend_from_slice(&0x1122_3344_u32.to_be_bytes());
        bytes.extend_from_slice(&(-1_i64).to_be_bytes());
        bytes.extend_from_slice(&0x0123_4567_89ab_cdef_i64.to_be_bytes());
        bytes.extend_from_slice(&(-2_i64).to_be_bytes());
        bytes.extend_from_slice(&9_i64.to_be_bytes());
        bytes.extend_from_slice(&0x0506_u16.to_be_bytes());
        bytes.extend_from_slice(&[7, 8, 0xff, 0xff]);

        let (statistic, consumed) = StatisticInt64Data::decode(194, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(statistic.start_frame(), 0x0102_0304);
        assert_eq!(statistic.end_frame(), Some(0x1122_3344));
        assert_eq!(statistic.temporary_value(), -1);
        assert_eq!(statistic.temporary_value_2(), 0x0123_4567_89ab_cdef);
        assert_eq!(statistic.latest_value(), -2);
        assert_eq!(statistic.total_value(), 9);
        assert_eq!(statistic.sample_count(), 0x0506);
        assert_eq!(statistic.shift(), 7);
        assert_eq!(statistic.index(), 8);
        assert_eq!(statistic.history(), &[0; 256]);
        assert_eq!(statistic.encode(), bytes);
        assert!(StatisticInt64Data::decode(194, &bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn statistic_int64_editor_preserves_history_bytes_for_scalar_changes() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&2_u32.to_be_bytes());
        bytes.extend_from_slice(&3_i64.to_be_bytes());
        bytes.extend_from_slice(&4_i64.to_be_bytes());
        bytes.extend_from_slice(&5_i64.to_be_bytes());
        bytes.extend_from_slice(&6_i64.to_be_bytes());
        bytes.extend_from_slice(&7_u16.to_be_bytes());
        bytes.extend_from_slice(&[8, 9, 0xff, 0xff]);

        let (statistic, consumed) = StatisticInt64Data::decode(194, &bytes).unwrap();
        let mut editor = statistic.into_editor();
        *editor.temporary_value_mut() = -11;
        let edited = editor.encode();
        let mut expected = bytes[..consumed].to_vec();
        expected[8..16].copy_from_slice(&(-11_i64).to_be_bytes());
        assert_eq!(edited, expected);

        editor.history_mut()[0] = -3;
        let history_edit = editor.encode();
        assert_eq!(
            &history_edit[44..53],
            &[0x3f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfd]
        );
        assert_eq!(history_edit.len(), 44 + 4 * (1 + 64 * 8));
        let (decoded, decoded_len) = StatisticInt64Data::decode(194, &history_edit).unwrap();
        assert_eq!(decoded_len, history_edit.len());
        assert_eq!(decoded.temporary_value(), -11);
        assert_eq!(decoded.history()[0], -3);
        assert_eq!(&decoded.history()[1..], &[0; 255]);
    }

    #[test]
    fn statistic_int64_legacy_layout_omits_end_frame() {
        let mut legacy = Vec::new();
        legacy.extend_from_slice(&1_u32.to_be_bytes());
        legacy.extend_from_slice(&2_i64.to_be_bytes());
        legacy.extend_from_slice(&3_i64.to_be_bytes());
        legacy.extend_from_slice(&4_i64.to_be_bytes());
        legacy.extend_from_slice(&5_i64.to_be_bytes());
        legacy.extend_from_slice(&6_u16.to_be_bytes());
        legacy.extend_from_slice(&[7, 8, 0xff, 0xff]);

        let (statistic, consumed) = StatisticInt64Data::decode(188, &legacy).unwrap();
        assert_eq!(consumed, legacy.len());
        assert_eq!(statistic.end_frame(), None);
        assert_eq!(statistic.encode(), legacy);

        let mut editor = statistic.into_editor();
        *editor.latest_value_mut() = -9;
        let mut expected = legacy;
        expected[20..28].copy_from_slice(&(-9_i64).to_be_bytes());
        assert_eq!(editor.encode(), expected);
    }
}
