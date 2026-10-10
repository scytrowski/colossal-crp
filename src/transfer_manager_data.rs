use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    read_encoded_bool_array, read_encoded_byte_array, read_encoded_uint_array,
    read_encoded_ushort_array, write_encoded_bool_array, write_encoded_byte_array,
    write_encoded_uint_array, write_encoded_ushort_array,
};

const MIN_SUPPORTED_VERSION: u32 = 37;
const EXCLUDE_FLAG_VERSION: u32 = 111_001;
const LOCAL_PARK_VERSION: u32 = 115_001;
const END_PROFILER_REFERENCE_LEN: usize = 2;
const MAX_DECODED_OFFERS: usize = 1_000_000;

/// One serialized offer in `TransferManager+Data`.
///
/// The source order of offers is retained. `excluded` and `local_park` are
/// absent in versions whose reader does not serialize those columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferOffer {
    /// Whether this offer slot is active.
    pub active: bool,
    /// Version-111001 exclusion flag.
    pub excluded: Option<bool>,
    /// Offer priority.
    pub priority: u8,
    /// Requested or supplied amount.
    pub amount: u8,
    /// X coordinate byte.
    pub x: u8,
    /// Z coordinate byte.
    pub z: u8,
    /// Serialized instance type byte.
    pub instance_type: u8,
    /// Serialized instance index.
    pub instance_index: u32,
    /// Version-115001 local-park byte.
    pub local_park: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EncodedColumns {
    incoming_amounts: Vec<u8>,
    outgoing_amounts: Vec<u8>,
    reason_counts: Vec<u8>,
    active: Vec<u8>,
    excluded: Option<Vec<u8>>,
    priority: Vec<u8>,
    amount: Vec<u8>,
    x: Vec<u8>,
    z: Vec<u8>,
    instance_type: Vec<u8>,
    instance_index: Vec<u8>,
    local_park: Option<Vec<u8>>,
    end_profiler_reference: Vec<u8>,
}

/// Decoded `TransferManager+Data` offer fields.
///
/// The two amount arrays and the 16-per-reason count array are exposed without
/// assigning game-specific meaning to each count slot. The final two-byte
/// profiler reference is retained verbatim and is not resolved here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferManagerData {
    format_version: u32,
    incoming_amounts: Vec<i32>,
    outgoing_amounts: Vec<i32>,
    reason_counts: Vec<u16>,
    offers: Vec<TransferOffer>,
    columns: EncodedColumns,
    original_bytes: Vec<u8>,
}

