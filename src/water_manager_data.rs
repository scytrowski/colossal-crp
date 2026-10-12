use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    read_encoded_bool_array, read_encoded_byte_array, read_encoded_ushort_array,
    write_encoded_bool_array, write_encoded_byte_array, write_encoded_ushort_array,
};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

const CELL_COUNT: usize = 65_536;
const NODE_COUNT: usize = 32_768;
const LEGACY_NODE_COUNT: usize = 16_384;
const PULSE_GROUP_CAPACITY: usize = 1_024;
const PULSE_UNIT_CAPACITY: usize = 32_768;
const PULSE_GROUP_RECORD_LEN: usize = 14;
const PULSE_UNIT_RECORD_LEN: usize = 6;
const COLLECT_PRESSURE_FIELD_LEN: usize = 4;
const CLOSEST_PIPE_SEGMENT_VERSION: u32 = 73;
const GUIDE_REFERENCES_VERSION: u32 = 94;
const POLLUTION_VERSION: u32 = 23;
const FULL_NODE_COUNT_VERSION: u32 = 202;
const HEATING_VERSION: u32 = 227;
const COLLECT_WATER_BOOL_VERSION: u32 = 270;
const COLLECT_WATER_USHORT_VERSION: u32 = 271;
const COLLECT_SEWAGE_USHORT_VERSION: u32 = 272;
const END_PROFILER_REFERENCE_LEN: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
struct EncodedArraySnapshot<T> {
    range: (usize, usize),
    values: Vec<T>,
}

/// One active water, sewage, or heating pulse group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterPulseGroup {
    /// Original pressure stored for the group.
    pub original_pressure: u32,
    /// Current pressure stored for the group.
    pub current_pressure: u32,
    /// Collect pressure for water and sewage from inner version 270.
    pub collect_pressure: Option<u32>,
    /// Merge target/index value.
    pub merge_index: u16,
    /// Number of merged groups.
    pub merge_count: u16,
    /// Node index associated with the group.
    pub node: u16,
}

/// One active water, sewage, or heating pulse unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterPulseUnit {
    /// Pulse group referenced by this unit.
    pub group: u16,
    /// Node referenced by this unit.
    pub node: u16,
    /// X coordinate in the simulation grid.
    pub x: u8,
    /// Z coordinate in the simulation grid.
    pub z: u8,
}

/// All serialized sparse fields for one active water or sewage cell.
///
/// `conductivity` must be nonzero. `pollution` must be present from inner
/// version 23 and absent before it. These values describe the saved fields;
/// they do not imply that the cell is connected to a live network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterConductiveCellData {
    /// Nonzero value in the primary conductivity plane.
    pub conductivity: u8,
    /// Signed water pressure.
    pub water_pressure: i16,
    /// Signed sewage pressure.
    pub sewage_pressure: i16,
    /// Water pulse-group index.
    pub water_pulse_group_index: u16,
    /// Sewage pulse-group index.
    pub sewage_pulse_group_index: u16,
    /// Current water electrification flag.
    pub water_electrified: bool,
    /// Temporary water electrification flag.
    pub water_temporary_electrified: bool,
    /// Current sewage electrification flag.
    pub sewage_electrified: bool,
    /// Temporary sewage electrification flag.
    pub sewage_temporary_electrified: bool,
    /// Conductive pollution byte from inner version 23.
    pub pollution: Option<u8>,
}

/// All serialized sparse fields for one active heating cell.
///
/// Heating fields exist from inner version 227 onward. `conductivity` must be
/// nonzero. These values describe the saved fields; they do not imply that the
/// cell is connected to a live heating network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatingConductiveCellData {
    /// Nonzero value in the heating conductivity plane.
    pub conductivity: u8,
    /// Signed heating pressure.
    pub pressure: i16,
    /// Heating pulse-group index.
    pub pulse_group_index: u16,
    /// Current heating electrification flag.
    pub electrified: bool,
    /// Temporary heating electrification flag.
    pub temporary_electrified: bool,
}

/// Decoded reader fields from `WaterManager+Data`.
///
/// Sparse cell columns use the corresponding conductivity plane's ascending
/// cell-index order. Conductivity values can be changed while retaining the
/// existing zero/nonzero cell topology, or cells can be inserted and removed
/// by supplying or receiving every sparse field for that cell. Node arrays retain their full
/// 32,768-entry size, with version-derived
/// defaults in the legacy upper half. The complete source body remains
/// available for exact unchanged writing. Pulse-group and pulse-unit records
/// and the scalar tail can be edited in place. Sparse, closest-segment, node,
/// and collect-pressure arrays re-encode only when changed and can resize the
/// body; all other bytes remain unchanged. The inner serializer version
/// controls the 23, 73, 94, 202, 227, and 270–272 gates. Corpus samples cover
/// versions 194–118011; active network values and gates 270–272 have synthetic
/// coverage only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaterManagerData {
    format_version: u32,
    conductivity: Vec<u8>,
    conductivity_snapshot: EncodedArraySnapshot<u8>,
    conductive_cells: Vec<usize>,
    conductivity_heating: Option<Vec<u8>>,
    conductivity_heating_snapshot: Option<EncodedArraySnapshot<u8>>,
    heating_cells: Vec<usize>,
    water_pressure: Vec<i16>,
    water_pressure_snapshot: EncodedArraySnapshot<i16>,
    sewage_pressure: Vec<i16>,
    sewage_pressure_snapshot: EncodedArraySnapshot<i16>,
    heating_pressure: Option<Vec<i16>>,
    heating_pressure_snapshot: Option<EncodedArraySnapshot<i16>>,
    water_pulse_group_indexes: Vec<u16>,
    water_pulse_group_indexes_snapshot: EncodedArraySnapshot<u16>,
    sewage_pulse_group_indexes: Vec<u16>,
    sewage_pulse_group_indexes_snapshot: EncodedArraySnapshot<u16>,
    heating_pulse_group_indexes: Option<Vec<u16>>,
    heating_pulse_group_indexes_snapshot: Option<EncodedArraySnapshot<u16>>,
    closest_pipe_segments: Option<Vec<u16>>,
    closest_pipe_segments_range: Option<(usize, usize)>,
    original_closest_pipe_segments: Option<Vec<u16>>,
    closest_heating_segments: Option<Vec<u16>>,
    closest_heating_segments_range: Option<(usize, usize)>,
    original_closest_heating_segments: Option<Vec<u16>>,
    water_electrified: Vec<bool>,
    water_electrified_snapshot: EncodedArraySnapshot<bool>,
    water_temporary_electrified: Vec<bool>,
    water_temporary_electrified_snapshot: EncodedArraySnapshot<bool>,
    sewage_electrified: Vec<bool>,
    sewage_electrified_snapshot: EncodedArraySnapshot<bool>,
    sewage_temporary_electrified: Vec<bool>,
    sewage_temporary_electrified_snapshot: EncodedArraySnapshot<bool>,
    heating_electrified: Option<Vec<bool>>,
    heating_electrified_snapshot: Option<EncodedArraySnapshot<bool>>,
    heating_temporary_electrified: Option<Vec<bool>>,
    heating_temporary_electrified_snapshot: Option<EncodedArraySnapshot<bool>>,
    conductive_pollution: Option<Vec<u8>>,
    conductive_pollution_snapshot: Option<EncodedArraySnapshot<u8>>,
    water_pulse_groups: Vec<WaterPulseGroup>,
    water_pulse_groups_offset: usize,
    water_pulse_units: Vec<WaterPulseUnit>,
    water_pulse_units_offset: usize,
    sewage_pulse_groups: Vec<WaterPulseGroup>,
    sewage_pulse_groups_offset: usize,
    sewage_pulse_units: Vec<WaterPulseUnit>,
    sewage_pulse_units_offset: usize,
    heating_pulse_groups: Option<Vec<WaterPulseGroup>>,
    heating_pulse_groups_offset: Option<usize>,
    heating_pulse_units: Option<Vec<WaterPulseUnit>>,
    heating_pulse_units_offset: Option<usize>,
    water_node_pulse_groups: Vec<u16>,
    water_node_pulse_groups_range: (usize, usize),
    original_water_node_pulse_groups: Vec<u16>,
    water_node_pressure: Vec<u16>,
    water_node_pressure_range: (usize, usize),
    original_water_node_pressure: Vec<u16>,
    water_node_extra_pressure: Vec<u16>,
    water_node_extra_pressure_range: (usize, usize),
    original_water_node_extra_pressure: Vec<u16>,
    sewage_node_pulse_groups: Vec<u16>,
    sewage_node_pulse_groups_range: (usize, usize),
    original_sewage_node_pulse_groups: Vec<u16>,
    sewage_node_pressure: Vec<u16>,
    sewage_node_pressure_range: (usize, usize),
    original_sewage_node_pressure: Vec<u16>,
    sewage_node_extra_pressure: Vec<u16>,
    sewage_node_extra_pressure_range: (usize, usize),
    original_sewage_node_extra_pressure: Vec<u16>,
    heating_node_pulse_groups: Option<Vec<u16>>,
    heating_node_pulse_groups_range: Option<(usize, usize)>,
    original_heating_node_pulse_groups: Option<Vec<u16>>,
    heating_node_pressure: Option<Vec<u16>>,
    heating_node_pressure_range: Option<(usize, usize)>,
    original_heating_node_pressure: Option<Vec<u16>>,
    heating_node_extra_pressure: Option<Vec<u16>>,
    heating_node_extra_pressure_range: Option<(usize, usize)>,
    original_heating_node_extra_pressure: Option<Vec<u16>>,
    node_pollution: Option<Vec<u8>>,
    node_pollution_range: Option<(usize, usize)>,
    original_node_pollution: Option<Vec<u8>>,
    collect_water_pressure: Vec<u16>,
    collect_water_pressure_range: Option<(usize, usize)>,
    original_collect_water_pressure: Option<Vec<u16>>,
    collect_water_pressure_selector: Option<Vec<bool>>,
    collect_water_pressure_selector_range: Option<(usize, usize)>,
    original_collect_water_pressure_selector: Option<Vec<bool>>,
    collect_sewage_pressure: Vec<u16>,
    collect_sewage_pressure_range: Option<(usize, usize)>,
    original_collect_sewage_pressure: Option<Vec<u16>>,
    processed_cell_count: i32,
    conductive_cell_count: i32,
    can_continue: bool,
    original_can_continue: bool,
    scalar_fields_offset: usize,
    guide_references: Vec<NetObjectHeader>,
    original_body: Vec<u8>,
}

impl WaterManagerData {
    /// Decodes the complete bounded body, including version-gated guide
    /// references and its two-byte closing profiler reference. Conditional
    /// arrays and pulse-record layouts follow the inner DataSerializer version,
    /// independent of the outer package version. Truncated data, pulse counts
    /// above the fixed game buffers, invalid object headers, or extra bytes
    /// return errors. Guide headers are parsed but not resolved against the
    /// shared object table; the original body remains exact for unchanged
    /// writing.
    pub fn decode(format_version: u32, bytes: &[u8]) -> Result<Self, CrpError> {
        let mut reader = Reader::new(bytes);
        let (conductivity, conductivity_snapshot) =
            read_captured_byte_array(&mut reader, CELL_COUNT)?;
        let conductive_cells = indexes_with_nonzero_values(&conductivity);
        let (conductivity_heating, conductivity_heating_snapshot) =
            if format_version >= HEATING_VERSION {
                let (values, snapshot) = read_captured_byte_array(&mut reader, CELL_COUNT)?;
                (Some(values), Some(snapshot))
            } else {
                (None, None)
            };
        let heating_cells = conductivity_heating
            .as_deref()
            .map(indexes_with_nonzero_values)
            .unwrap_or_default();

        let (water_pressure, water_pressure_snapshot) =
            read_captured_signed_ushort_array(&mut reader, conductive_cells.len())?;
        let (sewage_pressure, sewage_pressure_snapshot) =
            read_captured_signed_ushort_array(&mut reader, conductive_cells.len())?;
        let (heating_pressure, heating_pressure_snapshot) = if format_version >= HEATING_VERSION {
            let (values, snapshot) =
                read_captured_signed_ushort_array(&mut reader, heating_cells.len())?;
            (Some(values), Some(snapshot))
        } else {
            (None, None)
        };

        let (water_pulse_group_indexes, water_pulse_group_indexes_snapshot) =
            read_captured_ushort_array(&mut reader, conductive_cells.len())?;
        let (sewage_pulse_group_indexes, sewage_pulse_group_indexes_snapshot) =
            read_captured_ushort_array(&mut reader, conductive_cells.len())?;
        let (heating_pulse_group_indexes, heating_pulse_group_indexes_snapshot) = if format_version
            >= HEATING_VERSION
        {
            let (values, snapshot) = read_captured_ushort_array(&mut reader, heating_cells.len())?;
            (Some(values), Some(snapshot))
        } else {
            (None, None)
        };

        let (closest_pipe_segments, closest_pipe_segments_range, original_closest_pipe_segments) =
            if format_version >= CLOSEST_PIPE_SEGMENT_VERSION {
                let start = reader.offset();
                let values = read_encoded_ushort_array(&mut reader, CELL_COUNT)?;
                let end = reader.offset();
                (Some(values.clone()), Some((start, end)), Some(values))
            } else {
                (None, None, None)
            };
        let (
            closest_heating_segments,
            closest_heating_segments_range,
            original_closest_heating_segments,
        ) = if format_version >= HEATING_VERSION {
            let start = reader.offset();
            let values = read_encoded_ushort_array(&mut reader, CELL_COUNT)?;
            let end = reader.offset();
            (Some(values.clone()), Some((start, end)), Some(values))
        } else {
            (None, None, None)
        };

        let (water_electrified, water_electrified_snapshot) =
            read_captured_bool_array(&mut reader, conductive_cells.len())?;
        let (water_temporary_electrified, water_temporary_electrified_snapshot) =
            read_captured_bool_array(&mut reader, conductive_cells.len())?;
        let (sewage_electrified, sewage_electrified_snapshot) =
            read_captured_bool_array(&mut reader, conductive_cells.len())?;
        let (sewage_temporary_electrified, sewage_temporary_electrified_snapshot) =
            read_captured_bool_array(&mut reader, conductive_cells.len())?;
        let (
            heating_electrified,
            heating_electrified_snapshot,
            heating_temporary_electrified,
            heating_temporary_electrified_snapshot,
        ) = if format_version >= HEATING_VERSION {
            let (electrified, electrified_snapshot) =
                read_captured_bool_array(&mut reader, heating_cells.len())?;
            let (temporary, temporary_snapshot) =
                read_captured_bool_array(&mut reader, heating_cells.len())?;
            (
                Some(electrified),
                Some(electrified_snapshot),
                Some(temporary),
                Some(temporary_snapshot),
            )
        } else {
            (None, None, None, None)
        };
        let (conductive_pollution, conductive_pollution_snapshot) = if format_version
            >= POLLUTION_VERSION
        {
            let (values, snapshot) = read_captured_byte_array(&mut reader, conductive_cells.len())?;
            (Some(values), Some(snapshot))
        } else {
            (None, None)
        };

        let (water_pulse_groups, water_pulse_groups_offset) =
            read_pulse_groups(&mut reader, format_version, false)?;
        let (water_pulse_units, water_pulse_units_offset) = read_pulse_units(&mut reader)?;
        let (sewage_pulse_groups, sewage_pulse_groups_offset) =
            read_pulse_groups(&mut reader, format_version, false)?;
        let (sewage_pulse_units, sewage_pulse_units_offset) = read_pulse_units(&mut reader)?;
        let (
            heating_pulse_groups,
            heating_pulse_groups_offset,
            heating_pulse_units,
            heating_pulse_units_offset,
        ) = if format_version >= HEATING_VERSION {
            let (groups, groups_offset) = read_pulse_groups(&mut reader, format_version, true)?;
            let (units, units_offset) = read_pulse_units(&mut reader)?;
            (
                Some(groups),
                Some(groups_offset),
                Some(units),
                Some(units_offset),
            )
        } else {
            (None, None, None, None)
        };

        let stored_node_count = if format_version >= FULL_NODE_COUNT_VERSION {
            NODE_COUNT
        } else {
            LEGACY_NODE_COUNT
        };
        let water_node_pulse_groups_start = reader.offset();
        let water_node_pulse_groups =
            read_node_ushort_array(&mut reader, stored_node_count, u16::MAX)?;
        let water_node_pulse_groups_end = reader.offset();
        let original_water_node_pulse_groups =
            water_node_pulse_groups[..stored_node_count].to_vec();
        let water_node_pressure_start = reader.offset();
        let water_node_pressure = read_node_ushort_array(&mut reader, stored_node_count, 0)?;
        let water_node_pressure_end = reader.offset();
        let original_water_node_pressure = water_node_pressure[..stored_node_count].to_vec();
        let water_node_extra_pressure_start = reader.offset();
        let water_node_extra_pressure = read_node_ushort_array(&mut reader, stored_node_count, 0)?;
        let water_node_extra_pressure_end = reader.offset();
        let original_water_node_extra_pressure =
            water_node_extra_pressure[..stored_node_count].to_vec();
        let sewage_node_pulse_groups_start = reader.offset();
        let sewage_node_pulse_groups =
            read_node_ushort_array(&mut reader, stored_node_count, u16::MAX)?;
        let sewage_node_pulse_groups_end = reader.offset();
        let original_sewage_node_pulse_groups =
            sewage_node_pulse_groups[..stored_node_count].to_vec();
        let sewage_node_pressure_start = reader.offset();
        let sewage_node_pressure = read_node_ushort_array(&mut reader, stored_node_count, 0)?;
        let sewage_node_pressure_end = reader.offset();
        let original_sewage_node_pressure = sewage_node_pressure[..stored_node_count].to_vec();
        let sewage_node_extra_pressure_start = reader.offset();
        let sewage_node_extra_pressure = read_node_ushort_array(&mut reader, stored_node_count, 0)?;
        let sewage_node_extra_pressure_end = reader.offset();
        let original_sewage_node_extra_pressure =
            sewage_node_extra_pressure[..stored_node_count].to_vec();
        let (
            heating_node_pulse_groups,
            heating_node_pulse_groups_range,
            original_heating_node_pulse_groups,
            heating_node_pressure,
            heating_node_pressure_range,
            original_heating_node_pressure,
            heating_node_extra_pressure,
            heating_node_extra_pressure_range,
            original_heating_node_extra_pressure,
        ) = if format_version >= HEATING_VERSION {
            let pulse_groups_start = reader.offset();
            let pulse_groups = read_node_ushort_array(&mut reader, stored_node_count, u16::MAX)?;
            let pulse_groups_end = reader.offset();
            let original_pulse_groups = pulse_groups[..stored_node_count].to_vec();
            let pressure_start = reader.offset();
            let pressure = read_node_ushort_array(&mut reader, stored_node_count, 0)?;
            let pressure_end = reader.offset();
            let original_pressure = pressure[..stored_node_count].to_vec();
            let extra_pressure_start = reader.offset();
            let extra_pressure = read_node_ushort_array(&mut reader, stored_node_count, 0)?;
            let extra_pressure_end = reader.offset();
            let original_extra_pressure = extra_pressure[..stored_node_count].to_vec();
            (
                Some(pulse_groups),
                Some((pulse_groups_start, pulse_groups_end)),
                Some(original_pulse_groups),
                Some(pressure),
                Some((pressure_start, pressure_end)),
                Some(original_pressure),
                Some(extra_pressure),
                Some((extra_pressure_start, extra_pressure_end)),
                Some(original_extra_pressure),
            )
        } else {
            (None, None, None, None, None, None, None, None, None)
        };
        let (node_pollution, node_pollution_range, original_node_pollution) =
            if format_version >= POLLUTION_VERSION {
                let start = reader.offset();
                let values = read_node_byte_array(&mut reader, stored_node_count, 0)?;
                let end = reader.offset();
                let original = values[..stored_node_count].to_vec();
                (Some(values), Some((start, end)), Some(original))
            } else {
                (None, None, None)
            };

        let mut collect_water_pressure = vec![0; NODE_COUNT];
        let mut collect_water_pressure_range = None;
        let mut original_collect_water_pressure = None;
        let mut collect_water_pressure_selector = None;
        let mut collect_water_pressure_selector_range = None;
        let mut original_collect_water_pressure_selector = None;
        if format_version >= COLLECT_WATER_USHORT_VERSION {
            let start = reader.offset();
            let values = read_encoded_ushort_array(&mut reader, stored_node_count)?;
            let end = reader.offset();
            collect_water_pressure[..stored_node_count].copy_from_slice(&values);
            collect_water_pressure_range = Some((start, end));
            original_collect_water_pressure = Some(values);
        } else if format_version >= COLLECT_WATER_BOOL_VERSION {
            let start = reader.offset();
            let collects = read_encoded_bool_array(&mut reader, stored_node_count)?;
            let end = reader.offset();
            for (index, collects_pressure) in collects.iter().copied().enumerate() {
                if collects_pressure {
                    collect_water_pressure[index] = water_node_pressure[index];
                }
            }
            collect_water_pressure_selector_range = Some((start, end));
            original_collect_water_pressure_selector = Some(collects.clone());
            collect_water_pressure_selector = Some(collects);
        }
        let mut collect_sewage_pressure = vec![0; NODE_COUNT];
        let mut collect_sewage_pressure_range = None;
        let mut original_collect_sewage_pressure = None;
        if format_version >= COLLECT_SEWAGE_USHORT_VERSION {
            let start = reader.offset();
            let values = read_encoded_ushort_array(&mut reader, stored_node_count)?;
            let end = reader.offset();
            collect_sewage_pressure[..stored_node_count].copy_from_slice(&values);
            collect_sewage_pressure_range = Some((start, end));
            original_collect_sewage_pressure = Some(values);
        }

        let scalar_fields_offset = reader.offset();
        let processed_cell_count = reader.read_i32_be()?;
        let conductive_cell_count = reader.read_i32_be()?;
        let can_continue = reader.read_u8()? != 0;
        let guide_references = read_guide_references(&mut reader, format_version)?;
        read_end_profiler_reference(&mut reader)?;
        if reader.remaining() != 0 {
            return Err(reader.invalid(format!(
                "WaterManager decoder left {} unexpected bytes",
                reader.remaining()
            )));
        }

        Ok(Self {
            format_version,
            conductivity,
            conductivity_snapshot,
            conductive_cells,
            conductivity_heating,
            conductivity_heating_snapshot,
            heating_cells,
            water_pressure,
            water_pressure_snapshot,
            sewage_pressure,
            sewage_pressure_snapshot,
            heating_pressure,
            heating_pressure_snapshot,
            water_pulse_group_indexes,
            water_pulse_group_indexes_snapshot,
            sewage_pulse_group_indexes,
            sewage_pulse_group_indexes_snapshot,
            heating_pulse_group_indexes,
            heating_pulse_group_indexes_snapshot,
            closest_pipe_segments,
            closest_pipe_segments_range,
            original_closest_pipe_segments,
            closest_heating_segments,
            closest_heating_segments_range,
            original_closest_heating_segments,
            water_electrified,
            water_electrified_snapshot,
            water_temporary_electrified,
            water_temporary_electrified_snapshot,
            sewage_electrified,
            sewage_electrified_snapshot,
            sewage_temporary_electrified,
            sewage_temporary_electrified_snapshot,
            heating_electrified,
            heating_electrified_snapshot,
            heating_temporary_electrified,
            heating_temporary_electrified_snapshot,
            conductive_pollution,
            conductive_pollution_snapshot,
            water_pulse_groups,
            water_pulse_groups_offset,
            water_pulse_units,
            water_pulse_units_offset,
            sewage_pulse_groups,
            sewage_pulse_groups_offset,
            sewage_pulse_units,
            sewage_pulse_units_offset,
            heating_pulse_groups,
            heating_pulse_groups_offset,
            heating_pulse_units,
            heating_pulse_units_offset,
            water_node_pulse_groups,
            water_node_pulse_groups_range: (
                water_node_pulse_groups_start,
                water_node_pulse_groups_end,
            ),
            original_water_node_pulse_groups,
            water_node_pressure,
            water_node_pressure_range: (water_node_pressure_start, water_node_pressure_end),
            original_water_node_pressure,
            water_node_extra_pressure,
            water_node_extra_pressure_range: (
                water_node_extra_pressure_start,
                water_node_extra_pressure_end,
            ),
            original_water_node_extra_pressure,
            sewage_node_pulse_groups,
            sewage_node_pulse_groups_range: (
                sewage_node_pulse_groups_start,
                sewage_node_pulse_groups_end,
            ),
            original_sewage_node_pulse_groups,
            sewage_node_pressure,
            sewage_node_pressure_range: (sewage_node_pressure_start, sewage_node_pressure_end),
            original_sewage_node_pressure,
            sewage_node_extra_pressure,
            sewage_node_extra_pressure_range: (
                sewage_node_extra_pressure_start,
                sewage_node_extra_pressure_end,
            ),
            original_sewage_node_extra_pressure,
            heating_node_pulse_groups,
            heating_node_pulse_groups_range,
            original_heating_node_pulse_groups,
            heating_node_pressure,
            heating_node_pressure_range,
            original_heating_node_pressure,
            heating_node_extra_pressure,
            heating_node_extra_pressure_range,
            original_heating_node_extra_pressure,
            node_pollution,
            node_pollution_range,
            original_node_pollution,
            collect_water_pressure,
            collect_water_pressure_range,
            original_collect_water_pressure,
            collect_water_pressure_selector,
            collect_water_pressure_selector_range,
            original_collect_water_pressure_selector,
            collect_sewage_pressure,
            collect_sewage_pressure_range,
            original_collect_sewage_pressure,
            processed_cell_count,
            conductive_cell_count,
            can_continue,
            original_can_continue: can_continue,
            scalar_fields_offset,
            guide_references,
            original_body: bytes.to_vec(),
        })
    }

