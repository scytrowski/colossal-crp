use crate::CrpError;
use crate::data_serializer::{
    COMPACT_STRING_FLAG, Reader, write_unique_string, write_unique_string_with_mode,
};
use crate::encoded_array::{
    read_encoded_uint_array, read_encoded_ushort_array, write_encoded_uint_array,
    write_encoded_ushort_array,
};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};
use std::ops::Range;

const MAX_EVENT_SLOTS: usize = u16::MAX as usize + 1;
const FIRST_EVENT_RECORD_VERSION: u32 = 210;
const LAST_SUPPORTED_EVENT_RECORD_VERSION: u32 = 121_034;
const LAST_SUPPORTED_EVENT_ROUTE_VERSION: u32 = 121_034;
const CUSTOM_EVENT_SEED_FLAG: u32 = 1 << 10;
const MAX_RACE_EVENT_ENTRIES: usize = 1_000_000;
const MAX_EVENT_ROUTE_SCHEDULES: usize = 4;

/// One decoded active event from the versioned EventManager record layout.
///
/// This layout is supported for inner versions 210 through 121034. Its four
/// pre-245 color words are retained raw because their meaning is not
/// established; version 245 stores the color as RGB bytes. Version-gated event
/// values are exposed when present. The nested race-event data added at 120004
/// is retained as raw bytes after its known structure is boundary-checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventManagerEventRecord {
    /// One-based event slot represented by this record.
    pub slot_index: u16,
    /// Raw event-slot flags; the `0x400` bit selects a custom seed value.
    pub flags: u32,
    /// Event building index.
    pub building_index: u16,
    /// Event creation frame serialized from inner version 112021.
    pub created_frame: Option<u32>,
    /// Raw IEEE-754 bits for the two toga-party start times from version 112029.
    /// Storing bits preserves NaNs and other non-canonical float encodings.
    pub toga_party_start_time_bits: Option<[u32; 2]>,
    /// Event start frame.
    pub start_frame: u32,
    /// Expiry frame serialized from inner version 246.
    pub expiry_frame: Option<u32>,
    /// Ticket price serialized from inner version 249.
    pub ticket_price: Option<u16>,
    /// Raw `m_nextBuildingEvent` value serialized in versions 245–251.
    /// Its meaning is not established by the available research.
    pub next_building_event: Option<u16>,
    /// Four pre-245 32-bit color values, retained without interpretation.
    pub legacy_color_words: Option<[u32; 4]>,
    /// RGB color bytes stored from version 245.
    pub color_rgb: Option<[u8; 3]>,
    /// Reward amount and total serialized from inner version 250.
    pub reward_totals: Option<[u64; 2]>,
    /// Success and failure counts serialized from inner version 250.
    pub result_counts: Option<[u16; 2]>,
    /// Ticket income and total serialized from inner version 255.
    pub ticket_totals: Option<[u64; 2]>,
    /// Popularity adjustment serialized in the versioned 108005–108999 and
    /// 109005+ ranges.
    pub popularity_delta: Option<i16>,
    /// Raw security budget serialized in the versioned 108008–108999 and
    /// 109008+ ranges.
    pub security_budget: Option<u16>,
    /// Optional custom event seed selected by flag bit `0x400`.
    pub custom_seed: Option<u64>,
}

/// Known values decoded from an active nested `RaceEventData` block.
///
/// Fixed-width scalar fields, racer values, opaque fixed-size arrays, and
/// route-spline fields are editable. Three prefab scopes expose their
/// assignments and names with fixed counts and encoded widths. Other
/// unmodeled nested values remain preserved as raw source bytes. The fields
/// are available for EventManager inner versions 120004–121034.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventManagerRaceEventData {
    /// Existing racers in serialized order. The count and optional-reference
    /// presence are fixed.
    pub racers: Vec<EventManagerRaceRacer>,
    /// Opaque 24-byte state records in racer order.
    pub racer_state_records: Vec<[u8; 24]>,
    /// Schedule identifier.
    pub schedule_id: i32,
    /// Number of race laps.
    pub lap_count: i32,
    /// Raw race state value.
    pub state: i32,
    /// Number of frames spent in the current state.
    pub frames_in_state: i32,
    /// Race marshal identifier.
    pub race_marshal_id: u16,
    /// Opaque 12-byte track points in serialized order.
    pub track_path_points: Vec<[u8; 12]>,
    /// Original DataSerializer bool byte for the track-closed flag.
    pub track_closed_raw: u8,
    /// Opaque four-byte path-index records in racer order.
    pub racer_path_index_records: Vec<[u8; 4]>,
    /// Frame at which the event ends.
    pub event_end_frame: u32,
    /// Opaque fixed array containing four 16-bit finished-racer values.
    pub finished_racer_data: [u8; 8],
    /// Frames remaining until the next overtake.
    pub frames_to_next_overtake: i32,
    /// Opaque 16-byte overtake records in racer order.
    pub racer_overtake_state_records: Vec<[u8; 16]>,
    /// Opaque four-byte race-team member records.
    pub race_team_member_records: Vec<[u8; 4]>,
    /// Track path segment indexes in serialized order.
    pub track_path_segments: Vec<u16>,
    /// Opaque eight-byte racer oscillation records.
    pub racer_oscillation_records: Vec<[u8; 8]>,
    /// Opaque eight-byte racer state-timer records.
    pub racer_state_timer_records: Vec<[u8; 8]>,
    /// Tier value serialized from inner version 121032 onward.
    pub tier: Option<i32>,
    /// Spawned-entity count stored as a signed 32-bit value.
    pub spawned_entities: i32,
    /// Original DataSerializer bool byte for collision handling from version
    /// 121031.
    pub collisions_enabled_raw: Option<u8>,
    /// Prefab scope for per-racer `RaceTeamInfo` assignments.
    pub race_team_reference_scope: EventManagerRaceEventPrefabScope,
    /// Prefab scope for per-racer `ParadeGroupInfo` assignments.
    pub parade_group_reference_scope: EventManagerRaceEventPrefabScope,
    /// Prefab scope for the race-team collection.
    pub race_team_scope: EventManagerRaceEventPrefabScope,
    /// Route spline when the source presence flag is true. Presence, count,
    /// closed/open layout, and record counts cannot change in the editor.
    pub route_spline: Option<EventManagerRaceEventRouteSpline>,
}

/// Editable route spline fields from the legacy RaceEventData layout.
///
/// The serialized bezier count and closed flag select the record layout and
/// therefore remain fixed. Open splines store `count` records. Closed legacy
/// splines store `count - 1` records and a terminal record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventManagerRaceEventRouteSpline {
    /// Serialized bezier count.
    pub bezier_count: u32,
    /// Start/end building and node indexes in serialized order.
    pub indexes: [u16; 4],
    /// Original DataSerializer bool byte for the closed flag.
    pub closed_raw: u8,
    /// Serialized sub-bezier records, in route order.
    pub records: Vec<EventManagerRaceEventSubBezierRecord>,
    /// Final type/source record stored by a legacy closed spline.
    pub terminal_record: Option<EventManagerRaceEventSubBezierRecord>,
}

/// Existing prefab assignments and names for one `RaceEventData` scope.
///
/// Assignment count and name count are fixed. Index edits must preserve the
/// encoded-array width, and name edits must preserve the combined name-table
/// width so later race-event fields stay in place.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventManagerRaceEventPrefabScope {
    /// Prefab assignment indexes in serialized order.
    pub indexes: Vec<u16>,
    /// Names declared in this scope, including null entries.
    pub names: Vec<Option<String>>,
}

/// One serialized three-byte `RouteSpline` type/source record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventManagerRaceEventSubBezierRecord {
    /// Raw `SubBezierType` value. Known values are 0 through 4.
    pub kind: u8,
    /// Building or node source index.
    pub source: u16,
}

/// Known values from one racer entry in `RaceEventData`.
///
/// Fixed-width values and performer IDs can be edited while keeping the
/// existing counts. Names can be changed only when their existing nullness,
/// string encoding, and serialized width can be preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventManagerRaceRacer {
    /// Racer identifier.
    pub racer_id: u16,
    /// Raw racer-type value.
    pub racer_type: u16,
    /// Frame when this racer spawns.
    pub spawn_frame: u32,
    /// Parade-float identifier.
    pub parade_float: u16,
    /// Existing performer identifiers in serialized order.
    pub performer_ids: Vec<u16>,
    /// First path choice.
    pub first_path: u16,
    /// Second path choice.
    pub second_path: u16,
    /// Original DataSerializer bool byte for the paths-assigned flag.
    /// Only `1` is interpreted as true; other byte values are preserved raw.
    pub paths_assigned_raw: u8,
    /// Optional race-team reference index.
    pub race_team_index: Option<u16>,
    /// Racer name. Edits must preserve its existing nullness, encoding mode,
    /// and serialized width.
    pub name: Option<String>,
    /// Optional parade-group reference index.
    pub parade_group_index: Option<u16>,
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

/// One schedule attached to an active EventManager route.
///
/// Fields gated by inner version 121027 are `None` in earlier layouts. Names
/// retain their source compact or wide encoding when possible and may change
/// nullness or width through the route editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventManagerRouteSchedule {
    /// Schedule identifier serialized from inner version 121027.
    pub schedule_id: Option<i32>,
    /// Schedule tier.
    pub tier: u16,
    /// Number of laps.
    pub lap_count: u16,
    /// Raw ticket price value.
    pub ticket_price: u16,
    /// Optional schedule name, including a preserved null value.
    pub name: Option<String>,
    /// Schedule flags serialized from inner version 121027.
    pub flags: Option<u16>,
    /// Schedule frequency serialized from inner version 121027.
    pub frequency: Option<u8>,
    /// Schedule start day serialized from inner version 121027.
    pub start_day: Option<u8>,
    /// Schedule start month serialized from inner version 121027.
    pub start_month: Option<u8>,
}

/// One active EventManager route in ascending slot order.
///
/// The seven base indexes and version-gated route metadata are exposed as raw
/// values. Slot indexes, the activity pattern, and schedule counts are fixed
/// by the source body; raw route-flag bits can be edited without activating or
/// deactivating slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventManagerRouteRecord {
    /// One-based route slot represented by this record.
    pub slot_index: u16,
    /// Raw route-slot activity flags.
    pub flags: u32,
    /// Route policies serialized from inner version 121018.
    pub policies: Option<u32>,
    /// Seven route, building, and segment indexes in serialized order.
    pub indexes: [u16; 7],
    /// Route version serialized from inner version 121007.
    pub route_version: Option<u32>,
    /// Packed route color serialized from inner version 121007.
    pub color: Option<u32>,
    /// Start node serialized from inner version 121007.
    pub start_node: Option<u16>,
    /// End node serialized from inner version 121007.
    pub end_node: Option<u16>,
    /// Event prefab index serialized from inner version 121009.
    pub event_prefab_index: Option<u16>,
    /// Schedules attached to this route, in serialized order.
    pub schedules: Vec<EventManagerRouteSchedule>,
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
/// Active event records in versions 210–121034 are decoded and retained.
/// Race-event blocks are boundary-checked and retained raw, with selected
/// fixed-width scalar fields exposed for editing, including nonempty legacy
/// route splines. Active event routes from version 121005 are boundary-checked
/// through version 121034 and their known values are exposed for edits. The
/// legacy counted UShort block in race-event data is retained raw because its
/// meaning is unknown. Reader, writer, and one corpus-observed counted block
/// candidate are boundary-checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventManagerReferenceData {
    format_version: u32,
    event_flags: Vec<u32>,
    event_flags_range: Option<Range<usize>>,
    route_flags: Vec<u32>,
    route_flags_range: Option<Range<usize>>,
    pub(crate) active_event_records: Vec<EventManagerEventRecord>,
    active_event_record_ranges: Vec<Range<usize>>,
    active_event_race_data_ranges: Vec<Option<Range<usize>>>,
    pub(crate) active_event_race_data: Vec<Option<EventManagerRaceEventData>>,
    active_event_race_data_field_ranges: Vec<Option<EventManagerRaceEventDataFieldRanges>>,
    active_event_race_data_presence_bytes: Vec<Option<u8>>,
    pub(crate) active_event_routes: Vec<EventManagerRouteRecord>,
    active_event_route_ranges: Vec<Range<usize>>,
    route_schedule_name_ranges: Vec<Vec<Range<usize>>>,
    route_schedule_name_compact_modes: Vec<Vec<Option<bool>>>,
    route_schedule_prefab_scope: Option<EventPrefabScope>,
    pub(crate) object_headers: Vec<NetObjectHeader>,
    pub(crate) unidentified_counted_block: Option<UnidentifiedCountedBlock>,
    pub(crate) type_data: Vec<EventManagerTypeData>,
    pub(crate) prefab_indexes: Vec<u16>,
    prefab_index_group_counts: [usize; 3],
    prefab_scope_range: Range<usize>,
    prefab_names_range: Range<usize>,
    prefab_names: Vec<Option<String>>,
    prefab_name_ranges: Vec<Range<usize>>,
    prefab_name_compact_modes: Vec<Option<bool>>,
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

/// Editor for existing versioned active EventManager records.
///
/// The record count, slot indexes, activity pattern, and custom-seed presence
/// are fixed. It edits raw event-flag bits within those constraints and
/// same-width scalar values for inner versions 210–121034, including selected
/// `RaceEventData` fields, opaque fixed-size arrays, and racer-name contents
/// that retain their source encoding and width. Other nested race-event bytes
/// and the surrounding EventManager body are preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventManagerEventRecordsEditor {
    original: EventManagerReferenceData,
    records: Vec<EventManagerEventRecord>,
    race_event_data: Vec<Option<EventManagerRaceEventData>>,
}

/// Editor for existing EventManager routes and their schedules.
///
/// It edits route scalar values and schedule names for inner versions
/// 121005–121034. Route count, slot indexes, activity pattern, and schedule
/// counts are fixed. Raw route-flag bits can change while retaining activity.
/// Schedule names keep their source compact or wide encoding when possible.
/// Changed arrays and names can resize the manager body. Schedule prefab
/// indexes and other body bytes are preserved raw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventManagerRouteRecordsEditor {
    original: EventManagerReferenceData,
    routes: Vec<EventManagerRouteRecord>,
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

/// Editor for existing EventManager event, milestone, type-data, and route-schedule prefab data.
///
/// Event, milestone, and type-data indexes share one encoded UShort scope.
/// Schedule indexes use a second scope from version 121015. Changed scopes use
/// the crate's canonical UShort encoding and can change the manager body size.
/// Changed names retain their original compact or wide encoding when possible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventManagerPrefabIndicesEditor {
    original: EventManagerReferenceData,
    event_indexes: Vec<u16>,
    milestone_indexes: Vec<u16>,
    type_data_indexes: Vec<u16>,
    prefab_names: Vec<Option<String>>,
    route_schedule_indexes: Vec<u16>,
    route_schedule_names: Vec<Option<String>>,
}

impl EventManagerReferenceData {
    pub(crate) fn prefab_names(&self) -> &[Option<String>] {
        &self.prefab_names
    }

    pub(crate) fn route_schedule_indexes(&self) -> &[u16] {
        self.route_schedule_prefab_scope
            .as_ref()
            .map_or(&[], |scope| &scope.indexes)
    }

    pub(crate) fn route_schedule_names(&self) -> &[Option<String>] {
        self.route_schedule_prefab_scope
            .as_ref()
            .map_or(&[], |scope| &scope.names)
    }

    pub(crate) fn equivalent_reference_layout(&self, other: &Self) -> bool {
        self.active_event_records == other.active_event_records
            && self.equivalent_race_event_data(other)
            && self.equivalent_route_data(other)
            && self.object_headers == other.object_headers
            && self.unidentified_counted_block == other.unidentified_counted_block
            && self.type_data == other.type_data
            && self.prefab_indexes == other.prefab_indexes
            && self.prefab_names == other.prefab_names
            && (self.type_data.is_empty()
                || (self.type_data_range == other.type_data_range
                    && self.type_data_has_ticket_price == other.type_data_has_ticket_price))
    }

    fn equivalent_race_event_data(&self, other: &Self) -> bool {
        self.active_event_race_data_ranges.len() == other.active_event_race_data_ranges.len()
            && self.active_event_race_data == other.active_event_race_data
            && self.active_event_race_data_presence_bytes
                == other.active_event_race_data_presence_bytes
            && self
                .active_event_race_data_ranges
                .iter()
                .zip(&other.active_event_race_data_ranges)
                .all(|(left, right)| match (left, right) {
                    (None, None) => true,
                    (Some(left), Some(right)) => {
                        match (
                            self.serialized_body.get(left.clone()),
                            other.serialized_body.get(right.clone()),
                        ) {
                            (Some(left), Some(right)) => left == right,
                            _ => false,
                        }
                    }
                    _ => false,
                })
    }

