//! Read the outer container structure of City: Skylines (2015) `.crp` files.
//!
//! This reader supports the common package index observed in format versions
//! 4 through 10. It verifies each asset's stored MD5 and returns raw payloads.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom, Write};

mod asset_io;
mod data_serializer;
mod encoded_array;
mod game_area_data;
mod metadata;
mod simulation_manager_data;
mod simulation_metadata;
mod terrain_manager_data;
mod texture;
mod water_simulation_data;

pub use data_serializer::DataSerializerStream;
pub use game_area_data::{GameAreaManagerData, SavedCameraView};
pub use metadata::{MapMetaData, ModInfo};
pub use simulation_manager_data::{
    ManagerObjectReference, SerializableDataEntry, SimulationManagerData,
};
pub use simulation_metadata::{ModOverride, SimulationMetaData};
pub use terrain_manager_data::TerrainManagerData;
pub use texture::TextureAsset;
pub use water_simulation_data::{WaterSimulationData, WaterSimulationEditor, WaterSource};

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
        Ok(())
    }
}
