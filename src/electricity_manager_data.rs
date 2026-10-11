use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    read_encoded_bool_array, read_encoded_byte_array, read_encoded_ushort_array,
    write_encoded_bool_array, write_encoded_byte_array, write_encoded_ushort_array,
};
use std::ops::Range;

const CELL_COUNT: usize = 65_536;
const NODE_GROUP_COUNT: usize = 32_768;
const LEGACY_NODE_GROUP_COUNT: usize = 16_384;
const PULSE_GROUP_CAPACITY: usize = 1_024;
const PULSE_UNIT_CAPACITY: usize = 32_768;
const END_PROFILER_REFERENCE_LEN: usize = 2;
const TEMPORARY_ELECTRIFIED_BOOL_VERSION: u32 = 76;
const PULSE_UNIT_NODE_VERSION: u32 = 8;
const FULL_NODE_GROUPS_VERSION: u32 = 202;

#[derive(Debug, Clone, PartialEq, Eq)]
struct EncodedArraySnapshot<T> {
    range: Range<usize>,
    values: Vec<T>,
}

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

/// All serialized sparse fields for one active electricity cell.
///
/// `conductivity` must be nonzero. Versions 8–75 require the original raw byte
/// for temporary electrification; exactly version 75 also requires the value
/// of its otherwise-unmodeled UShort column. These raw values are retained so
/// topology edits do not discard legacy serialized data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElectricityConductiveCellData {
    /// Nonzero value in the conductivity plane.
    pub conductivity: u8,
    /// Signed current charge.
    pub current_charge: i16,
    /// Extra charge.
    pub extra_charge: u16,
    /// Pulse-group index.
    pub pulse_group_index: u16,
    /// Electrification flag.
    pub electrified: bool,
    /// Temporary-electrification state. Versions below 8 require false.
    pub temporary_electrified: bool,
    /// Exact serialized temporary-electrification byte from versions 8–75.
    pub legacy_temporary_electrified_byte: Option<u8>,
    /// Exact extra sparse UShort value from version 75.
    pub version_75_compatibility_value: Option<u16>,
}