    /// Returns the inner DataSerializer version used for reader gates.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    fn validate_primary_sparse_lengths(&self) -> Result<(), CrpError> {
        let count = self.conductive_cells.len();
        if self.conductivity.len() != CELL_COUNT {
            return Err(invalid_cell_grid(
                0,
                "water conductivity grid has an invalid length",
            ));
        }
        ensure_sparse_length(&self.water_pressure, count, 0, "water pressure")?;
        ensure_sparse_length(&self.sewage_pressure, count, 0, "sewage pressure")?;
        ensure_sparse_length(
            &self.water_pulse_group_indexes,
            count,
            0,
            "water pulse-group indexes",
        )?;
        ensure_sparse_length(
            &self.sewage_pulse_group_indexes,
            count,
            0,
            "sewage pulse-group indexes",
        )?;
        ensure_sparse_length(&self.water_electrified, count, 0, "water electrification")?;
        ensure_sparse_length(
            &self.water_temporary_electrified,
            count,
            0,
            "temporary water electrification",
        )?;
        ensure_sparse_length(&self.sewage_electrified, count, 0, "sewage electrification")?;
        ensure_sparse_length(
            &self.sewage_temporary_electrified,
            count,
            0,
            "temporary sewage electrification",
        )?;
        if self.conductive_pollution.is_some() != (self.format_version >= POLLUTION_VERSION) {
            return Err(invalid_cell_grid(
                0,
                "conductive pollution fields do not match the inner-version-23 gate",
            ));
        }
        if let Some(values) = self.conductive_pollution.as_deref() {
            ensure_sparse_length(values, count, 0, "conductive pollution")?;
        }
        Ok(())
    }

    fn validate_heating_sparse_lengths(&self, cell: usize) -> Result<(), CrpError> {
        if self.format_version < HEATING_VERSION {
            return Err(invalid_cell_grid(
                cell,
                "heating conductivity plane is absent before inner version 227",
            ));
        }
        let Some(conductivity) = self.conductivity_heating.as_deref() else {
            return Err(invalid_cell_grid(
                cell,
                "heating conductivity plane is absent before inner version 227",
            ));
        };
        if conductivity.len() != CELL_COUNT {
            return Err(invalid_cell_grid(
                cell,
                "heating conductivity grid has an invalid length",
            ));
        }
        let count = self.heating_cells.len();
        let (Some(pressure), Some(indexes), Some(electrified), Some(temporary)) = (
            self.heating_pressure.as_deref(),
            self.heating_pulse_group_indexes.as_deref(),
            self.heating_electrified.as_deref(),
            self.heating_temporary_electrified.as_deref(),
        ) else {
            return Err(invalid_cell_grid(
                cell,
                "heating sparse fields do not match the inner-version-227 gate",
            ));
        };
        ensure_sparse_length(pressure, count, cell, "heating pressure")?;
        ensure_sparse_length(indexes, count, cell, "heating pulse-group indexes")?;
        ensure_sparse_length(electrified, count, cell, "heating electrification")?;
        ensure_sparse_length(temporary, count, cell, "temporary heating electrification")?;
        Ok(())
    }

    /// Returns the full primary 65,536-cell conductivity plane.
    pub fn conductivity(&self) -> &[u8] {
        &self.conductivity
    }

    /// Replaces a conductivity value without changing whether its cell is
    /// active. The value must stay zero for an inactive cell and nonzero for an
    /// active cell so the sparse columns retain their current mapping.
    pub fn set_conductivity(&mut self, cell: usize, conductivity: u8) -> Result<(), CrpError> {
        set_conductivity_value(
            &mut self.conductivity,
            cell,
            conductivity,
            "water conductivity",
        )
    }

    /// Adds an active primary conductive cell and inserts all its sparse
    /// fields in ascending cell-index order.
    ///
    /// The conductivity value must be nonzero, and `pollution` must match the
    /// inner-version-23 gate. Existing sparse fields are shifted together.
    /// The separate `conductive_cell_count` scalar is left unchanged because
    /// the format does not establish that it equals the number of active
    /// conductivity cells. Returns an error for an out-of-range or already-active
    /// cell, a zero conductivity value, a mismatched version-gated pollution
    /// value, or inconsistent sparse columns; errors leave the editor unchanged.
    pub fn insert_conductive_cell(
        &mut self,
        cell: usize,
        state: WaterConductiveCellData,
    ) -> Result<(), CrpError> {
        if state.conductivity == 0 {
            return Err(invalid_cell_grid(
                cell,
                "inserted water conductivity must be nonzero",
            ));
        }
        if state.pollution.is_some() != (self.format_version >= POLLUTION_VERSION) {
            return Err(invalid_cell_grid(
                cell,
                "conductive pollution value does not match the inner-version-23 gate",
            ));
        }
        self.validate_primary_sparse_lengths()?;
        let Some(current) = self.conductivity.get(cell) else {
            return Err(invalid_cell_grid(
                cell,
                "water conductivity cell index is outside the grid",
            ));
        };
        let position = match self.conductive_cells.binary_search(&cell) {
            Ok(_) => {
                return Err(invalid_cell_grid(
                    cell,
                    "water conductivity cell is already active",
                ));
            }
            Err(position) => position,
        };
        if *current != 0 {
            return Err(invalid_cell_grid(
                cell,
                "water conductivity grid and sparse cell index are inconsistent",
            ));
        }

        let Some(conductivity) = self.conductivity.get_mut(cell) else {
            return Err(invalid_cell_grid(
                cell,
                "water conductivity cell index is outside the grid",
            ));
        };
        *conductivity = state.conductivity;
        self.conductive_cells.insert(position, cell);
        self.water_pressure.insert(position, state.water_pressure);
        self.sewage_pressure.insert(position, state.sewage_pressure);
        self.water_pulse_group_indexes
            .insert(position, state.water_pulse_group_index);
        self.sewage_pulse_group_indexes
            .insert(position, state.sewage_pulse_group_index);
        self.water_electrified
            .insert(position, state.water_electrified);
        self.water_temporary_electrified
            .insert(position, state.water_temporary_electrified);
        self.sewage_electrified
            .insert(position, state.sewage_electrified);
        self.sewage_temporary_electrified
            .insert(position, state.sewage_temporary_electrified);
        if let (Some(values), Some(pollution)) =
            (self.conductive_pollution.as_mut(), state.pollution)
        {
            values.insert(position, pollution);
        }
        Ok(())
    }

    /// Removes an active primary conductive cell and returns all of its
    /// serialized sparse fields, including its conductivity value. Returns an
    /// error if the cell is out of range or inactive, or if sparse columns are
    /// inconsistent; errors leave the editor unchanged.
    pub fn remove_conductive_cell(
        &mut self,
        cell: usize,
    ) -> Result<WaterConductiveCellData, CrpError> {
        self.validate_primary_sparse_lengths()?;
        let Some(conductivity) = self.conductivity.get(cell).copied() else {
            return Err(invalid_cell_grid(
                cell,
                "water conductivity cell index is outside the grid",
            ));
        };
        let position = sparse_cell_position(&self.conductive_cells, cell, "water conductivity")?;
        if conductivity == 0 {
            return Err(invalid_cell_grid(
                cell,
                "water conductivity grid and sparse cell index are inconsistent",
            ));
        }
        let state = WaterConductiveCellData {
            conductivity,
            water_pressure: sparse_value(&self.water_pressure, position, cell, "water pressure")?,
            sewage_pressure: sparse_value(
                &self.sewage_pressure,
                position,
                cell,
                "sewage pressure",
            )?,
            water_pulse_group_index: sparse_value(
                &self.water_pulse_group_indexes,
                position,
                cell,
                "water pulse-group index",
            )?,
            sewage_pulse_group_index: sparse_value(
                &self.sewage_pulse_group_indexes,
                position,
                cell,
                "sewage pulse-group index",
            )?,
            water_electrified: sparse_value(
                &self.water_electrified,
                position,
                cell,
                "water electrification",
            )?,
            water_temporary_electrified: sparse_value(
                &self.water_temporary_electrified,
                position,
                cell,
                "temporary water electrification",
            )?,
            sewage_electrified: sparse_value(
                &self.sewage_electrified,
                position,
                cell,
                "sewage electrification",
            )?,
            sewage_temporary_electrified: sparse_value(
                &self.sewage_temporary_electrified,
                position,
                cell,
                "temporary sewage electrification",
            )?,
            pollution: match self.conductive_pollution.as_deref() {
                Some(values) => Some(sparse_value(
                    values,
                    position,
                    cell,
                    "conductive pollution",
                )?),
                None => None,
            },
        };

        let Some(conductivity) = self.conductivity.get_mut(cell) else {
            return Err(invalid_cell_grid(
                cell,
                "water conductivity cell index is outside the grid",
            ));
        };
        *conductivity = 0;
        self.conductive_cells.remove(position);
        self.water_pressure.remove(position);
        self.sewage_pressure.remove(position);
        self.water_pulse_group_indexes.remove(position);
        self.sewage_pulse_group_indexes.remove(position);
        self.water_electrified.remove(position);
        self.water_temporary_electrified.remove(position);
        self.sewage_electrified.remove(position);
        self.sewage_temporary_electrified.remove(position);
        if let Some(values) = self.conductive_pollution.as_mut() {
            values.remove(position);
        }
        Ok(state)
    }

    /// Returns indexes of cells with nonzero primary conductivity.
    pub fn conductive_cells(&self) -> &[usize] {
        &self.conductive_cells
    }

    /// Returns the secondary conductivity plane from version 227.
    pub fn conductivity_heating(&self) -> Option<&[u8]> {
        self.conductivity_heating.as_deref()
    }

    /// Replaces a heating conductivity value from inner version 227 without
    /// changing whether its cell is active.
    pub fn set_heating_conductivity(
        &mut self,
        cell: usize,
        conductivity: u8,
    ) -> Result<(), CrpError> {
        let Some(values) = self.conductivity_heating.as_mut() else {
            return Err(invalid_cell_grid(
                cell,
                "heating conductivity plane is absent before inner version 227",
            ));
        };
        set_conductivity_value(values, cell, conductivity, "heating conductivity")
    }

    /// Adds an active heating cell and inserts all its sparse fields in
    /// ascending cell-index order. Available from inner version 227. Returns
    /// an error for earlier versions, an out-of-range or already-active cell,
    /// zero conductivity, or inconsistent sparse columns; errors leave the
    /// editor unchanged.
    pub fn insert_heating_cell(
        &mut self,
        cell: usize,
        state: HeatingConductiveCellData,
    ) -> Result<(), CrpError> {
        if state.conductivity == 0 {
            return Err(invalid_cell_grid(
                cell,
                "inserted heating conductivity must be nonzero",
            ));
        }
        self.validate_heating_sparse_lengths(cell)?;
        let Some(values) = self.conductivity_heating.as_mut() else {
            return Err(invalid_cell_grid(
                cell,
                "heating conductivity plane is absent before inner version 227",
            ));
        };
        let Some(current) = values.get(cell) else {
            return Err(invalid_cell_grid(
                cell,
                "heating conductivity cell index is outside the grid",
            ));
        };
        let position = match self.heating_cells.binary_search(&cell) {
            Ok(_) => {
                return Err(invalid_cell_grid(
                    cell,
                    "heating conductivity cell is already active",
                ));
            }
            Err(position) => position,
        };
        if *current != 0 {
            return Err(invalid_cell_grid(
                cell,
                "heating conductivity grid and sparse cell index are inconsistent",
            ));
        }
        let (Some(pressure), Some(indexes), Some(electrified), Some(temporary)) = (
            self.heating_pressure.as_mut(),
            self.heating_pulse_group_indexes.as_mut(),
            self.heating_electrified.as_mut(),
            self.heating_temporary_electrified.as_mut(),
        ) else {
            return Err(invalid_cell_grid(
                cell,
                "heating sparse fields do not match the inner-version-227 gate",
            ));
        };
        let Some(conductivity) = values.get_mut(cell) else {
            return Err(invalid_cell_grid(
                cell,
                "heating conductivity cell index is outside the grid",
            ));
        };
        *conductivity = state.conductivity;
        self.heating_cells.insert(position, cell);
        pressure.insert(position, state.pressure);
        indexes.insert(position, state.pulse_group_index);
        electrified.insert(position, state.electrified);
        temporary.insert(position, state.temporary_electrified);
        Ok(())
    }

    /// Removes an active heating cell and returns all of its serialized sparse
    /// fields, including its conductivity value. Available from inner version
    /// 227. Returns an error if the version is earlier or the cell is out of
    /// range or inactive, or if sparse columns are inconsistent; errors leave
    /// the editor unchanged.
    pub fn remove_heating_cell(
        &mut self,
        cell: usize,
    ) -> Result<HeatingConductiveCellData, CrpError> {
        self.validate_heating_sparse_lengths(cell)?;
        let Some(conductivity) = self
            .conductivity_heating
            .as_ref()
            .and_then(|values| values.get(cell))
            .copied()
        else {
            return Err(invalid_cell_grid(
                cell,
                "heating conductivity cell index is outside the grid or heating is unavailable",
            ));
        };
        let position = sparse_cell_position(&self.heating_cells, cell, "heating conductivity")?;
        if conductivity == 0 {
            return Err(invalid_cell_grid(
                cell,
                "heating conductivity grid and sparse cell index are inconsistent",
            ));
        }
        let (
            Some(pressure_values),
            Some(index_values),
            Some(electrified_values),
            Some(temporary_values),
        ) = (
            self.heating_pressure.as_deref(),
            self.heating_pulse_group_indexes.as_deref(),
            self.heating_electrified.as_deref(),
            self.heating_temporary_electrified.as_deref(),
        )
        else {
            return Err(invalid_cell_grid(
                cell,
                "heating sparse fields do not match the inner-version-227 gate",
            ));
        };
        let state = HeatingConductiveCellData {
            conductivity,
            pressure: sparse_value(pressure_values, position, cell, "heating pressure")?,
            pulse_group_index: sparse_value(
                index_values,
                position,
                cell,
                "heating pulse-group index",
            )?,
            electrified: sparse_value(
                electrified_values,
                position,
                cell,
                "heating electrification",
            )?,
            temporary_electrified: sparse_value(
                temporary_values,
                position,
                cell,
                "temporary heating electrification",
            )?,
        };
        let Some(values) = self.conductivity_heating.as_mut() else {
            return Err(invalid_cell_grid(
                cell,
                "heating conductivity plane is absent before inner version 227",
            ));
        };
        let Some(conductivity) = values.get_mut(cell) else {
            return Err(invalid_cell_grid(
                cell,
                "heating conductivity cell index is outside the grid",
            ));
        };
        *conductivity = 0;
        self.heating_cells.remove(position);
        if let Some(values) = self.heating_pressure.as_mut() {
            values.remove(position);
        }
        if let Some(values) = self.heating_pulse_group_indexes.as_mut() {
            values.remove(position);
        }
        if let Some(values) = self.heating_electrified.as_mut() {
            values.remove(position);
        }
        if let Some(values) = self.heating_temporary_electrified.as_mut() {
            values.remove(position);
        }
        Ok(state)
    }

    /// Returns indexes of cells with nonzero secondary conductivity.
    pub fn heating_cells(&self) -> &[usize] {
        &self.heating_cells
    }

    /// Returns sparse signed water pressures in primary-conductive-cell order.
    pub fn water_pressure(&self) -> &[i16] {
        &self.water_pressure
    }

    /// Replaces water pressure for one existing conductive cell.
    pub fn set_water_pressure(&mut self, cell: usize, pressure: i16) -> Result<(), CrpError> {
        set_sparse_i16_cell(
            &mut self.water_pressure,
            &self.conductive_cells,
            cell,
            pressure,
            "water pressure",
        )
    }

    /// Returns sparse signed sewage pressures in primary-conductive-cell order.
    pub fn sewage_pressure(&self) -> &[i16] {
        &self.sewage_pressure
    }

    /// Replaces sewage pressure for one existing conductive cell.
    pub fn set_sewage_pressure(&mut self, cell: usize, pressure: i16) -> Result<(), CrpError> {
        set_sparse_i16_cell(
            &mut self.sewage_pressure,
            &self.conductive_cells,
            cell,
            pressure,
            "sewage pressure",
        )
    }

    /// Returns sparse signed heating pressures from version 227.
    pub fn heating_pressure(&self) -> Option<&[i16]> {
        self.heating_pressure.as_deref()
    }

    /// Replaces heating pressure for one existing heating-conductive cell from
    /// inner version 227.
    pub fn set_heating_pressure(&mut self, cell: usize, pressure: i16) -> Result<(), CrpError> {
        let Some(values) = self.heating_pressure.as_mut() else {
            return Err(invalid_cell_grid(
                cell,
                "heating pressure values are absent before inner version 227",
            ));
        };
        set_sparse_i16_cell(
            values,
            &self.heating_cells,
            cell,
            pressure,
            "heating pressure",
        )
    }

    /// Returns sparse water pulse-group indexes in primary-conductive order.
    pub fn water_pulse_group_indexes(&self) -> &[u16] {
        &self.water_pulse_group_indexes
    }

    /// Replaces the pulse-group index for one existing conductive water cell.
    pub fn set_water_pulse_group_index_for_cell(
        &mut self,
        cell: usize,
        pulse_group_index: u16,
    ) -> Result<(), CrpError> {
        set_sparse_ushort_cell(
            &mut self.water_pulse_group_indexes,
            &self.conductive_cells,
            cell,
            pulse_group_index,
            "water pulse-group index",
        )
    }

    /// Returns sparse sewage pulse-group indexes in primary-conductive order.
    pub fn sewage_pulse_group_indexes(&self) -> &[u16] {
        &self.sewage_pulse_group_indexes
    }

    /// Replaces the pulse-group index for one existing conductive sewage cell.
    pub fn set_sewage_pulse_group_index_for_cell(
        &mut self,
        cell: usize,
        pulse_group_index: u16,
    ) -> Result<(), CrpError> {
        set_sparse_ushort_cell(
            &mut self.sewage_pulse_group_indexes,
            &self.conductive_cells,
            cell,
            pulse_group_index,
            "sewage pulse-group index",
        )
    }

    /// Returns sparse heating pulse-group indexes from version 227.
    pub fn heating_pulse_group_indexes(&self) -> Option<&[u16]> {
        self.heating_pulse_group_indexes.as_deref()
    }

    /// Replaces the pulse-group index for one existing heating-conductive cell
    /// from inner version 227.
    pub fn set_heating_pulse_group_index_for_cell(
        &mut self,
        cell: usize,
        pulse_group_index: u16,
    ) -> Result<(), CrpError> {
        let Some(values) = self.heating_pulse_group_indexes.as_mut() else {
            return Err(invalid_cell_grid(
                cell,
                "heating pulse-group indexes are absent before inner version 227",
            ));
        };
        set_sparse_ushort_cell(
            values,
            &self.heating_cells,
            cell,
            pulse_group_index,
            "heating pulse-group index",
        )
    }

    /// Returns the version-73 closest-pipe segment grid.
    pub fn closest_pipe_segments(&self) -> Option<&[u16]> {
        self.closest_pipe_segments.as_deref()
    }

    /// Replaces one closest-pipe segment value from inner version 73.
    pub fn set_closest_pipe_segment(&mut self, cell: usize, segment: u16) -> Result<(), CrpError> {
        let Some(values) = self.closest_pipe_segments.as_mut() else {
            return Err(invalid_cell_grid(
                cell,
                "closest-pipe segment grid is absent before inner version 73",
            ));
        };
        set_grid_ushort_value(values, cell, segment, "closest-pipe segment")
    }

    /// Returns the second closest-segment grid from version 227.
    pub fn closest_heating_segments(&self) -> Option<&[u16]> {
        self.closest_heating_segments.as_deref()
    }

    /// Replaces one closest-heating segment value from inner version 227.
    pub fn set_closest_heating_segment(
        &mut self,
        cell: usize,
        segment: u16,
    ) -> Result<(), CrpError> {
        let Some(values) = self.closest_heating_segments.as_mut() else {
            return Err(invalid_cell_grid(
                cell,
                "closest-heating segment grid is absent before inner version 227",
            ));
        };
        set_grid_ushort_value(values, cell, segment, "closest-heating segment")
    }

    /// Returns sparse current water electrification flags.
    pub fn water_electrified(&self) -> &[bool] {
        &self.water_electrified
    }

    /// Replaces one water electrification flag by its cell index. The cell
    /// must already be present in the conductivity plane.
    pub fn set_water_electrified(
        &mut self,
        cell: usize,
        electrified: bool,
    ) -> Result<(), CrpError> {
        set_sparse_bool_cell(
            &mut self.water_electrified,
            &self.conductive_cells,
            cell,
            electrified,
            "water electrification",
        )
    }

    /// Returns sparse temporary water electrification flags.
    pub fn water_temporary_electrified(&self) -> &[bool] {
        &self.water_temporary_electrified
    }

    /// Replaces one temporary water electrification flag by its cell index.
    /// The cell must already be present in the conductivity plane.
    pub fn set_water_temporary_electrified(
        &mut self,
        cell: usize,
        electrified: bool,
    ) -> Result<(), CrpError> {
        set_sparse_bool_cell(
            &mut self.water_temporary_electrified,
            &self.conductive_cells,
            cell,
            electrified,
            "temporary water electrification",
        )
    }

    /// Returns sparse current sewage electrification flags.
    pub fn sewage_electrified(&self) -> &[bool] {
        &self.sewage_electrified
    }

    /// Replaces one sewage electrification flag by its cell index. The cell
    /// must already be present in the conductivity plane.
    pub fn set_sewage_electrified(
        &mut self,
        cell: usize,
        electrified: bool,
    ) -> Result<(), CrpError> {
        set_sparse_bool_cell(
            &mut self.sewage_electrified,
            &self.conductive_cells,
            cell,
            electrified,
            "sewage electrification",
        )
    }

    /// Returns sparse temporary sewage electrification flags.
    pub fn sewage_temporary_electrified(&self) -> &[bool] {
        &self.sewage_temporary_electrified
    }

    /// Replaces one temporary sewage electrification flag by its cell index.
    /// The cell must already be present in the conductivity plane.
    pub fn set_sewage_temporary_electrified(
        &mut self,
        cell: usize,
        electrified: bool,
    ) -> Result<(), CrpError> {
        set_sparse_bool_cell(
            &mut self.sewage_temporary_electrified,
            &self.conductive_cells,
            cell,
            electrified,
            "temporary sewage electrification",
        )
    }

    /// Returns sparse heating electrification flags from version 227.
    pub fn heating_electrified(&self) -> Option<&[bool]> {
        self.heating_electrified.as_deref()
    }

    /// Replaces one heating electrification flag by its cell index from inner
    /// version 227. The cell must already be present in the heating plane.
    pub fn set_heating_electrified(
        &mut self,
        cell: usize,
        electrified: bool,
    ) -> Result<(), CrpError> {
        let Some(values) = self.heating_electrified.as_mut() else {
            return Err(invalid_cell_grid(
                cell,
                "heating electrification values are absent before inner version 227",
            ));
        };
        set_sparse_bool_cell(
            values,
            &self.heating_cells,
            cell,
            electrified,
            "heating electrification",
        )
    }

    /// Returns sparse temporary heating electrification flags from version 227.
    pub fn heating_temporary_electrified(&self) -> Option<&[bool]> {
        self.heating_temporary_electrified.as_deref()
    }

    /// Replaces one temporary heating electrification flag by its cell index
    /// from inner version 227. The cell must already be present in the heating
    /// plane.
    pub fn set_heating_temporary_electrified(
        &mut self,
        cell: usize,
        electrified: bool,
    ) -> Result<(), CrpError> {
        let Some(values) = self.heating_temporary_electrified.as_mut() else {
            return Err(invalid_cell_grid(
                cell,
                "temporary heating electrification values are absent before inner version 227",
            ));
        };
        set_sparse_bool_cell(
            values,
            &self.heating_cells,
            cell,
            electrified,
            "temporary heating electrification",
        )
    }

    /// Returns sparse pollution bytes from version 23.
    pub fn conductive_pollution(&self) -> Option<&[u8]> {
        self.conductive_pollution.as_deref()
    }

    /// Replaces one conductive pollution value by its cell index from inner
    /// version 23. The cell must already be present in the conductivity plane.
    pub fn set_conductive_pollution(&mut self, cell: usize, pollution: u8) -> Result<(), CrpError> {
        let Some(values) = self.conductive_pollution.as_mut() else {
            return Err(invalid_cell_grid(
                cell,
                "conductive pollution values are absent before inner version 23",
            ));
        };
        set_sparse_byte_cell(
            values,
            &self.conductive_cells,
            cell,
            pollution,
            "conductive pollution",
        )
    }

    /// Returns active water pulse groups in serialized order.
    pub fn water_pulse_groups(&self) -> &[WaterPulseGroup] {
        &self.water_pulse_groups
    }

    /// Returns active water pulse units in serialized order.
    pub fn water_pulse_units(&self) -> &[WaterPulseUnit] {
        &self.water_pulse_units
    }

    /// Returns active sewage pulse groups in serialized order.
    pub fn sewage_pulse_groups(&self) -> &[WaterPulseGroup] {
        &self.sewage_pulse_groups
    }

    /// Returns active sewage pulse units in serialized order.
    pub fn sewage_pulse_units(&self) -> &[WaterPulseUnit] {
        &self.sewage_pulse_units
    }

    /// Returns active heating pulse groups from version 227.
    pub fn heating_pulse_groups(&self) -> Option<&[WaterPulseGroup]> {
        self.heating_pulse_groups.as_deref()
    }

    /// Replaces one existing water pulse group without changing the serialized
    /// count or body length. `collect_pressure` must be present from inner
    /// version 270 and absent in earlier versions.
    pub fn set_water_pulse_group(
        &mut self,
        index: usize,
        group: WaterPulseGroup,
    ) -> Result<(), CrpError> {
        let has_collect_pressure = self.format_version >= COLLECT_WATER_BOOL_VERSION;
        if group.collect_pressure.is_some() != has_collect_pressure {
            return Err(invalid_pulse_group(
                self.water_pulse_groups_offset,
                "water pulse-group collect pressure does not match the version gate",
            ));
        }
        let Some(existing) = self.water_pulse_groups.get_mut(index) else {
            return Err(invalid_pulse_group(
                self.water_pulse_groups_offset,
                "water pulse-group index is outside the existing records",
            ));
        };
        *existing = group;
        Ok(())
    }

    /// Replaces one existing sewage pulse group without changing the serialized
    /// count or body length. `collect_pressure` must be present from inner
    /// version 270 and absent in earlier versions.
    pub fn set_sewage_pulse_group(
        &mut self,
        index: usize,
        group: WaterPulseGroup,
    ) -> Result<(), CrpError> {
        let has_collect_pressure = self.format_version >= COLLECT_WATER_BOOL_VERSION;
        if group.collect_pressure.is_some() != has_collect_pressure {
            return Err(invalid_pulse_group(
                self.sewage_pulse_groups_offset,
                "sewage pulse-group collect pressure does not match the version gate",
            ));
        }
        let Some(existing) = self.sewage_pulse_groups.get_mut(index) else {
            return Err(invalid_pulse_group(
                self.sewage_pulse_groups_offset,
                "sewage pulse-group index is outside the existing records",
            ));
        };
        *existing = group;
        Ok(())
    }

    /// Replaces one existing heating pulse group from inner version 227
    /// without changing the serialized count or body length. Heating groups
    /// do not have a collect-pressure field.
    pub fn set_heating_pulse_group(
        &mut self,
        index: usize,
        group: WaterPulseGroup,
    ) -> Result<(), CrpError> {
        let Some(groups_offset) = self.heating_pulse_groups_offset else {
            return Err(invalid_pulse_group(
                self.original_body.len(),
                "heating pulse groups are absent before inner version 227",
            ));
        };
        if group.collect_pressure.is_some() {
            return Err(invalid_pulse_group(
                groups_offset,
                "heating pulse groups do not contain collect pressure",
            ));
        }
        let Some(groups) = self.heating_pulse_groups.as_mut() else {
            return Err(invalid_pulse_group(
                groups_offset,
                "heating pulse groups are absent from the decoded body",
            ));
        };
        let Some(existing) = groups.get_mut(index) else {
            return Err(invalid_pulse_group(
                groups_offset,
                "heating pulse-group index is outside the existing records",
            ));
        };
        *existing = group;
        Ok(())
    }

    /// Returns active heating pulse units from version 227.
    pub fn heating_pulse_units(&self) -> Option<&[WaterPulseUnit]> {
        self.heating_pulse_units.as_deref()
    }

    /// Replaces one existing water pulse unit without changing its serialized
    /// count or body length.
    pub fn set_water_pulse_unit(
        &mut self,
        index: usize,
        unit: WaterPulseUnit,
    ) -> Result<(), CrpError> {
        let Some(existing) = self.water_pulse_units.get_mut(index) else {
            return Err(invalid_pulse_unit(
                self.water_pulse_units_offset,
                "water pulse-unit index is outside the existing records",
            ));
        };
        *existing = unit;
        Ok(())
    }

    /// Replaces one existing sewage pulse unit without changing its serialized
    /// count or body length.
    pub fn set_sewage_pulse_unit(
        &mut self,
        index: usize,
        unit: WaterPulseUnit,
    ) -> Result<(), CrpError> {
        let Some(existing) = self.sewage_pulse_units.get_mut(index) else {
            return Err(invalid_pulse_unit(
                self.sewage_pulse_units_offset,
                "sewage pulse-unit index is outside the existing records",
            ));
        };
        *existing = unit;
        Ok(())
    }

    /// Replaces one existing heating pulse unit from inner version 227 without
    /// changing its serialized count or body length.
    pub fn set_heating_pulse_unit(
        &mut self,
        index: usize,
        unit: WaterPulseUnit,
    ) -> Result<(), CrpError> {
        let Some(units_offset) = self.heating_pulse_units_offset else {
            return Err(invalid_pulse_unit(
                self.original_body.len(),
                "heating pulse units are absent before inner version 227",
            ));
        };
        let Some(units) = self.heating_pulse_units.as_mut() else {
            return Err(invalid_pulse_unit(
                units_offset,
                "heating pulse units are absent from the decoded body",
            ));
        };
        let Some(existing) = units.get_mut(index) else {
            return Err(invalid_pulse_unit(
                units_offset,
                "heating pulse-unit index is outside the existing records",
            ));
        };
        *existing = unit;
        Ok(())
    }

    /// Returns all 32,768 water node pulse-group indexes.
    pub fn water_node_pulse_groups(&self) -> &[u16] {
        &self.water_node_pulse_groups
    }

    /// Replaces one stored water node pulse-group index. The serialized prefix
    /// contains 16,384 nodes before inner version 202 and all 32,768 nodes from
    /// version 202 onward.
    pub fn set_water_node_pulse_group_index(
        &mut self,
        node: usize,
        pulse_group_index: u16,
    ) -> Result<(), CrpError> {
        set_node_ushort_value(
            &mut self.water_node_pulse_groups,
            self.format_version,
            node,
            pulse_group_index,
            "water pulse-group",
        )
    }

    /// Returns all 32,768 current water node pressures.
    pub fn water_node_pressure(&self) -> &[u16] {
        &self.water_node_pressure
    }

    /// Replaces one stored water node pressure. The serialized prefix contains
    /// 16,384 nodes before inner version 202 and all 32,768 nodes from version
    /// 202 onward.
    pub fn set_water_node_pressure(&mut self, node: usize, pressure: u16) -> Result<(), CrpError> {
        set_node_ushort_value(
            &mut self.water_node_pressure,
            self.format_version,
            node,
            pressure,
            "water",
        )?;
        if self
            .collect_water_pressure_selector
            .as_ref()
            .and_then(|selectors| selectors.get(node))
            .copied()
            .unwrap_or(false)
        {
            let Some(collect_pressure) = self.collect_water_pressure.get_mut(node) else {
                return Err(invalid_collect_pressure(
                    node,
                    "collect-water node index is outside the decoded grid",
                ));
            };
            *collect_pressure = pressure;
        }
        Ok(())
    }

    /// Returns all 32,768 extra water node pressures.
    pub fn water_node_extra_pressure(&self) -> &[u16] {
        &self.water_node_extra_pressure
    }

    /// Replaces one stored extra water node pressure.
    pub fn set_water_node_extra_pressure(
        &mut self,
        node: usize,
        pressure: u16,
    ) -> Result<(), CrpError> {
        set_node_ushort_value(
            &mut self.water_node_extra_pressure,
            self.format_version,
            node,
            pressure,
            "extra water",
        )
    }

    /// Returns all 32,768 sewage node pulse-group indexes.
    pub fn sewage_node_pulse_groups(&self) -> &[u16] {
        &self.sewage_node_pulse_groups
    }

    /// Replaces one stored sewage node pulse-group index. The serialized prefix
    /// contains 16,384 nodes before inner version 202 and all 32,768 nodes from
    /// version 202 onward.
    pub fn set_sewage_node_pulse_group_index(
        &mut self,
        node: usize,
        pulse_group_index: u16,
    ) -> Result<(), CrpError> {
        set_node_ushort_value(
            &mut self.sewage_node_pulse_groups,
            self.format_version,
            node,
            pulse_group_index,
            "sewage pulse-group",
        )
    }

    /// Returns all 32,768 current sewage node pressures.
    pub fn sewage_node_pressure(&self) -> &[u16] {
        &self.sewage_node_pressure
    }

    /// Replaces one stored sewage node pressure. The serialized prefix contains
    /// 16,384 nodes before inner version 202 and all 32,768 nodes from version
    /// 202 onward.
    pub fn set_sewage_node_pressure(&mut self, node: usize, pressure: u16) -> Result<(), CrpError> {
        set_node_ushort_value(
            &mut self.sewage_node_pressure,
            self.format_version,
            node,
            pressure,
            "sewage",
        )
    }

    /// Returns all 32,768 extra sewage node pressures.
    pub fn sewage_node_extra_pressure(&self) -> &[u16] {
        &self.sewage_node_extra_pressure
    }

    /// Replaces one stored extra sewage node pressure.
    pub fn set_sewage_node_extra_pressure(
        &mut self,
        node: usize,
        pressure: u16,
    ) -> Result<(), CrpError> {
        set_node_ushort_value(
            &mut self.sewage_node_extra_pressure,
            self.format_version,
            node,
            pressure,
            "extra sewage",
        )
    }

    /// Returns the version-227 heating node pulse-group indexes.
    pub fn heating_node_pulse_groups(&self) -> Option<&[u16]> {
        self.heating_node_pulse_groups.as_deref()
    }

    /// Replaces one stored heating node pulse-group index from inner version
    /// 227.
    pub fn set_heating_node_pulse_group_index(
        &mut self,
        node: usize,
        pulse_group_index: u16,
    ) -> Result<(), CrpError> {
        let Some(values) = self.heating_node_pulse_groups.as_mut() else {
            return Err(invalid_node_array(
                node,
                "heating node pulse-group values are absent before inner version 227",
            ));
        };
        set_node_ushort_value(
            values,
            self.format_version,
            node,
            pulse_group_index,
            "heating pulse-group",
        )
    }

    /// Returns the version-227 current heating node pressures.
    pub fn heating_node_pressure(&self) -> Option<&[u16]> {
        self.heating_node_pressure.as_deref()
    }

    /// Replaces one stored heating node pressure from inner version 227.
    pub fn set_heating_node_pressure(
        &mut self,
        node: usize,
        pressure: u16,
    ) -> Result<(), CrpError> {
        let Some(values) = self.heating_node_pressure.as_mut() else {
            return Err(invalid_node_array(
                node,
                "heating node-pressure values are absent before inner version 227",
            ));
        };
        set_node_ushort_value(values, self.format_version, node, pressure, "heating")
    }

    /// Returns the version-227 extra heating node pressures.
    pub fn heating_node_extra_pressure(&self) -> Option<&[u16]> {
        self.heating_node_extra_pressure.as_deref()
    }

    /// Replaces one stored extra heating node pressure from inner version 227.
    pub fn set_heating_node_extra_pressure(
        &mut self,
        node: usize,
        pressure: u16,
    ) -> Result<(), CrpError> {
        let Some(values) = self.heating_node_extra_pressure.as_mut() else {
            return Err(invalid_node_array(
                node,
                "heating extra node-pressure values are absent before inner version 227",
            ));
        };
        set_node_ushort_value(values, self.format_version, node, pressure, "extra heating")
    }

    /// Returns the version-23 node pollution grid.
    pub fn node_pollution(&self) -> Option<&[u8]> {
        self.node_pollution.as_deref()
    }

    /// Replaces one stored node pollution value from inner version 23.
    /// The serialized prefix contains 16,384 nodes before inner version 202
    /// and all 32,768 nodes from version 202 onward.
    pub fn set_node_pollution(&mut self, node: usize, value: u8) -> Result<(), CrpError> {
        let Some(values) = self.node_pollution.as_mut() else {
            return Err(invalid_node_array(
                node,
                "node pollution values are absent before inner version 23",
            ));
        };
        set_node_byte_value(values, self.format_version, node, value, "node pollution")
    }

    /// Returns the full collect-water pressure grid, including old defaults.
    pub fn collect_water_pressure(&self) -> &[u16] {
        &self.collect_water_pressure
    }

    /// Returns the version-270 Bool selectors that derive collect-water
    /// pressure from the current water-pressure grid.
    pub fn collect_water_pressure_selector(&self) -> Option<&[bool]> {
        self.collect_water_pressure_selector.as_deref()
    }

    /// Returns the full collect-sewage pressure grid, including old defaults.
    pub fn collect_sewage_pressure(&self) -> &[u16] {
        &self.collect_sewage_pressure
    }

    /// Replaces one stored collect-water pressure from inner version 271.
    /// Earlier versions either omit this field or store only a Bool selector
    /// at version 270, which cannot represent an arbitrary pressure value.
    pub fn set_collect_water_pressure(
        &mut self,
        node: usize,
        pressure: u16,
    ) -> Result<(), CrpError> {
        if self.collect_water_pressure_range.is_none() {
            let reason = if self.format_version == COLLECT_WATER_BOOL_VERSION {
                "version-270 collect-water values are Bool selectors and cannot store arbitrary pressure"
            } else {
                "collect-water pressure values are stored from inner version 271"
            };
            return Err(invalid_collect_pressure(self.scalar_fields_offset, reason));
        }
        let stored_node_count = stored_node_count(self.format_version);
        if node >= stored_node_count {
            return Err(invalid_collect_pressure(
                node,
                "collect-water node index is outside the stored prefix",
            ));
        }
        let Some(value) = self.collect_water_pressure.get_mut(node) else {
            return Err(invalid_collect_pressure(
                node,
                "collect-water node index is outside the decoded grid",
            ));
        };
        *value = pressure;
        Ok(())
    }

    /// Replaces one version-270 collect-water Bool selector. When enabled,
    /// the in-memory collect-water pressure follows the current water pressure
    /// for that node; setting it to false clears the derived pressure.
    pub fn set_collect_water_pressure_selector(
        &mut self,
        node: usize,
        selected: bool,
    ) -> Result<(), CrpError> {
        if self.collect_water_pressure_selector.is_none() {
            return Err(invalid_collect_pressure(
                self.scalar_fields_offset,
                "collect-water Bool selectors are stored only at inner version 270",
            ));
        }
        if node >= stored_node_count(self.format_version) {
            return Err(invalid_collect_pressure(
                node,
                "collect-water node index is outside the stored prefix",
            ));
        }
        let derived_pressure = if selected {
            *self.water_node_pressure.get(node).ok_or_else(|| {
                invalid_collect_pressure(
                    node,
                    "water node index is outside the decoded pressure grid",
                )
            })?
        } else {
            0
        };
        let selector = self
            .collect_water_pressure_selector
            .as_mut()
            .and_then(|selectors| selectors.get_mut(node))
            .ok_or_else(|| {
                invalid_collect_pressure(
                    node,
                    "collect-water node index is outside the decoded selector grid",
                )
            })?;
        let collect_pressure = self.collect_water_pressure.get_mut(node).ok_or_else(|| {
            invalid_collect_pressure(
                node,
                "collect-water node index is outside the decoded pressure grid",
            )
        })?;
        *selector = selected;
        *collect_pressure = derived_pressure;
        Ok(())
    }

    /// Replaces one stored collect-sewage pressure from inner version 272.
    pub fn set_collect_sewage_pressure(
        &mut self,
        node: usize,
        pressure: u16,
    ) -> Result<(), CrpError> {
        if self.collect_sewage_pressure_range.is_none() {
            return Err(invalid_collect_pressure(
                self.scalar_fields_offset,
                "collect-sewage pressure values are stored from inner version 272",
            ));
        }
        let stored_node_count = stored_node_count(self.format_version);
        if node >= stored_node_count {
            return Err(invalid_collect_pressure(
                node,
                "collect-sewage node index is outside the stored prefix",
            ));
        }
        let Some(value) = self.collect_sewage_pressure.get_mut(node) else {
            return Err(invalid_collect_pressure(
                node,
                "collect-sewage node index is outside the decoded grid",
            ));
        };
        *value = pressure;
        Ok(())
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

    /// Replaces the stored processed-cell counter without changing the body
    /// length.
    pub fn set_processed_cell_count(&mut self, value: i32) {
        self.processed_cell_count = value;
    }

    /// Replaces the stored conductive-cell counter without changing the body
    /// length.
    pub fn set_conductive_cell_count(&mut self, value: i32) {
        self.conductive_cell_count = value;
    }

    /// Replaces the continuation state without changing the body length.
    /// The original byte is retained when its decoded boolean value is unchanged.
    pub fn set_can_continue(&mut self, value: bool) {
        self.can_continue = value;
    }

    /// Returns the two guide headers from version 94, without resolving them.
    pub fn guide_references(&self) -> &[NetObjectHeader] {
        &self.guide_references
    }

    /// Returns whether the game reader derives `refreshGrid` as true.
    pub fn refresh_grid(&self) -> bool {
        self.format_version < CLOSEST_PIPE_SEGMENT_VERSION
    }

    /// Returns the complete source body unchanged, including guide headers and
    /// the closing profiler reference.
    pub fn encode_unchanged(&self) -> Vec<u8> {
        self.original_body.clone()
    }

    /// Encodes edited conductivity grids and their sparse cell fields, pulse
    /// records, node arrays, collect-pressure arrays, counters, and the
    /// continuation flag while preserving all other bytes, including guide
    /// references and the closing profiler reference. Re-encoded UShort arrays
    /// use the canonical UShort encoding, Byte arrays use literal blocks, and
    /// Bool arrays use bit-packed literal blocks. Topology edits and changed
    /// arrays can resize the body. Returns an error if a captured field range
    /// or sparse-column length is invalid.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut bytes = self.original_body.clone();
        encode_pulse_groups(
            &mut bytes,
            self.water_pulse_groups_offset,
            &self.water_pulse_groups,
            self.format_version >= COLLECT_WATER_BOOL_VERSION,
            "water",
        )?;
        encode_pulse_units(
            &mut bytes,
            self.water_pulse_units_offset,
            &self.water_pulse_units,
            "water",
        )?;
        encode_pulse_groups(
            &mut bytes,
            self.sewage_pulse_groups_offset,
            &self.sewage_pulse_groups,
            self.format_version >= COLLECT_WATER_BOOL_VERSION,
            "sewage",
        )?;
        encode_pulse_units(
            &mut bytes,
            self.sewage_pulse_units_offset,
            &self.sewage_pulse_units,
            "sewage",
        )?;
        if let (Some(offset), Some(groups)) = (
            self.heating_pulse_groups_offset,
            self.heating_pulse_groups.as_deref(),
        ) {
            encode_pulse_groups(&mut bytes, offset, groups, false, "heating")?;
        }
        if let (Some(offset), Some(units)) = (
            self.heating_pulse_units_offset,
            self.heating_pulse_units.as_deref(),
        ) {
            encode_pulse_units(&mut bytes, offset, units, "heating")?;
        }
        let processed_end =
            self.scalar_fields_offset
                .checked_add(4)
                .ok_or(CrpError::ArithmeticOverflow(
                    "WaterManager processed-cell counter end",
                ))?;
        let conductive_end = processed_end
            .checked_add(4)
            .ok_or(CrpError::ArithmeticOverflow(
                "WaterManager conductive-cell counter end",
            ))?;
        let flag_end = conductive_end
            .checked_add(1)
            .ok_or(CrpError::ArithmeticOverflow(
                "WaterManager continuation flag end",
            ))?;
        let closing_start = bytes.len().checked_sub(END_PROFILER_REFERENCE_LEN);
        if closing_start.is_none_or(|closing_start| flag_end > closing_start) {
            return Err(CrpError::InvalidAssetPayload {
                name: "WaterManager+Data".to_owned(),
                offset: self.scalar_fields_offset,
                reason: "scalar fields do not end before the closing profiler reference".to_owned(),
            });
        }
        bytes
            .get_mut(self.scalar_fields_offset..processed_end)
            .ok_or_else(|| CrpError::InvalidAssetPayload {
                name: "WaterManager+Data".to_owned(),
                offset: self.scalar_fields_offset,
                reason: "processed-cell counter range is invalid".to_owned(),
            })?
            .copy_from_slice(&self.processed_cell_count.to_be_bytes());
        bytes
            .get_mut(processed_end..conductive_end)
            .ok_or_else(|| CrpError::InvalidAssetPayload {
                name: "WaterManager+Data".to_owned(),
                offset: processed_end,
                reason: "conductive-cell counter range is invalid".to_owned(),
            })?
            .copy_from_slice(&self.conductive_cell_count.to_be_bytes());
        if self.can_continue != self.original_can_continue {
            bytes
                .get_mut(conductive_end..flag_end)
                .ok_or_else(|| CrpError::InvalidAssetPayload {
                    name: "WaterManager+Data".to_owned(),
                    offset: conductive_end,
                    reason: "continuation flag range is invalid".to_owned(),
                })?
                .copy_from_slice(&[u8::from(self.can_continue)]);
        }
        let stored_node_count = stored_node_count(self.format_version);
        let mut replacements = Vec::new();
        if let Some(encoded) = changed_byte_array(
            &self.conductivity,
            Some(&self.conductivity_snapshot.values),
            CELL_COUNT,
            self.conductivity_snapshot.range.0,
            "water conductivity",
        )? {
            replacements.push((
                self.conductivity_snapshot.range.0,
                self.conductivity_snapshot.range.1,
                encoded,
            ));
        }
        match (
            self.conductivity_heating.as_deref(),
            self.conductivity_heating_snapshot.as_ref(),
        ) {
            (Some(values), Some(snapshot)) => {
                if let Some(encoded) = changed_byte_array(
                    values,
                    Some(&snapshot.values),
                    CELL_COUNT,
                    snapshot.range.0,
                    "heating conductivity",
                )? {
                    replacements.push((snapshot.range.0, snapshot.range.1, encoded));
                }
            }
            (None, None) => {}
            _ => {
                return Err(invalid_cell_grid(
                    self.scalar_fields_offset,
                    "heating conductivity fields do not match the version gate",
                ));
            }
        }
        for (values, snapshot, field) in [
            (
                &self.water_pressure,
                &self.water_pressure_snapshot,
                "water pressure",
            ),
            (
                &self.sewage_pressure,
                &self.sewage_pressure_snapshot,
                "sewage pressure",
            ),
        ] {
            if let Some(encoded) = changed_signed_ushort_array(
                values,
                Some(&snapshot.values),
                self.conductive_cells.len(),
                snapshot.range.0,
                field,
            )? {
                replacements.push((snapshot.range.0, snapshot.range.1, encoded));
            }
        }
        match (
            self.heating_pressure.as_deref(),
            self.heating_pressure_snapshot.as_ref(),
        ) {
            (Some(values), Some(snapshot)) => {
                if let Some(encoded) = changed_signed_ushort_array(
                    values,
                    Some(&snapshot.values),
                    self.heating_cells.len(),
                    snapshot.range.0,
                    "heating pressure",
                )? {
                    replacements.push((snapshot.range.0, snapshot.range.1, encoded));
                }
            }
            (None, None) => {}
            _ => {
                return Err(invalid_cell_grid(
                    self.scalar_fields_offset,
                    "heating pressure fields do not match the version gate",
                ));
            }
        }
        for (values, snapshot, field) in [
            (
                &self.water_pulse_group_indexes,
                &self.water_pulse_group_indexes_snapshot,
                "water sparse pulse-group indexes",
            ),
            (
                &self.sewage_pulse_group_indexes,
                &self.sewage_pulse_group_indexes_snapshot,
                "sewage sparse pulse-group indexes",
            ),
        ] {
            if let Some(encoded) = changed_ushort_array(
                values,
                Some(&snapshot.values),
                self.conductive_cells.len(),
                snapshot.range.0,
                field,
            )? {
                replacements.push((snapshot.range.0, snapshot.range.1, encoded));
            }
        }
        match (
            self.heating_pulse_group_indexes.as_deref(),
            self.heating_pulse_group_indexes_snapshot.as_ref(),
        ) {
            (Some(values), Some(snapshot)) => {
                if let Some(encoded) = changed_ushort_array(
                    values,
                    Some(&snapshot.values),
                    self.heating_cells.len(),
                    snapshot.range.0,
                    "heating sparse pulse-group indexes",
                )? {
                    replacements.push((snapshot.range.0, snapshot.range.1, encoded));
                }
            }
            (None, None) => {}
            _ => {
                return Err(invalid_cell_grid(
                    self.scalar_fields_offset,
                    "heating sparse pulse-group indexes do not match the version gate",
                ));
            }
        }
        let (start, end) = self.water_node_pulse_groups_range;
        if let Some(encoded) = changed_ushort_array(
            &self.water_node_pulse_groups,
            Some(&self.original_water_node_pulse_groups),
            stored_node_count,
            start,
            "water node pulse-group indexes",
        )? {
            replacements.push((start, end, encoded));
        }
        let (start, end) = self.sewage_node_pulse_groups_range;
        if let Some(encoded) = changed_ushort_array(
            &self.sewage_node_pulse_groups,
            Some(&self.original_sewage_node_pulse_groups),
            stored_node_count,
            start,
            "sewage node pulse-group indexes",
        )? {
            replacements.push((start, end, encoded));
        }
        let (start, end) = self.water_node_pressure_range;
        if let Some(encoded) = changed_ushort_array(
            &self.water_node_pressure,
            Some(&self.original_water_node_pressure),
            stored_node_count,
            start,
            "water node pressure",
        )? {
            replacements.push((start, end, encoded));
        }
        let (start, end) = self.water_node_extra_pressure_range;
        if let Some(encoded) = changed_ushort_array(
            &self.water_node_extra_pressure,
            Some(&self.original_water_node_extra_pressure),
            stored_node_count,
            start,
            "extra water node pressure",
        )? {
            replacements.push((start, end, encoded));
        }
        let (start, end) = self.sewage_node_pressure_range;
        if let Some(encoded) = changed_ushort_array(
            &self.sewage_node_pressure,
            Some(&self.original_sewage_node_pressure),
            stored_node_count,
            start,
            "sewage node pressure",
        )? {
            replacements.push((start, end, encoded));
        }
        let (start, end) = self.sewage_node_extra_pressure_range;
        if let Some(encoded) = changed_ushort_array(
            &self.sewage_node_extra_pressure,
            Some(&self.original_sewage_node_extra_pressure),
            stored_node_count,
            start,
            "extra sewage node pressure",
        )? {
            replacements.push((start, end, encoded));
        }
        match (
            self.heating_node_pulse_groups_range,
            self.heating_node_pulse_groups.as_deref(),
            self.original_heating_node_pulse_groups.as_deref(),
        ) {
            (Some((start, end)), Some(values), Some(original)) => {
                if let Some(encoded) = changed_ushort_array(
                    values,
                    Some(original),
                    stored_node_count,
                    start,
                    "heating node pulse-group indexes",
                )? {
                    replacements.push((start, end, encoded));
                }
            }
            (None, None, None) => {}
            _ => {
                return Err(invalid_node_array(
                    self.scalar_fields_offset,
                    "heating node pulse-group fields do not match the version gate",
                ));
            }
        }
        match (
            self.heating_node_pressure_range,
            self.heating_node_pressure.as_deref(),
            self.original_heating_node_pressure.as_deref(),
        ) {
            (Some((start, end)), Some(values), Some(original)) => {
                if let Some(encoded) = changed_ushort_array(
                    values,
                    Some(original),
                    stored_node_count,
                    start,
                    "heating node pressure",
                )? {
                    replacements.push((start, end, encoded));
                }
            }
            (None, None, None) => {}
            _ => {
                return Err(invalid_node_array(
                    self.scalar_fields_offset,
                    "heating node-pressure fields do not match the version gate",
                ));
            }
        }
        match (
            self.heating_node_extra_pressure_range,
            self.heating_node_extra_pressure.as_deref(),
            self.original_heating_node_extra_pressure.as_deref(),
        ) {
            (Some((start, end)), Some(values), Some(original)) => {
                if let Some(encoded) = changed_ushort_array(
                    values,
                    Some(original),
                    stored_node_count,
                    start,
                    "extra heating node pressure",
                )? {
                    replacements.push((start, end, encoded));
                }
            }
            (None, None, None) => {}
            _ => {
                return Err(invalid_node_array(
                    self.scalar_fields_offset,
                    "heating extra node-pressure fields do not match the version gate",
                ));
            }
        }
        match (
            self.node_pollution_range,
            self.node_pollution.as_deref(),
            self.original_node_pollution.as_deref(),
        ) {
            (Some((start, end)), Some(values), Some(original)) => {
                if let Some(encoded) = changed_byte_array(
                    values,
                    Some(original),
                    stored_node_count,
                    start,
                    "node pollution",
                )? {
                    replacements.push((start, end, encoded));
                }
            }
            (None, None, None) => {}
            _ => {
                return Err(invalid_node_array(
                    self.scalar_fields_offset,
                    "node pollution fields do not match the version gate",
                ));
            }
        }
        match (
            self.closest_pipe_segments_range,
            self.closest_pipe_segments.as_deref(),
            self.original_closest_pipe_segments.as_deref(),
        ) {
            (Some((start, end)), Some(values), Some(original)) => {
                if let Some(encoded) = changed_ushort_array(
                    values,
                    Some(original),
                    CELL_COUNT,
                    start,
                    "closest-pipe segment grid",
                )? {
                    replacements.push((start, end, encoded));
                }
            }
            (None, None, None) => {}
            _ => {
                return Err(invalid_cell_grid(
                    self.scalar_fields_offset,
                    "closest-pipe segment fields do not match the version gate",
                ));
            }
        }
        match (
            self.closest_heating_segments_range,
            self.closest_heating_segments.as_deref(),
            self.original_closest_heating_segments.as_deref(),
        ) {
            (Some((start, end)), Some(values), Some(original)) => {
                if let Some(encoded) = changed_ushort_array(
                    values,
                    Some(original),
                    CELL_COUNT,
                    start,
                    "closest-heating segment grid",
                )? {
                    replacements.push((start, end, encoded));
                }
            }
            (None, None, None) => {}
            _ => {
                return Err(invalid_cell_grid(
                    self.scalar_fields_offset,
                    "closest-heating segment fields do not match the version gate",
                ));
            }
        }
        for (values, snapshot, field) in [
            (
                &self.water_electrified,
                &self.water_electrified_snapshot,
                "water electrification",
            ),
            (
                &self.water_temporary_electrified,
                &self.water_temporary_electrified_snapshot,
                "temporary water electrification",
            ),
            (
                &self.sewage_electrified,
                &self.sewage_electrified_snapshot,
                "sewage electrification",
            ),
            (
                &self.sewage_temporary_electrified,
                &self.sewage_temporary_electrified_snapshot,
                "temporary sewage electrification",
            ),
        ] {
            if let Some(encoded) = changed_bool_array(
                values,
                Some(&snapshot.values),
                self.conductive_cells.len(),
                snapshot.range.0,
                field,
            )? {
                replacements.push((snapshot.range.0, snapshot.range.1, encoded));
            }
        }
        match (
            self.heating_electrified.as_deref(),
            self.heating_electrified_snapshot.as_ref(),
        ) {
            (Some(values), Some(snapshot)) => {
                if let Some(encoded) = changed_bool_array(
                    values,
                    Some(&snapshot.values),
                    self.heating_cells.len(),
                    snapshot.range.0,
                    "heating electrification",
                )? {
                    replacements.push((snapshot.range.0, snapshot.range.1, encoded));
                }
            }
            (None, None) => {}
            _ => {
                return Err(invalid_cell_grid(
                    self.scalar_fields_offset,
                    "heating electrification fields do not match the version gate",
                ));
            }
        }
        match (
            self.heating_temporary_electrified.as_deref(),
            self.heating_temporary_electrified_snapshot.as_ref(),
        ) {
            (Some(values), Some(snapshot)) => {
                if let Some(encoded) = changed_bool_array(
                    values,
                    Some(&snapshot.values),
                    self.heating_cells.len(),
                    snapshot.range.0,
                    "temporary heating electrification",
                )? {
                    replacements.push((snapshot.range.0, snapshot.range.1, encoded));
                }
            }
            (None, None) => {}
            _ => {
                return Err(invalid_cell_grid(
                    self.scalar_fields_offset,
                    "temporary heating electrification fields do not match the version gate",
                ));
            }
        }
        match (
            self.conductive_pollution.as_deref(),
            self.conductive_pollution_snapshot.as_ref(),
        ) {
            (Some(values), Some(snapshot)) => {
                if let Some(encoded) = changed_byte_array(
                    values,
                    Some(&snapshot.values),
                    self.conductive_cells.len(),
                    snapshot.range.0,
                    "conductive pollution",
                )? {
                    replacements.push((snapshot.range.0, snapshot.range.1, encoded));
                }
            }
            (None, None) => {}
            _ => {
                return Err(invalid_cell_grid(
                    self.scalar_fields_offset,
                    "conductive pollution fields do not match the version gate",
                ));
            }
        }
        match (
            self.collect_water_pressure_selector_range,
            self.collect_water_pressure_selector.as_deref(),
            self.original_collect_water_pressure_selector.as_deref(),
        ) {
            (Some((start, end)), Some(values), Some(original)) => {
                if let Some(encoded) = changed_bool_array(
                    values,
                    Some(original),
                    stored_node_count,
                    start,
                    "collect-water Bool selectors",
                )? {
                    replacements.push((start, end, encoded));
                }
            }
            (None, None, None) => {}
            _ => {
                return Err(invalid_collect_pressure(
                    self.scalar_fields_offset,
                    "collect-water Bool fields do not match the version gate",
                ));
            }
        }
        if let Some((start, end)) = self.collect_water_pressure_range {
            if let Some(encoded) = changed_ushort_array(
                &self.collect_water_pressure,
                self.original_collect_water_pressure.as_deref(),
                stored_node_count,
                start,
                "collect-water pressure",
            )? {
                replacements.push((start, end, encoded));
            }
        }
        if let Some((start, end)) = self.collect_sewage_pressure_range {
            if let Some(encoded) = changed_ushort_array(
                &self.collect_sewage_pressure,
                self.original_collect_sewage_pressure.as_deref(),
                stored_node_count,
                start,
                "collect-sewage pressure",
            )? {
                replacements.push((start, end, encoded));
            }
        }
        replacements.sort_by_key(|(start, _, _)| *start);
        for (start, end, replacement) in replacements.into_iter().rev() {
            if start > end || end > bytes.len() {
                return Err(invalid_node_array(
                    start,
                    "node-array replacement range is invalid",
                ));
            }
            bytes.splice(start..end, replacement);
        }
        Ok(bytes)
    }
}

