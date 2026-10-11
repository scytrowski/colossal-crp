use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_ushort_array, write_encoded_ushort_array};
use std::ops::Range;

const CELL_COUNT: usize = 65_536;
const END_PROFILER_REFERENCE_LEN: usize = 2;
const I32_MULTIPLIER_VERSION: u32 = 242;
const FULL_TOTALS_VERSION: u32 = 147;

#[derive(Debug, Clone, PartialEq, Eq)]
struct EncodedArraySnapshot<T> {
    range: Range<usize>,
    values: Vec<T>,
}

/// Decoded reader fields from `ImmaterialResourceManager+Data`.
///
/// The final and temporary local-resource arrays are flat, cell-major
/// sequences. Each cell occupies `resource_slots_per_cell` values in those
/// vectors. The five aggregate arrays retain their serialized positional
/// order. Their serialized prefixes can be edited by index; local grids and
/// every other source byte remain unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmaterialResourceManagerData {
    format_version: u32,
    resource_slots_per_cell: usize,
    final_resources: Vec<u16>,
    final_resources_snapshot: EncodedArraySnapshot<u16>,
    temporary_resources: Vec<u16>,
    temporary_resources_snapshot: EncodedArraySnapshot<u16>,
    total_final: Vec<i32>,
    total_temporary: Vec<i32>,
    total_temporary_multiplier: Vec<i64>,
    global_final: Vec<i32>,
    global_temporary: Vec<i32>,
    aggregate_read_count: usize,
    total_final_offset: usize,
    total_temporary_offset: usize,
    total_temporary_multiplier_offset: usize,
    global_final_offset: Option<usize>,
    global_temporary_offset: Option<usize>,
    original_body: Vec<u8>,
}

impl ImmaterialResourceManagerData {
    /// Decodes the complete bounded body, including its two-byte closing
    /// profiler reference. Local grid lengths and aggregate-array gates follow
    /// the inner DataSerializer version. Truncated fields or extra bytes
    /// return errors.
    pub fn decode(format_version: u32, bytes: &[u8]) -> Result<Self, CrpError> {
        let mut reader = Reader::new(bytes);
        let resource_slots_per_cell = resource_slot_count(format_version);
        let local_value_count =
            CELL_COUNT
                .checked_mul(resource_slots_per_cell)
                .ok_or(CrpError::ArithmeticOverflow(
                    "ImmaterialResourceManager local array length",
                ))?;
        let (final_resources, final_resources_snapshot) =
            read_captured_ushort_array(&mut reader, local_value_count)?;
        let (temporary_resources, temporary_resources_snapshot) =
            read_captured_ushort_array(&mut reader, local_value_count)?;

        let aggregate_read_count = if format_version >= FULL_TOTALS_VERSION {
            resource_slots_per_cell
        } else {
            17
        };
        let mut total_final = vec![0; resource_slots_per_cell];
        let total_final_offset = reader.offset();
        read_i32_prefix(&mut reader, &mut total_final, aggregate_read_count)?;

        let mut total_temporary = vec![0; resource_slots_per_cell];
        let total_temporary_offset = reader.offset();
        read_i32_prefix(&mut reader, &mut total_temporary, aggregate_read_count)?;

        let mut total_temporary_multiplier = vec![0_i64; resource_slots_per_cell];
        let total_temporary_multiplier_offset = reader.offset();
        if format_version >= I32_MULTIPLIER_VERSION {
            for value in total_temporary_multiplier
                .iter_mut()
                .take(aggregate_read_count)
            {
                *value = reader.read_i64_be()?;
            }
        } else {
            for value in total_temporary_multiplier
                .iter_mut()
                .take(aggregate_read_count)
            {
                *value = i64::from(reader.read_i32_be()?);
            }
        }

        let mut global_final = vec![0; resource_slots_per_cell];
        let mut global_temporary = vec![0; resource_slots_per_cell];
        let (global_final_offset, global_temporary_offset) =
            if format_version >= FULL_TOTALS_VERSION {
                let global_final_offset = reader.offset();
                read_i32_prefix(&mut reader, &mut global_final, resource_slots_per_cell)?;
                let global_temporary_offset = reader.offset();
                read_i32_prefix(&mut reader, &mut global_temporary, resource_slots_per_cell)?;
                (Some(global_final_offset), Some(global_temporary_offset))
            } else {
                (None, None)
            };

        if reader.remaining() != END_PROFILER_REFERENCE_LEN {
            return Err(reader.invalid(format!(
                "ImmaterialResourceManager fields leave {} bytes before the next manager; expected the two-byte closing profiler reference",
                reader.remaining()
            )));
        }
        reader.read_bytes(END_PROFILER_REFERENCE_LEN)?;

        Ok(Self {
            format_version,
            resource_slots_per_cell,
            final_resources,
            final_resources_snapshot,
            temporary_resources,
            temporary_resources_snapshot,
            total_final,
            total_temporary,
            total_temporary_multiplier,
            global_final,
            global_temporary,
            aggregate_read_count,
            total_final_offset,
            total_temporary_offset,
            total_temporary_multiplier_offset,
            global_final_offset,
            global_temporary_offset,
            original_body: bytes.to_vec(),
        })
    }

