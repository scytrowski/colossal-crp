use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_uint_array, skip_encoded_ushort_array};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};
use std::ops::Range;

const SLOT_INDEX_MASK: u32 = !0x8000;
const LAST_SPAWN_FRAMES_VERSION: u32 = 291;
const WARNING_GUIDES_VERSION: u32 = 293;

/// One active record in `DisasterManager+Data`.
///
/// The `Option` fields match the reader's inner-version gates. Slot indexes
/// are one-based because the game reserves index zero in its disaster buffer.
#[derive(Debug, Clone, PartialEq)]
pub struct DisasterRecord {
    /// Index in the game disaster buffer, excluding its reserved index zero.
    pub slot_index: usize,
    /// Target position, present from inner version 258.
    pub target_position: Option<[f32; 3]>,
    /// Random seed, present from inner version 258.
    pub random_seed: Option<u64>,
    /// Creation frame, present from inner version 258.
    pub start_frame: Option<u32>,
    /// Activation frame, present from inner version 263.
    pub activation_frame: Option<u32>,
    /// Wave index, present from inner version 259.
    pub wave_index: Option<u16>,
    /// Rotation angle, present from inner version 260.
    pub angle: Option<f32>,
    /// Intensity, present from inner version 264.
    pub intensity: Option<u8>,
    /// Casualty count, present from inner version 289.
    pub casualty_count: Option<u32>,
    /// Collapsed-building count, present from inner version 289.
    pub collapsed_count: Option<u16>,
    /// Chirp cooldown, present from inner version 296.
    pub chirp_cooldown: Option<u8>,
    /// Broadcast cooldown, present from inner version 297.
    pub broadcast_cooldown: Option<u8>,
    /// Destroyed-road length, present from inner version 300.
    pub destroyed_road_length: Option<u32>,
    /// Destroyed-track length, present from inner version 301.
    pub destroyed_track_length: Option<u32>,
    /// Destroyed-power length, present from inner version 301.
    pub destroyed_power_length: Option<u32>,
    /// Tree-fire count, present from inner version 301.
    pub tree_fire_count: Option<u32>,
    /// Upgraded-building count, present from inner version 301.
    pub upgraded_count: Option<u16>,
    /// Building-fire count, present from inner version 301.
    pub building_fire_count: Option<u16>,
}

/// Decoded fields from a bounded `DisasterManager+Data` body.
///
/// Active record fields follow the inspected reader gates. The original body
/// and manager object references are retained for unchanged writing and queue
/// traversal. No supplied map sample contains active disaster records, so
/// active-record validation currently has synthetic fixture coverage.
#[derive(Debug, Clone, PartialEq)]
pub struct DisasterManagerData {
    format_version: u32,
    records: Vec<DisasterRecord>,
    record_ranges: Vec<Range<usize>>,
    object_headers: Vec<NetObjectHeader>,
    milestone_reference_count: usize,
    last_spawn_frames: Vec<u32>,
    last_spawn_frames_range: Range<usize>,
    random_disaster_cooldown: Option<u16>,
    random_disaster_cooldown_range: Option<Range<usize>>,
    serialized_body: Vec<u8>,
}

/// Editor for active disaster records and fixed-width manager counters.
///
/// The set of records, last-spawn frame count, and version-selected field
/// presence must remain unchanged. Editing these fixed-width values preserves
/// all other manager bytes, including prefab tables, references, and the
/// profiler boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct DisasterManagerEditor {
    original: DisasterManagerData,
    records: Vec<DisasterRecord>,
    last_spawn_frames: Vec<u32>,
    random_disaster_cooldown: Option<u16>,
}

