use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    EncodedUIntArrayReader, read_encoded_byte_array, write_encoded_byte_array,
    write_encoded_uint_array,
};
use std::ops::Range;

const CELL_COUNT: usize = 262_144;
const AREA_CELL_COUNT: usize = 81;
const END_PROFILER_REFERENCE_LEN: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
struct EncodedGridSnapshot {
    range: Range<usize>,
    values: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EncodedUIntScopeSnapshot {
    range: Range<usize>,
    arrays: Vec<Vec<u32>>,
}

/// Decoded reader fields from `NaturalResourceManager+Data`.
///
/// Grid columns are exposed in their serialized form. Tree values correspond
/// to nonzero forest cells; fertility values correspond to zero forest cells.
/// The area-resource UInt arrays and used-resource counters retain their
/// serialized order because the Rust API does not assign names to those
/// positional entries. Used-resource counters and pollution-disposal rates
/// can be edited at their version gates. Direct grid columns can be edited by
/// cell while retaining the sparse forest selection and pre-version-22 legacy
/// topology. Sparse tree and fertility values can be edited by their selected
/// cell; the complete source body is retained for all other bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NaturalResourceManagerData {
    format_version: u32,
    ore: Vec<u8>,
    ore_snapshot: EncodedGridSnapshot,
    oil: Vec<u8>,
    oil_snapshot: EncodedGridSnapshot,
    legacy_zero_ore: Option<Vec<u8>>,
    legacy_zero_oil: Option<Vec<u8>>,
    sand: Option<Vec<u8>>,
    sand_snapshot: Option<EncodedGridSnapshot>,
    forest: Vec<u8>,
    forest_snapshot: EncodedGridSnapshot,
    trees: Vec<u8>,
    trees_snapshot: EncodedGridSnapshot,
    fertility: Vec<u8>,
    fertility_snapshot: EncodedGridSnapshot,
    water: Option<Vec<u8>>,
    water_snapshot: Option<EncodedGridSnapshot>,
    shore: Option<Vec<u8>>,
    shore_snapshot: Option<EncodedGridSnapshot>,
    pollution: Vec<u8>,
    pollution_snapshot: EncodedGridSnapshot,
    burned: Option<Vec<u8>>,
    burned_snapshot: Option<EncodedGridSnapshot>,
    destroyed: Option<Vec<u8>>,
    destroyed_snapshot: Option<EncodedGridSnapshot>,
    area_resource_arrays: Vec<Vec<u32>>,
    area_resource_snapshot: Option<EncodedUIntScopeSnapshot>,
    used_resource_counters: Option<Vec<u32>>,
    used_resource_counters_offset: Option<usize>,
    pollution_disposal_rates: Option<[i8; 2]>,
    pollution_disposal_rates_offset: Option<usize>,
    original_body: Vec<u8>,
}

impl NaturalResourceManagerData {
    /// Decodes the complete bounded body, including its two-byte closing
    /// profiler reference. Version-gated fields follow the supplied inner
    /// DataSerializer version. Truncated fields or bytes beyond the closing
    /// reference return errors.
    pub fn decode(format_version: u32, bytes: &[u8]) -> Result<Self, CrpError> {
        let mut reader = Reader::new(bytes);
        let (ore, ore_snapshot) = read_captured_grid(&mut reader, CELL_COUNT)?;
        let (oil, oil_snapshot) = read_captured_grid(&mut reader, CELL_COUNT)?;

        let legacy_zero_ore = if format_version < 22 {
            Some(read_encoded_byte_array(
                &mut reader,
                ore.iter().filter(|value| **value == 0).count(),
            )?)
        } else {
            None
        };
        let legacy_zero_oil = if format_version < 22 {
            Some(read_encoded_byte_array(
                &mut reader,
                oil.iter().filter(|value| **value == 0).count(),
            )?)
        } else {
            None
        };

        let (sand, sand_snapshot) = if format_version >= 27 {
            let (values, snapshot) = read_captured_grid(&mut reader, CELL_COUNT)?;
            (Some(values), Some(snapshot))
        } else {
            (None, None)
        };
        let (forest, forest_snapshot) = read_captured_grid(&mut reader, CELL_COUNT)?;
        let nonzero_forest_count = forest.iter().filter(|value| **value != 0).count();
        let zero_forest_count =
            CELL_COUNT
                .checked_sub(nonzero_forest_count)
                .ok_or(CrpError::ArithmeticOverflow(
                    "NaturalResourceManager fertility count",
                ))?;
        let (trees, trees_snapshot) = read_captured_grid(&mut reader, nonzero_forest_count)?;
        let (fertility, fertility_snapshot) = read_captured_grid(&mut reader, zero_forest_count)?;
        let (water, water_snapshot) = if format_version >= 22 {
            let (values, snapshot) = read_captured_grid(&mut reader, CELL_COUNT)?;
            (Some(values), Some(snapshot))
        } else {
            (None, None)
        };
        let (shore, shore_snapshot) = if format_version >= 223 {
            let (values, snapshot) = read_captured_grid(&mut reader, CELL_COUNT)?;
            (Some(values), Some(snapshot))
        } else {
            (None, None)
        };
        let (pollution, pollution_snapshot) = read_captured_grid(&mut reader, CELL_COUNT)?;
        let (burned, burned_snapshot, destroyed, destroyed_snapshot) = if format_version >= 265 {
            let (burned, burned_snapshot) = read_captured_grid(&mut reader, CELL_COUNT)?;
            let (destroyed, destroyed_snapshot) = read_captured_grid(&mut reader, CELL_COUNT)?;
            (
                Some(burned),
                Some(burned_snapshot),
                Some(destroyed),
                Some(destroyed_snapshot),
            )
        } else {
            (None, None, None, None)
        };

        let area_resource_start = reader.offset();
        let mut area_resource_arrays = Vec::new();
        if format_version >= 111 {
            let array_count = area_resource_array_count(format_version);
            let mut array_reader = EncodedUIntArrayReader::default();
            area_resource_arrays.reserve(array_count);
            for _ in 0..array_count {
                area_resource_arrays.push(array_reader.read_array(&mut reader, AREA_CELL_COUNT)?);
            }
        }
        let area_resource_end = reader.offset();
        let area_resource_snapshot = (format_version >= 111).then(|| EncodedUIntScopeSnapshot {
            range: area_resource_start..area_resource_end,
            arrays: area_resource_arrays.clone(),
        });

        let (used_resource_counters_offset, used_resource_counters) = if format_version >= 116 {
            let offset = reader.offset();
            let mut values = Vec::with_capacity(8);
            for _ in 0..8 {
                values.push(reader.read_u32_be()?);
            }
            (Some(offset), Some(values))
        } else {
            (None, None)
        };
        let (pollution_disposal_rates_offset, pollution_disposal_rates) = if format_version >= 147 {
            let offset = reader.offset();
            (
                Some(offset),
                Some([reader.read_u8()? as i8, reader.read_u8()? as i8]),
            )
        } else {
            (None, None)
        };

        if reader.remaining() != END_PROFILER_REFERENCE_LEN {
            return Err(reader.invalid(format!(
                "NaturalResourceManager fields leave {} bytes before the next manager; expected the two-byte closing profiler reference",
                reader.remaining()
            )));
        }
        reader.read_bytes(END_PROFILER_REFERENCE_LEN)?;

        Ok(Self {
            format_version,
            ore,
            ore_snapshot,
            oil,
            oil_snapshot,
            legacy_zero_ore,
            legacy_zero_oil,
            sand,
            sand_snapshot,
            forest,
            forest_snapshot,
            trees,
            trees_snapshot,
            fertility,
            fertility_snapshot,
            water,
            water_snapshot,
            shore,
            shore_snapshot,
            pollution,
            pollution_snapshot,
            burned,
            burned_snapshot,
            destroyed,
            destroyed_snapshot,
            area_resource_arrays,
            area_resource_snapshot,
            used_resource_counters,
            used_resource_counters_offset,
            pollution_disposal_rates,
            pollution_disposal_rates_offset,
            original_body: bytes.to_vec(),
        })
    }

