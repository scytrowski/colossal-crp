use crate::asset_io::{
    PayloadReader, write_bool, write_f32_le, write_i32_le, write_i64_le, write_string, write_u32_le,
};
use crate::{Asset, CrpError};

const MAP_METADATA_TYPE: i32 = 100;
const MAP_METADATA_FIELD_COUNT: usize = 25;
const SYSTEM_MAP_METADATA_TYPE: i32 = 102;
const SYSTEM_MAP_THEME_MAP_METADATA_TYPE: i32 = 107;
const SYSTEM_MAP_METADATA_FIELD_COUNT: usize = 4;
const DISTRICT_STYLE_METADATA_TYPE: i32 = 105;
const DISTRICT_STYLE_METADATA_FIELD_COUNT: usize = 8;

const SYSTEM_MAP_FIELD_NAMES: [&str; SYSTEM_MAP_METADATA_FIELD_COUNT] =
    ["environment", "timeStamp", "imageRef", "assetRef"];
const SYSTEM_MAP_FIELD_TYPES: [&str; SYSTEM_MAP_METADATA_FIELD_COUNT] = [
    "System.String, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.DateTime, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "ColossalFramework.Packaging.Package+Asset, ColossalManaged, Version=0.3.0.0, Culture=neutral, PublicKeyToken=null",
    "ColossalFramework.Packaging.Package+Asset, ColossalManaged, Version=0.3.0.0, Culture=neutral, PublicKeyToken=null",
];

const DISTRICT_STYLE_FIELD_NAMES: [&str; DISTRICT_STYLE_METADATA_FIELD_COUNT] = [
    "builtin",
    "name",
    "steamTags",
    "assets",
    "timeStamp",
    "imageRef",
    "steamPreviewRef",
    "assetRef",
];
const DISTRICT_STYLE_FIELD_TYPES: [&str; DISTRICT_STYLE_METADATA_FIELD_COUNT] = [
    "System.Boolean, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.String, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.String[], mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.String[], mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.DateTime, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "ColossalFramework.Packaging.Package+Asset, ColossalManaged, Version=0.3.0.0, Culture=neutral, PublicKeyToken=null",
    "ColossalFramework.Packaging.Package+Asset, ColossalManaged, Version=0.3.0.0, Culture=neutral, PublicKeyToken=null",
    "ColossalFramework.Packaging.Package+Asset, ColossalManaged, Version=0.3.0.0, Culture=neutral, PublicKeyToken=null",
];

const FIELD_NAMES: [&str; MAP_METADATA_FIELD_COUNT] = [
    "builtin",
    "environment",
    "mapThemeRef",
    "mapName",
    "timeStamp",
    "imageRef",
    "steamPreviewRef",
    "publishedHash",
    "buildableArea",
    "maxResourceAmount",
    "oilAmount",
    "oreAmount",
    "forestAmount",
    "fertileLandAmount",
    "waterAmount",
    "incomingRoads",
    "outgoingRoads",
    "incomingTrainTracks",
    "outgoingTrainTracks",
    "incomingShipPath",
    "outgoingShipPath",
    "incomingPlanePath",
    "outgoingPlanePath",
    "mods",
    "assetRef",
];

const FIELD_TYPES: [&str; MAP_METADATA_FIELD_COUNT] = [
    "System.Boolean, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.String, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.String, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.String, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.DateTime, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "ColossalFramework.Packaging.Package+Asset, ColossalManaged, Version=0.3.0.0, Culture=neutral, PublicKeyToken=null",
    "ColossalFramework.Packaging.Package+Asset, ColossalManaged, Version=0.3.0.0, Culture=neutral, PublicKeyToken=null",
    "System.Int32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.Int32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.Single, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.UInt32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.UInt32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.UInt32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.UInt32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.UInt32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.Int32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.Int32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.Int32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.Int32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.Int32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.Int32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.Int32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "System.Int32, mscorlib, Version=2.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    "ModInfo[], Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null",
    "ColossalFramework.Packaging.Package+Asset, ColossalManaged, Version=0.3.0.0, Culture=neutral, PublicKeyToken=null",
];

/// A mod entry stored in a map's `MapMetaData` asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModInfo {
    /// The mod name recorded in the map metadata.
    pub name: String,
    /// The recorded published file ID, or `-1` when no ID is stored.
    pub published_file_id: i64,
}