impl DisasterManagerData {
    /// Decodes the complete manager body, including its closing profiler
    /// reference, and returns the number of bytes consumed.
    ///
    /// The layout and field gates follow `DisasterManager+Data.Deserialize`
    /// in the inspected Assembly-CSharp.dll. Unknown or truncated layouts
    /// return an error; empty and active record arrays are supported.
    pub fn decode(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let slot_count = usize::from(reader.read_u16_be()?)
            .checked_add(1)
            .ok_or(CrpError::ArithmeticOverflow("DisasterManager slot count"))?;
        let flags = read_encoded_uint_array(&mut reader, slot_count - 1)?;
        let active_slots = flags
            .iter()
            .enumerate()
            .filter_map(|(index, flag)| (flag & SLOT_INDEX_MASK != 0).then_some(index + 1))
            .collect::<Vec<_>>();

        let milestone_count = usize::from(reader.read_u16_be()?);
        let last_spawn_frame_count = if format_version >= LAST_SPAWN_FRAMES_VERSION {
            usize::from(reader.read_u16_be()?)
        } else {
            0
        };
        let warning_guide_count = if format_version >= WARNING_GUIDES_VERSION {
            usize::from(reader.read_u16_be()?)
        } else {
            0
        };
        let prefab_count = active_slots
            .len()
            .checked_add(milestone_count)
            .and_then(|count| count.checked_add(last_spawn_frame_count))
            .and_then(|count| count.checked_add(warning_guide_count))
            .ok_or(CrpError::ArithmeticOverflow("DisasterManager prefab count"))?;
        skip_encoded_ushort_array(&mut reader, prefab_count)?;
        skip_prefab_names(&mut reader)?;

        let mut records = Vec::with_capacity(active_slots.len());
        let mut record_ranges = Vec::with_capacity(active_slots.len());
        for slot_index in active_slots {
            let start = reader.offset();
            let record = read_disaster_record(&mut reader, format_version, slot_index)?;
            record_ranges.push(start..reader.offset());
            records.push(record);
        }

        let mut object_headers = read_object_headers(&mut reader, milestone_count)?;
        let last_spawn_frames_start = reader.offset();
        let mut last_spawn_frames = Vec::with_capacity(last_spawn_frame_count);
        for _ in 0..last_spawn_frame_count {
            last_spawn_frames.push(reader.read_u32_be()?);
        }
        let last_spawn_frames_range = last_spawn_frames_start..reader.offset();
        object_headers.extend(read_object_headers(&mut reader, warning_guide_count)?);
        let random_disaster_cooldown_start = reader.offset();
        let random_disaster_cooldown = if format_version >= 282 {
            Some(reader.read_u16_be()?)
        } else {
            None
        };
        let random_disaster_cooldown_range =
            (format_version >= 282).then_some(random_disaster_cooldown_start..reader.offset());
        reader.read_u16_be()?;

        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                records,
                record_ranges,
                object_headers,
                milestone_reference_count: milestone_count,
                last_spawn_frames,
                last_spawn_frames_range,
                random_disaster_cooldown,
                random_disaster_cooldown_range,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version used for this manager.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns active records in ascending buffer-slot order.
    pub fn records(&self) -> &[DisasterRecord] {
        &self.records
    }

    /// Returns unresolved milestone and warning-guide references in stream order.
    pub fn object_headers(&self) -> &[NetObjectHeader] {
        &self.object_headers
    }

    /// Returns milestone references before the last-spawn and warning-guide fields.
    pub fn milestone_references(&self) -> &[NetObjectHeader] {
        &self.object_headers[..self.milestone_reference_count]
    }

    /// Returns the last-spawn frame stored for each counted disaster prefab.
    pub fn last_spawn_frames(&self) -> &[u32] {
        &self.last_spawn_frames
    }

    /// Returns the random-disaster cooldown when serialized from inner version 282.
    pub fn random_disaster_cooldown(&self) -> Option<u16> {
        self.random_disaster_cooldown
    }

    /// Returns warning-guide references after the milestone reference fields.
    pub fn warning_guide_references(&self) -> &[NetObjectHeader] {
        &self.object_headers[self.milestone_reference_count..]
    }

    /// Returns the original manager body unchanged.
    pub fn encode_unchanged(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }

    /// Converts the decoded records to an editor.
    pub fn into_editor(self) -> DisasterManagerEditor {
        DisasterManagerEditor {
            records: self.records.clone(),
            last_spawn_frames: self.last_spawn_frames.clone(),
            random_disaster_cooldown: self.random_disaster_cooldown,
            original: self,
        }
    }
}

impl DisasterManagerEditor {
    /// Returns the inner DataSerializer version used for this manager.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns the active records that can be edited.
    pub fn records(&self) -> &[DisasterRecord] {
        &self.records
    }