    /// Returns the inner DataSerializer version used for reader gates.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the 262,144-cell ore column.
    pub fn ore(&self) -> &[u8] {
        &self.ore
    }

    /// Replaces one ore value at its cell index. Before inner version 22 the
    /// zero/nonzero pattern cannot change because it selects a legacy column.
    /// Invalid cell indexes or legacy mask changes return an error.
    pub fn set_ore(&mut self, cell: usize, value: u8) -> Result<(), CrpError> {
        set_grid_cell(
            &mut self.ore,
            &self.ore_snapshot,
            cell,
            value,
            "ore",
            self.format_version < 22,
        )
    }

    /// Returns the 262,144-cell oil column.
    pub fn oil(&self) -> &[u8] {
        &self.oil
    }

    /// Replaces one oil value at its cell index. Before inner version 22 the
    /// zero/nonzero pattern cannot change because it selects a legacy column.
    /// Invalid cell indexes or legacy mask changes return an error.
    pub fn set_oil(&mut self, cell: usize, value: u8) -> Result<(), CrpError> {
        set_grid_cell(
            &mut self.oil,
            &self.oil_snapshot,
            cell,
            value,
            "oil",
            self.format_version < 22,
        )
    }

    /// Returns the pre-version-22 values for cells with zero ore, if present.
    pub fn legacy_zero_ore_values(&self) -> Option<&[u8]> {
        self.legacy_zero_ore.as_deref()
    }

    /// Returns the pre-version-22 values for cells with zero oil, if present.
    pub fn legacy_zero_oil_values(&self) -> Option<&[u8]> {
        self.legacy_zero_oil.as_deref()
    }

    /// Returns the sand column from version 27 onward.
    pub fn sand(&self) -> Option<&[u8]> {
        self.sand.as_deref()
    }

    /// Replaces one sand value from inner version 27 onward. Versions without
    /// the stored sand column return an error.
    pub fn set_sand(&mut self, cell: usize, value: u8) -> Result<(), CrpError> {
        set_versioned_grid_cell(
            &mut self.sand,
            &self.sand_snapshot,
            cell,
            value,
            "sand",
            self.format_version,
            27,
        )
    }

    /// Returns the forest column used to select sparse tree and fertility data.
    pub fn forest(&self) -> &[u8] {
        &self.forest
    }

    /// Replaces one forest value while keeping its zero/nonzero selection
    /// state unchanged. Changing that state would shift the sparse tree and
    /// fertility arrays and therefore returns an error.
    pub fn set_forest(&mut self, cell: usize, value: u8) -> Result<(), CrpError> {
        set_grid_cell(
            &mut self.forest,
            &self.forest_snapshot,
            cell,
            value,
            "forest",
            true,
        )
    }

    /// Returns tree values for cells with nonzero forest, in cell order.
    pub fn trees_for_nonzero_forest(&self) -> &[u8] {
        &self.trees
    }

    /// Replaces the tree value associated with a cell whose forest value is
    /// nonzero. Cells without forest and indexes outside the grid return an
    /// error.
    pub fn set_tree_for_cell(&mut self, cell: usize, value: u8) -> Result<(), CrpError> {
        set_sparse_grid_cell(
            &mut self.trees,
            &self.forest,
            &self.trees_snapshot,
            cell,
            true,
            value,
            "tree",
        )
    }

    /// Returns fertility values for cells with zero forest, in cell order.
    pub fn fertility_for_zero_forest(&self) -> &[u8] {
        &self.fertility
    }

    /// Replaces the fertility value associated with a cell whose forest value
    /// is zero. Cells with forest and indexes outside the grid return an error.
    pub fn set_fertility_for_cell(&mut self, cell: usize, value: u8) -> Result<(), CrpError> {
        set_sparse_grid_cell(
            &mut self.fertility,
            &self.forest,
            &self.fertility_snapshot,
            cell,
            false,
            value,
            "fertility",
        )
    }

    /// Returns the water column from version 22 onward.
    pub fn water(&self) -> Option<&[u8]> {
        self.water.as_deref()
    }

    /// Replaces one water value from inner version 22 onward. Versions without
    /// the stored water column return an error.
    pub fn set_water(&mut self, cell: usize, value: u8) -> Result<(), CrpError> {
        set_versioned_grid_cell(
            &mut self.water,
            &self.water_snapshot,
            cell,
            value,
            "water",
            self.format_version,
            22,
        )
    }

    /// Returns the serialized shore column from version 223 onward.
    ///
    /// Earlier readers derive shore from water, so those versions have no
    /// serialized shore column to return here.
    pub fn shore(&self) -> Option<&[u8]> {
        self.shore.as_deref()
    }

    /// Replaces one serialized shore value from inner version 223 onward.
    /// Earlier versions derive shore from water and return an error.
    pub fn set_shore(&mut self, cell: usize, value: u8) -> Result<(), CrpError> {
        set_versioned_grid_cell(
            &mut self.shore,
            &self.shore_snapshot,
            cell,
            value,
            "shore",
            self.format_version,
            223,
        )
    }