/// Typed view of the reflected type-100 `MapMetaData` payload.
#[derive(Debug, Clone, PartialEq)]
pub struct MapMetaData {
    /// Assembly-qualified runtime type stored in the payload.
    pub runtime_type: String,
    /// Object name stored in the reflected-object header.
    pub object_name: String,
    /// Whether the map is a built-in asset.
    pub builtin: Option<bool>,
    /// Map environment name.
    pub environment: Option<String>,
    /// Map-theme reference stored as a string; its semantics are not established.
    pub map_theme_ref: Option<String>,
    /// User-visible map name.
    pub map_name: Option<String>,
    /// Timestamp in .NET round-trip (`O`) string format.
    pub time_stamp: Option<String>,
    /// Referenced image asset checksum.
    pub image_ref: Option<String>,
    /// Referenced Steam preview asset checksum.
    pub steam_preview_ref: Option<String>,
    /// Published hash stored by the game.
    pub published_hash: Option<i32>,
    /// Buildable area value stored by the game.
    pub buildable_area: Option<i32>,
    /// Maximum resource amount.
    pub max_resource_amount: Option<f32>,
    /// Oil resource amount.
    pub oil_amount: Option<u32>,
    /// Ore resource amount.
    pub ore_amount: Option<u32>,
    /// Forest resource amount.
    pub forest_amount: Option<u32>,
    /// Fertile land resource amount.
    pub fertile_land_amount: Option<u32>,
    /// Water resource amount.
    pub water_amount: Option<u32>,
    /// Incoming road count.
    pub incoming_roads: Option<i32>,
    /// Outgoing road count.
    pub outgoing_roads: Option<i32>,
    /// Incoming train-track count.
    pub incoming_train_tracks: Option<i32>,
    /// Outgoing train-track count.
    pub outgoing_train_tracks: Option<i32>,
    /// Incoming ship-path count.
    pub incoming_ship_path: Option<i32>,
    /// Outgoing ship-path count.
    pub outgoing_ship_path: Option<i32>,
    /// Incoming plane-path count.
    pub incoming_plane_path: Option<i32>,
    /// Outgoing plane-path count.
    pub outgoing_plane_path: Option<i32>,
    /// Mods recorded when the map was saved.
    pub mods: Option<Vec<ModInfo>>,
    /// Referenced map data asset checksum.
    pub asset_ref: Option<String>,
    /// Bytes remaining after the known fields, preserved for round-tripping.
    pub trailing_bytes: Vec<u8>,
    field_types: Vec<Option<String>>,
}

