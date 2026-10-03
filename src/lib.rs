//! Read the outer container structure of City: Skylines (2015) `.crp` files.
//!
//! This initial reader supports the common package index observed in format
//! versions 4 through 10. Asset payloads are returned as raw bytes.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

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
    /// The package index exceeds the declared data start.
    IndexPastDataStart { index_end: u64, data_start: u64 },
    /// An asset's data range extends past the end of the input.
    AssetOutOfBounds {
        name: String,
        offset: u64,
        size: u64,
        file_size: u64,
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
                "asset {name:?} range {offset}..{} exceeds file size {file_size}",
                offset.saturating_add(*size)
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
    /// This covers the common map files currently examined. Other package
    /// implementations may add a separate implementation-specific data offset.
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
        if data_start_offset > file_size {
            return Err(CrpError::IndexPastDataStart {
                index_end: index_start,
                data_start: data_start_offset,
            });
        }
        let index_bytes_available = data_start_offset.saturating_sub(index_start);
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