    /// Returns the 262,144-cell pollution column.
    pub fn pollution(&self) -> &[u8] {
        &self.pollution
    }

    /// Replaces one pollution value at its cell index. Invalid cell indexes
    /// return an error.
    pub fn set_pollution(&mut self, cell: usize, value: u8) -> Result<(), CrpError> {
        set_grid_cell(
            &mut self.pollution,
            &self.pollution_snapshot,
            cell,
            value,
            "pollution",
            false,
        )
    }

    /// Returns the burned column from version 265 onward.
    pub fn burned(&self) -> Option<&[u8]> {
        self.burned.as_deref()
    }

    /// Replaces one burned value from inner version 265 onward. Earlier
    /// versions have no stored burned column and return an error.
    pub fn set_burned(&mut self, cell: usize, value: u8) -> Result<(), CrpError> {
        set_versioned_grid_cell(
            &mut self.burned,
            &self.burned_snapshot,
            cell,
            value,
            "burned",
            self.format_version,
            265,
        )
    }

    /// Returns the destroyed column from version 265 onward.
    pub fn destroyed(&self) -> Option<&[u8]> {
        self.destroyed.as_deref()
    }

    /// Replaces one destroyed value from inner version 265 onward. Earlier
    /// versions have no stored destroyed column and return an error.
    pub fn set_destroyed(&mut self, cell: usize, value: u8) -> Result<(), CrpError> {
        set_versioned_grid_cell(
            &mut self.destroyed,
            &self.destroyed_snapshot,
            cell,
            value,
            "destroyed",
            self.format_version,
            265,
        )
    }

    /// Returns the versioned 81-value area-resource UInt arrays in serialized
    /// order. Their positional resource names are not assigned by this API.
    pub fn area_resource_arrays(&self) -> &[Vec<u32>] {
        &self.area_resource_arrays
    }

    /// Replaces a positional value in one 81-cell area-resource UInt array.
    /// Array indexes follow serialized order; versions before inner version
    /// 111 and indexes outside the stored arrays return an error. Encoding an
    /// edit rewrites the complete shared UInt-array scope canonically.
    pub fn set_area_resource_value(
        &mut self,
        array_index: usize,
        cell_index: usize,
        value: u32,
    ) -> Result<(), CrpError> {
        let offset = self
            .area_resource_snapshot
            .as_ref()
            .map_or(0, |snapshot| snapshot.range.start);
        if cell_index >= AREA_CELL_COUNT {
            return Err(invalid_manager_field(
                offset,
                format!(
                    "area-resource cell index {cell_index} is outside the {AREA_CELL_COUNT}-cell array"
                ),
            ));
        }
        let stored_array_count = self.area_resource_arrays.len();
        let Some(array) = self.area_resource_arrays.get_mut(array_index) else {
            return Err(invalid_manager_field(
                offset,
                format!(
                    "area-resource array index {array_index} is outside the {stored_array_count} stored arrays"
                ),
            ));
        };
        let Some(target) = array.get_mut(cell_index) else {
            return Err(invalid_manager_field(
                offset,
                "area-resource array is shorter than 81 values",
            ));
        };
        *target = value;
        Ok(())
    }

    /// Returns the eight direct used-resource counters from version 116.
    pub fn used_resource_counters(&self) -> Option<&[u32]> {
        self.used_resource_counters.as_deref()
    }

    /// Replaces one of the eight serialized used-resource counters from
    /// version 116 onward. Earlier versions return an error because no counter
    /// bytes are stored in their layout.
    pub fn set_used_resource_counter(&mut self, index: usize, value: u32) -> Result<(), CrpError> {
        let body_end = self.body_end_offset()?;
        let offset = self.used_resource_counters_offset.ok_or_else(|| {
            invalid_manager_field(
                body_end,
                "used-resource counters are not serialized before inner version 116",
            )
        })?;
        if index >= 8 {
            return Err(invalid_manager_field(
                field_end(offset, 8, 4)?,
                format!("used-resource counter index {index} is outside the eight stored values"),
            ));
        }
        let counters = self.used_resource_counters.as_mut().ok_or_else(|| {
            invalid_manager_field(offset, "used-resource counter storage is inconsistent")
        })?;
        let counter = counters.get_mut(index).ok_or_else(|| {
            invalid_manager_field(
                offset,
                "used-resource counter storage is shorter than eight values",
            )
        })?;
        *counter = value;
        Ok(())
    }

    /// Returns the two signed pollution-disposal rates from version 147.
    pub fn pollution_disposal_rates(&self) -> Option<[i8; 2]> {
        self.pollution_disposal_rates
    }

    /// Replaces one of the two serialized pollution-disposal rates from
    /// version 147 onward. Earlier versions return an error because these
    /// bytes are not present in their layout.
    pub fn set_pollution_disposal_rate(&mut self, index: usize, value: i8) -> Result<(), CrpError> {
        let body_end = self.body_end_offset()?;
        let offset = self.pollution_disposal_rates_offset.ok_or_else(|| {
            invalid_manager_field(
                body_end,
                "pollution-disposal rates are not serialized before inner version 147",
            )
        })?;
        if index >= 2 {
            return Err(invalid_manager_field(
                field_end(offset, 2, 1)?,
                format!("pollution-disposal rate index {index} is outside the two stored values"),
            ));
        }
        let rates = self.pollution_disposal_rates.as_mut().ok_or_else(|| {
            invalid_manager_field(offset, "pollution-disposal rate storage is inconsistent")
        })?;
        let rate = rates.get_mut(index).ok_or_else(|| {
            invalid_manager_field(
                offset,
                "pollution-disposal rate storage is shorter than two values",
            )
        })?;
        *rate = value;
        Ok(())
    }

    /// Returns the complete source body unchanged, including legacy columns
    /// and the closing profiler reference.
    pub fn encode_unchanged(&self) -> Vec<u8> {
        self.original_body.clone()
    }

