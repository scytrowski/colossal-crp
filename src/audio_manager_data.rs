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
    channel_record_offsets: Vec<usize>,
    radio_content_capacity: u32,
    content_flags: Vec<u32>,
    content_prefab_indices: Vec<u16>,
    content_prefab_names: Vec<Option<String>>,
    unlock_milestone_prefab_indices: Vec<u16>,
    unlock_milestone_references: Vec<NetObjectHeader>,
    radio_contents: Vec<AudioRadioContent>,
    content_cooldown_offsets: Vec<usize>,
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
        let mut channel_record_offsets = Vec::with_capacity(channel_count);
        let mut prefab_indices = channel_prefab_indices.iter().copied();
        for (index, flags) in channel_flags.iter().enumerate() {
            if *flags == 0 {
                continue;
            }
            let slot_index = u16::try_from(index + 1)
                .map_err(|_| CrpError::ArithmeticOverflow("AudioManager channel slot index"))?;
            channel_record_offsets.push(reader.offset());
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
        let mut content_cooldown_offsets = Vec::with_capacity(content_count);
        let mut prefab_indices = content_prefab_indices.iter().copied();
        for (index, flags) in content_flags.iter().enumerate() {
            if *flags == 0 {
                continue;
            }
            let slot_index = u16::try_from(index + 1)
                .map_err(|_| CrpError::ArithmeticOverflow("AudioManager content slot index"))?;
            content_cooldown_offsets.push(reader.offset());
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
                channel_record_offsets,
                radio_content_capacity,
                content_flags,
                content_prefab_indices,
                content_prefab_names,
                unlock_milestone_prefab_indices,
                unlock_milestone_references,
                radio_contents,
                content_cooldown_offsets,
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

    /// Creates an editor for fixed-width fields in existing channels and content.
    ///
    /// Active-slot counts and prefab assignments stay fixed, so edits preserve
    /// the manager body length and every name-table and reference byte.
    pub fn into_editor(self) -> AudioManagerEditor {
        AudioManagerEditor::from(self)
    }
}

/// Fixed-width edits for existing AudioManager radio records.
///
/// The editor can change channel playback state and content cooldowns. Slot
/// indices, prefab assignments, active flags, and all opaque scope bytes are
/// retained from the source. Encoding preserves the original body length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioManagerEditor {
    original: AudioManagerData,
    radio_channels: Vec<AudioRadioChannel>,
    radio_contents: Vec<AudioRadioContent>,
}

impl From<AudioManagerData> for AudioManagerEditor {
    fn from(original: AudioManagerData) -> Self {
        Self {
            radio_channels: original.radio_channels.clone(),
            radio_contents: original.radio_contents.clone(),
            original,
        }
    }
}

impl AudioManagerEditor {
    /// Returns the inner DataSerializer version used for the source fields.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns the existing radio channels in serialized slot order.
    pub fn radio_channels(&self) -> &[AudioRadioChannel] {
        &self.radio_channels
    }

    /// Returns mutable access to existing radio channels.
    ///
    /// Slot indices and prefab assignments must remain unchanged when encoded.
    pub fn radio_channels_mut(&mut self) -> &mut [AudioRadioChannel] {
        &mut self.radio_channels
    }

    /// Returns the existing radio contents in serialized slot order.
    pub fn radio_contents(&self) -> &[AudioRadioContent] {
        &self.radio_contents
    }

    /// Returns mutable access to existing radio contents.
    ///
    /// Slot indices and prefab assignments must remain unchanged when encoded.
    pub fn radio_contents_mut(&mut self) -> &mut [AudioRadioContent] {
        &mut self.radio_contents
    }

    /// Encodes the edited records into a copy of their original AudioManager bytes.
    ///
    /// The output has the original length. Any attempt to change active slots,
    /// prefab assignments, or the version-selected play-position presence is
    /// rejected.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.radio_channels.len() != self.original.radio_channels.len()
            || self.radio_channels.len() != self.original.channel_record_offsets.len()
            || self.radio_contents.len() != self.original.radio_contents.len()
            || self.radio_contents.len() != self.original.content_cooldown_offsets.len()
        {
            return Err(invalid(0, "AudioManager active record count changed"));
        }

