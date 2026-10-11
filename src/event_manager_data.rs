use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_uint_array, skip_encoded_ushort_array};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};
use std::ops::Range;

const MAX_EVENT_SLOTS: usize = u16::MAX as usize + 1;
const FIRST_EVENT_RECORD_VERSION: u32 = 210;
const LAST_SUPPORTED_LEGACY_EVENT_RECORD_VERSION: u32 = 245;
const CUSTOM_EVENT_SEED_FLAG: u32 = 1 << 10;

/// One decoded active event from the legacy EventManager record layout.
///
/// This layout is supported for inner versions 210 through 245. Its four
/// pre-245 color words are retained raw because their meaning is not
/// established; version 245 stores the color as RGB bytes. Later active-event
/// layouts are not decoded yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventManagerEventRecord {
    /// One-based event slot represented by this record.
    pub slot_index: u16,
    /// Raw event-slot flags; the `0x400` bit selects a custom seed value.
    pub flags: u32,
    /// Event building index.
    pub building_index: u16,
    /// Event start frame.
    pub start_frame: u32,
    /// Four pre-245 32-bit color values, retained without interpretation.
    pub legacy_color_words: Option<[u32; 4]>,
    /// RGB color bytes stored from version 245.
    pub color_rgb: Option<[u8; 3]>,
    /// Optional custom event seed selected by flag bit `0x400`.
    pub custom_seed: Option<u64>,
}

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

/// Known fixed-width EventManager tail values selected by inner version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EventManagerTailData {
    /// Band popularity adjustment stored from the documented version gates.
    pub band_popularity_bonus: Option<i16>,
    /// Event schedule count stored from inner version 121027.
    pub event_schedule_count: Option<i32>,
}

/// Supported fields decoded from a bounded `EventManager+Data` body.
///
/// Legacy event records in versions 210–245 are decoded and retained. Later
/// active-event layouts and active routes remain unsupported. Reader, writer,
/// and one corpus-observed counted-block candidate are boundary-checked. The
/// counted block is retained as raw bytes because its meaning and type are
/// unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventManagerReferenceData {
    format_version: u32,
    pub(crate) active_event_records: Vec<EventManagerEventRecord>,
    active_event_record_ranges: Vec<Range<usize>>,
    pub(crate) object_headers: Vec<NetObjectHeader>,
    pub(crate) unidentified_counted_block: Option<UnidentifiedCountedBlock>,
    pub(crate) type_data: Vec<EventManagerTypeData>,
    pub(crate) tail_data: EventManagerTailData,
    type_data_has_ticket_price: bool,
    type_data_range: Range<usize>,
    band_popularity_bonus_range: Option<Range<usize>>,
    event_schedule_count_range: Option<Range<usize>>,
    pub(crate) serialized_body: Vec<u8>,
}

/// Editor for the fixed-width EventManager type-data table.
///
/// The table size and version-selected field widths remain unchanged. Legacy
/// event-record bytes, prefab assignments, and the unidentified counted block
/// are preserved as source bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventManagerTypeDataEditor {
    original: EventManagerReferenceData,
    type_data: Vec<EventManagerTypeData>,
}

/// Editor for existing legacy active EventManager records.
///
/// The record count, slot indexes, and activity flags are fixed. It edits only
/// same-width scalar values for inner versions 210–245 and preserves the
/// surrounding EventManager body bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventManagerEventRecordsEditor {
    original: EventManagerReferenceData,
    records: Vec<EventManagerEventRecord>,
}

/// Editor for existing versioned EventManager tail values.
///
/// It changes only fields present in the source body and preserves their
/// version-selected widths and surrounding bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventManagerTailEditor {
    original: EventManagerReferenceData,
    tail_data: EventManagerTailData,
}