    /// Encodes edited direct grid values, used-resource counters, and
    /// pollution-disposal rates. Changed grids use the canonical byte-array
    /// writer and can resize the body. Editing an area-resource value rewrites
    /// the full shared UInt-array scope canonically; other source bytes retain
    /// their original representation.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut bytes = self.original_body.clone();
        match (
            self.used_resource_counters.as_deref(),
            self.used_resource_counters_offset,
        ) {
            (Some(values), Some(offset)) => {
                write_u32_values(&mut bytes, offset, values, "used-resource counters")?;
            }
            (None, None) => {}
            _ => {
                return Err(invalid_manager_field(
                    0,
                    "used-resource counter values and offset do not match",
                ));
            }
        }
        match (
            self.pollution_disposal_rates,
            self.pollution_disposal_rates_offset,
        ) {
            (Some(values), Some(offset)) => {
                write_i8_values(&mut bytes, offset, &values, "pollution-disposal rates")?;
            }
            (None, None) => {}
            _ => {
                return Err(invalid_manager_field(
                    0,
                    "pollution-disposal rate values and offset do not match",
                ));
            }
        }
        let mut replacements = Vec::new();
        let tree_count = self.forest.iter().filter(|value| **value != 0).count();
        let fertility_count =
            CELL_COUNT
                .checked_sub(tree_count)
                .ok_or(CrpError::ArithmeticOverflow(
                    "NaturalResourceManager fertility count",
                ))?;
        if self.forest.len() != CELL_COUNT
            || self.trees.len() != tree_count
            || self.trees_snapshot.values.len() != tree_count
            || self.fertility.len() != fertility_count
            || self.fertility_snapshot.values.len() != fertility_count
        {
            return Err(invalid_manager_field(
                self.forest_snapshot.range.start,
                "forest, tree, and fertility values do not match their sparse layout",
            ));
        }
        push_changed_grid(
            &mut replacements,
            &self.ore,
            &self.ore_snapshot,
            bytes.len(),
            CELL_COUNT,
            "ore",
        )?;
        push_changed_grid(
            &mut replacements,
            &self.oil,
            &self.oil_snapshot,
            bytes.len(),
            CELL_COUNT,
            "oil",
        )?;
        push_changed_grid(
            &mut replacements,
            &self.forest,
            &self.forest_snapshot,
            bytes.len(),
            CELL_COUNT,
            "forest",
        )?;
        push_changed_grid(
            &mut replacements,
            &self.trees,
            &self.trees_snapshot,
            bytes.len(),
            tree_count,
            "tree",
        )?;
        push_changed_grid(
            &mut replacements,
            &self.fertility,
            &self.fertility_snapshot,
            bytes.len(),
            fertility_count,
            "fertility",
        )?;
        push_optional_changed_grid(
            &mut replacements,
            self.sand.as_deref(),
            self.sand_snapshot.as_ref(),
            bytes.len(),
            CELL_COUNT,
            "sand",
        )?;
        push_optional_changed_grid(
            &mut replacements,
            self.water.as_deref(),
            self.water_snapshot.as_ref(),
            bytes.len(),
            CELL_COUNT,
            "water",
        )?;
        push_optional_changed_grid(
            &mut replacements,
            self.shore.as_deref(),
            self.shore_snapshot.as_ref(),
            bytes.len(),
            CELL_COUNT,
            "shore",
        )?;
        push_changed_grid(
            &mut replacements,
            &self.pollution,
            &self.pollution_snapshot,
            bytes.len(),
            CELL_COUNT,
            "pollution",
        )?;
        push_optional_changed_grid(
            &mut replacements,
            self.burned.as_deref(),
            self.burned_snapshot.as_ref(),
            bytes.len(),
            CELL_COUNT,
            "burned",
        )?;
        push_optional_changed_grid(
            &mut replacements,
            self.destroyed.as_deref(),
            self.destroyed_snapshot.as_ref(),
            bytes.len(),
            CELL_COUNT,
            "destroyed",
        )?;
        if let Some(replacement) = changed_area_resource_scope(
            &self.area_resource_arrays,
            self.area_resource_snapshot.as_ref(),
            bytes.len(),
            self.format_version,
        )? {
            replacements.push(replacement);
        }
        replacements.sort_by_key(|(range, _)| range.start);
        for (range, encoded) in replacements.into_iter().rev() {
            bytes.splice(range, encoded);
        }
        Ok(bytes)
    }

    fn body_end_offset(&self) -> Result<usize, CrpError> {
        self.original_body
            .len()
            .checked_sub(END_PROFILER_REFERENCE_LEN)
            .ok_or_else(|| invalid_manager_field(0, "body lacks its closing profiler reference"))
    }
}