/// Decoded reader fields from `ElectricityManager+Data`.
///
/// Sparse cell columns use the order of [`Self::conductive_cells`]. Conductive
/// cells can be inserted and removed with all their sparse values. The source
/// body remains available for exact unchanged writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElectricityManagerData {
    format_version: u32,
    conductivity: Vec<u8>,
    conductivity_snapshot: EncodedArraySnapshot<u8>,
    conductive_cells: Vec<usize>,
    current_charge: Vec<i16>,
    current_charge_snapshot: EncodedArraySnapshot<i16>,
    extra_charge: Vec<u16>,
    extra_charge_snapshot: EncodedArraySnapshot<u16>,
    pulse_group_indexes: Vec<u16>,
    pulse_group_indexes_snapshot: EncodedArraySnapshot<u16>,
    version_75_compatibility_values: Option<Vec<u16>>,
    version_75_compatibility_snapshot: Option<EncodedArraySnapshot<u16>>,
    electrified: Vec<bool>,
    electrified_snapshot: EncodedArraySnapshot<bool>,
    temporary_electrified: Vec<bool>,
    legacy_temporary_electrified_bytes: Option<Vec<u8>>,
    legacy_temporary_electrified_snapshot: Option<EncodedArraySnapshot<u8>>,
    temporary_electrified_snapshot: Option<EncodedArraySnapshot<bool>>,
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
        let (conductivity, conductivity_snapshot) =
            read_captured_byte_array(&mut reader, CELL_COUNT)?;
        let conductive_cells = conductivity
            .iter()
            .enumerate()
            .filter_map(|(index, value)| (*value != 0).then_some(index))
            .collect::<Vec<_>>();
        let conductive_count = conductive_cells.len();

        let (current_charge, current_charge_snapshot) =
            read_captured_signed_ushort_array(&mut reader, conductive_count)?;
        let (extra_charge, extra_charge_snapshot) =
            read_captured_ushort_array(&mut reader, conductive_count)?;
        let (pulse_group_indexes, pulse_group_indexes_snapshot) =
            read_captured_ushort_array(&mut reader, conductive_count)?;
        let (version_75_compatibility_values, version_75_compatibility_snapshot) =
            if format_version == 75 {
                // The version-75 reader consumes this sparse compatibility column
                // but does not assign it to a field.
                let (values, snapshot) = read_captured_ushort_array(&mut reader, conductive_count)?;
                (Some(values), Some(snapshot))
            } else {
                (None, None)
            };
        let (electrified, electrified_snapshot) =
            read_captured_bool_array(&mut reader, conductive_count)?;
        let (
            temporary_electrified,
            legacy_temporary_electrified_bytes,
            legacy_temporary_electrified_snapshot,
            temporary_electrified_snapshot,
        ) = if format_version < PULSE_UNIT_NODE_VERSION {
            (vec![false; conductive_count], None, None, None)
        } else if format_version < TEMPORARY_ELECTRIFIED_BOOL_VERSION {
            let (values, snapshot) = read_captured_byte_array(&mut reader, conductive_count)?;
            (
                values.iter().map(|value| *value != 0).collect(),
                Some(values),
                Some(snapshot),
                None,
            )
        } else {
            let (values, snapshot) = read_captured_bool_array(&mut reader, conductive_count)?;
            (values, None, None, Some(snapshot))
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
            conductivity_snapshot,
            conductive_cells,
            current_charge,
            current_charge_snapshot,
            extra_charge,
            extra_charge_snapshot,
            pulse_group_indexes,
            pulse_group_indexes_snapshot,
            version_75_compatibility_values,
            version_75_compatibility_snapshot,
            electrified,
            electrified_snapshot,
            temporary_electrified,
            legacy_temporary_electrified_bytes,
            legacy_temporary_electrified_snapshot,
            temporary_electrified_snapshot,
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

    /// Returns raw temporary-electrification bytes from inner versions 8–75.
    /// The byte values are retained separately from their decoded Bool state.
    pub fn legacy_temporary_electrified_bytes(&self) -> Option<&[u8]> {
        self.legacy_temporary_electrified_bytes.as_deref()
    }

    /// Returns the otherwise-unmodeled sparse UShort column from inner
    /// version 75.
    pub fn version_75_compatibility_values(&self) -> Option<&[u16]> {
        self.version_75_compatibility_values.as_deref()
    }

    /// Replaces a conductivity value without changing whether its cell is
    /// active. Encoding a changed conductivity plane uses the canonical
    /// encoded-array writer. Use [`Self::insert_conductive_cell`] or
    /// [`Self::remove_conductive_cell`] to change the sparse topology.
    pub fn set_conductivity(&mut self, cell: usize, conductivity: u8) -> Result<(), CrpError> {
        let Some(target) = self.conductivity.get_mut(cell) else {
            return Err(invalid(
                cell,
                "electricity conductivity cell index is outside the grid",
            ));
        };
        if (*target == 0) != (conductivity == 0) {
            return Err(invalid(
                cell,
                "electricity conductivity edits must preserve the active-cell topology",
            ));
        }
        *target = conductivity;
        Ok(())
    }

    /// Inserts an active electricity cell and all of its sparse values in
    /// ascending cell-index order. Versions 8–75 require the raw temporary
    /// electrification byte, and version 75 also requires its compatibility
    /// UShort; both must match their version gates and decoded state.
    ///
    /// The separate conductive-cell counter is left unchanged. Returns an
    /// error for invalid state, an out-of-range or already-active cell, or
    /// inconsistent sparse columns; errors leave the data unchanged.
    pub fn insert_conductive_cell(
        &mut self,
        cell: usize,
        state: ElectricityConductiveCellData,
    ) -> Result<(), CrpError> {
        if state.conductivity == 0 {
            return Err(invalid(
                cell,
                "inserted electricity conductivity must be nonzero",
            ));
        }
        self.validate_sparse_lengths(cell)?;
        validate_cell_version_fields(self.format_version, cell, state)?;
        let Some(current) = self.conductivity.get(cell) else {
            return Err(invalid(
                cell,
                "electricity conductivity cell index is outside the grid",
            ));
        };
        let position = match self.conductive_cells.binary_search(&cell) {
            Ok(_) => {
                return Err(invalid(
                    cell,
                    "electricity conductivity cell is already active",
                ));
            }
            Err(position) => position,
        };
        if *current != 0 {
            return Err(invalid(
                cell,
                "electricity conductivity grid and sparse cell index are inconsistent",
            ));
        }

        let Some(conductivity) = self.conductivity.get_mut(cell) else {
            return Err(invalid(
                cell,
                "electricity conductivity cell index is outside the grid",
            ));
        };
        *conductivity = state.conductivity;
        self.conductive_cells.insert(position, cell);
        self.current_charge.insert(position, state.current_charge);
        self.extra_charge.insert(position, state.extra_charge);
        self.pulse_group_indexes
            .insert(position, state.pulse_group_index);
        self.electrified.insert(position, state.electrified);
        self.temporary_electrified
            .insert(position, state.temporary_electrified);
        if let (Some(values), Some(byte)) = (
            self.legacy_temporary_electrified_bytes.as_mut(),
            state.legacy_temporary_electrified_byte,
        ) {
            values.insert(position, byte);
        }
        if let (Some(values), Some(compatibility)) = (
            self.version_75_compatibility_values.as_mut(),
            state.version_75_compatibility_value,
        ) {
            values.insert(position, compatibility);
        }
        Ok(())
    }

    /// Removes an active electricity cell and returns all of its sparse
    /// values, including legacy raw fields when present.
    pub fn remove_conductive_cell(
        &mut self,
        cell: usize,
    ) -> Result<ElectricityConductiveCellData, CrpError> {
        self.validate_sparse_lengths(cell)?;
        let Some(conductivity) = self.conductivity.get(cell).copied() else {
            return Err(invalid(
                cell,
                "electricity conductivity cell index is outside the grid",
            ));
        };
        let position = self.conductive_cells.binary_search(&cell).map_err(|_| {
            invalid(
                cell,
                "electricity conductivity cell is absent from its conductivity grid",
            )
        })?;
        if conductivity == 0 {
            return Err(invalid(
                cell,
                "electricity conductivity grid and sparse cell index are inconsistent",
            ));
        }
        let state = ElectricityConductiveCellData {
            conductivity,
            current_charge: sparse_value(&self.current_charge, position, cell, "current charge")?,
            extra_charge: sparse_value(&self.extra_charge, position, cell, "extra charge")?,
            pulse_group_index: sparse_value(
                &self.pulse_group_indexes,
                position,
                cell,
                "pulse-group index",
            )?,
            electrified: sparse_value(&self.electrified, position, cell, "electrified state")?,
            temporary_electrified: sparse_value(
                &self.temporary_electrified,
                position,
                cell,
                "temporary-electrified state",
            )?,
            legacy_temporary_electrified_byte: match self
                .legacy_temporary_electrified_bytes
                .as_deref()
            {
                Some(values) => Some(sparse_value(
                    values,
                    position,
                    cell,
                    "legacy temporary-electrified byte",
                )?),
                None => None,
            },
            version_75_compatibility_value: match self.version_75_compatibility_values.as_deref() {
                Some(values) => Some(sparse_value(
                    values,
                    position,
                    cell,
                    "version-75 compatibility value",
                )?),
                None => None,
            },
        };
        let Some(conductivity) = self.conductivity.get_mut(cell) else {
            return Err(invalid(
                cell,
                "electricity conductivity cell index is outside the grid",
            ));
        };
        *conductivity = 0;
        self.conductive_cells.remove(position);
        self.current_charge.remove(position);
        self.extra_charge.remove(position);
        self.pulse_group_indexes.remove(position);
        self.electrified.remove(position);
        self.temporary_electrified.remove(position);
        if let Some(values) = self.legacy_temporary_electrified_bytes.as_mut() {
            values.remove(position);
        }
        if let Some(values) = self.version_75_compatibility_values.as_mut() {
            values.remove(position);
        }
        Ok(state)
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

    fn validate_sparse_lengths(&self, cell: usize) -> Result<(), CrpError> {
        if self.conductivity.len() != CELL_COUNT {
            return Err(invalid(
                cell,
                "electricity conductivity grid has an invalid length",
            ));
        }
        let count = self.conductive_cells.len();
        ensure_sparse_length(&self.current_charge, count, cell, "current charge")?;
        ensure_sparse_length(&self.extra_charge, count, cell, "extra charge")?;
        ensure_sparse_length(
            &self.pulse_group_indexes,
            count,
            cell,
            "pulse-group indexes",
        )?;
        ensure_sparse_length(&self.electrified, count, cell, "electrified values")?;
        ensure_sparse_length(
            &self.temporary_electrified,
            count,
            cell,
            "temporary-electrified values",
        )?;
        if self.legacy_temporary_electrified_bytes.is_some()
            != (self.format_version >= PULSE_UNIT_NODE_VERSION
                && self.format_version < TEMPORARY_ELECTRIFIED_BOOL_VERSION)
            || self.legacy_temporary_electrified_snapshot.is_some()
                != self.legacy_temporary_electrified_bytes.is_some()
        {
            return Err(invalid(
                cell,
                "legacy temporary-electrification fields do not match the version gate",
            ));
        }
        if let Some(values) = self.legacy_temporary_electrified_bytes.as_deref() {
            ensure_sparse_length(
                values,
                count,
                cell,
                "legacy temporary-electrification bytes",
            )?;
        }
        if self.temporary_electrified_snapshot.is_some()
            != (self.format_version >= TEMPORARY_ELECTRIFIED_BOOL_VERSION)
        {
            return Err(invalid(
                cell,
                "temporary-electrification Bool fields do not match the version gate",
            ));
        }
        if self.version_75_compatibility_values.is_some() != (self.format_version == 75)
            || self.version_75_compatibility_snapshot.is_some()
                != self.version_75_compatibility_values.is_some()
        {
            return Err(invalid(
                cell,
                "version-75 compatibility fields do not match the version gate",
            ));
        }
        if let Some(values) = self.version_75_compatibility_values.as_deref() {
            ensure_sparse_length(values, count, cell, "version-75 compatibility values")?;
        }
        Ok(())
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

    /// Encodes edited counters, continuation state, conductivity, and sparse
    /// cell columns. Changed arrays use canonical encoded-array writers;
    /// unchanged fields retain their original bytes, including pulse records,
    /// node groups, and the closing profiler reference. The body can change
    /// length. Returns an error if captured field ranges or sparse lengths are
    /// invalid.
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
        self.validate_sparse_lengths(self.scalar_fields_offset)?;
        let mut replacements = Vec::new();
        if let Some(replacement) = changed_byte_array(
            &self.conductivity,
            &self.conductivity_snapshot,
            bytes.len(),
            "conductivity plane",
        )? {
            replacements.push(replacement);
        }
        if let Some(replacement) = changed_signed_ushort_array(
            &self.current_charge,
            &self.current_charge_snapshot,
            bytes.len(),
            "current charge",
        )? {
            replacements.push(replacement);
        }
        if let Some(replacement) = changed_ushort_array(
            &self.extra_charge,
            &self.extra_charge_snapshot,
            bytes.len(),
            "extra charge",
        )? {
            replacements.push(replacement);
        }
        if let Some(replacement) = changed_ushort_array(
            &self.pulse_group_indexes,
            &self.pulse_group_indexes_snapshot,
            bytes.len(),
            "pulse-group indexes",
        )? {
            replacements.push(replacement);
        }
        match (
            self.version_75_compatibility_values.as_deref(),
            self.version_75_compatibility_snapshot.as_ref(),
        ) {
            (Some(values), Some(snapshot)) => {
                if let Some(replacement) = changed_ushort_array(
                    values,
                    snapshot,
                    bytes.len(),
                    "version-75 compatibility column",
                )? {
                    replacements.push(replacement);
                }
            }
            (None, None) => {}
            _ => {
                return Err(invalid(
                    self.scalar_fields_offset,
                    "version-75 compatibility fields do not match the version gate",
                ));
            }
        }
        if let Some(replacement) = changed_bool_array(
            &self.electrified,
            &self.electrified_snapshot,
            bytes.len(),
            "electrified values",
        )? {
            replacements.push(replacement);
        }
        match (
            self.legacy_temporary_electrified_bytes.as_deref(),
            self.legacy_temporary_electrified_snapshot.as_ref(),
        ) {
            (Some(values), Some(snapshot)) => {
                if let Some(replacement) = changed_byte_array(
                    values,
                    snapshot,
                    bytes.len(),
                    "legacy temporary-electrification values",
                )? {
                    replacements.push(replacement);
                }
            }
            (None, None) => {}
            _ => {
                return Err(invalid(
                    self.scalar_fields_offset,
                    "legacy temporary-electrification fields do not match the version gate",
                ));
            }
        }
        match (
            self.temporary_electrified_snapshot.as_ref(),
            (self.format_version >= TEMPORARY_ELECTRIFIED_BOOL_VERSION)
                .then_some(self.temporary_electrified.as_slice()),
        ) {
            (Some(snapshot), Some(values)) => {
                if let Some(replacement) = changed_bool_array(
                    values,
                    snapshot,
                    bytes.len(),
                    "temporary-electrification Bool values",
                )? {
                    replacements.push(replacement);
                }
            }
            (None, None) => {}
            _ => {
                return Err(invalid(
                    self.scalar_fields_offset,
                    "temporary-electrification Bool fields do not match the version gate",
                ));
            }
        }
        replacements.sort_by_key(|(range, _)| range.start);
        for (range, encoded) in replacements.into_iter().rev() {
            bytes.splice(range, encoded);
        }
        Ok(bytes)
    }
}

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "ElectricityManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

fn ensure_sparse_length<T>(
    values: &[T],
    expected: usize,
    cell: usize,
    field: &str,
) -> Result<(), CrpError> {
    if values.len() == expected {
        Ok(())
    } else {
        Err(invalid(
            cell,
            format!("{field} length does not match the conductivity grid"),
        ))
    }
}

fn sparse_value<T: Copy>(
    values: &[T],
    position: usize,
    cell: usize,
    field: &str,
) -> Result<T, CrpError> {
    values.get(position).copied().ok_or_else(|| {
        invalid(
            cell,
            format!("{field} values are shorter than the conductivity grid"),
        )
    })
}

fn validate_cell_version_fields(
    format_version: u32,
    cell: usize,
    state: ElectricityConductiveCellData,
) -> Result<(), CrpError> {
    let has_legacy_temporary_byte =
        (PULSE_UNIT_NODE_VERSION..TEMPORARY_ELECTRIFIED_BOOL_VERSION).contains(&format_version);
    if state.legacy_temporary_electrified_byte.is_some() != has_legacy_temporary_byte {
        return Err(invalid(
            cell,
            "legacy temporary-electrification byte does not match the version gate",
        ));
    }
    if let Some(byte) = state.legacy_temporary_electrified_byte {
        if (byte != 0) != state.temporary_electrified {
            return Err(invalid(
                cell,
                "legacy temporary-electrification byte disagrees with its decoded Bool state",
            ));
        }
    }
    if format_version < PULSE_UNIT_NODE_VERSION && state.temporary_electrified {
        return Err(invalid(
            cell,
            "temporary-electrification state is false before inner version 8",
        ));
    }
    if state.version_75_compatibility_value.is_some() != (format_version == 75) {
        return Err(invalid(
            cell,
            "version-75 compatibility value does not match its exact version gate",
        ));
    }
    Ok(())
}

fn read_captured_byte_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<(Vec<u8>, EncodedArraySnapshot<u8>), CrpError> {
    let start = reader.offset();
    let values = read_encoded_byte_array(reader, count)?;
    let end = reader.offset();
    Ok((
        values.clone(),
        EncodedArraySnapshot {
            range: start..end,
            values,
        },
    ))
}

fn read_captured_ushort_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<(Vec<u16>, EncodedArraySnapshot<u16>), CrpError> {
    let start = reader.offset();
    let values = read_encoded_ushort_array(reader, count)?;
    let end = reader.offset();
    Ok((
        values.clone(),
        EncodedArraySnapshot {
            range: start..end,
            values,
        },
    ))
}

fn read_captured_signed_ushort_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<(Vec<i16>, EncodedArraySnapshot<i16>), CrpError> {
    let (values, snapshot) = read_captured_ushort_array(reader, count)?;
    let values = values
        .into_iter()
        .map(|value| value as i16)
        .collect::<Vec<_>>();
    let original_values = snapshot
        .values
        .into_iter()
        .map(|value| value as i16)
        .collect();
    Ok((
        values,
        EncodedArraySnapshot {
            range: snapshot.range,
            values: original_values,
        },
    ))
}

fn read_captured_bool_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<(Vec<bool>, EncodedArraySnapshot<bool>), CrpError> {
    let start = reader.offset();
    let values = read_encoded_bool_array(reader, count)?;
    let end = reader.offset();
    Ok((
        values.clone(),
        EncodedArraySnapshot {
            range: start..end,
            values,
        },
    ))
}

fn checked_source_range(
    snapshot: &EncodedArraySnapshot<impl Sized>,
    source_body_len: usize,
    field: &str,
) -> Result<(), CrpError> {
    if snapshot.range.start <= snapshot.range.end && snapshot.range.end <= source_body_len {
        Ok(())
    } else {
        Err(invalid(
            snapshot.range.start,
            format!("{field} source range exceeds the body"),
        ))
    }
}

fn changed_byte_array(
    values: &[u8],
    snapshot: &EncodedArraySnapshot<u8>,
    source_body_len: usize,
    field: &str,
) -> Result<Option<(Range<usize>, Vec<u8>)>, CrpError> {
    checked_source_range(snapshot, source_body_len, field)?;
    if values == snapshot.values {
        return Ok(None);
    }
    let mut encoded = Vec::new();
    write_encoded_byte_array(&mut encoded, values);
    Ok(Some((snapshot.range.clone(), encoded)))
}

fn changed_ushort_array(
    values: &[u16],
    snapshot: &EncodedArraySnapshot<u16>,
    source_body_len: usize,
    field: &str,
) -> Result<Option<(Range<usize>, Vec<u8>)>, CrpError> {
    checked_source_range(snapshot, source_body_len, field)?;
    if values == snapshot.values {
        return Ok(None);
    }
    let mut encoded = Vec::new();
    write_encoded_ushort_array(&mut encoded, values);
    Ok(Some((snapshot.range.clone(), encoded)))
}

fn changed_signed_ushort_array(
    values: &[i16],
    snapshot: &EncodedArraySnapshot<i16>,
    source_body_len: usize,
    field: &str,
) -> Result<Option<(Range<usize>, Vec<u8>)>, CrpError> {
    checked_source_range(snapshot, source_body_len, field)?;
    if values == snapshot.values {
        return Ok(None);
    }
    let unsigned_values = values.iter().map(|value| *value as u16).collect::<Vec<_>>();
    let mut encoded = Vec::new();
    write_encoded_ushort_array(&mut encoded, &unsigned_values);
    Ok(Some((snapshot.range.clone(), encoded)))
}

fn changed_bool_array(
    values: &[bool],
    snapshot: &EncodedArraySnapshot<bool>,
    source_body_len: usize,
    field: &str,
) -> Result<Option<(Range<usize>, Vec<u8>)>, CrpError> {
    checked_source_range(snapshot, source_body_len, field)?;
    if values == snapshot.values {
        return Ok(None);
    }
    let mut encoded = Vec::new();
    write_encoded_bool_array(&mut encoded, values);
    Ok(Some((snapshot.range.clone(), encoded)))
}

#[cfg(test)]
mod tests {
    use super::{
        CELL_COUNT, END_PROFILER_REFERENCE_LEN, ElectricityConductiveCellData,
        ElectricityManagerData, FULL_NODE_GROUPS_VERSION, NODE_GROUP_COUNT, PULSE_GROUP_CAPACITY,
        PULSE_UNIT_CAPACITY,
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

    fn append_ushort_literal_array(output: &mut Vec<u8>, values: &[u16]) {
        if values.is_empty() {
            return;
        }
        output.push((values.len() - 1) as u8);
        for value in values {
            output.extend_from_slice(&value.to_be_bytes());
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

    fn active_current_body() -> Vec<u8> {
        let mut conductivity = vec![0; CELL_COUNT];
        conductivity[5] = 1;
        conductivity[10] = 2;
        let mut body = Vec::new();
        append_literal_byte_array(&mut body, &conductivity);
        append_ushort_literal_array(&mut body, &[0xffff, 0x1234]);
        append_ushort_literal_array(&mut body, &[3, 4]);
        append_ushort_literal_array(&mut body, &[7, 8]);
        body.extend_from_slice(&[0, 1]); // Bool values: true, false.
        body.extend_from_slice(&[0, 2]); // Bool values: false, true.
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
        body
    }

    fn active_version_75_body() -> Vec<u8> {
        let mut conductivity = vec![0; CELL_COUNT];
        conductivity[7] = 1;
        let mut body = Vec::new();
        append_literal_byte_array(&mut body, &conductivity);
        append_ushort_literal_array(&mut body, &[0xfffe]);
        append_ushort_literal_array(&mut body, &[3]);
        append_ushort_literal_array(&mut body, &[4]);
        append_ushort_literal_array(&mut body, &[0x1122]); // Version-75 raw column.
        body.extend_from_slice(&[0, 1]); // Electrified Bool: true.
        body.extend_from_slice(&[0, 0x80]); // Legacy temp byte is true, raw 0x80.
        body.extend_from_slice(&[0; 4]); // Empty pulse-group and pulse-unit tables.
        append_zero_encoded_ushort_array(&mut body, NODE_GROUP_COUNT / 2);
        body.extend_from_slice(&[0; 9]); // Scalar tail.
        body.extend_from_slice(&[0x12, 0x34]);
        body
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
        let body = active_current_body();
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
        assert_eq!(data.encode().unwrap(), body);
    }

    #[test]
    fn inserts_and_removes_current_conductive_cells_with_exact_sparse_bytes() {
        let body = active_current_body();
        let mut data = ElectricityManagerData::decode(117_000, &body).unwrap();
        let state = ElectricityConductiveCellData {
            conductivity: 0x44,
            current_charge: -3,
            extra_charge: 0x2222,
            pulse_group_index: 0x3333,
            electrified: false,
            temporary_electrified: true,
            legacy_temporary_electrified_byte: None,
            version_75_compatibility_value: None,
        };
        let mut conductivity = vec![0; CELL_COUNT];
        conductivity[3] = state.conductivity;
        conductivity[5] = 1;
        conductivity[10] = 2;
        let mut expected = body.clone();
        let mut replacements = vec![
            (data.conductivity_snapshot.range.clone(), {
                let mut encoded = Vec::new();
                append_literal_byte_array(&mut encoded, &conductivity);
                encoded
            }),
            (data.current_charge_snapshot.range.clone(), {
                let mut encoded = Vec::new();
                append_ushort_literal_array(&mut encoded, &[0xfffd, 0xffff, 0x1234]);
                encoded
            }),
            (data.extra_charge_snapshot.range.clone(), {
                let mut encoded = Vec::new();
                append_ushort_literal_array(&mut encoded, &[0x2222, 3, 4]);
                encoded
            }),
            (data.pulse_group_indexes_snapshot.range.clone(), {
                let mut encoded = Vec::new();
                append_ushort_literal_array(&mut encoded, &[0x3333, 7, 8]);
                encoded
            }),
            (data.electrified_snapshot.range.clone(), vec![0, 0b010]),
            (
                data.temporary_electrified_snapshot
                    .as_ref()
                    .unwrap()
                    .range
                    .clone(),
                vec![0, 0b101],
            ),
        ];
        data.insert_conductive_cell(3, state).unwrap();
        assert_eq!(data.conductive_cells(), [3, 5, 10]);
        assert_eq!(data.conductive_cell_count(), 18);
        replacements.sort_by_key(|(range, _)| range.start);
        for (range, encoded) in replacements.into_iter().rev() {
            expected.splice(range, encoded);
        }
        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = ElectricityManagerData::decode(117_000, &encoded).unwrap();
        assert_eq!(decoded.conductive_cells(), [3, 5, 10]);
        assert_eq!(decoded.current_charge(), [-3, -1, 0x1234]);
        assert_eq!(decoded.extra_charge(), [0x2222, 3, 4]);
        assert_eq!(decoded.pulse_group_indexes(), [0x3333, 7, 8]);
        assert_eq!(decoded.electrified(), [false, true, false]);
        assert_eq!(decoded.temporary_electrified(), [true, false, true]);

        assert_eq!(data.remove_conductive_cell(3).unwrap(), state);
        assert_eq!(data.conductive_cells(), [5, 10]);
        assert_eq!(data.encode().unwrap(), body);
        assert!(data.insert_conductive_cell(5, state).is_err());
        assert!(data.insert_conductive_cell(CELL_COUNT, state).is_err());
        let zero_state = ElectricityConductiveCellData {
            conductivity: 0,
            ..state
        };
        assert!(data.insert_conductive_cell(3, zero_state).is_err());
        assert!(data.remove_conductive_cell(CELL_COUNT).is_err());
        assert_eq!(data.encode().unwrap(), body);
    }

    #[test]
    fn conductivity_edits_keep_the_sparse_topology_and_write_expected_plane_bytes() {
        let body = active_current_body();
        let mut data = ElectricityManagerData::decode(117_000, &body).unwrap();
        let mut conductivity = vec![0; CELL_COUNT];
        conductivity[5] = 4;
        conductivity[10] = 2;
        let mut expected = body.clone();
        let mut encoded_plane = Vec::new();
        append_literal_byte_array(&mut encoded_plane, &conductivity);
        expected.splice(data.conductivity_snapshot.range.clone(), encoded_plane);

        data.set_conductivity(5, 4).unwrap();

        assert_eq!(data.conductive_cells(), [5, 10]);
        assert_eq!(data.encode().unwrap(), expected);
        let decoded = ElectricityManagerData::decode(117_000, &expected).unwrap();
        assert_eq!(decoded.conductive_cells(), [5, 10]);
        assert_eq!(decoded.conductivity()[5], 4);
        assert_eq!(decoded.conductivity()[10], 2);

        assert!(data.set_conductivity(5, 0).is_err());
        assert!(data.set_conductivity(CELL_COUNT, 1).is_err());
        assert_eq!(data.encode().unwrap(), expected);
    }

    #[test]
    fn topology_edits_preserve_version_75_legacy_sparse_columns() {
        let body = active_version_75_body();
        let mut data = ElectricityManagerData::decode(75, &body).unwrap();
        assert_eq!(
            data.legacy_temporary_electrified_bytes(),
            Some([0x80].as_slice())
        );
        assert_eq!(
            data.version_75_compatibility_values(),
            Some([0x1122].as_slice())
        );
        assert_eq!(data.temporary_electrified(), [true]);
        assert_eq!(data.encode().unwrap(), body);
        let state = ElectricityConductiveCellData {
            conductivity: 0x22,
            current_charge: -5,
            extra_charge: 0x4567,
            pulse_group_index: 0x89ab,
            electrified: false,
            temporary_electrified: false,
            legacy_temporary_electrified_byte: Some(0),
            version_75_compatibility_value: Some(0x3344),
        };
        let mut conductivity = vec![0; CELL_COUNT];
        conductivity[3] = state.conductivity;
        conductivity[7] = 1;
        let mut expected = body.clone();
        let mut replacements = vec![
            (data.conductivity_snapshot.range.clone(), {
                let mut encoded = Vec::new();
                append_literal_byte_array(&mut encoded, &conductivity);
                encoded
            }),
            (
                data.current_charge_snapshot.range.clone(),
                vec![1, 0xff, 0xfb, 0xff, 0xfe],
            ),
            (
                data.extra_charge_snapshot.range.clone(),
                vec![1, 0x45, 0x67, 0, 3],
            ),
            (
                data.pulse_group_indexes_snapshot.range.clone(),
                vec![1, 0x89, 0xab, 0, 4],
            ),
            (
                data.version_75_compatibility_snapshot
                    .as_ref()
                    .unwrap()
                    .range
                    .clone(),
                vec![1, 0x33, 0x44, 0x11, 0x22],
            ),
            (data.electrified_snapshot.range.clone(), vec![0, 0b10]),
            (
                data.legacy_temporary_electrified_snapshot
                    .as_ref()
                    .unwrap()
                    .range
                    .clone(),
                vec![1, 0, 0x80],
            ),
        ];
        data.insert_conductive_cell(3, state).unwrap();
        replacements.sort_by_key(|(range, _)| range.start);
        for (range, encoded) in replacements.into_iter().rev() {
            expected.splice(range, encoded);
        }
        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = ElectricityManagerData::decode(75, &encoded).unwrap();
        assert_eq!(decoded.conductive_cells(), [3, 7]);
        assert_eq!(
            decoded.legacy_temporary_electrified_bytes(),
            Some([0, 0x80].as_slice())
        );
        assert_eq!(
            decoded.version_75_compatibility_values(),
            Some([0x3344, 0x1122].as_slice())
        );
        assert_eq!(decoded.temporary_electrified(), [false, true]);
        assert_eq!(data.remove_conductive_cell(3).unwrap(), state);
        assert_eq!(data.encode().unwrap(), body);
    }

    #[test]
    fn version_7_topology_edits_require_its_derived_false_temporary_state() {
        let mut data = ElectricityManagerData::decode(7, &empty_body(7)).unwrap();
        let state = ElectricityConductiveCellData {
            conductivity: 1,
            current_charge: 0,
            extra_charge: 0,
            pulse_group_index: 0,
            electrified: false,
            temporary_electrified: false,
            legacy_temporary_electrified_byte: None,
            version_75_compatibility_value: None,
        };
        data.insert_conductive_cell(0, state).unwrap();
        let encoded = data.encode().unwrap();
        let decoded = ElectricityManagerData::decode(7, &encoded).unwrap();
        assert_eq!(decoded.conductive_cells(), [0]);
        assert_eq!(decoded.temporary_electrified(), [false]);
        assert_eq!(data.remove_conductive_cell(0).unwrap(), state);

        let invalid_state = ElectricityConductiveCellData {
            temporary_electrified: true,
            ..state
        };
        assert!(data.insert_conductive_cell(0, invalid_state).is_err());
        assert!(data.conductive_cells().is_empty());
    }

    #[test]
    fn version_8_topology_edits_retain_legacy_temporary_byte_without_v75_column() {
        let body = empty_body(8);
        let mut data = ElectricityManagerData::decode(8, &body).unwrap();
        let state = ElectricityConductiveCellData {
            conductivity: 1,
            current_charge: 2,
            extra_charge: 3,
            pulse_group_index: 4,
            electrified: true,
            temporary_electrified: true,
            legacy_temporary_electrified_byte: Some(0x80),
            version_75_compatibility_value: None,
        };
        data.insert_conductive_cell(2, state).unwrap();
        let encoded = data.encode().unwrap();
        let decoded = ElectricityManagerData::decode(8, &encoded).unwrap();
        assert_eq!(decoded.conductive_cells(), [2]);
        assert_eq!(
            decoded.legacy_temporary_electrified_bytes(),
            Some([0x80].as_slice())
        );
        assert_eq!(decoded.version_75_compatibility_values(), None);
        assert_eq!(decoded.temporary_electrified(), [true]);
        assert_eq!(data.remove_conductive_cell(2).unwrap(), state);
        assert_eq!(data.encode().unwrap(), body);

        let invalid_state = ElectricityConductiveCellData {
            legacy_temporary_electrified_byte: None,
            ..state
        };
        assert!(data.insert_conductive_cell(2, invalid_state).is_err());
        assert!(data.conductive_cells().is_empty());
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