impl TransferManagerData {
    /// Decodes a complete bounded manager body, including its two-byte closing
    /// profiler reference. Supports inner versions 37 and newer using the
    /// reader's version gates. Truncated arrays, impossible counts, more than
    /// one million offer slots, and extra bytes return an error. The offer
    /// limit bounds memory use for untrusted count arrays.
    pub fn decode(format_version: u32, bytes: &[u8]) -> Result<Self, CrpError> {
        let mut reader = Reader::new(bytes);
        if format_version < MIN_SUPPORTED_VERSION {
            return Err(reader.invalid(format!(
                "TransferManager requires inner version {MIN_SUPPORTED_VERSION} or newer"
            )));
        }

        let reason_count = transfer_reason_count(format_version);
        let incoming_amounts = read_direct_i32_array(&mut reader, reason_count)?;
        let outgoing_amounts = read_direct_i32_array(&mut reader, reason_count)?;
        let amount_bytes_end = reader.offset();
        let reason_counts = read_encoded_ushort_array(
            &mut reader,
            reason_count
                .checked_mul(16)
                .ok_or(CrpError::ArithmeticOverflow(
                    "TransferManager count array length",
                ))?,
        )?;
        let reason_counts_end = reader.offset();
        let offer_count = reason_counts.iter().try_fold(0_usize, |total, count| {
            total
                .checked_add(usize::from(*count))
                .ok_or(CrpError::ArithmeticOverflow("TransferManager offer count"))
        })?;
        if offer_count > MAX_DECODED_OFFERS {
            return Err(reader.invalid(format!(
                "offer count {offer_count} exceeds the {MAX_DECODED_OFFERS}-offer decoding limit"
            )));
        }

        // Byte and UInt columns need one control per 128 values at minimum.
        // Bool columns first pack eight offers per byte, so their minimum
        // control count is lower. Check before allocating from file counts.
        let ordinary_columns = 6 + usize::from(format_version >= LOCAL_PARK_VERSION);
        let ordinary_controls = offer_count
            .div_ceil(128)
            .checked_mul(ordinary_columns)
            .ok_or(CrpError::ArithmeticOverflow(
                "TransferManager minimum column bytes",
            ))?;
        let bool_column_count = 1 + usize::from(format_version >= EXCLUDE_FLAG_VERSION);
        let bool_controls = offer_count.div_ceil(8).div_ceil(128);
        let minimum_controls = ordinary_controls
            .checked_add(bool_controls.checked_mul(bool_column_count).ok_or(
                CrpError::ArithmeticOverflow("TransferManager minimum bool bytes"),
            )?)
            .ok_or(CrpError::ArithmeticOverflow(
                "TransferManager minimum column bytes",
            ))?;
        if minimum_controls
            > reader
                .remaining()
                .saturating_sub(END_PROFILER_REFERENCE_LEN)
        {
            return Err(reader.invalid(format!(
                "offer count {offer_count} cannot fit in the remaining encoded columns"
            )));
        }

        let active_start = reader.offset();
        let active = read_encoded_bool_array(&mut reader, offer_count)?;
        let active_end = reader.offset();
        let excluded = if format_version >= EXCLUDE_FLAG_VERSION {
            Some(read_encoded_bool_array(&mut reader, offer_count)?)
        } else {
            None
        };
        let excluded_end = reader.offset();
        let priority_start = reader.offset();
        let priority = read_encoded_byte_array(&mut reader, offer_count)?;
        let priority_end = reader.offset();
        let amount = read_encoded_byte_array(&mut reader, offer_count)?;
        let amount_end = reader.offset();
        let x = read_encoded_byte_array(&mut reader, offer_count)?;
        let x_end = reader.offset();
        let z = read_encoded_byte_array(&mut reader, offer_count)?;
        let z_end = reader.offset();
        let instance_type = read_encoded_byte_array(&mut reader, offer_count)?;
        let instance_type_end = reader.offset();
        let instance_index = read_encoded_uint_array(&mut reader, offer_count)?;
        let instance_index_end = reader.offset();
        let local_park = if format_version >= LOCAL_PARK_VERSION {
            Some(read_encoded_byte_array(&mut reader, offer_count)?)
        } else {
            None
        };
        let local_park_end = reader.offset();

        if reader.remaining() != END_PROFILER_REFERENCE_LEN {
            return Err(reader.invalid(format!(
                "TransferManager fields leave {} bytes before the next manager; expected the two-byte closing profiler reference",
                reader.remaining()
            )));
        }
        let end_profiler_reference = reader.read_bytes(END_PROFILER_REFERENCE_LEN)?.to_vec();
        let bytes = reader.consumed_bytes();

        let mut offers = Vec::with_capacity(offer_count);
        for index in 0..offer_count {
            offers.push(TransferOffer {
                active: active[index],
                excluded: excluded.as_ref().map(|values| values[index]),
                priority: priority[index],
                amount: amount[index],
                x: x[index],
                z: z[index],
                instance_type: instance_type[index],
                instance_index: instance_index[index],
                local_park: local_park.as_ref().map(|values| values[index]),
            });
        }

        let columns = EncodedColumns {
            incoming_amounts: bytes[..amount_bytes_end / 2].to_vec(),
            outgoing_amounts: bytes[amount_bytes_end / 2..amount_bytes_end].to_vec(),
            reason_counts: bytes[amount_bytes_end..reason_counts_end].to_vec(),
            active: bytes[active_start..active_end].to_vec(),
            excluded: if format_version >= EXCLUDE_FLAG_VERSION {
                Some(bytes[active_end..excluded_end].to_vec())
            } else {
                None
            },
            priority: bytes[priority_start..priority_end].to_vec(),
            amount: bytes[priority_end..amount_end].to_vec(),
            x: bytes[amount_end..x_end].to_vec(),
            z: bytes[x_end..z_end].to_vec(),
            instance_type: bytes[z_end..instance_type_end].to_vec(),
            instance_index: bytes[instance_type_end..instance_index_end].to_vec(),
            local_park: if format_version >= LOCAL_PARK_VERSION {
                Some(bytes[instance_index_end..local_park_end].to_vec())
            } else {
                None
            },
            end_profiler_reference,
        };

        Ok(Self {
            format_version,
            incoming_amounts,
            outgoing_amounts,
            reason_counts,
            offers,
            columns,
            original_bytes: bytes.to_vec(),
        })
    }

