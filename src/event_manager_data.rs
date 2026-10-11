use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_uint_array, skip_encoded_ushort_array};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};
use std::ops::Range;

const MAX_EVENT_SLOTS: usize = u16::MAX as usize + 1;

/// One versioned popularity and ticket-price override for an event type.
///
/// The ticket-price value remains a raw 24-bit integer because its signedness
/// and units are not established by the available format research.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventManagerTypeData {
    /// Signed popularity adjustment stored in the event type table.
    pub popularity_offset: i8,
    /// Raw 24-bit ticket-price adjustment when this version serializes it.
    pub ticket_price_offset: Option<u32>,
}

/// Object references extracted from an empty-record `EventManager+Data` body.
///
/// Event and route records are currently unsupported. Reader, writer, and
/// one corpus-observed counted-block candidate are boundary-checked. The
/// counted block is retained as raw bytes because its meaning and type are
/// unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventManagerReferenceData {
    format_version: u32,
    pub(crate) object_headers: Vec<NetObjectHeader>,
    pub(crate) unidentified_counted_block: Option<UnidentifiedCountedBlock>,
    pub(crate) type_data: Vec<EventManagerTypeData>,
    type_data_has_ticket_price: bool,
    type_data_range: Range<usize>,
    pub(crate) serialized_body: Vec<u8>,
}

/// Editor for the fixed-width EventManager type-data table.
///
/// The table size and version-selected field widths remain unchanged. Event
/// and route records, prefab assignments, and the unidentified counted block
/// are preserved as source bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventManagerTypeDataEditor {
    original: EventManagerReferenceData,
    type_data: Vec<EventManagerTypeData>,
}

impl EventManagerReferenceData {
    pub(crate) fn equivalent_reference_layout(&self, other: &Self) -> bool {
        self.object_headers == other.object_headers
            && self.unidentified_counted_block == other.unidentified_counted_block
            && self.type_data == other.type_data
            && (self.type_data.is_empty()
                || (self.type_data_range == other.type_data_range
                    && self.type_data_has_ticket_price == other.type_data_has_ticket_price))
    }

    pub(crate) fn into_type_data_editor(self) -> EventManagerTypeDataEditor {
        EventManagerTypeDataEditor {
            type_data: self.type_data.clone(),
            original: self,
        }
    }
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
        let type_data_start = reader.offset();
        let type_data = read_type_data(&mut reader, format_version, type_data_count, false)?;
        let type_data_has_ticket_price = type_data_ticket_price_present(format_version, false);
        let type_data_range = type_data_start..reader.offset();

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
        finish(
            &mut reader,
            format_version,
            object_headers,
            unidentified_counted_block,
            type_data,
            type_data_has_ticket_price,
            type_data_range,
            bytes,
        )
    }

    pub(crate) fn decode_writer(
        format_version: u32,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
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
        let type_data_start = reader.offset();
        let type_data = read_type_data(&mut reader, 0, type_data_count, true)?;
        let type_data_has_ticket_price = true;
        let type_data_range = type_data_start..reader.offset();

        // The current writer opens the schedule-prefab scope at every version.
        skip_event_prefab_scope(&mut reader, 0, 0, 0)?;
        object_headers.push(read_net_object_header(&mut reader)?);
        object_headers.push(read_net_object_header(&mut reader)?);
        object_headers.push(read_net_object_header(&mut reader)?);
        reader.read_i16_be()?;
        reader.read_i32_be()?;
        finish(
            &mut reader,
            format_version,
            object_headers,
            None,
            type_data,
            type_data_has_ticket_price,
            type_data_range,
            bytes,
        )
    }
}

impl EventManagerTypeDataEditor {
    /// Returns the DataSerializer version used to select the table layout.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns the event type overrides in their serialized order.
    pub fn entries(&self) -> &[EventManagerTypeData] {
        &self.type_data
    }

