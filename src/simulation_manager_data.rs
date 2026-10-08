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

/// A non-null reference in `SimulationManager+Data`'s manager-data array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagerObjectReference {
    /// Object-list ID assigned by DataSerializer.
    pub object_id: u32,
    /// Assembly-qualified runtime type name resolved from the shared-type list.
    pub type_name: String,
}

/// One entry in `SimulationManager.m_serializableDataStorage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializableDataEntry {
    /// Data key stored by the game or a mod.
    pub key: String,
    /// Serialized bytes; a null value is preserved as `None`.
    pub data: Option<Vec<u8>>,
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
    use super::{ManagerObjectReference, SimulationManagerData};
    use crate::data_serializer::write_unique_string;
    use crate::district_manager_data::{
        DistrictParkIndustryResource, DistrictParkIndustryResourceKind,
        DistrictParkIndustryTradeData,
    };
    use crate::encoded_array::{
        write_encoded_bool_array, write_encoded_uint_array, write_encoded_ushort_array,
    };

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
