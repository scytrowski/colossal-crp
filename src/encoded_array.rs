use crate::CrpError;
use crate::data_serializer::Reader;

pub(crate) fn read_encoded_byte_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<u8>, CrpError> {
    let mut values = Vec::with_capacity(count);
    let mut previous_value = 0_u8;
    let mut previous_delta = 0_i8;
    while values.len() < count {
        let control = reader.read_u8()?;
        if control < 0x80 {
            let run_length = usize::from(control) + 1;
            // The game stops reading when the preallocated array is full,
            // even if this literal control advertises additional values.
            for _ in 0..run_length.min(count - values.len()) {
                let value = reader.read_u8()?;
                previous_delta = value.wrapping_sub(previous_value) as i8;
                previous_value = value;
                values.push(value);
            }
        } else {
            let run_length = usize::from(control - 0x7f);
            // The game decoder stops after filling its preallocated array even
            // when a repeat control advertises a longer run.
            for _ in 0..run_length.min(count - values.len()) {
                previous_value = previous_value.wrapping_add(previous_delta as u8);
                values.push(previous_value);
            }
        }
    }
    Ok(values)
}

/// Writes byte values as literal blocks of at most 128 items.
///
/// This is a valid, deterministic encoding; it does not reproduce the game's
/// choice between literal and repeated-delta blocks.
pub(crate) fn write_encoded_byte_array(output: &mut Vec<u8>, values: &[u8]) {
    for chunk in values.chunks(128) {
        // `chunks(128)` guarantees a nonempty block of at most 128 values.
        output.push((chunk.len() - 1) as u8);
        output.extend_from_slice(chunk);
    }
}

pub(crate) fn read_encoded_bool_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<bool>, CrpError> {
    let packed_byte_count = count
        .checked_add(7)
        .ok_or(CrpError::ArithmeticOverflow("EncodedArray.Bool bit count"))?
        / 8;
    let packed = read_encoded_byte_array(reader, packed_byte_count)?;
    Ok((0..count)
        .map(|index| packed[index / 8] & (1 << (index % 8)) != 0)
        .collect())
}

/// Decodes `EncodedArray.Float` literal and repeat controls.
pub(crate) fn read_encoded_float_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<f32>, CrpError> {
    // Even a repeat control produces at most 128 values per encoded byte.
    let maximum_decodable_count = reader.remaining().saturating_mul(128);
    if count > maximum_decodable_count {
        return Err(reader.invalid(format!(
            "EncodedArray.Float count {count} cannot fit in {} remaining control bytes",
            reader.remaining()
        )));
    }
    let mut values = Vec::with_capacity(count);
    let mut previous_value = 0.0_f32;
    while values.len() < count {
        let control = reader.read_u8()?;
        let remaining = count - values.len();
        if control < 0x80 {
            let run_length = (usize::from(control) + 1).min(remaining);
            for _ in 0..run_length {
                previous_value = reader.read_f32_be()?;
                values.push(previous_value);
            }
        } else {
            let run_length = (usize::from(control) - 0x7f).min(remaining);
            for _ in 0..run_length {
                values.push(previous_value);
            }
        }
    }
    Ok(values)
}

/// Writes float values as literal blocks of at most 128 items.
///
/// This deterministic encoding is accepted by `EncodedArray.Float.Read`; it
/// does not reproduce the game's choice of repeat controls.
pub(crate) fn write_encoded_float_array(output: &mut Vec<u8>, values: &[f32]) {
    for chunk in values.chunks(128) {
        // `chunks(128)` guarantees a nonempty block of at most 128 values.
        output.push((chunk.len() - 1) as u8);
        for value in chunk {
            output.extend_from_slice(&value.to_bits().to_be_bytes());
        }
    }
}

pub(crate) fn write_encoded_bool_array(output: &mut Vec<u8>, values: &[bool]) {
    let mut packed = vec![0_u8; values.len().div_ceil(8)];
    for (index, value) in values.iter().copied().enumerate() {
        if value {
            packed[index / 8] |= 1 << (index % 8);
        }
    }
    write_encoded_byte_array(output, &packed);
}