    fn equivalent_route_data(&self, other: &Self) -> bool {
        self.active_event_routes == other.active_event_routes
            && self.active_event_route_ranges.len() == other.active_event_route_ranges.len()
            && self
                .active_event_route_ranges
                .iter()
                .zip(&other.active_event_route_ranges)
                .all(|(left, right)| {
                    match (
                        self.serialized_body.get(left.clone()),
                        other.serialized_body.get(right.clone()),
                    ) {
                        (Some(left), Some(right)) => left == right,
                        _ => false,
                    }
                })
            && match (
                self.route_schedule_prefab_scope.as_ref(),
                other.route_schedule_prefab_scope.as_ref(),
            ) {
                (None, None) => true,
                (Some(left), Some(right)) => {
                    match (
                        self.serialized_body.get(left.scope_range.clone()),
                        other.serialized_body.get(right.scope_range.clone()),
                    ) {
                        (Some(left), Some(right)) => left == right,
                        _ => false,
                    }
                }
                _ => false,
            }
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
            race_event_data: self.active_event_race_data.clone(),
            original: self,
        }
    }

    pub(crate) fn into_route_records_editor(self) -> EventManagerRouteRecordsEditor {
        EventManagerRouteRecordsEditor {
            routes: self.active_event_routes.clone(),
            original: self,
        }
    }

    pub(crate) fn into_tail_editor(self) -> EventManagerTailEditor {
        EventManagerTailEditor {
            tail_data: self.tail_data,
            original: self,
        }
    }

    pub(crate) fn into_prefab_indices_editor(self) -> EventManagerPrefabIndicesEditor {
        let [event_count, milestone_count, type_data_count] = self.prefab_index_group_counts;
        let event_indexes = self.prefab_indexes[..event_count].to_vec();
        let milestone_end = event_count + milestone_count;
        let milestone_indexes = self.prefab_indexes[event_count..milestone_end].to_vec();
        let type_data_indexes =
            self.prefab_indexes[milestone_end..milestone_end + type_data_count].to_vec();
        let prefab_names = self.prefab_names.clone();
        let route_schedule_indexes = self
            .route_schedule_prefab_scope
            .as_ref()
            .map_or_else(Vec::new, |scope| scope.indexes.clone());
        let route_schedule_names = self
            .route_schedule_prefab_scope
            .as_ref()
            .map_or_else(Vec::new, |scope| scope.names.clone());
        EventManagerPrefabIndicesEditor {
            original: self,
            event_indexes,
            milestone_indexes,
            type_data_indexes,
            prefab_names,
            route_schedule_indexes,
            route_schedule_names,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnidentifiedCountedBlock {
    pub(crate) count: u16,
    pub(crate) raw_headers: Vec<u8>,
    pub(crate) contains_only_null_headers: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EventPrefabScope {
    indexes: Vec<u16>,
    scope_range: Range<usize>,
    names_range: Range<usize>,
    names: Vec<Option<String>>,
    name_ranges: Vec<Range<usize>>,
    name_compact_modes: Vec<Option<bool>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EventManagerRaceEventDataFieldRanges {
    racers: Vec<EventManagerRaceRacerFieldRanges>,
    race_team_reference_scope: EventPrefabScope,
    parade_group_reference_scope: EventPrefabScope,
    race_team_scope: EventPrefabScope,
    tier: Option<Range<usize>>,
    spawned_entities: Range<usize>,
    legacy_counted_values: Option<Range<usize>>,
    collisions_enabled_raw: Option<Range<usize>>,
    racer_state_records: Range<usize>,
    schedule_id: Range<usize>,
    lap_count: Range<usize>,
    state: Range<usize>,
    frames_in_state: Range<usize>,
    race_marshal_id: Range<usize>,
    track_path_points: Range<usize>,
    track_closed_raw: Range<usize>,
    racer_path_index_records: Range<usize>,
    event_end_frame: Range<usize>,
    finished_racer_data: Range<usize>,
    frames_to_next_overtake: Range<usize>,
    racer_overtake_state_records: Range<usize>,
    race_team_member_records: Range<usize>,
    track_path_segments: Range<usize>,
    racer_oscillation_records: Range<usize>,
    racer_state_timer_records: Range<usize>,
    route_spline: Option<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EventManagerRaceRacerFieldRanges {
    racer_id: Range<usize>,
    racer_type: Range<usize>,
    spawn_frame: Range<usize>,
    parade_float: Range<usize>,
    performer_ids: Vec<Range<usize>>,
    first_path: Range<usize>,
    second_path: Range<usize>,
    paths_assigned_raw: Range<usize>,
    race_team_index: Option<Range<usize>>,
    name: Range<usize>,
    name_compact_mode: Option<bool>,
    parade_group_index: Option<Range<usize>>,
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
        let event_flags_start = reader.offset();
        let (event_flags, event_flags_range) =
            if format_version < 254 && event_capacity == MAX_EVENT_SLOTS {
                (Vec::new(), None)
            } else {
                let flags = read_encoded_uint_array(&mut reader, event_capacity - 1)?;
                (flags, Some(event_flags_start..reader.offset()))
            };
        let (
            active_event_records,
            active_event_record_ranges,
            active_event_race_data_ranges,
            active_event_race_data,
            active_event_race_data_field_ranges,
            active_event_race_data_presence_bytes,
        ) = read_active_event_records(&mut reader, format_version, &event_flags)?;

        let (
            route_flags,
            route_flags_range,
            active_event_routes,
            active_event_route_ranges,
            route_schedule_name_ranges,
            route_schedule_name_compact_modes,
            route_schedule_count,
        ) = if format_version >= 121_005 {
            let route_capacity = read_capacity(&mut reader, "event route")?;
            let route_flags_start = reader.offset();
            let route_flags = read_encoded_uint_array(&mut reader, route_capacity - 1)?;
            let route_flags_range = route_flags_start..reader.offset();
            let (routes, route_ranges, name_ranges, compact_modes, schedule_count) =
                read_active_event_routes(&mut reader, format_version, &route_flags)?;
            (
                route_flags,
                Some(route_flags_range),
                routes,
                route_ranges,
                name_ranges,
                compact_modes,
                schedule_count,
            )
        } else {
            (
                Vec::new(),
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                0,
            )
        };

        let milestone_count = usize::from(reader.read_u16_be()?);
        let type_data_count = if (108_002..109_000).contains(&format_version) {
            usize::from(reader.read_u16_be()?)
        } else {
            0
        };
        let prefab_index_group_counts =
            [active_event_records.len(), milestone_count, type_data_count];
        let prefab_scope = read_event_prefab_scope(
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

        let route_schedule_prefab_scope = if format_version >= 121_015 {
            Some(read_event_prefab_scope(
                &mut reader,
                0,
                0,
                route_schedule_count,
            )?)
        } else {
            None
        };
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
            event_flags,
            event_flags_range,
            route_flags,
            route_flags_range,
            active_event_records,
            active_event_record_ranges,
            active_event_race_data_ranges,
            active_event_race_data,
            active_event_race_data_field_ranges,
            active_event_race_data_presence_bytes,
            active_event_routes,
            active_event_route_ranges,
            route_schedule_name_ranges,
            route_schedule_name_compact_modes,
            route_schedule_prefab_scope,
            object_headers,
            unidentified_counted_block,
            type_data,
            type_data_has_ticket_price,
            type_data_range,
            prefab_scope,
            prefab_index_group_counts,
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
        let event_flags_start = reader.offset();
        let event_flags = read_encoded_uint_array(&mut reader, event_capacity - 1)?;
        reject_active_flags(&reader, &event_flags, "event")?;
        let event_flags_range = Some(event_flags_start..reader.offset());

        let route_capacity = read_capacity(&mut reader, "event route")?;
        let route_flags_start = reader.offset();
        let route_flags = read_encoded_uint_array(&mut reader, route_capacity - 1)?;
        reject_active_flags(&reader, &route_flags, "event route")?;
        let route_flags_range = Some(route_flags_start..reader.offset());

        let milestone_count = usize::from(reader.read_u16_be()?);
        let type_data_count = usize::from(reader.read_u16_be()?);
        let prefab_index_group_counts = [0, milestone_count, type_data_count];
        let prefab_scope =
            read_event_prefab_scope(&mut reader, 0, milestone_count, type_data_count)?;

        let mut object_headers = read_object_headers(&mut reader, milestone_count)?;
        let type_data_start = reader.offset();
        let type_data = read_type_data(&mut reader, 0, type_data_count, true)?;
        let type_data_has_ticket_price = true;
        let type_data_range = type_data_start..reader.offset();

        // The current writer opens the schedule-prefab scope at every version.
        let route_schedule_prefab_scope = Some(read_event_prefab_scope(&mut reader, 0, 0, 0)?);
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
            event_flags,
            event_flags_range,
            route_flags,
            route_flags_range,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            route_schedule_prefab_scope,
            object_headers,
            None,
            type_data,
            type_data_has_ticket_price,
            type_data_range,
            prefab_scope,
            prefab_index_group_counts,
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

    /// Replaces raw bits in one active event flag while preserving its slot
    /// activity and the `0x400` custom-seed presence bit.
    ///
    /// The index is zero-based among active event records in ascending slot
    /// order. The encoded flag array must retain its original byte width.
    pub fn set_event_flag_bits(&mut self, record_index: usize, value: u32) -> Result<(), CrpError> {
        let original = self
            .original
            .active_event_records
            .get(record_index)
            .ok_or_else(|| {
                invalid_event_record(record_index, "event record index is out of range")
            })?;
        let range = self
            .original
            .active_event_record_ranges
            .get(record_index)
            .ok_or_else(|| {
                invalid_event_record(
                    record_index,
                    "event record range is missing from the source",
                )
            })?;
        if (original.flags == 0) != (value == 0) {
            return Err(invalid_event_record(
                range.start,
                "EventManager flag edits must preserve the slot's active state",
            ));
        }
        if (original.flags ^ value) & CUSTOM_EVENT_SEED_FLAG != 0 {
            return Err(invalid_event_record(
                range.start,
                "EventManager flag edits must preserve custom-seed presence",
            ));
        }
        let record = self
            .records
            .get_mut(record_index)
            .ok_or_else(|| invalid_event_record(record_index, "edited event record is missing"))?;
        record.flags = value;
        Ok(())
    }

    /// Returns decoded race-event scalar values aligned with `records`.
    ///
    /// Entries are `None` when the record has no nested race-event block.
    pub fn race_event_data(&self) -> &[Option<EventManagerRaceEventData>] {
        &self.race_event_data
    }

    /// Returns mutable race-event scalar values without changing block presence.
    pub fn race_event_data_mut(&mut self) -> &mut [Option<EventManagerRaceEventData>] {
        &mut self.race_event_data
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.records.len() != self.original.active_event_records.len()
            || self.records.len() != self.original.active_event_record_ranges.len()
            || self.race_event_data.len() != self.original.active_event_race_data.len()
            || self.race_event_data.len() != self.original.active_event_race_data_field_ranges.len()
        {
            return Err(invalid_event_record(
                0,
                "EventManager event record count cannot change in this editor",
            ));
        }
        let mut output = self.original.serialized_body.clone();
        let mut event_flags = self.original.event_flags.clone();
        for (record_index, ((record, original_record), range)) in self
            .records
            .iter()
            .zip(&self.original.active_event_records)
            .zip(&self.original.active_event_record_ranges)
            .enumerate()
        {
            if record.slot_index != original_record.slot_index
                || (record.flags == 0) != (original_record.flags == 0)
                || (record.flags ^ original_record.flags) & CUSTOM_EVENT_SEED_FLAG != 0
            {
                return Err(invalid_event_record(
                    range.start,
                    "EventManager slot activity and custom-seed presence cannot change in this editor",
                ));
            }
            let flag_index = usize::from(record.slot_index).checked_sub(1).ok_or(
                CrpError::ArithmeticOverflow("EventManager event slot index"),
            )?;
            let source_flag = event_flags.get_mut(flag_index).ok_or_else(|| {
                invalid_event_record(
                    range.start,
                    "EventManager event flag is absent from the source array",
                )
            })?;
            *source_flag = record.flags;
            let mut encoded =
                encode_event_record(self.original.format_version, record, range.start)?;
            if let Some(presence_byte) = self
                .original
                .active_event_race_data_presence_bytes
                .get(record_index)
                .copied()
                .flatten()
            {
                encoded.push(presence_byte);
            }
            if let Some(race_range) = self
                .original
                .active_event_race_data_ranges
                .get(record_index)
                .and_then(Option::as_ref)
            {
                let race_data = self
                    .original
                    .serialized_body
                    .get(race_range.clone())
                    .ok_or_else(|| {
                        invalid_event_record(
                            range.start,
                            "EventManager race-event range exceeds source body",
                        )
                    })?;
                encoded.extend_from_slice(race_data);
            }
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

        if event_flags != self.original.event_flags {
            let range = self.original.event_flags_range.as_ref().ok_or_else(|| {
                invalid_event_record(0, "EventManager event flag array is absent from the source")
            })?;
            let mut encoded_flags = Vec::new();
            write_encoded_uint_array(&mut encoded_flags, &event_flags);
            if encoded_flags.len() != range.len() {
                return Err(invalid_event_record(
                    range.start,
                    "EventManager flag edit changes the encoded array width",
                ));
            }
            output
                .get_mut(range.clone())
                .ok_or_else(|| {
                    invalid_event_record(range.start, "EventManager flag range exceeds body")
                })?
                .copy_from_slice(&encoded_flags);
        }

        for (record_index, ((edited, race_range), field_ranges)) in self
            .race_event_data
            .iter()
            .zip(&self.original.active_event_race_data_ranges)
            .zip(&self.original.active_event_race_data_field_ranges)
            .enumerate()
        {
            match (edited, race_range, field_ranges) {
                (Some(data), Some(_), Some(ranges)) => {
                    let original_data = self
                        .original
                        .active_event_race_data
                        .get(record_index)
                        .and_then(Option::as_ref)
                        .ok_or_else(|| {
                            invalid_event_record(
                                ranges.schedule_id.start,
                                "race-event decoded values are missing from the source layout",
                            )
                        })?;
                    if data.racers.len() != ranges.racers.len()
                        || data.racers.len() != original_data.racers.len()
                    {
                        return Err(invalid_event_record(
                            ranges.schedule_id.start,
                            "race-event racer count cannot change in this editor",
                        ));
                    }
                    patch_race_event_prefab_scope(
                        &mut output,
                        &self.original.serialized_body,
                        &ranges.race_team_reference_scope,
                        &data.race_team_reference_scope,
                        "RaceTeamInfo reference",
                    )?;
                    patch_race_event_prefab_scope(
                        &mut output,
                        &self.original.serialized_body,
                        &ranges.parade_group_reference_scope,
                        &data.parade_group_reference_scope,
                        "ParadeGroupInfo reference",
                    )?;
                    patch_race_event_raw_records(
                        &mut output,
                        &ranges.racer_state_records,
                        &data.racer_state_records,
                        "racer state",
                    )?;
                    patch_race_event_raw_records(
                        &mut output,
                        &ranges.track_path_points,
                        &data.track_path_points,
                        "track path point",
                    )?;
                    patch_race_event_field(
                        &mut output,
                        &ranges.track_closed_raw,
                        &[data.track_closed_raw],
                        "track-closed flag",
                    )?;
                    patch_race_event_raw_records(
                        &mut output,
                        &ranges.racer_path_index_records,
                        &data.racer_path_index_records,
                        "racer path index",
                    )?;
                    match (data.tier, ranges.tier.as_ref()) {
                        (Some(value), Some(range)) => patch_race_event_field(
                            &mut output,
                            range,
                            &value.to_be_bytes(),
                            "tier",
                        )?,
                        (None, None) => {}
                        _ => {
                            return Err(invalid_event_record(
                                ranges.event_end_frame.start,
                                "race-event tier presence cannot change in this editor",
                            ));
                        }
                    }
                    patch_race_event_field(
                        &mut output,
                        &ranges.finished_racer_data,
                        &data.finished_racer_data,
                        "finished-racer data",
                    )?;
                    patch_race_event_raw_records(
                        &mut output,
                        &ranges.racer_overtake_state_records,
                        &data.racer_overtake_state_records,
                        "racer overtake state",
                    )?;
                    patch_race_event_raw_records(
                        &mut output,
                        &ranges.race_team_member_records,
                        &data.race_team_member_records,
                        "race-team member",
                    )?;
                    patch_race_event_u16_values(
                        &mut output,
                        &ranges.track_path_segments,
                        &data.track_path_segments,
                        "track path segment",
                    )?;
                    patch_race_event_raw_records(
                        &mut output,
                        &ranges.racer_oscillation_records,
                        &data.racer_oscillation_records,
                        "racer oscillation",
                    )?;
                    patch_race_event_raw_records(
                        &mut output,
                        &ranges.racer_state_timer_records,
                        &data.racer_state_timer_records,
                        "racer state timer",
                    )?;
                    if let Some(range) = ranges.legacy_counted_values.as_ref() {
                        let source_values = self
                            .original
                            .serialized_body
                            .get(range.clone())
                            .ok_or_else(|| {
                                invalid_event_record(
                                    range.start,
                                    "legacy race-event values exceed source body",
                                )
                            })?;
                        patch_race_event_field(
                            &mut output,
                            range,
                            source_values,
                            "legacy race-event values",
                        )?;
                    }
                    patch_race_event_field(
                        &mut output,
                        &ranges.spawned_entities,
                        &data.spawned_entities.to_be_bytes(),
                        "spawned-entity count",
                    )?;
                    patch_race_event_prefab_scope(
                        &mut output,
                        &self.original.serialized_body,
                        &ranges.race_team_scope,
                        &data.race_team_scope,
                        "race-team collection",
                    )?;
                    match (
                        data.route_spline.as_ref(),
                        original_data.route_spline.as_ref(),
                        ranges.route_spline.as_ref(),
                    ) {
                        (Some(spline), Some(original_spline), Some(range)) => {
                            let encoded = encode_race_event_route_spline(
                                self.original.format_version,
                                spline,
                                original_spline,
                                range.start,
                            )?;
                            patch_race_event_field(&mut output, range, &encoded, "route spline")?;
                        }
                        (None, None, None) => {}
                        _ => {
                            return Err(invalid_event_record(
                                ranges
                                    .route_spline
                                    .as_ref()
                                    .map_or(ranges.schedule_id.start, |range| range.start),
                                "race-event route-spline presence cannot change in this editor",
                            ));
                        }
                    }
                    match (
                        data.collisions_enabled_raw,
                        ranges.collisions_enabled_raw.as_ref(),
                    ) {
                        (Some(value), Some(range)) => {
                            patch_race_event_field(
                                &mut output,
                                range,
                                &[value],
                                "collision-enabled flag",
                            )?;
                        }
                        (None, None) => {}
                        _ => {
                            return Err(invalid_event_record(
                                ranges
                                    .collisions_enabled_raw
                                    .as_ref()
                                    .map_or(ranges.spawned_entities.start, |range| range.start),
                                "race-event collision-flag presence cannot change in this editor",
                            ));
                        }
                    }
                    for (racer_index, ((racer, original_racer), racer_ranges)) in data
                        .racers
                        .iter()
                        .zip(&original_data.racers)
                        .zip(&ranges.racers)
                        .enumerate()
                    {
                        if racer.performer_ids.len() != racer_ranges.performer_ids.len()
                            || racer.performer_ids.len() != original_racer.performer_ids.len()
                        {
                            return Err(invalid_event_record(
                                racer_ranges.racer_id.start,
                                format!(
                                    "race-event racer {racer_index} performer count cannot change"
                                ),
                            ));
                        }
                        if racer.name != original_racer.name {
                            let mut encoded_name = Vec::new();
                            match racer.name.as_deref() {
                                None => encoded_name.push(0xff),
                                Some(name) => match racer_ranges.name_compact_mode {
                                    Some(compact) => write_unique_string_with_mode(
                                        &mut encoded_name,
                                        name,
                                        compact,
                                    )?,
                                    None => write_unique_string(&mut encoded_name, name)?,
                                },
                            }
                            if encoded_name.len() != racer_ranges.name.len() {
                                return Err(invalid_event_record(
                                    racer_ranges.name.start,
                                    format!(
                                        "race-event racer {racer_index} name cannot change nullness or serialized width"
                                    ),
                                ));
                            }
                            patch_race_event_field(
                                &mut output,
                                &racer_ranges.name,
                                &encoded_name,
                                "racer name",
                            )?;
                        }
                        patch_race_event_field(
                            &mut output,
                            &racer_ranges.racer_id,
                            &racer.racer_id.to_be_bytes(),
                            "racer ID",
                        )?;
                        patch_race_event_field(
                            &mut output,
                            &racer_ranges.racer_type,
                            &racer.racer_type.to_be_bytes(),
                            "racer type",
                        )?;
                        patch_race_event_field(
                            &mut output,
                            &racer_ranges.spawn_frame,
                            &racer.spawn_frame.to_be_bytes(),
                            "racer spawn frame",
                        )?;
                        patch_race_event_field(
                            &mut output,
                            &racer_ranges.parade_float,
                            &racer.parade_float.to_be_bytes(),
                            "racer parade float",
                        )?;
                        for (performer_id, performer_range) in
                            racer.performer_ids.iter().zip(&racer_ranges.performer_ids)
                        {
                            patch_race_event_field(
                                &mut output,
                                performer_range,
                                &performer_id.to_be_bytes(),
                                "performer ID",
                            )?;
                        }
                        patch_race_event_field(
                            &mut output,
                            &racer_ranges.first_path,
                            &racer.first_path.to_be_bytes(),
                            "racer first path",
                        )?;
                        patch_race_event_field(
                            &mut output,
                            &racer_ranges.second_path,
                            &racer.second_path.to_be_bytes(),
                            "racer second path",
                        )?;
                        patch_race_event_field(
                            &mut output,
                            &racer_ranges.paths_assigned_raw,
                            &[racer.paths_assigned_raw],
                            "racer paths-assigned flag",
                        )?;
                        match (racer.race_team_index, racer_ranges.race_team_index.as_ref()) {
                            (Some(index), Some(range)) => patch_race_event_field(
                                &mut output,
                                range,
                                &index.to_be_bytes(),
                                "racer race-team index",
                            )?,
                            (None, None) => {}
                            _ => {
                                return Err(invalid_event_record(
                                    racer_ranges.racer_id.start,
                                    format!(
                                        "race-event racer {racer_index} race-team reference presence cannot change"
                                    ),
                                ));
                            }
                        }
                        match (
                            racer.parade_group_index,
                            racer_ranges.parade_group_index.as_ref(),
                        ) {
                            (Some(index), Some(range)) => patch_race_event_field(
                                &mut output,
                                range,
                                &index.to_be_bytes(),
                                "racer parade-group index",
                            )?,
                            (None, None) => {}
                            _ => {
                                return Err(invalid_event_record(
                                    racer_ranges.racer_id.start,
                                    format!(
                                        "race-event racer {racer_index} parade-group reference presence cannot change"
                                    ),
                                ));
                            }
                        }
                    }
                    patch_race_event_field(
                        &mut output,
                        &ranges.schedule_id,
                        &data.schedule_id.to_be_bytes(),
                        "schedule ID",
                    )?;
                    patch_race_event_field(
                        &mut output,
                        &ranges.lap_count,
                        &data.lap_count.to_be_bytes(),
                        "lap count",
                    )?;
                    patch_race_event_field(
                        &mut output,
                        &ranges.state,
                        &data.state.to_be_bytes(),
                        "state",
                    )?;
                    patch_race_event_field(
                        &mut output,
                        &ranges.frames_in_state,
                        &data.frames_in_state.to_be_bytes(),
                        "frames in state",
                    )?;
                    patch_race_event_field(
                        &mut output,
                        &ranges.race_marshal_id,
                        &data.race_marshal_id.to_be_bytes(),
                        "race marshal ID",
                    )?;
                    patch_race_event_field(
                        &mut output,
                        &ranges.event_end_frame,
                        &data.event_end_frame.to_be_bytes(),
                        "event end frame",
                    )?;
                    patch_race_event_field(
                        &mut output,
                        &ranges.frames_to_next_overtake,
                        &data.frames_to_next_overtake.to_be_bytes(),
                        "frames to next overtake",
                    )?;
                }
                (None, None, None) => {}
                _ => {
                    return Err(invalid_event_record(
                        self.original
                            .active_event_record_ranges
                            .get(record_index)
                            .map_or(0, |range| range.start),
                        "race-event block presence cannot change in this editor",
                    ));
                }
            }
        }
        Ok(output)
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original.serialized_body
    }
}

impl EventManagerRouteRecordsEditor {
    /// Returns the inner DataSerializer version used for these route records.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns active routes in ascending route-slot order.
    pub fn routes(&self) -> &[EventManagerRouteRecord] {
        &self.routes
    }

    /// Returns mutable routes without changing their slot topology.
    pub fn routes_mut(&mut self) -> &mut [EventManagerRouteRecord] {
        &mut self.routes
    }

    /// Replaces raw bits in one active route flag without changing slot
    /// activity.
    ///
    /// The index is zero-based among active routes in ascending slot order.
    /// The encoded flag array is rewritten canonically and may change width.
    pub fn set_route_flag_bits(&mut self, route_index: usize, value: u32) -> Result<(), CrpError> {
        let original = self
            .original
            .active_event_routes
            .get(route_index)
            .ok_or_else(|| invalid_event_route(route_index, "route index is out of range"))?;
        let range = self
            .original
            .active_event_route_ranges
            .get(route_index)
            .ok_or_else(|| {
                invalid_event_route(route_index, "route range is missing from the source")
            })?;
        if (original.flags == 0) != (value == 0) {
            return Err(invalid_event_route(
                range.start,
                "EventManager flag edits must preserve the slot's active state",
            ));
        }
        let route = self
            .routes
            .get_mut(route_index)
            .ok_or_else(|| invalid_event_route(route_index, "edited route is missing"))?;
        route.flags = value;
        Ok(())
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.routes.len() != self.original.active_event_routes.len()
            || self.routes.len() != self.original.active_event_route_ranges.len()
            || self.routes.len() != self.original.route_schedule_name_ranges.len()
            || self.routes.len() != self.original.route_schedule_name_compact_modes.len()
        {
            return Err(invalid_event_route(
                0,
                "EventManager route count cannot change in this editor",
            ));
        }
        let mut output = self.original.serialized_body.clone();
        let mut route_flags = self.original.route_flags.clone();
        let mut edits = Vec::with_capacity(self.routes.len());
        for (((route, original_route), range), (name_ranges, compact_modes)) in self
            .routes
            .iter()
            .zip(&self.original.active_event_routes)
            .zip(&self.original.active_event_route_ranges)
            .zip(
                self.original
                    .route_schedule_name_ranges
                    .iter()
                    .zip(&self.original.route_schedule_name_compact_modes),
            )
        {
            if route.slot_index != original_route.slot_index
                || (route.flags == 0) != (original_route.flags == 0)
            {
                return Err(invalid_event_route(
                    range.start,
                    "EventManager route slot and activity pattern cannot change in this editor",
                ));
            }
            let flag_index = usize::from(route.slot_index).checked_sub(1).ok_or(
                CrpError::ArithmeticOverflow("EventManager route slot index"),
            )?;
            let source_flag = route_flags.get_mut(flag_index).ok_or_else(|| {
                invalid_event_route(
                    range.start,
                    "EventManager route flag is absent from the source array",
                )
            })?;
            *source_flag = route.flags;
            if route.schedules.len() != original_route.schedules.len()
                || route.schedules.len() != name_ranges.len()
                || route.schedules.len() != compact_modes.len()
            {
                return Err(invalid_event_route(
                    range.start,
                    "EventManager route schedule count cannot change in this editor",
                ));
            }
            let encoded = encode_event_route(
                self.original.format_version,
                route,
                original_route,
                name_ranges,
                compact_modes,
                &self.original.serialized_body,
                range.start,
            )?;
            edits.push((range.clone(), encoded));
        }
        if route_flags != self.original.route_flags {
            let range = self.original.route_flags_range.as_ref().ok_or_else(|| {
                invalid_event_route(0, "EventManager route flag array is absent from the source")
            })?;
            let mut encoded_flags = Vec::new();
            write_encoded_uint_array(&mut encoded_flags, &route_flags);
            edits.insert(0, (range.clone(), encoded_flags));
        }
        for (range, encoded) in edits.into_iter().rev() {
            output.splice(range, encoded).for_each(drop);
        }
        Ok(output)
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original.serialized_body
    }
}

fn encode_event_route(
    format_version: u32,
    route: &EventManagerRouteRecord,
    original_route: &EventManagerRouteRecord,
    name_ranges: &[Range<usize>],
    compact_modes: &[Option<bool>],
    source_body: &[u8],
    offset: usize,
) -> Result<Vec<u8>, CrpError> {
    let mut output = Vec::new();
    match (format_version >= 121_018, route.policies) {
        (true, Some(value)) => output.extend_from_slice(&value.to_be_bytes()),
        (false, None) => {}
        (true, None) => {
            return Err(invalid_event_route(offset, "route policies are missing"));
        }
        (false, Some(_)) => {
            return Err(invalid_event_route(
                offset,
                "route policies are unavailable before version 121018",
            ));
        }
    }
    for index in route.indexes {
        output.extend_from_slice(&index.to_be_bytes());
    }
    match (
        format_version >= 121_007,
        route.route_version,
        route.color,
        route.start_node,
        route.end_node,
    ) {
        (true, Some(version), Some(color), Some(start), Some(end)) => {
            output.extend_from_slice(&version.to_be_bytes());
            output.extend_from_slice(&color.to_be_bytes());
            output.extend_from_slice(&start.to_be_bytes());
            output.extend_from_slice(&end.to_be_bytes());
        }
        (false, None, None, None, None) => {}
        (true, _, _, _, _) => {
            return Err(invalid_event_route(
                offset,
                "version-121007-or-later route metadata is incomplete",
            ));
        }
        (false, _, _, _, _) => {
            return Err(invalid_event_route(
                offset,
                "route metadata is unavailable before version 121007",
            ));
        }
    }
    match (format_version >= 121_009, route.event_prefab_index) {
        (true, Some(index)) => output.extend_from_slice(&index.to_be_bytes()),
        (false, None) => {}
        (true, None) => {
            return Err(invalid_event_route(offset, "event prefab index is missing"));
        }
        (false, Some(_)) => {
            return Err(invalid_event_route(
                offset,
                "event prefab index is unavailable before version 121009",
            ));
        }
    }
    if format_version >= 121_015 {
        let schedule_count = u8::try_from(route.schedules.len())
            .map_err(|_| invalid_event_route(offset, "route schedule count exceeds u8"))?;
        if usize::from(schedule_count) > MAX_EVENT_ROUTE_SCHEDULES {
            return Err(invalid_event_route(
                offset,
                format!("route schedule count exceeds {MAX_EVENT_ROUTE_SCHEDULES}"),
            ));
        }
        output.push(schedule_count);
    } else if !route.schedules.is_empty() {
        return Err(invalid_event_route(
            offset,
            "route schedules are unavailable before version 121015",
        ));
    }
    for ((schedule, original_schedule), (name_range, compact_mode)) in route
        .schedules
        .iter()
        .zip(&original_route.schedules)
        .zip(name_ranges.iter().zip(compact_modes))
    {
        let schedule_offset = name_range.start;
        match (format_version >= 121_027, schedule.schedule_id) {
            (true, Some(value)) => output.extend_from_slice(&value.to_be_bytes()),
            (false, None) => {}
            (true, None) => {
                return Err(invalid_event_route(
                    schedule_offset,
                    "schedule ID is missing",
                ));
            }
            (false, Some(_)) => {
                return Err(invalid_event_route(
                    schedule_offset,
                    "schedule ID is unavailable before version 121027",
                ));
            }
        }
        output.extend_from_slice(&schedule.tier.to_be_bytes());
        output.extend_from_slice(&schedule.lap_count.to_be_bytes());
        output.extend_from_slice(&schedule.ticket_price.to_be_bytes());
        if schedule.name == original_schedule.name {
            let raw_name = source_body.get(name_range.clone()).ok_or_else(|| {
                invalid_event_route(schedule_offset, "route schedule name exceeds source body")
            })?;
            output.extend_from_slice(raw_name);
        } else {
            match (&schedule.name, compact_mode) {
                (None, _) => output.push(0xff),
                (Some(name), Some(true))
                    if name.chars().all(|character| u32::from(character) < 256) =>
                {
                    write_unique_string_with_mode(&mut output, name, true)?;
                }
                (Some(name), Some(false)) => {
                    write_unique_string_with_mode(&mut output, name, false)?;
                }
                (Some(name), _) => write_unique_string(&mut output, name)?,
            }
        }
        match (
            format_version >= 121_027,
            schedule.flags,
            schedule.frequency,
            schedule.start_day,
            schedule.start_month,
        ) {
            (true, Some(flags), Some(frequency), Some(day), Some(month)) => {
                output.extend_from_slice(&flags.to_be_bytes());
                output.extend_from_slice(&[frequency, day, month]);
            }
            (false, None, None, None, None) => {}
            (true, _, _, _, _) => {
                return Err(invalid_event_route(
                    schedule_offset,
                    "version-121027-or-later schedule metadata is incomplete",
                ));
            }
            (false, _, _, _, _) => {
                return Err(invalid_event_route(
                    schedule_offset,
                    "schedule metadata is unavailable before version 121027",
                ));
            }
        }
    }
    Ok(output)
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

impl EventManagerPrefabIndicesEditor {
    /// Returns the inner DataSerializer version used for the prefab scope.
    pub fn format_version(&self) -> u32 {
        self.original.format_version
    }

    /// Returns existing active-event prefab indexes in event-slot order.
    pub fn event_indexes(&self) -> &[u16] {
        &self.event_indexes
    }

    /// Returns mutable active-event prefab indexes without changing their count.
    pub fn event_indexes_mut(&mut self) -> &mut [u16] {
        &mut self.event_indexes
    }

    /// Returns existing milestone prefab indexes in serialized order.
    pub fn milestone_indexes(&self) -> &[u16] {
        &self.milestone_indexes
    }

    /// Returns mutable milestone prefab indexes without changing their count.
    pub fn milestone_indexes_mut(&mut self) -> &mut [u16] {
        &mut self.milestone_indexes
    }

    /// Returns existing event-type prefab indexes in serialized order.
    pub fn type_data_indexes(&self) -> &[u16] {
        &self.type_data_indexes
    }

    /// Returns mutable event-type prefab indexes without changing their count.
    pub fn type_data_indexes_mut(&mut self) -> &mut [u16] {
        &mut self.type_data_indexes
    }

    /// Returns route-schedule prefab indexes in serialized schedule order.
    pub fn route_schedule_indexes(&self) -> &[u16] {
        &self.route_schedule_indexes
    }

    /// Returns mutable route-schedule prefab indexes without changing their count.
    pub fn route_schedule_indexes_mut(&mut self) -> &mut [u16] {
        &mut self.route_schedule_indexes
    }

    /// Returns the EventInfo prefab-name table, preserving null entries.
    pub fn prefab_names(&self) -> &[Option<String>] {
        &self.prefab_names
    }

    /// Returns the separate route-schedule EventInfo name table.
    pub fn route_schedule_names(&self) -> &[Option<String>] {
        &self.route_schedule_names
    }

    pub(crate) fn all_prefab_names(&self) -> &[Option<String>] {
        &self.prefab_names
    }

    pub(crate) fn all_route_schedule_indexes(&self) -> &[u16] {
        &self.route_schedule_indexes
    }

    pub(crate) fn all_route_schedule_names(&self) -> &[Option<String>] {
        &self.route_schedule_names
    }

    /// Replaces one existing EventInfo prefab name without changing the table count.
    ///
    /// The original compact or wide encoding is retained when it can represent
    /// the new value. Renaming a null entry chooses the standard encoding for
    /// its new value; setting an entry to `None` writes a null string marker.
    pub fn set_prefab_name(&mut self, index: usize, value: Option<String>) -> Result<(), CrpError> {
        let name = self
            .prefab_names
            .get_mut(index)
            .ok_or_else(|| invalid_event_prefabs(index, "prefab-name index is out of range"))?;
        *name = value;
        Ok(())
    }

    /// Replaces one existing route-schedule EventInfo name without changing its table count.
    pub fn set_route_schedule_name(
        &mut self,
        index: usize,
        value: Option<String>,
    ) -> Result<(), CrpError> {
        let name = self.route_schedule_names.get_mut(index).ok_or_else(|| {
            invalid_event_prefabs(index, "route-schedule prefab-name index is out of range")
        })?;
        *name = value;
        Ok(())
    }

    pub(crate) fn all_indexes(&self) -> Vec<u16> {
        let mut indexes = Vec::with_capacity(self.original.prefab_indexes.len());
        indexes.extend_from_slice(&self.event_indexes);
        indexes.extend_from_slice(&self.milestone_indexes);
        indexes.extend_from_slice(&self.type_data_indexes);
        indexes
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let indexes = self.all_indexes();
        let indexes_changed = indexes != self.original.prefab_indexes;
        let names_changed = self.prefab_names != self.original.prefab_names;
        let primary_scope_replacement = if indexes_changed || names_changed {
            let encoded_indexes = if indexes_changed {
                if indexes.len() != self.original.prefab_indexes.len() {
                    return Err(invalid_event_prefabs(
                        self.original.prefab_scope_range.start,
                        "EventManager prefab-index counts changed",
                    ));
                }
                let mut encoded = Vec::new();
                write_encoded_ushort_array(&mut encoded, &indexes);
                encoded
            } else {
                self.original
                    .serialized_body
                    .get(
                        self.original.prefab_scope_range.start
                            ..self.original.prefab_names_range.start,
                    )
                    .ok_or_else(|| {
                        invalid_event_prefabs(
                            self.original.prefab_scope_range.start,
                            "EventManager prefab index bytes exceed body",
                        )
                    })?
                    .to_vec()
            };
            let names = self.encode_prefab_names()?;
            let mut replacement_scope = encoded_indexes;
            replacement_scope.extend_from_slice(&names);
            Some(replacement_scope)
        } else {
            None
        };
        let route_schedule_scope_replacement =
            match self.original.route_schedule_prefab_scope.as_ref() {
                Some(scope)
                    if self.route_schedule_indexes != scope.indexes
                        || self.route_schedule_names != scope.names =>
                {
                    if self.route_schedule_indexes.len() != scope.indexes.len() {
                        return Err(invalid_event_prefabs(
                            scope.scope_range.start,
                            "route-schedule prefab-index counts changed",
                        ));
                    }
                    let encoded_indexes = if self.route_schedule_indexes == scope.indexes {
                        self.original
                            .serialized_body
                            .get(scope.scope_range.start..scope.names_range.start)
                            .ok_or_else(|| {
                                invalid_event_prefabs(
                                    scope.scope_range.start,
                                    "route-schedule prefab-index bytes exceed body",
                                )
                            })?
                            .to_vec()
                    } else {
                        let mut encoded = Vec::new();
                        write_encoded_ushort_array(&mut encoded, &self.route_schedule_indexes);
                        encoded
                    };
                    let names = encode_prefab_name_table(
                        &self.original.serialized_body,
                        scope.names_range.clone(),
                        &scope.names,
                        &scope.name_ranges,
                        &scope.name_compact_modes,
                        &self.route_schedule_names,
                    )?;
                    let mut replacement_scope = encoded_indexes;
                    replacement_scope.extend_from_slice(&names);
                    Some((scope.scope_range.clone(), replacement_scope))
                }
                Some(_) => None,
                None if self.route_schedule_indexes.is_empty()
                    && self.route_schedule_names.is_empty() =>
                {
                    None
                }
                None => {
                    return Err(invalid_event_prefabs(
                        0,
                        "route-schedule prefab data is unavailable before version 121015",
                    ));
                }
            };

        let mut output = self.original.serialized_body.clone();
        if let Some((range, replacement)) = route_schedule_scope_replacement {
            output.splice(range, replacement).for_each(drop);
        }
        if let Some(replacement) = primary_scope_replacement {
            output
                .splice(self.original.prefab_scope_range.clone(), replacement)
                .for_each(drop);
        }
        Ok(output)
    }

    fn encode_prefab_names(&self) -> Result<Vec<u8>, CrpError> {
        encode_prefab_name_table(
            &self.original.serialized_body,
            self.original.prefab_names_range.clone(),
            &self.original.prefab_names,
            &self.original.prefab_name_ranges,
            &self.original.prefab_name_compact_modes,
            &self.prefab_names,
        )
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original.serialized_body
    }
}

fn encode_prefab_name_table(
    source_body: &[u8],
    table_range: Range<usize>,
    original_names: &[Option<String>],
    name_ranges: &[Range<usize>],
    compact_modes: &[Option<bool>],
    edited_names: &[Option<String>],
) -> Result<Vec<u8>, CrpError> {
    if edited_names.len() != original_names.len()
        || name_ranges.len() != original_names.len()
        || compact_modes.len() != original_names.len()
    {
        return Err(invalid_event_prefabs(
            table_range.start,
            "EventManager prefab-name counts changed",
        ));
    }
    let source_table = source_body.get(table_range.clone()).ok_or_else(|| {
        invalid_event_prefabs(
            table_range.start,
            "EventManager prefab-name table exceeds body",
        )
    })?;
    let count_prefix = source_table.get(..2).ok_or_else(|| {
        invalid_event_prefabs(
            table_range.start,
            "EventManager prefab-name count is truncated",
        )
    })?;
    let mut output = Vec::with_capacity(source_table.len());
    output.extend_from_slice(count_prefix);
    let mut expected_source_start =
        table_range
            .start
            .checked_add(2)
            .ok_or(CrpError::ArithmeticOverflow(
                "EventManager prefab-name table start",
            ))?;
    for (index, name) in edited_names.iter().enumerate() {
        let range = &name_ranges[index];
        if range.start != expected_source_start || range.end < range.start {
            return Err(invalid_event_prefabs(
                range.start,
                "EventManager prefab-name ranges are not contiguous",
            ));
        }
        let raw_name = source_body.get(range.clone()).ok_or_else(|| {
            invalid_event_prefabs(range.start, "EventManager prefab name exceeds body")
        })?;
        if name == &original_names[index] {
            output.extend_from_slice(raw_name);
        } else {
            let mut encoded_name = Vec::new();
            match name {
                None => encoded_name.push(0xff),
                Some(value) => {
                    let can_keep_compact =
                        value.chars().all(|character| u32::from(character) < 256);
                    match compact_modes[index] {
                        Some(true) if can_keep_compact => {
                            write_unique_string_with_mode(&mut encoded_name, value, true)?;
                        }
                        Some(false) => {
                            write_unique_string_with_mode(&mut encoded_name, value, false)?;
                        }
                        _ => write_unique_string(&mut encoded_name, value)?,
                    }
                }
            }
            output.extend_from_slice(&encoded_name);
        }
        expected_source_start = range.end;
    }
    if expected_source_start != table_range.end {
        return Err(invalid_event_prefabs(
            expected_source_start,
            "EventManager prefab-name ranges do not cover the complete table",
        ));
    }
    Ok(output)
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

fn read_active_event_routes(
    reader: &mut Reader<'_>,
    format_version: u32,
    flags: &[u32],
) -> Result<
    (
        Vec<EventManagerRouteRecord>,
        Vec<Range<usize>>,
        Vec<Vec<Range<usize>>>,
        Vec<Vec<Option<bool>>>,
        usize,
    ),
    CrpError,
> {
    let active_count = flags.iter().filter(|flag| **flag != 0).count();
    if active_count > 0 && format_version > LAST_SUPPORTED_EVENT_ROUTE_VERSION {
        return Err(reader.invalid(format!(
            "active EventManager routes are unsupported at inner version {format_version}"
        )));
    }
    let mut routes = Vec::with_capacity(active_count);
    let mut route_ranges = Vec::with_capacity(active_count);
    let mut schedule_name_ranges = Vec::with_capacity(active_count);
    let mut schedule_name_compact_modes = Vec::with_capacity(active_count);
    let mut total_schedule_count = 0_usize;
    for (slot, flags) in flags.iter().copied().enumerate() {
        if flags == 0 {
            continue;
        }
        let slot_index = u16::try_from(slot + 1)
            .map_err(|_| reader.invalid("EventManager route slot index exceeds u16"))?;
        let start = reader.offset();
        let policies = if format_version >= 121_018 {
            Some(reader.read_u32_be()?)
        } else {
            None
        };
        let indexes = [
            reader.read_u16_be()?,
            reader.read_u16_be()?,
            reader.read_u16_be()?,
            reader.read_u16_be()?,
            reader.read_u16_be()?,
            reader.read_u16_be()?,
            reader.read_u16_be()?,
        ];
        let (route_version, color, start_node, end_node) = if format_version >= 121_007 {
            (
                Some(reader.read_u32_be()?),
                Some(reader.read_u32_be()?),
                Some(reader.read_u16_be()?),
                Some(reader.read_u16_be()?),
            )
        } else {
            (None, None, None, None)
        };
        let event_prefab_index = if format_version >= 121_009 {
            Some(reader.read_u16_be()?)
        } else {
            None
        };

        let schedule_count = if format_version >= 121_015 {
            usize::from(reader.read_u8()?)
        } else {
            0
        };
        if schedule_count > MAX_EVENT_ROUTE_SCHEDULES {
            return Err(reader.invalid(format!(
                "event route slot {} schedule count exceeds {MAX_EVENT_ROUTE_SCHEDULES}",
                slot + 1
            )));
        }
        total_schedule_count = total_schedule_count
            .checked_add(schedule_count)
            .ok_or_else(|| reader.invalid("event route schedule count overflowed"))?;
        let mut schedules = Vec::with_capacity(schedule_count);
        let mut route_name_ranges = Vec::with_capacity(schedule_count);
        let mut route_name_compact_modes = Vec::with_capacity(schedule_count);
        for _ in 0..schedule_count {
            let schedule_id = if format_version >= 121_027 {
                Some(reader.read_i32_be()?)
            } else {
                None
            };
            let tier = reader.read_u16_be()?;
            let lap_count = reader.read_u16_be()?;
            let ticket_price = reader.read_u16_be()?;
            let name_start = reader.offset();
            let name = reader.read_unique_string()?;
            let name_end = reader.offset();
            let compact_mode = if name.is_some() {
                let raw = reader
                    .consumed_bytes()
                    .get(name_start..name_end)
                    .ok_or_else(|| {
                        reader.invalid("EventManager route schedule name range is invalid")
                    })?;
                let first = *raw
                    .first()
                    .ok_or_else(|| reader.invalid("EventManager route schedule name is empty"))?;
                Some((u32::from(first) << 16) & COMPACT_STRING_FLAG != 0)
            } else {
                None
            };
            let (schedule_flags, frequency, start_day, start_month) = if format_version >= 121_027 {
                (
                    Some(reader.read_u16_be()?),
                    Some(reader.read_u8()?),
                    Some(reader.read_u8()?),
                    Some(reader.read_u8()?),
                )
            } else {
                (None, None, None, None)
            };
            schedules.push(EventManagerRouteSchedule {
                schedule_id,
                tier,
                lap_count,
                ticket_price,
                name,
                flags: schedule_flags,
                frequency,
                start_day,
                start_month,
            });
            route_name_ranges.push(name_start..name_end);
            route_name_compact_modes.push(compact_mode);
        }
        routes.push(EventManagerRouteRecord {
            slot_index,
            flags,
            policies,
            indexes,
            route_version,
            color,
            start_node,
            end_node,
            event_prefab_index,
            schedules,
        });
        route_ranges.push(start..reader.offset());
        schedule_name_ranges.push(route_name_ranges);
        schedule_name_compact_modes.push(route_name_compact_modes);
    }
    Ok((
        routes,
        route_ranges,
        schedule_name_ranges,
        schedule_name_compact_modes,
        total_schedule_count,
    ))
}

fn event_popularity_delta_present(format_version: u32) -> bool {
    (108_005..109_000).contains(&format_version) || format_version >= 109_005
}

fn event_security_budget_present(format_version: u32) -> bool {
    (108_008..109_000).contains(&format_version) || format_version >= 109_008
}

fn read_active_event_records(
    reader: &mut Reader<'_>,
    format_version: u32,
    flags: &[u32],
) -> Result<
    (
        Vec<EventManagerEventRecord>,
        Vec<Range<usize>>,
        Vec<Option<Range<usize>>>,
        Vec<Option<EventManagerRaceEventData>>,
        Vec<Option<EventManagerRaceEventDataFieldRanges>>,
        Vec<Option<u8>>,
    ),
    CrpError,
> {
    let active_count = flags.iter().filter(|flag| **flag != 0).count();
    if active_count == 0 {
        return Ok((
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ));
    }
    if !(FIRST_EVENT_RECORD_VERSION..=LAST_SUPPORTED_EVENT_RECORD_VERSION).contains(&format_version)
    {
        return Err(reader.invalid(format!(
            "active EventManager records are unsupported at inner version {format_version}"
        )));
    }

    let mut records = Vec::with_capacity(active_count);
    let mut record_ranges = Vec::with_capacity(active_count);
    let mut race_data_blocks = Vec::with_capacity(active_count);
    let mut race_event_data = Vec::with_capacity(active_count);
    let mut race_event_data_field_ranges = Vec::with_capacity(active_count);
    let mut race_data_presence_bytes = Vec::with_capacity(active_count);
    for (slot, flags) in flags.iter().copied().enumerate() {
        if flags == 0 {
            continue;
        }
        let slot_index = u16::try_from(slot + 1)
            .map_err(|_| reader.invalid("EventManager event slot index exceeds u16"))?;
        let record_start = reader.offset();
        let building_index = reader.read_u16_be()?;
        let created_frame = if format_version >= 112_021 {
            Some(reader.read_u32_be()?)
        } else {
            None
        };
        let toga_party_start_time_bits = if format_version >= 112_029 {
            Some([reader.read_u32_be()?, reader.read_u32_be()?])
        } else {
            None
        };
        let start_frame = reader.read_u32_be()?;
        let expiry_frame = if format_version >= 246 {
            Some(reader.read_u32_be()?)
        } else {
            None
        };
        let ticket_price = if format_version >= 249 {
            Some(reader.read_u16_be()?)
        } else {
            None
        };
        let next_building_event = if (245..252).contains(&format_version) {
            Some(reader.read_u16_be()?)
        } else {
            None
        };
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
        let ticket_totals = if format_version >= 255 {
            Some([reader.read_u64_be()?, reader.read_u64_be()?])
        } else {
            None
        };
        let reward_totals = if format_version >= 250 {
            Some([reader.read_u64_be()?, reader.read_u64_be()?])
        } else {
            None
        };
        let result_counts = if format_version >= 250 {
            Some([reader.read_u16_be()?, reader.read_u16_be()?])
        } else {
            None
        };
        let popularity_delta = if event_popularity_delta_present(format_version) {
            Some(reader.read_i16_be()?)
        } else {
            None
        };
        let security_budget = if event_security_budget_present(format_version) {
            Some(reader.read_u16_be()?)
        } else {
            None
        };
        let custom_seed = if flags & CUSTOM_EVENT_SEED_FLAG != 0 {
            Some(reader.read_u64_be()?)
        } else {
            None
        };
        let (race_data, race_fields, race_field_ranges, race_data_presence_byte) =
            if (120_004..=121_005).contains(&format_version) {
                let (range, fields, field_ranges) =
                    read_race_event_data_range(reader, format_version)?;
                (Some(range), Some(fields), Some(field_ranges), None)
            } else if (121_006..=LAST_SUPPORTED_EVENT_RECORD_VERSION).contains(&format_version) {
                let presence_byte = reader.read_u8()?;
                let (race_data, race_fields, race_field_ranges) =
                    if data_serializer_bool_is_true(presence_byte) {
                        let (range, fields, field_ranges) =
                            read_race_event_data_range(reader, format_version)?;
                        (Some(range), Some(fields), Some(field_ranges))
                    } else {
                        (None, None, None)
                    };
                (
                    race_data,
                    race_fields,
                    race_field_ranges,
                    Some(presence_byte),
                )
            } else {
                (None, None, None, None)
            };
        records.push(EventManagerEventRecord {
            slot_index,
            flags,
            building_index,
            created_frame,
            toga_party_start_time_bits,
            start_frame,
            expiry_frame,
            ticket_price,
            next_building_event,
            legacy_color_words,
            color_rgb,
            reward_totals,
            result_counts,
            ticket_totals,
            popularity_delta,
            security_budget,
            custom_seed,
        });
        record_ranges.push(record_start..reader.offset());
        race_data_blocks.push(race_data);
        race_event_data.push(race_fields);
        race_event_data_field_ranges.push(race_field_ranges);
        race_data_presence_bytes.push(race_data_presence_byte);
    }
    Ok((
        records,
        record_ranges,
        race_data_blocks,
        race_event_data,
        race_event_data_field_ranges,
        race_data_presence_bytes,
    ))
}

fn read_race_event_data_range(
    reader: &mut Reader<'_>,
    format_version: u32,
) -> Result<
    (
        Range<usize>,
        EventManagerRaceEventData,
        EventManagerRaceEventDataFieldRanges,
    ),
    CrpError,
> {
    let start = reader.offset();
    let (fields, field_ranges) = skip_race_event_data(reader, format_version)?;
    Ok((start..reader.offset(), fields, field_ranges))
}

fn data_serializer_bool_is_true(value: u8) -> bool {
    value == 1
}

fn skip_race_event_data(
    reader: &mut Reader<'_>,
    format_version: u32,
) -> Result<
    (
        EventManagerRaceEventData,
        EventManagerRaceEventDataFieldRanges,
    ),
    CrpError,
> {
    let racer_count = read_race_event_count(reader, "racer", 20)?;
    let mut race_team_reference_count = 0;
    let mut parade_group_reference_count = 0;
    let mut racers = Vec::with_capacity(racer_count);
    let mut racer_field_ranges = Vec::with_capacity(racer_count);
    for _ in 0..racer_count {
        let racer_id_start = reader.offset();
        let racer_id = reader.read_u16_be()?;
        let racer_id_range = racer_id_start..reader.offset();
        let racer_type_start = reader.offset();
        let racer_type = reader.read_u16_be()?;
        let racer_type_range = racer_type_start..reader.offset();
        let spawn_frame_start = reader.offset();
        let spawn_frame = reader.read_u32_be()?;
        let spawn_frame_range = spawn_frame_start..reader.offset();
        let parade_float_start = reader.offset();
        let parade_float = reader.read_u16_be()?;
        let parade_float_range = parade_float_start..reader.offset();
        let performer_count = usize::from(reader.read_u16_be()?);
        let mut performer_ids = Vec::with_capacity(performer_count);
        let mut performer_ranges = Vec::with_capacity(performer_count);
        for _ in 0..performer_count {
            let performer_start = reader.offset();
            performer_ids.push(reader.read_u16_be()?);
            performer_ranges.push(performer_start..reader.offset());
        }
        let first_path_start = reader.offset();
        let first_path = reader.read_u16_be()?;
        let first_path_range = first_path_start..reader.offset();
        let second_path_start = reader.offset();
        let second_path = reader.read_u16_be()?;
        let second_path_range = second_path_start..reader.offset();
        let paths_assigned_start = reader.offset();
        let paths_assigned_raw = reader.read_u8()?;
        let paths_assigned_range = paths_assigned_start..reader.offset();
        let race_team_presence = reader.read_u8()?;
        let (race_team_index, race_team_index_range) =
            if data_serializer_bool_is_true(race_team_presence) {
                let index_start = reader.offset();
                let index = reader.read_u16_be()?;
                race_team_reference_count += 1;
                (Some(index), Some(index_start..reader.offset()))
            } else {
                (None, None)
            };
        let name_start = reader.offset();
        let name = reader.read_unique_string()?;
        let name_range = name_start..reader.offset();
        let name_compact_mode = if name.is_some() {
            let raw_name = reader
                .consumed_bytes()
                .get(name_range.clone())
                .ok_or_else(|| reader.invalid("race-event racer name range is invalid"))?;
            let first = raw_name
                .first()
                .copied()
                .ok_or_else(|| reader.invalid("race-event racer name is empty"))?;
            Some((u32::from(first) << 16) & COMPACT_STRING_FLAG != 0)
        } else {
            None
        };
        let parade_group_presence = reader.read_u8()?;
        let (parade_group_index, parade_group_index_range) =
            if data_serializer_bool_is_true(parade_group_presence) {
                let index_start = reader.offset();
                let index = reader.read_u16_be()?;
                parade_group_reference_count += 1;
                (Some(index), Some(index_start..reader.offset()))
            } else {
                (None, None)
            };
        racers.push(EventManagerRaceRacer {
            racer_id,
            racer_type,
            spawn_frame,
            parade_float,
            performer_ids,
            first_path,
            second_path,
            paths_assigned_raw,
            race_team_index,
            name,
            parade_group_index,
        });
        racer_field_ranges.push(EventManagerRaceRacerFieldRanges {
            racer_id: racer_id_range,
            racer_type: racer_type_range,
            spawn_frame: spawn_frame_range,
            parade_float: parade_float_range,
            performer_ids: performer_ranges,
            first_path: first_path_range,
            second_path: second_path_range,
            paths_assigned_raw: paths_assigned_range,
            race_team_index: race_team_index_range,
            name: name_range,
            name_compact_mode,
            parade_group_index: parade_group_index_range,
        });
    }

    let race_team_reference_scope =
        read_event_prefab_scope(reader, race_team_reference_count, 0, 0)?;
    let parade_group_reference_scope =
        read_event_prefab_scope(reader, parade_group_reference_count, 0, 0)?;

    reader.read_u32_be()?; // Last spawn frame.
    reader.read_u32_be()?; // Next spawn frame.
    let (racer_state_records, racer_state_record_range) =
        read_race_event_raw_records::<24>(reader, racer_count, "racer state")?;

    let schedule_id_start = reader.offset();
    let schedule_id = reader.read_i32_be()?;
    let schedule_id_range = schedule_id_start..reader.offset();
    let lap_count_start = reader.offset();
    let lap_count = reader.read_i32_be()?;
    let lap_count_range = lap_count_start..reader.offset();
    let state_start = reader.offset();
    let state = reader.read_i32_be()?;
    let state_range = state_start..reader.offset();
    let frames_in_state_start = reader.offset();
    let frames_in_state = reader.read_i32_be()?;
    let frames_in_state_range = frames_in_state_start..reader.offset();
    let race_marshal_id_start = reader.offset();
    let race_marshal_id = reader.read_u16_be()?;
    let race_marshal_id_range = race_marshal_id_start..reader.offset();
    let track_path_length = read_race_event_count(reader, "track path point", 12)?;
    let (track_path_points, track_path_point_range) =
        read_race_event_raw_records::<12>(reader, track_path_length, "track path point")?;
    let track_closed_start = reader.offset();
    let track_closed_raw = reader.read_u8()?;
    let track_closed_range = track_closed_start..reader.offset();
    let (racer_path_index_records, racer_path_index_record_range) =
        read_race_event_raw_records::<4>(reader, racer_count, "racer path index")?;

    let (tier, tier_range) = if format_version >= 121_032 {
        let start = reader.offset();
        (Some(reader.read_i32_be()?), Some(start..reader.offset()))
    } else {
        (None, None)
    };
    let event_end_frame_start = reader.offset();
    let event_end_frame = reader.read_u32_be()?;
    let event_end_frame_range = event_end_frame_start..reader.offset();
    let finished_racer_start = reader.offset();
    let mut finished_racer_data = [0; 8];
    finished_racer_data.copy_from_slice(reader.read_bytes(8)?);
    let finished_racer_range = finished_racer_start..reader.offset();
    let frames_to_next_overtake_start = reader.offset();
    let frames_to_next_overtake = reader.read_i32_be()?;
    let frames_to_next_overtake_range = frames_to_next_overtake_start..reader.offset();
    let (racer_overtake_state_records, racer_overtake_state_record_range) =
        read_race_event_raw_records::<16>(reader, racer_count, "racer overtake state")?;

    let race_team_count = read_race_event_count(reader, "race team", 1)?;
    let race_team_scope = read_event_prefab_scope(reader, race_team_count, 0, 0)?;
    let race_team_member_count = read_race_event_count(reader, "race team member", 4)?;
    let (race_team_member_records, race_team_member_record_range) =
        read_race_event_raw_records::<4>(reader, race_team_member_count, "race team member")?;
    let mut track_path_segments = Vec::with_capacity(track_path_length);
    let track_path_segment_start = reader.offset();
    for _ in 0..track_path_length {
        track_path_segments.push(reader.read_u16_be()?);
    }
    let track_path_segment_range = track_path_segment_start..reader.offset();
    let (racer_oscillation_records, racer_oscillation_record_range) =
        read_race_event_raw_records::<8>(reader, racer_count, "racer oscillation")?;
    let (racer_state_timer_records, racer_state_timer_record_range) =
        read_race_event_raw_records::<8>(reader, racer_count, "racer state timer")?;
    let spawned_entities_start = reader.offset();
    let spawned_entities = reader.read_i32_be()?;
    let spawned_entities_range = spawned_entities_start..reader.offset();
    let legacy_counted_values = if format_version < 121_032 {
        let start = reader.offset();
        let count = read_race_event_count(reader, "legacy race-event value", 2)?;
        for _ in 0..count {
            reader.read_u16_be()?;
        }
        Some(start..reader.offset())
    } else {
        None
    };

    let route_spline = if data_serializer_bool_is_true(reader.read_u8()?) {
        let start = reader.offset();
        let route_spline = read_race_event_route_spline(reader, format_version)?;
        Some((route_spline, start..reader.offset()))
    } else {
        None
    };
    let (route_spline, route_spline_range) = match route_spline {
        Some((route_spline, range)) => (Some(route_spline), Some(range)),
        None => (None, None),
    };
    let (collisions_enabled_raw, collisions_enabled_range) = if format_version >= 121_031 {
        let start = reader.offset();
        (Some(reader.read_u8()?), Some(start..reader.offset()))
    } else {
        (None, None)
    };
    Ok((
        EventManagerRaceEventData {
            racers,
            racer_state_records,
            schedule_id,
            lap_count,
            state,
            frames_in_state,
            race_marshal_id,
            track_path_points,
            track_closed_raw,
            racer_path_index_records,
            event_end_frame,
            finished_racer_data,
            frames_to_next_overtake,
            racer_overtake_state_records,
            race_team_member_records,
            track_path_segments,
            racer_oscillation_records,
            racer_state_timer_records,
            race_team_reference_scope: EventManagerRaceEventPrefabScope {
                indexes: race_team_reference_scope.indexes.clone(),
                names: race_team_reference_scope.names.clone(),
            },
            parade_group_reference_scope: EventManagerRaceEventPrefabScope {
                indexes: parade_group_reference_scope.indexes.clone(),
                names: parade_group_reference_scope.names.clone(),
            },
            race_team_scope: EventManagerRaceEventPrefabScope {
                indexes: race_team_scope.indexes.clone(),
                names: race_team_scope.names.clone(),
            },
            tier,
            spawned_entities,
            collisions_enabled_raw,
            route_spline,
        },
        EventManagerRaceEventDataFieldRanges {
            racers: racer_field_ranges,
            race_team_reference_scope,
            parade_group_reference_scope,
            race_team_scope,
            tier: tier_range,
            spawned_entities: spawned_entities_range,
            legacy_counted_values,
            collisions_enabled_raw: collisions_enabled_range,
            racer_state_records: racer_state_record_range,
            schedule_id: schedule_id_range,
            lap_count: lap_count_range,
            state: state_range,
            frames_in_state: frames_in_state_range,
            race_marshal_id: race_marshal_id_range,
            track_path_points: track_path_point_range,
            track_closed_raw: track_closed_range,
            racer_path_index_records: racer_path_index_record_range,
            event_end_frame: event_end_frame_range,
            finished_racer_data: finished_racer_range,
            frames_to_next_overtake: frames_to_next_overtake_range,
            racer_overtake_state_records: racer_overtake_state_record_range,
            race_team_member_records: race_team_member_record_range,
            track_path_segments: track_path_segment_range,
            racer_oscillation_records: racer_oscillation_record_range,
            racer_state_timer_records: racer_state_timer_record_range,
            route_spline: route_spline_range,
        },
    ))
}

fn read_race_event_raw_records<const WIDTH: usize>(
    reader: &mut Reader<'_>,
    count: usize,
    description: &str,
) -> Result<(Vec<[u8; WIDTH]>, Range<usize>), CrpError> {
    if WIDTH == 0 {
        return Err(reader.invalid(format!(
            "{description} record has an invalid zero-byte width"
        )));
    }
    let byte_length = count
        .checked_mul(WIDTH)
        .ok_or_else(|| reader.invalid(format!("{description} byte length overflow")))?;
    let start = reader.offset();
    let bytes = reader
        .read_bytes(byte_length)
        .map_err(|error| reader.invalid(format!("truncated {description} records: {error}")))?;
    let mut records = Vec::with_capacity(count);
    for record_bytes in bytes.chunks_exact(WIDTH) {
        let mut record = [0; WIDTH];
        record.copy_from_slice(record_bytes);
        records.push(record);
    }
    Ok((records, start..reader.offset()))
}

fn read_race_event_route_spline(
    reader: &mut Reader<'_>,
    format_version: u32,
) -> Result<EventManagerRaceEventRouteSpline, CrpError> {
    let bezier_count = reader.read_u32_be()?;
    let serialized_bezier_count = usize::try_from(bezier_count)
        .map_err(|_| reader.invalid("route-spline count does not fit this platform"))?;
    if serialized_bezier_count > MAX_RACE_EVENT_ENTRIES {
        return Err(reader.invalid(format!(
            "route-spline count exceeds {MAX_RACE_EVENT_ENTRIES}"
        )));
    }
    let indexes = [
        reader.read_u16_be()?,
        reader.read_u16_be()?,
        reader.read_u16_be()?,
        reader.read_u16_be()?,
    ];
    let closed_raw = reader.read_u8()?;
    let closed = data_serializer_bool_is_true(closed_raw);

    let record_count = if format_version >= 121_034 {
        serialized_bezier_count
    } else if closed {
        serialized_bezier_count
            .checked_sub(1)
            .ok_or_else(|| reader.invalid("closed route-spline count must be nonzero"))?
    } else {
        // The legacy reader synthesizes start/end building entries around these records.
        serialized_bezier_count
    };
    if record_count > reader.remaining() / 3 {
        return Err(reader.invalid("route-spline records cannot fit in the remaining body"));
    }
    let mut records = Vec::with_capacity(record_count);
    for _ in 0..record_count {
        records.push(read_race_event_sub_bezier_record(reader)?);
    }
    let terminal_record = if format_version < 121_034 && closed {
        Some(read_race_event_sub_bezier_record(reader)?)
    } else {
        None
    };
    Ok(EventManagerRaceEventRouteSpline {
        bezier_count,
        indexes,
        closed_raw,
        records,
        terminal_record,
    })
}

fn read_race_event_sub_bezier_record(
    reader: &mut Reader<'_>,
) -> Result<EventManagerRaceEventSubBezierRecord, CrpError> {
    Ok(EventManagerRaceEventSubBezierRecord {
        kind: reader.read_u8()?,
        source: reader.read_u16_be()?,
    })
}

fn read_race_event_count(
    reader: &mut Reader<'_>,
    description: &str,
    minimum_bytes_per_entry: usize,
) -> Result<usize, CrpError> {
    let signed_count = reader.read_i32_be()?;
    let count = usize::try_from(signed_count)
        .map_err(|_| reader.invalid(format!("negative {description} count")))?;
    if count > MAX_RACE_EVENT_ENTRIES {
        return Err(reader.invalid(format!(
            "{description} count exceeds {MAX_RACE_EVENT_ENTRIES}"
        )));
    }
    if minimum_bytes_per_entry > 0 && count > reader.remaining() / minimum_bytes_per_entry {
        return Err(reader.invalid(format!(
            "{description} count cannot fit in the remaining body"
        )));
    }
    Ok(count)
}

fn read_event_prefab_scope(
    reader: &mut Reader<'_>,
    active_event_count: usize,
    milestone_count: usize,
    type_data_count: usize,
) -> Result<EventPrefabScope, CrpError> {
    let index_count = active_event_count
        .checked_add(milestone_count)
        .and_then(|count| count.checked_add(type_data_count))
        .ok_or(CrpError::ArithmeticOverflow(
            "EventManager prefab index count",
        ))?;
    if index_count > MAX_RACE_EVENT_ENTRIES {
        return Err(reader.invalid("EventManager prefab scope exceeds 1,000,000 indexes"));
    }
    let scope_start = reader.offset();
    let indexes = read_encoded_ushort_array(reader, index_count)?;
    let names_start = reader.offset();
    let (names, name_ranges, name_compact_modes) = read_prefab_names(reader)?;
    let scope_end = reader.offset();
    Ok(EventPrefabScope {
        indexes,
        scope_range: scope_start..scope_end,
        names_range: names_start..scope_end,
        names,
        name_ranges,
        name_compact_modes,
    })
}

fn read_prefab_names(
    reader: &mut Reader<'_>,
) -> Result<(Vec<Option<String>>, Vec<Range<usize>>, Vec<Option<bool>>), CrpError> {
    let count = usize::from(reader.read_u16_be()?);
    let mut names = Vec::with_capacity(count);
    let mut ranges = Vec::with_capacity(count);
    let mut compact_modes = Vec::with_capacity(count);
    for _ in 0..count {
        let start = reader.offset();
        let name = reader.read_unique_string()?;
        let end = reader.offset();
        let compact = if name.is_some() {
            let raw = reader
                .consumed_bytes()
                .get(start..end)
                .ok_or_else(|| reader.invalid("EventManager prefab-name range is invalid"))?;
            let first = *raw
                .first()
                .ok_or_else(|| reader.invalid("EventManager prefab name is empty"))?;
            Some((u32::from(first) << 16) & COMPACT_STRING_FLAG != 0)
        } else {
            None
        };
        names.push(name);
        ranges.push(start..end);
        compact_modes.push(compact);
    }
    Ok((names, ranges, compact_modes))
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
    event_flags: Vec<u32>,
    event_flags_range: Option<Range<usize>>,
    route_flags: Vec<u32>,
    route_flags_range: Option<Range<usize>>,
    active_event_records: Vec<EventManagerEventRecord>,
    active_event_record_ranges: Vec<Range<usize>>,
    active_event_race_data_ranges: Vec<Option<Range<usize>>>,
    active_event_race_data: Vec<Option<EventManagerRaceEventData>>,
    active_event_race_data_field_ranges: Vec<Option<EventManagerRaceEventDataFieldRanges>>,
    active_event_race_data_presence_bytes: Vec<Option<u8>>,
    active_event_routes: Vec<EventManagerRouteRecord>,
    active_event_route_ranges: Vec<Range<usize>>,
    route_schedule_name_ranges: Vec<Vec<Range<usize>>>,
    route_schedule_name_compact_modes: Vec<Vec<Option<bool>>>,
    route_schedule_prefab_scope: Option<EventPrefabScope>,
    object_headers: Vec<NetObjectHeader>,
    unidentified_counted_block: Option<UnidentifiedCountedBlock>,
    type_data: Vec<EventManagerTypeData>,
    type_data_has_ticket_price: bool,
    type_data_range: Range<usize>,
    prefab_scope: EventPrefabScope,
    prefab_index_group_counts: [usize; 3],
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
            event_flags,
            event_flags_range,
            route_flags,
            route_flags_range,
            active_event_records,
            active_event_record_ranges,
            active_event_race_data_ranges,
            active_event_race_data,
            active_event_race_data_field_ranges,
            active_event_race_data_presence_bytes,
            active_event_routes,
            active_event_route_ranges,
            route_schedule_name_ranges,
            route_schedule_name_compact_modes,
            route_schedule_prefab_scope,
            object_headers,
            unidentified_counted_block,
            type_data,
            prefab_indexes: prefab_scope.indexes,
            prefab_index_group_counts,
            prefab_scope_range: prefab_scope.scope_range,
            prefab_names_range: prefab_scope.names_range,
            prefab_names: prefab_scope.names,
            prefab_name_ranges: prefab_scope.name_ranges,
            prefab_name_compact_modes: prefab_scope.name_compact_modes,
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

fn patch_race_event_field(
    output: &mut [u8],
    range: &Range<usize>,
    value: &[u8],
    description: &str,
) -> Result<(), CrpError> {
    if range.len() != value.len() {
        return Err(invalid_event_record(
            range.start,
            format!("race-event {description} has an unexpected serialized width"),
        ));
    }
    output
        .get_mut(range.clone())
        .ok_or_else(|| {
            invalid_event_record(
                range.start,
                format!("race-event {description} range exceeds source body"),
            )
        })?
        .copy_from_slice(value);
    Ok(())
}

fn patch_race_event_raw_records<const WIDTH: usize>(
    output: &mut [u8],
    range: &Range<usize>,
    records: &[[u8; WIDTH]],
    description: &str,
) -> Result<(), CrpError> {
    if WIDTH == 0 {
        return Err(invalid_event_record(
            range.start,
            format!("race-event {description} has an invalid zero-byte record width"),
        ));
    }
    let byte_length = records
        .len()
        .checked_mul(WIDTH)
        .ok_or(CrpError::ArithmeticOverflow(
            "EventManager race record length",
        ))?;
    if byte_length != range.len() {
        return Err(invalid_event_record(
            range.start,
            format!("race-event {description} count cannot change in this editor"),
        ));
    }
    let destination = output.get_mut(range.clone()).ok_or_else(|| {
        invalid_event_record(
            range.start,
            format!("race-event {description} range exceeds source body"),
        )
    })?;
    for (destination_record, record) in destination.chunks_exact_mut(WIDTH).zip(records) {
        destination_record.copy_from_slice(record);
    }
    Ok(())
}

fn patch_race_event_u16_values(
    output: &mut [u8],
    range: &Range<usize>,
    values: &[u16],
    description: &str,
) -> Result<(), CrpError> {
    let byte_length = values
        .len()
        .checked_mul(2)
        .ok_or(CrpError::ArithmeticOverflow(
            "EventManager race segment length",
        ))?;
    if byte_length != range.len() {
        return Err(invalid_event_record(
            range.start,
            format!("race-event {description} count cannot change in this editor"),
        ));
    }
    let destination = output.get_mut(range.clone()).ok_or_else(|| {
        invalid_event_record(
            range.start,
            format!("race-event {description} range exceeds source body"),
        )
    })?;
    for (destination_value, value) in destination.chunks_exact_mut(2).zip(values) {
        destination_value.copy_from_slice(&value.to_be_bytes());
    }
    Ok(())
}

fn patch_race_event_prefab_scope(
    output: &mut [u8],
    source_body: &[u8],
    original: &EventPrefabScope,
    edited: &EventManagerRaceEventPrefabScope,
    description: &str,
) -> Result<(), CrpError> {
    if edited.indexes.len() != original.indexes.len() || edited.names.len() != original.names.len()
    {
        return Err(invalid_event_record(
            original.scope_range.start,
            format!("race-event {description} assignment and name counts cannot change"),
        ));
    }
    let assignment_range = original.scope_range.start..original.names_range.start;
    let encoded_indexes = if edited.indexes == original.indexes {
        source_body
            .get(assignment_range.clone())
            .ok_or_else(|| {
                invalid_event_record(
                    assignment_range.start,
                    format!("race-event {description} assignment bytes exceed source body"),
                )
            })?
            .to_vec()
    } else {
        let mut encoded = Vec::new();
        write_encoded_ushort_array(&mut encoded, &edited.indexes);
        encoded
    };
    if encoded_indexes.len() != assignment_range.len() {
        return Err(invalid_event_record(
            assignment_range.start,
            format!("race-event {description} edits change encoded assignment width"),
        ));
    }
    patch_race_event_field(
        output,
        &assignment_range,
        &encoded_indexes,
        "prefab assignments",
    )?;

    let encoded_names = encode_prefab_name_table(
        source_body,
        original.names_range.clone(),
        &original.names,
        &original.name_ranges,
        &original.name_compact_modes,
        &edited.names,
    )?;
    if encoded_names.len() != original.names_range.len() {
        return Err(invalid_event_record(
            original.names_range.start,
            format!("race-event {description} edits change name-table width"),
        ));
    }
    patch_race_event_field(
        output,
        &original.names_range,
        &encoded_names,
        "prefab names",
    )
}

fn encode_race_event_route_spline(
    format_version: u32,
    spline: &EventManagerRaceEventRouteSpline,
    original: &EventManagerRaceEventRouteSpline,
    offset: usize,
) -> Result<Vec<u8>, CrpError> {
    if spline.bezier_count != original.bezier_count
        || spline.closed_raw != original.closed_raw
        || spline.records.len() != original.records.len()
        || spline.terminal_record.is_some() != original.terminal_record.is_some()
    {
        return Err(invalid_event_record(
            offset,
            "race-event route-spline count and layout cannot change in this editor",
        ));
    }
    let bezier_count = usize::try_from(spline.bezier_count)
        .map_err(|_| CrpError::ArithmeticOverflow("EventManager route-spline count"))?;
    if bezier_count > MAX_RACE_EVENT_ENTRIES {
        return Err(invalid_event_record(
            offset,
            "race-event route-spline count exceeds the supported limit",
        ));
    }
    let closed = data_serializer_bool_is_true(spline.closed_raw);
    let expected_record_count = if format_version >= 121_034 {
        bezier_count
    } else if closed {
        bezier_count.checked_sub(1).ok_or_else(|| {
            invalid_event_record(
                offset,
                "closed race-event route-spline count must be nonzero",
            )
        })?
    } else {
        bezier_count
    };
    let has_legacy_terminal = format_version < 121_034 && closed;
    if spline.records.len() != expected_record_count
        || spline.terminal_record.is_some() != has_legacy_terminal
    {
        return Err(invalid_event_record(
            offset,
            "race-event route-spline records do not match the source version layout",
        ));
    }
    let record_bytes = spline
        .records
        .len()
        .checked_mul(3)
        .and_then(|length| {
            length.checked_add(if spline.terminal_record.is_some() {
                3
            } else {
                0
            })
        })
        .ok_or(CrpError::ArithmeticOverflow(
            "EventManager route-spline record bytes",
        ))?;
    let capacity = 13_usize
        .checked_add(record_bytes)
        .ok_or(CrpError::ArithmeticOverflow(
            "EventManager route-spline serialized width",
        ))?;
    let mut output = Vec::with_capacity(capacity);
    output.extend_from_slice(&spline.bezier_count.to_be_bytes());
    for index in spline.indexes {
        output.extend_from_slice(&index.to_be_bytes());
    }
    output.push(spline.closed_raw);
    for record in &spline.records {
        output.push(record.kind);
        output.extend_from_slice(&record.source.to_be_bytes());
    }
    if let Some(record) = spline.terminal_record {
        output.push(record.kind);
        output.extend_from_slice(&record.source.to_be_bytes());
    }
    Ok(output)
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
    let mut output = Vec::new();
    output.extend_from_slice(&record.building_index.to_be_bytes());
    match (format_version >= 112_021, record.created_frame) {
        (true, Some(frame)) => output.extend_from_slice(&frame.to_be_bytes()),
        (false, None) => {}
        (true, None) => {
            return Err(invalid_event_record(
                offset,
                "version-112021-or-later event record requires a created frame",
            ));
        }
        (false, Some(_)) => {
            return Err(invalid_event_record(
                offset,
                "created frame is unavailable before version 112021",
            ));
        }
    }
    match (format_version >= 112_029, record.toga_party_start_time_bits) {
        (true, Some(times)) => {
            for bits in times {
                output.extend_from_slice(&bits.to_be_bytes());
            }
        }
        (false, None) => {}
        (true, None) => {
            return Err(invalid_event_record(
                offset,
                "version-112029-or-later event record requires both toga-party start times",
            ));
        }
        (false, Some(_)) => {
            return Err(invalid_event_record(
                offset,
                "toga-party start times are unavailable before version 112029",
            ));
        }
    }
    output.extend_from_slice(&record.start_frame.to_be_bytes());

    match (format_version >= 246, record.expiry_frame) {
        (true, Some(frame)) => output.extend_from_slice(&frame.to_be_bytes()),
        (false, None) => {}
        (true, None) => {
            return Err(invalid_event_record(
                offset,
                "version-246-or-later event record requires an expiry frame",
            ));
        }
        (false, Some(_)) => {
            return Err(invalid_event_record(
                offset,
                "expiry frame is unavailable before version 246",
            ));
        }
    }
    match (format_version >= 249, record.ticket_price) {
        (true, Some(price)) => output.extend_from_slice(&price.to_be_bytes()),
        (false, None) => {}
        (true, None) => {
            return Err(invalid_event_record(
                offset,
                "version-249-or-later event record requires a ticket price",
            ));
        }
        (false, Some(_)) => {
            return Err(invalid_event_record(
                offset,
                "ticket price is unavailable before version 249",
            ));
        }
    }
    match (
        (245..252).contains(&format_version),
        record.next_building_event,
    ) {
        (true, Some(index)) => output.extend_from_slice(&index.to_be_bytes()),
        (false, None) => {}
        (true, None) => {
            return Err(invalid_event_record(
                offset,
                "versions 245–251 require the next-building-event value",
            ));
        }
        (false, Some(_)) => {
            return Err(invalid_event_record(
                offset,
                "next-building-event value is unavailable outside versions 245–251",
            ));
        }
    }
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
                "version-245-or-later event record requires RGB bytes and no legacy color words",
            ));
        }
        (false, _, _) => {
            return Err(invalid_event_record(
                offset,
                "pre-245 event record requires four legacy color words and no RGB bytes",
            ));
        }
    }
    match (format_version >= 255, record.ticket_totals) {
        (true, Some(totals)) => {
            for total in totals {
                output.extend_from_slice(&total.to_be_bytes());
            }
        }
        (false, None) => {}
        (true, None) => {
            return Err(invalid_event_record(
                offset,
                "version-255-or-later event record requires ticket totals",
            ));
        }
        (false, Some(_)) => {
            return Err(invalid_event_record(
                offset,
                "ticket totals are unavailable before version 255",
            ));
        }
    }
    match (
        format_version >= 250,
        record.reward_totals,
        record.result_counts,
    ) {
        (true, Some(totals), Some(counts)) => {
            for total in totals {
                output.extend_from_slice(&total.to_be_bytes());
            }
            for count in counts {
                output.extend_from_slice(&count.to_be_bytes());
            }
        }
        (false, None, None) => {}
        (true, _, _) => {
            return Err(invalid_event_record(
                offset,
                "version-250-or-later event record requires reward totals and result counts",
            ));
        }
        (false, _, _) => {
            return Err(invalid_event_record(
                offset,
                "reward totals and result counts are unavailable before version 250",
            ));
        }
    }
    match (
        event_popularity_delta_present(format_version),
        record.popularity_delta,
    ) {
        (true, Some(delta)) => output.extend_from_slice(&delta.to_be_bytes()),
        (false, None) => {}
        (true, None) => {
            return Err(invalid_event_record(
                offset,
                "this version requires an event popularity delta",
            ));
        }
        (false, Some(_)) => {
            return Err(invalid_event_record(
                offset,
                "event popularity delta is unavailable at this version",
            ));
        }
    }
    match (
        event_security_budget_present(format_version),
        record.security_budget,
    ) {
        (true, Some(budget)) => output.extend_from_slice(&budget.to_be_bytes()),
        (false, None) => {}
        (true, None) => {
            return Err(invalid_event_record(
                offset,
                "this version requires an event security budget",
            ));
        }
        (false, Some(_)) => {
            return Err(invalid_event_record(
                offset,
                "event security budget is unavailable at this version",
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

fn invalid_event_route(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "EventManager+Data route".to_owned(),
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

fn invalid_event_prefabs(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "EventManager+Data prefab indexes".to_owned(),
        offset,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EventManagerEventRecord, EventManagerRaceEventData, EventManagerReferenceData,
        EventManagerTailData, EventManagerTypeData,
    };
    use crate::data_serializer::Reader;
    use crate::net_manager_data::NetObjectHeader;

    const EMPTY_RACE_EVENT_DATA: [u8; 74] = [
        0x00, 0x00, 0x00, 0x00, // No racers.
        0x00, 0x00, // Empty RaceTeamInfo reference scope.
        0x00, 0x00, // Empty ParadeGroupInfo reference scope.
        0x01, 0x02, 0x03, 0x04, // Last spawn frame.
        0x05, 0x06, 0x07, 0x08, // Next spawn frame.
        0x09, 0x0a, 0x0b, 0x0c, // Schedule ID.
        0x0d, 0x0e, 0x0f, 0x10, // Lap count.
        0x11, 0x12, 0x13, 0x14, // State.
        0x15, 0x16, 0x17, 0x18, // Frames spent in state.
        0x19, 0x20, // Race marshal ID.
        0x00, 0x00, 0x00, 0x00, // No track path points.
        0x00, // Track is open.
        0x21, 0x22, 0x23, 0x24, // Event end frame.
        0x25, 0x26, // Finished racer count.
        0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, // First three finishers.
        0x2d, 0x2e, 0x2f, 0x30, // Frames to next overtake.
        0x00, 0x00, 0x00, 0x00, // No race teams.
        0x00, 0x00, // Empty RaceTeamInfo team scope.
        0x00, 0x00, 0x00, 0x00, // No team members.
        0x31, 0x32, 0x33, 0x34, // Spawned-entity count.
        0x00, 0x00, 0x00, 0x00, // Empty legacy counted UShort block.
        0x00, // No route spline.
    ];

    fn race_event_data_for_version(source: &[u8], format_version: u32) -> Vec<u8> {
        if format_version < 121_031 {
            return source.to_vec();
        }
        let mut reader = Reader::new(source);
        let (_, field_ranges) =
            super::skip_race_event_data(&mut reader, 120_004).expect("base race-event fixture");
        assert_eq!(reader.offset(), source.len());
        let mut output = source.to_vec();
        if format_version >= 121_032 {
            let legacy_range = field_ranges
                .legacy_counted_values
                .expect("pre-121032 fixture has a legacy counted block");
            output.drain(legacy_range);
            output.splice(
                field_ranges.event_end_frame.start..field_ranges.event_end_frame.start,
                0_i32.to_be_bytes(),
            );
        }
        if format_version >= 121_031 {
            output.push(0); // collisionsEnabled is false.
        }
        output
    }

    fn one_active_event(format_version: u32, flags: u32) -> Vec<u8> {
        let mut bytes = vec![
            0x00, 0x01, // Two event slots.
            0x00, // One literal event flag follows.
        ];
        bytes.extend_from_slice(&flags.to_be_bytes());
        bytes.extend_from_slice(&[0x12, 0x34]); // Building index.
        if format_version >= 112_021 {
            bytes.extend_from_slice(&0x0506_0708_u32.to_be_bytes()); // Created frame.
        }
        if format_version >= 112_029 {
            bytes.extend_from_slice(&0x3f80_0000_u32.to_be_bytes()); // Toga start time 1.
            bytes.extend_from_slice(&0xbf00_0000_u32.to_be_bytes()); // Toga start time 2.
        }
        bytes.extend_from_slice(&0x0102_0304_u32.to_be_bytes()); // Start frame.
        if format_version >= 246 {
            bytes.extend_from_slice(&0x0506_0708_u32.to_be_bytes()); // Expiry frame.
        }
        if format_version >= 249 {
            bytes.extend_from_slice(&[0x12, 0x34]); // Ticket price.
        }
        if (245..252).contains(&format_version) {
            bytes.extend_from_slice(&[0x45, 0x67]); // m_nextBuildingEvent.
        }
        if format_version >= 245 {
            bytes.extend_from_slice(&[0x11, 0x22, 0x33]);
        } else {
            for color_word in [0x1111_1111_u32, 0x2222_2222, 0x3333_3333, 0x4444_4444] {
                bytes.extend_from_slice(&color_word.to_be_bytes());
            }
        }
        if format_version >= 255 {
            bytes.extend_from_slice(&0x2122_2324_2526_2728_u64.to_be_bytes()); // Ticket income.
            bytes.extend_from_slice(&0x3132_3334_3536_3738_u64.to_be_bytes()); // Total ticket.
        }
        if format_version >= 250 {
            bytes.extend_from_slice(&0x0102_0304_0506_0708_u64.to_be_bytes()); // Reward money.
            bytes.extend_from_slice(&0x1112_1314_1516_1718_u64.to_be_bytes()); // Total reward.
            bytes.extend_from_slice(&[0x21, 0x22, 0x23, 0x24]); // Success/failure counts.
        }
        if (108_005..109_000).contains(&format_version) || format_version >= 109_005 {
            bytes.extend_from_slice(&0x1234_i16.to_be_bytes()); // Popularity delta.
        }
        if (108_008..109_000).contains(&format_version) || format_version >= 109_008 {
            bytes.extend_from_slice(&[0x56, 0x78]); // Security budget.
        }
        if flags & (1 << 10) != 0 {
            bytes.extend_from_slice(&0x0102_0304_0506_0708_u64.to_be_bytes());
        }
        if (120_004..=121_005).contains(&format_version) {
            bytes.extend_from_slice(&race_event_data_for_version(
                &EMPTY_RACE_EVENT_DATA,
                format_version,
            ));
        } else if (121_006..=super::LAST_SUPPORTED_EVENT_RECORD_VERSION).contains(&format_version) {
            bytes.push(1); // RaceEventData is present.
            bytes.extend_from_slice(&race_event_data_for_version(
                &EMPTY_RACE_EVENT_DATA,
                format_version,
            ));
        }
        if format_version >= 121_005 {
            bytes.extend_from_slice(&[0x00, 0x00]); // One route slot; no active routes.
        }
        bytes.extend_from_slice(&[0x00, 0x00]); // No unlock milestones.
        if (108_002..109_000).contains(&format_version) {
            bytes.extend_from_slice(&[0x00, 0x00]); // No modified event types.
        }
        bytes.extend_from_slice(&[0x00, 0x00, 0x07]); // One event prefab index.
        bytes.extend_from_slice(&[0x00, 0x00]); // Empty prefab names.
        if format_version >= 121_015 {
            bytes.extend_from_slice(&[0x00, 0x00]); // Empty event-schedule prefab names.
        }
        if format_version >= 253 {
            bytes.push(0xff); // Match-preparing reference.
        }
        if format_version >= 109_011 || (108_011..109_000).contains(&format_version) {
            bytes.extend_from_slice(&[0xff, 0xff]); // Band references.
        }
        if format_version >= 109_003 || (108_003..109_000).contains(&format_version) {
            bytes.extend_from_slice(&[0x34, 0x56]); // Band-popularity bonus.
        }
        if format_version >= 121_027 {
            bytes.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]); // Event-schedule count.
        }
        bytes.extend_from_slice(&[0x12, 0x34]); // Profiler end index.
        bytes
    }

    fn one_active_event_with_race_data(format_version: u32, race_data: &[u8]) -> Vec<u8> {
        let mut bytes = one_active_event(format_version, 1);
        let empty_race_data = race_event_data_for_version(&EMPTY_RACE_EVENT_DATA, format_version);
        let serialized_race_data = race_event_data_for_version(race_data, format_version);
        let race_data_start = bytes
            .windows(empty_race_data.len())
            .position(|window| window == empty_race_data)
            .expect("empty race-event fixture occurs in the event record");
        bytes.splice(
            race_data_start..race_data_start + empty_race_data.len(),
            serialized_race_data,
        );
        bytes
    }

    #[test]
    fn event_flag_edits_reject_encoded_array_width_changes() {
        let one_record = one_active_event(244, 1);
        let (decoded, _) = EventManagerReferenceData::decode_reader(244, &one_record).unwrap();
        let record_range = decoded.active_event_record_ranges[0].clone();
        let record_bytes = &one_record[record_range];

        let mut bytes = vec![0x00, 0x04]; // Five event slots.
        // Values 1, 2, 3, 4: two literal deltas, then two repeats.
        bytes.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x02, 0x81,
        ]);
        for _ in 0..4 {
            bytes.extend_from_slice(record_bytes);
        }
        bytes.extend_from_slice(&[0x00, 0x00]); // No unlock milestones.
        crate::encoded_array::write_encoded_ushort_array(&mut bytes, &[7, 7, 7, 7]);
        bytes.extend_from_slice(&[0x00, 0x00]); // Empty prefab names.
        bytes.extend_from_slice(&[0x12, 0x34]); // Profiler end index.

        let (decoded, consumed) = EventManagerReferenceData::decode_reader(244, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(
            decoded
                .active_event_records
                .iter()
                .map(|record| record.flags)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );

        let mut editor = decoded.into_event_records_editor();
        editor.set_event_flag_bits(0, 5).unwrap();
        assert!(editor.encode().is_err());
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

    fn one_active_route_layout(version: u32, schedule_count: u8) -> (Vec<u8>, Vec<u8>) {
        use crate::encoded_array::{write_encoded_uint_array, write_encoded_ushort_array};

        let mut bytes = vec![
            0x00, 0x00, // One event slot, no active event records.
            0x00, 0x01, // Two route slots, including the reserved slot zero.
        ];
        write_encoded_uint_array(&mut bytes, &[0x1020_3040]); // One active route.

        let mut route = Vec::new();
        if version >= 121_018 {
            route.extend_from_slice(&0x1122_3344_u32.to_be_bytes()); // Event policies.
        }
        for index in 1_u16..=7 {
            route.extend_from_slice(&index.to_be_bytes()); // Route and building/segment indexes.
        }
        if version >= 121_007 {
            route.extend_from_slice(&0x89ab_cdef_u32.to_be_bytes()); // Route version.
            route.extend_from_slice(&0x1020_3040_u32.to_be_bytes()); // Packed color.
            route.extend_from_slice(&8_u16.to_be_bytes()); // Start node.
            route.extend_from_slice(&9_u16.to_be_bytes()); // End node.
        }
        if version >= 121_009 {
            route.extend_from_slice(&10_u16.to_be_bytes()); // Event prefab index.
        }
        if version >= 121_015 {
            route.push(schedule_count);
            for _ in 0..schedule_count {
                if version >= 121_027 {
                    route.extend_from_slice(&(-11_i32).to_be_bytes()); // Schedule ID.
                }
                route.extend_from_slice(&12_u16.to_be_bytes()); // Tier.
                route.extend_from_slice(&13_u16.to_be_bytes()); // Laps.
                route.extend_from_slice(&14_u16.to_be_bytes()); // Ticket price.
                route.push(0xff); // Null schedule name.
                if version >= 121_027 {
                    route.extend_from_slice(&15_u16.to_be_bytes()); // Schedule flags.
                    route.extend_from_slice(&[16, 17, 18]); // Frequency, start day, start month.
                }
            }
        }
        bytes.extend_from_slice(&route);

        bytes.extend_from_slice(&[0x00, 0x00]); // No unlock milestones.
        bytes.extend_from_slice(&[0x00, 0x00]); // Empty event-prefab indexes and names.
        if version >= 121_015 {
            let schedule_indexes = vec![0x1234; usize::from(schedule_count)];
            write_encoded_ushort_array(&mut bytes, &schedule_indexes);
            bytes.extend_from_slice(&[0x00, 0x00]); // Empty schedule-prefab names.
        }
        if version >= 253 {
            bytes.push(0xff); // Match-preparing reference.
        }
        if version >= 109_011 {
            bytes.extend_from_slice(&[0xff, 0xff]); // Band references.
        }
        if version >= 109_003 {
            bytes.extend_from_slice(&[0x34, 0x56]); // Band-popularity bonus.
        }
        if version >= 121_027 {
            bytes.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]); // Event-schedule count.
        }
        bytes.extend_from_slice(&[0x12, 0x34]); // Profiler end index.
        (bytes, route)
    }

    fn one_racer_race_event_data() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_i32.to_be_bytes()); // One racer.
        bytes.extend_from_slice(&[
            0x00, 0x01, // Racer ID.
            0x00, 0x02, // Racer type.
            0x00, 0x00, 0x00, 0x03, // Spawn frame.
            0x00, 0x04, // Parade float.
            0x00, 0x01, // One performer.
            0x00, 0x05, // Performer ID.
            0x00, 0x06, // First path.
            0x00, 0x07, // Second path.
            0x01, // Paths assigned.
            0x01, 0x00, 0x08, // Present RaceTeamInfo index.
            0xff, // Null racer name.
            0x01, 0x00, 0x09, // Present ParadeGroupInfo index.
            0x00, 0x00, 0x0a, 0x00, 0x00, // RaceTeamInfo reference scope.
            0x00, 0x00, 0x0b, 0x00, 0x00, // ParadeGroupInfo reference scope.
        ]);
        bytes.extend_from_slice(&0x0102_0304_u32.to_be_bytes()); // Last spawn frame.
        bytes.extend_from_slice(&0x0506_0708_u32.to_be_bytes()); // Next spawn frame.
        bytes.extend_from_slice(&[
            0x3f, 0x80, 0x00, 0x00, // Maximum speed.
            0x40, 0x00, 0x00, 0x00, // Acceleration.
            0x40, 0x40, 0x00, 0x00, // Braking.
            0x40, 0x80, 0x00, 0x00, // Turning.
            0x01, 0x02, // Completed lap and lap count.
            0x00, 0x00, 0x00, 0x03, // Completed offset.
            0x00, 0x04, // Finish position.
        ]);
        for value in [0x1112_1314_u32, 0x1516_1718, 0x1920_2122, 0x2324_2526] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.extend_from_slice(&[0x00, 0x27]); // Race marshal ID.
        bytes.extend_from_slice(&1_i32.to_be_bytes()); // One track point.
        bytes.extend_from_slice(&[
            0x3f, 0x80, 0x00, 0x00, // Track point X.
            0x40, 0x00, 0x00, 0x00, // Track point Y.
            0x40, 0x40, 0x00, 0x00, // Track point Z.
            0x01, // Closed track.
        ]);
        bytes.extend_from_slice(&0x0000_0028_i32.to_be_bytes()); // Racer path index.
        bytes.extend_from_slice(&0x2930_3132_u32.to_be_bytes()); // Event end frame.
        bytes.extend_from_slice(&[
            0x00, 0x01, // One finished racer.
            0x00, 0x02, 0x00, 0x03, 0x00, 0x04, // First three finishers.
        ]);
        bytes.extend_from_slice(&0x3334_3536_i32.to_be_bytes()); // Frames to overtake.
        bytes.extend_from_slice(&0x0000_0037_i32.to_be_bytes()); // Overtake duration.
        bytes.extend_from_slice(&[0x3f, 0xc0, 0x00, 0x00]); // Overtake offset.
        bytes.extend_from_slice(&[0x40, 0x00, 0x00, 0x00]); // Overtake speed multiplier.
        bytes.extend_from_slice(&[0x40, 0x40, 0x00, 0x00]); // Track lateral offset.
        bytes.extend_from_slice(&0_i32.to_be_bytes()); // No race-team collection entries.
        bytes.extend_from_slice(&[0x00, 0x00]); // Empty team reference scope.
        bytes.extend_from_slice(&1_i32.to_be_bytes()); // One team member.
        bytes.extend_from_slice(&0x3839_4041_i32.to_be_bytes()); // Team member ID.
        bytes.extend_from_slice(&[0x00, 0x0d]); // Track path segment.
        bytes.extend_from_slice(&[0x3f, 0x80, 0x00, 0x00]); // Lateral oscillation.
        bytes.extend_from_slice(&[0x40, 0x00, 0x00, 0x00]); // Speed oscillation.
        bytes.extend_from_slice(&0x4243_4445_i32.to_be_bytes()); // Racer state.
        bytes.extend_from_slice(&0x4647_4849_i32.to_be_bytes()); // Frames in state.
        bytes.extend_from_slice(&0x4a4b_4c4d_i32.to_be_bytes()); // Spawned-entity count.
        bytes.extend_from_slice(&0_i32.to_be_bytes()); // Empty legacy counted UShort block.
        bytes.push(0); // No route spline.
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
    fn active_event_routes_follow_version_gates_and_roundtrip() {
        for (version, schedule_count) in [
            (121_005, 0),
            (121_006, 0),
            (121_007, 0),
            (121_009, 0),
            (121_014, 0),
            (121_015, 1),
            (121_026, 4),
            (121_027, 2),
            (121_030, 4),
            (121_031, 2),
            (121_032, 2),
            (121_033, 2),
            (121_034, 4),
        ] {
            let (bytes, expected_route_bytes) = one_active_route_layout(version, schedule_count);
            let (decoded, consumed) = EventManagerReferenceData::decode_reader(version, &bytes)
                .unwrap_or_else(|error| panic!("version {version}: {error}"));
            assert_eq!(consumed, bytes.len(), "version {version}");
            assert_eq!(
                decoded.active_event_route_ranges.len(),
                1,
                "version {version}"
            );
            let route = &decoded.active_event_routes[0];
            assert_eq!(route.slot_index, 1, "version {version}");
            assert_eq!(route.flags, 0x1020_3040, "version {version}");
            assert_eq!(route.policies, (version >= 121_018).then_some(0x1122_3344));
            assert_eq!(route.indexes, [1, 2, 3, 4, 5, 6, 7]);
            assert_eq!(
                route.route_version,
                (version >= 121_007).then_some(0x89ab_cdef)
            );
            assert_eq!(route.color, (version >= 121_007).then_some(0x1020_3040));
            assert_eq!(route.start_node, (version >= 121_007).then_some(8));
            assert_eq!(route.end_node, (version >= 121_007).then_some(9));
            assert_eq!(route.event_prefab_index, (version >= 121_009).then_some(10));
            let expected_schedule_count = if version >= 121_015 {
                usize::from(schedule_count)
            } else {
                0
            };
            assert_eq!(route.schedules.len(), expected_schedule_count);
            if let Some(schedule) = route.schedules.first() {
                assert_eq!(schedule.schedule_id, (version >= 121_027).then_some(-11));
                assert_eq!(schedule.tier, 12);
                assert_eq!(schedule.lap_count, 13);
                assert_eq!(schedule.ticket_price, 14);
                assert_eq!(schedule.name, None);
                assert_eq!(schedule.flags, (version >= 121_027).then_some(15));
                assert_eq!(schedule.frequency, (version >= 121_027).then_some(16));
                assert_eq!(schedule.start_day, (version >= 121_027).then_some(17));
                assert_eq!(schedule.start_month, (version >= 121_027).then_some(18));
            }
            assert_eq!(
                decoded
                    .serialized_body
                    .get(decoded.active_event_route_ranges[0].clone()),
                Some(expected_route_bytes.as_slice()),
                "version {version}"
            );
            assert_eq!(
                decoded
                    .clone()
                    .into_event_records_editor()
                    .encode()
                    .unwrap(),
                bytes,
                "version {version}"
            );
            assert_eq!(
                decoded
                    .clone()
                    .into_route_records_editor()
                    .encode()
                    .unwrap(),
                bytes,
                "route editor version {version}"
            );
            let prefab_editor = decoded.clone().into_prefab_indices_editor();
            assert_eq!(
                prefab_editor.route_schedule_indexes(),
                vec![0x1234; expected_schedule_count]
            );
            assert!(prefab_editor.route_schedule_names().is_empty());
            assert_eq!(prefab_editor.encode().unwrap(), bytes);
            assert_eq!(
                decoded.route_schedule_prefab_scope.is_some(),
                version >= 121_015,
                "schedule prefab scope at version {version}"
            );
        }

        let (single_route_body, single_route) = one_active_route_layout(121_030, 1);
        let schedule_count_offset = 4 + 7 * 2 + 4 + 4 + 4 + 2;
        let mut second_route = single_route[..schedule_count_offset].to_vec();
        second_route.push(2);
        let schedule_record = &single_route[schedule_count_offset + 1..];
        second_route.extend_from_slice(schedule_record);
        second_route.extend_from_slice(schedule_record);

        let first_route_start = 4 + 1 + 4; // Two capacity fields plus one literal flag.
        let first_route_end = first_route_start + single_route.len();
        let old_schedule_scope_start = first_route_end + 2 + 2;
        let old_schedule_scope_end = old_schedule_scope_start + 5;
        let mut two_route_body = single_route_body[..2].to_vec();
        two_route_body.extend_from_slice(&2_u16.to_be_bytes()); // Three slots total.
        crate::encoded_array::write_encoded_uint_array(&mut two_route_body, &[1, 2]);
        two_route_body.extend_from_slice(&single_route);
        two_route_body.extend_from_slice(&second_route);
        two_route_body.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Empty milestones/scope.
        crate::encoded_array::write_encoded_ushort_array(
            &mut two_route_body,
            &[0x1234, 0x1234, 0x1234],
        );
        two_route_body.extend_from_slice(&[0x00, 0x00]); // Empty schedule-prefab names.
        two_route_body.extend_from_slice(&single_route_body[old_schedule_scope_end..]);
        let (decoded_two_routes, consumed_two_routes) =
            EventManagerReferenceData::decode_reader(121_030, &two_route_body).unwrap();
        assert_eq!(consumed_two_routes, two_route_body.len());
        assert_eq!(decoded_two_routes.active_event_route_ranges.len(), 2);
        let two_route_prefab_editor = decoded_two_routes.clone().into_prefab_indices_editor();
        assert_eq!(
            two_route_prefab_editor.route_schedule_indexes(),
            [0x1234, 0x1234, 0x1234]
        );
        assert_eq!(two_route_prefab_editor.encode().unwrap(), two_route_body);
        assert_eq!(
            decoded_two_routes
                .clone()
                .into_event_records_editor()
                .encode()
                .unwrap(),
            two_route_body
        );

        let (bytes, _) = one_active_route_layout(121_030, 5);
        assert!(EventManagerReferenceData::decode_reader(121_030, &bytes).is_err());

        let (mut invalid_string, route) = one_active_route_layout(121_015, 1);
        let route_start = invalid_string
            .windows(route.len())
            .position(|window| window == route)
            .expect("active route fixture occurs in the manager body");
        let name_offset = route
            .iter()
            .position(|byte| *byte == 0xff)
            .expect("schedule name null marker occurs in the route");
        invalid_string.splice(
            route_start + name_offset..route_start + name_offset + 1,
            [0x00, 0x00, 0x01, 0xd7, 0x80], // One unpaired UTF-16 high surrogate.
        );
        assert!(EventManagerReferenceData::decode_reader(121_015, &invalid_string).is_err());

        let (bytes, _) = one_active_route_layout(121_030, 1);
        for end in 0..bytes.len() {
            assert!(
                EventManagerReferenceData::decode_reader(121_030, &bytes[..end]).is_err(),
                "truncated active route body at {end}"
            );
        }
    }

    #[test]
    fn route_flag_edits_reencode_and_resize_the_flag_array() {
        let mut bytes = vec![0x00, 0x00, 0x00, 0x04]; // One event and five route slots.
        // Values 1, 2, 3, 4: two literal deltas, then two repeats.
        bytes.extend_from_slice(&[
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x02, 0x81,
        ]);
        for route in 0..4_u16 {
            for index in 0..7_u16 {
                bytes.extend_from_slice(&(route * 10 + index).to_be_bytes());
            }
        }
        bytes.extend_from_slice(&[0x00, 0x00]); // No unlock milestones.
        bytes.extend_from_slice(&[0x00, 0x00]); // Empty event-prefab names.
        bytes.push(0xff); // Match-preparing reference.
        bytes.extend_from_slice(&[0xff, 0xff]); // Null band references.
        bytes.extend_from_slice(&[0x34, 0x56]); // Band-popularity bonus.
        bytes.extend_from_slice(&[0x12, 0x34]); // Profiler end index.

        let (decoded, consumed) =
            EventManagerReferenceData::decode_reader(121_005, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(
            decoded
                .active_event_routes
                .iter()
                .map(|route| route.flags)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        let mut editor = decoded.into_route_records_editor();
        assert!(editor.set_route_flag_bits(0, 0).is_err());
        assert!(editor.set_route_flag_bits(4, 5).is_err());
        editor.set_route_flag_bits(0, 5).unwrap();

        let encoded = editor.encode().unwrap();
        assert_eq!(encoded.len(), bytes.len() + 6);
        assert_eq!(
            &encoded[..21],
            [
                0x00, 0x00, 0x00, 0x04, 0x03, // Capacity and canonical literal control.
                0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00,
                0x00, 0x04,
            ]
        );
        assert_eq!(&encoded[21..], &bytes[15..]);

        let (rewritten, rewritten_len) =
            EventManagerReferenceData::decode_reader(121_005, &encoded).unwrap();
        assert_eq!(rewritten_len, encoded.len());
        assert_eq!(
            rewritten
                .active_event_routes
                .iter()
                .map(|route| route.flags)
                .collect::<Vec<_>>(),
            [5, 2, 3, 4]
        );
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
    fn prefab_index_editor_splits_groups_and_preserves_name_table_bytes() {
        let mut bytes = vec![
            0x00, 0x00, // One event slot; no active events.
            0x00, 0x00, // No unlock milestones.
            0x00, 0x04, // Four modified event types.
            0x03, 0x00, 0x0a, 0x00, 0x14, 0x00, 0x1e, 0x00, 0x28, // Four literal indexes.
        ];
        let names_start = bytes.len();
        bytes.extend_from_slice(&[0x00, 0x01]); // One unique prefab name.
        crate::data_serializer::write_unique_string(&mut bytes, "EventPrefab")
            .expect("short test name is encodable");
        let names_end = bytes.len();
        for (popularity, price) in [(-1_i8, 1_u32), (2, 2), (3, 3), (4, 4)] {
            bytes.push(popularity as u8);
            bytes.extend_from_slice(&price.to_be_bytes()[1..]);
        }
        bytes.push(0xff); // Match-preparing reference.
        bytes.extend_from_slice(&[0x00, 0x09]); // Band-popularity bonus.
        bytes.extend_from_slice(&[0x12, 0x34]); // Profiler end index.

        let (decoded, consumed) =
            EventManagerReferenceData::decode_reader(108_004, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        let mut editor = decoded.into_prefab_indices_editor();
        assert!(editor.event_indexes().is_empty());
        assert!(editor.milestone_indexes().is_empty());
        assert_eq!(editor.type_data_indexes(), [10, 20, 30, 40]);
        editor
            .type_data_indexes_mut()
            .copy_from_slice(&[1, 2, 3, 4]);

        let mut expected = vec![
            0x00, 0x00, 0x00, 0x00, 0x00, 0x04, // Counts are unchanged.
            0x00, 0x00, 0x01, 0x82, // Canonical literal followed by three repeated deltas.
        ];
        expected.extend_from_slice(&bytes[names_start..names_end]);
        expected.extend_from_slice(&bytes[names_end..]);
        assert_eq!(editor.encode().unwrap(), expected);
        assert!(expected.len() < bytes.len());

        let (rewritten, rewritten_len) =
            EventManagerReferenceData::decode_reader(108_004, &expected).unwrap();
        assert_eq!(rewritten_len, expected.len());
        assert_eq!(rewritten.prefab_indexes, [1, 2, 3, 4]);
        assert_eq!(
            &expected[6 + 4..6 + 4 + (names_end - names_start)],
            &bytes[names_start..names_end]
        );

        for end in 0..bytes.len() {
            assert!(
                EventManagerReferenceData::decode_reader(108_004, &bytes[..end]).is_err(),
                "truncated prefab-index body at {end}"
            );
        }
    }

    #[test]
    fn prefab_index_editor_keeps_active_event_group_separate() {
        let bytes = one_active_event(244, 1);
        let (decoded, consumed) = EventManagerReferenceData::decode_reader(244, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        let mut editor = decoded.into_prefab_indices_editor();
        assert_eq!(editor.event_indexes(), [7]);
        assert!(editor.milestone_indexes().is_empty());
        assert!(editor.type_data_indexes().is_empty());
        editor.event_indexes_mut()[0] = 0x1234;

        let mut expected = bytes.clone();
        let scope_start = expected.len() - 7;
        expected[scope_start..scope_start + 3].copy_from_slice(&[0x00, 0x12, 0x34]);
        assert_eq!(editor.encode().unwrap(), expected);
        let (rewritten, _) = EventManagerReferenceData::decode_reader(244, &expected).unwrap();
        assert_eq!(rewritten.prefab_indexes, [0x1234]);
    }

    #[test]
    fn prefab_name_editor_preserves_compact_and_wide_modes_and_resizes_table() {
        let bytes = vec![
            0x00, 0x00, // One event slot; no active events.
            0x00, 0x00, // No unlock milestones.
            0x00, 0x00, // No modified event types.
            0x00, 0x04, // Four names.
            0xff, // Null name.
            0x80, 0x00, 0x01, 0xc1, // Compact "A".
            0x80, 0x00, 0x01, 0xc3, // Compact "C".
            0x00, 0x00, 0x01, 0x03, 0x29, // Wide "Ω".
            0xff, // Match-preparing reference.
            0x00, 0x09, // Band-popularity bonus.
            0x12, 0x34, // Profiler end index.
        ];
        let (decoded, consumed) =
            EventManagerReferenceData::decode_reader(108_004, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        let mut editor = decoded.into_prefab_indices_editor();
        assert_eq!(
            editor.prefab_names(),
            [
                None,
                Some("A".to_owned()),
                Some("C".to_owned()),
                Some("Ω".to_owned())
            ]
        );
        editor.set_prefab_name(0, Some("XY".to_owned())).unwrap();
        editor.set_prefab_name(1, Some("B".to_owned())).unwrap();
        editor.set_prefab_name(2, Some("Δ".to_owned())).unwrap();
        editor.set_prefab_name(3, Some("Δ".to_owned())).unwrap();
        assert!(editor.set_prefab_name(4, None).is_err());

        let expected = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Counts unchanged.
            0x00, 0x04, // Four names.
            0x80, 0x00, 0x02, 0xd8, 0x01, // Compact "XY".
            0x80, 0x00, 0x01, 0xc2, // Compact "B".
            0x00, 0x00, 0x01, 0x03, 0x14, // Widened "Δ".
            0x00, 0x00, 0x01, 0x03, 0x14, // Wide "Δ".
            0xff, 0x00, 0x09, 0x12, 0x34,
        ];
        let encoded = editor.encode().unwrap();
        assert_eq!(encoded, expected);
        assert!(encoded.len() > bytes.len());
        let (rewritten, rewritten_len) =
            EventManagerReferenceData::decode_reader(108_004, &encoded).unwrap();
        assert_eq!(rewritten_len, encoded.len());
        assert_eq!(
            rewritten.prefab_names,
            [
                Some("XY".to_owned()),
                Some("B".to_owned()),
                Some("Δ".to_owned()),
                Some("Δ".to_owned())
            ]
        );
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
        assert!(EventManagerReferenceData::decode_reader(120_004, &active_event).is_err());
        let version_121031_event = one_active_event(121_031, 1);
        let (decoded_121031_event, consumed_121031_event) =
            EventManagerReferenceData::decode_reader(121_031, &version_121031_event)
                .expect("version-121031 race-event fields decode");
        assert_eq!(consumed_121031_event, version_121031_event.len());
        assert_eq!(
            decoded_121031_event.active_event_race_data[0]
                .as_ref()
                .expect("race-event data exists")
                .collisions_enabled_raw,
            Some(0)
        );

        let active_route = [
            0x00, 0x00, // One event slot.
            0x00, 0x01, // Two route slots.
            0x00, 0x00, 0x00, 0x00, 0x01, // One active route flag.
        ];
        assert!(EventManagerReferenceData::decode_reader(121_005, &active_route).is_err());
        let (version_121035_route, _) = one_active_route_layout(121_035, 1);
        assert!(EventManagerReferenceData::decode_reader(121_035, &version_121035_route).is_err());

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
                created_frame: None,
                toga_party_start_time_bits: None,
                start_frame: 0x0102_0304,
                expiry_frame: None,
                ticket_price: None,
                next_building_event: None,
                legacy_color_words: Some([0x1111_1111, 0x2222_2222, 0x3333_3333, 0x4444_4444,]),
                color_rgb: None,
                reward_totals: None,
                result_counts: None,
                ticket_totals: None,
                popularity_delta: None,
                security_budget: None,
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
            decoded.active_event_records[0].next_building_event,
            Some(0x4567)
        );
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
            0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x12, 0x34, 0x01, 0x02, 0x03, 0x04, 0x45,
            0x67, 0xaa, 0xbb, 0xcc, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x00, 0x00,
            0x00, 0x00, 0x07, 0x00, 0x00, 0x12, 0x34,
        ];
        assert_eq!(editor.encode().unwrap(), expected);
    }

    #[test]
    fn versioned_event_records_decode_and_edit_through_version_254() {
        let flags = 0x0000_0401;
        for (version, expected_ticket, expected_next, expected_rewards) in [
            (246, None, Some(0x4567), None),
            (248, None, Some(0x4567), None),
            (249, Some(0x1234), Some(0x4567), None),
            (
                250,
                Some(0x1234),
                Some(0x4567),
                Some([0x0102_0304_0506_0708, 0x1112_1314_1516_1718]),
            ),
            (
                251,
                Some(0x1234),
                Some(0x4567),
                Some([0x0102_0304_0506_0708, 0x1112_1314_1516_1718]),
            ),
            (
                252,
                Some(0x1234),
                None,
                Some([0x0102_0304_0506_0708, 0x1112_1314_1516_1718]),
            ),
            (
                253,
                Some(0x1234),
                None,
                Some([0x0102_0304_0506_0708, 0x1112_1314_1516_1718]),
            ),
            (
                254,
                Some(0x1234),
                None,
                Some([0x0102_0304_0506_0708, 0x1112_1314_1516_1718]),
            ),
        ] {
            let bytes = one_active_event(version, flags);
            let (decoded, consumed) = EventManagerReferenceData::decode_reader(version, &bytes)
                .unwrap_or_else(|error| panic!("version {version}: {error}"));
            assert_eq!(consumed, bytes.len(), "version {version}");
            let record = decoded.active_event_records[0];
            assert_eq!(record.expiry_frame, Some(0x0506_0708), "version {version}");
            assert_eq!(record.ticket_price, expected_ticket, "version {version}");
            assert_eq!(
                record.next_building_event, expected_next,
                "version {version}"
            );
            assert_eq!(record.reward_totals, expected_rewards, "version {version}");
            assert_eq!(
                record.result_counts,
                expected_rewards.map(|_| [0x2122, 0x2324]),
                "version {version}"
            );
            let encoded = decoded.into_event_records_editor().encode().unwrap();
            assert_eq!(encoded, bytes, "version {version}");
            let independently_expected: Option<&[u8]> = match version {
                246 => Some(&[
                    0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x12, 0x34, 0x01, 0x02, 0x03, 0x04,
                    0x05, 0x06, 0x07, 0x08, 0x45, 0x67, 0x11, 0x22, 0x33, 0x01, 0x02, 0x03, 0x04,
                    0x05, 0x06, 0x07, 0x08, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x12, 0x34,
                ]),
                249 => Some(&[
                    0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x12, 0x34, 0x01, 0x02, 0x03, 0x04,
                    0x05, 0x06, 0x07, 0x08, 0x12, 0x34, 0x45, 0x67, 0x11, 0x22, 0x33, 0x01, 0x02,
                    0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00,
                    0x12, 0x34,
                ]),
                252 => Some(&[
                    0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x12, 0x34, 0x01, 0x02, 0x03, 0x04,
                    0x05, 0x06, 0x07, 0x08, 0x12, 0x34, 0x11, 0x22, 0x33, 0x01, 0x02, 0x03, 0x04,
                    0x05, 0x06, 0x07, 0x08, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x21,
                    0x22, 0x23, 0x24, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x00, 0x00,
                    0x00, 0x00, 0x07, 0x00, 0x00, 0x12, 0x34,
                ]),
                _ => None,
            };
            if let Some(expected) = independently_expected {
                assert_eq!(encoded, expected, "exact bytes for version {version}");
            }
        }

        let bytes = one_active_event(250, flags);
        let (decoded, _) = EventManagerReferenceData::decode_reader(250, &bytes).unwrap();
        let mut editor = decoded.into_event_records_editor();
        let record = &mut editor.records_mut()[0];
        record.expiry_frame = Some(0xaabb_ccdd);
        record.ticket_price = Some(0x5566);
        record.next_building_event = Some(0x7788);
        record.color_rgb = Some([0xaa, 0xbb, 0xcc]);
        record.reward_totals = Some([0x2122_2324_2526_2728, 0x3132_3334_3536_3738]);
        record.result_counts = Some([0x3344, 0x5566]);
        let expected = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x12, 0x34, 0x01, 0x02, 0x03, 0x04, 0xaa,
            0xbb, 0xcc, 0xdd, 0x55, 0x66, 0x77, 0x88, 0xaa, 0xbb, 0xcc, 0x21, 0x22, 0x23, 0x24,
            0x25, 0x26, 0x27, 0x28, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x33, 0x44,
            0x55, 0x66, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x00, 0x00, 0x00, 0x00,
            0x07, 0x00, 0x00, 0x12, 0x34,
        ];
        assert_eq!(editor.encode().unwrap(), expected);

        let mut invalid = editor.clone();
        invalid.records_mut()[0].ticket_price = None;
        assert!(invalid.encode().is_err());
        let mut invalid = editor.clone();
        invalid.records_mut()[0].next_building_event = None;
        assert!(invalid.encode().is_err());
        let mut invalid = editor.clone();
        invalid.records_mut()[0].result_counts = None;
        assert!(invalid.encode().is_err());

        for end in 0..bytes.len() {
            assert!(
                EventManagerReferenceData::decode_reader(250, &bytes[..end]).is_err(),
                "truncated version-250 active event at {end}"
            );
        }
    }

    #[test]
    fn late_event_record_fields_follow_version_gates_through_121030() {
        let flags = 0x0000_0401;
        let expected_tickets = Some([0x2122_2324_2526_2728, 0x3132_3334_3536_3738]);
        let expected_times = Some([0x3f80_0000, 0xbf00_0000]);
        for (version, ticket_totals, popularity_delta, security_budget, created_frame, times) in [
            (254, None, None, None, None, None),
            (255, expected_tickets, None, None, None, None),
            (108_004, expected_tickets, None, None, None, None),
            (108_005, expected_tickets, Some(0x1234), None, None, None),
            (
                108_008,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                None,
                None,
            ),
            (
                108_999,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                None,
                None,
            ),
            (109_000, expected_tickets, None, None, None, None),
            (109_005, expected_tickets, Some(0x1234), None, None, None),
            (
                109_008,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                None,
                None,
            ),
            (
                112_020,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                None,
                None,
            ),
            (
                112_021,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                None,
            ),
            (
                112_028,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                None,
            ),
            (
                112_029,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
            (
                120_003,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
            (
                120_004,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
            (
                121_004,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
            (
                121_005,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
            (
                121_006,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
            (
                121_014,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
            (
                121_015,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
            (
                121_026,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
            (
                121_027,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
            (
                121_030,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
            (
                121_031,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
            (
                121_032,
                expected_tickets,
                Some(0x1234),
                Some(0x5678),
                Some(0x0506_0708),
                expected_times,
            ),
        ] {
            let bytes = one_active_event(version, flags);
            let (decoded, consumed) =
                EventManagerReferenceData::decode_reader(version, &bytes).unwrap();
            assert_eq!(consumed, bytes.len(), "version {version}");
            let record = decoded.active_event_records[0];
            assert_eq!(record.ticket_totals, ticket_totals, "version {version}");
            assert_eq!(
                record.popularity_delta, popularity_delta,
                "version {version}"
            );
            assert_eq!(record.security_budget, security_budget, "version {version}");
            assert_eq!(record.created_frame, created_frame, "version {version}");
            assert_eq!(
                record.toga_party_start_time_bits, times,
                "version {version}"
            );
            assert_eq!(
                decoded
                    .clone()
                    .into_event_records_editor()
                    .encode()
                    .unwrap(),
                bytes,
                "version {version}"
            );

            if version == 255 {
                let (decoded, _) =
                    EventManagerReferenceData::decode_reader(version, &bytes).unwrap();
                let expected = [
                    0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x12, 0x34, 0x01, 0x02, 0x03, 0x04,
                    0x05, 0x06, 0x07, 0x08, 0x12, 0x34, 0x11, 0x22, 0x33, 0x21, 0x22, 0x23, 0x24,
                    0x25, 0x26, 0x27, 0x28, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x01,
                    0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16,
                    0x17, 0x18, 0x21, 0x22, 0x23, 0x24, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
                    0x08, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0xff, 0x12, 0x34,
                ];
                assert_eq!(
                    decoded.into_event_records_editor().encode().unwrap(),
                    expected
                );
            }

            if version == 112_029 {
                let range = decoded.active_event_record_ranges[0].clone();
                let expected_record = [
                    0x12, 0x34, 0x05, 0x06, 0x07, 0x08, 0x3f, 0x80, 0x00, 0x00, 0xbf, 0x00, 0x00,
                    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x12, 0x34, 0x11, 0x22,
                    0x33, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x31, 0x32, 0x33, 0x34,
                    0x35, 0x36, 0x37, 0x38, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x11,
                    0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x21, 0x22, 0x23, 0x24, 0x12, 0x34,
                    0x56, 0x78, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
                ];
                assert_eq!(&bytes[range], expected_record);
                for end in 0..bytes.len() {
                    assert!(
                        EventManagerReferenceData::decode_reader(version, &bytes[..end]).is_err(),
                        "truncated version-112029 active event at {end}"
                    );
                }
            }

            if version == 120_003 {
                for end in 0..bytes.len() {
                    assert!(
                        EventManagerReferenceData::decode_reader(version, &bytes[..end]).is_err(),
                        "truncated version-{version} active event at {end}"
                    );
                }
            }

            if (120_004..=121_032).contains(&version) {
                let range = decoded.active_event_record_ranges[0].clone();
                let race_data_range = decoded.active_event_race_data_ranges[0]
                    .as_ref()
                    .expect("race-event range exists at this version")
                    .clone();
                assert_eq!(
                    decoded.serialized_body.get(race_data_range),
                    Some(race_event_data_for_version(&EMPTY_RACE_EVENT_DATA, version).as_slice()),
                    "race data for version {version}"
                );
                let expected_race_data =
                    race_event_data_for_version(&EMPTY_RACE_EVENT_DATA, version);
                assert_eq!(
                    &bytes[range.end - expected_race_data.len()..range.end],
                    expected_race_data,
                    "race-data bytes for version {version}"
                );
                assert_eq!(
                    decoded.active_event_race_data_presence_bytes[0],
                    (version >= 121_006).then_some(1),
                    "race-data presence byte for version {version}"
                );
                for end in 0..bytes.len() {
                    assert!(
                        EventManagerReferenceData::decode_reader(version, &bytes[..end]).is_err(),
                        "truncated version-{version} active event at {end}"
                    );
                }
            }
        }

        let bytes = one_active_event(255, flags);
        let (decoded, _) = EventManagerReferenceData::decode_reader(255, &bytes).unwrap();
        let mut editor = decoded.into_event_records_editor();
        editor.records_mut()[0].ticket_totals = None;
        assert!(editor.encode().is_err());
    }

    #[test]
    fn race_event_blocks_are_bounded_and_preserved_raw() {
        let bytes = one_active_event(120_004, 1);
        let (decoded, consumed) = EventManagerReferenceData::decode_reader(120_004, &bytes)
            .expect("empty race-event body should decode");
        assert_eq!(consumed, bytes.len());
        assert_eq!(
            decoded.active_event_race_data[0],
            Some(EventManagerRaceEventData {
                racers: Vec::new(),
                racer_state_records: Vec::new(),
                schedule_id: 0x090a_0b0c,
                lap_count: 0x0d0e_0f10,
                state: 0x1112_1314,
                frames_in_state: 0x1516_1718,
                race_marshal_id: 0x1920,
                track_path_points: Vec::new(),
                track_closed_raw: 0,
                racer_path_index_records: Vec::new(),
                event_end_frame: 0x2122_2324,
                finished_racer_data: [0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c,],
                frames_to_next_overtake: 0x2d2e_2f30,
                racer_overtake_state_records: Vec::new(),
                race_team_member_records: Vec::new(),
                track_path_segments: Vec::new(),
                racer_oscillation_records: Vec::new(),
                racer_state_timer_records: Vec::new(),
                race_team_reference_scope: Default::default(),
                parade_group_reference_scope: Default::default(),
                race_team_scope: Default::default(),
                tier: None,
                spawned_entities: 0x3132_3334,
                collisions_enabled_raw: None,
                route_spline: None,
            })
        );
        let race_data_range = decoded.active_event_race_data_ranges[0]
            .as_ref()
            .expect("race-event range exists")
            .clone();
        assert_eq!(
            decoded.serialized_body.get(race_data_range),
            Some(EMPTY_RACE_EVENT_DATA.as_slice())
        );
        assert_eq!(
            decoded
                .clone()
                .into_event_records_editor()
                .encode()
                .unwrap(),
            bytes
        );

        let versioned_presence = one_active_event(121_006, 1);
        let versioned_race_start = versioned_presence
            .windows(EMPTY_RACE_EVENT_DATA.len())
            .position(|window| window == EMPTY_RACE_EVENT_DATA)
            .expect("version-121006 race-event fixture occurs in the event record");
        let presence_offset = versioned_race_start - 1;
        let mut absent_race = versioned_presence.clone();
        absent_race[presence_offset] = 0;
        absent_race.drain(versioned_race_start..versioned_race_start + EMPTY_RACE_EVENT_DATA.len());
        let (decoded_absent, consumed_absent) =
            EventManagerReferenceData::decode_reader(121_006, &absent_race).unwrap();
        assert_eq!(consumed_absent, absent_race.len());
        assert_eq!(decoded_absent.active_event_race_data_ranges[0], None);
        assert_eq!(decoded_absent.active_event_race_data[0], None);
        assert_eq!(
            decoded_absent.active_event_race_data_presence_bytes[0],
            Some(0)
        );
        assert_eq!(
            decoded_absent.into_event_records_editor().encode().unwrap(),
            absent_race
        );

        absent_race[presence_offset] = 2;
        let (decoded_noncanonical, consumed_noncanonical) =
            EventManagerReferenceData::decode_reader(121_006, &absent_race).unwrap();
        assert_eq!(consumed_noncanonical, absent_race.len());
        assert_eq!(decoded_noncanonical.active_event_race_data_ranges[0], None);
        assert_eq!(
            decoded_noncanonical.active_event_race_data_presence_bytes[0],
            Some(2)
        );
        assert_eq!(decoded_noncanonical.active_event_race_data[0], None);
        assert_eq!(
            decoded_noncanonical
                .clone()
                .into_event_records_editor()
                .encode()
                .unwrap(),
            absent_race
        );
        let mut invalid_editor = decoded_noncanonical.into_event_records_editor();
        invalid_editor.race_event_data_mut()[0] = Some(EventManagerRaceEventData {
            racers: Vec::new(),
            racer_state_records: Vec::new(),
            schedule_id: 1,
            lap_count: 1,
            state: 1,
            frames_in_state: 1,
            race_marshal_id: 1,
            track_path_points: Vec::new(),
            track_closed_raw: 0,
            racer_path_index_records: Vec::new(),
            event_end_frame: 1,
            finished_racer_data: [0; 8],
            frames_to_next_overtake: 1,
            racer_overtake_state_records: Vec::new(),
            race_team_member_records: Vec::new(),
            track_path_segments: Vec::new(),
            racer_oscillation_records: Vec::new(),
            racer_state_timer_records: Vec::new(),
            race_team_reference_scope: Default::default(),
            parade_group_reference_scope: Default::default(),
            race_team_scope: Default::default(),
            tier: None,
            spawned_entities: 0,
            collisions_enabled_raw: None,
            route_spline: None,
        });
        assert!(invalid_editor.encode().is_err());

        let mut one_racer_bytes = bytes.clone();
        let race_start = one_racer_bytes
            .windows(EMPTY_RACE_EVENT_DATA.len())
            .position(|window| window == EMPTY_RACE_EVENT_DATA)
            .expect("race-event fixture occurs in the event record");
        let one_racer = one_racer_race_event_data();
        one_racer_bytes.splice(
            race_start..race_start + EMPTY_RACE_EVENT_DATA.len(),
            one_racer.iter().copied(),
        );
        let (decoded_racer, consumed_racer) =
            EventManagerReferenceData::decode_reader(120_004, &one_racer_bytes).unwrap();
        assert_eq!(consumed_racer, one_racer_bytes.len());
        let racer_range = decoded_racer.active_event_race_data_ranges[0]
            .as_ref()
            .expect("race-event range exists")
            .clone();
        assert_eq!(
            decoded_racer.serialized_body.get(racer_range),
            Some(one_racer.as_slice())
        );
        let race_range = decoded_racer.active_event_race_data_ranges[0]
            .as_ref()
            .expect("race-event range exists")
            .clone();
        assert_eq!(
            decoded_racer.active_event_race_data[0],
            Some(EventManagerRaceEventData {
                racers: vec![super::EventManagerRaceRacer {
                    racer_id: 1,
                    racer_type: 2,
                    spawn_frame: 3,
                    parade_float: 4,
                    performer_ids: vec![5],
                    first_path: 6,
                    second_path: 7,
                    paths_assigned_raw: 1,
                    race_team_index: Some(8),
                    name: None,
                    parade_group_index: Some(9),
                }],
                racer_state_records: vec![[
                    0x3f, 0x80, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x40,
                    0x80, 0x00, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x04,
                ]],
                schedule_id: 0x1112_1314,
                lap_count: 0x1516_1718,
                state: 0x1920_2122,
                frames_in_state: 0x2324_2526,
                race_marshal_id: 0x0027,
                track_path_points: vec![[
                    0x3f, 0x80, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00,
                ]],
                track_closed_raw: 1,
                racer_path_index_records: vec![[0x00, 0x00, 0x00, 0x28]],
                event_end_frame: 0x2930_3132,
                finished_racer_data: [0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04],
                frames_to_next_overtake: 0x3334_3536,
                racer_overtake_state_records: vec![[
                    0x00, 0x00, 0x00, 0x37, 0x3f, 0xc0, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x40,
                    0x40, 0x00, 0x00,
                ]],
                race_team_member_records: vec![[0x38, 0x39, 0x40, 0x41]],
                track_path_segments: vec![0x000d],
                racer_oscillation_records: vec![[0x3f, 0x80, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00,]],
                racer_state_timer_records: vec![[0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,]],
                race_team_reference_scope: crate::EventManagerRaceEventPrefabScope {
                    indexes: vec![10],
                    names: Vec::new(),
                },
                parade_group_reference_scope: crate::EventManagerRaceEventPrefabScope {
                    indexes: vec![11],
                    names: Vec::new(),
                },
                race_team_scope: Default::default(),
                tier: None,
                spawned_entities: 0x4a4b_4c4d,
                collisions_enabled_raw: None,
                route_spline: None,
            })
        );
        for (source_name, replacement_name, value) in [
            (
                &[0x80, 0x00, 0x01, 0xc1][..],
                &[0x80, 0x00, 0x01, 0xc2][..],
                "B",
            ),
            (
                &[0x00, 0x00, 0x01, 0xff, 0xc1][..],
                &[0x00, 0x00, 0x01, 0xff, 0xc2][..],
                "B",
            ),
        ] {
            let mut named_race_data = one_racer.clone();
            // Racer name follows the fixed fields and optional team index.
            named_race_data.splice(26..27, source_name.iter().copied());
            let mut named_bytes = one_racer_bytes.clone();
            named_bytes.splice(race_range.clone(), named_race_data.iter().copied());
            let (decoded_named, _) =
                EventManagerReferenceData::decode_reader(120_004, &named_bytes)
                    .expect("named racer fixture decodes");
            let name_range = decoded_named.active_event_race_data_field_ranges[0]
                .as_ref()
                .expect("named race-event field ranges exist")
                .racers[0]
                .name
                .clone();
            assert_eq!(
                decoded_named.serialized_body.get(name_range.clone()),
                Some(source_name)
            );
            let mut name_editor = decoded_named.clone().into_event_records_editor();
            name_editor.race_event_data_mut()[0]
                .as_mut()
                .expect("named race-event data is present")
                .racers[0]
                .name = Some(value.to_owned());
            let mut expected_named_bytes = named_bytes;
            expected_named_bytes[name_range].copy_from_slice(replacement_name);
            assert_eq!(name_editor.encode().unwrap(), expected_named_bytes);
            let (decoded_renamed, _) =
                EventManagerReferenceData::decode_reader(120_004, &expected_named_bytes)
                    .expect("renamed racer fixture remains parseable");
            assert_eq!(
                decoded_renamed.active_event_race_data[0]
                    .as_ref()
                    .expect("renamed race-event data exists")
                    .racers[0]
                    .name
                    .as_deref(),
                Some(value)
            );
            let mut invalid_length_editor = decoded_named.into_event_records_editor();
            invalid_length_editor.race_event_data_mut()[0]
                .as_mut()
                .expect("named race-event data is present")
                .racers[0]
                .name = Some("longer name".to_owned());
            assert!(invalid_length_editor.encode().is_err());
        }
        let mut scoped_race_data = one_racer.clone();
        // Each reference scope has one assignment and initially no names.
        scoped_race_data.splice(33..35, [0x00, 0x01, 0x80, 0x00, 0x01, 0xc1]);
        scoped_race_data.splice(42..44, [0x00, 0x01, 0x80, 0x00, 0x01, 0xc1]);
        let mut scoped_bytes = one_racer_bytes.clone();
        scoped_bytes.splice(race_range.clone(), scoped_race_data.iter().copied());
        let (decoded_scopes, _) = EventManagerReferenceData::decode_reader(120_004, &scoped_bytes)
            .expect("race-event prefab scopes decode");
        let scoped_race_range = decoded_scopes.active_event_race_data_ranges[0]
            .as_ref()
            .expect("race-event data range exists")
            .clone();
        let decoded_scoped_data = decoded_scopes.active_event_race_data[0]
            .as_ref()
            .expect("race-event data is present");
        assert_eq!(decoded_scoped_data.race_team_reference_scope.indexes, [10]);
        assert_eq!(
            decoded_scoped_data.race_team_reference_scope.names,
            [Some("A".to_owned())]
        );
        assert_eq!(
            decoded_scoped_data.parade_group_reference_scope.indexes,
            [11]
        );
        assert_eq!(
            decoded_scoped_data.parade_group_reference_scope.names,
            [Some("A".to_owned())]
        );
        assert_eq!(decoded_scoped_data.race_team_scope, Default::default());
        let mut scope_editor = decoded_scopes.clone().into_event_records_editor();
        let edited_scoped_data = scope_editor.race_event_data_mut()[0]
            .as_mut()
            .expect("race-event data is present");
        edited_scoped_data.race_team_reference_scope.indexes[0] = 12;
        edited_scoped_data.race_team_reference_scope.names[0] = Some("B".to_owned());
        edited_scoped_data.parade_group_reference_scope.indexes[0] = 13;
        edited_scoped_data.parade_group_reference_scope.names[0] = Some("C".to_owned());
        let mut expected_scoped_race = scoped_race_data.clone();
        expected_scoped_race[31..33].copy_from_slice(&12_u16.to_be_bytes());
        expected_scoped_race[38] = 0xc2; // Compact name "B".
        expected_scoped_race[40..42].copy_from_slice(&13_u16.to_be_bytes());
        expected_scoped_race[47] = 0xc3; // Compact name "C".
        let mut expected_scoped_bytes = scoped_bytes.clone();
        expected_scoped_bytes.splice(scoped_race_range, expected_scoped_race.iter().copied());
        assert_eq!(scope_editor.encode().unwrap(), expected_scoped_bytes);

        let mut team_collection_data = one_racer.clone();
        team_collection_data[143..147].copy_from_slice(&1_i32.to_be_bytes());
        team_collection_data.splice(
            147..149,
            [
                0x00, 0x00, 0x0a, // One RaceTeamInfo assignment, index 10.
                0x00, 0x01, 0x80, 0x00, 0x01, 0xc1, // One compact name, "A".
            ],
        );
        let mut team_collection_bytes = one_racer_bytes.clone();
        team_collection_bytes.splice(race_range.clone(), team_collection_data.iter().copied());
        let (decoded_team_collection, consumed_team_collection) =
            EventManagerReferenceData::decode_reader(120_004, &team_collection_bytes)
                .expect("race-team collection scope decodes");
        assert_eq!(consumed_team_collection, team_collection_bytes.len());
        assert_eq!(
            decoded_team_collection.active_event_race_data[0]
                .as_ref()
                .expect("race-event data is present")
                .race_team_scope,
            crate::EventManagerRaceEventPrefabScope {
                indexes: vec![10],
                names: vec![Some("A".to_owned())],
            }
        );
        let mut team_collection_editor = decoded_team_collection.into_event_records_editor();
        let team_scope = &mut team_collection_editor.race_event_data_mut()[0]
            .as_mut()
            .expect("race-event data is present")
            .race_team_scope;
        team_scope.indexes[0] = 11;
        team_scope.names[0] = Some("B".to_owned());
        let mut expected_team_collection_data = team_collection_data;
        expected_team_collection_data[148..150].copy_from_slice(&11_u16.to_be_bytes());
        expected_team_collection_data[155] = 0xc2; // Compact name "B".
        let mut expected_team_collection_bytes = one_racer_bytes.clone();
        expected_team_collection_bytes.splice(race_range.clone(), expected_team_collection_data);
        assert_eq!(
            team_collection_editor.encode().unwrap(),
            expected_team_collection_bytes
        );

        let mut variable_width_team_data = one_racer.clone();
        variable_width_team_data[143..147].copy_from_slice(&3_i32.to_be_bytes());
        variable_width_team_data.splice(
            147..149,
            [
                0x02, 0x00, 0x0a, 0x00, 0x14, 0x00, 0x1e, // Three literal indexes.
                0x00, 0x00, // No names.
            ],
        );
        let mut variable_width_team_bytes = one_racer_bytes.clone();
        variable_width_team_bytes
            .splice(race_range.clone(), variable_width_team_data.iter().copied());
        let (decoded_variable_width_team, _) =
            EventManagerReferenceData::decode_reader(120_004, &variable_width_team_bytes)
                .expect("race-team literal assignment scope decodes");
        let mut invalid_assignment_width_editor =
            decoded_variable_width_team.into_event_records_editor();
        invalid_assignment_width_editor.race_event_data_mut()[0]
            .as_mut()
            .expect("race-event data is present")
            .race_team_scope
            .indexes = vec![0, 0, 0];
        assert!(invalid_assignment_width_editor.encode().is_err());

        let mut invalid_scope_name_editor = decoded_scopes.clone().into_event_records_editor();
        invalid_scope_name_editor.race_event_data_mut()[0]
            .as_mut()
            .unwrap()
            .race_team_reference_scope
            .names[0] = Some("A longer name".to_owned());
        assert!(invalid_scope_name_editor.encode().is_err());
        let mut invalid_scope_count_editor = decoded_scopes.into_event_records_editor();
        invalid_scope_count_editor.race_event_data_mut()[0]
            .as_mut()
            .unwrap()
            .race_team_reference_scope
            .indexes
            .push(14);
        assert!(invalid_scope_count_editor.encode().is_err());
        let field_ranges = decoded_racer.active_event_race_data_field_ranges[0]
            .as_ref()
            .expect("race-event field ranges exist");
        assert_eq!(field_ranges.schedule_id.start - race_range.start, 72);
        assert_eq!(field_ranges.lap_count.start - race_range.start, 76);
        assert_eq!(field_ranges.state.start - race_range.start, 80);
        assert_eq!(field_ranges.frames_in_state.start - race_range.start, 84);
        assert_eq!(field_ranges.race_marshal_id.start - race_range.start, 88);
        assert_eq!(field_ranges.event_end_frame.start - race_range.start, 111);
        assert_eq!(
            field_ranges.frames_to_next_overtake.start - race_range.start,
            123
        );

        let mut editor = decoded_racer.clone().into_event_records_editor();
        let data = editor.race_event_data_mut()[0]
            .as_mut()
            .expect("race-event data is present");
        data.racers[0].racer_id = 0xabcd;
        data.racers[0].racer_type = 0x1234;
        data.racers[0].spawn_frame = 0xcafe_babe;
        data.racers[0].parade_float = 0x2345;
        data.racers[0].performer_ids[0] = 0x6789;
        data.racers[0].first_path = 0x4567;
        data.racers[0].second_path = 0x89ab;
        data.racers[0].paths_assigned_raw = 2;
        data.racers[0].race_team_index = Some(0xbeef);
        data.racers[0].parade_group_index = Some(0xcafe);
        data.race_team_reference_scope.indexes[0] = 12;
        data.parade_group_reference_scope.indexes[0] = 13;
        data.racer_state_records[0][0] = 0xaa;
        data.racer_state_records[0][23] = 0xbb;
        data.schedule_id = -101;
        data.lap_count = 202;
        data.state = -303;
        data.frames_in_state = 404;
        data.race_marshal_id = 0xabcd;
        data.track_path_points[0][0] = 0xde;
        data.track_closed_raw = 2;
        data.racer_path_index_records[0][3] = 0xfe;
        data.event_end_frame = 0xdead_beef;
        data.finished_racer_data[0] = 0xaa;
        data.frames_to_next_overtake = -505;
        data.racer_overtake_state_records[0][15] = 0x99;
        data.race_team_member_records[0] = [0xaa, 0xbb, 0xcc, 0xdd];
        data.track_path_segments[0] = 0xbeef;
        data.racer_oscillation_records[0][0] = 0x11;
        data.racer_state_timer_records[0][7] = 0x22;
        let mut expected = one_racer_bytes.clone();
        for (offset, value) in [
            (4, 0xabcd_u16.to_be_bytes().to_vec()),
            (6, 0x1234_u16.to_be_bytes().to_vec()),
            (8, 0xcafe_babe_u32.to_be_bytes().to_vec()),
            (12, 0x2345_u16.to_be_bytes().to_vec()),
            (16, 0x6789_u16.to_be_bytes().to_vec()),
            (18, 0x4567_u16.to_be_bytes().to_vec()),
            (20, 0x89ab_u16.to_be_bytes().to_vec()),
            (22, vec![2]),
            (24, 0xbeef_u16.to_be_bytes().to_vec()),
            (28, 0xcafe_u16.to_be_bytes().to_vec()),
            (31, 12_u16.to_be_bytes().to_vec()),
            (36, 13_u16.to_be_bytes().to_vec()),
            (72, (-101_i32).to_be_bytes().to_vec()),
            (76, 202_i32.to_be_bytes().to_vec()),
            (80, (-303_i32).to_be_bytes().to_vec()),
            (84, 404_i32.to_be_bytes().to_vec()),
            (88, 0xabcd_u16.to_be_bytes().to_vec()),
            (94, vec![0xde]),
            (106, vec![2]),
            (110, vec![0xfe]),
            (111, 0xdead_beef_u32.to_be_bytes().to_vec()),
            (115, vec![0xaa]),
            (123, (-505_i32).to_be_bytes().to_vec()),
            (127 + 15, vec![0x99]),
            (153, vec![0xaa, 0xbb, 0xcc, 0xdd]),
            (157, 0xbeef_u16.to_be_bytes().to_vec()),
            (159, vec![0x11]),
            (174, vec![0x22]),
        ] {
            let start = race_range.start + offset;
            expected[start..start + value.len()].copy_from_slice(&value);
        }
        expected[race_range.start + 48] = 0xaa;
        expected[race_range.start + 71] = 0xbb;
        assert_eq!(editor.encode().unwrap(), expected);
        let (decoded_edited, _) = EventManagerReferenceData::decode_reader(120_004, &expected)
            .expect("edited race arrays remain parseable");
        assert_eq!(
            decoded_edited.active_event_race_data[0],
            editor.race_event_data()[0]
        );

        let mut invalid_editor = decoded_racer.clone().into_event_records_editor();
        invalid_editor.race_event_data_mut()[0] = None;
        assert!(invalid_editor.encode().is_err());

        let mut invalid_name_editor = decoded_racer.clone().into_event_records_editor();
        invalid_name_editor.race_event_data_mut()[0]
            .as_mut()
            .unwrap()
            .racers[0]
            .name = Some("new name".to_owned());
        assert!(invalid_name_editor.encode().is_err());

        let mut invalid_reference_editor = decoded_racer.clone().into_event_records_editor();
        invalid_reference_editor.race_event_data_mut()[0]
            .as_mut()
            .unwrap()
            .racers[0]
            .race_team_index = None;
        assert!(invalid_reference_editor.encode().is_err());

        let mut invalid_count_editor = decoded_racer.clone().into_event_records_editor();
        invalid_count_editor.race_event_data_mut()[0]
            .as_mut()
            .unwrap()
            .racers[0]
            .performer_ids
            .clear();
        assert!(invalid_count_editor.encode().is_err());

        let mut invalid_array_count_editor = decoded_racer.clone().into_event_records_editor();
        invalid_array_count_editor.race_event_data_mut()[0]
            .as_mut()
            .unwrap()
            .track_path_points
            .clear();
        assert!(invalid_array_count_editor.encode().is_err());

        let mut invalid_racer_count_editor = decoded_racer.clone().into_event_records_editor();
        invalid_racer_count_editor.race_event_data_mut()[0]
            .as_mut()
            .unwrap()
            .racers
            .clear();
        assert!(invalid_racer_count_editor.encode().is_err());
        assert_eq!(
            decoded_racer.into_event_records_editor().encode().unwrap(),
            one_racer_bytes
        );
        for end in 0..one_racer_bytes.len() {
            assert!(
                EventManagerReferenceData::decode_reader(120_004, &one_racer_bytes[..end]).is_err(),
                "truncated one-racer event body at {end}"
            );
        }

        let race_start = bytes
            .windows(EMPTY_RACE_EVENT_DATA.len())
            .position(|window| window == EMPTY_RACE_EVENT_DATA)
            .expect("race-event fixture occurs in the event record");
        let mut negative_racer_count = bytes.clone();
        negative_racer_count[race_start..race_start + 4].copy_from_slice(&(-1_i32).to_be_bytes());
        assert!(EventManagerReferenceData::decode_reader(120_004, &negative_racer_count).is_err());

        let mut excessive_racer_count = bytes.clone();
        excessive_racer_count[race_start..race_start + 4]
            .copy_from_slice(&1_000_001_i32.to_be_bytes());
        assert!(EventManagerReferenceData::decode_reader(120_004, &excessive_racer_count).is_err());

        let mut open_race_data = EMPTY_RACE_EVENT_DATA.to_vec();
        *open_race_data.last_mut().unwrap() = 1; // RouteSpline is present.
        open_race_data.extend_from_slice(&[
            0, 0, 0, 2, // Two serialized route records.
            0, 1, 0, 2, 0, 3, 0, 4, // Building and node indexes.
            0, // Open route.
            1, 0, 5, // Segment sub-bezier from source 5.
            0xff, 0x12, 0x34, // Unknown enum byte is retained as raw data.
        ]);
        let mut closed_race_data = EMPTY_RACE_EVENT_DATA.to_vec();
        *closed_race_data.last_mut().unwrap() = 1;
        closed_race_data.extend_from_slice(&[
            0, 0, 0, 3, // Three serialized route entries; legacy reader uses two records.
            0, 1, 0, 2, 0, 3, 0, 4, // Building and node indexes.
            1, // Closed route.
            2, 0, 6, // Node sub-bezier from source 6.
            4, 0, 7, // End-building sub-bezier from source 7.
            0x80, 0x12, 0x34, // Legacy closed-spline terminal values.
        ]);

        for version in [120_004, 121_005, 121_006, 121_030, 121_033, 121_034] {
            for (race_data, description) in
                [(&open_race_data, "open"), (&closed_race_data, "closed")]
            {
                let event = one_active_event_with_race_data(version, race_data);
                let (decoded, consumed) =
                    EventManagerReferenceData::decode_reader(version, &event).unwrap();
                assert_eq!(consumed, event.len(), "{description}, version {version}");
                let race_range = decoded.active_event_race_data_ranges[0]
                    .as_ref()
                    .expect("race-event data range exists")
                    .clone();
                let expected_race_data = race_event_data_for_version(race_data, version);
                assert_eq!(
                    decoded.serialized_body.get(race_range.clone()),
                    Some(expected_race_data.as_slice()),
                    "{description}, version {version}"
                );
                let spline = decoded.active_event_race_data[0]
                    .as_ref()
                    .expect("race-event data exists")
                    .route_spline
                    .as_ref()
                    .expect("route spline exists");
                assert_eq!(
                    spline.indexes,
                    [1, 2, 3, 4],
                    "{description}, version {version}"
                );
                assert_eq!(spline.closed_raw, u8::from(description == "closed"));
                let expected_record_count = if description == "closed" && version >= 121_034 {
                    3
                } else {
                    2
                };
                assert_eq!(
                    spline.records.len(),
                    expected_record_count,
                    "{description}, version {version}"
                );
                if description == "open" {
                    assert_eq!(spline.bezier_count, 2);
                    assert_eq!(spline.records[0].kind, 1);
                    assert_eq!(spline.records[0].source, 5);
                    assert_eq!(spline.records[1].kind, 0xff);
                    assert_eq!(spline.records[1].source, 0x1234);
                    assert_eq!(spline.terminal_record, None);
                } else {
                    assert_eq!(spline.bezier_count, 3);
                    assert_eq!(spline.records[0].kind, 2);
                    assert_eq!(spline.records[0].source, 6);
                    assert_eq!(spline.records[1].kind, 4);
                    assert_eq!(spline.records[1].source, 7);
                    if version >= 121_034 {
                        assert_eq!(spline.terminal_record, None);
                        assert_eq!(spline.records[2].kind, 0x80);
                        assert_eq!(spline.records[2].source, 0x1234);
                    } else {
                        assert_eq!(
                            spline.terminal_record,
                            Some(super::EventManagerRaceEventSubBezierRecord {
                                kind: 0x80,
                                source: 0x1234,
                            })
                        );
                    }
                }
                assert_eq!(
                    decoded
                        .clone()
                        .into_event_records_editor()
                        .encode()
                        .unwrap(),
                    event,
                    "{description}, version {version}"
                );

                let mut spline_editor = decoded.into_event_records_editor();
                let edited_spline = spline_editor.race_event_data_mut()[0]
                    .as_mut()
                    .expect("race-event data exists")
                    .route_spline
                    .as_mut()
                    .expect("route spline exists");
                edited_spline.indexes[0] = 0xabcd;
                edited_spline.records[0].source = 0x1234;
                if let Some(terminal) = edited_spline.terminal_record.as_mut() {
                    terminal.kind = 0x55;
                } else if description == "closed" && version >= 121_034 {
                    edited_spline.records[2].kind = 0x55;
                }
                let mut expected_race_data = (*race_data).clone();
                let spline_start = EMPTY_RACE_EVENT_DATA.len();
                expected_race_data[spline_start + 4..spline_start + 6]
                    .copy_from_slice(&0xabcd_u16.to_be_bytes());
                expected_race_data[spline_start + 14..spline_start + 16]
                    .copy_from_slice(&0x1234_u16.to_be_bytes());
                if description == "closed" {
                    expected_race_data[spline_start + 19] = 0x55;
                }
                let expected_event = one_active_event_with_race_data(version, &expected_race_data);
                assert_eq!(
                    spline_editor.encode().unwrap(),
                    expected_event,
                    "edited {description}, version {version}"
                );

                let route_data_end = race_range.end;
                let truncated = event[..route_data_end - 1].to_vec();
                assert!(
                    EventManagerReferenceData::decode_reader(version, &truncated).is_err(),
                    "truncated {description} route spline at version {version}"
                );
            }
        }

        let closed_event = one_active_event_with_race_data(120_004, &closed_race_data);
        let (decoded_closed, _) =
            EventManagerReferenceData::decode_reader(120_004, &closed_event).unwrap();
        let mut invalid_spline_count_editor = decoded_closed.clone().into_event_records_editor();
        invalid_spline_count_editor.race_event_data_mut()[0]
            .as_mut()
            .unwrap()
            .route_spline
            .as_mut()
            .unwrap()
            .bezier_count += 1;
        assert!(invalid_spline_count_editor.encode().is_err());

        let mut invalid_spline_topology_editor = decoded_closed.into_event_records_editor();
        invalid_spline_topology_editor.race_event_data_mut()[0]
            .as_mut()
            .unwrap()
            .route_spline
            .as_mut()
            .unwrap()
            .records
            .pop();
        assert!(invalid_spline_topology_editor.encode().is_err());

        let mut empty_closed_race_data = EMPTY_RACE_EVENT_DATA.to_vec();
        *empty_closed_race_data.last_mut().unwrap() = 1;
        empty_closed_race_data.extend_from_slice(&[
            0, 0, 0, 0, // A closed legacy spline cannot have a zero count.
            0, 1, 0, 2, 0, 3, 0, 4, 1,
        ]);
        let malformed = one_active_event_with_race_data(120_004, &empty_closed_race_data);
        assert!(EventManagerReferenceData::decode_reader(120_004, &malformed).is_err());

        let mut excessive_race_data = open_race_data.clone();
        let spline_start = EMPTY_RACE_EVENT_DATA.len();
        excessive_race_data[spline_start..spline_start + 4]
            .copy_from_slice(&1_000_001_u32.to_be_bytes());
        let malformed = one_active_event_with_race_data(120_004, &excessive_race_data);
        assert!(EventManagerReferenceData::decode_reader(120_004, &malformed).is_err());
    }

    #[test]
    fn race_event_fields_follow_121031_and_121032_reader_gates() {
        let mut race_data = one_racer_race_event_data();
        let mut reader = Reader::new(&race_data);
        let (_, base_ranges) =
            super::skip_race_event_data(&mut reader, 120_004).expect("base race-event fixture");
        let legacy_values_range = base_ranges
            .legacy_counted_values
            .expect("legacy values exist before version 121032");
        race_data[legacy_values_range.start..legacy_values_range.start + 4]
            .copy_from_slice(&1_i32.to_be_bytes());
        race_data.splice(
            legacy_values_range.end..legacy_values_range.end,
            [0x12, 0x34],
        );

        let version_121031_body = race_event_data_for_version(&race_data, 121_031);
        let version_121031_event = one_active_event_with_race_data(121_031, &race_data);
        let (decoded_121031, consumed_121031) =
            EventManagerReferenceData::decode_reader(121_031, &version_121031_event)
                .expect("version-121031 RaceEventData decodes");
        assert_eq!(consumed_121031, version_121031_event.len());
        let decoded_121031_data = decoded_121031.active_event_race_data[0]
            .as_ref()
            .expect("race-event data is present");
        assert_eq!(decoded_121031_data.tier, None);
        assert_eq!(decoded_121031_data.spawned_entities, 0x4a4b_4c4d);
        assert_eq!(decoded_121031_data.collisions_enabled_raw, Some(0));
        assert_eq!(
            decoded_121031.active_event_race_data_ranges[0]
                .as_ref()
                .and_then(|range| decoded_121031.serialized_body.get(range.clone())),
            Some(version_121031_body.as_slice())
        );
        let field_ranges_121031 = decoded_121031.active_event_race_data_field_ranges[0]
            .as_ref()
            .expect("race-event field ranges are present")
            .clone();
        assert_eq!(
            field_ranges_121031
                .legacy_counted_values
                .as_ref()
                .and_then(|range| decoded_121031.serialized_body.get(range.clone())),
            Some(&[0, 0, 0, 1, 0x12, 0x34][..])
        );
        let mut editor_121031 = decoded_121031.into_event_records_editor();
        let edited_121031 = editor_121031.race_event_data_mut()[0]
            .as_mut()
            .expect("race-event data is present");
        edited_121031.spawned_entities = -101;
        edited_121031.collisions_enabled_raw = Some(2);
        let mut expected_121031 = version_121031_event;
        expected_121031[field_ranges_121031.spawned_entities.clone()]
            .copy_from_slice(&(-101_i32).to_be_bytes());
        expected_121031[field_ranges_121031
            .collisions_enabled_raw
            .clone()
            .unwrap()
            .start] = 2;
        assert_eq!(editor_121031.encode().unwrap(), expected_121031);

        let version_121032_body = race_event_data_for_version(&race_data, 121_032);
        let version_121032_event = one_active_event_with_race_data(121_032, &race_data);
        let (decoded_121032, consumed_121032) =
            EventManagerReferenceData::decode_reader(121_032, &version_121032_event)
                .expect("version-121032 RaceEventData decodes");
        assert_eq!(consumed_121032, version_121032_event.len());
        let decoded_121032_data = decoded_121032.active_event_race_data[0]
            .as_ref()
            .expect("race-event data is present");
        assert_eq!(decoded_121032_data.tier, Some(0));
        assert_eq!(decoded_121032_data.spawned_entities, 0x4a4b_4c4d);
        assert_eq!(decoded_121032_data.collisions_enabled_raw, Some(0));
        assert_eq!(
            decoded_121032.active_event_race_data_ranges[0]
                .as_ref()
                .and_then(|range| decoded_121032.serialized_body.get(range.clone())),
            Some(version_121032_body.as_slice())
        );
        let field_ranges_121032 = decoded_121032.active_event_race_data_field_ranges[0]
            .as_ref()
            .expect("race-event field ranges are present")
            .clone();
        assert_eq!(field_ranges_121032.legacy_counted_values, None);
        let mut editor_121032 = decoded_121032.into_event_records_editor();
        let edited_121032 = editor_121032.race_event_data_mut()[0]
            .as_mut()
            .expect("race-event data is present");
        edited_121032.tier = Some(-17);
        edited_121032.spawned_entities = 303;
        edited_121032.collisions_enabled_raw = Some(1);
        let mut expected_121032 = version_121032_event;
        expected_121032[field_ranges_121032.tier.clone().unwrap()]
            .copy_from_slice(&(-17_i32).to_be_bytes());
        expected_121032[field_ranges_121032.spawned_entities.clone()]
            .copy_from_slice(&303_i32.to_be_bytes());
        expected_121032[field_ranges_121032
            .collisions_enabled_raw
            .clone()
            .unwrap()
            .start] = 1;
        assert_eq!(editor_121032.encode().unwrap(), expected_121032);

        for invalid_count in [-1_i32, 1_000_001_i32] {
            let mut malformed = race_data.clone();
            malformed[legacy_values_range.start..legacy_values_range.start + 4]
                .copy_from_slice(&invalid_count.to_be_bytes());
            let mut malformed_reader = Reader::new(&malformed);
            assert!(super::skip_race_event_data(&mut malformed_reader, 120_004).is_err());
        }
        let mut truncated = race_data;
        truncated[legacy_values_range.start..legacy_values_range.start + 4]
            .copy_from_slice(&1_i32.to_be_bytes());
        truncated.drain(legacy_values_range.end..legacy_values_range.end + 2);
        let mut truncated_reader = Reader::new(&truncated);
        assert!(super::skip_race_event_data(&mut truncated_reader, 120_004).is_err());
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