    /// Returns mutable entries while retaining the original table count.
    pub fn entries_mut(&mut self) -> &mut [EventManagerTypeData] {
        &mut self.type_data
    }

    /// Rewrites the fixed-width type-data table in a copy of the manager body.
    pub(crate) fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.type_data.len() != self.original.type_data.len() {
            return Err(invalid_type_data(
                self.original.type_data_range.start,
                "EventManager type-data count cannot change in this editor",
            ));
        }
        let encoded = encode_type_data(
            self.original.type_data_has_ticket_price,
            self.original.type_data_range.start,
            &self.type_data,
        )?;
        if encoded.len() != self.original.type_data_range.len() {
            return Err(invalid_type_data(
                self.original.type_data_range.start,
                "EventManager type-data edit changed its serialized width",
            ));
        }
        let mut output = self.original.serialized_body.clone();
        output
            .get_mut(self.original.type_data_range.clone())
            .ok_or_else(|| {
                invalid_type_data(
                    self.original.type_data_range.start,
                    "EventManager type-data range exceeds source body",
                )
            })?
            .copy_from_slice(&encoded);
        Ok(output)
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original.serialized_body
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

fn read_type_data(
    reader: &mut Reader<'_>,
    format_version: u32,
    count: usize,
    writer_layout: bool,
) -> Result<Vec<EventManagerTypeData>, CrpError> {
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let popularity_offset = reader.read_u8()? as i8;
        let ticket_price_offset = if type_data_ticket_price_present(format_version, writer_layout) {
            Some(reader.read_u24_be()?)
        } else {
            None
        };
        entries.push(EventManagerTypeData {
            popularity_offset,
            ticket_price_offset,
        });
    }
    Ok(entries)
}

fn encode_type_data(
    ticket_price_is_present: bool,
    base_offset: usize,
    entries: &[EventManagerTypeData],
) -> Result<Vec<u8>, CrpError> {
    let entry_width = if ticket_price_is_present { 4 } else { 1 };
    let capacity = entries
        .len()
        .checked_mul(entry_width)
        .ok_or(CrpError::ArithmeticOverflow(
            "EventManager type-data encoded capacity",
        ))?;
    let mut output = Vec::with_capacity(capacity);
    for (index, entry) in entries.iter().enumerate() {
        let entry_offset = index
            .checked_mul(entry_width)
            .and_then(|offset| base_offset.checked_add(offset))
            .ok_or(CrpError::ArithmeticOverflow(
                "EventManager type-data entry offset",
            ))?;
        output.push(entry.popularity_offset as u8);
        match (ticket_price_is_present, entry.ticket_price_offset) {
            (true, Some(value)) if value <= 0x00ff_ffff => {
                output.push((value >> 16) as u8);
                output.push((value >> 8) as u8);
                output.push(value as u8);
            }
            (true, Some(_)) => {
                return Err(invalid_type_data(
                    entry_offset,
                    "24-bit ticket-price offset exceeds 0xFFFFFF",
                ));
            }
            (true, None) => {
                return Err(invalid_type_data(
                    entry_offset,
                    "version-gated ticket-price offset is missing",
                ));
            }
            (false, None) => {}
            (false, Some(_)) => {
                return Err(invalid_type_data(
                    entry_offset,
                    "ticket-price offset is unavailable at this version",
                ));
            }
        }
    }
    Ok(output)
}

fn type_data_ticket_price_present(format_version: u32, writer_layout: bool) -> bool {
    writer_layout || format_version >= 109_004 || (108_004..109_000).contains(&format_version)
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
    format_version: u32,
    object_headers: Vec<NetObjectHeader>,
    unidentified_counted_block: Option<UnidentifiedCountedBlock>,
    type_data: Vec<EventManagerTypeData>,
    type_data_has_ticket_price: bool,
    type_data_range: Range<usize>,
    bytes: &[u8],
) -> Result<(EventManagerReferenceData, usize), CrpError> {
    reader.read_u16_be()?;
    let consumed = reader.offset();
    Ok((
        EventManagerReferenceData {
            format_version,
            object_headers,
            unidentified_counted_block,
            type_data,
            type_data_has_ticket_price,
            type_data_range,
            serialized_body: bytes[..consumed].to_vec(),
        },
        consumed,
    ))
}