/// Writes unsigned 32-bit values as canonical EncodedArray literal blocks.
pub(crate) fn write_encoded_uint_array(output: &mut Vec<u8>, values: &[u32]) {
    for chunk in values.chunks(64) {
        output.push((chunk.len() - 1) as u8);
        for value in chunk {
            output.extend_from_slice(&value.to_be_bytes());
        }
    }
}

/// Writes unsigned 64-bit values as canonical EncodedArray literal blocks.
pub(crate) fn write_encoded_ulong_array(output: &mut Vec<u8>, values: &[u64]) {
    for chunk in values.chunks(64) {
        output.push((chunk.len() - 1) as u8);
        for value in chunk {
            output.extend_from_slice(&value.to_be_bytes());
        }
    }
}

/// Writes signed 64-bit values with the ULong codec's identical bit layout.
pub(crate) fn write_encoded_long_array(output: &mut Vec<u8>, values: &[i64]) {
    for chunk in values.chunks(64) {
        output.push((chunk.len() - 1) as u8);
        for value in chunk {
            output.extend_from_slice(&value.to_be_bytes());
        }
    }
}

/// Writes signed 32-bit values using the unsigned codec's identical bit layout.
pub(crate) fn write_encoded_int_array(output: &mut Vec<u8>, values: &[i32]) {
    for chunk in values.chunks(64) {
        output.push((chunk.len() - 1) as u8);
        for value in chunk {
            output.extend_from_slice(&value.to_be_bytes());
        }
    }
}

// The encoded UShort and Short controls share the same 16-bit bit pattern.
pub(crate) fn read_encoded_ushort_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<u16>, CrpError> {
    let mut values = Vec::with_capacity(count);
    let mut previous_value = 0_u16;
    let mut previous_delta = 0_i16;
    let mut full_count = 0_usize;
    let mut close_count = 0_usize;
    let mut same_count = 0_usize;

    while values.len() < count {
        if same_count > 0 {
            previous_value = previous_value.wrapping_add(previous_delta as u16);
            same_count -= 1;
        } else if close_count > 0 {
            let delta_adjustment = reader.read_u8()? as i8;
            previous_delta = previous_delta.wrapping_add(i16::from(delta_adjustment));
            previous_value = previous_value.wrapping_add(previous_delta as u16);
            close_count -= 1;
        } else if full_count > 0 {
            let value = reader.read_u16_be()?;
            previous_delta = value.wrapping_sub(previous_value) as i16;
            previous_value = value;
            full_count -= 1;
        } else {
            let control = reader.read_u8()?;
            if control >= 0x80 {
                same_count = usize::from(control - 0x7f);
            } else if control >= 0x40 {
                close_count = usize::from(control - 0x3f);
            } else {
                full_count = usize::from(control) + 1;
            }
            continue;
        }
        values.push(previous_value);
    }
    Ok(values)
}

/// Consumes `EncodedArray.UShort` values without allocating a value buffer.
///
/// Each call represents one `BeginRead`/`EndRead` scope. A control run may
/// extend past the requested count; only bytes for requested values are read.
pub(crate) fn skip_encoded_ushort_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<(), CrpError> {
    let mut remaining = count;
    while remaining > 0 {
        let control = reader.read_u8()?;
        let (run, bytes_per_value) = if control >= 0x80 {
            (usize::from(control - 0x7f), 0)
        } else if control >= 0x40 {
            (usize::from(control - 0x3f), 1)
        } else {
            (usize::from(control) + 1, 2)
        };
        let used = run.min(remaining);
        reader.read_bytes(used * bytes_per_value)?;
        remaining -= used;
    }
    Ok(())
}

/// Decodes `EncodedArray.UInt`; its close-delta adjustment is a signed i16.
pub(crate) fn read_encoded_uint_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<u32>, CrpError> {
    EncodedUIntArrayReader::default().read_array(reader, count)
}

/// Reads consecutive unsigned `EncodedArray.UInt` fields in one shared scope.
///
/// `BeginRead` resets the delta state once; callers keep this reader while
/// decoding multiple arrays before `EndRead`.
#[derive(Debug, Default)]
pub(crate) struct EncodedUIntArrayReader {
    previous_value: u32,
    previous_delta: i32,
    full_count: usize,
    close_count: usize,
    same_count: usize,
}

