use crate::CrpError;
use crate::data_serializer::{Reader, write_unique_string};
use std::collections::{HashMap, HashSet};

const GROUPS_VERSION: u32 = 258;
const MAX_ENTRIES: u32 = 1_000_000;

/// One `InstanceID` and its optional name in `InstanceManager+Data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceName {
    /// The raw 32-bit instance ID used by the game.
    pub id: u32,
    /// The stored name; `None` represents DataSerializer's null string marker.
    pub name: Option<String>,
}

/// The fourth queued manager body, excluding its loading-profiler tags.
///
/// `encode` preserves the original bytes. Use `into_editor` to change names;
/// group references and their queued object bodies remain opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceManagerData {
    format_version: u32,
    names: Vec<InstanceName>,
    name_entry_bytes: Vec<Vec<u8>>,
    group_count: Option<u32>,
    group_references: Vec<Vec<u8>>,
    serialized_body: Vec<u8>,
}

/// Editable names with the original group references retained in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceManagerEditor {
    format_version: u32,
    names: Vec<InstanceName>,
    original_names: Vec<InstanceName>,
    original_name_entry_bytes: Vec<Vec<u8>>,
    original_body: Vec<u8>,
    group_references: Vec<Vec<u8>>,
}

impl InstanceManagerData {
    /// Decodes the body for inner versions 164 and newer, returning bytes used.
    /// Truncation, excessive counts, and malformed strings return errors.
    /// Versions before 258 have no group count or references.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        if format_version < 164 {
            return Err(invalid(
                0,
                "InstanceManager requires inner version 164 or newer",
            ));
        }
        let mut reader = Reader::new(bytes);
        let name_count = usize::try_from(read_count(&mut reader, "name")?)
            .map_err(|_| reader.invalid("name count does not fit this platform"))?;
        if name_count > reader.remaining() / 5 {
            return Err(reader.invalid("InstanceManager names cannot fit in remaining bytes"));
        }
        let mut names = Vec::with_capacity(name_count);
        let mut name_entry_bytes = Vec::with_capacity(name_count);
        let mut name_ids = HashSet::with_capacity(name_count);
        for _ in 0..name_count {
            let start = reader.offset();
            let id = reader.read_u32_be()?;
            if !name_ids.insert(id) {
                return Err(reader.invalid("duplicate InstanceManager name ID"));
            }
            let name = reader.read_unique_string()?;
            names.push(InstanceName { id, name });
            name_entry_bytes.push(reader.consumed_bytes()[start..].to_vec());
        }
        let group_count = if format_version >= GROUPS_VERSION {
            Some(read_count(&mut reader, "group")?)
        } else {
            None
        };
        let mut group_references = Vec::new();
        if let Some(count) = group_count {
            if usize::try_from(count).map_or(true, |count| count > reader.remaining() / 5) {
                return Err(reader.invalid("InstanceManager groups cannot fit in remaining bytes"));
            }
            group_references.reserve(
                usize::try_from(count)
                    .map_err(|_| reader.invalid("group count does not fit this platform"))?,
            );
            let mut group_ids = HashSet::with_capacity(
                usize::try_from(count)
                    .map_err(|_| reader.invalid("group count does not fit this platform"))?,
            );
            for _ in 0..count {
                let start = reader.offset();
                let id = reader.read_u32_be()?;
                if !group_ids.insert(id) {
                    return Err(reader.invalid("duplicate InstanceManager group ID"));
                }
                skip_object_reference(&mut reader)?;
                group_references.push(reader.consumed_bytes()[start..].to_vec());
            }
        }
        let consumed = reader.offset();
        Ok((
            Self {
                format_version,
                names,
                name_entry_bytes,
                group_count,
                group_references,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the stored names in dictionary serialization order.
    pub fn names(&self) -> &[InstanceName] {
        &self.names
    }

    /// Returns the stored group count, absent before inner version 258.
    pub fn group_count(&self) -> Option<u32> {
        self.group_count
    }

    /// Returns group IDs and their raw object-reference headers in field order.
    /// The first four bytes of each entry are the group ID.
    pub fn group_references(&self) -> &[Vec<u8>] {
        &self.group_references
    }

    /// Writes the original body byte for byte, including unknown references.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }

    /// Creates a writer for editing names while retaining group references.
    pub fn into_editor(self) -> InstanceManagerEditor {
        let original_names = self.names.clone();
        InstanceManagerEditor {
            format_version: self.format_version,
            names: self.names,
            original_names,
            original_name_entry_bytes: self.name_entry_bytes,
            original_body: self.serialized_body,
            group_references: self.group_references,
        }
    }
}

impl InstanceManagerEditor {
    /// Returns names for explicit edits. Instance IDs need not be decoded.
    pub fn names_mut(&mut self) -> &mut Vec<InstanceName> {
        &mut self.names
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    pub(crate) fn retains_groups(&self, original: &InstanceManagerData) -> bool {
        self.group_references == original.group_references
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original_body
    }

    /// Preserves unchanged name entries and group references byte for byte.
    /// Changed or new names use canonical string encoding. Invalid lengths
    /// return an error without changing the source graph.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        if self.names.len() > MAX_ENTRIES as usize
            || self.group_references.len() > MAX_ENTRIES as usize
        {
            return Err(invalid(0, "InstanceManager entry count exceeds 1,000,000"));
        }
        let name_count = u32::try_from(self.names.len())
            .map_err(|_| CrpError::ArithmeticOverflow("InstanceManager name count"))?;
        if self.original_names.len() != self.original_name_entry_bytes.len() {
            return Err(invalid(
                0,
                "InstanceManager source name entries are inconsistent",
            ));
        }
        if self.names == self.original_names {
            return Ok(self.original_body.clone());
        }
        let mut original_entries = HashMap::with_capacity(self.original_names.len());
        for (index, entry) in self.original_names.iter().enumerate() {
            if original_entries.insert(entry.id, index).is_some() {
                return Err(invalid(0, "duplicate InstanceManager source name ID"));
            }
        }
        let mut output = name_count.to_be_bytes().to_vec();
        let mut name_ids = HashSet::with_capacity(self.names.len());
        for entry in &self.names {
            if !name_ids.insert(entry.id) {
                return Err(invalid(output.len(), "duplicate InstanceManager name ID"));
            }
            if let Some(index) = original_entries.get(&entry.id).copied()
                && self.original_names[index] == *entry
            {
                output.extend_from_slice(&self.original_name_entry_bytes[index]);
            } else {
                output.extend_from_slice(&entry.id.to_be_bytes());
                if let Some(name) = &entry.name {
                    write_unique_string(&mut output, name)?;
                } else {
                    output.push(0xff);
                }
            }
        }
        if self.format_version >= GROUPS_VERSION {
            let count = u32::try_from(self.group_references.len())
                .map_err(|_| CrpError::ArithmeticOverflow("InstanceManager group count"))?;
            output.extend_from_slice(&count.to_be_bytes());
            for reference in &self.group_references {
                output.extend_from_slice(reference);
            }
        } else if !self.group_references.is_empty() {
            return Err(invalid(0, "group references require inner version 258"));
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::{InstanceManagerData, InstanceName};
    use crate::data_serializer::write_unique_string_with_mode;

    #[test]
    fn editor_preserves_unchanged_name_bytes_when_another_name_changes() {
        let mut source = 2_u32.to_be_bytes().to_vec();
        let first_entry_start = source.len();
        source.extend_from_slice(&1_u32.to_be_bytes());
        write_unique_string_with_mode(&mut source, "Wide", false).unwrap();
        let first_entry = source[first_entry_start..].to_vec();
        source.extend_from_slice(&2_u32.to_be_bytes());
        write_unique_string_with_mode(&mut source, "Compact", true).unwrap();
        source.extend_from_slice(&0_u32.to_be_bytes());

        let (data, consumed) = InstanceManagerData::decode_prefix(258, &source).unwrap();
        assert_eq!(consumed, source.len());
        assert_eq!(data.encode(), source);

        let mut editor = data.into_editor();
        assert_eq!(editor.encode().unwrap(), source);
        editor.names_mut()[1].name = Some("Changed".to_owned());

        let mut expected = 2_u32.to_be_bytes().to_vec();
        expected.extend_from_slice(&first_entry);
        expected.extend_from_slice(&2_u32.to_be_bytes());
        crate::data_serializer::write_unique_string(&mut expected, "Changed").unwrap();
        expected.extend_from_slice(&0_u32.to_be_bytes());
        assert_eq!(editor.encode().unwrap(), expected);
    }

    #[test]
    fn editor_preserves_unchanged_name_entry_when_names_are_reordered() {
        let mut source = 2_u32.to_be_bytes().to_vec();
        source.extend_from_slice(&10_u32.to_be_bytes());
        crate::data_serializer::write_unique_string(&mut source, "First").unwrap();
        let second_entry_start = source.len();
        source.extend_from_slice(&20_u32.to_be_bytes());
        write_unique_string_with_mode(&mut source, "Second", false).unwrap();
        let second_entry = source[second_entry_start..].to_vec();
        source.extend_from_slice(&0_u32.to_be_bytes());

        let (data, _) = InstanceManagerData::decode_prefix(258, &source).unwrap();
        let mut editor = data.into_editor();
        editor.names_mut().swap(0, 1);

        let mut expected = 2_u32.to_be_bytes().to_vec();
        expected.extend_from_slice(&second_entry);
        expected.extend_from_slice(&10_u32.to_be_bytes());
        crate::data_serializer::write_unique_string(&mut expected, "First").unwrap();
        expected.extend_from_slice(&0_u32.to_be_bytes());
        assert_eq!(editor.encode().unwrap(), expected);
    }

    #[test]
    fn changed_name_id_uses_canonical_encoding() {
        let mut source = 1_u32.to_be_bytes().to_vec();
        source.extend_from_slice(&10_u32.to_be_bytes());
        crate::data_serializer::write_unique_string(&mut source, "Original").unwrap();
        source.extend_from_slice(&0_u32.to_be_bytes());
        let (data, _) = InstanceManagerData::decode_prefix(258, &source).unwrap();
        let mut editor = data.into_editor();
        editor.names_mut()[0] = InstanceName {
            id: 11,
            name: Some("Replacement".to_owned()),
        };
        let mut expected = 1_u32.to_be_bytes().to_vec();
        expected.extend_from_slice(&11_u32.to_be_bytes());
        crate::data_serializer::write_unique_string(&mut expected, "Replacement").unwrap();
        expected.extend_from_slice(&0_u32.to_be_bytes());
        assert_eq!(editor.encode().unwrap(), expected);
    }
}

fn read_count(reader: &mut Reader<'_>, field: &str) -> Result<u32, CrpError> {
    let count = reader.read_u32_be()?;
    if count > MAX_ENTRIES {
        return Err(reader.invalid(format!("InstanceManager {field} count exceeds 1,000,000")));
    }
    Ok(count)
}

fn skip_object_reference(reader: &mut Reader<'_>) -> Result<(), CrpError> {
    match reader.read_u8()? {
        0xff => Ok(()),
        0xfe => {
            if reader.read_u8()? == 0xfe {
                reader
                    .read_unique_string()?
                    .ok_or_else(|| reader.invalid("group shared-type name cannot be null"))?;
            } else {
                reader.read_u8()?;
            }
            Ok(())
        }
        _ => {
            reader.read_bytes(2)?;
            Ok(())
        }
    }
}

fn invalid(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "InstanceManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}
