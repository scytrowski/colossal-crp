use crate::CrpError;
use crate::data_serializer::{Reader, write_nullable_unique_string};
use crate::encoded_array::{read_encoded_ushort_array, write_encoded_ushort_array};
use std::ops::Range;

const TREE_CAPACITY: usize = 262_144;
const BURNING_TREES_VERSION: u32 = 266;
const TREE_FIRE_FLAGS: u16 = 0x00c0;

/// One active tree entry from `TreeManager+Data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeInstanceRecord {
    /// One-based buffer index selected by the serialized flags.
    pub index: u32,
    /// Serialized prefab index.
    pub prefab_index: u16,
    /// Horizontal x and z coordinates as signed 16-bit values.
    pub position_x: i16,
    pub position_z: i16,
}

/// A tree's serialized fire state, present from inner version 266.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BurningTreeRecord {
    /// One-based tree buffer index.
    pub tree_index: u32,
    /// Fire intensity byte.
    pub fire_intensity: u8,
    /// Fire damage byte.
    pub fire_damage: u8,
}

/// Known `TreeManager+Data` fields through the optional burning-tree list.
/// The original field bytes are retained for exact unchanged writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeManagerDataPrefix {
    format_version: u32,
    flags: Vec<u16>,
    serialized_fire_flags: Vec<u16>,
    prefab_names: Vec<Option<String>>,
    prefab_name_ranges: Vec<Range<usize>>,
    trees: Vec<TreeInstanceRecord>,
    flags_range: Range<usize>,
    prefab_indices_range: Range<usize>,
    position_x_range: Range<usize>,
    position_z_range: Range<usize>,
    burning_trees: Option<Vec<BurningTreeRecord>>,
    burning_trees_range: Option<Range<usize>>,
    serialized_prefix: Vec<u8>,
}

/// Editor for existing tree flags, active trees, and burning-tree records.
///
/// Raw non-fire flag bits can be edited while preserving active slots and the
/// source fire bits. Tree slots and their count remain unchanged. Burning-tree
/// records from version 266 can be replaced as a list. Changed
/// `EncodedArray.UShort` columns use the canonical encoding; unchanged columns
/// and all other bytes retain their source representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeManagerEditor {
    data: TreeManagerDataPrefix,
    original_flags: Vec<u16>,
    original_prefab_names: Vec<Option<String>>,
    original_trees: Vec<TreeInstanceRecord>,
}