impl EncodedUIntArrayReader {
    pub(crate) fn read_array(
        &mut self,
        reader: &mut Reader<'_>,
        count: usize,
    ) -> Result<Vec<u32>, CrpError> {
        let mut values = Vec::with_capacity(count);
        while values.len() < count {
            if self.same_count > 0 {
                self.previous_value = self.previous_value.wrapping_add(self.previous_delta as u32);
                self.same_count -= 1;
            } else if self.close_count > 0 {
                let adjustment = i32::from(reader.read_i16_be()?);
                self.previous_delta = self.previous_delta.wrapping_add(adjustment);
                self.previous_value = self.previous_value.wrapping_add(self.previous_delta as u32);
                self.close_count -= 1;
            } else if self.full_count > 0 {
                let value = reader.read_u32_be()?;
                self.previous_delta = value.wrapping_sub(self.previous_value) as i32;
                self.previous_value = value;
                self.full_count -= 1;
            } else {
                let control = reader.read_u8()?;
                if control >= 0x80 {
                    self.same_count = usize::from(control - 0x7f);
                } else if control >= 0x40 {
                    self.close_count = usize::from(control - 0x3f);
                } else {
                    self.full_count = usize::from(control) + 1;
                }
                continue;
            }
            values.push(self.previous_value);
        }
        Ok(values)
    }
}

/// `EncodedArray.Int` has the UInt control layout and modulo-32-bit deltas.
pub(crate) fn read_encoded_int_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<i32>, CrpError> {
    Ok(read_encoded_uint_array(reader, count)?
        .into_iter()
        .map(|bits| bits as i32)
        .collect())
}

/// Decodes `EncodedArray.ULong`; close-delta adjustments are signed i32.
pub(crate) fn read_encoded_ulong_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<u64>, CrpError> {
    let mut values = Vec::with_capacity(count);
    let mut previous_value = 0_u64;
    let mut previous_delta = 0_i64;
    let mut full_count = 0_usize;
    let mut close_count = 0_usize;
    let mut same_count = 0_usize;

    while values.len() < count {
        if same_count > 0 {
            previous_value = previous_value.wrapping_add(previous_delta as u64);
            same_count -= 1;
        } else if close_count > 0 {
            let adjustment = i64::from(reader.read_i32_be()?);
            previous_delta = previous_delta.wrapping_add(adjustment);
            previous_value = previous_value.wrapping_add(previous_delta as u64);
            close_count -= 1;
        } else if full_count > 0 {
            let value = reader.read_u64_be()?;
            previous_delta = value.wrapping_sub(previous_value) as i64;
            previous_value = value;
            full_count -= 1;
        } else {
            let control = reader.read_u8()?;
            if control >= 0x80 {
                same_count = usize::from(control - 0x7f);
            } else if control >= 0x40 {
                close_count = usize::from(control - 0x3f);
            } else {
                full_count = usize::from(control) + 1;
            }
            continue;
        }
        values.push(previous_value);
    }
    Ok(values)
}

/// Decodes `EncodedArray.Long` using the ULong control layout and signed bit patterns.
#[cfg(test)]
pub(crate) fn read_encoded_long_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<i64>, CrpError> {
    EncodedLongArrayReader::default().read_array(reader, count)
}

/// Reads consecutive signed `EncodedArray.Long` arrays in one shared scope.
///
/// `BeginRead` resets the delta state once, while multiple array fields can be
/// read before `EndRead`. Keep one reader for all fields in that scope.
#[derive(Debug, Default)]
pub(crate) struct EncodedLongArrayReader {
    previous_value: i64,
    previous_delta: i64,
    full_count: usize,
    close_count: usize,
    same_count: usize,
}