fn invalid_pulse_group(offset: usize, reason: &str) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "WaterManager+Data".to_owned(),
        offset,
        reason: reason.to_owned(),
    }
}

fn invalid_pulse_unit(offset: usize, reason: &str) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "WaterManager+Data".to_owned(),
        offset,
        reason: reason.to_owned(),
    }
}

fn invalid_collect_pressure(offset: usize, reason: &str) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "WaterManager+Data".to_owned(),
        offset,
        reason: reason.to_owned(),
    }
}

fn invalid_node_array(offset: usize, reason: &str) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "WaterManager+Data".to_owned(),
        offset,
        reason: reason.to_owned(),
    }
}

fn invalid_cell_grid(offset: usize, reason: &str) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "WaterManager+Data".to_owned(),
        offset,
        reason: reason.to_owned(),
    }
}

fn set_node_ushort_value(
    values: &mut [u16],
    format_version: u32,
    node: usize,
    value: u16,
    kind: &str,
) -> Result<(), CrpError> {
    if node >= stored_node_count(format_version) {
        return Err(invalid_node_array(
            node,
            &format!("{kind} node index is outside the stored prefix"),
        ));
    }
    let Some(target) = values.get_mut(node) else {
        return Err(invalid_node_array(
            node,
            &format!("{kind} node index is outside the decoded grid"),
        ));
    };
    *target = value;
    Ok(())
}