impl TreeManagerDataPrefix {
    /// Decodes the known TreeManager fields for inner versions 164 and newer.
    /// Returns the consumed length before TreeManager's closing profiler tag.
    /// Truncated or malformed fields return an error.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(invalid_tree_manager(
                0,
                "TreeManager prefix requires inner version 164 or newer",
            ));
        }

        let mut reader = Reader::new(bytes);
        let flags_start = reader.offset();
        let serialized_flags = read_encoded_ushort_array(&mut reader, TREE_CAPACITY - 1)?;
        let flags_range = flags_start..reader.offset();
        let serialized_fire_flags = serialized_flags
            .iter()
            .map(|flags| flags & TREE_FIRE_FLAGS)
            .collect();
        let mut flags = serialized_flags;
        flags.resize(TREE_CAPACITY - 1, 0);
        // Deserialize clears these runtime-only flags before loading the
        // separate burning-tree table (when that table exists).
        for flag in &mut flags {
            *flag &= !TREE_FIRE_FLAGS;
        }

        let active_indices: Vec<usize> = flags
            .iter()
            .enumerate()
            .filter_map(|(index, flag)| (*flag != 0).then_some(index))
            .collect();
        let prefab_indices_start = reader.offset();
        let prefab_indices = read_encoded_ushort_array(&mut reader, active_indices.len())?;
        let prefab_indices_range = prefab_indices_start..reader.offset();
        let (prefab_names, prefab_name_ranges) = read_prefab_names(&mut reader)?;
        let position_x_start = reader.offset();
        let position_x = read_encoded_ushort_array(&mut reader, active_indices.len())?;
        let position_x_range = position_x_start..reader.offset();
        let position_z_start = reader.offset();
        let position_z = read_encoded_ushort_array(&mut reader, active_indices.len())?;
        let position_z_range = position_z_start..reader.offset();

        let trees = active_indices
            .into_iter()
            .enumerate()
            .map(|(record_index, buffer_index)| TreeInstanceRecord {
                index: (buffer_index + 1) as u32,
                prefab_index: prefab_indices[record_index],
                position_x: position_x[record_index] as i16,
                position_z: position_z[record_index] as i16,
            })
            .collect();

        let (burning_trees, burning_trees_range) = if format_version >= BURNING_TREES_VERSION {
            let count = usize::try_from(reader.read_u24_be()?)
                .map_err(|_| CrpError::ArithmeticOverflow("burning tree count"))?;
            if count > TREE_CAPACITY - 1 {
                return Err(invalid_tree_manager(
                    reader.offset() - 3,
                    "burning tree count exceeds the tree buffer capacity",
                ));
            }
            let records_start = reader.offset();
            let mut records = Vec::with_capacity(count);
            for _ in 0..count {
                records.push(BurningTreeRecord {
                    tree_index: reader.read_u24_be()?,
                    fire_intensity: reader.read_u8()?,
                    fire_damage: reader.read_u8()?,
                });
            }
            (Some(records), Some(records_start..reader.offset()))
        } else {
            (None, None)
        };

        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                flags,
                serialized_fire_flags,
                prefab_names,
                prefab_name_ranges,
                trees,
                flags_range,
                prefab_indices_range,
                position_x_range,
                position_z_range,
                burning_trees,
                burning_trees_range,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns all 262,143 one-based tree flags after clearing runtime fire bits.
    pub fn flags(&self) -> &[u16] {
        &self.flags
    }

    /// Returns the prefab name scope in its serialized order.
    pub fn prefab_names(&self) -> &[Option<String>] {
        &self.prefab_names
    }

    /// Returns active tree records in ascending buffer order.
    pub fn trees(&self) -> &[TreeInstanceRecord] {
        &self.trees
    }

    /// Returns the burning-tree records when the versioned table is present.
    pub fn burning_trees(&self) -> Option<&[BurningTreeRecord]> {
        self.burning_trees.as_deref()
    }

    /// Creates an editor for raw flags, existing active trees, and any
    /// versioned burning-tree records.
    pub fn into_editor(self) -> TreeManagerEditor {
        let original_flags = self.flags.clone();
        let original_prefab_names = self.prefab_names.clone();
        let original_trees = self.trees.clone();
        TreeManagerEditor {
            data: self,
            original_flags,
            original_prefab_names,
            original_trees,
        }
    }

    /// Writes the original known prefix exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl TreeManagerEditor {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.data.format_version
    }

    /// Replaces raw non-fire flag bits for one serialized tree slot.
    ///
    /// `index` is zero-based, so zero represents serialized slot one. The
    /// value must preserve the slot's zero/nonzero active state and cannot set
    /// runtime fire bits `0x00C0`; those bits are preserved from the source
    /// array. A separate burning-tree table is present from version 266.
    pub fn set_flag_bits(&mut self, index: usize, value: u16) -> Result<(), CrpError> {
        let current = *self
            .data
            .flags
            .get(index)
            .ok_or_else(|| invalid_tree_manager(index, "tree flag index is out of range"))?;
        if (current == 0) != (value == 0) {
            return Err(invalid_tree_manager(
                index,
                "tree flag edits must preserve the slot's active state",
            ));
        }
        if value & TREE_FIRE_FLAGS != 0 {
            return Err(invalid_tree_manager(
                index,
                "runtime tree fire bits are preserved from the source and cannot be changed here",
            ));
        }
        self.data.flags[index] = value;
        Ok(())
    }

    /// Returns mutable active tree records without changing their count or slots.
    pub fn trees_mut(&mut self) -> &mut [TreeInstanceRecord] {
        &mut self.data.trees
    }

    /// Returns mutable tree prefab names in their original table order.
    /// The number and order of entries are fixed; changed strings use
    /// canonical DataSerializer encoding and can resize the manager body.
    pub fn prefab_names_mut(&mut self) -> &mut [Option<String>] {
        &mut self.data.prefab_names
    }

    /// Returns mutable burning-tree records without changing their count.
    /// Returns `None` for versions before 266.
    pub fn burning_trees_mut(&mut self) -> Option<&mut [BurningTreeRecord]> {
        self.data.burning_trees.as_deref_mut()
    }

    /// Replaces the burning-tree list from inner version 266.
    ///
    /// The list can contain at most 262,143 records, matching the serialized
    /// tree buffer. Record indexes must fit in 24 bits when encoded. Returns an
    /// error for versions before 266 or for a list larger than the tree buffer.
    pub fn set_burning_trees(&mut self, records: Vec<BurningTreeRecord>) -> Result<(), CrpError> {
        if self.data.burning_trees.is_none() {
            return Err(invalid_tree_manager(
                0,
                "burning-tree records are unavailable before inner version 266",
            ));
        }
        if records.len() > TREE_CAPACITY - 1 {
            return Err(invalid_tree_manager(
                0,
                "burning tree count exceeds the tree buffer capacity",
            ));
        }
        self.data.burning_trees = Some(records);
        Ok(())
    }

    /// Writes edited fields while retaining unchanged source bytes.
    ///
    /// Changing raw flags, prefab indexes, or positions re-encodes only the
    /// affected `EncodedArray.UShort` column. Changed prefab names use
    /// canonical DataSerializer encoding; both may change the body length.
    /// Preserved source fire bits are restored when flags are rewritten.
    /// Burning-tree records use fixed-width fields, and their list can be
    /// replaced from version 266. Tree slots and their count cannot change.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut bytes = self.data.serialized_prefix.clone();
        if self.data.flags.len() != TREE_CAPACITY - 1
            || self.original_flags.len() != TREE_CAPACITY - 1
            || self.data.serialized_fire_flags.len() != TREE_CAPACITY - 1
            || self.data.prefab_names.len() != self.original_prefab_names.len()
            || self.data.prefab_names.len() != self.data.prefab_name_ranges.len()
            || self.data.trees.len() != self.original_trees.len()
        {
            return Err(invalid_tree_manager(
                0,
                "tree flags or active record counts do not match the source",
            ));
        }

        if self
            .data
            .trees
            .iter()
            .zip(&self.original_trees)
            .any(|(tree, original)| tree.index != original.index)
        {
            return Err(invalid_tree_manager(
                0,
                "active tree buffer indexes cannot be changed",
            ));
        }

        let mut replacements = Vec::new();
        if self.data.flags != self.original_flags {
            let serialized_flags = self
                .data
                .flags
                .iter()
                .zip(&self.data.serialized_fire_flags)
                .map(|(flags, fire_flags)| flags | fire_flags)
                .collect::<Vec<_>>();
            replacements.push(encode_ushort_column(
                &bytes,
                &self.data.flags_range,
                &serialized_flags,
                "tree flags",
            )?);
        }

        replacements.extend(encode_name_replacements(
            &bytes,
            &self.original_prefab_names,
            &self.data.prefab_names,
            &self.data.prefab_name_ranges,
            "tree prefab names",
        )?);

        let current_prefab_indices = self
            .data
            .trees
            .iter()
            .map(|tree| tree.prefab_index)
            .collect::<Vec<_>>();
        let original_prefab_indices = self
            .original_trees
            .iter()
            .map(|tree| tree.prefab_index)
            .collect::<Vec<_>>();
        if current_prefab_indices != original_prefab_indices {
            replacements.push(encode_ushort_column(
                &bytes,
                &self.data.prefab_indices_range,
                &current_prefab_indices,
                "tree prefab indexes",
            )?);
        }

        let current_position_x = self
            .data
            .trees
            .iter()
            .map(|tree| tree.position_x as u16)
            .collect::<Vec<_>>();
        let original_position_x = self
            .original_trees
            .iter()
            .map(|tree| tree.position_x as u16)
            .collect::<Vec<_>>();
        if current_position_x != original_position_x {
            replacements.push(encode_ushort_column(
                &bytes,
                &self.data.position_x_range,
                &current_position_x,
                "tree x positions",
            )?);
        }

        let current_position_z = self
            .data
            .trees
            .iter()
            .map(|tree| tree.position_z as u16)
            .collect::<Vec<_>>();
        let original_position_z = self
            .original_trees
            .iter()
            .map(|tree| tree.position_z as u16)
            .collect::<Vec<_>>();
        if current_position_z != original_position_z {
            replacements.push(encode_ushort_column(
                &bytes,
                &self.data.position_z_range,
                &current_position_z,
                "tree z positions",
            )?);
        }

        if let (Some(records), Some(range)) = (
            self.data.burning_trees.as_deref(),
            self.data.burning_trees_range.as_ref(),
        ) {
            let count_start = range.start.checked_sub(3).ok_or_else(|| {
                invalid_tree_manager(range.start, "burning-tree count precedes the data prefix")
            })?;
            let replacement_range = count_start..range.end;
            validate_range(
                &bytes,
                &replacement_range,
                replacement_range.len(),
                "burning-tree count and records",
            )?;
            if range.len() % 5 != 0 {
                return Err(invalid_tree_manager(
                    range.start,
                    "burning-tree record byte range is not a multiple of five",
                ));
            }
            let source_count = (usize::from(bytes[count_start]) << 16)
                | (usize::from(bytes[count_start + 1]) << 8)
                | usize::from(bytes[count_start + 2]);
            if source_count != range.len() / 5 {
                return Err(invalid_tree_manager(
                    count_start,
                    "burning-tree count does not match its source record bytes",
                ));
            }
            let records_len = records
                .len()
                .checked_mul(5)
                .ok_or(CrpError::ArithmeticOverflow("burning tree record bytes"))?;
            let encoded_count = u32::try_from(records.len())
                .map_err(|_| CrpError::ArithmeticOverflow("burning tree count"))?;
            let mut encoded_records = Vec::with_capacity(3 + records_len);
            encoded_records.extend_from_slice(&[
                (encoded_count >> 16) as u8,
                (encoded_count >> 8) as u8,
                encoded_count as u8,
            ]);
            for record in records {
                if record.tree_index > 0x00ff_ffff {
                    return Err(invalid_tree_manager(
                        range.start,
                        "burning-tree index does not fit in 24 bits",
                    ));
                }
                encoded_records.extend_from_slice(&[
                    (record.tree_index >> 16) as u8,
                    (record.tree_index >> 8) as u8,
                    record.tree_index as u8,
                    record.fire_intensity,
                    record.fire_damage,
                ]);
            }
            if bytes.get(replacement_range.clone()) != Some(encoded_records.as_slice()) {
                replacements.push((replacement_range, encoded_records));
            }
        }

        replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let mut previous_start = bytes.len();
        for (range, replacement) in replacements {
            if range.end > previous_start {
                return Err(invalid_tree_manager(
                    range.start,
                    "edited TreeManager field ranges overlap",
                ));
            }
            validate_range(&bytes, &range, range.len(), "edited TreeManager field")?;
            bytes.splice(range.clone(), replacement);
            previous_start = range.start;
        }
        Ok(bytes)
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.data.serialized_prefix
    }
}