    /// Returns mutable active records while retaining their original slots.
    ///
    /// Encoding rejects changes to record count, slot indexes, or version-gated
    /// field presence, so this access cannot change the manager layout.
    pub fn records_mut(&mut self) -> &mut [DisasterRecord] {
        &mut self.records
    }

    /// Returns the last-spawn frames that can be edited without changing their count.
    pub fn last_spawn_frames_mut(&mut self) -> &mut [u32] {
        &mut self.last_spawn_frames
    }

    /// Returns the stored random-disaster cooldown, if this version serializes it.
    pub fn random_disaster_cooldown(&self) -> Option<u16> {
        self.random_disaster_cooldown
    }

    /// Changes the random-disaster cooldown at its existing version-gated field.
    pub fn set_random_disaster_cooldown(&mut self, value: u16) -> Result<(), CrpError> {
        let Some(current) = self.random_disaster_cooldown.as_mut() else {
            return Err(invalid_disaster_manager(
                0,
                "random-disaster cooldown is not serialized before inner version 282",
            ));
        };
        *current = value;
        Ok(())
    }

    /// Encodes edited records and counters into a copy of the source body.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.records.len() != self.original.records.len()
            || self.records.len() != self.original.record_ranges.len()
            || self.last_spawn_frames.len() != self.original.last_spawn_frames.len()
        {
            return Err(invalid_disaster_manager(
                0,
                "active disaster record count cannot change in this editor",
            ));
        }

        let mut output = self.original.serialized_body.clone();
        for ((edited, original), range) in self
            .records
            .iter()
            .zip(&self.original.records)
            .zip(&self.original.record_ranges)
        {
            if edited.slot_index != original.slot_index {
                return Err(invalid_disaster_manager(
                    range.start,
                    "disaster record slot indexes cannot change in this editor",
                ));
            }
            let encoded = encode_disaster_record(self.format_version(), edited)?;
            if encoded.len() != range.len() {
                return Err(invalid_disaster_manager(
                    range.start,
                    "edited disaster record changed its serialized width",
                ));
            }
            let target = output.get_mut(range.clone()).ok_or_else(|| {
                invalid_disaster_manager(range.start, "disaster record range exceeds source body")
            })?;
            target.copy_from_slice(&encoded);
        }

        let frame_range = self.original.last_spawn_frames_range.clone();
        let frame_bytes = self
            .last_spawn_frames
            .iter()
            .flat_map(|frame| frame.to_be_bytes())
            .collect::<Vec<_>>();
        if frame_bytes.len() != frame_range.len() {
            return Err(invalid_disaster_manager(
                frame_range.start,
                "last-spawn frame edit changed its serialized width",
            ));
        }
        output
            .get_mut(frame_range.clone())
            .ok_or_else(|| {
                invalid_disaster_manager(
                    frame_range.start,
                    "last-spawn frame range exceeds source body",
                )
            })?
            .copy_from_slice(&frame_bytes);

        match (
            self.random_disaster_cooldown,
            self.original.random_disaster_cooldown,
            self.original.random_disaster_cooldown_range.as_ref(),
        ) {
            (Some(value), Some(_), Some(range)) if range.len() == 2 => {
                output
                    .get_mut(range.clone())
                    .ok_or_else(|| {
                        invalid_disaster_manager(
                            range.start,
                            "random-disaster cooldown range exceeds source body",
                        )
                    })?
                    .copy_from_slice(&value.to_be_bytes());
            }
            (None, None, None) => {}
            _ => {
                return Err(invalid_disaster_manager(
                    0,
                    "random-disaster cooldown layout changed in this editor",
                ));
            }
        }
        Ok(output)
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original.serialized_body
    }
}

