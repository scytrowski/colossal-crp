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
            if run_length > count - values.len() {
                return Err(
                    reader.invalid("EncodedArray.Byte literal run exceeds the array length")
                );
            }
            for _ in 0..run_length {
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

/// Decodes `EncodedArray.UInt`; its close-delta adjustment is a signed i16.
pub(crate) fn read_encoded_uint_array(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<u32>, CrpError> {
    let mut values = Vec::with_capacity(count);
    let mut previous_value = 0_u32;
    let mut previous_delta = 0_i32;
    let mut full_count = 0_usize;
    let mut close_count = 0_usize;
    let mut same_count = 0_usize;

    while values.len() < count {
        if same_count > 0 {
            previous_value = previous_value.wrapping_add(previous_delta as u32);
            same_count -= 1;
        } else if close_count > 0 {
            let adjustment = i32::from(reader.read_i16_be()?);
            previous_delta = previous_delta.wrapping_add(adjustment);
            previous_value = previous_value.wrapping_add(previous_delta as u32);
            close_count -= 1;
        } else if full_count > 0 {
            let value = reader.read_u32_be()?;
            previous_delta = value.wrapping_sub(previous_value) as i32;
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
    use super::{read_encoded_uint_array, read_encoded_ushort_array};
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
    fn uint_array_rejects_truncated_close_delta() {
        let mut reader = Reader::new(&[0, 0, 0, 0, 5, 0x40, 0]);
        assert!(read_encoded_uint_array(&mut reader, 2).is_err());
    }
}