        let mut bytes = self.original.serialized_prefix.clone();
        for (index, ((edited, original), offset)) in self
            .radio_channels
            .iter()
            .zip(&self.original.radio_channels)
            .zip(&self.original.channel_record_offsets)
            .enumerate()
        {
            if edited.slot_index != original.slot_index
                || edited.prefab_index != original.prefab_index
            {
                return Err(invalid(
                    *offset,
                    format!("radio channel {index} changed its slot or prefab assignment"),
                ));
            }
            if edited.play_position.is_some() != original.play_position.is_some() {
                return Err(invalid(
                    *offset,
                    format!("radio channel {index} changed its versioned play-position field"),
                ));
            }
            let mut field_offset = *offset;
            write_u16_at(&mut bytes, field_offset, edited.current_content_index)?;
            field_offset = field_offset
                .checked_add(2)
                .ok_or(CrpError::ArithmeticOverflow("AudioManager field offset"))?;
            write_u16_at(&mut bytes, field_offset, edited.next_content_index)?;
            field_offset = field_offset
                .checked_add(2)
                .ok_or(CrpError::ArithmeticOverflow("AudioManager field offset"))?;
            write_u8_at(&mut bytes, field_offset, edited.state_chain_index)?;
            field_offset = field_offset
                .checked_add(1)
                .ok_or(CrpError::ArithmeticOverflow("AudioManager field offset"))?;
            write_u8_at(&mut bytes, field_offset, edited.state_repeat_count)?;
            field_offset += 1;
            if let Some(play_position) = edited.play_position {
                write_u16_at(&mut bytes, field_offset, play_position)?;
            }
        }

        for (index, ((edited, original), offset)) in self
            .radio_contents
            .iter()
            .zip(&self.original.radio_contents)
            .zip(&self.original.content_cooldown_offsets)
            .enumerate()
        {
            if edited.slot_index != original.slot_index
                || edited.prefab_index != original.prefab_index
            {
                return Err(invalid(
                    *offset,
                    format!("radio content {index} changed its slot or prefab assignment"),
                ));
            }
            write_u16_at(&mut bytes, *offset, edited.cooldown)?;
        }
        Ok(bytes)
    }

    pub(crate) fn original_prefix(&self) -> &[u8] {
        &self.original.serialized_prefix
    }
}

fn write_u16_at(bytes: &mut [u8], offset: usize, value: u16) -> Result<(), CrpError> {
    let end = offset
        .checked_add(2)
        .ok_or(CrpError::ArithmeticOverflow("AudioManager field end"))?;
    let target = bytes
        .get_mut(offset..end)
        .ok_or_else(|| invalid(offset, "AudioManager field extends past its original bytes"))?;
    target.copy_from_slice(&value.to_be_bytes());
    Ok(())
}

fn write_u8_at(bytes: &mut [u8], offset: usize, value: u8) -> Result<(), CrpError> {
    let target = bytes
        .get_mut(offset)
        .ok_or_else(|| invalid(offset, "AudioManager field extends past its original bytes"))?;
    *target = value;
    Ok(())
}

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "AudioManager+Data".to_owned(),
        offset,
        reason: reason.into(),
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
    fn editor_changes_only_existing_channel_and_content_values() {
        let bytes = one_channel_one_content_body(281);
        let (decoded, _) = AudioManagerData::decode_prefix(281, &bytes).unwrap();
        let mut editor = decoded.into_editor();
        let channel = &mut editor.radio_channels_mut()[0];
        channel.current_content_index = 12;
        channel.next_content_index = 13;
        channel.state_chain_index = 14;
        channel.state_repeat_count = 15;
        channel.play_position = Some(0x1234);
        editor.radio_contents_mut()[0].cooldown = 0x5678;

        let mut expected = bytes;
        expected[32..40].copy_from_slice(&[0, 12, 0, 13, 14, 15, 0x12, 0x34]);
        expected[81..83].copy_from_slice(&[0x56, 0x78]);
        assert_eq!(editor.encode().unwrap(), expected);
    }

    #[test]
    fn editor_preserves_the_pre_281_channel_record_width() {
        let bytes = one_channel_one_content_body(280);
        let (decoded, _) = AudioManagerData::decode_prefix(280, &bytes).unwrap();
        let mut editor = decoded.into_editor();
        let channel = &mut editor.radio_channels_mut()[0];
        channel.current_content_index = 0x1234;
        channel.next_content_index = 0x5678;
        channel.state_chain_index = 9;
        channel.state_repeat_count = 10;

        let mut expected = bytes;
        expected[32..38].copy_from_slice(&[0x12, 0x34, 0x56, 0x78, 9, 10]);
        assert_eq!(editor.encode().unwrap(), expected);
    }

    #[test]
    fn editor_rejects_changes_to_slots_prefabs_or_versioned_fields() {
        let bytes = one_channel_one_content_body(281);
        let (decoded, _) = AudioManagerData::decode_prefix(281, &bytes).unwrap();

        let mut changed_slot = decoded.clone().into_editor();
        changed_slot.radio_channels_mut()[0].slot_index = 3;
        assert!(changed_slot.encode().is_err());

        let mut changed_prefab = decoded.clone().into_editor();
        changed_prefab.radio_contents_mut()[0].prefab_index = 11;
        assert!(changed_prefab.encode().is_err());

        let mut changed_play_position = decoded.into_editor();
        changed_play_position.radio_channels_mut()[0].play_position = None;
        assert!(changed_play_position.encode().is_err());
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