impl EncodedLongArrayReader {
    pub(crate) fn read_array(
        &mut self,
        reader: &mut Reader<'_>,
        count: usize,
    ) -> Result<Vec<i64>, CrpError> {
        let mut values = Vec::with_capacity(count);
        while values.len() < count {
            if self.same_count > 0 {
                self.previous_value = self.previous_value.wrapping_add(self.previous_delta);
                self.same_count -= 1;
            } else if self.close_count > 0 {
                let adjustment = i64::from(reader.read_i32_be()?);
                self.previous_delta = self.previous_delta.wrapping_add(adjustment);
                self.previous_value = self.previous_value.wrapping_add(self.previous_delta);
                self.close_count -= 1;
            } else if self.full_count > 0 {
                let value = reader.read_i64_be()?;
                self.previous_delta = value.wrapping_sub(self.previous_value);
                self.previous_value = self.previous_value.wrapping_add(self.previous_delta);
                self.full_count -= 1;
            } else {
                let control = reader.read_u8()?;
                if control >= 0x80 {
                    self.same_count = usize::from(control - 0x7f);
                } else if control >= 0x40 {
                    self.close_count = usize::from(control - 0x3f);
                } else {
                    self.full_count = usize::from(control) + 1;
                }
                continue;
            }
            values.push(self.previous_value);
        }
        Ok(values)
    }
}

// Explicit canonical writer: literal blocks and repeated signed deltas.
pub(crate) fn write_encoded_ushort_array(output: &mut Vec<u8>, values: &[u16]) {
    let mut index = 0;
    let mut previous_value = 0_u16;
    let mut previous_delta = 0_i16;
    while index < values.len() {
        let repeat = repeated_delta_run(values, index, previous_value, previous_delta);
        if repeat >= 3 {
            output.push(0x7f + repeat as u8);
            previous_value = values[index + repeat - 1];
            index += repeat;
            continue;
        }

        let start = index;
        while index < values.len() && index - start < 64 {
            if index > start
                && repeated_delta_run(values, index, previous_value, previous_delta) >= 3
            {
                break;
            }
            let value = values[index];
            previous_delta = value.wrapping_sub(previous_value) as i16;
            previous_value = value;
            index += 1;
        }
        output.push((index - start - 1) as u8);
        for value in &values[start..index] {
            output.extend_from_slice(&value.to_be_bytes());
        }
    }
}

fn repeated_delta_run(values: &[u16], start: usize, previous: u16, delta: i16) -> usize {
    let mut expected = previous;
    let mut length = 0;
    for value in values[start..].iter().take(128) {
        expected = expected.wrapping_add(delta as u16);
        if *value != expected {
            break;
        }
        length += 1;
    }
    length
}

#[cfg(test)]
mod tests {
    use super::{
        EncodedLongArrayReader, EncodedUIntArrayReader, read_encoded_bool_array,
        read_encoded_byte_array, read_encoded_float_array, read_encoded_long_array,
        read_encoded_uint_array, read_encoded_ulong_array, read_encoded_ushort_array,
        skip_encoded_ushort_array, write_encoded_bool_array, write_encoded_float_array,
        write_encoded_long_array,
    };
    use crate::data_serializer::Reader;

    #[test]
    fn ushort_array_decodes_literal_repeat_and_delta_adjustment_runs() {
        let literal_and_repeat = [0x00, 0x00, 0x05, 0x81];
        let mut reader = Reader::new(&literal_and_repeat);
        assert_eq!(
            read_encoded_ushort_array(&mut reader, 3).expect("valid encoded values"),
            [5, 10, 15]
        );

        let close_run = [0x00, 0x10, 0x00, 0x40, 0x01];
        let mut reader = Reader::new(&close_run);
        assert_eq!(
            read_encoded_ushort_array(&mut reader, 2).expect("valid adjusted delta"),
            [0x1000, 0x2001]
        );
    }

    #[test]
    fn ushort_skip_preserves_control_boundaries_without_allocating_values() {
        let bytes = [0x01, 0x12, 0x34, 0xab, 0xcd, 0x40, 0xfe, 0x81, 0xee];
        let mut reader = Reader::new(&bytes);
        skip_encoded_ushort_array(&mut reader, 4).unwrap();
        assert_eq!(reader.offset(), 8);
        assert_eq!(reader.read_u8().unwrap(), 0xee);

        let mut reader = Reader::new(&[0x3f, 0x12]);
        assert!(skip_encoded_ushort_array(&mut reader, 1).is_err());
        let mut reader = Reader::new(&[0x40]);
        assert!(skip_encoded_ushort_array(&mut reader, 1).is_err());
    }

