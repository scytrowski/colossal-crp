use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    read_encoded_bool_array, read_encoded_byte_array, read_encoded_ushort_array,
};

const CELL_COUNT: usize = 65_536;
const NODE_GROUP_COUNT: usize = 32_768;
const LEGACY_NODE_GROUP_COUNT: usize = 16_384;
const PULSE_GROUP_CAPACITY: usize = 1_024;
const PULSE_UNIT_CAPACITY: usize = 32_768;
const END_PROFILER_REFERENCE_LEN: usize = 2;
const TEMPORARY_ELECTRIFIED_BOOL_VERSION: u32 = 76;
const PULSE_UNIT_NODE_VERSION: u32 = 8;
const FULL_NODE_GROUPS_VERSION: u32 = 202;

/// A serialized active electricity pulse group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElectricityPulseGroup {
    /// First serialized charge value.
    pub charge_value_1: u32,
    /// Second serialized charge value.
    pub charge_value_2: u32,
    /// First serialized merge value.
    pub merge_value_1: u16,
    /// Second serialized merge value.
    pub merge_value_2: u16,
    /// X coordinate in the simulation grid.
    pub x: u8,
    /// Z coordinate in the simulation grid.
    pub z: u8,
}

/// A serialized active electricity pulse unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElectricityPulseUnit {
    /// Pulse group referenced by this unit.
    pub group: u16,
    /// Node referenced by this unit, absent in inner versions below 8.
    pub node: Option<u16>,
    /// X coordinate in the simulation grid.
    pub x: u8,
    /// Z coordinate in the simulation grid.
    pub z: u8,
}

/// Decoded reader fields from `ElectricityManager+Data`.
///
/// Sparse cell columns use the order of [`Self::conductive_cells`]. The source
/// body remains available for exact unchanged writing; no writer is provided
/// for edited electricity state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElectricityManagerData {
    format_version: u32,
    conductivity: Vec<u8>,
    conductive_cells: Vec<usize>,
    current_charge: Vec<i16>,
    extra_charge: Vec<u16>,
    pulse_group_indexes: Vec<u16>,
    electrified: Vec<bool>,
    temporary_electrified: Vec<bool>,
    pulse_groups: Vec<ElectricityPulseGroup>,
    pulse_units: Vec<ElectricityPulseUnit>,
    node_groups: Vec<u16>,
    processed_cell_count: i32,
    conductive_cell_count: i32,
    can_continue: bool,
    scalar_fields_offset: usize,
    original_body: Vec<u8>,
}