    /// Returns the inner DataSerializer version used for reader gates.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns incoming amount values in transfer-reason order.
    pub fn incoming_amounts(&self) -> &[i32] {
        &self.incoming_amounts
    }

    /// Returns outgoing amount values in transfer-reason order.
    pub fn outgoing_amounts(&self) -> &[i32] {
        &self.outgoing_amounts
    }

    /// Returns the serialized 16-per-reason count array in original order.
    pub fn reason_counts(&self) -> &[u16] {
        &self.reason_counts
    }

    /// Returns offers in their serialized column index order.
    pub fn offers(&self) -> &[TransferOffer] {
        &self.offers
    }

    /// Returns the complete bounded body unchanged, including its closing
    /// profiler reference.
    pub fn encode_unchanged(&self) -> Vec<u8> {
        self.original_bytes.clone()
    }

    /// Creates a fixed-record editor for amount values, count distribution,
    /// and existing offers. Offer slots cannot be inserted or removed.
    pub fn into_editor(self) -> TransferManagerEditor {
        TransferManagerEditor {
            incoming_amounts: self.incoming_amounts.clone(),
            outgoing_amounts: self.outgoing_amounts.clone(),
            reason_counts: self.reason_counts.clone(),
            offers: self.offers.clone(),
            original: self,
        }
    }
}

/// Edits fixed-size TransferManager arrays and existing offer slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferManagerEditor {
    original: TransferManagerData,
    incoming_amounts: Vec<i32>,
    outgoing_amounts: Vec<i32>,
    reason_counts: Vec<u16>,
    offers: Vec<TransferOffer>,
}

impl TransferManagerEditor {
    /// Returns the inner DataSerializer version of the source body.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns mutable incoming amount values.
    pub fn incoming_amounts_mut(&mut self) -> &mut [i32] {
        &mut self.incoming_amounts
    }

    /// Returns mutable outgoing amount values.
    pub fn outgoing_amounts_mut(&mut self) -> &mut [i32] {
        &mut self.outgoing_amounts
    }

    /// Returns the edited serialized reason counts.
    pub fn reason_counts(&self) -> &[u16] {
        &self.reason_counts
    }

    /// Returns mutable serialized reason counts. Their total must continue to
    /// equal the number of existing offer slots.
    pub fn reason_counts_mut(&mut self) -> &mut [u16] {
        &mut self.reason_counts
    }

    /// Returns mutable access to existing offer slots in serialized order.
    pub fn offers_mut(&mut self) -> &mut [TransferOffer] {
        &mut self.offers
    }

    /// Returns existing offer slots in serialized order.
    pub fn offers(&self) -> &[TransferOffer] {
        &self.offers
    }

    /// Encodes edited arrays while copying every unchanged encoded column
    /// verbatim. The two-byte closing profiler reference remains unchanged.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.incoming_amounts.len() != self.original.incoming_amounts.len()
            || self.outgoing_amounts.len() != self.original.outgoing_amounts.len()
            || self.reason_counts.len() != self.original.reason_counts.len()
            || self.offers.len() != self.original.offers.len()
        {
            return Err(invalid(0, "TransferManager array length changed"));
        }
        if self.format_version() < EXCLUDE_FLAG_VERSION
            && self.offers.iter().any(|offer| offer.excluded.is_some())
        {
            return Err(invalid(
                0,
                "excluded values are unavailable before inner version 111001",
            ));
        }
        if self.format_version() < LOCAL_PARK_VERSION
            && self.offers.iter().any(|offer| offer.local_park.is_some())
        {
            return Err(invalid(
                0,
                "local-park values are unavailable before inner version 115001",
            ));
        }
        let offer_count = self
            .reason_counts
            .iter()
            .try_fold(0_usize, |total, count| {
                total
                    .checked_add(usize::from(*count))
                    .ok_or(CrpError::ArithmeticOverflow("TransferManager offer count"))
            })?;
        if offer_count != self.offers.len() {
            return Err(invalid(
                0,
                "TransferManager reason-count total must equal the existing offer count",
            ));
        }

