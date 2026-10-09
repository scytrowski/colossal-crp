use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_uint_array, skip_encoded_ushort_array};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

const MAX_EVENT_SLOTS: usize = u16::MAX as usize + 1;

/// Object references extracted from one empty-record `EventManager+Data` body.
///
/// Event and route records are currently unsupported. The DLL reader and
/// writer candidates are both boundary-checked, alongside one counted
/// block variant observed in a local sample. That block is retained as raw
/// bytes because its field meaning and type are unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventManagerReferenceData {
    pub(crate) object_headers: Vec<NetObjectHeader>,
    pub(crate) unidentified_counted_block: Option<UnidentifiedCountedBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnidentifiedCountedBlock {
    pub(crate) count: u16,
    pub(crate) raw_headers: Vec<u8>,
    pub(crate) contains_only_null_headers: bool,
}

impl EventManagerReferenceData {
    pub(crate) fn decode_reader(
        format_version: u32,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        Self::decode_reader_layout(format_version, bytes, false)
    }

    /// Tries the reader fields plus a corpus-observed counted block immediately
    /// before its versioned tail fields. The block's meaning and type have not
    /// been identified in the supplied DLL, so its count and encoded bytes are
    /// retained separately from known object references.
    pub(crate) fn decode_reader_with_counted_references(
        format_version: u32,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        Self::decode_reader_layout(format_version, bytes, true)
    }

    fn decode_reader_layout(
        format_version: u32,
        bytes: &[u8],
        counted_reference_block: bool,
    ) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let event_capacity = read_capacity(&mut reader, "event")?;
        let event_flags = if format_version < 254 && event_capacity == MAX_EVENT_SLOTS {
            Vec::new()
        } else {
            read_encoded_uint_array(&mut reader, event_capacity - 1)?
        };
        reject_active_flags(&reader, &event_flags, "event")?;

        if format_version >= 121_005 {
            let route_capacity = read_capacity(&mut reader, "event route")?;
            let flags = read_encoded_uint_array(&mut reader, route_capacity - 1)?;
            reject_active_flags(&reader, &flags, "event route")?;
        }

        let milestone_count = usize::from(reader.read_u16_be()?);
        let type_data_count = if (108_002..109_000).contains(&format_version) {
            usize::from(reader.read_u16_be()?)
        } else {
            0
        };
        skip_event_prefab_scope(&mut reader, 0, milestone_count, type_data_count)?;

        let mut object_headers = read_object_headers(&mut reader, milestone_count)?;
        skip_type_data(&mut reader, format_version, type_data_count, false)?;

        if format_version >= 121_015 {
            skip_event_prefab_scope(&mut reader, 0, 0, 0)?;
        }
        let unidentified_counted_block = if counted_reference_block {
            let count = reader.read_u16_be()?;
            let block_start = reader.offset();
            let headers = read_object_headers(&mut reader, usize::from(count))?;
            let raw_headers = bytes
                .get(block_start..reader.offset())
                .ok_or_else(|| reader.invalid("EventManager counted block exceeds its body"))?
                .to_vec();
            Some(UnidentifiedCountedBlock {
                count,
                contains_only_null_headers: headers
                    .iter()
                    .all(|header| matches!(header, NetObjectHeader::Null)),
                raw_headers,
            })
        } else {
            None
        };
        read_reader_tail(&mut reader, format_version, &mut object_headers)?;
        finish(&mut reader, object_headers, unidentified_counted_block)
    }

    pub(crate) fn decode_writer(bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let event_capacity = read_capacity(&mut reader, "event")?;
        let event_flags = read_encoded_uint_array(&mut reader, event_capacity - 1)?;
        reject_active_flags(&reader, &event_flags, "event")?;

        let route_capacity = read_capacity(&mut reader, "event route")?;
        let route_flags = read_encoded_uint_array(&mut reader, route_capacity - 1)?;
        reject_active_flags(&reader, &route_flags, "event route")?;

        let milestone_count = usize::from(reader.read_u16_be()?);
        let type_data_count = usize::from(reader.read_u16_be()?);
        skip_event_prefab_scope(&mut reader, 0, milestone_count, type_data_count)?;

        let mut object_headers = read_object_headers(&mut reader, milestone_count)?;
        skip_type_data(&mut reader, 0, type_data_count, true)?;

        // The current writer opens the schedule-prefab scope at every version.
        skip_event_prefab_scope(&mut reader, 0, 0, 0)?;
        object_headers.push(read_net_object_header(&mut reader)?);
        object_headers.push(read_net_object_header(&mut reader)?);
        object_headers.push(read_net_object_header(&mut reader)?);
        reader.read_i16_be()?;
        reader.read_i32_be()?;
        finish(&mut reader, object_headers, None)
    }
}