impl ElectricityManagerData {
    /// Decodes the complete bounded body, including its two-byte closing
    /// profiler reference. Version-dependent temporary-electrified, pulse-unit,
    /// and node-group layouts follow the inner DataSerializer version.
    /// Truncated fields, excessive pulse counts, or extra bytes return errors.
    pub fn decode(format_version: u32, bytes: &[u8]) -> Result<Self, CrpError> {
        let mut reader = Reader::new(bytes);
        let conductivity = read_encoded_byte_array(&mut reader, CELL_COUNT)?;
        let conductive_cells = conductivity
            .iter()
            .enumerate()
            .filter_map(|(index, value)| (*value != 0).then_some(index))
            .collect::<Vec<_>>();
        let conductive_count = conductive_cells.len();

        let current_charge = read_encoded_ushort_array(&mut reader, conductive_count)?
            .into_iter()
            .map(|value| value as i16)
            .collect();
        let extra_charge = read_encoded_ushort_array(&mut reader, conductive_count)?;
        let pulse_group_indexes = read_encoded_ushort_array(&mut reader, conductive_count)?;
        if format_version == 75 {
            // The version-75 reader consumes this sparse compatibility column
            // but does not assign it to a field.
            read_encoded_ushort_array(&mut reader, conductive_count)?;
        }
        let electrified = read_encoded_bool_array(&mut reader, conductive_count)?;
        let temporary_electrified = if format_version < PULSE_UNIT_NODE_VERSION {
            vec![false; conductive_count]
        } else if format_version < TEMPORARY_ELECTRIFIED_BOOL_VERSION {
            read_encoded_byte_array(&mut reader, conductive_count)?
                .into_iter()
                .map(|value| value != 0)
                .collect()
        } else {
            read_encoded_bool_array(&mut reader, conductive_count)?
        };

        let pulse_group_count = usize::from(reader.read_u16_be()?);
        if pulse_group_count > PULSE_GROUP_CAPACITY {
            return Err(reader.invalid(format!(
                "electricity pulse-group count {pulse_group_count} exceeds capacity {PULSE_GROUP_CAPACITY}"
            )));
        }
        let mut pulse_groups = Vec::with_capacity(pulse_group_count);
        for _ in 0..pulse_group_count {
            pulse_groups.push(ElectricityPulseGroup {
                charge_value_1: reader.read_u32_be()?,
                charge_value_2: reader.read_u32_be()?,
                merge_value_1: reader.read_u16_be()?,
                merge_value_2: reader.read_u16_be()?,
                x: reader.read_u8()?,
                z: reader.read_u8()?,
            });
        }

        let pulse_unit_count = usize::from(reader.read_u16_be()?);
        if pulse_unit_count > PULSE_UNIT_CAPACITY {
            return Err(reader.invalid(format!(
                "electricity pulse-unit count {pulse_unit_count} exceeds capacity {PULSE_UNIT_CAPACITY}"
            )));
        }
        let mut pulse_units = Vec::with_capacity(pulse_unit_count);
        for _ in 0..pulse_unit_count {
            pulse_units.push(ElectricityPulseUnit {
                group: reader.read_u16_be()?,
                node: if format_version >= PULSE_UNIT_NODE_VERSION {
                    Some(reader.read_u16_be()?)
                } else {
                    None
                },
                x: reader.read_u8()?,
                z: reader.read_u8()?,
            });
        }

        let node_groups = if format_version < PULSE_UNIT_NODE_VERSION {
            vec![u16::MAX; NODE_GROUP_COUNT]
        } else {
            let stored_count = if format_version >= FULL_NODE_GROUPS_VERSION {
                NODE_GROUP_COUNT
            } else {
                LEGACY_NODE_GROUP_COUNT
            };
            let mut values = vec![u16::MAX; NODE_GROUP_COUNT];
            values[..stored_count]
                .copy_from_slice(&read_encoded_ushort_array(&mut reader, stored_count)?);
            values
        };

        let scalar_fields_offset = reader.offset();
        let processed_cell_count = reader.read_i32_be()?;
        let conductive_cell_count = reader.read_i32_be()?;
        let can_continue = reader.read_u8()? != 0;
        if reader.remaining() != END_PROFILER_REFERENCE_LEN {
            return Err(reader.invalid(format!(
                "ElectricityManager fields leave {} bytes before the next manager; expected the two-byte closing profiler reference",
                reader.remaining()
            )));
        }
        reader.read_bytes(END_PROFILER_REFERENCE_LEN)?;

        Ok(Self {
            format_version,
            conductivity,
            conductive_cells,
            current_charge,
            extra_charge,
            pulse_group_indexes,
            electrified,
            temporary_electrified,
            pulse_groups,
            pulse_units,
            node_groups,
            processed_cell_count,
            conductive_cell_count,
            can_continue,
            scalar_fields_offset,
            original_body: bytes.to_vec(),
        })
    }

    /// Returns the inner DataSerializer version used for reader gates.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the full 65,536-cell conductivity plane.
    pub fn conductivity(&self) -> &[u8] {
        &self.conductivity
    }

    /// Returns cell indexes with nonzero conductivity, in ascending order.
    pub fn conductive_cells(&self) -> &[usize] {
        &self.conductive_cells
    }