    #[test]
    fn ushort_array_rejects_truncated_literal_values() {
        let mut reader = Reader::new(&[0x01, 0x12]);
        assert!(read_encoded_ushort_array(&mut reader, 2).is_err());
    }

    #[test]
    fn uint_array_decodes_literal_repeat_and_signed_close_delta() {
        let bytes = [0, 0, 0, 0, 5, 0x81, 0x40, 0xff, 0xfe];
        let mut reader = Reader::new(&bytes);
        assert_eq!(
            read_encoded_uint_array(&mut reader, 4).expect("valid UInt controls"),
            [5, 10, 15, 18]
        );
        assert_eq!(reader.offset(), bytes.len());
    }

    #[test]
    fn uint_reader_carries_delta_state_between_fields_in_one_scope() {
        let bytes = [0, 0, 0, 0, 1, 0x80];
        let mut reader = Reader::new(&bytes);
        let mut scope = EncodedUIntArrayReader::default();
        assert_eq!(scope.read_array(&mut reader, 1).unwrap(), [1]);
        assert_eq!(scope.read_array(&mut reader, 1).unwrap(), [2]);

        let mut separate_scope = Reader::new(&[0x80]);
        assert_eq!(
            read_encoded_uint_array(&mut separate_scope, 1).unwrap(),
            [0]
        );
    }

    #[test]
    fn uint_array_rejects_truncated_close_delta() {
        let mut reader = Reader::new(&[0, 0, 0, 0, 5, 0x40, 0]);
        assert!(read_encoded_uint_array(&mut reader, 2).is_err());
    }

    #[test]
    fn ulong_array_decodes_literal_repeat_and_signed_close_delta() {
        let bytes = [
            0, 0, 0, 0, 0, 0, 0, 0, 5, 0x81, 0x40, 0xff, 0xff, 0xff, 0xfe,
        ];
        let mut reader = Reader::new(&bytes);
        assert_eq!(
            read_encoded_ulong_array(&mut reader, 4).expect("valid ULong controls"),
            [5, 10, 15, 18]
        );
        assert_eq!(reader.offset(), bytes.len());
    }

