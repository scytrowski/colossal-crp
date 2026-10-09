use crate::CrpError;
use crate::data_serializer::Reader;
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

const RECENT_MESSAGE_INDEX_VERSION: u32 = 85;
const MESSAGE_DATA_VERSION: u32 = 89;
const RANDOM_MESSAGE_TIMERS_VERSION: u32 = 155;
const RANDOM_MESSAGE_TIMER_COUNT: usize = 27;
const MAX_MESSAGE_ENTRIES: u32 = 1_000_000;
const MAX_OBJECT_ARRAY_ENTRIES: u32 = 1_000_000;

/// One key and last-created frame from `MessageManager+Data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageDataEntry {
    key: Option<String>,
    last_created_frame: u32,
}

impl MessageDataEntry {
    /// Returns the serialized unique string, retaining a null key if present.
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// Returns the saved frame at which this message was last created.
    pub fn last_created_frame(&self) -> u32 {
        self.last_created_frame
    }
}

/// Preserved reader fields from one `MessageManager+Data` body.
///
/// Object references remain unresolved, and the original serialized body is
/// retained for exact writing. Version-dependent fields absent from the body
/// are represented as `None`. Random-message timer bytes are retained at their
/// serialized count; the game reader zero-fills any remaining timer slots up
/// to 27.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageManagerData {
    format_version: u32,
    message_queue: Option<Vec<NetObjectHeader>>,
    recent_messages: Option<Vec<NetObjectHeader>>,
    recent_message_index: Option<i16>,
    message_data: Option<Vec<MessageDataEntry>>,
    last_random_message_frame: Option<u32>,
    random_message_timer_count: Option<i16>,
    random_message_timers: Vec<u8>,
    profile_end_shared_string_index: u16,
    serialized_body: Vec<u8>,
}

impl MessageManagerData {
    /// Returns the inner DataSerializer version used for the field gates.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns queued message references, preserving null versus empty arrays.
    pub fn message_queue(&self) -> Option<&[NetObjectHeader]> {
        self.message_queue.as_deref()
    }

    /// Returns the recent-message reference slots, if the array is non-null.
    pub fn recent_messages(&self) -> Option<&[NetObjectHeader]> {
        self.recent_messages.as_deref()
    }

    /// Returns the recent-message index when the versioned field is present.
    pub fn recent_message_index(&self) -> Option<i16> {
        self.recent_message_index
    }

    /// Returns version-present message data entries in serialized order.
    pub fn message_data(&self) -> Option<&[MessageDataEntry]> {
        self.message_data.as_deref()
    }

    /// Returns the last random-message frame when present.
    pub fn last_random_message_frame(&self) -> Option<u32> {
        self.last_random_message_frame
    }

    /// Returns the serialized timer count when the field is present.
    pub fn random_message_timer_count(&self) -> Option<i16> {
        self.random_message_timer_count
    }