impl MapMetaData {
    /// Decodes an outer type-100 asset using the observed `MapMetaData` schema.
    ///
    /// The schema is confirmed against game-produced map assets in the supplied
    /// corpus. Other reflected metadata runtime types are not accepted here.
    pub fn decode(asset: &Asset) -> Result<Self, CrpError> {
        if asset.asset_type != MAP_METADATA_TYPE {
            return Err(CrpError::AssetTypeMismatch {
                name: asset.name.clone(),
                expected: MAP_METADATA_TYPE,
                actual: asset.asset_type,
            });
        }

        let mut reader = PayloadReader::new(&asset.name, &asset.data);
        if reader.read_bool()? {
            return Err(reader.invalid("null MapMetaData root object is unsupported"));
        }
        let runtime_type = reader.read_string()?;
        if runtime_type.split(',').next() != Some("MapMetaData") {
            return Err(reader.invalid(format!(
                "unsupported reflected runtime type {runtime_type:?}"
            )));
        }
        let object_name = reader.read_string()?;
        let field_count = reader.read_i32_le()?;
        if field_count != MAP_METADATA_FIELD_COUNT as i32 {
            return Err(reader.invalid(format!(
                "MapMetaData field count is {field_count}, expected {MAP_METADATA_FIELD_COUNT}"
            )));
        }

        let mut field_types = vec![None; MAP_METADATA_FIELD_COUNT];
        let mut output = Self {
            runtime_type,
            object_name,
            builtin: None,
            environment: None,
            map_theme_ref: None,
            map_name: None,
            time_stamp: None,
            image_ref: None,
            steam_preview_ref: None,
            published_hash: None,
            buildable_area: None,
            max_resource_amount: None,
            oil_amount: None,
            ore_amount: None,
            forest_amount: None,
            fertile_land_amount: None,
            water_amount: None,
            incoming_roads: None,
            outgoing_roads: None,
            incoming_train_tracks: None,
            outgoing_train_tracks: None,
            incoming_ship_path: None,
            outgoing_ship_path: None,
            incoming_plane_path: None,
            outgoing_plane_path: None,
            mods: None,
            asset_ref: None,
            trailing_bytes: Vec::new(),
            field_types: Vec::new(),
        };

        for index in 0..MAP_METADATA_FIELD_COUNT {
            let Some(field_type) =
                read_field_header(&mut reader, index, &FIELD_NAMES, &FIELD_TYPES)?
            else {
                field_types[index] = None;
                continue;
            };
            field_types[index] = Some(field_type);
            match index {
                0 => output.builtin = Some(reader.read_bool()?),
                1 => output.environment = Some(reader.read_string()?),
                2 => output.map_theme_ref = Some(reader.read_string()?),
                3 => output.map_name = Some(reader.read_string()?),
                4 => output.time_stamp = Some(reader.read_string()?),
                5 => output.image_ref = Some(reader.read_string()?),
                6 => output.steam_preview_ref = Some(reader.read_string()?),
                7 => output.published_hash = Some(reader.read_i32_le()?),
                8 => output.buildable_area = Some(reader.read_i32_le()?),
                9 => output.max_resource_amount = Some(reader.read_f32_le()?),
                10 => output.oil_amount = Some(reader.read_u32_le()?),
                11 => output.ore_amount = Some(reader.read_u32_le()?),
                12 => output.forest_amount = Some(reader.read_u32_le()?),
                13 => output.fertile_land_amount = Some(reader.read_u32_le()?),
                14 => output.water_amount = Some(reader.read_u32_le()?),
                15 => output.incoming_roads = Some(reader.read_i32_le()?),
                16 => output.outgoing_roads = Some(reader.read_i32_le()?),
                17 => output.incoming_train_tracks = Some(reader.read_i32_le()?),
                18 => output.outgoing_train_tracks = Some(reader.read_i32_le()?),
                19 => output.incoming_ship_path = Some(reader.read_i32_le()?),
                20 => output.outgoing_ship_path = Some(reader.read_i32_le()?),
                21 => output.incoming_plane_path = Some(reader.read_i32_le()?),
                22 => output.outgoing_plane_path = Some(reader.read_i32_le()?),
                23 => output.mods = Some(read_mods(&mut reader)?),
                24 => output.asset_ref = Some(reader.read_string()?),
                _ => return Err(reader.invalid("unexpected MapMetaData field index")),
            }
        }
        output.field_types = field_types;
        output.trailing_bytes = reader.read_remaining().to_vec();
        Ok(output)
    }

    /// Encodes this value using the observed type-100 reflected field layout.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut output = Vec::new();
        write_bool(&mut output, false);
        write_string(&mut output, &self.runtime_type)?;
        write_string(&mut output, &self.object_name)?;
        write_i32_le(&mut output, MAP_METADATA_FIELD_COUNT as i32);

