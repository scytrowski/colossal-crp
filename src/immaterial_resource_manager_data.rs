use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::read_encoded_ushort_array;

const CELL_COUNT: usize = 65_536;
const END_PROFILER_REFERENCE_LEN: usize = 2;
const I32_MULTIPLIER_VERSION: u32 = 242;
const FULL_TOTALS_VERSION: u32 = 147;

/// Decoded reader fields from `ImmaterialResourceManager+Data`.
///
/// The final and temporary local-resource arrays are flat, cell-major
/// sequences. Each cell occupies `resource_slots_per_cell` values in those
/// vectors. The five aggregate arrays retain their serialized positional
/// order; the source body remains available for exact unchanged writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmaterialResourceManagerData {
    format_version: u32,
    resource_slots_per_cell: usize,
    final_resources: Vec<u16>,
    temporary_resources: Vec<u16>,
    total_final: Vec<i32>,
    total_temporary: Vec<i32>,
    total_temporary_multiplier: Vec<i64>,
    global_final: Vec<i32>,
    global_temporary: Vec<i32>,
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
        let final_resources = read_encoded_ushort_array(&mut reader, local_value_count)?;
        let temporary_resources = read_encoded_ushort_array(&mut reader, local_value_count)?;

        let aggregate_read_count = if format_version >= FULL_TOTALS_VERSION {
            resource_slots_per_cell
        } else {
            17
        };
        let mut total_final = vec![0; resource_slots_per_cell];
        read_i32_prefix(&mut reader, &mut total_final, aggregate_read_count)?;

        let mut total_temporary = vec![0; resource_slots_per_cell];
        read_i32_prefix(&mut reader, &mut total_temporary, aggregate_read_count)?;

        let mut total_temporary_multiplier = vec![0_i64; resource_slots_per_cell];
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
        if format_version >= FULL_TOTALS_VERSION {
            read_i32_prefix(&mut reader, &mut global_final, resource_slots_per_cell)?;
            read_i32_prefix(&mut reader, &mut global_temporary, resource_slots_per_cell)?;
        }

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
            temporary_resources,
            total_final,
            total_temporary,
            total_temporary_multiplier,
            global_final,
            global_temporary,
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

    /// Returns the temporary-resource grid in cell-major order.
    pub fn temporary_resources(&self) -> &[u16] {
        &self.temporary_resources
    }

    /// Returns total-final values in resource-slot order. On versions below
    /// 147, entries after the first 17 are initialized to zero by the reader.
    pub fn total_final(&self) -> &[i32] {
        &self.total_final
    }

    /// Returns total-temporary values in resource-slot order. On versions
    /// below 147, entries after the first 17 are initialized to zero.
    pub fn total_temporary(&self) -> &[i32] {
        &self.total_temporary
    }

    /// Returns temporary multipliers as signed 64-bit values. Their on-disk
    /// width is four bytes before version 242 and eight bytes thereafter.
    pub fn total_temporary_multiplier(&self) -> &[i64] {
        &self.total_temporary_multiplier
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

    /// Returns global-temporary values in resource-slot order. Versions below
    /// 147 initialize this array to zero without reading it from the stream.
    pub fn global_temporary(&self) -> &[i32] {
        &self.global_temporary
    }

    /// Returns the complete source body unchanged, including the closing
    /// profiler reference.
    pub fn encode_unchanged(&self) -> Vec<u8> {
        self.original_body.clone()
    }
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
    fn rejects_truncated_and_overlong_bodies() {
        let body = synthetic_body(242, false);
        assert!(ImmaterialResourceManagerData::decode(242, &body[..body.len() - 1]).is_err());
        let mut overlong = body;
        overlong.push(0);
        assert!(ImmaterialResourceManagerData::decode(242, &overlong).is_err());
    }
}