        let original = &self.original;
        let mut output = Vec::with_capacity(original.original_bytes.len());
        append_direct_i32_column(
            &mut output,
            &self.incoming_amounts,
            &original.incoming_amounts,
            &original.columns.incoming_amounts,
        );
        append_direct_i32_column(
            &mut output,
            &self.outgoing_amounts,
            &original.outgoing_amounts,
            &original.columns.outgoing_amounts,
        );
        append_ushort_column(
            &mut output,
            &self.reason_counts,
            &original.reason_counts,
            &original.columns.reason_counts,
        );

        let active = self
            .offers
            .iter()
            .map(|offer| offer.active)
            .collect::<Vec<_>>();
        let original_active = original
            .offers
            .iter()
            .map(|offer| offer.active)
            .collect::<Vec<_>>();
        append_encoded_column(
            &mut output,
            &active,
            &original_active,
            &original.columns.active,
            write_encoded_bool_array,
        );

        if self.format_version() >= EXCLUDE_FLAG_VERSION {
            let excluded = collect_offer_values(&self.offers, |offer| offer.excluded)?;
            let original_excluded = collect_offer_values(&original.offers, |offer| offer.excluded)?;
            append_encoded_column(
                &mut output,
                &excluded,
                &original_excluded,
                original
                    .columns
                    .excluded
                    .as_deref()
                    .ok_or_else(|| invalid(0, "excluded column is missing"))?,
                write_encoded_bool_array,
            );
        }

        for (column, original_column, original_bytes) in [
            (
                self.offers
                    .iter()
                    .map(|offer| offer.priority)
                    .collect::<Vec<_>>(),
                original
                    .offers
                    .iter()
                    .map(|offer| offer.priority)
                    .collect::<Vec<_>>(),
                &original.columns.priority,
            ),
            (
                self.offers
                    .iter()
                    .map(|offer| offer.amount)
                    .collect::<Vec<_>>(),
                original
                    .offers
                    .iter()
                    .map(|offer| offer.amount)
                    .collect::<Vec<_>>(),
                &original.columns.amount,
            ),
            (
                self.offers.iter().map(|offer| offer.x).collect::<Vec<_>>(),
                original
                    .offers
                    .iter()
                    .map(|offer| offer.x)
                    .collect::<Vec<_>>(),
                &original.columns.x,
            ),
            (
                self.offers.iter().map(|offer| offer.z).collect::<Vec<_>>(),
                original
                    .offers
                    .iter()
                    .map(|offer| offer.z)
                    .collect::<Vec<_>>(),
                &original.columns.z,
            ),
            (
                self.offers
                    .iter()
                    .map(|offer| offer.instance_type)
                    .collect::<Vec<_>>(),
                original
                    .offers
                    .iter()
                    .map(|offer| offer.instance_type)
                    .collect::<Vec<_>>(),
                &original.columns.instance_type,
            ),
        ] {
            append_encoded_column(
                &mut output,
                &column,
                &original_column,
                original_bytes,
                write_encoded_byte_array,
            );
        }

        let instance_indexes = self
            .offers
            .iter()
            .map(|offer| offer.instance_index)
            .collect::<Vec<_>>();
        let original_instance_indexes = original
            .offers
            .iter()
            .map(|offer| offer.instance_index)
            .collect::<Vec<_>>();
        append_encoded_column(
            &mut output,
            &instance_indexes,
            &original_instance_indexes,
            &original.columns.instance_index,
            write_encoded_uint_array,
        );

        if self.format_version() >= LOCAL_PARK_VERSION {
            let local_park = collect_offer_values(&self.offers, |offer| offer.local_park)?;
            let original_local_park =
                collect_offer_values(&original.offers, |offer| offer.local_park)?;
            append_encoded_column(
                &mut output,
                &local_park,
                &original_local_park,
                original
                    .columns
                    .local_park
                    .as_deref()
                    .ok_or_else(|| invalid(0, "local-park column is missing"))?,
                write_encoded_byte_array,
            );
        }
        output.extend_from_slice(&original.columns.end_profiler_reference);
        Ok(output)
    }

    pub(crate) fn original_bytes(&self) -> &[u8] {
        &self.original.original_bytes
    }
}

