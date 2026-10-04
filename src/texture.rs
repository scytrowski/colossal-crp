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
        let runtime_type = reader.read_string()?;
        if runtime_type.split(',').next() != Some("ColossalFramework.Importers.Image") {
            return Err(
                reader.invalid(format!("unsupported texture runtime type {runtime_type:?}"))
            );
        }
        let name = reader.read_string()?;
        let linear = reader.read_bool()?;
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

        Ok(Self {
            runtime_type,
            name,
            linear,
            aniso_level,
            image_bytes,
        })
    }

    /// Encodes the texture wrapper for the supplied outer package version.
    pub fn encode(&self, package_version: u16) -> Result<Vec<u8>, CrpError> {
        validate_version(package_version)?;
        let image_len = i32::try_from(self.image_bytes.len())
            .map_err(|_| CrpError::ArithmeticOverflow("texture image byte count"))?;

        let mut output = Vec::new();
        write_bool(&mut output, false);
        write_string(&mut output, &self.runtime_type)?;
        write_string(&mut output, &self.name)?;
        write_bool(&mut output, self.linear);
        if package_version >= 6 {
            write_i32_le(&mut output, self.aniso_level);
        }
        write_i32_le(&mut output, image_len);
        output.extend_from_slice(&self.image_bytes);
        Ok(output)
    }
}

fn validate_version(version: u16) -> Result<(), CrpError> {
    if !(MIN_SUPPORTED_VERSION..=MAX_SUPPORTED_VERSION).contains(&version) {
        return Err(CrpError::UnsupportedVersion(version));
    }
    Ok(())
}
