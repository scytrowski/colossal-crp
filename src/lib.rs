//! Read the outer container structure of City: Skylines (2015) `.crp` files.
//!
//! This reader supports the common package index observed in format versions
//! 4 through 10. It verifies each asset's stored MD5 and returns raw payloads.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom, Write};

mod asset_io;
mod building_manager_data;
mod data_serializer;
mod encoded_array;
mod game_area_data;
mod instance_manager_data;
mod metadata;
mod net_manager_data;
mod path_manager_data;
mod simulation_manager_data;
mod simulation_metadata;
mod terrain_manager_data;
mod texture;
mod water_simulation_data;

pub use building_manager_data::{
    BuildingFlag, BuildingFlagsPrefix, BuildingManagerObjectHeadersPrefix, BuildingPrefabPrefix,
};
pub use data_serializer::DataSerializerStream;
pub use game_area_data::{GameAreaManagerData, SavedCameraView};
pub use instance_manager_data::{InstanceManagerData, InstanceManagerEditor, InstanceName};
pub use metadata::{MapMetaData, ModInfo};
pub use net_manager_data::{
    NetFencePrefabPrefix, NetGuideCountsPrefix, NetInfoPrefabPrefix, NetLane, NetLanePrefix,
    NetManagerTailPrefix, NetNodeCounter, NetNodeCountersPrefix, NetNodeFlag, NetNodeFlagsPrefix,
    NetNodeProblem, NetNodeProblemsPrefix, NetNodeRecord, NetNodeRecordsPrefix, NetObjectHeader,
    NetSegmentCoverage, NetSegmentCoveragePrefix, NetSegmentEnvironment,
    NetSegmentEnvironmentPrefix, NetSegmentFlag, NetSegmentFlagsPrefix, NetSegmentProblem,
    NetSegmentProblemsPrefix, NetSegmentRecord, NetSegmentRecordsPrefix, NetSegmentTrafficLight,
    NetSegmentTrafficLightsPrefix, NetTreePrefabPrefix,
};
pub use path_manager_data::{PathManagerData, PathPosition, PathUnit};
pub use simulation_manager_data::{
    ManagerObjectReference, SerializableDataEntry, SimulationManagerData,
};
pub use simulation_metadata::{ModOverride, SimulationMetaData};
pub use terrain_manager_data::TerrainManagerData;
pub use texture::TextureAsset;
pub use water_simulation_data::{
    WaterSimulationData, WaterSimulationEditor, WaterSource, WaterWave,
};

const MAGIC: &[u8; 4] = b"CRAP";
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

        let mut assets = Vec::with_capacity(asset_count);
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
        let mut encoded = vec![0xff; 60];
        encoded.extend_from_slice(&[0; 32]);
        let (headers, used) =
            BuildingManagerObjectHeadersPrefix::decode_prefix(116_020, &prefabs, &encoded)?;
        assert_eq!(headers.object_headers().len(), 60);
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
    fn building_object_headers_game_fixture_stops_before_problem_array() -> Result<(), CrpError> {
        let flags_bytes = include_bytes!("../tests/fixtures/building-flags-v116020-desert.bin");
        let (flags, _) = BuildingFlagsPrefix::decode_prefix(116_020, flags_bytes)?;
        let prefab_bytes = include_bytes!("../tests/fixtures/building-prefabs-v116020-desert.bin");
        let (prefabs, _) =
            BuildingPrefabPrefix::decode_prefix(116_020, &flags, 0, 0, prefab_bytes)?;
        let bytes = include_bytes!("../tests/fixtures/building-object-headers-v116020-desert.bin");
        let (headers, used) =
            BuildingManagerObjectHeadersPrefix::decode_prefix(116_020, &prefabs, bytes)?;
        assert_eq!(headers.object_headers().len(), 60);
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

        let mut cursor = Cursor::new(encoded);
        assert!(matches!(
            Package::read_from(&mut cursor),
            Err(CrpError::AssetOutOfBounds { .. })
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
                Some(("UnlimitedMoney", -1)),
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
        let mut checked_building_flags_convenience = false;
        let mut checked_building_prefab_convenience = false;
        let mut checked_building_object_convenience = false;
        let mut active_buildings = 0_usize;
        let mut buildings_with_secondary_flags = 0_usize;
        let mut building_guide_refs = 0_usize;
        let mut building_milestone_refs = 0_usize;
        let mut building_prefab_names = 0_usize;
        let mut building_object_headers = 0_usize;
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
                        if version >= 115_000 {
                            let active_building_count = building_flags
                                .building_flags()
                                .iter()
                                .filter(|building| building.flags != 0)
                                .count();
                            let array_start = net_reader.offset();
                            let problem_value_count = active_building_count.checked_mul(2).ok_or(
                                CrpError::ArithmeticOverflow("BuildingManager problem value count"),
                            )?;
                            let problem_values = crate::encoded_array::read_encoded_ulong_array(
                                &mut net_reader,
                                problem_value_count,
                            )?;
                            assert_eq!(problem_values.len(), problem_value_count);
                            if std::env::var_os("CRP_EXTRACT_BUILDING_OBJECT_FIXTURE").is_some()
                                && path
                                    .file_name()
                                    .is_some_and(|name| name == "CCMP2 Desert Map 1.crp")
                            {
                                let mut fixture = building_objects.encode();
                                fixture
                                    .extend_from_slice(&net_reader.consumed_bytes()[array_start..]);
                                std::fs::write(
                                    "tests/fixtures/building-object-headers-v116020-desert.bin",
                                    fixture,
                                )?;
                            }
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
        assert!(checked_building_flags_convenience);
        assert!(checked_building_prefab_convenience);
        assert!(checked_building_object_convenience);
        assert!(active_buildings > 0);
        assert_eq!(segment_records, active_net_segments);
        eprintln!(
            "external corpus: {simulation_metadata_count} type-52 assets, {nonempty_wave_assets} with waves, {active_net_lanes} active lanes, {reserved_net_lanes} with nonzero reservation state, {active_net_nodes} active nodes, {nodes_with_problems} with problems, {nodes_with_counters} with counters, {nodes_with_segments} with segments, {nodes_with_event_routes} with event routes, {active_net_segments} active segments, {segments_with_secondary_flags} with secondary flags, {segments_with_problems} with problems, {segments_with_traffic_lights} with traffic-light state, {segments_with_fire_coverage} with fire coverage, {wet_segments} wet, {segments_with_condition} with condition, {segments_with_noise_buffer} with noise buffer, {segments_with_noise_density} with noise density, {nonzero_guide_counts} with guide references, {nonzero_milestone_counts} with milestone references, {info_prefab_names} info prefab names, {tree_prefab_names} tree prefab names, {active_buildings} active buildings, {buildings_with_secondary_flags} with secondary flags, {building_guide_refs} building guide references, {building_milestone_refs} building milestone references, {building_prefab_names} building prefab names, {building_object_headers} BuildingManager object headers"
        );
        Ok(())
    }
}
