use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_uint_array, read_encoded_ushort_array};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

const MIN_SUPPORTED_VERSION: u32 = 164;
const PLAY_POSITION_VERSION: u32 = 281;

/// One active radio channel serialized by `AudioManager+Data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioRadioChannel {
    /// One-based slot selected by the channel flags.
    pub slot_index: u16,
    /// Prefab index from the channel `PrefabCollection` scope.
    pub prefab_index: u16,
    /// Current radio-content slot.
    pub current_content_index: u16,
    /// Next radio-content slot.
    pub next_content_index: u16,
    /// Current state-chain index.
    pub state_chain_index: u8,
    /// Number of repeats in the current state.
    pub state_repeat_count: u8,
    /// Play position, present from inner version 281.
    pub play_position: Option<u16>,
}

/// One active radio content entry serialized by `AudioManager+Data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioRadioContent {
    /// One-based slot selected by the content flags.
    pub slot_index: u16,
    /// Prefab index from the content `PrefabCollection` scope.
    pub prefab_index: u16,
    /// Saved cooldown value.
    pub cooldown: u16,
}

/// Decoded reader fields from `AudioManager+Data`.
///
/// The original prefix bytes are retained for exact unchanged writing. The
/// local map corpus has no active channels, active content, or unlock
/// milestones, so those nonempty records are currently covered by synthetic
/// byte tests only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioManagerData {
    format_version: u32,
    radio_channel_capacity: u32,
    channel_flags: Vec<u32>,
    channel_prefab_indices: Vec<u16>,
    channel_prefab_names: Vec<Option<String>>,
    radio_channels: Vec<AudioRadioChannel>,
    radio_content_capacity: u32,
    content_flags: Vec<u32>,
    content_prefab_indices: Vec<u16>,
    content_prefab_names: Vec<Option<String>>,
    unlock_milestone_prefab_indices: Vec<u16>,
    unlock_milestone_references: Vec<NetObjectHeader>,
    radio_contents: Vec<AudioRadioContent>,
    serialized_prefix: Vec<u8>,
}