fn read_captured_grid(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<(Vec<u8>, EncodedGridSnapshot), CrpError> {
    let start = reader.offset();
    let values = read_encoded_byte_array(reader, count)?;
    let end = reader.offset();
    Ok((
        values.clone(),
        EncodedGridSnapshot {
            range: start..end,
            values,
        },
    ))
}

fn set_grid_cell(
    values: &mut [u8],
    snapshot: &EncodedGridSnapshot,
    cell: usize,
    value: u8,
    field: &str,
    preserve_zero_mask: bool,
) -> Result<(), CrpError> {
    if cell >= CELL_COUNT {
        return Err(invalid_manager_field(
            snapshot.range.start,
            format!("{field} cell index {cell} is outside the {CELL_COUNT}-cell grid"),
        ));
    }
    let Some(target) = values.get_mut(cell) else {
        return Err(invalid_manager_field(
            snapshot.range.start,
            format!("{field} grid is shorter than its {CELL_COUNT}-cell layout"),
        ));
    };
    if preserve_zero_mask && (*target == 0) != (value == 0) {
        return Err(invalid_manager_field(
            snapshot.range.start,
            format!(
                "changing the {field} zero/nonzero mask would alter a sparse or legacy column layout"
            ),
        ));
    }
    *target = value;
    Ok(())
}

fn set_sparse_grid_cell(
    values: &mut [u8],
    selector: &[u8],
    snapshot: &EncodedGridSnapshot,
    cell: usize,
    selected: bool,
    value: u8,
    field: &str,
) -> Result<(), CrpError> {
    if cell >= CELL_COUNT {
        return Err(invalid_manager_field(
            snapshot.range.start,
            format!("{field} cell index {cell} is outside the {CELL_COUNT}-cell grid"),
        ));
    }
    let selector_value = selector.get(cell).ok_or_else(|| {
        invalid_manager_field(
            snapshot.range.start,
            "forest selector is shorter than its 262144-cell layout",
        )
    })?;
    if (*selector_value != 0) != selected {
        return Err(invalid_manager_field(
            snapshot.range.start,
            format!(
                "{field} value is not stored for cell {cell} under the current forest selection"
            ),
        ));
    }
    let sparse_index = if selected {
        selector[..cell].iter().filter(|value| **value != 0).count()
    } else {
        selector[..cell].iter().filter(|value| **value == 0).count()
    };
    let Some(target) = values.get_mut(sparse_index) else {
        return Err(invalid_manager_field(
            snapshot.range.start,
            format!("{field} sparse array is shorter than the forest selection"),
        ));
    };
    *target = value;
    Ok(())
}

fn set_versioned_grid_cell(
    values: &mut Option<Vec<u8>>,
    snapshot: &Option<EncodedGridSnapshot>,
    cell: usize,
    value: u8,
    field: &'static str,
    format_version: u32,
    minimum_version: u32,
) -> Result<(), CrpError> {
    match (values.as_mut(), snapshot.as_ref()) {
        (Some(values), Some(snapshot)) => {
            set_grid_cell(values, snapshot, cell, value, field, false)
        }
        (None, None) => Err(invalid_manager_field(
            0,
            format!("{field} grid is not serialized before inner version {minimum_version}"),
        )),
        _ => Err(invalid_manager_field(
            0,
            format!("{field} grid values and source range do not match version {format_version}"),
        )),
    }
}

fn push_optional_changed_grid(
    replacements: &mut Vec<(Range<usize>, Vec<u8>)>,
    values: Option<&[u8]>,
    snapshot: Option<&EncodedGridSnapshot>,
    source_body_len: usize,
    expected_count: usize,
    field: &'static str,
) -> Result<(), CrpError> {
    match (values, snapshot) {
        (Some(values), Some(snapshot)) => push_changed_grid(
            replacements,
            values,
            snapshot,
            source_body_len,
            expected_count,
            field,
        ),
        (None, None) => Ok(()),
        _ => Err(invalid_manager_field(
            0,
            format!("{field} grid values and source range do not match"),
        )),
    }
}

fn push_changed_grid(
    replacements: &mut Vec<(Range<usize>, Vec<u8>)>,
    values: &[u8],
    snapshot: &EncodedGridSnapshot,
    source_body_len: usize,
    expected_count: usize,
    field: &'static str,
) -> Result<(), CrpError> {
    if snapshot.range.start > snapshot.range.end || snapshot.range.end > source_body_len {
        return Err(invalid_manager_field(
            snapshot.range.start,
            format!("{field} source range exceeds the body"),
        ));
    }
    if values.len() != expected_count || snapshot.values.len() != expected_count {
        return Err(invalid_manager_field(
            snapshot.range.start,
            format!("{field} values do not match the expected {expected_count}-value layout"),
        ));
    }
    if values == snapshot.values {
        return Ok(());
    }
    let mut encoded = Vec::new();
    write_encoded_byte_array(&mut encoded, values);
    replacements.push((snapshot.range.clone(), encoded));
    Ok(())
}

fn changed_area_resource_scope(
    arrays: &[Vec<u32>],
    snapshot: Option<&EncodedUIntScopeSnapshot>,
    source_body_len: usize,
    format_version: u32,
) -> Result<Option<(Range<usize>, Vec<u8>)>, CrpError> {
    let expected_array_count = area_resource_array_count(format_version);
    if arrays.len() != expected_array_count {
        return Err(invalid_manager_field(
            snapshot.map_or(0, |source| source.range.start),
            format!(
                "area-resource array count {} does not match the {expected_array_count} arrays stored by inner version {format_version}",
                arrays.len()
            ),
        ));
    }
    let Some(snapshot) = snapshot else {
        if expected_array_count == 0 {
            return Ok(None);
        }
        return Err(invalid_manager_field(
            0,
            "area-resource arrays lack their source scope",
        ));
    };
    if expected_array_count == 0
        || snapshot.arrays.len() != expected_array_count
        || snapshot.range.start > snapshot.range.end
        || snapshot.range.end > source_body_len
        || arrays
            .iter()
            .chain(snapshot.arrays.iter())
            .any(|array| array.len() != AREA_CELL_COUNT)
    {
        return Err(invalid_manager_field(
            snapshot.range.start,
            "area-resource values do not match their versioned 81-cell UInt layout",
        ));
    }
    if arrays == snapshot.arrays {
        return Ok(None);
    }

    let mut encoded = Vec::new();
    for array in arrays {
        write_encoded_uint_array(&mut encoded, array);
    }
    Ok(Some((snapshot.range.clone(), encoded)))
}

fn area_resource_array_count(format_version: u32) -> usize {
    if format_version >= 125 {
        10
    } else if format_version >= 111 {
        8
    } else {
        0
    }
}

fn invalid_manager_field(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "NaturalResourceManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

fn field_end(offset: usize, count: usize, width: usize) -> Result<usize, CrpError> {
    count
        .checked_mul(width)
        .and_then(|byte_count| offset.checked_add(byte_count))
        .ok_or_else(|| CrpError::ArithmeticOverflow("NaturalResourceManager field end"))
}

fn payload_end(bytes: &[u8], offset: usize) -> Result<usize, CrpError> {
    bytes
        .len()
        .checked_sub(END_PROFILER_REFERENCE_LEN)
        .ok_or_else(|| invalid_manager_field(offset, "body lacks its closing profiler reference"))
}

fn write_u32_values(
    bytes: &mut [u8],
    offset: usize,
    values: &[u32],
    field: &'static str,
) -> Result<(), CrpError> {
    let end = field_end(offset, values.len(), 4)?;
    if end > payload_end(bytes, offset)? {
        return Err(invalid_manager_field(
            offset,
            format!("{field} field overlaps the closing profiler reference"),
        ));
    }
    let output = bytes
        .get_mut(offset..end)
        .ok_or_else(|| invalid_manager_field(offset, format!("{field} byte range is invalid")))?;
    for (chunk, value) in output.chunks_exact_mut(4).zip(values) {
        chunk.copy_from_slice(&value.to_be_bytes());
    }
    Ok(())
}

fn write_i8_values(
    bytes: &mut [u8],
    offset: usize,
    values: &[i8; 2],
    field: &'static str,
) -> Result<(), CrpError> {
    let end = field_end(offset, values.len(), 1)?;
    if end > payload_end(bytes, offset)? {
        return Err(invalid_manager_field(
            offset,
            format!("{field} field overlaps the closing profiler reference"),
        ));
    }
    let output = bytes
        .get_mut(offset..end)
        .ok_or_else(|| invalid_manager_field(offset, format!("{field} byte range is invalid")))?;
    for (byte, value) in output.iter_mut().zip(values) {
        *byte = value.to_be_bytes()[0];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AREA_CELL_COUNT, CELL_COUNT, NaturalResourceManagerData};
    use crate::encoded_array::write_encoded_byte_array;

    fn append_zero_byte_array(output: &mut Vec<u8>, count: usize) {
        if count == 0 {
            return;
        }
        output.extend_from_slice(&[0, 0]); // One literal zero establishes a zero delta.
        let mut remaining = count - 1;
        while remaining > 0 {
            let run = remaining.min(128);
            output.push(0x7f + run as u8);
            remaining -= run;
        }
    }

    fn append_literal_uint_array(output: &mut Vec<u8>, values: &[u32]) {
        for chunk in values.chunks(64) {
            output.push((chunk.len() - 1) as u8);
            for value in chunk {
                output.extend_from_slice(&value.to_be_bytes());
            }
        }
    }

    fn append_constant_uint_array(output: &mut Vec<u8>, count: usize, value: u32) {
        assert!(count >= 2);
        output.push(1); // Two literals establish a zero delta.
        output.extend_from_slice(&value.to_be_bytes());
        output.extend_from_slice(&value.to_be_bytes());
        let mut remaining = count - 2;
        while remaining > 0 {
            let run = remaining.min(128);
            output.push(0x7f + run as u8);
            remaining -= run;
        }
    }

    fn literal_byte_grid_with_value(cell: usize, value: u8) -> Vec<u8> {
        let mut values = vec![0; CELL_COUNT];
        values[cell] = value;
        literal_byte_array(&values)
    }

    fn literal_byte_array(values: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::new();
        let mut start = 0;
        while start < values.len() {
            let end = (start + 128).min(values.len());
            encoded.push((end - start - 1) as u8);
            encoded.extend_from_slice(&values[start..end]);
            start = end;
        }
        encoded
    }

    fn synthetic_body(format_version: u32, active_forest: bool) -> Vec<u8> {
        let mut body = Vec::new();
        append_zero_byte_array(&mut body, CELL_COUNT);
        append_zero_byte_array(&mut body, CELL_COUNT);
        if format_version < 22 {
            append_zero_byte_array(&mut body, CELL_COUNT);
            append_zero_byte_array(&mut body, CELL_COUNT);
        }
        if format_version >= 27 {
            append_zero_byte_array(&mut body, CELL_COUNT);
        }
        let mut forest = vec![0_u8; CELL_COUNT];
        if active_forest {
            forest[1] = 4;
            forest[CELL_COUNT - 1] = 8;
        }
        write_encoded_byte_array(&mut body, &forest);
        let tree_count = forest.iter().filter(|value| **value != 0).count();
        let trees = (0..tree_count)
            .map(|index| 0x31 + index as u8)
            .collect::<Vec<_>>();
        write_encoded_byte_array(&mut body, &trees);
        append_zero_byte_array(&mut body, CELL_COUNT - tree_count);
        if format_version >= 22 {
            append_zero_byte_array(&mut body, CELL_COUNT);
        }
        if format_version >= 223 {
            append_zero_byte_array(&mut body, CELL_COUNT);
        }
        append_zero_byte_array(&mut body, CELL_COUNT);
        if format_version >= 265 {
            append_zero_byte_array(&mut body, CELL_COUNT);
            append_zero_byte_array(&mut body, CELL_COUNT);
        }

        let area_array_count = if format_version >= 125 {
            10
        } else if format_version >= 111 {
            8
        } else {
            0
        };
        for index in 0..area_array_count {
            append_constant_uint_array(&mut body, AREA_CELL_COUNT, index as u32 + 1);
        }
        if format_version >= 116 {
            for index in 0..8_u32 {
                body.extend_from_slice(&(0x1020_3040 + index).to_be_bytes());
            }
        }
        if format_version >= 147 {
            body.extend_from_slice(&[0xfe, 0x03]); // -2 and 3 as signed bytes.
        }
        body.extend_from_slice(&[0x12, 0x34]);
        body
    }

    #[test]
    fn decodes_sparse_grid_area_arrays_and_preserves_complete_body() {
        let body = synthetic_body(118_011, true);
        let data = NaturalResourceManagerData::decode(118_011, &body).unwrap();
        assert_eq!(data.format_version(), 118_011);
        assert_eq!(data.ore().len(), CELL_COUNT);
        assert_eq!(data.oil().len(), CELL_COUNT);
        assert_eq!(data.sand().unwrap().len(), CELL_COUNT);
        assert_eq!(data.forest()[1], 4);
        assert_eq!(data.forest()[CELL_COUNT - 1], 8);
        assert_eq!(data.trees_for_nonzero_forest(), [0x31, 0x32]);
        assert_eq!(data.fertility_for_zero_forest().len(), CELL_COUNT - 2);
        assert_eq!(data.water().unwrap().len(), CELL_COUNT);
        assert_eq!(data.shore().unwrap().len(), CELL_COUNT);
        assert_eq!(data.pollution().len(), CELL_COUNT);
        assert_eq!(data.burned().unwrap().len(), CELL_COUNT);
        assert_eq!(data.destroyed().unwrap().len(), CELL_COUNT);
        assert_eq!(data.area_resource_arrays().len(), 10);
        assert!(
            data.area_resource_arrays()
                .iter()
                .enumerate()
                .all(|(index, values)| values.len() == AREA_CELL_COUNT
                    && values.iter().all(|value| *value == index as u32 + 1))
        );
        assert_eq!(
            data.used_resource_counters().unwrap(),
            [
                0x1020_3040,
                0x1020_3041,
                0x1020_3042,
                0x1020_3043,
                0x1020_3044,
                0x1020_3045,
                0x1020_3046,
                0x1020_3047
            ]
        );
        assert_eq!(data.pollution_disposal_rates(), Some([-2, 3]));
        assert_eq!(data.encode_unchanged(), body);
    }

    #[test]
    fn edits_only_current_scalar_tail_fields_with_expected_endian_bytes() {
        let body = synthetic_body(118_011, false);
        let mut data = NaturalResourceManagerData::decode(118_011, &body).unwrap();
        data.set_used_resource_counter(0, 0xaabb_ccdd).unwrap();
        data.set_used_resource_counter(7, 0x5566_7788).unwrap();
        data.set_pollution_disposal_rate(0, i8::MIN).unwrap();
        data.set_pollution_disposal_rate(1, i8::MAX).unwrap();

        let mut expected = body.clone();
        let counters_start = body.len() - 2 - 2 - 8 * 4;
        let rates_start = body.len() - 2 - 2;
        expected[counters_start..counters_start + 4]
            .copy_from_slice(&0xaabb_ccdd_u32.to_be_bytes());
        expected[counters_start + 7 * 4..counters_start + 8 * 4]
            .copy_from_slice(&0x5566_7788_u32.to_be_bytes());
        expected[rates_start] = 0x80;
        expected[rates_start + 1] = 0x7f;

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        assert_eq!(data.encode_unchanged(), body);
        let decoded = NaturalResourceManagerData::decode(118_011, &encoded).unwrap();
        assert_eq!(decoded.used_resource_counters().unwrap()[0], 0xaabb_ccdd);
        assert_eq!(decoded.used_resource_counters().unwrap()[7], 0x5566_7788);
        assert_eq!(decoded.pollution_disposal_rates(), Some([i8::MIN, i8::MAX]));
    }

    #[test]
    fn direct_grid_edits_use_expected_literal_bytes_and_preserve_other_columns() {
        let body = synthetic_body(265, false);
        let mut data = NaturalResourceManagerData::decode(265, &body).unwrap();
        data.set_ore(3, 0x11).unwrap();
        data.set_oil(17, 0x22).unwrap();
        data.set_sand(31, 0x33).unwrap();
        data.set_water(47, 0x44).unwrap();
        data.set_shore(63, 0x55).unwrap();
        data.set_pollution(79, 0x66).unwrap();
        data.set_burned(95, 0x77).unwrap();
        data.set_destroyed(111, 0x88).unwrap();

        let mut expected = body.clone();
        let mut replacements = vec![
            (
                data.ore_snapshot.range.clone(),
                literal_byte_grid_with_value(3, 0x11),
            ),
            (
                data.oil_snapshot.range.clone(),
                literal_byte_grid_with_value(17, 0x22),
            ),
            (
                data.sand_snapshot.as_ref().unwrap().range.clone(),
                literal_byte_grid_with_value(31, 0x33),
            ),
            (
                data.water_snapshot.as_ref().unwrap().range.clone(),
                literal_byte_grid_with_value(47, 0x44),
            ),
            (
                data.shore_snapshot.as_ref().unwrap().range.clone(),
                literal_byte_grid_with_value(63, 0x55),
            ),
            (
                data.pollution_snapshot.range.clone(),
                literal_byte_grid_with_value(79, 0x66),
            ),
            (
                data.burned_snapshot.as_ref().unwrap().range.clone(),
                literal_byte_grid_with_value(95, 0x77),
            ),
            (
                data.destroyed_snapshot.as_ref().unwrap().range.clone(),
                literal_byte_grid_with_value(111, 0x88),
            ),
        ];
        replacements.sort_by_key(|(range, _)| range.start);
        for (range, encoded) in replacements.into_iter().rev() {
            expected.splice(range, encoded);
        }

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        assert_eq!(data.encode_unchanged(), body);
        let decoded = NaturalResourceManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.ore()[3], 0x11);
        assert_eq!(decoded.oil()[17], 0x22);
        assert_eq!(decoded.sand().unwrap()[31], 0x33);
        assert_eq!(decoded.water().unwrap()[47], 0x44);
        assert_eq!(decoded.shore().unwrap()[63], 0x55);
        assert_eq!(decoded.pollution()[79], 0x66);
        assert_eq!(decoded.burned().unwrap()[95], 0x77);
        assert_eq!(decoded.destroyed().unwrap()[111], 0x88);

        assert!(data.set_ore(CELL_COUNT, 1).is_err());
        assert_eq!(data.encode().unwrap(), encoded);
    }

    #[test]
    fn forest_selection_and_sparse_values_edit_by_cell_without_changing_topology() {
        let body = synthetic_body(265, true);
        let mut data = NaturalResourceManagerData::decode(265, &body).unwrap();
        data.set_forest(1, 5).unwrap();
        data.set_tree_for_cell(1, 0x41).unwrap();
        data.set_fertility_for_cell(0, 0x51).unwrap();
        assert!(data.set_forest(0, 1).is_err());
        assert!(data.set_tree_for_cell(0, 1).is_err());
        assert!(data.set_fertility_for_cell(1, 1).is_err());
        assert!(data.set_tree_for_cell(CELL_COUNT, 1).is_err());

        let mut forest_values = data.forest_snapshot.values.clone();
        forest_values[1] = 5;
        let mut tree_values = data.trees_snapshot.values.clone();
        tree_values[0] = 0x41;
        let mut fertility_values = data.fertility_snapshot.values.clone();
        fertility_values[0] = 0x51;
        let mut expected = body.clone();
        let mut replacements = vec![
            (
                data.forest_snapshot.range.clone(),
                literal_byte_array(&forest_values),
            ),
            (
                data.trees_snapshot.range.clone(),
                literal_byte_array(&tree_values),
            ),
            (
                data.fertility_snapshot.range.clone(),
                literal_byte_array(&fertility_values),
            ),
        ];
        replacements.sort_by_key(|(range, _)| range.start);
        for (range, encoded) in replacements.into_iter().rev() {
            expected.splice(range, encoded);
        }

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        let decoded = NaturalResourceManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.forest()[1], 5);
        assert_eq!(decoded.trees_for_nonzero_forest(), [0x41, 0x32]);
        assert_eq!(decoded.fertility_for_zero_forest()[0], 0x51);
    }

    #[test]
    fn area_resource_edits_rewrite_the_shared_uint_scope_with_expected_bytes() {
        let body = synthetic_body(265, false);
        let mut data = NaturalResourceManagerData::decode(265, &body).unwrap();
        assert_eq!(data.area_resource_arrays().len(), 10);
        data.set_area_resource_value(2, 5, 0x1122_3344).unwrap();
        data.set_area_resource_value(9, 80, u32::MAX).unwrap();
        assert!(data.set_area_resource_value(10, 0, 1).is_err());
        assert!(data.set_area_resource_value(0, 81, 1).is_err());

        let snapshot = data.area_resource_snapshot.as_ref().unwrap();
        let mut expected_scope = Vec::new();
        for values in &data.area_resource_arrays {
            append_literal_uint_array(&mut expected_scope, values);
        }
        let mut expected = body.clone();
        expected.splice(snapshot.range.clone(), expected_scope);

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        assert!(encoded.len() > body.len());
        let decoded = NaturalResourceManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.area_resource_arrays()[2][5], 0x1122_3344);
        assert_eq!(decoded.area_resource_arrays()[9][80], u32::MAX);
        assert_eq!(decoded.area_resource_arrays()[3][5], 4);

        let mut before_gate =
            NaturalResourceManagerData::decode(110, &synthetic_body(110, false)).unwrap();
        assert!(before_gate.area_resource_arrays().is_empty());
        assert!(before_gate.set_area_resource_value(0, 0, 1).is_err());

        let mut version_111 =
            NaturalResourceManagerData::decode(111, &synthetic_body(111, false)).unwrap();
        assert_eq!(version_111.area_resource_arrays().len(), 8);
        version_111
            .set_area_resource_value(7, 80, 0xaabb_ccdd)
            .unwrap();
        assert!(version_111.set_area_resource_value(8, 0, 1).is_err());
        let decoded_111 =
            NaturalResourceManagerData::decode(111, &version_111.encode().unwrap()).unwrap();
        assert_eq!(decoded_111.area_resource_arrays()[7][80], 0xaabb_ccdd);

        let mut version_125 =
            NaturalResourceManagerData::decode(125, &synthetic_body(125, false)).unwrap();
        assert_eq!(version_125.area_resource_arrays().len(), 10);
        version_125
            .set_area_resource_value(9, 80, 0x5566_7788)
            .unwrap();
        let decoded_125 =
            NaturalResourceManagerData::decode(125, &version_125.encode().unwrap()).unwrap();
        assert_eq!(decoded_125.area_resource_arrays()[9][80], 0x5566_7788);
    }

    #[test]
    fn direct_grid_editors_follow_version_gates_and_legacy_masks() {
        let legacy_body = synthetic_body(21, false);
        let mut legacy = NaturalResourceManagerData::decode(21, &legacy_body).unwrap();
        assert!(legacy.set_ore(0, 1).is_err());
        assert!(legacy.set_oil(0, 1).is_err());
        assert!(legacy.set_sand(0, 1).is_err());
        assert!(legacy.set_water(0, 1).is_err());
        assert!(legacy.set_shore(0, 1).is_err());
        assert!(legacy.set_burned(0, 1).is_err());
        assert!(legacy.set_destroyed(0, 1).is_err());
        assert_eq!(legacy.encode().unwrap(), legacy_body);

        let mut version_22 =
            NaturalResourceManagerData::decode(22, &synthetic_body(22, false)).unwrap();
        version_22.set_ore(0, 1).unwrap();
        version_22.set_oil(1, 2).unwrap();
        version_22.set_water(2, 3).unwrap();
        assert!(version_22.set_sand(0, 1).is_err());
        assert!(version_22.set_shore(0, 1).is_err());
        assert!(version_22.set_burned(0, 1).is_err());
        assert!(version_22.set_destroyed(0, 1).is_err());
        let encoded = version_22.encode().unwrap();
        let decoded = NaturalResourceManagerData::decode(22, &encoded).unwrap();
        assert_eq!(decoded.ore()[0], 1);
        assert_eq!(decoded.oil()[1], 2);
        assert_eq!(decoded.water().unwrap()[2], 3);
    }

    #[test]
    fn scalar_editors_follow_version_gates_and_fixed_array_limits() {
        let body_before_counters = synthetic_body(115, false);
        let mut before_counters =
            NaturalResourceManagerData::decode(115, &body_before_counters).unwrap();
        assert!(before_counters.set_used_resource_counter(0, 1).is_err());
        assert!(before_counters.set_pollution_disposal_rate(0, 1).is_err());

        let body_before_rates = synthetic_body(116, false);
        let mut before_rates = NaturalResourceManagerData::decode(116, &body_before_rates).unwrap();
        before_rates.set_used_resource_counter(7, u32::MAX).unwrap();
        assert!(before_rates.set_used_resource_counter(8, 1).is_err());
        assert!(before_rates.set_pollution_disposal_rate(0, 1).is_err());
        assert_eq!(
            NaturalResourceManagerData::decode(116, &before_rates.encode().unwrap())
                .unwrap()
                .used_resource_counters()
                .unwrap()[7],
            u32::MAX
        );
    }

    #[test]
    fn reader_version_gates_cover_legacy_and_current_columns() {
        for (version, legacy, sand, water, area_arrays, counters, rates, shore, burned) in [
            (21, true, false, false, 0, false, false, false, false),
            (22, false, false, true, 0, false, false, false, false),
            (27, false, true, true, 0, false, false, false, false),
            (110, false, true, true, 0, false, false, false, false),
            (111, false, true, true, 8, false, false, false, false),
            (115, false, true, true, 8, false, false, false, false),
            (116, false, true, true, 8, true, false, false, false),
            (124, false, true, true, 8, true, false, false, false),
            (125, false, true, true, 10, true, false, false, false),
            (146, false, true, true, 10, true, false, false, false),
            (147, false, true, true, 10, true, true, false, false),
            (222, false, true, true, 10, true, true, false, false),
            (223, false, true, true, 10, true, true, true, false),
            (264, false, true, true, 10, true, true, true, false),
            (265, false, true, true, 10, true, true, true, true),
        ] {
            let body = synthetic_body(version, false);
            let data = NaturalResourceManagerData::decode(version, &body).unwrap();
            assert_eq!(
                data.legacy_zero_ore_values().is_some(),
                legacy,
                "version {version}"
            );
            assert_eq!(
                data.legacy_zero_oil_values().is_some(),
                legacy,
                "version {version}"
            );
            assert_eq!(data.sand().is_some(), sand, "version {version}");
            assert_eq!(data.water().is_some(), water, "version {version}");
            assert_eq!(
                data.area_resource_arrays().len(),
                area_arrays,
                "version {version}"
            );
            assert_eq!(
                data.used_resource_counters().is_some(),
                counters,
                "version {version}"
            );
            assert_eq!(
                data.pollution_disposal_rates().is_some(),
                rates,
                "version {version}"
            );
            assert_eq!(data.shore().is_some(), shore, "version {version}");
            assert_eq!(data.burned().is_some(), burned, "version {version}");
            assert_eq!(data.destroyed().is_some(), burned, "version {version}");
            assert_eq!(data.encode_unchanged(), body, "version {version}");
        }
    }

    #[test]
    fn rejects_truncated_and_overlong_manager_bodies() {
        let body = synthetic_body(265, false);
        assert!(NaturalResourceManagerData::decode(265, &body[..body.len() - 1]).is_err());
        let mut overlong = body;
        overlong.push(0);
        assert!(NaturalResourceManagerData::decode(265, &overlong).is_err());
    }
}