        for index in 0..MAP_METADATA_FIELD_COUNT {
            let is_null = self.field_is_null(index);
            write_bool(&mut output, is_null);
            if is_null {
                continue;
            }
            let field_type = match self.field_types[index].as_deref() {
                Some(field_type) => field_type,
                None => FIELD_TYPES[index],
            };
            write_string(&mut output, field_type)?;
            write_string(&mut output, FIELD_NAMES[index])?;
            self.write_field_value(index, &mut output)?;
        }
        output.extend_from_slice(&self.trailing_bytes);
        Ok(output)
    }

    fn field_is_null(&self, index: usize) -> bool {
        match index {
            0 => self.builtin.is_none(),
            1 => self.environment.is_none(),
            2 => self.map_theme_ref.is_none(),
            3 => self.map_name.is_none(),
            4 => self.time_stamp.is_none(),
            5 => self.image_ref.is_none(),
            6 => self.steam_preview_ref.is_none(),
            7 => self.published_hash.is_none(),
            8 => self.buildable_area.is_none(),
            9 => self.max_resource_amount.is_none(),
            10 => self.oil_amount.is_none(),
            11 => self.ore_amount.is_none(),
            12 => self.forest_amount.is_none(),
            13 => self.fertile_land_amount.is_none(),
            14 => self.water_amount.is_none(),
            15 => self.incoming_roads.is_none(),
            16 => self.outgoing_roads.is_none(),
            17 => self.incoming_train_tracks.is_none(),
            18 => self.outgoing_train_tracks.is_none(),
            19 => self.incoming_ship_path.is_none(),
            20 => self.outgoing_ship_path.is_none(),
            21 => self.incoming_plane_path.is_none(),
            22 => self.outgoing_plane_path.is_none(),
            23 => self.mods.is_none(),
            24 => self.asset_ref.is_none(),
            _ => true,
        }
    }

    fn write_field_value(&self, index: usize, output: &mut Vec<u8>) -> Result<(), CrpError> {
        match index {
            0 => write_bool(output, *self.required_field(self.builtin.as_ref(), index)?),
            1 => write_string(
                output,
                self.required_field(self.environment.as_deref(), index)?,
            )?,
            2 => write_string(
                output,
                self.required_field(self.map_theme_ref.as_deref(), index)?,
            )?,
            3 => write_string(
                output,
                self.required_field(self.map_name.as_deref(), index)?,
            )?,
            4 => write_string(
                output,
                self.required_field(self.time_stamp.as_deref(), index)?,
            )?,
            5 => write_string(
                output,
                self.required_field(self.image_ref.as_deref(), index)?,
            )?,
            6 => write_string(
                output,
                self.required_field(self.steam_preview_ref.as_deref(), index)?,
            )?,
            7 => write_i32_le(
                output,
                *self.required_field(self.published_hash.as_ref(), index)?,
            ),
            8 => write_i32_le(
                output,
                *self.required_field(self.buildable_area.as_ref(), index)?,
            ),
            9 => write_f32_le(
                output,
                *self.required_field(self.max_resource_amount.as_ref(), index)?,
            ),
            10 => write_u32_le(
                output,
                *self.required_field(self.oil_amount.as_ref(), index)?,
            ),
            11 => write_u32_le(
                output,
                *self.required_field(self.ore_amount.as_ref(), index)?,
            ),
            12 => write_u32_le(
                output,
                *self.required_field(self.forest_amount.as_ref(), index)?,
            ),
            13 => write_u32_le(
                output,
                *self.required_field(self.fertile_land_amount.as_ref(), index)?,
            ),
            14 => write_u32_le(
                output,
                *self.required_field(self.water_amount.as_ref(), index)?,
            ),
            15 => write_i32_le(
                output,
                *self.required_field(self.incoming_roads.as_ref(), index)?,
            ),
            16 => write_i32_le(
                output,
                *self.required_field(self.outgoing_roads.as_ref(), index)?,
            ),
            17 => write_i32_le(
                output,
                *self.required_field(self.incoming_train_tracks.as_ref(), index)?,
            ),
            18 => write_i32_le(
                output,
                *self.required_field(self.outgoing_train_tracks.as_ref(), index)?,
            ),
            19 => write_i32_le(
                output,
                *self.required_field(self.incoming_ship_path.as_ref(), index)?,
            ),
            20 => write_i32_le(
                output,
                *self.required_field(self.outgoing_ship_path.as_ref(), index)?,
            ),
            21 => write_i32_le(
                output,
                *self.required_field(self.incoming_plane_path.as_ref(), index)?,
            ),
            22 => write_i32_le(
                output,
                *self.required_field(self.outgoing_plane_path.as_ref(), index)?,
            ),
            23 => write_mods(output, self.required_field(self.mods.as_deref(), index)?)?,
            24 => write_string(
                output,
                self.required_field(self.asset_ref.as_deref(), index)?,
            )?,
            _ => {
                return Err(CrpError::InvalidAssetPayload {
                    name: self.object_name.clone(),
                    offset: 0,
                    reason: "unexpected MapMetaData field index".to_owned(),
                });
            }
        }
        Ok(())
    }

    fn required_field<'a, T: ?Sized>(
        &'a self,
        value: Option<&'a T>,
        index: usize,
    ) -> Result<&'a T, CrpError> {
        value.ok_or_else(|| CrpError::InvalidAssetPayload {
            name: self.object_name.clone(),
            offset: 0,
            reason: format!(
                "field {:?} is marked present but has no value",
                FIELD_NAMES[index]
            ),
        })
    }
}

/// Typed view of a reflected `SystemMapMetaData` payload.
///
/// This schema is DLL-confirmed and has been checked against map-produced
/// outer types 102 and 107 in the supplied corpus. The outer type is retained
/// because the type-107 sample advertises this runtime class despite the DLL
/// enum naming a different subtype. Trailing bytes are preserved verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemMapMetaData {
    /// Outer package asset type (`102` or the observed mismatched `107`).
    pub outer_asset_type: i32,
    /// Assembly-qualified runtime type stored in the payload.
    pub runtime_type: String,
    /// Object name stored in the reflected-object header.
    pub object_name: String,
    /// Environment name.
    pub environment: Option<String>,
    /// Timestamp in .NET round-trip (`O`) string format.
    pub time_stamp: Option<String>,
    /// Referenced image asset checksum.
    pub image_ref: Option<String>,
    /// Referenced asset checksum inherited from `MetaData`.
    pub asset_ref: Option<String>,
    /// Bytes after the known fields, preserved for round-tripping.
    pub trailing_bytes: Vec<u8>,
    field_types: Vec<Option<String>>,
}

