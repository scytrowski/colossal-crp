//! Read the outer container structure of City: Skylines (2015) `.crp` files.
//!
//! This reader supports the common package index observed in format versions
//! 4 through 10. It verifies each asset's stored MD5 and returns raw payloads.

use std::fmt;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

mod asset_io;
mod audio_manager_data;
mod building_manager_data;
mod data_serializer;
mod disaster_manager_data;
mod district_manager_data;
mod encoded_array;
mod event_manager_data;
mod game_area_data;
mod guide_data;
mod guide_manager_data;
mod info_manager_data;
mod instance_manager_data;
mod message_manager_data;
mod metadata;
mod net_manager_data;
mod path_manager_data;
mod prop_manager_data;
mod simulation_manager_data;
mod simulation_metadata;
mod statistics_manager_data;
mod terrain_manager_data;
mod texture;
mod tree_manager_data;
mod unlock_manager_data;
mod vehicle_manager_data;
mod water_simulation_data;
mod weather_manager_data;
mod zone_manager_data;

pub use audio_manager_data::{
    AudioManagerData, AudioManagerEditor, AudioRadioChannel, AudioRadioContent,
};
pub use building_manager_data::{
    BuildingBufferFieldsPrefix, BuildingByteStatePrefix, BuildingEconomicFieldsEditor,
    BuildingEconomicFieldsPrefix, BuildingEffectValue, BuildingFacultyBonusesEditor,
    BuildingFacultyBonusesPrefix, BuildingFlag, BuildingFlagsPrefix, BuildingInstanceRecord,
    BuildingInstanceRecordsEditor, BuildingInstanceRecordsPrefix,
    BuildingManagerObjectHeadersPrefix, BuildingMonumentEffectsEditor,
    BuildingMonumentEffectsPrefix, BuildingPopulationEditor, BuildingPopulationPrefix,
    BuildingPrefabPrefix, BuildingProblemEditor, BuildingProblemPrefix, BuildingProblemWords,
    BuildingRaceTeamReferencesEditor, BuildingRaceTeamReferencesPrefix,
    BuildingRaceTeamUnassignedEditor, BuildingRaceTeamUnassignedPrefix, BuildingServiceSelection,
    BuildingServiceSelectionPrefix, BuildingTimerField, BuildingTimerGeometryEditor,
    BuildingTimerGeometryPrefix,
};
pub use data_serializer::DataSerializerStream;
pub use district_manager_data::{
    DistrictAgeCategory, DistrictAgePopulation, DistrictAirportData, DistrictAirportEditor,
    DistrictCampusData, DistrictCampusSportsData, DistrictCampusSportsEditor,
    DistrictCampusYearReportData, DistrictCampusYearReportEditor, DistrictDemographicBlock,
    DistrictDemographicKind, DistrictHotelBlock, DistrictManagerFlagsPrefix,
    DistrictManagerGridData, DistrictManagerGridEditor, DistrictManagerTailData,
    DistrictParkAttractiveness, DistrictParkCapacities, DistrictParkEditor, DistrictParkGateData,
    DistrictParkIndustryResource, DistrictParkIndustryResourceKind, DistrictParkIndustryTradeData,
    DistrictParkProductionData, DistrictParkRecord, DistrictParkVisitorData,
    DistrictParkWorkStorageDeltas, DistrictPopulationRecord, DistrictPrivateDataBlock,
    DistrictPrivateDataKind, DistrictSpecializationBlock, DistrictSpecializationKind,
    DistrictStatisticsBlock, DistrictStatisticsKind, DistrictSubCultureBlock,
    DistrictSubCultureKind, DistrictTourismBlock, DistrictTourismKind,
};
pub use game_area_data::{GameAreaManagerData, SavedCameraView};
pub use guide_data::{
    GuideInstanceBody, GuideInstanceEditor, GuideTriggerBaseEditor, GuideTriggerBasePrefix,
};
pub use guide_manager_data::{
    GuideManagerReference, GuideManagerReferenceField, GuideManagerReferences,
};
pub use info_manager_data::InfoManagerData;
pub use instance_manager_data::{InstanceManagerData, InstanceManagerEditor, InstanceName};
pub use message_manager_data::{MessageDataEntry, MessageManagerData, MessageManagerDataEditor};
pub use metadata::{DistrictStyleMetaData, MapMetaData, ModInfo, SystemMapMetaData};
pub use net_manager_data::{
    NetFencePrefabPrefix, NetGuideCountsPrefix, NetInfoPrefabPrefix, NetLane, NetLanePrefix,
    NetManagerTailPrefix, NetNodeCounter, NetNodeCountersPrefix, NetNodeFlag, NetNodeFlagsPrefix,
    NetNodeInstanceGuideBody, NetNodeInstanceGuideEditor, NetNodeProblem, NetNodeProblemsPrefix,
    NetNodeRecord, NetNodeRecordsPrefix, NetObjectHeader, NetSegmentCoverage,
    NetSegmentCoveragePrefix, NetSegmentEnvironment, NetSegmentEnvironmentPrefix, NetSegmentFlag,
    NetSegmentFlagsPrefix, NetSegmentProblem, NetSegmentProblemsPrefix, NetSegmentRecord,
    NetSegmentRecordsPrefix, NetSegmentTrafficLight, NetSegmentTrafficLightsPrefix,
    NetTreePrefabPrefix,
};
pub use path_manager_data::{PathManagerData, PathPosition, PathUnit};
pub use prop_manager_data::PropManagerDataPrefix;
pub use simulation_manager_data::{
    EconomyInvestmentData, EconomyInvestmentPrefabAssignment, EconomyInvestmentPrefabTable,
    EconomyInvestmentValueSeries, EconomyLoanRecord, EconomyManagerBudgetEditor,
    EconomyManagerIncomeExpenseEditor, EconomyManagerInvestmentsEditor,
    EconomyManagerLoansAndCashEditor, GuideBodyLocation, ManagerObjectReference,
    ResolvedObjectReference, SerializableDataEntry, SimulationManagerData,
    TransportLineVehicleSelection, TransportManagerLineRecord, TransportManagerSelection,
    TransportManagerSelectionPrefix, TransportManagerSharedStringValue, TransportPassengerCounts,
    TransportPassengerRecord,
};
pub use simulation_metadata::{ModOverride, SimulationMetaData};
pub use statistics_manager_data::{
    StatisticArrayReferences, StatisticInt32Data, StatisticInt32Editor, StatisticInt64Data,
    StatisticInt64Editor, StatisticsManagerData, StatisticsManagerEditor,
};
pub use terrain_manager_data::TerrainManagerData;
pub use texture::TextureAsset;
pub use tree_manager_data::{BurningTreeRecord, TreeInstanceRecord, TreeManagerDataPrefix};
pub use unlock_manager_data::{
    UnlockManagerMilestoneArray, UnlockManagerMilestoneKind, UnlockManagerReferences,
};
pub use vehicle_manager_data::{
    VehicleInstanceFields, VehicleInstanceFieldsPrefix, VehicleLastFrameDataPrefix,
    VehicleLastFrameRecord, VehicleManagerData, VehicleManagerDataPrefix,
    VehicleManagerSparseFieldsPrefix, VehicleManagerTailDataPrefix, VehicleParkedInstanceRecord,
    VehicleParkedRecordsPrefix, VehicleTargetPosition0,
};
pub use water_simulation_data::{
    WaterSimulationData, WaterSimulationEditor, WaterSource, WaterWave,
};
pub use weather_manager_data::{WeatherLightningStrike, WeatherManagerData};
pub use zone_manager_data::{ZoneBlockRecord, ZoneManagerDataPrefix};

const MAGIC: &[u8; 4] = b"CRAP";
const ASSET_TYPE_PACKAGE: i32 = 53;
const MIN_SUPPORTED_VERSION: u16 = 4;
const MAX_SUPPORTED_VERSION: u16 = 10;
const MAX_STRING_BYTES: u32 = 16 * 1024 * 1024;
const MIN_INDEX_RECORD_SIZE: u64 = 22;

/// A `.crp` package whose entries contain their original payload bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// The outer `.crp` package format version.
    pub format_version: u16,
    /// Package name from the index header.
    pub name: String,
    /// The author field exactly as stored in the package.
    ///
    /// The game encrypts non-empty author values. This reader preserves the
    /// stored string and does not decrypt it.
    pub author_field: String,
    /// Package version value from the index header.
    pub package_version: u32,
    /// Name of the package's main asset.
    pub main_asset_name: String,
    /// Data-area offset from the package index header.
    pub data_start_offset: u64,
    /// Assets in their original index order.
    pub assets: Vec<Asset>,
}

/// An indexed asset and its unmodified payload bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// Asset name from the index.
    pub name: String,
    /// Checksum text exactly as stored in the index.
    pub checksum: String,
    /// Numeric asset type. Unknown values are retained.
    pub asset_type: i32,
    /// Relative offset from the package data area.
    pub relative_offset: u64,
    /// Payload bytes read from the package.
    pub data: Vec<u8>,
}

impl Asset {
    /// Reads this type-53 asset payload as a nested package.
    ///
    /// The nested reader is bounded to this asset's payload bytes, so child
    /// offsets cannot read into following assets in the parent package. The
    /// nested package must use an outer format version supported by
    /// [`Package::read_from`] (currently versions 4 through 10). No
    /// game-produced nested package has been validated against this method.
    pub fn read_nested_package(&self) -> Result<Package, CrpError> {
        if self.asset_type != ASSET_TYPE_PACKAGE {
            return Err(CrpError::AssetTypeMismatch {
                name: self.name.clone(),
                expected: ASSET_TYPE_PACKAGE,
                actual: self.asset_type,
            });
        }

        Package::read_from(&mut Cursor::new(self.data.as_slice()))
    }
}

/// Errors returned while reading a `.crp` package's outer container.
#[derive(Debug)]
pub enum CrpError {
    /// An underlying input operation failed.
    Io(io::Error),
    /// The file does not begin with the `CRAP` magic bytes.
    InvalidMagic([u8; 4]),
    /// The package format version is outside the versions currently supported.
    UnsupportedVersion(u16),
    /// A BinaryWriter string had an invalid or excessive encoded length.
    InvalidStringLength,
    /// A package string was not valid UTF-8.
    InvalidUtf8,
    /// The asset count was negative.
    NegativeAssetCount(i32),
    /// An asset record has a negative relative offset or size.
    InvalidAssetRange {
        name: String,
        relative_offset: i64,
        size: i64,
    },
    /// The package index exceeds the declared data start.
    IndexPastDataStart { index_end: u64, data_start: u64 },
    /// An asset's data range extends past the end of the input.
    AssetOutOfBounds {
        name: String,
        offset: u64,
        size: u64,
        file_size: u64,
    },
    /// The payload's MD5 does not match the checksum stored in the index.
    ChecksumMismatch {
        name: String,
        expected: String,
        actual: String,
    },
    /// An asset-specific decoder was called for a different asset type.
    AssetTypeMismatch {
        name: String,
        expected: i32,
        actual: i32,
    },
    /// An asset payload is malformed or uses an unsupported field schema.
    InvalidAssetPayload {
        name: String,
        offset: usize,
        reason: String,
    },
    /// An offset or size calculation overflowed.
    ArithmeticOverflow(&'static str),
    /// An index count cannot fit in the remaining declared index area.
    InvalidAssetCount,
}

impl fmt::Display for CrpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::InvalidMagic(magic) => write!(f, "invalid package magic: {magic:?}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported package format version {version}")
            }
            Self::InvalidStringLength => write!(f, "invalid or excessive package string length"),
            Self::InvalidUtf8 => write!(f, "package string is not valid UTF-8"),
            Self::NegativeAssetCount(count) => write!(f, "negative asset count {count}"),
            Self::InvalidAssetRange {
                name,
                relative_offset,
                size,
            } => write!(
                f,
                "asset {name:?} has invalid relative offset {relative_offset} or size {size}"
            ),
            Self::IndexPastDataStart {
                index_end,
                data_start,
            } => write!(
                f,
                "package index ends at {index_end}, after declared data start {data_start}"
            ),
            Self::AssetOutOfBounds {
                name,
                offset,
                size,
                file_size,
            } => write!(
                f,
                "asset {name:?} range starts at {offset} with size {size}, beyond file size {file_size}"
            ),
            Self::ChecksumMismatch {
                name,
                expected,
                actual,
            } => write!(
                f,
                "asset {name:?} checksum mismatch: index has {expected}, payload has {actual}"
            ),
            Self::AssetTypeMismatch {
                name,
                expected,
                actual,
            } => write!(
                f,
                "asset {name:?} has type {actual}; decoder requires type {expected}"
            ),
            Self::InvalidAssetPayload {
                name,
                offset,
                reason,
            } => write!(
                f,
                "asset {name:?} has an invalid payload at byte {offset}: {reason}"
            ),
            Self::ArithmeticOverflow(context) => {
                write!(f, "integer overflow while calculating {context}")
            }
            Self::InvalidAssetCount => write!(f, "asset count does not fit in the index area"),
        }
    }
}

impl std::error::Error for CrpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CrpError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl Package {
    /// Reads a package using the base package implementation's zero data offset.
    ///
    /// This covers the common map files currently examined and rejects payloads
    /// whose MD5 does not match the index. Other package implementations may add
    /// a separate implementation-specific data offset.
    pub fn read_from<R: Read + Seek>(reader: &mut R) -> Result<Self, CrpError> {
        Self::read_from_with_data_offset(reader, 0)
    }

    /// Reads a package with an implementation-specific data offset.
    ///
    /// Asset payload locations are calculated as the declared data start plus
    /// each asset's relative offset plus `implementation_data_offset`.
    pub fn read_from_with_data_offset<R: Read + Seek>(
        reader: &mut R,
        implementation_data_offset: u64,
    ) -> Result<Self, CrpError> {
        Self::read_with_data_offset_and_asset_type(reader, implementation_data_offset, None)
    }

    /// Reads and verifies only assets with the requested asset type.
    ///
    /// The package header and every index record are parsed, and every payload
    /// range is checked against the file size. Payload bytes and checksums are
    /// read and verified only for matching assets. Use [`Package::read_from`]
    /// when the entire package must be validated or rewritten.
    pub fn read_assets_of_type_from<R: Read + Seek>(
        reader: &mut R,
        asset_type: i32,
    ) -> Result<Vec<Asset>, CrpError> {
        Self::read_assets_of_type_from_with_data_offset(reader, asset_type, 0)
    }

    /// Reads and verifies matching assets using an implementation-specific
    /// payload data offset.
    ///
    /// All index records and payload ranges are validated, but payload bytes
    /// and checksums are read only for assets whose type matches `asset_type`.
    pub fn read_assets_of_type_from_with_data_offset<R: Read + Seek>(
        reader: &mut R,
        asset_type: i32,
        implementation_data_offset: u64,
    ) -> Result<Vec<Asset>, CrpError> {
        Ok(Self::read_with_data_offset_and_asset_type(
            reader,
            implementation_data_offset,
            Some(asset_type),
        )?
        .assets)
    }

    fn read_with_data_offset_and_asset_type<R: Read + Seek>(
        reader: &mut R,
        implementation_data_offset: u64,
        selected_asset_type: Option<i32>,
    ) -> Result<Self, CrpError> {
        let file_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;

        let mut magic = [0; 4];
        reader.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(CrpError::InvalidMagic(magic));
        }

        let format_version = read_u16_le(reader)?;
        if !(MIN_SUPPORTED_VERSION..=MAX_SUPPORTED_VERSION).contains(&format_version) {
            return Err(CrpError::UnsupportedVersion(format_version));
        }

        let name = read_binary_string(reader)?;
        let author_field = read_binary_string(reader)?;
        let package_version = read_u32_le(reader)?;
        let main_asset_name = read_binary_string(reader)?;
        let asset_count_signed = read_i32_le(reader)?;
        if asset_count_signed < 0 {
            return Err(CrpError::NegativeAssetCount(asset_count_signed));
        }
        let data_start_offset = read_u64_le(reader)?;

        let index_start = reader.stream_position()?;
        if index_start > data_start_offset {
            return Err(CrpError::IndexPastDataStart {
                index_end: index_start,
                data_start: data_start_offset,
            });
        }
        if data_start_offset > file_size {
            return Err(CrpError::IndexPastDataStart {
                index_end: index_start,
                data_start: data_start_offset,
            });
        }
        let index_bytes_available = data_start_offset - index_start;
        let asset_count =
            usize::try_from(asset_count_signed).map_err(|_| CrpError::InvalidAssetCount)?;
        let min_index_bytes = u64::try_from(asset_count)
            .ok()
            .and_then(|count| count.checked_mul(MIN_INDEX_RECORD_SIZE))
            .ok_or(CrpError::InvalidAssetCount)?;
        if min_index_bytes > index_bytes_available {
            return Err(CrpError::InvalidAssetCount);
        }

        let mut records = Vec::with_capacity(asset_count);
        for _ in 0..asset_count {
            let asset_name = read_binary_string_bounded(reader, data_start_offset)?;
            let checksum = read_binary_string_bounded(reader, data_start_offset)?;
            let fixed_fields_end = reader
                .stream_position()?
                .checked_add(MIN_INDEX_RECORD_SIZE - 2)
                .ok_or(CrpError::ArithmeticOverflow("index record end"))?;
            if fixed_fields_end > data_start_offset {
                return Err(CrpError::IndexPastDataStart {
                    index_end: fixed_fields_end,
                    data_start: data_start_offset,
                });
            }
            let asset_type = read_i32_le(reader)?;
            let relative_offset_signed = read_i64_le(reader)?;
            let size_signed = read_i64_le(reader)?;
            if relative_offset_signed < 0 || size_signed < 0 {
                return Err(CrpError::InvalidAssetRange {
                    name: asset_name,
                    relative_offset: relative_offset_signed,
                    size: size_signed,
                });
            }
            let relative_offset = u64::try_from(relative_offset_signed)
                .map_err(|_| CrpError::ArithmeticOverflow("asset relative offset"))?;
            let size = u64::try_from(size_signed)
                .map_err(|_| CrpError::ArithmeticOverflow("asset size"))?;
            records.push((asset_name, checksum, asset_type, relative_offset, size));
        }

        let index_end = reader.stream_position()?;
        if index_end > data_start_offset {
            return Err(CrpError::IndexPastDataStart {
                index_end,
                data_start: data_start_offset,
            });
        }

        let selected_asset_count = records
            .iter()
            .filter(|record| selected_asset_type.is_none_or(|asset_type| record.2 == asset_type))
            .count();
        let mut assets = Vec::with_capacity(selected_asset_count);
        for (asset_name, checksum, asset_type, relative_offset, size) in records {
            let payload_offset = data_start_offset
                .checked_add(implementation_data_offset)
                .and_then(|offset| offset.checked_add(relative_offset))
                .ok_or(CrpError::ArithmeticOverflow("asset payload offset"))?;
            let payload_end = payload_offset
                .checked_add(size)
                .ok_or(CrpError::ArithmeticOverflow("asset payload range"))?;
            if payload_end > file_size {
                return Err(CrpError::AssetOutOfBounds {
                    name: asset_name,
                    offset: payload_offset,
                    size,
                    file_size,
                });
            }

            if selected_asset_type.is_some_and(|selected_type| selected_type != asset_type) {
                continue;
            }

            let payload_size = usize::try_from(size)
                .map_err(|_| CrpError::ArithmeticOverflow("asset allocation size"))?;
            let mut data = vec![0; payload_size];
            reader.seek(SeekFrom::Start(payload_offset))?;
            reader.read_exact(&mut data)?;
            let actual_checksum = checksum_hex(&data);
            if checksum != actual_checksum {
                return Err(CrpError::ChecksumMismatch {
                    name: asset_name,
                    expected: checksum,
                    actual: actual_checksum,
                });
            }

            assets.push(Asset {
                name: asset_name,
                checksum,
                asset_type,
                relative_offset,
                data,
            });
        }

        Ok(Self {
            format_version,
            name,
            author_field,
            package_version,
            main_asset_name,
            data_start_offset,
            assets,
        })
    }

    /// Writes a compact package, recalculating offsets and payload checksums.
    ///
    /// Assets are emitted in index order with contiguous payload ranges. The
    /// package's format version and stored author field are preserved. This
    /// method does not encrypt or decrypt non-empty author fields; a field read
    /// from a package is written back as stored.
    pub fn write_compact_to<W: Write>(&self, writer: &mut W) -> Result<(), CrpError> {
        if !(MIN_SUPPORTED_VERSION..=MAX_SUPPORTED_VERSION).contains(&self.format_version) {
            return Err(CrpError::UnsupportedVersion(self.format_version));
        }
        let asset_count =
            i32::try_from(self.assets.len()).map_err(|_| CrpError::InvalidAssetCount)?;

        let mut header = Vec::new();
        header.write_all(MAGIC)?;
        write_u16_le(&mut header, self.format_version)?;
        write_binary_string(&mut header, &self.name)?;
        write_binary_string(&mut header, &self.author_field)?;
        write_u32_le(&mut header, self.package_version)?;
        write_binary_string(&mut header, &self.main_asset_name)?;
        write_i32_le(&mut header, asset_count)?;

        let mut index = Vec::new();
        let mut relative_offset = 0_u64;
        for asset in &self.assets {
            write_binary_string(&mut index, &asset.name)?;
            write_binary_string(&mut index, &checksum_hex(&asset.data))?;
            write_i32_le(&mut index, asset.asset_type)?;
            write_i64_le(
                &mut index,
                i64::try_from(relative_offset)
                    .map_err(|_| CrpError::ArithmeticOverflow("asset relative offset"))?,
            )?;
            write_i64_le(
                &mut index,
                i64::try_from(asset.data.len())
                    .map_err(|_| CrpError::ArithmeticOverflow("asset size"))?,
            )?;
            relative_offset = relative_offset
                .checked_add(
                    u64::try_from(asset.data.len())
                        .map_err(|_| CrpError::ArithmeticOverflow("asset size"))?,
                )
                .ok_or(CrpError::ArithmeticOverflow("asset relative offset"))?;
        }

        let data_start_offset = u64::try_from(header.len())
            .ok()
            .and_then(|header_size| header_size.checked_add(8))
            .and_then(|header_size| header_size.checked_add(u64::try_from(index.len()).ok()?))
            .ok_or(CrpError::ArithmeticOverflow("package data start offset"))?;
        write_u64_le(&mut header, data_start_offset)?;

        writer.write_all(&header)?;
        writer.write_all(&index)?;
        for asset in &self.assets {
            writer.write_all(&asset.data)?;
        }
        Ok(())
    }
}

fn read_binary_string<R: Read>(reader: &mut R) -> Result<String, CrpError> {
    let byte_len = read_7bit_encoded_u32(reader)?;
    read_string_bytes(reader, byte_len)
}

fn read_string_bytes<R: Read>(reader: &mut R, byte_len: u32) -> Result<String, CrpError> {
    if byte_len > MAX_STRING_BYTES {
        return Err(CrpError::InvalidStringLength);
    }
    let mut bytes = vec![0; byte_len as usize];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|_| CrpError::InvalidUtf8)
}

fn read_binary_string_bounded<R: Read + Seek>(
    reader: &mut R,
    bound: u64,
) -> Result<String, CrpError> {
    let byte_len = read_7bit_encoded_u32(reader)?;
    let payload_start = reader.stream_position()?;
    let payload_end = payload_start
        .checked_add(u64::from(byte_len))
        .ok_or(CrpError::ArithmeticOverflow("index string end"))?;
    if payload_end > bound {
        return Err(CrpError::IndexPastDataStart {
            index_end: payload_end,
            data_start: bound,
        });
    }
    read_string_bytes(reader, byte_len)
}

fn read_7bit_encoded_u32<R: Read>(reader: &mut R) -> Result<u32, CrpError> {
    let mut value = 0_u32;
    for shift in (0..35).step_by(7) {
        let mut byte = [0];
        reader.read_exact(&mut byte)?;
        if shift == 28 && byte[0] > 0x0F {
            return Err(CrpError::InvalidStringLength);
        }
        value |= u32::from(byte[0] & 0x7F) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(CrpError::InvalidStringLength)
}

fn read_u16_le<R: Read>(reader: &mut R) -> Result<u16, CrpError> {
    let mut bytes = [0; 2];
    reader.read_exact(&mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32_le<R: Read>(reader: &mut R) -> Result<u32, CrpError> {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_i32_le<R: Read>(reader: &mut R) -> Result<i32, CrpError> {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(i32::from_le_bytes(bytes))
}

fn read_i64_le<R: Read>(reader: &mut R) -> Result<i64, CrpError> {
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes)?;
    Ok(i64::from_le_bytes(bytes))
}

fn read_u64_le<R: Read>(reader: &mut R) -> Result<u64, CrpError> {
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn write_binary_string<W: Write>(writer: &mut W, value: &str) -> Result<(), CrpError> {
    let bytes = value.as_bytes();
    let byte_len = u32::try_from(bytes.len())
        .map_err(|_| CrpError::ArithmeticOverflow("string byte length"))?;
    write_7bit_encoded_u32(writer, byte_len)?;
    writer.write_all(bytes)?;
    Ok(())
}

fn write_7bit_encoded_u32<W: Write>(writer: &mut W, mut value: u32) -> Result<(), CrpError> {
    while value >= 0x80 {
        let low_bits = u8::try_from(value & 0x7f)
            .map_err(|_| CrpError::ArithmeticOverflow("string length encoding"))?;
        writer.write_all(&[low_bits | 0x80])?;
        value >>= 7;
    }
    let final_byte =
        u8::try_from(value).map_err(|_| CrpError::ArithmeticOverflow("string length encoding"))?;
    writer.write_all(&[final_byte])?;
    Ok(())
}

fn write_u16_le<W: Write>(writer: &mut W, value: u16) -> Result<(), CrpError> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_u32_le<W: Write>(writer: &mut W, value: u32) -> Result<(), CrpError> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_i32_le<W: Write>(writer: &mut W, value: i32) -> Result<(), CrpError> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_i64_le<W: Write>(writer: &mut W, value: i64) -> Result<(), CrpError> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_u64_le<W: Write>(writer: &mut W, value: u64) -> Result<(), CrpError> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn checksum_hex(data: &[u8]) -> String {
    let digest = md5_digest(data);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(32);
    for byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0F)]));
    }
    output
}

fn md5_digest(data: &[u8]) -> [u8; 16] {
    const SHIFTS: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const CONSTANTS: [u32; 64] = [
        0xd76a_a478,
        0xe8c7_b756,
        0x2420_70db,
        0xc1bd_ceee,
        0xf57c_0faf,
        0x4787_c62a,
        0xa830_4613,
        0xfd46_9501,
        0x6980_98d8,
        0x8b44_f7af,
        0xffff_5bb1,
        0x895c_d7be,
        0x6b90_1122,
        0xfd98_7193,
        0xa679_438e,
        0x49b4_0821,
        0xf61e_2562,
        0xc040_b340,
        0x265e_5a51,
        0xe9b6_c7aa,
        0xd62f_105d,
        0x0244_1453,
        0xd8a1_e681,
        0xe7d3_fbc8,
        0x21e1_cde6,
        0xc337_07d6,
        0xf4d5_0d87,
        0x455a_14ed,
        0xa9e3_e905,
        0xfcef_a3f8,
        0x676f_02d9,
        0x8d2a_4c8a,
        0xfffa_3942,
        0x8771_f681,
        0x6d9d_6122,
        0xfde5_380c,
        0xa4be_ea44,
        0x4bde_cfa9,
        0xf6bb_4b60,
        0xbebf_bc70,
        0x289b_7ec6,
        0xeaa1_27fa,
        0xd4ef_3085,
        0x0488_1d05,
        0xd9d4_d039,
        0xe6db_99e5,
        0x1fa2_7cf8,
        0xc4ac_5665,
        0xf429_2244,
        0x432a_ff97,
        0xab94_23a7,
        0xfc93_a039,
        0x655b_59c3,
        0x8f0c_cc92,
        0xffef_f47d,
        0x8584_5dd1,
        0x6fa8_7e4f,
        0xfe2c_e6e0,
        0xa301_4314,
        0x4e08_11a1,
        0xf753_7e82,
        0xbd3a_f235,
        0x2ad7_d2bb,
        0xeb86_d391,
    ];

    let mut state = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let chunks = data.chunks_exact(64);
    for chunk in chunks.clone() {
        md5_compress(&mut state, chunk, &SHIFTS, &CONSTANTS);
    }

    let remainder = chunks.remainder();
    let mut final_blocks = [0_u8; 128];
    final_blocks[..remainder.len()].copy_from_slice(remainder);
    final_blocks[remainder.len()] = 0x80;
    // Rust targets supported by this crate have usize no wider than u64; MD5
    // defines its bit length modulo 2^64.
    let bit_length = (data.len() as u64).wrapping_mul(8).to_le_bytes();

    if remainder.len() >= 56 {
        final_blocks[120..128].copy_from_slice(&bit_length);
        md5_compress(&mut state, &final_blocks[..64], &SHIFTS, &CONSTANTS);
        md5_compress(&mut state, &final_blocks[64..], &SHIFTS, &CONSTANTS);
    } else {
        final_blocks[56..64].copy_from_slice(&bit_length);
        md5_compress(&mut state, &final_blocks[..64], &SHIFTS, &CONSTANTS);
    }

    let mut digest = [0_u8; 16];
    for (index, word) in state.into_iter().enumerate() {
        let start = index * 4;
        digest[start..start + 4].copy_from_slice(&word.to_le_bytes());
    }
    digest
}

