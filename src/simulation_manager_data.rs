use std::collections::{HashMap, HashSet};

use crate::CrpError;
use crate::GameAreaManagerData;
use crate::InstanceManagerData;
use crate::InstanceManagerEditor;
use crate::PathManagerData;
use crate::PropManagerDataPrefix;
use crate::TerrainManagerData;
use crate::TreeManagerDataPrefix;
use crate::WaterSimulationData;
use crate::WaterSimulationEditor;
use crate::ZoneManagerDataPrefix;
use crate::data_serializer::{DataSerializerStream, Reader, write_u24_be, write_unique_string};
use crate::encoded_array::{
    read_encoded_float_array, read_encoded_uint_array, read_encoded_ushort_array,
};
use crate::guide_data::{
    GUIDE_INSTANCE_BODY_LEN, GUIDE_TRIGGER_BASE_PREFIX_LEN, GuideInstanceBody, GuideInstanceEditor,
    GuideTriggerBaseEditor, GuideTriggerBasePrefix,
};
use crate::guide_manager_data::GuideManagerReferences;
use crate::message_manager_data::{MessageManagerData, MessageManagerDataEditor};
use crate::net_manager_data::{
    NetNodeInstanceGuideBody, NetNodeInstanceGuideEditor, NetObjectHeader, read_net_object_header,
};
use crate::statistics_manager_data::{
    StatisticInt32Editor, StatisticInt64Editor, StatisticsManagerData, StatisticsManagerEditor,
};
use crate::unlock_manager_data::UnlockManagerReferences;
use crate::{
    BuildingBufferFieldsPrefix, BuildingByteStatePrefix, BuildingEconomicFieldsPrefix,
    BuildingFacultyBonusesEditor, BuildingFacultyBonusesPrefix, BuildingFlagsPrefix,
    BuildingInstanceRecordsEditor, BuildingInstanceRecordsPrefix,
    BuildingManagerObjectHeadersPrefix, BuildingMonumentEffectsEditor,
    BuildingMonumentEffectsPrefix, BuildingPopulationPrefix, BuildingPrefabPrefix,
    BuildingProblemEditor, BuildingProblemPrefix, BuildingRaceTeamReferencesEditor,
    BuildingRaceTeamReferencesPrefix, BuildingRaceTeamUnassignedEditor,
    BuildingRaceTeamUnassignedPrefix, BuildingServiceSelectionPrefix, BuildingTimerGeometryPrefix,
};
use crate::{BuildingEconomicFieldsEditor, BuildingPopulationEditor, BuildingTimerGeometryEditor};
use crate::{
    DistrictManagerFlagsPrefix, DistrictManagerGridData, DistrictManagerTailData,
    DistrictParkEditor, DistrictParkRecord,
};
use crate::{
    NetFencePrefabPrefix, NetGuideCountsPrefix, NetInfoPrefabPrefix, NetLanePrefix,
    NetManagerTailPrefix, NetNodeCountersPrefix, NetNodeFlagsPrefix, NetNodeProblemsPrefix,
    NetNodeRecordsPrefix, NetSegmentCoveragePrefix, NetSegmentEnvironmentPrefix,
    NetSegmentFlagsPrefix, NetSegmentProblemsPrefix, NetSegmentRecordsPrefix,
    NetSegmentTrafficLightsPrefix, NetTreePrefabPrefix,
};

const ROOT_TYPE: &str = "SimulationManager+Data, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null";
const BUILD_INDEX_HISTORY_LENGTH: usize = 32;
const MAX_MANAGER_OBJECTS: u32 = 1_000_000;
const MAX_STORAGE_ENTRIES: u32 = 1_000_000;
const MAX_OBJECT_ID: u32 = 0xfe_0000;
const MAX_SHARED_TYPES: usize = 65_024;
const LOADING_PROFILER_END_TAG_BYTES: usize = 2;
const MAX_TRAILING_HEADER_CANDIDATES: usize = 64;
const ECONOMY_INVESTMENT_FLAG_COUNT: usize = 254;

/// A non-null reference in `SimulationManager+Data`'s manager-data array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagerObjectReference {
    /// Object-list ID assigned by DataSerializer.
    pub object_id: u32,
    /// Assembly-qualified runtime type name resolved from the shared-type list.
    pub type_name: String,
}

/// One non-null queued DataSerializer reference resolved to its object ID and type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedObjectReference {
    /// Object-list ID assigned by DataSerializer.
    pub object_id: u32,
    /// Assembly-qualified runtime type name resolved from the shared-type list.
    pub type_name: String,
}

/// One active EconomyManager investment's serialized float history.
#[derive(Debug, Clone, PartialEq)]
pub struct EconomyInvestmentValueSeries {
    /// Investment index in the game's serialized range 1–254.
    pub investment_index: u16,
    /// Float values retained in the order written by the game.
    pub values: Vec<f32>,
}

/// Modeled numeric fields for one active EconomyManager investment.
///
/// Prefab assignments and names remain in the opaque bytes following these
/// fields and are not represented here. Fields introduced after the file's
/// inner-version gate use their game defaults when they are absent.
#[derive(Debug, Clone, PartialEq)]
pub struct EconomyInvestmentData {
    /// Investment index in the game's serialized range 1–254.
    pub investment_index: u16,
    /// Float history values retained in serialized order.
    pub value_history: Vec<f32>,
    /// Amount of money spent on the investment.
    pub player_spent: i32,
    /// Number of owned shares.
    pub owned_share_count: u32,
    /// Base value recorded for the investment.
    pub base_value: f32,
    /// .NET `DateTime` ticks for the missing requirement date.
    pub requirement_missing_date_ticks: i64,
    /// Serialized investment state.
    pub current_state: i16,
    /// Random influence value.
    pub random_influence: f32,
}

/// A prefab index associated with one active EconomyManager investment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EconomyInvestmentPrefabAssignment {
    /// Investment index in the game's serialized range 1–254.
    pub investment_index: u16,
    /// Index in the serialized prefab collection.
    pub prefab_index: u16,
}

/// Prefab assignments and the serialized investment-prefab name table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EconomyInvestmentPrefabTable {
    /// One assignment for each active investment, in investment order.
    pub assignments: Vec<EconomyInvestmentPrefabAssignment>,
    /// Names in the game's serialized table order; null entries are retained.
    pub names: Vec<Option<String>>,
}

/// One entry in `SimulationManager.m_serializableDataStorage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializableDataEntry {
    /// Data key stored by the game or a mod.
    pub key: String,
    /// Serialized bytes; a null value is preserved as `None`.
    pub data: Option<Vec<u8>>,
}

/// Location and declared type of one queued `BuildingManager` guide body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideBodyLocation {
    /// DataSerializer object-list ID referenced by BuildingManager.
    pub object_id: u32,
    /// Byte offset relative to [`SimulationManagerData::serialized_object_graph`].
    pub graph_offset: usize,
    /// Serialized body length derived from the declared guide type.
    pub body_length: usize,
    /// Assembly-qualified type name from the object-reference table.
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QueuedGuideObject {
    object_id: u32,
    type_name: String,
}

#[derive(Debug)]
struct QueuedObjectReferencePrefix {
    children_start: usize,
    object_table: DataSerializerObjectTable,
    queued_objects: Vec<QueuedGuideObject>,
    net_manager_object_ids: Vec<Option<u32>>,
    water_manager_object_ids: Vec<Option<u32>>,
    zone_manager_object_ids: Vec<Option<u32>>,
    transport_manager_object_ids: Vec<Option<u32>>,
    economy_manager_object_ids: Vec<Option<u32>>,
    district_manager_object_ids: Vec<Option<u32>>,
}

#[derive(Debug, Clone)]
struct DataSerializerObjectTable {
    shared_types: Vec<String>,
    object_types: Vec<String>,
}

impl DataSerializerObjectTable {
    fn new(root_type: &str) -> Self {
        Self {
            shared_types: vec![root_type.to_owned()],
            object_types: vec![root_type.to_owned()],
        }
    }

    fn register(&mut self, header: &NetObjectHeader) -> Result<Option<u32>, CrpError> {
        match header {
            NetObjectHeader::Null => Ok(None),
            NetObjectHeader::Existing(object_id) => {
                let index = usize::try_from(*object_id)
                    .map_err(|_| CrpError::ArithmeticOverflow("DataSerializer object ID"))?;
                if self.object_types.get(index).is_none() {
                    return Err(invalid(
                        0,
                        format!("object ID {object_id} is outside the object-list table"),
                    ));
                }
                Ok(Some(*object_id))
            }
            NetObjectHeader::NewType(type_name) => {
                if self.shared_types.len() >= MAX_SHARED_TYPES {
                    return Err(invalid(0, "shared-type table exceeds the game limit"));
                }
                self.shared_types.push(type_name.clone());
                self.allocate(type_name)
            }
            NetObjectHeader::NewSharedType(type_index) => {
                let type_name = self
                    .shared_types
                    .get(usize::from(*type_index))
                    .cloned()
                    .ok_or_else(|| {
                        invalid(
                            0,
                            format!("shared-type index {type_index} is outside the type table"),
                        )
                    })?;
                self.allocate(&type_name)
            }
        }
    }

    fn allocate(&mut self, type_name: &str) -> Result<Option<u32>, CrpError> {
        let object_id = u32::try_from(self.object_types.len())
            .map_err(|_| CrpError::ArithmeticOverflow("DataSerializer object ID"))?;
        if object_id >= MAX_OBJECT_ID {
            return Err(invalid(0, "object ID exceeds the DataSerializer limit"));
        }
        self.object_types.push(type_name.to_owned());
        Ok(Some(object_id))
    }

    fn type_name(&self, object_id: u32) -> Option<&str> {
        self.object_types
            .get(usize::try_from(object_id).ok()?)
            .map(String::as_str)
    }
}

/// Typed root fields of a type-52 `SimulationManager+Data` stream.
///
/// Manager-specific object bodies are retained as opaque bytes. The manager
/// reference array's original encoding is also retained, while resolved object
/// references are available through [`manager_objects`](Self::manager_objects).
/// The game uses a fixed 32-element build-index history in the versions
/// supported here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationManagerData {
    format_version: u32,
    /// Current simulation tick.
    pub current_tick_index: u32,
    /// Current simulation frame.
    pub current_frame_index: u32,
    /// Current build index.
    pub current_build_index: u32,
    /// Randomizer seed.
    pub randomizer_seed: u64,
    /// Day-time offset; absent below DataSerializer version 212.
    pub day_time_offset_frames: Option<u32>,
    /// The game's fixed-size build-index history.
    pub build_index_history: Vec<u32>,
    /// Serializable data owned by the game and mods; absent below version 186.
    pub serializable_data_storage: Option<Vec<SerializableDataEntry>>,
    root_fields_len: usize,
    root_profile_begin_bytes: Vec<u8>,
    root_profile_end_bytes: Vec<u8>,
    manager_objects: Option<Vec<Option<ManagerObjectReference>>>,
    manager_array_bytes: Vec<u8>,
    serialized_object_graph: Vec<u8>,
}

impl SimulationManagerData {
    /// Decodes the second stream in a map's type-52 `_Data` payload.
    ///
    /// Pass the bytes beginning at the second stream's version word, commonly
    /// the suffix returned by [`crate::SimulationMetaData::decode_from`]. The
    /// typed root fields are decoded and all later manager object bodies are
    /// retained unchanged. This codec supports inner versions 164 and newer;
    /// earlier versions have a different history and manager-data layout.
    /// Invalid root types, object/shared-type indices, counts, or truncated data
    /// return errors instead of panicking.
    pub fn decode(bytes: &[u8]) -> Result<Self, CrpError> {
        let stream = DataSerializerStream::decode(bytes)?;
        if stream.root_type != ROOT_TYPE {
            return Err(invalid(0, "root type is not SimulationManager+Data"));
        }
        let version = stream.format_version;
        if version < 164 {
            return Err(invalid(
                0,
                format!("DataSerializer version {version} predates the 32-entry build history"),
            ));
        }

        let mut reader = Reader::new(&stream.root_data);
        let mut shared_strings = Vec::new();
        let begin_start = reader.offset();
        expect_profile_tag(&mut reader, &mut shared_strings, "SimulationManager")?;
        let root_profile_begin_bytes = reader.consumed_bytes()[begin_start..].to_vec();
        let current_tick_index = reader.read_u32_be()?;
        let current_frame_index = reader.read_u32_be()?;
        let current_build_index = reader.read_u32_be()?;
        let randomizer_seed = read_u64_be(&mut reader)?;
        let day_time_offset_frames = if version >= 212 {
            Some(reader.read_u32_be()?)
        } else {
            None
        };

        let mut build_index_history = Vec::with_capacity(BUILD_INDEX_HISTORY_LENGTH);
        for _ in 0..BUILD_INDEX_HISTORY_LENGTH {
            build_index_history.push(reader.read_u32_be()?);
        }

        let manager_array_start = reader.offset();
        let manager_objects = read_manager_array(&mut reader, &stream.root_type)?;
        let manager_array_end = reader.offset();
        let manager_array_bytes =
            reader.consumed_bytes()[manager_array_start..manager_array_end].to_vec();

        let serializable_data_storage = if version >= 186 {
            let count = reader.read_u32_be()?;
            if count > MAX_STORAGE_ENTRIES {
                return Err(reader.invalid("serializable-data entry count exceeds 1,000,000"));
            }
            if usize::try_from(count).map_or(true, |count| count > reader.remaining() / 2) {
                return Err(reader.invalid("serializable-data count cannot fit in remaining bytes"));
            }
            let count = usize::try_from(count).map_err(|_| {
                reader.invalid("serializable-data count does not fit this platform")
            })?;
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                let key = reader
                    .read_unique_string()?
                    .ok_or_else(|| reader.invalid("serializable-data key cannot be null"))?;
                let data = read_byte_array(&mut reader)?;
                entries.push(SerializableDataEntry { key, data });
            }
            Some(entries)
        } else {
            None
        };

        let end_start = reader.offset();
        expect_profile_tag(&mut reader, &mut shared_strings, "SimulationManager")?;
        let root_profile_end_bytes = reader.consumed_bytes()[end_start..].to_vec();