impl AudioManagerData {
    /// Decodes the known fields after the opening profiler tag and returns the
    /// consumed length before AudioManager's closing profiler tag. This decoder
    /// supports inner versions 164 and newer; play position is read from version
    /// 281. Truncated or malformed fields return an error.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        if format_version < MIN_SUPPORTED_VERSION {
            return Err(reader.invalid(format!(
                "AudioManager prefix requires inner version {MIN_SUPPORTED_VERSION} or newer"
            )));
        }
        let radio_channel_capacity = u32::from(reader.read_u16_be()?) + 1;
        let radio_content_capacity = u32::from(reader.read_u16_be()?) + 1;
        let channel_flag_count = usize::try_from(radio_channel_capacity - 1)
            .map_err(|_| CrpError::ArithmeticOverflow("AudioManager channel flag count"))?;
        let channel_flags = read_encoded_uint_array(&mut reader, channel_flag_count)?;
        let channel_count = channel_flags.iter().filter(|flags| **flags != 0).count();
        let channel_prefab_indices = read_encoded_ushort_array(&mut reader, channel_count)?;
        let channel_prefab_names = read_prefab_names(&mut reader)?;

        let mut radio_channels = Vec::with_capacity(channel_count);
        let mut prefab_indices = channel_prefab_indices.iter().copied();
        for (index, flags) in channel_flags.iter().enumerate() {
            if *flags == 0 {
                continue;
            }
            let slot_index = u16::try_from(index + 1)
                .map_err(|_| CrpError::ArithmeticOverflow("AudioManager channel slot index"))?;
            radio_channels.push(AudioRadioChannel {
                slot_index,
                prefab_index: prefab_indices
                    .next()
                    .ok_or_else(|| reader.invalid("channel prefab index is missing"))?,
                current_content_index: reader.read_u16_be()?,
                next_content_index: reader.read_u16_be()?,
                state_chain_index: reader.read_u8()?,
                state_repeat_count: reader.read_u8()?,
                play_position: if format_version >= PLAY_POSITION_VERSION {
                    Some(reader.read_u16_be()?)
                } else {
                    None
                },
            });
        }

        let content_flag_count = usize::try_from(radio_content_capacity - 1)
            .map_err(|_| CrpError::ArithmeticOverflow("AudioManager content flag count"))?;
        let content_flags = read_encoded_uint_array(&mut reader, content_flag_count)?;
        let content_count = content_flags.iter().filter(|flags| **flags != 0).count();
        let unlock_milestone_count = usize::from(reader.read_u16_be()?);
        let content_prefab_index_count = content_count.checked_add(unlock_milestone_count).ok_or(
            CrpError::ArithmeticOverflow("AudioManager content prefab index count"),
        )?;
        let all_content_prefab_indices =
            read_encoded_ushort_array(&mut reader, content_prefab_index_count)?;
        let content_prefab_indices = all_content_prefab_indices[..content_count].to_vec();
        let unlock_milestone_prefab_indices = all_content_prefab_indices[content_count..].to_vec();
        let content_prefab_names = read_prefab_names(&mut reader)?;

        let mut unlock_milestone_references = Vec::with_capacity(unlock_milestone_count);
        for _ in 0..unlock_milestone_count {
            unlock_milestone_references.push(read_net_object_header(&mut reader)?);
        }

        let mut radio_contents = Vec::with_capacity(content_count);
        let mut prefab_indices = content_prefab_indices.iter().copied();
        for (index, flags) in content_flags.iter().enumerate() {
            if *flags == 0 {
                continue;
            }
            let slot_index = u16::try_from(index + 1)
                .map_err(|_| CrpError::ArithmeticOverflow("AudioManager content slot index"))?;
            radio_contents.push(AudioRadioContent {
                slot_index,
                prefab_index: prefab_indices
                    .next()
                    .ok_or_else(|| reader.invalid("content prefab index is missing"))?,
                cooldown: reader.read_u16_be()?,
            });
        }

        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                radio_channel_capacity,
                channel_flags,
                channel_prefab_indices,
                channel_prefab_names,
                radio_channels,
                radio_content_capacity,
                content_flags,
                content_prefab_indices,
                content_prefab_names,
                unlock_milestone_prefab_indices,
                unlock_milestone_references,
                radio_contents,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version used for the reader gates.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the channel buffer size including reserved slot zero.
    pub fn radio_channel_capacity(&self) -> u32 {
        self.radio_channel_capacity
    }

    /// Returns serialized channel flags for slots 1 through capacity minus one.
    pub fn channel_flags(&self) -> &[u32] {
        &self.channel_flags
    }

    /// Returns the channel prefab-index scope in active-slot order.
    pub fn channel_prefab_indices(&self) -> &[u16] {
        &self.channel_prefab_indices
    }

    /// Returns the channel prefab names in serialized order.
    pub fn channel_prefab_names(&self) -> &[Option<String>] {
        &self.channel_prefab_names
    }

    /// Returns active channels in ascending slot order.
    pub fn radio_channels(&self) -> &[AudioRadioChannel] {
        &self.radio_channels
    }

    /// Returns the content buffer size including reserved slot zero.
    pub fn radio_content_capacity(&self) -> u32 {
        self.radio_content_capacity
    }

    /// Returns serialized content flags for slots 1 through capacity minus one.
    pub fn content_flags(&self) -> &[u32] {
        &self.content_flags
    }

    /// Returns active content prefab indices in slot order.
    pub fn content_prefab_indices(&self) -> &[u16] {
        &self.content_prefab_indices
    }

    /// Returns content and milestone prefab names in serialized order.
    pub fn content_prefab_names(&self) -> &[Option<String>] {
        &self.content_prefab_names
    }

    /// Returns unlock-milestone prefab indices after the active-content indices.
    pub fn unlock_milestone_prefab_indices(&self) -> &[u16] {
        &self.unlock_milestone_prefab_indices
    }

    /// Returns milestone object headers in serialized order.
    pub fn unlock_milestone_references(&self) -> &[NetObjectHeader] {
        &self.unlock_milestone_references
    }

    /// Returns active radio contents in ascending slot order.
    pub fn radio_contents(&self) -> &[AudioRadioContent] {
        &self.radio_contents
    }

    /// Returns the original decoded fields unchanged.
    pub fn encode_unchanged(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Returns the number of decoded field bytes, excluding the profiler end tag.
    pub fn encoded_len(&self) -> usize {
        self.serialized_prefix.len()
    }
}