fn set_node_byte_value(
    values: &mut [u8],
    format_version: u32,
    node: usize,
    value: u8,
    kind: &str,
) -> Result<(), CrpError> {
    if node >= stored_node_count(format_version) {
        return Err(invalid_node_array(
            node,
            &format!("{kind} node index is outside the stored prefix"),
        ));
    }
    let Some(target) = values.get_mut(node) else {
        return Err(invalid_node_array(
            node,
            &format!("{kind} node index is outside the decoded grid"),
        ));
    };
    *target = value;
    Ok(())
}

fn set_grid_ushort_value(
    values: &mut [u16],
    cell: usize,
    value: u16,
    field: &str,
) -> Result<(), CrpError> {
    if cell >= CELL_COUNT {
        return Err(invalid_cell_grid(
            cell,
            &format!("{field} cell index is outside the grid"),
        ));
    }
    let Some(target) = values.get_mut(cell) else {
        return Err(invalid_cell_grid(
            cell,
            &format!("{field} decoded grid is truncated"),
        ));
    };
    *target = value;
    Ok(())
}

fn set_conductivity_value(
    values: &mut [u8],
    cell: usize,
    conductivity: u8,
    field: &str,
) -> Result<(), CrpError> {
    let Some(target) = values.get_mut(cell) else {
        return Err(invalid_cell_grid(
            cell,
            &format!("{field} cell index is outside the grid"),
        ));
    };
    if (*target == 0) != (conductivity == 0) {
        return Err(invalid_cell_grid(
            cell,
            &format!(
                "{field} edits must preserve whether the cell is active to retain sparse field alignment"
            ),
        ));
    }
    *target = conductivity;
    Ok(())
}