        Ok(Self {
            format_version: version,
            current_tick_index,
            current_frame_index,
            current_build_index,
            randomizer_seed,
            day_time_offset_frames,
            build_index_history,
            serializable_data_storage,
            root_fields_len: stream.header_len.checked_add(reader.offset()).ok_or(
                CrpError::ArithmeticOverflow("SimulationManager+Data root length"),
            )?,
            root_profile_begin_bytes,
            root_profile_end_bytes,
            manager_objects,
            manager_array_bytes,
            serialized_object_graph: reader.remaining_bytes().to_vec(),
        })
    }

    /// Returns this stream's inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns the stream length through the root fields, before queued manager
    /// objects are serialized.
    pub fn root_fields_len(&self) -> usize {
        self.root_fields_len
    }

    /// Returns the manager data references in their original order.
    pub fn manager_objects(&self) -> Option<&[Option<ManagerObjectReference>]> {
        self.manager_objects.as_deref()
    }

    /// Returns the uninterpreted queued manager-object serialization bytes.
    pub fn serialized_object_graph(&self) -> &[u8] {
        &self.serialized_object_graph
    }

    /// Decodes the version-gated object references stored in `GuideManager+Data`.
    ///
    /// The returned headers retain their exact field order and the profiler
    /// end index. The index is not resolved against the full DataSerializer
    /// shared-string table. Shared-type and existing-object IDs are also not
    /// resolved because that requires replaying every preceding queued object.
    /// The rest of the manager graph remains byte-preserved.
    pub fn decode_guide_manager_references(&self) -> Result<GuideManagerReferences, CrpError> {
        unique_manager_slot(self.manager_objects.as_deref(), "GuideManager+Data")?;
        self.decode_guide_manager_references_at()
            .map(|(references, _)| references)
    }

    /// Decodes the inline `StatisticsManager+Data` fields.
    ///
    /// The result contains the start frame and raw `StatisticArray` reference
    /// header, with the original profiler end index retained for exact
    /// writing. The index is not resolved against the full DataSerializer
    /// shared-string table, and the reference is not resolved to a queued
    /// object ID.
    pub fn decode_statistics_manager_data(&self) -> Result<StatisticsManagerData, CrpError> {
        unique_manager_slot(self.manager_objects.as_deref(), "StatisticsManager+Data")?;
        self.decode_statistics_manager_data_at()
            .map(|(data, _)| data)
    }

    fn decode_statistics_manager_data_at(
        &self,
    ) -> Result<(StatisticsManagerData, usize), CrpError> {
        let tag = new_profiler_tag_bytes("StatisticsManager")?;
        let tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &tag,
            "StatisticsManager profiler tag",
        )?;
        let body_start = tag_start
            .checked_add(tag.len())
            .ok_or(CrpError::ArithmeticOverflow("StatisticsManager body start"))?;
        let (data, consumed) = StatisticsManagerData::decode(
            self.format_version,
            &self.serialized_object_graph[body_start..],
        )?;
        let body_end = body_start
            .checked_add(consumed)
            .ok_or(CrpError::ArithmeticOverflow("StatisticsManager body end"))?;
        Ok((data, body_end))
    }

    /// Decodes the versioned milestone-reference arrays in `UnlockManager+Data`.
    ///
    /// The decoder retains unresolved object references and the profiler end
    /// index. It supports the current array layout from inner version 127,
    /// except for versions 262–278 and non-empty scenario-trigger data. The
    /// body boundary is checked against the following `MessageManager+Data`
    /// profiler tag in the serialized object graph.
    pub fn decode_unlock_manager_references(&self) -> Result<UnlockManagerReferences, CrpError> {
        let slot = unique_manager_slot(self.manager_objects.as_deref(), "UnlockManager+Data")?;
        let next_slot = slot
            .checked_add(1)
            .ok_or(CrpError::ArithmeticOverflow("UnlockManager next slot"))?;
        expect_manager_type(
            self.manager_objects.as_deref(),
            next_slot,
            "MessageManager+Data",
        )?;
        self.decode_unlock_manager_references_at()
            .map(|(references, _)| references)
    }

    fn decode_unlock_manager_references_at(
        &self,
    ) -> Result<(UnlockManagerReferences, usize), CrpError> {
        let tag = new_profiler_tag_bytes("UnlockManager")?;
        let tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &tag,
            "UnlockManager profiler tag",
        )?;
        let body_start = tag_start
            .checked_add(tag.len())
            .ok_or(CrpError::ArithmeticOverflow("UnlockManager body start"))?;
        let (references, consumed) = UnlockManagerReferences::decode(
            self.format_version,
            &self.serialized_object_graph[body_start..],
        )?;
        let body_end = body_start
            .checked_add(consumed)
            .ok_or(CrpError::ArithmeticOverflow("UnlockManager body end"))?;
        let next_tag = new_profiler_tag_bytes("MessageManager")?;
        let next_tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &next_tag,
            "MessageManager profiler tag",
        )?;
        if body_end != next_tag_start {
            return Err(invalid(
                body_end,
                format!(
                    "UnlockManager ends at {body_end}, but MessageManager begins at {next_tag_start}"
                ),
            ));
        }
        Ok((references, body_end))
    }

    /// Decodes the versioned message arrays and counters in `MessageManager+Data`.
    ///
    /// Object references and message strings remain as decoded values, while
    /// the original body is retained for exact unchanged writing. The body
    /// boundary is checked against the following `GuideManager+Data` tag.
    pub fn decode_message_manager_data(&self) -> Result<MessageManagerData, CrpError> {
        let slot = unique_manager_slot(self.manager_objects.as_deref(), "MessageManager+Data")?;
        let next_slot = slot
            .checked_add(1)
            .ok_or(CrpError::ArithmeticOverflow("MessageManager next slot"))?;
        expect_manager_type(
            self.manager_objects.as_deref(),
            next_slot,
            "GuideManager+Data",
        )?;
        self.decode_message_manager_data_at().map(|(data, _)| data)
    }

    fn decode_message_manager_data_at(&self) -> Result<(MessageManagerData, usize), CrpError> {
        let tag = new_profiler_tag_bytes("MessageManager")?;
        let tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &tag,
            "MessageManager profiler tag",
        )?;
        let body_start = tag_start
            .checked_add(tag.len())
            .ok_or(CrpError::ArithmeticOverflow("MessageManager body start"))?;
        let (data, consumed) = MessageManagerData::decode(
            self.format_version,
            &self.serialized_object_graph[body_start..],
        )?;
        let body_end = body_start
            .checked_add(consumed)
            .ok_or(CrpError::ArithmeticOverflow("MessageManager body end"))?;
        let next_tag = new_profiler_tag_bytes("GuideManager")?;
        let next_tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &next_tag,
            "GuideManager profiler tag",
        )?;
        if body_end != next_tag_start {
            return Err(invalid(
                body_end,
                format!(
                    "MessageManager ends at {body_end}, but GuideManager begins at {next_tag_start}"
                ),
            ));
        }
        Ok((data, body_end))
    }

    /// Replaces the start frame in the queued `StatisticsManager+Data` body.
    ///
    /// Create `edited` from this manager's decoded statistics body. The inner
    /// version and original bytes must still match. Only the four-byte start
    /// frame changes, so the statistic-array reference, profiler index, queue
    /// offsets, and object IDs remain unchanged.
    pub fn replace_statistics_manager_data(
        &mut self,
        edited: &StatisticsManagerEditor,
    ) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                0,
                "edited StatisticsManager version does not match this stream",
            ));
        }
        unique_manager_slot(self.manager_objects.as_deref(), "StatisticsManager+Data")?;
        let tag = new_profiler_tag_bytes("StatisticsManager")?;
        let tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &tag,
            "StatisticsManager profiler tag",
        )?;
        let body_start = tag_start
            .checked_add(tag.len())
            .ok_or(CrpError::ArithmeticOverflow("StatisticsManager body start"))?;
        let original = edited.original_body_bytes();
        let body_end = body_start
            .checked_add(original.len())
            .ok_or(CrpError::ArithmeticOverflow("StatisticsManager body end"))?;
        let Some(current) = self.serialized_object_graph.get(body_start..body_end) else {
            return Err(invalid(
                body_start,
                "StatisticsManager body extends past manager graph",
            ));
        };
        if current != original {
            return Err(invalid(
                body_start,
                "StatisticsManager bytes do not match the editor's source body",
            ));
        }
        let replacement = edited.encode();
        if replacement.len() != original.len() {
            return Err(invalid(
                body_start,
                "StatisticsManager replacement must preserve the original length",
            ));
        }
        self.serialized_object_graph[body_start..body_end].copy_from_slice(&replacement);
        Ok(())
    }

    /// Replaces editable fields in the queued `MessageManager+Data` body.
    ///
    /// Create `edited` from this manager's decoded message body. The inner
    /// version and original bytes must still match. The editor changes only
    /// fixed-width counters and stored timer bytes, leaving object references,
    /// body length, queue offsets, and object IDs unchanged.
    pub fn replace_message_manager_data(
        &mut self,
        edited: &MessageManagerDataEditor,
    ) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                0,
                "edited MessageManager version does not match this stream",
            ));
        }
        self.decode_message_manager_data()?;
        let tag = new_profiler_tag_bytes("MessageManager")?;
        let tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &tag,
            "MessageManager profiler tag",
        )?;
        let body_start = tag_start
            .checked_add(tag.len())
            .ok_or(CrpError::ArithmeticOverflow("MessageManager body start"))?;
        let original = edited.original_body_bytes();
        let body_end = body_start
            .checked_add(original.len())
            .ok_or(CrpError::ArithmeticOverflow("MessageManager body end"))?;
        let Some(current) = self.serialized_object_graph.get(body_start..body_end) else {
            return Err(invalid(
                body_start,
                "MessageManager body extends past manager graph",
            ));
        };
        if current != original {
            return Err(invalid(
                body_start,
                "MessageManager bytes do not match the editor's source body",
            ));
        }
        let replacement = edited.encode();
        if replacement.len() != original.len() {
            return Err(invalid(
                body_start,
                "MessageManager replacement must preserve the original length",
            ));
        }
        self.serialized_object_graph[body_start..body_end].copy_from_slice(&replacement);
        Ok(())
    }

    /// Replaces one already-located `StatisticInt32` body in the queued graph.
    ///
    /// `graph_offset` is relative to [`serialized_object_graph`](Self::serialized_object_graph),
    /// after the root fields. Create `edited` from the body at that offset.
    /// The source bytes and inner version must still match. Editing the encoded
    /// history can change the body's length and shifts all later queued bytes;
    /// this method does not locate the record or validate its declared object
    /// type, so callers must derive the offset from the preceding queue.
    pub fn replace_statistic_int32_at(
        &mut self,
        graph_offset: usize,
        edited: &StatisticInt32Editor,
    ) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                graph_offset,
                "StatisticInt32 editor uses a different DataSerializer version",
            ));
        }
        self.replace_queued_bytes_at(
            graph_offset,
            edited.original_body_bytes(),
            &edited.encode(),
            "StatisticInt32 body",
            false,
        )
    }

    /// Replaces one already-located `StatisticInt64` body in the queued graph.
    ///
    /// `graph_offset` is relative to [`serialized_object_graph`](Self::serialized_object_graph),
    /// after the root fields. Create `edited` from the body at that offset.
    /// The source bytes and inner version must still match. Editing the encoded
    /// history can change the body's length and shifts all later queued bytes;
    /// this method does not locate the record or validate its declared object
    /// type, so callers must derive the offset from the preceding queue.
    pub fn replace_statistic_int64_at(
        &mut self,
        graph_offset: usize,
        edited: &StatisticInt64Editor,
    ) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                graph_offset,
                "StatisticInt64 editor uses a different DataSerializer version",
            ));
        }
        self.replace_queued_bytes_at(
            graph_offset,
            edited.original_body_bytes(),
            &edited.encode(),
            "StatisticInt64 body",
            false,
        )
    }

    fn decode_guide_manager_references_at(
        &self,
    ) -> Result<(GuideManagerReferences, usize), CrpError> {
        let tag = new_profiler_tag_bytes("GuideManager")?;
        let tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &tag,
            "GuideManager profiler tag",
        )?;
        let body_start = tag_start
            .checked_add(tag.len())
            .ok_or(CrpError::ArithmeticOverflow("GuideManager body start"))?;
        let (references, consumed) = GuideManagerReferences::decode(
            self.format_version,
            &self.serialized_object_graph[body_start..],
        )?;
        let body_end = body_start
            .checked_add(consumed)
            .ok_or(CrpError::ArithmeticOverflow("GuideManager body end"))?;
        Ok((references, body_end))
    }

    /// Decodes the first queued manager object when it is `GameAreaManager+Data`.
    ///
    /// Returns the typed object and the number of bytes consumed from the opaque
    /// graph. Remaining manager objects stay available through
    /// [`serialized_object_graph`](Self::serialized_object_graph).
    pub fn decode_game_area_prefix(&self) -> Result<(GameAreaManagerData, usize), CrpError> {
        let Some(Some(first)) = self
            .manager_objects
            .as_ref()
            .and_then(|objects| objects.first())
        else {
            return Err(invalid(0, "manager object array has no first object"));
        };
        if !first.type_name.starts_with("GameAreaManager+Data,") {
            return Err(invalid(
                0,
                format!(
                    "first manager object is {}, not GameAreaManager+Data",
                    first.type_name
                ),
            ));
        }
        let mut reader = Reader::new(&self.serialized_object_graph);
        let mut strings = vec!["SimulationManager".to_owned()];
        expect_profile_tag(&mut reader, &mut strings, "GameAreaManager")?;
        let (area, body_length) =
            GameAreaManagerData::decode_prefix(self.format_version, reader.remaining_bytes())?;
        reader.read_bytes(body_length)?;
        expect_profile_tag(&mut reader, &mut strings, "GameAreaManager")?;
        Ok((area, reader.offset()))
    }

    /// Decodes the queued `TerrainManager+Data` body after the game-area body.
    ///
    /// The returned byte length includes the preceding game-area object body
    /// and the terrain body. Other queued manager objects remain opaque.
    pub fn decode_terrain_prefix(&self) -> Result<(TerrainManagerData, usize), CrpError> {
        let Some(Some(terrain)) = self
            .manager_objects
            .as_ref()
            .and_then(|objects| objects.get(1))
        else {
            return Err(invalid(0, "manager object array has no second object"));
        };
        if !terrain.type_name.starts_with("TerrainManager+Data,") {
            return Err(invalid(
                0,
                format!(
                    "second manager object is {}, not TerrainManager+Data",
                    terrain.type_name
                ),
            ));
        }
        let mut reader = Reader::new(&self.serialized_object_graph);
        let mut strings = vec!["SimulationManager".to_owned()];
        expect_profile_tag(&mut reader, &mut strings, "GameAreaManager")?;
        let (_, area_length) =
            GameAreaManagerData::decode_prefix(self.format_version, reader.remaining_bytes())?;
        reader.read_bytes(area_length)?;
        expect_profile_tag(&mut reader, &mut strings, "GameAreaManager")?;
        expect_profile_tag(&mut reader, &mut strings, "TerrainManager")?;
        let (terrain_data, terrain_length) =
            TerrainManagerData::decode_prefix(self.format_version, reader.remaining_bytes())?;
        reader.read_bytes(terrain_length)?;
        expect_profile_tag(&mut reader, &mut strings, "TerrainManager")?;
        Ok((terrain_data, reader.offset()))
    }

    /// Decodes the queued `WaterSimulation+Data` body after GameArea and Terrain.
    ///
    /// The returned length includes all three bodies and their profiler tags.
    /// Water source records before inner version 184 are unsupported. The
    /// original graph, including any later manager objects, remains unchanged.
    pub fn decode_water_prefix(&self) -> Result<(WaterSimulationData, usize), CrpError> {
        let Some(Some(water)) = self
            .manager_objects
            .as_ref()
            .and_then(|objects| objects.get(2))
        else {
            return Err(invalid(0, "manager object array has no third object"));
        };
        if !water.type_name.starts_with("WaterSimulation+Data,") {
            return Err(invalid(
                0,
                "third manager object is not WaterSimulation+Data",
            ));
        }
        let (_, terrain_end) = self.decode_terrain_prefix()?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(terrain_end)?;
        let mut strings = vec![
            "SimulationManager".to_owned(),
            "GameAreaManager".to_owned(),
            "TerrainManager".to_owned(),
        ];
        expect_profile_tag(&mut reader, &mut strings, "WaterSimulation")?;
        let (water_data, body_length) =
            WaterSimulationData::decode_prefix(self.format_version, reader.remaining_bytes())?;
        reader.read_bytes(body_length)?;
        expect_profile_tag(&mut reader, &mut strings, "WaterSimulation")?;
        let end = reader.offset();
        if self
            .manager_objects
            .as_ref()
            .and_then(|objects| objects.get(3))
            .and_then(Option::as_ref)
            .is_some_and(|next| next.type_name.starts_with("InstanceManager+Data,"))
        {
            expect_profile_tag(&mut reader, &mut strings, "InstanceManager")?;
        }
        Ok((water_data, end))
    }

    /// Decodes the fourth queued manager body and returns the graph offset
    /// following its closing profiler tag. Later manager bytes remain opaque.
    pub fn decode_instance_prefix(&self) -> Result<(InstanceManagerData, usize), CrpError> {
        let Some(Some(instance)) = self
            .manager_objects
            .as_ref()
            .and_then(|objects| objects.get(3))
        else {
            return Err(invalid(0, "manager object array has no fourth object"));
        };
        if !instance.type_name.starts_with("InstanceManager+Data,") {
            return Err(invalid(
                0,
                "fourth manager object is not InstanceManager+Data",
            ));
        }
        let (_, water_end) = self.decode_water_prefix()?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(water_end)?;
        let mut strings = vec![
            "SimulationManager".to_owned(),
            "GameAreaManager".to_owned(),
            "TerrainManager".to_owned(),
            "WaterSimulation".to_owned(),
        ];
        expect_profile_tag(&mut reader, &mut strings, "InstanceManager")?;
        let (data, body_length) =
            InstanceManagerData::decode_prefix(self.format_version, reader.remaining_bytes())?;
        reader.read_bytes(body_length)?;
        expect_profile_tag(&mut reader, &mut strings, "InstanceManager")?;
        let end = reader.offset();
        if self
            .manager_objects
            .as_ref()
            .and_then(|objects| objects.get(4))
            .and_then(Option::as_ref)
            .is_some_and(|next| next.type_name.starts_with("PathManager+Data,"))
        {
            expect_profile_tag(&mut reader, &mut strings, "PathManager")?;
        }
        Ok((data, end))
    }

    /// Decodes the fifth queued manager body and returns the graph offset
    /// following its closing profiler tag. Requires inner versions 164–121010.
    pub fn decode_path_prefix(&self) -> Result<(PathManagerData, usize), CrpError> {
        let Some(Some(path)) = self
            .manager_objects
            .as_ref()
            .and_then(|objects| objects.get(4))
        else {
            return Err(invalid(0, "manager object array has no fifth object"));
        };
        if !path.type_name.starts_with("PathManager+Data,") {
            return Err(invalid(0, "fifth manager object is not PathManager+Data"));
        }
        let (_, instance_end) = self.decode_instance_prefix()?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(instance_end)?;
        let mut strings = vec![
            "SimulationManager".to_owned(),
            "GameAreaManager".to_owned(),
            "TerrainManager".to_owned(),
            "WaterSimulation".to_owned(),
            "InstanceManager".to_owned(),
        ];
        expect_profile_tag(&mut reader, &mut strings, "PathManager")?;
        let (data, body_length) =
            PathManagerData::decode_prefix(self.format_version, reader.remaining_bytes())?;
        reader.read_bytes(body_length)?;
        expect_profile_tag(&mut reader, &mut strings, "PathManager")?;
        let end = reader.offset();
        if self
            .manager_objects
            .as_ref()
            .and_then(|objects| objects.get(5))
            .and_then(Option::as_ref)
            .is_some_and(|next| next.type_name.starts_with("NetManager+Data,"))
        {
            expect_profile_tag(&mut reader, &mut strings, "NetManager")?;
        }
        Ok((data, end))
    }

    /// Decodes NetManager's lane flags and active next-lane links after the
    /// PathManager body. The returned offset is inside NetManager's body;
    /// later fields can be decoded by other prefix methods.
    pub fn decode_net_lane_prefix(&self) -> Result<(NetLanePrefix, usize), CrpError> {
        let Some(Some(net)) = self
            .manager_objects
            .as_ref()
            .and_then(|objects| objects.get(5))
        else {
            return Err(invalid(0, "manager object array has no sixth object"));
        };
        if !net.type_name.starts_with("NetManager+Data,") {
            return Err(invalid(0, "sixth manager object is not NetManager+Data"));
        }
        let (_, path_end) = self.decode_path_prefix()?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(path_end)?;
        let mut strings = vec![
            "SimulationManager".to_owned(),
            "GameAreaManager".to_owned(),
            "TerrainManager".to_owned(),
            "WaterSimulation".to_owned(),
            "InstanceManager".to_owned(),
            "PathManager".to_owned(),
        ];
        expect_profile_tag(&mut reader, &mut strings, "NetManager")?;
        let (data, body_length) =
            NetLanePrefix::decode_prefix(self.format_version, reader.remaining_bytes())?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes NetManager's node flags after its lane section. The returned
    /// graph offset is before node problem arrays and other opaque fields.
    pub fn decode_net_node_flags_prefix(&self) -> Result<(NetNodeFlagsPrefix, usize), CrpError> {
        let (_, lane_end) = self.decode_net_lane_prefix()?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(lane_end)?;
        let (data, body_length) =
            NetNodeFlagsPrefix::decode_prefix(self.format_version, reader.remaining_bytes())?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes node problem bits after node flags. The save-app version from
    /// the first type-52 root selects a legacy encoding for versions 221–114999.
    /// The returned offset is before later opaque NetManager fields.
    pub fn decode_net_node_problems_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetNodeProblemsPrefix, usize), CrpError> {
        let (node_flags, node_end) = self.decode_net_node_flags_prefix()?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(node_end)?;
        let (data, body_length) = NetNodeProblemsPrefix::decode_prefix(
            self.format_version,
            save_app_version,
            &node_flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes sparse node wait times and counters after the problem array.
    /// The returned graph offset is before individual opaque node records.
    pub fn decode_net_node_counters_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetNodeCountersPrefix, usize), CrpError> {
        let (_, problem_end) = self.decode_net_node_problems_prefix(save_app_version)?;
        let (node_flags, _) = self.decode_net_node_flags_prefix()?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(problem_end)?;
        let (data, body_length) = NetNodeCountersPrefix::decode_prefix(
            self.format_version,
            &node_flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes active node records after the sparse counter arrays.
    /// The returned graph offset is before segment flags.
    pub fn decode_net_node_records_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetNodeRecordsPrefix, usize), CrpError> {
        let (_, counter_end) = self.decode_net_node_counters_prefix(save_app_version)?;
        let (node_flags, _) = self.decode_net_node_flags_prefix()?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(counter_end)?;
        let (data, body_length) = NetNodeRecordsPrefix::decode_prefix(
            self.format_version,
            &node_flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes the versioned segment-flag arrays after node records.
    /// The returned graph offset is before segment problems.
    pub fn decode_net_segment_flags_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetSegmentFlagsPrefix, usize), CrpError> {
        let (_, records_end) = self.decode_net_node_records_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(records_end)?;
        let (data, body_length) =
            NetSegmentFlagsPrefix::decode_prefix(self.format_version, reader.remaining_bytes())?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes segment problems after the versioned flag arrays.
    /// The returned graph offset is before later segment fields.
    pub fn decode_net_segment_problems_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetSegmentProblemsPrefix, usize), CrpError> {
        let (segment_flags, flags_end) = self.decode_net_segment_flags_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(flags_end)?;
        let (data, body_length) = NetSegmentProblemsPrefix::decode_prefix(
            self.format_version,
            &segment_flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes sparse segment traffic-light states after segment problems.
    /// The returned graph offset is before fire coverage.
    pub fn decode_net_segment_traffic_lights_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetSegmentTrafficLightsPrefix, usize), CrpError> {
        let (_, problems_end) = self.decode_net_segment_problems_prefix(save_app_version)?;
        let (segment_flags, _) = self.decode_net_segment_flags_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(problems_end)?;
        let (data, body_length) = NetSegmentTrafficLightsPrefix::decode_prefix(
            self.format_version,
            &segment_flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes sparse segment fire coverage and wetness after traffic lights.
    /// The returned graph offset is before the condition array.
    pub fn decode_net_segment_coverage_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetSegmentCoveragePrefix, usize), CrpError> {
        let (_, lights_end) = self.decode_net_segment_traffic_lights_prefix(save_app_version)?;
        let (segment_flags, _) = self.decode_net_segment_flags_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(lights_end)?;
        let (data, body_length) = NetSegmentCoveragePrefix::decode_prefix(
            self.format_version,
            &segment_flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes segment condition, noise, and attractiveness after wetness.
    /// The returned graph offset is before the individual segment records.
    pub fn decode_net_segment_environment_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetSegmentEnvironmentPrefix, usize), CrpError> {
        let (_, coverage_end) = self.decode_net_segment_coverage_prefix(save_app_version)?;
        let (segment_flags, _) = self.decode_net_segment_flags_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(coverage_end)?;
        let (data, body_length) = NetSegmentEnvironmentPrefix::decode_prefix(
            self.format_version,
            &segment_flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes individual active-segment records after the environment arrays.
    /// The returned graph offset is before the guide-count tables.
    pub fn decode_net_segment_records_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetSegmentRecordsPrefix, usize), CrpError> {
        let (_, environment_end) = self.decode_net_segment_environment_prefix(save_app_version)?;
        let (segment_flags, _) = self.decode_net_segment_flags_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(environment_end)?;
        let (data, body_length) = NetSegmentRecordsPrefix::decode_prefix(
            self.format_version,
            &segment_flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes the two guide-table counts after individual segment records.
    /// The returned graph offset is before the prefab-reference tables.
    pub fn decode_net_guide_counts_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetGuideCountsPrefix, usize), CrpError> {
        let (_, records_end) = self.decode_net_segment_records_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(records_end)?;
        let (data, body_length) =
            NetGuideCountsPrefix::decode_prefix(self.format_version, reader.remaining_bytes())?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes the network-info prefab-index and name table after guide counts.
    /// The returned graph offset is before the tree-prefab table.
    pub fn decode_net_info_prefab_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetInfoPrefabPrefix, usize), CrpError> {
        let (guide_counts, counts_end) = self.decode_net_guide_counts_prefix(save_app_version)?;
        let (node_flags, _) = self.decode_net_node_flags_prefix()?;
        let (segment_flags, _) = self.decode_net_segment_flags_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(counts_end)?;
        let (data, body_length) = NetInfoPrefabPrefix::decode_prefix(
            self.format_version,
            &node_flags,
            &segment_flags,
            &guide_counts,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes the tree-prefab index and name table after network-info prefabs.
    /// The returned graph offset is before optional fence-prefab tables.
    pub fn decode_net_tree_prefab_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetTreePrefabPrefix, usize), CrpError> {
        let (_, info_end) = self.decode_net_info_prefab_prefix(save_app_version)?;
        let (records, _) = self.decode_net_segment_records_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(info_end)?;
        let (data, body_length) = NetTreePrefabPrefix::decode_prefix(
            self.format_version,
            &records,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes optional left and right fence-prefab tables after tree prefabs.
    /// The returned graph offset is before guide and milestone object references.
    pub fn decode_net_fence_prefab_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetFencePrefabPrefix, usize), CrpError> {
        let (_, tree_end) = self.decode_net_tree_prefab_prefix(save_app_version)?;
        let (records, _) = self.decode_net_segment_records_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(tree_end)?;
        let (data, body_length) = NetFencePrefabPrefix::decode_prefix(
            self.format_version,
            &records,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes the final NetManager fields and profiler end tag.
    /// The returned graph offset is before the next queued manager body.
    pub fn decode_net_manager_tail_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(NetManagerTailPrefix, usize), CrpError> {
        let (_, fence_end) = self.decode_net_fence_prefab_prefix(save_app_version)?;
        let (guide_counts, _) = self.decode_net_guide_counts_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(fence_end)?;
        let (data, body_length) = NetManagerTailPrefix::decode_prefix(
            self.format_version,
            &guide_counts,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(body_length)?;
        Ok((data, reader.offset()))
    }

    /// Decodes ZoneManager after locating its profiler tag in the queued graph.
    ///
    /// The type array must contain `ZoneManager+Data` at slot 13 and
    /// `PropManager+Data` at slot 14. Returns the graph offset after
    /// ZoneManager's closing profiler reference and before PropManager's
    /// opening profiler tag.
    pub fn decode_zone_manager_prefix(&self) -> Result<(ZoneManagerDataPrefix, usize), CrpError> {
        expect_manager_type(self.manager_objects.as_deref(), 13, "ZoneManager+Data")?;
        expect_manager_type(self.manager_objects.as_deref(), 14, "PropManager+Data")?;

        let graph = &self.serialized_object_graph;
        let zone_tag = new_profiler_tag_bytes("ZoneManager")?;
        let zone_body_start = find_unique_bytes(graph, &zone_tag, "ZoneManager profiler tag")?
            .checked_add(zone_tag.len())
            .ok_or(CrpError::ArithmeticOverflow(
                "ZoneManager profiler tag offset",
            ))?;
        let (zone, body_len) =
            ZoneManagerDataPrefix::decode_prefix(self.format_version, &graph[zone_body_start..])?;
        let end_tag_start = zone_body_start
            .checked_add(body_len)
            .ok_or(CrpError::ArithmeticOverflow("ZoneManager body end"))?;
        let prop_tag = new_profiler_tag_bytes("PropManager")?;
        let prop_tag_start = end_tag_start
            .checked_add(LOADING_PROFILER_END_TAG_BYTES)
            .ok_or(CrpError::ArithmeticOverflow("ZoneManager end tag"))?;
        if graph
            .get(end_tag_start)
            .is_none_or(|marker| *marker >= 0xfe)
            || !graph
                .get(prop_tag_start..)
                .is_some_and(|tail| tail.starts_with(&prop_tag))
        {
            return Err(invalid(
                end_tag_start,
                "ZoneManager fields are not followed by its closing tag and PropManager",
            ));
        }
        Ok((zone, prop_tag_start))
    }

    /// Decodes PropManager after the ZoneManager boundary in the manager graph.
    ///
    /// The type array must contain `PropManager+Data` at slot 14 and
    /// `TreeManager+Data` at slot 15. Returns the graph offset after
    /// PropManager's closing profiler reference and before TreeManager.
    pub fn decode_prop_manager_prefix(&self) -> Result<(PropManagerDataPrefix, usize), CrpError> {
        expect_manager_type(self.manager_objects.as_deref(), 14, "PropManager+Data")?;
        expect_manager_type(self.manager_objects.as_deref(), 15, "TreeManager+Data")?;

        let (_, prop_tag_start) = self.decode_zone_manager_prefix()?;
        let graph = &self.serialized_object_graph;
        let prop_tag = new_profiler_tag_bytes("PropManager")?;
        if !graph
            .get(prop_tag_start..)
            .is_some_and(|tail| tail.starts_with(&prop_tag))
        {
            return Err(invalid(
                prop_tag_start,
                "expected PropManager opening profiler tag",
            ));
        }
        let prop_body_start = prop_tag_start
            .checked_add(prop_tag.len())
            .ok_or(CrpError::ArithmeticOverflow("PropManager profiler tag"))?;
        let (props, body_len) =
            PropManagerDataPrefix::decode_prefix(self.format_version, &graph[prop_body_start..])?;
        let end_tag_start = prop_body_start
            .checked_add(body_len)
            .ok_or(CrpError::ArithmeticOverflow("PropManager body end"))?;
        let tree_tag = new_profiler_tag_bytes("TreeManager")?;
        let tree_tag_start = end_tag_start
            .checked_add(LOADING_PROFILER_END_TAG_BYTES)
            .ok_or(CrpError::ArithmeticOverflow("PropManager end tag"))?;
        if graph
            .get(end_tag_start)
            .is_none_or(|marker| *marker >= 0xfe)
            || !graph
                .get(tree_tag_start..)
                .is_some_and(|tail| tail.starts_with(&tree_tag))
        {
            return Err(invalid(
                end_tag_start,
                "PropManager fields are not followed by its closing tag and TreeManager",
            ));
        }
        Ok((props, tree_tag_start))
    }

    /// Decodes TreeManager and locates the following queued manager by type.
    ///
    /// The type array must contain `TreeManager+Data` at slot 15 and a
    /// non-null manager reference at slot 16. Returns the graph offset after
    /// TreeManager's closing profiler reference and before the next manager.
    pub fn decode_tree_manager_prefix(&self) -> Result<(TreeManagerDataPrefix, usize), CrpError> {
        expect_manager_type(self.manager_objects.as_deref(), 15, "TreeManager+Data")?;
        let next_reference = self
            .manager_objects
            .as_deref()
            .and_then(|objects| objects.get(16))
            .and_then(Option::as_ref)
            .ok_or_else(|| invalid(16, "missing manager reference after TreeManager+Data"))?;
        let next_type = next_reference
            .type_name
            .split(',')
            .next()
            .unwrap_or_default()
            .strip_suffix("+Data")
            .ok_or_else(|| invalid(16, "following manager type has no +Data suffix"))?;

        let graph = &self.serialized_object_graph;
        let tree_tag = new_profiler_tag_bytes("TreeManager")?;
        let tree_body_start = find_unique_bytes(graph, &tree_tag, "TreeManager profiler tag")?
            .checked_add(tree_tag.len())
            .ok_or(CrpError::ArithmeticOverflow(
                "TreeManager profiler tag offset",
            ))?;
        let (trees, body_len) =
            TreeManagerDataPrefix::decode_prefix(self.format_version, &graph[tree_body_start..])?;
        let end_tag_start = tree_body_start
            .checked_add(body_len)
            .ok_or(CrpError::ArithmeticOverflow("TreeManager body end"))?;
        let next_tag = new_profiler_tag_bytes(next_type)?;
        let next_tag_start = end_tag_start
            .checked_add(LOADING_PROFILER_END_TAG_BYTES)
            .ok_or(CrpError::ArithmeticOverflow("TreeManager end tag"))?;
        if graph
            .get(end_tag_start)
            .is_none_or(|marker| *marker >= 0xfe)
            || !graph
                .get(next_tag_start..)
                .is_some_and(|tail| tail.starts_with(&next_tag))
        {
            return Err(invalid(
                end_tag_start,
                format!("TreeManager fields are not followed by its closing tag and {next_type}"),
            ));
        }
        Ok((trees, next_tag_start))
    }

    /// Decodes the known DistrictManager prefix for the requested inner version.
    ///
    /// The type array must contain `DistrictManager+Data` at slot 20. The
    /// decoded prefix includes district and park policies, active district
    /// records, and their tourism, hotel, style, library-visitor, and airline
    /// fields. It returns the graph offset immediately after the active district
    /// records and before the park records.
    pub fn decode_district_manager_flags_prefix(
        &self,
    ) -> Result<(DistrictManagerFlagsPrefix, usize), CrpError> {
        expect_manager_type(self.manager_objects.as_deref(), 20, "DistrictManager+Data")?;
        let graph = &self.serialized_object_graph;
        let tag = new_profiler_tag_bytes("DistrictManager")?;
        let body_start = find_unique_bytes(graph, &tag, "DistrictManager profiler tag")?
            .checked_add(tag.len())
            .ok_or(CrpError::ArithmeticOverflow(
                "DistrictManager profiler tag offset",
            ))?;
        let (districts, body_len) =
            DistrictManagerFlagsPrefix::decode_prefix(self.format_version, &graph[body_start..])?;
        let end = body_start
            .checked_add(body_len)
            .ok_or(CrpError::ArithmeticOverflow("DistrictManager prefix end"))?;
        Ok((districts, end))
    }

    /// Decodes the district and park grids after the park-record sequence.
    ///
    /// Empty park buffers need no record decoding from inner version 164 onward;
    /// active parks require the supported type-0–15 decoder's version range.
    /// The returned graph offset points after the grids, before the
    /// following versioned manager objects. When a following manager reference
    /// is present, the ten- and thirteen-report layouts are selected by
    /// validating the grids, DistrictManager tail, and next new profiler tag.
    /// Missing or ambiguous boundaries return an error. Unchanged grid bytes
    /// are retained.
    pub fn decode_district_manager_grids(
        &self,
    ) -> Result<(DistrictManagerGridData, usize), CrpError> {
        let (prefix, park_start) = self.decode_district_manager_flags_prefix()?;
        let (_, park_end) = self.decode_district_park_records_layout(&prefix, park_start)?;
        let graph = &self.serialized_object_graph;
        let (grids, grid_len) =
            DistrictManagerGridData::decode(self.format_version, &graph[park_end..])?;
        let grid_end = park_end
            .checked_add(grid_len)
            .ok_or(CrpError::ArithmeticOverflow("DistrictManager grid end"))?;
        Ok((grids, grid_end))
    }

    /// Decodes active park records using the enclosing graph when available.
    ///
    /// For the ten- and thirteen-report variants, the full manager graph is
    /// used to validate the following grids, tail, and manager boundary.
    pub fn decode_district_manager_park_records(
        &self,
    ) -> Result<Vec<DistrictParkRecord>, CrpError> {
        let (prefix, park_start) = self.decode_district_manager_flags_prefix()?;
        self.decode_district_park_records_layout(&prefix, park_start)
            .map(|(records, _)| records)
    }

    /// Replaces one park record while preserving its unmodeled bytes.
    ///
    /// The editor must originate from this manager's current park record. The
    /// replacement may change length; the resulting park, grid, tail, and next
    /// manager boundaries are validated before the edit is accepted.
    pub fn replace_district_park_record(
        &mut self,
        edited: &DistrictParkEditor,
    ) -> Result<(), CrpError> {
        let (prefix, park_start) = self.decode_district_manager_flags_prefix()?;
        let (records, park_end) = self.decode_district_park_records_layout(&prefix, park_start)?;
        let Some(record_index) = records
            .iter()
            .position(|record| record.park_index() == edited.park_index())
        else {
            return Err(invalid(
                park_start,
                format!("park {} is not active", edited.park_index()),
            ));
        };
        let mut relative_start = 0_usize;
        for record in records.iter().take(record_index) {
            relative_start = relative_start
                .checked_add(record.encode().len())
                .ok_or(CrpError::ArithmeticOverflow("park record offset"))?;
        }
        let start = park_start
            .checked_add(relative_start)
            .ok_or(CrpError::ArithmeticOverflow("park record start"))?;
        let original = records[record_index].encode();
        if original != edited.original_bytes() {
            return Err(invalid(
                start,
                "park editor does not match this manager graph",
            ));
        }
        let replacement = edited.encode();
        let end = start
            .checked_add(original.len())
            .ok_or(CrpError::ArithmeticOverflow("park record end"))?;
        if end > park_end {
            return Err(invalid(
                start,
                "park record extends past its validated boundary",
            ));
        }
        if replacement.len() == original.len() {
            self.serialized_object_graph.splice(start..end, replacement);
            return Ok(());
        }
        let mut candidate_graph = self.serialized_object_graph.clone();
        candidate_graph.splice(start..end, replacement);
        let original_graph = std::mem::replace(&mut self.serialized_object_graph, candidate_graph);
        if let Err(error) = self.decode_district_manager_park_records() {
            self.serialized_object_graph = original_graph;
            return Err(invalid(
                start,
                format!("edited park record failed manager-boundary validation: {error}"),
            ));
        }
        Ok(())
    }

    fn decode_district_park_records_layout(
        &self,
        prefix: &DistrictManagerFlagsPrefix,
        park_start: usize,
    ) -> Result<(Vec<DistrictParkRecord>, usize), CrpError> {
        let graph = &self.serialized_object_graph;
        let next_manager = self
            .manager_objects
            .as_deref()
            .and_then(|objects| objects.get(21))
            .and_then(Option::as_ref);
        let Some(next_manager) = next_manager else {
            let (records, park_len) = prefix.decode_park_records(&graph[park_start..])?;
            let park_end = park_start
                .checked_add(park_len)
                .ok_or(CrpError::ArithmeticOverflow("DistrictManager park end"))?;
            return Ok((records, park_end));
        };
        let next_name = next_manager
            .type_name
            .split_once("+Data,")
            .map(|(name, _)| name)
            .ok_or_else(|| invalid(park_start, "invalid next manager type name"))?;
        let next_tag = new_profiler_tag_bytes(next_name)?;
        let report_layouts: &[bool] =
            if self.format_version >= 112_020 && prefix.park_flags()[0] != 0 {
                &[false, true]
            } else {
                &[false]
            };
        let mut selected = None;
        for &use_ten_reports in report_layouts {
            let candidate = (|| {
                let (records, park_len) = prefix.decode_park_records_with_ten_reports(
                    &graph[park_start..],
                    15,
                    use_ten_reports,
                )?;
                let park_end = park_start
                    .checked_add(park_len)
                    .ok_or(CrpError::ArithmeticOverflow("DistrictManager park end"))?;
                let (_, grid_len) =
                    DistrictManagerGridData::decode(self.format_version, &graph[park_end..])?;
                let grid_end = park_end
                    .checked_add(grid_len)
                    .ok_or(CrpError::ArithmeticOverflow("DistrictManager grid end"))?;
                let (_, tail_len) =
                    DistrictManagerTailData::decode(self.format_version, &graph[grid_end..])?;
                let next_start = grid_end
                    .checked_add(tail_len)
                    .ok_or(CrpError::ArithmeticOverflow("next manager tag start"))?;
                if !graph[next_start..].starts_with(&next_tag) {
                    return Err(invalid(
                        next_start,
                        format!("expected {next_name} tag after DistrictManager"),
                    ));
                }
                Ok((park_end, records))
            })();
            if let Ok((park_end, records)) = candidate {
                if selected
                    .as_ref()
                    .is_some_and(|(start, _)| *start != park_end)
                {
                    return Err(invalid(
                        park_start,
                        "ambiguous DistrictManager report layout",
                    ));
                }
                selected = Some((park_end, records));
            }
        }
        selected
            .map(|(park_end, records)| (records, park_end))
            .ok_or_else(|| {
                invalid(
                    park_start,
                    "no DistrictManager report layout matches grids, tail, and next manager tag",
                )
            })
    }

    /// Decodes DistrictManager object headers and scalars after the grids.
    ///
    /// The returned graph offset follows the manager's profiler end tag.
    /// Queued object bodies remain opaque and available in the original graph.
    pub fn decode_district_manager_tail(
        &self,
    ) -> Result<(DistrictManagerTailData, usize), CrpError> {
        let (_, grid_end) = self.decode_district_manager_grids()?;
        let (tail, tail_len) = DistrictManagerTailData::decode(
            self.format_version,
            &self.serialized_object_graph[grid_end..],
        )?;
        let tail_end = grid_end
            .checked_add(tail_len)
            .ok_or(CrpError::ArithmeticOverflow("DistrictManager tail end"))?;
        Ok((tail, tail_end))
    }

    /// Replaces district and park grid streams in the manager graph.
    ///
    /// The editor must originate from this graph's current grid bytes. Other
    /// manager bytes, including the park records and following objects, remain
    /// unchanged. Modified grids use literal `EncodedArray.Byte` blocks.
    pub fn replace_district_manager_grids(
        &mut self,
        edited: &crate::DistrictManagerGridEditor,
    ) -> Result<(), CrpError> {
        let (original, grid_end) = self.decode_district_manager_grids()?;
        if edited.original_bytes() != original.original_bytes() {
            return Err(invalid(
                0,
                "edited district grids do not match this manager graph",
            ));
        }
        let grid_start = grid_end
            .checked_sub(original.original_bytes().len())
            .ok_or(CrpError::ArithmeticOverflow("DistrictManager grid start"))?;
        self.serialized_object_graph
            .splice(grid_start..grid_end, edited.encode());
        Ok(())
    }

    /// Decodes the first two BuildingManager flag arrays after the NetManager body.
    /// The returned graph offset is before the guide and milestone counts.
    pub fn decode_building_flags_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingFlagsPrefix, usize), CrpError> {
        let (_, net_end) = self.decode_net_manager_tail_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(net_end)?;
        let mut strings = manager_profiler_seed();
        expect_profile_tag(&mut reader, &mut strings, "BuildingManager")?;
        let (data, used) =
            BuildingFlagsPrefix::decode_prefix(self.format_version, reader.remaining_bytes())?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes BuildingManager flags, counts, and the prefab index/name table.
    /// The returned graph offset is before queued guide and milestone objects.
    pub fn decode_building_prefabs_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingPrefabPrefix, usize), CrpError> {
        let (flags, flags_end) = self.decode_building_flags_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(flags_end)?;
        let guide_count = reader.read_u16_be()?;
        let milestone_count = reader.read_u16_be()?;
        let (data, used) = BuildingPrefabPrefix::decode_prefix(
            self.format_version,
            &flags,
            guide_count,
            milestone_count,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes BuildingManager prefab data and versioned object headers.
    /// The returned graph offset is before later per-building serialized arrays.
    pub fn decode_building_object_headers_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingManagerObjectHeadersPrefix, usize), CrpError> {
        let (prefabs, prefab_end) = self.decode_building_prefabs_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(prefab_end)?;
        let (data, used) = BuildingManagerObjectHeadersPrefix::decode_prefix(
            self.format_version,
            &prefabs,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes per-building problem words following the manager object headers.
    /// Pass the saved application version for the legacy version 221 gate.
    pub fn decode_building_problems_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingProblemPrefix, usize), CrpError> {
        let (flags, _) = self.decode_building_flags_prefix(save_app_version)?;
        let (_, object_end) = self.decode_building_object_headers_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(object_end)?;
        let (data, used) = BuildingProblemPrefix::decode_prefix(
            self.format_version,
            save_app_version,
            &flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Replaces opaque per-building problem words and preserves the following fields.
    pub fn replace_building_problems(
        &mut self,
        edited: &BuildingProblemEditor,
        save_app_version: Option<u32>,
    ) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                0,
                "edited BuildingManager problem version does not match this stream",
            ));
        }
        let (original, graph_end) = self.decode_building_problems_prefix(save_app_version)?;
        if edited.problem_count() != original.problems().len() {
            return Err(invalid(
                0,
                "edited BuildingManager problem count does not match this stream",
            ));
        }
        let body_start =
            graph_end
                .checked_sub(original.encode().len())
                .ok_or(CrpError::ArithmeticOverflow(
                    "BuildingManager problem body start",
                ))?;
        let replacement = edited.encode(save_app_version)?;
        if graph_end > self.serialized_object_graph.len() {
            return Err(invalid(
                body_start,
                "BuildingManager problem words extend past manager graph",
            ));
        }
        self.serialized_object_graph
            .splice(body_start..graph_end, replacement);
        Ok(())
    }

    /// Decodes per-building network, utility, and byte fields after problem state.
    /// The returned graph offset is before the next BuildingManager arrays.
    pub fn decode_building_buffer_fields_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingBufferFieldsPrefix, usize), CrpError> {
        let (flags, _) = self.decode_building_flags_prefix(save_app_version)?;
        let (_, problem_end) = self.decode_building_problems_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(problem_end)?;
        let (data, used) = BuildingBufferFieldsPrefix::decode_prefix(
            self.format_version,
            &flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes versioned per-building byte state after network and utility buffers.
    /// The returned graph offset is before the next BuildingManager arrays.
    pub fn decode_building_byte_state_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingByteStatePrefix, usize), CrpError> {
        let (flags, _) = self.decode_building_flags_prefix(save_app_version)?;
        let (_, buffer_end) = self.decode_building_buffer_fields_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(buffer_end)?;
        let (data, used) = BuildingByteStatePrefix::decode_prefix(
            self.format_version,
            &flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes timer, level-up, width, and length arrays after BuildingManager byte state.
    ///
    /// Returns the decoded prefix and its end offset in the serialized object
    /// graph. The optional save-app version is required by earlier versioned
    /// BuildingManager problem encodings.
    pub fn decode_building_timer_geometry_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingTimerGeometryPrefix, usize), CrpError> {
        let (_, byte_state_end) = self.decode_building_byte_state_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(byte_state_end)?;
        let (data, used) = BuildingTimerGeometryPrefix::decode_prefix(
            self.format_version,
            &self.decode_building_flags_prefix(save_app_version)?.0,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes education, population, hazard, and level arrays after geometry.
    ///
    /// Returns the decoded prefix and its end offset in the serialized object
    /// graph. The original encoded bytes are retained for exact writeback.
    pub fn decode_building_population_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingPopulationPrefix, usize), CrpError> {
        let (_, geometry_end) = self.decode_building_timer_geometry_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(geometry_end)?;
        let (data, used) = BuildingPopulationPrefix::decode_prefix(
            self.format_version,
            &self.decode_building_flags_prefix(save_app_version)?.0,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes traffic, routing, cash, room, and employment arrays after population fields.
    pub fn decode_building_economic_fields_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingEconomicFieldsPrefix, usize), CrpError> {
        let (_, population_end) = self.decode_building_population_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(population_end)?;
        let (data, used) = BuildingEconomicFieldsPrefix::decode_prefix(
            self.format_version,
            &self.decode_building_flags_prefix(save_app_version)?.0,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes versioned service-vehicle selection tables after economic fields.
    ///
    /// The shared-string table is seeded from the preceding root and manager
    /// profiler tags, then extended by newly encountered strings in these tables.
    pub fn decode_building_service_selection_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingServiceSelectionPrefix, usize), CrpError> {
        let (_, economic_end) = self.decode_building_economic_fields_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(economic_end)?;
        let (data, used) = BuildingServiceSelectionPrefix::decode_prefix(
            self.format_version,
            reader.remaining_bytes(),
            self.building_manager_shared_strings(save_app_version)?,
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes direct per-active-building records after service-selection tables.
    /// The returned graph offset is before later BuildingManager arrays and queued objects.
    pub fn decode_building_instance_records_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingInstanceRecordsPrefix, usize), CrpError> {
        let (_, selections_end) =
            self.decode_building_service_selection_prefix(save_app_version)?;
        let (flags, _) = self.decode_building_flags_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(selections_end)?;
        let (data, used) = BuildingInstanceRecordsPrefix::decode_prefix(
            self.format_version,
            &flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes the fixed monument-effect tables following direct building records.
    pub fn decode_building_monument_effects_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingMonumentEffectsPrefix, usize), CrpError> {
        let (_, records_end) = self.decode_building_instance_records_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(records_end)?;
        let (data, used) = BuildingMonumentEffectsPrefix::decode_prefix(
            self.format_version,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes the fixed faculty-bonus tables after monument effects.
    pub fn decode_building_faculty_bonuses_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingFacultyBonusesPrefix, usize), CrpError> {
        let (_, monument_end) = self.decode_building_monument_effects_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(monument_end)?;
        let (data, used) = BuildingFacultyBonusesPrefix::decode_prefix(
            self.format_version,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes race-team-unassigned bytes after faculty bonuses.
    /// Queued race-team reference bodies begin after this prefix and remain opaque.
    pub fn decode_building_race_team_unassigned_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingRaceTeamUnassignedPrefix, usize), CrpError> {
        let (_, faculty_end) = self.decode_building_faculty_bonuses_prefix(save_app_version)?;
        let (flags, _) = self.decode_building_flags_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(faculty_end)?;
        let (data, used) = BuildingRaceTeamUnassignedPrefix::decode_prefix(
            self.format_version,
            &flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    /// Decodes the race-team prefab-reference scope after its unassigned flags.
    /// The returned boundary includes the encoded indices and prefab-name table.
    pub fn decode_building_race_team_references_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingRaceTeamReferencesPrefix, usize), CrpError> {
        let (unassigned, unassigned_end) =
            self.decode_building_race_team_unassigned_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(unassigned_end)?;
        let (data, used) = BuildingRaceTeamReferencesPrefix::decode_after_unassigned(
            self.format_version,
            &unassigned,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
    }

    fn building_manager_shared_strings(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<Vec<Option<String>>, CrpError> {
        let (_, net_end) = self.decode_net_manager_tail_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(net_end)?;
        let mut strings = manager_profiler_seed();
        expect_profile_tag(&mut reader, &mut strings, "BuildingManager")?;
        Ok(strings.into_iter().map(Some).collect())
    }

    /// Replaces timer and building-dimension arrays, preserving the rest of the graph.
    ///
    /// The editor must come from this stream's BuildingManager body. Edited
    /// arrays use canonical literal controls; validation failures leave the
    /// serialized object graph unchanged.
    pub fn replace_building_timer_geometry(
        &mut self,
        edited: &BuildingTimerGeometryEditor,
        save_app_version: Option<u32>,
    ) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                0,
                "edited BuildingManager timer version does not match this stream",
            ));
        }
        let (original, graph_end) = self.decode_building_timer_geometry_prefix(save_app_version)?;
        if edited.active_building_count() != original.active_building_count() {
            return Err(invalid(
                0,
                "edited BuildingManager active-building count does not match this stream",
            ));
        }
        let body_start =
            graph_end
                .checked_sub(original.encoded_len())
                .ok_or(CrpError::ArithmeticOverflow(
                    "BuildingManager timer body start",
                ))?;
        let replacement = edited.encode()?;
        if graph_end > self.serialized_object_graph.len() {
            return Err(invalid(
                body_start,
                "BuildingManager timer body extends past manager graph",
            ));
        }
        self.serialized_object_graph
            .splice(body_start..graph_end, replacement);
        Ok(())
    }

    /// Replaces only the population-related arrays, preserving all other graph bytes.
    pub fn replace_building_population(
        &mut self,
        edited: &BuildingPopulationEditor,
        save_app_version: Option<u32>,
    ) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                0,
                "edited BuildingManager population version does not match this stream",
            ));
        }
        let (original, graph_end) = self.decode_building_population_prefix(save_app_version)?;
        if edited.active_building_count() != original.active_building_count() {
            return Err(invalid(
                0,
                "edited BuildingManager active-building count does not match this stream",
            ));
        }
        let body_start =
            graph_end
                .checked_sub(original.encode().len())
                .ok_or(CrpError::ArithmeticOverflow(
                    "BuildingManager population body start",
                ))?;
        let replacement = edited.encode()?;
        if graph_end > self.serialized_object_graph.len() {
            return Err(invalid(
                body_start,
                "BuildingManager population body extends past manager graph",
            ));
        }
        self.serialized_object_graph
            .splice(body_start..graph_end, replacement);
        Ok(())
    }

    /// Replaces only the traffic, routing, cash, room, and employment arrays.
    pub fn replace_building_economic_fields(
        &mut self,
        edited: &BuildingEconomicFieldsEditor,
        save_app_version: Option<u32>,
    ) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                0,
                "edited BuildingManager economic-fields version does not match this stream",
            ));
        }
        let (original, graph_end) =
            self.decode_building_economic_fields_prefix(save_app_version)?;
        if edited.active_building_count() != original.active_building_count() {
            return Err(invalid(
                0,
                "edited BuildingManager active-building count does not match this stream",
            ));
        }
        let body_start =
            graph_end
                .checked_sub(original.encode().len())
                .ok_or(CrpError::ArithmeticOverflow(
                    "BuildingManager economic fields body start",
                ))?;
        let replacement = edited.encode()?;
        if graph_end > self.serialized_object_graph.len() {
            return Err(invalid(
                body_start,
                "BuildingManager economic fields extend past manager graph",
            ));
        }
        self.serialized_object_graph
            .splice(body_start..graph_end, replacement);
        Ok(())
    }

    /// Replaces direct building records while preserving later BuildingManager bytes.
    pub fn replace_building_instance_records(
        &mut self,
        edited: &BuildingInstanceRecordsEditor,
        save_app_version: Option<u32>,
    ) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                0,
                "edited BuildingManager instance-record version does not match this stream",
            ));
        }
        let (original, graph_end) =
            self.decode_building_instance_records_prefix(save_app_version)?;
        if edited.record_count() != original.records().len() {
            return Err(invalid(
                0,
                "edited BuildingManager instance-record count does not match this stream",
            ));
        }
        let original_bytes = original.encode();
        let body_start =
            graph_end
                .checked_sub(original_bytes.len())
                .ok_or(CrpError::ArithmeticOverflow(
                    "BuildingManager instance-record body start",
                ))?;
        let replacement = edited.encode()?;
        if graph_end > self.serialized_object_graph.len() {
            return Err(invalid(
                body_start,
                "BuildingManager instance records extend past manager graph",
            ));
        }
        self.serialized_object_graph
            .splice(body_start..graph_end, replacement);
        Ok(())
    }

    /// Replaces monument-effect tables while preserving later BuildingManager bytes.
    pub fn replace_building_monument_effects(
        &mut self,
        edited: &BuildingMonumentEffectsEditor,
        save_app_version: Option<u32>,
    ) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                0,
                "edited monument-effect version does not match this stream",
            ));
        }
        let (original, graph_end) =
            self.decode_building_monument_effects_prefix(save_app_version)?;
        if edited.temporary().len() != original.temporary().len()
            || edited.final_values().len() != original.final_values().len()
        {
            return Err(invalid(
                0,
                "edited monument-effect table lengths do not match this stream",
            ));
        }
        let body_start =
            graph_end
                .checked_sub(original.encode().len())
                .ok_or(CrpError::ArithmeticOverflow(
                    "BuildingManager monument-effect start",
                ))?;
        let replacement = edited.encode()?;
        if graph_end > self.serialized_object_graph.len() {
            return Err(invalid(
                body_start,
                "monument-effect tables extend past manager graph",
            ));
        }
        self.serialized_object_graph
            .splice(body_start..graph_end, replacement);
        Ok(())
    }

    /// Replaces faculty-bonus tables while preserving later BuildingManager bytes.
    pub fn replace_building_faculty_bonuses(
        &mut self,
        edited: &BuildingFacultyBonusesEditor,
        save_app_version: Option<u32>,
    ) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                0,
                "edited faculty-bonus version does not match this stream",
            ));
        }
        let (original, graph_end) =
            self.decode_building_faculty_bonuses_prefix(save_app_version)?;
        if edited.temporary().len() != original.temporary().len()
            || edited.final_values().len() != original.final_values().len()
        {
            return Err(invalid(
                0,
                "edited faculty-bonus table lengths do not match this stream",
            ));
        }
        let body_start =
            graph_end
                .checked_sub(original.encode().len())
                .ok_or(CrpError::ArithmeticOverflow(
                    "BuildingManager faculty-bonus start",
                ))?;
        let replacement = edited.encode()?;
        if graph_end > self.serialized_object_graph.len() {
            return Err(invalid(
                body_start,
                "faculty-bonus tables extend past manager graph",
            ));
        }
        self.serialized_object_graph
            .splice(body_start..graph_end, replacement);
        Ok(())
    }

    /// Rewrites unchanged race-team assignment flags while preserving references.
    /// Changes that alter which buildings have a team are rejected; use
    /// `replace_building_race_team` with both editors for paired changes.
    pub fn replace_building_race_team_unassigned(
        &mut self,
        edited: &BuildingRaceTeamUnassignedEditor,
        save_app_version: Option<u32>,
    ) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                0,
                "edited race-team-unassigned version does not match this stream",
            ));
        }
        let (original, graph_end) =
            self.decode_building_race_team_unassigned_prefix(save_app_version)?;
        if edited.unassigned().len() != original.unassigned().len() {
            return Err(invalid(
                0,
                "edited race-team-unassigned count does not match this stream",
            ));
        }
        if edited.unassigned() != original.unassigned() {
            return Err(invalid(
                0,
                "changing race-team assignment flags requires rewriting the following prefab-reference scope",
            ));
        }
        let body_start =
            graph_end
                .checked_sub(original.encode().len())
                .ok_or(CrpError::ArithmeticOverflow(
                    "BuildingManager race-team-unassigned start",
                ))?;
        let replacement = edited.encode()?;
        if graph_end > self.serialized_object_graph.len() {
            return Err(invalid(
                body_start,
                "race-team-unassigned bytes extend past manager graph",
            ));
        }
        self.serialized_object_graph
            .splice(body_start..graph_end, replacement);
        Ok(())
    }

    /// Replaces race-team assignment flags, prefab indices, and names together.
    /// The paired edits keep the encoded index count consistent with assigned
    /// buildings. The remaining manager graph bytes are preserved exactly.
    pub fn replace_building_race_team(
        &mut self,
        unassigned: &BuildingRaceTeamUnassignedEditor,
        references: &BuildingRaceTeamReferencesEditor,
        save_app_version: Option<u32>,
    ) -> Result<(), CrpError> {
        if unassigned.format_version() != self.format_version
            || references.format_version() != self.format_version
        {
            return Err(invalid(
                0,
                "race-team editors do not match this stream's inner version",
            ));
        }
        if self.format_version < 120_002 {
            return Err(invalid(
                0,
                "race-team reference scope requires inner version 120002 or newer",
            ));
        }
        let (original_unassigned, unassigned_end) =
            self.decode_building_race_team_unassigned_prefix(save_app_version)?;
        let (_, reference_end) =
            self.decode_building_race_team_references_prefix(save_app_version)?;
        if unassigned.unassigned().len() != original_unassigned.unassigned().len() {
            return Err(invalid(
                0,
                "race-team assignment count does not match this stream",
            ));
        }
        let expected_indices = unassigned
            .unassigned()
            .iter()
            .filter(|is_unassigned| !**is_unassigned)
            .count();
        if references.indices().len() != expected_indices {
            return Err(invalid(
                0,
                "race-team index count does not match assigned buildings",
            ));
        }
        let start = unassigned_end
            .checked_sub(original_unassigned.encode().len())
            .ok_or(CrpError::ArithmeticOverflow(
                "BuildingManager race-team assignment start",
            ))?;
        if reference_end < unassigned_end || reference_end > self.serialized_object_graph.len() {
            return Err(invalid(
                start,
                "race-team reference scope extends past manager graph",
            ));
        }
        let mut replacement = unassigned.encode()?;
        replacement.extend_from_slice(&references.encode()?);
        self.serialized_object_graph
            .splice(start..reference_end, replacement);
        Ok(())
    }

    /// Replaces only the InstanceManager body; group references and all other
    /// queued manager bodies keep their original bytes and order.
    pub fn replace_instance(&mut self, edited: &InstanceManagerEditor) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                0,
                "edited InstanceManager version does not match this stream",
            ));
        }
        let (original, _) = self.decode_instance_prefix()?;
        if !edited.retains_groups(&original) {
            return Err(invalid(
                0,
                "edited InstanceManager group references differ from this stream",
            ));
        }
        let (_, water_end) = self.decode_water_prefix()?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(water_end)?;
        let mut strings = vec![
            "SimulationManager".to_owned(),
            "GameAreaManager".to_owned(),
            "TerrainManager".to_owned(),
            "WaterSimulation".to_owned(),
        ];
        expect_profile_tag(&mut reader, &mut strings, "InstanceManager")?;
        let body_start = reader.offset();
        let body_end = body_start
            .checked_add(original.encode().len())
            .ok_or(CrpError::ArithmeticOverflow("InstanceManager body end"))?;
        let replacement = edited.encode()?;
        self.serialized_object_graph
            .splice(body_start..body_end, replacement);
        Ok(())
    }

    /// Replaces one already-located 17-byte guide body in the queued graph.
    ///
    /// `graph_offset` is relative to [`serialized_object_graph`](Self::serialized_object_graph),
    /// after the root fields. Create `edited` from that body's decoded bytes.
    /// This method verifies the original bytes still match at the supplied
    /// offset and requires an equal-length replacement. Guide instance bodies
    /// contain no object references, so this preserves queue ordering and
    /// object IDs. It does not locate the body or validate its declared type;
    /// the caller must derive the offset from the preceding queued objects.
    pub fn replace_guide_instance_body_at(
        &mut self,
        graph_offset: usize,
        edited: &GuideInstanceEditor,
    ) -> Result<(), CrpError> {
        self.replace_queued_bytes_at(
            graph_offset,
            edited.original_body_bytes(),
            &edited.encode(),
            "guide body",
            true,
        )
    }

    /// Replaces one already-located 11-byte `GuideTriggerBase` prefix.
    ///
    /// `graph_offset` is relative to [`serialized_object_graph`](Self::serialized_object_graph),
    /// after the root fields. Create `edited` from the queued object's decoded
    /// prefix. This method verifies the original bytes at the supplied offset
    /// and requires an equal-length replacement. It does not locate the prefix
    /// or validate its declared object type; the caller must derive the offset
    /// from the preceding queued objects.
    pub fn replace_guide_trigger_base_at(
        &mut self,
        graph_offset: usize,
        edited: &GuideTriggerBaseEditor,
    ) -> Result<(), CrpError> {
        self.replace_queued_bytes_at(
            graph_offset,
            edited.original_prefix_bytes(),
            &edited.encode(),
            "guide trigger prefix",
            true,
        )
    }

    fn replace_queued_bytes_at(
        &mut self,
        graph_offset: usize,
        original: &[u8],
        replacement: &[u8],
        description: &str,
        preserve_length: bool,
    ) -> Result<(), CrpError> {
        if preserve_length && original.len() != replacement.len() {
            return Err(invalid(
                graph_offset,
                format!("{description} replacement must preserve the original length"),
            ));
        }
        let end = graph_offset
            .checked_add(original.len())
            .ok_or(CrpError::ArithmeticOverflow(
                "queued object replacement range",
            ))?;
        let Some(current) = self.serialized_object_graph.get(graph_offset..end) else {
            return Err(invalid(
                graph_offset,
                format!("queued {description} extends past manager graph"),
            ));
        };
        if current != original {
            return Err(invalid(
                graph_offset,
                format!("queued bytes do not match the editor's original {description}"),
            ));
        }
        if preserve_length {
            self.serialized_object_graph[graph_offset..end].copy_from_slice(replacement);
        } else {
            self.serialized_object_graph
                .splice(graph_offset..end, replacement.iter().copied());
        }
        Ok(())
    }

    fn net_manager_guide_queue_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<QueuedObjectReferencePrefix, CrpError> {
        let managers = self
            .manager_objects
            .as_deref()
            .ok_or_else(|| invalid(0, "manager object array is null"))?;
        for (slot, manager_type) in [
            "GameAreaManager+Data",
            "TerrainManager+Data",
            "WaterSimulation+Data",
            "InstanceManager+Data",
            "PathManager+Data",
            "NetManager+Data",
            "BuildingManager+Data",
        ]
        .into_iter()
        .enumerate()
        {
            expect_manager_type(Some(managers), slot, manager_type)?;
        }
        let guide_slots = managers
            .iter()
            .enumerate()
            .filter_map(|(slot, reference)| {
                reference
                    .as_ref()
                    .is_some_and(|reference| reference.type_name.starts_with("GuideManager+Data"))
                    .then_some(slot)
            })
            .collect::<Vec<_>>();
        if guide_slots.len() != 1 {
            return Err(invalid(
                0,
                "manager array must contain exactly one GuideManager+Data reference",
            ));
        }
        let guide_slot = guide_slots[0];
        let last_populated = managers.iter().rposition(Option::is_some);
        if last_populated != Some(guide_slot) {
            return Err(invalid(
                guide_slot,
                "GuideManager+Data is not the last populated manager reference",
            ));
        }

        let (mut object_table, root_object_ids) = self.replay_manager_array()?;
        if root_object_ids.len() != managers.len() {
            return Err(invalid(
                0,
                "decoded manager references do not match their serialized array",
            ));
        }
        for (slot, (reference, object_id)) in managers.iter().zip(&root_object_ids).enumerate() {
            match (reference, object_id) {
                (None, None) => {}
                (Some(reference), Some(object_id))
                    if reference.object_id == *object_id
                        && object_table.type_name(*object_id)
                            == Some(reference.type_name.as_str()) => {}
                _ => {
                    return Err(invalid(
                        slot,
                        "decoded manager reference disagrees with its serialized object ID or type",
                    ));
                }
            }
        }

        let children_start = self.guide_manager_children_start()?;
        let mut queued_objects = Vec::new();

        let (game_area, _) = self.decode_game_area_prefix()?;
        register_raw_queued_reference(
            game_area.area_not_unlocked_reference(),
            &mut object_table,
            &mut queued_objects,
            "GameAreaManager guide reference",
        )?;

        let (terrain, _) = self.decode_terrain_prefix()?;
        for reference in terrain.object_references() {
            register_raw_queued_reference(
                reference,
                &mut object_table,
                &mut queued_objects,
                "TerrainManager object reference",
            )?;
        }

        let (instance, _) = self.decode_instance_prefix()?;
        for reference in instance.group_references() {
            let mut reader = Reader::new(reference);
            reader.read_u32_be()?;
            let header = read_net_object_header(&mut reader)?;
            if reader.remaining() != 0 {
                return Err(invalid(
                    reader.offset(),
                    "InstanceManager group reference has trailing bytes",
                ));
            }
            register_queued_reference(&header, &mut object_table, &mut queued_objects)?;
        }

        let (net_tail, _) = self.decode_net_manager_tail_prefix(save_app_version)?;
        let mut net_manager_object_ids = Vec::with_capacity(net_tail.object_headers().len());
        for header in net_tail.object_headers() {
            net_manager_object_ids.push(register_queued_reference(
                header,
                &mut object_table,
                &mut queued_objects,
            )?);
        }

        Ok(QueuedObjectReferencePrefix {
            children_start,
            object_table,
            queued_objects,
            net_manager_object_ids,
            water_manager_object_ids: Vec::new(),
            zone_manager_object_ids: Vec::new(),
            transport_manager_object_ids: Vec::new(),
            economy_manager_object_ids: Vec::new(),
            district_manager_object_ids: Vec::new(),
        })
    }

    fn water_manager_guide_queue_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<QueuedObjectReferencePrefix, CrpError> {
        let managers = self
            .manager_objects
            .as_deref()
            .ok_or_else(|| invalid(0, "manager object array is null"))?;
        for (slot, manager_type) in [
            "CitizenManager+Data",
            "TransferManager+Data",
            "NaturalResourceManager+Data",
            "ImmaterialResourceManager+Data",
            "ElectricityManager+Data",
            "WaterManager+Data",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, manager_type)| (index + 7, manager_type))
        {
            expect_manager_type(Some(managers), slot, manager_type)?;
        }

        let mut prefix = self.net_manager_guide_queue_prefix(save_app_version)?;
        let (building, _) = self.decode_building_object_headers_prefix(save_app_version)?;
        for header in building.object_headers() {
            register_queued_reference(
                header,
                &mut prefix.object_table,
                &mut prefix.queued_objects,
            )?;
        }

        for header in self.decode_water_manager_guide_reference_headers(&prefix.object_table)? {
            prefix
                .water_manager_object_ids
                .push(register_queued_reference(
                    &header,
                    &mut prefix.object_table,
                    &mut prefix.queued_objects,
                )?);
        }
        Ok(prefix)
    }

    fn zone_manager_guide_queue_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<QueuedObjectReferencePrefix, CrpError> {
        let mut prefix = self.water_manager_guide_queue_prefix(save_app_version)?;
        let (zone, _) = self.decode_zone_manager_prefix()?;
        for header in zone.object_references() {
            prefix
                .zone_manager_object_ids
                .push(register_queued_reference(
                    header,
                    &mut prefix.object_table,
                    &mut prefix.queued_objects,
                )?);
        }
        Ok(prefix)
    }

    fn transport_manager_queue_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<QueuedObjectReferencePrefix, CrpError> {
        let mut prefix = self.zone_manager_guide_queue_prefix(save_app_version)?;
        let headers = self.decode_transport_manager_reference_headers(&prefix.object_table)?;
        prefix.transport_manager_object_ids = register_reference_headers(
            &headers,
            &mut prefix.object_table,
            &mut prefix.queued_objects,
        )?;
        Ok(prefix)
    }

    fn economy_manager_queue_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<QueuedObjectReferencePrefix, CrpError> {
        let mut prefix = self.transport_manager_queue_prefix(save_app_version)?;
        let headers = self.decode_economy_manager_reference_headers(&prefix.object_table)?;
        prefix.economy_manager_object_ids = register_reference_headers(
            &headers,
            &mut prefix.object_table,
            &mut prefix.queued_objects,
        )?;
        Ok(prefix)
    }

    fn district_manager_queue_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<QueuedObjectReferencePrefix, CrpError> {
        let mut prefix = self.economy_manager_queue_prefix(save_app_version)?;
        expect_manager_type(self.manager_objects.as_deref(), 20, "DistrictManager+Data")?;
        let manager_slot = self
            .manager_objects
            .as_deref()
            .and_then(|managers| managers.get(20))
            .and_then(Option::as_ref)
            .ok_or_else(|| invalid(20, "DistrictManager reference is null"))?;
        if prefix.object_table.type_name(manager_slot.object_id)
            != Some(manager_slot.type_name.as_str())
        {
            return Err(invalid(
                20,
                "DistrictManager root reference disagrees with the replayed object table",
            ));
        }

        let (tail, _) = self.decode_district_manager_tail()?;
        prefix.district_manager_object_ids = register_reference_headers(
            tail.object_headers(),
            &mut prefix.object_table,
            &mut prefix.queued_objects,
        )?;
        Ok(prefix)
    }

    fn decode_transport_manager_reference_headers(
        &self,
        object_table: &DataSerializerObjectTable,
    ) -> Result<Vec<NetObjectHeader>, CrpError> {
        expect_manager_type(self.manager_objects.as_deref(), 15, "TreeManager+Data")?;
        expect_manager_type(self.manager_objects.as_deref(), 17, "VehicleManager+Data")?;
        let weather_type = self
            .manager_objects
            .as_deref()
            .and_then(|managers| managers.get(16))
            .and_then(Option::as_ref)
            .map(|reference| reference.type_name.as_str());
        if !weather_type.is_some_and(|type_name| {
            type_name.starts_with("WeatherManager+Data")
                || type_name.starts_with("WindManager+Data")
        }) {
            return Err(invalid(
                16,
                "expected WeatherManager+Data or WindManager+Data before VehicleManager",
            ));
        }
        expect_manager_type(self.manager_objects.as_deref(), 18, "TransportManager+Data")?;
        expect_manager_type(self.manager_objects.as_deref(), 19, "EconomyManager+Data")?;
        if self.format_version < 164 {
            return Err(invalid(
                0,
                "TransportManager reference resolution requires inner version 164 or newer",
            ));
        }
        let manager_slot = self
            .manager_objects
            .as_deref()
            .and_then(|managers| managers.get(18))
            .and_then(Option::as_ref)
            .ok_or_else(|| invalid(18, "TransportManager reference is null"))?;
        if object_table.type_name(manager_slot.object_id) != Some(manager_slot.type_name.as_str()) {
            return Err(invalid(
                18,
                "TransportManager root reference disagrees with the replayed object table",
            ));
        }
        let transport_tag = new_profiler_tag_bytes("TransportManager")?;
        let transport_tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &transport_tag,
            "TransportManager profiler tag",
        )?;
        let body_start = transport_tag_start
            .checked_add(transport_tag.len())
            .ok_or(CrpError::ArithmeticOverflow("TransportManager body start"))?;

        let economy_tag = new_profiler_tag_bytes("EconomyManager")?;
        let economy_tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &economy_tag,
            "EconomyManager profiler tag",
        )?;
        let end_tag_start = economy_tag_start
            .checked_sub(LOADING_PROFILER_END_TAG_BYTES)
            .ok_or_else(|| invalid(economy_tag_start, "TransportManager end tag is truncated"))?;
        if end_tag_start < body_start
            || self
                .serialized_object_graph
                .get(end_tag_start)
                .is_none_or(|marker| *marker >= 0xfe)
        {
            return Err(invalid(
                end_tag_start,
                "TransportManager fields are not followed by a valid closing tag and EconomyManager",
            ));
        }

        let body = &self.serialized_object_graph[body_start..end_tag_start];
        decode_transport_manager_reference_body(self.format_version, body, object_table)
    }

    fn economy_manager_body(
        &self,
        object_table: &DataSerializerObjectTable,
    ) -> Result<&[u8], CrpError> {
        expect_manager_type(self.manager_objects.as_deref(), 19, "EconomyManager+Data")?;
        expect_manager_type(self.manager_objects.as_deref(), 20, "DistrictManager+Data")?;
        if self.format_version < 164 {
            return Err(invalid(
                0,
                "EconomyManager reference resolution requires inner version 164 or newer",
            ));
        }

        let manager_slot = self
            .manager_objects
            .as_deref()
            .and_then(|managers| managers.get(19))
            .and_then(Option::as_ref)
            .ok_or_else(|| invalid(19, "EconomyManager reference is null"))?;
        if object_table.type_name(manager_slot.object_id) != Some(manager_slot.type_name.as_str()) {
            return Err(invalid(
                19,
                "EconomyManager root reference disagrees with the replayed object table",
            ));
        }

        let economy_tag = new_profiler_tag_bytes("EconomyManager")?;
        let economy_tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &economy_tag,
            "EconomyManager profiler tag",
        )?;
        let body_start = economy_tag_start
            .checked_add(economy_tag.len())
            .ok_or(CrpError::ArithmeticOverflow("EconomyManager body start"))?;

        let district_tag = new_profiler_tag_bytes("DistrictManager")?;
        let district_tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &district_tag,
            "DistrictManager profiler tag",
        )?;
        let end_tag_start = district_tag_start
            .checked_sub(LOADING_PROFILER_END_TAG_BYTES)
            .ok_or_else(|| invalid(district_tag_start, "EconomyManager end tag is truncated"))?;
        if end_tag_start < body_start
            || self
                .serialized_object_graph
                .get(end_tag_start)
                .is_none_or(|marker| *marker >= 0xfe)
        {
            return Err(invalid(
                end_tag_start,
                "EconomyManager fields are not followed by a valid closing tag and DistrictManager",
            ));
        }

        Ok(&self.serialized_object_graph[body_start..end_tag_start])
    }

    fn decode_economy_manager_reference_headers(
        &self,
        object_table: &DataSerializerObjectTable,
    ) -> Result<Vec<NetObjectHeader>, CrpError> {
        let body = self.economy_manager_body(object_table)?;
        if self.format_version >= 116_000 {
            return find_economy_manager_guide_headers(self.format_version, body, object_table);
        }
        let reference_count = economy_manager_object_header_count(self.format_version);
        find_trailing_object_headers(body, reference_count, object_table)
    }

    fn decode_water_manager_guide_reference_headers(
        &self,
        object_table: &DataSerializerObjectTable,
    ) -> Result<Vec<NetObjectHeader>, CrpError> {
        expect_manager_type(self.manager_objects.as_deref(), 12, "WaterManager+Data")?;
        expect_manager_type(self.manager_objects.as_deref(), 13, "ZoneManager+Data")?;
        if self.format_version < 94 {
            return Ok(vec![NetObjectHeader::Null; 2]);
        }

        let zone_tag = new_profiler_tag_bytes("ZoneManager")?;
        let zone_tag_start = find_unique_bytes(
            &self.serialized_object_graph,
            &zone_tag,
            "ZoneManager profiler tag",
        )?;
        let end_tag_start = zone_tag_start
            .checked_sub(LOADING_PROFILER_END_TAG_BYTES)
            .ok_or_else(|| invalid(zone_tag_start, "WaterManager closing tag is truncated"))?;
        if self
            .serialized_object_graph
            .get(end_tag_start)
            .is_none_or(|marker| *marker >= 0xfe)
        {
            return Err(invalid(
                end_tag_start,
                "WaterManager closing profiler reference is malformed",
            ));
        }

        let mut matches = Vec::new();
        for encoded_length in [2_usize, 4, 6] {
            let Some(start) = end_tag_start.checked_sub(encoded_length) else {
                continue;
            };
            let Some(header_bytes) = self.serialized_object_graph.get(start..end_tag_start) else {
                continue;
            };
            let mut reader = Reader::new(header_bytes);
            let Ok(first) = read_net_object_header(&mut reader) else {
                continue;
            };
            let Ok(second) = read_net_object_header(&mut reader) else {
                continue;
            };
            if reader.remaining() != 0 || start < 9 {
                continue;
            }
            let fields_start = start - 9;
            let mut fields = Reader::new(&self.serialized_object_graph[fields_start..start]);
            let Ok(processed_cells) = fields.read_i32_be() else {
                continue;
            };
            let Ok(conductive_cells) = fields.read_i32_be() else {
                continue;
            };
            if !(0..=65_536).contains(&processed_cells)
                || !(0..=65_536).contains(&conductive_cells)
                || fields.read_u8().is_err()
            {
                continue;
            }
            let mut candidate_table = object_table.clone();
            if candidate_table.register(&first).is_err()
                || candidate_table.register(&second).is_err()
            {
                continue;
            }
            matches.push(vec![first, second]);
        }
        if matches.len() != 1 {
            return Err(invalid(
                end_tag_start,
                format!(
                    "WaterManager guide-reference tail has {} valid interpretations",
                    matches.len()
                ),
            ));
        }
        matches.pop().ok_or_else(|| {
            invalid(
                end_tag_start,
                "WaterManager guide-reference tail could not be resolved",
            )
        })
    }

    /// Locates a queued NetManager guide body by its reference-header index.
    ///
    /// The locator reconstructs the DataSerializer type/object tables and
    /// follows GameArea, Terrain, InstanceManager, and NetManager references.
    /// It returns an error when an earlier queued object type is not
    /// understood. Pass the saved application version for inner-version-221+
    /// `NetNodeInstanceGuide` bodies. The supported manager layout requires
    /// GameArea, Terrain, WaterSimulation, InstanceManager, PathManager,
    /// NetManager, and BuildingManager in slots 0–6, with GuideManager as the
    /// last populated manager reference.
    pub fn locate_net_manager_guide_body(
        &self,
        header_index: usize,
        save_app_version: Option<u32>,
    ) -> Result<GuideBodyLocation, CrpError> {
        let prefix = self.net_manager_guide_queue_prefix(save_app_version)?;
        let object_id = prefix
            .net_manager_object_ids
            .get(header_index)
            .copied()
            .flatten()
            .ok_or_else(|| {
                invalid(
                    header_index,
                    "NetManager guide-header index is out of bounds or references null",
                )
            })?;
        locate_queued_object_body(
            &self.serialized_object_graph,
            prefix.children_start,
            &prefix.queued_objects,
            object_id,
            self.format_version,
            save_app_version,
        )
    }

    /// Locates every queued NetManager guide body in header order.
    ///
    /// Null references remain `None`. All non-null bodies are resolved in one
    /// queue walk, and repeated references return the same body location.
    /// Unknown body types before the last requested object return an error.
    pub fn locate_net_manager_guide_bodies(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<Vec<Option<GuideBodyLocation>>, CrpError> {
        let prefix = self.net_manager_guide_queue_prefix(save_app_version)?;
        locate_queued_object_bodies(
            &self.serialized_object_graph,
            prefix.children_start,
            &prefix.queued_objects,
            &prefix.net_manager_object_ids,
            self.format_version,
            save_app_version,
        )
    }

    /// Locates a queued WaterManager guide body by its reference-field index.
    ///
    /// The queue replay follows GameArea, Terrain, InstanceManager, NetManager,
    /// BuildingManager, and WaterManager references. In the supported root
    /// layout, CitizenManager through ElectricityManager (slots 7–11) declare
    /// no DataSerializer child objects. WaterManager's two version-94 guide
    /// headers are read immediately before its closing profiler reference.
    /// The manager array must contain WaterManager in slot 12 and GuideManager
    /// as the last populated reference. Pass the saved application version
    /// for inner-version-221+ NetNodeInstanceGuide bodies in earlier queues.
    pub fn locate_water_manager_guide_body(
        &self,
        header_index: usize,
        save_app_version: Option<u32>,
    ) -> Result<GuideBodyLocation, CrpError> {
        let prefix = self.water_manager_guide_queue_prefix(save_app_version)?;
        let object_id = prefix
            .water_manager_object_ids
            .get(header_index)
            .copied()
            .flatten()
            .ok_or_else(|| {
                invalid(
                    header_index,
                    "WaterManager guide-header index is out of bounds or references null",
                )
            })?;
        locate_queued_object_body(
            &self.serialized_object_graph,
            prefix.children_start,
            &prefix.queued_objects,
            object_id,
            self.format_version,
            save_app_version,
        )
    }

    /// Locates both queued WaterManager guide bodies in header order.
    ///
    /// Null references remain `None`; repeated references return the same
    /// body location. Earlier unsupported queued guide types return an error.
    pub fn locate_water_manager_guide_bodies(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<Vec<Option<GuideBodyLocation>>, CrpError> {
        let prefix = self.water_manager_guide_queue_prefix(save_app_version)?;
        locate_queued_object_bodies(
            &self.serialized_object_graph,
            prefix.children_start,
            &prefix.queued_objects,
            &prefix.water_manager_object_ids,
            self.format_version,
            save_app_version,
        )
    }

    /// Locates one queued ZoneManager guide body by its reference-field index.
    ///
    /// The queue replay follows the supported GameArea, Terrain, InstanceManager,
    /// NetManager, BuildingManager, and WaterManager references before ZoneManager.
    /// ZoneManager references are ordered as eight unused-zone entries, three demand
    /// wrappers, options, and zones. The locator currently supports the base-only
    /// `ZoneTypeGuide` and `GenericGuide` body layouts, and errors if an earlier
    /// queued object has an unknown body schema. Pass the saved application version
    /// for inner-version-221+ `NetNodeInstanceGuide` bodies in earlier queues.
    pub fn locate_zone_manager_guide_body(
        &self,
        header_index: usize,
        save_app_version: Option<u32>,
    ) -> Result<GuideBodyLocation, CrpError> {
        let prefix = self.zone_manager_guide_queue_prefix(save_app_version)?;
        let object_id = prefix
            .zone_manager_object_ids
            .get(header_index)
            .copied()
            .flatten()
            .ok_or_else(|| {
                invalid(
                    header_index,
                    "ZoneManager guide-header index is out of bounds or references null",
                )
            })?;
        locate_queued_object_body(
            &self.serialized_object_graph,
            prefix.children_start,
            &prefix.queued_objects,
            object_id,
            self.format_version,
            save_app_version,
        )
    }

    /// Locates all queued ZoneManager guide bodies in reference-field order.
    ///
    /// Null references remain `None`, and repeated references resolve to the same
    /// body location. The locator supports `ZoneTypeGuide` and `GenericGuide` bodies
    /// after the supported earlier queues; an unknown preceding body type returns an
    /// error.
    pub fn locate_zone_manager_guide_bodies(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<Vec<Option<GuideBodyLocation>>, CrpError> {
        let prefix = self.zone_manager_guide_queue_prefix(save_app_version)?;
        locate_queued_object_bodies(
            &self.serialized_object_graph,
            prefix.children_start,
            &prefix.queued_objects,
            &prefix.zone_manager_object_ids,
            self.format_version,
            save_app_version,
        )
    }

    /// Resolves TransportManager's fixed object-reference fields in order.
    ///
    /// This reader supports inner versions 164–115999 when line and milestone
    /// buffers are empty. It follows the known queue through ZoneManager, then
    /// reads the version-gated TransportManager tail, including the selection
    /// table from version 114001. Selection values are consumed for boundary
    /// validation but remain opaque; active lines and milestones return an error.
    /// Pass the saved application version for inner-version-221+
    /// NetNodeInstanceGuide fields in the earlier manager queue.
    pub fn resolve_transport_manager_references(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<Vec<Option<ResolvedObjectReference>>, CrpError> {
        let prefix = self.transport_manager_queue_prefix(save_app_version)?;
        prefix
            .transport_manager_object_ids
            .iter()
            .map(|object_id| {
                object_id
                    .map(|object_id| {
                        let type_name =
                            prefix.object_table.type_name(object_id).ok_or_else(|| {
                                invalid(0, "TransportManager object type is unresolved")
                            })?;
                        Ok(ResolvedObjectReference {
                            object_id,
                            type_name: type_name.to_owned(),
                        })
                    })
                    .transpose()
            })
            .collect()
    }

    /// Resolves EconomyManager's trailing guide object references.
    ///
    /// The reader supports inner version 164 and newer. The base references
    /// are gated at versions 94 and 166. From version 116017, three additional
    /// `GenericGuide` references are serialized by `DeserializeInvestments`.
    /// For version 116000 and newer, the method scans the bounded EconomyManager
    /// body for newly declared `GenericGuide` references and requires exactly
    /// the version-gated count. Null or existing-object headers in these fields
    /// are not yet resolved and cause an error. Pass the saved application
    /// version for earlier inner-version-221+ NetNodeInstanceGuide fields in
    /// the manager queue.
    pub fn resolve_economy_manager_references(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<Vec<Option<ResolvedObjectReference>>, CrpError> {
        let prefix = self.economy_manager_queue_prefix(save_app_version)?;
        prefix
            .economy_manager_object_ids
            .iter()
            .map(|object_id| {
                object_id
                    .map(|object_id| {
                        let type_name =
                            prefix.object_table.type_name(object_id).ok_or_else(|| {
                                invalid(0, "EconomyManager object type is unresolved")
                            })?;
                        Ok(ResolvedObjectReference {
                            object_id,
                            type_name: type_name.to_owned(),
                        })
                    })
                    .transpose()
            })
            .collect()
    }

    /// Decodes EconomyManager's investment activation flags from inner version 116009 onward.
    ///
    /// The returned vector contains 254 flags for investment indices 1–254;
    /// element zero corresponds to serialized investment index 1. This path
    /// requires two newly declared `GenericGuide` references through version
    /// 116016, or five from version 116017, plus a resolvable preceding
    /// TransportManager layout. For versions 116009–116016, the bounded
    /// investment payload must match a supported version layout. Investment
    /// values and prefab assignments remain opaque. Pass
    /// the saved application version for earlier
    /// inner-version-221+ NetNodeInstanceGuide fields in the manager queue.
    pub fn decode_economy_manager_investment_flags(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<Vec<u16>, CrpError> {
        let prefix = self.transport_manager_queue_prefix(save_app_version)?;
        let body = self.economy_manager_body(&prefix.object_table)?;
        decode_economy_manager_investment_flags_from_body(
            self.format_version,
            body,
            &prefix.object_table,
        )
        .map(|(flags, _)| flags)
    }

    /// Decodes each active investment's `EncodedArray.Float` value history.
    ///
    /// This supports inner version 116009 and newer and returns active entries
    /// in serialized investment-index order. Negative value counts and
    /// truncated arrays return errors. The same newly declared guide-reference
    /// and preceding TransportManager requirements as
    /// [`decode_economy_manager_investment_flags`](Self::decode_economy_manager_investment_flags)
    /// apply. Other investment columns and prefab assignments remain opaque.
    pub fn decode_economy_manager_investment_value_series(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<Vec<EconomyInvestmentValueSeries>, CrpError> {
        let prefix = self.transport_manager_queue_prefix(save_app_version)?;
        let body = self.economy_manager_body(&prefix.object_table)?;
        decode_economy_manager_investment_value_series(
            self.format_version,
            body,
            &prefix.object_table,
        )
        .map(|(series, _)| series)
    }

    /// Decodes the modeled numeric fields for active investments.
    ///
    /// This supports inner version 116009 and newer. It includes the float
    /// history, player-spent amount, owned-share count, base value, missing
    /// requirement date, current state, and random influence. Prefab
    /// assignments and names remain opaque. Negative history counts and
    /// truncated fields return errors. Fields not serialized before their
    /// version gate are returned as zero defaults. The same queue and
    /// guide-reference requirements as
    /// [`decode_economy_manager_investment_value_series`](Self::decode_economy_manager_investment_value_series)
    /// apply.
    pub fn decode_economy_manager_investments(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<Vec<EconomyInvestmentData>, CrpError> {
        let prefix = self.transport_manager_queue_prefix(save_app_version)?;
        let body = self.economy_manager_body(&prefix.object_table)?;
        decode_economy_manager_investments(self.format_version, body, &prefix.object_table)
            .map(|(investments, _)| investments)
    }

    /// Decodes active-investment prefab indices and the serialized name table.
    ///
    /// Supports inner version 116009 and newer when the version-gated guide
    /// references are newly declared `GenericGuide` objects. Assignments are
    /// returned in active-investment order, and null name entries are
    /// preserved. This method also validates the known investment numeric
    /// prefix before reading the prefab scope.
    pub fn decode_economy_manager_investment_prefabs(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<EconomyInvestmentPrefabTable, CrpError> {
        let prefix = self.transport_manager_queue_prefix(save_app_version)?;
        let body = self.economy_manager_body(&prefix.object_table)?;
        decode_economy_manager_investment_prefabs_from_body(
            self.format_version,
            body,
            &prefix.object_table,
        )
        .map(|(prefabs, _)| prefabs)
    }

    /// Resolves the version-gated object references at the end of DistrictManager.
    ///
    /// This reader supports inner version 164 and newer when the preceding
    /// TransportManager and EconomyManager reference layouts can be resolved.
    /// The modern EconomyManager path currently requires newly declared guide
    /// references. The post-grid DistrictManager tail is version-gated; its
    /// layout has been exercised against samples through version 116019.
    /// Referenced object bodies remain opaque. Pass the saved application
    /// version for earlier inner-version-221+ NetNodeInstanceGuide fields.
    pub fn resolve_district_manager_references(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<Vec<Option<ResolvedObjectReference>>, CrpError> {
        let prefix = self.district_manager_queue_prefix(save_app_version)?;
        prefix
            .district_manager_object_ids
            .iter()
            .map(|object_id| {
                object_id
                    .map(|object_id| {
                        let type_name =
                            prefix.object_table.type_name(object_id).ok_or_else(|| {
                                invalid(0, "DistrictManager object type is unresolved")
                            })?;
                        Ok(ResolvedObjectReference {
                            object_id,
                            type_name: type_name.to_owned(),
                        })
                    })
                    .transpose()
            })
            .collect()
    }

    /// Locates a queued BuildingManager object body by its reference-header index.
    ///
    /// The locator follows the supported GameArea, Terrain, InstanceManager,
    /// NetManager, and BuildingManager reference fields, reconstructs the
    /// DataSerializer type/object tables, and advances through recognized
    /// guide bodies preceding the requested object. It returns an error when
    /// an earlier queued object type is not understood. Pass the saved
    /// application version for inner-version-221+ `NetNodeInstanceGuide`
    /// bodies. The current layout requires GameArea, Terrain, WaterSimulation,
    /// InstanceManager, PathManager, NetManager, and BuildingManager in slots
    /// 0–6, with GuideManager as the last populated manager reference.
    pub fn locate_building_manager_guide_body(
        &self,
        header_index: usize,
        save_app_version: Option<u32>,
    ) -> Result<GuideBodyLocation, CrpError> {
        let mut prefix = self.net_manager_guide_queue_prefix(save_app_version)?;
        let (building_headers, _) = self.decode_building_object_headers_prefix(save_app_version)?;
        if header_index >= building_headers.object_headers().len() {
            return Err(invalid(
                header_index,
                "BuildingManager object-header index is out of bounds",
            ));
        }
        let mut target_object_id = None;
        for (index, header) in building_headers
            .object_headers()
            .iter()
            .take(header_index + 1)
            .enumerate()
        {
            let object_id = register_queued_reference(
                header,
                &mut prefix.object_table,
                &mut prefix.queued_objects,
            )?;
            if index == header_index {
                target_object_id = object_id;
            }
        }
        let target_object_id = target_object_id
            .ok_or_else(|| invalid(header_index, "BuildingManager guide reference is null"))?;

        locate_queued_object_body(
            &self.serialized_object_graph,
            prefix.children_start,
            &prefix.queued_objects,
            target_object_id,
            self.format_version,
            save_app_version,
        )
    }

    /// Locates every queued BuildingManager guide body in header order.
    ///
    /// Null references remain `None`. All non-null bodies are resolved in one
    /// queue walk, and repeated references return the same body location.
    /// Unknown body types before the last requested object return an error.
    pub fn locate_building_manager_guide_bodies(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<Vec<Option<GuideBodyLocation>>, CrpError> {
        let mut prefix = self.net_manager_guide_queue_prefix(save_app_version)?;
        let (building_headers, _) = self.decode_building_object_headers_prefix(save_app_version)?;
        let mut building_object_ids = Vec::with_capacity(building_headers.object_headers().len());
        for header in building_headers.object_headers() {
            building_object_ids.push(register_queued_reference(
                header,
                &mut prefix.object_table,
                &mut prefix.queued_objects,
            )?);
        }
        locate_queued_object_bodies(
            &self.serialized_object_graph,
            prefix.children_start,
            &prefix.queued_objects,
            &building_object_ids,
            self.format_version,
            save_app_version,
        )
    }

    /// Replaces a BuildingManager `BuildingInstanceGuide` or
    /// `BuildingOnFireGuide` body located from its reference-header index.
    ///
    /// The editor must originate from the body currently stored at that
    /// location. The replacement remains exactly 17 bytes and does not change
    /// object IDs or following queued data.
    pub fn replace_building_manager_guide_instance_body(
        &mut self,
        header_index: usize,
        save_app_version: Option<u32>,
        edited: &GuideInstanceEditor,
    ) -> Result<(), CrpError> {
        let location = self.locate_building_manager_guide_body(header_index, save_app_version)?;
        let class_name = object_class_name(&location.type_name);
        if !matches!(class_name, "BuildingInstanceGuide" | "BuildingOnFireGuide")
            || location.body_length != GUIDE_INSTANCE_BODY_LEN
        {
            return Err(invalid(
                header_index,
                format!(
                    "BuildingManager object is {}, not a 17-byte building-instance guide",
                    location.type_name
                ),
            ));
        }
        self.replace_guide_instance_body_at(location.graph_offset, edited)
    }

    /// Replaces a BuildingManager guide whose body is only a
    /// `GuideTriggerBase` prefix.
    ///
    /// The method locates the body from its BuildingManager reference-header
    /// index and accepts `GenericGuide`, `ServiceTypeGuide`, or
    /// `BuildingTypeGuide` objects. The editor must originate from the prefix
    /// currently stored at that location. The replacement remains 11 bytes,
    /// preserving object IDs and following queued data.
    pub fn replace_building_manager_guide_trigger_base(
        &mut self,
        header_index: usize,
        save_app_version: Option<u32>,
        edited: &GuideTriggerBaseEditor,
    ) -> Result<(), CrpError> {
        let location = self.locate_building_manager_guide_body(header_index, save_app_version)?;
        let class_name = object_class_name(&location.type_name);
        if !matches!(
            class_name,
            "GenericGuide" | "ServiceTypeGuide" | "BuildingTypeGuide"
        ) || location.body_length != GUIDE_TRIGGER_BASE_PREFIX_LEN
        {
            return Err(invalid(
                header_index,
                format!(
                    "BuildingManager object is {}, not an 11-byte trigger-only guide",
                    location.type_name
                ),
            ));
        }
        self.replace_guide_trigger_base_at(location.graph_offset, edited)
    }

    /// Replaces a NetManager `NetSegmentInstanceGuide` body.
    ///
    /// The method locates the body from its NetManager reference-header index
    /// and verifies the declared type. The editor must originate from the body
    /// currently stored there. The replacement remains 17 bytes.
    pub fn replace_net_manager_guide_instance_body(
        &mut self,
        header_index: usize,
        save_app_version: Option<u32>,
        edited: &GuideInstanceEditor,
    ) -> Result<(), CrpError> {
        let location = self.locate_net_manager_guide_body(header_index, save_app_version)?;
        if object_class_name(&location.type_name) != "NetSegmentInstanceGuide"
            || location.body_length != GUIDE_INSTANCE_BODY_LEN
        {
            return Err(invalid(
                header_index,
                format!(
                    "NetManager object is {}, not a 17-byte segment-instance guide",
                    location.type_name
                ),
            ));
        }
        self.replace_guide_instance_body_at(location.graph_offset, edited)
    }

    /// Replaces a NetManager guide whose body is only a `GuideTriggerBase`.
    ///
    /// The method locates the body from its NetManager reference-header index
    /// and accepts `GenericGuide`, `ServiceTypeGuide`, or `BuildingTypeGuide`.
    /// The editor must originate from the prefix currently stored there. The
    /// replacement remains 11 bytes.
    pub fn replace_net_manager_guide_trigger_base(
        &mut self,
        header_index: usize,
        save_app_version: Option<u32>,
        edited: &GuideTriggerBaseEditor,
    ) -> Result<(), CrpError> {
        let location = self.locate_net_manager_guide_body(header_index, save_app_version)?;
        let class_name = object_class_name(&location.type_name);
        if !matches!(
            class_name,
            "GenericGuide" | "ServiceTypeGuide" | "BuildingTypeGuide"
        ) || location.body_length != GUIDE_TRIGGER_BASE_PREFIX_LEN
        {
            return Err(invalid(
                header_index,
                format!(
                    "NetManager object is {}, not an 11-byte trigger-only guide",
                    location.type_name
                ),
            ));
        }
        self.replace_guide_trigger_base_at(location.graph_offset, edited)
    }

    /// Replaces a NetManager `NetNodeInstanceGuide` body.
    ///
    /// The method locates the body from its NetManager reference-header index
    /// and verifies both the declared type and the version-dependent body
    /// length. The editor must originate from the body currently stored there.
    pub fn replace_net_manager_node_instance_guide(
        &mut self,
        header_index: usize,
        save_app_version: Option<u32>,
        edited: &NetNodeInstanceGuideEditor,
    ) -> Result<(), CrpError> {
        if !edited.matches_version_gates(self.format_version, save_app_version) {
            return Err(invalid(
                header_index,
                "NetNodeInstanceGuide editor uses different format or save-application version gates",
            ));
        }
        let location = self.locate_net_manager_guide_body(header_index, save_app_version)?;
        let original = edited.original_body_bytes();
        if object_class_name(&location.type_name) != "NetNodeInstanceGuide"
            || location.body_length != original.len()
        {
            return Err(invalid(
                header_index,
                format!(
                    "NetManager object is {}, not a matching NetNodeInstanceGuide body",
                    location.type_name
                ),
            ));
        }
        let replacement = edited.encode()?;
        self.replace_queued_bytes_at(
            location.graph_offset,
            original,
            &replacement,
            "NetNodeInstanceGuide body",
            true,
        )
    }

    fn replay_manager_array(
        &self,
    ) -> Result<(DataSerializerObjectTable, Vec<Option<u32>>), CrpError> {
        let mut reader = Reader::new(&self.manager_array_bytes);
        let first = reader.read_u8()?;
        let mut object_table = DataSerializerObjectTable::new(ROOT_TYPE);
        if first == 0xff {
            if reader.remaining() != 0 {
                return Err(invalid(1, "null manager array has trailing bytes"));
            }
            return Ok((object_table, Vec::new()));
        }
        let count = (u32::from(first) << 16) | u32::from(reader.read_u16_be()?);
        if count > MAX_MANAGER_OBJECTS {
            return Err(invalid(0, "manager object count exceeds 1,000,000"));
        }
        let count = usize::try_from(count)
            .map_err(|_| CrpError::ArithmeticOverflow("manager array count"))?;
        let mut object_ids = Vec::with_capacity(count);
        for _ in 0..count {
            object_ids.push(object_table.register(&read_net_object_header(&mut reader)?)?);
        }
        if reader.remaining() != 0 {
            return Err(invalid(reader.offset(), "manager array has trailing bytes"));
        }
        Ok((object_table, object_ids))
    }

    fn guide_manager_children_start(&self) -> Result<usize, CrpError> {
        let (_, body_end) = self.decode_guide_manager_references_at()?;
        Ok(body_end)
    }

    /// Replaces the WaterSimulation body while retaining profiler tags and all
    /// other manager bytes in their original order.
    ///
    /// Pass an editor obtained from this stream's Water body. Inner serializer
    /// versions must match. The replacement uses the editor's canonical array
    /// encoding; the original body remains untouched if encoding fails.
    pub fn replace_water(&mut self, edited: &WaterSimulationEditor) -> Result<(), CrpError> {
        if edited.format_version() != self.format_version {
            return Err(invalid(
                0,
                "edited WaterSimulation version does not match this stream",
            ));
        }
        let (original, _) = self.decode_water_prefix()?;
        let (_, terrain_end) = self.decode_terrain_prefix()?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(terrain_end)?;
        let mut strings = vec![
            "SimulationManager".to_owned(),
            "GameAreaManager".to_owned(),
            "TerrainManager".to_owned(),
        ];
        expect_profile_tag(&mut reader, &mut strings, "WaterSimulation")?;
        let body_start = reader.offset();
        let body_end = body_start
            .checked_add(original.encoded_len())
            .ok_or(CrpError::ArithmeticOverflow("WaterSimulation body end"))?;
        if body_end > self.serialized_object_graph.len() {
            return Err(invalid(
                body_start,
                "WaterSimulation body extends past manager graph",
            ));
        }
        let replacement = edited.encode()?;
        self.serialized_object_graph
            .splice(body_start..body_end, replacement);
        Ok(())
    }

    /// Encodes the typed root fields and copies the manager object graph raw.
    ///
    /// The original manager-reference array encoding is preserved, so its
    /// object IDs remain consistent with the opaque queued graph. The history
    /// must contain exactly 32 values.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut root_data = self.encode_root_data()?;
        root_data.extend_from_slice(&self.serialized_object_graph);
        Ok(
            DataSerializerStream::from_parts(self.format_version, ROOT_TYPE.to_owned(), root_data)?
                .encode(),
        )
    }

    /// Encodes root fields without the queued manager-object bodies.
    pub fn encode_root_fields(&self) -> Result<Vec<u8>, CrpError> {
        let root_data = self.encode_root_data()?;
        Ok(
            DataSerializerStream::from_parts(self.format_version, ROOT_TYPE.to_owned(), root_data)?
                .encode(),
        )
    }

    fn encode_root_data(&self) -> Result<Vec<u8>, CrpError> {
        if self.build_index_history.len() != BUILD_INDEX_HISTORY_LENGTH {
            return Err(invalid(
                0,
                format!("build-index history must contain {BUILD_INDEX_HISTORY_LENGTH} entries"),
            ));
        }
        let mut root_data = Vec::new();
        root_data.extend_from_slice(&self.root_profile_begin_bytes);
        root_data.extend_from_slice(&self.current_tick_index.to_be_bytes());
        root_data.extend_from_slice(&self.current_frame_index.to_be_bytes());
        root_data.extend_from_slice(&self.current_build_index.to_be_bytes());
        root_data.extend_from_slice(&self.randomizer_seed.to_be_bytes());
        if self.format_version >= 212 {
            let offset = self
                .day_time_offset_frames
                .ok_or_else(|| invalid(root_data.len(), "day-time offset is required"))?;
            root_data.extend_from_slice(&offset.to_be_bytes());
        }
        for index in &self.build_index_history {
            root_data.extend_from_slice(&index.to_be_bytes());
        }
        root_data.extend_from_slice(&self.manager_array_bytes);
        if self.format_version >= 186 {
            let entries = self
                .serializable_data_storage
                .as_ref()
                .ok_or_else(|| invalid(root_data.len(), "serializable-data storage is required"))?;
            let count = u32::try_from(entries.len())
                .map_err(|_| CrpError::ArithmeticOverflow("serializable-data entry count"))?;
            if count > MAX_STORAGE_ENTRIES {
                return Err(invalid(
                    root_data.len(),
                    "too many serializable-data entries",
                ));
            }
            root_data.extend_from_slice(&count.to_be_bytes());
            for entry in entries {
                write_unique_string(&mut root_data, &entry.key)?;
                write_byte_array(&mut root_data, entry.data.as_deref())?;
            }
        }
        root_data.extend_from_slice(&self.root_profile_end_bytes);
        Ok(root_data)
    }
}

fn read_manager_array(
    reader: &mut Reader<'_>,
    root_type: &str,
) -> Result<Option<Vec<Option<ManagerObjectReference>>>, CrpError> {
    let first = reader.read_u8()?;
    if first == 0xff {
        return Ok(None);
    }
    let count = (u32::from(first) << 16)
        | (u32::from(reader.read_u8()?) << 8)
        | u32::from(reader.read_u8()?);
    if count > MAX_MANAGER_OBJECTS {
        return Err(reader.invalid("manager object count exceeds 1,000,000"));
    }
    if usize::try_from(count).map_or(true, |count| count > reader.remaining()) {
        return Err(reader.invalid("manager object count cannot fit in remaining bytes"));
    }
    let count = usize::try_from(count)
        .map_err(|_| reader.invalid("manager object count does not fit this platform"))?;
    let mut shared_types = vec![root_type.to_owned()];
    let mut object_types = vec![0_usize];
    let mut objects = Vec::with_capacity(count);
    for _ in 0..count {
        let marker = reader.read_u8()?;
        if marker == 0xff {
            objects.push(None);
            continue;
        }
        if marker == 0xfe {
            let type_marker = reader.read_u8()?;
            let type_index = if type_marker == 0xfe {
                let type_name = reader
                    .read_unique_string()?
                    .ok_or_else(|| reader.invalid("shared type name cannot be null"))?;
                if shared_types.len() >= MAX_SHARED_TYPES {
                    return Err(reader.invalid("shared-type table exceeds the game limit"));
                }
                shared_types.push(type_name);
                shared_types.len() - 1
            } else {
                let type_index = (u16::from(type_marker) << 8) | u16::from(reader.read_u8()?);
                usize::from(type_index)
            };
            let type_name = shared_types
                .get(type_index)
                .ok_or_else(|| reader.invalid("shared-type index is out of bounds"))?
                .clone();
            let object_id = u32::try_from(object_types.len())
                .map_err(|_| reader.invalid("object-list index does not fit u32"))?;
            if object_id >= MAX_OBJECT_ID {
                return Err(reader.invalid("object ID exceeds the DataSerializer limit"));
            }
            object_types.push(type_index);
            objects.push(Some(ManagerObjectReference {
                object_id,
                type_name,
            }));
            continue;
        }

        let object_id = (u32::from(marker) << 16)
            | (u32::from(reader.read_u8()?) << 8)
            | u32::from(reader.read_u8()?);
        let type_index = object_types
            .get(
                usize::try_from(object_id)
                    .map_err(|_| reader.invalid("object ID does not fit this platform"))?,
            )
            .copied()
            .ok_or_else(|| reader.invalid("object ID is out of bounds"))?;
        let type_name = shared_types
            .get(type_index)
            .ok_or_else(|| reader.invalid("object type index is out of bounds"))?
            .clone();
        objects.push(Some(ManagerObjectReference {
            object_id,
            type_name,
        }));
    }
    Ok(Some(objects))
}

fn expect_profile_tag(
    reader: &mut Reader<'_>,
    strings: &mut Vec<String>,
    expected: &str,
) -> Result<(), CrpError> {
    let marker = reader.read_u8()?;
    let name = match marker {
        0xff => return Err(reader.invalid("loading-profiler tag cannot be null")),
        0xfe => {
            let name = reader
                .read_unique_string()?
                .ok_or_else(|| reader.invalid("loading-profiler string cannot be null"))?;
            if strings.len() >= MAX_SHARED_TYPES {
                return Err(reader.invalid("shared-string table exceeds the game limit"));
            }
            strings.push(name.clone());
            name
        }
        high => {
            let index = usize::from(u16::from(high) << 8 | u16::from(reader.read_u8()?));
            strings
                .get(index)
                .ok_or_else(|| reader.invalid("shared-string index is out of bounds"))?
                .clone()
        }
    };
    if name != expected {
        return Err(reader.invalid(format!(
            "expected loading-profiler tag {expected}, found {name}"
        )));
    }
    Ok(())
}

fn manager_profiler_seed() -> Vec<String> {
    [
        "SimulationManager",
        "GameAreaManager",
        "TerrainManager",
        "WaterSimulation",
        "InstanceManager",
        "PathManager",
        "NetManager",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn read_byte_array(reader: &mut Reader<'_>) -> Result<Option<Vec<u8>>, CrpError> {
    let first = reader.read_u8()?;
    if first == 0xff {
        return Ok(None);
    }
    let length = (u32::from(first) << 16)
        | (u32::from(reader.read_u8()?) << 8)
        | u32::from(reader.read_u8()?);
    let length = usize::try_from(length)
        .map_err(|_| reader.invalid("byte-array length does not fit this platform"))?;
    Ok(Some(reader.read_bytes(length)?.to_vec()))
}

fn write_byte_array(output: &mut Vec<u8>, value: Option<&[u8]>) -> Result<(), CrpError> {
    let Some(value) = value else {
        output.push(0xff);
        return Ok(());
    };
    let length = u32::try_from(value.len())
        .map_err(|_| CrpError::ArithmeticOverflow("DataSerializer byte-array length"))?;
    write_u24_be(output, length)?;
    output.extend_from_slice(value);
    Ok(())
}

fn read_u64_be(reader: &mut Reader<'_>) -> Result<u64, CrpError> {
    let bytes = reader.read_bytes(8)?;
    Ok(u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "SimulationManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

fn economy_manager_object_header_count(format_version: u32) -> usize {
    if format_version >= 166 {
        2
    } else if format_version >= 94 {
        1
    } else {
        0
    }
}

fn serialized_net_object_header_end(bytes: &[u8], start: usize, limit: usize) -> Option<usize> {
    let marker = *bytes.get(start)?;
    let after_marker = start.checked_add(1)?;
    match marker {
        0xff => Some(after_marker),
        0xfe => {
            let type_marker = *bytes.get(after_marker)?;
            match type_marker {
                0xff => None,
                0xfe => {
                    let string_start = after_marker.checked_add(1)?;
                    let first = *bytes.get(string_start)?;
                    if first == 0xff {
                        return None;
                    }
                    let length_end = string_start.checked_add(3)?;
                    if length_end > limit {
                        return None;
                    }
                    let encoded_length = (u32::from(first) << 16)
                        | (u32::from(*bytes.get(string_start.checked_add(1)?)?) << 8)
                        | u32::from(*bytes.get(string_start.checked_add(2)?)?);
                    let compact = encoded_length & 0x80_0000 != 0;
                    let character_count = usize::try_from(encoded_length & !0x80_0000).ok()?;
                    if character_count > 16 * 1024 * 1024 {
                        return None;
                    }
                    let encoded_data_length = if compact {
                        character_count
                    } else {
                        character_count.checked_mul(2)?
                    };
                    let end = length_end.checked_add(encoded_data_length)?;
                    (end <= limit).then_some(end)
                }
                _ => {
                    let end = start.checked_add(3)?;
                    (end <= limit).then_some(end)
                }
            }
        }
        _ => {
            let end = start.checked_add(3)?;
            (end <= limit).then_some(end)
        }
    }
}

fn parse_exact_net_object_header(
    bytes: &[u8],
    start: usize,
    end: usize,
) -> Option<NetObjectHeader> {
    let mut reader = Reader::new(bytes.get(start..end)?);
    let header = read_net_object_header(&mut reader).ok()?;
    (reader.remaining() == 0).then_some(header)
}

fn find_economy_manager_guide_headers(
    format_version: u32,
    bytes: &[u8],
    object_table: &DataSerializerObjectTable,
) -> Result<Vec<NetObjectHeader>, CrpError> {
    Ok(
        find_economy_manager_guide_header_positions(format_version, bytes, object_table)?
            .into_iter()
            .map(|(_, _, header)| header)
            .collect(),
    )
}

fn find_economy_manager_guide_header_positions(
    format_version: u32,
    bytes: &[u8],
    object_table: &DataSerializerObjectTable,
) -> Result<Vec<(usize, usize, NetObjectHeader)>, CrpError> {
    let expected_count = if format_version >= 116_017 { 5 } else { 2 };
    let mut matches = Vec::new();
    for start in 0..bytes.len() {
        let Some(end) = serialized_net_object_header_end(bytes, start, bytes.len()) else {
            continue;
        };
        let Some(header) = parse_exact_net_object_header(bytes, start, end) else {
            continue;
        };
        if !matches!(
            &header,
            NetObjectHeader::NewType(_) | NetObjectHeader::NewSharedType(_)
        ) {
            continue;
        }
        let type_name = match &header {
            NetObjectHeader::NewType(type_name) => type_name.as_str(),
            NetObjectHeader::NewSharedType(type_index) => object_table
                .shared_types
                .get(usize::from(*type_index))
                .map(String::as_str)
                .unwrap_or_default(),
            NetObjectHeader::Null | NetObjectHeader::Existing(_) => continue,
        };
        if type_name.is_empty() {
            continue;
        }
        if data_serializer_class_name(type_name) == "GenericGuide" {
            matches.push((start, end, header));
        }
    }
    if matches.len() != expected_count {
        return Err(invalid(
            bytes.len(),
            format!(
                "expected {expected_count} version-gated GenericGuide references, found {} at {:?}",
                matches.len(),
                matches
                    .iter()
                    .map(|(offset, _, header)| (*offset, header))
                    .collect::<Vec<_>>()
            ),
        ));
    }
    Ok(matches)
}

fn economy_manager_investment_flags_start(
    format_version: u32,
    bytes: &[u8],
    object_table: &DataSerializerObjectTable,
) -> Result<usize, CrpError> {
    if format_version < 116_009 {
        return Err(invalid(
            0,
            "EconomyManager investment data requires inner version 116009 or newer",
        ));
    }
    let guide_headers =
        find_economy_manager_guide_header_positions(format_version, bytes, object_table)?;
    let guide_end = guide_headers
        .last()
        .map(|(_, end, _)| *end)
        .ok_or_else(|| invalid(bytes.len(), "EconomyManager guide references are missing"))?;
    if format_version >= 116_017 {
        return Ok(guide_end);
    }

    let mut candidates = vec![(
        parse_economy_manager_investment_scalar_prefix(format_version, bytes, guide_end, false)?,
        "DLL reader layout",
    )];
    if format_version == 116_011 {
        // NorthEurope2FINAL (116011) has a bounded, complete investment scope
        // with the month field but without the two monthly-earnings i32s that
        // the supplied DLL reader expects from 116010.
        candidates.push((
            parse_economy_manager_investment_scalar_prefix(format_version, bytes, guide_end, true)?,
            "observed 116011 layout",
        ));
    }

    let mut complete = Vec::new();
    let mut failures = Vec::new();
    for (flags_start, layout_name) in candidates {
        match decode_economy_manager_investment_payload_end(format_version, bytes, flags_start) {
            Ok(scope_end) if scope_end == bytes.len() => {
                complete.push((flags_start, layout_name));
            }
            Ok(scope_end) => failures.push(format!(
                "{layout_name} ended at {scope_end}, body ends at {}",
                bytes.len()
            )),
            Err(error) => failures.push(format!("{layout_name}: {error}")),
        }
    }
    match complete.as_slice() {
        [(flags_start, _)] => Ok(*flags_start),
        [] => Err(invalid(
            bytes.len(),
            format!(
                "no supported EconomyManager investment layout reaches the body boundary ({})",
                failures.join("; ")
            ),
        )),
        _ => Err(invalid(
            guide_end,
            "EconomyManager investment prefix is ambiguous between supported layouts",
        )),
    }
}

fn parse_economy_manager_investment_scalar_prefix(
    format_version: u32,
    bytes: &[u8],
    prefix_start: usize,
    omit_monthly_earnings: bool,
) -> Result<usize, CrpError> {
    let prefix_bytes = bytes.get(prefix_start..).ok_or_else(|| {
        invalid(
            prefix_start,
            "EconomyManager investment scalar prefix starts outside the body",
        )
    })?;
    let mut reader = Reader::new(prefix_bytes);
    if format_version >= 116_000 {
        read_economy_investment_prefix_field(
            &mut reader,
            prefix_start,
            "last-updated day",
            Reader::read_i16_be,
        )?;
    }
    if format_version >= 116_008 {
        let transition_count = read_economy_investment_prefix_count(
            &mut reader,
            prefix_start,
            "transition record count",
        )?;
        for transition_index in 0..transition_count {
            let value_count = read_economy_investment_prefix_count(
                &mut reader,
                prefix_start,
                &format!("transition {transition_index} integer count"),
            )?;
            let value_bytes = value_count
                .checked_mul(4)
                .ok_or(CrpError::ArithmeticOverflow(
                    "EconomyManager investment transition values",
                ))?;
            read_economy_investment_prefix_bytes(
                &mut reader,
                prefix_start,
                value_bytes,
                &format!("transition {transition_index} integer values"),
            )?;

            let float_count = read_economy_investment_prefix_count(
                &mut reader,
                prefix_start,
                &format!("transition {transition_index} float count"),
            )?;
            skip_economy_investment_float_array(
                &mut reader,
                prefix_start,
                float_count,
                &format!("transition {transition_index} float values"),
            )?;
            if format_version >= 116_010 {
                read_economy_investment_prefix_field(
                    &mut reader,
                    prefix_start,
                    &format!("transition {transition_index} trailing integer"),
                    Reader::read_i32_be,
                )?;
                read_economy_investment_prefix_field(
                    &mut reader,
                    prefix_start,
                    &format!("transition {transition_index} trailing unsigned integer"),
                    Reader::read_u32_be,
                )?;
            }
        }
    }
    if format_version >= 116_005 {
        read_economy_investment_prefix_field(
            &mut reader,
            prefix_start,
            "aggregate investment earnings",
            Reader::read_i32_be,
        )?;
    }
    if format_version >= 116_010 {
        read_economy_investment_prefix_field(
            &mut reader,
            prefix_start,
            "last-updated month",
            Reader::read_i16_be,
        )?;
        if !omit_monthly_earnings {
            read_economy_investment_prefix_field(
                &mut reader,
                prefix_start,
                "temporary monthly earnings",
                Reader::read_i32_be,
            )?;
            read_economy_investment_prefix_field(
                &mut reader,
                prefix_start,
                "final monthly earnings",
                Reader::read_i32_be,
            )?;
        }
    }
    if format_version >= 116_014 {
        read_economy_investment_prefix_field(
            &mut reader,
            prefix_start,
            "temporary cash collection",
            Reader::read_i32_be,
        )?;
        read_economy_investment_prefix_field(
            &mut reader,
            prefix_start,
            "final cash collection",
            Reader::read_i32_be,
        )?;
    }
    if format_version >= 116_015 {
        for field_name in ["temporary investment spending", "final investment spending"] {
            read_economy_investment_prefix_field(
                &mut reader,
                prefix_start,
                field_name,
                Reader::read_i32_be,
            )?;
        }
        for field_name in ["last investment buy", "last investment sell"] {
            read_economy_investment_prefix_field(
                &mut reader,
                prefix_start,
                field_name,
                Reader::read_i64_be,
            )?;
        }
        read_economy_investment_prefix_field(
            &mut reader,
            prefix_start,
            "all-in flag",
            Reader::read_u8,
        )?;
    }
    prefix_start
        .checked_add(reader.offset())
        .ok_or(CrpError::ArithmeticOverflow(
            "EconomyManager investment flags start",
        ))
}

fn read_economy_investment_prefix_field<'a, T>(
    reader: &mut Reader<'a>,
    prefix_start: usize,
    field_name: &str,
    read: impl FnOnce(&mut Reader<'a>) -> Result<T, CrpError>,
) -> Result<T, CrpError> {
    let field_offset =
        prefix_start
            .checked_add(reader.offset())
            .ok_or(CrpError::ArithmeticOverflow(
                "EconomyManager investment prefix field offset",
            ))?;
    read(reader).map_err(|error| {
        invalid(
            field_offset,
            format!("EconomyManager investment {field_name}: {error}"),
        )
    })
}

fn read_economy_investment_prefix_count(
    reader: &mut Reader<'_>,
    prefix_start: usize,
    field_name: &str,
) -> Result<usize, CrpError> {
    let field_offset =
        prefix_start
            .checked_add(reader.offset())
            .ok_or(CrpError::ArithmeticOverflow(
                "EconomyManager investment prefix count offset",
            ))?;
    let count = read_economy_investment_prefix_field(
        reader,
        prefix_start,
        field_name,
        Reader::read_i16_be,
    )?;
    if count < 0 {
        return Err(invalid(
            field_offset,
            format!("negative EconomyManager investment {field_name} {count}"),
        ));
    }
    usize::try_from(count).map_err(|_| {
        invalid(
            field_offset,
            format!("{field_name} does not fit this platform"),
        )
    })
}

fn read_economy_investment_prefix_bytes(
    reader: &mut Reader<'_>,
    prefix_start: usize,
    count: usize,
    field_name: &str,
) -> Result<(), CrpError> {
    let field_offset =
        prefix_start
            .checked_add(reader.offset())
            .ok_or(CrpError::ArithmeticOverflow(
                "EconomyManager investment prefix data offset",
            ))?;
    reader.read_bytes(count).map(|_| ()).map_err(|error| {
        invalid(
            field_offset,
            format!("EconomyManager investment {field_name}: {error}"),
        )
    })
}

fn skip_economy_investment_float_array(
    reader: &mut Reader<'_>,
    prefix_start: usize,
    count: usize,
    field_name: &str,
) -> Result<(), CrpError> {
    let mut remaining = count;
    while remaining > 0 {
        let control_offset =
            prefix_start
                .checked_add(reader.offset())
                .ok_or(CrpError::ArithmeticOverflow(
                    "EconomyManager transition float control offset",
                ))?;
        let control = reader.read_u8().map_err(|error| {
            invalid(
                control_offset,
                format!("EconomyManager investment {field_name}: {error}"),
            )
        })?;
        let run = if control >= 0x80 {
            usize::from(control - 0x7f)
        } else {
            usize::from(control) + 1
        };
        let used = run.min(remaining);
        if control < 0x80 {
            let literal_bytes = used.checked_mul(4).ok_or(CrpError::ArithmeticOverflow(
                "EconomyManager transition float bytes",
            ))?;
            read_economy_investment_prefix_bytes(reader, prefix_start, literal_bytes, field_name)?;
        }
        remaining -= used;
    }
    Ok(())
}

fn decode_economy_manager_investment_payload_end(
    format_version: u32,
    bytes: &[u8],
    flags_start: usize,
) -> Result<usize, CrpError> {
    let (flags, values_start) = decode_economy_manager_investment_flags_at(bytes, flags_start)?;
    let (series, values_end) =
        decode_economy_manager_investment_value_series_at(bytes, flags, values_start)?;
    let (investments, prefab_start) =
        decode_economy_manager_investments_after_series(format_version, bytes, series, values_end)?;
    decode_economy_manager_investment_prefabs_after_investments(bytes, investments, prefab_start)
        .map(|(_, scope_end)| scope_end)
}

fn decode_economy_manager_investment_flags_from_body(
    format_version: u32,
    bytes: &[u8],
    object_table: &DataSerializerObjectTable,
) -> Result<(Vec<u16>, usize), CrpError> {
    let flags_start = economy_manager_investment_flags_start(format_version, bytes, object_table)?;
    decode_economy_manager_investment_flags_at(bytes, flags_start)
}

fn decode_economy_manager_investment_flags_at(
    bytes: &[u8],
    flags_start: usize,
) -> Result<(Vec<u16>, usize), CrpError> {
    let flag_bytes = bytes.get(flags_start..).ok_or_else(|| {
        invalid(
            flags_start,
            "investment flag scope starts outside EconomyManager",
        )
    })?;
    let mut reader = Reader::new(flag_bytes);
    let flags = read_encoded_ushort_array(&mut reader, ECONOMY_INVESTMENT_FLAG_COUNT)?;
    let values_start =
        flags_start
            .checked_add(reader.offset())
            .ok_or(CrpError::ArithmeticOverflow(
                "EconomyManager investment values start",
            ))?;
    Ok((flags, values_start))
}

fn decode_economy_manager_investment_value_series(
    format_version: u32,
    bytes: &[u8],
    object_table: &DataSerializerObjectTable,
) -> Result<(Vec<EconomyInvestmentValueSeries>, usize), CrpError> {
    let flags_start = economy_manager_investment_flags_start(format_version, bytes, object_table)?;
    let (flags, values_start) = decode_economy_manager_investment_flags_at(bytes, flags_start)?;
    decode_economy_manager_investment_value_series_at(bytes, flags, values_start)
}

fn decode_economy_manager_investment_value_series_at(
    bytes: &[u8],
    flags: Vec<u16>,
    values_start: usize,
) -> Result<(Vec<EconomyInvestmentValueSeries>, usize), CrpError> {
    let value_bytes = bytes.get(values_start..).ok_or_else(|| {
        invalid(
            values_start,
            "investment values start outside EconomyManager",
        )
    })?;
    let mut reader = Reader::new(value_bytes);
    let mut series = Vec::new();
    for (offset, flag) in flags.into_iter().enumerate() {
        if flag == 0 {
            continue;
        }
        let investment_index = u16::try_from(offset + 1)
            .map_err(|_| CrpError::ArithmeticOverflow("EconomyManager investment index"))?;
        let count_offset =
            values_start
                .checked_add(reader.offset())
                .ok_or(CrpError::ArithmeticOverflow(
                    "EconomyManager investment value-count offset",
                ))?;
        let count = reader.read_i16_be().map_err(|error| {
            invalid(
                count_offset,
                format!("investment {investment_index} value count: {error}"),
            )
        })?;
        if count < 0 {
            return Err(invalid(
                count_offset,
                format!("negative investment value count {count}"),
            ));
        }
        let count = usize::try_from(count)
            .map_err(|_| reader.invalid("investment value count does not fit this platform"))?;
        let value_offset =
            values_start
                .checked_add(reader.offset())
                .ok_or(CrpError::ArithmeticOverflow(
                    "EconomyManager investment values offset",
                ))?;
        let values = read_encoded_float_array(&mut reader, count).map_err(|error| {
            invalid(
                value_offset,
                format!("investment {investment_index} values: {error}"),
            )
        })?;
        series.push(EconomyInvestmentValueSeries {
            investment_index,
            values,
        });
    }
    let values_end =
        values_start
            .checked_add(reader.offset())
            .ok_or(CrpError::ArithmeticOverflow(
                "EconomyManager investment values end",
            ))?;
    Ok((series, values_end))
}

fn decode_economy_manager_investments(
    format_version: u32,
    bytes: &[u8],
    object_table: &DataSerializerObjectTable,
) -> Result<(Vec<EconomyInvestmentData>, usize), CrpError> {
    let flags_start = economy_manager_investment_flags_start(format_version, bytes, object_table)?;
    let (flags, values_start) = decode_economy_manager_investment_flags_at(bytes, flags_start)?;
    let (series, values_end) =
        decode_economy_manager_investment_value_series_at(bytes, flags, values_start)?;
    decode_economy_manager_investments_after_series(format_version, bytes, series, values_end)
}

fn decode_economy_manager_investments_after_series(
    format_version: u32,
    bytes: &[u8],
    series: Vec<EconomyInvestmentValueSeries>,
    values_end: usize,
) -> Result<(Vec<EconomyInvestmentData>, usize), CrpError> {
    let field_bytes = bytes.get(values_end..).ok_or_else(|| {
        invalid(
            values_end,
            "investment numeric fields start outside EconomyManager",
        )
    })?;
    let mut reader = Reader::new(field_bytes);
    let mut investments = series
        .into_iter()
        .map(|entry| EconomyInvestmentData {
            investment_index: entry.investment_index,
            value_history: entry.values,
            player_spent: 0,
            owned_share_count: 0,
            base_value: 0.0,
            requirement_missing_date_ticks: 0,
            current_state: 0,
            random_influence: 0.0,
        })
        .collect::<Vec<_>>();

    if format_version >= 116_009 {
        for investment in &mut investments {
            investment.player_spent = read_economy_investment_field(
                &mut reader,
                values_end,
                investment.investment_index,
                "player-spent amount",
                Reader::read_i32_be,
            )?;
            investment.owned_share_count = read_economy_investment_field(
                &mut reader,
                values_end,
                investment.investment_index,
                "owned-share count",
                Reader::read_u32_be,
            )?;
            investment.base_value = read_economy_investment_field(
                &mut reader,
                values_end,
                investment.investment_index,
                "base value",
                Reader::read_f32_be,
            )?;
        }
    }
    if format_version >= 116_010 {
        for investment in &mut investments {
            investment.requirement_missing_date_ticks = read_economy_investment_field(
                &mut reader,
                values_end,
                investment.investment_index,
                "requirement-missing date",
                Reader::read_i64_be,
            )?;
        }
    }
    if format_version >= 116_012 {
        for investment in &mut investments {
            investment.current_state = read_economy_investment_field(
                &mut reader,
                values_end,
                investment.investment_index,
                "current state",
                Reader::read_i16_be,
            )?;
            investment.random_influence = read_economy_investment_field(
                &mut reader,
                values_end,
                investment.investment_index,
                "random influence",
                Reader::read_f32_be,
            )?;
        }
    }
    let fields_end =
        values_end
            .checked_add(reader.offset())
            .ok_or(CrpError::ArithmeticOverflow(
                "EconomyManager investment fields end",
            ))?;
    Ok((investments, fields_end))
}

fn decode_economy_manager_investment_prefabs_from_body(
    format_version: u32,
    bytes: &[u8],
    object_table: &DataSerializerObjectTable,
) -> Result<(EconomyInvestmentPrefabTable, usize), CrpError> {
    let flags_start = economy_manager_investment_flags_start(format_version, bytes, object_table)?;
    let (flags, values_start) = decode_economy_manager_investment_flags_at(bytes, flags_start)?;
    let (series, values_end) =
        decode_economy_manager_investment_value_series_at(bytes, flags, values_start)?;
    let (investments, prefab_start) =
        decode_economy_manager_investments_after_series(format_version, bytes, series, values_end)?;
    decode_economy_manager_investment_prefabs_after_investments(bytes, investments, prefab_start)
}

fn decode_economy_manager_investment_prefabs_after_investments(
    bytes: &[u8],
    investments: Vec<EconomyInvestmentData>,
    prefab_start: usize,
) -> Result<(EconomyInvestmentPrefabTable, usize), CrpError> {
    let prefab_bytes = bytes.get(prefab_start..).ok_or_else(|| {
        invalid(
            prefab_start,
            "investment prefab scope starts outside EconomyManager",
        )
    })?;
    let mut reader = Reader::new(prefab_bytes);
    let prefab_indices = read_encoded_ushort_array(&mut reader, investments.len())
        .map_err(|error| invalid(prefab_start, format!("investment prefab indices: {error}")))?;
    let name_count_offset =
        prefab_start
            .checked_add(reader.offset())
            .ok_or(CrpError::ArithmeticOverflow(
                "EconomyManager investment prefab-name count offset",
            ))?;
    let name_count = usize::from(reader.read_u16_be().map_err(|error| {
        invalid(
            name_count_offset,
            format!("investment prefab-name count: {error}"),
        )
    })?);
    if name_count > reader.remaining() {
        return Err(invalid(
            name_count_offset,
            format!(
                "investment prefab-name count {name_count} cannot fit in {} remaining bytes",
                reader.remaining()
            ),
        ));
    }
    let mut names = Vec::with_capacity(name_count);
    for name_index in 0..name_count {
        let name_offset =
            prefab_start
                .checked_add(reader.offset())
                .ok_or(CrpError::ArithmeticOverflow(
                    "EconomyManager investment prefab-name offset",
                ))?;
        names.push(reader.read_unique_string().map_err(|error| {
            invalid(
                name_offset,
                format!("investment prefab name {name_index}: {error}"),
            )
        })?);
    }
    let assignments = investments
        .iter()
        .zip(prefab_indices)
        .map(
            |(investment, prefab_index)| EconomyInvestmentPrefabAssignment {
                investment_index: investment.investment_index,
                prefab_index,
            },
        )
        .collect();
    let scope_end =
        prefab_start
            .checked_add(reader.offset())
            .ok_or(CrpError::ArithmeticOverflow(
                "EconomyManager investment prefab scope end",
            ))?;
    Ok((
        EconomyInvestmentPrefabTable { assignments, names },
        scope_end,
    ))
}

fn read_economy_investment_field<'a, T>(
    reader: &mut Reader<'a>,
    fields_start: usize,
    investment_index: u16,
    field_name: &str,
    read: impl FnOnce(&mut Reader<'a>) -> Result<T, CrpError>,
) -> Result<T, CrpError> {
    let offset = fields_start
        .checked_add(reader.offset())
        .ok_or(CrpError::ArithmeticOverflow(
            "EconomyManager investment field offset",
        ))?;
    read(reader).map_err(|error| {
        invalid(
            offset,
            format!("investment {investment_index} {field_name}: {error}"),
        )
    })
}

fn data_serializer_class_name(type_name: &str) -> &str {
    type_name
        .split(',')
        .next()
        .unwrap_or(type_name)
        .trim()
        .rsplit(['+', '.'])
        .next()
        .unwrap_or(type_name)
}

fn find_trailing_object_headers(
    bytes: &[u8],
    header_count: usize,
    object_table: &DataSerializerObjectTable,
) -> Result<Vec<NetObjectHeader>, CrpError> {
    if header_count == 0 {
        return Ok(Vec::new());
    }
    if header_count > 2 {
        return Err(invalid(
            bytes.len(),
            "EconomyManager has more trailing object references than this reader supports",
        ));
    }

    let mut trailing_headers = Vec::new();
    for start in 0..bytes.len() {
        if serialized_net_object_header_end(bytes, start, bytes.len()) == Some(bytes.len())
            && let Some(header) = parse_exact_net_object_header(bytes, start, bytes.len())
        {
            trailing_headers.push((start, header));
            if trailing_headers.len() > MAX_TRAILING_HEADER_CANDIDATES {
                return Err(invalid(
                    start,
                    "EconomyManager trailing reference boundary is too ambiguous",
                ));
            }
        }
    }

    if header_count == 1 {
        let mut valid = trailing_headers.into_iter().filter_map(|(start, header)| {
            let mut table = object_table.clone();
            table.register(&header).ok().map(|_| (start, header))
        });
        let Some((_, first)) = valid.next() else {
            return Err(invalid(
                bytes.len(),
                "EconomyManager trailing object reference is missing or invalid",
            ));
        };
        if valid.next().is_some() {
            return Err(invalid(
                bytes.len(),
                "EconomyManager trailing object reference is ambiguous",
            ));
        }
        return Ok(vec![first]);
    }

    let prior_ends = trailing_headers
        .iter()
        .map(|(start, _)| *start)
        .collect::<HashSet<_>>();
    let mut prior_headers = HashMap::<usize, Vec<(usize, NetObjectHeader)>>::new();
    for start in 0..bytes.len() {
        if let Some(end) = serialized_net_object_header_end(bytes, start, bytes.len())
            && prior_ends.contains(&end)
            && let Some(header) = parse_exact_net_object_header(bytes, start, end)
        {
            let candidates = prior_headers.entry(end).or_default();
            candidates.push((start, header));
            if candidates.len() > MAX_TRAILING_HEADER_CANDIDATES {
                return Err(invalid(
                    end,
                    "EconomyManager trailing reference boundary is too ambiguous",
                ));
            }
        }
    }

    let mut valid_pairs = Vec::new();
    for (second_start, second) in trailing_headers {
        for (first_start, first) in prior_headers.get(&second_start).into_iter().flatten() {
            let mut table = object_table.clone();
            if table.register(first).is_ok() && table.register(&second).is_ok() {
                valid_pairs.push((*first_start, second_start, first.clone(), second.clone()));
                if valid_pairs.len() > 1 {
                    return Err(invalid(
                        bytes.len(),
                        "EconomyManager trailing object references are ambiguous",
                    ));
                }
            }
        }
    }
    match valid_pairs.pop() {
        Some((_, _, first, second)) => Ok(vec![first, second]),
        None => Err(invalid(
            bytes.len(),
            "EconomyManager trailing object references are missing or invalid",
        )),
    }
}

fn transport_manager_object_header_count(format_version: u32) -> usize {
    if format_version >= 318 {
        7
    } else if format_version >= 317 {
        3
    } else if format_version >= 316 {
        2
    } else {
        0
    }
}

fn transport_manager_line_number_count(format_version: u32) -> usize {
    if format_version >= 113_008 {
        17
    } else if format_version >= 113_000 {
        16
    } else if format_version >= 111_006 {
        14
    } else if format_version >= 110_021 {
        13
    } else if format_version >= 110_000 {
        12
    } else if format_version >= 310 {
        10
    } else if format_version >= 211 {
        8
    } else if format_version >= 122 {
        5
    } else {
        0
    }
}

fn decode_transport_manager_reference_body(
    format_version: u32,
    bytes: &[u8],
    object_table: &DataSerializerObjectTable,
) -> Result<Vec<NetObjectHeader>, CrpError> {
    if format_version < 164 {
        return Err(invalid(
            0,
            "TransportManager reference resolution requires inner version 164 or newer",
        ));
    }
    let mut reader = Reader::new(bytes);
    let line_flags = read_encoded_uint_array(&mut reader, 255)?;
    if line_flags.iter().any(|flag| *flag != 0) {
        return Err(invalid(
            reader.offset(),
            "TransportManager queue resolution currently requires an empty line buffer",
        ));
    }
    let milestone_count = if format_version >= 151 {
        usize::from(reader.read_u16_be()?)
    } else {
        0
    };
    if milestone_count != 0 {
        let count_offset = reader
            .offset()
            .checked_sub(2)
            .ok_or(CrpError::ArithmeticOverflow(
                "TransportManager milestone count offset",
            ))?;
        return Err(invalid(
            count_offset,
            "TransportManager queue resolution currently requires an empty milestone list",
        ));
    }

    let prefab_name_count = usize::from(reader.read_u16_be()?);
    for _ in 0..prefab_name_count {
        reader.read_unique_string()?;
    }

    let line_number_count = transport_manager_line_number_count(format_version);
    if format_version >= 153 {
        for _ in 0..line_number_count {
            reader.read_u16_be()?;
        }
    }
    if format_version >= 122 {
        let passenger_group_count = if format_version >= 176 { 8_usize } else { 7 };
        let passenger_record_count = line_number_count;
        let passenger_bytes = passenger_record_count
            .checked_mul(passenger_group_count)
            .and_then(|groups| groups.checked_mul(9))
            .ok_or(CrpError::ArithmeticOverflow(
                "TransportManager passenger record length",
            ))?;
        reader.read_bytes(passenger_bytes)?;
    }

    let header_count = transport_manager_object_header_count(format_version);
    let mut headers = Vec::with_capacity(header_count);
    for _ in 0..header_count {
        headers.push(read_net_object_header(&mut reader)?);
    }
    if format_version >= 114_001 {
        let selection_count = reader.read_i16_be()?;
        if selection_count < 0 {
            let count_offset =
                reader
                    .offset()
                    .checked_sub(2)
                    .ok_or(CrpError::ArithmeticOverflow(
                        "TransportManager selection-count offset",
                    ))?;
            return Err(invalid(
                count_offset,
                format!("negative TransportManager selection count {selection_count}"),
            ));
        }
        let selection_count = usize::try_from(selection_count)
            .map_err(|_| reader.invalid("selection count does not fit this platform"))?;
        if selection_count > reader.remaining() / 4 {
            return Err(
                reader.invalid("TransportManager selection count cannot fit in remaining bytes")
            );
        }
        for _ in 0..selection_count {
            reader.read_i16_be()?;
            skip_shared_string_value(&mut reader)?;
        }
    }
    if reader.remaining() != 0 {
        return Err(invalid(
            reader.offset(),
            "TransportManager reference fields do not end at the manager boundary",
        ));
    }
    let mut checked_table = object_table.clone();
    for header in &headers {
        checked_table.register(header)?;
    }
    Ok(headers)
}

fn skip_shared_string_value(reader: &mut Reader<'_>) -> Result<(), CrpError> {
    // Earlier managers seed this shared-string table; their values are opaque
    // here, so consume the wire form without resolving an existing table index.
    match reader.read_u8()? {
        0xfe => {
            reader.read_unique_string()?;
            Ok(())
        }
        0xff => Err(reader.invalid("shared-string reference uses a reserved marker")),
        _ => {
            reader.read_u8()?;
            Ok(())
        }
    }
}

fn register_raw_queued_reference(
    bytes: &[u8],
    object_table: &mut DataSerializerObjectTable,
    queued_objects: &mut Vec<QueuedGuideObject>,
    description: &str,
) -> Result<(), CrpError> {
    if bytes.is_empty() {
        return Ok(());
    }
    let mut reader = Reader::new(bytes);
    let header = read_net_object_header(&mut reader)?;
    if reader.remaining() != 0 {
        return Err(invalid(
            reader.offset(),
            format!("{description} has trailing bytes"),
        ));
    }
    register_queued_reference(&header, object_table, queued_objects)?;
    Ok(())
}

fn register_queued_reference(
    header: &NetObjectHeader,
    object_table: &mut DataSerializerObjectTable,
    queued_objects: &mut Vec<QueuedGuideObject>,
) -> Result<Option<u32>, CrpError> {
    let previous_object_count = object_table.object_types.len();
    let object_id = object_table.register(header)?;
    if let Some(object_id) = object_id
        && usize::try_from(object_id)
            .ok()
            .is_some_and(|index| index >= previous_object_count)
    {
        let type_name = object_table
            .type_name(object_id)
            .ok_or_else(|| invalid(0, "new object is missing from the object-list table"))?
            .to_owned();
        queued_objects.push(QueuedGuideObject {
            object_id,
            type_name,
        });
    }
    Ok(object_id)
}

fn register_reference_headers(
    headers: &[NetObjectHeader],
    object_table: &mut DataSerializerObjectTable,
    queued_objects: &mut Vec<QueuedGuideObject>,
) -> Result<Vec<Option<u32>>, CrpError> {
    headers
        .iter()
        .map(|header| register_queued_reference(header, object_table, queued_objects))
        .collect()
}

fn locate_queued_object_body(
    graph: &[u8],
    start_offset: usize,
    queued_objects: &[QueuedGuideObject],
    target_object_id: u32,
    format_version: u32,
    save_app_version: Option<u32>,
) -> Result<GuideBodyLocation, CrpError> {
    let mut locations = locate_queued_object_bodies(
        graph,
        start_offset,
        queued_objects,
        &[Some(target_object_id)],
        format_version,
        save_app_version,
    )?;
    locations.pop().flatten().ok_or_else(|| {
        invalid(
            start_offset,
            format!("queued object ID {target_object_id} has no body"),
        )
    })
}

fn locate_queued_object_bodies(
    graph: &[u8],
    start_offset: usize,
    queued_objects: &[QueuedGuideObject],
    target_object_ids: &[Option<u32>],
    format_version: u32,
    save_app_version: Option<u32>,
) -> Result<Vec<Option<GuideBodyLocation>>, CrpError> {
    let mut target_indices = HashMap::<u32, Vec<usize>>::new();
    let mut last_target_id = None;
    for (index, object_id) in target_object_ids.iter().enumerate() {
        if let Some(object_id) = object_id {
            target_indices.entry(*object_id).or_default().push(index);
            last_target_id =
                Some(last_target_id.map_or(*object_id, |previous: u32| previous.max(*object_id)));
        }
    }
    let Some(last_target_id) = last_target_id else {
        return Ok(vec![None; target_object_ids.len()]);
    };

    let mut locations = vec![None; target_object_ids.len()];
    let mut graph_offset = start_offset;
    for object in queued_objects {
        if object.object_id > last_target_id {
            break;
        }
        let body_length = guide_object_body_length(
            &object.type_name,
            format_version,
            save_app_version,
            graph.get(graph_offset..).ok_or_else(|| {
                invalid(
                    graph_offset,
                    "queued guide body starts outside the manager graph",
                )
            })?,
            graph_offset,
        )?;
        if let Some(indices) = target_indices.get(&object.object_id) {
            let location = GuideBodyLocation {
                object_id: object.object_id,
                graph_offset,
                body_length,
                type_name: object.type_name.clone(),
            };
            for index in indices {
                locations[*index] = Some(location.clone());
            }
        }
        graph_offset = graph_offset
            .checked_add(body_length)
            .ok_or(CrpError::ArithmeticOverflow("queued guide body offset"))?;
    }

    for (index, object_id) in target_object_ids.iter().enumerate() {
        if let Some(object_id) = object_id
            && locations[index].is_none()
        {
            return Err(invalid(
                start_offset,
                format!("queued object ID {object_id} has no body"),
            ));
        }
    }
    Ok(locations)
}

fn guide_object_body_length(
    type_name: &str,
    format_version: u32,
    save_app_version: Option<u32>,
    bytes: &[u8],
    graph_offset: usize,
) -> Result<usize, CrpError> {
    match object_class_name(type_name) {
        "GenericGuide" | "ServiceTypeGuide" | "BuildingTypeGuide" | "ZoneTypeGuide" => {
            GuideTriggerBasePrefix::decode(bytes).map(|(_, consumed)| consumed)
        }
        "NetSegmentInstanceGuide" | "BuildingInstanceGuide" | "BuildingOnFireGuide" => {
            GuideInstanceBody::decode(bytes).map(|(_, consumed)| consumed)
        }
        "NetNodeInstanceGuide" => {
            NetNodeInstanceGuideBody::decode(format_version, save_app_version, bytes)
                .map(|(_, consumed)| consumed)
        }
        _ => Err(invalid(
            graph_offset,
            format!(
                "cannot locate target guide because queued type {type_name} has no known body schema"
            ),
        )),
    }
}

fn object_class_name(type_name: &str) -> &str {
    type_name
        .split(',')
        .next()
        .unwrap_or(type_name)
        .trim()
        .rsplit('.')
        .next()
        .unwrap_or(type_name)
}

fn unique_manager_slot(
    manager_objects: Option<&[Option<ManagerObjectReference>]>,
    expected_type: &str,
) -> Result<usize, CrpError> {
    let managers = manager_objects.ok_or_else(|| invalid(0, "manager object array is null"))?;
    let slots = managers
        .iter()
        .enumerate()
        .filter_map(|(slot, reference)| {
            reference
                .as_ref()
                .is_some_and(|reference| reference.type_name.starts_with(expected_type))
                .then_some(slot)
        })
        .collect::<Vec<_>>();
    if slots.len() != 1 {
        return Err(invalid(
            0,
            format!("manager array must contain exactly one {expected_type} reference"),
        ));
    }
    Ok(slots[0])
}

fn expect_manager_type(
    manager_objects: Option<&[Option<ManagerObjectReference>]>,
    index: usize,
    expected: &str,
) -> Result<(), CrpError> {
    let Some(Some(reference)) = manager_objects.and_then(|objects| objects.get(index)) else {
        return Err(invalid(
            index,
            format!("missing manager reference for {expected}"),
        ));
    };
    if !reference.type_name.starts_with(expected) {
        return Err(invalid(
            index,
            format!(
                "manager slot {index} is {}, expected {expected}",
                reference.type_name
            ),
        ));
    }
    Ok(())
}

fn new_profiler_tag_bytes(name: &str) -> Result<Vec<u8>, CrpError> {
    let mut bytes = vec![0xfe];
    write_unique_string(&mut bytes, name)?;
    Ok(bytes)
}

fn find_unique_bytes(haystack: &[u8], needle: &[u8], description: &str) -> Result<usize, CrpError> {
    let mut matches = haystack
        .windows(needle.len())
        .enumerate()
        .filter_map(|(offset, window)| (window == needle).then_some(offset));
    let Some(offset) = matches.next() else {
        return Err(invalid(
            0,
            format!("{description} not found in manager graph"),
        ));
    };
    if matches.next().is_some() {
        return Err(invalid(
            offset,
            format!("{description} is ambiguous in manager graph"),
        ));
    }
    Ok(offset)
}

#[cfg(test)]
mod manager_boundary_tests {
    use super::{
        DataSerializerObjectTable, EconomyInvestmentData, EconomyInvestmentPrefabAssignment,
        EconomyInvestmentPrefabTable, GUIDE_TRIGGER_BASE_PREFIX_LEN, ManagerObjectReference,
        QueuedGuideObject, SimulationManagerData,
        decode_economy_manager_investment_flags_from_body,
        decode_economy_manager_investment_prefabs_from_body,
        decode_economy_manager_investment_value_series, decode_economy_manager_investments,
        decode_transport_manager_reference_body, economy_manager_object_header_count,
        find_economy_manager_guide_headers, find_trailing_object_headers,
        locate_queued_object_bodies, locate_queued_object_body, register_reference_headers,
        transport_manager_line_number_count, transport_manager_object_header_count,
    };
    use crate::data_serializer::write_unique_string;
    use crate::district_manager_data::{
        DistrictParkIndustryResource, DistrictParkIndustryResourceKind,
        DistrictParkIndustryTradeData,
    };
    use crate::encoded_array::{
        write_encoded_bool_array, write_encoded_uint_array, write_encoded_ushort_array,
    };
    use crate::net_manager_data::NetObjectHeader;
    use crate::{
        GuideInstanceBody, GuideTriggerBasePrefix, StatisticInt32Data, StatisticInt64Data,
    };

    fn manager_with_graph(graph: Vec<u8>) -> SimulationManagerData {
        SimulationManagerData {
            format_version: 112_004,
            current_tick_index: 0,
            current_frame_index: 0,
            current_build_index: 0,
            randomizer_seed: 0,
            day_time_offset_frames: None,
            build_index_history: vec![0; 32],
            serializable_data_storage: None,
            root_fields_len: 0,
            root_profile_begin_bytes: Vec::new(),
            root_profile_end_bytes: Vec::new(),
            manager_objects: None,
            manager_array_bytes: Vec::new(),
            serialized_object_graph: graph,
        }
    }

    fn synthetic_manager_data() -> SimulationManagerData {
        let mut graph = profiler_tag("ZoneManager");
        write_encoded_uint_array(&mut graph, &vec![0; 49_151]);
        graph.extend_from_slice(&[0; 3 + 3 + 16]);
        graph.extend_from_slice(&[0xff; 13]);
        graph.extend_from_slice(&[0, 14]);
        graph.extend(profiler_tag("PropManager"));
        write_encoded_ushort_array(&mut graph, &vec![0; 65_535]);
        graph.extend_from_slice(&[0, 0]); // Empty PropInfo name table.
        graph.extend_from_slice(&[0, 15]);
        graph.extend(profiler_tag("TreeManager"));
        write_encoded_ushort_array(&mut graph, &vec![0; 262_143]);
        graph.extend_from_slice(&[0, 0]); // Empty TreeInfo name table.
        graph.extend_from_slice(&[0, 0, 0]); // Empty burning-tree list.
        graph.extend_from_slice(&[0, 16]);
        graph.extend(profiler_tag("WeatherManager"));
        graph.extend_from_slice(&[0, 20]);
        graph.extend(profiler_tag("DistrictManager"));
        write_unique_string(&mut graph, "Airline").expect("short test name is encodable");
        graph.push(1);
        write_encoded_uint_array(&mut graph, &[0; 128]);
        write_encoded_uint_array(&mut graph, &[0; 128]);
        write_encoded_bool_array(&mut graph, &vec![false; 16 * 32 * 128]);

        let names = [
            (13, "ZoneManager+Data"),
            (14, "PropManager+Data"),
            (15, "TreeManager+Data"),
            (16, "WeatherManager+Data"),
            (20, "DistrictManager+Data"),
        ];
        let mut manager_objects = vec![None; 21];
        for (slot, type_name) in names {
            manager_objects[slot] = Some(ManagerObjectReference {
                object_id: slot as u32,
                type_name: format!("{type_name}, Assembly-CSharp"),
            });
        }

        SimulationManagerData {
            format_version: 116_020,
            current_tick_index: 0,
            current_frame_index: 0,
            current_build_index: 0,
            randomizer_seed: 0,
            day_time_offset_frames: None,
            build_index_history: vec![0; 32],
            serializable_data_storage: None,
            root_fields_len: 0,
            root_profile_begin_bytes: Vec::new(),
            root_profile_end_bytes: Vec::new(),
            manager_objects: Some(manager_objects),
            manager_array_bytes: Vec::new(),
            serialized_object_graph: graph,
        }
    }

    fn profiler_tag(name: &str) -> Vec<u8> {
        let mut bytes = vec![0xfe];
        write_unique_string(&mut bytes, name).expect("short test tag is encodable");
        bytes
    }

    #[test]
    fn guide_manager_reference_decoder_exposes_order_and_preserves_body_bytes() {
        let mut graph = profiler_tag("GuideManager");
        for _ in 0..48 {
            graph.extend_from_slice(&[0xfe, 0x00, 0x24]);
        }
        graph.extend_from_slice(&[0x00, 0x01]);

        let mut manager = manager_with_graph(graph.clone());
        manager.manager_objects = Some(vec![Some(ManagerObjectReference {
            object_id: 1,
            type_name: "GuideManager+Data, Assembly-CSharp".to_owned(),
        })]);
        let references = manager.decode_guide_manager_references().unwrap();
        assert_eq!(references.format_version(), 112_004);
        assert_eq!(references.references().len(), 48);
        assert_eq!(
            references.references()[0].field(),
            crate::GuideManagerReferenceField::ServiceNotUsed(0)
        );
        assert_eq!(
            references.references()[1].field(),
            crate::GuideManagerReferenceField::ServiceNeeded(0)
        );
        assert_eq!(
            references.references()[0].header(),
            &NetObjectHeader::NewSharedType(36)
        );
        assert_eq!(references.profile_end_shared_string_index(), 1);
        assert_eq!(
            references.encode(),
            graph[profiler_tag("GuideManager").len()..]
        );

        manager.serialized_object_graph.pop();
        assert!(manager.decode_guide_manager_references().is_err());
    }

    #[test]
    fn guide_manager_preserves_game_profile_index_without_resolving_it() {
        let fixture =
            include_bytes!("../tests/fixtures/guide-manager-references-v114009-ap-hanami.bin");
        let mut manager = manager_with_graph(fixture.to_vec());
        manager.format_version = 114_009;
        manager.manager_objects = Some(vec![Some(ManagerObjectReference {
            object_id: 1,
            type_name: "GuideManager+Data, Assembly-CSharp".to_owned(),
        })]);

        let guide = manager.decode_guide_manager_references().unwrap();
        assert_eq!(guide.format_version(), 114_009);
        assert_eq!(guide.references().len(), 54);
        assert_eq!(guide.profile_end_shared_string_index(), 33);
        assert_eq!(
            guide.encode(),
            fixture[profiler_tag("GuideManager").len()..]
        );
    }

    #[test]
    fn statistic_replacements_splice_edited_histories_and_validate_source() {
        const INT32: &[u8] =
            include_bytes!("../tests/fixtures/statistic-int32-v112004-wolf-creek.bin");
        const INT64: &[u8] =
            include_bytes!("../tests/fixtures/statistic-int64-v112004-wolf-creek.bin");
        let prefix = [0xa1, 0xb2, 0xc3];
        let suffix = [0xd4, 0xe5];

        let mut graph = prefix.to_vec();
        graph.extend_from_slice(INT32);
        graph.extend_from_slice(INT64);
        graph.extend_from_slice(&suffix);
        let int32_offset = prefix.len();
        let (int32, int32_len) =
            StatisticInt32Data::decode(112_004, &graph[int32_offset..]).unwrap();
        assert_eq!(int32_len, INT32.len());
        let int64_offset = int32_offset + int32_len;
        let (int64, int64_len) =
            StatisticInt64Data::decode(112_004, &graph[int64_offset..]).unwrap();
        assert_eq!(int64_len, INT64.len());

        let mut int32_editor = int32.into_editor();
        int32_editor.history_mut().fill(0);
        let edited_int32 = int32_editor.encode();
        assert_ne!(edited_int32.len(), INT32.len());

        let mut manager = manager_with_graph(graph.clone());
        manager
            .replace_statistic_int32_at(int32_offset, &int32_editor)
            .unwrap();
        assert_eq!(&manager.serialized_object_graph[..prefix.len()], &prefix);
        assert_eq!(
            &manager.serialized_object_graph[int32_offset..int32_offset + edited_int32.len()],
            edited_int32
        );
        let int64_offset_after_edit = int32_offset + edited_int32.len();
        assert_eq!(
            &manager.serialized_object_graph
                [int64_offset_after_edit..int64_offset_after_edit + INT64.len()],
            INT64
        );
        assert_eq!(
            &manager.serialized_object_graph
                [manager.serialized_object_graph.len() - suffix.len()..],
            &suffix
        );

        let mut int64_editor = int64.into_editor();
        *int64_editor.latest_value_mut() = -17;
        int64_editor.history_mut().fill(0);
        let edited_int64 = int64_editor.encode();
        assert_ne!(edited_int64.len(), INT64.len());
        manager
            .replace_statistic_int64_at(int64_offset_after_edit, &int64_editor)
            .unwrap();
        let int64_body_start = int64_offset_after_edit;
        assert_eq!(
            &manager.serialized_object_graph
                [int64_body_start..int64_body_start + edited_int64.len()],
            edited_int64
        );
        assert_eq!(
            &manager.serialized_object_graph
                [manager.serialized_object_graph.len() - suffix.len()..],
            &suffix
        );
        let (rewritten_int64, rewritten_int64_len) = StatisticInt64Data::decode(
            112_004,
            &manager.serialized_object_graph[int64_body_start..],
        )
        .unwrap();
        assert_eq!(rewritten_int64_len, edited_int64.len());
        assert_eq!(rewritten_int64.latest_value(), -17);
        assert_eq!(rewritten_int64.history(), &[0; 256]);

        let mut wrong_version = manager.clone();
        wrong_version.format_version = 112_005;
        assert!(
            wrong_version
                .replace_statistic_int64_at(int64_body_start, &int64_editor)
                .is_err()
        );

        let mut stale = manager.clone();
        stale.serialized_object_graph[int64_body_start] ^= 1;
        assert!(
            stale
                .replace_statistic_int64_at(int64_body_start, &int64_editor)
                .is_err()
        );
        assert!(
            manager
                .replace_statistic_int64_at(manager.serialized_object_graph.len(), &int64_editor)
                .is_err()
        );
    }

    #[test]
    fn statistics_manager_preserves_game_profile_index_without_resolving_it() {
        let fixture = include_bytes!("../tests/fixtures/statistics-manager-v114009-ap-hanami.bin");
        let mut manager = manager_with_graph(fixture.to_vec());
        manager.format_version = 114_009;
        manager.manager_objects = Some(vec![Some(ManagerObjectReference {
            object_id: 1,
            type_name: "StatisticsManager+Data, Assembly-CSharp".to_owned(),
        })]);
        let statistics = manager.decode_statistics_manager_data().unwrap();
        assert_eq!(statistics.format_version(), 114_009);
        assert_eq!(statistics.start_frame(), 4_775);
        assert_eq!(
            statistics.statistic_array_header(),
            &NetObjectHeader::NewType(
                "StatisticArray, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null"
                    .to_owned()
            )
        );
        assert_eq!(statistics.profile_end_shared_string_index(), 30);
        assert_eq!(
            statistics.encode(),
            fixture[profiler_tag("StatisticsManager").len()..]
        );

        let original_graph_len = manager.serialized_object_graph.len();
        let mut edited = statistics.into_editor();
        *edited.start_frame_mut() = 0x1234_5678;
        let replacement = edited.encode();
        let tag_len = profiler_tag("StatisticsManager").len();
        assert_eq!(replacement.len(), fixture.len() - tag_len);
        assert_eq!(&replacement[..4], &0x1234_5678_u32.to_be_bytes());
        assert_eq!(&replacement[4..], &fixture[tag_len + 4..]);

        manager.replace_statistics_manager_data(&edited).unwrap();
        assert_eq!(manager.serialized_object_graph.len(), original_graph_len);
        let rewritten = manager.decode_statistics_manager_data().unwrap();
        assert_eq!(rewritten.start_frame(), 0x1234_5678);
        assert_eq!(rewritten.profile_end_shared_string_index(), 30);
        let mut wrong_version = manager.clone();
        wrong_version.format_version = 114_008;
        assert!(
            wrong_version
                .replace_statistics_manager_data(&edited)
                .is_err()
        );
        assert!(manager.replace_statistics_manager_data(&edited).is_err());

        manager.serialized_object_graph.pop();
        assert!(manager.decode_statistics_manager_data().is_err());
    }

    #[test]
    fn unlock_manager_checks_versioned_fields_against_the_next_manager_tag() {
        let mut graph = profiler_tag("UnlockManager");
        for _ in 0..15 {
            graph.extend_from_slice(&0_u16.to_be_bytes());
        }
        graph.push(0xff);
        graph.extend_from_slice(&0x002a_u16.to_be_bytes());
        graph.extend(profiler_tag("MessageManager"));

        let mut manager = manager_with_graph(graph.clone());
        manager.format_version = 248;
        manager.manager_objects = Some(vec![
            Some(ManagerObjectReference {
                object_id: 1,
                type_name: "UnlockManager+Data, Assembly-CSharp".to_owned(),
            }),
            Some(ManagerObjectReference {
                object_id: 2,
                type_name: "MessageManager+Data, Assembly-CSharp".to_owned(),
            }),
        ]);
        let record = manager.decode_unlock_manager_references().unwrap();
        assert_eq!(record.arrays().len(), 15);
        assert_eq!(record.profile_end_shared_string_index(), 42);
        assert_eq!(
            record.encode(),
            graph[profiler_tag("UnlockManager").len()
                ..graph.len() - profiler_tag("MessageManager").len()]
        );

        manager
            .serialized_object_graph
            .insert(graph.len() - profiler_tag("MessageManager").len(), 0xaa);
        assert!(manager.decode_unlock_manager_references().is_err());
    }

    #[test]
    fn message_manager_checks_versioned_fields_against_the_guide_tag() {
        let mut graph = profiler_tag("MessageManager");
        graph.push(0xff);
        graph.extend_from_slice(&[0x00, 0x00, 0x10]);
        graph.extend([0xff; 16]);
        graph.extend_from_slice(&0x0007_i16.to_be_bytes());
        graph.extend_from_slice(&[0, 0, 0]);
        graph.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
        graph.extend_from_slice(&0_i16.to_be_bytes());
        graph.extend_from_slice(&0x0009_u16.to_be_bytes());
        graph.extend(profiler_tag("GuideManager"));

        let mut manager = manager_with_graph(graph.clone());
        manager.format_version = 194;
        manager.manager_objects = Some(vec![
            Some(ManagerObjectReference {
                object_id: 1,
                type_name: "MessageManager+Data, Assembly-CSharp".to_owned(),
            }),
            Some(ManagerObjectReference {
                object_id: 2,
                type_name: "GuideManager+Data, Assembly-CSharp".to_owned(),
            }),
        ]);
        let data = manager.decode_message_manager_data().unwrap();
        assert_eq!(data.message_queue(), None);
        assert_eq!(data.recent_messages().unwrap().len(), 16);
        assert_eq!(data.recent_message_index(), Some(7));
        assert!(data.message_data().unwrap().is_empty());
        assert_eq!(data.last_random_message_frame(), Some(0x0102_0304));
        assert_eq!(data.random_message_timer_count(), Some(0));
        assert!(data.random_message_timers().is_empty());
        assert_eq!(data.profile_end_shared_string_index(), 9);
        assert_eq!(
            data.encode(),
            graph[profiler_tag("MessageManager").len()
                ..graph.len() - profiler_tag("GuideManager").len()]
        );

        manager
            .serialized_object_graph
            .insert(graph.len() - profiler_tag("GuideManager").len(), 0xaa);
        assert!(manager.decode_message_manager_data().is_err());
    }

    #[test]
    fn message_manager_editor_replaces_only_fixed_width_body_fields() {
        let mut body = vec![0xff, 0x00, 0x00, 0x10];
        body.extend([0xff; 16]);
        body.extend_from_slice(&0x0007_i16.to_be_bytes());
        body.extend_from_slice(&[0, 0, 0]);
        body.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
        body.extend_from_slice(&0_i16.to_be_bytes());
        body.extend_from_slice(&0x0009_u16.to_be_bytes());

        let mut graph = profiler_tag("MessageManager");
        graph.extend_from_slice(&body);
        graph.extend(profiler_tag("GuideManager"));
        let mut manager = manager_with_graph(graph.clone());
        manager.format_version = 194;
        manager.manager_objects = Some(vec![
            Some(ManagerObjectReference {
                object_id: 1,
                type_name: "MessageManager+Data, Assembly-CSharp".to_owned(),
            }),
            Some(ManagerObjectReference {
                object_id: 2,
                type_name: "GuideManager+Data, Assembly-CSharp".to_owned(),
            }),
        ]);

        let mut edited = manager.decode_message_manager_data().unwrap().into_editor();
        *edited.recent_message_index_mut().unwrap() = -2;
        *edited.last_random_message_frame_mut().unwrap() = 0x5566_7788;
        manager.replace_message_manager_data(&edited).unwrap();

        let tag = profiler_tag("MessageManager");
        let body_start = tag.len();
        let mut expected = graph;
        expected[body_start + 20..body_start + 22].copy_from_slice(&(-2_i16).to_be_bytes());
        expected[body_start + 25..body_start + 29].copy_from_slice(&0x5566_7788_u32.to_be_bytes());
        assert_eq!(manager.serialized_object_graph(), expected);
        assert_eq!(manager.serialized_object_graph().len(), expected.len());
        let rewritten = manager.decode_message_manager_data().unwrap();
        assert_eq!(rewritten.recent_message_index(), Some(-2));
        assert_eq!(rewritten.last_random_message_frame(), Some(0x5566_7788));
        assert_eq!(rewritten.profile_end_shared_string_index(), 9);

        assert!(manager.replace_message_manager_data(&edited).is_err());
        let mut wrong_version = manager.clone();
        wrong_version.format_version += 1;
        assert!(wrong_version.replace_message_manager_data(&edited).is_err());
    }

    #[test]
    fn water_manager_guide_headers_align_with_the_zone_boundary() {
        let mut graph = vec![0; 9]; // Two counters and the can-continue byte.
        graph.extend_from_slice(&[0xfe, 0x00, 0x24, 0xfe, 0x00, 0x24]);
        graph.extend_from_slice(&[0x00, 0x0d]); // WaterManager profiler end index.
        graph.extend(profiler_tag("ZoneManager"));

        let mut manager = manager_with_graph(graph.clone());
        manager.format_version = 112_004;
        manager.manager_objects = Some(
            (0..14)
                .map(|slot| match slot {
                    12 => Some(ManagerObjectReference {
                        object_id: 13,
                        type_name: "WaterManager+Data, Assembly-CSharp".to_owned(),
                    }),
                    13 => Some(ManagerObjectReference {
                        object_id: 14,
                        type_name: "ZoneManager+Data, Assembly-CSharp".to_owned(),
                    }),
                    _ => None,
                })
                .collect(),
        );
        let mut object_table = DataSerializerObjectTable::new(super::ROOT_TYPE);
        for index in 1..=36 {
            object_table.shared_types.push(format!("TestType{index}"));
        }

        let headers = manager
            .decode_water_manager_guide_reference_headers(&object_table)
            .unwrap();
        assert_eq!(
            headers,
            [
                NetObjectHeader::NewSharedType(36),
                NetObjectHeader::NewSharedType(36)
            ]
        );

        let mut null_graph = vec![0; 9];
        null_graph.extend_from_slice(&[0xff, 0xff, 0x00, 0x0d]);
        null_graph.extend(profiler_tag("ZoneManager"));
        manager.serialized_object_graph = null_graph;
        assert_eq!(
            manager
                .decode_water_manager_guide_reference_headers(&object_table)
                .unwrap(),
            [NetObjectHeader::Null, NetObjectHeader::Null]
        );

        manager.serialized_object_graph[11] = 0xfe;
        assert!(
            manager
                .decode_water_manager_guide_reference_headers(&object_table)
                .is_err()
        );
    }

    #[test]
    fn guide_body_replacement_checks_source_bytes_and_preserves_queue_length() {
        let original_body = [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0x7f, // base fields with noncanonical false bool
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, // build index and instance ID
        ];
        let mut graph = vec![0xaa, 0xbb, 0xcc];
        graph.extend_from_slice(&original_body);
        graph.extend_from_slice(&[0xdd, 0xee]);
        let mut manager = manager_with_graph(graph.clone());
        let (guide, consumed) = GuideInstanceBody::decode(&graph[3..]).unwrap();
        assert_eq!(consumed, original_body.len());
        let mut editor = guide.into_editor();
        *editor.build_index_mut() = 0xa1b2_c3d4;
        *editor.instance_id_mut() = 0xe5f6;

        assert!(manager.replace_guide_instance_body_at(4, &editor).is_err());
        assert_eq!(manager.serialized_object_graph(), graph);
        assert!(
            manager
                .replace_guide_instance_body_at(usize::MAX, &editor)
                .is_err()
        );
        assert!(
            manager
                .replace_guide_instance_body_at(graph.len() - 2, &editor)
                .is_err()
        );

        manager.replace_guide_instance_body_at(3, &editor).unwrap();
        let mut expected = graph;
        expected[14..18].copy_from_slice(&0xa1b2_c3d4_u32.to_be_bytes());
        expected[18..20].copy_from_slice(&0xe5f6_u16.to_be_bytes());
        assert_eq!(manager.serialized_object_graph(), expected);
        assert_eq!(manager.serialized_object_graph().len(), expected.len());
        assert_eq!(manager.serialized_object_graph()[0..3], [0xaa, 0xbb, 0xcc]);
        assert_eq!(manager.serialized_object_graph()[20..], [0xdd, 0xee]);
    }

    #[test]
    fn guide_trigger_prefix_replacement_checks_source_bytes_and_preserves_queue_length() {
        let original_prefix = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0x7f];
        let mut graph = vec![0xaa, 0xbb, 0xcc];
        graph.extend_from_slice(&original_prefix);
        graph.extend_from_slice(&[0xdd, 0xee]);
        let mut manager = manager_with_graph(graph.clone());
        let (guide, consumed) = GuideTriggerBasePrefix::decode(&graph[3..]).unwrap();
        assert_eq!(consumed, GUIDE_TRIGGER_BASE_PREFIX_LEN);
        let mut editor = guide.into_editor();
        *editor.next_repeat_mut() = 0xa1b2_c3d4;
        *editor.disabled_mut() = true;

        assert!(manager.replace_guide_trigger_base_at(4, &editor).is_err());
        assert_eq!(manager.serialized_object_graph(), graph);
        assert!(
            manager
                .replace_guide_trigger_base_at(usize::MAX, &editor)
                .is_err()
        );
        assert!(
            manager
                .replace_guide_trigger_base_at(graph.len() - 2, &editor)
                .is_err()
        );

        manager.replace_guide_trigger_base_at(3, &editor).unwrap();
        let mut expected = graph;
        expected[3..7].copy_from_slice(&0xa1b2_c3d4_u32.to_be_bytes());
        expected[13] = 1;
        assert_eq!(manager.serialized_object_graph(), expected);
        assert_eq!(manager.serialized_object_graph().len(), expected.len());
        assert_eq!(manager.serialized_object_graph()[0..3], [0xaa, 0xbb, 0xcc]);
        assert_eq!(manager.serialized_object_graph()[14..], [0xdd, 0xee]);
    }

    #[test]
    fn object_table_tracks_new_shared_and_existing_references() {
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        assert_eq!(
            table
                .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
                .unwrap(),
            Some(1)
        );
        assert_eq!(
            table.register(&NetObjectHeader::NewSharedType(1)).unwrap(),
            Some(2)
        );
        assert_eq!(
            table.register(&NetObjectHeader::Existing(1)).unwrap(),
            Some(1)
        );
        assert_eq!(table.register(&NetObjectHeader::Null).unwrap(), None);
        assert_eq!(table.type_name(2), Some("GenericGuide"));
        assert!(table.register(&NetObjectHeader::Existing(3)).is_err());
        assert!(table.register(&NetObjectHeader::NewSharedType(9)).is_err());
    }

    #[test]
    fn transport_manager_resolves_fixed_tail_headers_for_empty_buffers() {
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        table
            .register(&NetObjectHeader::NewType(
                "TransportLine+Data, Assembly-CSharp".to_owned(),
            ))
            .unwrap();

        let mut bytes = vec![0xff, 0xfe]; // 255 zero line flags.
        bytes.extend_from_slice(&0_u16.to_be_bytes()); // No unlock milestones.
        bytes.extend_from_slice(&0_u16.to_be_bytes()); // Empty prefab-name table.
        bytes.extend_from_slice(&[0; 14 * 2]); // Version 112004 line numbers.
        bytes.extend_from_slice(&[0; 14 * 8 * 9]); // Global passenger records.
        bytes.extend_from_slice(&[
            0xfe, 0x00, 0x01, // New TransportLine object.
            0x00, 0x00, 0x01, // Existing object 1.
            0xff, // Null.
            0x00, 0x00, 0x00, // Existing root object 0.
            0xff, // Null.
            0xfe, 0x00, 0x01, // Another new object with the same type.
            0x00, 0x00, 0x02, // Existing object 2.
        ]);

        let headers = decode_transport_manager_reference_body(112_004, &bytes, &table).unwrap();
        assert_eq!(
            headers,
            [
                NetObjectHeader::NewSharedType(1),
                NetObjectHeader::Existing(1),
                NetObjectHeader::Null,
                NetObjectHeader::Existing(0),
                NetObjectHeader::Null,
                NetObjectHeader::NewSharedType(1),
                NetObjectHeader::Existing(2),
            ]
        );
        let mut nonempty_milestones = bytes.clone();
        nonempty_milestones[3] = 1;
        assert!(
            decode_transport_manager_reference_body(112_004, &nonempty_milestones, &table).is_err()
        );
        let mut active_lines = Vec::new();
        write_encoded_uint_array(&mut active_lines, &[1, 0, 0, 0]);
        write_encoded_uint_array(&mut active_lines, &[0; 251]);
        active_lines.extend_from_slice(&bytes[2..]);
        assert!(decode_transport_manager_reference_body(112_004, &active_lines, &table).is_err());
        assert!(
            decode_transport_manager_reference_body(112_004, &bytes[..bytes.len() - 1], &table)
                .is_err()
        );

        let mut selection_bytes = vec![0xff, 0xfe];
        selection_bytes.extend_from_slice(&0_u16.to_be_bytes());
        selection_bytes.extend_from_slice(&0_u16.to_be_bytes());
        selection_bytes.extend_from_slice(&[0; 17 * 2 + 17 * 8 * 9]);
        selection_bytes.extend([0xff; 7]);
        let count_offset = selection_bytes.len();
        selection_bytes.extend_from_slice(&2_i16.to_be_bytes());
        selection_bytes.extend_from_slice(&(-3_i16).to_be_bytes());
        selection_bytes.push(0xfe);
        write_unique_string(&mut selection_bytes, "TramVehicle").unwrap();
        selection_bytes.extend_from_slice(&5_i16.to_be_bytes());
        let second_shared_string_offset = selection_bytes.len();
        selection_bytes.extend_from_slice(&[0x00, 0x00]);
        assert_eq!(
            decode_transport_manager_reference_body(
                114_001,
                &selection_bytes,
                &DataSerializerObjectTable::new("SimulationManager+Data"),
            )
            .unwrap(),
            vec![NetObjectHeader::Null; 7]
        );

        let mut negative_count = selection_bytes.clone();
        negative_count[count_offset..count_offset + 2].copy_from_slice(&(-1_i16).to_be_bytes());
        assert!(
            decode_transport_manager_reference_body(
                114_001,
                &negative_count,
                &DataSerializerObjectTable::new("SimulationManager+Data"),
            )
            .is_err()
        );

        let mut reserved_shared_marker = selection_bytes.clone();
        reserved_shared_marker[second_shared_string_offset] = 0xff;
        assert!(
            decode_transport_manager_reference_body(
                114_001,
                &reserved_shared_marker,
                &DataSerializerObjectTable::new("SimulationManager+Data"),
            )
            .is_err()
        );

        assert!(
            decode_transport_manager_reference_body(
                114_001,
                &selection_bytes[..selection_bytes.len() - 1],
                &DataSerializerObjectTable::new("SimulationManager+Data"),
            )
            .is_err()
        );
    }

    #[test]
    fn transport_manager_reference_counts_follow_il_gates() {
        assert_eq!(transport_manager_object_header_count(315), 0);
        assert_eq!(transport_manager_object_header_count(316), 2);
        assert_eq!(transport_manager_object_header_count(317), 3);
        assert_eq!(transport_manager_object_header_count(318), 7);

        assert_eq!(transport_manager_line_number_count(121), 0);
        assert_eq!(transport_manager_line_number_count(122), 5);
        assert_eq!(transport_manager_line_number_count(210), 5);
        assert_eq!(transport_manager_line_number_count(211), 8);
        assert_eq!(transport_manager_line_number_count(310), 10);
        assert_eq!(transport_manager_line_number_count(110_000), 12);
        assert_eq!(transport_manager_line_number_count(110_021), 13);
        assert_eq!(transport_manager_line_number_count(111_006), 14);
        assert_eq!(transport_manager_line_number_count(113_000), 16);
        assert_eq!(transport_manager_line_number_count(113_008), 17);
    }

    #[test]
    fn economy_manager_reference_count_follows_reader_gates() {
        assert_eq!(economy_manager_object_header_count(93), 0);
        assert_eq!(economy_manager_object_header_count(94), 1);
        assert_eq!(economy_manager_object_header_count(165), 1);
        assert_eq!(economy_manager_object_header_count(166), 2);
    }

    #[test]
    fn economy_manager_trailing_references_resolve_null_shared_and_inline_headers() {
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        assert_eq!(
            find_trailing_object_headers(&[0xff, 0xff], 2, &table).unwrap(),
            [NetObjectHeader::Null, NetObjectHeader::Null]
        );

        table
            .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
            .unwrap();
        assert_eq!(
            find_trailing_object_headers(&[0xfe, 0x00, 0x01, 0x00, 0x00, 0x01], 2, &table).unwrap(),
            [
                NetObjectHeader::NewSharedType(1),
                NetObjectHeader::Existing(1)
            ]
        );

        let mut inline_header = vec![0x00, 0x00, 0x01, 0xfe, 0xfe];
        write_unique_string(&mut inline_header, "InlineGuide").unwrap();
        assert_eq!(
            find_trailing_object_headers(&inline_header, 2, &table).unwrap(),
            [
                NetObjectHeader::Existing(1),
                NetObjectHeader::NewType("InlineGuide".to_owned())
            ]
        );
    }

    #[test]
    fn economy_manager_trailing_references_reject_missing_invalid_and_ambiguous_headers() {
        let table = DataSerializerObjectTable::new("SimulationManager+Data");
        assert!(find_trailing_object_headers(&[0xff], 2, &table).is_err());
        assert!(find_trailing_object_headers(&[0x00, 0x00, 0x01, 0xff], 2, &table).is_err());
        assert!(find_trailing_object_headers(&[0xff], 3, &table).is_err());

        let mut large_table = table;
        large_table
            .object_types
            .resize(65_536, "SimulationManager+Data".to_owned());
        assert!(find_trailing_object_headers(&[0xff, 0x00, 0xff, 0xff], 2, &large_table).is_err());
    }

    #[test]
    fn modern_economy_manager_guide_references_follow_investment_gate() {
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        table
            .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
            .unwrap();

        let mut old_investment_body = vec![0xfe, 0x00, 0x01, 0xfe, 0x00, 0x01];
        assert_eq!(
            find_economy_manager_guide_headers(116_016, &old_investment_body, &table).unwrap(),
            [
                NetObjectHeader::NewSharedType(1),
                NetObjectHeader::NewSharedType(1)
            ]
        );

        old_investment_body
            .extend_from_slice(&[0xfe, 0x00, 0x01, 0xfe, 0x00, 0x01, 0xfe, 0x00, 0x01]);
        assert_eq!(
            find_economy_manager_guide_headers(116_017, &old_investment_body, &table).unwrap(),
            [
                NetObjectHeader::NewSharedType(1),
                NetObjectHeader::NewSharedType(1),
                NetObjectHeader::NewSharedType(1),
                NetObjectHeader::NewSharedType(1),
                NetObjectHeader::NewSharedType(1)
            ]
        );
    }

    #[test]
    fn economy_manager_investment_flags_follow_modern_guide_references() {
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        table
            .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
            .unwrap();
        let mut body = vec![0xfe, 0x00, 0x01].repeat(5);
        body.extend_from_slice(&[0x00, 0x00, 0x01, 0xff, 0xff]);

        let (flags, values_start) =
            decode_economy_manager_investment_flags_from_body(116_017, &body, &table).unwrap();
        assert_eq!(flags, (1_u16..=254).collect::<Vec<_>>());
        assert_eq!(values_start, 20);
        assert!(decode_economy_manager_investment_flags_from_body(116_016, &body, &table).is_err());

        let mut truncated = vec![0xfe, 0x00, 0x01].repeat(5);
        truncated.extend_from_slice(&[0x00, 0x00, 0x01, 0x00]);
        assert!(
            decode_economy_manager_investment_flags_from_body(116_017, &truncated, &table).is_err()
        );
    }

    #[test]
    fn economy_manager_investments_match_north_europe_116011_fixture() {
        let bytes =
            include_bytes!("../tests/fixtures/economy-investments-v116011-north-europe.bin");
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        for index in 0..29 {
            table
                .register(&NetObjectHeader::NewType(format!("FixtureType{index}")))
                .unwrap();
        }
        table
            .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
            .unwrap();
        assert_eq!(
            table.shared_types.get(30).map(String::as_str),
            Some("GenericGuide")
        );

        let (flags, values_start) =
            decode_economy_manager_investment_flags_from_body(116_011, bytes, &table).unwrap();
        assert_eq!(values_start, 397);
        assert_eq!(
            flags
                .iter()
                .enumerate()
                .filter_map(|(offset, flag)| (*flag != 0).then_some(offset + 1))
                .collect::<Vec<_>>(),
            [8, 24, 27, 36, 54, 90, 122, 138, 157, 176]
        );
        assert!(
            flags
                .iter()
                .filter(|flag| **flag != 0)
                .all(|flag| *flag == 2)
        );

        let (series, values_end) =
            decode_economy_manager_investment_value_series(116_011, bytes, &table).unwrap();
        assert_eq!(values_end, 451);
        assert_eq!(
            series
                .iter()
                .map(|entry| entry.investment_index)
                .collect::<Vec<_>>(),
            [8, 24, 27, 36, 54, 90, 122, 138, 157, 176]
        );
        assert!(series.iter().all(|entry| entry.values.len() == 2));
        assert_eq!(
            series[0]
                .values
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            [0x447a_0000, 0x447a_0000]
        );
        assert_eq!(
            series[7]
                .values
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            [0x4478_3eb8, 0x4476_7e14]
        );

        let (investments, fields_end) =
            decode_economy_manager_investments(116_011, bytes, &table).unwrap();
        assert_eq!(fields_end, 651);
        assert!(investments.iter().all(|investment| {
            investment.player_spent == 0
                && investment.owned_share_count == 0
                && investment.requirement_missing_date_ticks
                    == if matches!(investment.investment_index, 138 | 176) {
                        3_155_378_112_000_000_000
                    } else {
                        637_545_600_000_000_000
                    }
                && investment.current_state == 0
                && investment.random_influence.to_bits() == 0
        }));
        assert_eq!(investments[7].base_value.to_bits(), 0x447a_0000);
        assert!(
            investments
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != 7)
                .all(|(_, investment)| investment.base_value.to_bits() == 0)
        );

        let (prefabs, scope_end) =
            decode_economy_manager_investment_prefabs_from_body(116_011, bytes, &table).unwrap();
        assert_eq!(scope_end, bytes.len());
        assert_eq!(
            prefabs
                .assignments
                .iter()
                .map(|assignment| (assignment.investment_index, assignment.prefab_index))
                .collect::<Vec<_>>(),
            [
                (8, 1),
                (24, 5),
                (27, 9),
                (36, 6),
                (54, 3),
                (90, 7),
                (122, 8),
                (138, 0),
                (157, 4),
                (176, 2),
            ]
        );
        assert_eq!(
            prefabs.names,
            [
                "Chirp Coin",
                "Death Care Sector Investment",
                "Fuel Company Investment",
                "Health Care Sector Investment",
                "Health Care Sector Random Investment",
                "Farming Industry Investment",
                "Forestry Industry Investment",
                "Generic Industry Investment",
                "Oil Industry Investment",
                "Ore Industry Investment",
            ]
            .map(|name| Some(name.to_owned()))
        );
        assert!(
            decode_economy_manager_investment_prefabs_from_body(
                116_011,
                &bytes[..bytes.len() - 1],
                &table
            )
            .is_err()
        );
    }

    #[test]
    fn economy_manager_investment_prefix_and_numeric_columns_follow_version_gates() {
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        for index in 0..29 {
            table
                .register(&NetObjectHeader::NewType(format!("FixtureType{index}")))
                .unwrap();
        }
        table
            .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
            .unwrap();

        for format_version in [116_009, 116_010, 116_011, 116_012, 116_014, 116_015] {
            let mut body = vec![0xfe, 0x00, 0x1e].repeat(2);
            if format_version >= 116_000 {
                body.extend_from_slice(&7_i16.to_be_bytes());
            }
            if format_version >= 116_008 {
                if format_version == 116_010 {
                    body.extend_from_slice(&1_i16.to_be_bytes()); // One transition.
                    body.extend_from_slice(&1_i16.to_be_bytes()); // One integer value.
                    body.extend_from_slice(&0x1122_3344_u32.to_be_bytes());
                    body.extend_from_slice(&1_i16.to_be_bytes()); // One float value.
                    body.extend_from_slice(&[0, 0x3f, 0x80, 0, 0]);
                    body.extend_from_slice(&(-21_i32).to_be_bytes());
                    body.extend_from_slice(&22_u32.to_be_bytes());
                } else {
                    body.extend_from_slice(&0_i16.to_be_bytes());
                }
            }
            if format_version >= 116_005 {
                body.extend_from_slice(&9_i32.to_be_bytes());
            }
            if format_version >= 116_010 {
                body.extend_from_slice(&3_i16.to_be_bytes());
                body.extend_from_slice(&11_i32.to_be_bytes());
                body.extend_from_slice(&12_i32.to_be_bytes());
            }
            if format_version >= 116_014 {
                body.extend_from_slice(&13_i32.to_be_bytes());
                body.extend_from_slice(&14_i32.to_be_bytes());
            }
            if format_version >= 116_015 {
                body.extend_from_slice(&15_i32.to_be_bytes());
                body.extend_from_slice(&16_i32.to_be_bytes());
                body.extend_from_slice(&17_i64.to_be_bytes());
                body.extend_from_slice(&18_i64.to_be_bytes());
                body.push(1);
            }

            let mut flags = vec![0_u16; 254];
            flags[0] = 1;
            write_encoded_ushort_array(&mut body, &flags);
            body.extend_from_slice(&0_i16.to_be_bytes()); // Empty float history.
            body.extend_from_slice(&(-5_i32).to_be_bytes());
            body.extend_from_slice(&12_u32.to_be_bytes());
            body.extend_from_slice(&5.5_f32.to_bits().to_be_bytes());
            if format_version >= 116_010 {
                body.extend_from_slice(&19_i64.to_be_bytes());
            }
            if format_version >= 116_012 {
                body.extend_from_slice(&(-2_i16).to_be_bytes());
                body.extend_from_slice(&0.25_f32.to_bits().to_be_bytes());
            }
            write_encoded_ushort_array(&mut body, &[4]);
            body.extend_from_slice(&0_u16.to_be_bytes()); // No prefab names.

            let (investments, fields_end) =
                decode_economy_manager_investments(format_version, &body, &table).unwrap();
            assert_eq!(investments.len(), 1, "version {format_version}");
            assert_eq!(investments[0].investment_index, 1);
            assert_eq!(investments[0].player_spent, -5);
            assert_eq!(investments[0].owned_share_count, 12);
            assert_eq!(investments[0].base_value.to_bits(), 5.5_f32.to_bits());
            assert_eq!(
                investments[0].requirement_missing_date_ticks,
                if format_version >= 116_010 { 19 } else { 0 }
            );
            assert_eq!(
                investments[0].current_state,
                if format_version >= 116_012 { -2 } else { 0 }
            );
            assert_eq!(
                investments[0].random_influence.to_bits(),
                if format_version >= 116_012 {
                    0.25_f32.to_bits()
                } else {
                    0
                }
            );
            let (prefabs, scope_end) =
                decode_economy_manager_investment_prefabs_from_body(format_version, &body, &table)
                    .unwrap();
            assert_eq!(fields_end + 5, scope_end, "version {format_version}");
            assert_eq!(scope_end, body.len(), "version {format_version}");
            assert_eq!(prefabs.assignments[0].prefab_index, 4);
            assert!(prefabs.names.is_empty());
        }
    }

    #[test]
    fn economy_manager_investment_transition_prefix_rejects_negative_and_truncated_counts() {
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        for index in 0..29 {
            table
                .register(&NetObjectHeader::NewType(format!("FixtureType{index}")))
                .unwrap();
        }
        table
            .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
            .unwrap();

        let mut negative_count = vec![0xfe, 0x00, 0x1e].repeat(2);
        negative_count.extend_from_slice(&0_i16.to_be_bytes()); // Last-updated day.
        negative_count.extend_from_slice(&(-1_i16).to_be_bytes());
        assert!(
            decode_economy_manager_investment_flags_from_body(116_010, &negative_count, &table)
                .is_err()
        );

        let mut truncated_values = vec![0xfe, 0x00, 0x1e].repeat(2);
        truncated_values.extend_from_slice(&0_i16.to_be_bytes()); // Last-updated day.
        truncated_values.extend_from_slice(&1_i16.to_be_bytes()); // One transition.
        truncated_values.extend_from_slice(&1_i16.to_be_bytes()); // One integer value.
        assert!(
            decode_economy_manager_investment_flags_from_body(116_010, &truncated_values, &table)
                .is_err()
        );
    }

    #[test]
    fn economy_manager_investment_value_series_follow_active_flags() {
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        table
            .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
            .unwrap();
        let mut body = vec![0xfe, 0x00, 0x01].repeat(5);
        body.extend_from_slice(&[
            0x04, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0xff, 0xf8, 0x00,
            0x02, 0x01, 0x3f, 0x80, 0x00, 0x00, 0xc0, 0x20, 0x00, 0x00, 0x00, 0x01, 0x00, 0x40,
            0xa0, 0x00, 0x00,
        ]);

        let (series, values_end) =
            decode_economy_manager_investment_value_series(116_017, &body, &table).unwrap();
        assert_eq!(values_end, body.len());
        assert_eq!(
            series,
            [
                super::EconomyInvestmentValueSeries {
                    investment_index: 1,
                    values: [1.0, -2.5].to_vec(),
                },
                super::EconomyInvestmentValueSeries {
                    investment_index: 3,
                    values: [5.0].to_vec(),
                },
            ]
        );

        let mut negative_count = body[..28].to_vec();
        negative_count.extend_from_slice(&[0xff, 0xff]);
        assert!(
            decode_economy_manager_investment_value_series(116_017, &negative_count, &table)
                .is_err()
        );

        let mut truncated_values = body[..28].to_vec();
        truncated_values.extend_from_slice(&[0x00, 0x02, 0x00, 0x3f, 0x80]);
        assert!(
            decode_economy_manager_investment_value_series(116_017, &truncated_values, &table)
                .is_err()
        );
    }

    #[test]
    fn economy_manager_investment_value_series_matches_boreal_fixture() {
        let bytes = include_bytes!("../tests/fixtures/economy-investments-v116019-boreal.bin");
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        for index in 0..29 {
            table
                .register(&NetObjectHeader::NewType(format!("FixtureType{index}")))
                .unwrap();
        }
        table
            .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
            .unwrap();

        let (series, values_end) =
            decode_economy_manager_investment_value_series(116_019, bytes, &table).unwrap();
        assert_eq!(values_end, 607);
        assert_eq!(
            series
                .iter()
                .map(|entry| entry.investment_index)
                .collect::<Vec<_>>(),
            [8, 16, 19, 34, 69, 81, 85, 124, 158, 162, 185, 187, 250]
        );
        assert!(series.iter().all(|entry| entry.values.len() == 1));
        assert_eq!(
            series
                .iter()
                .map(|entry| entry.values[0].to_bits())
                .collect::<Vec<_>>(),
            [
                0x4401_0000,
                0x4504_3000,
                0x4248_0000,
                0x447b_7666,
                0x42fa_0000,
                0x447a_0000,
                0x44fe_a000,
                0x4382_0000,
                0x444d_c000,
                0x44b2_2000,
                0x445d_4000,
                0x4315_0000,
                0x4455_0000,
            ]
        );
    }

    #[test]
    fn economy_manager_investment_numeric_fields_follow_independent_wire_order() {
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        table
            .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
            .unwrap();

        let mut body = vec![0xfe, 0x00, 0x01].repeat(5);
        let mut flags = vec![0_u16; 254];
        flags[0] = 1;
        flags[2] = 1;
        write_encoded_ushort_array(&mut body, &flags);
        body.extend_from_slice(&[
            0x00, 0x02, 0x01, 0x3f, 0x80, 0x00, 0x00, 0xc0, 0x20, 0x00,
            0x00, // investment 1 history
            0x00, 0x01, 0x00, 0x40, 0xa0, 0x00, 0x00, // investment 3 history
            0xff, 0xff, 0xff, 0xfb, 0x00, 0x00, 0x00, 0x0c, 0x40, 0xb0, 0x00,
            0x00, // -5, 12, 5.5
            0x00, 0x00, 0x00, 0x07, 0x80, 0x00, 0x00, 0x01, 0x80, 0x00, 0x00,
            0x00, // 7, shares, -0.0
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // first requirement date
            0x2b, 0xca, 0x27, 0xac, 0xc9, 0xcd, 0x80, 0x00, // second requirement date
            0x00, 0x02, 0x3e, 0x80, 0x00, 0x00, // state 2, influence 0.25
            0xff, 0xfc, 0xbf, 0x00, 0x00, 0x00, // state -4, influence -0.5
        ]);
        let numeric_fields_end = body.len();
        write_encoded_ushort_array(&mut body, &[1, 0]);
        body.extend_from_slice(&2_u16.to_be_bytes());
        write_unique_string(&mut body, "A").unwrap();
        body.push(0xff); // Null prefab name.
        body.push(0xaa); // Following manager bytes are outside this scope.

        let (investments, fields_end) =
            decode_economy_manager_investments(116_017, &body, &table).unwrap();
        assert_eq!(fields_end, numeric_fields_end);
        assert_eq!(
            investments,
            [
                EconomyInvestmentData {
                    investment_index: 1,
                    value_history: [1.0, -2.5].to_vec(),
                    player_spent: -5,
                    owned_share_count: 12,
                    base_value: 5.5,
                    requirement_missing_date_ticks: 1,
                    current_state: 2,
                    random_influence: 0.25,
                },
                EconomyInvestmentData {
                    investment_index: 3,
                    value_history: [5.0].to_vec(),
                    player_spent: 7,
                    owned_share_count: 0x8000_0001,
                    base_value: -0.0,
                    requirement_missing_date_ticks: 0x2bca_27ac_c9cd_8000,
                    current_state: -4,
                    random_influence: -0.5,
                },
            ]
        );
        for truncated_end in 15..fields_end {
            assert!(
                decode_economy_manager_investments(116_017, &body[..truncated_end], &table)
                    .is_err(),
                "truncated EconomyManager body unexpectedly parsed through byte {truncated_end}"
            );
        }
        let (prefabs, prefab_end) =
            decode_economy_manager_investment_prefabs_from_body(116_017, &body, &table).unwrap();
        assert_eq!(prefab_end, body.len() - 1);
        assert_eq!(
            prefabs,
            EconomyInvestmentPrefabTable {
                assignments: vec![
                    EconomyInvestmentPrefabAssignment {
                        investment_index: 1,
                        prefab_index: 1,
                    },
                    EconomyInvestmentPrefabAssignment {
                        investment_index: 3,
                        prefab_index: 0,
                    },
                ],
                names: [Some("A".to_owned()), None].to_vec(),
            }
        );
        assert!(
            decode_economy_manager_investment_prefabs_from_body(
                116_017,
                &body[..body.len() - 2],
                &table
            )
            .is_err()
        );
    }

    #[test]
    fn economy_manager_investment_numeric_fields_match_boreal_fixture() {
        let bytes = include_bytes!("../tests/fixtures/economy-investments-v116019-boreal.bin");
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        for index in 0..29 {
            table
                .register(&NetObjectHeader::NewType(format!("FixtureType{index}")))
                .unwrap();
        }
        table
            .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
            .unwrap();

        let (investments, fields_end) =
            decode_economy_manager_investments(116_019, bytes, &table).unwrap();
        assert_eq!(fields_end, 945);
        assert_eq!(
            investments
                .iter()
                .map(|entry| entry.investment_index)
                .collect::<Vec<_>>(),
            [8, 16, 19, 34, 69, 81, 85, 124, 158, 162, 185, 187, 250]
        );
        assert!(investments.iter().all(|entry| {
            entry.value_history.len() == 1
                && entry.player_spent == 0
                && entry.owned_share_count == 0
                && entry.requirement_missing_date_ticks == 0x2bca_27ac_c9cd_8000
        }));
        for entry in &investments {
            if entry.investment_index == 34 {
                assert_eq!(entry.base_value.to_bits(), 1000.0_f32.to_bits());
                assert_eq!(entry.current_state, 1);
                assert_eq!(entry.random_influence.to_bits(), 0x3f15_c290);
            } else {
                assert_eq!(entry.base_value.to_bits(), 0);
                assert_eq!(entry.current_state, 0);
                assert_eq!(entry.random_influence.to_bits(), 0);
            }
        }
    }

    #[test]
    fn economy_manager_investment_prefab_table_matches_boreal_fixture() {
        let bytes = include_bytes!("../tests/fixtures/economy-investments-v116019-boreal.bin");
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        for index in 0..29 {
            table
                .register(&NetObjectHeader::NewType(format!("FixtureType{index}")))
                .unwrap();
        }
        table
            .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
            .unwrap();

        let (prefabs, scope_end) =
            decode_economy_manager_investment_prefabs_from_body(116_019, bytes, &table).unwrap();
        assert_eq!(scope_end, bytes.len());
        assert_eq!(
            prefabs,
            EconomyInvestmentPrefabTable {
                assignments: vec![
                    (8, 8),
                    (16, 9),
                    (19, 10),
                    (34, 0),
                    (69, 5),
                    (81, 1),
                    (85, 2),
                    (124, 7),
                    (158, 3),
                    (162, 11),
                    (185, 4),
                    (187, 6),
                    (250, 12),
                ]
                .into_iter()
                .map(
                    |(investment_index, prefab_index)| EconomyInvestmentPrefabAssignment {
                        investment_index,
                        prefab_index,
                    }
                )
                .collect(),
                names: [
                    Some("Chirp Coin".to_owned()),
                    Some("Death Care Sector Investment".to_owned()),
                    Some("Fuel Company Investment".to_owned()),
                    Some("Health Care Sector Investment".to_owned()),
                    Some("Farming Industry Investment".to_owned()),
                    Some("Forestry Industry Investment".to_owned()),
                    Some("Generic Industry Investment".to_owned()),
                    Some("Oil Industry Investment".to_owned()),
                    Some("Ore Industry Investment".to_owned()),
                    Some("TrafficJelly".to_owned()),
                    Some("VeryLegitCompany".to_owned()),
                    Some("AirplaneInvestment".to_owned()),
                    Some("ShipInvestment".to_owned()),
                ]
                .to_vec(),
            }
        );
    }

    #[test]
    fn modern_economy_manager_guide_references_reject_ambiguous_or_unresolved_data() {
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        table
            .register(&NetObjectHeader::NewType("GenericGuide".to_owned()))
            .unwrap();
        let too_few = [0xfe, 0x00, 0x01];
        assert!(find_economy_manager_guide_headers(116_016, &too_few, &table).is_err());

        let too_many = [0xfe, 0x00, 0x01, 0xfe, 0x00, 0x01, 0xfe, 0x00, 0x01];
        assert!(find_economy_manager_guide_headers(116_016, &too_many, &table).is_err());

        let null_reference = [0xff, 0xfe, 0x00, 0x01];
        assert!(find_economy_manager_guide_headers(116_016, &null_reference, &table).is_err());

        let existing_reference = [0x00, 0x00, 0x01, 0xfe, 0x00, 0x01];
        assert!(find_economy_manager_guide_headers(116_016, &existing_reference, &table).is_err());

        let other_type_table = DataSerializerObjectTable::new("SimulationManager+Data");
        assert!(
            find_economy_manager_guide_headers(
                116_016,
                &[0xfe, 0x00, 0x01, 0xfe, 0x00, 0x01],
                &other_type_table,
            )
            .is_err()
        );
    }

    #[test]
    fn manager_reference_registration_preserves_header_order_and_queues_new_objects() {
        let mut table = DataSerializerObjectTable::new("SimulationManager+Data");
        let mut queued = Vec::new();
        let headers = [
            NetObjectHeader::NewType("GenericGuide".to_owned()),
            NetObjectHeader::Existing(1),
            NetObjectHeader::Null,
            NetObjectHeader::NewSharedType(1),
        ];

        let object_ids = register_reference_headers(&headers, &mut table, &mut queued).unwrap();
        assert_eq!(object_ids, [Some(1), Some(1), None, Some(2)]);
        assert_eq!(table.type_name(1), Some("GenericGuide"));
        assert_eq!(table.type_name(2), Some("GenericGuide"));
        assert_eq!(
            queued,
            [
                QueuedGuideObject {
                    object_id: 1,
                    type_name: "GenericGuide".to_owned(),
                },
                QueuedGuideObject {
                    object_id: 2,
                    type_name: "GenericGuide".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn queued_body_locator_advances_by_declared_body_schema() {
        let first = [0; 11];
        let second = [0; 17];
        let third = [0; 11];
        let fourth = [0; 11];
        let mut graph = Vec::new();
        graph.extend_from_slice(&first);
        graph.extend_from_slice(&second);
        graph.extend_from_slice(&third);
        graph.extend_from_slice(&fourth);
        let objects = vec![
            QueuedGuideObject {
                object_id: 5,
                type_name: "GenericGuide, Assembly-CSharp".to_owned(),
            },
            QueuedGuideObject {
                object_id: 6,
                type_name: "BuildingInstanceGuide, Assembly-CSharp".to_owned(),
            },
            QueuedGuideObject {
                object_id: 7,
                type_name: "ServiceTypeGuide, Assembly-CSharp".to_owned(),
            },
            QueuedGuideObject {
                object_id: 8,
                type_name: "ZoneTypeGuide, Assembly-CSharp".to_owned(),
            },
        ];

        let location = locate_queued_object_body(&graph, 0, &objects, 6, 112_004, None).unwrap();
        assert_eq!(location.object_id, 6);
        assert_eq!(location.graph_offset, 11);
        assert_eq!(location.body_length, 17);
        assert_eq!(location.type_name, "BuildingInstanceGuide, Assembly-CSharp");
        let zone_location =
            locate_queued_object_body(&graph, 0, &objects, 8, 112_004, None).unwrap();
        assert_eq!(zone_location.graph_offset, 39);
        assert_eq!(zone_location.body_length, 11);
        assert_eq!(zone_location.type_name, "ZoneTypeGuide, Assembly-CSharp");
        assert!(locate_queued_object_body(&graph, 0, &objects, 9, 112_004, None).is_err());
        assert!(locate_queued_object_body(&graph[..20], 0, &objects, 6, 112_004, None).is_err());
    }

    #[test]
    fn batch_queued_body_locator_preserves_header_order_and_repeated_references() {
        let mut graph = vec![0; 11 + 17 + 11];
        let objects = vec![
            QueuedGuideObject {
                object_id: 5,
                type_name: "GenericGuide, Assembly-CSharp".to_owned(),
            },
            QueuedGuideObject {
                object_id: 6,
                type_name: "BuildingInstanceGuide, Assembly-CSharp".to_owned(),
            },
            QueuedGuideObject {
                object_id: 7,
                type_name: "ServiceTypeGuide, Assembly-CSharp".to_owned(),
            },
        ];
        let object_ids = [Some(5), Some(6), Some(5), None, Some(7)];

        let locations =
            locate_queued_object_bodies(&graph, 0, &objects, &object_ids, 112_004, None).unwrap();

        assert_eq!(locations.len(), object_ids.len());
        assert_eq!(locations[0].as_ref().unwrap().graph_offset, 0);
        assert_eq!(locations[1].as_ref().unwrap().graph_offset, 11);
        assert_eq!(locations[2], locations[0]);
        assert_eq!(locations[3], None);
        assert_eq!(locations[4].as_ref().unwrap().graph_offset, 28);
        assert!(
            locate_queued_object_bodies(&graph, 0, &objects, &[Some(8)], 112_004, None,).is_err()
        );

        graph.truncate(20);
        assert!(
            locate_queued_object_bodies(&graph, 0, &objects, &[Some(6)], 112_004, None,).is_err()
        );
    }

    fn manager_with_park_boundary(
        version: u32,
        park: &[u8],
        grids: &[u8],
        tail: &[u8],
        next: &[u8],
    ) -> SimulationManagerData {
        let mut data = synthetic_manager_data();
        data.format_version = version;
        let tag = profiler_tag("DistrictManager");
        let tag_start = data
            .serialized_object_graph
            .windows(tag.len())
            .position(|window| window == tag)
            .unwrap();
        let graph = &mut data.serialized_object_graph;
        graph.truncate(tag_start + tag.len());
        write_unique_string(graph, "Airline").unwrap();
        graph.push(0);
        write_encoded_uint_array(graph, &[0; 128]);
        let mut park_flags = [0_u32; 128];
        park_flags[0] = 1;
        write_encoded_uint_array(graph, &park_flags);
        write_encoded_bool_array(graph, &vec![false; 16 * 32 * 128]);
        graph.extend_from_slice(park);
        graph.extend_from_slice(grids);
        graph.extend_from_slice(tail);
        graph.extend_from_slice(next);
        data.manager_objects
            .as_mut()
            .unwrap()
            .push(Some(ManagerObjectReference {
                object_id: 22,
                type_name: "EventManager+Data, Assembly-CSharp".to_owned(),
            }));
        data
    }

    #[test]
    fn district_report_layout_uses_verified_graph_boundaries() {
        let ten = include_bytes!("../tests/fixtures/district-park-v117013-mp23-map01.bin");
        let grids = include_bytes!("../tests/fixtures/district-grids-v117013-mp23-map01.bin");
        let tail = include_bytes!("../tests/fixtures/district-tail-v117013-mp23-map01.bin");
        let next = include_bytes!("../tests/fixtures/district-next-v117013-event.bin");
        let data = manager_with_park_boundary(117_013, ten, grids, tail, next);
        let (_, park_start) = data.decode_district_manager_flags_prefix().unwrap();
        let (decoded, grid_end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(decoded.encode(), grids);
        assert_eq!(grid_end, park_start + ten.len() + grids.len());
        assert_eq!(
            data.decode_district_manager_tail().unwrap().1,
            grid_end + tail.len()
        );

        let mp27_park = include_bytes!("../tests/fixtures/district-park-v118011-mp27-map04.bin");
        let mp27_grids = include_bytes!("../tests/fixtures/district-grids-v118011-mp27-map04.bin");
        let mp27_tail = include_bytes!("../tests/fixtures/district-tail-v118011-mp27-map04.bin");
        let mp27_next = include_bytes!("../tests/fixtures/district-next-v118011-event.bin");
        let data = manager_with_park_boundary(118_011, mp27_park, mp27_grids, mp27_tail, mp27_next);
        let (_, park_start) = data.decode_district_manager_flags_prefix().unwrap();
        let (decoded, grid_end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(decoded.encode(), mp27_grids);
        assert_eq!(grid_end, park_start + mp27_park.len() + mp27_grids.len());

        // Three empty, game-produced report encodings inserted before the
        // trailing campus fields produce the DLL's thirteen-report layout.
        let mut thirteen = ten[..ten.len() - 29].to_vec();
        let empty_report = &ten[ten.len() - 29 - 70..ten.len() - 29];
        for _ in 0..3 {
            thirteen.extend_from_slice(empty_report);
        }
        thirteen.extend_from_slice(&ten[ten.len() - 29..]);
        let mut data = manager_with_park_boundary(117_013, &thirteen, grids, tail, next);
        let (_, park_start) = data.decode_district_manager_flags_prefix().unwrap();
        let (decoded, grid_end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(decoded.encode(), grids);
        assert_eq!(grid_end, park_start + thirteen.len() + grids.len());
        let mut editor = decoded.into_editor();
        let mut plane = vec![0; 262_144];
        plane[0] = 7;
        editor.replace_district_plane(0, &plane).unwrap();
        data.replace_district_manager_grids(&editor).unwrap();
        let (updated, updated_end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(updated.district_plane(0).unwrap(), plane);
        assert_eq!(
            &data.serialized_object_graph()[park_start..park_start + thirteen.len()],
            thirteen
        );
        assert_eq!(
            &data.serialized_object_graph()[updated_end..updated_end + tail.len() + next.len()],
            &[tail.as_slice(), next.as_slice()].concat()
        );

        let mut wrong_next = next.to_vec();
        wrong_next[0] = 0xfd;
        let malformed = manager_with_park_boundary(117_013, ten, grids, tail, &wrong_next);
        assert!(malformed.decode_district_manager_grids().is_err());
        let malformed =
            manager_with_park_boundary(117_013, ten, &grids[..grids.len() - 1], tail, next);
        assert!(malformed.decode_district_manager_grids().is_err());
    }

    #[test]
    fn park_editor_updates_common_scalars_and_preserves_neighboring_bytes() {
        let park = include_bytes!("../tests/fixtures/district-park-v117013-mp23-map01.bin");
        let grids = include_bytes!("../tests/fixtures/district-grids-v117013-mp23-map01.bin");
        let tail = include_bytes!("../tests/fixtures/district-tail-v117013-mp23-map01.bin");
        let next = include_bytes!("../tests/fixtures/district-next-v117013-event.bin");
        let mut data = manager_with_park_boundary(117_013, park, grids, tail, next);
        let records = data.decode_district_manager_park_records().unwrap();
        assert_eq!(records.len(), 1);
        let stale_editor = records[0].clone().into_editor();
        let mut editor = records[0].clone().into_editor();
        editor.set_random_seed(0x0102_0304_0506_0708).unwrap();
        editor.set_main_gate(0x1122).unwrap();
        editor.set_condition(0x3344).unwrap();
        editor.set_level(0x55).unwrap();
        data.replace_district_park_record(&editor).unwrap();

        let mut expected = park.to_vec();
        expected[0..8].copy_from_slice(&0x0102_0304_0506_0708_u64.to_be_bytes());
        expected[8..10].copy_from_slice(&0x1122_u16.to_be_bytes());
        expected[10..12].copy_from_slice(&0x3344_u16.to_be_bytes());
        expected[59] = 0x55;
        assert_eq!(
            data.decode_district_manager_park_records().unwrap()[0].encode(),
            expected
        );
        assert!(data.replace_district_park_record(&stale_editor).is_err());

        let (_, park_start) = data.decode_district_manager_flags_prefix().unwrap();
        assert_eq!(
            &data.serialized_object_graph()
                [park_start + expected.len()..park_start + expected.len() + grids.len()],
            grids
        );
        let (_, grid_end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(
            &data.serialized_object_graph()[grid_end..grid_end + tail.len() + next.len()],
            &[tail.as_slice(), next.as_slice()].concat()
        );
        assert_eq!(
            stale_editor.random_seed(),
            u64::from_be_bytes(park[..8].try_into().unwrap())
        );
    }

    #[test]
    fn airport_field_edits_replace_only_park_bytes_in_manager_graph() {
        let type_zero_park =
            include_bytes!("../tests/fixtures/district-park-v117013-mp23-map01.bin");
        let grids = include_bytes!("../tests/fixtures/district-grids-v117013-mp23-map01.bin");
        let tail = include_bytes!("../tests/fixtures/district-tail-v117013-mp23-map01.bin");
        let next = include_bytes!("../tests/fixtures/district-next-v117013-event.bin");
        let scalar_bytes = 94;
        let campus_ten = &type_zero_park[scalar_bytes..];
        let campus_tail_start = campus_ten.len() - 29;
        let empty_report_start = campus_tail_start - 70;
        let empty_report = &campus_ten[empty_report_start..campus_tail_start];
        let mut campus_thirteen = campus_ten[..campus_tail_start].to_vec();
        for _ in 0..3 {
            campus_thirteen.extend_from_slice(empty_report);
        }
        campus_thirteen.extend_from_slice(&campus_ten[campus_tail_start..]);

        let mut park = type_zero_park[..scalar_bytes].to_vec();
        park[58] = 14;
        park.extend_from_slice(&campus_thirteen);
        let airport_start = park.len();
        let mut airport_bytes = [0x5a; 31];
        airport_bytes[12..15].copy_from_slice(&[1, 0x7f, 2]);
        park.extend_from_slice(&airport_bytes);

        let mut data = manager_with_park_boundary(117_013, &park, grids, tail, next);
        let records = data.decode_district_manager_park_records().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].airport_data().unwrap().has_aircraft_stand(),
            Some(true)
        );

        let mut editor = records[0].clone().into_editor();
        {
            let mut airport = editor.airport_data_mut().unwrap();
            airport.set_terrain_height(1.5).unwrap();
            airport
                .set_passenger_totals([0x0102_0304, 0x1122_3344])
                .unwrap();
            airport.set_runway(true).unwrap();
            airport
                .set_weekly_capacities([0x5566_7788, 0x99aa_bbcc])
                .unwrap();
            airport
                .set_incoming_passenger_counts([0xdead_beef, 0x1234_5678])
                .unwrap();
        }
        let mut expected = park.to_vec();
        expected[airport_start..airport_start + 4].copy_from_slice(&[0x3f, 0xc0, 0, 0]);
        expected[airport_start + 4..airport_start + 12]
            .copy_from_slice(&[1, 2, 3, 4, 0x11, 0x22, 0x33, 0x44]);
        expected[airport_start + 13] = 1;
        expected[airport_start + 15..airport_start + 23]
            .copy_from_slice(&[0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc]);
        expected[airport_start + 23..airport_start + 31]
            .copy_from_slice(&[0xde, 0xad, 0xbe, 0xef, 0x12, 0x34, 0x56, 0x78]);
        data.replace_district_park_record(&editor).unwrap();

        let updated_records = data.decode_district_manager_park_records().unwrap();
        assert_eq!(updated_records[0].encode(), expected);
        let updated_airport = updated_records[0].airport_data().unwrap();
        assert_eq!(
            updated_airport.terrain_height().map(f32::to_bits),
            Some(1.5_f32.to_bits())
        );
        assert_eq!(
            updated_airport.passenger_totals(),
            Some([0x0102_0304, 0x1122_3344])
        );
        assert_eq!(updated_airport.has_runway(), Some(true));
        assert_eq!(updated_airport.has_terminal(), Some(false));
        assert_eq!(
            updated_airport.weekly_capacities(),
            Some([0x5566_7788, 0x99aa_bbcc])
        );
        assert_eq!(
            updated_airport.incoming_passenger_counts(),
            Some([0xdead_beef, 0x1234_5678])
        );

        let (_, park_start) = data.decode_district_manager_flags_prefix().unwrap();
        let (updated_grids, grid_end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(updated_grids.encode(), grids);
        assert_eq!(
            &data.serialized_object_graph()[park_start..park_start + expected.len()],
            expected
        );
        assert_eq!(
            &data.serialized_object_graph()[grid_end..grid_end + tail.len() + next.len()],
            &[tail.as_slice(), next.as_slice()].concat()
        );
        assert_eq!(
            data.decode_district_manager_tail().unwrap().1,
            grid_end + tail.len()
        );
    }

    #[test]
    fn campus_report_and_sports_array_edits_resize_park_safely() {
        let park = include_bytes!("../tests/fixtures/district-park-v117013-mp23-map01.bin");
        let grids = include_bytes!("../tests/fixtures/district-grids-v117013-mp23-map01.bin");
        let tail = include_bytes!("../tests/fixtures/district-tail-v117013-mp23-map01.bin");
        let next = include_bytes!("../tests/fixtures/district-next-v117013-event.bin");
        let mut data = manager_with_park_boundary(117_013, park, grids, tail, next);
        let (prefix, park_start) = data.decode_district_manager_flags_prefix().unwrap();
        let records = data.decode_district_manager_park_records().unwrap();
        assert_eq!(records.len(), 1);
        let (_, original_grid_end) = data.decode_district_manager_grids().unwrap();
        let original_grid_start = original_grid_end - grids.len();

        let mut editor = records[0].clone().into_editor();
        editor
            .campus_year_report_mut(0)
            .unwrap()
            .set_tickets_income([1, 2, 3, 4, 5])
            .unwrap();
        editor
            .campus_year_report_mut(0)
            .unwrap()
            .set_milestone_unlocked(Some("Field notes"))
            .unwrap();
        editor
            .campus_year_report_mut(1)
            .unwrap()
            .set_total_works(77)
            .unwrap();
        {
            let mut sports = editor.campus_sports_mut().unwrap();
            sports
                .set_away_matches_done([true, false, true, false, true])
                .unwrap();
            sports.set_grant_type(0x5a).unwrap();
        }
        let edited_record = editor.encode();
        assert_ne!(edited_record.len(), park.len());
        let resize = edited_record.len().abs_diff(park.len());
        assert!(edited_record.len() > park.len());
        data.replace_district_park_record(&editor).unwrap();

        let updated_records = data.decode_district_manager_park_records().unwrap();
        assert_eq!(updated_records[0].encode(), edited_record);
        let reports = updated_records[0].campus_year_reports().unwrap();
        assert_eq!(reports[0].tickets_income(), Some([1, 2, 3, 4, 5]));
        assert_eq!(reports[0].milestone_unlocked(), Some("Field notes"));
        assert_eq!(reports[1].total_works(), Some(77));
        assert_eq!(
            updated_records[0]
                .campus_sports_data()
                .unwrap()
                .away_matches_done(),
            Some([true, false, true, false, true])
        );
        assert_eq!(
            updated_records[0]
                .campus_sports_data()
                .unwrap()
                .grant_type(),
            0x5a
        );
        let (_, updated_grid_end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(updated_grid_end, original_grid_end + resize);
        assert_eq!(
            &data.serialized_object_graph()[original_grid_start + resize..updated_grid_end],
            grids
        );
        assert_eq!(
            &data.serialized_object_graph()
                [updated_grid_end..updated_grid_end + tail.len() + next.len()],
            &[tail.as_slice(), next.as_slice()].concat()
        );
        assert_eq!(
            data.decode_district_manager_tail().unwrap().1,
            updated_grid_end + tail.len()
        );

        let mut shrink_editor = updated_records[0].clone().into_editor();
        shrink_editor
            .campus_year_report_mut(0)
            .unwrap()
            .set_milestone_unlocked(None)
            .unwrap();
        let shrunk_record = shrink_editor.encode();
        assert!(shrunk_record.len() < edited_record.len());
        let shrink = edited_record.len() - shrunk_record.len();
        data.replace_district_park_record(&shrink_editor).unwrap();

        let shrunk_records = data.decode_district_manager_park_records().unwrap();
        assert_eq!(shrunk_records[0].encode(), shrunk_record);
        let shrunk_reports = shrunk_records[0].campus_year_reports().unwrap();
        assert_eq!(shrunk_reports[0].tickets_income(), Some([1, 2, 3, 4, 5]));
        assert_eq!(shrunk_reports[0].milestone_unlocked(), None);
        assert_eq!(shrunk_reports[1].total_works(), Some(77));
        assert_eq!(
            shrunk_records[0]
                .campus_sports_data()
                .unwrap()
                .away_matches_done(),
            Some([true, false, true, false, true])
        );
        let (_, shrunk_grid_end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(shrunk_grid_end, updated_grid_end - shrink);
        let shrunk_grid_start = shrunk_grid_end - grids.len();
        assert_eq!(
            &data.serialized_object_graph()[shrunk_grid_start..shrunk_grid_end],
            grids
        );
        assert_eq!(
            &data.serialized_object_graph()
                [shrunk_grid_end..shrunk_grid_end + tail.len() + next.len()],
            &[tail.as_slice(), next.as_slice()].concat()
        );
        assert_eq!(
            data.decode_district_manager_tail().unwrap().1,
            shrunk_grid_end + tail.len()
        );
        assert_eq!(prefix.park_flags()[0], 1);
        assert_eq!(park_start + records[0].encode().len(), original_grid_start);
    }

    #[test]
    fn industry_resource_edit_replaces_only_its_manager_record_bytes() {
        let type_zero_park =
            include_bytes!("../tests/fixtures/district-park-v117013-mp23-map01.bin");
        let grids = include_bytes!("../tests/fixtures/district-grids-v117013-mp23-map01.bin");
        let tail = include_bytes!("../tests/fixtures/district-tail-v117013-mp23-map01.bin");
        let next = include_bytes!("../tests/fixtures/district-next-v117013-event.bin");
        let scalar_bytes = 94;
        let resource_bytes = 13 * 56;
        let campus_ten = &type_zero_park[scalar_bytes..];
        let campus_tail_start = campus_ten.len() - 29;
        let empty_report_start = campus_tail_start - 70;
        let empty_report = &campus_ten[empty_report_start..campus_tail_start];
        let mut campus_thirteen = campus_ten[..campus_tail_start].to_vec();
        for _ in 0..3 {
            campus_thirteen.extend_from_slice(empty_report);
        }
        campus_thirteen.extend_from_slice(&campus_ten[campus_tail_start..]);

        let mut park = type_zero_park[..scalar_bytes].to_vec();
        park[58] = 5;
        park.extend(vec![0x5a; resource_bytes]);
        park.extend_from_slice(&campus_thirteen);
        let mut data = manager_with_park_boundary(117_013, &park, grids, tail, next);
        let records = data.decode_district_manager_park_records().unwrap();
        assert_eq!(records[0].industry_resources().unwrap().len(), 13);

        let replacement = DistrictParkIndustryResource {
            kind: DistrictParkIndustryResourceKind::Grain,
            temporary_consumption: 0x0102_0304,
            temporary_production: 0x1112_1314,
            temporary_buffer_amount: 0x2122_2324,
            temporary_incoming_transfer: 0x3132_3334,
            temporary_buffer_capacity: 0x4142_4344,
            final_consumption: 0x5152_5354,
            final_production: 0x6162_6364,
            final_buffer_amount: 0x7172_7374,
            final_incoming_transfer: 0x8182_8384,
            final_buffer_capacity: 0x9192_9394,
            trade_data: Some(DistrictParkIndustryTradeData {
                temporary_import: 0xa1a2_a3a4,
                temporary_export: 0xb1b2_b3b4,
                final_import: 0xc1c2_c3c4,
                final_export: 0xd1d2_d3d4,
            }),
        };
        let mut editor = records[0].clone().into_editor();
        editor.set_industry_resource(replacement).unwrap();
        data.replace_district_park_record(&editor).unwrap();

        let mut expected = park;
        let values = [
            replacement.temporary_consumption,
            replacement.temporary_production,
            replacement.temporary_buffer_amount,
            replacement.temporary_incoming_transfer,
            replacement.temporary_buffer_capacity,
            replacement.final_consumption,
            replacement.final_production,
            replacement.final_buffer_amount,
            replacement.final_incoming_transfer,
            replacement.final_buffer_capacity,
            replacement.trade_data.unwrap().temporary_import,
            replacement.trade_data.unwrap().temporary_export,
            replacement.trade_data.unwrap().final_import,
            replacement.trade_data.unwrap().final_export,
        ];
        for (field, value) in values.into_iter().enumerate() {
            let start = scalar_bytes + field * 4;
            expected[start..start + 4].copy_from_slice(&value.to_be_bytes());
        }
        let updated = data.decode_district_manager_park_records().unwrap();
        assert_eq!(updated[0].encode(), expected);
        assert_eq!(updated[0].industry_resources().unwrap()[0], replacement);

        let (_, park_start) = data.decode_district_manager_flags_prefix().unwrap();
        let graph = data.serialized_object_graph();
        assert_eq!(
            &graph[park_start + expected.len()..park_start + expected.len() + grids.len()],
            grids
        );
        let (_, grid_end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(
            &graph[grid_end..grid_end + tail.len() + next.len()],
            &[tail.as_slice(), next.as_slice()].concat()
        );
    }

    #[test]
    fn locates_zone_and_prop_prefixes_between_manager_tags() {
        let data = synthetic_manager_data();
        let (zone, zone_end) = data.decode_zone_manager_prefix().unwrap();
        let (props, prop_end) = data.decode_prop_manager_prefix().unwrap();
        let (trees, tree_end) = data.decode_tree_manager_prefix().unwrap();
        let (districts, district_end) = data.decode_district_manager_flags_prefix().unwrap();

        assert_eq!(zone.format_version(), 116_020);
        assert_eq!(zone.blocks().len(), 0);
        assert_eq!(zone.object_references().len(), 13);
        assert!(
            data.serialized_object_graph()[zone_end..].starts_with(&profiler_tag("PropManager"))
        );
        assert_eq!(props.format_version(), 116_020);
        assert_eq!(props.flags().len(), 65_535);
        assert!(props.prefab_names().is_empty());
        assert!(
            data.serialized_object_graph()[prop_end..].starts_with(&profiler_tag("TreeManager"))
        );
        assert_eq!(trees.flags().len(), 262_143);
        assert!(trees.trees().is_empty());
        assert!(
            data.serialized_object_graph()[tree_end..].starts_with(&profiler_tag("WeatherManager"))
        );
        assert_eq!(districts.airline_name(), Some("Airline"));
        assert_eq!(districts.airport_panel_opened(), Some(true));
        assert_eq!(districts.district_flags(), &[0; 128]);
        assert_eq!(districts.park_flags(), &[0; 128]);
        assert_eq!(district_end, data.serialized_object_graph().len());
    }

    #[test]
    fn district_grid_lookup_follows_the_park_records() {
        let mut data = synthetic_manager_data();
        let grid_start = data.serialized_object_graph.len();
        // Each 0xff EncodedArray.Byte control reconstructs 128 zero values.
        data.serialized_object_graph
            .extend(vec![0xff; 2 * 8 * 262_144 / 128]);

        let (grids, grid_end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(grid_end, data.serialized_object_graph.len());
        assert_eq!(grid_end - grid_start, 32_768);
        assert_eq!(grids.district_plane(0).unwrap()[0], 0);
        assert_eq!(grids.park_plane(7).unwrap()[262_143], 0);
        assert_eq!(grids.encode(), data.serialized_object_graph()[grid_start..]);
    }

    #[test]
    fn replaces_only_district_grid_bytes_in_manager_graph() {
        let mut data = synthetic_manager_data();
        let grid_start = data.serialized_object_graph.len();
        data.serialized_object_graph
            .extend(vec![0xff; 2 * 8 * 262_144 / 128]);
        let grid_end = data.serialized_object_graph.len();
        data.serialized_object_graph
            .extend_from_slice(&[0xfe, 0x12, 0x34]);
        let prefix = data.serialized_object_graph[..grid_start].to_vec();
        let suffix = data.serialized_object_graph[grid_end..].to_vec();
        let (grids, _) = data.decode_district_manager_grids().unwrap();
        let mut editor = grids.into_editor();
        let mut plane = vec![0; 262_144];
        plane[0] = 42;
        editor.replace_park_plane(3, &plane).unwrap();
        data.replace_district_manager_grids(&editor).unwrap();
        assert_eq!(&data.serialized_object_graph[..grid_start], prefix);
        let (updated, new_end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(updated.park_plane(3).unwrap()[0], 42);
        assert_eq!(&data.serialized_object_graph[new_end..], suffix);
        assert_eq!(
            &data.serialized_object_graph[grid_start..grid_start + 16_384],
            &[0xff; 16_384]
        );
        let stable = data.serialized_object_graph.clone();
        assert!(data.replace_district_manager_grids(&editor).is_err());
        assert_eq!(data.serialized_object_graph, stable);
    }

    #[test]
    fn empty_park_buffer_allows_grid_edit_after_active_record_version_range() {
        let mut data = synthetic_manager_data();
        data.format_version = 118_012;
        data.serialized_object_graph
            .extend(vec![0xff; 2 * 8 * 262_144 / 128]);
        let (grids, _) = data.decode_district_manager_grids().unwrap();
        let mut editor = grids.into_editor();
        let mut plane = vec![0; 262_144];
        plane[262_143] = 7;
        editor.replace_district_plane(7, &plane).unwrap();
        data.replace_district_manager_grids(&editor).unwrap();
        let (updated, end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(end, data.serialized_object_graph.len());
        assert_eq!(updated.district_plane(7).unwrap(), plane);
    }

    #[test]
    fn district_grid_lookup_crosses_an_industry_park_record() {
        let mut data = synthetic_manager_data();
        let tag = profiler_tag("DistrictManager");
        let tag_start = data
            .serialized_object_graph
            .windows(tag.len())
            .position(|window| window == tag)
            .unwrap();
        data.serialized_object_graph.truncate(tag_start + tag.len());
        let graph = &mut data.serialized_object_graph;
        write_unique_string(graph, "Airline").unwrap();
        graph.push(1);
        write_encoded_uint_array(graph, &[0; 128]);
        let mut park_flags = [0_u32; 128];
        park_flags[5] = 1;
        write_encoded_uint_array(graph, &park_flags);
        write_encoded_bool_array(graph, &vec![false; 16 * 32 * 128]);
        let park_start = graph.len();
        let mut park_record = vec![0; 94 + 13 * 56];
        park_record[58] = 5; // Type byte after the v115022 happiness block.
        graph.extend_from_slice(&park_record);
        let grid_start = graph.len();
        graph.extend(vec![0xff; 2 * 8 * 262_144 / 128]);
        let (grids, end) = data.decode_district_manager_grids().unwrap();
        assert_eq!(grid_start - park_start, park_record.len());
        assert_eq!(end, data.serialized_object_graph.len());
        assert_eq!(grids.district_plane(0).unwrap()[0], 0);
    }

    #[test]
    fn district_grid_edit_preserves_a_pedestrian_park_record() {
        let mut data = synthetic_manager_data();
        let tag = profiler_tag("DistrictManager");
        let tag_start = data
            .serialized_object_graph
            .windows(tag.len())
            .position(|window| window == tag)
            .unwrap();
        data.serialized_object_graph.truncate(tag_start + tag.len());
        let graph = &mut data.serialized_object_graph;
        write_unique_string(graph, "Airline").unwrap();
        graph.push(1);
        write_encoded_uint_array(graph, &[0; 128]);
        let mut park_flags = [0_u32; 128];
        park_flags[5] = 1;
        write_encoded_uint_array(graph, &park_flags);
        write_encoded_bool_array(graph, &vec![false; 16 * 32 * 128]);
        let park_start = graph.len();
        let mut park_record = vec![0; 94];
        park_record[58] = 15;
        park_record.extend_from_slice(&0_i32.to_be_bytes()); // Zero material groups.
        park_record.push(0); // Zero random service points.
        park_record.extend([0x5a; 4 * 46]); // Four private-data blocks.
        park_record.extend([0, 1]); // Zone focus and dirty flag.
        park_record.extend([0x5a; 8]); // Goods sold.
        park_record.extend([0, 1]); // Service-point flags.
        graph.extend_from_slice(&park_record);
        graph.extend(vec![0xff; 2 * 8 * 262_144 / 128]);
        let (grids, _) = data.decode_district_manager_grids().unwrap();
        let mut editor = grids.into_editor();
        let mut plane = vec![0; 262_144];
        plane[0] = 7;
        editor.replace_district_plane(0, &plane).unwrap();
        data.replace_district_manager_grids(&editor).unwrap();
        assert_eq!(
            &data.serialized_object_graph[park_start..park_start + park_record.len()],
            park_record
        );
        let (updated, _) = data.decode_district_manager_grids().unwrap();
        assert_eq!(updated.district_plane(0).unwrap()[0], 7);
    }

    #[test]
    fn rejects_missing_manager_boundary_tag() {
        let mut data = synthetic_manager_data();
        let tree_tag = profiler_tag("TreeManager");
        let offset = data
            .serialized_object_graph
            .windows(tree_tag.len())
            .position(|window| window == tree_tag)
            .unwrap();
        data.serialized_object_graph[offset] = 0xfd;
        assert!(data.decode_prop_manager_prefix().is_err());
    }
}
