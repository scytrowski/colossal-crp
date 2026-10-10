use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{
    EncodedUIntArrayReader, read_encoded_byte_array, read_encoded_ushort_array,
};

const CITIZEN_FLAG_COUNT: usize = 1_048_575;
const BYTE_CITIZEN_FLAGS_VERSION: u32 = 30;
const LEGACY_CITIZEN_COLUMNS_VERSION: u32 = 23;
const CITIZEN_HEALTH_VERSION: u32 = 65;
const CITIZEN_AGE_VERSION: u32 = 157;
const CITIZEN_AGE_USHORT_VERSION: u32 = 113_007;
const CITIZEN_FAMILY_VERSION: u32 = 190;
const CITIZEN_JOB_TITLE_VERSION: u32 = 112_007;
const CITIZEN_TOURIST_TYPE_VERSION: u32 = 117_002;

/// The decoded flag and sparse-column prefix of `CitizenManager+Data`.
///
/// The prefix ends before the fixed-capacity citizen-unit and citizen-instance
/// arrays. Those later fields remain in the serialized manager graph and are
/// not decoded here. The exact consumed source bytes are retained for unchanged
/// writing. Reader gates come from `CitizenManager+Data.Deserialize` in the
/// supplied Assembly-CSharp.dll; the local map corpus has no active citizens,
/// so nonzero values have synthetic test coverage only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitizenManagerDataPrefix {
    format_version: u32,
    citizen_flags: Vec<u32>,
    legacy_columns: Option<Vec<Vec<u8>>>,
    health: Option<Vec<u8>>,
    wellbeing: Option<Vec<u8>>,
    age: Option<Vec<u16>>,
    family: Option<Vec<u8>>,
    job_title: Option<Vec<u8>>,
    tourist_type: Option<Vec<u8>>,
    serialized_prefix: Vec<u8>,
}

