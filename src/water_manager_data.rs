use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    read_encoded_bool_array, read_encoded_byte_array, read_encoded_ushort_array,
};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

const CELL_COUNT: usize = 65_536;
const NODE_COUNT: usize = 32_768;
const LEGACY_NODE_COUNT: usize = 16_384;
const PULSE_GROUP_CAPACITY: usize = 1_024;
const PULSE_UNIT_CAPACITY: usize = 32_768;
const PULSE_GROUP_RECORD_LEN: usize = 14;
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

/// Decoded reader fields from `WaterManager+Data`.
///
/// Sparse cell columns use the corresponding conductivity plane's ascending
/// cell-index order. Node arrays retain their full 32,768-entry size, with
/// version-derived defaults in the legacy upper half. The complete source body
/// remains available for exact unchanged writing. The processed-cell count,
/// conductive-cell count, and continuation flag can be edited in place; all
/// other fields remain byte-identical. The inner serializer version controls
/// the 23, 73, 94, 202, 227, and 270–272 gates. Corpus samples cover versions
/// 194–118011; active network values and gates 270–272 have synthetic coverage
/// only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaterManagerData {
    format_version: u32,
    conductivity: Vec<u8>,
    conductive_cells: Vec<usize>,
    conductivity_heating: Option<Vec<u8>>,
    heating_cells: Vec<usize>,
    water_pressure: Vec<i16>,
    sewage_pressure: Vec<i16>,
    heating_pressure: Option<Vec<i16>>,
    water_pulse_group_indexes: Vec<u16>,
    sewage_pulse_group_indexes: Vec<u16>,
    heating_pulse_group_indexes: Option<Vec<u16>>,
    closest_pipe_segments: Option<Vec<u16>>,
    closest_heating_segments: Option<Vec<u16>>,
    water_electrified: Vec<bool>,
    water_temporary_electrified: Vec<bool>,
    sewage_electrified: Vec<bool>,
    sewage_temporary_electrified: Vec<bool>,
    heating_electrified: Option<Vec<bool>>,
    heating_temporary_electrified: Option<Vec<bool>>,
    conductive_pollution: Option<Vec<u8>>,
    water_pulse_groups: Vec<WaterPulseGroup>,
    water_pulse_groups_offset: usize,
    water_pulse_units: Vec<WaterPulseUnit>,
    sewage_pulse_groups: Vec<WaterPulseGroup>,
    sewage_pulse_groups_offset: usize,
    sewage_pulse_units: Vec<WaterPulseUnit>,
    heating_pulse_groups: Option<Vec<WaterPulseGroup>>,
    heating_pulse_groups_offset: Option<usize>,
    heating_pulse_units: Option<Vec<WaterPulseUnit>>,
    water_node_pulse_groups: Vec<u16>,
    water_node_pressure: Vec<u16>,
    water_node_extra_pressure: Vec<u16>,
    sewage_node_pulse_groups: Vec<u16>,
    sewage_node_pressure: Vec<u16>,
    sewage_node_extra_pressure: Vec<u16>,
    heating_node_pulse_groups: Option<Vec<u16>>,
    heating_node_pressure: Option<Vec<u16>>,
    heating_node_extra_pressure: Option<Vec<u16>>,
    node_pollution: Option<Vec<u8>>,
    collect_water_pressure: Vec<u16>,
    collect_sewage_pressure: Vec<u16>,
    processed_cell_count: i32,
    conductive_cell_count: i32,
    can_continue: bool,
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
        let conductivity = read_encoded_byte_array(&mut reader, CELL_COUNT)?;
        let conductive_cells = indexes_with_nonzero_values(&conductivity);
        let conductivity_heating = if format_version >= HEATING_VERSION {
            Some(read_encoded_byte_array(&mut reader, CELL_COUNT)?)
        } else {
            None
        };
        let heating_cells = conductivity_heating
            .as_deref()
            .map(indexes_with_nonzero_values)
            .unwrap_or_default();

        let water_pressure = read_signed_ushort_array(&mut reader, conductive_cells.len())?;
        let sewage_pressure = read_signed_ushort_array(&mut reader, conductive_cells.len())?;
        let heating_pressure = if format_version >= HEATING_VERSION {
            Some(read_signed_ushort_array(&mut reader, heating_cells.len())?)
        } else {
            None
        };

        let water_pulse_group_indexes =
            read_encoded_ushort_array(&mut reader, conductive_cells.len())?;
        let sewage_pulse_group_indexes =
            read_encoded_ushort_array(&mut reader, conductive_cells.len())?;
        let heating_pulse_group_indexes = if format_version >= HEATING_VERSION {
            Some(read_encoded_ushort_array(&mut reader, heating_cells.len())?)
        } else {
            None
        };

        let closest_pipe_segments = if format_version >= CLOSEST_PIPE_SEGMENT_VERSION {
            Some(read_encoded_ushort_array(&mut reader, CELL_COUNT)?)
        } else {
            None
        };
        let closest_heating_segments = if format_version >= HEATING_VERSION {
            Some(read_encoded_ushort_array(&mut reader, CELL_COUNT)?)
        } else {
            None
        };

        let water_electrified = read_encoded_bool_array(&mut reader, conductive_cells.len())?;
        let water_temporary_electrified =
            read_encoded_bool_array(&mut reader, conductive_cells.len())?;
        let sewage_electrified = read_encoded_bool_array(&mut reader, conductive_cells.len())?;
        let sewage_temporary_electrified =
            read_encoded_bool_array(&mut reader, conductive_cells.len())?;
        let heating_electrified = if format_version >= HEATING_VERSION {
            Some(read_encoded_bool_array(&mut reader, heating_cells.len())?)
        } else {
            None
        };
        let heating_temporary_electrified = if format_version >= HEATING_VERSION {
            Some(read_encoded_bool_array(&mut reader, heating_cells.len())?)
        } else {
            None
        };
        let conductive_pollution = if format_version >= POLLUTION_VERSION {
            Some(read_encoded_byte_array(
                &mut reader,
                conductive_cells.len(),
            )?)
        } else {
            None
        };

        let (water_pulse_groups, water_pulse_groups_offset) =
            read_pulse_groups(&mut reader, format_version, false)?;
        let water_pulse_units = read_pulse_units(&mut reader)?;
        let (sewage_pulse_groups, sewage_pulse_groups_offset) =
            read_pulse_groups(&mut reader, format_version, false)?;
        let sewage_pulse_units = read_pulse_units(&mut reader)?;
        let (heating_pulse_groups, heating_pulse_groups_offset, heating_pulse_units) =
            if format_version >= HEATING_VERSION {
                let (groups, groups_offset) = read_pulse_groups(&mut reader, format_version, true)?;
                (
                    Some(groups),
                    Some(groups_offset),
                    Some(read_pulse_units(&mut reader)?),
                )
            } else {
                (None, None, None)
            };

        let stored_node_count = if format_version >= FULL_NODE_COUNT_VERSION {
            NODE_COUNT
        } else {
            LEGACY_NODE_COUNT
        };
        let water_node_pulse_groups =
            read_node_ushort_array(&mut reader, stored_node_count, u16::MAX)?;
        let water_node_pressure = read_node_ushort_array(&mut reader, stored_node_count, 0)?;
        let water_node_extra_pressure = read_node_ushort_array(&mut reader, stored_node_count, 0)?;
        let sewage_node_pulse_groups =
            read_node_ushort_array(&mut reader, stored_node_count, u16::MAX)?;
        let sewage_node_pressure = read_node_ushort_array(&mut reader, stored_node_count, 0)?;
        let sewage_node_extra_pressure = read_node_ushort_array(&mut reader, stored_node_count, 0)?;
        let (heating_node_pulse_groups, heating_node_pressure, heating_node_extra_pressure) =
            if format_version >= HEATING_VERSION {
                (
                    Some(read_node_ushort_array(
                        &mut reader,
                        stored_node_count,
                        u16::MAX,
                    )?),
                    Some(read_node_ushort_array(&mut reader, stored_node_count, 0)?),
                    Some(read_node_ushort_array(&mut reader, stored_node_count, 0)?),
                )
            } else {
                (None, None, None)
            };
        let node_pollution = if format_version >= POLLUTION_VERSION {
            Some(read_node_byte_array(&mut reader, stored_node_count, 0)?)
        } else {
            None
        };

        let mut collect_water_pressure = vec![0; NODE_COUNT];
        if format_version >= COLLECT_WATER_USHORT_VERSION {
            collect_water_pressure[..stored_node_count]
                .copy_from_slice(&read_encoded_ushort_array(&mut reader, stored_node_count)?);
        } else if format_version >= COLLECT_WATER_BOOL_VERSION {
            let collects = read_encoded_bool_array(&mut reader, stored_node_count)?;
            for (index, collects_pressure) in collects.into_iter().enumerate() {
                if collects_pressure {
                    collect_water_pressure[index] = water_node_pressure[index];
                }
            }
        }
        let mut collect_sewage_pressure = vec![0; NODE_COUNT];
        if format_version >= COLLECT_SEWAGE_USHORT_VERSION {
            collect_sewage_pressure[..stored_node_count]
                .copy_from_slice(&read_encoded_ushort_array(&mut reader, stored_node_count)?);
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
            conductive_cells,
            conductivity_heating,
            heating_cells,
            water_pressure,
            sewage_pressure,
            heating_pressure,
            water_pulse_group_indexes,
            sewage_pulse_group_indexes,
            heating_pulse_group_indexes,
            closest_pipe_segments,
            closest_heating_segments,
            water_electrified,
            water_temporary_electrified,
            sewage_electrified,
            sewage_temporary_electrified,
            heating_electrified,
            heating_temporary_electrified,
            conductive_pollution,
            water_pulse_groups,
            water_pulse_groups_offset,
            water_pulse_units,
            sewage_pulse_groups,
            sewage_pulse_groups_offset,
            sewage_pulse_units,
            heating_pulse_groups,
            heating_pulse_groups_offset,
            heating_pulse_units,
            water_node_pulse_groups,
            water_node_pressure,
            water_node_extra_pressure,
            sewage_node_pulse_groups,
            sewage_node_pressure,
            sewage_node_extra_pressure,
            heating_node_pulse_groups,
            heating_node_pressure,
            heating_node_extra_pressure,
            node_pollution,
            collect_water_pressure,
            collect_sewage_pressure,
            processed_cell_count,
            conductive_cell_count,
            can_continue,
            scalar_fields_offset,
            guide_references,
            original_body: bytes.to_vec(),
        })
    }

    /// Returns the inner DataSerializer version used for reader gates.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the full primary 65,536-cell conductivity plane.
    pub fn conductivity(&self) -> &[u8] {
        &self.conductivity
    }

    /// Returns indexes of cells with nonzero primary conductivity.
    pub fn conductive_cells(&self) -> &[usize] {
        &self.conductive_cells
    }

    /// Returns the secondary conductivity plane from version 227.
    pub fn conductivity_heating(&self) -> Option<&[u8]> {
        self.conductivity_heating.as_deref()
    }

    /// Returns indexes of cells with nonzero secondary conductivity.
    pub fn heating_cells(&self) -> &[usize] {
        &self.heating_cells
    }

    /// Returns sparse signed water pressures in primary-conductive-cell order.
    pub fn water_pressure(&self) -> &[i16] {
        &self.water_pressure
    }

    /// Returns sparse signed sewage pressures in primary-conductive-cell order.
    pub fn sewage_pressure(&self) -> &[i16] {
        &self.sewage_pressure
    }

    /// Returns sparse signed heating pressures from version 227.
    pub fn heating_pressure(&self) -> Option<&[i16]> {
        self.heating_pressure.as_deref()
    }

    /// Returns sparse water pulse-group indexes in primary-conductive order.
    pub fn water_pulse_group_indexes(&self) -> &[u16] {
        &self.water_pulse_group_indexes
    }

    /// Returns sparse sewage pulse-group indexes in primary-conductive order.
    pub fn sewage_pulse_group_indexes(&self) -> &[u16] {
        &self.sewage_pulse_group_indexes
    }

    /// Returns sparse heating pulse-group indexes from version 227.
    pub fn heating_pulse_group_indexes(&self) -> Option<&[u16]> {
        self.heating_pulse_group_indexes.as_deref()
    }

    /// Returns the version-73 closest-pipe segment grid.
    pub fn closest_pipe_segments(&self) -> Option<&[u16]> {
        self.closest_pipe_segments.as_deref()
    }

    /// Returns the second closest-segment grid from version 227.
    pub fn closest_heating_segments(&self) -> Option<&[u16]> {
        self.closest_heating_segments.as_deref()
    }

    /// Returns sparse current water electrification flags.
    pub fn water_electrified(&self) -> &[bool] {
        &self.water_electrified
    }

    /// Returns sparse temporary water electrification flags.
    pub fn water_temporary_electrified(&self) -> &[bool] {
        &self.water_temporary_electrified
    }

    /// Returns sparse current sewage electrification flags.
    pub fn sewage_electrified(&self) -> &[bool] {
        &self.sewage_electrified
    }

    /// Returns sparse temporary sewage electrification flags.
    pub fn sewage_temporary_electrified(&self) -> &[bool] {
        &self.sewage_temporary_electrified
    }

    /// Returns sparse heating electrification flags from version 227.
    pub fn heating_electrified(&self) -> Option<&[bool]> {
        self.heating_electrified.as_deref()
    }

    /// Returns sparse temporary heating electrification flags from version 227.
    pub fn heating_temporary_electrified(&self) -> Option<&[bool]> {
        self.heating_temporary_electrified.as_deref()
    }

    /// Returns sparse pollution bytes from version 23.
    pub fn conductive_pollution(&self) -> Option<&[u8]> {
        self.conductive_pollution.as_deref()
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

    /// Returns all 32,768 water node pulse-group indexes.
    pub fn water_node_pulse_groups(&self) -> &[u16] {
        &self.water_node_pulse_groups
    }

    /// Returns all 32,768 current water node pressures.
    pub fn water_node_pressure(&self) -> &[u16] {
        &self.water_node_pressure
    }

    /// Returns all 32,768 extra water node pressures.
    pub fn water_node_extra_pressure(&self) -> &[u16] {
        &self.water_node_extra_pressure
    }

    /// Returns all 32,768 sewage node pulse-group indexes.
    pub fn sewage_node_pulse_groups(&self) -> &[u16] {
        &self.sewage_node_pulse_groups
    }

    /// Returns all 32,768 current sewage node pressures.
    pub fn sewage_node_pressure(&self) -> &[u16] {
        &self.sewage_node_pressure
    }

    /// Returns all 32,768 extra sewage node pressures.
    pub fn sewage_node_extra_pressure(&self) -> &[u16] {
        &self.sewage_node_extra_pressure
    }

    /// Returns the version-227 heating node pulse-group indexes.
    pub fn heating_node_pulse_groups(&self) -> Option<&[u16]> {
        self.heating_node_pulse_groups.as_deref()
    }

    /// Returns the version-227 current heating node pressures.
    pub fn heating_node_pressure(&self) -> Option<&[u16]> {
        self.heating_node_pressure.as_deref()
    }

    /// Returns the version-227 extra heating node pressures.
    pub fn heating_node_extra_pressure(&self) -> Option<&[u16]> {
        self.heating_node_extra_pressure.as_deref()
    }

    /// Returns the version-23 node pollution grid.
    pub fn node_pollution(&self) -> Option<&[u8]> {
        self.node_pollution.as_deref()
    }

    /// Returns the full collect-water pressure grid, including old defaults.
    pub fn collect_water_pressure(&self) -> &[u16] {
        &self.collect_water_pressure
    }

    /// Returns the full collect-sewage pressure grid, including old defaults.
    pub fn collect_sewage_pressure(&self) -> &[u16] {
        &self.collect_sewage_pressure
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

    /// Replaces the serialized continuation flag without changing the body
    /// length.
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

    /// Encodes edited pulse groups, counters, and the continuation flag while
    /// preserving all other bytes, including encoded grids, guide references,
    /// and the closing profiler reference. Returns an error if a captured
    /// field range is invalid.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut bytes = self.original_body.clone();
        encode_pulse_groups(
            &mut bytes,
            self.water_pulse_groups_offset,
            &self.water_pulse_groups,
            self.format_version >= COLLECT_WATER_BOOL_VERSION,
            "water",
        )?;
        encode_pulse_groups(
            &mut bytes,
            self.sewage_pulse_groups_offset,
            &self.sewage_pulse_groups,
            self.format_version >= COLLECT_WATER_BOOL_VERSION,
            "sewage",
        )?;
        if let (Some(offset), Some(groups)) = (
            self.heating_pulse_groups_offset,
            self.heating_pulse_groups.as_deref(),
        ) {
            encode_pulse_groups(&mut bytes, offset, groups, false, "heating")?;
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
        bytes
            .get_mut(conductive_end..flag_end)
            .ok_or_else(|| CrpError::InvalidAssetPayload {
                name: "WaterManager+Data".to_owned(),
                offset: conductive_end,
                reason: "continuation flag range is invalid".to_owned(),
            })?
            .copy_from_slice(&[u8::from(self.can_continue)]);
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

fn read_pulse_units(reader: &mut Reader<'_>) -> Result<Vec<WaterPulseUnit>, CrpError> {
    let count = usize::from(reader.read_u16_be()?);
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
    Ok(units)
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
        LEGACY_NODE_COUNT, NODE_COUNT, POLLUTION_VERSION, PULSE_GROUP_CAPACITY,
        PULSE_GROUP_RECORD_LEN, PULSE_UNIT_CAPACITY, WaterManagerData, WaterPulseGroup,
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
        body.extend_from_slice(&0_u16.to_be_bytes()); // No sewage pulse units.
        body.extend_from_slice(&1_u16.to_be_bytes()); // Heating pulse group.
        body.extend_from_slice(&0x6162_6364_u32.to_be_bytes());
        body.extend_from_slice(&0x7172_7374_u32.to_be_bytes());
        body.extend_from_slice(&0x8182_u16.to_be_bytes());
        body.extend_from_slice(&0x8384_u16.to_be_bytes());
        body.extend_from_slice(&0x8586_u16.to_be_bytes());
        body.extend_from_slice(&0_u16.to_be_bytes()); // No heating pulse units.

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
    fn collect_water_bool_gate_copies_current_pressure() {
        let body = active_body(270);
        let data = WaterManagerData::decode(270, &body).unwrap();
        assert_eq!(data.collect_water_pressure()[0], 0x1234);
        assert_eq!(data.collect_water_pressure()[1], 0);
        assert_eq!(
            data.water_pulse_groups()[0].collect_pressure,
            Some(0x1516_1718)
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