impl EventManagerReferenceData {
    pub(crate) fn equivalent_reference_layout(&self, other: &Self) -> bool {
        self.active_event_records == other.active_event_records
            && self.object_headers == other.object_headers
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

    pub(crate) fn into_event_records_editor(self) -> EventManagerEventRecordsEditor {
        EventManagerEventRecordsEditor {
            records: self.active_event_records.clone(),
            original: self,
        }
    }

    pub(crate) fn into_tail_editor(self) -> EventManagerTailEditor {
        EventManagerTailEditor {
            tail_data: self.tail_data,
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
        let (active_event_records, active_event_record_ranges) =
            read_active_event_records(&mut reader, format_version, &event_flags)?;

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
        skip_event_prefab_scope(
            &mut reader,
            active_event_records.len(),
            milestone_count,
            type_data_count,
        )?;

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
        let (tail_data, band_popularity_bonus_range, event_schedule_count_range) =
            read_reader_tail(&mut reader, format_version, &mut object_headers)?;
        finish(
            &mut reader,
            format_version,
            active_event_records,
            active_event_record_ranges,
            object_headers,
            unidentified_counted_block,
            type_data,
            type_data_has_ticket_price,
            type_data_range,
            tail_data,
            band_popularity_bonus_range,
            event_schedule_count_range,
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
        let band_popularity_bonus_start = reader.offset();
        let band_popularity_bonus = reader.read_i16_be()?;
        let band_popularity_bonus_range = band_popularity_bonus_start..reader.offset();
        let event_schedule_count_start = reader.offset();
        let event_schedule_count = reader.read_i32_be()?;
        let event_schedule_count_range = event_schedule_count_start..reader.offset();
        finish(
            &mut reader,
            format_version,
            Vec::new(),
            Vec::new(),
            object_headers,
            None,
            type_data,
            type_data_has_ticket_price,
            type_data_range,
            EventManagerTailData {
                band_popularity_bonus: Some(band_popularity_bonus),
                event_schedule_count: Some(event_schedule_count),
            },
            Some(band_popularity_bonus_range),
            Some(event_schedule_count_range),
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

impl EventManagerEventRecordsEditor {
    /// Returns the inner DataSerializer version used for these records.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns existing records in ascending event-slot order.
    pub fn records(&self) -> &[EventManagerEventRecord] {
        &self.records
    }

    /// Returns mutable existing records without changing their slot topology.
    pub fn records_mut(&mut self) -> &mut [EventManagerEventRecord] {
        &mut self.records
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.records.len() != self.original.active_event_records.len()
            || self.records.len() != self.original.active_event_record_ranges.len()
        {
            return Err(invalid_event_record(
                0,
                "EventManager event record count cannot change in this editor",
            ));
        }
        let mut output = self.original.serialized_body.clone();
        for ((record, original_record), range) in self
            .records
            .iter()
            .zip(&self.original.active_event_records)
            .zip(&self.original.active_event_record_ranges)
        {
            if record.slot_index != original_record.slot_index
                || record.flags != original_record.flags
            {
                return Err(invalid_event_record(
                    range.start,
                    "EventManager event slot and activity flags cannot change in this editor",
                ));
            }
            let encoded = encode_event_record(self.original.format_version, record, range.start)?;
            if encoded.len() != range.len() {
                return Err(invalid_event_record(
                    range.start,
                    "EventManager event edit changed its serialized width",
                ));
            }
            output
                .get_mut(range.clone())
                .ok_or_else(|| {
                    invalid_event_record(range.start, "EventManager record range exceeds body")
                })?
                .copy_from_slice(&encoded);
        }
        Ok(output)
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original.serialized_body
    }
}

impl EventManagerTailEditor {
    /// Returns the inner DataSerializer version used to select tail fields.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns the known version-gated tail values.
    pub fn data(&self) -> EventManagerTailData {
        self.tail_data
    }

    /// Returns mutable version-gated tail values while retaining their gates.
    pub fn data_mut(&mut self) -> &mut EventManagerTailData {
        &mut self.tail_data
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut output = self.original.serialized_body.clone();
        match (
            self.tail_data.band_popularity_bonus,
            self.original.band_popularity_bonus_range.as_ref(),
        ) {
            (Some(value), Some(range)) => {
                let destination = output.get_mut(range.clone()).ok_or_else(|| {
                    invalid_event_tail(range.start, "band popularity field exceeds body")
                })?;
                if destination.len() != 2 {
                    return Err(invalid_event_tail(
                        range.start,
                        "band popularity field does not have its two-byte width",
                    ));
                }
                destination.copy_from_slice(&value.to_be_bytes());
            }
            (None, None) => {}
            _ => {
                return Err(invalid_event_tail(
                    self.original
                        .band_popularity_bonus_range
                        .as_ref()
                        .map_or(0, |range| range.start),
                    "band popularity value cannot be added or removed across its version gate",
                ));
            }
        }
        match (
            self.tail_data.event_schedule_count,
            self.original.event_schedule_count_range.as_ref(),
        ) {
            (Some(value), Some(range)) => {
                let destination = output.get_mut(range.clone()).ok_or_else(|| {
                    invalid_event_tail(range.start, "event schedule count exceeds body")
                })?;
                if destination.len() != 4 {
                    return Err(invalid_event_tail(
                        range.start,
                        "event schedule count does not have its four-byte width",
                    ));
                }
                destination.copy_from_slice(&value.to_be_bytes());
            }
            (None, None) => {}
            _ => {
                return Err(invalid_event_tail(
                    self.original
                        .event_schedule_count_range
                        .as_ref()
                        .map_or(0, |range| range.start),
                    "event schedule count cannot be added or removed across its version gate",
                ));
            }
        }
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

fn read_active_event_records(
    reader: &mut Reader<'_>,
    format_version: u32,
    flags: &[u32],
) -> Result<(Vec<EventManagerEventRecord>, Vec<Range<usize>>), CrpError> {
    let active_count = flags.iter().filter(|flag| **flag != 0).count();
    if active_count == 0 {
        return Ok((Vec::new(), Vec::new()));
    }
    if !(FIRST_EVENT_RECORD_VERSION..=LAST_SUPPORTED_LEGACY_EVENT_RECORD_VERSION)
        .contains(&format_version)
    {
        return Err(reader.invalid(format!(
            "active EventManager records are unsupported at inner version {format_version}"
        )));
    }

    let mut records = Vec::with_capacity(active_count);
    let mut record_ranges = Vec::with_capacity(active_count);
    for (slot, flags) in flags.iter().copied().enumerate() {
        if flags == 0 {
            continue;
        }
        let slot_index = u16::try_from(slot + 1)
            .map_err(|_| reader.invalid("EventManager event slot index exceeds u16"))?;
        let record_start = reader.offset();
        let building_index = reader.read_u16_be()?;
        let start_frame = reader.read_u32_be()?;
        let (legacy_color_words, color_rgb) = if format_version >= 245 {
            (
                None,
                Some([reader.read_u8()?, reader.read_u8()?, reader.read_u8()?]),
            )
        } else {
            (
                Some([
                    reader.read_u32_be()?,
                    reader.read_u32_be()?,
                    reader.read_u32_be()?,
                    reader.read_u32_be()?,
                ]),
                None,
            )
        };
        let custom_seed = if flags & CUSTOM_EVENT_SEED_FLAG != 0 {
            Some(reader.read_u64_be()?)
        } else {
            None
        };
        records.push(EventManagerEventRecord {
            slot_index,
            flags,
            building_index,
            start_frame,
            legacy_color_words,
            color_rgb,
            custom_seed,
        });
        record_ranges.push(record_start..reader.offset());
    }
    Ok((records, record_ranges))
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
) -> Result<
    (
        EventManagerTailData,
        Option<Range<usize>>,
        Option<Range<usize>>,
    ),
    CrpError,
> {
    if format_version >= 253 {
        object_headers.push(read_net_object_header(reader)?);
    }
    let band_fields_present =
        format_version >= 109_011 || (108_011..109_000).contains(&format_version);
    if band_fields_present {
        object_headers.push(read_net_object_header(reader)?);
        object_headers.push(read_net_object_header(reader)?);
    }
    let (band_popularity_bonus, band_popularity_bonus_range) =
        if format_version >= 109_003 || (108_003..109_000).contains(&format_version) {
            let start = reader.offset();
            let value = reader.read_i16_be()?;
            (Some(value), Some(start..reader.offset()))
        } else {
            (None, None)
        };
    let (event_schedule_count, event_schedule_count_range) = if format_version >= 121_027 {
        let start = reader.offset();
        let value = reader.read_i32_be()?;
        (Some(value), Some(start..reader.offset()))
    } else {
        (None, None)
    };
    Ok((
        EventManagerTailData {
            band_popularity_bonus,
            event_schedule_count,
        },
        band_popularity_bonus_range,
        event_schedule_count_range,
    ))
}

fn finish(
    reader: &mut Reader<'_>,
    format_version: u32,
    active_event_records: Vec<EventManagerEventRecord>,
    active_event_record_ranges: Vec<Range<usize>>,
    object_headers: Vec<NetObjectHeader>,
    unidentified_counted_block: Option<UnidentifiedCountedBlock>,
    type_data: Vec<EventManagerTypeData>,
    type_data_has_ticket_price: bool,
    type_data_range: Range<usize>,
    tail_data: EventManagerTailData,
    band_popularity_bonus_range: Option<Range<usize>>,
    event_schedule_count_range: Option<Range<usize>>,
    bytes: &[u8],
) -> Result<(EventManagerReferenceData, usize), CrpError> {
    reader.read_u16_be()?;
    let consumed = reader.offset();
    Ok((
        EventManagerReferenceData {
            format_version,
            active_event_records,
            active_event_record_ranges,
            object_headers,
            unidentified_counted_block,
            type_data,
            tail_data,
            type_data_has_ticket_price,
            type_data_range,
            band_popularity_bonus_range,
            event_schedule_count_range,
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

fn encode_event_record(
    format_version: u32,
    record: &EventManagerEventRecord,
    offset: usize,
) -> Result<Vec<u8>, CrpError> {
    let mut output = Vec::with_capacity(if format_version >= 245 { 17 } else { 30 });
    output.extend_from_slice(&record.building_index.to_be_bytes());
    output.extend_from_slice(&record.start_frame.to_be_bytes());
    match (
        format_version >= 245,
        record.legacy_color_words,
        record.color_rgb,
    ) {
        (false, Some(words), None) => {
            for word in words {
                output.extend_from_slice(&word.to_be_bytes());
            }
        }
        (true, None, Some(rgb)) => output.extend_from_slice(&rgb),
        (true, _, _) => {
            return Err(invalid_event_record(
                offset,
                "version-245 event record requires RGB bytes and no legacy color words",
            ));
        }
        (false, _, _) => {
            return Err(invalid_event_record(
                offset,
                "pre-245 event record requires four legacy color words and no RGB bytes",
            ));
        }
    }
    match (
        record.flags & CUSTOM_EVENT_SEED_FLAG != 0,
        record.custom_seed,
    ) {
        (true, Some(seed)) => output.extend_from_slice(&seed.to_be_bytes()),
        (false, None) => {}
        (true, None) => {
            return Err(invalid_event_record(
                offset,
                "custom-seed flag requires an event seed value",
            ));
        }
        (false, Some(_)) => {
            return Err(invalid_event_record(
                offset,
                "event seed value is unavailable without the custom-seed flag",
            ));
        }
    }
    Ok(output)
}

fn invalid_event_record(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "EventManager+Data event record".to_owned(),
        offset,
        reason: reason.into(),
    }
}

fn invalid_event_tail(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "EventManager+Data tail".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EventManagerEventRecord, EventManagerReferenceData, EventManagerTailData,
        EventManagerTypeData,
    };
    use crate::net_manager_data::NetObjectHeader;

    fn one_active_event(format_version: u32, flags: u32) -> Vec<u8> {
        let mut bytes = vec![
            0x00, 0x01, // Two event slots.
            0x00, // One literal event flag follows.
        ];
        bytes.extend_from_slice(&flags.to_be_bytes());
        bytes.extend_from_slice(&[0x12, 0x34]); // Building index.
        bytes.extend_from_slice(&0x0102_0304_u32.to_be_bytes()); // Start frame.
        if format_version >= 245 {
            bytes.extend_from_slice(&[0x11, 0x22, 0x33]);
        } else {
            for color_word in [0x1111_1111_u32, 0x2222_2222, 0x3333_3333, 0x4444_4444] {
                bytes.extend_from_slice(&color_word.to_be_bytes());
            }
        }
        if flags & (1 << 10) != 0 {
            bytes.extend_from_slice(&0x0102_0304_0506_0708_u64.to_be_bytes());
        }
        bytes.extend_from_slice(&[0x00, 0x00]); // No unlock milestones.
        bytes.extend_from_slice(&[0x00, 0x00, 0x07]); // One event prefab index.
        bytes.extend_from_slice(&[0x00, 0x00]); // Empty prefab names.
        bytes.extend_from_slice(&[0x12, 0x34]); // Profiler end index.
        bytes
    }

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

    #[test]
    fn legacy_event_records_decode_in_flag_order_and_preserve_raw_values() {
        let flags = 0x0000_0401;
        let bytes = one_active_event(244, flags);
        let (decoded, consumed) = EventManagerReferenceData::decode_reader(244, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(
            decoded.active_event_records,
            [EventManagerEventRecord {
                slot_index: 1,
                flags,
                building_index: 0x1234,
                start_frame: 0x0102_0304,
                legacy_color_words: Some([0x1111_1111, 0x2222_2222, 0x3333_3333, 0x4444_4444,]),
                color_rgb: None,
                custom_seed: Some(0x0102_0304_0506_0708),
            }]
        );

        let mut editor = decoded.into_event_records_editor();
        editor.records_mut()[0].building_index = 0xabcd;
        editor.records_mut()[0].start_frame = 0x0102_0305;
        editor.records_mut()[0].legacy_color_words =
            Some([0xaabb_ccdd, 0x2222_2222, 0x3333_3333, 0x4444_4444]);
        editor.records_mut()[0].custom_seed = Some(0x1122_3344_5566_7788);
        let expected = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0xab, 0xcd, 0x01, 0x02, 0x03, 0x05, 0xaa,
            0xbb, 0xcc, 0xdd, 0x22, 0x22, 0x22, 0x22, 0x33, 0x33, 0x33, 0x33, 0x44, 0x44, 0x44,
            0x44, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x00, 0x00, 0x00, 0x00, 0x07,
            0x00, 0x00, 0x12, 0x34,
        ];
        assert_eq!(editor.encode().unwrap(), expected);

        editor.records_mut()[0].flags ^= 1 << 10;
        assert!(editor.encode().is_err());

        for end in 0..bytes.len() {
            assert!(
                EventManagerReferenceData::decode_reader(244, &bytes[..end]).is_err(),
                "truncated legacy event body at {end}"
            );
        }
        assert!(EventManagerReferenceData::decode_reader(246, &bytes).is_err());
        assert!(EventManagerReferenceData::decode_reader(209, &bytes).is_err());
    }

    #[test]
    fn event_record_color_changes_to_rgb_at_version_245() {
        let flags = 0x0000_0401;
        let bytes = one_active_event(245, flags);
        let (decoded, consumed) = EventManagerReferenceData::decode_reader(245, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.active_event_records[0].legacy_color_words, None);
        assert_eq!(
            decoded.active_event_records[0].color_rgb,
            Some([0x11, 0x22, 0x33])
        );
        assert_eq!(
            decoded.active_event_records[0].custom_seed,
            Some(0x0102_0304_0506_0708)
        );
        let mut editor = decoded.into_event_records_editor();
        editor.records_mut()[0].color_rgb = Some([0xaa, 0xbb, 0xcc]);
        let expected = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x12, 0x34, 0x01, 0x02, 0x03, 0x04, 0xaa,
            0xbb, 0xcc, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x00, 0x00, 0x00, 0x00,
            0x07, 0x00, 0x00, 0x12, 0x34,
        ];
        assert_eq!(editor.encode().unwrap(), expected);
    }

    #[test]
    fn reader_tail_editor_obeys_bonus_and_schedule_count_gates() {
        let bytes = [
            0x00, 0x00, // One event slot.
            0x00, 0x00, // No unlock milestones.
            0x00, 0x00, // Empty prefab names.
            0xff, // Match-preparing reference.
            0xff, 0xff, // Band-preparing and band-popularity references.
            0x12, 0x34, // Band-popularity bonus.
            0x56, 0x78, // Profiler end index.
        ];
        let (decoded, consumed) =
            EventManagerReferenceData::decode_reader(109_011, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(
            decoded.tail_data,
            EventManagerTailData {
                band_popularity_bonus: Some(0x1234),
                event_schedule_count: None,
            }
        );
        let mut editor = decoded.into_tail_editor();
        editor.data_mut().band_popularity_bonus = Some(-2);
        let expected = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xfe, 0x56, 0x78,
        ];
        assert_eq!(editor.encode().unwrap(), expected);

        let bytes = [
            0x00, 0x00, // One event slot.
            0x00, 0x00, // One event route slot.
            0x00, 0x00, // No unlock milestones.
            0x00, 0x00, // Empty EventInfo names.
            0x00, 0x00, // Empty schedule prefab names.
            0xff, // Match-preparing reference.
            0xff, 0xff, // Band-preparing and band-popularity references.
            0x00, 0x04, // Band-popularity bonus.
            0x01, 0x02, 0x03, 0x04, // Event schedule count.
            0x12, 0x34, // Profiler end index.
        ];
        let (decoded, consumed) =
            EventManagerReferenceData::decode_reader(121_027, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded.tail_data.event_schedule_count, Some(0x0102_0304));
        let mut editor = decoded.into_tail_editor();
        editor.data_mut().band_popularity_bonus = Some(-3);
        editor.data_mut().event_schedule_count = Some(-2);
        let expected = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff,
            0xfd, 0xff, 0xff, 0xff, 0xfe, 0x12, 0x34,
        ];
        assert_eq!(editor.encode().unwrap(), expected);
    }
}
