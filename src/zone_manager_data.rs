use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_uint_array, write_encoded_uint_array_runs};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};
use std::ops::Range;

const ZONE_BLOCK_COUNT: usize = 49_152;
const ZONE_FLAGS_OLD_COUNT: usize = 16_383;
const ZONE_FLAGS_MID_COUNT: usize = 32_767;
const ZONE_FLAGS_MID_VERSION: u32 = 205;
const ZONE_FLAGS_FULL_VERSION: u32 = 222;
const ZONE_DEMAND_VERSION: u32 = 74;
const ZONE_ACTUAL_DEMAND_VERSION: u32 = 168;
const ZONE_GOOD_AREA_VERSION: u32 = 194;
const ZONE_OBJECTS_VERSION: u32 = 130;
const ZONE_OPTIONS_VERSION: u32 = 134;
const ZONE_BLOCKS_VERSION: u32 = 160;
const ZONE_DEMAND_COUNT: usize = 3;
const GOOD_AREA_COUNT: usize = 8;
const INITIAL_OBJECT_COUNT: usize = 11;
const ZONE_BLOCK_RECORD_LEN: usize = 70;

/// One active `ZoneBlock` record from `ZoneManager+Data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneBlockRecord {
    /// One-based buffer index selected by the serialized flags.
    pub index: u16,
    /// Serialized build index.
    pub build_index: u32,
    /// Position x, y, and z as their original IEEE-754 bit patterns.
    pub position_bits: [u32; 3],
    /// Angle as its original IEEE-754 bit pattern.
    pub angle_bits: u32,
    /// `m_valid` bitfield.
    pub valid: u64,
    /// `m_shared` bitfield.
    pub shared: u64,
    /// `m_occupied1` bitfield.
    pub occupied1: u64,
    /// `m_occupied2` bitfield.
    pub occupied2: u64,
    /// `m_zone1` bitfield.
    pub zone1: u64,
    /// `m_zone2` bitfield.
    pub zone2: u64,
    /// Serialized segment index.
    pub segment: u16,
}

/// Known fields from `ZoneManager+Data` through its queued object references.
/// The original bytes are retained because the profiler end tag follows this
/// prefix and is resolved through DataSerializer's shared-string table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneManagerDataPrefix {
    format_version: u32,
    serialized_flag_count: usize,
    flags: Vec<u32>,
    flags_range: Range<usize>,
    blocks: Vec<ZoneBlockRecord>,
    demand: [i8; ZONE_DEMAND_COUNT],
    actual_demand: [i8; ZONE_DEMAND_COUNT],
    good_area_found: [i16; GOOD_AREA_COUNT],
    object_references: Vec<NetObjectHeader>,
    blocks_range: Range<usize>,
    demand_range: Option<Range<usize>>,
    actual_demand_range: Option<Range<usize>>,
    good_area_range: Option<Range<usize>>,
    serialized_prefix: Vec<u8>,
}

/// Editor for existing ZoneBlock flags, values, and versioned demand fields.
///
/// Flag edits preserve the active slots and are limited to positions serialized
/// by the selected version. Queued object references remain fixed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneManagerEditor {
    data: ZoneManagerDataPrefix,
    original_flags: Vec<u32>,
}