    /// Returns only the timer bytes present in the serialized body.
    pub fn random_message_timers(&self) -> &[u8] {
        &self.random_message_timers
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
        let mut reader = Reader::new(bytes);
        let message_queue = read_object_array(&mut reader, "message queue")?;
        let recent_messages = read_object_array(&mut reader, "recent messages")?;
        let recent_message_index = if format_version >= RECENT_MESSAGE_INDEX_VERSION {
            Some(reader.read_i16_be()?)
        } else {
            None
        };
        let message_data = if format_version >= MESSAGE_DATA_VERSION {
            let count = reader.read_u24_be()?;
            if count > MAX_MESSAGE_ENTRIES {
                return Err(reader.invalid("message-data entry count exceeds 1,000,000"));
            }
            if usize::try_from(count).map_or(true, |count| count > reader.remaining() / 5) {
                return Err(
                    reader.invalid("message-data entry count cannot fit in the remaining body")
                );
            }
            let count = usize::try_from(count)
                .map_err(|_| reader.invalid("message-data count does not fit this platform"))?;
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                entries.push(MessageDataEntry {
                    key: reader.read_unique_string()?,
                    last_created_frame: reader.read_u32_be()?,
                });
            }
            Some(entries)
        } else {
            None
        };
        let (last_random_message_frame, random_message_timer_count, random_message_timers) =
            if format_version >= RANDOM_MESSAGE_TIMERS_VERSION {
                let last_frame = reader.read_u32_be()?;
                let timer_count = reader.read_i16_be()?;
                if timer_count < 0 {
                    return Err(reader.invalid("random-message timer count is negative"));
                }
                let timer_count_usize = usize::try_from(timer_count)
                    .map_err(|_| reader.invalid("timer count does not fit this platform"))?;
                if timer_count_usize > RANDOM_MESSAGE_TIMER_COUNT {
                    return Err(reader.invalid(format!(
                        "random-message timer count {timer_count_usize} exceeds {RANDOM_MESSAGE_TIMER_COUNT}"
                    )));
                }
                let timers = reader.read_bytes(timer_count_usize)?.to_vec();
                (Some(last_frame), Some(timer_count), timers)
            } else {
                (None, None, Vec::new())
            };
        let profile_end_shared_string_index = reader.read_u16_be()?;
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                message_queue,
                recent_messages,
                recent_message_index,
                message_data,
                last_random_message_frame,
                random_message_timer_count,
                random_message_timers,
                profile_end_shared_string_index,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }
}

fn read_object_array(
    reader: &mut Reader<'_>,
    description: &str,
) -> Result<Option<Vec<NetObjectHeader>>, CrpError> {
    let high = reader.read_u8()?;
    if high == 0xff {
        return Ok(None);
    }
    let count = (u32::from(high) << 16) | u32::from(reader.read_u16_be()?);
    if count > MAX_OBJECT_ARRAY_ENTRIES {
        return Err(reader.invalid(format!("{description} object count exceeds 1,000,000")));
    }
    if usize::try_from(count).map_or(true, |count| count > reader.remaining()) {
        return Err(reader.invalid(format!(
            "{description} object count cannot fit in the remaining body"
        )));
    }
    let count = usize::try_from(count)
        .map_err(|_| reader.invalid("object count does not fit this platform"))?;
    let mut references = Vec::with_capacity(count);
    for _ in 0..count {
        references.push(read_net_object_header(reader)?);
    }
    Ok(Some(references))
}

#[cfg(test)]
mod tests {
    use super::MessageManagerData;
    use crate::net_manager_data::NetObjectHeader;

    fn current_empty_body(version: u32) -> Vec<u8> {
        let mut bytes = vec![0xff, 0x00, 0x00, 0x10];
        bytes.extend([0xff; 16]);
        if version >= 85 {
            bytes.extend_from_slice(&(-1_i16).to_be_bytes());
        }
        if version >= 89 {
            bytes.extend_from_slice(&[0, 0, 0]);
        }
        if version >= 155 {
            bytes.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
            bytes.extend_from_slice(&27_i16.to_be_bytes());
            bytes.extend(0_u8..27);
        }
        bytes.extend_from_slice(&0x1234_u16.to_be_bytes());
        bytes
    }

    #[test]
    fn message_manager_fields_follow_version_gates_and_preserve_exact_bytes() {
        for version in [84, 85, 88, 89, 148, 149, 154, 155, 194] {
            let bytes = current_empty_body(version);
            let (manager, consumed) = MessageManagerData::decode(version, &bytes).unwrap();
            assert_eq!(consumed, bytes.len());
            assert_eq!(manager.format_version(), version);
            assert_eq!(manager.message_queue(), None);
            assert_eq!(manager.recent_messages().unwrap().len(), 16);
            assert!(
                manager
                    .recent_messages()
                    .unwrap()
                    .iter()
                    .all(|reference| *reference == NetObjectHeader::Null)
            );
            assert_eq!(manager.recent_message_index().is_some(), version >= 85);
            assert_eq!(manager.message_data().is_some(), version >= 89);
            assert_eq!(
                manager.last_random_message_frame().is_some(),
                version >= 155
            );
            assert_eq!(
                manager.random_message_timers().len(),
                if version >= 155 { 27 } else { 0 }
            );
            assert_eq!(manager.profile_end_shared_string_index(), 0x1234);
            assert_eq!(manager.encode(), bytes);
        }
    }