    /// Returns the inner DataSerializer version used for reader gates.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the number of resource slots serialized for each local cell.
    pub fn resource_slots_per_cell(&self) -> usize {
        self.resource_slots_per_cell
    }

    /// Returns the final-resource grid in cell-major order.
    pub fn final_resources(&self) -> &[u16] {
        &self.final_resources
    }

    /// Replaces one serialized final-resource value at a zero-based cell and
    /// resource-slot index. The slot must exist in this inner version; an
    /// out-of-range cell or slot returns an error.
    pub fn set_final_resource(
        &mut self,
        cell_index: usize,
        slot_index: usize,
        value: u16,
    ) -> Result<(), CrpError> {
        set_local_resource_value(
            &mut self.final_resources,
            self.final_resources_snapshot.range.start,
            self.resource_slots_per_cell,
            cell_index,
            slot_index,
            value,
            "final-resource",
        )
    }

    /// Returns the temporary-resource grid in cell-major order.
    pub fn temporary_resources(&self) -> &[u16] {
        &self.temporary_resources
    }

    /// Replaces one serialized temporary-resource value at a zero-based cell
    /// and resource-slot index. The slot must exist in this inner version; an
    /// out-of-range cell or slot returns an error.
    pub fn set_temporary_resource(
        &mut self,
        cell_index: usize,
        slot_index: usize,
        value: u16,
    ) -> Result<(), CrpError> {
        set_local_resource_value(
            &mut self.temporary_resources,
            self.temporary_resources_snapshot.range.start,
            self.resource_slots_per_cell,
            cell_index,
            slot_index,
            value,
            "temporary-resource",
        )
    }

    /// Returns total-final values in resource-slot order. On versions below
    /// 147, entries after the first 17 are initialized to zero by the reader.
    pub fn total_final(&self) -> &[i32] {
        &self.total_final
    }

    /// Replaces one serialized total-final value.
    ///
    /// Versions below 147 serialize only the first 17 entries; indexes beyond
    /// that stored prefix return an error even if the reader exposes defaults.
    pub fn set_total_final(&mut self, index: usize, value: i32) -> Result<(), CrpError> {
        require_stored_aggregate_index(
            index,
            self.aggregate_read_count,
            self.total_final_offset,
            4,
            "total-final",
        )?;
        self.total_final[index] = value;
        Ok(())
    }

    /// Returns total-temporary values in resource-slot order. On versions
    /// below 147, entries after the first 17 are initialized to zero.
    pub fn total_temporary(&self) -> &[i32] {
        &self.total_temporary
    }

    /// Replaces one serialized total-temporary value.
    pub fn set_total_temporary(&mut self, index: usize, value: i32) -> Result<(), CrpError> {
        require_stored_aggregate_index(
            index,
            self.aggregate_read_count,
            self.total_temporary_offset,
            4,
            "total-temporary",
        )?;
        self.total_temporary[index] = value;
        Ok(())
    }

    /// Returns temporary multipliers as signed 64-bit values. Their on-disk
    /// width is four bytes before version 242 and eight bytes thereafter.
    pub fn total_temporary_multiplier(&self) -> &[i64] {
        &self.total_temporary_multiplier
    }