    /// Returns signed current charge values in conductive-cell order.
    pub fn current_charge(&self) -> &[i16] {
        &self.current_charge
    }

    /// Returns extra charge values in conductive-cell order.
    pub fn extra_charge(&self) -> &[u16] {
        &self.extra_charge
    }

    /// Returns pulse-group indexes in conductive-cell order.
    pub fn pulse_group_indexes(&self) -> &[u16] {
        &self.pulse_group_indexes
    }

    /// Returns electrified flags in conductive-cell order.
    pub fn electrified(&self) -> &[bool] {
        &self.electrified
    }

    /// Returns temporary-electrified flags in conductive-cell order.
    pub fn temporary_electrified(&self) -> &[bool] {
        &self.temporary_electrified
    }

    /// Returns active pulse groups in serialized order.
    pub fn pulse_groups(&self) -> &[ElectricityPulseGroup] {
        &self.pulse_groups
    }

    /// Returns active pulse units in serialized order.
    pub fn pulse_units(&self) -> &[ElectricityPulseUnit] {
        &self.pulse_units
    }

    /// Returns all 32,768 node-group values, including version-derived defaults.
    pub fn node_groups(&self) -> &[u16] {
        &self.node_groups
    }

    /// Returns the stored processed-cell counter.
    pub fn processed_cell_count(&self) -> i32 {
        self.processed_cell_count
    }

    /// Returns the stored conductive-cell counter.
    pub fn conductive_cell_count(&self) -> i32 {
        self.conductive_cell_count
    }

    /// Returns the serialized continuation flag.
    pub fn can_continue(&self) -> bool {
        self.can_continue
    }

    /// Replaces the stored processed-cell counter without changing the body length.
    pub fn set_processed_cell_count(&mut self, value: i32) {
        self.processed_cell_count = value;
    }

    /// Replaces the stored conductive-cell counter without changing the body length.
    pub fn set_conductive_cell_count(&mut self, value: i32) {
        self.conductive_cell_count = value;
    }

    /// Replaces the serialized continuation flag without changing the body length.
    pub fn set_can_continue(&mut self, value: bool) {
        self.can_continue = value;
    }

    /// Returns whether the game reader derives `refreshGrid` as true.
    pub fn refresh_grid(&self) -> bool {
        self.format_version < TEMPORARY_ELECTRIFIED_BOOL_VERSION
    }

    /// Returns the complete source body unchanged, including the closing
    /// profiler reference.
    pub fn encode_unchanged(&self) -> Vec<u8> {
        self.original_body.clone()
    }