fn read_disaster_record(
    reader: &mut Reader<'_>,
    format_version: u32,
    slot_index: usize,
) -> Result<DisasterRecord, CrpError> {
    let target_position = if format_version >= 258 {
        Some([
            reader.read_f32_be()?,
            reader.read_f32_be()?,
            reader.read_f32_be()?,
        ])
    } else {
        None
    };
    let random_seed = if format_version >= 258 {
        Some(reader.read_u64_be()?)
    } else {
        None
    };
    let start_frame = if format_version >= 258 {
        Some(reader.read_u32_be()?)
    } else {
        None
    };
    let activation_frame = if format_version >= 263 {
        Some(reader.read_u32_be()?)
    } else {
        None
    };
    let wave_index = if format_version >= 259 {
        Some(reader.read_u16_be()?)
    } else {
        None
    };
    let angle = if format_version >= 260 {
        Some(reader.read_f32_be()?)
    } else {
        None
    };
    let intensity = if format_version >= 264 {
        Some(reader.read_u8()?)
    } else {
        None
    };
    let (casualty_count, collapsed_count) = if format_version >= 289 {
        (Some(reader.read_u32_be()?), Some(reader.read_u16_be()?))
    } else {
        (None, None)
    };
    let destroyed_road_length = if format_version >= 300 {
        Some(reader.read_u32_be()?)
    } else {
        None
    };
    let (destroyed_track_length, destroyed_power_length, tree_fire_count) = if format_version >= 301
    {
        (
            Some(reader.read_u32_be()?),
            Some(reader.read_u32_be()?),
            Some(reader.read_u32_be()?),
        )
    } else {
        (None, None, None)
    };
    let (upgraded_count, building_fire_count) = if format_version >= 301 {
        (Some(reader.read_u16_be()?), Some(reader.read_u16_be()?))
    } else {
        (None, None)
    };
    let chirp_cooldown = if format_version >= 296 {
        Some(reader.read_u8()?)
    } else {
        None
    };
    let broadcast_cooldown = if format_version >= 297 {
        Some(reader.read_u8()?)
    } else {
        None
    };

    Ok(DisasterRecord {
        slot_index,
        target_position,
        random_seed,
        start_frame,
        activation_frame,
        wave_index,
        angle,
        intensity,
        casualty_count,
        collapsed_count,
        chirp_cooldown,
        broadcast_cooldown,
        destroyed_road_length,
        destroyed_track_length,
        destroyed_power_length,
        tree_fire_count,
        upgraded_count,
        building_fire_count,
    })
}

fn encode_disaster_record(
    format_version: u32,
    record: &DisasterRecord,
) -> Result<Vec<u8>, CrpError> {
    let mut output = Vec::new();
    encode_field(
        format_version >= 258,
        record.target_position,
        "target position",
        &mut output,
        |v, out| {
            for component in v {
                out.extend_from_slice(&component.to_be_bytes());
            }
        },
    )?;
    encode_field(
        format_version >= 258,
        record.random_seed,
        "random seed",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 258,
        record.start_frame,
        "start frame",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 263,
        record.activation_frame,
        "activation frame",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 259,
        record.wave_index,
        "wave index",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 260,
        record.angle,
        "angle",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 264,
        record.intensity,
        "intensity",
        &mut output,
        |v, out| {
            out.push(v);
        },
    )?;
    encode_field(
        format_version >= 289,
        record.casualty_count,
        "casualty count",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 289,
        record.collapsed_count,
        "collapsed count",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 300,
        record.destroyed_road_length,
        "destroyed road length",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 301,
        record.destroyed_track_length,
        "destroyed track length",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 301,
        record.destroyed_power_length,
        "destroyed power length",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 301,
        record.tree_fire_count,
        "tree fire count",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 301,
        record.upgraded_count,
        "upgraded count",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 301,
        record.building_fire_count,
        "building fire count",
        &mut output,
        |v, out| {
            out.extend_from_slice(&v.to_be_bytes());
        },
    )?;
    encode_field(
        format_version >= 296,
        record.chirp_cooldown,
        "chirp cooldown",
        &mut output,
        |v, out| {
            out.push(v);
        },
    )?;
    encode_field(
        format_version >= 297,
        record.broadcast_cooldown,
        "broadcast cooldown",
        &mut output,
        |v, out| {
            out.push(v);
        },
    )?;
    Ok(output)
}