impl ZoneManagerDataPrefix {
    /// Decodes the known ZoneManager prefix for inner versions 164 and newer.
    /// Returns the byte length through its 13 queued object references, before
    /// the loading-profiler end tag. Truncated fields return an error.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(invalid_zone_manager(
                0,
                "ZoneManager prefix requires inner version 164 or newer",
            ));
        }
        let serialized_flag_count = if format_version >= ZONE_FLAGS_FULL_VERSION {
            ZONE_BLOCK_COUNT - 1
        } else if format_version >= ZONE_FLAGS_MID_VERSION {
            ZONE_FLAGS_MID_COUNT
        } else {
            ZONE_FLAGS_OLD_COUNT
        };
        let mut reader = Reader::new(bytes);
        let flags_start = reader.offset();
        let mut flags = read_encoded_uint_array(&mut reader, serialized_flag_count)?;
        let flags_range = flags_start..reader.offset();
        flags.resize(ZONE_BLOCK_COUNT - 1, 0);

        let blocks_start = reader.offset();
        let active_count = flags.iter().filter(|flag| **flag != 0).count();
        let mut blocks = Vec::with_capacity(active_count);
        for (index, flag) in flags.iter().enumerate() {
            if *flag == 0 {
                continue;
            }
            let block_index = u16::try_from(index + 1)
                .map_err(|_| CrpError::ArithmeticOverflow("ZoneBlock buffer index"))?;
            let build_index = reader.read_u32_be()?;
            let position_bits = [
                reader.read_u32_be()?,
                reader.read_u32_be()?,
                reader.read_u32_be()?,
            ];
            let angle_bits = reader.read_u32_be()?;
            let valid = reader.read_u64_be()?;
            let shared = reader.read_u64_be()?;
            let occupied1 = reader.read_u64_be()?;
            let occupied2 = reader.read_u64_be()?;
            let zone1 = reader.read_u64_be()?;
            let zone2 = reader.read_u64_be()?;
            let segment = reader.read_u16_be()?;
            blocks.push(ZoneBlockRecord {
                index: block_index,
                build_index,
                position_bits,
                angle_bits,
                valid,
                shared,
                occupied1,
                occupied2,
                zone1,
                zone2,
                segment,
            });
        }
        let blocks_range = blocks_start..reader.offset();

        let (demand, demand_range) = if format_version >= ZONE_DEMAND_VERSION {
            let start = reader.offset();
            let values = [
                reader.read_u8()? as i8,
                reader.read_u8()? as i8,
                reader.read_u8()? as i8,
            ];
            (values, Some(start..reader.offset()))
        } else {
            ([50; ZONE_DEMAND_COUNT], None)
        };
        let (actual_demand, actual_demand_range) = if format_version >= ZONE_ACTUAL_DEMAND_VERSION {
            let start = reader.offset();
            let values = [
                reader.read_u8()? as i8,
                reader.read_u8()? as i8,
                reader.read_u8()? as i8,
            ];
            (values, Some(start..reader.offset()))
        } else {
            (demand, None)
        };
        let (good_area_found, good_area_range) = if format_version >= ZONE_GOOD_AREA_VERSION {
            let start = reader.offset();
            let mut values = [0_i16; GOOD_AREA_COUNT];
            for value in &mut values {
                *value = reader.read_i16_be()?;
            }
            (values, Some(start..reader.offset()))
        } else {
            ([0; GOOD_AREA_COUNT], None)
        };

        let mut object_references = Vec::with_capacity(INITIAL_OBJECT_COUNT + 2);
        if format_version >= ZONE_OBJECTS_VERSION {
            for _ in 0..INITIAL_OBJECT_COUNT {
                object_references.push(read_net_object_header(&mut reader)?);
            }
        } else {
            object_references.extend((0..INITIAL_OBJECT_COUNT).map(|_| NetObjectHeader::Null));
        }
        object_references.push(if format_version >= ZONE_OPTIONS_VERSION {
            read_net_object_header(&mut reader)?
        } else {
            NetObjectHeader::Null
        });
        object_references.push(if format_version >= ZONE_BLOCKS_VERSION {
            read_net_object_header(&mut reader)?
        } else {
            NetObjectHeader::Null
        });

        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                serialized_flag_count,
                flags,
                flags_range,
                blocks,
                demand,
                actual_demand,
                good_area_found,
                object_references,
                blocks_range,
                demand_range,
                actual_demand_range,
                good_area_range,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the number of flag values physically stored for this version.
    pub fn serialized_flag_count(&self) -> usize {
        self.serialized_flag_count
    }

    /// Returns all 49,151 one-based flags, zero-filling version-omitted slots.
    pub fn flags(&self) -> &[u32] {
        &self.flags
    }

    /// Returns direct records for blocks with nonzero flags.
    pub fn blocks(&self) -> &[ZoneBlockRecord] {
        &self.blocks
    }

    /// Returns residential, commercial, and workplace demand.
    pub fn demand(&self) -> [i8; ZONE_DEMAND_COUNT] {
        self.demand
    }

    /// Returns actual residential, commercial, and workplace demand.
    pub fn actual_demand(&self) -> [i8; ZONE_DEMAND_COUNT] {
        self.actual_demand
    }

    /// Returns the eight good-area values.
    pub fn good_area_found(&self) -> &[i16; GOOD_AREA_COUNT] {
        &self.good_area_found
    }

    /// Returns references in serialized order: 8 unused zones, 3 demand
    /// wrappers, options, then zones. New-type names and indices are retained.
    pub fn object_references(&self) -> &[NetObjectHeader] {
        &self.object_references
    }

    /// Creates an editor for flags, existing blocks, and serialized demand fields.
    pub fn into_editor(self) -> ZoneManagerEditor {
        let original_flags = self.flags.clone();
        ZoneManagerEditor {
            data: self,
            original_flags,
        }
    }

    /// Writes the original known prefix exactly, including encoded array runs.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl ZoneManagerEditor {
    /// Replaces raw flag bits for one serialized ZoneBlock slot.
    ///
    /// `index` is zero-based, so zero represents serialized slot one. The
    /// value must preserve whether the slot is active. Slots omitted by the
    /// selected version cannot be edited.
    pub fn set_flag_bits(&mut self, index: usize, value: u32) -> Result<(), CrpError> {
        if index >= self.data.serialized_flag_count {
            return Err(invalid_zone_manager(
                index,
                "ZoneBlock flag slot is not serialized by this inner version",
            ));
        }
        let current =
            *self.data.flags.get(index).ok_or_else(|| {
                invalid_zone_manager(index, "ZoneBlock flag index is out of range")
            })?;
        if (current == 0) != (value == 0) {
            return Err(invalid_zone_manager(
                index,
                "ZoneBlock flag edits must preserve the slot's active state",
            ));
        }
        self.data.flags[index] = value;
        Ok(())
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.data.format_version
    }

    /// Returns mutable records for existing nonzero-flag blocks.
    ///
    /// Block indexes cannot be changed, and this slice cannot add or remove
    /// records. Flag edits are available through [`Self::set_flag_bits`].
    pub fn blocks_mut(&mut self) -> &mut [ZoneBlockRecord] {
        &mut self.data.blocks
    }

    /// Returns mutable residential, commercial, and workplace demand from
    /// inner version 74. Earlier layouts expose no serialized demand fields.
    pub fn demand_mut(&mut self) -> Option<&mut [i8; ZONE_DEMAND_COUNT]> {
        self.data.demand_range.as_ref()?;
        Some(&mut self.data.demand)
    }

    /// Returns mutable actual demand from inner version 168 onward.
    pub fn actual_demand_mut(&mut self) -> Option<&mut [i8; ZONE_DEMAND_COUNT]> {
        self.data.actual_demand_range.as_ref()?;
        Some(&mut self.data.actual_demand)
    }

    /// Returns mutable good-area values from inner version 194 onward.
    pub fn good_area_found_mut(&mut self) -> Option<&mut [i16; GOOD_AREA_COUNT]> {
        self.data.good_area_range.as_ref()?;
        Some(&mut self.data.good_area_found)
    }

    /// Writes edited fields and retains every other source byte.
    /// Re-encoding changed flags can resize the prefix. Block slots and queued
    /// object references remain fixed.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut bytes = self.data.serialized_prefix.clone();
        if self.data.flags.len() != ZONE_BLOCK_COUNT - 1
            || self.original_flags.len() != ZONE_BLOCK_COUNT - 1
            || self.data.serialized_flag_count > ZONE_BLOCK_COUNT - 1
        {
            return Err(invalid_zone_manager(
                0,
                "ZoneBlock flag count does not match the source layout",
            ));
        }
        let active_indices = self
            .data
            .flags
            .iter()
            .enumerate()
            .filter_map(|(index, flag)| (*flag != 0).then_some(index + 1));
        let mut active_indices = active_indices;
        if self.data.blocks.len() != self.data.flags.iter().filter(|flag| **flag != 0).count()
            || self
                .data
                .blocks
                .iter()
                .zip(&mut active_indices)
                .any(|(block, index)| usize::from(block.index) != index)
        {
            return Err(invalid_zone_manager(
                0,
                "ZoneBlock indexes and count must match the source flags",
            ));
        }

        let expected_blocks_len = self
            .data
            .blocks
            .len()
            .checked_mul(ZONE_BLOCK_RECORD_LEN)
            .ok_or(CrpError::ArithmeticOverflow("ZoneBlock record bytes"))?;
        validate_zone_range(
            &bytes,
            &self.data.flags_range,
            self.data.flags_range.len(),
            "ZoneBlock flags",
        )?;
        validate_zone_range(
            &bytes,
            &self.data.blocks_range,
            expected_blocks_len,
            "ZoneBlock records",
        )?;
        let mut encoded_blocks = Vec::with_capacity(expected_blocks_len);
        for block in &self.data.blocks {
            append_zone_block(&mut encoded_blocks, block);
        }
        let mut replacements = Vec::new();
        if self.data.flags != self.original_flags {
            if self.data.flags[self.data.serialized_flag_count..]
                != self.original_flags[self.data.serialized_flag_count..]
            {
                return Err(invalid_zone_manager(
                    self.data.serialized_flag_count,
                    "ZoneBlock flag slots omitted by this version cannot be changed",
                ));
            }
            let mut encoded_flags = Vec::new();
            write_encoded_uint_array_runs(
                &mut encoded_flags,
                &self.data.flags[..self.data.serialized_flag_count],
            );
            replacements.push((self.data.flags_range.clone(), encoded_flags));
        }
        if bytes.get(self.data.blocks_range.clone()) != Some(encoded_blocks.as_slice()) {
            replacements.push((self.data.blocks_range.clone(), encoded_blocks));
        }

        if let Some(range) = self.data.demand_range.as_ref() {
            validate_zone_range(&bytes, range, 3, "demand values")?;
            let encoded = self.data.demand.map(|value| value as u8).to_vec();
            if bytes.get(range.clone()) != Some(encoded.as_slice()) {
                replacements.push((range.clone(), encoded));
            }
        }
        if let Some(range) = self.data.actual_demand_range.as_ref() {
            validate_zone_range(&bytes, range, 3, "actual-demand values")?;
            let encoded = self.data.actual_demand.map(|value| value as u8).to_vec();
            if bytes.get(range.clone()) != Some(encoded.as_slice()) {
                replacements.push((range.clone(), encoded));
            }
        }
        if let Some(range) = self.data.good_area_range.as_ref() {
            validate_zone_range(&bytes, range, GOOD_AREA_COUNT * 2, "good-area values")?;
            let mut encoded = Vec::with_capacity(GOOD_AREA_COUNT * 2);
            for value in &self.data.good_area_found {
                encoded.extend_from_slice(&value.to_be_bytes());
            }
            if bytes.get(range.clone()) != Some(encoded.as_slice()) {
                replacements.push((range.clone(), encoded));
            }
        }

        replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let mut previous_start = bytes.len();
        for (range, replacement) in replacements {
            if range.end > previous_start {
                return Err(invalid_zone_manager(
                    range.start,
                    "edited ZoneManager field ranges overlap",
                ));
            }
            validate_zone_range(&bytes, &range, range.len(), "edited field")?;
            bytes.splice(range.clone(), replacement);
            previous_start = range.start;
        }
        Ok(bytes)
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.data.serialized_prefix
    }
}

