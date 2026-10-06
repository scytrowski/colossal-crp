use crate::CrpError;
use crate::GameAreaManagerData;
use crate::InstanceManagerData;
use crate::InstanceManagerEditor;
use crate::PathManagerData;
use crate::TerrainManagerData;
use crate::WaterSimulationData;
use crate::WaterSimulationEditor;
use crate::data_serializer::{DataSerializerStream, Reader, write_u24_be, write_unique_string};
use crate::{
    BuildingFlagsPrefix, BuildingManagerObjectHeadersPrefix, BuildingPrefabPrefix,
    BuildingProblemPrefix,
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

    /// Decodes the first two BuildingManager flag arrays after the NetManager body.
    /// The returned graph offset is before the guide and milestone counts.
    pub fn decode_building_flags_prefix(
        &self,
        save_app_version: Option<u32>,
    ) -> Result<(BuildingFlagsPrefix, usize), CrpError> {
        let (_, net_end) = self.decode_net_manager_tail_prefix(save_app_version)?;
        let mut reader = Reader::new(&self.serialized_object_graph);
        reader.read_bytes(net_end)?;
        let mut strings = vec![
            "SimulationManager".to_owned(),
            "GameAreaManager".to_owned(),
            "TerrainManager".to_owned(),
            "WaterSimulation".to_owned(),
            "InstanceManager".to_owned(),
            "PathManager".to_owned(),
            "NetManager".to_owned(),
        ];
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
    /// Before version 115000 this returns an empty prefix at the same offset.
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
            &flags,
            reader.remaining_bytes(),
        )?;
        reader.read_bytes(used)?;
        Ok((data, reader.offset()))
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
