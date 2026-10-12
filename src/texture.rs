use crate::asset_io::{PayloadReader, write_bool, write_i32_le, write_string};
use crate::{Asset, CrpError};

const TEXTURE_ASSET_TYPE: i32 = 3;
const MIN_SUPPORTED_VERSION: u16 = 4;
const MAX_SUPPORTED_VERSION: u16 = 10;

/// Decoded type-3 image asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureAsset {
    /// Assembly-qualified image runtime type stored by the game.
    pub runtime_type: String,
    /// Texture name stored in the asset wrapper.
    pub name: String,
    /// Whether the image is treated as linear data.
    pub linear: bool,
    /// Anisotropic filtering level. Versions below 6 use the default value 1.
    pub aniso_level: i32,
    /// Encoded image bytes, usually PNG data in map assets.
    pub image_bytes: Vec<u8>,
    original_encoding: Option<OriginalTextureEncoding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OriginalTextureEncoding {
    runtime_type: String,
    runtime_type_bytes: Vec<u8>,
    name: String,
    name_bytes: Vec<u8>,
    linear: bool,
    linear_byte: u8,
}

impl TextureAsset {
    /// Creates a texture asset for explicit encoding.
    ///
    /// New values have no source field encodings to preserve. [`decode`](Self::decode)
    /// retains unchanged source strings and bool bytes when the asset is edited.
    pub fn new(
        runtime_type: String,
        name: String,
        linear: bool,
        aniso_level: i32,
        image_bytes: Vec<u8>,
    ) -> Self {
        Self {
            runtime_type,
            name,
            linear,
            aniso_level,
            image_bytes,
            original_encoding: None,
        }
    }
}

impl TextureAsset {
    /// Decodes an outer type-3 texture using the supplied package version.
    pub fn decode(asset: &Asset, package_version: u16) -> Result<Self, CrpError> {
        if asset.asset_type != TEXTURE_ASSET_TYPE {
            return Err(CrpError::AssetTypeMismatch {
                name: asset.name.clone(),
                expected: TEXTURE_ASSET_TYPE,
                actual: asset.asset_type,
            });
        }
        validate_version(package_version)?;

        let mut reader = PayloadReader::new(&asset.name, &asset.data);
        if reader.read_bool()? {
            return Err(reader.invalid("null texture root object is unsupported"));
        }
        let runtime_type_start = reader.offset();
        let runtime_type = reader.read_string()?;
        let runtime_type_bytes = asset.data[runtime_type_start..reader.offset()].to_vec();
        if runtime_type.split(',').next() != Some("ColossalFramework.Importers.Image") {
            return Err(
                reader.invalid(format!("unsupported texture runtime type {runtime_type:?}"))
            );
        }
        let name_start = reader.offset();
        let name = reader.read_string()?;
        let name_bytes = asset.data[name_start..reader.offset()].to_vec();
        let linear_byte = reader.read_u8()?;
        let linear = linear_byte != 0;
        let aniso_level = if package_version >= 6 {
            reader.read_i32_le()?
        } else {
            1
        };
        let image_len = reader.read_i32_le()?;
        if image_len < 0 {
            return Err(reader.invalid(format!("negative image byte count {image_len}")));
        }
        let image_len = usize::try_from(image_len)
            .map_err(|_| reader.invalid("image byte count does not fit this platform"))?;
        if image_len != reader.remaining() {
            return Err(reader.invalid(format!(
                "image byte count is {image_len}, but {} payload bytes remain",
                reader.remaining()
            )));
        }
        let image_bytes = reader.read_bytes(image_len)?.to_vec();
        let original_encoding = OriginalTextureEncoding {
            runtime_type: runtime_type.clone(),
            runtime_type_bytes,
            name: name.clone(),
            name_bytes,
            linear,
            linear_byte,
        };

        Ok(Self {
            runtime_type,
            name,
            linear,
            aniso_level,
            image_bytes,
            original_encoding: Some(original_encoding),
        })
    }