fn sparse_cell_position(cells: &[usize], cell: usize, field: &str) -> Result<usize, CrpError> {
    cells.binary_search(&cell).map_err(|_| {
        invalid_cell_grid(
            cell,
            &format!("{field} cell is absent from its conductivity grid"),
        )
    })
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
        Err(invalid_cell_grid(
            cell,
            &format!("{field} length does not match its conductivity grid"),
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
        invalid_cell_grid(
            cell,
            &format!("{field} values are shorter than the conductivity grid"),
        )
    })
}

fn set_sparse_bool_cell(
    values: &mut [bool],
    cells: &[usize],
    cell: usize,
    value: bool,
    field: &str,
) -> Result<(), CrpError> {
    let index = sparse_cell_position(cells, cell, field)?;
    let Some(target) = values.get_mut(index) else {
        return Err(invalid_cell_grid(
            cell,
            &format!("{field} values are shorter than the conductivity grid"),
        ));
    };
    *target = value;
    Ok(())
}

fn set_sparse_byte_cell(
    values: &mut [u8],
    cells: &[usize],
    cell: usize,
    value: u8,
    field: &str,
) -> Result<(), CrpError> {
    let index = sparse_cell_position(cells, cell, field)?;
    let Some(target) = values.get_mut(index) else {
        return Err(invalid_cell_grid(
            cell,
            &format!("{field} values are shorter than the conductivity grid"),
        ));
    };
    *target = value;
    Ok(())
}

fn set_sparse_i16_cell(
    values: &mut [i16],
    cells: &[usize],
    cell: usize,
    value: i16,
    field: &str,
) -> Result<(), CrpError> {
    let index = sparse_cell_position(cells, cell, field)?;
    let Some(target) = values.get_mut(index) else {
        return Err(invalid_cell_grid(
            cell,
            &format!("{field} values are shorter than the conductivity grid"),
        ));
    };
    *target = value;
    Ok(())
}

fn set_sparse_ushort_cell(
    values: &mut [u16],
    cells: &[usize],
    cell: usize,
    value: u16,
    field: &str,
) -> Result<(), CrpError> {
    let index = sparse_cell_position(cells, cell, field)?;
    let Some(target) = values.get_mut(index) else {
        return Err(invalid_cell_grid(
            cell,
            &format!("{field} values are shorter than the conductivity grid"),
        ));
    };
    *target = value;
    Ok(())
}

fn stored_node_count(format_version: u32) -> usize {
    if format_version >= FULL_NODE_COUNT_VERSION {
        NODE_COUNT
    } else {
        LEGACY_NODE_COUNT
    }
}

fn changed_ushort_array(
    values: &[u16],
    original_values: Option<&[u16]>,
    count: usize,
    offset: usize,
    field: &str,
) -> Result<Option<Vec<u8>>, CrpError> {
    let current = values.get(..count).ok_or_else(|| {
        invalid_node_array(offset, &format!("{field} decoded prefix is truncated"))
    })?;
    let original = original_values.ok_or_else(|| {
        invalid_node_array(offset, &format!("{field} original values are missing"))
    })?;
    // A changed conductivity topology can legitimately change a sparse
    // column's current length from the captured source length.
    if current == original {
        return Ok(None);
    }
    let mut encoded = Vec::new();
    write_encoded_ushort_array(&mut encoded, current);
    Ok(Some(encoded))
}

fn changed_signed_ushort_array(
    values: &[i16],
    original_values: Option<&[i16]>,
    count: usize,
    offset: usize,
    field: &str,
) -> Result<Option<Vec<u8>>, CrpError> {
    let current = values.get(..count).ok_or_else(|| {
        invalid_cell_grid(offset, &format!("{field} decoded prefix is truncated"))
    })?;
    let original = original_values.ok_or_else(|| {
        invalid_cell_grid(offset, &format!("{field} original values are missing"))
    })?;
    if current == original {
        return Ok(None);
    }
    let unsigned_values: Vec<u16> = current.iter().map(|value| *value as u16).collect();
    let mut encoded = Vec::new();
    write_encoded_ushort_array(&mut encoded, &unsigned_values);
    Ok(Some(encoded))
}

fn changed_byte_array(
    values: &[u8],
    original_values: Option<&[u8]>,
    count: usize,
    offset: usize,
    field: &str,
) -> Result<Option<Vec<u8>>, CrpError> {
    let current = values.get(..count).ok_or_else(|| {
        invalid_node_array(offset, &format!("{field} decoded prefix is truncated"))
    })?;
    let original = original_values.ok_or_else(|| {
        invalid_node_array(offset, &format!("{field} original values are missing"))
    })?;
    if current == original {
        return Ok(None);
    }
    let mut encoded = Vec::new();
    write_encoded_byte_array(&mut encoded, current);
    Ok(Some(encoded))
}

fn changed_bool_array(
    values: &[bool],
    original_values: Option<&[bool]>,
    count: usize,
    offset: usize,
    field: &str,
) -> Result<Option<Vec<u8>>, CrpError> {
    let current = values.get(..count).ok_or_else(|| {
        invalid_collect_pressure(offset, &format!("{field} decoded prefix is truncated"))
    })?;
    let original = original_values.ok_or_else(|| {
        invalid_collect_pressure(offset, &format!("{field} original values are missing"))
    })?;
    if current == original {
        return Ok(None);
    }
    let mut encoded = Vec::new();
    write_encoded_bool_array(&mut encoded, current);
    Ok(Some(encoded))
}

fn encode_pulse_units(
    bytes: &mut [u8],
    records_offset: usize,
    units: &[WaterPulseUnit],
    kind: &str,
) -> Result<(), CrpError> {
    for (index, unit) in units.iter().enumerate() {
        let record_offset = index
            .checked_mul(PULSE_UNIT_RECORD_LEN)
            .and_then(|delta| records_offset.checked_add(delta))
            .ok_or(CrpError::ArithmeticOverflow(
                "WaterManager pulse-unit record offset",
            ))?;
        let record_end = record_offset.checked_add(PULSE_UNIT_RECORD_LEN).ok_or(
            CrpError::ArithmeticOverflow("WaterManager pulse-unit record end"),
        )?;
        let mut encoded = Vec::with_capacity(PULSE_UNIT_RECORD_LEN);
        encoded.extend_from_slice(&unit.group.to_be_bytes());
        encoded.extend_from_slice(&unit.node.to_be_bytes());
        encoded.push(unit.x);
        encoded.push(unit.z);
        bytes
            .get_mut(record_offset..record_end)
            .ok_or_else(|| {
                invalid_pulse_unit(
                    record_offset,
                    &format!("{kind} pulse-unit record range is invalid"),
                )
            })?
            .copy_from_slice(&encoded);
    }
    Ok(())
}

fn encode_pulse_groups(
    bytes: &mut [u8],
    records_offset: usize,
    groups: &[WaterPulseGroup],
    has_collect_pressure: bool,
    kind: &str,
) -> Result<(), CrpError> {
    let record_len = PULSE_GROUP_RECORD_LEN
        .checked_add(if has_collect_pressure {
            COLLECT_PRESSURE_FIELD_LEN
        } else {
            0
        })
        .ok_or(CrpError::ArithmeticOverflow(
            "WaterManager pulse-group record length",
        ))?;
    for (index, group) in groups.iter().enumerate() {
        if group.collect_pressure.is_some() != has_collect_pressure {
            return Err(invalid_pulse_group(
                records_offset,
                "pulse-group collect pressure does not match the version gate",
            ));
        }
        let record_offset = index
            .checked_mul(record_len)
            .and_then(|delta| records_offset.checked_add(delta))
            .ok_or(CrpError::ArithmeticOverflow(
                "WaterManager pulse-group record offset",
            ))?;
        let record_end =
            record_offset
                .checked_add(record_len)
                .ok_or(CrpError::ArithmeticOverflow(
                    "WaterManager pulse-group record end",
                ))?;
        let mut encoded = Vec::with_capacity(record_len);
        encoded.extend_from_slice(&group.original_pressure.to_be_bytes());
        encoded.extend_from_slice(&group.current_pressure.to_be_bytes());
        if let Some(collect_pressure) = group.collect_pressure {
            encoded.extend_from_slice(&collect_pressure.to_be_bytes());
        }
        encoded.extend_from_slice(&group.merge_index.to_be_bytes());
        encoded.extend_from_slice(&group.merge_count.to_be_bytes());
        encoded.extend_from_slice(&group.node.to_be_bytes());
        if encoded.len() != record_len {
            return Err(invalid_pulse_group(
                record_offset,
                "pulse-group values do not match the serialized record width",
            ));
        }
        bytes
            .get_mut(record_offset..record_end)
            .ok_or_else(|| {
                invalid_pulse_group(
                    record_offset,
                    &format!("{kind} pulse-group record range is invalid"),
                )
            })?
            .copy_from_slice(&encoded);
    }
    Ok(())
}

fn indexes_with_nonzero_values(values: &[u8]) -> Vec<usize> {
    values
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (*value != 0).then_some(index))
        .collect()
}

fn read_signed_ushort_array(reader: &mut Reader<'_>, count: usize) -> Result<Vec<i16>, CrpError> {
    Ok(read_encoded_ushort_array(reader, count)?
        .into_iter()
        .map(|value| value as i16)
        .collect())
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
            range: (start, end),
            values,
        },
    ))
}

fn read_captured_signed_ushort_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<(Vec<i16>, EncodedArraySnapshot<i16>), CrpError> {
    let start = reader.offset();
    let values = read_signed_ushort_array(reader, count)?;
    let end = reader.offset();
    Ok((
        values.clone(),
        EncodedArraySnapshot {
            range: (start, end),
            values,
        },
    ))
}

fn read_node_ushort_array(
    reader: &mut Reader<'_>,
    stored_count: usize,
    default: u16,
) -> Result<Vec<u16>, CrpError> {
    let mut values = vec![default; NODE_COUNT];
    values[..stored_count].copy_from_slice(&read_encoded_ushort_array(reader, stored_count)?);
    Ok(values)
}

fn read_node_byte_array(
    reader: &mut Reader<'_>,
    stored_count: usize,
    default: u8,
) -> Result<Vec<u8>, CrpError> {
    let mut values = vec![default; NODE_COUNT];
    values[..stored_count].copy_from_slice(&read_encoded_byte_array(reader, stored_count)?);
    Ok(values)
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
            range: (start, end),
            values,
        },
    ))
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
            range: (start, end),
            values,
        },
    ))
}

fn read_pulse_groups(
    reader: &mut Reader<'_>,
    format_version: u32,
    heating: bool,
) -> Result<(Vec<WaterPulseGroup>, usize), CrpError> {
    let count = usize::from(reader.read_u16_be()?);
    let records_offset = reader.offset();
    if count > PULSE_GROUP_CAPACITY {
        return Err(reader.invalid(format!(
            "WaterManager pulse-group count {count} exceeds capacity {PULSE_GROUP_CAPACITY}"
        )));
    }
    let has_collect_pressure = !heating && format_version >= COLLECT_WATER_BOOL_VERSION;
    let mut groups = Vec::with_capacity(count);
    for _ in 0..count {
        groups.push(WaterPulseGroup {
            original_pressure: reader.read_u32_be()?,
            current_pressure: reader.read_u32_be()?,
            collect_pressure: if has_collect_pressure {
                Some(reader.read_u32_be()?)
            } else {
                None
            },
            merge_index: reader.read_u16_be()?,
            merge_count: reader.read_u16_be()?,
            node: reader.read_u16_be()?,
        });
    }
    Ok((groups, records_offset))
}

fn read_pulse_units(reader: &mut Reader<'_>) -> Result<(Vec<WaterPulseUnit>, usize), CrpError> {
    let count = usize::from(reader.read_u16_be()?);
    let records_offset = reader.offset();
    if count > PULSE_UNIT_CAPACITY {
        return Err(reader.invalid(format!(
            "WaterManager pulse-unit count {count} exceeds capacity {PULSE_UNIT_CAPACITY}"
        )));
    }
    let mut units = Vec::with_capacity(count);
    for _ in 0..count {
        units.push(WaterPulseUnit {
            group: reader.read_u16_be()?,
            node: reader.read_u16_be()?,
            x: reader.read_u8()?,
            z: reader.read_u8()?,
        });
    }
    Ok((units, records_offset))
}