fn encode_ushort_column(
    source: &[u8],
    range: &Range<usize>,
    values: &[u16],
    field: &str,
) -> Result<(Range<usize>, Vec<u8>), CrpError> {
    if range.start > range.end || range.end > source.len() {
        return Err(invalid_tree_manager(
            range.start,
            format!("{field} byte range is invalid"),
        ));
    }
    let mut encoded = Vec::new();
    write_encoded_ushort_array(&mut encoded, values);
    Ok((range.clone(), encoded))
}

fn validate_range(
    source: &[u8],
    range: &Range<usize>,
    expected_len: usize,
    field: &str,
) -> Result<(), CrpError> {
    if range.start > range.end || range.end > source.len() || range.len() != expected_len {
        return Err(invalid_tree_manager(
            range.start,
            format!("{field} byte range does not match its decoded values"),
        ));
    }
    Ok(())
}

fn read_prefab_names(
    reader: &mut Reader<'_>,
) -> Result<(Vec<Option<String>>, Vec<Range<usize>>), CrpError> {
    let count = usize::from(reader.read_u16_be()?);
    let mut names = Vec::with_capacity(count);
    let mut ranges = Vec::with_capacity(count);
    for _ in 0..count {
        let start = reader.offset();
        names.push(reader.read_unique_string()?);
        ranges.push(start..reader.offset());
    }
    Ok((names, ranges))
}

