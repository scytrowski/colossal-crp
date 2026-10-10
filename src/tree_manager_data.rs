use crate::CrpError;
use crate::data_serializer::Reader;
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
    prefab_names: Vec<Option<String>>,
    trees: Vec<TreeInstanceRecord>,
    prefab_indices_range: Range<usize>,
    position_x_range: Range<usize>,
    position_z_range: Range<usize>,
    burning_trees: Option<Vec<BurningTreeRecord>>,
    burning_trees_range: Option<Range<usize>>,
    serialized_prefix: Vec<u8>,
}

/// Editor for existing active trees and burning-tree records.
///
/// Tree slots and record counts remain unchanged. Changed `EncodedArray.UShort`
/// columns use the canonical encoding; unchanged columns and all other bytes
/// retain their source representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeManagerEditor {
    data: TreeManagerDataPrefix,
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
        let serialized_flags = read_encoded_ushort_array(&mut reader, TREE_CAPACITY - 1)?;
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
        let prefab_names = read_prefab_names(&mut reader)?;
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
                prefab_names,
                trees,
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

    /// Creates an editor for existing active trees and burning-tree records.
    pub fn into_editor(self) -> TreeManagerEditor {
        let original_trees = self.trees.clone();
        TreeManagerEditor {
            data: self,
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

    /// Returns mutable active tree records without changing their count or slots.
    pub fn trees_mut(&mut self) -> &mut [TreeInstanceRecord] {
        &mut self.data.trees
    }

    /// Returns mutable burning-tree records without changing their count.
    /// Returns `None` for versions before 266.
    pub fn burning_trees_mut(&mut self) -> Option<&mut [BurningTreeRecord]> {
        self.data.burning_trees.as_deref_mut()
    }

    /// Writes edited fields while retaining unchanged source bytes.
    ///
    /// Changing prefab indexes or positions re-encodes only the affected
    /// `EncodedArray.UShort` column. That column may change length. Burning-tree
    /// records remain fixed-width, and tree slots and record counts cannot change.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut bytes = self.data.serialized_prefix.clone();
        if self.data.trees.len() != self.original_trees.len() {
            return Err(invalid_tree_manager(
                0,
                "active tree record count does not match the source",
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
            let expected_len = records
                .len()
                .checked_mul(5)
                .ok_or(CrpError::ArithmeticOverflow("burning tree record bytes"))?;
            validate_range(&bytes, range, expected_len, "burning-tree records")?;
            let mut encoded_records = Vec::with_capacity(expected_len);
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
            if bytes.get(range.clone()) != Some(encoded_records.as_slice()) {
                replacements.push((range.clone(), encoded_records));
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

fn read_prefab_names(reader: &mut Reader<'_>) -> Result<Vec<Option<String>>, CrpError> {
    let count = usize::from(reader.read_u16_be()?);
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        names.push(reader.read_unique_string()?);
    }
    Ok(names)
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
    use super::TreeManagerDataPrefix;
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