fn read_capacity(reader: &mut Reader<'_>, description: &str) -> Result<usize, CrpError> {
    usize::from(reader.read_u16_be()?)
        .checked_add(1)
        .ok_or_else(|| reader.invalid(format!("{description} slot count overflowed")))
}

fn reject_active_flags(
    reader: &Reader<'_>,
    flags: &[u32],
    description: &str,
) -> Result<(), CrpError> {
    if flags.iter().any(|flag| *flag != 0) {
        return Err(reader.invalid(format!("active {description} records are unsupported")));
    }
    Ok(())
}

fn skip_event_prefab_scope(
    reader: &mut Reader<'_>,
    active_event_count: usize,
    milestone_count: usize,
    type_data_count: usize,
) -> Result<(), CrpError> {
    let index_count = active_event_count
        .checked_add(milestone_count)
        .and_then(|count| count.checked_add(type_data_count))
        .ok_or(CrpError::ArithmeticOverflow(
            "EventManager prefab index count",
        ))?;
    skip_encoded_ushort_array(reader, index_count)?;
    skip_prefab_names(reader)
}

fn skip_prefab_names(reader: &mut Reader<'_>) -> Result<(), CrpError> {
    let count = usize::from(reader.read_u16_be()?);
    for _ in 0..count {
        reader.read_unique_string()?;
    }
    Ok(())
}