impl SystemMapMetaData {
    /// Decodes an outer type-102 or observed type-107 `SystemMapMetaData` asset.
    ///
    /// Returns an error for a different asset type, a null or unexpected root,
    /// an unsupported field count, or malformed field data. The original
    /// assembly-qualified field type strings and trailing bytes are retained.
    pub fn decode(asset: &Asset) -> Result<Self, CrpError> {
        if !matches!(
            asset.asset_type,
            SYSTEM_MAP_METADATA_TYPE | SYSTEM_MAP_THEME_MAP_METADATA_TYPE
        ) {
            return Err(CrpError::AssetTypeMismatch {
                name: asset.name.clone(),
                expected: SYSTEM_MAP_METADATA_TYPE,
                actual: asset.asset_type,
            });
        }

        let mut reader = PayloadReader::new(&asset.name, &asset.data);
        let (runtime_type, object_name) = read_reflected_object_header(
            &mut reader,
            "SystemMapMetaData",
            SYSTEM_MAP_METADATA_FIELD_COUNT,
        )?;
        let mut field_types = vec![None; SYSTEM_MAP_METADATA_FIELD_COUNT];
        let mut output = Self {
            outer_asset_type: asset.asset_type,
            runtime_type,
            object_name,
            environment: None,
            time_stamp: None,
            image_ref: None,
            asset_ref: None,
            trailing_bytes: Vec::new(),
            field_types: Vec::new(),
        };

        for (index, field_type_slot) in field_types.iter_mut().enumerate() {
            let Some(field_type) = read_field_header(
                &mut reader,
                index,
                &SYSTEM_MAP_FIELD_NAMES,
                &SYSTEM_MAP_FIELD_TYPES,
            )?
            else {
                continue;
            };
            *field_type_slot = Some(field_type);
            match index {
                0 => output.environment = Some(reader.read_string()?),
                1 => output.time_stamp = Some(reader.read_string()?),
                2 => output.image_ref = Some(reader.read_string()?),
                3 => output.asset_ref = Some(reader.read_string()?),
                _ => return Err(reader.invalid("unexpected SystemMapMetaData field index")),
            }
        }
        output.field_types = field_types;
        output.trailing_bytes = reader.read_remaining().to_vec();
        Ok(output)
    }

    /// Encodes the known `SystemMapMetaData` fields and preserves trailing bytes.
    ///
    /// The caller should write the result using the `outer_asset_type` field.
    /// Field nullness and original assembly-qualified field type strings are
    /// retained; fields changed from null use the DLL-derived schema defaults.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if !matches!(
            self.outer_asset_type,
            SYSTEM_MAP_METADATA_TYPE | SYSTEM_MAP_THEME_MAP_METADATA_TYPE
        ) {
            return Err(CrpError::InvalidAssetPayload {
                name: self.object_name.clone(),
                offset: 0,
                reason: format!(
                    "unsupported outer asset type {} for SystemMapMetaData",
                    self.outer_asset_type
                ),
            });
        }

        let mut output = Vec::new();
        write_reflected_object_header(
            &mut output,
            &self.runtime_type,
            &self.object_name,
            SYSTEM_MAP_METADATA_FIELD_COUNT,
        )?;

        for index in 0..SYSTEM_MAP_METADATA_FIELD_COUNT {
            let field_name = SYSTEM_MAP_FIELD_NAMES[index];
            let value = match index {
                0 => self.environment.as_deref(),
                1 => self.time_stamp.as_deref(),
                2 => self.image_ref.as_deref(),
                3 => self.asset_ref.as_deref(),
                _ => None,
            };
            write_bool(&mut output, value.is_none());
            let Some(value) = value else {
                continue;
            };
            let field_type = self
                .field_types
                .get(index)
                .and_then(Option::as_deref)
                .unwrap_or(SYSTEM_MAP_FIELD_TYPES[index]);
            write_string(&mut output, field_type)?;
            write_string(&mut output, field_name)?;
            write_string(&mut output, value)?;
        }
        output.extend_from_slice(&self.trailing_bytes);
        Ok(output)
    }
}