fn append_zone_block(output: &mut Vec<u8>, block: &ZoneBlockRecord) {
    output.extend_from_slice(&block.build_index.to_be_bytes());
    for position in block.position_bits {
        output.extend_from_slice(&position.to_be_bytes());
    }
    output.extend_from_slice(&block.angle_bits.to_be_bytes());
    for value in [
        block.valid,
        block.shared,
        block.occupied1,
        block.occupied2,
        block.zone1,
        block.zone2,
    ] {
        output.extend_from_slice(&value.to_be_bytes());
    }
    output.extend_from_slice(&block.segment.to_be_bytes());
}

fn validate_zone_range(
    source: &[u8],
    range: &Range<usize>,
    expected_len: usize,
    field: &str,
) -> Result<(), CrpError> {
    if range.start > range.end || range.end > source.len() || range.len() != expected_len {
        return Err(invalid_zone_manager(
            range.start,
            format!("{field} range does not match its decoded values"),
        ));
    }
    Ok(())
}

fn invalid_zone_manager(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "ZoneManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::ZoneManagerDataPrefix;
    use crate::encoded_array::{write_encoded_uint_array, write_encoded_uint_array_runs};
    use crate::net_manager_data::NetObjectHeader;

    fn zero_flags(format_version: u32) -> Vec<u8> {
        let count = if format_version >= 222 {
            49_151
        } else if format_version >= 205 {
            32_767
        } else {
            16_383
        };
        let mut bytes = Vec::new();
        write_encoded_uint_array(&mut bytes, &vec![0; count]);
        bytes
    }

    fn null_references(bytes: &mut Vec<u8>, format_version: u32) {
        let count = if format_version >= 134 {
            13
        } else if format_version >= 130 {
            11
        } else {
            0
        };
        bytes.extend(vec![0xff; count]);
    }

    #[test]
    fn zone_manager_decodes_versioned_flag_counts_and_demand_gates() {
        for (version, flag_count) in [(164, 16_383), (205, 32_767), (222, 49_151)] {
            let mut bytes = zero_flags(version);
            if version >= 74 {
                bytes.extend_from_slice(&[0xff, 2, 3]);
            }
            if version >= 168 {
                bytes.extend_from_slice(&[4, 5, 6]);
            }
            if version >= 194 {
                for value in [-4_i16, -3, -2, -1, 0, 1, 2, 3] {
                    bytes.extend_from_slice(&value.to_be_bytes());
                }
            }
            null_references(&mut bytes, version);

            let (zone, used) = ZoneManagerDataPrefix::decode_prefix(version, &bytes)
                .expect("ZoneManager prefix decodes");
            assert_eq!(zone.serialized_flag_count(), flag_count);
            assert_eq!(zone.flags().len(), 49_151);
            assert!(zone.flags().iter().all(|flag| *flag == 0));
            assert!(zone.blocks().is_empty());
            assert_eq!(zone.demand(), [-1, 2, 3]);
            assert_eq!(
                zone.actual_demand(),
                if version >= 168 {
                    [4, 5, 6]
                } else {
                    [-1, 2, 3]
                }
            );
            assert_eq!(
                zone.good_area_found(),
                if version >= 194 {
                    &[-4, -3, -2, -1, 0, 1, 2, 3]
                } else {
                    &[0; 8]
                }
            );
            assert_eq!(used, bytes.len());
            assert_eq!(zone.encode(), bytes);
            assert_eq!(zone.object_references().len(), 13);
            if version == 164 {
                let mut editor = zone.clone().into_editor();
                assert!(editor.actual_demand_mut().is_none());
                assert!(editor.good_area_found_mut().is_none());
            }
        }
    }

    #[test]
    fn zone_manager_editor_changes_raw_flags_without_changing_active_slots() {
        for (version, flag_count) in [(164, 16_383), (205, 32_767), (222, 49_151)] {
            let mut flags = vec![0_u32; flag_count];
            flags[0] = 1;
            flags[flag_count - 1] = 2;
            let mut source = Vec::new();
            write_encoded_uint_array(&mut source, &flags);
            let source_flags_len = source.len();
            source.resize(source.len() + 2 * 70, 0);
            if version >= 74 {
                source.extend_from_slice(&[10, 20, 30]);
            }
            if version >= 168 {
                source.extend_from_slice(&[40, 50, 60]);
            }
            if version >= 194 {
                source.extend_from_slice(&[0; 16]);
            }
            null_references(&mut source, version);

            let (zone, _) = ZoneManagerDataPrefix::decode_prefix(version, &source).unwrap();
            let mut editor = zone.into_editor();
            editor.set_flag_bits(0, 3).unwrap();
            editor.set_flag_bits(flag_count - 1, 4).unwrap();
            assert!(editor.set_flag_bits(0, 0).is_err());
            assert!(editor.set_flag_bits(1, 1).is_err());
            assert!(editor.set_flag_bits(flag_count, 0).is_err());

            flags[0] = 3;
            flags[flag_count - 1] = 4;
            let mut expected = Vec::new();
            write_encoded_uint_array_runs(&mut expected, &flags);
            expected.extend_from_slice(&source[source_flags_len..]);
            let encoded = editor.encode().unwrap();
            assert_eq!(encoded, expected);

            let (updated, used) = ZoneManagerDataPrefix::decode_prefix(version, &encoded).unwrap();
            assert_eq!(used, encoded.len());
            assert_eq!(updated.flags()[0], 3);
            assert_eq!(updated.flags()[flag_count - 1], 4);
            assert_eq!(updated.blocks().len(), 2);
            assert_eq!(updated.blocks()[0].index, 1);
            assert_eq!(usize::from(updated.blocks()[1].index), flag_count);
        }
    }

    #[test]
    fn zone_manager_reads_active_records_in_flag_order_and_rejects_truncation() {
        let mut flags = vec![0; 16_383];
        flags[0] = 1;
        flags[2] = 7;
        let mut bytes = Vec::new();
        write_encoded_uint_array(&mut bytes, &flags);
        for block in [0_u32, 1] {
            bytes.extend_from_slice(&(0x1000 + block).to_be_bytes());
            for bits in [0x3f80_0000_u32 + block, 0xc020_0000, 0x4040_0000] {
                bytes.extend_from_slice(&bits.to_be_bytes());
            }
            bytes.extend_from_slice(&0x3f00_0000_u32.to_be_bytes());
            for value in 1_u64..=6 {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend_from_slice(&(0x20 + block as u16).to_be_bytes());
        }
        bytes.extend_from_slice(&[0xff, 2, 3]);
        null_references(&mut bytes, 164);

        let (zone, used) = ZoneManagerDataPrefix::decode_prefix(164, &bytes)
            .expect("active ZoneBlock records decode");
        assert_eq!(used, bytes.len());
        assert_eq!(zone.flags()[0], 1);
        assert_eq!(zone.flags()[2], 7);
        assert_eq!(zone.blocks().len(), 2);
        assert_eq!(zone.blocks()[0].index, 1);
        assert_eq!(zone.blocks()[1].index, 3);
        assert_eq!(zone.blocks()[0].build_index, 0x1000);
        assert_eq!(zone.blocks()[1].position_bits[0], 0x3f80_0001);
        assert_eq!(zone.blocks()[1].zone2, 6);
        assert_eq!(zone.blocks()[1].segment, 0x21);
        assert!(
            zone.object_references()
                .iter()
                .all(|header| *header == NetObjectHeader::Null)
        );
        assert!(ZoneManagerDataPrefix::decode_prefix(164, &bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn zone_manager_editor_patches_active_records_and_versioned_demand_values() {
        let mut flags = vec![0; 16_383];
        flags[0] = 1;
        flags[2] = 7;
        let mut bytes = Vec::new();
        write_encoded_uint_array(&mut bytes, &flags);
        let blocks_start = bytes.len();
        for block in [0_u32, 1] {
            bytes.extend_from_slice(&(0x1000 + block).to_be_bytes());
            for bits in [0x3f80_0000_u32 + block, 0xc020_0000, 0x4040_0000] {
                bytes.extend_from_slice(&bits.to_be_bytes());
            }
            bytes.extend_from_slice(&0x3f00_0000_u32.to_be_bytes());
            for value in 1_u64..=6 {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend_from_slice(&(0x20 + block as u16).to_be_bytes());
        }
        let demand_start = bytes.len();
        bytes.extend_from_slice(&[10, 20, 30]);
        let actual_demand_start = bytes.len();
        bytes.extend_from_slice(&[40, 50, 60]);
        let good_area_start = bytes.len();
        for value in [-4_i16, -3, -2, -1, 0, 1, 2, 3] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        null_references(&mut bytes, 194);

        let (zone, _) = ZoneManagerDataPrefix::decode_prefix(194, &bytes).unwrap();
        let mut editor = zone.into_editor();
        let block = &mut editor.blocks_mut()[1];
        block.build_index = 0xdead_beef;
        block.position_bits[0] = 0x4120_0000;
        block.valid = 0x1122_3344_5566_7788;
        block.segment = 0x7788;
        editor.demand_mut().unwrap()[0] = -5;
        editor.actual_demand_mut().unwrap()[2] = 42;
        editor.good_area_found_mut().unwrap()[7] = -1234;

        let second_block_start = blocks_start + 70;
        let mut expected = bytes;
        expected[second_block_start..second_block_start + 4]
            .copy_from_slice(&0xdead_beef_u32.to_be_bytes());
        expected[second_block_start + 4..second_block_start + 8]
            .copy_from_slice(&0x4120_0000_u32.to_be_bytes());
        expected[second_block_start + 20..second_block_start + 28]
            .copy_from_slice(&0x1122_3344_5566_7788_u64.to_be_bytes());
        expected[second_block_start + 68..second_block_start + 70]
            .copy_from_slice(&0x7788_u16.to_be_bytes());
        expected[demand_start] = (-5_i8) as u8;
        expected[actual_demand_start + 2] = 42;
        expected[good_area_start + 14..good_area_start + 16]
            .copy_from_slice(&(-1234_i16).to_be_bytes());
        assert_eq!(editor.encode().unwrap(), expected);

        let (updated, used) = ZoneManagerDataPrefix::decode_prefix(194, &expected).unwrap();
        assert_eq!(used, expected.len());
        assert_eq!(updated.blocks()[1].build_index, 0xdead_beef);
        assert_eq!(updated.blocks()[1].position_bits[0], 0x4120_0000);
        assert_eq!(updated.blocks()[1].valid, 0x1122_3344_5566_7788);
        assert_eq!(updated.blocks()[1].segment, 0x7788);
        assert_eq!(updated.demand(), [-5, 20, 30]);
        assert_eq!(updated.actual_demand(), [40, 50, 42]);
        assert_eq!(updated.good_area_found()[7], -1234);

        let (zone, _) = ZoneManagerDataPrefix::decode_prefix(194, &expected).unwrap();
        let mut invalid_editor = zone.into_editor();
        invalid_editor.blocks_mut()[0].index = 2;
        assert!(invalid_editor.encode().is_err());
    }

    #[test]
    fn zone_manager_rejects_unsupported_and_truncated_input() {
        assert!(ZoneManagerDataPrefix::decode_prefix(163, &[]).is_err());
        assert!(ZoneManagerDataPrefix::decode_prefix(164, &[]).is_err());
    }
}