impl CitizenManagerDataPrefix {
    /// Decodes citizen flags and the versioned sparse citizen columns at the
    /// start of the manager body, returning the byte count before unit data.
    ///
    /// Versions below 30 use byte flags; version 30 and later use unsigned
    /// 32-bit flags. Sparse values are stored in ascending citizen-index order
    /// for each nonzero flag. Truncated or malformed arrays return an error.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        Self::decode_prefix_with_count(format_version, CITIZEN_FLAG_COUNT, bytes)
    }

    fn decode_prefix_with_count(
        format_version: u32,
        flag_count: usize,
        bytes: &[u8],
    ) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let citizen_flags = if format_version < BYTE_CITIZEN_FLAGS_VERSION {
            read_encoded_byte_array(&mut reader, flag_count)?
                .into_iter()
                .map(u32::from)
                .collect()
        } else {
            EncodedUIntArrayReader::default().read_array(&mut reader, flag_count)?
        };
        let active_count = citizen_flags.iter().filter(|flags| **flags != 0).count();

        let legacy_columns = if format_version < LEGACY_CITIZEN_COLUMNS_VERSION {
            Some(vec![read_encoded_byte_array(&mut reader, active_count)?])
        } else if format_version < BYTE_CITIZEN_FLAGS_VERSION {
            let mut columns = Vec::with_capacity(4);
            for _ in 0..4 {
                columns.push(read_encoded_byte_array(&mut reader, active_count)?);
            }
            Some(columns)
        } else {
            None
        };

        let (health, wellbeing) = if format_version >= CITIZEN_HEALTH_VERSION {
            (
                Some(read_encoded_byte_array(&mut reader, active_count)?),
                Some(read_encoded_byte_array(&mut reader, active_count)?),
            )
        } else {
            (None, None)
        };
        let age = if format_version >= CITIZEN_AGE_USHORT_VERSION {
            Some(read_encoded_ushort_array(&mut reader, active_count)?)
        } else if format_version >= CITIZEN_AGE_VERSION {
            Some(
                read_encoded_byte_array(&mut reader, active_count)?
                    .into_iter()
                    .map(u16::from)
                    .collect(),
            )
        } else {
            None
        };
        let family = if format_version >= CITIZEN_FAMILY_VERSION {
            Some(read_encoded_byte_array(&mut reader, active_count)?)
        } else {
            None
        };
        let job_title = if format_version >= CITIZEN_JOB_TITLE_VERSION {
            Some(read_encoded_byte_array(&mut reader, active_count)?)
        } else {
            None
        };
        let tourist_type = if format_version >= CITIZEN_TOURIST_TYPE_VERSION {
            Some(read_encoded_byte_array(&mut reader, active_count)?)
        } else {
            None
        };

        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                citizen_flags,
                legacy_columns,
                health,
                wellbeing,
                age,
                family,
                job_title,
                tourist_type,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version used for the reader gates.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Returns flags for citizen slots 1 through 1,048,575.
    ///
    /// Slots with a nonzero value have sparse-column entries. The vector's
    /// index is the serialized array index minus one.
    pub fn citizen_flags(&self) -> &[u32] {
        &self.citizen_flags
    }

    /// Returns one column before version 23 or four columns in versions 23–29.
    /// Their game meanings are not established by the inspected reader.
    pub fn legacy_columns(&self) -> Option<&[Vec<u8>]> {
        self.legacy_columns.as_deref()
    }

    /// Returns the sparse health values from version 65.
    pub fn health(&self) -> Option<&[u8]> {
        self.health.as_deref()
    }

    /// Returns the sparse wellbeing values from version 65.
    pub fn wellbeing(&self) -> Option<&[u8]> {
        self.wellbeing.as_deref()
    }

    /// Returns sparse age values from version 157, widened to `u16` for a
    /// consistent API. The serialized width changes at version 113007.
    pub fn age(&self) -> Option<&[u16]> {
        self.age.as_deref()
    }

    /// Returns the sparse family values from version 190.
    pub fn family(&self) -> Option<&[u8]> {
        self.family.as_deref()
    }

    /// Returns sparse job-title indices from version 112007.
    pub fn job_title(&self) -> Option<&[u8]> {
        self.job_title.as_deref()
    }

    /// Returns sparse tourist types from version 117002.
    pub fn tourist_type(&self) -> Option<&[u8]> {
        self.tourist_type.as_deref()
    }

    /// Returns the exact source bytes consumed by this prefix decoder.
    pub fn original_prefix(&self) -> &[u8] {
        &self.serialized_prefix
    }

    /// Reproduces this prefix unchanged. Sparse values are not editable yet.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::CitizenManagerDataPrefix;

    fn byte_array(values: &[u8]) -> Vec<u8> {
        let mut bytes = vec![(values.len() - 1) as u8];
        bytes.extend_from_slice(values);
        bytes
    }

    fn uint_array(values: &[u32]) -> Vec<u8> {
        let mut bytes = vec![(values.len() - 1) as u8];
        for value in values {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes
    }

    #[test]
    fn decodes_flag_width_and_legacy_sparse_column_gates() {
        let old_flags = [2, 1, 0, 0];
        let mut v22 = old_flags.to_vec();
        v22.extend_from_slice(&byte_array(&[0x55]));
        v22.push(0xaa); // first byte after the decoded prefix
        let (old, consumed) = CitizenManagerDataPrefix::decode_prefix_with_count(22, 3, &v22)
            .expect("v22 citizen prefix");
        assert_eq!(old.citizen_flags(), [1, 0, 0]);
        assert_eq!(old.legacy_columns(), Some([vec![0x55]].as_slice()));
        assert_eq!(consumed, v22.len() - 1);
        assert_eq!(old.encode(), v22[..consumed]);

        let mut v23 = old_flags.to_vec();
        for value in [0x11, 0x22, 0x33, 0x44] {
            v23.extend_from_slice(&byte_array(&[value]));
        }
        let (middle, middle_consumed) =
            CitizenManagerDataPrefix::decode_prefix_with_count(23, 3, &v23)
                .expect("v23 citizen prefix");
        assert_eq!(
            middle.legacy_columns(),
            Some([vec![0x11], vec![0x22], vec![0x33], vec![0x44]].as_slice())
        );
        assert_eq!(middle_consumed, v23.len());

        let v30 = uint_array(&[1, 0, 0]);
        let (modern, modern_consumed) =
            CitizenManagerDataPrefix::decode_prefix_with_count(30, 3, &v30)
                .expect("v30 citizen prefix");
        assert_eq!(modern.citizen_flags(), [1, 0, 0]);
        assert_eq!(modern.legacy_columns(), None);
        assert_eq!(modern_consumed, v30.len());
    }

    #[test]
    fn decodes_active_citizen_columns_across_age_and_later_gates() {
        for (version, age_bytes, expected_age, has_tourist_type) in [
            (113_006, vec![0, 0x7f], vec![127], false),
            (113_007, vec![0, 0x01, 0x23], vec![0x0123], false),
            (117_002, vec![0, 0, 0x7f], vec![127], true),
        ] {
            let mut bytes = uint_array(&[1, 0, 0]);
            bytes.extend_from_slice(&byte_array(&[0x21])); // health
            bytes.extend_from_slice(&byte_array(&[0x32])); // wellbeing
            bytes.extend_from_slice(&age_bytes);
            bytes.extend_from_slice(&byte_array(&[0x43])); // family
            bytes.extend_from_slice(&byte_array(&[0x54])); // job title
            if has_tourist_type {
                bytes.extend_from_slice(&byte_array(&[0x65]));
            }
            bytes.extend_from_slice(&[0xfe, 0xed]); // next manager data

            let (decoded, consumed) =
                CitizenManagerDataPrefix::decode_prefix_with_count(version, 3, &bytes)
                    .expect("active citizen prefix");
            assert_eq!(decoded.citizen_flags(), [1, 0, 0]);
            assert_eq!(decoded.health(), Some([0x21].as_slice()));
            assert_eq!(decoded.wellbeing(), Some([0x32].as_slice()));
            assert_eq!(decoded.age(), Some(expected_age.as_slice()));
            assert_eq!(decoded.family(), Some([0x43].as_slice()));
            assert_eq!(decoded.job_title(), Some([0x54].as_slice()));
            assert_eq!(decoded.tourist_type().is_some(), has_tourist_type);
            assert_eq!(decoded.encode(), bytes[..consumed]);
            assert_eq!(bytes[consumed..], [0xfe, 0xed]);
        }
    }

    #[test]
    fn rejects_truncated_flag_and_sparse_columns() {
        assert!(CitizenManagerDataPrefix::decode_prefix_with_count(30, 3, &[0, 0, 0]).is_err());
        let mut missing_sparse_value = uint_array(&[1, 0, 0]);
        missing_sparse_value.extend_from_slice(&[0]); // health array advertises one byte
        assert!(
            CitizenManagerDataPrefix::decode_prefix_with_count(65, 3, &missing_sparse_value)
                .is_err()
        );
    }
}
