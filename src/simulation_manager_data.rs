use crate::CrpError;
use crate::data_serializer::{DataSerializerStream, Reader, write_u24_be, write_unique_string};

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
    leading_shared_type_names: Vec<String>,
    leading_shared_type_bytes: Vec<u8>,
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
        let (leading_shared_type_names, leading_shared_type_bytes) =
            read_leading_shared_types(&mut reader)?;
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
        let manager_objects =
            read_manager_array(&mut reader, &stream.root_type, &leading_shared_type_names)?;
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
            leading_shared_type_names,
            leading_shared_type_bytes,
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

    /// Returns shared types declared before the scalar root fields.
    pub fn leading_shared_type_names(&self) -> &[String] {
        &self.leading_shared_type_names
    }

    /// Returns the uninterpreted queued manager-object serialization bytes.
    pub fn serialized_object_graph(&self) -> &[u8] {
        &self.serialized_object_graph
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
        root_data.extend_from_slice(&self.leading_shared_type_bytes);
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
        Ok(root_data)
    }
}

fn read_manager_array(
    reader: &mut Reader<'_>,
    root_type: &str,
    leading_shared_types: &[String],
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
    shared_types.extend_from_slice(leading_shared_types);
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

fn read_leading_shared_types(reader: &mut Reader<'_>) -> Result<(Vec<String>, Vec<u8>), CrpError> {
    let start = reader.offset();
    let mut names = Vec::new();
    while reader.peek_u8() == Some(0xfe) {
        let marker = reader.read_u8()?;
        if marker != 0xfe {
            return Err(reader.invalid("invalid leading shared-type marker"));
        }
        let name = reader
            .read_unique_string()?
            .ok_or_else(|| reader.invalid("leading shared-type name cannot be null"))?;
        if names.len() >= MAX_SHARED_TYPES - 1 {
            return Err(reader.invalid("leading shared-type table exceeds the game limit"));
        }
        names.push(name);
    }
    let bytes = reader.consumed_bytes()[start..].to_vec();
    Ok((names, bytes))
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