fn encode_field<T: Copy>(
    present: bool,
    value: Option<T>,
    field: &str,
    output: &mut Vec<u8>,
    mut write: impl FnMut(T, &mut Vec<u8>),
) -> Result<(), CrpError> {
    match (present, value) {
        (true, Some(value)) => write(value, output),
        (false, None) => {}
        (true, None) => {
            return Err(invalid_disaster_manager(
                0,
                format!("version-gated disaster field {field} is missing"),
            ));
        }
        (false, Some(_)) => {
            return Err(invalid_disaster_manager(
                0,
                format!("disaster field {field} is unavailable at this version"),
            ));
        }
    }
    Ok(())
}

fn skip_prefab_names(reader: &mut Reader<'_>) -> Result<(), CrpError> {
    let count = usize::from(reader.read_u16_be()?);
    for _ in 0..count {
        reader.read_unique_string()?;
    }
    Ok(())
}

fn read_object_headers(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<NetObjectHeader>, CrpError> {
    if count > reader.remaining() {
        return Err(reader.invalid(format!(
            "DisasterManager declares {count} object references but only {} bytes remain",
            reader.remaining()
        )));
    }
    (0..count).map(|_| read_net_object_header(reader)).collect()
}

fn invalid_disaster_manager(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "DisasterManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::DisasterManagerData;

    fn active_record_body(version: u32, record: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0x00, 0x01]; // Two slots; slot zero is reserved.
        bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x01]); // One active flag.
        bytes.extend_from_slice(&[0x00, 0x00]); // No unlock milestones.
        if version >= 291 {
            bytes.extend_from_slice(&[0x00, 0x00]); // No last-spawn frames.
        }
        if version >= 293 {
            bytes.extend_from_slice(&[0x00, 0x00]); // No warning guides.
        }
        bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00]); // One prefab index and empty names.
        bytes.extend_from_slice(record);
        if version >= 282 {
            bytes.extend_from_slice(&[0x12, 0x34]); // Random-disaster cooldown.
        }
        bytes.extend_from_slice(&[0xab, 0xcd]); // Closing profiler reference.
        bytes
    }

    #[test]
    fn decodes_and_edits_active_disaster_fields_with_expected_bytes() {
        let mut record = Vec::new();
        for value in [1.0_f32, -2.0, 3.5] {
            record.extend_from_slice(&value.to_be_bytes());
        }
        record.extend_from_slice(&0x0102_0304_0506_0708_u64.to_be_bytes());
        record.extend_from_slice(&0x1122_3344_u32.to_be_bytes());
        record.extend_from_slice(&0x5566_7788_u32.to_be_bytes());
        record.extend_from_slice(&0x99aa_u16.to_be_bytes());
        record.extend_from_slice(&(-0.25_f32).to_be_bytes());
        record.push(55);
        record.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
        record.extend_from_slice(&0x0506_u16.to_be_bytes());
        record.extend_from_slice(&0x1112_1314_u32.to_be_bytes());
        record.extend_from_slice(&0x2122_2324_u32.to_be_bytes());
        record.extend_from_slice(&0x3132_3334_u32.to_be_bytes());
        record.extend_from_slice(&0x4142_4344_u32.to_be_bytes());
        record.extend_from_slice(&0x5152_u16.to_be_bytes());
        record.extend_from_slice(&0x6162_u16.to_be_bytes());
        record.push(7);
        record.push(8);

        let body = active_record_body(301, &record);
        let (decoded, consumed) = DisasterManagerData::decode(301, &body).unwrap();
        assert_eq!(consumed, body.len());
        assert_eq!(decoded.records().len(), 1);
        let parsed = &decoded.records()[0];
        assert_eq!(parsed.slot_index, 1);
        assert_eq!(parsed.target_position, Some([1.0, -2.0, 3.5]));
        assert_eq!(parsed.random_seed, Some(0x0102_0304_0506_0708));
        assert_eq!(parsed.start_frame, Some(0x1122_3344));
        assert_eq!(parsed.activation_frame, Some(0x5566_7788));
        assert_eq!(parsed.wave_index, Some(0x99aa));
        assert_eq!(parsed.angle, Some(-0.25));
        assert_eq!(parsed.intensity, Some(55));
        assert_eq!(parsed.casualty_count, Some(0x0102_0304));
        assert_eq!(parsed.collapsed_count, Some(0x0506));
        assert_eq!(parsed.chirp_cooldown, Some(7));
        assert_eq!(parsed.broadcast_cooldown, Some(8));
        assert_eq!(parsed.destroyed_road_length, Some(0x1112_1314));
        assert_eq!(parsed.destroyed_track_length, Some(0x2122_2324));
        assert_eq!(parsed.destroyed_power_length, Some(0x3132_3334));
        assert_eq!(parsed.tree_fire_count, Some(0x4142_4344));
        assert_eq!(parsed.upgraded_count, Some(0x5152));
        assert_eq!(parsed.building_fire_count, Some(0x6162));
        assert_eq!(decoded.encode_unchanged(), body);

        let mut editor = decoded.into_editor();
        editor.records_mut()[0].casualty_count = Some(0xa1b2_c3d4);
        editor.records_mut()[0].target_position = Some([4.0, 5.0, 6.0]);
        let encoded = editor.encode().unwrap();
        let mut expected = body.clone();
        for (offset, value) in [4.0_f32, 5.0, 6.0].into_iter().enumerate() {
            let start = 18 + offset * 4;
            expected[start..start + 4].copy_from_slice(&value.to_be_bytes());
        }
        expected[53..57].copy_from_slice(&0xa1b2_c3d4_u32.to_be_bytes());
        assert_eq!(encoded, expected);
        let (rewritten, rewritten_len) = DisasterManagerData::decode(301, &encoded).unwrap();
        assert_eq!(rewritten_len, body.len());
        assert_eq!(rewritten.records()[0].casualty_count, Some(0xa1b2_c3d4));
        assert_eq!(
            rewritten.records()[0].target_position,
            Some([4.0, 5.0, 6.0])
        );
        assert_eq!(&encoded[encoded.len() - 4..], &[0x12, 0x34, 0xab, 0xcd]);
    }

    #[test]
    fn disaster_record_fields_follow_each_reader_gate() {
        for version in [
            258, 259, 260, 263, 264, 281, 282, 289, 291, 293, 296, 297, 300, 301,
        ] {
            let record = vec![
                0_u8;
                if version >= 301 {
                    63
                } else if version >= 300 {
                    47
                } else if version >= 297 {
                    43
                } else if version >= 296 {
                    42
                } else if version >= 289 {
                    41
                } else if version >= 264 {
                    35
                } else if version >= 263 {
                    34
                } else if version >= 260 {
                    30
                } else if version >= 259 {
                    26
                } else {
                    24
                }
            ];
            let body = active_record_body(version, &record);
            let (decoded, consumed) = DisasterManagerData::decode(version, &body).unwrap();
            assert_eq!(consumed, body.len(), "version {version}");
            let decoded_record = &decoded.records()[0];
            assert_eq!(decoded_record.target_position.is_some(), version >= 258);
            assert_eq!(decoded_record.wave_index.is_some(), version >= 259);
            assert_eq!(decoded_record.angle.is_some(), version >= 260);
            assert_eq!(decoded_record.activation_frame.is_some(), version >= 263);
            assert_eq!(decoded_record.intensity.is_some(), version >= 264);
            assert_eq!(decoded_record.casualty_count.is_some(), version >= 289);
            assert_eq!(decoded_record.chirp_cooldown.is_some(), version >= 296);
            assert_eq!(decoded_record.broadcast_cooldown.is_some(), version >= 297);
            assert_eq!(
                decoded_record.destroyed_road_length.is_some(),
                version >= 300
            );
            assert_eq!(
                decoded_record.destroyed_track_length.is_some(),
                version >= 301
            );
            assert_eq!(decoded_record.building_fire_count.is_some(), version >= 301);
            assert_eq!(decoded.random_disaster_cooldown().is_some(), version >= 282);
            assert_eq!(decoded.clone().into_editor().encode().unwrap(), body);
        }
    }

    #[test]
    fn decodes_and_edits_milestone_warning_spawn_and_cooldown_fields() {
        let mut body = vec![0x00, 0x00]; // One reserved slot and no flag array.
        body.extend_from_slice(&[0x00, 0x01]); // One milestone reference.
        body.extend_from_slice(&[0x00, 0x01]); // One last-spawn frame.
        body.extend_from_slice(&[0x00, 0x01]); // One warning-guide reference.
        body.extend_from_slice(&[
            0x02, // Three literal prefab indexes.
            0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x00, // Empty prefab-name table.
            0xff, // Null milestone reference.
            0x01, 0x02, 0x03, 0x04, // Last-spawn frame.
            0xff, // Null warning-guide reference.
            0x05, 0x06, // Random-disaster cooldown.
            0x07, 0x08, // Closing profiler reference.
        ]);

        let (decoded, consumed) = DisasterManagerData::decode(301, &body).unwrap();
        assert_eq!(consumed, body.len());
        assert!(decoded.records().is_empty());
        assert_eq!(
            decoded.milestone_references(),
            [crate::net_manager_data::NetObjectHeader::Null,]
        );
        assert_eq!(
            decoded.warning_guide_references(),
            [crate::net_manager_data::NetObjectHeader::Null,]
        );
        assert_eq!(decoded.last_spawn_frames(), [0x0102_0304]);
        assert_eq!(decoded.random_disaster_cooldown(), Some(0x0506));
        assert_eq!(decoded.encode_unchanged(), body);

        let mut editor = decoded.into_editor();
        editor.last_spawn_frames_mut()[0] = 0x1122_3344;
        editor.set_random_disaster_cooldown(0xaabb).unwrap();
        let encoded = editor.encode().unwrap();
        let mut expected = body.clone();
        expected[18..22].copy_from_slice(&0x1122_3344_u32.to_be_bytes());
        expected[23..25].copy_from_slice(&0xaabb_u16.to_be_bytes());
        assert_eq!(encoded, expected);
        let (rewritten, _) = DisasterManagerData::decode(301, &encoded).unwrap();
        assert_eq!(rewritten.last_spawn_frames(), [0x1122_3344]);
        assert_eq!(rewritten.random_disaster_cooldown(), Some(0xaabb));
    }

    #[test]
    fn rejects_truncated_active_records_and_editor_layout_changes() {
        let body = active_record_body(301, &[0; 63]);
        for end in 0..body.len() {
            assert!(
                DisasterManagerData::decode(301, &body[..end]).is_err(),
                "{end}"
            );
        }

        let (decoded, _) = DisasterManagerData::decode(301, &body).unwrap();
        let mut editor = decoded.into_editor();
        editor.records_mut()[0].intensity = None;
        assert!(editor.encode().is_err());

        let (decoded, _) = DisasterManagerData::decode(301, &body).unwrap();
        let mut editor = decoded.into_editor();
        editor.records_mut()[0].slot_index = 2;
        assert!(editor.encode().is_err());
    }
}