/// Typed view of a reflected type-105 `DistrictStyleMetaData` payload.
///
/// The schema is DLL-confirmed and sample-validated against the supplied map
/// corpus. Null fields and array values are retained, as are trailing bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictStyleMetaData {
    /// Assembly-qualified runtime type stored in the payload.
    pub runtime_type: String,
    /// Object name stored in the reflected-object header.
    pub object_name: String,
    /// Whether this is a built-in district style.
    pub builtin: Option<bool>,
    /// District style name.
    pub name: Option<String>,
    /// Steam tags; null and an empty array are distinct values.
    pub steam_tags: Option<Vec<String>>,
    /// Included asset names.
    pub assets: Option<Vec<String>>,
    /// Timestamp in .NET round-trip (`O`) string format.
    pub time_stamp: Option<String>,
    /// Referenced image asset checksum.
    pub image_ref: Option<String>,
    /// Referenced Steam preview asset checksum.
    pub steam_preview_ref: Option<String>,
    /// Referenced asset checksum inherited from `MetaData`.
    pub asset_ref: Option<String>,
    /// Bytes after the known fields, preserved for round-tripping.
    pub trailing_bytes: Vec<u8>,
    field_types: Vec<Option<String>>,
}

impl DistrictStyleMetaData {
    /// Decodes an outer type-105 `DistrictStyleMetaData` asset.
    ///
    /// Returns an error for a different asset type, a null or unexpected root,
    /// an unsupported field count, or malformed field data. The original
    /// assembly-qualified field type strings and trailing bytes are retained.
    pub fn decode(asset: &Asset) -> Result<Self, CrpError> {
        if asset.asset_type != DISTRICT_STYLE_METADATA_TYPE {
            return Err(CrpError::AssetTypeMismatch {
                name: asset.name.clone(),
                expected: DISTRICT_STYLE_METADATA_TYPE,
                actual: asset.asset_type,
            });
        }

        let mut reader = PayloadReader::new(&asset.name, &asset.data);
        let (runtime_type, object_name) = read_reflected_object_header(
            &mut reader,
            "DistrictStyleMetaData",
            DISTRICT_STYLE_METADATA_FIELD_COUNT,
        )?;
        let mut field_types = vec![None; DISTRICT_STYLE_METADATA_FIELD_COUNT];
        let mut output = Self {
            runtime_type,
            object_name,
            builtin: None,
            name: None,
            steam_tags: None,
            assets: None,
            time_stamp: None,
            image_ref: None,
            steam_preview_ref: None,
            asset_ref: None,
            trailing_bytes: Vec::new(),
            field_types: Vec::new(),
        };

        for (index, field_type_slot) in field_types.iter_mut().enumerate() {
            let Some(field_type) = read_field_header(
                &mut reader,
                index,
                &DISTRICT_STYLE_FIELD_NAMES,
                &DISTRICT_STYLE_FIELD_TYPES,
            )?
            else {
                continue;
            };
            *field_type_slot = Some(field_type);
            match index {
                0 => output.builtin = Some(reader.read_bool()?),
                1 => output.name = Some(reader.read_string()?),
                2 => output.steam_tags = Some(read_string_array(&mut reader)?),
                3 => output.assets = Some(read_string_array(&mut reader)?),
                4 => output.time_stamp = Some(reader.read_string()?),
                5 => output.image_ref = Some(reader.read_string()?),
                6 => output.steam_preview_ref = Some(reader.read_string()?),
                7 => output.asset_ref = Some(reader.read_string()?),
                _ => return Err(reader.invalid("unexpected DistrictStyleMetaData field index")),
            }
        }
        output.field_types = field_types;
        output.trailing_bytes = reader.read_remaining().to_vec();
        Ok(output)
    }

    /// Encodes the known `DistrictStyleMetaData` fields and preserves trailing bytes.
    ///
    /// Field nullness and original assembly-qualified field type strings are
    /// retained; fields changed from null use the DLL-derived schema defaults.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let mut output = Vec::new();
        write_reflected_object_header(
            &mut output,
            &self.runtime_type,
            &self.object_name,
            DISTRICT_STYLE_METADATA_FIELD_COUNT,
        )?;

