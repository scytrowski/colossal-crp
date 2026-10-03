//! Read the outer container structure of City: Skylines (2015) `.crp` files.
//!
//! This initial reader supports the common package index observed in format
//! versions 4 through 10. Asset payloads are returned as raw bytes.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom, Write};

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
        writer.write_all(&[((value as u8) & 0x7F) | 0x80])?;
        value >>= 7;
    }
    writer.write_all(&[value as u8])?;
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
    fn compact_writer_round_trips_metadata_and_unknown_assets() -> Result<(), CrpError> {
        let package = sample_package();
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
        let size_offset = cursor.position() as usize;
        encoded[size_offset..size_offset + 8].copy_from_slice(&i64::MAX.to_le_bytes());

        let mut cursor = Cursor::new(encoded);
        assert!(matches!(
            Package::read_from(&mut cursor),
            Err(CrpError::AssetOutOfBounds { .. })
        ));
        Ok(())
    }

    #[test]
    fn wolf_creek_round_trips_with_matching_payload_checksums() -> Result<(), CrpError> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/maps/CL Wolf Creek.crp");
        let mut source = File::open(path)?;
        let original = Package::read_from(&mut source)?;

        let mut encoded = Vec::new();
        original.write_compact_to(&mut encoded)?;
        let mut cursor = Cursor::new(encoded);
        let rewritten = Package::read_from(&mut cursor)?;

        assert_eq!(rewritten.name, original.name);
        assert_eq!(rewritten.main_asset_name, original.main_asset_name);
        assert_eq!(rewritten.assets.len(), original.assets.len());
        for (before, after) in original.assets.iter().zip(&rewritten.assets) {
            assert_eq!(before.name, after.name);
            assert_eq!(before.asset_type, after.asset_type);
            assert_eq!(before.data, after.data);
            assert_eq!(before.checksum, after.checksum);
        }
        Ok(())
    }

    #[test]
    fn reads_all_supplied_map_containers() -> Result<(), CrpError> {
        let maps_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/maps");
        let mut paths = Vec::new();
        for entry in fs::read_dir(maps_dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|extension| extension == "crp") {
                paths.push(path);
            }
        }
        paths.sort();
        assert_eq!(paths.len(), 105, "unexpected supplied map corpus size");

        for path in paths {
            let mut file = File::open(&path)?;
            let package = Package::read_from(&mut file)?;
            assert!(
                !package.assets.is_empty(),
                "{} has no assets",
                path.display()
            );
        }
        Ok(())
    }
}