fn invalid_type_data(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "EventManager+Data type table".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{EventManagerReferenceData, EventManagerTypeData};
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
        let (decoded, consumed) =
            EventManagerReferenceData::decode_writer(112_004, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.object_headers, expected_refs);
        assert_eq!(decoded.unidentified_counted_block, None);
        assert_eq!(decoded.type_data[0].popularity_offset, 3);
        assert_eq!(decoded.type_data[0].ticket_price_offset, Some(4));

        for end in 0..bytes.len() {
            assert!(
                EventManagerReferenceData::decode_writer(112_004, &bytes[..end]).is_err(),
                "{end}"
            );
        }
        bytes[0] = 0xff;
        assert!(EventManagerReferenceData::decode_writer(112_004, &bytes).is_err());
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
        assert_eq!(decoded.type_data[0].popularity_offset, 3);
        assert_eq!(decoded.type_data[0].ticket_price_offset, Some(4));
    }

    #[test]
    fn type_data_editor_writes_signed_popularity_and_raw_u24_price() {
        let bytes = [
            0x00, 0x00, // One event slot.
            0x00, 0x00, // No unlock milestones.
            0x00, 0x01, // One modified type.
            0x00, 0x12, 0x34, // One prefab index.
            0x00, 0x00, // Empty prefab names.
            0xfd, 0x01, 0x02, 0x03, // Popularity -3 and raw ticket value 0x010203.
            0xff, // Match-preparing reference.
            0x00, 0x09, // Popularity bonus.
            0x12, 0x34, // Profiler end index.
        ];
        let (decoded, consumed) =
            EventManagerReferenceData::decode_reader(108_004, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(
            decoded.type_data,
            [EventManagerTypeData {
                popularity_offset: -3,
                ticket_price_offset: Some(0x010203),
            }]
        );

        let mut editor = decoded.into_type_data_editor();
        editor.entries_mut()[0] = EventManagerTypeData {
            popularity_offset: -7,
            ticket_price_offset: Some(0x00ff_ffff),
        };
        let expected = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x34, 0x00, 0x00, 0xf9, 0xff, 0xff,
            0xff, 0xff, 0x00, 0x09, 0x12, 0x34,
        ];
        assert_eq!(editor.encode().unwrap(), expected);
    }

    #[test]
    fn type_data_editor_checks_version_gated_price_width() {
        let bytes = [
            0x00, 0x00, // One event slot.
            0x00, 0x00, // No unlock milestones.
            0x00, 0x01, // One modified type.
            0x00, 0x12, 0x34, // One prefab index.
            0x00, 0x00, // Empty prefab names.
            0xfd, // Popularity -3; no price before version 108004.
            0xff, // Match-preparing reference.
            0x00, 0x09, // Popularity bonus.
            0x12, 0x34, // Profiler end index.
        ];
        let (decoded, consumed) =
            EventManagerReferenceData::decode_reader(108_003, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.type_data[0].ticket_price_offset, None);
        let mut editor = decoded.into_type_data_editor();
        editor.entries_mut()[0].ticket_price_offset = Some(1);
        assert!(editor.encode().is_err());

        let bytes = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x34, 0x00, 0x00, 0xfd, 0x01, 0x02,
            0x03, 0xff, 0x00, 0x09, 0x12, 0x34,
        ];
        let (decoded, _) = EventManagerReferenceData::decode_reader(108_004, &bytes).unwrap();
        let mut editor = decoded.into_type_data_editor();
        editor.entries_mut()[0].ticket_price_offset = Some(0x0100_0000);
        assert!(editor.encode().is_err());
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