        for index in 0..DISTRICT_STYLE_METADATA_FIELD_COUNT {
            let value_is_null = match index {
                0 => self.builtin.is_none(),
                1 => self.name.is_none(),
                2 => self.steam_tags.is_none(),
                3 => self.assets.is_none(),
                4 => self.time_stamp.is_none(),
                5 => self.image_ref.is_none(),
                6 => self.steam_preview_ref.is_none(),
                7 => self.asset_ref.is_none(),
                _ => true,
            };
            write_bool(&mut output, value_is_null);
            if value_is_null {
                continue;
            }
            let field_type = self
                .field_types
                .get(index)
                .and_then(Option::as_deref)
                .unwrap_or(DISTRICT_STYLE_FIELD_TYPES[index]);
            write_string(&mut output, field_type)?;
            write_string(&mut output, DISTRICT_STYLE_FIELD_NAMES[index])?;
            match index {
                0 => write_bool(
                    &mut output,
                    *required_schema_field(
                        &self.object_name,
                        &DISTRICT_STYLE_FIELD_NAMES,
                        self.builtin.as_ref(),
                        index,
                    )?,
                ),
                1 => write_string(
                    &mut output,
                    required_schema_field(
                        &self.object_name,
                        &DISTRICT_STYLE_FIELD_NAMES,
                        self.name.as_deref(),
                        index,
                    )?,
                )?,
                2 => write_string_array(
                    &mut output,
                    required_schema_field(
                        &self.object_name,
                        &DISTRICT_STYLE_FIELD_NAMES,
                        self.steam_tags.as_deref(),
                        index,
                    )?,
                )?,
                3 => write_string_array(
                    &mut output,
                    required_schema_field(
                        &self.object_name,
                        &DISTRICT_STYLE_FIELD_NAMES,
                        self.assets.as_deref(),
                        index,
                    )?,
                )?,
                4 => write_string(
                    &mut output,
                    required_schema_field(
                        &self.object_name,
                        &DISTRICT_STYLE_FIELD_NAMES,
                        self.time_stamp.as_deref(),
                        index,
                    )?,
                )?,
                5 => write_string(
                    &mut output,
                    required_schema_field(
                        &self.object_name,
                        &DISTRICT_STYLE_FIELD_NAMES,
                        self.image_ref.as_deref(),
                        index,
                    )?,
                )?,
                6 => write_string(
                    &mut output,
                    required_schema_field(
                        &self.object_name,
                        &DISTRICT_STYLE_FIELD_NAMES,
                        self.steam_preview_ref.as_deref(),
                        index,
                    )?,
                )?,
                7 => write_string(
                    &mut output,
                    required_schema_field(
                        &self.object_name,
                        &DISTRICT_STYLE_FIELD_NAMES,
                        self.asset_ref.as_deref(),
                        index,
                    )?,
                )?,
                _ => {
                    return Err(CrpError::InvalidAssetPayload {
                        name: self.object_name.clone(),
                        offset: 0,
                        reason: "unexpected DistrictStyleMetaData field index".to_owned(),
                    });
                }
            }
        }
        output.extend_from_slice(&self.trailing_bytes);
        Ok(output)
    }
}

fn read_reflected_object_header(
    reader: &mut PayloadReader<'_>,
    expected_type: &str,
    expected_field_count: usize,
) -> Result<(String, String), CrpError> {
    if reader.read_bool()? {
        return Err(reader.invalid(format!("null {expected_type} root object is unsupported")));
    }
    let runtime_type = reader.read_string()?;
    if runtime_type.split(',').next() != Some(expected_type) {
        return Err(reader.invalid(format!(
            "unsupported reflected runtime type {runtime_type:?}"
        )));
    }
    let object_name = reader.read_string()?;
    let field_count = reader.read_i32_le()?;
    if field_count != expected_field_count as i32 {
        return Err(reader.invalid(format!(
            "{expected_type} field count is {field_count}, expected {expected_field_count}"
        )));
    }
    Ok((runtime_type, object_name))
}

fn write_reflected_object_header(
    output: &mut Vec<u8>,
    runtime_type: &str,
    object_name: &str,
    field_count: usize,
) -> Result<(), CrpError> {
    write_bool(output, false);
    write_string(output, runtime_type)?;
    write_string(output, object_name)?;
    let field_count = i32::try_from(field_count)
        .map_err(|_| CrpError::ArithmeticOverflow("reflected metadata field count"))?;
    write_i32_le(output, field_count);
    Ok(())
}

