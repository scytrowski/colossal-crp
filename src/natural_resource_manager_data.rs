use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{EncodedUIntArrayReader, read_encoded_byte_array};

const CELL_COUNT: usize = 262_144;
const AREA_CELL_COUNT: usize = 81;
const END_PROFILER_REFERENCE_LEN: usize = 2;

/// Decoded reader fields from `NaturalResourceManager+Data`.
///
/// Grid columns are exposed in their serialized form. Tree values correspond
/// to nonzero forest cells; fertility values correspond to zero forest cells.
/// The area-resource UInt arrays and used-resource counters retain their
/// serialized order because the Rust API does not assign names to those
/// positional entries. Used-resource counters and pollution-disposal rates
/// can be edited at their version gates; the complete source body is retained
/// so other bytes, including legacy fields, remain unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NaturalResourceManagerData {
    format_version: u32,
    ore: Vec<u8>,
    oil: Vec<u8>,
    legacy_zero_ore: Option<Vec<u8>>,
    legacy_zero_oil: Option<Vec<u8>>,
    sand: Option<Vec<u8>>,
    forest: Vec<u8>,
    trees: Vec<u8>,
    fertility: Vec<u8>,
    water: Option<Vec<u8>>,
    shore: Option<Vec<u8>>,
    pollution: Vec<u8>,
    burned: Option<Vec<u8>>,
    destroyed: Option<Vec<u8>>,
    area_resource_arrays: Vec<Vec<u32>>,
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
        let ore = read_encoded_byte_array(&mut reader, CELL_COUNT)?;
        let oil = read_encoded_byte_array(&mut reader, CELL_COUNT)?;

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

        let sand = if format_version >= 27 {
            Some(read_encoded_byte_array(&mut reader, CELL_COUNT)?)
        } else {
            None
        };
        let forest = read_encoded_byte_array(&mut reader, CELL_COUNT)?;
        let nonzero_forest_count = forest.iter().filter(|value| **value != 0).count();
        let zero_forest_count =
            CELL_COUNT
                .checked_sub(nonzero_forest_count)
                .ok_or(CrpError::ArithmeticOverflow(
                    "NaturalResourceManager fertility count",
                ))?;
        let trees = read_encoded_byte_array(&mut reader, nonzero_forest_count)?;
        let fertility = read_encoded_byte_array(&mut reader, zero_forest_count)?;
        let water = if format_version >= 22 {
            Some(read_encoded_byte_array(&mut reader, CELL_COUNT)?)
        } else {
            None
        };
        let shore = if format_version >= 223 {
            Some(read_encoded_byte_array(&mut reader, CELL_COUNT)?)
        } else {
            None
        };
        let pollution = read_encoded_byte_array(&mut reader, CELL_COUNT)?;
        let (burned, destroyed) = if format_version >= 265 {
            (
                Some(read_encoded_byte_array(&mut reader, CELL_COUNT)?),
                Some(read_encoded_byte_array(&mut reader, CELL_COUNT)?),
            )
        } else {
            (None, None)
        };

        let mut area_resource_arrays = Vec::new();
        if format_version >= 111 {
            let array_count = if format_version >= 125 { 10 } else { 8 };
            let mut array_reader = EncodedUIntArrayReader::default();
            area_resource_arrays.reserve(array_count);
            for _ in 0..array_count {
                area_resource_arrays.push(array_reader.read_array(&mut reader, AREA_CELL_COUNT)?);
            }
        }

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
            oil,
            legacy_zero_ore,
            legacy_zero_oil,
            sand,
            forest,
            trees,
            fertility,
            water,
            shore,
            pollution,
            burned,
            destroyed,
            area_resource_arrays,
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

    /// Returns the 262,144-cell oil column.
    pub fn oil(&self) -> &[u8] {
        &self.oil
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

    /// Returns the forest column used to select sparse tree and fertility data.
    pub fn forest(&self) -> &[u8] {
        &self.forest
    }

    /// Returns tree values for cells with nonzero forest, in cell order.
    pub fn trees_for_nonzero_forest(&self) -> &[u8] {
        &self.trees
    }

    /// Returns fertility values for cells with zero forest, in cell order.
    pub fn fertility_for_zero_forest(&self) -> &[u8] {
        &self.fertility
    }

    /// Returns the water column from version 22 onward.
    pub fn water(&self) -> Option<&[u8]> {
        self.water.as_deref()
    }

    /// Returns the serialized shore column from version 223 onward.
    ///
    /// Earlier readers derive shore from water, so those versions have no
    /// serialized shore column to return here.
    pub fn shore(&self) -> Option<&[u8]> {
        self.shore.as_deref()
    }

    /// Returns the 262,144-cell pollution column.
    pub fn pollution(&self) -> &[u8] {
        &self.pollution
    }

    /// Returns the burned column from version 265 onward.
    pub fn burned(&self) -> Option<&[u8]> {
        self.burned.as_deref()
    }

    /// Returns the destroyed column from version 265 onward.
    pub fn destroyed(&self) -> Option<&[u8]> {
        self.destroyed.as_deref()
    }

    /// Returns the versioned 81-value area-resource UInt arrays in serialized
    /// order. Their positional resource names are not assigned by this API.
    pub fn area_resource_arrays(&self) -> &[Vec<u32>] {
        &self.area_resource_arrays
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

    /// Encodes the edited used-resource counters and pollution-disposal rates
    /// at their existing fixed-width offsets while preserving every other
    /// body byte, including encoded grids and the closing profiler reference.
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
        Ok(bytes)
    }

    fn body_end_offset(&self) -> Result<usize, CrpError> {
        self.original_body
            .len()
            .checked_sub(END_PROFILER_REFERENCE_LEN)
            .ok_or_else(|| invalid_manager_field(0, "body lacks its closing profiler reference"))
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
            let values = vec![index as u32 + 1; AREA_CELL_COUNT];
            append_literal_uint_array(&mut body, &values);
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