    #[test]
    fn object_arrays_distinguish_null_arrays_from_references() {
        let bytes = [0x00, 0x00, 0x01, 0x00, 0x00, 0x01, 0xff, 0x12, 0x34];
        let (manager, consumed) = MessageManagerData::decode(84, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(
            manager.message_queue(),
            Some(&[NetObjectHeader::Existing(1)][..])
        );
        assert_eq!(manager.recent_messages(), None);
        assert_eq!(manager.recent_message_index(), None);
        assert_eq!(manager.message_data(), None);
        assert_eq!(manager.encode(), bytes);

        let oversized_count = [0x0f, 0xff, 0xff];
        assert!(MessageManagerData::decode(84, &oversized_count).is_err());
    }

    #[test]
    fn message_data_keys_and_timer_prefix_are_retained() {
        let mut bytes = vec![0xff, 0xff];
        bytes.extend_from_slice(&0x1234_i16.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 1]);
        bytes.extend_from_slice(&[0x80, 0x00, 0x01]); // One compact unique string.
        bytes.extend_from_slice(&[0xc1]); // Delta from the initial byte 0x80 gives 'A'.
        bytes.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
        bytes.extend_from_slice(&0xaabb_ccdd_u32.to_be_bytes());
        bytes.extend_from_slice(&2_i16.to_be_bytes());
        bytes.extend_from_slice(&[7, 8]);
        bytes.extend_from_slice(&0x1234_u16.to_be_bytes());

        let (manager, consumed) = MessageManagerData::decode(155, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(manager.recent_message_index(), Some(0x1234));
        assert_eq!(manager.message_data().unwrap()[0].key(), Some("A"));
        assert_eq!(
            manager.message_data().unwrap()[0].last_created_frame(),
            0x0102_0304
        );
        assert_eq!(manager.last_random_message_frame(), Some(0xaabb_ccdd));
        assert_eq!(manager.random_message_timer_count(), Some(2));
        assert_eq!(manager.random_message_timers(), &[7, 8]);
        assert_eq!(manager.encode(), bytes);
    }

    #[test]
    fn wolf_creek_fixture_preserves_null_slots_and_short_timer_array() {
        const FIXTURE: &[u8] =
            include_bytes!("../tests/fixtures/message-manager-v112004-wolf-creek.bin");

        let (manager, consumed) = MessageManagerData::decode(112_004, FIXTURE).unwrap();
        assert_eq!(consumed, FIXTURE.len());
        assert!(manager.message_queue().unwrap().is_empty());
        let recent = manager.recent_messages().unwrap();
        assert_eq!(recent.len(), 16);
        assert!(
            recent
                .iter()
                .all(|reference| *reference == NetObjectHeader::Null)
        );
        assert_eq!(manager.recent_message_index(), Some(0));
        assert!(manager.message_data().unwrap().is_empty());
        assert_eq!(manager.last_random_message_frame(), Some(0));
        assert_eq!(manager.random_message_timer_count(), Some(26));
        assert_eq!(manager.random_message_timers(), &[0; 26]);
        assert_eq!(manager.profile_end_shared_string_index(), 28);
        assert_eq!(manager.encode(), FIXTURE);
    }

    #[test]
    fn message_manager_rejects_negative_or_oversized_timer_counts() {
        let mut negative = current_empty_body(155);
        let count_offset = negative.len() - 2 - 27 - 2;
        negative[count_offset..count_offset + 2].copy_from_slice(&(-1_i16).to_be_bytes());
        assert!(MessageManagerData::decode(155, &negative).is_err());

        let mut oversized = current_empty_body(155);
        let count_offset = oversized.len() - 2 - 27 - 2;
        oversized[count_offset..count_offset + 2].copy_from_slice(&28_i16.to_be_bytes());
        assert!(MessageManagerData::decode(155, &oversized).is_err());

        let mut truncated = current_empty_body(155);
        truncated.pop();
        assert!(MessageManagerData::decode(155, &truncated).is_err());
    }
}