fn collect_offer_values<T: Copy>(
    offers: &[TransferOffer],
    field: impl Fn(&TransferOffer) -> Option<T>,
) -> Result<Vec<T>, CrpError> {
    offers
        .iter()
        .map(|offer| field(offer).ok_or_else(|| invalid(0, "versioned offer column is missing")))
        .collect()
}

fn append_direct_i32_column(
    output: &mut Vec<u8>,
    values: &[i32],
    original_values: &[i32],
    original_bytes: &[u8],
) {
    if values == original_values {
        output.extend_from_slice(original_bytes);
    } else {
        for value in values {
            output.extend_from_slice(&value.to_be_bytes());
        }
    }
}

fn append_ushort_column(
    output: &mut Vec<u8>,
    values: &[u16],
    original_values: &[u16],
    original_bytes: &[u8],
) {
    if values == original_values {
        output.extend_from_slice(original_bytes);
    } else {
        write_encoded_ushort_array(output, values);
    }
}

fn append_encoded_column<T: PartialEq>(
    output: &mut Vec<u8>,
    values: &[T],
    original_values: &[T],
    original_bytes: &[u8],
    encode: impl FnOnce(&mut Vec<u8>, &[T]),
) {
    if values == original_values {
        output.extend_from_slice(original_bytes);
    } else {
        encode(output, values);
    }
}

fn read_direct_i32_array(reader: &mut Reader<'_>, count: usize) -> Result<Vec<i32>, CrpError> {
    let byte_count = count.checked_mul(4).ok_or(CrpError::ArithmeticOverflow(
        "TransferManager amount array length",
    ))?;
    if byte_count > reader.remaining() {
        return Err(reader.invalid(format!(
            "amount array needs {byte_count} bytes but only {} remain",
            reader.remaining()
        )));
    }
    (0..count).map(|_| reader.read_i32_be()).collect()
}