fn required_schema_field<'a, T: ?Sized>(
    object_name: &str,
    field_names: &[&str],
    value: Option<&'a T>,
    index: usize,
) -> Result<&'a T, CrpError> {
    value.ok_or_else(|| CrpError::InvalidAssetPayload {
        name: object_name.to_owned(),
        offset: 0,
        reason: format!(
            "field {:?} is marked present but has no value",
            field_names.get(index).copied().unwrap_or("unknown")
        ),
    })
}

fn read_field_header(
    reader: &mut PayloadReader<'_>,
    index: usize,
    field_names: &[&str],
    field_types: &[&str],
) -> Result<Option<String>, CrpError> {
    if reader.read_bool()? {
        return Ok(None);
    }
    let field_type = reader.read_string()?;
    let field_name = reader.read_string()?;
    let expected_field_name = field_names
        .get(index)
        .ok_or_else(|| reader.invalid("reflected field index is outside the schema"))?;
    let declared_type = field_types
        .get(index)
        .ok_or_else(|| reader.invalid("reflected field type index is outside the schema"))?;
    let expected_type = declared_type
        .split_once(',')
        .map_or(*declared_type, |(base, _)| base);
    let actual_type = field_type
        .split_once(',')
        .map_or(field_type.as_str(), |(base, _)| base);
    if field_name != *expected_field_name || actual_type != expected_type {
        return Err(reader.invalid(format!(
            "unexpected reflected field {field_name:?} of type {actual_type:?} at index {index}; expected {:?} of type {expected_type:?}",
            expected_field_name
        )));
    }
    Ok(Some(field_type))
}

fn read_string_array(reader: &mut PayloadReader<'_>) -> Result<Vec<String>, CrpError> {
    let count = reader.read_i32_le()?;
    if count < 0 {
        return Err(reader.invalid(format!("negative string-array count {count}")));
    }
    let count = usize::try_from(count)
        .map_err(|_| reader.invalid("string-array count does not fit this platform"))?;
    if count > reader.remaining() {
        return Err(reader.invalid("string-array count exceeds remaining payload"));
    }

    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(reader.read_string()?);
    }
    Ok(values)
}

fn write_string_array(output: &mut Vec<u8>, values: &[String]) -> Result<(), CrpError> {
    let count = i32::try_from(values.len())
        .map_err(|_| CrpError::ArithmeticOverflow("metadata string-array count"))?;
    write_i32_le(output, count);
    for value in values {
        write_string(output, value)?;
    }
    Ok(())
}

fn read_mods(reader: &mut PayloadReader<'_>) -> Result<Vec<ModInfo>, CrpError> {
    let count = reader.read_i32_le()?;
    if count < 0 {
        return Err(reader.invalid(format!("negative mod count {count}")));
    }
    let count = usize::try_from(count).map_err(|_| reader.invalid("mod count is too large"))?;
    let minimum_size = count
        .checked_mul(9)
        .ok_or_else(|| reader.invalid("mod array size overflow"))?;
    if minimum_size > reader.remaining() {
        return Err(reader.invalid("mod array count exceeds remaining payload"));
    }
    let mut mods = Vec::with_capacity(count);
    for _ in 0..count {
        mods.push(ModInfo {
            name: reader.read_string()?,
            published_file_id: reader.read_i64_le()?,
        });
    }
    Ok(mods)
}

fn write_mods(output: &mut Vec<u8>, mods: &[ModInfo]) -> Result<(), CrpError> {
    let count = i32::try_from(mods.len())
        .map_err(|_| CrpError::ArithmeticOverflow("metadata mod count"))?;
    write_i32_le(output, count);
    for entry in mods {
        write_string(output, &entry.name)?;
        write_i64_le(output, entry.published_file_id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{read_string_array, write_string_array};
    use crate::CrpError;
    use crate::asset_io::PayloadReader;

    #[test]
    fn string_array_writer_emits_count_and_binarywriter_strings() -> Result<(), CrpError> {
        let values = vec!["oak".to_owned(), "pine".to_owned()];
        let mut encoded = Vec::new();
        write_string_array(&mut encoded, &values)?;

        assert_eq!(
            encoded,
            [2, 0, 0, 0, 3, b'o', b'a', b'k', 4, b'p', b'i', b'n', b'e']
        );
        Ok(())
    }

    #[test]
    fn string_array_reader_rejects_negative_counts() {
        let bytes = (-1_i32).to_le_bytes();
        let mut reader = PayloadReader::new("string-array", &bytes);
        assert!(matches!(
            read_string_array(&mut reader),
            Err(CrpError::InvalidAssetPayload { .. })
        ));
    }
}