    /// Replaces one serialized temporary multiplier.
    ///
    /// Before inner version 242, the on-disk field is a signed `i32`; values
    /// outside that range return an error instead of being truncated.
    pub fn set_total_temporary_multiplier(
        &mut self,
        index: usize,
        value: i64,
    ) -> Result<(), CrpError> {
        let width = self.multiplier_width_bytes();
        require_stored_aggregate_index(
            index,
            self.aggregate_read_count,
            self.total_temporary_multiplier_offset,
            width,
            "total-temporary multiplier",
        )?;
        let value_offset =
            aggregate_element_offset(self.total_temporary_multiplier_offset, index, width)?;
        if width == 4 && i32::try_from(value).is_err() {
            return Err(invalid_aggregate_value(
                value_offset,
                "total-temporary multiplier does not fit its serialized i32 field",
            ));
        }
        self.total_temporary_multiplier[index] = value;
        Ok(())
    }

    /// Returns the serialized multiplier width in bytes.
    pub fn multiplier_width_bytes(&self) -> usize {
        if self.format_version >= I32_MULTIPLIER_VERSION {
            8
        } else {
            4
        }
    }

    /// Returns global-final values in resource-slot order. Versions below 147
    /// initialize this array to zero without reading it from the stream.
    pub fn global_final(&self) -> &[i32] {
        &self.global_final
    }

    /// Replaces one serialized global-final value.
    ///
    /// This field is present from inner version 147. Earlier versions return an
    /// error because the reader synthesizes this array as zeroes.
    pub fn set_global_final(&mut self, index: usize, value: i32) -> Result<(), CrpError> {
        let end_offset = self
            .original_body
            .len()
            .checked_sub(END_PROFILER_REFERENCE_LEN)
            .ok_or_else(|| {
                invalid_aggregate_value(0, "body lacks its closing profiler reference")
            })?;
        let offset = self.global_final_offset.ok_or_else(|| {
            invalid_aggregate_value(
                end_offset,
                "global-final values are not serialized before inner version 147",
            )
        })?;
        require_stored_aggregate_index(
            index,
            self.resource_slots_per_cell,
            offset,
            4,
            "global-final",
        )?;
        self.global_final[index] = value;
        Ok(())
    }

    /// Returns global-temporary values in resource-slot order. Versions below
    /// 147 initialize this array to zero without reading it from the stream.
    pub fn global_temporary(&self) -> &[i32] {
        &self.global_temporary
    }

    /// Replaces one serialized global-temporary value.
    ///
    /// This field is present from inner version 147. Earlier versions return an
    /// error because the reader synthesizes this array as zeroes.
    pub fn set_global_temporary(&mut self, index: usize, value: i32) -> Result<(), CrpError> {
        let end_offset = self
            .original_body
            .len()
            .checked_sub(END_PROFILER_REFERENCE_LEN)
            .ok_or_else(|| {
                invalid_aggregate_value(0, "body lacks its closing profiler reference")
            })?;
        let offset = self.global_temporary_offset.ok_or_else(|| {
            invalid_aggregate_value(
                end_offset,
                "global-temporary values are not serialized before inner version 147",
            )
        })?;
        require_stored_aggregate_index(
            index,
            self.resource_slots_per_cell,
            offset,
            4,
            "global-temporary",
        )?;
        self.global_temporary[index] = value;
        Ok(())
    }

    /// Returns the complete source body unchanged, including the closing
    /// profiler reference.
    pub fn encode_unchanged(&self) -> Vec<u8> {
        self.original_body.clone()
    }