fn md5_compress(state: &mut [u32; 4], block: &[u8], shifts: &[u32; 64], constants: &[u32; 64]) {
    let mut words = [0_u32; 16];
    for (index, chunk) in block.chunks_exact(4).enumerate() {
        words[index] = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }

    let [mut a, mut b, mut c, mut d] = *state;
    for index in 0..64 {
        let (function, word_index) = match index {
            0..=15 => ((b & c) | (!b & d), index),
            16..=31 => ((d & b) | (!d & c), (5 * index + 1) % 16),
            32..=47 => (b ^ c ^ d, (3 * index + 5) % 16),
            _ => (c ^ (b | !d), (7 * index) % 16),
        };
        let sum = a
            .wrapping_add(function)
            .wrapping_add(constants[index])
            .wrapping_add(words[word_index]);
        let next_b = b.wrapping_add(sum.rotate_left(shifts[index]));
        a = d;
        d = c;
        c = b;
        b = next_b;
    }

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Cursor;
    use std::path::PathBuf;

    struct RejectReadAtOffset {
        cursor: Cursor<Vec<u8>>,
        rejected_offset: u64,
    }

    impl Read for RejectReadAtOffset {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.cursor.position() == self.rejected_offset {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "test rejected a read from an unmatched payload",
                ));
            }
            self.cursor.read(buffer)
        }
    }

    impl Seek for RejectReadAtOffset {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            self.cursor.seek(position)
        }
    }

    fn sample_package() -> Package {
        Package {
            format_version: 6,
            name: "Test Map".to_owned(),
            author_field: String::new(),
            package_version: 3,
            main_asset_name: "Test Map".to_owned(),
            data_start_offset: 0,
            assets: vec![
                Asset {
                    name: "preview".to_owned(),
                    checksum: "stale checksum is replaced".to_owned(),
                    asset_type: 3,
                    relative_offset: 900,
                    data: b"first payload".to_vec(),
                },
                Asset {
                    name: "future asset".to_owned(),
                    checksum: String::new(),
                    asset_type: 712,
                    relative_offset: 0,
                    data: vec![0, 1, 2, 255],
                },
            ],
        }
    }

    #[test]
    fn md5_matches_standard_vectors() {
        assert_eq!(checksum_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(checksum_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            checksum_hex(b"message digest"),
            "f96b697d7cb7938d525a2f31aaf161d0"
        );
    }

    #[test]
    fn data_serializer_stream_reads_root_header_and_preserves_opaque_bytes() -> Result<(), CrpError>
    {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/simulation-metadata-v6.bin");
        let bytes = fs::read(fixture_path)?;
        let stream = DataSerializerStream::decode(&bytes)?;
        assert_eq!(stream.format_version(), 0x0001_b584);
        assert_eq!(
            stream.root_type(),
            "SimulationMetaData, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null"
        );
        assert_eq!(stream.encode(), bytes);
        Ok(())
    }

    #[test]
    fn data_serializer_decodes_compact_and_wide_unique_strings() -> Result<(), CrpError> {
        let compact = [0, 0, 0, 1, 0xfe, 0xfe, 0x80, 0, 2, 0xc1, 1];
        let decoded_compact = DataSerializerStream::decode(&compact)?;
        assert_eq!(decoded_compact.root_type, "AB");
        assert_eq!(
            DataSerializerStream::from_parts(
                decoded_compact.format_version(),
                decoded_compact.root_type().to_owned(),
                decoded_compact.root_data().to_vec(),
            )?
            .encode(),
            compact
        );

        let wide = [0, 0, 0, 1, 0xfe, 0xfe, 0, 0, 1, 0, 0x80];
        assert_eq!(DataSerializerStream::decode(&wide)?.root_type, "Ā");
        let supplementary = DataSerializerStream::from_parts(1, "🚀".to_owned(), vec![])?;
        assert_eq!(
            DataSerializerStream::decode(&supplementary.encode())?.root_type,
            "🚀"
        );
        Ok(())
    }

    #[test]
    fn data_serializer_rejects_invalid_root_headers_and_truncation() {
        for bytes in [
            &b""[..],
            &[0, 0, 0, 1, 0xff],
            &[0, 0, 0, 1, 0xfe, 0],
            &[0, 0, 0, 1, 0xfe, 0xfe, 0xff],
            &[0, 0, 0, 1, 0xfe, 0xfe, 0x80, 0, 1],
        ] {
            assert!(DataSerializerStream::decode(bytes).is_err());
        }
    }

    #[test]
    fn simulation_metadata_decodes_and_reencodes_game_stream_exactly() -> Result<(), CrpError> {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/simulation-metadata-v6.bin");
        let bytes = fs::read(fixture_path)?;
        let (metadata, consumed) = SimulationMetaData::decode_from(&bytes)?;
        assert_eq!(consumed, bytes.len());
        assert_eq!(
            metadata.game_instance_identifier.as_deref(),
            Some("7a8bf7ba-d1dc-45ed-b96b-93e75f9c7130")
        );
        assert_eq!(metadata.environment.as_deref(), Some("North"));
        assert_eq!(metadata.scenario_asset, None);
        assert_eq!(metadata.scenario_name, None);
        assert_eq!(metadata.encode()?, bytes);
        Ok(())
    }

    #[test]
    fn simulation_metadata_reports_first_stream_boundary_in_combined_data() -> Result<(), CrpError>
    {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/simulation-metadata-v6.bin");
        let metadata_stream = fs::read(fixture_path)?;
        let mut combined = metadata_stream.clone();
        combined.extend_from_slice(&[0, 1, 0xb5, 0x84, 0xfe, 0xfe]);
        let (metadata, consumed) = SimulationMetaData::decode_from(&combined)?;
        assert_eq!(consumed, metadata_stream.len());
        assert_eq!(&combined[consumed..], &[0, 1, 0xb5, 0x84, 0xfe, 0xfe]);
        assert_eq!(metadata.encode()?, metadata_stream);
        Ok(())
    }

    #[test]
    fn simulation_metadata_respects_legacy_version_gates() -> Result<(), CrpError> {
        let version = 194;
        let fields = [
            0xff, 0xff, 0xa5, 0x5a, // nullable strings and raw boolean bytes
            1, 2, 3, 4, 5, 6, 7, 8, // current DateTime ticks
            0x11, 0x22, 0x33, 0x44, // save application version
            0x55, 0x66, 0x77, 0x88, // data format version
        ];
        let stream = DataSerializerStream::from_parts(
            version,
            "SimulationMetaData, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null".to_owned(),
            fields.to_vec(),
        )?;
        let encoded = stream.encode();
        let (metadata, consumed) = SimulationMetaData::decode_from(&encoded)?;
        assert_eq!(consumed, encoded.len());
        assert_eq!(metadata.game_instance_identifier, None);
        assert_eq!(metadata.environment, None);
        assert_eq!(metadata.scenario_asset, None);
        assert_eq!(metadata.scenario_name, None);
        assert_eq!(metadata.starting_datetime_ticks, None);
        assert_eq!(metadata.invert_traffic, 0xa5);
        assert_eq!(metadata.disable_achievements, Some(0x5a));
        assert_eq!(metadata.current_datetime_ticks, Some(0x0102_0304_0506_0708));
        assert_eq!(metadata.current_day_hour_bits, None);
        assert_eq!(metadata.new_map_app_version, None);
        assert_eq!(metadata.save_app_version, Some(0x1122_3344));
        assert_eq!(metadata.data_format_version, 0x5566_7788);
        assert_eq!(metadata.encode()?, encoded);
        Ok(())
    }

    #[test]
    fn simulation_metadata_handles_midrange_and_109000_gates() -> Result<(), CrpError> {
        let fields = [
            0xff, 0xff, 0xff, 0xff, // instance, environment, scenario asset, scenario name
            0, 1, // scenario outcomes
            0, 0, 0, 0, // empty mod override map
            2, 3, // traffic and achievements flags
            1, 2, 3, 4, 5, 6, 7, 8, // current DateTime ticks
            0x41, 0x40, 0, 0, // day-hour 12.0
            0, 0, 0, 9, // new-map app version
            0, 0, 0, 10, // new-game app version
            0, 0, 0, 11, // save app version
            0, 0, 0, 12, // data format version
        ];
        for version in [300, 109_000] {
            let stream = DataSerializerStream::from_parts(
                version,
                "SimulationMetaData, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null".to_owned(),
                fields.to_vec(),
            )?;
            let encoded = stream.encode();
            let (metadata, consumed) = SimulationMetaData::decode_from(&encoded)?;
            assert_eq!(consumed, encoded.len());
            assert_eq!(metadata.scenario_asset, None);
            assert_eq!(metadata.scenario_name, None);
            assert_eq!(metadata.scenario_won, 0);
            assert_eq!(metadata.scenario_failed, 1);
            assert_eq!(metadata.starting_datetime_ticks, None);
            assert_eq!(metadata.mod_override.len(), 0);
            assert_eq!(metadata.current_day_hour_bits, Some(0x4140_0000));
            assert_eq!(metadata.new_map_app_version, Some(9));
            assert_eq!(metadata.save_app_version, Some(11));
            assert_eq!(metadata.data_format_version, 12);
            assert_eq!(metadata.encode()?, encoded);
        }
        Ok(())
    }

    #[test]
    fn simulation_metadata_rejects_wrong_root_and_truncated_fields() -> Result<(), CrpError> {
        let root = DataSerializerStream::from_parts(0x1_b584, "WrongRoot".to_owned(), vec![])?;
        assert!(SimulationMetaData::decode_from(&root.encode()).is_err());

        let truncated = DataSerializerStream::from_parts(
            0x1_b584,
            "SimulationMetaData, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null".to_owned(),
            vec![0xff],
        )?;
        assert!(SimulationMetaData::decode_from(&truncated.encode()).is_err());

        let mut malformed_fields = vec![0xff, 0xff, 0xff, 0xff];
        malformed_fields.extend_from_slice(&[0, 0]);
        malformed_fields.extend_from_slice(&[0; 8]);
        malformed_fields.push(0xff);
        malformed_fields.extend_from_slice(&(-1_i32).to_be_bytes());
        let malformed = DataSerializerStream::from_parts(
            0x1_b584,
            "SimulationMetaData, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null".to_owned(),
            malformed_fields,
        )?;
        assert!(SimulationMetaData::decode_from(&malformed.encode()).is_err());
        Ok(())
    }

    #[test]
    fn simulation_manager_root_fixture_decodes_and_reencodes_exactly() -> Result<(), CrpError> {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/simulation-manager-data-v6-root.bin");
        let bytes = fs::read(fixture_path)?;
        let manager = SimulationManagerData::decode(&bytes)?;
        assert_eq!(manager.format_version(), 0x0001_b584);
        assert_eq!(manager.current_tick_index, 0x0038_a2c2);
        assert_eq!(manager.current_frame_index, 0x0199_d77b);
        assert_eq!(manager.current_build_index, 0x0000_23ef);
        assert_eq!(manager.randomizer_seed, 0x81c3_f923_d483_d023);
        assert_eq!(manager.day_time_offset_frames, Some(0x0000_a885));
        assert_eq!(manager.build_index_history.len(), 32);
        assert_eq!(manager.manager_objects().map_or(0, <[_]>::len), 29);
        assert_eq!(
            manager.serializable_data_storage.as_ref().map(Vec::len),
            Some(0)
        );
        assert!(manager.serialized_object_graph().is_empty());
        assert_eq!(manager.root_fields_len(), bytes.len());
        assert_eq!(manager.encode()?, bytes);
        Ok(())
    }

    #[test]
    fn simulation_manager_rejects_missing_or_wrong_root_profile_end() {
        let mut bytes =
            include_bytes!("../tests/fixtures/simulation-manager-data-v6-root.bin").to_vec();
        bytes.pop();
        assert!(SimulationManagerData::decode(&bytes).is_err());

        let mut bytes =
            include_bytes!("../tests/fixtures/simulation-manager-data-v6-root.bin").to_vec();
        if let Some(last) = bytes.last_mut() {
            *last = 1;
        }
        assert!(SimulationManagerData::decode(&bytes).is_err());
    }

    #[test]
    fn replacing_water_preserves_neighboring_manager_bodies() -> Result<(), CrpError> {
        fn begin_tag(bytes: &mut Vec<u8>, name: &str) -> Result<(), CrpError> {
            bytes.push(0xfe);
            crate::data_serializer::write_unique_string(bytes, name)
        }

        let fixture = include_bytes!("../tests/fixtures/simulation-manager-data-v6-root.bin");
        let root = DataSerializerStream::decode(fixture)?;
        let mut graph = Vec::new();
        begin_tag(&mut graph, "GameAreaManager")?;
        graph.extend_from_slice(include_bytes!(
            "../tests/fixtures/game-area-manager-body-v112004.bin"
        ));
        graph.extend_from_slice(&[0, 1]);
        begin_tag(&mut graph, "TerrainManager")?;
        // A full zero-height grid followed by an empty sparse block-delta grid.
        graph.extend(std::iter::repeat_n(0xff, 9_129));
        graph.push(0xb0);
        graph.extend_from_slice(&524_288_i32.to_be_bytes());
        graph.extend_from_slice(&[0xff; 3]);
        graph.extend_from_slice(&[0, 2]);
        begin_tag(&mut graph, "WaterSimulation")?;
        graph.extend_from_slice(include_bytes!(
            "../tests/fixtures/water-simulation-body-v116020-desert.bin"
        ));
        graph.extend_from_slice(&[0, 3]);
        begin_tag(&mut graph, "InstanceManager")?;
        graph.extend_from_slice(include_bytes!(
            "../tests/fixtures/instance-manager-body-v116020-desert.bin"
        ));
        graph.extend_from_slice(&[0, 4]);
        begin_tag(&mut graph, "PathManager")?;
        graph.extend_from_slice(include_bytes!(
            "../tests/fixtures/path-manager-body-v116020-desert.bin"
        ));
        graph.extend_from_slice(&[0, 5]);
        begin_tag(&mut graph, "NetManager")?;
        graph.extend(std::iter::repeat_n(0xff, 2047));
        graph.push(0xfe);
        graph.extend(std::iter::repeat_n(0xff, 511));
        graph.push(0xfd);
        graph.extend_from_slice(b"opaque next manager");

        let mut root_data = root.root_data().to_vec();
        root_data.extend_from_slice(&graph);
        let stream = DataSerializerStream::from_parts(
            root.format_version(),
            root.root_type().to_owned(),
            root_data,
        )?;
        let mut manager = SimulationManagerData::decode(&stream.encode())?;
        let (_, terrain_end) = manager.decode_terrain_prefix()?;
        let (water, old_water_end) = manager.decode_water_prefix()?;
        let old_graph = manager.serialized_object_graph().to_vec();
        let mut editor = water.into_editor();
        editor.set_sea_level(41.5);
        editor.sources_mut()[0].parameters[4] = 42;
        let new_wave = WaterWave {
            wave_type: 2,
            min_x: 1,
            min_z: 2,
            max_x: 3,
            max_z: 4,
            origin_x: 5,
            origin_z: 6,
            direction_x: -1,
            direction_z: -2,
            delta: -3,
            duration: 7,
            current_time: 8,
        };
        editor.waves_mut().push(new_wave.clone());
        manager.replace_water(&editor)?;
        let (_, new_water_end) = manager.decode_water_prefix()?;
        assert_eq!(
            &manager.serialized_object_graph()[..terrain_end],
            &old_graph[..terrain_end]
        );
        assert_eq!(
            &manager.serialized_object_graph()[new_water_end..],
            &old_graph[old_water_end..]
        );
        let stable_graph = manager.serialized_object_graph().to_vec();
        editor.sources_mut()[0].parameters[0] = 0x0100_0000;
        assert!(manager.replace_water(&editor).is_err());
        assert_eq!(manager.serialized_object_graph(), stable_graph);
        let (other_version, _) = WaterSimulationData::decode_prefix(
            116_020,
            include_bytes!("../tests/fixtures/water-simulation-body-v116020-desert.bin"),
        )?;
        assert!(manager.replace_water(&other_version.into_editor()).is_err());
        assert_eq!(manager.serialized_object_graph(), stable_graph);
        let rewritten = SimulationManagerData::decode(&manager.encode()?)?;
        let (water, _) = rewritten.decode_water_prefix()?;
        assert_eq!(water.sea_level(), 41.5);
        assert_eq!(water.sources()[0].parameters[4], 42);
        assert_eq!(water.waves(), &[new_wave.clone()]);
        let (instance, instance_end) = manager.decode_instance_prefix()?;
        assert_eq!(instance.names(), &[]);
        assert_eq!(instance.group_count(), Some(0));
        let after_instance = manager.serialized_object_graph()[instance_end..].to_vec();
        let mut instance_editor = instance.into_editor();
        instance_editor.names_mut().push(InstanceName {
            id: 0x0102_0304,
            name: Some("A".to_owned()),
        });
        manager.replace_instance(&instance_editor)?;
        let (edited_instance, edited_instance_end) = manager.decode_instance_prefix()?;
        assert_eq!(
            edited_instance.names(),
            instance_editor.names_mut().as_slice()
        );
        assert_eq!(
            manager.serialized_object_graph()[edited_instance_end..],
            after_instance
        );
        let (path, path_end) = manager.decode_path_prefix()?;
        assert!(path.active_units().is_empty());
        assert!(path.simulation_flags().iter().all(|flag| *flag == 0));
        assert_eq!(
            &manager.serialized_object_graph()[path_end..path_end + 4],
            &[0xfe, 0x80, 0, 10]
        );
        let (net_lanes, _) = manager.decode_net_lane_prefix()?;
        assert!(net_lanes.active_lanes().is_empty());
        let (node_flags, _) = manager.decode_net_node_flags_prefix()?;
        assert!(
            node_flags
                .node_flags()
                .iter()
                .all(|node| node.flags == 0 && node.flags2 == 0)
        );
        let (problems, _) = manager.decode_net_node_problems_prefix(Some(201_499_120))?;
        assert!(problems.problems().is_empty());
        let (counters, _) = manager.decode_net_node_counters_prefix(Some(201_499_120))?;
        assert!(counters.counters().is_empty());
        let (records, _) = manager.decode_net_node_records_prefix(Some(201_499_120))?;
        assert!(records.records().is_empty());

        let mut package = sample_package();
        let mut payload = include_bytes!("../tests/fixtures/simulation-metadata-v6.bin").to_vec();
        payload.extend_from_slice(&manager.encode()?);
        package.assets.push(Asset {
            name: "edited_Data".to_owned(),
            checksum: String::new(),
            asset_type: 52,
            relative_offset: 0,
            data: payload,
        });
        let mut package_bytes = Vec::new();
        package.write_compact_to(&mut package_bytes)?;
        let reread = Package::read_from(&mut Cursor::new(package_bytes))?;
        let asset = reread
            .assets
            .iter()
            .find(|asset| asset.asset_type == 52)
            .ok_or_else(|| std::io::Error::other("missing edited _Data asset"))?;
        let (_, metadata_end) = SimulationMetaData::decode_from(&asset.data)?;
        let reread_manager = SimulationManagerData::decode(&asset.data[metadata_end..])?;
        let (water, _) = reread_manager.decode_water_prefix()?;
        assert_eq!(water.sea_level(), 41.5);
        assert_eq!(water.sources()[0].parameters[4], 42);
        assert_eq!(water.waves(), &[new_wave]);
        let (instance, _) = reread_manager.decode_instance_prefix()?;
        assert_eq!(instance.names()[0].name.as_deref(), Some("A"));
        let (path, _) = reread_manager.decode_path_prefix()?;
        assert!(path.active_units().is_empty());
        let (net_lanes, _) = reread_manager.decode_net_lane_prefix()?;
        assert!(net_lanes.active_lanes().is_empty());
        let (node_flags, _) = reread_manager.decode_net_node_flags_prefix()?;
        assert_eq!(node_flags.node_flags().len(), 32_767);
        let (problems, _) = reread_manager.decode_net_node_problems_prefix(Some(201_499_120))?;
        assert!(problems.problems().is_empty());
        let (counters, _) = reread_manager.decode_net_node_counters_prefix(Some(201_499_120))?;
        assert!(counters.counters().is_empty());
        let (records, _) = reread_manager.decode_net_node_records_prefix(Some(201_499_120))?;
        assert!(records.records().is_empty());
        Ok(())
    }

    #[test]
    fn net_node_counters_follow_active_node_order() -> Result<(), CrpError> {
        let mut flag_bytes = vec![2];
        for value in [1_u32, 0, 0] {
            flag_bytes.extend_from_slice(&value.to_be_bytes());
        }
        flag_bytes.extend(std::iter::repeat_n(0xff, 511));
        flag_bytes.push(0xfa);
        let (flags, _) = NetNodeFlagsPrefix::decode_prefix(115_018, &flag_bytes)?;
        let bytes = [0, 10, 1, 0, 11, 0, 12];
        let (counters, used) = NetNodeCountersPrefix::decode_prefix(115_018, &flags, &bytes)?;
        assert_eq!(used, bytes.len());
        assert_eq!(
            counters.counters(),
            &[NetNodeCounter {
                index: 1,
                max_wait_time: 10,
                temp_counter: 11,
                final_counter: 12,
            }]
        );
        assert_eq!(counters.encode(), bytes);
        assert!(NetNodeCountersPrefix::decode_prefix(115_018, &flags, &bytes[..6]).is_err());
        assert!(NetNodeCountersPrefix::decode_prefix(115_000, &flags, &bytes).is_err());
        Ok(())
    }

    #[test]
    fn net_node_counters_game_fixture_preserves_sparse_arrays() -> Result<(), CrpError> {
        let flags_fixture = include_bytes!("../tests/fixtures/net-node-flags-v116020-desert.bin");
        let (flags, _) = NetNodeFlagsPrefix::decode_prefix(116_020, flags_fixture)?;
        let fixture = include_bytes!("../tests/fixtures/net-node-counters-v116020-desert.bin");
        let (counters, used) = NetNodeCountersPrefix::decode_prefix(116_020, &flags, fixture)?;
        assert_eq!(used, 50);
        assert_eq!(counters.counters().len(), 2_082);
        assert_eq!(counters.counters()[0].index, 11);
        assert!(counters.counters().iter().all(|counter| {
            counter.max_wait_time == 0 && counter.temp_counter == 0 && counter.final_counter == 0
        }));
        assert_eq!(counters.encode(), fixture);
        assert!(NetNodeCountersPrefix::decode_prefix(116_020, &flags, &fixture[..49]).is_err());
        Ok(())
    }

    #[test]
    fn net_node_counters_game_fixture_preserves_nonzero_wait_time() -> Result<(), CrpError> {
        let flags_fixture =
            include_bytes!("../tests/fixtures/net-node-flags-v117013-mp23-map02.bin");
        let (flags, flag_bytes_used) = NetNodeFlagsPrefix::decode_prefix(117_013, flags_fixture)?;
        assert_eq!(flag_bytes_used, 31_793);
        let fixture = include_bytes!("../tests/fixtures/net-node-counters-v117013-mp23-map02.bin");
        let (counters, used) = NetNodeCountersPrefix::decode_prefix(117_013, &flags, fixture)?;
        assert_eq!(used, 62);
        assert_eq!(counters.counters().len(), 2_405);
        let nonzero = counters
            .counters()
            .iter()
            .filter(|counter| {
                counter.max_wait_time != 0
                    || counter.temp_counter != 0
                    || counter.final_counter != 0
            })
            .collect::<Vec<_>>();
        assert_eq!(
            nonzero,
            vec![&NetNodeCounter {
                index: 4_269,
                max_wait_time: 4,
                temp_counter: 0,
                final_counter: 0,
            }]
        );
        assert_eq!(counters.encode(), fixture);
        Ok(())
    }

    #[test]
    fn net_node_records_follow_segment_and_event_route_gates() -> Result<(), CrpError> {
        fn one_active_node_flags(version: u32) -> Result<NetNodeFlagsPrefix, CrpError> {
            let mut bytes = vec![2];
            bytes.extend_from_slice(&1_u32.to_be_bytes());
            bytes.extend_from_slice(&0_u32.to_be_bytes());
            bytes.extend_from_slice(&0_u32.to_be_bytes());
            bytes.extend(std::iter::repeat_n(0xff, 511));
            bytes.push(0xfa);
            Ok(NetNodeFlagsPrefix::decode_prefix(version, &bytes)?.0)
        }

        let mut bytes = Vec::new();
        for bits in [1.25_f32.to_bits(), (-2.5_f32).to_bits(), 3.0_f32.to_bits()] {
            bytes.extend_from_slice(&bits.to_be_bytes());
        }
        bytes.push(2);
        bytes.extend_from_slice(&11_u16.to_be_bytes());
        bytes.extend_from_slice(&22_u16.to_be_bytes());
        bytes.extend_from_slice(&33_u32.to_be_bytes());
        bytes.extend_from_slice(&44_u16.to_be_bytes());
        bytes.extend_from_slice(&55_u16.to_be_bytes());
        bytes.extend_from_slice(&[0x01, 0x02, 0x03, 66, 77, 88, 99]);
        let legacy_flags = one_active_node_flags(121_006)?;
        let (legacy, used) = NetNodeRecordsPrefix::decode_prefix(121_006, &legacy_flags, &bytes)?;
        assert_eq!(used, bytes.len());
        assert_eq!(legacy.records().len(), 1);
        let record = &legacy.records()[0];
        assert_eq!(record.index, 1);
        assert_eq!(
            record.position_bits,
            [1.25_f32.to_bits(), (-2.5_f32).to_bits(), 3.0_f32.to_bits()]
        );
        assert_eq!(record.segments, [11, 22]);
        assert_eq!(
            (
                record.build_index,
                record.building,
                record.next_building_node
            ),
            (33, 44, 55)
        );
        assert_eq!(
            (
                record.lane,
                record.lane_offset,
                record.elevation,
                record.height_offset,
                record.connect_count,
                record.event_route
            ),
            (0x01_02_03, 66, 77, 88, 99, 0)
        );
        assert_eq!(legacy.encode(), bytes);
        assert!(NetNodeRecordsPrefix::decode_prefix(121_006, &legacy_flags, &bytes[..12]).is_err());
        let mut too_many = bytes.clone();
        too_many[12] = 9;
        assert!(NetNodeRecordsPrefix::decode_prefix(121_006, &legacy_flags, &too_many).is_err());

        bytes.extend_from_slice(&123_u16.to_be_bytes());
        let modern_flags = one_active_node_flags(121_007)?;
        let (modern, used) = NetNodeRecordsPrefix::decode_prefix(121_007, &modern_flags, &bytes)?;
        assert_eq!(used, bytes.len());
        assert_eq!(modern.records()[0].event_route, 123);
        assert_eq!(modern.encode(), bytes);
        assert!(
            NetNodeRecordsPrefix::decode_prefix(121_007, &modern_flags, &bytes[..bytes.len() - 1])
                .is_err()
        );
        assert!(NetNodeRecordsPrefix::decode_prefix(121_006, &modern_flags, &bytes).is_err());
        Ok(())
    }

    #[test]
    fn net_node_records_game_fixture_preserves_segment_order() -> Result<(), CrpError> {
        let flags_fixture = include_bytes!("../tests/fixtures/net-node-flags-v116020-desert.bin");
        let (flags, _) = NetNodeFlagsPrefix::decode_prefix(116_020, flags_fixture)?;
        let fixture = include_bytes!("../tests/fixtures/net-node-records-v116020-desert.bin");
        let (records, used) = NetNodeRecordsPrefix::decode_prefix(116_020, &flags, fixture)?;
        assert_eq!(used, 66_620);
        assert_eq!(records.records().len(), 2_082);
        assert_eq!(
            records.records()[0],
            NetNodeRecord {
                index: 11,
                position_bits: [1_173_408_624, 1_134_519_429, 1_116_051_800],
                segments: vec![14_839, 4_636],
                build_index: 2_124,
                building: 0,
                next_building_node: 0,
                lane: 0,
                lane_offset: 0,
                elevation: 0,
                height_offset: 64,
                connect_count: 0,
                event_route: 0,
            }
        );
        assert_eq!(
            records
                .records()
                .iter()
                .filter(|record| !record.segments.is_empty())
                .count(),
            2_077
        );
        assert_eq!(records.encode(), fixture);
        assert!(NetNodeRecordsPrefix::decode_prefix(116_020, &flags, &fixture[..66_619]).is_err());
        Ok(())
    }

    #[test]
    fn net_segment_flags_follow_versioned_array_layout() -> Result<(), CrpError> {
        fn flags_with_first_value(count: usize, first: u32) -> Vec<u8> {
            let mut bytes = vec![2];
            for value in [first, 0, 0] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            let remaining = count - 3;
            bytes.extend(std::iter::repeat_n(0xff, remaining / 128));
            bytes.push(0x7f + (remaining % 128) as u8);
            bytes
        }

        let old_bytes = flags_with_first_value(32_767, 5);
        let (old, used) = NetSegmentFlagsPrefix::decode_prefix(194, &old_bytes)?;
        assert_eq!(used, old_bytes.len());
        assert_eq!(old.segment_flags().len(), 36_863);
        assert_eq!(
            old.segment_flags()[0],
            NetSegmentFlag {
                index: 1,
                flags: 5,
                flags2: 0
            }
        );
        assert_eq!(old.segment_flags()[32_767].flags, 0);
        assert_eq!(old.encode(), old_bytes);

        let middle_bytes = flags_with_first_value(36_863, 6);
        let (middle, used) = NetSegmentFlagsPrefix::decode_prefix(222, &middle_bytes)?;
        assert_eq!(used, middle_bytes.len());
        assert_eq!(middle.segment_flags()[0].flags, 6);
        assert_eq!(middle.segment_flags()[36_862].index, 36_863);

        let mut modern_bytes = flags_with_first_value(36_863, 7);
        modern_bytes.extend_from_slice(&flags_with_first_value(36_863, 9));
        let (modern, used) = NetSegmentFlagsPrefix::decode_prefix(114_012, &modern_bytes)?;
        assert_eq!(used, modern_bytes.len());
        assert_eq!(
            modern.segment_flags()[0],
            NetSegmentFlag {
                index: 1,
                flags: 7,
                flags2: 9
            }
        );
        assert_eq!(modern.encode(), modern_bytes);
        assert!(
            NetSegmentFlagsPrefix::decode_prefix(114_012, &modern_bytes[..modern_bytes.len() - 1])
                .is_err()
        );
        assert!(NetSegmentFlagsPrefix::decode_prefix(163, &old_bytes).is_err());
        Ok(())
    }

    #[test]
    fn net_segment_flag_fixtures_preserve_game_arrays() -> Result<(), CrpError> {
        let desert = include_bytes!("../tests/fixtures/net-segment-flags-v116020-desert.bin");
        let (modern, used) = NetSegmentFlagsPrefix::decode_prefix(116_020, desert)?;
        assert_eq!(used, 21_446);
        assert_eq!(modern.segment_flags().len(), 36_863);
        assert_eq!(
            modern.segment_flags()[5],
            NetSegmentFlag {
                index: 6,
                flags: 805_306_385,
                flags2: 0,
            }
        );
        assert_eq!(
            modern
                .segment_flags()
                .iter()
                .filter(|segment| segment.flags != 0)
                .count(),
            2_081
        );
        assert!(
            modern
                .segment_flags()
                .iter()
                .all(|segment| segment.flags2 == 0)
        );
        assert_eq!(modern.encode(), desert);

        let old = include_bytes!("../tests/fixtures/net-segment-flags-v241-co.bin");
        let (legacy, used) = NetSegmentFlagsPrefix::decode_prefix(241, old)?;
        assert_eq!(used, 517);
        assert_eq!(
            legacy.segment_flags()[474],
            NetSegmentFlag {
                index: 475,
                flags: 17,
                flags2: 0,
            }
        );
        assert_eq!(
            legacy
                .segment_flags()
                .iter()
                .filter(|segment| segment.flags != 0)
                .count(),
            30
        );
        assert_eq!(legacy.encode(), old);
        assert!(NetSegmentFlagsPrefix::decode_prefix(241, &old[..old.len() - 1]).is_err());

        let mp23 = include_bytes!("../tests/fixtures/net-segment-flags-v117013-mp23-map01.bin");
        let (with_secondary, used) = NetSegmentFlagsPrefix::decode_prefix(117_013, mp23)?;
        assert_eq!(used, 52_158);
        assert_eq!(
            with_secondary.segment_flags()[51],
            NetSegmentFlag {
                index: 52,
                flags: 1,
                flags2: 192,
            }
        );
        assert_eq!(
            with_secondary
                .segment_flags()
                .iter()
                .filter(|segment| segment.flags2 != 0)
                .count(),
            3_116
        );
        assert_eq!(with_secondary.encode(), mp23);
        Ok(())
    }

    #[test]
    fn net_segment_problems_follow_version_gates() -> Result<(), CrpError> {
        fn one_active_segment_flags(version: u32) -> Result<NetSegmentFlagsPrefix, CrpError> {
            let mut bytes = vec![2];
            for value in [1_u32, 0, 0] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend(std::iter::repeat_n(0xff, 287));
            bytes.push(0xfb);
            if version >= 114_012 {
                bytes.extend(std::iter::repeat_n(0xff, 287));
                bytes.push(0xfe);
            }
            Ok(NetSegmentFlagsPrefix::decode_prefix(version, &bytes)?.0)
        }

        let old_flags = one_active_segment_flags(225)?;
        let (old, used) = NetSegmentProblemsPrefix::decode_prefix(225, &old_flags, &[0xaa])?;
        assert_eq!(used, 0);
        assert_eq!(
            old.problems(),
            &[NetSegmentProblem {
                index: 1,
                problems1: 0,
                problems2: 0
            }]
        );
        assert!(old.encode().is_empty());

        let single_flags = one_active_segment_flags(226)?;
        let mut one_value = vec![0];
        one_value.extend_from_slice(&9_u64.to_be_bytes());
        let (single, used) =
            NetSegmentProblemsPrefix::decode_prefix(226, &single_flags, &one_value)?;
        assert_eq!(used, one_value.len());
        assert_eq!(
            single.problems(),
            &[NetSegmentProblem {
                index: 1,
                problems1: 9,
                problems2: 0
            }]
        );
        assert_eq!(single.encode(), one_value);
        assert!(
            NetSegmentProblemsPrefix::decode_prefix(226, &single_flags, &one_value[..8]).is_err()
        );

        let paired_flags = one_active_segment_flags(115_000)?;
        let mut two_values = vec![1];
        two_values.extend_from_slice(&11_u64.to_be_bytes());
        two_values.extend_from_slice(&12_u64.to_be_bytes());
        let (paired, used) =
            NetSegmentProblemsPrefix::decode_prefix(115_000, &paired_flags, &two_values)?;
        assert_eq!(used, two_values.len());
        assert_eq!(
            paired.problems(),
            &[NetSegmentProblem {
                index: 1,
                problems1: 11,
                problems2: 12
            }]
        );
        assert_eq!(paired.encode(), two_values);
        assert!(
            NetSegmentProblemsPrefix::decode_prefix(115_000, &paired_flags, &two_values[..16])
                .is_err()
        );
        assert!(NetSegmentProblemsPrefix::decode_prefix(226, &paired_flags, &one_value).is_err());
        Ok(())
    }

    #[test]
    fn net_segment_problem_fixtures_cover_game_versions() -> Result<(), CrpError> {
        let co_flags = include_bytes!("../tests/fixtures/net-segment-flags-v241-co.bin");
        let (co_flags, _) = NetSegmentFlagsPrefix::decode_prefix(241, co_flags)?;
        let co_fixture = include_bytes!("../tests/fixtures/net-segment-problems-v241-co.bin");
        let (co, used) = NetSegmentProblemsPrefix::decode_prefix(241, &co_flags, co_fixture)?;
        assert_eq!(used, 27);
        assert_eq!(co.problems().len(), 30);
        assert_eq!(
            co.problems()
                .iter()
                .filter(|problem| problem.problems1 != 0 || problem.problems2 != 0)
                .collect::<Vec<_>>(),
            vec![&NetSegmentProblem {
                index: 14_842,
                problems1: 4_611_686_018_695_823_360,
                problems2: 0,
            }]
        );
        assert_eq!(co.encode(), co_fixture);

        let desert_flags = include_bytes!("../tests/fixtures/net-segment-flags-v116020-desert.bin");
        let (desert_flags, _) = NetSegmentFlagsPrefix::decode_prefix(116_020, desert_flags)?;
        let desert_fixture =
            include_bytes!("../tests/fixtures/net-segment-problems-v116020-desert.bin");
        let (desert, used) =
            NetSegmentProblemsPrefix::decode_prefix(116_020, &desert_flags, desert_fixture)?;
        assert_eq!(used, 33);
        assert_eq!(desert.problems().len(), 2_081);
        assert!(
            desert
                .problems()
                .iter()
                .all(|problem| problem.problems1 == 0 && problem.problems2 == 0)
        );
        assert_eq!(desert.encode(), desert_fixture);
        assert!(
            NetSegmentProblemsPrefix::decode_prefix(116_020, &desert_flags, &desert_fixture[..32])
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn net_segment_traffic_lights_follow_byte_and_int_gates() -> Result<(), CrpError> {
        fn one_active_segment_flags(version: u32) -> Result<NetSegmentFlagsPrefix, CrpError> {
            let mut bytes = vec![2];
            for value in [1_u32, 0, 0] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend(std::iter::repeat_n(0xff, 287));
            bytes.push(0xfb);
            bytes.extend(std::iter::repeat_n(0xff, 287));
            bytes.push(0xfe);
            Ok(NetSegmentFlagsPrefix::decode_prefix(version, &bytes)?.0)
        }

        let old_flags = one_active_segment_flags(115_016)?;
        let old_bytes = [1, 3, 250];
        let (old, used) =
            NetSegmentTrafficLightsPrefix::decode_prefix(115_016, &old_flags, &old_bytes)?;
        assert_eq!(used, old_bytes.len());
        assert_eq!(
            old.states(),
            &[NetSegmentTrafficLight {
                index: 1,
                state0: 3,
                state1: 250
            }]
        );
        assert_eq!(old.encode(), old_bytes);
        assert!(
            NetSegmentTrafficLightsPrefix::decode_prefix(115_016, &old_flags, &old_bytes[..2])
                .is_err()
        );

        let new_flags = one_active_segment_flags(115_017)?;
        let mut new_bytes = vec![1];
        new_bytes.extend_from_slice(&(-3_i32).to_be_bytes());
        new_bytes.extend_from_slice(&123_456_i32.to_be_bytes());
        let (new, used) =
            NetSegmentTrafficLightsPrefix::decode_prefix(115_017, &new_flags, &new_bytes)?;
        assert_eq!(used, new_bytes.len());
        assert_eq!(
            new.states(),
            &[NetSegmentTrafficLight {
                index: 1,
                state0: -3,
                state1: 123_456
            }]
        );
        assert_eq!(new.encode(), new_bytes);
        assert!(
            NetSegmentTrafficLightsPrefix::decode_prefix(
                115_017,
                &new_flags,
                &new_bytes[..new_bytes.len() - 1]
            )
            .is_err()
        );
        assert!(
            NetSegmentTrafficLightsPrefix::decode_prefix(115_016, &new_flags, &old_bytes).is_err()
        );
        Ok(())
    }

    #[test]
    fn net_segment_traffic_light_fixtures_cover_game_versions() -> Result<(), CrpError> {
        let co_flags = include_bytes!("../tests/fixtures/net-segment-flags-v241-co.bin");
        let (co_flags, _) = NetSegmentFlagsPrefix::decode_prefix(241, co_flags)?;
        let co_fixture = include_bytes!("../tests/fixtures/net-segment-traffic-v241-co.bin");
        let (co, used) = NetSegmentTrafficLightsPrefix::decode_prefix(241, &co_flags, co_fixture)?;
        assert_eq!(used, 1);
        assert_eq!(co.states().len(), 30);
        assert!(
            co.states()
                .iter()
                .all(|state| state.state0 == 0 && state.state1 == 0)
        );
        assert_eq!(co.encode(), co_fixture);

        let desert_flags = include_bytes!("../tests/fixtures/net-segment-flags-v116020-desert.bin");
        let (desert_flags, _) = NetSegmentFlagsPrefix::decode_prefix(116_020, desert_flags)?;
        let desert_fixture =
            include_bytes!("../tests/fixtures/net-segment-traffic-v116020-desert.bin");
        let (desert, used) =
            NetSegmentTrafficLightsPrefix::decode_prefix(116_020, &desert_flags, desert_fixture)?;
        assert_eq!(used, 89);
        assert_eq!(desert.states().len(), 2_081);
        assert_eq!(
            desert
                .states()
                .iter()
                .filter(|state| state.state0 != 0 || state.state1 != 0)
                .count(),
            6
        );
        assert!(
            desert
                .states()
                .iter()
                .any(|state| { state.index == 2_555 && state.state0 == 34 && state.state1 == 34 })
        );
        assert_eq!(desert.encode(), desert_fixture);
        assert!(
            NetSegmentTrafficLightsPrefix::decode_prefix(
                116_020,
                &desert_flags,
                &desert_fixture[..88]
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn net_segment_coverage_follows_version_gates() -> Result<(), CrpError> {
        fn one_active_segment_flags(version: u32) -> Result<NetSegmentFlagsPrefix, CrpError> {
            let mut bytes = vec![2];
            for value in [1_u32, 0, 0] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            let full_runs = if version < 222 { 255 } else { 287 };
            bytes.extend(std::iter::repeat_n(0xff, full_runs));
            bytes.push(0xfb);
            if version >= 114_012 {
                bytes.extend(std::iter::repeat_n(0xff, 287));
                bytes.push(0xfe);
            }
            Ok(NetSegmentFlagsPrefix::decode_prefix(version, &bytes)?.0)
        }

        let early_flags = one_active_segment_flags(184)?;
        let (early, used) = NetSegmentCoveragePrefix::decode_prefix(184, &early_flags, &[0xaa])?;
        assert_eq!(used, 0);
        assert_eq!(
            early.coverage(),
            &[NetSegmentCoverage {
                index: 1,
                fire_coverage: 0,
                wetness: 0
            }]
        );
        assert!(early.encode().is_empty());

        let fire_flags = one_active_segment_flags(185)?;
        let fire_bytes = [0, 9];
        let (fire, used) = NetSegmentCoveragePrefix::decode_prefix(185, &fire_flags, &fire_bytes)?;
        assert_eq!(used, 2);
        assert_eq!(
            fire.coverage(),
            &[NetSegmentCoverage {
                index: 1,
                fire_coverage: 9,
                wetness: 0
            }]
        );
        assert_eq!(fire.encode(), fire_bytes);

        let wet_flags = one_active_segment_flags(224)?;
        let wet_bytes = [0, 9, 0, 10];
        let (wet, used) = NetSegmentCoveragePrefix::decode_prefix(224, &wet_flags, &wet_bytes)?;
        assert_eq!(used, 4);
        assert_eq!(
            wet.coverage(),
            &[NetSegmentCoverage {
                index: 1,
                fire_coverage: 9,
                wetness: 10
            }]
        );
        assert_eq!(wet.encode(), wet_bytes);
        assert!(NetSegmentCoveragePrefix::decode_prefix(224, &wet_flags, &wet_bytes[..3]).is_err());
        assert!(NetSegmentCoveragePrefix::decode_prefix(185, &wet_flags, &fire_bytes).is_err());
        Ok(())
    }

    #[test]
    fn net_segment_coverage_fixtures_preserve_game_values() -> Result<(), CrpError> {
        let co_flags = include_bytes!("../tests/fixtures/net-segment-flags-v241-co.bin");
        let (co_flags, _) = NetSegmentFlagsPrefix::decode_prefix(241, co_flags)?;
        let co_fixture = include_bytes!("../tests/fixtures/net-segment-coverage-v241-co.bin");
        let (co, used) = NetSegmentCoveragePrefix::decode_prefix(241, &co_flags, co_fixture)?;
        assert_eq!(used, 7);
        assert_eq!(co.coverage().len(), 30);
        assert_eq!(
            co.coverage()
                .iter()
                .filter(|item| item.wetness != 0)
                .collect::<Vec<_>>(),
            vec![&NetSegmentCoverage {
                index: 14_842,
                fire_coverage: 0,
                wetness: 255
            }]
        );
        assert_eq!(co.encode(), co_fixture);

        let desert_flags = include_bytes!("../tests/fixtures/net-segment-flags-v116020-desert.bin");
        let (desert_flags, _) = NetSegmentFlagsPrefix::decode_prefix(116_020, desert_flags)?;
        let desert_fixture =
            include_bytes!("../tests/fixtures/net-segment-coverage-v116020-desert.bin");
        let (desert, used) =
            NetSegmentCoveragePrefix::decode_prefix(116_020, &desert_flags, desert_fixture)?;
        assert_eq!(used, 34);
        assert_eq!(desert.coverage().len(), 2_081);
        assert!(
            desert
                .coverage()
                .iter()
                .all(|item| item.fire_coverage == 0 && item.wetness == 0)
        );
        assert_eq!(desert.encode(), desert_fixture);

        let mp23_flags =
            include_bytes!("../tests/fixtures/net-segment-flags-v117013-mp23-map01.bin");
        let (mp23_flags, _) = NetSegmentFlagsPrefix::decode_prefix(117_013, mp23_flags)?;
        let mp23_fixture =
            include_bytes!("../tests/fixtures/net-segment-coverage-v117013-mp23-map01.bin");
        let (mp23, used) =
            NetSegmentCoveragePrefix::decode_prefix(117_013, &mp23_flags, mp23_fixture)?;
        assert_eq!(used, 55);
        assert_eq!(mp23.coverage().len(), 3_116);
        assert_eq!(
            mp23.coverage()
                .iter()
                .filter(|item| item.wetness != 0)
                .collect::<Vec<_>>(),
            vec![&NetSegmentCoverage {
                index: 12_733,
                fire_coverage: 0,
                wetness: 37
            }]
        );
        assert_eq!(mp23.encode(), mp23_fixture);
        assert!(
            NetSegmentCoveragePrefix::decode_prefix(117_013, &mp23_flags, &mp23_fixture[..54])
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn net_segment_environment_respects_version_gates_and_encoded_bytes() -> Result<(), CrpError> {
        fn flags(version: u32) -> Result<NetSegmentFlagsPrefix, CrpError> {
            let mut bytes = vec![2];
            for value in [1_u32, 0, 0] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend(std::iter::repeat_n(
                0xff,
                if version < 222 { 255 } else { 287 },
            ));
            bytes.push(0xfb);
            if version >= 114_012 {
                bytes.extend(std::iter::repeat_n(0xff, 287));
                bytes.push(0xfe);
            }
            Ok(NetSegmentFlagsPrefix::decode_prefix(version, &bytes)?.0)
        }
        let expected = |condition, noise_buffer, noise_density, attractiveness| {
            vec![NetSegmentEnvironment {
                index: 1,
                condition,
                noise_buffer,
                noise_density,
                attractiveness,
            }]
        };
        for (version, bytes, fields) in [
            (228, vec![], expected(0, 0, 0, 0)),
            (229, vec![0, 7], expected(7, 0, 0, 0)),
            (109_006, vec![0, 7], expected(7, 0, 0, 0)),
            (
                109_007,
                vec![0, 7, 0, 0x12, 0x34, 0, 8],
                expected(7, 0x1234, 8, 0),
            ),
            (
                121_029,
                vec![0, 7, 0, 0x12, 0x34, 0, 8],
                expected(7, 0x1234, 8, 0),
            ),
            (
                121_030,
                vec![0, 7, 0, 0x12, 0x34, 0, 8, 0, 9],
                expected(7, 0x1234, 8, 9),
            ),
        ] {
            let segment_flags = flags(version)?;
            let (prefix, used) =
                NetSegmentEnvironmentPrefix::decode_prefix(version, &segment_flags, &bytes)?;
            assert_eq!(used, bytes.len());
            assert_eq!(prefix.environment(), fields);
            assert_eq!(prefix.encode(), bytes);
            if !bytes.is_empty() {
                assert!(
                    NetSegmentEnvironmentPrefix::decode_prefix(
                        version,
                        &segment_flags,
                        &bytes[..bytes.len() - 1]
                    )
                    .is_err()
                );
            }
            assert!(
                NetSegmentEnvironmentPrefix::decode_prefix(version + 1, &segment_flags, &bytes)
                    .is_err()
            );
        }
        let mut empty_flags_bytes = vec![0xff; 287];
        empty_flags_bytes.push(0xfe);
        empty_flags_bytes.extend(std::iter::repeat_n(0xff, 287));
        empty_flags_bytes.push(0xfe);
        let empty_flags = NetSegmentFlagsPrefix::decode_prefix(121_030, &empty_flags_bytes)?.0;
        let (empty, used) =
            NetSegmentEnvironmentPrefix::decode_prefix(121_030, &empty_flags, &[0xaa])?;
        assert_eq!(used, 0);
        assert!(empty.environment().is_empty());
        assert!(empty.encode().is_empty());
        Ok(())
    }

    #[test]
    fn net_segment_environment_fixtures_preserve_game_bytes() -> Result<(), CrpError> {
        for (version, flag_bytes, environment_bytes, count) in [
            (
                241,
                &include_bytes!("../tests/fixtures/net-segment-flags-v241-co.bin")[..],
                &include_bytes!("../tests/fixtures/net-segment-environment-v241-co.bin")[..],
                30,
            ),
            (
                116_020,
                &include_bytes!("../tests/fixtures/net-segment-flags-v116020-desert.bin")[..],
                &include_bytes!("../tests/fixtures/net-segment-environment-v116020-desert.bin")[..],
                2_081,
            ),
            (
                117_013,
                &include_bytes!("../tests/fixtures/net-segment-flags-v117013-mp23-map01.bin")[..],
                &include_bytes!("../tests/fixtures/net-segment-environment-v117013-mp23-map01.bin")
                    [..],
                3_116,
            ),
        ] {
            let (flags, _) = NetSegmentFlagsPrefix::decode_prefix(version, flag_bytes)?;
            let (environment, used) =
                NetSegmentEnvironmentPrefix::decode_prefix(version, &flags, environment_bytes)?;
            assert_eq!(used, environment_bytes.len());
            assert_eq!(environment.environment().len(), count);
            assert!(environment.environment().iter().all(|item| {
                item.condition == 0
                    && item.noise_buffer == 0
                    && item.noise_density == 0
                    && item.attractiveness == 0
            }));
            assert_eq!(environment.encode(), environment_bytes);
            assert!(
                NetSegmentEnvironmentPrefix::decode_prefix(
                    version,
                    &flags,
                    &environment_bytes[..environment_bytes.len() - 1]
                )
                .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn net_segment_records_follow_fixed_field_gates() -> Result<(), CrpError> {
        fn flags(version: u32) -> Result<NetSegmentFlagsPrefix, CrpError> {
            let mut bytes = vec![2];
            for value in [1_u32, 0, 0] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend(std::iter::repeat_n(
                0xff,
                if version < 222 { 255 } else { 287 },
            ));
            bytes.push(0xfb);
            if version >= 114_012 {
                bytes.extend(std::iter::repeat_n(0xff, 287));
                bytes.push(0xfe);
            }
            Ok(NetSegmentFlagsPrefix::decode_prefix(version, &bytes)?.0)
        }
        let mut base = Vec::new();
        base.extend_from_slice(&0x0102_0304_u32.to_be_bytes());
        base.extend_from_slice(&0x0506_0708_u32.to_be_bytes());
        base.extend_from_slice(&[0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e]);
        for bits in [1_u32, 2, 3, 4, 5, 6] {
            base.extend_from_slice(&bits.to_be_bytes());
        }
        for value in [7_u16, 8, 9, 10, 11, 12, 13] {
            base.extend_from_slice(&value.to_be_bytes());
        }
        base.push(14);
        assert_eq!(base.len(), 53);
        for (version, suffix, expected_length) in [
            (164, vec![], 53),
            (312, vec![0x12, 0x34], 55),
            (113_015, vec![0x12, 0x34, 1], 56),
            (114_002, vec![0x12, 0x34, 1, 1], 57),
            (120_003, vec![0x12, 0x34, 1, 1, 1, 1], 59),
            (
                121_005,
                vec![0x12, 0x34, 1, 1, 1, 1, 0xab, 0xcd, 0xef, 0x01],
                63,
            ),
        ] {
            let mut bytes = base.clone();
            bytes.extend_from_slice(&suffix);
            let segment_flags = flags(version)?;
            let (prefix, used) =
                NetSegmentRecordsPrefix::decode_prefix(version, &segment_flags, &bytes)?;
            assert_eq!(used, expected_length);
            assert_eq!(prefix.encode(), bytes);
            let record = &prefix.records()[0];
            assert_eq!(record.index, 1);
            assert_eq!(
                (record.build_index, record.modified_index),
                (0x0102_0304, 0x0506_0708)
            );
            assert_eq!((record.lanes, record.path), (0x09_0a_0b, 0x0c_0d_0e));
            assert_eq!(record.start_direction_bits, [1, 2, 3]);
            assert_eq!(record.end_direction_bits, [4, 5, 6]);
            assert_eq!((record.start_node, record.end_node), (7, 8));
            assert_eq!(record.blocks, [9, 10, 11, 12]);
            assert_eq!((record.traffic_buffer, record.traffic_density), (13, 14));
            assert_eq!(record.name_seed, if version >= 312 { 0x1234 } else { 0 });
            assert_eq!(
                record.override_path_find_direction_limit,
                u8::from(version >= 113_015)
            );
            assert_eq!(record.tree_present, u8::from(version >= 114_002));
            assert_eq!(
                record.fence_present,
                if version >= 120_003 { [1, 1] } else { [0, 0] }
            );
            assert_eq!(
                record.event_route,
                if version >= 121_005 { 0xabcd } else { 0 }
            );
            assert_eq!(
                record.next_event_route_segment,
                if version >= 121_005 { 0xef01 } else { 0 }
            );
            let tree_bytes = [0, 0, 42, 0, 0, 0xee];
            let (tree, tree_used) =
                NetTreePrefabPrefix::decode_prefix(version, &prefix, &tree_bytes)?;
            if version >= 114_002 {
                assert_eq!(tree_used, 5);
                assert_eq!(tree.indices(), [42]);
                assert!(tree.names().is_empty());
                assert_eq!(tree.encode(), tree_bytes[..5]);
                assert!(
                    NetTreePrefabPrefix::decode_prefix(version, &prefix, &tree_bytes[..4]).is_err()
                );
            } else {
                assert_eq!(tree_used, 0);
                assert!(tree.indices().is_empty());
                assert!(tree.encode().is_empty());
            }
            let fence_bytes = [0, 0, 21, 0, 1, 0xff, 0, 0, 22, 0, 1, 0x80, 0, 1, 0xd2, 0xee];
            let (fence, fence_used) =
                NetFencePrefabPrefix::decode_prefix(version, &prefix, &fence_bytes)?;
            if version >= 120_003 {
                assert_eq!(fence_used, 15);
                assert_eq!(fence.left_indices(), [21]);
                assert_eq!(fence.left_names(), [None]);
                assert_eq!(fence.right_indices(), [22]);
                assert_eq!(fence.right_names(), [Some("R".to_owned())]);
                assert_eq!(fence.encode(), fence_bytes[..15]);
                assert!(
                    NetFencePrefabPrefix::decode_prefix(version, &prefix, &fence_bytes[..14])
                        .is_err()
                );
                if version == 120_003 {
                    let mut noncanonical_bool = bytes.clone();
                    noncanonical_bool[57] = 2;
                    noncanonical_bool[58] = 0;
                    let (noncanonical_record, _) = NetSegmentRecordsPrefix::decode_prefix(
                        version,
                        &segment_flags,
                        &noncanonical_bool,
                    )?;
                    let (empty_fence, used) = NetFencePrefabPrefix::decode_prefix(
                        version,
                        &noncanonical_record,
                        &[0, 0, 0, 0],
                    )?;
                    assert_eq!(used, 4);
                    assert!(empty_fence.left_indices().is_empty());
                    assert!(empty_fence.right_indices().is_empty());
                }
            } else {
                assert_eq!(fence_used, 0);
                assert!(fence.left_indices().is_empty());
                assert!(fence.right_indices().is_empty());
                assert!(fence.encode().is_empty());
            }
            assert!(
                NetSegmentRecordsPrefix::decode_prefix(
                    version,
                    &segment_flags,
                    &bytes[..bytes.len() - 1]
                )
                .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn net_segment_records_game_fixture_preserves_fields() -> Result<(), CrpError> {
        let flags = include_bytes!("../tests/fixtures/net-segment-flags-v116020-desert.bin");
        let (flags, _) = NetSegmentFlagsPrefix::decode_prefix(116_020, flags)?;
        let bytes = include_bytes!("../tests/fixtures/net-segment-records-v116020-desert.bin");
        let (prefix, used) = NetSegmentRecordsPrefix::decode_prefix(116_020, &flags, bytes)?;
        assert_eq!(used, 118_617);
        assert_eq!(prefix.records().len(), 2_081);
        let first = &prefix.records()[0];
        assert_eq!(first.index, 6);
        assert_eq!((first.build_index, first.modified_index), (3916, 3917));
        assert_eq!((first.lanes, first.path), (208_302, 0));
        assert_eq!((first.start_node, first.end_node), (13_411, 7_207));
        assert_eq!(first.name_seed, 25_077);
        assert_eq!(prefix.encode(), &bytes[..used]);
        assert_eq!(&bytes[used..], &[0, 0, 0, 0]);
        assert!(
            NetSegmentRecordsPrefix::decode_prefix(116_020, &flags, &bytes[..used - 1]).is_err()
        );
        Ok(())
    }

    #[test]
    fn net_guide_counts_follow_segment_records() -> Result<(), CrpError> {
        let bytes = include_bytes!("../tests/fixtures/net-segment-records-v116020-desert.bin");
        let (counts, used) = NetGuideCountsPrefix::decode_prefix(116_020, &bytes[118_617..])?;
        assert_eq!(used, 4);
        assert_eq!(counts.guide_count(), 0);
        assert_eq!(counts.milestone_count(), 0);
        assert_eq!(counts.encode(), [0, 0, 0, 0]);

        let expected = [0x12, 0x34, 0xab, 0xcd, 0xef];
        let (counts, used) = NetGuideCountsPrefix::decode_prefix(164, &expected)?;
        assert_eq!(used, 4);
        assert_eq!(counts.guide_count(), 0x1234);
        assert_eq!(counts.milestone_count(), 0xabcd);
        assert_eq!(counts.encode(), expected[..4]);
        assert!(NetGuideCountsPrefix::decode_prefix(163, &expected).is_err());
        for length in 0..4 {
            assert!(NetGuideCountsPrefix::decode_prefix(164, &expected[..length]).is_err());
        }
        Ok(())
    }

    #[test]
    fn net_info_prefab_table_reads_indices_and_names() -> Result<(), CrpError> {
        let version = 164;
        let mut node_bytes = vec![2];
        for value in [1_u32, 0, 0] {
            node_bytes.extend_from_slice(&value.to_be_bytes());
        }
        node_bytes.extend(std::iter::repeat_n(0xff, 127));
        node_bytes.push(0xfb);
        let (nodes, _) = NetNodeFlagsPrefix::decode_prefix(version, &node_bytes)?;
        let mut segment_bytes = vec![2];
        for value in [1_u32, 0, 0] {
            segment_bytes.extend_from_slice(&value.to_be_bytes());
        }
        segment_bytes.extend(std::iter::repeat_n(0xff, 255));
        segment_bytes.push(0xfb);
        let (segments, _) = NetSegmentFlagsPrefix::decode_prefix(version, &segment_bytes)?;
        let (counts, _) = NetGuideCountsPrefix::decode_prefix(version, &[0, 1, 0, 1])?;
        // Four literal prefab indices, two names: null and compact "A".
        let bytes = [
            3, 0, 2, 0, 3, 0, 4, 0, 5, 0, 2, 0xff, 0x80, 0, 1, 0xc1, 0xee,
        ];
        let (table, used) =
            NetInfoPrefabPrefix::decode_prefix(version, &nodes, &segments, &counts, &bytes)?;
        assert_eq!(used, 16);
        assert_eq!(table.node_info_indices(), [2]);
        assert_eq!(table.segment_info_indices(), [3]);
        assert_eq!(table.guide_indices(), [4]);
        assert_eq!(table.milestone_indices(), [5]);
        assert_eq!(table.names(), [None, Some("A".to_owned())]);
        assert_eq!(table.encode(), bytes[..used]);
        assert!(
            NetInfoPrefabPrefix::decode_prefix(version, &nodes, &segments, &counts, &bytes[..15])
                .is_err()
        );
        assert!(
            NetInfoPrefabPrefix::decode_prefix(version + 1, &nodes, &segments, &counts, &bytes)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn net_info_prefab_game_fixture_preserves_names() -> Result<(), CrpError> {
        let (nodes, _) = NetNodeFlagsPrefix::decode_prefix(
            116_020,
            include_bytes!("../tests/fixtures/net-node-flags-v116020-desert.bin"),
        )?;
        let (segments, _) = NetSegmentFlagsPrefix::decode_prefix(
            116_020,
            include_bytes!("../tests/fixtures/net-segment-flags-v116020-desert.bin"),
        )?;
        let (counts, _) = NetGuideCountsPrefix::decode_prefix(116_020, &[0; 4])?;
        let bytes = include_bytes!("../tests/fixtures/net-info-prefabs-v116020-desert.bin");
        let (table, used) =
            NetInfoPrefabPrefix::decode_prefix(116_020, &nodes, &segments, &counts, bytes)?;
        assert_eq!(used, 6_946);
        assert_eq!(table.node_info_indices().len(), 2_082);
        assert_eq!(table.segment_info_indices().len(), 2_081);
        assert!(table.guide_indices().is_empty());
        assert!(table.milestone_indices().is_empty());
        assert_eq!(table.names()[0].as_deref(), Some("Pedestrian Gravel"));
        assert_eq!(
            table.names().iter().filter(|name| name.is_some()).count(),
            22
        );
        assert!(
            table
                .names()
                .iter()
                .any(|name| name.as_deref() == Some("Water Pipe"))
        );
        assert_eq!(table.encode(), bytes);
        assert!(
            NetInfoPrefabPrefix::decode_prefix(
                116_020,
                &nodes,
                &segments,
                &counts,
                &bytes[..bytes.len() - 1]
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn net_tree_prefab_game_fixture_preserves_null_names() -> Result<(), CrpError> {
        let (flags, _) = NetSegmentFlagsPrefix::decode_prefix(
            116_020,
            include_bytes!("../tests/fixtures/net-segment-flags-v116020-desert.bin"),
        )?;
        let records_bytes =
            include_bytes!("../tests/fixtures/net-segment-records-v116020-desert.bin");
        let (records, _) = NetSegmentRecordsPrefix::decode_prefix(116_020, &flags, records_bytes)?;
        let bytes = include_bytes!("../tests/fixtures/net-tree-prefabs-v116020-desert.bin");
        let (tree, used) = NetTreePrefabPrefix::decode_prefix(116_020, &records, bytes)?;
        assert_eq!(used, 44);
        assert!(tree.indices().is_empty());
        assert_eq!(tree.names().len(), 42);
        assert!(tree.names().iter().all(Option::is_none));
        assert_eq!(tree.encode(), &bytes[..used]);
        assert_eq!(&bytes[used..], &[0xfe, 0xfe]);
        assert!(NetTreePrefabPrefix::decode_prefix(116_020, &records, &bytes[..43]).is_err());
        Ok(())
    }

    #[test]
    fn net_manager_tail_follows_object_and_scalar_version_gates() -> Result<(), CrpError> {
        for version in [164, 166, 236, 237, 295, 318, 319, 321] {
            let (counts, _) = NetGuideCountsPrefix::decode_prefix(version, &[0; 4])?;
            let header_count = 8
                + usize::from(version >= 166)
                + 3 * usize::from(version >= 236)
                + usize::from(version >= 237)
                + 2 * usize::from(version >= 295)
                + 3 * usize::from(version >= 318);
            let mut bytes = vec![0xff; header_count];
            if version >= 237 {
                bytes.extend_from_slice(&[7, 8]);
            }
            if version >= 319 {
                bytes.extend_from_slice(&[1, 2, 3]);
            }
            if version >= 321 {
                bytes.extend_from_slice(&[0, 0, 2, 1]);
                bytes.extend_from_slice(&1_u64.to_be_bytes());
                bytes.extend_from_slice(&2_u64.to_be_bytes());
            }
            bytes.extend_from_slice(&[0, 6, 0xee]);
            let (tail, used) = NetManagerTailPrefix::decode_prefix(version, &counts, &bytes)?;
            assert_eq!(used, bytes.len() - 1);
            assert_eq!(tail.object_headers().len(), header_count);
            assert!(
                tail.object_headers()
                    .iter()
                    .all(|item| *item == NetObjectHeader::Null)
            );
            assert_eq!(
                tail.max_wetness(),
                if version >= 237 { (7, 8) } else { (0, 0) }
            );
            assert_eq!(
                tail.name_modified_count(),
                if version >= 319 { 0x01_02_03 } else { 0 }
            );
            assert_eq!(
                tail.adjusted_segments(),
                if version >= 321 { &[1, 2][..] } else { &[][..] }
            );
            assert_eq!(tail.profile_end_shared_string_index(), Some(6));
            assert_eq!(tail.encode(), bytes[..used]);
            assert!(
                NetManagerTailPrefix::decode_prefix(version, &counts, &bytes[..used - 1]).is_err()
            );
        }
        let (counts, _) = NetGuideCountsPrefix::decode_prefix(164, &[0; 4])?;
        let mut mixed = vec![0xfe, 0xfe, 0x80, 0, 1, 0xc1, 0xfe, 0, 1, 0, 0, 5];
        mixed.extend(std::iter::repeat_n(0xff, 5));
        mixed.extend_from_slice(&[0, 6]);
        let (tail, used) = NetManagerTailPrefix::decode_prefix(164, &counts, &mixed)?;
        assert_eq!(used, mixed.len());
        assert_eq!(
            tail.object_headers()[0],
            NetObjectHeader::NewType("A".to_owned())
        );
        assert_eq!(tail.object_headers()[1], NetObjectHeader::NewSharedType(1));
        assert_eq!(tail.object_headers()[2], NetObjectHeader::Existing(5));
        assert!(NetManagerTailPrefix::decode_prefix(164, &counts, &[0xfe, 0xff]).is_err());
        let (counts, _) = NetGuideCountsPrefix::decode_prefix(321, &[0; 4])?;
        let mut oversized = vec![0xff; 18];
        oversized.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x02, 0x41]);
        assert!(NetManagerTailPrefix::decode_prefix(321, &counts, &oversized).is_err());
        Ok(())
    }

    #[test]
    fn net_manager_tail_game_fixture_reaches_building_manager() -> Result<(), CrpError> {
        let bytes = include_bytes!("../tests/fixtures/net-manager-tail-v116020-desert.bin");
        let (counts, _) = NetGuideCountsPrefix::decode_prefix(116_020, &[0; 4])?;
        let (tail, used) = NetManagerTailPrefix::decode_prefix(116_020, &counts, bytes)?;
        assert_eq!(used, 350);
        assert_eq!(tail.object_headers().len(), 18);
        assert_eq!(
            tail.object_headers()[0],
            NetObjectHeader::NewType(
                "NetNodeInstanceGuide, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null".to_owned()
            )
        );
        assert_eq!(tail.object_headers()[1], NetObjectHeader::NewSharedType(31));
        assert_eq!(tail.profile_end_shared_string_index(), Some(6));
        assert_eq!(tail.encode(), &bytes[..used]);
        let mut next = crate::data_serializer::Reader::new(&bytes[used..]);
        assert_eq!(next.read_u8()?, 0xfe);
        assert_eq!(
            next.read_unique_string()?.as_deref(),
            Some("BuildingManager")
        );
        assert_eq!(next.remaining(), 0);
        assert!(NetManagerTailPrefix::decode_prefix(116_020, &counts, &bytes[..used - 1]).is_err());
        Ok(())
    }

    #[test]
    fn building_flags_follow_both_inner_version_gates() -> Result<(), CrpError> {
        fn encoded_flags(count: usize, first: u32) -> Vec<u8> {
            let mut bytes = vec![2];
            for value in [first, 0_u32, 0_u32] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            let remaining = count - 3;
            bytes.extend(std::iter::repeat_n(0xff, remaining / 128));
            bytes.push(0x7f + u8::try_from(remaining % 128).unwrap());
            bytes
        }
        for (version, primary_count, secondary) in [
            (221, 32_767, false),
            (222, 49_151, false),
            (112_029, 49_151, false),
            (112_030, 49_151, true),
        ] {
            let mut bytes = encoded_flags(primary_count, 0x1234_5678);
            if secondary {
                bytes.extend(encoded_flags(49_151, 0x8765_4321));
            }
            bytes.extend_from_slice(&[0, 1, 0, 2]);
            let (flags, used) = BuildingFlagsPrefix::decode_prefix(version, &bytes)?;
            assert_eq!(flags.format_version(), version);
            assert_eq!(flags.building_flags().len(), 49_151);
            assert_eq!(flags.building_flags()[0].index, 1);
            assert_eq!(flags.building_flags()[0].flags, 0x1234_5678);
            assert_eq!(
                flags.building_flags()[0].flags2,
                if secondary { 0x8765_4321 } else { 0 }
            );
            assert_eq!(flags.building_flags()[32_767].flags, 0);
            assert_eq!(flags.building_flags()[49_150].flags2, 0);
            assert_eq!(flags.encode(), bytes[..used]);
            assert_eq!(&bytes[used..], &[0, 1, 0, 2]);
            assert!(BuildingFlagsPrefix::decode_prefix(version, &bytes[..used - 1]).is_err());
        }
        assert!(BuildingFlagsPrefix::decode_prefix(163, &[]).is_err());
        Ok(())
    }

    #[test]
    fn building_flags_game_fixture_preserves_boundary() -> Result<(), CrpError> {
        let bytes = include_bytes!("../tests/fixtures/building-flags-v116020-desert.bin");
        let (flags, used) = BuildingFlagsPrefix::decode_prefix(116_020, bytes)?;
        assert_eq!(used, 2_792);
        assert_eq!(flags.encode(), bytes[..used]);
        assert_eq!(flags.building_flags()[22].flags, 0x25);
        assert_eq!(flags.building_flags()[38].flags, 0x05);
        assert_eq!(flags.building_flags()[305].flags, 0x10_0025);
        assert_eq!(
            flags
                .building_flags()
                .iter()
                .filter(|item| item.flags != 0)
                .count(),
            189
        );
        assert!(flags.building_flags().iter().all(|item| item.flags2 == 0));
        assert_eq!(&bytes[used..], &[0, 0, 0, 0]);
        assert!(BuildingFlagsPrefix::decode_prefix(116_020, &bytes[..used - 1]).is_err());
        Ok(())
    }

    #[test]
    fn building_prefab_indices_follow_three_counts_and_names() -> Result<(), CrpError> {
        let flag_bytes = include_bytes!("../tests/fixtures/building-flags-v116020-desert.bin");
        let (flags, flag_bytes_used) = BuildingFlagsPrefix::decode_prefix(116_020, flag_bytes)?;
        fn encoded_literals(values: &[u16]) -> Vec<u8> {
            let mut bytes = Vec::new();
            for chunk in values.chunks(64) {
                bytes.push(u8::try_from(chunk.len() - 1).unwrap());
                for value in chunk {
                    bytes.extend_from_slice(&value.to_be_bytes());
                }
            }
            bytes
        }
        let building_indices = (100..289).collect::<Vec<u16>>();
        let mut bytes = encoded_literals(&building_indices);
        bytes.extend_from_slice(&[1, 0, 7, 0, 8]);
        bytes.extend_from_slice(&[0, 0, 9]);
        bytes.extend_from_slice(&[0, 2, 0xff]);
        crate::data_serializer::write_unique_string(&mut bytes, "A")?;
        let (prefabs, used) = BuildingPrefabPrefix::decode_prefix(116_020, &flags, 2, 1, &bytes)?;
        assert_eq!(used, bytes.len());
        assert_eq!(prefabs.building_indices(), building_indices);
        assert_eq!(prefabs.guide_indices(), &[7, 8]);
        assert_eq!(prefabs.milestone_indices(), &[9]);
        assert_eq!(prefabs.names(), &[None, Some("A".to_owned())]);
        assert_eq!(prefabs.encode(), bytes);
        assert!(
            BuildingPrefabPrefix::decode_prefix(116_020, &flags, 2, 1, &bytes[..used - 1]).is_err()
        );
        assert!(BuildingPrefabPrefix::decode_prefix(116_020, &flags, 0, 0, &[0]).is_err());
        assert!(BuildingPrefabPrefix::decode_prefix(116_020, &flags, 0, 0, &[]).is_err());
        assert!(flag_bytes_used < flag_bytes.len());
        Ok(())
    }

    #[test]
    fn building_prefab_game_fixture_preserves_names_and_boundary() -> Result<(), CrpError> {
        let flag_bytes = include_bytes!("../tests/fixtures/building-flags-v116020-desert.bin");
        let (flags, flags_used) = BuildingFlagsPrefix::decode_prefix(116_020, flag_bytes)?;
        assert_eq!(&flag_bytes[flags_used..], &[0, 0, 0, 0]);
        let bytes = include_bytes!("../tests/fixtures/building-prefabs-v116020-desert.bin");
        let (prefabs, used) = BuildingPrefabPrefix::decode_prefix(116_020, &flags, 0, 0, bytes)?;
        assert_eq!(used, 3_042);
        assert_eq!(prefabs.building_indices().len(), 189);
        assert_eq!(&prefabs.building_indices()[..4], &[498, 488, 2, 497]);
        assert_eq!(prefabs.names().len(), 2_194);
        assert_eq!(
            prefabs.names().iter().filter(|name| name.is_some()).count(),
            26
        );
        assert_eq!(prefabs.names()[2].as_deref(), Some("Wildlife Spawn Point"));
        assert_eq!(prefabs.encode(), bytes[..used]);
        assert_eq!(used, bytes.len());
        assert!(
            BuildingPrefabPrefix::decode_prefix(116_020, &flags, 0, 0, &bytes[..used - 1]).is_err()
        );
        Ok(())
    }

    #[test]
    fn building_object_headers_follow_versioned_gates() -> Result<(), CrpError> {
        let bytes = include_bytes!("../tests/fixtures/building-prefabs-v116020-desert.bin");
        let flags_bytes = include_bytes!("../tests/fixtures/building-flags-v116020-desert.bin");
        let (flags, _) = BuildingFlagsPrefix::decode_prefix(116_020, flags_bytes)?;
        let (prefabs, _) = BuildingPrefabPrefix::decode_prefix(116_020, &flags, 0, 0, bytes)?;
        let mut encoded = vec![0xff; 61];
        encoded.extend_from_slice(&[0; 32]);
        let (headers, used) =
            BuildingManagerObjectHeadersPrefix::decode_prefix(116_020, &prefabs, &encoded)?;
        assert_eq!(headers.object_headers().len(), 61);
        assert!(
            headers
                .object_headers()
                .iter()
                .all(|header| *header == NetObjectHeader::Null)
        );
        assert_eq!(headers.trailing_problem_fields(), &[0; 32]);
        assert_eq!(headers.encode(), encoded);
        assert_eq!(used, encoded.len());
        assert!(
            BuildingManagerObjectHeadersPrefix::decode_prefix(
                116_020,
                &prefabs,
                &encoded[..used - 1]
            )
            .is_err()
        );
        assert!(
            BuildingManagerObjectHeadersPrefix::decode_prefix(164, &prefabs, &[0xff; 8]).is_err()
        );
        Ok(())
    }

    #[test]
    fn building_object_headers_game_fixture_decodes_following_problem_array() -> Result<(), CrpError>
    {
        let flags_bytes = include_bytes!("../tests/fixtures/building-flags-v116020-desert.bin");
        let (flags, _) = BuildingFlagsPrefix::decode_prefix(116_020, flags_bytes)?;
        let prefab_bytes = include_bytes!("../tests/fixtures/building-prefabs-v116020-desert.bin");
        let (prefabs, _) =
            BuildingPrefabPrefix::decode_prefix(116_020, &flags, 0, 0, prefab_bytes)?;
        let bytes = include_bytes!("../tests/fixtures/building-object-headers-v116020-desert.bin");
        let (headers, used) =
            BuildingManagerObjectHeadersPrefix::decode_prefix(116_020, &prefabs, bytes)?;
        assert_eq!(headers.object_headers().len(), 61);
        assert_eq!(headers.trailing_problem_fields().len(), 32);
        assert!(used < bytes.len());
        assert_eq!(headers.encode(), bytes[..used]);
        let mut array_reader = crate::data_serializer::Reader::new(&bytes[used..]);
        let problems = crate::encoded_array::read_encoded_ulong_array(
            &mut array_reader,
            flags
                .building_flags()
                .iter()
                .filter(|building| building.flags != 0)
                .count()
                .checked_mul(2)
                .ok_or(CrpError::ArithmeticOverflow(
                    "BuildingManager problem value count",
                ))?,
        )?;
        assert_eq!(problems.len(), 378);
        assert_eq!(array_reader.remaining(), 0);
        let (decoded, problem_bytes) =
            BuildingProblemPrefix::decode_prefix(116_020, None, &flags, &bytes[used..])?;
        assert_eq!(&bytes[used..], &[0xff, 0xff, 0xf9]);
        assert!(
            decoded
                .problems()
                .iter()
                .all(|entry| entry.first == 0 && entry.second == 0)
        );
        assert_eq!(decoded.problems().len(), 189);
        assert_eq!(
            decoded.problems()[0],
            BuildingProblemWords {
                index: 23,
                first: 0,
                second: 0,
            }
        );
        assert_eq!(
            decoded.problems()[1],
            BuildingProblemWords {
                index: 39,
                first: 0,
                second: 0,
            }
        );
        assert_eq!(decoded.encode(), bytes[used..]);
        assert_eq!(problem_bytes, bytes.len() - used);
        Ok(())
    }

    #[test]
    fn building_problem_array_uses_save_app_gate_before_version_115000() -> Result<(), CrpError> {
        let mut flag_bytes = vec![2];
        for value in [1_u32, 0, 0] {
            flag_bytes.extend_from_slice(&value.to_be_bytes());
        }
        let primary_remaining = 49_151 - 3;
        flag_bytes.extend(std::iter::repeat_n(0xff, primary_remaining / 128));
        flag_bytes.push(0x7f + u8::try_from(primary_remaining % 128).unwrap());
        flag_bytes.extend(std::iter::repeat_n(0xff, 383));
        flag_bytes.push(0xfe);
        let (flags, used) = BuildingFlagsPrefix::decode_prefix(114_999, &flag_bytes)?;
        assert_eq!(used, flag_bytes.len());
        assert_eq!(flags.building_flags()[0].flags, 1);
        assert!(BuildingProblemPrefix::decode_prefix(114_999, None, &flags, &[]).is_err());

        let save_app_gate = 146_809_088;
        let one_ulong = [0, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
        let (modern, modern_used) =
            BuildingProblemPrefix::decode_prefix(114_999, Some(save_app_gate), &flags, &one_ulong)?;
        assert_eq!(modern_used, one_ulong.len());
        assert_eq!(modern.problems().len(), 1);
        assert_eq!(modern.problems()[0].index, 1);
        assert_eq!(modern.problems()[0].first, 0x0123_4567_89ab_cdef);
        assert_eq!(modern.problems()[0].second, 0);
        assert_eq!(modern.encode(), one_ulong);
        let mut modern_editor = modern.clone().into_editor();
        modern_editor.problems_mut()[0].first = 0xfedc_ba98_7654_3210;
        let mut expected_ulong = vec![0];
        expected_ulong.extend_from_slice(&0xfedc_ba98_7654_3210_u64.to_be_bytes());
        assert_eq!(modern_editor.encode(Some(save_app_gate))?, expected_ulong);

        let one_uint = [0, 0xc0, 0, 0, 1];
        let (legacy, legacy_used) = BuildingProblemPrefix::decode_prefix(
            114_999,
            Some(save_app_gate - 1),
            &flags,
            &one_uint,
        )?;
        assert_eq!(legacy_used, one_uint.len());
        assert_eq!(legacy.problems()[0].first, 0xc000_0000_0000_0001);
        assert_eq!(legacy.problems()[0].second, 0);
        assert_eq!(legacy.encode(), one_uint);
        let mut legacy_editor = legacy.clone().into_editor();
        legacy_editor.problems_mut()[0].first = 0xc000_0000_0000_0002;
        assert_eq!(
            legacy_editor.encode(Some(save_app_gate - 1))?,
            [0, 0xc0, 0, 0, 2]
        );
        legacy_editor.problems_mut()[0].first |= 1_u64 << 40;
        assert!(legacy_editor.encode(Some(save_app_gate - 1)).is_err());
        let mut remapped_legacy_editor = legacy.clone().into_editor();
        remapped_legacy_editor.problems_mut()[0].index = 2;
        assert!(
            remapped_legacy_editor
                .encode(Some(save_app_gate - 1))
                .is_err()
        );

        let (paired_flags, _) = BuildingFlagsPrefix::decode_prefix(115_000, &flag_bytes)?;
        let (paired, _) = BuildingProblemPrefix::decode_prefix(
            115_000,
            None,
            &paired_flags,
            &[1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2],
        )?;
        let mut paired_editor = paired.into_editor();
        paired_editor.problems_mut()[0].second = 3;
        assert_eq!(
            paired_editor.encode(None)?,
            [1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 3]
        );
        Ok(())
    }

    #[test]
    fn building_buffer_fields_follow_version_gates_and_round_trip() -> Result<(), CrpError> {
        fn ushort_literals(values: &[u16]) -> Vec<u8> {
            let mut bytes = Vec::new();
            for chunk in values.chunks(64) {
                bytes.push(u8::try_from(chunk.len() - 1).unwrap());
                for value in chunk {
                    bytes.extend_from_slice(&value.to_be_bytes());
                }
            }
            bytes
        }
        fn byte_literals(values: &[u8]) -> Vec<u8> {
            let mut bytes = Vec::new();
            for chunk in values.chunks(128) {
                bytes.push(u8::try_from(chunk.len() - 1).unwrap());
                bytes.extend_from_slice(chunk);
            }
            bytes
        }

        let flags_bytes = include_bytes!("../tests/fixtures/building-flags-v116020-desert.bin");
        let (flags, _) = BuildingFlagsPrefix::decode_prefix(116_020, flags_bytes)?;
        let count = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .count();
        assert_eq!(count, 189);
        let values = [11_u16, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23];
        let mut bytes = Vec::new();
        for first in values {
            let mut field = vec![0_u16; count];
            field[0] = first;
            bytes.extend(ushort_literals(&field));
        }
        let mut production = vec![0_u8; count];
        production[0] = 24;
        bytes.extend(byte_literals(&production));
        let mut pollution = vec![0_u8; count];
        pollution[0] = 25;
        bytes.extend(byte_literals(&pollution));

        let (decoded, used) = BuildingBufferFieldsPrefix::decode_prefix(116_020, &flags, &bytes)?;
        assert_eq!(used, bytes.len());
        assert_eq!(decoded.net_node()[0], 11);
        assert_eq!(decoded.sub_building()[0], 12);
        assert_eq!(decoded.water_source()[0], 13);
        assert_eq!(decoded.electricity_buffer()[0], 14);
        assert_eq!(decoded.water_buffer()[0], 15);
        assert_eq!(decoded.sewage_buffer()[0], 16);
        assert_eq!(decoded.heating_buffer()[0], 17);
        assert_eq!(decoded.garbage_buffer()[0], 18);
        assert_eq!(decoded.crime_buffer()[0], 19);
        assert_eq!(decoded.mail_buffer()[0], 20);
        assert_eq!(decoded.custom_buffer1()[0], 21);
        assert_eq!(decoded.custom_buffer2()[0], 22);
        assert_eq!(decoded.build_water_height()[0], 23);
        assert_eq!(decoded.production_rate()[0], 24);
        assert_eq!(decoded.water_pollution()[0], 25);
        assert_eq!(decoded.encode(), bytes);
        assert!(
            BuildingBufferFieldsPrefix::decode_prefix(116_020, &flags, &bytes[..bytes.len() - 1])
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn building_buffer_fields_include_legacy_water_source_release() -> Result<(), CrpError> {
        fn encoded_one_active_flag() -> Vec<u8> {
            let count = 32_767;
            let remaining = count - 3;
            let mut bytes = vec![2];
            for value in [1_u32, 0, 0] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend(std::iter::repeat_n(0xff, remaining / 128));
            if remaining % 128 != 0 {
                bytes.push(0x7f + u8::try_from(remaining % 128).unwrap());
            }
            bytes
        }
        fn one_ushort(value: u16) -> [u8; 3] {
            [0, (value >> 8) as u8, value as u8]
        }

        let (flags, _) = BuildingFlagsPrefix::decode_prefix(180, &encoded_one_active_flag())?;
        let mut bytes = Vec::new();
        for value in 1..=10 {
            bytes.extend_from_slice(&one_ushort(value));
        }
        bytes.extend_from_slice(&[0, 11, 0, 12]);
        let (decoded, used) = BuildingBufferFieldsPrefix::decode_prefix(180, &flags, &bytes)?;
        assert_eq!(used, bytes.len());
        assert_eq!(decoded.net_node(), &[1]);
        assert_eq!(decoded.water_source(), &[2]);
        assert_eq!(decoded.legacy_water_source_release(), &[3]);
        assert_eq!(decoded.electricity_buffer(), &[4]);
        assert_eq!(decoded.custom_buffer1(), &[9]);
        assert_eq!(decoded.custom_buffer2(), &[10]);
        assert_eq!(decoded.production_rate(), &[11]);
        assert_eq!(decoded.water_pollution(), &[12]);
        assert!(decoded.sub_building().is_empty());
        assert!(decoded.mail_buffer().is_empty());
        assert_eq!(decoded.encode(), bytes);
        Ok(())
    }

    #[test]
    fn building_buffer_fields_game_fixture_preserves_order_and_boundary() -> Result<(), CrpError> {
        let flags_bytes = include_bytes!("../tests/fixtures/building-flags-v116020-desert.bin");
        let (flags, _) = BuildingFlagsPrefix::decode_prefix(116_020, flags_bytes)?;
        let bytes = include_bytes!("../tests/fixtures/building-buffer-fields-v116020-desert.bin");
        let (decoded, used) = BuildingBufferFieldsPrefix::decode_prefix(116_020, &flags, bytes)?;
        assert_eq!(used, bytes.len());
        assert_eq!(decoded.net_node().len(), 189);
        assert_eq!(decoded.sub_building().len(), 189);
        assert_eq!(decoded.water_source().len(), 189);
        assert_eq!(decoded.heating_buffer().len(), 189);
        assert_eq!(decoded.build_water_height().len(), 189);
        assert_eq!(decoded.production_rate().len(), 189);
        assert_eq!(decoded.water_pollution().len(), 189);
        assert_eq!(decoded.encode(), bytes);
        assert!(
            BuildingBufferFieldsPrefix::decode_prefix(116_020, &flags, &bytes[..used - 1]).is_err()
        );
        Ok(())
    }

    #[test]
    fn building_byte_state_fields_follow_version_gates() -> Result<(), CrpError> {
        fn encoded_literals(values: &[u8]) -> Vec<u8> {
            let mut bytes = Vec::new();
            for chunk in values.chunks(128) {
                bytes.push(u8::try_from(chunk.len() - 1).unwrap());
                bytes.extend_from_slice(chunk);
            }
            bytes
        }
        let flags_bytes = include_bytes!("../tests/fixtures/building-flags-v116020-desert.bin");
        let (flags, _) = BuildingFlagsPrefix::decode_prefix(116_020, flags_bytes)?;
        let count = flags
            .building_flags()
            .iter()
            .filter(|building| building.flags != 0)
            .count();
        let values = [1_u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
        let mut bytes = Vec::new();
        for first in values {
            let mut field = vec![0_u8; count];
            field[0] = first;
            bytes.extend(encoded_literals(&field));
        }
        let (decoded, used) = BuildingByteStatePrefix::decode_prefix(116_020, &flags, &bytes)?;
        assert_eq!(used, bytes.len());
        assert_eq!(decoded.construct_state()[0], 1);
        assert_eq!(decoded.fire_intensity()[0], 2);
        assert_eq!(decoded.fire_damage()[0], 3);
        assert_eq!(decoded.production_state()[0], 4);
        assert_eq!(decoded.health()[0], 5);
        assert_eq!(decoded.child_health()[0], 6);
        assert_eq!(decoded.senior_health()[0], 7);
        assert_eq!(decoded.happiness()[0], 8);
        assert_eq!(decoded.citizen_count()[0], 9);
        assert_eq!(decoded.temp_import()[0], 10);
        assert_eq!(decoded.temp_export()[0], 11);
        assert_eq!(decoded.final_import()[0], 12);
        assert_eq!(decoded.final_export()[0], 13);
        assert_eq!(decoded.encode(), bytes);
        assert!(
            BuildingByteStatePrefix::decode_prefix(116_020, &flags, &bytes[..bytes.len() - 1])
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn building_byte_state_game_fixture_preserves_order_and_boundary() -> Result<(), CrpError> {
        let flags_bytes = include_bytes!("../tests/fixtures/building-flags-v116020-desert.bin");
        let (flags, _) = BuildingFlagsPrefix::decode_prefix(116_020, flags_bytes)?;
        let bytes = include_bytes!("../tests/fixtures/building-byte-state-v116020-desert.bin");
        let (decoded, used) = BuildingByteStatePrefix::decode_prefix(116_020, &flags, bytes)?;
        assert_eq!(used, bytes.len());
        assert_eq!(decoded.construct_state().len(), 189);
        assert_eq!(decoded.fire_damage().len(), 189);
        assert_eq!(decoded.child_health().len(), 189);
        assert_eq!(decoded.final_export().len(), 189);
        assert_eq!(decoded.encode(), bytes);
        assert!(
            BuildingByteStatePrefix::decode_prefix(116_020, &flags, &bytes[..used - 1]).is_err()
        );
        Ok(())
    }

    #[test]
    fn building_timer_boundary_matches_asanu_game_fixture() -> Result<(), CrpError> {
        // This fixture begins immediately after the thirteen byte-state arrays
        // in the version-114007 BuildingManager body. The DLL reads all eleven
        // timer fields through one EncodedArray.Byte instance, field by field.
        let mut flags_bytes = Vec::new();
        let mut flags = vec![0_u32; 49_151];
        flags[..554].fill(1);
        for chunk in flags.chunks(64) {
            flags_bytes.push(u8::try_from(chunk.len() - 1).map_err(|_| {
                CrpError::ArithmeticOverflow("test BuildingManager flag block length")
            })?);
            for value in chunk {
                flags_bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
        flags_bytes.extend(std::iter::repeat_n(0xff, 383));
        flags_bytes.push(0xfe);
        flags_bytes.extend(std::iter::repeat_n(0xff, 383));
        flags_bytes.push(0xfe);
        let (flags, _) = BuildingFlagsPrefix::decode_prefix(114_007, &flags_bytes)?;
        let bytes = include_bytes!("../tests/fixtures/building-timer-geometry-v114007-asanu.bin");
        let (decoded, used) = BuildingTimerGeometryPrefix::decode_prefix(114_007, &flags, bytes)?;
        assert_eq!(used, bytes.len());
        assert_eq!(decoded.active_building_count(), 554);
        assert_eq!(decoded.timer_fields().len(), 11);
        assert!(
            decoded
                .timer(BuildingTimerField::Electricity)
                .is_some_and(|values| values.iter().all(|value| *value == 0))
        );
        assert!(
            decoded
                .timer(BuildingTimerField::Heating)
                .is_some_and(|values| values.iter().all(|value| *value == 0))
        );
        assert!(decoded.timer(BuildingTimerField::Tax).is_some());
        assert!(decoded.timer(BuildingTimerField::Major).is_some());
        assert_eq!(decoded.level_up_progress().len(), 554);
        assert!(decoded.level_up_progress().iter().all(|value| *value == 0));
        assert_eq!(&decoded.width()[..10], &[6, 2, 7, 8, 8, 8, 6, 2, 16, 2]);
        assert!(decoded.width().iter().all(|value| (1..=16).contains(value)));
        assert_eq!(&decoded.length()[..10], &[6, 6, 7, 8, 8, 8, 6, 1, 8, 1]);
        assert!(
            decoded
                .length()
                .iter()
                .all(|value| (1..=16).contains(value))
        );
        assert_eq!(decoded.encode(), bytes);
        assert!(
            BuildingTimerGeometryPrefix::decode_prefix(114_007, &flags, &bytes[..used - 1])
                .is_err()
        );
        assert!(BuildingTimerGeometryPrefix::decode_prefix(114_008, &flags, bytes).is_err());

        let mut editor = decoded.into_editor();
        editor.width_mut()[0] = 15;
        let major = editor
            .timer_mut(BuildingTimerField::Major)
            .ok_or(CrpError::InvalidAssetCount)?;
        major[0] = 1;
        let encoded = editor.encode()?;
        let (rewritten, rewritten_used) =
            BuildingTimerGeometryPrefix::decode_prefix(114_007, &flags, &encoded)?;
        assert_eq!(rewritten_used, encoded.len());
        assert_eq!(rewritten.width()[0], 15);
        assert_eq!(
            rewritten.timer(BuildingTimerField::Major).unwrap_or(&[])[0],
            1
        );
        Ok(())
    }

    #[test]
    fn net_node_problems_respect_version_and_app_gates() -> Result<(), CrpError> {
        fn one_active_node_flags(version: u32) -> Result<NetNodeFlagsPrefix, CrpError> {
            let count = if version < 202 {
                16_383
            } else if version >= 115_018 {
                65_534
            } else {
                32_767
            };
            let mut bytes = vec![2];
            for value in [1_u32, 0, 0] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            let remaining = count - 3;
            bytes.extend(std::iter::repeat_n(0xff, remaining / 128));
            if remaining % 128 != 0 {
                bytes.push(0x7f + (remaining % 128) as u8);
            }
            let (flags, used) = NetNodeFlagsPrefix::decode_prefix(version, &bytes)?;
            assert_eq!(used, bytes.len());
            Ok(flags)
        }

        let modern_flags = one_active_node_flags(115_000)?;
        let mut two_values = vec![1];
        two_values.extend_from_slice(&5_u64.to_be_bytes());
        two_values.extend_from_slice(&7_u64.to_be_bytes());
        let (modern, used) =
            NetNodeProblemsPrefix::decode_prefix(115_000, None, &modern_flags, &two_values)?;
        assert_eq!(used, two_values.len());
        assert_eq!(
            modern.problems(),
            &[NetNodeProblem {
                index: 1,
                problems1: 5,
                problems2: 7
            }]
        );
        assert_eq!(modern.encode(), two_values);

        let legacy_flags = one_active_node_flags(221)?;
        let mut one_ulong = vec![0];
        one_ulong.extend_from_slice(&9_u64.to_be_bytes());
        let (legacy_ulong, used) = NetNodeProblemsPrefix::decode_prefix(
            221,
            Some(146_809_088),
            &legacy_flags,
            &one_ulong,
        )?;
        assert_eq!(used, one_ulong.len());
        assert_eq!(legacy_ulong.problems()[0].problems1, 9);
        let one_uint = [0, 0xc0, 0, 0, 1];
        let (legacy_uint, used) =
            NetNodeProblemsPrefix::decode_prefix(221, Some(146_809_087), &legacy_flags, &one_uint)?;
        assert_eq!(used, one_uint.len());
        assert_eq!(legacy_uint.problems()[0].problems1, 0xc000_0000_0000_0001);
        assert!(NetNodeProblemsPrefix::decode_prefix(221, None, &legacy_flags, &one_uint).is_err());
        assert!(
            NetNodeProblemsPrefix::decode_prefix(
                221,
                Some(146_809_088),
                &legacy_flags,
                &one_ulong[..one_ulong.len() - 1]
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn net_node_problem_fixtures_cover_game_versions() -> Result<(), CrpError> {
        let (modern_flags, _) = NetNodeFlagsPrefix::decode_prefix(
            116_020,
            include_bytes!("../tests/fixtures/net-node-flags-v116020-desert.bin"),
        )?;
        let modern_fixture =
            include_bytes!("../tests/fixtures/net-node-problems-v116020-desert.bin");
        let (modern, modern_len) = NetNodeProblemsPrefix::decode_prefix(
            116_020,
            Some(201_499_120),
            &modern_flags,
            modern_fixture,
        )?;
        assert_eq!(modern_len, 33);
        assert_eq!(modern.problems().len(), 2_082);
        assert!(
            modern
                .problems()
                .iter()
                .all(|problem| problem.problems1 == 0 && problem.problems2 == 0)
        );
        assert_eq!(modern.encode(), modern_fixture);

        let (wolf_flags, _) = NetNodeFlagsPrefix::decode_prefix(
            112_004,
            include_bytes!("../tests/fixtures/net-node-flags-v112004-wolf-creek.bin"),
        )?;
        let wolf_fixture =
            include_bytes!("../tests/fixtures/net-node-problems-v112004-wolf-creek.bin");
        let (wolf, wolf_len) = NetNodeProblemsPrefix::decode_prefix(
            112_004,
            Some(184_558_064),
            &wolf_flags,
            wolf_fixture,
        )?;
        assert_eq!(wolf_len, 39);
        assert_eq!(wolf.problems().len(), 1_551);
        assert_eq!(
            wolf.problems()
                .iter()
                .filter(|problem| problem.problems1 != 0)
                .count(),
            2
        );
        assert!(
            wolf.problems()
                .iter()
                .any(|problem| problem.index == 15_865 && problem.problems1 == 4_096)
        );
        assert!(
            wolf.problems()
                .iter()
                .any(|problem| problem.index == 27_884 && problem.problems1 == 4_096)
        );
        assert_eq!(wolf.encode(), wolf_fixture);
        Ok(())
    }

    #[test]
    fn net_node_flags_decode_versioned_arrays() -> Result<(), CrpError> {
        for (version, full_controls, last_control, expected_len) in [
            (194, 127, 0xfe, 128),
            (202, 255, 0xfe, 256),
            (115_018, 511, 0xfd, 512),
        ] {
            let mut bytes = vec![0xff; full_controls];
            bytes.push(last_control);
            let (nodes, used) = NetNodeFlagsPrefix::decode_prefix(version, &bytes)?;
            assert_eq!(used, expected_len);
            assert_eq!(nodes.node_flags().len(), 32_767);
            assert_eq!(nodes.node_flags()[0].index, 1);
            assert_eq!(nodes.node_flags()[32_766].index, 32_767);
            assert_eq!(nodes.encode(), bytes);
            assert!(NetNodeFlagsPrefix::decode_prefix(version, &bytes[..bytes.len() - 1]).is_err());
        }
        let mut bytes = vec![3];
        for value in [5_u32, 2, 0, 0] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.extend(std::iter::repeat_n(0xff, 511));
        bytes.push(0xf9);
        let (nodes, used) = NetNodeFlagsPrefix::decode_prefix(115_018, &bytes)?;
        assert_eq!(used, bytes.len());
        assert_eq!(
            nodes.node_flags()[0],
            NetNodeFlag {
                index: 1,
                flags: 5,
                flags2: 2
            }
        );
        assert_eq!(
            nodes.node_flags()[1],
            NetNodeFlag {
                index: 2,
                flags: 0,
                flags2: 0
            }
        );
        Ok(())
    }

    #[test]
    fn net_node_flags_game_fixture_preserves_real_values() -> Result<(), CrpError> {
        let fixture = include_bytes!("../tests/fixtures/net-node-flags-v116020-desert.bin");
        let (nodes, used) = NetNodeFlagsPrefix::decode_prefix(116_020, fixture)?;
        assert_eq!(used, 27_596);
        assert_eq!(nodes.node_flags().len(), 32_767);
        assert_eq!(
            nodes
                .node_flags()
                .iter()
                .filter(|node| node.flags != 0 || node.flags2 != 0)
                .count(),
            2_082
        );
        assert_eq!(
            nodes.node_flags()[10],
            NetNodeFlag {
                index: 11,
                flags: 20_988_193,
                flags2: 0,
            }
        );
        assert_eq!(nodes.encode(), fixture);
        assert!(NetNodeFlagsPrefix::decode_prefix(116_020, &fixture[..fixture.len() - 1]).is_err());
        Ok(())
    }

    #[test]
    fn net_lane_prefix_reads_flags_and_links() -> Result<(), CrpError> {
        let mut bytes = vec![2, 0, 1, 0, 0, 0, 0];
        bytes.extend(std::iter::repeat_n(0xff, 2047));
        bytes.push(0xfb);
        bytes.extend_from_slice(&[0, 0, 5]);
        bytes.extend_from_slice(&[0, 11, 0, 12, 0, 13, 0, 0, 14]);
        let (lanes, consumed) = NetLanePrefix::decode_prefix(116_020, &bytes)?;
        assert_eq!(consumed, bytes.len());
        assert_eq!(lanes.lane_flags().len(), 262_143);
        assert_eq!(lanes.lane_flags()[0], 1);
        assert_eq!(
            lanes.active_lanes(),
            &[NetLane {
                index: 1,
                flags: 1,
                next_lane: 5,
                reserved_prev: 11,
                reserved_next: 12,
                reserved_frame: 13,
                last_reserve_id: 14,
            }]
        );
        assert_eq!(lanes.encode(), bytes);
        let (legacy, used) = NetLanePrefix::decode_prefix(205, &bytes[..bytes.len() - 3])?;
        assert_eq!(used, bytes.len() - 3);
        assert_eq!(legacy.active_lanes()[0].last_reserve_id, 0);
        assert!(NetLanePrefix::decode_prefix(116_020, &bytes[..bytes.len() - 1]).is_err());
        assert!(NetLanePrefix::decode_prefix(116_020, &[0, 0]).is_err());
        Ok(())
    }

    #[test]
    fn net_lane_prefix_game_fixture_preserves_real_links() -> Result<(), CrpError> {
        let mut fixture =
            include_bytes!("../tests/fixtures/net-lane-prefix-v116020-desert.bin").to_vec();
        fixture.extend_from_slice(include_bytes!(
            "../tests/fixtures/net-lane-reservations-v116020-desert.bin"
        ));
        let (lanes, consumed) = NetLanePrefix::decode_prefix(116_020, &fixture)?;
        assert_eq!(consumed, 62_131);
        assert_eq!(lanes.lane_flags().len(), 262_143);
        assert_eq!(lanes.active_lanes().len(), 7_867);
        assert_eq!(
            lanes.active_lanes()[0],
            NetLane {
                index: 45,
                flags: 1,
                next_lane: 109_741,
                reserved_prev: 0,
                reserved_next: 0,
                reserved_frame: 0,
                last_reserve_id: 0,
            }
        );
        assert_eq!(lanes.encode(), fixture);
        assert!(NetLanePrefix::decode_prefix(116_020, &fixture[..fixture.len() - 1]).is_err());
        Ok(())
    }

    #[test]
    fn path_manager_fixture_and_active_unit_vector() -> Result<(), CrpError> {
        let fixture = include_bytes!("../tests/fixtures/path-manager-body-v116020-desert.bin");
        let (path, used) = PathManagerData::decode_prefix(116_020, fixture)?;
        assert_eq!(used, 2048);
        assert_eq!(path.simulation_flags().len(), 262_143);
        assert!(path.simulation_flags().iter().all(|flag| *flag == 0));
        assert!(path.active_units().is_empty());
        assert_eq!(path.encode(), fixture);

        let mut body = vec![2, 1, 0, 0];
        body.extend(std::iter::repeat_n(0xff, 2047));
        body.push(0xfb);
        body.extend_from_slice(&[
            2, 3, 0, 0, 0, 4, 1, 0, 0, 5, 0, 0, 0, 6, 0x3f, 0xc0, 0, 0, 100, 0, 7, 8, 9,
        ]);
        let (active, used) = PathManagerData::decode_prefix(116_020, &body)?;
        assert_eq!(used, body.len());
        assert_eq!(active.simulation_flags()[0], 1);
        assert_eq!(active.active_units().len(), 1);
        let unit = &active.active_units()[0];
        assert_eq!(unit.index, 1);
        assert_eq!(unit.vehicle_types, 4);
        assert_eq!(unit.next_path_unit, 5);
        assert_eq!(unit.build_index, 6);
        assert_eq!(unit.length, 1.5);
        assert_eq!(unit.speed, 100);
        assert_eq!(
            unit.positions,
            [PathPosition {
                segment: 7,
                offset: 8,
                lane: 9
            }]
        );
        assert_eq!(active.encode(), body);
        let flag_prefix = &body[..2052];
        for (version, vehicle_bytes, speed_byte, expected_vehicle, expected_speed) in [
            (194, &[0x12][..], None, 0x12, 100),
            (311, &[0x12, 0x34][..], None, 0x1234, 100),
            (110_026, &[0x12, 0x34][..], Some(77), 0x1234, 77),
            (111_000, &[0x12, 0x34][..], None, 0x1234, 100),
            (111_012, &[0x12, 0x34][..], Some(77), 0x1234, 77),
            (
                113_001,
                &[0x12, 0x34, 0x56, 0x78][..],
                Some(77),
                0x1234_5678,
                77,
            ),
        ] {
            let mut bytes = flag_prefix.to_vec();
            bytes.extend_from_slice(&[2, 3]);
            bytes.extend_from_slice(vehicle_bytes);
            bytes.extend_from_slice(&[0, 0, 0, 5, 0, 0, 0, 6, 0x3f, 0xc0, 0, 0]);
            if let Some(speed) = speed_byte {
                bytes.push(speed);
            }
            let (decoded, used) = PathManagerData::decode_prefix(version, &bytes)?;
            assert_eq!(used, bytes.len(), "version {version}");
            assert_eq!(decoded.active_units()[0].vehicle_types, expected_vehicle);
            assert_eq!(decoded.active_units()[0].speed, expected_speed);
            assert_eq!(decoded.encode(), bytes);
        }
        let mut too_many_positions = body.clone();
        too_many_positions[2058] = 13;
        assert!(PathManagerData::decode_prefix(116_020, &too_many_positions).is_err());
        for cut in [0, 1, 2045, 2046, 2047] {
            assert!(PathManagerData::decode_prefix(116_020, &fixture[..cut]).is_err());
        }
        assert!(PathManagerData::decode_prefix(116_020, &body[..body.len() - 1]).is_err());
        assert!(PathManagerData::decode_prefix(121_011, fixture).is_err());
        Ok(())
    }

    #[test]
    fn instance_manager_fixture_and_version_gate() -> Result<(), CrpError> {
        let fixture = include_bytes!("../tests/fixtures/instance-manager-body-v116020-desert.bin");
        let (instance, used) = InstanceManagerData::decode_prefix(116_020, fixture)?;
        assert_eq!(used, 8);
        assert!(instance.names().is_empty());
        assert_eq!(instance.group_count(), Some(0));
        assert_eq!(instance.encode(), fixture);
        let (legacy, used) = InstanceManagerData::decode_prefix(194, &fixture[..4])?;
        assert_eq!(used, 4);
        assert_eq!(legacy.group_count(), None);
        assert_eq!(legacy.encode(), &fixture[..4]);

        // Independent DataSerializer byte vector: ID, compact "A", null name,
        // one group ID with a null object reference.
        let expected = [
            0, 0, 0, 2, 1, 2, 3, 4, 0x80, 0, 1, 0xc1, 0, 0, 0, 5, 0xff, 0, 0, 0, 1, 0x0a, 0x0b,
            0x0c, 0x0d, 0xff,
        ];
        let (instance, used) = InstanceManagerData::decode_prefix(258, &expected)?;
        assert_eq!(used, expected.len());
        assert_eq!(instance.names()[0].id, 0x0102_0304);
        assert_eq!(instance.names()[0].name.as_deref(), Some("A"));
        assert_eq!(instance.names()[1].name, None);
        assert_eq!(instance.group_count(), Some(1));
        assert_eq!(
            instance.group_references(),
            &[vec![0x0a, 0x0b, 0x0c, 0x0d, 0xff]]
        );
        assert_eq!(instance.encode(), expected);
        assert_eq!(instance.into_editor().encode()?, expected);
        let (legacy_names, legacy_used) = InstanceManagerData::decode_prefix(194, &expected[..17])?;
        assert_eq!(legacy_used, 17);
        let mut editor = legacy_names.into_editor();
        assert_eq!(editor.encode()?, expected[..17]);
        for cut in 0..expected.len() {
            assert!(InstanceManagerData::decode_prefix(258, &expected[..cut]).is_err());
        }
        let duplicate_names = [0, 0, 0, 2, 0, 0, 0, 7, 0xff, 0, 0, 0, 7, 0xff];
        assert!(InstanceManagerData::decode_prefix(194, &duplicate_names).is_err());
        editor.names_mut().push(InstanceName {
            id: 0x0102_0304,
            name: None,
        });
        assert!(editor.encode().is_err());
        let bad_count = [0xff, 0xff, 0xff, 0xff];
        assert!(InstanceManagerData::decode_prefix(258, &bad_count).is_err());
        let bad_utf16 = [0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 0xd8, 0];
        assert!(InstanceManagerData::decode_prefix(194, &bad_utf16).is_err());
        Ok(())
    }

    #[test]
    fn game_area_manager_fixture_decodes_fields_and_preserves_body() -> Result<(), CrpError> {
        let bytes = include_bytes!("../tests/fixtures/game-area-manager-body-v112004.bin");
        let (area, consumed) = GameAreaManagerData::decode_prefix(112_004, bytes)?;
        assert_eq!(consumed, bytes.len());
        assert_eq!(area.area_count(), 0);
        assert_eq!(area.start_tile(), 12);
        assert_eq!(area.area_grid(), &[0; 25]);
        assert_eq!(&area.area_not_unlocked_reference()[..2], &[0xfe, 0xfe]);
        assert_eq!(
            area.buildable_area().map(f32::to_bits),
            [0x3f68_2598, 0x3f49_56fb, 0x3f2c_7bfa, 0x3f1d_1504]
        );
        assert_eq!(area.saved_camera_view().mode, -1);
        assert_eq!(area.encode(), bytes);
        Ok(())
    }

    #[test]
    fn game_area_manager_rejects_truncated_or_overlong_grid_encoding() {
        assert!(GameAreaManagerData::decode_prefix(199, &[0, 0, 0]).is_err());
        let mut malformed = vec![0, 0, 0x19];
        malformed.extend([0; 25]);
        assert!(GameAreaManagerData::decode_prefix(199, &malformed).is_err());
    }

    #[test]
    fn game_area_manager_applies_legacy_version_gates() -> Result<(), CrpError> {
        let mut bytes = vec![7, 0x18];
        bytes.extend([0; 25]);
        let (area, consumed) = GameAreaManagerData::decode_prefix(86, &bytes)?;
        assert_eq!(consumed, bytes.len());
        assert_eq!(area.area_count(), 7);
        assert_eq!(area.start_tile(), 12);
        assert!(area.area_not_unlocked_reference().is_empty());
        assert_eq!(area.buildable_area(), [-1.0; 4]);
        assert_eq!(area.saved_camera_view().mode, -1);
        Ok(())
    }

    #[test]
    fn simulation_manager_rejects_invalid_manager_type_reference() -> Result<(), CrpError> {
        let mut root_data = vec![0xfe];
        root_data.extend_from_slice(&[0x80, 0, 17]);
        let mut previous = 128_u8;
        for byte in b"SimulationManager" {
            root_data.push(byte.wrapping_sub(previous));
            previous = *byte;
        }
        root_data.extend_from_slice(&[0; 12]); // tick, frame, build
        root_data.extend_from_slice(&[0; 8]); // randomizer seed
        root_data.extend_from_slice(&[0; 4]); // day-time offset
        root_data.extend_from_slice(&[0; 128]); // 32 history entries
        root_data.extend_from_slice(&[0, 0, 1, 0xfe, 0xff, 0xff]); // invalid shared type ID
        let stream = DataSerializerStream::from_parts(
            212,
            "SimulationManager+Data, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null".to_owned(),
            root_data,
        )?;
        assert!(SimulationManagerData::decode(&stream.encode()).is_err());
        Ok(())
    }

    #[test]
    fn compact_writer_round_trips_metadata_and_unknown_assets() -> Result<(), CrpError> {
        for version in MIN_SUPPORTED_VERSION..=MAX_SUPPORTED_VERSION {
            let mut package = sample_package();
            package.format_version = version;
            let mut encoded = Vec::new();
            package.write_compact_to(&mut encoded)?;

            let mut cursor = Cursor::new(encoded);
            let decoded = Package::read_from(&mut cursor)?;

            assert_eq!(decoded.format_version, package.format_version);
            assert_eq!(decoded.name, package.name);
            assert_eq!(decoded.author_field, package.author_field);
            assert_eq!(decoded.package_version, package.package_version);
            assert_eq!(decoded.main_asset_name, package.main_asset_name);
            assert_eq!(decoded.assets.len(), 2);
            assert_eq!(decoded.assets[0].relative_offset, 0);
            assert_eq!(decoded.assets[0].data, b"first payload");
            assert_eq!(
                decoded.assets[0].checksum,
                "c027f85a8c79a1c936c3fd3119b1e0da"
            );
            assert_eq!(decoded.assets[1].asset_type, 712);
            assert_eq!(decoded.assets[1].data, [0, 1, 2, 255]);
            assert_eq!(decoded.assets[1].relative_offset, 13);
        }
        Ok(())
    }

    #[test]
    fn writer_emits_expected_little_endian_index_layout() -> Result<(), CrpError> {
        let mut encoded = Vec::new();
        sample_package().write_compact_to(&mut encoded)?;

        assert_eq!(&encoded[..4], b"CRAP");
        assert_eq!(&encoded[4..6], &[6, 0]);
        assert_eq!(&encoded[33..41], &168_u64.to_le_bytes());
        assert_eq!(&encoded[41..49], b"\x07preview");
        assert_eq!(&encoded[49..82], b"\x20c027f85a8c79a1c936c3fd3119b1e0da");
        assert_eq!(&encoded[82..86], &3_i32.to_le_bytes());
        assert_eq!(&encoded[86..94], &0_i64.to_le_bytes());
        assert_eq!(&encoded[94..102], &13_i64.to_le_bytes());
        assert_eq!(&encoded[102..115], b"\x0cfuture asset");
        assert_eq!(&encoded[115..148], b"\x200416dab819887333af831f8c765ac2ae");
        assert_eq!(&encoded[148..152], &712_i32.to_le_bytes());
        assert_eq!(&encoded[152..160], &13_i64.to_le_bytes());
        assert_eq!(&encoded[160..168], &4_i64.to_le_bytes());
        assert_eq!(&encoded[168..], b"first payload\x00\x01\x02\xff");
        Ok(())
    }

    #[test]
    fn reader_rejects_asset_ranges_past_end_of_file() -> Result<(), CrpError> {
        let mut encoded = Vec::new();
        sample_package().write_compact_to(&mut encoded)?;

        let mut cursor = Cursor::new(encoded.as_slice());
        cursor.seek(SeekFrom::Start(6))?;
        read_binary_string(&mut cursor)?;
        read_binary_string(&mut cursor)?;
        read_u32_le(&mut cursor)?;
        read_binary_string(&mut cursor)?;
        read_i32_le(&mut cursor)?;
        read_u64_le(&mut cursor)?;
        read_binary_string(&mut cursor)?;
        read_binary_string(&mut cursor)?;
        read_i32_le(&mut cursor)?;
        read_i64_le(&mut cursor)?;
        let size_offset = usize::try_from(cursor.position())
            .map_err(|_| CrpError::ArithmeticOverflow("test cursor position"))?;
        encoded[size_offset..size_offset + 8].copy_from_slice(&i64::MAX.to_le_bytes());

        let mut cursor = Cursor::new(encoded.as_slice());
        assert!(matches!(
            Package::read_from(&mut cursor),
            Err(CrpError::AssetOutOfBounds { .. })
        ));

        let mut cursor = Cursor::new(encoded);
        assert!(matches!(
            Package::read_assets_of_type_from(&mut cursor, 712),
            Err(CrpError::AssetOutOfBounds { name, .. }) if name == "preview"
        ));
        Ok(())
    }

    #[test]
    fn filtered_reader_returns_matching_assets_in_index_order() -> Result<(), CrpError> {
        let mut package = sample_package();
        package.assets[1].asset_type = 3;
        package.assets.push(Asset {
            name: "later matching asset".to_owned(),
            checksum: String::new(),
            asset_type: 3,
            relative_offset: 0,
            data: b"third payload".to_vec(),
        });
        package.assets.push(Asset {
            name: "unmatched asset".to_owned(),
            checksum: String::new(),
            asset_type: 712,
            relative_offset: 0,
            data: b"fourth payload".to_vec(),
        });
        let mut encoded = Vec::new();
        package.write_compact_to(&mut encoded)?;

        let mut cursor = Cursor::new(encoded);
        let assets = Package::read_assets_of_type_from(&mut cursor, 3)?;

        assert_eq!(assets.len(), 3);
        assert_eq!(assets[0].name, "preview");
        assert_eq!(assets[0].data, b"first payload");
        assert_eq!(assets[1].name, "future asset");
        assert_eq!(assets[1].data, [0, 1, 2, 255]);
        assert_eq!(assets[2].name, "later matching asset");
        assert_eq!(assets[2].data, b"third payload");
        Ok(())
    }

    #[test]
    fn filtered_reader_skips_unmatched_payload_checksums() -> Result<(), CrpError> {
        let mut encoded = Vec::new();
        sample_package().write_compact_to(&mut encoded)?;

        let mut valid_cursor = Cursor::new(encoded.as_slice());
        let decoded = Package::read_from(&mut valid_cursor)?;
        let unmatched_offset =
            usize::try_from(decoded.data_start_offset + decoded.assets[0].relative_offset)
                .map_err(|_| CrpError::ArithmeticOverflow("test payload offset"))?;
        let selected_offset =
            usize::try_from(decoded.data_start_offset + decoded.assets[1].relative_offset)
                .map_err(|_| CrpError::ArithmeticOverflow("test payload offset"))?;

        encoded[unmatched_offset] ^= 0xff;
        let mut full_cursor = Cursor::new(encoded.as_slice());
        assert!(matches!(
            Package::read_from(&mut full_cursor),
            Err(CrpError::ChecksumMismatch { name, .. }) if name == "preview"
        ));

        let mut filtered_cursor = RejectReadAtOffset {
            cursor: Cursor::new(encoded.clone()),
            rejected_offset: u64::try_from(unmatched_offset)
                .map_err(|_| CrpError::ArithmeticOverflow("test payload offset"))?,
        };
        let assets = Package::read_assets_of_type_from(&mut filtered_cursor, 712)?;
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].name, "future asset");
        assert_eq!(assets[0].data, [0, 1, 2, 255]);

        encoded[selected_offset] ^= 0xff;
        let mut selected_corrupt_cursor = Cursor::new(encoded);
        assert!(matches!(
            Package::read_assets_of_type_from(&mut selected_corrupt_cursor, 712),
            Err(CrpError::ChecksumMismatch { name, .. }) if name == "future asset"
        ));
        Ok(())
    }

    #[test]
    fn nested_package_reader_is_bounded_to_the_asset_payload() -> Result<(), CrpError> {
        let mut child_bytes = Vec::new();
        sample_package().write_compact_to(&mut child_bytes)?;
        let expected_child = Package::read_from(&mut Cursor::new(child_bytes.as_slice()))?;

        let mut parent = sample_package();
        parent.assets = vec![Asset {
            name: "nested package".to_owned(),
            checksum: String::new(),
            asset_type: ASSET_TYPE_PACKAGE,
            relative_offset: 0,
            data: child_bytes.clone(),
        }];
        let mut parent_bytes = Vec::new();
        parent.write_compact_to(&mut parent_bytes)?;
        let decoded_parent = Package::read_from(&mut Cursor::new(parent_bytes))?;
        assert_eq!(
            decoded_parent.assets[0].read_nested_package()?,
            expected_child
        );

        let mut wrong_type = sample_package();
        assert!(matches!(
            wrong_type.assets.remove(0).read_nested_package(),
            Err(CrpError::AssetTypeMismatch {
                expected: ASSET_TYPE_PACKAGE,
                actual: 3,
                ..
            })
        ));

        child_bytes.pop();
        let mut truncated_parent = sample_package();
        truncated_parent.assets = vec![
            Asset {
                name: "truncated nested package".to_owned(),
                checksum: String::new(),
                asset_type: ASSET_TYPE_PACKAGE,
                relative_offset: 0,
                data: child_bytes,
            },
            Asset {
                name: "following parent payload".to_owned(),
                checksum: String::new(),
                asset_type: 3,
                relative_offset: 0,
                data: b"outside the nested asset".to_vec(),
            },
        ];
        let mut truncated_parent_bytes = Vec::new();
        truncated_parent.write_compact_to(&mut truncated_parent_bytes)?;
        let decoded_parent = Package::read_from(&mut Cursor::new(truncated_parent_bytes))?;
        assert!(matches!(
            decoded_parent.assets[0].read_nested_package(),
            Err(CrpError::AssetOutOfBounds { name, .. }) if name == "future asset"
        ));
        Ok(())
    }

    #[test]
    fn reader_rejects_payload_checksum_mismatch() -> Result<(), CrpError> {
        let mut encoded = Vec::new();
        sample_package().write_compact_to(&mut encoded)?;
        encoded[50] = b'0';

        let mut cursor = Cursor::new(encoded);
        assert!(matches!(
            Package::read_from(&mut cursor),
            Err(CrpError::ChecksumMismatch { name, .. }) if name == "preview"
        ));
        Ok(())
    }

    #[test]
    fn reader_rejects_truncated_input_and_invalid_strings() -> Result<(), CrpError> {
        let mut invalid_magic = Cursor::new(b"NOPE\0\0".to_vec());
        assert!(matches!(
            Package::read_from(&mut invalid_magic),
            Err(CrpError::InvalidMagic(_))
        ));

        let mut short_header = Cursor::new(&b"CRAP"[..]);
        assert!(matches!(
            Package::read_from(&mut short_header),
            Err(CrpError::Io(_))
        ));

        let mut invalid_utf8 = Cursor::new([1, 0xff]);
        assert!(matches!(
            read_binary_string(&mut invalid_utf8),
            Err(CrpError::InvalidUtf8)
        ));

        let mut invalid_length = Cursor::new([0x80; 5]);
        assert!(matches!(
            read_7bit_encoded_u32(&mut invalid_length),
            Err(CrpError::InvalidStringLength)
        ));

        let mut truncated_payload = Vec::new();
        sample_package().write_compact_to(&mut truncated_payload)?;
        truncated_payload.pop();
        let mut cursor = Cursor::new(truncated_payload);
        assert!(matches!(
            Package::read_from(&mut cursor),
            Err(CrpError::AssetOutOfBounds { name, .. }) if name == "future asset"
        ));
        Ok(())
    }

    #[test]
    fn reader_rejects_negative_counts_and_asset_ranges() -> Result<(), CrpError> {
        let mut negative_count = Vec::new();
        sample_package().write_compact_to(&mut negative_count)?;
        negative_count[29..33].copy_from_slice(&(-1_i32).to_le_bytes());
        let mut cursor = Cursor::new(negative_count);
        assert!(matches!(
            Package::read_from(&mut cursor),
            Err(CrpError::NegativeAssetCount(-1))
        ));

        let mut negative_offset = Vec::new();
        sample_package().write_compact_to(&mut negative_offset)?;
        negative_offset[86..94].copy_from_slice(&(-1_i64).to_le_bytes());
        let mut cursor = Cursor::new(negative_offset);
        assert!(matches!(
            Package::read_from(&mut cursor),
            Err(CrpError::InvalidAssetRange { name, .. }) if name == "preview"
        ));

        let mut negative_size = Vec::new();
        sample_package().write_compact_to(&mut negative_size)?;
        negative_size[94..102].copy_from_slice(&(-1_i64).to_le_bytes());
        let mut cursor = Cursor::new(negative_size);
        assert!(matches!(
            Package::read_from(&mut cursor),
            Err(CrpError::InvalidAssetRange { name, .. }) if name == "preview"
        ));
        Ok(())
    }

    #[test]
    fn reader_rejects_invalid_data_start_and_offset_overflow() -> Result<(), CrpError> {
        let mut before_header = Vec::new();
        sample_package().write_compact_to(&mut before_header)?;
        before_header[33..41].copy_from_slice(&0_u64.to_le_bytes());
        let mut cursor = Cursor::new(before_header);
        assert!(matches!(
            Package::read_from(&mut cursor),
            Err(CrpError::IndexPastDataStart { .. })
        ));

        let mut encoded = Vec::new();
        sample_package().write_compact_to(&mut encoded)?;
        let mut cursor = Cursor::new(encoded);
        assert!(matches!(
            Package::read_from_with_data_offset(&mut cursor, u64::MAX),
            Err(CrpError::ArithmeticOverflow("asset payload offset"))
        ));
        Ok(())
    }

    #[test]
    fn writer_handles_empty_payloads() -> Result<(), CrpError> {
        let mut package = sample_package();
        package.assets.truncate(1);
        package.assets[0].data.clear();
        let mut encoded = Vec::new();
        package.write_compact_to(&mut encoded)?;

        let mut cursor = Cursor::new(encoded);
        let decoded = Package::read_from(&mut cursor)?;
        assert!(decoded.assets[0].data.is_empty());
        assert_eq!(
            decoded.assets[0].checksum,
            "d41d8cd98f00b204e9800998ecf8427e"
        );
        Ok(())
    }

    #[test]
    fn committed_map_fixtures_cover_versions_4_and_10() -> Result<(), CrpError> {
        let fixtures = [
            (
                "metadata-v4.crp",
                4,
                "ND Island Hopping",
                "3b51f60cc678c6d4a9608cab2c885c9a",
            ),
            (
                "metadata-v10.crp",
                10,
                "MM Shifting Sands",
                "ea1f16657409f0c1349488925597d5d3",
            ),
        ];
        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

        for (filename, expected_version, expected_name, expected_checksum) in fixtures {
            let path = fixture_dir.join(filename);
            let mut source = File::open(path)?;
            let original = Package::read_from(&mut source)?;
            assert_eq!(original.format_version, expected_version);
            assert_eq!(original.name, expected_name);
            assert_eq!(original.main_asset_name, expected_name);
            assert_eq!(original.assets.len(), 1);
            assert_eq!(original.assets[0].asset_type, 100);
            assert_eq!(original.assets[0].checksum, expected_checksum);
            assert_eq!(checksum_hex(&original.assets[0].data), expected_checksum);

            let mut encoded = Vec::new();
            original.write_compact_to(&mut encoded)?;
            let mut cursor = Cursor::new(encoded);
            let rewritten = Package::read_from(&mut cursor)?;
            assert_eq!(rewritten.format_version, original.format_version);
            assert_eq!(rewritten.name, original.name);
            assert_eq!(rewritten.main_asset_name, original.main_asset_name);
            assert_eq!(rewritten.assets.len(), original.assets.len());
            assert_eq!(rewritten.assets[0].name, original.assets[0].name);
            assert_eq!(
                rewritten.assets[0].asset_type,
                original.assets[0].asset_type
            );
            assert_eq!(rewritten.assets[0].data, original.assets[0].data);
            assert_eq!(rewritten.assets[0].checksum, expected_checksum);
        }
        Ok(())
    }

    #[test]
    fn map_metadata_fixtures_decode_and_reencode_exactly() -> Result<(), CrpError> {
        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let cases = [
            (
                "metadata-v4.crp",
                "ND Island Hopping",
                "The Archipelago",
                Some(("UnlimitedMoney", u64::MAX)),
                1153,
            ),
            (
                "metadata-v10.crp",
                "MM Shifting Sands",
                "Shifting Sands",
                None,
                0,
            ),
        ];

        for (filename, object_name, map_name, expected_mod, trailing_len) in cases {
            let mut file = File::open(fixture_dir.join(filename))?;
            let package = Package::read_from(&mut file)?;
            let asset = &package.assets[0];
            let metadata = MapMetaData::decode(asset)?;

            assert_eq!(metadata.object_name, object_name);
            assert_eq!(metadata.builtin, Some(true));
            assert_eq!(metadata.environment.as_deref(), Some("Tropical"));
            assert_eq!(metadata.map_theme_ref, None);
            assert_eq!(metadata.map_name.as_deref(), Some(map_name));
            assert_eq!(
                metadata.mods.as_ref().map(Vec::len),
                Some(usize::from(expected_mod.is_some()))
            );
            if let Some((mod_name, file_id)) = expected_mod {
                let mods = metadata
                    .mods
                    .as_ref()
                    .ok_or_else(|| CrpError::InvalidAssetPayload {
                        name: asset.name.clone(),
                        offset: 0,
                        reason: "expected mod array was absent".to_owned(),
                    })?;
                assert_eq!(mods[0].name, mod_name);
                assert_eq!(mods[0].published_file_id, file_id);
            }
            assert_eq!(metadata.trailing_bytes.len(), trailing_len);
            assert_eq!(metadata.encode()?, asset.data);

            if filename == "metadata-v10.crp" {
                let mut edited = metadata.clone();
                edited.map_name = Some("Edited Map Name".to_owned());
                let mut edited_asset = asset.clone();
                edited_asset.data = edited.encode()?;
                let decoded_edit = MapMetaData::decode(&edited_asset)?;
                assert_eq!(decoded_edit.map_name.as_deref(), Some("Edited Map Name"));
            }
        }
        Ok(())
    }

    #[test]
    fn system_map_and_district_style_metadata_fixtures_round_trip() -> Result<(), CrpError> {
        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let system_map_asset = Asset {
            name: "BuiltinTerrainMap-Sunny".to_owned(),
            checksum: String::new(),
            asset_type: 102,
            relative_offset: 0,
            data: fs::read(fixture_dir.join("system-map-metadata-v10.bin"))?,
        };
        let system_map = SystemMapMetaData::decode(&system_map_asset)?;
        assert_eq!(system_map.outer_asset_type, 102);
        assert_eq!(system_map.environment.as_deref(), Some("Sunny"));
        assert_eq!(
            system_map.time_stamp.as_deref(),
            Some("2015-02-03T00:44:52.8767263+02:00")
        );
        assert_eq!(
            system_map.image_ref.as_deref(),
            Some("e1dc891bfb96977abd25e9c044dbddb8")
        );
        assert_eq!(
            system_map.asset_ref.as_deref(),
            Some("868304b7299cdbc6288371ba19f5da86")
        );
        assert_eq!(system_map.encode()?, system_map_asset.data);

        let district_style_asset = Asset {
            name: "EuropeanBuiltinStyle_q3oji4g".to_owned(),
            checksum: String::new(),
            asset_type: 105,
            relative_offset: 0,
            data: fs::read(fixture_dir.join("district-style-metadata-v10.bin"))?,
        };
        let district_style = DistrictStyleMetaData::decode(&district_style_asset)?;
        assert_eq!(district_style.builtin, Some(true));
        assert_eq!(
            district_style.name.as_deref(),
            Some("EuropeanBuiltinStyle_q3oji4g")
        );
        assert_eq!(district_style.steam_tags, None);
        assert_eq!(district_style.assets, Some(Vec::new()));
        assert_eq!(
            district_style.time_stamp.as_deref(),
            Some("2015-09-11T12:08:50.6724948+03:00")
        );
        assert_eq!(
            district_style.image_ref.as_deref(),
            Some("2281027cb20e062aa739f92e1fdbab04")
        );
        assert_eq!(
            district_style.steam_preview_ref.as_deref(),
            Some("337d62b997b9d7b9f25c6747f473f9c0")
        );
        assert_eq!(district_style.asset_ref, None);
        assert_eq!(district_style.encode()?, district_style_asset.data);

        let mut edited_style = district_style;
        edited_style.assets = Some(vec!["asset-one".to_owned(), "asset-two".to_owned()]);
        let mut edited_asset = district_style_asset;
        edited_asset.data = edited_style.encode()?;
        let decoded_edit = DistrictStyleMetaData::decode(&edited_asset)?;
        assert_eq!(
            decoded_edit.assets,
            Some(vec!["asset-one".to_owned(), "asset-two".to_owned()])
        );
        Ok(())
    }

    #[test]
    fn type_107_system_map_metadata_preserves_outer_type_and_zero_suffix() -> Result<(), CrpError> {
        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let asset = Asset {
            name: "BuiltinTerrainMap-1aa5g6dfs748sf".to_owned(),
            checksum: String::new(),
            asset_type: 107,
            relative_offset: 0,
            data: fs::read(fixture_dir.join("system-map-theme-map-type107-v10.bin"))?,
        };
        let metadata = SystemMapMetaData::decode(&asset)?;
        assert_eq!(metadata.outer_asset_type, 107);
        assert_eq!(metadata.environment.as_deref(), Some("Europe"));
        assert_eq!(metadata.image_ref, None);
        assert_eq!(
            metadata.asset_ref.as_deref(),
            Some("93276827714a6175062b6645471e2680")
        );
        assert_eq!(metadata.trailing_bytes.len(), 489);
        assert!(metadata.trailing_bytes.iter().all(|byte| *byte == 0));
        assert_eq!(metadata.encode()?, asset.data);
        Ok(())
    }

    #[test]
    fn map_metadata_map_theme_reference_uses_string_wire_type() -> Result<(), CrpError> {
        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut file = File::open(fixture_dir.join("metadata-v10.crp"))?;
        let package = Package::read_from(&mut file)?;
        let asset = &package.assets[0];
        let mut metadata = MapMetaData::decode(asset)?;
        metadata.map_theme_ref = Some("theme-reference".to_owned());

        let encoded = metadata.encode()?;
        let mut reader = crate::asset_io::PayloadReader::new(&asset.name, &encoded);
        assert!(!reader.read_bool()?);
        assert_eq!(reader.read_string()?, metadata.runtime_type);
        assert_eq!(reader.read_string()?, metadata.object_name);
        assert_eq!(reader.read_i32_le()?, 25);

        for (index, expected_field) in ["builtin", "environment", "mapThemeRef"]
            .into_iter()
            .enumerate()
        {
            assert!(!reader.read_bool()?);
            let field_type = reader.read_string()?;
            let field_name = reader.read_string()?;
            assert_eq!(field_name, expected_field);
            if index == 0 {
                assert_eq!(field_type.split(',').next(), Some("System.Boolean"));
                let _builtin = reader.read_bool()?;
            } else {
                assert_eq!(field_type.split(',').next(), Some("System.String"));
                let value = reader.read_string()?;
                if index == 2 {
                    assert_eq!(value, "theme-reference");
                }
            }
        }

        let mut edited_asset = asset.clone();
        edited_asset.data = encoded;
        let decoded = MapMetaData::decode(&edited_asset)?;
        assert_eq!(decoded.map_theme_ref.as_deref(), Some("theme-reference"));
        Ok(())
    }

    #[test]
    fn metadata_decoder_rejects_truncated_field_data() -> Result<(), CrpError> {
        let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut file = File::open(fixture_dir.join("metadata-v10.crp"))?;
        let package = Package::read_from(&mut file)?;
        let mut asset = package.assets[0].clone();
        asset.data.pop();

        assert!(matches!(
            MapMetaData::decode(&asset),
            Err(CrpError::InvalidAssetPayload { .. })
        ));
        Ok(())
    }

    #[test]
    fn texture_fixture_decodes_and_round_trips_across_version_gate() -> Result<(), CrpError> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/texture-v6.crp");
        let mut file = File::open(path)?;
        let package = Package::read_from(&mut file)?;
        let asset = &package.assets[0];
        let texture = TextureAsset::decode(asset, package.format_version)?;

        assert_eq!(texture.name, "Wolf Creek_Snapshot");
        assert!(!texture.linear);
        assert_eq!(texture.aniso_level, 1);
        assert!(texture.image_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(texture.encode(6)?, asset.data);

        let mut edited = texture.clone();
        edited.name = "Edited Texture".to_owned();
        edited.linear = true;
        edited.aniso_level = 4;
        let mut edited_asset = asset.clone();
        edited_asset.data = edited.encode(6)?;
        let decoded_edit = TextureAsset::decode(&edited_asset, 6)?;
        assert_eq!(decoded_edit.name, "Edited Texture");
        assert!(decoded_edit.linear);
        assert_eq!(decoded_edit.aniso_level, 4);

        let legacy_payload = texture.encode(5)?;
        let mut legacy_asset = asset.clone();
        legacy_asset.data = legacy_payload;
        let legacy_texture = TextureAsset::decode(&legacy_asset, 5)?;
        assert_eq!(legacy_texture.aniso_level, 1);
        assert_eq!(legacy_texture.image_bytes, texture.image_bytes);
        assert_eq!(legacy_texture.encode(5)?, legacy_asset.data);
        Ok(())
    }

    #[test]
    fn texture_decoder_rejects_incorrect_image_length() -> Result<(), CrpError> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/texture-v6.crp");
        let mut file = File::open(path)?;
        let package = Package::read_from(&mut file)?;
        let original = &package.assets[0];
        let texture = TextureAsset::decode(original, package.format_version)?;
        let mut malformed = original.clone();
        malformed.data = texture.encode(package.format_version)?;
        let length_offset = malformed.data.len() - texture.image_bytes.len() - 4;
        malformed.data[length_offset..length_offset + 4].copy_from_slice(&i32::MAX.to_le_bytes());

        assert!(matches!(
            TextureAsset::decode(&malformed, package.format_version),
            Err(CrpError::InvalidAssetPayload { .. })
        ));
        Ok(())
    }

    fn district_records_match_version(record: &DistrictPopulationRecord, version: u32) -> bool {
        let mut expected = Vec::new();
        if version >= 92 {
            let mut production_bytes = 86;
            for (gate, added_bytes) in [
                (215, 12),
                (216, 8),
                (219, 4),
                (226, 16),
                (228, 6),
                (240, 8),
                (307, 28),
                (109_010, 12),
                (112_006, 6),
                (116_008, 24),
                (116_011, 90),
            ] {
                if version >= gate {
                    production_bytes += added_bytes;
                }
            }
            expected.push((DistrictStatisticsKind::Production, production_bytes));
            let consumption_bytes = 72 + if version >= 234 { 6 } else { 0 };
            expected.extend(
                [
                    DistrictStatisticsKind::PlayerConsumption,
                    DistrictStatisticsKind::ResidentialConsumption,
                    DistrictStatisticsKind::CommercialConsumption,
                    DistrictStatisticsKind::IndustrialConsumption,
                    DistrictStatisticsKind::OfficeConsumption,
                ]
                .map(|kind| (kind, consumption_bytes)),
            );
        }
        if version >= 239 {
            expected.push((DistrictStatisticsKind::Usage, 24));
        }
        if version >= 97 {
            expected.push((DistrictStatisticsKind::Ground, 15));
        }
        if version >= 118 {
            let resource_bytes = 60
                + if version >= 111_014 { 12 } else { 0 }
                + if version >= 113_013 { 12 } else { 0 };
            expected.push((DistrictStatisticsKind::ImportResources, resource_bytes));
            expected.push((DistrictStatisticsKind::ExportResources, resource_bytes));
        }
        let statistics_match = record.statistics().len() == expected.len()
            && record
                .statistics()
                .iter()
                .zip(expected)
                .all(|(actual, (kind, byte_count))| {
                    actual.kind() == kind && actual.as_bytes().len() == byte_count
                });
        let tourist_size =
            6 + if version >= 122 { 3 } else { 0 } + if version >= 117_011 { 6 } else { 0 };
        let expected_tourism_count = usize::from(version >= 119) * 3;
        let tourism_match = record.tourism().len() == expected_tourism_count
            && record.tourism().iter().enumerate().all(|(index, block)| {
                block.kind()
                    == [
                        DistrictTourismKind::Tourist1,
                        DistrictTourismKind::Tourist2,
                        DistrictTourismKind::Tourist3,
                    ][index]
                    && block.as_bytes().len() == tourist_size
            });
        let subculture_kinds = [
            DistrictSubCultureKind::Hippie,
            DistrictSubCultureKind::Hipster,
            DistrictSubCultureKind::Redneck,
            DistrictSubCultureKind::Gangsta,
        ];
        let expected_subculture_count = usize::from(version >= 207) * 4;
        let subcultures_match = record.subcultures().len() == expected_subculture_count
            && record
                .subcultures()
                .iter()
                .zip(subculture_kinds)
                .all(|(block, kind)| block.kind() == kind && block.as_bytes().len() == 11);
        let expected_hotel_size =
            12 + if version >= 117_007 { 4 } else { 0 } + if version >= 117_008 { 9 } else { 0 };
        let hotel_present = (117_004..117_009).contains(&version);
        let hotel_match = record.hotel_data().map(|hotel| hotel.as_bytes().len())
            == hotel_present.then_some(expected_hotel_size);
        let style_match = record.district_style_present().is_some() == (version >= 217)
            && record
                .district_style_name()
                .is_none_or(|_| record.district_style_present() == Some(true));
        let library_match = record.library_visitor_data().is_some() == (version >= 112_006)
            && record
                .library_visitor_data()
                .is_none_or(|age| age.category() == DistrictAgeCategory::LibraryVisitors);
        let airline_match = record.airline().is_some() == (version >= 114_003);
        statistics_match
            && tourism_match
            && subcultures_match
            && hotel_match
            && style_match
            && library_match
            && airline_match
    }

    #[test]
    #[ignore = "set CRP_CORPUS_DIR and run explicitly to check local DistrictManager grids"]
    fn validates_optional_district_grid_corpus() -> Result<(), Box<dyn std::error::Error>> {
        let maps_dir = std::env::var_os("CRP_CORPUS_DIR")
            .map(PathBuf::from)
            .ok_or_else(|| std::io::Error::other("CRP_CORPUS_DIR is not set"))?;
        let mut paths = fs::read_dir(maps_dir)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.retain(|path| path.extension().is_some_and(|extension| extension == "crp"));
        paths.sort();
        assert!(!paths.is_empty(), "external corpus contains no .crp files");
        let mut manager_assets = 0_usize;
        let mut active_parks = 0_usize;
        let mut empty_park_assets = 0_usize;
        let mut matching_tails = 0_usize;
        let mut differing_tails = Vec::new();
        for path in &paths {
            let package = Package::read_from(&mut File::open(path)?)?;
            for asset in package.assets.iter().filter(|asset| asset.asset_type == 52) {
                let (_, metadata_bytes) = SimulationMetaData::decode_from(&asset.data)?;
                let manager = SimulationManagerData::decode(&asset.data[metadata_bytes..])?;
                let (prefix, park_start) = manager.decode_district_manager_flags_prefix()?;
                let graph = manager.serialized_object_graph();
                let (records, park_bytes) = prefix.decode_park_records(&graph[park_start..])?;
                let flagged = prefix
                    .park_flags()
                    .iter()
                    .filter(|flag| **flag != 0)
                    .count();
                assert_eq!(records.len(), flagged, "{}", path.display());
                active_parks += flagged;
                empty_park_assets += usize::from(flagged == 0);
                let (grids, grid_end) = manager.decode_district_manager_grids()?;
                let grid_start = park_start + park_bytes;
                assert_eq!(grid_end - grid_start, grids.encode().len());
                assert_eq!(&graph[grid_start..grid_end], grids.encode());
                assert!(grid_end < graph.len(), "{}", path.display());
                match manager.decode_district_manager_tail() {
                    Ok((tail, tail_end)) => {
                        assert!(tail_end > grid_end && tail_end <= graph.len());
                        assert_eq!(&graph[grid_end..tail_end], tail.encode());
                        matching_tails += 1;
                    }
                    Err(error) => {
                        let leading_nulls = graph[grid_end..]
                            .iter()
                            .take_while(|byte| **byte == 0xff)
                            .count();
                        differing_tails.push((
                            path.display().to_string(),
                            manager.format_version(),
                            leading_nulls,
                            error.to_string(),
                        ));
                    }
                }
                manager_assets += 1;
            }
        }
        assert!(manager_assets > 0);
        eprintln!(
            "DistrictManager corpus: {} maps, {} manager assets, {} active parks, {} empty-park assets, {} matching typed tails, {} differing typed tails",
            paths.len(),
            manager_assets,
            active_parks,
            empty_park_assets,
            matching_tails,
            differing_tails.len()
        );
        for (path, version, nulls, error) in differing_tails.iter().take(10) {
            eprintln!(
                "tail difference: {path}, version {version}, {nulls} initial FF bytes: {error}"
            );
        }
        Ok(())
    }

    #[test]
    #[ignore = "set CRP_CORPUS_DIR and run explicitly to check reflected metadata assets"]
    fn validates_optional_reflected_metadata_corpus() -> Result<(), Box<dyn std::error::Error>> {
        let maps_dir = std::env::var_os("CRP_CORPUS_DIR")
            .map(PathBuf::from)
            .ok_or_else(|| std::io::Error::other("CRP_CORPUS_DIR is not set"))?;
        let mut paths = Vec::new();
        for entry in fs::read_dir(maps_dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|extension| extension == "crp") {
                paths.push(path);
            }
        }
        paths.sort();
        assert!(!paths.is_empty(), "external corpus contains no .crp files");

        let mut map_metadata_count = 0_usize;
        let mut system_map_metadata_count = 0_usize;
        let mut district_style_metadata_count = 0_usize;
        for path in paths {
            let mut file = File::open(&path)?;
            let package = Package::read_from(&mut file)?;
            for asset in &package.assets {
                match asset.asset_type {
                    100 => {
                        let metadata = MapMetaData::decode(asset)?;
                        assert_eq!(metadata.encode()?, asset.data);
                        map_metadata_count += 1;
                    }
                    102 | 107 => {
                        let metadata = SystemMapMetaData::decode(asset)?;
                        assert_eq!(metadata.outer_asset_type, asset.asset_type);
                        assert_eq!(metadata.encode()?, asset.data);
                        system_map_metadata_count += 1;
                    }
                    105 => {
                        let metadata = DistrictStyleMetaData::decode(asset)?;
                        assert_eq!(metadata.encode()?, asset.data);
                        district_style_metadata_count += 1;
                    }
                    _ => {}
                }
            }
        }

        assert!(
            map_metadata_count > 0,
            "corpus contains no type-100 metadata"
        );
        assert!(
            system_map_metadata_count > 0,
            "corpus contains no type-102 or type-107 metadata"
        );
        assert!(
            district_style_metadata_count > 0,
            "corpus contains no type-105 metadata"
        );
        Ok(())
    }

    #[test]
    #[ignore = "reads the local CL Wolf Creek game map from data/maps"]
    fn validates_wolf_creek_statistic_child_queue() -> Result<(), Box<dyn std::error::Error>> {
        const STATISTIC_ARRAY_TYPE_INDEX: u16 = 38;
        const STATISTIC_INT64_TYPE_INDEX: u16 = 39;
        const STATISTIC_INT32_TYPE_INDEX: u16 = 40;
        const CHILDREN_START: usize = 4_202_413;
        const CHILDREN_END: usize = 4_205_015;
        const NESTED_CHILDREN_START: usize = 4_205_019;
        const NESTED_CHILDREN_END: usize = 4_211_695;

        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/maps/CL Wolf Creek.crp");
        let mut file = File::open(path)?;
        let package = Package::read_from(&mut file)?;
        let asset = package
            .assets
            .iter()
            .find(|asset| asset.asset_type == 52)
            .ok_or_else(|| std::io::Error::other("Wolf Creek has no type-52 asset"))?;
        let (_, metadata_len) = SimulationMetaData::decode_from(&asset.data)?;
        let manager = SimulationManagerData::decode(&asset.data[metadata_len..])?;
        assert_eq!(manager.format_version(), 112_004);

        let references_fixture =
            include_bytes!("../tests/fixtures/statistic-array-references-v112004-wolf-creek.bin");
        let int64_fixture =
            include_bytes!("../tests/fixtures/statistic-int64-v112004-wolf-creek.bin");
        let int32_fixture =
            include_bytes!("../tests/fixtures/statistic-int32-v112004-wolf-creek.bin");
        let nested_array_fixture =
            include_bytes!("../tests/fixtures/statistic-array-nested-v112004-wolf-creek.bin");
        let (references, _) = StatisticArrayReferences::decode(references_fixture)?;
        let graph = manager.serialized_object_graph();
        let mut offset = CHILDREN_START;
        let mut nested_references = Vec::new();

        for (index, header) in references.references().iter().enumerate() {
            let bytes = graph
                .get(offset..)
                .ok_or_else(|| std::io::Error::other("statistic body offset exceeds graph"))?;
            let consumed = match header {
                NetObjectHeader::NewSharedType(STATISTIC_ARRAY_TYPE_INDEX) => {
                    let (array, consumed) = StatisticArrayReferences::decode(bytes)?;
                    nested_references.extend_from_slice(array.references());
                    consumed
                }
                NetObjectHeader::NewSharedType(STATISTIC_INT64_TYPE_INDEX) => {
                    StatisticInt64Data::decode(manager.format_version(), bytes)?.1
                }
                NetObjectHeader::NewType(type_name) if type_name.starts_with("StatisticInt64,") => {
                    StatisticInt64Data::decode(manager.format_version(), bytes)?.1
                }
                NetObjectHeader::NewSharedType(STATISTIC_INT32_TYPE_INDEX) => {
                    StatisticInt32Data::decode(manager.format_version(), bytes)?.1
                }
                NetObjectHeader::NewType(type_name) if type_name.starts_with("StatisticInt32,") => {
                    StatisticInt32Data::decode(manager.format_version(), bytes)?.1
                }
                other => {
                    return Err(std::io::Error::other(format!(
                        "unexpected statistic child reference {index}: {other:?}"
                    ))
                    .into());
                }
            };
            let body = &graph[offset..offset + consumed];
            match index {
                3 => assert_eq!(body, int64_fixture),
                4 => assert_eq!(body, int32_fixture),
                14 => assert_eq!(body, nested_array_fixture),
                _ => {}
            }
            offset += consumed;
        }

        assert_eq!(references.references().len(), 60);
        assert_eq!(offset, CHILDREN_END);
        assert_eq!(nested_references.len(), 159);

        let mut offset = NESTED_CHILDREN_START;
        let mut int64_count = 0;
        let mut int32_count = 0;
        for (index, header) in nested_references.iter().enumerate() {
            let bytes = graph.get(offset..).ok_or_else(|| {
                std::io::Error::other("nested statistic body offset exceeds graph")
            })?;
            let consumed = match header {
                NetObjectHeader::NewSharedType(STATISTIC_INT64_TYPE_INDEX) => {
                    int64_count += 1;
                    StatisticInt64Data::decode(manager.format_version(), bytes)?.1
                }
                NetObjectHeader::NewSharedType(STATISTIC_INT32_TYPE_INDEX) => {
                    int32_count += 1;
                    StatisticInt32Data::decode(manager.format_version(), bytes)?.1
                }
                other => {
                    return Err(std::io::Error::other(format!(
                        "unexpected nested statistic reference {index}: {other:?}"
                    ))
                    .into());
                }
            };
            offset += consumed;
        }

        assert_eq!(int64_count, 44);
        assert_eq!(int32_count, 115);
        assert_eq!(offset, NESTED_CHILDREN_END);
        let trailing_bytes = graph
            .get(offset..)
            .ok_or_else(|| std::io::Error::other("nested statistic run exceeds graph"))?;
        assert_eq!(trailing_bytes.len(), 60);
        Ok(())
    }

    #[test]
    #[ignore = "reads the local data/maps corpus"]
    fn validates_statistics_and_guide_manager_records_in_local_map_corpus()
    -> Result<(), Box<dyn std::error::Error>> {
        let maps_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/maps");
        let mut paths = fs::read_dir(maps_dir)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.retain(|path| path.extension().is_some_and(|extension| extension == "crp"));
        paths.sort();

        let mut manager_record_count = 0;
        for path in paths {
            let mut file = File::open(&path)?;
            let package = Package::read_from(&mut file)?;
            for asset in package.assets.iter().filter(|asset| asset.asset_type == 52) {
                let (_, metadata_len) = SimulationMetaData::decode_from(&asset.data)?;
                let manager = SimulationManagerData::decode(&asset.data[metadata_len..])?;
                if manager.format_version() >= 116_000 {
                    let references =
                        manager
                            .resolve_economy_manager_references(None)
                            .map_err(|error| {
                                std::io::Error::other(format!(
                                    "{} asset {:?}: {error}",
                                    path.display(),
                                    asset.name
                                ))
                            })?;
                    let expected_count = if manager.format_version() >= 116_017 {
                        5
                    } else {
                        2
                    };
                    assert_eq!(references.len(), expected_count);
                    assert!(references.iter().flatten().all(|reference| {
                        reference.type_name.starts_with("GenericGuide,")
                            || reference.type_name == "GenericGuide"
                    }));
                }
                let statistics = manager.decode_statistics_manager_data().map_err(|error| {
                    std::io::Error::other(format!(
                        "{} asset {:?}: {error}",
                        path.display(),
                        asset.name
                    ))
                })?;
                assert_eq!(statistics.format_version(), manager.format_version());
                let guides = manager.decode_guide_manager_references().map_err(|error| {
                    std::io::Error::other(format!(
                        "{} asset {:?}: {error}",
                        path.display(),
                        asset.name
                    ))
                })?;
                assert_eq!(guides.format_version(), manager.format_version());
                let unlock = manager
                    .decode_unlock_manager_references()
                    .map_err(|error| {
                        std::io::Error::other(format!(
                            "{} asset {:?}: {error}",
                            path.display(),
                            asset.name
                        ))
                    })?;
                assert_eq!(unlock.format_version(), manager.format_version());
                assert!(
                    unlock
                        .arrays()
                        .iter()
                        .all(|array| array.references().is_empty())
                );
                let expected_array_count = 14
                    + usize::from(manager.format_version() >= 248)
                    + usize::from(manager.format_version() >= 110_009);
                assert_eq!(unlock.arrays().len(), expected_array_count);
                if manager.format_version() >= 279 {
                    assert_eq!(unlock.scenario_trigger_count(), Some(0));
                    assert!(unlock.scenario_trigger_references().is_empty());
                } else {
                    assert_eq!(unlock.scenario_trigger_count(), None);
                }
                assert!(unlock.milestones_not_used().is_some());
                assert_eq!(
                    unlock.scenario_goals_not_used().is_some(),
                    manager.format_version() >= 295
                );
                let messages = manager.decode_message_manager_data().map_err(|error| {
                    std::io::Error::other(format!(
                        "{} asset {:?}: {error}",
                        path.display(),
                        asset.name
                    ))
                })?;
                assert_eq!(messages.format_version(), manager.format_version());
                assert!(
                    messages
                        .message_queue()
                        .is_none_or(|references| references.is_empty())
                );
                let recent_messages = messages
                    .recent_messages()
                    .ok_or_else(|| std::io::Error::other("recent-message array is null"))?;
                assert_eq!(recent_messages.len(), 16);
                assert!(
                    recent_messages
                        .iter()
                        .all(|reference| *reference == NetObjectHeader::Null)
                );
                assert!(
                    messages
                        .message_data()
                        .is_some_and(|entries| entries.is_empty())
                );
                let timer_count = messages
                    .random_message_timer_count()
                    .ok_or_else(|| std::io::Error::other("timer count is absent"))?;
                assert_eq!(
                    usize::try_from(timer_count)?,
                    messages.random_message_timers().len()
                );
                manager_record_count += 1;
            }
        }

        assert_eq!(manager_record_count, 110);
        Ok(())
    }

    #[test]
    #[ignore = "reads the local CL Wolf Creek game map from data/maps"]
    fn validates_wolf_creek_economy_manager_references() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/maps/CL Wolf Creek.crp");
        let mut file = File::open(&path)?;
        let package = Package::read_from(&mut file)?;
        let asset = package
            .assets
            .iter()
            .find(|asset| asset.asset_type == 52)
            .ok_or_else(|| std::io::Error::other("Wolf Creek has no type-52 asset"))?;
        let (metadata, metadata_len) = SimulationMetaData::decode_from(&asset.data)?;
        let manager = SimulationManagerData::decode(&asset.data[metadata_len..])?;
        assert_eq!(manager.format_version(), 112_004);

        let references = manager.resolve_economy_manager_references(metadata.save_app_version)?;
        let guide_type =
            "GenericGuide, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null";
        assert_eq!(
            references,
            [
                Some(ResolvedObjectReference {
                    object_id: 112,
                    type_name: guide_type.to_owned(),
                }),
                Some(ResolvedObjectReference {
                    object_id: 113,
                    type_name: guide_type.to_owned(),
                }),
            ]
        );

        let district_references =
            manager.resolve_district_manager_references(metadata.save_app_version)?;
        assert_eq!(
            district_references,
            (114..120)
                .map(|object_id| {
                    Some(ResolvedObjectReference {
                        object_id,
                        type_name: guide_type.to_owned(),
                    })
                })
                .collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    #[ignore = "reads the local NorthEurope2FINAL and CCMP2 Boreal Map 1 samples from data/maps"]
    fn validates_economy_manager_investments_and_district_references()
    -> Result<(), Box<dyn std::error::Error>> {
        let samples = [
            ("NorthEurope2FINAL.crp", 116_011, 2, 10),
            ("CCMP2 Boreal Map 1.crp", 116_019, 5, 13),
        ];
        let expected_type =
            "GenericGuide, Assembly-CSharp, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null";
        for (file_name, expected_version, expected_count, expected_active_count) in samples {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("data/maps")
                .join(file_name);
            let mut file = File::open(&path)?;
            let package = Package::read_from(&mut file)?;
            let asset = package
                .assets
                .iter()
                .find(|asset| asset.asset_type == 52)
                .ok_or_else(|| {
                    std::io::Error::other(format!("{file_name} has no type-52 asset"))
                })?;
            let (metadata, metadata_len) = SimulationMetaData::decode_from(&asset.data)?;
            let manager = SimulationManagerData::decode(&asset.data[metadata_len..])?;
            assert_eq!(manager.format_version(), expected_version);
            let references =
                manager.resolve_economy_manager_references(metadata.save_app_version)?;
            assert_eq!(references.len(), expected_count, "{file_name}");
            assert!(references.iter().all(|reference| {
                reference
                    .as_ref()
                    .is_some_and(|reference| reference.type_name == expected_type)
            }));
            let income_expenses =
                manager.decode_economy_manager_income_expense_editor(metadata.save_app_version)?;
            assert_eq!(income_expenses.income().len(), 2_380, "{file_name}");
            assert_eq!(income_expenses.total_income().map(<[i64]>::len), Some(18));
            assert_eq!(income_expenses.expenses().len(), 2_380, "{file_name}");
            assert_eq!(
                income_expenses.loan_expenses().map(<[i64]>::len),
                Some(680),
                "{file_name}"
            );
            assert_eq!(
                income_expenses.policy_expenses().map(<[i64]>::len),
                Some(680),
                "{file_name}"
            );
            assert_eq!(income_expenses.total_expenses().map(<[i64]>::len), Some(18));
            let district_references = manager
                .resolve_district_manager_references(metadata.save_app_version)
                .map_err(|error| {
                    std::io::Error::other(format!("{file_name} district references: {error}"))
                })?;
            let expected_district_count = if expected_version >= 116_016 { 19 } else { 18 };
            assert_eq!(
                district_references.len(),
                expected_district_count,
                "{file_name}"
            );
            let investment_flags =
                manager.decode_economy_manager_investment_flags(metadata.save_app_version)?;
            assert_eq!(investment_flags.len(), 254, "{file_name}");
            assert_eq!(
                investment_flags.iter().filter(|flag| **flag != 0).count(),
                expected_active_count,
                "{file_name}"
            );
            let value_series = manager
                .decode_economy_manager_investment_value_series(metadata.save_app_version)?;
            assert_eq!(value_series.len(), expected_active_count, "{file_name}");
            assert!(
                value_series
                    .iter()
                    .map(|series| series.values.len())
                    .sum::<usize>()
                    > 0,
                "{file_name}"
            );
            let investments =
                manager.decode_economy_manager_investments(metadata.save_app_version)?;
            assert_eq!(investments.len(), expected_active_count, "{file_name}");
            assert_eq!(
                investments
                    .iter()
                    .map(|investment| investment.investment_index)
                    .collect::<Vec<_>>(),
                value_series
                    .iter()
                    .map(|series| series.investment_index)
                    .collect::<Vec<_>>(),
                "{file_name}"
            );
            let prefabs =
                manager.decode_economy_manager_investment_prefabs(metadata.save_app_version)?;
            assert_eq!(prefabs.assignments.len(), investments.len(), "{file_name}");
            assert_eq!(prefabs.names.len(), expected_active_count, "{file_name}");
        }
        Ok(())
    }

    #[test]
    #[ignore = "reads the local CCMP2 Boreal Map 1 sample from data/maps"]
    fn rewrites_boreal_economy_manager_investments() -> Result<(), Box<dyn std::error::Error>> {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/maps/CCMP2 Boreal Map 1.crp");
        let mut file = File::open(path)?;
        let package = Package::read_from(&mut file)?;
        let asset = package
            .assets
            .iter()
            .find(|asset| asset.asset_type == 52)
            .ok_or_else(|| std::io::Error::other("Boreal has no type-52 asset"))?;
        let (metadata, metadata_len) = SimulationMetaData::decode_from(&asset.data)?;
        let mut manager = SimulationManagerData::decode(&asset.data[metadata_len..])?;
        assert_eq!(manager.format_version(), 116_019);

        let mut editor =
            manager.decode_economy_manager_investments_editor(metadata.save_app_version)?;
        let changed_spent = -123_456;
        editor.investments_mut()[0].player_spent = changed_spent;
        manager.replace_economy_manager_investments(&editor, metadata.save_app_version)?;
        assert_eq!(
            manager.decode_economy_manager_investments(metadata.save_app_version)?[0].player_spent,
            changed_spent
        );

        let mut budget_editor =
            manager.decode_economy_manager_budget_editor(metadata.save_app_version)?;
        let changed_tax_rate = 73;
        budget_editor.tax_rates_mut()[0] = changed_tax_rate;
        manager.replace_economy_manager_budget(&budget_editor, metadata.save_app_version)?;
        assert_eq!(
            manager
                .decode_economy_manager_budget_editor(metadata.save_app_version)?
                .tax_rates()[0],
            changed_tax_rate
        );

        let mut income_expenses =
            manager.decode_economy_manager_income_expense_editor(metadata.save_app_version)?;
        let changed_income = -987_654_321_i64;
        let changed_loan_expense = 123_456_789_i64;
        let changed_policy_expense = -456_789_123_i64;
        income_expenses.income_mut()[0] = changed_income;
        income_expenses.loan_expenses_mut().ok_or_else(|| {
            std::io::Error::other("Boreal has no version-gated loan-expense array")
        })?[0] = changed_loan_expense;
        income_expenses.policy_expenses_mut().ok_or_else(|| {
            std::io::Error::other("Boreal has no version-gated policy-expense array")
        })?[0] = changed_policy_expense;
        manager
            .replace_economy_manager_income_expense(&income_expenses, metadata.save_app_version)?;
        let decoded_income_expenses =
            manager.decode_economy_manager_income_expense_editor(metadata.save_app_version)?;
        assert_eq!(decoded_income_expenses.income()[0], changed_income);
        assert_eq!(
            decoded_income_expenses.loan_expenses().ok_or_else(|| {
                std::io::Error::other("Boreal loan-expense array disappeared after replacement")
            })?[0],
            changed_loan_expense
        );
        assert_eq!(
            decoded_income_expenses.policy_expenses().ok_or_else(|| {
                std::io::Error::other("Boreal policy-expense array disappeared after replacement")
            })?[0],
            changed_policy_expense
        );

        let mut loans_and_cash =
            manager.decode_economy_manager_loans_and_cash_editor(metadata.save_app_version)?;
        loans_and_cash.loans_mut()[0].amount_taken = -765_432;
        loans_and_cash.cash_values_mut()[0] = 0x0102_0304_0506_0708;
        *loans_and_cash.starting_money_mut().ok_or_else(|| {
            std::io::Error::other("Boreal has no version-gated starting-money field")
        })? = 7_654_321;
        *loans_and_cash
            .unmodeled_cash_value_raw_mut()
            .ok_or_else(|| std::io::Error::other("Boreal has no additional cash field"))? = -9;
        manager
            .replace_economy_manager_loans_and_cash(&loans_and_cash, metadata.save_app_version)?;
        let decoded_loans_and_cash =
            manager.decode_economy_manager_loans_and_cash_editor(metadata.save_app_version)?;
        assert_eq!(decoded_loans_and_cash.loans()[0].amount_taken, -765_432);
        assert_eq!(
            decoded_loans_and_cash.cash_values()[0],
            0x0102_0304_0506_0708
        );
        assert_eq!(decoded_loans_and_cash.starting_money(), Some(7_654_321));
        assert_eq!(decoded_loans_and_cash.unmodeled_cash_value_raw(), Some(-9));
        Ok(())
    }

    #[test]
    #[ignore = "reads the local CCMP2 Desert Map 1 game map from data/maps"]
    fn validates_desert_transport_manager_selection_suffix()
    -> Result<(), Box<dyn std::error::Error>> {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/maps/CCMP2 Desert Map 1.crp");
        let mut file = File::open(&path)?;
        let package = Package::read_from(&mut file)?;
        let asset = package
            .assets
            .iter()
            .find(|asset| asset.asset_type == 52)
            .ok_or_else(|| std::io::Error::other("Desert Map 1 has no type-52 asset"))?;
        let (metadata, metadata_len) = SimulationMetaData::decode_from(&asset.data)?;
        let manager = SimulationManagerData::decode(&asset.data[metadata_len..])?;
        assert_eq!(manager.format_version(), 116_020);

        let references = manager.resolve_transport_manager_references(metadata.save_app_version)?;
        let references = references
            .into_iter()
            .map(|reference| {
                reference.ok_or_else(|| {
                    std::io::Error::other("unexpected null TransportManager reference")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            references
                .iter()
                .map(|reference| reference.object_id)
                .collect::<Vec<_>>(),
            [128, 129, 130, 131, 132, 133, 134]
        );
        assert!(references[0].type_name.starts_with("GenericGuide,"));
        assert!(
            references[1]
                .type_name
                .starts_with("BuildingInstanceGuide,")
        );
        assert!(
            references[2]
                .type_name
                .starts_with("BuildingInstanceGuide,")
        );
        for reference in &references[3..] {
            assert!(reference.type_name.starts_with("GenericGuide,"));
        }
        let investments = manager.decode_economy_manager_investments(metadata.save_app_version)?;
        assert!(!investments.is_empty());
        let prefabs =
            manager.decode_economy_manager_investment_prefabs(metadata.save_app_version)?;
        assert_eq!(prefabs.assignments.len(), investments.len());
        Ok(())
    }

    #[test]
    #[ignore = "reads the local NorthEurope2FINAL sample from data/maps"]
    fn validates_north_europe_transport_shared_strings() -> Result<(), Box<dyn std::error::Error>> {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/maps/NorthEurope2FINAL.crp");
        let mut file = File::open(path)?;
        let package = Package::read_from(&mut file)?;
        let asset = package
            .assets
            .iter()
            .find(|asset| asset.asset_type == 52)
            .ok_or_else(|| std::io::Error::other("NorthEurope2FINAL has no type-52 asset"))?;
        let (metadata, metadata_len) = SimulationMetaData::decode_from(&asset.data)?;
        let manager = SimulationManagerData::decode(&asset.data[metadata_len..])?;
        assert_eq!(manager.format_version(), 116_011);

        let selections =
            manager.decode_transport_manager_selection_prefix(metadata.save_app_version)?;
        assert_eq!(selections.format_version(), 116_011);
        assert_eq!(
            selections.encoded_len(),
            selections.encode_unchanged().len()
        );
        assert!(selections.selections().is_empty());
        assert_eq!(selections.encode_unchanged(), [0, 0]);
        Ok(())
    }

    #[test]
    #[ignore = "reads the local CL Wolf Creek sample from data/maps"]
    fn validates_wolf_creek_info_manager_mask() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/maps/CL Wolf Creek.crp");
        let mut file = File::open(path)?;
        let package = Package::read_from(&mut file)?;
        let asset = package
            .assets
            .iter()
            .find(|asset| asset.asset_type == 52)
            .ok_or_else(|| std::io::Error::other("CL Wolf Creek has no type-52 asset"))?;
        let (_, metadata_len) = SimulationMetaData::decode_from(&asset.data)?;
        let mut manager = SimulationManagerData::decode(&asset.data[metadata_len..])?;
        assert_eq!(manager.format_version(), 112_004);

        let original_graph = manager.serialized_object_graph().to_vec();
        let mut info = manager.decode_info_manager_data()?;
        assert_eq!(info.encoded_len(), 8);
        assert_eq!(info.encode_unchanged().len(), 8);
        info.set_visited_info_views(info.visited_info_views())?;
        manager.replace_info_manager_data(&info)?;
        assert_eq!(manager.serialized_object_graph(), original_graph);
        Ok(())
    }

    #[test]
    #[ignore = "reads the local CL Wolf Creek sample from data/maps"]
    fn validates_wolf_creek_audio_manager_fields() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/maps/CL Wolf Creek.crp");
        let mut file = File::open(path)?;
        let package = Package::read_from(&mut file)?;
        let asset = package
            .assets
            .iter()
            .find(|asset| asset.asset_type == 52)
            .ok_or_else(|| std::io::Error::other("CL Wolf Creek has no type-52 asset"))?;
        let (_, metadata_len) = SimulationMetaData::decode_from(&asset.data)?;
        let manager = SimulationManagerData::decode(&asset.data[metadata_len..])?;
        assert_eq!(manager.format_version(), 112_004);

        let audio = manager.decode_audio_manager_data()?;
        assert!(audio.radio_channels().is_empty());
        assert!(audio.radio_contents().is_empty());
        assert!(audio.unlock_milestone_references().is_empty());
        assert!(audio.encoded_len() >= 20);
        Ok(())
    }

    #[test]
    #[ignore = "reads the local CL Wolf Creek game map from data/maps"]
    fn validates_wolf_creek_building_manager_guide_locator()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/maps/CL Wolf Creek.crp");
        let mut file = File::open(&path)?;
        let package = Package::read_from(&mut file)?;
        let asset = package
            .assets
            .iter()
            .find(|asset| asset.asset_type == 52)
            .ok_or_else(|| std::io::Error::other("Wolf Creek has no type-52 asset"))?;
        let (metadata, metadata_len) = SimulationMetaData::decode_from(&asset.data)?;
        let manager = SimulationManagerData::decode(&asset.data[metadata_len..])?;
        assert_eq!(manager.format_version(), 112_004);

        let water_guides = manager.locate_water_manager_guide_bodies(metadata.save_app_version)?;
        assert_eq!(water_guides.len(), 2);
        for location in water_guides {
            let location = location
                .ok_or_else(|| std::io::Error::other("Wolf Creek WaterManager guide is null"))?;
            assert_eq!(location.body_length, 11);
            assert!(location.type_name.starts_with("BuildingTypeGuide,"));
            let (_, consumed) = GuideTriggerBasePrefix::decode(
                &manager.serialized_object_graph()[location.graph_offset..],
            )?;
            assert_eq!(consumed, 11);
        }

        let zone_guides = manager.locate_zone_manager_guide_bodies(metadata.save_app_version)?;
        assert_eq!(zone_guides.len(), 13);
        let mut zone_type_guides = 0;
        let mut generic_guides = 0;
        for location in zone_guides {
            let location = location
                .ok_or_else(|| std::io::Error::other("Wolf Creek ZoneManager guide is null"))?;
            assert_eq!(location.body_length, 11, "{location:?}");
            let (guide, consumed) = GuideTriggerBasePrefix::decode(
                &manager.serialized_object_graph()[location.graph_offset..],
            )?;
            assert_eq!(consumed, 11);
            assert_eq!(
                guide.encode(),
                manager.serialized_object_graph()
                    [location.graph_offset..location.graph_offset + 11]
            );
            if location.type_name.starts_with("ZoneTypeGuide,") {
                zone_type_guides += 1;
            } else if location.type_name.starts_with("GenericGuide,") {
                generic_guides += 1;
            } else {
                return Err(std::io::Error::other(format!(
                    "unexpected Wolf Creek ZoneManager guide type: {}",
                    location.type_name
                ))
                .into());
            }
        }
        assert_eq!(zone_type_guides, 11);
        assert_eq!(generic_guides, 2);

        let transport_references =
            manager.resolve_transport_manager_references(metadata.save_app_version)?;
        assert_eq!(transport_references.len(), 7);
        assert!(transport_references.iter().all(|reference| {
            reference
                .as_ref()
                .is_none_or(|reference| !reference.type_name.is_empty())
        }));

        let message = manager.decode_message_manager_data()?;
        assert_eq!(message.random_message_timers().len(), 26);
        let original_queue = message.message_queue().map(<[_]>::to_vec);
        let original_recent = message.recent_messages().map(<[_]>::to_vec);
        let original_profile_index = message.profile_end_shared_string_index();
        let mut message_editor = message.into_editor();
        *message_editor
            .recent_message_index_mut()
            .ok_or_else(|| std::io::Error::other("Wolf Creek has no recent-message index"))? = 1;
        message_editor.random_message_timers_mut()[0] = 1;
        let expected_message_body = message_editor.encode();
        let mut rewritten_message = manager.clone();
        rewritten_message.replace_message_manager_data(&message_editor)?;
        assert_eq!(
            rewritten_message.serialized_object_graph().len(),
            manager.serialized_object_graph().len()
        );
        let rewritten_message_data = rewritten_message.decode_message_manager_data()?;
        assert_eq!(rewritten_message_data.encode(), expected_message_body);
        assert_eq!(rewritten_message_data.recent_message_index(), Some(1));
        assert_eq!(rewritten_message_data.random_message_timers()[0], 1);
        assert_eq!(rewritten_message_data.random_message_timers().len(), 26);
        assert_eq!(
            rewritten_message_data.message_queue(),
            original_queue.as_deref()
        );
        assert_eq!(
            rewritten_message_data.recent_messages(),
            original_recent.as_deref()
        );
        assert_eq!(
            rewritten_message_data.profile_end_shared_string_index(),
            original_profile_index
        );

        let statistics = manager.decode_statistics_manager_data()?;
        assert!(statistics.start_frame() <= 4_775);
        assert_ne!(statistics.statistic_array_header(), &NetObjectHeader::Null);
        assert!(statistics.encode().len() >= 6);

        let guide_references = manager.decode_guide_manager_references()?;
        assert_eq!(guide_references.references().len(), 48);
        assert_eq!(guide_references.encode().len(), 48 * 3 + 2);
        assert_eq!(
            guide_references
                .references()
                .iter()
                .filter(|reference| matches!(
                    reference.header(),
                    NetObjectHeader::NewSharedType(30)
                ))
                .count(),
            19
        );
        assert_eq!(
            guide_references
                .references()
                .iter()
                .filter(|reference| matches!(
                    reference.header(),
                    NetObjectHeader::NewSharedType(32)
                ))
                .count(),
            28
        );
        assert_eq!(
            guide_references
                .references()
                .iter()
                .filter(|reference| matches!(
                    reference.header(),
                    NetObjectHeader::NewSharedType(36)
                ))
                .count(),
            1
        );

        let guide_objects =
            manager.locate_guide_manager_reference_bodies(metadata.save_app_version)?;
        assert_eq!(guide_objects.len(), guide_references.references().len());
        let guide_objects = guide_objects
            .into_iter()
            .map(|location| {
                location.ok_or_else(|| std::io::Error::other("null GuideManager child"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(guide_objects[0].graph_offset, 4_201_881);
        assert!(
            guide_objects
                .iter()
                .all(|location| location.body_length == 11)
        );
        assert_eq!(
            guide_objects
                .last()
                .map(|location| location.graph_offset + location.body_length),
            Some(4_202_409)
        );
        const GUIDE_BODY_FIXTURE: &[u8] =
            include_bytes!("../tests/fixtures/guide-manager-bodies-v112004-wolf-creek.bin");
        assert_eq!(
            &manager.serialized_object_graph()[4_201_881..4_202_409],
            GUIDE_BODY_FIXTURE
        );

        let net_locations = manager.locate_net_manager_guide_bodies(metadata.save_app_version)?;
        let net_location = net_locations
            .first()
            .and_then(Option::as_ref)
            .cloned()
            .ok_or_else(|| std::io::Error::other("Wolf Creek NetManager guide 0 is null"))?;
        let net_class = net_location
            .type_name
            .split(',')
            .next()
            .unwrap_or_default()
            .rsplit('.')
            .next()
            .unwrap_or_default();
        let net_body = &manager.serialized_object_graph()[net_location.graph_offset..];
        let mut rewritten_net = manager.clone();
        let mut expected_net_graph = manager.serialized_object_graph().to_vec();
        match net_class {
            "NetNodeInstanceGuide" => {
                let (guide, consumed) = NetNodeInstanceGuideBody::decode(
                    manager.format_version(),
                    metadata.save_app_version,
                    net_body,
                )?;
                assert_eq!(consumed, net_location.body_length);
                let mut editor = guide.into_editor();
                *editor.activation_frame_mut() = 0x0102_0304;
                rewritten_net.replace_net_manager_node_instance_guide(
                    0,
                    metadata.save_app_version,
                    &editor,
                )?;
                expected_net_graph[net_location.graph_offset + 4..net_location.graph_offset + 8]
                    .copy_from_slice(&0x0102_0304_u32.to_be_bytes());
            }
            "NetSegmentInstanceGuide" => {
                let (guide, consumed) = GuideInstanceBody::decode(net_body)?;
                assert_eq!(consumed, net_location.body_length);
                let mut editor = guide.into_editor();
                *editor.build_index_mut() = 0x0102_0304;
                rewritten_net.replace_net_manager_guide_instance_body(
                    0,
                    metadata.save_app_version,
                    &editor,
                )?;
                expected_net_graph[net_location.graph_offset + 11..net_location.graph_offset + 15]
                    .copy_from_slice(&0x0102_0304_u32.to_be_bytes());
            }
            "GenericGuide" | "ServiceTypeGuide" | "BuildingTypeGuide" => {
                let (guide, consumed) = GuideTriggerBasePrefix::decode(net_body)?;
                assert_eq!(consumed, net_location.body_length);
                let mut editor = guide.into_editor();
                *editor.activation_frame_mut() = 0x0102_0304;
                rewritten_net.replace_net_manager_guide_trigger_base(
                    0,
                    metadata.save_app_version,
                    &editor,
                )?;
                expected_net_graph[net_location.graph_offset + 4..net_location.graph_offset + 8]
                    .copy_from_slice(&0x0102_0304_u32.to_be_bytes());
            }
            _ => {
                return Err(std::io::Error::other(format!(
                    "unsupported NetManager guide class in Wolf Creek: {}",
                    net_location.type_name
                ))
                .into());
            }
        }
        assert_eq!(rewritten_net.serialized_object_graph(), expected_net_graph);
        assert_eq!(
            rewritten_net.serialized_object_graph().len(),
            manager.serialized_object_graph().len()
        );

        let building_locations =
            manager.locate_building_manager_guide_bodies(metadata.save_app_version)?;
        let location = building_locations
            .get(4)
            .and_then(Option::as_ref)
            .cloned()
            .ok_or_else(|| std::io::Error::other("Wolf Creek BuildingManager guide 4 is null"))?;
        assert_eq!(location.graph_offset, 4_200_608, "{location:?}");
        assert_eq!(location.body_length, 17);
        assert!(location.type_name.starts_with("BuildingOnFireGuide,"));

        let (guide, consumed) =
            GuideInstanceBody::decode(&manager.serialized_object_graph()[location.graph_offset..])?;
        assert_eq!(consumed, 17);
        let mut edited = guide.into_editor();
        *edited.build_index_mut() = 0x1234_5678;
        let mut rewritten = manager.clone();
        rewritten.replace_guide_instance_body_at(location.graph_offset, &edited)?;
        assert_eq!(
            &rewritten.serialized_object_graph()
                [location.graph_offset + 11..location.graph_offset + 15],
            &0x1234_5678_u32.to_be_bytes()
        );
        assert_eq!(
            rewritten.serialized_object_graph().len(),
            manager.serialized_object_graph().len()
        );

        const BUILDING_GUIDES: &[u8] = include_bytes!(
            "../tests/fixtures/building-manager-guide-bodies-v112004-wolf-creek.bin"
        );
        let (base, consumed) = GuideTriggerBasePrefix::decode(&BUILDING_GUIDES[85..])?;
        assert_eq!(consumed, 11);
        let mut base_editor = base.into_editor();
        *base_editor.activation_frame_mut() = 0x0102_0304;
        let base_location = building_locations
            .get(5)
            .and_then(Option::as_ref)
            .cloned()
            .ok_or_else(|| std::io::Error::other("Wolf Creek BuildingManager guide 5 is null"))?;
        assert_eq!(base_location.body_length, 11, "{base_location:?}");
        assert!(
            base_location.type_name.starts_with("GenericGuide,")
                || base_location.type_name.starts_with("ServiceTypeGuide,")
                || base_location.type_name.starts_with("BuildingTypeGuide,"),
            "{base_location:?}"
        );
        let mut rewritten_base = manager.clone();
        rewritten_base.replace_building_manager_guide_trigger_base(
            5,
            metadata.save_app_version,
            &base_editor,
        )?;
        let changed_offsets = manager
            .serialized_object_graph()
            .iter()
            .zip(rewritten_base.serialized_object_graph())
            .enumerate()
            .filter_map(|(offset, (original, rewritten))| (original != rewritten).then_some(offset))
            .collect::<Vec<_>>();
        let expected_changed_offsets =
            (base_location.graph_offset + 4..base_location.graph_offset + 8).collect::<Vec<_>>();
        assert_eq!(
            changed_offsets, expected_changed_offsets,
            "{base_location:?}"
        );
        assert_eq!(
            &rewritten_base.serialized_object_graph()[changed_offsets[0]..changed_offsets[0] + 4],
            &0x0102_0304_u32.to_be_bytes()
        );
        assert_eq!(
            rewritten_base.serialized_object_graph().len(),
            manager.serialized_object_graph().len()
        );
        Ok(())
    }

    #[test]
    #[ignore = "set CRP_CORPUS_DIR and run explicitly to check a local full corpus"]
    fn validates_optional_external_map_corpus() -> Result<(), Box<dyn std::error::Error>> {
        let maps_dir = std::env::var_os("CRP_CORPUS_DIR")
            .map(PathBuf::from)
            .ok_or_else(|| std::io::Error::other("CRP_CORPUS_DIR is not set"))?;
        let mut paths = Vec::new();
        for entry in fs::read_dir(maps_dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|extension| extension == "crp") {
                paths.push(path);
            }
        }
        paths.sort();
        assert!(!paths.is_empty(), "external corpus contains no .crp files");

        let mut simulation_metadata_count = 0;
        let mut nonempty_wave_assets = 0;
        let mut active_net_lanes = 0_usize;
        let mut reserved_net_lanes = 0_usize;
        let mut active_net_nodes = 0_usize;
        let mut nodes_with_problems = 0_usize;
        let mut nodes_with_counters = 0_usize;
        let mut nodes_with_segments = 0_usize;
        let mut nodes_with_event_routes = 0_usize;
        let mut active_net_segments = 0_usize;
        let mut segments_with_secondary_flags = 0_usize;
        let mut segments_with_problems = 0_usize;
        let mut segments_with_traffic_lights = 0_usize;
        let mut segments_with_fire_coverage = 0_usize;
        let mut wet_segments = 0_usize;
        let mut checked_coverage_convenience = false;
        let mut segments_with_condition = 0_usize;
        let mut segments_with_noise_buffer = 0_usize;
        let mut segments_with_noise_density = 0_usize;
        let mut checked_environment_convenience = false;
        let mut segment_records = 0_usize;
        let mut checked_records_convenience = false;
        let mut nonzero_guide_counts = 0_usize;
        let mut nonzero_milestone_counts = 0_usize;
        let mut checked_guide_counts_convenience = false;
        let mut info_prefab_names = 0_usize;
        let mut checked_info_prefab_convenience = false;
        let mut tree_prefab_names = 0_usize;
        let mut checked_tree_prefab_convenience = false;
        let mut checked_fence_prefab_convenience = false;
        let mut checked_net_tail_convenience = false;
        let mut checked_zone_manager_convenience = false;
        let mut checked_prop_manager_convenience = false;
        let mut checked_tree_manager_convenience = false;
        let mut checked_weather_manager_convenience = false;
        let mut checked_district_manager_convenience = false;
        let mut checked_building_flags_convenience = false;
        let mut checked_building_prefab_convenience = false;
        let mut checked_building_object_convenience = false;
        let mut checked_building_problem_convenience = false;
        let mut checked_building_buffer_convenience = false;
        let mut checked_building_byte_state_convenience = false;
        let mut checked_building_timer_convenience = false;
        let mut checked_building_population_convenience = false;
        let mut checked_building_economic_convenience = false;
        let mut checked_building_selection_convenience = false;
        let mut checked_building_records_convenience = false;
        let mut checked_monument_effects_convenience = false;
        let mut checked_faculty_bonuses_convenience = false;
        let mut checked_race_team_unassigned_convenience = false;
        let mut checked_guide_body_locator = false;
        let mut active_buildings = 0_usize;
        let mut buildings_with_secondary_flags = 0_usize;
        let mut building_guide_refs = 0_usize;
        let mut building_milestone_refs = 0_usize;
        let mut building_prefab_names = 0_usize;
        let mut building_object_headers = 0_usize;
        let mut buildings_with_problem_words = 0_usize;
        for path in paths {
            let mut file = File::open(&path)?;
            let package = Package::read_from(&mut file)?;
            assert!(
                !package.assets.is_empty(),
                "{} has no assets",
                path.display()
            );
            for asset in &package.assets {
                assert_eq!(
                    checksum_hex(&asset.data),
                    asset.checksum,
                    "checksum mismatch for asset {:?} in {}",
                    asset.name,
                    path.display()
                );
                match asset.asset_type {
                    3 => {
                        let texture = TextureAsset::decode(asset, package.format_version)?;
                        assert_eq!(
                            texture.encode(package.format_version)?,
                            asset.data,
                            "texture round-trip mismatch for {:?} in {}",
                            asset.name,
                            path.display()
                        );
                    }
                    100 => {
                        let metadata = MapMetaData::decode(asset)?;
                        assert_eq!(
                            metadata.encode()?,
                            asset.data,
                            "metadata round-trip mismatch for {:?} in {}",
                            asset.name,
                            path.display()
                        );
                    }
                    102 | 107 => {
                        let metadata = SystemMapMetaData::decode(asset)?;
                        assert_eq!(metadata.outer_asset_type, asset.asset_type);
                        assert_eq!(
                            metadata.encode()?,
                            asset.data,
                            "system-map metadata round-trip mismatch for {:?} in {}",
                            asset.name,
                            path.display()
                        );
                    }
                    105 => {
                        let metadata = DistrictStyleMetaData::decode(asset)?;
                        assert_eq!(
                            metadata.encode()?,
                            asset.data,
                            "district-style metadata round-trip mismatch for {:?} in {}",
                            asset.name,
                            path.display()
                        );
                    }
                    52 => {
                        let (metadata, consumed) = SimulationMetaData::decode_from(&asset.data)?;
                        assert_eq!(
                            metadata.encode()?,
                            asset.data[..consumed],
                            "simulation metadata round-trip mismatch for {:?} in {}",
                            asset.name,
                            path.display()
                        );
                        let manager_data = SimulationManagerData::decode(&asset.data[consumed..])?;
                        assert_eq!(
                            manager_data.encode()?,
                            asset.data[consumed..],
                            "simulation manager data round-trip mismatch for {:?} in {}",
                            asset.name,
                            path.display()
                        );
                        if path.file_name().and_then(|name| name.to_str())
                            == Some("CL Wolf Creek.crp")
                            && manager_data.format_version() == 112_004
                        {
                            let location = manager_data
                                .locate_building_manager_guide_body(4, metadata.save_app_version)?;
                            assert_eq!(location.graph_offset, 4_200_608);
                            assert_eq!(location.body_length, 17);
                            assert!(location.type_name.starts_with("BuildingOnFireGuide,"));
                            assert_eq!(
                                &manager_data.serialized_object_graph()
                                    [location.graph_offset..location.graph_offset + 17],
                                &[0; 17]
                            );
                            let (guide, consumed) = GuideInstanceBody::decode(
                                &manager_data.serialized_object_graph()[location.graph_offset..],
                            )?;
                            assert_eq!(consumed, 17);
                            let mut editor = guide.into_editor();
                            *editor.build_index_mut() = 0x1234_5678;
                            let mut edited_manager = manager_data.clone();
                            edited_manager.replace_building_manager_guide_instance_body(
                                4,
                                metadata.save_app_version,
                                &editor,
                            )?;
                            assert_eq!(
                                &edited_manager.serialized_object_graph()
                                    [location.graph_offset + 11..location.graph_offset + 15],
                                &0x1234_5678_u32.to_be_bytes()
                            );
                            assert_eq!(
                                edited_manager.serialized_object_graph().len(),
                                manager_data.serialized_object_graph().len()
                            );
                            checked_guide_body_locator = true;
                        }
                        let (zone, zone_end) = manager_data.decode_zone_manager_prefix()?;
                        assert_eq!(zone.format_version(), manager_data.format_version());
                        assert_eq!(zone.object_references().len(), 13);
                        assert!(zone_end < manager_data.serialized_object_graph().len());
                        checked_zone_manager_convenience = true;
                        let (props, prop_end) = manager_data.decode_prop_manager_prefix()?;
                        assert_eq!(props.format_version(), manager_data.format_version());
                        assert_eq!(props.flags().len(), 65_535);
                        assert!(prop_end < manager_data.serialized_object_graph().len());
                        checked_prop_manager_convenience = true;
                        let (trees, trees_end) = manager_data.decode_tree_manager_prefix()?;
                        assert_eq!(trees.format_version(), manager_data.format_version());
                        assert_eq!(trees.flags().len(), 262_143);
                        assert!(trees_end < manager_data.serialized_object_graph().len());
                        checked_tree_manager_convenience = true;
                        let (weather, weather_end) = manager_data.decode_weather_manager_data()?;
                        assert_eq!(weather.format_version(), manager_data.format_version());
                        assert_eq!(weather.self_heights().len(), 16_384);
                        assert_eq!(weather.total_heights().len(), 16_384);
                        assert!(weather_end < manager_data.serialized_object_graph().len());
                        checked_weather_manager_convenience = true;
                        let (vehicle_data, vehicles_end) =
                            manager_data.decode_vehicle_manager_data()?;
                        let vehicles = vehicle_data.prefix();
                        assert_eq!(vehicles.format_version(), manager_data.format_version());
                        assert_eq!(vehicles.vehicle_flags().len(), 16_383);
                        assert_eq!(
                            vehicles.vehicle_flags2().is_some(),
                            manager_data.format_version() >= 274
                        );
                        assert_eq!(
                            vehicles.vehicle_prefab_indices().map(<[u16]>::len),
                            (manager_data.format_version() >= 30).then(|| {
                                vehicles
                                    .vehicle_flags()
                                    .iter()
                                    .filter(|flags| **flags != 0)
                                    .count()
                            })
                        );
                        assert!(vehicles_end < manager_data.serialized_object_graph().len());
                        let encoded_vehicle_body = vehicle_data.encode();
                        assert_eq!(
                            manager_data.serialized_object_graph()
                                [vehicles_end - encoded_vehicle_body.len()..vehicles_end],
                            encoded_vehicle_body
                        );
                        let active_vehicle_count = vehicles
                            .vehicle_flags()
                            .iter()
                            .filter(|flags| **flags != 0)
                            .count();
                        let vehicle_columns = vehicle_data.sparse_columns();
                        assert_eq!(
                            vehicle_columns.gate_indices().map(<[u8]>::len),
                            (manager_data.format_version() >= 182).then_some(active_vehicle_count)
                        );
                        assert_eq!(
                            vehicle_columns.race_teams().map(<[u16]>::len),
                            (manager_data.format_version() >= 121_008)
                                .then_some(active_vehicle_count)
                        );
                        assert_eq!(
                            vehicle_columns.water_sources().map(<[u16]>::len),
                            (manager_data.format_version() >= 273).then_some(active_vehicle_count)
                        );
                        let last_frame = vehicle_data.last_frame_records();
                        assert_eq!(last_frame.records().len(), active_vehicle_count);
                        let vehicle_fields = vehicle_data.instance_fields();
                        assert_eq!(vehicle_fields.records().len(), active_vehicle_count);
                        let parked_count = vehicles.active_parked_vehicle_count();
                        let parked_vehicles = vehicle_data.parked_records();
                        assert_eq!(parked_vehicles.records().map(<[_]>::len), parked_count);
                        assert_eq!(
                            vehicle_data.tail().traffic_flow_totals().is_some(),
                            manager_data.format_version() >= 315
                        );
                        assert_eq!(
                            vehicle_data.tail().temporary_vehicle_count().is_some(),
                            manager_data.format_version() >= 116_013
                        );
                        let (districts, districts_end) =
                            manager_data.decode_district_manager_flags_prefix()?;
                        assert_eq!(districts.format_version(), manager_data.format_version());
                        assert_eq!(districts.district_flags().len(), 128);
                        assert_eq!(districts.park_flags().len(), 128);
                        assert!(
                            districts
                                .district_population_records()
                                .iter()
                                .all(|record| {
                                    record.final_crime_rate().is_some()
                                        == (districts.format_version() >= 203)
                                        && record.last_average_lifespan().is_some()
                                            == (districts.format_version() >= 113_006)
                                        && record.private_data().len() == 6
                                        && record.private_data().iter().all(|block| {
                                            block.as_bytes().len()
                                                == if districts.format_version() >= 256 {
                                                    79
                                                } else {
                                                    75
                                                }
                                        })
                                        && record.specializations().len()
                                            == if districts.format_version() >= 109_006 {
                                                9
                                            } else if districts.format_version() >= 218 {
                                                6
                                            } else {
                                                0
                                            }
                                        && record
                                            .specializations()
                                            .iter()
                                            .all(|block| block.as_bytes().len() == 24)
                                        && record.demographic_data().len()
                                            == usize::from(districts.format_version() >= 167) * 2
                                                + usize::from(districts.format_version() >= 67) * 4
                                                + usize::from(districts.format_version() >= 113) * 3
                                                + usize::from(districts.format_version() >= 143) * 3
                                        && district_records_match_version(
                                            record,
                                            districts.format_version(),
                                        )
                                })
                        );
                        assert!(districts_end < manager_data.serialized_object_graph().len());
                        let (district_grids, district_grid_end) =
                            manager_data.decode_district_manager_grids()?;
                        assert!(district_grid_end > districts_end);
                        assert!(district_grid_end <= manager_data.serialized_object_graph().len());
                        assert_eq!(district_grids.district_plane(0).unwrap().len(), 262_144);
                        assert_eq!(
                            district_grids.park_grid_present(),
                            districts.format_version() >= 110_001
                        );
                        checked_district_manager_convenience = true;
                        let (game_area, game_area_bytes) =
                            manager_data.decode_game_area_prefix()?;
                        assert_eq!(game_area.area_grid().len(), 25);
                        assert!(game_area_bytes <= manager_data.serialized_object_graph().len());
                        let (terrain, manager_prefix_bytes) =
                            manager_data.decode_terrain_prefix()?;
                        assert_eq!(terrain.raw_heights().len(), 1_168_561);
                        assert_eq!(terrain.block_height_deltas().len(), 1_168_561);
                        assert!(
                            manager_prefix_bytes <= manager_data.serialized_object_graph().len()
                        );
                        let (water, water_end) = manager_data.decode_water_prefix()?;
                        assert_eq!(water.heights().len(), 1_168_561);
                        assert_eq!(water.pollution().len(), 1_168_561);
                        assert_eq!(water.velocity_x().len(), 1_168_561);
                        assert_eq!(water.velocity_z().len(), 1_168_561);
                        if water.wave_count() != 0 {
                            nonempty_wave_assets += 1;
                        }
                        assert!(water_end > manager_prefix_bytes);
                        assert_eq!(
                            manager_data
                                .serialized_object_graph()
                                .get(water_end..)
                                .and_then(|remaining| remaining.get(..4)),
                            Some(&[0xfe, 0x80, 0x00, 0x0f][..]),
                            "InstanceManager profile start in {}",
                            path.display()
                        );
                        let (instance, instance_end) = manager_data.decode_instance_prefix()?;
                        assert!(instance.names().is_empty(), "{}", path.display());
                        assert_eq!(
                            instance.group_count(),
                            (manager_data.format_version() >= 258).then_some(0),
                            "{}",
                            path.display()
                        );
                        assert!(instance_end > water_end);
                        assert_eq!(
                            instance.encode(),
                            if manager_data.format_version() >= 258 {
                                vec![0; 8]
                            } else {
                                vec![0; 4]
                            },
                            "{}",
                            path.display()
                        );
                        let (path_data, path_end) = manager_data.decode_path_prefix()?;
                        assert_eq!(path_data.simulation_flags().len(), 262_143);
                        assert!(path_data.active_units().is_empty(), "{}", path.display());
                        assert_eq!(
                            path_data.encode(),
                            include_bytes!(
                                "../tests/fixtures/path-manager-body-v116020-desert.bin"
                            ),
                            "{}",
                            path.display()
                        );
                        assert!(path_end > instance_end);
                        let (net_lanes, net_prefix_end) = manager_data.decode_net_lane_prefix()?;
                        assert_eq!(net_lanes.lane_flags().len(), 262_143);
                        assert!(net_prefix_end > path_end);
                        active_net_lanes += net_lanes.active_lanes().len();
                        reserved_net_lanes += net_lanes
                            .active_lanes()
                            .iter()
                            .filter(|lane| {
                                lane.reserved_prev != 0
                                    || lane.reserved_next != 0
                                    || lane.reserved_frame != 0
                                    || lane.last_reserve_id != 0
                            })
                            .count();
                        // Decode each section once; the convenience methods
                        // intentionally rediscover earlier graph boundaries.
                        let graph = manager_data.serialized_object_graph();
                        let tail = graph.get(net_prefix_end..).ok_or_else(|| {
                            std::io::Error::other("NetManager offset exceeds graph")
                        })?;
                        let mut net_reader = crate::data_serializer::Reader::new(tail);
                        let version = manager_data.format_version();
                        let (net_nodes, used) = NetNodeFlagsPrefix::decode_prefix(
                            version,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        assert_eq!(net_nodes.node_flags().len(), 32_767);
                        assert!(used > 0);
                        active_net_nodes += net_nodes
                            .node_flags()
                            .iter()
                            .filter(|node| node.flags != 0 || node.flags2 != 0)
                            .count();
                        let (problems, used) = NetNodeProblemsPrefix::decode_prefix(
                            version,
                            metadata.save_app_version,
                            &net_nodes,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        nodes_with_problems += problems
                            .problems()
                            .iter()
                            .filter(|problem| problem.problems1 != 0 || problem.problems2 != 0)
                            .count();
                        let (counters, used) = NetNodeCountersPrefix::decode_prefix(
                            version,
                            &net_nodes,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        nodes_with_counters += counters
                            .counters()
                            .iter()
                            .filter(|counter| {
                                counter.max_wait_time != 0
                                    || counter.temp_counter != 0
                                    || counter.final_counter != 0
                            })
                            .count();
                        let (records, used) = NetNodeRecordsPrefix::decode_prefix(
                            version,
                            &net_nodes,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        assert_eq!(records.records().len(), counters.counters().len());
                        nodes_with_segments += records
                            .records()
                            .iter()
                            .filter(|record| !record.segments.is_empty())
                            .count();
                        nodes_with_event_routes += records
                            .records()
                            .iter()
                            .filter(|record| record.event_route != 0)
                            .count();
                        let (segment_flags, used) = NetSegmentFlagsPrefix::decode_prefix(
                            version,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        assert!(used > 0);
                        assert_eq!(segment_flags.segment_flags().len(), 36_863);
                        let active_segments = segment_flags
                            .segment_flags()
                            .iter()
                            .filter(|segment| segment.flags != 0)
                            .count();
                        active_net_segments += active_segments;
                        segments_with_secondary_flags += segment_flags
                            .segment_flags()
                            .iter()
                            .filter(|segment| segment.flags2 != 0)
                            .count();
                        let (segment_problems, used) = NetSegmentProblemsPrefix::decode_prefix(
                            version,
                            &segment_flags,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        assert_eq!(segment_problems.problems().len(), active_segments);
                        segments_with_problems += segment_problems
                            .problems()
                            .iter()
                            .filter(|problem| problem.problems1 != 0 || problem.problems2 != 0)
                            .count();
                        let (traffic_lights, used) = NetSegmentTrafficLightsPrefix::decode_prefix(
                            version,
                            &segment_flags,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        assert_eq!(traffic_lights.states().len(), active_segments);
                        segments_with_traffic_lights += traffic_lights
                            .states()
                            .iter()
                            .filter(|state| state.state0 != 0 || state.state1 != 0)
                            .count();
                        let (coverage, used) = NetSegmentCoveragePrefix::decode_prefix(
                            version,
                            &segment_flags,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        assert_eq!(coverage.coverage().len(), active_segments);
                        segments_with_fire_coverage += coverage
                            .coverage()
                            .iter()
                            .filter(|item| item.fire_coverage != 0)
                            .count();
                        wet_segments += coverage
                            .coverage()
                            .iter()
                            .filter(|item| item.wetness != 0)
                            .count();
                        if !checked_coverage_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_net_segment_coverage_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, coverage);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_coverage_convenience = true;
                        }
                        let (environment, used) = NetSegmentEnvironmentPrefix::decode_prefix(
                            version,
                            &segment_flags,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        assert_eq!(environment.environment().len(), active_segments);
                        segments_with_condition += environment
                            .environment()
                            .iter()
                            .filter(|item| item.condition != 0)
                            .count();
                        segments_with_noise_buffer += environment
                            .environment()
                            .iter()
                            .filter(|item| item.noise_buffer != 0)
                            .count();
                        segments_with_noise_density += environment
                            .environment()
                            .iter()
                            .filter(|item| item.noise_density != 0)
                            .count();
                        if !checked_environment_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_net_segment_environment_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, environment);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_environment_convenience = true;
                        }
                        let (records, used) = NetSegmentRecordsPrefix::decode_prefix(
                            version,
                            &segment_flags,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        assert_eq!(records.records().len(), active_segments);
                        segment_records += records.records().len();
                        if !checked_records_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_net_segment_records_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, records);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_records_convenience = true;
                        }
                        let (guide_counts, used) = NetGuideCountsPrefix::decode_prefix(
                            version,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        nonzero_guide_counts += usize::from(guide_counts.guide_count() != 0);
                        nonzero_milestone_counts +=
                            usize::from(guide_counts.milestone_count() != 0);
                        if !checked_guide_counts_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_net_guide_counts_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, guide_counts);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_guide_counts_convenience = true;
                        }
                        let (info_prefabs, used) = NetInfoPrefabPrefix::decode_prefix(
                            version,
                            &net_nodes,
                            &segment_flags,
                            &guide_counts,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        assert_eq!(
                            info_prefabs.node_info_indices().len(),
                            net_nodes
                                .node_flags()
                                .iter()
                                .filter(|node| node.flags != 0)
                                .count()
                        );
                        assert_eq!(info_prefabs.segment_info_indices().len(), active_segments);
                        info_prefab_names += info_prefabs
                            .names()
                            .iter()
                            .filter(|name| name.is_some())
                            .count();
                        if !checked_info_prefab_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_net_info_prefab_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, info_prefabs);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_info_prefab_convenience = true;
                        }
                        let (tree_prefabs, used) = NetTreePrefabPrefix::decode_prefix(
                            version,
                            &records,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        tree_prefab_names += tree_prefabs
                            .names()
                            .iter()
                            .filter(|name| name.is_some())
                            .count();
                        if !checked_tree_prefab_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_net_tree_prefab_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, tree_prefabs);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_tree_prefab_convenience = true;
                        }
                        let (fence_prefabs, used) = NetFencePrefabPrefix::decode_prefix(
                            version,
                            &records,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        if version < 120_003 {
                            assert_eq!(used, 0);
                            assert!(fence_prefabs.left_indices().is_empty());
                            assert!(fence_prefabs.right_indices().is_empty());
                        }
                        if !checked_fence_prefab_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_net_fence_prefab_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, fence_prefabs);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_fence_prefab_convenience = true;
                        }
                        let (tail, used) = NetManagerTailPrefix::decode_prefix(
                            version,
                            &guide_counts,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        assert_eq!(tail.profile_end_shared_string_index(), Some(6));
                        let mut next_manager =
                            crate::data_serializer::Reader::new(net_reader.remaining_bytes());
                        assert_eq!(next_manager.read_u8()?, 0xfe);
                        assert_eq!(
                            next_manager.read_unique_string()?.as_deref(),
                            Some("BuildingManager")
                        );
                        if !checked_net_tail_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_net_manager_tail_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, tail);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_net_tail_convenience = true;
                        }
                        net_reader.read_bytes(next_manager.offset())?;
                        let (building_flags, used) = BuildingFlagsPrefix::decode_prefix(
                            version,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(used)?;
                        let mut building_counts =
                            crate::data_serializer::Reader::new(net_reader.remaining_bytes());
                        let guides = building_counts.read_u16_be()?;
                        let milestones = building_counts.read_u16_be()?;
                        net_reader.read_bytes(building_counts.offset())?;
                        building_guide_refs += usize::from(guides);
                        building_milestone_refs += usize::from(milestones);
                        assert_eq!(
                            building_flags.encode(),
                            net_reader.consumed_bytes()
                                [net_reader.offset() - used - 4..net_reader.offset() - 4]
                        );
                        active_buildings += building_flags
                            .building_flags()
                            .iter()
                            .filter(|flag| flag.flags != 0)
                            .count();
                        buildings_with_secondary_flags += building_flags
                            .building_flags()
                            .iter()
                            .filter(|flag| flag.flags2 != 0)
                            .count();
                        if !checked_building_flags_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_flags_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, building_flags);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset() - 4);
                            checked_building_flags_convenience = true;
                        }
                        let (building_prefabs, prefab_used) = BuildingPrefabPrefix::decode_prefix(
                            version,
                            &building_flags,
                            guides,
                            milestones,
                            net_reader.remaining_bytes(),
                        )?;
                        net_reader.read_bytes(prefab_used)?;
                        building_prefab_names += building_prefabs
                            .names()
                            .iter()
                            .filter(|name| name.is_some())
                            .count();
                        if !checked_building_prefab_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_prefabs_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, building_prefabs);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_building_prefab_convenience = true;
                        }
                        let (building_objects, object_used) =
                            BuildingManagerObjectHeadersPrefix::decode_prefix(
                                version,
                                &building_prefabs,
                                net_reader.remaining_bytes(),
                            )?;
                        net_reader.read_bytes(object_used)?;
                        building_object_headers += building_objects.object_headers().len();
                        if !checked_building_object_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_object_headers_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, building_objects);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_building_object_convenience = true;
                        }
                        let active_building_count = building_flags
                            .building_flags()
                            .iter()
                            .filter(|building| building.flags != 0)
                            .count();
                        let array_start = net_reader.offset();
                        let (building_problems, problem_used) =
                            BuildingProblemPrefix::decode_prefix(
                                version,
                                metadata.save_app_version,
                                &building_flags,
                                net_reader.remaining_bytes(),
                            )?;
                        net_reader.read_bytes(problem_used)?;
                        assert_eq!(building_problems.problems().len(), active_building_count);
                        assert_eq!(
                            building_problems.encode(),
                            &net_reader.consumed_bytes()[array_start..]
                        );
                        buildings_with_problem_words += building_problems
                            .problems()
                            .iter()
                            .filter(|problem| problem.first != 0 || problem.second != 0)
                            .count();
                        if !checked_building_problem_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_problems_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, building_problems);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());

                            if !building_problems.problems().is_empty() {
                                let mut edited_manager = manager_data.clone();
                                let mut editor = building_problems.clone().into_editor();
                                let edited_first = editor.problems_mut()[0].first.wrapping_add(1);
                                editor.problems_mut()[0].first = edited_first;
                                let original_suffix =
                                    manager_data.serialized_object_graph()[graph_end..].to_vec();
                                edited_manager.replace_building_problems(
                                    &editor,
                                    metadata.save_app_version,
                                )?;
                                let (rewritten, rewritten_end) = edited_manager
                                    .decode_building_problems_prefix(metadata.save_app_version)?;
                                assert_eq!(rewritten.problems()[0].first, edited_first);
                                assert_eq!(
                                    &edited_manager.serialized_object_graph()[rewritten_end..],
                                    original_suffix
                                );
                            }
                            checked_building_problem_convenience = true;
                        }
                        let (building_buffers, buffer_used) =
                            BuildingBufferFieldsPrefix::decode_prefix(
                                version,
                                &building_flags,
                                net_reader.remaining_bytes(),
                            )?;
                        let buffer_start = net_reader.offset();
                        net_reader.read_bytes(buffer_used)?;
                        assert_eq!(
                            building_buffers.encode(),
                            &net_reader.consumed_bytes()[buffer_start..]
                        );
                        if !checked_building_buffer_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_buffer_fields_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, building_buffers);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_building_buffer_convenience = true;
                        }
                        let (building_byte_state, byte_state_used) =
                            BuildingByteStatePrefix::decode_prefix(
                                version,
                                &building_flags,
                                net_reader.remaining_bytes(),
                            )?;
                        let byte_state_start = net_reader.offset();
                        net_reader.read_bytes(byte_state_used)?;
                        assert_eq!(
                            building_byte_state.encode(),
                            &net_reader.consumed_bytes()[byte_state_start..]
                        );
                        if !checked_building_byte_state_convenience && active_segments > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_byte_state_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, building_byte_state);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_building_byte_state_convenience = true;
                        }
                        let timer_start = net_reader.offset();
                        let (timer_geometry, timer_geometry_used) =
                            BuildingTimerGeometryPrefix::decode_prefix(
                                version,
                                &building_flags,
                                net_reader.remaining_bytes(),
                            )?;
                        net_reader.read_bytes(timer_geometry_used)?;
                        assert_eq!(
                            timer_geometry.encode(),
                            &net_reader.consumed_bytes()
                                [timer_start..timer_start + timer_geometry_used]
                        );
                        assert_eq!(timer_geometry.width().len(), active_building_count);
                        assert_eq!(timer_geometry.length().len(), active_building_count);
                        if !checked_building_timer_convenience && active_building_count > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_timer_geometry_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, timer_geometry);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());

                            let mut edited_manager = manager_data.clone();
                            let original_width = timer_geometry.width()[0];
                            let mut editor = timer_geometry.into_editor();
                            let edited_width = original_width.wrapping_add(1);
                            editor.width_mut()[0] = edited_width;
                            let original_suffix =
                                manager_data.serialized_object_graph()[graph_end..].to_vec();
                            edited_manager.replace_building_timer_geometry(
                                &editor,
                                metadata.save_app_version,
                            )?;
                            let (rewritten, rewritten_end) = edited_manager
                                .decode_building_timer_geometry_prefix(metadata.save_app_version)?;
                            assert_eq!(rewritten.width()[0], edited_width);
                            assert_eq!(
                                &edited_manager.serialized_object_graph()[rewritten_end..],
                                original_suffix
                            );
                            checked_building_timer_convenience = true;
                        }
                        let population_start = net_reader.offset();
                        let (building_population, population_used) =
                            BuildingPopulationPrefix::decode_prefix(
                                version,
                                &building_flags,
                                net_reader.remaining_bytes(),
                            )?;
                        net_reader.read_bytes(population_used)?;
                        assert_eq!(
                            building_population.encode(),
                            &net_reader.consumed_bytes()
                                [population_start..population_start + population_used]
                        );
                        assert_eq!(
                            building_population.education1().len(),
                            active_building_count
                        );
                        assert_eq!(building_population.teens().len(), active_building_count);
                        if !checked_building_population_convenience && active_building_count > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_population_prefix(metadata.save_app_version)?;
                            assert_eq!(via_manager, building_population);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());

                            let mut edited_manager = manager_data.clone();
                            let mut editor = building_population.into_editor();
                            let original_education = editor.education1_mut()[0];
                            let edited_education = original_education.wrapping_add(1);
                            editor.education1_mut()[0] = edited_education;
                            let original_suffix =
                                manager_data.serialized_object_graph()[graph_end..].to_vec();
                            edited_manager
                                .replace_building_population(&editor, metadata.save_app_version)?;
                            let (rewritten, rewritten_end) = edited_manager
                                .decode_building_population_prefix(metadata.save_app_version)?;
                            assert_eq!(rewritten.education1()[0], edited_education);
                            assert_eq!(
                                &edited_manager.serialized_object_graph()[rewritten_end..],
                                original_suffix
                            );
                            checked_building_population_convenience = true;
                        }
                        let economic_start = net_reader.offset();
                        let (building_economic, economic_used) =
                            BuildingEconomicFieldsPrefix::decode_prefix(
                                version,
                                &building_flags,
                                net_reader.remaining_bytes(),
                            )?;
                        net_reader.read_bytes(economic_used)?;
                        assert_eq!(
                            building_economic.encode(),
                            &net_reader.consumed_bytes()
                                [economic_start..economic_start + economic_used]
                        );
                        assert_eq!(
                            building_economic.cargo_traffic_rate().len(),
                            if version >= 115_004 {
                                active_building_count
                            } else {
                                0
                            }
                        );
                        if !checked_building_economic_convenience
                            && active_building_count > 0
                            && version >= 115_004
                        {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_economic_fields_prefix(
                                    metadata.save_app_version,
                                )?;
                            assert_eq!(via_manager, building_economic);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());

                            let mut edited_manager = manager_data.clone();
                            let mut editor = building_economic.into_editor();
                            let original_rate = editor.cargo_traffic_rate_mut()[0];
                            let edited_rate = original_rate.wrapping_add(1);
                            editor.cargo_traffic_rate_mut()[0] = edited_rate;
                            let original_suffix =
                                manager_data.serialized_object_graph()[graph_end..].to_vec();
                            edited_manager.replace_building_economic_fields(
                                &editor,
                                metadata.save_app_version,
                            )?;
                            let (rewritten, rewritten_end) = edited_manager
                                .decode_building_economic_fields_prefix(
                                    metadata.save_app_version,
                                )?;
                            assert_eq!(rewritten.cargo_traffic_rate()[0], edited_rate);
                            assert_eq!(
                                &edited_manager.serialized_object_graph()[rewritten_end..],
                                original_suffix
                            );
                            checked_building_economic_convenience = true;
                        }
                        let service_selection_start = net_reader.offset();
                        let shared_strings = [
                            "SimulationManager",
                            "GameAreaManager",
                            "TerrainManager",
                            "WaterSimulation",
                            "InstanceManager",
                            "PathManager",
                            "NetManager",
                            "BuildingManager",
                        ]
                        .into_iter()
                        .map(|value| Some(value.to_owned()))
                        .collect();
                        let (service_selection, service_selection_used) =
                            BuildingServiceSelectionPrefix::decode_prefix(
                                version,
                                net_reader.remaining_bytes(),
                                shared_strings,
                            )?;
                        net_reader.read_bytes(service_selection_used)?;
                        assert_eq!(
                            service_selection.encode(),
                            &net_reader.consumed_bytes()[service_selection_start
                                ..service_selection_start + service_selection_used]
                        );
                        if !checked_building_selection_convenience && active_building_count > 0 {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_service_selection_prefix(
                                    metadata.save_app_version,
                                )?;
                            assert_eq!(via_manager, service_selection);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            checked_building_selection_convenience = true;
                        }
                        let instance_records_start = net_reader.offset();
                        let (instance_records, instance_records_used) =
                            BuildingInstanceRecordsPrefix::decode_prefix(
                                version,
                                &building_flags,
                                net_reader.remaining_bytes(),
                            )?;
                        net_reader.read_bytes(instance_records_used)?;
                        assert_eq!(
                            instance_records.encode(),
                            &net_reader.consumed_bytes()[instance_records_start
                                ..instance_records_start + instance_records_used]
                        );
                        if !checked_building_records_convenience
                            && !instance_records.records().is_empty()
                        {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_instance_records_prefix(
                                    metadata.save_app_version,
                                )?;
                            assert_eq!(via_manager, instance_records);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());

                            let mut edited_manager = manager_data.clone();
                            let mut editor = instance_records.clone().into_editor();
                            editor.records_mut()[0].angle_bits ^= 1;
                            let original_suffix =
                                manager_data.serialized_object_graph()[graph_end..].to_vec();
                            edited_manager.replace_building_instance_records(
                                &editor,
                                metadata.save_app_version,
                            )?;
                            let (rewritten, rewritten_end) = edited_manager
                                .decode_building_instance_records_prefix(
                                    metadata.save_app_version,
                                )?;
                            assert_eq!(
                                rewritten.records()[0].angle_bits,
                                editor.records_mut()[0].angle_bits
                            );
                            assert_eq!(
                                &edited_manager.serialized_object_graph()[rewritten_end..],
                                original_suffix
                            );
                            checked_building_records_convenience = true;
                        }
                        let monument_effects_start = net_reader.offset();
                        let (monument_effects, monument_effects_used) =
                            BuildingMonumentEffectsPrefix::decode_prefix(
                                version,
                                net_reader.remaining_bytes(),
                            )?;
                        net_reader.read_bytes(monument_effects_used)?;
                        assert_eq!(
                            monument_effects.encode(),
                            &net_reader.consumed_bytes()[monument_effects_start
                                ..monument_effects_start + monument_effects_used]
                        );
                        if !checked_monument_effects_convenience
                            && !monument_effects.temporary().is_empty()
                        {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_monument_effects_prefix(
                                    metadata.save_app_version,
                                )?;
                            assert_eq!(via_manager, monument_effects);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            let mut edited_manager = manager_data.clone();
                            let mut editor = monument_effects.clone().into_editor();
                            editor.temporary_mut()[0].factor ^= 1;
                            let original_suffix =
                                manager_data.serialized_object_graph()[graph_end..].to_vec();
                            edited_manager.replace_building_monument_effects(
                                &editor,
                                metadata.save_app_version,
                            )?;
                            let (rewritten, rewritten_end) = edited_manager
                                .decode_building_monument_effects_prefix(
                                    metadata.save_app_version,
                                )?;
                            assert_eq!(
                                rewritten.temporary()[0].factor,
                                editor.temporary()[0].factor
                            );
                            assert_eq!(
                                &edited_manager.serialized_object_graph()[rewritten_end..],
                                original_suffix
                            );
                            checked_monument_effects_convenience = true;
                        }

                        let faculty_bonuses_start = net_reader.offset();
                        let (faculty_bonuses, faculty_bonuses_used) =
                            BuildingFacultyBonusesPrefix::decode_prefix(
                                version,
                                net_reader.remaining_bytes(),
                            )?;
                        net_reader.read_bytes(faculty_bonuses_used)?;
                        assert_eq!(
                            faculty_bonuses.encode(),
                            &net_reader.consumed_bytes()[faculty_bonuses_start
                                ..faculty_bonuses_start + faculty_bonuses_used]
                        );
                        if !checked_faculty_bonuses_convenience
                            && !faculty_bonuses.temporary().is_empty()
                        {
                            let (via_manager, graph_end) = manager_data
                                .decode_building_faculty_bonuses_prefix(
                                    metadata.save_app_version,
                                )?;
                            assert_eq!(via_manager, faculty_bonuses);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            let mut edited_manager = manager_data.clone();
                            let mut editor = faculty_bonuses.clone().into_editor();
                            editor.final_values_mut()[0].factor ^= 1;
                            let original_suffix =
                                manager_data.serialized_object_graph()[graph_end..].to_vec();
                            edited_manager.replace_building_faculty_bonuses(
                                &editor,
                                metadata.save_app_version,
                            )?;
                            let (rewritten, rewritten_end) = edited_manager
                                .decode_building_faculty_bonuses_prefix(
                                    metadata.save_app_version,
                                )?;
                            assert_eq!(
                                rewritten.final_values()[0].factor,
                                editor.final_values()[0].factor
                            );
                            assert_eq!(
                                &edited_manager.serialized_object_graph()[rewritten_end..],
                                original_suffix
                            );
                            checked_faculty_bonuses_convenience = true;
                        }
                        if !checked_race_team_unassigned_convenience {
                            let race_start = net_reader.offset();
                            let (race_presence, race_presence_used) =
                                BuildingRaceTeamUnassignedPrefix::decode_prefix(
                                    version,
                                    &building_flags,
                                    net_reader.remaining_bytes(),
                                )?;
                            net_reader.read_bytes(race_presence_used)?;
                            assert_eq!(
                                race_presence.encode(),
                                &net_reader.consumed_bytes()
                                    [race_start..race_start + race_presence_used]
                            );
                            let (via_manager, graph_end) = manager_data
                                .decode_building_race_team_unassigned_prefix(
                                    metadata.save_app_version,
                                )?;
                            assert_eq!(via_manager, race_presence);
                            assert_eq!(graph_end, net_prefix_end + net_reader.offset());
                            let mut edited_manager = manager_data.clone();
                            let editor = race_presence.clone().into_editor();
                            let original_suffix =
                                manager_data.serialized_object_graph()[graph_end..].to_vec();
                            edited_manager.replace_building_race_team_unassigned(
                                &editor,
                                metadata.save_app_version,
                            )?;
                            let (_, rewritten_end) = edited_manager
                                .decode_building_race_team_unassigned_prefix(
                                    metadata.save_app_version,
                                )?;
                            assert_eq!(
                                &edited_manager.serialized_object_graph()[rewritten_end..],
                                original_suffix
                            );
                            checked_race_team_unassigned_convenience = true;
                        }
                        if std::env::var_os("CRP_EXTRACT_BUILDING_BYTE_STATE_FIXTURE").is_some()
                            && path
                                .file_name()
                                .is_some_and(|name| name == "CCMP2 Desert Map 1.crp")
                        {
                            std::fs::write(
                                "tests/fixtures/building-byte-state-v116020-desert.bin",
                                building_byte_state.encode(),
                            )?;
                        }
                        if std::env::var_os("CRP_EXTRACT_BUILDING_BUFFER_FIXTURE").is_some()
                            && path
                                .file_name()
                                .is_some_and(|name| name == "CCMP2 Desert Map 1.crp")
                        {
                            std::fs::write(
                                "tests/fixtures/building-buffer-fields-v116020-desert.bin",
                                building_buffers.encode(),
                            )?;
                        }
                        if std::env::var_os("CRP_EXTRACT_BUILDING_OBJECT_FIXTURE").is_some()
                            && path
                                .file_name()
                                .is_some_and(|name| name == "CCMP2 Desert Map 1.crp")
                        {
                            let mut fixture = building_objects.encode();
                            fixture.extend_from_slice(&building_problems.encode());
                            std::fs::write(
                                "tests/fixtures/building-object-headers-v116020-desert.bin",
                                fixture,
                            )?;
                        }
                        if std::env::var_os("CRP_EXTRACT_BUILDING_PREFAB_FIXTURE").is_some()
                            && path
                                .file_name()
                                .is_some_and(|name| name == "CCMP2 Desert Map 1.crp")
                        {
                            std::fs::write(
                                "tests/fixtures/building-prefabs-v116020-desert.bin",
                                building_prefabs.encode(),
                            )?;
                        }
                        let canonical = water.clone().into_editor().encode()?;
                        let (rewritten, rewritten_length) = WaterSimulationData::decode_prefix(
                            manager_data.format_version(),
                            &canonical,
                        )?;
                        assert_eq!(rewritten_length, canonical.len());
                        assert_eq!(rewritten.heights(), water.heights());
                        assert_eq!(rewritten.pollution(), water.pollution());
                        assert_eq!(rewritten.velocity_x(), water.velocity_x());
                        assert_eq!(rewritten.velocity_z(), water.velocity_z());
                        assert_eq!(rewritten.wave_records(), water.wave_records());
                        assert_eq!(rewritten.waves(), water.waves());
                        assert_eq!(rewritten.sea_level().to_bits(), water.sea_level().to_bits());
                        assert_eq!(rewritten.pollution_rates(), water.pollution_rates());
                        assert_eq!(rewritten.frame_index(), water.frame_index());
                        assert_eq!(rewritten.sources().len(), water.sources().len());
                        for (actual, expected) in rewritten.sources().iter().zip(water.sources()) {
                            assert_eq!(
                                actual.input_position.map(f32::to_bits),
                                expected.input_position.map(f32::to_bits)
                            );
                            assert_eq!(
                                actual.output_position.map(f32::to_bits),
                                expected.output_position.map(f32::to_bits)
                            );
                            assert_eq!(actual.source_type, expected.source_type);
                            assert_eq!(actual.target, expected.target);
                            assert_eq!(actual.parameters, expected.parameters);
                        }
                        simulation_metadata_count += 1;
                    }
                    _ => {}
                }
            }
        }
        assert!(simulation_metadata_count > 0);
        assert!(checked_coverage_convenience);
        assert!(checked_environment_convenience);
        assert!(checked_records_convenience);
        assert!(checked_guide_counts_convenience);
        assert!(checked_info_prefab_convenience);
        assert!(checked_tree_prefab_convenience);
        assert!(checked_fence_prefab_convenience);
        assert!(checked_net_tail_convenience);
        assert!(checked_zone_manager_convenience);
        assert!(checked_prop_manager_convenience);
        assert!(checked_tree_manager_convenience);
        assert!(checked_weather_manager_convenience);
        assert!(checked_district_manager_convenience);
        assert!(checked_building_flags_convenience);
        assert!(checked_building_prefab_convenience);
        assert!(checked_building_object_convenience);
        assert!(checked_building_problem_convenience);
        assert!(checked_building_buffer_convenience);
        assert!(checked_building_byte_state_convenience);
        assert!(checked_building_timer_convenience);
        assert!(checked_building_population_convenience);
        assert!(checked_building_economic_convenience);
        assert!(checked_building_selection_convenience);
        assert!(checked_building_records_convenience);
        assert!(checked_monument_effects_convenience);
        assert!(checked_faculty_bonuses_convenience);
        assert!(checked_race_team_unassigned_convenience);
        assert!(checked_guide_body_locator);
        assert!(active_buildings > 0);
        assert_eq!(segment_records, active_net_segments);
        eprintln!(
            "external corpus: {simulation_metadata_count} type-52 assets, {nonempty_wave_assets} with waves, {active_net_lanes} active lanes, {reserved_net_lanes} with nonzero reservation state, {active_net_nodes} active nodes, {nodes_with_problems} with problems, {nodes_with_counters} with counters, {nodes_with_segments} with segments, {nodes_with_event_routes} with event routes, {active_net_segments} active segments, {segments_with_secondary_flags} with secondary flags, {segments_with_problems} with problems, {segments_with_traffic_lights} with traffic-light state, {segments_with_fire_coverage} with fire coverage, {wet_segments} wet, {segments_with_condition} with condition, {segments_with_noise_buffer} with noise buffer, {segments_with_noise_density} with noise density, {nonzero_guide_counts} with guide references, {nonzero_milestone_counts} with milestone references, {info_prefab_names} info prefab names, {tree_prefab_names} tree prefab names, {active_buildings} active buildings, {buildings_with_secondary_flags} with secondary flags, {building_guide_refs} building guide references, {building_milestone_refs} building milestone references, {building_prefab_names} building prefab names, {building_object_headers} BuildingManager object headers, {buildings_with_problem_words} buildings with nonzero problem words"
        );
        Ok(())
    }
}