fn encode_name_replacements(
    source: &[u8],
    original: &[Option<String>],
    edited: &[Option<String>],
    ranges: &[Range<usize>],
    field: &str,
) -> Result<Vec<(Range<usize>, Vec<u8>)>, CrpError> {
    if original.len() != edited.len() || original.len() != ranges.len() {
        return Err(invalid_tree_manager(
            0,
            format!("{field} entry counts do not match the source"),
        ));
    }

    let mut replacements = Vec::new();
    for (index, ((before, after), range)) in original.iter().zip(edited).zip(ranges).enumerate() {
        if before == after {
            continue;
        }
        if range.start > range.end || range.end > source.len() {
            return Err(invalid_tree_manager(
                range.start,
                format!("{field} entry {index} byte range is invalid"),
            ));
        }
        let mut encoded = Vec::new();
        write_nullable_unique_string(&mut encoded, after.as_deref())?;
        replacements.push((range.clone(), encoded));
    }
    Ok(replacements)
}

fn invalid_tree_manager(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "TreeManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{BURNING_TREES_VERSION, BurningTreeRecord, TREE_CAPACITY, TreeManagerDataPrefix};
    use crate::data_serializer::write_u24_be;
    use crate::encoded_array::write_encoded_ushort_array;

    fn empty_prefix(format_version: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_encoded_ushort_array(&mut bytes, &vec![0; 262_143]);
        bytes.extend_from_slice(&[0, 0]); // Empty prefab-name scope.
        if format_version >= 266 {
            write_u24_be(&mut bytes, 0).unwrap();
        }
        bytes
    }

    #[test]
    fn empty_tree_manager_prefix_respects_burning_tree_gate() {
        for (version, has_burning_table) in [(194, false), (265, false), (266, true)] {
            let bytes = empty_prefix(version);
            let (tree_manager, used) =
                TreeManagerDataPrefix::decode_prefix(version, &bytes).unwrap();
            assert_eq!(used, bytes.len());
            assert_eq!(tree_manager.flags().len(), 262_143);
            assert!(tree_manager.trees().is_empty());
            assert_eq!(tree_manager.burning_trees().is_some(), has_burning_table);
            assert_eq!(tree_manager.encode(), bytes);
        }
    }

    #[test]
    fn tree_manager_clears_runtime_fire_flags_and_decodes_sparse_records() {
        let mut bytes = Vec::new();
        let mut flags = vec![0_u16; 262_143];
        flags[0] = 1;
        flags[8] = 0x40; // Cleared as a runtime-only burning-tree flag.
        flags[9] = 3;
        write_encoded_ushort_array(&mut bytes, &flags);
        write_encoded_ushort_array(&mut bytes, &[12, 14]);
        bytes.extend_from_slice(&[0, 1, 0x80, 0, 1, 212]);
        write_encoded_ushort_array(&mut bytes, &[0xfffe, 7]);
        write_encoded_ushort_array(&mut bytes, &[0x8001, 8]);

        let (tree_manager, used) = TreeManagerDataPrefix::decode_prefix(194, &bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(tree_manager.trees().len(), 2);
        assert_eq!(tree_manager.trees()[0].index, 1);
        assert_eq!(tree_manager.trees()[0].prefab_index, 12);
        assert_eq!(tree_manager.trees()[0].position_x, -2);
        assert_eq!(tree_manager.trees()[0].position_z, 0x8001_u16 as i16);
        assert_eq!(tree_manager.trees()[1].index, 10);
        assert_eq!(tree_manager.trees()[1].prefab_index, 14);
        assert_eq!(tree_manager.prefab_names(), &[Some("T".to_owned())]);
        assert_eq!(tree_manager.flags()[8], 0);
        assert_eq!(tree_manager.encode(), bytes);
    }

    #[test]
    fn tree_manager_editor_preserves_fire_bits_when_editing_raw_flags() {
        for format_version in [194, 266] {
            let mut serialized_flags = vec![0_u16; TREE_CAPACITY - 1];
            serialized_flags[0] = 1;
            serialized_flags[8] = 0x40;
            serialized_flags[9] = 3;
            let mut bytes = Vec::new();
            write_encoded_ushort_array(&mut bytes, &serialized_flags);
            let flags_len = bytes.len();
            write_encoded_ushort_array(&mut bytes, &[12, 14]);
            bytes.extend_from_slice(&[0, 0]); // Empty prefab-name scope.
            write_encoded_ushort_array(&mut bytes, &[10, 20]);
            write_encoded_ushort_array(&mut bytes, &[30, 40]);
            if format_version >= BURNING_TREES_VERSION {
                bytes.extend_from_slice(&[0, 0, 1, 0, 0, 10, 4, 5]);
            }

            let (trees, _) = TreeManagerDataPrefix::decode_prefix(format_version, &bytes).unwrap();
            assert_eq!(trees.flags()[8], 0);
            let mut editor = trees.into_editor();
            editor.set_flag_bits(0, 5).unwrap();
            editor.set_flag_bits(9, 2).unwrap();
            assert!(editor.set_flag_bits(0, 0).is_err());
            assert!(editor.set_flag_bits(8, 1).is_err());
            assert!(editor.set_flag_bits(0, 0x41).is_err());
            assert!(editor.set_flag_bits(TREE_CAPACITY - 1, 1).is_err());

            let mut expected_wire_flags = serialized_flags;
            expected_wire_flags[0] = 5;
            expected_wire_flags[9] = 2;
            let mut expected = Vec::new();
            write_encoded_ushort_array(&mut expected, &expected_wire_flags);
            expected.extend_from_slice(&bytes[flags_len..]);
            let encoded = editor.encode().unwrap();
            assert_eq!(encoded, expected);

            let (updated, consumed) =
                TreeManagerDataPrefix::decode_prefix(format_version, &encoded).unwrap();
            assert_eq!(consumed, encoded.len());
            assert_eq!(updated.flags()[0], 5);
            assert_eq!(updated.flags()[8], 0);
            assert_eq!(updated.flags()[9], 2);
            assert_eq!(
                updated
                    .trees()
                    .iter()
                    .map(|tree| tree.index)
                    .collect::<Vec<_>>(),
                [1, 10]
            );
            if format_version >= BURNING_TREES_VERSION {
                assert_eq!(
                    updated.burning_trees().unwrap(),
                    &[BurningTreeRecord {
                        tree_index: 10,
                        fire_intensity: 4,
                        fire_damage: 5,
                    }]
                );
            } else {
                assert_eq!(updated.burning_trees(), None);
            }
        }
    }

    #[test]
    fn burning_tree_table_decodes_twenty_four_bit_indices() {
        let mut bytes = empty_prefix(266);
        let count_offset = bytes.len() - 3;
        bytes[count_offset..].copy_from_slice(&[0, 0, 1]);
        write_u24_be(&mut bytes, 0x01_2345).unwrap();
        bytes.extend_from_slice(&[7, 9]);

        let (tree_manager, used) = TreeManagerDataPrefix::decode_prefix(266, &bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(
            tree_manager.burning_trees().unwrap(),
            &[super::BurningTreeRecord {
                tree_index: 0x01_2345,
                fire_intensity: 7,
                fire_damage: 9,
            }]
        );
        assert_eq!(tree_manager.encode(), bytes);
    }

    #[test]
    fn burning_tree_editor_patches_only_existing_fixed_width_records() {
        let mut bytes = empty_prefix(266);
        let count_offset = bytes.len() - 3;
        bytes[count_offset..].copy_from_slice(&[0, 0, 2]);
        write_u24_be(&mut bytes, 0x01_2345).unwrap();
        bytes.extend_from_slice(&[7, 9]);
        write_u24_be(&mut bytes, 0xab_cdef).unwrap();
        bytes.extend_from_slice(&[11, 13]);
        let record_offset = count_offset + 3;

        let (tree_manager, used) = TreeManagerDataPrefix::decode_prefix(266, &bytes).unwrap();
        assert_eq!(used, bytes.len());
        let mut editor = tree_manager.into_editor();
        let records = editor.burning_trees_mut().unwrap();
        records[0].tree_index = 0x02_3456;
        records[0].fire_intensity = 17;
        records[1].fire_damage = 19;

        let mut expected = bytes;
        expected[record_offset..record_offset + 5].copy_from_slice(&[0x02, 0x34, 0x56, 17, 9]);
        expected[record_offset + 9] = 19;
        assert_eq!(editor.encode().unwrap(), expected);

        let (updated, _) = TreeManagerDataPrefix::decode_prefix(266, &expected).unwrap();
        assert_eq!(updated.burning_trees().unwrap()[0].tree_index, 0x02_3456);
        assert_eq!(updated.burning_trees().unwrap()[1].fire_damage, 19);
    }

    #[test]
    fn burning_tree_editor_grows_and_shrinks_the_record_list() {
        let mut source = empty_prefix(266);
        let count_offset = source.len() - 3;
        source[count_offset..].copy_from_slice(&[0, 0, 1]);
        write_u24_be(&mut source, 7).unwrap();
        source.extend_from_slice(&[8, 9]);

        let (tree_manager, _) = TreeManagerDataPrefix::decode_prefix(266, &source).unwrap();
        let mut grow_editor = tree_manager.into_editor();
        grow_editor
            .set_burning_trees(vec![
                BurningTreeRecord {
                    tree_index: 7,
                    fire_intensity: 8,
                    fire_damage: 9,
                },
                BurningTreeRecord {
                    tree_index: 0x01_0203,
                    fire_intensity: 10,
                    fire_damage: 11,
                },
            ])
            .unwrap();

        let mut expected_grown = source;
        expected_grown.splice(count_offset.., [0, 0, 2, 0, 0, 7, 8, 9, 1, 2, 3, 10, 11]);
        assert_eq!(grow_editor.encode().unwrap(), expected_grown);

        let (grown, _) = TreeManagerDataPrefix::decode_prefix(266, &expected_grown).unwrap();
        let mut shrink_editor = grown.into_editor();
        shrink_editor.set_burning_trees(Vec::new()).unwrap();
        let mut expected_shrunk = expected_grown;
        expected_shrunk.truncate(count_offset);
        expected_shrunk.extend_from_slice(&[0, 0, 0]);
        assert_eq!(shrink_editor.encode().unwrap(), expected_shrunk);
    }

    #[test]
    fn burning_tree_editor_rejects_unavailable_or_oversized_lists() {
        let (legacy_tree_manager, _) =
            TreeManagerDataPrefix::decode_prefix(265, &empty_prefix(265)).unwrap();
        assert!(
            legacy_tree_manager
                .into_editor()
                .set_burning_trees(Vec::new())
                .is_err()
        );

        let (tree_manager, _) =
            TreeManagerDataPrefix::decode_prefix(266, &empty_prefix(266)).unwrap();
        let oversized = vec![
            BurningTreeRecord {
                tree_index: 1,
                fire_intensity: 0,
                fire_damage: 0,
            };
            TREE_CAPACITY
        ];
        assert!(
            tree_manager
                .into_editor()
                .set_burning_trees(oversized)
                .is_err()
        );
    }

    #[test]
    fn burning_tree_editor_rejects_indexes_wider_than_twenty_four_bits() {
        let bytes = {
            let mut bytes = empty_prefix(266);
            let count_offset = bytes.len() - 3;
            bytes[count_offset..].copy_from_slice(&[0, 0, 1]);
            write_u24_be(&mut bytes, 1).unwrap();
            bytes.extend_from_slice(&[2, 3]);
            bytes
        };
        let (tree_manager, _) = TreeManagerDataPrefix::decode_prefix(266, &bytes).unwrap();
        let mut editor = tree_manager.into_editor();
        editor.burning_trees_mut().unwrap()[0].tree_index = 0x0100_0000;
        assert!(editor.encode().is_err());
    }

    #[test]
    fn active_tree_editor_reencodes_only_changed_columns() {
        let mut flags = vec![0_u16; 262_143];
        flags[..3].fill(1);
        let mut source = Vec::new();
        write_encoded_ushort_array(&mut source, &flags);
        write_encoded_ushort_array(&mut source, &[10, 20, 30]);
        source.extend_from_slice(&[0, 0]); // Empty prefab-name scope.
        write_encoded_ushort_array(&mut source, &[10, 20, 30]);
        write_encoded_ushort_array(&mut source, &[0xfffe, 7, 12]);
        let source_z = source[source.len() - 7..].to_vec();

        let (tree_manager, _) = TreeManagerDataPrefix::decode_prefix(194, &source).unwrap();
        let mut editor = tree_manager.into_editor();
        assert_eq!(editor.encode().unwrap(), source);
        for tree in editor.trees_mut() {
            tree.prefab_index = 0;
            tree.position_x = 0;
        }

        let mut expected = Vec::new();
        write_encoded_ushort_array(&mut expected, &flags);
        expected.push(0x82); // Canonical encoding of three zero prefab indexes.
        expected.extend_from_slice(&[0, 0]);
        expected.push(0x82); // Canonical encoding of three zero x positions.
        expected.extend_from_slice(&source_z);
        let encoded = editor.encode().unwrap();
        assert_eq!(encoded, expected);
        assert!(encoded.len() < source.len());

        let (updated, used) = TreeManagerDataPrefix::decode_prefix(194, &encoded).unwrap();
        assert_eq!(used, encoded.len());
        assert!(
            updated
                .trees()
                .iter()
                .all(|tree| tree.prefab_index == 0 && tree.position_x == 0)
        );
        assert_eq!(updated.trees()[0].position_z, -2);
        assert_eq!(updated.trees()[2].position_z, 12);
    }

    #[test]
    fn tree_manager_editor_changes_existing_prefab_names_exactly() {
        let mut flags = vec![0_u16; TREE_CAPACITY - 1];
        flags[..2].fill(1);
        let mut source = Vec::new();
        write_encoded_ushort_array(&mut source, &flags);
        write_encoded_ushort_array(&mut source, &[12, 14]);
        source.extend_from_slice(&[0, 2]); // Two tree prefab names.
        source.extend_from_slice(&[0, 0, 1, 0xff, 0xd0]); // Wide "P".
        source.push(0xff); // Null name.
        write_encoded_ushort_array(&mut source, &[1, 2]);
        write_encoded_ushort_array(&mut source, &[3, 4]);

        let (tree_manager, consumed) = TreeManagerDataPrefix::decode_prefix(194, &source).unwrap();
        assert_eq!(consumed, source.len());
        assert_eq!(tree_manager.prefab_names(), &[Some("P".to_owned()), None]);
        let name_ranges = tree_manager.prefab_name_ranges.clone();
        let mut editor = tree_manager.into_editor();
        editor.prefab_names_mut()[0] = None;
        editor.prefab_names_mut()[1] = Some("A".to_owned());

        let mut expected = source.clone();
        expected.splice(name_ranges[1].clone(), [0x80, 0, 1, 0xc1]);
        expected.splice(name_ranges[0].clone(), [0xff]);
        let encoded = editor.encode().unwrap();
        assert_eq!(encoded, expected);

        let (updated, consumed) = TreeManagerDataPrefix::decode_prefix(194, &encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(updated.prefab_names(), &[None, Some("A".to_owned())]);
        assert_eq!(updated.trees()[0].prefab_index, 12);
        assert_eq!(updated.trees()[1].prefab_index, 14);
    }

    #[test]
    fn active_tree_editor_rejects_changes_to_flag_selected_slots() {
        let mut flags = vec![0_u16; 262_143];
        flags[0] = 1;
        let mut source = Vec::new();
        write_encoded_ushort_array(&mut source, &flags);
        write_encoded_ushort_array(&mut source, &[1]);
        source.extend_from_slice(&[0, 0]);
        write_encoded_ushort_array(&mut source, &[2]);
        write_encoded_ushort_array(&mut source, &[3]);

        let (tree_manager, _) = TreeManagerDataPrefix::decode_prefix(194, &source).unwrap();
        let mut editor = tree_manager.into_editor();
        editor.trees_mut()[0].index = 2;
        assert!(editor.encode().is_err());
    }

    #[test]
    fn tree_manager_rejects_truncated_fields_and_old_versions() {
        let bytes = empty_prefix(266);
        assert!(TreeManagerDataPrefix::decode_prefix(266, &bytes[..bytes.len() - 1]).is_err());
        assert!(TreeManagerDataPrefix::decode_prefix(163, &[]).is_err());

        let mut oversized_burning_list = empty_prefix(266);
        let count_offset = oversized_burning_list.len() - 3;
        oversized_burning_list[count_offset..].copy_from_slice(&[0x04, 0, 0]);
        assert!(TreeManagerDataPrefix::decode_prefix(266, &oversized_burning_list).is_err());
    }
}