fn transfer_reason_count(format_version: u32) -> usize {
    match format_version {
        0..=224 => 64,
        225..=268 => 72,
        269..=285 => 80,
        286..=110_014 => 88,
        110_015..=111_003 => 96,
        _ => 128,
    }
}

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "TransferManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{TransferManagerData, transfer_reason_count};

    const FORMAT_VERSION: u32 = 116_000;

    fn append_i32_values(output: &mut Vec<u8>, values: &[i32]) {
        for value in values {
            output.extend_from_slice(&value.to_be_bytes());
        }
    }

    fn append_literal_ushort_values(output: &mut Vec<u8>, values: &[u16]) {
        for chunk in values.chunks(64) {
            output.push((chunk.len() - 1) as u8);
            for value in chunk {
                output.extend_from_slice(&value.to_be_bytes());
            }
        }
    }

    fn one_offer_body() -> Vec<u8> {
        let reason_count = transfer_reason_count(FORMAT_VERSION);
        let mut incoming = vec![0_i32; reason_count];
        incoming[0] = -2;
        let mut outgoing = vec![0_i32; reason_count];
        outgoing[1] = 0x0102_0304;
        let mut counts = vec![0_u16; reason_count * 16];
        counts[0] = 1;
        let mut body = Vec::new();
        append_i32_values(&mut body, &incoming);
        append_i32_values(&mut body, &outgoing);
        // Full literal blocks are legal and keep this fixture independent of
        // the crate's EncodedArray writer's compression choices.
        append_literal_ushort_values(&mut body, &counts);
        body.extend_from_slice(&[0, 1]); // Active Bool column.
        body.extend_from_slice(&[0, 0]); // Excluded Bool column.
        body.extend_from_slice(&[0, 6]); // Priority Byte column.
        body.extend_from_slice(&[0, 7]); // Amount Byte column.
        body.extend_from_slice(&[0, 8]); // X Byte column.
        body.extend_from_slice(&[0, 9]); // Z Byte column.
        body.extend_from_slice(&[0, 10]); // Instance type Byte column.
        body.extend_from_slice(&[0, 1, 2, 3, 4]); // Instance index UInt column.
        body.extend_from_slice(&[0, 11]); // Local-park Byte column.
        body.extend_from_slice(&[0x12, 0x34]); // Opaque closing profiler reference.
        body
    }

    #[test]
    fn decodes_reader_columns_and_preserves_noncanonical_source_bytes() {
        let body = one_offer_body();
        let data = TransferManagerData::decode(FORMAT_VERSION, &body).unwrap();
        assert_eq!(data.format_version(), FORMAT_VERSION);
        assert_eq!(data.incoming_amounts().len(), 128);
        assert_eq!(data.incoming_amounts()[0], -2);
        assert_eq!(data.outgoing_amounts()[1], 0x0102_0304);
        assert_eq!(data.reason_counts().len(), 16 * 128);
        assert_eq!(data.reason_counts()[0], 1);
        assert_eq!(data.offers().len(), 1);
        assert!(data.offers()[0].active);
        assert_eq!(data.offers()[0].excluded, Some(false));
        assert_eq!(data.offers()[0].priority, 6);
        assert_eq!(data.offers()[0].amount, 7);
        assert_eq!(data.offers()[0].x, 8);
        assert_eq!(data.offers()[0].z, 9);
        assert_eq!(data.offers()[0].instance_type, 10);
        assert_eq!(data.offers()[0].instance_index, 0x0102_0304);
        assert_eq!(data.offers()[0].local_park, Some(11));
        assert_eq!(data.encode_unchanged(), body);
    }

    #[test]
    fn editor_updates_fields_and_keeps_untouched_columns_verbatim() {
        let body = one_offer_body();
        let data = TransferManagerData::decode(FORMAT_VERSION, &body).unwrap();
        let mut editor = data.into_editor();
        editor.incoming_amounts_mut()[0] = 0x1122_3344;
        editor.offers_mut()[0].amount = 200;

        let mut expected = body.clone();
        expected[..4].copy_from_slice(&0x1122_3344_i32.to_be_bytes());
        let amount_column_start = expected.len() - 17;
        expected[amount_column_start..amount_column_start + 2].copy_from_slice(&[0, 200]);
        assert_eq!(editor.encode().unwrap(), expected);

        editor.reason_counts_mut()[0] = 0;
        assert!(editor.encode().is_err());
    }

    #[test]
    fn applies_reason_count_and_offer_field_version_gates() {
        assert_eq!(transfer_reason_count(224), 64);
        assert_eq!(transfer_reason_count(225), 72);
        assert_eq!(transfer_reason_count(269), 80);
        assert_eq!(transfer_reason_count(286), 88);
        assert_eq!(transfer_reason_count(110_015), 96);
        assert_eq!(transfer_reason_count(111_004), 128);

        let version = 111_000;
        let reason_count = transfer_reason_count(version);
        let mut body = vec![0; reason_count * 8];
        let counts = vec![0_u16; reason_count * 16];
        append_literal_ushort_values(&mut body, &counts);
        body.extend_from_slice(&[0x12, 0x34]);
        let data = TransferManagerData::decode(version, &body).unwrap();
        assert!(data.offers().is_empty());
        assert_eq!(data.encode_unchanged(), body);

        let old_editor = data.into_editor();
        assert!(old_editor.encode().is_ok());
    }

    #[test]
    fn rejects_impossible_offer_count_before_allocating_records() {
        let reason_count = transfer_reason_count(37);
        let mut body = vec![0; reason_count * 8];
        let mut counts = vec![0_u16; reason_count * 16];
        counts[0] = u16::MAX;
        append_literal_ushort_values(&mut body, &counts);
        body.extend_from_slice(&[0x12, 0x34]);
        assert!(TransferManagerData::decode(37, &body).is_err());

        let mut over_limit_counts = vec![0_u16; reason_count * 16];
        over_limit_counts[..16].fill(u16::MAX);
        let mut over_limit_body = vec![0; reason_count * 8];
        append_literal_ushort_values(&mut over_limit_body, &over_limit_counts);
        over_limit_body.extend_from_slice(&[0x12, 0x34]);
        assert!(TransferManagerData::decode(37, &over_limit_body).is_err());
    }

    #[test]
    fn rejects_truncation_extra_bytes_and_unsupported_versions() {
        let body = one_offer_body();
        assert!(TransferManagerData::decode(FORMAT_VERSION, &body[..body.len() - 1]).is_err());
        let mut with_extra = body;
        with_extra.push(0);
        assert!(TransferManagerData::decode(FORMAT_VERSION, &with_extra).is_err());
        assert!(TransferManagerData::decode(36, &[0; 2]).is_err());
    }
}