    /// Encodes edited counters and the continuation flag while preserving every
    /// other byte, including encoded network columns and the closing profiler
    /// reference. Returns an error if the captured scalar field range is invalid.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut bytes = self.original_body.clone();
        let processed_end =
            self.scalar_fields_offset
                .checked_add(4)
                .ok_or(CrpError::ArithmeticOverflow(
                    "ElectricityManager processed-cell counter end",
                ))?;
        let conductive_end = processed_end
            .checked_add(4)
            .ok_or(CrpError::ArithmeticOverflow(
                "ElectricityManager conductive-cell counter end",
            ))?;
        let flag_end = conductive_end
            .checked_add(1)
            .ok_or(CrpError::ArithmeticOverflow(
                "ElectricityManager continuation flag end",
            ))?;
        if flag_end
            .checked_add(END_PROFILER_REFERENCE_LEN)
            .is_none_or(|end| end != bytes.len())
        {
            return Err(CrpError::InvalidAssetPayload {
                name: "ElectricityManager+Data".to_owned(),
                offset: self.scalar_fields_offset,
                reason: "scalar fields do not end before the closing profiler reference".to_owned(),
            });
        }
        bytes
            .get_mut(self.scalar_fields_offset..processed_end)
            .ok_or_else(|| CrpError::InvalidAssetPayload {
                name: "ElectricityManager+Data".to_owned(),
                offset: self.scalar_fields_offset,
                reason: "processed-cell counter range is invalid".to_owned(),
            })?
            .copy_from_slice(&self.processed_cell_count.to_be_bytes());
        bytes
            .get_mut(processed_end..conductive_end)
            .ok_or_else(|| CrpError::InvalidAssetPayload {
                name: "ElectricityManager+Data".to_owned(),
                offset: processed_end,
                reason: "conductive-cell counter range is invalid".to_owned(),
            })?
            .copy_from_slice(&self.conductive_cell_count.to_be_bytes());
        bytes
            .get_mut(conductive_end..flag_end)
            .ok_or_else(|| CrpError::InvalidAssetPayload {
                name: "ElectricityManager+Data".to_owned(),
                offset: conductive_end,
                reason: "continuation flag range is invalid".to_owned(),
            })?
            .copy_from_slice(&[u8::from(self.can_continue)]);
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CELL_COUNT, END_PROFILER_REFERENCE_LEN, ElectricityManagerData, FULL_NODE_GROUPS_VERSION,
        NODE_GROUP_COUNT, PULSE_GROUP_CAPACITY, PULSE_UNIT_CAPACITY,
    };
    fn append_zero_encoded_byte_array(output: &mut Vec<u8>, mut count: usize) {
        while count > 0 {
            let run = count.min(128);
            output.push(0x7f + run as u8);
            count -= run;
        }
    }

    fn append_literal_byte_array(output: &mut Vec<u8>, values: &[u8]) {
        for chunk in values.chunks(128) {
            output.push((chunk.len() - 1) as u8);
            output.extend_from_slice(chunk);
        }
    }

    fn empty_body(format_version: u32) -> Vec<u8> {
        let mut body = Vec::new();
        append_zero_encoded_byte_array(&mut body, CELL_COUNT);
        body.extend_from_slice(&[0; 4]); // Empty pulse-group and pulse-unit tables.
        let node_group_count = if format_version < 8 {
            0
        } else if format_version >= FULL_NODE_GROUPS_VERSION {
            NODE_GROUP_COUNT
        } else {
            NODE_GROUP_COUNT / 2
        };
        if node_group_count > 0 {
            append_zero_encoded_ushort_array(&mut body, node_group_count);
        }
        body.extend_from_slice(&[0; 8]); // Counters.
        body.push(1); // canContinue.
        body.extend_from_slice(&[0x12, 0x34]);
        body
    }

    fn append_zero_encoded_ushort_array(output: &mut Vec<u8>, mut count: usize) {
        while count > 0 {
            let run = count.min(128);
            output.push(0x7f + run as u8);
            count -= run;
        }
    }

    #[test]
    fn empty_network_decodes_current_layout_and_preserves_body() {
        let body = empty_body(117_000);
        let data = ElectricityManagerData::decode(117_000, &body).unwrap();
        assert_eq!(data.conductivity().len(), CELL_COUNT);
        assert!(data.conductive_cells().is_empty());
        assert!(data.current_charge().is_empty());
        assert!(data.temporary_electrified().is_empty());
        assert_eq!(data.node_groups().len(), NODE_GROUP_COUNT);
        assert_eq!(data.node_groups()[0], 0);
        assert!(data.can_continue());
        assert!(!data.refresh_grid());
        assert_eq!(data.encode_unchanged(), body);
    }

    #[test]
    fn decodes_nonempty_current_fields_against_expected_values() {
        let mut conductivity = vec![0; CELL_COUNT];
        conductivity[5] = 1;
        conductivity[10] = 2;
        let mut body = Vec::new();
        append_literal_byte_array(&mut body, &conductivity);
        body.extend_from_slice(&[1, 0xff, 0xff, 0x12, 0x34]);
        body.extend_from_slice(&[1, 0, 3, 0, 4]);
        body.extend_from_slice(&[1, 0, 7, 0, 8]);
        body.extend_from_slice(&[0, 1]); // EncodedArray.Bool: packed true, false.
        body.extend_from_slice(&[0, 2]); // EncodedArray.Bool: packed false, true.
        body.extend_from_slice(&1_u16.to_be_bytes());
        body.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
        body.extend_from_slice(&0x0506_0708_u32.to_be_bytes());
        body.extend_from_slice(&9_u16.to_be_bytes());
        body.extend_from_slice(&10_u16.to_be_bytes());
        body.extend_from_slice(&[11, 12]);
        body.extend_from_slice(&1_u16.to_be_bytes());
        body.extend_from_slice(&13_u16.to_be_bytes());
        body.extend_from_slice(&14_u16.to_be_bytes());
        body.extend_from_slice(&[15, 16]);
        append_zero_encoded_ushort_array(&mut body, NODE_GROUP_COUNT);
        body.extend_from_slice(&(-17_i32).to_be_bytes());
        body.extend_from_slice(&18_i32.to_be_bytes());
        body.push(1);
        body.extend_from_slice(&[0x12, 0x34]);

        let data = ElectricityManagerData::decode(117_000, &body).unwrap();
        assert_eq!(data.conductive_cells(), [5, 10]);
        assert_eq!(data.current_charge(), [-1, 0x1234]);
        assert_eq!(data.extra_charge(), [3, 4]);
        assert_eq!(data.pulse_group_indexes(), [7, 8]);
        assert_eq!(data.electrified(), [true, false]);
        assert_eq!(data.temporary_electrified(), [false, true]);
        assert_eq!(data.pulse_groups().len(), 1);
        assert_eq!(data.pulse_groups()[0].charge_value_1, 0x0102_0304);
        assert_eq!(data.pulse_groups()[0].charge_value_2, 0x0506_0708);
        assert_eq!(data.pulse_groups()[0].merge_value_1, 9);
        assert_eq!(data.pulse_groups()[0].x, 11);
        assert_eq!(data.pulse_units()[0].group, 13);
        assert_eq!(data.pulse_units()[0].node, Some(14));
        assert_eq!(data.pulse_units()[0].z, 16);
        assert_eq!(data.processed_cell_count(), -17);
        assert_eq!(data.conductive_cell_count(), 18);
        assert!(data.can_continue());
        assert_eq!(data.encode_unchanged(), body);
    }

    #[test]
    fn edits_scalar_tail_using_big_endian_values_and_preserves_other_bytes() {
        let body = empty_body(117_000);
        let mut data = ElectricityManagerData::decode(117_000, &body).unwrap();
        data.set_processed_cell_count(-17);
        data.set_conductive_cell_count(0x0102_0304);
        data.set_can_continue(false);

        let encoded = data.encode().unwrap();
        let scalar_start = body.len() - END_PROFILER_REFERENCE_LEN - 9;
        assert_eq!(&encoded[..scalar_start], &body[..scalar_start]);
        assert_eq!(
            &encoded[scalar_start..scalar_start + 4],
            &(-17_i32).to_be_bytes()
        );
        assert_eq!(
            &encoded[scalar_start + 4..scalar_start + 8],
            &0x0102_0304_i32.to_be_bytes()
        );
        assert_eq!(encoded[scalar_start + 8], 0);
        assert_eq!(&encoded[scalar_start + 9..], &body[scalar_start + 9..]);
        assert_eq!(data.encode_unchanged(), body);

        let decoded = ElectricityManagerData::decode(117_000, &encoded).unwrap();
        assert_eq!(decoded.processed_cell_count(), -17);
        assert_eq!(decoded.conductive_cell_count(), 0x0102_0304);
        assert!(!decoded.can_continue());
    }

    #[test]
    fn legacy_version_gates_fill_or_read_expected_columns() {
        for (version, expected_node_default, expected_refresh) in [
            (7, u16::MAX, true),
            (8, 0, true),
            (201, 0, false),
            (202, 0, false),
        ] {
            let data = ElectricityManagerData::decode(version, &empty_body(version)).unwrap();
            assert_eq!(data.node_groups().len(), NODE_GROUP_COUNT);
            assert_eq!(data.node_groups()[0], expected_node_default);
            assert_eq!(data.refresh_grid(), expected_refresh);
        }

        let mut version_75 = Vec::new();
        let mut conductivity = vec![0; CELL_COUNT];
        conductivity[0] = 1;
        append_literal_byte_array(&mut version_75, &conductivity);
        for _ in 0..4 {
            version_75.extend_from_slice(&[0, 0, 0]);
        }
        version_75.extend_from_slice(&[0, 0]); // One false packed Bool.
        version_75.extend_from_slice(&[0, 0]); // One legacy false Byte.
        version_75.extend_from_slice(&[0; 4]);
        append_zero_encoded_ushort_array(&mut version_75, NODE_GROUP_COUNT / 2);
        version_75.extend_from_slice(&[0; 9]);
        version_75.extend_from_slice(&[0x12, 0x34]);
        let data = ElectricityManagerData::decode(75, &version_75).unwrap();
        assert!(!data.temporary_electrified()[0]);
        assert_eq!(data.node_groups()[NODE_GROUP_COUNT / 2], u16::MAX);
    }

    #[test]
    fn pulse_unit_node_field_starts_at_inner_version_eight() {
        for (version, node) in [(7, None), (8, Some(0x1234_u16))] {
            let mut body = Vec::new();
            append_zero_encoded_byte_array(&mut body, CELL_COUNT);
            body.extend_from_slice(&0_u16.to_be_bytes()); // No pulse groups.
            body.extend_from_slice(&1_u16.to_be_bytes()); // One pulse unit.
            body.extend_from_slice(&0x0102_u16.to_be_bytes());
            if let Some(node) = node {
                body.extend_from_slice(&node.to_be_bytes());
            }
            body.extend_from_slice(&[3, 4]);
            if version >= 8 {
                append_zero_encoded_ushort_array(&mut body, NODE_GROUP_COUNT / 2);
            }
            body.extend_from_slice(&[0; 9]);
            body.extend_from_slice(&[0x12, 0x34]);

            let data = ElectricityManagerData::decode(version, &body).unwrap();
            assert_eq!(data.pulse_units().len(), 1);
            assert_eq!(data.pulse_units()[0].group, 0x0102);
            assert_eq!(data.pulse_units()[0].node, node);
            assert_eq!(data.pulse_units()[0].x, 3);
            assert_eq!(data.pulse_units()[0].z, 4);
        }
    }

    #[test]
    fn rejects_over_capacity_pulse_counts_and_bad_boundaries() {
        assert!(PULSE_GROUP_CAPACITY < usize::from(u16::MAX));
        assert!(PULSE_UNIT_CAPACITY < usize::from(u16::MAX));
        let current = empty_body(202);
        assert!(ElectricityManagerData::decode(202, &current[..current.len() - 1]).is_err());

        let mut too_many_groups = empty_body(202);
        let groups_start = too_many_groups.len() - 2 - 9 - (NODE_GROUP_COUNT / 128) - 2 - 2;
        too_many_groups[groups_start..groups_start + 2]
            .copy_from_slice(&((PULSE_GROUP_CAPACITY + 1) as u16).to_be_bytes());
        assert!(ElectricityManagerData::decode(202, &too_many_groups).is_err());

        let mut too_many_units = empty_body(202);
        let units_start = too_many_units.len() - 2 - 9 - (NODE_GROUP_COUNT / 128) - 2;
        too_many_units[units_start..units_start + 2]
            .copy_from_slice(&((PULSE_UNIT_CAPACITY + 1) as u16).to_be_bytes());
        assert!(ElectricityManagerData::decode(202, &too_many_units).is_err());

        let mut overlong = current;
        overlong.push(0);
        assert!(ElectricityManagerData::decode(202, &overlong).is_err());
    }
}