    /// Encodes edited local-resource arrays and aggregate values. Changed
    /// arrays use the canonical encoded-array writer and can change the body
    /// length; unchanged arrays and the closing profiler reference retain
    /// their original bytes.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut bytes = self.original_body.clone();
        write_i32_values(
            &mut bytes,
            self.total_final_offset,
            &self.total_final,
            self.aggregate_read_count,
            "total-final",
        )?;
        write_i32_values(
            &mut bytes,
            self.total_temporary_offset,
            &self.total_temporary,
            self.aggregate_read_count,
            "total-temporary",
        )?;
        write_multiplier_values(
            &mut bytes,
            self.total_temporary_multiplier_offset,
            &self.total_temporary_multiplier,
            self.aggregate_read_count,
            self.multiplier_width_bytes(),
        )?;
        if let (Some(global_final_offset), Some(global_temporary_offset)) =
            (self.global_final_offset, self.global_temporary_offset)
        {
            write_i32_values(
                &mut bytes,
                global_final_offset,
                &self.global_final,
                self.resource_slots_per_cell,
                "global-final",
            )?;
            write_i32_values(
                &mut bytes,
                global_temporary_offset,
                &self.global_temporary,
                self.resource_slots_per_cell,
                "global-temporary",
            )?;
        }
        let expected_local_count = CELL_COUNT.checked_mul(self.resource_slots_per_cell).ok_or(
            CrpError::ArithmeticOverflow("ImmaterialResourceManager local array length"),
        )?;
        if self.resource_slots_per_cell != resource_slot_count(self.format_version)
            || self.final_resources.len() != expected_local_count
            || self.temporary_resources.len() != expected_local_count
            || self.final_resources_snapshot.values.len() != expected_local_count
            || self.temporary_resources_snapshot.values.len() != expected_local_count
        {
            return Err(invalid_aggregate_value(
                0,
                "local-resource array lengths do not match the versioned slot count",
            ));
        }
        let mut replacements = Vec::new();
        if let Some(replacement) = changed_ushort_array(
            &self.final_resources,
            &self.final_resources_snapshot,
            bytes.len(),
            "final-resource grid",
        )? {
            replacements.push(replacement);
        }
        if let Some(replacement) = changed_ushort_array(
            &self.temporary_resources,
            &self.temporary_resources_snapshot,
            bytes.len(),
            "temporary-resource grid",
        )? {
            replacements.push(replacement);
        }
        replacements.sort_by_key(|(range, _)| range.start);
        for (range, encoded) in replacements.into_iter().rev() {
            bytes.splice(range, encoded);
        }
        Ok(bytes)
    }
}