    /// Encodes the texture wrapper for the supplied outer package version.
    ///
    /// Unchanged source strings retain their original encoded bytes, and an
    /// unchanged `linear` value retains its original bool byte. Changed strings
    /// and bools use canonical encodings. Encoding before outer version 6 fails
    /// if `aniso_level` is not the default value 1, because those versions have
    /// no field in which to store it.
    pub fn encode(&self, package_version: u16) -> Result<Vec<u8>, CrpError> {
        validate_version(package_version)?;
        if package_version < 6 && self.aniso_level != 1 {
            return Err(CrpError::InvalidAssetPayload {
                name: self.name.clone(),
                offset: 0,
                reason: "non-default anisotropic level cannot be encoded before outer version 6"
                    .to_owned(),
            });
        }
        let image_len = i32::try_from(self.image_bytes.len())
            .map_err(|_| CrpError::ArithmeticOverflow("texture image byte count"))?;

        let mut output = Vec::new();
        write_bool(&mut output, false);
        if let Some(original) = &self.original_encoding {
            write_string_preserving_source(
                &mut output,
                &self.runtime_type,
                &original.runtime_type,
                &original.runtime_type_bytes,
            )?;
            write_string_preserving_source(
                &mut output,
                &self.name,
                &original.name,
                &original.name_bytes,
            )?;
            if self.linear == original.linear {
                output.push(original.linear_byte);
            } else {
                write_bool(&mut output, self.linear);
            }
        } else {
            write_string(&mut output, &self.runtime_type)?;
            write_string(&mut output, &self.name)?;
            write_bool(&mut output, self.linear);
        }
        if package_version >= 6 {
            write_i32_le(&mut output, self.aniso_level);
        }
        write_i32_le(&mut output, image_len);
        output.extend_from_slice(&self.image_bytes);
        Ok(output)
    }
}

fn write_string_preserving_source(
    output: &mut Vec<u8>,
    value: &str,
    original_value: &str,
    original_bytes: &[u8],
) -> Result<(), CrpError> {
    if value == original_value {
        output.extend_from_slice(original_bytes);
        Ok(())
    } else {
        write_string(output, value)
    }
}

fn validate_version(version: u16) -> Result<(), CrpError> {
    if !(MIN_SUPPORTED_VERSION..=MAX_SUPPORTED_VERSION).contains(&version) {
        return Err(CrpError::UnsupportedVersion(version));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::TextureAsset;
    use crate::Asset;

    const RUNTIME_TYPE: &str = "ColossalFramework.Importers.Image, Test";

    fn append_string(output: &mut Vec<u8>, value: &str, overlong_length: bool) {
        let length = u8::try_from(value.len()).unwrap();
        if overlong_length {
            output.extend_from_slice(&[length | 0x80, 0]);
        } else {
            output.push(length);
        }
        output.extend_from_slice(value.as_bytes());
    }

    fn asset_with_noncanonical_fields() -> Asset {
        let mut data = vec![0];
        append_string(&mut data, RUNTIME_TYPE, false);
        append_string(&mut data, "A", true);
        data.push(2); // BinaryReader treats any nonzero bool byte as true.
        data.extend_from_slice(&3_i32.to_le_bytes());
        data.extend_from_slice(&2_i32.to_le_bytes());
        data.extend_from_slice(&[0xaa, 0xbb]);
        Asset {
            name: "texture".to_owned(),
            checksum: String::new(),
            asset_type: 3,
            relative_offset: 0,
            data,
        }
    }

    #[test]
    fn texture_preserves_unchanged_source_fields_during_other_edits() {
        let asset = asset_with_noncanonical_fields();
        let mut texture = TextureAsset::decode(&asset, 6).expect("valid texture payload");
        assert_eq!(texture.runtime_type, RUNTIME_TYPE);
        assert_eq!(texture.name, "A");
        assert!(texture.linear);
        assert_eq!(texture.aniso_level, 3);
        assert_eq!(texture.image_bytes, [0xaa, 0xbb]);
        assert_eq!(texture.encode(6).expect("unchanged encoding"), asset.data);

        texture.name = "B".to_owned();
        let mut expected = vec![0];
        append_string(&mut expected, RUNTIME_TYPE, false);
        append_string(&mut expected, "B", false);
        expected.push(2);
        expected.extend_from_slice(&3_i32.to_le_bytes());
        expected.extend_from_slice(&2_i32.to_le_bytes());
        expected.extend_from_slice(&[0xaa, 0xbb]);
        assert_eq!(texture.encode(6).expect("name edit encoding"), expected);

        texture.linear = false;
        let linear_offset = expected.len() - 11;
        expected[linear_offset] = 0;
        assert_eq!(texture.encode(6).expect("bool edit encoding"), expected);
    }

    #[test]
    fn texture_rejects_anisotropic_data_that_old_versions_cannot_store() {
        let texture = TextureAsset::new(
            RUNTIME_TYPE.to_owned(),
            "texture".to_owned(),
            false,
            4,
            vec![0xaa],
        );
        assert!(texture.encode(5).is_err());
        assert!(texture.encode(6).is_ok());
    }
}