fn read_prefab_names(reader: &mut Reader<'_>) -> Result<Vec<Option<String>>, CrpError> {
    let count = usize::from(reader.read_u16_be()?);
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        names.push(reader.read_unique_string()?);
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::{AudioManagerData, AudioRadioChannel, AudioRadioContent};
    use crate::net_manager_data::NetObjectHeader;

    fn one_channel_one_content_body(format_version: u32) -> Vec<u8> {
        let mut bytes = vec![
            0x00, 0x03, // Four channel slots including reserved slot zero.
            0x00, 0x02, // Three content slots including reserved slot zero.
            0x02, // Three channel flags as one literal run.
            0x00, 0x00, 0x00, 0x00, // Channel slot 1: inactive.
            0x00, 0x00, 0x00, 0x01, // Channel slot 2: active.
            0x00, 0x00, 0x00, 0x00, // Channel slot 3: inactive.
            0x00, 0x00, 0x07, // One channel prefab index: 7.
            0x00, 0x01, // One channel prefab name.
            0x80, 0x00, 0x07, 0xc3, 0x25, 0xf9, 0x0d, 0x00, 0xf7, 0x07, // "Channel".
            0x00, 0x03, 0x00, 0x04, 0x05, 0x06, // Channel record before play position.
        ];
        if format_version >= 281 {
            bytes.extend_from_slice(&[0x00, 0x07]);
        }
        bytes.extend_from_slice(&[
            0x01, // Two content flags as one literal run.
            0x00, 0x00, 0x00, 0x00, // Content slot 1: inactive.
            0x00, 0x00, 0x00, 0x01, // Content slot 2: active.
            0x00, 0x01, // One unlock milestone.
            0x01, 0x00, 0x0a, 0x00, 0x0b, // Content and milestone prefab indices.
            0x00, 0x02, // Two content prefab names.
            0x80, 0x00, 0x07, 0xc3, 0x2c, 0xff, 0x06, 0xf1, 0x09, 0x06, // "Content".
            0x80, 0x00, 0x09, 0xcd, 0x1c, 0x03, 0xf9, 0x0e, 0x01, 0xfb, 0xff, 0xf7,
            // "Milestone".
            0xff, // Null milestone object reference.
            0x00, 0x09, // Active-content cooldown.
        ]);
        bytes
    }

    #[test]
    fn active_audio_records_follow_expected_reader_field_order() {
        let bytes = one_channel_one_content_body(281);
        let (decoded, consumed) = AudioManagerData::decode_prefix(281, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.encoded_len(), bytes.len());
        assert_eq!(decoded.encode_unchanged(), bytes);
        assert_eq!(decoded.radio_channel_capacity(), 4);
        assert_eq!(decoded.channel_flags(), [0, 1, 0]);
        assert_eq!(decoded.channel_prefab_indices(), [7]);
        assert_eq!(decoded.channel_prefab_names(), [Some("Channel".to_owned())]);
        assert_eq!(
            decoded.radio_channels(),
            [AudioRadioChannel {
                slot_index: 2,
                prefab_index: 7,
                current_content_index: 3,
                next_content_index: 4,
                state_chain_index: 5,
                state_repeat_count: 6,
                play_position: Some(7),
            }]
        );
        assert_eq!(decoded.radio_content_capacity(), 3);
        assert_eq!(decoded.content_flags(), [0, 1]);
        assert_eq!(decoded.content_prefab_indices(), [10]);
        assert_eq!(
            decoded.content_prefab_names(),
            [Some("Content".to_owned()), Some("Milestone".to_owned())]
        );
        assert_eq!(decoded.unlock_milestone_prefab_indices(), [11]);
        assert_eq!(
            decoded.unlock_milestone_references(),
            [NetObjectHeader::Null]
        );
        assert_eq!(
            decoded.radio_contents(),
            [AudioRadioContent {
                slot_index: 2,
                prefab_index: 10,
                cooldown: 9,
            }]
        );
    }

    #[test]
    fn play_position_follows_the_281_reader_gate() {
        let bytes = one_channel_one_content_body(280);
        let (decoded, consumed) = AudioManagerData::decode_prefix(280, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.radio_channels()[0].play_position, None);
    }

    #[test]
    fn audio_manager_decoder_rejects_truncation_and_malformed_object_headers() {
        let bytes = one_channel_one_content_body(281);
        for end in 0..bytes.len() {
            assert!(
                AudioManagerData::decode_prefix(281, &bytes[..end]).is_err(),
                "{end}"
            );
        }

        let mut malformed = bytes.clone();
        let null_reference = malformed.len() - 3;
        malformed[null_reference] = 0xfe;
        malformed[null_reference + 1] = 0xff;
        assert!(AudioManagerData::decode_prefix(281, &malformed).is_err());
        assert!(AudioManagerData::decode_prefix(163, &[]).is_err());
    }
}