fn read_guide_references(
    reader: &mut Reader<'_>,
    format_version: u32,
) -> Result<Vec<NetObjectHeader>, CrpError> {
    if format_version < GUIDE_REFERENCES_VERSION {
        return Ok(Vec::new());
    }
    let header_and_end_len = reader.remaining();
    let header_len = header_and_end_len
        .checked_sub(END_PROFILER_REFERENCE_LEN)
        .ok_or_else(|| reader.invalid("WaterManager guide references are truncated"))?;
    let header_bytes = reader.read_bytes(header_len)?;
    let mut header_reader = Reader::new(header_bytes);
    let references = [
        read_net_object_header(&mut header_reader)?,
        read_net_object_header(&mut header_reader)?,
    ];
    if header_reader.remaining() != 0 {
        return Err(header_reader.invalid(
            "WaterManager guide headers have trailing bytes before the closing profiler reference",
        ));
    }
    Ok(references.into_iter().collect())
}

fn read_end_profiler_reference(reader: &mut Reader<'_>) -> Result<(), CrpError> {
    if reader.remaining() != END_PROFILER_REFERENCE_LEN {
        return Err(reader.invalid(format!(
            "WaterManager fields leave {} bytes before the next manager; expected a two-byte closing profiler reference",
            reader.remaining()
        )));
    }
    let first = reader.read_u8()?;
    if first >= 0xfe {
        return Err(reader.invalid("WaterManager closing profiler reference is malformed"));
    }
    reader.read_u8()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CELL_COUNT, COLLECT_PRESSURE_FIELD_LEN, FULL_NODE_COUNT_VERSION, HEATING_VERSION,
        HeatingConductiveCellData, LEGACY_NODE_COUNT, NODE_COUNT, POLLUTION_VERSION,
        PULSE_GROUP_CAPACITY, PULSE_GROUP_RECORD_LEN, PULSE_UNIT_CAPACITY, PULSE_UNIT_RECORD_LEN,
        WaterConductiveCellData, WaterManagerData, WaterPulseGroup, WaterPulseUnit,
    };

    fn append_repeat_zeroes(output: &mut Vec<u8>, mut count: usize) {
        while count > 0 {
            let run = count.min(128);
            output.push(0x7f + run as u8);
            count -= run;
        }
    }

    fn append_zero_byte_array(output: &mut Vec<u8>, count: usize) {
        append_repeat_zeroes(output, count);
    }

    fn append_zero_ushort_array(output: &mut Vec<u8>, count: usize) {
        append_repeat_zeroes(output, count);
    }

    fn append_byte_array_with_one(output: &mut Vec<u8>, count: usize, index: usize, value: u8) {
        assert!(index < count);
        append_repeat_zeroes(output, index);
        output.extend_from_slice(&[0, value]);
        let trailing = count - index - 1;
        if trailing > 0 {
            output.extend_from_slice(&[0, 0]);
            let trailing = trailing - 1;
            if trailing > 0 {
                output.extend_from_slice(&[0, 0]);
                append_repeat_zeroes(output, trailing - 1);
            }
        }
    }

    fn append_ushort_array_with_first(output: &mut Vec<u8>, count: usize, value: u16) {
        if count == 0 {
            return;
        }
        output.push(0);
        output.extend_from_slice(&value.to_be_bytes());
        let trailing = count - 1;
        if trailing > 0 {
            output.extend_from_slice(&[0, 0, 0]);
            let trailing = trailing - 1;
            if trailing > 0 {
                output.extend_from_slice(&[0, 0, 0]);
                append_repeat_zeroes(output, trailing - 1);
            }
        }
    }

    fn append_zero_bool_array(output: &mut Vec<u8>, count: usize) {
        append_zero_byte_array(output, count.div_ceil(8));
    }

    fn empty_body(format_version: u32) -> Vec<u8> {
        let stored_node_count = if format_version >= FULL_NODE_COUNT_VERSION {
            NODE_COUNT
        } else {
            LEGACY_NODE_COUNT
        };
        let mut body = Vec::new();
        append_zero_byte_array(&mut body, CELL_COUNT);
        if format_version >= HEATING_VERSION {
            append_zero_byte_array(&mut body, CELL_COUNT);
        }
        if format_version >= 73 {
            append_zero_ushort_array(&mut body, CELL_COUNT);
        }
        if format_version >= HEATING_VERSION {
            append_zero_ushort_array(&mut body, CELL_COUNT);
        }
        for _ in 0..if format_version >= HEATING_VERSION {
            6
        } else {
            4
        } {
            append_zero_bool_array(&mut body, 0);
        }
        for _ in 0..if format_version >= HEATING_VERSION {
            3
        } else {
            2
        } {
            body.extend_from_slice(&0_u16.to_be_bytes()); // No pulse groups or units.
            body.extend_from_slice(&0_u16.to_be_bytes());
        }
        let node_array_count = if format_version >= HEATING_VERSION {
            9
        } else {
            6
        };
        for _ in 0..node_array_count {
            append_zero_ushort_array(&mut body, stored_node_count);
        }
        if format_version >= 23 {
            append_zero_byte_array(&mut body, stored_node_count);
        }
        if format_version == 270 {
            append_zero_bool_array(&mut body, stored_node_count);
        } else if format_version >= 271 {
            append_zero_ushort_array(&mut body, stored_node_count);
        }
        if format_version >= 272 {
            append_zero_ushort_array(&mut body, stored_node_count);
        }
        body.extend_from_slice(&[0; 8]); // Processed and conductive-cell counters.
        body.push(1); // canContinue.
        if format_version >= 94 {
            body.extend_from_slice(&[0xff, 0xff]); // Two null guide references.
        }
        body.extend_from_slice(&[0x12, 0x34]); // Closing profiler reference.
        body
    }

    fn active_body(format_version: u32) -> Vec<u8> {
        assert!(format_version >= HEATING_VERSION);
        let stored_node_count = if format_version >= FULL_NODE_COUNT_VERSION {
            NODE_COUNT
        } else {
            LEGACY_NODE_COUNT
        };
        let mut body = Vec::new();
        append_byte_array_with_one(&mut body, CELL_COUNT, 2, 1);
        append_byte_array_with_one(&mut body, CELL_COUNT, 4, 1);
        for value in [0xfffe_u16, 3, 4, 5, 6, 7] {
            body.extend_from_slice(&[0]);
            body.extend_from_slice(&value.to_be_bytes());
        }
        append_zero_ushort_array(&mut body, CELL_COUNT);
        append_zero_ushort_array(&mut body, CELL_COUNT);
        for value in [true, false, false, true, true, false] {
            body.extend_from_slice(&[0, u8::from(value)]);
        }
        body.extend_from_slice(&[0, 9]); // Sparse conductive-cell pollution.

        body.extend_from_slice(&1_u16.to_be_bytes()); // Water pulse group.
        body.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
        body.extend_from_slice(&0x1112_1314_u32.to_be_bytes());
        if format_version >= 270 {
            body.extend_from_slice(&0x1516_1718_u32.to_be_bytes());
        }
        body.extend_from_slice(&15_u16.to_be_bytes());
        body.extend_from_slice(&16_u16.to_be_bytes());
        body.extend_from_slice(&17_u16.to_be_bytes());
        body.extend_from_slice(&1_u16.to_be_bytes()); // Water pulse unit.
        body.extend_from_slice(&18_u16.to_be_bytes());
        body.extend_from_slice(&19_u16.to_be_bytes());
        body.extend_from_slice(&[20, 21]);
        body.extend_from_slice(&1_u16.to_be_bytes()); // Sewage pulse group.
        body.extend_from_slice(&0x2122_2324_u32.to_be_bytes());
        body.extend_from_slice(&0x3132_3334_u32.to_be_bytes());
        if format_version >= 270 {
            body.extend_from_slice(&0x4142_4344_u32.to_be_bytes());
        }
        body.extend_from_slice(&0x5152_u16.to_be_bytes());
        body.extend_from_slice(&0x5354_u16.to_be_bytes());
        body.extend_from_slice(&0x5556_u16.to_be_bytes());
        body.extend_from_slice(&1_u16.to_be_bytes()); // Sewage pulse unit.
        body.extend_from_slice(&0x6162_u16.to_be_bytes());
        body.extend_from_slice(&0x7172_u16.to_be_bytes());
        body.extend_from_slice(&[0x81, 0x82]);
        body.extend_from_slice(&1_u16.to_be_bytes()); // Heating pulse group.
        body.extend_from_slice(&0x6162_6364_u32.to_be_bytes());
        body.extend_from_slice(&0x7172_7374_u32.to_be_bytes());
        body.extend_from_slice(&0x8182_u16.to_be_bytes());
        body.extend_from_slice(&0x8384_u16.to_be_bytes());
        body.extend_from_slice(&0x8586_u16.to_be_bytes());
        body.extend_from_slice(&1_u16.to_be_bytes()); // Heating pulse unit.
        body.extend_from_slice(&0x9192_u16.to_be_bytes());
        body.extend_from_slice(&0xa1a2_u16.to_be_bytes());
        body.extend_from_slice(&[0xb1, 0xb2]);

        for value in [0x22, 0x1234, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29] {
            append_ushort_array_with_first(&mut body, stored_node_count, value);
        }
        append_byte_array_with_one(&mut body, stored_node_count, 0, 30);
        if format_version == 270 {
            append_byte_array_with_one(&mut body, stored_node_count.div_ceil(8), 0, 1);
        } else if format_version >= 271 {
            append_ushort_array_with_first(&mut body, stored_node_count, 0x3210);
        }
        if format_version >= 272 {
            append_ushort_array_with_first(&mut body, stored_node_count, 0x4567);
        }
        body.extend_from_slice(&(-31_i32).to_be_bytes());
        body.extend_from_slice(&32_i32.to_be_bytes());
        body.push(1);
        body.extend_from_slice(&[0xff, 0xff]); // Two null guide references.
        body.extend_from_slice(&[0x12, 0x34]);
        body
    }

    fn expected_pulse_group_bytes(group: WaterPulseGroup) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&group.original_pressure.to_be_bytes());
        bytes.extend_from_slice(&group.current_pressure.to_be_bytes());
        if let Some(collect_pressure) = group.collect_pressure {
            bytes.extend_from_slice(&collect_pressure.to_be_bytes());
        }
        bytes.extend_from_slice(&group.merge_index.to_be_bytes());
        bytes.extend_from_slice(&group.merge_count.to_be_bytes());
        bytes.extend_from_slice(&group.node.to_be_bytes());
        bytes
    }

    fn expected_pulse_unit_bytes(unit: WaterPulseUnit) -> [u8; PULSE_UNIT_RECORD_LEN] {
        let group = unit.group.to_be_bytes();
        let node = unit.node.to_be_bytes();
        [group[0], group[1], node[0], node[1], unit.x, unit.z]
    }

    fn expected_literal_byte_array(values: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for chunk in values.chunks(128) {
            bytes.push((chunk.len() - 1) as u8);
            bytes.extend_from_slice(chunk);
        }
        bytes
    }

    fn expected_ushort_literal_array(values: &[u16]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for chunk in values.chunks(64) {
            bytes.push((chunk.len() - 1) as u8);
            for value in chunk {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
        bytes
    }

    fn expected_ushort_array_with_two_nonzero_values(
        count: usize,
        first: u16,
        second: u16,
    ) -> Vec<u8> {
        let mut bytes = vec![3]; // Four literal UShort values.
        bytes.extend_from_slice(&first.to_be_bytes());
        bytes.extend_from_slice(&second.to_be_bytes());
        bytes.extend_from_slice(&[0; 4]);
        let mut remaining = count - 4;
        while remaining > 0 {
            let run = remaining.min(128);
            bytes.push(0x7f + run as u8);
            remaining -= run;
        }
        bytes
    }

    #[test]
    fn empty_network_layout_obeys_heating_node_collect_and_guide_gates() {
        for version in [
            22, 23, 72, 73, 93, 94, 194, 201, 202, 226, 227, 269, 270, 271, 272,
        ] {
            let body = empty_body(version);
            let data = WaterManagerData::decode(version, &body).unwrap();
            assert_eq!(data.conductivity().len(), CELL_COUNT, "version {version}");
            assert!(data.conductive_cells().is_empty());
            assert_eq!(data.closest_pipe_segments().is_some(), version >= 73);
            assert_eq!(data.conductivity_heating().is_some(), version >= 227);
            assert_eq!(data.heating_pulse_groups().is_some(), version >= 227);
            assert_eq!(data.water_node_pressure().len(), NODE_COUNT);
            assert_eq!(data.water_node_pressure()[NODE_COUNT - 1], 0);
            assert_eq!(
                data.water_node_pulse_groups()[NODE_COUNT - 1],
                if version < FULL_NODE_COUNT_VERSION {
                    u16::MAX
                } else {
                    0
                }
            );
            assert_eq!(
                data.guide_references().len(),
                usize::from(version >= 94) * 2
            );
            assert_eq!(data.collect_water_pressure().len(), NODE_COUNT);
            assert_eq!(data.collect_water_pressure()[0], 0);
            assert_eq!(data.collect_sewage_pressure().len(), NODE_COUNT);
            assert_eq!(
                data.node_pollution().is_some(),
                version >= POLLUTION_VERSION
            );
            assert_eq!(
                data.conductive_pollution().is_some(),
                version >= POLLUTION_VERSION
            );
            assert_eq!(data.refresh_grid(), version < 73);
            assert_eq!(data.encode_unchanged(), body);
        }
    }

    #[test]
    fn active_water_and_heating_values_follow_conductive_cell_order() {
        let body = active_body(265);
        let data = WaterManagerData::decode(265, &body).unwrap();
        assert_eq!(data.conductive_cells(), [2]);
        assert_eq!(data.heating_cells(), [4]);
        assert_eq!(data.water_pressure(), [-2]);
        assert_eq!(data.sewage_pressure(), [3]);
        assert_eq!(data.heating_pressure(), Some([4].as_slice()));
        assert_eq!(data.water_pulse_group_indexes(), [5]);
        assert_eq!(data.sewage_pulse_group_indexes(), [6]);
        assert_eq!(data.heating_pulse_group_indexes(), Some([7].as_slice()));
        assert_eq!(data.water_electrified(), [true]);
        assert_eq!(data.water_temporary_electrified(), [false]);
        assert_eq!(data.sewage_electrified(), [false]);
        assert_eq!(data.sewage_temporary_electrified(), [true]);
        assert_eq!(data.heating_electrified(), Some([true].as_slice()));
        assert_eq!(
            data.heating_temporary_electrified(),
            Some([false].as_slice())
        );
        assert_eq!(data.conductive_pollution(), Some([9].as_slice()));
        assert_eq!(data.water_pulse_groups().len(), 1);
        assert_eq!(data.water_pulse_groups()[0].original_pressure, 0x0102_0304);
        assert_eq!(data.water_pulse_groups()[0].merge_index, 15);
        assert_eq!(data.water_pulse_units()[0].node, 19);
        assert_eq!(data.water_pulse_units()[0].z, 21);
        assert_eq!(data.sewage_pulse_units()[0].group, 0x6162);
        assert_eq!(data.sewage_pulse_units()[0].node, 0x7172);
        assert_eq!(data.heating_pulse_units().unwrap()[0].x, 0xb1);
        assert_eq!(data.sewage_pulse_groups()[0].original_pressure, 0x2122_2324);
        assert_eq!(data.sewage_pulse_groups()[0].current_pressure, 0x3132_3334);
        assert_eq!(data.heating_pulse_groups().unwrap()[0].node, 0x8586);
        assert_eq!(data.water_node_pulse_groups()[0], 0x22);
        assert_eq!(data.water_node_pressure()[0], 0x1234);
        assert_eq!(data.sewage_node_extra_pressure()[0], 0x26);
        assert_eq!(data.heating_node_pressure().unwrap()[0], 0x28);
        assert_eq!(data.node_pollution().unwrap()[0], 30);
        assert_eq!(data.processed_cell_count(), -31);
        assert_eq!(data.conductive_cell_count(), 32);
        assert_eq!(data.guide_references().len(), 2);
        assert_eq!(data.encode_unchanged(), body);
    }

    #[test]
    fn conductivity_edits_preserve_sparse_cell_topology() {
        let body = active_body(265);
        let mut data = WaterManagerData::decode(265, &body).unwrap();
        assert_eq!(data.encode().unwrap(), body);
        let mut water_values = vec![0; CELL_COUNT];
        water_values[2] = 0x44;
        let mut heating_values = vec![0; CELL_COUNT];
        heating_values[4] = 0x55;
        let mut expected = body.clone();
        let mut replacements = vec![
            (
                data.conductivity_snapshot.range,
                expected_literal_byte_array(&water_values),
            ),
            (
                data.conductivity_heating_snapshot.as_ref().unwrap().range,
                expected_literal_byte_array(&heating_values),
            ),
        ];
        data.set_conductivity(2, 0x44).unwrap();
        data.set_heating_conductivity(4, 0x55).unwrap();
        replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in replacements.into_iter().rev() {
            expected.splice(start..end, replacement);
        }

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = WaterManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.conductive_cells(), [2]);
        assert_eq!(decoded.heating_cells(), [4]);
        assert_eq!(decoded.conductivity()[2], 0x44);
        assert_eq!(decoded.conductivity_heating().unwrap()[4], 0x55);
        assert!(data.set_conductivity(2, 0).is_err());
        assert!(data.set_conductivity(3, 1).is_err());
        assert!(data.set_conductivity(CELL_COUNT, 0).is_err());
        assert!(data.set_heating_conductivity(4, 0).is_err());
        assert!(data.set_heating_conductivity(5, 1).is_err());

        let mut before_heating = WaterManagerData::decode(226, &empty_body(226)).unwrap();
        assert!(before_heating.set_heating_conductivity(0, 1).is_err());
    }

    #[test]
    fn inserting_and_removing_primary_conductive_cell_keeps_all_sparse_fields_aligned() {
        let body = active_body(265);
        let mut data = WaterManagerData::decode(265, &body).unwrap();
        let state = WaterConductiveCellData {
            conductivity: 0x40,
            water_pressure: -4,
            sewage_pressure: -5,
            water_pulse_group_index: 0x1234,
            sewage_pulse_group_index: 0x2345,
            water_electrified: false,
            water_temporary_electrified: true,
            sewage_electrified: true,
            sewage_temporary_electrified: false,
            pollution: Some(0xa5),
        };
        let mut conductivity = vec![0; CELL_COUNT];
        conductivity[1] = state.conductivity;
        conductivity[2] = 1;
        let mut expected = body.clone();
        let mut replacements = vec![
            (
                data.conductivity_snapshot.range,
                expected_literal_byte_array(&conductivity),
            ),
            (
                data.water_pressure_snapshot.range,
                expected_ushort_literal_array(&[state.water_pressure as u16, (-2_i16) as u16]),
            ),
            (
                data.sewage_pressure_snapshot.range,
                expected_ushort_literal_array(&[state.sewage_pressure as u16, 3]),
            ),
            (
                data.water_pulse_group_indexes_snapshot.range,
                expected_ushort_literal_array(&[state.water_pulse_group_index, 5]),
            ),
            (
                data.sewage_pulse_group_indexes_snapshot.range,
                expected_ushort_literal_array(&[state.sewage_pulse_group_index, 6]),
            ),
            (data.water_electrified_snapshot.range, vec![0, 0b10]),
            (
                data.water_temporary_electrified_snapshot.range,
                vec![0, 0b01],
            ),
            (data.sewage_electrified_snapshot.range, vec![0, 0b01]),
            (
                data.sewage_temporary_electrified_snapshot.range,
                vec![0, 0b10],
            ),
            (
                data.conductive_pollution_snapshot.as_ref().unwrap().range,
                vec![1, 0xa5, 9],
            ),
        ];
        data.insert_conductive_cell(1, state).unwrap();
        assert_eq!(data.conductive_cells(), [1, 2]);
        assert_eq!(data.conductive_cell_count(), 32);
        replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in replacements.into_iter().rev() {
            expected.splice(start..end, replacement);
        }
        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = WaterManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.conductive_cells(), [1, 2]);
        assert_eq!(decoded.conductivity()[1], 0x40);
        assert_eq!(decoded.water_pressure(), [-4, -2]);
        assert_eq!(decoded.sewage_pressure(), [-5, 3]);
        assert_eq!(decoded.water_pulse_group_indexes(), [0x1234, 5]);
        assert_eq!(decoded.sewage_pulse_group_indexes(), [0x2345, 6]);
        assert_eq!(decoded.water_electrified(), [false, true]);
        assert_eq!(decoded.water_temporary_electrified(), [true, false]);
        assert_eq!(decoded.sewage_electrified(), [true, false]);
        assert_eq!(decoded.sewage_temporary_electrified(), [false, true]);
        assert_eq!(decoded.conductive_pollution(), Some([0xa5, 9].as_slice()));

        assert_eq!(data.remove_conductive_cell(1).unwrap(), state);
        assert_eq!(data.conductive_cells(), [2]);
        assert_eq!(data.encode().unwrap(), body);

        let last_cell = WaterConductiveCellData {
            conductivity: 1,
            water_pressure: -2,
            sewage_pressure: 3,
            water_pulse_group_index: 5,
            sewage_pulse_group_index: 6,
            water_electrified: true,
            water_temporary_electrified: false,
            sewage_electrified: false,
            sewage_temporary_electrified: true,
            pollution: Some(9),
        };
        assert_eq!(data.remove_conductive_cell(2).unwrap(), last_cell);
        let mut empty_expected = body.clone();
        let mut empty_replacements = vec![
            (
                data.conductivity_snapshot.range,
                expected_literal_byte_array(&vec![0; CELL_COUNT]),
            ),
            (data.water_pressure_snapshot.range, vec![]),
            (data.sewage_pressure_snapshot.range, vec![]),
            (data.water_pulse_group_indexes_snapshot.range, vec![]),
            (data.sewage_pulse_group_indexes_snapshot.range, vec![]),
            (data.water_electrified_snapshot.range, vec![]),
            (data.water_temporary_electrified_snapshot.range, vec![]),
            (data.sewage_electrified_snapshot.range, vec![]),
            (data.sewage_temporary_electrified_snapshot.range, vec![]),
            (
                data.conductive_pollution_snapshot.as_ref().unwrap().range,
                vec![],
            ),
        ];
        empty_replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in empty_replacements.into_iter().rev() {
            empty_expected.splice(start..end, replacement);
        }
        assert_eq!(data.encode().unwrap(), empty_expected);
        let empty = WaterManagerData::decode(265, &empty_expected).unwrap();
        assert!(empty.conductive_cells().is_empty());
        assert!(empty.water_pressure().is_empty());
        assert!(empty.conductive_pollution().unwrap().is_empty());
    }

    #[test]
    fn inserting_and_removing_heating_cell_keeps_heating_sparse_fields_aligned() {
        let body = active_body(227);
        let mut data = WaterManagerData::decode(227, &body).unwrap();
        let state = HeatingConductiveCellData {
            conductivity: 0x66,
            pressure: -11,
            pulse_group_index: 0x9876,
            electrified: false,
            temporary_electrified: true,
        };
        let mut conductivity = vec![0; CELL_COUNT];
        conductivity[3] = state.conductivity;
        conductivity[4] = 1;
        let mut expected = body.clone();
        let mut replacements = vec![
            (
                data.conductivity_heating_snapshot.as_ref().unwrap().range,
                expected_literal_byte_array(&conductivity),
            ),
            (
                data.heating_pressure_snapshot.as_ref().unwrap().range,
                expected_ushort_literal_array(&[state.pressure as u16, 4]),
            ),
            (
                data.heating_pulse_group_indexes_snapshot
                    .as_ref()
                    .unwrap()
                    .range,
                expected_ushort_literal_array(&[state.pulse_group_index, 7]),
            ),
            (
                data.heating_electrified_snapshot.as_ref().unwrap().range,
                vec![0, 0b10],
            ),
            (
                data.heating_temporary_electrified_snapshot
                    .as_ref()
                    .unwrap()
                    .range,
                vec![0, 0b01],
            ),
        ];
        data.insert_heating_cell(3, state).unwrap();
        assert_eq!(data.heating_cells(), [3, 4]);
        replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in replacements.into_iter().rev() {
            expected.splice(start..end, replacement);
        }
        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = WaterManagerData::decode(227, &encoded).unwrap();
        assert_eq!(decoded.heating_cells(), [3, 4]);
        assert_eq!(decoded.conductivity_heating().unwrap()[3], 0x66);
        assert_eq!(decoded.heating_pressure(), Some([-11, 4].as_slice()));
        assert_eq!(
            decoded.heating_pulse_group_indexes(),
            Some([0x9876, 7].as_slice())
        );
        assert_eq!(
            decoded.heating_electrified(),
            Some([false, true].as_slice())
        );
        assert_eq!(
            decoded.heating_temporary_electrified(),
            Some([true, false].as_slice())
        );

        assert_eq!(data.remove_heating_cell(3).unwrap(), state);
        assert_eq!(data.heating_cells(), [4]);
        assert_eq!(data.encode().unwrap(), body);
        let last_cell = HeatingConductiveCellData {
            conductivity: 1,
            pressure: 4,
            pulse_group_index: 7,
            electrified: true,
            temporary_electrified: false,
        };
        assert_eq!(data.remove_heating_cell(4).unwrap(), last_cell);
        let mut empty_expected = body.clone();
        let mut empty_replacements = vec![
            (
                data.conductivity_heating_snapshot.as_ref().unwrap().range,
                expected_literal_byte_array(&vec![0; CELL_COUNT]),
            ),
            (
                data.heating_pressure_snapshot.as_ref().unwrap().range,
                vec![],
            ),
            (
                data.heating_pulse_group_indexes_snapshot
                    .as_ref()
                    .unwrap()
                    .range,
                vec![],
            ),
            (
                data.heating_electrified_snapshot.as_ref().unwrap().range,
                vec![],
            ),
            (
                data.heating_temporary_electrified_snapshot
                    .as_ref()
                    .unwrap()
                    .range,
                vec![],
            ),
        ];
        empty_replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in empty_replacements.into_iter().rev() {
            empty_expected.splice(start..end, replacement);
        }
        assert_eq!(data.encode().unwrap(), empty_expected);
        let empty = WaterManagerData::decode(227, &empty_expected).unwrap();
        assert!(empty.heating_cells().is_empty());
        assert!(empty.heating_pressure().unwrap().is_empty());
        let mut before_heating = WaterManagerData::decode(226, &empty_body(226)).unwrap();
        assert!(before_heating.insert_heating_cell(0, state).is_err());
    }

    #[test]
    fn primary_topology_edits_enforce_pollution_gate_and_active_state() {
        let body_without_pollution = empty_body(22);
        let mut before_pollution = WaterManagerData::decode(22, &body_without_pollution).unwrap();
        let state = WaterConductiveCellData {
            conductivity: 1,
            water_pressure: 0,
            sewage_pressure: 0,
            water_pulse_group_index: 0,
            sewage_pulse_group_index: 0,
            water_electrified: false,
            water_temporary_electrified: false,
            sewage_electrified: false,
            sewage_temporary_electrified: false,
            pollution: None,
        };
        before_pollution.insert_conductive_cell(0, state).unwrap();
        assert_eq!(before_pollution.conductive_cells(), [0]);
        assert_eq!(before_pollution.conductive_pollution(), None);
        assert_eq!(
            WaterManagerData::decode(22, &before_pollution.encode().unwrap())
                .unwrap()
                .conductive_cells(),
            [0]
        );
        assert_eq!(before_pollution.remove_conductive_cell(0).unwrap(), state);
        assert_eq!(before_pollution.encode().unwrap(), body_without_pollution);
        assert!(before_pollution.remove_conductive_cell(0).is_err());

        let mut version_23 = WaterManagerData::decode(23, &empty_body(23)).unwrap();
        assert!(version_23.insert_conductive_cell(0, state).is_err());
        assert!(version_23.conductive_cells().is_empty());
        let invalid_zero = WaterConductiveCellData {
            conductivity: 0,
            pollution: Some(0),
            ..state
        };
        assert!(version_23.insert_conductive_cell(0, invalid_zero).is_err());
        assert!(version_23.conductive_cells().is_empty());
        let state_with_pollution = WaterConductiveCellData {
            pollution: Some(0x7f),
            ..state
        };
        version_23
            .insert_conductive_cell(0, state_with_pollution)
            .unwrap();
        assert!(
            version_23
                .insert_conductive_cell(0, state_with_pollution)
                .is_err()
        );
        assert!(
            version_23
                .insert_conductive_cell(CELL_COUNT, state_with_pollution)
                .is_err()
        );
        assert!(version_23.remove_conductive_cell(1).is_err());
        assert_eq!(version_23.conductive_cells(), [0]);
        let decoded = WaterManagerData::decode(23, &version_23.encode().unwrap()).unwrap();
        assert_eq!(decoded.conductive_cells(), [0]);
        assert_eq!(decoded.conductive_pollution(), Some([0x7f].as_slice()));
        assert!(version_23.remove_conductive_cell(CELL_COUNT).is_err());
    }

    #[test]
    fn sparse_electrification_and_pollution_edits_preserve_active_cell_order() {
        let body = active_body(265);
        let mut data = WaterManagerData::decode(265, &body).unwrap();
        let mut expected = body.clone();
        let mut replacements = vec![
            (data.water_electrified_snapshot.range, vec![0, 0]),
            (data.water_temporary_electrified_snapshot.range, vec![0, 1]),
            (data.sewage_electrified_snapshot.range, vec![0, 1]),
            (data.sewage_temporary_electrified_snapshot.range, vec![0, 0]),
            (
                data.heating_electrified_snapshot.as_ref().unwrap().range,
                vec![0, 0],
            ),
            (
                data.heating_temporary_electrified_snapshot
                    .as_ref()
                    .unwrap()
                    .range,
                vec![0, 1],
            ),
            (
                data.conductive_pollution_snapshot.as_ref().unwrap().range,
                vec![0, 0xa5],
            ),
        ];
        data.set_water_electrified(2, false).unwrap();
        data.set_water_temporary_electrified(2, true).unwrap();
        data.set_sewage_electrified(2, true).unwrap();
        data.set_sewage_temporary_electrified(2, false).unwrap();
        data.set_heating_electrified(4, false).unwrap();
        data.set_heating_temporary_electrified(4, true).unwrap();
        data.set_conductive_pollution(2, 0xa5).unwrap();
        replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in replacements.into_iter().rev() {
            expected.splice(start..end, replacement);
        }

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = WaterManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.water_electrified(), [false]);
        assert_eq!(decoded.water_temporary_electrified(), [true]);
        assert_eq!(decoded.sewage_electrified(), [true]);
        assert_eq!(decoded.sewage_temporary_electrified(), [false]);
        assert_eq!(decoded.heating_electrified(), Some([false].as_slice()));
        assert_eq!(
            decoded.heating_temporary_electrified(),
            Some([true].as_slice())
        );
        assert_eq!(decoded.conductive_pollution(), Some([0xa5].as_slice()));
        assert!(data.set_water_electrified(3, true).is_err());

        let mut before_heating = WaterManagerData::decode(226, &empty_body(226)).unwrap();
        assert!(before_heating.set_heating_electrified(0, true).is_err());
        let mut before_pollution = WaterManagerData::decode(22, &empty_body(22)).unwrap();
        assert!(before_pollution.set_conductive_pollution(0, 1).is_err());
    }

    #[test]
    fn sparse_signed_pressure_edits_preserve_conductive_cell_order() {
        let body = active_body(265);
        let mut data = WaterManagerData::decode(265, &body).unwrap();
        let mut expected = body.clone();
        let mut replacements = vec![
            (data.water_pressure_snapshot.range, vec![0, 0x80, 0x00]),
            (data.sewage_pressure_snapshot.range, vec![0, 0x7f, 0xff]),
            (
                data.heating_pressure_snapshot.as_ref().unwrap().range,
                vec![0, 0xff, 0xff],
            ),
        ];
        data.set_water_pressure(2, i16::MIN).unwrap();
        data.set_sewage_pressure(2, i16::MAX).unwrap();
        data.set_heating_pressure(4, -1).unwrap();
        replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in replacements.into_iter().rev() {
            expected.splice(start..end, replacement);
        }

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = WaterManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.water_pressure(), [i16::MIN]);
        assert_eq!(decoded.sewage_pressure(), [i16::MAX]);
        assert_eq!(decoded.heating_pressure(), Some([-1].as_slice()));
        assert!(data.set_water_pressure(3, 0).is_err());

        let mut before_heating = WaterManagerData::decode(226, &empty_body(226)).unwrap();
        assert!(before_heating.set_heating_pressure(0, 1).is_err());
    }

    #[test]
    fn sparse_pulse_group_indexes_edit_only_existing_conductive_cells() {
        let body = active_body(265);
        let mut data = WaterManagerData::decode(265, &body).unwrap();
        let mut expected = body.clone();
        let mut replacements = vec![
            (
                data.water_pulse_group_indexes_snapshot.range,
                vec![0, 0x12, 0x34],
            ),
            (
                data.sewage_pulse_group_indexes_snapshot.range,
                vec![0, 0x23, 0x45],
            ),
            (
                data.heating_pulse_group_indexes_snapshot
                    .as_ref()
                    .unwrap()
                    .range,
                vec![0, 0x34, 0x56],
            ),
        ];
        data.set_water_pulse_group_index_for_cell(2, 0x1234)
            .unwrap();
        data.set_sewage_pulse_group_index_for_cell(2, 0x2345)
            .unwrap();
        data.set_heating_pulse_group_index_for_cell(4, 0x3456)
            .unwrap();
        replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in replacements.into_iter().rev() {
            expected.splice(start..end, replacement);
        }

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = WaterManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.water_pulse_group_indexes(), [0x1234]);
        assert_eq!(decoded.sewage_pulse_group_indexes(), [0x2345]);
        assert_eq!(
            decoded.heating_pulse_group_indexes(),
            Some([0x3456].as_slice())
        );
        assert!(data.set_water_pulse_group_index_for_cell(3, 1).is_err());

        let mut before_heating = WaterManagerData::decode(226, &empty_body(226)).unwrap();
        assert!(
            before_heating
                .set_heating_pulse_group_index_for_cell(0, 1)
                .is_err()
        );
    }

    #[test]
    fn existing_pulse_groups_edit_fixed_records_at_versioned_widths() {
        for version in [265, 270] {
            let body = active_body(version);
            let mut data = WaterManagerData::decode(version, &body).unwrap();
            let water_group = WaterPulseGroup {
                original_pressure: 0xa1b2_c3d4,
                current_pressure: 0x0102_0304,
                collect_pressure: (version >= 270).then_some(0x1122_3344),
                merge_index: 0x5566,
                merge_count: 0x7788,
                node: 0x99aa,
            };
            let sewage_group = WaterPulseGroup {
                original_pressure: 0xb1c2_d3e4,
                current_pressure: 0x1112_1314,
                collect_pressure: (version >= 270).then_some(0x2122_2324),
                merge_index: 0x3132,
                merge_count: 0x4142,
                node: 0x5152,
            };
            let heating_group = WaterPulseGroup {
                original_pressure: 0xc1d2_e3f4,
                current_pressure: 0x2122_2324,
                collect_pressure: None,
                merge_index: 0x3132,
                merge_count: 0x4142,
                node: 0x5152,
            };
            let mut expected = body.clone();
            let width = PULSE_GROUP_RECORD_LEN
                + if version >= 270 {
                    COLLECT_PRESSURE_FIELD_LEN
                } else {
                    0
                };
            let water_offset = data.water_pulse_groups_offset;
            let sewage_offset = data.sewage_pulse_groups_offset;
            let heating_offset = data.heating_pulse_groups_offset.unwrap();
            let water_bytes = expected_pulse_group_bytes(water_group);
            let sewage_bytes = expected_pulse_group_bytes(sewage_group);
            let heating_bytes = expected_pulse_group_bytes(heating_group);
            assert_eq!(water_bytes.len(), width);
            assert_eq!(sewage_bytes.len(), width);
            assert_eq!(heating_bytes.len(), PULSE_GROUP_RECORD_LEN);

            data.set_water_pulse_group(0, water_group).unwrap();
            data.set_sewage_pulse_group(0, sewage_group).unwrap();
            data.set_heating_pulse_group(0, heating_group).unwrap();
            expected[water_offset..water_offset + width].copy_from_slice(&water_bytes);
            expected[sewage_offset..sewage_offset + width].copy_from_slice(&sewage_bytes);
            expected[heating_offset..heating_offset + PULSE_GROUP_RECORD_LEN]
                .copy_from_slice(&heating_bytes);

            let encoded = data.encode().unwrap();
            assert_eq!(encoded, expected, "version {version}");
            let decoded = WaterManagerData::decode(version, &encoded).unwrap();
            assert_eq!(decoded.water_pulse_groups(), &[water_group]);
            assert_eq!(decoded.sewage_pulse_groups(), &[sewage_group]);
            assert_eq!(
                decoded.heating_pulse_groups(),
                Some([heating_group].as_slice())
            );

            let mut invalid_water = water_group;
            invalid_water.collect_pressure = (version < 270).then_some(1);
            assert!(data.set_water_pulse_group(0, invalid_water).is_err());
            assert!(data.set_water_pulse_group(1, water_group).is_err());
            let mut invalid_heating = heating_group;
            invalid_heating.collect_pressure = Some(1);
            assert!(data.set_heating_pulse_group(0, invalid_heating).is_err());
        }

        let mut before_heating = WaterManagerData::decode(226, &empty_body(226)).unwrap();
        assert!(
            before_heating
                .set_heating_pulse_group(
                    0,
                    WaterPulseGroup {
                        original_pressure: 0,
                        current_pressure: 0,
                        collect_pressure: None,
                        merge_index: 0,
                        merge_count: 0,
                        node: 0,
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn existing_pulse_units_edit_only_their_fixed_width_records() {
        let body = active_body(265);
        let mut data = WaterManagerData::decode(265, &body).unwrap();
        let water_unit = WaterPulseUnit {
            group: 0x1234,
            node: 0x5678,
            x: 0x9a,
            z: 0xbc,
        };
        let sewage_unit = WaterPulseUnit {
            group: 0x2345,
            node: 0x6789,
            x: 0xab,
            z: 0xcd,
        };
        let heating_unit = WaterPulseUnit {
            group: 0x3456,
            node: 0x789a,
            x: 0xbc,
            z: 0xde,
        };
        let mut expected = body.clone();
        let unit_records = [
            (data.water_pulse_units_offset, water_unit),
            (data.sewage_pulse_units_offset, sewage_unit),
            (data.heating_pulse_units_offset.unwrap(), heating_unit),
        ];
        for (offset, unit) in unit_records {
            expected[offset..offset + PULSE_UNIT_RECORD_LEN]
                .copy_from_slice(&expected_pulse_unit_bytes(unit));
        }

        data.set_water_pulse_unit(0, water_unit).unwrap();
        data.set_sewage_pulse_unit(0, sewage_unit).unwrap();
        data.set_heating_pulse_unit(0, heating_unit).unwrap();

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = WaterManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.water_pulse_units(), &[water_unit]);
        assert_eq!(decoded.sewage_pulse_units(), &[sewage_unit]);
        assert_eq!(
            decoded.heating_pulse_units(),
            Some([heating_unit].as_slice())
        );
        assert!(data.set_water_pulse_unit(1, water_unit).is_err());
        assert!(data.set_sewage_pulse_unit(1, sewage_unit).is_err());
        assert!(data.set_heating_pulse_unit(1, heating_unit).is_err());

        let mut before_heating = WaterManagerData::decode(226, &empty_body(226)).unwrap();
        assert!(
            before_heating
                .set_heating_pulse_unit(
                    0,
                    WaterPulseUnit {
                        group: 0,
                        node: 0,
                        x: 0,
                        z: 0,
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn scalar_edits_patch_only_big_endian_tail_fields() {
        for version in [93, 94, 265] {
            let body = empty_body(version);
            let mut data = WaterManagerData::decode(version, &body).unwrap();
            data.set_processed_cell_count(-17);
            data.set_conductive_cell_count(0x1234_5678);
            data.set_can_continue(false);

            let encoded = data.encode().unwrap();
            let scalar_tail_len = if version >= 94 { 13 } else { 11 };
            let scalar_start = body.len() - scalar_tail_len;
            assert_eq!(encoded.len(), body.len());
            assert_eq!(
                &encoded[scalar_start..scalar_start + 4],
                &(-17_i32).to_be_bytes()
            );
            assert_eq!(
                &encoded[scalar_start + 4..scalar_start + 8],
                &0x1234_5678_i32.to_be_bytes()
            );
            assert_eq!(encoded[scalar_start + 8], 0);
            assert_eq!(&encoded[..scalar_start], &body[..scalar_start]);
            assert_eq!(&encoded[scalar_start + 9..], &body[scalar_start + 9..]);
            let decoded = WaterManagerData::decode(version, &encoded).unwrap();
            assert_eq!(decoded.processed_cell_count(), -17);
            assert_eq!(decoded.conductive_cell_count(), 0x1234_5678);
            assert!(!decoded.can_continue());
        }
    }

    #[test]
    fn unchanged_noncanonical_continuation_byte_is_preserved() {
        let mut body = empty_body(265);
        let continuation_offset = body.len() - 5; // Two null references and the end tag follow.
        body[continuation_offset] = 2;

        let mut data = WaterManagerData::decode(265, &body).unwrap();
        assert!(data.can_continue());
        assert_eq!(data.encode().unwrap(), body);

        data.set_can_continue(false);
        let encoded = data.encode().unwrap();
        assert_eq!(encoded[continuation_offset], 0);
        assert_eq!(
            &encoded[..continuation_offset],
            &body[..continuation_offset]
        );
        assert_eq!(
            &encoded[continuation_offset + 1..],
            &body[continuation_offset + 1..]
        );
    }

    #[test]
    fn collect_water_bool_gate_copies_current_pressure() {
        let body = active_body(270);
        let data = WaterManagerData::decode(270, &body).unwrap();
        assert_eq!(data.collect_water_pressure()[0], 0x1234);
        assert_eq!(data.collect_water_pressure()[1], 0);
        assert_eq!(
            data.collect_water_pressure_selector().unwrap()[..2],
            [true, false]
        );
        assert_eq!(
            data.water_pulse_groups()[0].collect_pressure,
            Some(0x1516_1718)
        );
    }

    #[test]
    fn version_270_collect_water_selectors_edit_and_reencode_as_packed_bools() {
        let body = active_body(270);
        let mut data = WaterManagerData::decode(270, &body).unwrap();
        let selector_range = data.collect_water_pressure_selector_range.unwrap();
        let pressure_range = data.water_node_pressure_range;
        data.set_collect_water_pressure_selector(0, true).unwrap();
        assert_eq!(data.collect_water_pressure()[0], 0x1234);
        data.set_water_node_pressure(0, 0x4321).unwrap();
        data.set_water_node_pressure(1, 0x5678).unwrap();
        data.set_collect_water_pressure_selector(1, true).unwrap();
        assert_eq!(data.collect_water_pressure()[0], 0x4321);
        assert_eq!(data.collect_water_pressure()[1], 0x5678);
        data.set_collect_water_pressure_selector(0, false).unwrap();
        assert_eq!(data.collect_water_pressure()[0], 0);
        data.set_collect_water_pressure_selector(0, true).unwrap();

        let mut expected_selectors = vec![false; NODE_COUNT];
        expected_selectors[0] = true;
        expected_selectors[1] = true;
        let mut packed_selectors = vec![0; NODE_COUNT.div_ceil(8)];
        for (node, selected) in expected_selectors.iter().copied().enumerate() {
            if selected {
                packed_selectors[node / 8] |= 1 << (node % 8);
            }
        }
        let mut expected_selector_bytes = Vec::new();
        for chunk in packed_selectors.chunks(128) {
            expected_selector_bytes.push((chunk.len() - 1) as u8);
            expected_selector_bytes.extend_from_slice(chunk);
        }
        let mut expected = body;
        let mut replacements = vec![
            (selector_range, expected_selector_bytes),
            (
                pressure_range,
                expected_ushort_array_with_two_nonzero_values(NODE_COUNT, 0x4321, 0x5678),
            ),
        ];
        replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in replacements.into_iter().rev() {
            expected.splice(start..end, replacement);
        }

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = WaterManagerData::decode(270, &encoded).unwrap();
        assert_eq!(
            decoded.collect_water_pressure_selector().unwrap()[..2],
            [true, true]
        );
        assert_eq!(decoded.collect_water_pressure()[0], 0x4321);
        assert_eq!(decoded.collect_water_pressure()[1], 0x5678);
        assert!(data.set_collect_water_pressure(0, 1).is_err());
        assert!(
            data.set_collect_water_pressure_selector(NODE_COUNT, true)
                .is_err()
        );

        let mut before_gate = WaterManagerData::decode(269, &empty_body(269)).unwrap();
        assert!(
            before_gate
                .set_collect_water_pressure_selector(0, true)
                .is_err()
        );
        let mut after_gate = WaterManagerData::decode(271, &empty_body(271)).unwrap();
        assert!(
            after_gate
                .set_collect_water_pressure_selector(0, true)
                .is_err()
        );
    }

    #[test]
    fn collect_pressure_ushort_arrays_follow_271_and_272_gates() {
        let version_271 = WaterManagerData::decode(271, &active_body(271)).unwrap();
        assert_eq!(version_271.collect_water_pressure()[0], 0x3210);
        assert_eq!(version_271.collect_sewage_pressure()[0], 0);

        let version_272 = WaterManagerData::decode(272, &active_body(272)).unwrap();
        assert_eq!(version_272.collect_water_pressure()[0], 0x3210);
        assert_eq!(version_272.collect_sewage_pressure()[0], 0x4567);
    }

    #[test]
    fn collect_pressure_edits_reencode_only_supported_arrays() {
        for version in [271, 272] {
            let body = active_body(version);
            let mut data = WaterManagerData::decode(version, &body).unwrap();
            assert_eq!(data.encode().unwrap(), body);
            let mut expected = body.clone();
            let water_range = data.collect_water_pressure_range.unwrap();
            let water_bytes =
                expected_ushort_array_with_two_nonzero_values(NODE_COUNT, 0x3210, 0x4567);
            let mut replacements = vec![(water_range, water_bytes)];
            data.set_collect_water_pressure(1, 0x4567).unwrap();

            if version >= 272 {
                let sewage_range = data.collect_sewage_pressure_range.unwrap();
                let sewage_bytes =
                    expected_ushort_array_with_two_nonzero_values(NODE_COUNT, 0x4567, 0x789a);
                replacements.push((sewage_range, sewage_bytes));
                data.set_collect_sewage_pressure(1, 0x789a).unwrap();
            } else {
                assert!(data.set_collect_sewage_pressure(1, 0x789a).is_err());
            }
            assert!(data.set_collect_water_pressure(NODE_COUNT, 1).is_err());

            replacements.sort_by_key(|((start, _), _)| *start);
            for ((start, end), replacement) in replacements.into_iter().rev() {
                expected.splice(start..end, replacement);
            }

            let encoded = data.encode().unwrap();
            assert_eq!(encoded, expected, "version {version}");
            let decoded = WaterManagerData::decode(version, &encoded).unwrap();
            assert_eq!(decoded.collect_water_pressure()[0], 0x3210);
            assert_eq!(decoded.collect_water_pressure()[1], 0x4567);
            if version >= 272 {
                assert_eq!(decoded.collect_sewage_pressure()[0], 0x4567);
                assert_eq!(decoded.collect_sewage_pressure()[1], 0x789a);
            }
        }

        let mut legacy_bool = WaterManagerData::decode(270, &active_body(270)).unwrap();
        assert!(legacy_bool.set_collect_water_pressure(0, 0x1234).is_err());
    }

    #[test]
    fn water_and_sewage_node_pressure_edits_reencode_only_selected_arrays() {
        let body = active_body(265);
        let mut data = WaterManagerData::decode(265, &body).unwrap();
        assert_eq!(data.encode().unwrap(), body);
        let mut expected = body.clone();
        let mut replacements = vec![
            (
                data.water_node_pressure_range,
                expected_ushort_array_with_two_nonzero_values(NODE_COUNT, 0x1234, 0x4567),
            ),
            (
                data.sewage_node_pressure_range,
                expected_ushort_array_with_two_nonzero_values(NODE_COUNT, 0x25, 0x789a),
            ),
        ];
        data.set_water_node_pressure(1, 0x4567).unwrap();
        data.set_sewage_node_pressure(1, 0x789a).unwrap();
        replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in replacements.into_iter().rev() {
            expected.splice(start..end, replacement);
        }

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = WaterManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.water_node_pressure()[0], 0x1234);
        assert_eq!(decoded.water_node_pressure()[1], 0x4567);
        assert_eq!(decoded.sewage_node_pressure()[0], 0x25);
        assert_eq!(decoded.sewage_node_pressure()[1], 0x789a);
        assert!(data.set_water_node_pressure(NODE_COUNT, 1).is_err());

        let mut legacy = WaterManagerData::decode(201, &empty_body(201)).unwrap();
        assert!(
            legacy
                .set_water_node_pressure(LEGACY_NODE_COUNT, 1)
                .is_err()
        );
    }

    #[test]
    fn extra_and_heating_node_pressure_edits_follow_node_array_gates() {
        let body = active_body(265);
        let mut data = WaterManagerData::decode(265, &body).unwrap();
        let mut expected = body.clone();
        let mut replacements = vec![
            (
                data.water_node_extra_pressure_range,
                expected_ushort_array_with_two_nonzero_values(NODE_COUNT, 0x23, 0x1111),
            ),
            (
                data.sewage_node_extra_pressure_range,
                expected_ushort_array_with_two_nonzero_values(NODE_COUNT, 0x26, 0x2222),
            ),
            (
                data.heating_node_pressure_range.unwrap(),
                expected_ushort_array_with_two_nonzero_values(NODE_COUNT, 0x28, 0x3333),
            ),
            (
                data.heating_node_extra_pressure_range.unwrap(),
                expected_ushort_array_with_two_nonzero_values(NODE_COUNT, 0x29, 0x4444),
            ),
        ];
        data.set_water_node_extra_pressure(1, 0x1111).unwrap();
        data.set_sewage_node_extra_pressure(1, 0x2222).unwrap();
        data.set_heating_node_pressure(1, 0x3333).unwrap();
        data.set_heating_node_extra_pressure(1, 0x4444).unwrap();
        replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in replacements.into_iter().rev() {
            expected.splice(start..end, replacement);
        }

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = WaterManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.water_node_extra_pressure()[1], 0x1111);
        assert_eq!(decoded.sewage_node_extra_pressure()[1], 0x2222);
        assert_eq!(decoded.heating_node_pressure().unwrap()[1], 0x3333);
        assert_eq!(decoded.heating_node_extra_pressure().unwrap()[1], 0x4444);

        let mut before_heating = WaterManagerData::decode(226, &empty_body(226)).unwrap();
        assert!(before_heating.set_heating_node_pressure(0, 1).is_err());
        assert!(
            before_heating
                .set_heating_node_extra_pressure(0, 1)
                .is_err()
        );
    }

    #[test]
    fn node_pulse_group_index_edits_reencode_only_selected_arrays() {
        let body = active_body(265);
        let mut data = WaterManagerData::decode(265, &body).unwrap();
        let mut expected = body.clone();
        let mut replacements = vec![
            (
                data.water_node_pulse_groups_range,
                expected_ushort_array_with_two_nonzero_values(NODE_COUNT, 0x22, 0x1111),
            ),
            (
                data.sewage_node_pulse_groups_range,
                expected_ushort_array_with_two_nonzero_values(NODE_COUNT, 0x24, 0x2222),
            ),
            (
                data.heating_node_pulse_groups_range.unwrap(),
                expected_ushort_array_with_two_nonzero_values(NODE_COUNT, 0x27, 0x3333),
            ),
        ];
        data.set_water_node_pulse_group_index(1, 0x1111).unwrap();
        data.set_sewage_node_pulse_group_index(1, 0x2222).unwrap();
        data.set_heating_node_pulse_group_index(1, 0x3333).unwrap();
        replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in replacements.into_iter().rev() {
            expected.splice(start..end, replacement);
        }

        let encoded = data.encode().unwrap();
        if encoded != expected {
            let mismatch = encoded
                .iter()
                .zip(&expected)
                .position(|(actual, expected)| actual != expected)
                .unwrap_or(encoded.len().min(expected.len()));
            panic!(
                "encoded WaterManager differs at byte {mismatch}; actual length {}, expected length {}, actual bytes {:?}, expected bytes {:?}",
                encoded.len(),
                expected.len(),
                &encoded[mismatch.saturating_sub(4)..(mismatch + 12).min(encoded.len())],
                &expected[mismatch.saturating_sub(4)..(mismatch + 12).min(expected.len())],
            );
        }
        let decoded = WaterManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.water_node_pulse_groups()[0], 0x22);
        assert_eq!(decoded.water_node_pulse_groups()[1], 0x1111);
        assert_eq!(decoded.sewage_node_pulse_groups()[0], 0x24);
        assert_eq!(decoded.sewage_node_pulse_groups()[1], 0x2222);
        assert_eq!(decoded.heating_node_pulse_groups().unwrap()[0], 0x27);
        assert_eq!(decoded.heating_node_pulse_groups().unwrap()[1], 0x3333);
        assert!(
            data.set_water_node_pulse_group_index(NODE_COUNT, 1)
                .is_err()
        );

        let mut legacy = WaterManagerData::decode(201, &empty_body(201)).unwrap();
        assert!(
            legacy
                .set_water_node_pulse_group_index(LEGACY_NODE_COUNT, 1)
                .is_err()
        );

        let mut before_heating = WaterManagerData::decode(226, &empty_body(226)).unwrap();
        assert!(
            before_heating
                .set_heating_node_pulse_group_index(0, 1)
                .is_err()
        );
    }

    #[test]
    fn node_pollution_edits_follow_byte_array_and_node_count_gates() {
        let body = active_body(265);
        let mut data = WaterManagerData::decode(265, &body).unwrap();
        assert_eq!(data.encode().unwrap(), body);
        let range = data.node_pollution_range.unwrap();
        let mut expected_values = vec![0; NODE_COUNT];
        expected_values[0] = 30;
        expected_values[1] = 0xa5;
        let mut expected_array = Vec::new();
        for chunk in expected_values.chunks(128) {
            expected_array.push((chunk.len() - 1) as u8);
            expected_array.extend_from_slice(chunk);
        }
        let mut expected = body;
        expected.splice(range.0..range.1, expected_array);
        data.set_node_pollution(1, 0xa5).unwrap();

        let encoded = data.encode().unwrap();
        if encoded != expected {
            let mismatch = encoded
                .iter()
                .zip(&expected)
                .position(|(actual, expected)| actual != expected)
                .unwrap_or(encoded.len().min(expected.len()));
            panic!(
                "encoded WaterManager differs at byte {mismatch}; actual length {}, expected length {}, actual bytes {:?}, expected bytes {:?}",
                encoded.len(),
                expected.len(),
                &encoded[mismatch.saturating_sub(4)..(mismatch + 12).min(encoded.len())],
                &expected[mismatch.saturating_sub(4)..(mismatch + 12).min(expected.len())],
            );
        }
        let decoded = WaterManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.node_pollution().unwrap()[0], 30);
        assert_eq!(decoded.node_pollution().unwrap()[1], 0xa5);
        assert!(data.set_node_pollution(NODE_COUNT, 1).is_err());

        let mut legacy = WaterManagerData::decode(201, &empty_body(201)).unwrap();
        assert!(legacy.set_node_pollution(LEGACY_NODE_COUNT, 1).is_err());

        let mut before_pollution = WaterManagerData::decode(22, &empty_body(22)).unwrap();
        assert!(before_pollution.set_node_pollution(0, 1).is_err());
    }

    #[test]
    fn closest_segment_grids_edit_existing_cells_at_version_gates() {
        let body = active_body(265);
        let mut data = WaterManagerData::decode(265, &body).unwrap();
        let mut expected = body.clone();
        let mut replacements = vec![
            (
                data.closest_pipe_segments_range.unwrap(),
                expected_ushort_array_with_two_nonzero_values(CELL_COUNT, 0x1111, 0x2222),
            ),
            (
                data.closest_heating_segments_range.unwrap(),
                expected_ushort_array_with_two_nonzero_values(CELL_COUNT, 0x3333, 0x4444),
            ),
        ];
        data.set_closest_pipe_segment(0, 0x1111).unwrap();
        data.set_closest_pipe_segment(1, 0x2222).unwrap();
        data.set_closest_heating_segment(0, 0x3333).unwrap();
        data.set_closest_heating_segment(1, 0x4444).unwrap();
        replacements.sort_by_key(|((start, _), _)| *start);
        for ((start, end), replacement) in replacements.into_iter().rev() {
            expected.splice(start..end, replacement);
        }

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = WaterManagerData::decode(265, &encoded).unwrap();
        assert_eq!(
            decoded.closest_pipe_segments().unwrap()[..2],
            [0x1111, 0x2222]
        );
        assert_eq!(
            decoded.closest_heating_segments().unwrap()[..2],
            [0x3333, 0x4444]
        );
        assert!(data.set_closest_pipe_segment(CELL_COUNT, 1).is_err());
        assert!(data.set_closest_heating_segment(CELL_COUNT, 1).is_err());

        let mut before_pipe_grid = WaterManagerData::decode(72, &empty_body(72)).unwrap();
        assert!(before_pipe_grid.set_closest_pipe_segment(0, 1).is_err());
        let mut before_heating_grid = WaterManagerData::decode(226, &empty_body(226)).unwrap();
        assert!(
            before_heating_grid
                .set_closest_heating_segment(0, 1)
                .is_err()
        );
    }

    #[test]
    fn rejects_pulse_counts_above_fixed_buffer_capacities() {
        const EMPTY_V265_PULSE_START: usize = 4 * CELL_COUNT / 128;

        let mut excessive_groups = empty_body(265);
        excessive_groups[EMPTY_V265_PULSE_START..EMPTY_V265_PULSE_START + 2]
            .copy_from_slice(&1_025_u16.to_be_bytes());
        assert!(WaterManagerData::decode(265, &excessive_groups).is_err());

        let mut excessive_units = empty_body(265);
        let unit_count_offset = EMPTY_V265_PULSE_START + 2;
        excessive_units[unit_count_offset..unit_count_offset + 2]
            .copy_from_slice(&32_769_u16.to_be_bytes());
        assert!(WaterManagerData::decode(265, &excessive_units).is_err());
    }

    #[test]
    fn accepts_pulse_counts_at_fixed_buffer_capacities() {
        const EMPTY_V265_PULSE_START: usize = 4 * CELL_COUNT / 128;
        let empty = empty_body(265);
        let mut pulse_section = Vec::new();
        pulse_section.extend_from_slice(&(PULSE_GROUP_CAPACITY as u16).to_be_bytes());
        pulse_section.extend(vec![0; PULSE_GROUP_CAPACITY * 14]);
        pulse_section.extend_from_slice(&(PULSE_UNIT_CAPACITY as u16).to_be_bytes());
        pulse_section.extend(vec![0; PULSE_UNIT_CAPACITY * 6]);
        pulse_section.extend_from_slice(&[0; 8]); // Empty sewage and heating tables.

        let mut body = empty;
        body.splice(
            EMPTY_V265_PULSE_START..EMPTY_V265_PULSE_START + 12,
            pulse_section,
        );
        let data = WaterManagerData::decode(265, &body).unwrap();
        assert_eq!(data.water_pulse_groups().len(), PULSE_GROUP_CAPACITY);
        assert_eq!(data.water_pulse_units().len(), PULSE_UNIT_CAPACITY);
    }

    #[test]
    fn rejects_truncated_and_overlong_water_bodies() {
        let body = empty_body(272);
        assert!(WaterManagerData::decode(272, &body[..body.len() - 1]).is_err());
        let mut overlong = body;
        overlong.push(0);
        assert!(WaterManagerData::decode(272, &overlong).is_err());

        let mut invalid_guide = empty_body(272);
        let guide_start = invalid_guide.len() - 4;
        invalid_guide[guide_start..guide_start + 2].copy_from_slice(&[0xfe, 0xff]);
        assert!(WaterManagerData::decode(272, &invalid_guide).is_err());
    }
}