    #[test]
    fn long_array_fields_in_one_scope_share_delta_state() {
        let bytes = [
            0x01, // Two literal values follow.
            0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 10,
            0x80, // Repeat the previous delta once in the next field.
        ];
        let mut reader = Reader::new(&bytes);
        let mut scope = EncodedLongArrayReader::default();
        assert_eq!(scope.read_array(&mut reader, 2).unwrap(), [5, 10]);
        assert_eq!(scope.read_array(&mut reader, 1).unwrap(), [15]);
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn ulong_array_rejects_truncated_close_delta() {
        let mut reader = Reader::new(&[0, 0, 0, 0, 0, 0, 0, 0, 5, 0x40, 0]);
        assert!(read_encoded_ulong_array(&mut reader, 2).is_err());
    }

    #[test]
    fn long_array_decodes_signed_literals_repeats_and_close_deltas() {
        let mut bytes = vec![0x00];
        bytes.extend_from_slice(&(-7_i64).to_be_bytes());
        bytes.extend_from_slice(&[0x81, 0x40, 0x00, 0x00, 0x00, 0x02]);
        let mut reader = Reader::new(&bytes);
        assert_eq!(
            read_encoded_long_array(&mut reader, 4).expect("valid signed Long controls"),
            [-7, -14, -21, -26]
        );
        assert_eq!(reader.offset(), bytes.len());
    }

    #[test]
    fn long_array_rejects_truncated_signed_literals() {
        let mut reader = Reader::new(&[0x00, 0xff, 0xff, 0xff]);
        assert!(read_encoded_long_array(&mut reader, 1).is_err());
    }

    #[test]
    fn long_array_writer_emits_expected_big_endian_literal_bits() {
        let values = [-7_i64, i64::MIN, 0, i64::MAX];
        let mut encoded = Vec::new();
        write_encoded_long_array(&mut encoded, &values);

        let mut expected = vec![0x03];
        expected.extend_from_slice(&(-7_i64).to_be_bytes());
        expected.extend_from_slice(&i64::MIN.to_be_bytes());
        expected.extend_from_slice(&0_i64.to_be_bytes());
        expected.extend_from_slice(&i64::MAX.to_be_bytes());
        assert_eq!(encoded, expected);

        let mut reader = Reader::new(&encoded);
        assert_eq!(
            read_encoded_long_array(&mut reader, values.len()).expect("valid Long literals"),
            values
        );
    }

    #[test]
    fn bool_array_uses_least_significant_bit_first_and_expected_byte_codec() {
        let expected_values = [true, false, true, false, false, false, false, true, true];
        let expected_bytes = [0x01, 0x85, 0x01];

        let mut encoded = Vec::new();
        write_encoded_bool_array(&mut encoded, &expected_values);
        assert_eq!(encoded, expected_bytes);

        let mut reader = Reader::new(&expected_bytes);
        assert_eq!(
            read_encoded_bool_array(&mut reader, expected_values.len()).unwrap(),
            expected_values
        );
        assert_eq!(reader.offset(), expected_bytes.len());
    }

    #[test]
    fn bool_array_rejects_truncated_packed_bytes_and_handles_empty_input() {
        let mut truncated = Reader::new(&[0x01, 0x85]);
        assert!(read_encoded_bool_array(&mut truncated, 9).is_err());

        let mut empty = Reader::new(&[]);
        assert!(read_encoded_bool_array(&mut empty, 0).unwrap().is_empty());
        assert_eq!(empty.offset(), 0);
    }

    #[test]
    fn float_array_decodes_literal_ieee_values_and_repeats_the_previous_value() {
        let bytes = [0x01, 0x3f, 0x80, 0x00, 0x00, 0xc0, 0x20, 0x00, 0x00, 0x80];
        let mut reader = Reader::new(&bytes);
        let values = read_encoded_float_array(&mut reader, 3).expect("valid Float controls");

        assert_eq!(
            values
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            [
                1.0_f32.to_bits(),
                (-2.5_f32).to_bits(),
                (-2.5_f32).to_bits()
            ]
        );
        assert_eq!(reader.offset(), bytes.len());
    }

    #[test]
    fn float_array_stops_inside_repeat_runs_and_rejects_truncated_literals() {
        let bytes = [0x00, 0x40, 0xa0, 0x00, 0x00, 0x83, 0x12];
        let mut reader = Reader::new(&bytes);
        assert_eq!(
            read_encoded_float_array(&mut reader, 3).expect("valid repeat run"),
            [5.0, 5.0, 5.0]
        );
        assert_eq!(reader.offset(), 6);
        assert_eq!(reader.read_u8().unwrap(), 0x12);

        let mut truncated = Reader::new(&[0x00, 0x3f, 0x80, 0x00]);
        assert!(read_encoded_float_array(&mut truncated, 1).is_err());

        let mut impossible = Reader::new(&[0xff]);
        assert!(read_encoded_float_array(&mut impossible, 129).is_err());

        let mut empty = Reader::new(&[]);
        assert!(read_encoded_float_array(&mut empty, 0).unwrap().is_empty());
        assert_eq!(empty.offset(), 0);
    }

    #[test]
    fn float_array_writer_emits_exact_literal_blocks() {
        let values = [1.0_f32, -2.5, -0.0];
        let mut encoded = Vec::new();
        write_encoded_float_array(&mut encoded, &values);
        assert_eq!(
            encoded,
            [
                0x02, 0x3f, 0x80, 0x00, 0x00, 0xc0, 0x20, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00,
            ]
        );
        let mut reader = Reader::new(&encoded);
        let decoded = read_encoded_float_array(&mut reader, 3).unwrap();
        assert_eq!(
            decoded
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            values
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        );
        assert_eq!(reader.remaining(), 0);

        let mut empty = Vec::new();
        write_encoded_float_array(&mut empty, &[]);
        assert!(empty.is_empty());
    }

    #[test]
    fn byte_array_stops_at_requested_count_inside_literal_run() {
        let bytes = [0x02, 0x11, 0x22, 0x33];
        let mut reader = Reader::new(&bytes);

        assert_eq!(
            read_encoded_byte_array(&mut reader, 2).unwrap(),
            [0x11, 0x22]
        );
        assert_eq!(reader.offset(), 3);
        assert_eq!(reader.read_u8().unwrap(), 0x33);
    }
}