fn read_object_headers(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<NetObjectHeader>, CrpError> {
    if count > reader.remaining() {
        return Err(reader.invalid(format!(
            "EventManager declares {count} object references but only {} bytes remain",
            reader.remaining()
        )));
    }
    (0..count).map(|_| read_net_object_header(reader)).collect()
}

fn skip_type_data(
    reader: &mut Reader<'_>,
    format_version: u32,
    count: usize,
    writer_layout: bool,
) -> Result<(), CrpError> {
    for _ in 0..count {
        reader.read_u8()?;
        if writer_layout
            || format_version >= 109_004
            || (108_004..109_000).contains(&format_version)
        {
            reader.read_bytes(3)?;
        }
    }
    Ok(())
}

fn read_reader_tail(
    reader: &mut Reader<'_>,
    format_version: u32,
    object_headers: &mut Vec<NetObjectHeader>,
) -> Result<(), CrpError> {
    if format_version >= 253 {
        object_headers.push(read_net_object_header(reader)?);
    }
    let band_fields_present =
        format_version >= 109_011 || (108_011..109_000).contains(&format_version);
    if band_fields_present {
        object_headers.push(read_net_object_header(reader)?);
        object_headers.push(read_net_object_header(reader)?);
    }
    if format_version >= 109_003 || (108_003..109_000).contains(&format_version) {
        reader.read_i16_be()?;
    }
    if format_version >= 121_027 {
        reader.read_i32_be()?;
    }
    Ok(())
}

fn finish(
    reader: &mut Reader<'_>,
    object_headers: Vec<NetObjectHeader>,
    unidentified_counted_block: Option<UnidentifiedCountedBlock>,
) -> Result<(EventManagerReferenceData, usize), CrpError> {
    reader.read_u16_be()?;
    let consumed = reader.offset();
    Ok((
        EventManagerReferenceData {
            object_headers,
            unidentified_counted_block,
        },
        consumed,
    ))
}

#[cfg(test)]
mod tests {
    use super::EventManagerReferenceData;
    use crate::net_manager_data::NetObjectHeader;

    fn empty_reader_layout(version: u32) -> Vec<u8> {
        let mut bytes = vec![0x00, 0x00]; // One event slot; its flag array is empty.
        if version >= 121_005 {
            bytes.extend_from_slice(&[0x00, 0x00]); // One route slot; no route flags.
        }
        bytes.extend_from_slice(&[0x00, 0x00]); // No unlock milestones.
        if (108_002..109_000).contains(&version) {
            bytes.extend_from_slice(&[0x00, 0x00]); // No modified type data.
        }
        bytes.extend_from_slice(&[0x00, 0x00]); // Empty EventInfo prefab names.
        if version >= 121_015 {
            bytes.extend_from_slice(&[0x00, 0x00]); // Empty route-schedule prefab names.
        }
        if version >= 253 {
            bytes.push(0xff);
        }
        if version >= 109_011 || (108_011..109_000).contains(&version) {
            bytes.extend_from_slice(&[0xff, 0xff]);
        }
        if version >= 109_003 || (108_003..109_000).contains(&version) {
            bytes.extend_from_slice(&[0x00, 0x00]);
        }
        if version >= 121_027 {
            bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        }
        bytes.extend_from_slice(&[0x12, 0x34]);
        bytes
    }

    #[test]
    fn reader_layout_obeys_event_manager_reference_and_route_gates() {
        for version in [
            252, 253, 108_001, 108_002, 108_003, 108_010, 108_011, 108_999, 109_000, 109_002,
            109_003, 109_011, 121_004, 121_005, 121_015,
        ] {
            let bytes = empty_reader_layout(version);
            let (decoded, consumed) =
                EventManagerReferenceData::decode_reader(version, &bytes).unwrap();
            assert_eq!(consumed, bytes.len(), "version {version}");
            let header_count = usize::from(version >= 253)
                + 2 * usize::from(version >= 109_011 || (108_011..109_000).contains(&version));
            let expected_headers = vec![NetObjectHeader::Null; header_count];
            assert_eq!(
                decoded.object_headers, expected_headers,
                "version {version}"
            );
        }

        let version = 121_027;
        let bytes = empty_reader_layout(version);
        let (decoded, consumed) =
            EventManagerReferenceData::decode_reader(version, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.object_headers, vec![NetObjectHeader::Null; 3]);
    }

    #[test]
    fn writer_layout_collects_milestone_and_tail_headers() {
        let mut bytes = vec![
            0x00, 0x00, // One event slot.
            0x00, 0x00, // One route slot.
            0x00, 0x01, // One milestone.
            0x00, 0x01, // One modified type.
            0x01, 0x00, 0x07, 0x00, 0x08, // Two prefab indexes.
            0x00, 0x00, // Empty prefab names.
            0xff, // Null milestone reference.
            0x03, 0x00, 0x00, 0x04, // Type-data fields.
            0x00, 0x00, // Empty route-schedule prefab names.
            0xfe, 0xfe, 0x80, 0x00, 0x00, // New match-preparing type with empty name.
            0xff, // Band-preparing reference.
            0xff, // Band-popularity reference.
            0x00, 0x09, // Band-popularity bonus.
            0x00, 0x00, 0x00, 0x0a, // Schedule count.
            0x12, 0x34, // Profiler end index.
        ];
        let expected_refs = vec![
            NetObjectHeader::Null,
            NetObjectHeader::NewType("".to_owned()),
            NetObjectHeader::Null,
            NetObjectHeader::Null,
        ];
        let (decoded, consumed) = EventManagerReferenceData::decode_writer(&bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.object_headers, expected_refs);
        assert_eq!(decoded.unidentified_counted_block, None);

        for end in 0..bytes.len() {
            assert!(
                EventManagerReferenceData::decode_writer(&bytes[..end]).is_err(),
                "{end}"
            );
        }
        bytes[0] = 0xff;
        assert!(EventManagerReferenceData::decode_writer(&bytes).is_err());
    }

    #[test]
    fn reader_layout_can_extract_boundary_verified_counted_reference_block() {
        let bytes = [
            0x00, 0x00, // One event slot.
            0x00, 0x00, // No unlock milestones.
            0x00, 0x00, // Empty prefab names.
            0x00, 0x01, 0xff, // One observed reference in the extra block.
            0xff, 0xff, 0xff, // Match-preparing and two band references.
            0x00, 0x00, // Band-popularity bonus.
            0x12, 0x34, // Profiler end index.
        ];
        let (decoded, consumed) =
            EventManagerReferenceData::decode_reader_with_counted_references(109_011, &bytes)
                .unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.object_headers, vec![NetObjectHeader::Null; 3]);
        let block = decoded.unidentified_counted_block.unwrap();
        assert_eq!(block.count, 1);
        assert_eq!(block.raw_headers, [0xff]);
        assert!(block.contains_only_null_headers);
    }

    #[test]
    fn unidentified_counted_block_retains_non_null_header_bytes() {
        let bytes = [
            0x00, 0x00, // One event slot.
            0x00, 0x00, // No unlock milestones.
            0x00, 0x00, // Empty prefab names.
            0x00, 0x01, 0xfe, 0xfe, 0x80, 0x00, 0x00, // One new-type header.
            0xff, 0xff, 0xff, // Match-preparing and two band references.
            0x00, 0x00, // Band-popularity bonus.
            0x12, 0x34, // Profiler end index.
        ];
        let (decoded, consumed) =
            EventManagerReferenceData::decode_reader_with_counted_references(109_011, &bytes)
                .unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.object_headers, vec![NetObjectHeader::Null; 3]);
        let block = decoded.unidentified_counted_block.unwrap();
        assert_eq!(block.count, 1);
        assert_eq!(block.raw_headers, [0xfe, 0xfe, 0x80, 0x00, 0x00]);
        assert!(!block.contains_only_null_headers);

        for end in 0..bytes.len() {
            assert!(
                EventManagerReferenceData::decode_reader_with_counted_references(
                    109_011,
                    &bytes[..end]
                )
                .is_err(),
                "truncated body at {end}"
            );
        }
        let mut excessive_count = bytes;
        excessive_count[6..8].copy_from_slice(&u16::MAX.to_be_bytes());
        assert!(
            EventManagerReferenceData::decode_reader_with_counted_references(
                109_011,
                &excessive_count
            )
            .is_err()
        );
    }

    #[test]
    fn type_data_ticket_offsets_include_the_108999_transition_version() {
        let bytes = [
            0x00, 0x00, // One event slot.
            0x00, 0x00, // No unlock milestones.
            0x00, 0x01, // One modified type.
            0x00, 0x00, 0x07, // One prefab index.
            0x00, 0x00, // Empty prefab names.
            0x03, 0x00, 0x00, 0x04, // Popularity byte and 24-bit ticket offset.
            0xff, 0xff, 0xff, // Match-preparing and two band references.
            0x00, 0x00, // Band-popularity bonus.
            0x12, 0x34, // Profiler end index.
        ];
        let (decoded, consumed) =
            EventManagerReferenceData::decode_reader(108_999, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.object_headers, vec![NetObjectHeader::Null; 3]);
    }

    #[test]
    fn reader_layout_rejects_active_records_and_truncated_fields() {
        let active_event = [
            0x00, 0x01, // Two event slots.
            0x00, 0x00, 0x00, 0x01, // One active event flag.
        ];
        assert!(EventManagerReferenceData::decode_reader(253, &active_event).is_err());

        let active_route = [
            0x00, 0x00, // One event slot.
            0x00, 0x01, // Two route slots.
            0x00, 0x00, 0x00, 0x00, 0x01, // One active route flag.
        ];
        assert!(EventManagerReferenceData::decode_reader(121_005, &active_route).is_err());

        let bytes = empty_reader_layout(109_011);
        for end in 0..bytes.len() {
            assert!(
                EventManagerReferenceData::decode_reader(109_011, &bytes[..end]).is_err(),
                "{end}"
            );
        }
    }
}