fn set_local_resource_value(
    values: &mut [u16],
    field_offset: usize,
    slots_per_cell: usize,
    cell_index: usize,
    slot_index: usize,
    value: u16,
    field: &str,
) -> Result<(), CrpError> {
    if cell_index >= CELL_COUNT {
        return Err(invalid_aggregate_value(
            field_offset,
            format!("{field} cell index {cell_index} is outside the {CELL_COUNT}-cell grid"),
        ));
    }
    if slot_index >= slots_per_cell {
        return Err(invalid_aggregate_value(
            field_offset,
            format!(
                "{field} slot index {slot_index} is outside the {slots_per_cell} slots in this version"
            ),
        ));
    }
    let index = cell_index
        .checked_mul(slots_per_cell)
        .and_then(|start| start.checked_add(slot_index))
        .ok_or(CrpError::ArithmeticOverflow(
            "ImmaterialResourceManager local resource index",
        ))?;
    let Some(target) = values.get_mut(index) else {
        return Err(invalid_aggregate_value(
            field_offset,
            format!("{field} local array is shorter than its versioned layout"),
        ));
    };
    *target = value;
    Ok(())
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

fn checked_source_range(
    snapshot: &EncodedArraySnapshot<impl Sized>,
    source_body_len: usize,
    field: &str,
) -> Result<(), CrpError> {
    if snapshot.range.start <= snapshot.range.end && snapshot.range.end <= source_body_len {
        Ok(())
    } else {
        Err(invalid_aggregate_value(
            snapshot.range.start,
            format!("{field} source range exceeds the body"),
        ))
    }
}

fn changed_ushort_array(
    values: &[u16],
    snapshot: &EncodedArraySnapshot<u16>,
    source_body_len: usize,
    field: &str,
) -> Result<Option<(Range<usize>, Vec<u8>)>, CrpError> {
    checked_source_range(snapshot, source_body_len, field)?;
    if values.len() != snapshot.values.len() {
        return Err(invalid_aggregate_value(
            snapshot.range.start,
            format!("{field} value count changed"),
        ));
    }
    if values == snapshot.values {
        return Ok(None);
    }
    let mut encoded = Vec::new();
    write_encoded_ushort_array(&mut encoded, values);
    Ok(Some((snapshot.range.clone(), encoded)))
}

fn require_stored_aggregate_index(
    index: usize,
    count: usize,
    offset: usize,
    width: usize,
    field: &'static str,
) -> Result<(), CrpError> {
    if index >= count {
        let field_end = count
            .checked_mul(width)
            .and_then(|length| offset.checked_add(length))
            .ok_or(CrpError::ArithmeticOverflow(
                "ImmaterialResourceManager aggregate field end",
            ))?;
        return Err(invalid_aggregate_value(
            field_end,
            format!("{field} index {index} is outside the {count}-value serialized prefix"),
        ));
    }
    Ok(())
}

fn aggregate_element_offset(base: usize, index: usize, width: usize) -> Result<usize, CrpError> {
    index
        .checked_mul(width)
        .and_then(|delta| base.checked_add(delta))
        .ok_or(CrpError::ArithmeticOverflow(
            "ImmaterialResourceManager aggregate field offset",
        ))
}

fn invalid_aggregate_value(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "ImmaterialResourceManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

fn write_i32_values(
    bytes: &mut [u8],
    offset: usize,
    values: &[i32],
    count: usize,
    field: &'static str,
) -> Result<(), CrpError> {
    let values = values.get(..count).ok_or_else(|| {
        invalid_aggregate_value(offset, format!("{field} value count is inconsistent"))
    })?;
    let byte_count = count.checked_mul(4).ok_or(CrpError::ArithmeticOverflow(
        "ImmaterialResourceManager i32 aggregate size",
    ))?;
    let end = checked_aggregate_end(bytes, offset, byte_count, field)?;
    let output = bytes
        .get_mut(offset..end)
        .ok_or_else(|| invalid_aggregate_value(offset, format!("{field} byte range is invalid")))?;
    for (chunk, value) in output.chunks_exact_mut(4).zip(values) {
        chunk.copy_from_slice(&value.to_be_bytes());
    }
    Ok(())
}

fn write_multiplier_values(
    bytes: &mut [u8],
    offset: usize,
    values: &[i64],
    count: usize,
    width: usize,
) -> Result<(), CrpError> {
    let values = values.get(..count).ok_or_else(|| {
        invalid_aggregate_value(
            offset,
            "total-temporary multiplier value count is inconsistent",
        )
    })?;
    let byte_count = count
        .checked_mul(width)
        .ok_or(CrpError::ArithmeticOverflow(
            "ImmaterialResourceManager multiplier aggregate size",
        ))?;
    let end = checked_aggregate_end(bytes, offset, byte_count, "total-temporary multiplier")?;
    let output = bytes.get_mut(offset..end).ok_or_else(|| {
        invalid_aggregate_value(offset, "total-temporary multiplier byte range is invalid")
    })?;
    if width == 8 {
        for (chunk, value) in output.chunks_exact_mut(8).zip(values) {
            chunk.copy_from_slice(&value.to_be_bytes());
        }
    } else if width == 4 {
        for (index, (chunk, value)) in output.chunks_exact_mut(4).zip(values).enumerate() {
            let value_offset = aggregate_element_offset(offset, index, width)?;
            let value = i32::try_from(*value).map_err(|_| {
                invalid_aggregate_value(
                    value_offset,
                    "total-temporary multiplier does not fit its serialized i32 field",
                )
            })?;
            chunk.copy_from_slice(&value.to_be_bytes());
        }
    } else {
        return Err(invalid_aggregate_value(
            offset,
            format!("unsupported temporary multiplier width {width}"),
        ));
    }
    Ok(())
}

fn checked_aggregate_end(
    bytes: &[u8],
    offset: usize,
    byte_count: usize,
    field: &'static str,
) -> Result<usize, CrpError> {
    let end = offset
        .checked_add(byte_count)
        .ok_or(CrpError::ArithmeticOverflow(
            "ImmaterialResourceManager aggregate end",
        ))?;
    let payload_end = bytes
        .len()
        .checked_sub(END_PROFILER_REFERENCE_LEN)
        .ok_or_else(|| {
            invalid_aggregate_value(offset, "body lacks its closing profiler reference")
        })?;
    if end > payload_end {
        return Err(invalid_aggregate_value(
            offset,
            format!("{field} byte range overlaps the closing profiler reference"),
        ));
    }
    Ok(end)
}

fn read_i32_prefix(
    reader: &mut Reader<'_>,
    values: &mut [i32],
    count: usize,
) -> Result<(), CrpError> {
    if count > values.len() {
        return Err(CrpError::ArithmeticOverflow(
            "ImmaterialResourceManager aggregate count",
        ));
    }
    for value in values.iter_mut().take(count) {
        *value = reader.read_i32_be()?;
    }
    Ok(())
}

fn resource_slot_count(format_version: u32) -> usize {
    match format_version {
        0..=111 => 17,
        112..=132 => 18,
        133..=135 => 19,
        136..=266 => 20,
        267 => 21,
        268..=284 => 22,
        285..=304 => 23,
        305..=110_003 => 24,
        110_004..=111_012 => 25,
        111_013..=112_004 => 26,
        112_005..=113_001 => 27,
        113_002..=116_003 => 29,
        116_004..=116_999 => 31,
        _ => 35,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CELL_COUNT, FULL_TOTALS_VERSION, ImmaterialResourceManagerData, resource_slot_count,
    };

    fn append_repeat_ushort_values(output: &mut Vec<u8>, count: usize) {
        let mut remaining = count;
        while remaining > 0 {
            let run = remaining.min(128);
            output.push((0x7f + run) as u8);
            remaining -= run;
        }
    }

    fn encoded_ushort_array_with_first_value(count: usize, value: u16) -> Vec<u8> {
        assert!(count >= 3);
        let mut encoded = vec![2]; // Three literal values: value, zero, zero.
        encoded.extend_from_slice(&value.to_be_bytes());
        encoded.extend_from_slice(&[0, 0, 0, 0]);
        append_repeat_ushort_values(&mut encoded, count - 3);
        encoded
    }

    fn append_ramp_ushort_values(output: &mut Vec<u8>, count: usize) {
        if count == 0 {
            return;
        }
        output.push(0); // One full literal value.
        output.extend_from_slice(&1_u16.to_be_bytes());
        append_repeat_ushort_values(output, count - 1);
    }

    fn append_i32_values(output: &mut Vec<u8>, values: impl IntoIterator<Item = i32>) {
        for value in values {
            output.extend_from_slice(&value.to_be_bytes());
        }
    }

    fn synthetic_body(format_version: u32, ramp_final_grid: bool) -> Vec<u8> {
        let slots = resource_slot_count(format_version);
        let local_count = CELL_COUNT * slots;
        let mut body = Vec::new();
        if ramp_final_grid {
            append_ramp_ushort_values(&mut body, local_count);
        } else {
            append_repeat_ushort_values(&mut body, local_count);
        }
        append_repeat_ushort_values(&mut body, local_count);

        let aggregate_count = if format_version >= FULL_TOTALS_VERSION {
            slots
        } else {
            17
        };
        append_i32_values(
            &mut body,
            (0..aggregate_count).map(|index| -(100 + index as i32)),
        );
        append_i32_values(
            &mut body,
            (0..aggregate_count).map(|index| 200 + index as i32),
        );
        if format_version >= 242 {
            for index in 0..aggregate_count {
                body.extend_from_slice(&(-(300_i64 + index as i64)).to_be_bytes());
            }
        } else {
            append_i32_values(
                &mut body,
                (0..aggregate_count).map(|index| -(300 + index as i32)),
            );
        }
        if format_version >= FULL_TOTALS_VERSION {
            append_i32_values(&mut body, (0..slots).map(|index| 400 + index as i32));
            append_i32_values(&mut body, (0..slots).map(|index| -(500 + index as i32)));
        }
        body.extend_from_slice(&[0x12, 0x34]);
        body
    }

    #[test]
    fn resource_slot_count_tracks_all_dll_version_ranges() {
        for (version, expected) in [
            (111, 17),
            (112, 18),
            (132, 18),
            (133, 19),
            (135, 19),
            (136, 20),
            (266, 20),
            (267, 21),
            (268, 22),
            (284, 22),
            (285, 23),
            (304, 23),
            (305, 24),
            (110_003, 24),
            (110_004, 25),
            (111_012, 25),
            (111_013, 26),
            (112_004, 26),
            (112_005, 27),
            (113_001, 27),
            (113_002, 29),
            (116_003, 29),
            (116_004, 31),
            (116_999, 31),
            (117_000, 35),
        ] {
            assert_eq!(resource_slot_count(version), expected, "version {version}");
        }
    }

    #[test]
    fn decodes_latest_grid_totals_and_preserves_exact_body() {
        let body = synthetic_body(117_000, true);
        let data = ImmaterialResourceManagerData::decode(117_000, &body).unwrap();
        let local_count = CELL_COUNT * 35;
        assert_eq!(data.resource_slots_per_cell(), 35);
        assert_eq!(data.final_resources().len(), local_count);
        assert_eq!(data.final_resources()[0], 1);
        assert_eq!(data.final_resources()[1], 2);
        assert_eq!(data.temporary_resources().len(), local_count);
        assert_eq!(data.temporary_resources()[0], 0);
        assert_eq!(data.total_final()[0], -100);
        assert_eq!(data.total_final()[34], -134);
        assert_eq!(data.total_temporary()[0], 200);
        assert_eq!(data.total_temporary_multiplier()[0], -300);
        assert_eq!(data.multiplier_width_bytes(), 8);
        assert_eq!(data.global_final()[34], 434);
        assert_eq!(data.global_temporary()[34], -534);
        assert_eq!(data.encode_unchanged(), body);
    }

    #[test]
    fn aggregate_and_multiplier_gates_use_reader_widths_and_counts() {
        for (version, slots, aggregate_count, multiplier_width) in [
            (146, 20, 17, 4),
            (147, 20, 20, 4),
            (241, 20, 20, 4),
            (242, 20, 20, 8),
        ] {
            let body = synthetic_body(version, false);
            let data = ImmaterialResourceManagerData::decode(version, &body).unwrap();
            assert_eq!(data.resource_slots_per_cell(), slots);
            assert_eq!(data.total_final().len(), slots);
            assert_eq!(
                data.total_final()[aggregate_count - 1],
                -(99 + aggregate_count as i32)
            );
            if aggregate_count < slots {
                assert_eq!(data.total_final()[aggregate_count], 0);
                assert_eq!(data.global_final(), vec![0; slots]);
                assert_eq!(data.global_temporary(), vec![0; slots]);
            } else {
                assert_eq!(data.global_final()[slots - 1], 400 + (slots - 1) as i32);
                assert_eq!(
                    data.global_temporary()[slots - 1],
                    -(500 + (slots - 1) as i32)
                );
            }
            assert_eq!(data.multiplier_width_bytes(), multiplier_width);
            assert_eq!(data.total_temporary_multiplier()[0], -300);
            assert_eq!(data.encode_unchanged(), body);
        }
    }

    #[test]
    fn edits_current_aggregate_values_at_independently_expected_offsets() {
        let body = synthetic_body(242, false);
        let mut data = ImmaterialResourceManagerData::decode(242, &body).unwrap();
        data.set_total_final(1, -0x0102_0304).unwrap();
        data.set_total_temporary(2, 0x1122_3344).unwrap();
        data.set_total_temporary_multiplier(3, -0x0102_0304_0506_0708)
            .unwrap();
        data.set_global_final(4, i32::MIN).unwrap();
        data.set_global_temporary(5, i32::MAX).unwrap();

        let local_array_bytes = 2 * (CELL_COUNT * 20 / 128);
        let mut expected = body.clone();
        let total_final_offset = local_array_bytes + 4;
        let total_temporary_offset = local_array_bytes + 20 * 4 + 2 * 4;
        let multiplier_offset = local_array_bytes + 20 * 4 * 2 + 3 * 8;
        let global_final_offset = local_array_bytes + 20 * 4 * 2 + 20 * 8 + 4 * 4;
        let global_temporary_offset = local_array_bytes + 20 * 4 * 2 + 20 * 8 + 20 * 4 + 5 * 4;
        expected[total_final_offset..total_final_offset + 4]
            .copy_from_slice(&(-0x0102_0304_i32).to_be_bytes());
        expected[total_temporary_offset..total_temporary_offset + 4]
            .copy_from_slice(&0x1122_3344_i32.to_be_bytes());
        expected[multiplier_offset..multiplier_offset + 8]
            .copy_from_slice(&(-0x0102_0304_0506_0708_i64).to_be_bytes());
        expected[global_final_offset..global_final_offset + 4]
            .copy_from_slice(&i32::MIN.to_be_bytes());
        expected[global_temporary_offset..global_temporary_offset + 4]
            .copy_from_slice(&i32::MAX.to_be_bytes());

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        assert_eq!(data.encode_unchanged(), body);
        let decoded = ImmaterialResourceManagerData::decode(242, &encoded).unwrap();
        assert_eq!(decoded.total_final()[1], -0x0102_0304);
        assert_eq!(decoded.total_temporary()[2], 0x1122_3344);
        assert_eq!(
            decoded.total_temporary_multiplier()[3],
            -0x0102_0304_0506_0708
        );
        assert_eq!(decoded.global_final()[4], i32::MIN);
        assert_eq!(decoded.global_temporary()[5], i32::MAX);
    }

    #[test]
    fn local_resource_edits_reencode_only_changed_arrays_with_expected_bytes() {
        let body = synthetic_body(265, false);
        let mut data = ImmaterialResourceManagerData::decode(265, &body).unwrap();
        data.set_final_resource(0, 0, 0x1234).unwrap();
        data.set_temporary_resource(0, 0, 0xabcd).unwrap();

        let local_count = CELL_COUNT * 20;
        let expected_final = encoded_ushort_array_with_first_value(local_count, 0x1234);
        let expected_temporary = encoded_ushort_array_with_first_value(local_count, 0xabcd);
        let mut expected = body.clone();
        expected.splice(
            data.temporary_resources_snapshot.range.clone(),
            expected_temporary,
        );
        expected.splice(data.final_resources_snapshot.range.clone(), expected_final);

        let encoded = data.encode().unwrap();
        assert_eq!(encoded, expected);
        assert!(encoded.len() > body.len());
        assert_eq!(data.encode_unchanged(), body);

        let decoded = ImmaterialResourceManagerData::decode(265, &encoded).unwrap();
        assert_eq!(decoded.final_resources()[0], 0x1234);
        assert_eq!(decoded.temporary_resources()[0], 0xabcd);
        assert_eq!(decoded.final_resources()[1], 0);
        assert_eq!(decoded.temporary_resources()[1], 0);

        assert!(data.set_final_resource(CELL_COUNT, 0, 1).is_err());
        assert!(data.set_temporary_resource(0, 20, 1).is_err());
        assert_eq!(data.encode().unwrap(), encoded);
    }

    #[test]
    fn local_resource_editors_follow_versioned_slot_counts() {
        let mut before_slot_increase =
            ImmaterialResourceManagerData::decode(111, &synthetic_body(111, false)).unwrap();
        assert_eq!(before_slot_increase.resource_slots_per_cell(), 17);
        before_slot_increase
            .set_final_resource(1, 16, 0x1234)
            .unwrap();
        assert!(before_slot_increase.set_final_resource(1, 17, 1).is_err());

        let mut after_slot_increase =
            ImmaterialResourceManagerData::decode(112, &synthetic_body(112, false)).unwrap();
        assert_eq!(after_slot_increase.resource_slots_per_cell(), 18);
        after_slot_increase
            .set_temporary_resource(1, 17, 0xabcd)
            .unwrap();
        assert!(
            after_slot_increase
                .set_temporary_resource(1, 18, 1)
                .is_err()
        );

        let encoded = after_slot_increase.encode().unwrap();
        let decoded = ImmaterialResourceManagerData::decode(112, &encoded).unwrap();
        assert_eq!(decoded.temporary_resources()[18 + 17], 0xabcd);
    }

    #[test]
    fn aggregate_editors_respect_legacy_counts_and_multiplier_width() {
        let body = synthetic_body(146, false);
        let mut data = ImmaterialResourceManagerData::decode(146, &body).unwrap();
        data.set_total_final(16, -123).unwrap();
        data.set_total_temporary(16, 456).unwrap();
        data.set_total_temporary_multiplier(16, i64::from(i32::MIN))
            .unwrap();
        assert!(data.set_total_final(17, 1).is_err());
        assert!(data.set_total_temporary(17, 1).is_err());
        assert!(data.set_total_temporary_multiplier(17, 1).is_err());
        assert!(
            data.set_total_temporary_multiplier(0, i64::from(i32::MAX) + 1)
                .is_err()
        );
        assert!(data.set_global_final(0, 1).is_err());
        assert!(data.set_global_temporary(0, 1).is_err());

        let encoded = data.encode().unwrap();
        let decoded = ImmaterialResourceManagerData::decode(146, &encoded).unwrap();
        assert_eq!(decoded.total_final()[16], -123);
        assert_eq!(decoded.total_temporary()[16], 456);
        assert_eq!(
            decoded.total_temporary_multiplier()[16],
            i64::from(i32::MIN)
        );
        assert_eq!(decoded.encode_unchanged(), encoded);
    }

    #[test]
    fn rejects_truncated_and_overlong_bodies() {
        let body = synthetic_body(242, false);
        assert!(ImmaterialResourceManagerData::decode(242, &body[..body.len() - 1]).is_err());
        let mut overlong = body;
        overlong.push(0);
        assert!(ImmaterialResourceManagerData::decode(242, &overlong).is_err());
    }
}
