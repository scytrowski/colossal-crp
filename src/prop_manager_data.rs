use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_ushort_array, write_encoded_ushort_array};
use std::ops::Range;

const PROP_INSTANCE_COUNT: usize = 65_535;
const LOGO_TABLE_VERSION: u32 = 121_026;

/// Known fields from `PropManager+Data`, excluding its loading-profiler tags.
///
/// The decoder supports the legacy layout below inner version 121026 and the
/// logo-table layout from version 121026. It retains the exact known bytes for
/// lossless rewriting. The editor can change raw flag bits while preserving
/// active slots and logo-scope selection, along with active-prop columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropManagerDataPrefix {
    format_version: u32,
    flags: Vec<u16>,
    flags_range: Range<usize>,
    prefab_indices: Vec<u16>,
    prefab_indices_range: Range<usize>,
    prefab_names: Vec<Option<String>>,
    position_x: Vec<i16>,
    position_x_range: Range<usize>,
    position_z: Vec<i16>,
    position_z_range: Range<usize>,
    angles: Vec<u16>,
    angles_range: Range<usize>,
    race_logo_indices: Vec<u16>,
    race_logo_indices_range: Option<Range<usize>>,
    race_logo_names: Vec<Option<String>>,
    event_logo_indices: Vec<u16>,
    event_logo_indices_range: Option<Range<usize>>,
    event_logo_names: Vec<Option<String>>,
    logo_tables_present: bool,
    serialized_prefix: Vec<u8>,
}

/// Editor for existing active PropManager prefab assignments and transform columns.
///
/// Raw Prop flags can change while preserving slot activity and logo-scope
/// selection. Changed encoded arrays use canonical `EncodedArray.UShort`
/// encoding; unchanged columns and all other bytes retain their original
/// representation. From version 121026, existing race-team and event logo
/// assignments can also be edited without changing either name table.
/// The source layout may be legacy or include the version-121026 logo tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropManagerEditor {
    data: PropManagerDataPrefix,
    original_flags: Vec<u16>,
    original_prefab_indices: Vec<u16>,
    original_position_x: Vec<i16>,
    original_position_z: Vec<i16>,
    original_angles: Vec<u16>,
    original_race_logo_indices: Vec<u16>,
    original_event_logo_indices: Vec<u16>,
}

impl PropManagerDataPrefix {
    /// Decodes the known PropManager fields after its opening profiler tag.
    ///
    /// Returns the consumed length before the closing profiler tag. Versions
    /// below 121026 omit both logo scopes; versions 121026 and newer include
    /// them. All 65,535 serialized flag positions are decoded. Truncated or
    /// malformed fields return an error.
    pub fn decode_prefix(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let flags_start = reader.offset();
        let flags = read_encoded_ushort_array(&mut reader, PROP_INSTANCE_COUNT)?;
        let flags_range = flags_start..reader.offset();
        let active_count = flags.iter().filter(|flags| **flags != 0).count();
        let prefab_indices_start = reader.offset();
        let prefab_indices = read_encoded_ushort_array(&mut reader, active_count)?;
        let prefab_indices_range = prefab_indices_start..reader.offset();
        let prefab_names = read_prefab_names(&mut reader)?;
        let position_x_start = reader.offset();
        let position_x = read_encoded_ushort_array(&mut reader, active_count)?
            .into_iter()
            .map(|value| value as i16)
            .collect();
        let position_x_range = position_x_start..reader.offset();
        let position_z_start = reader.offset();
        let position_z = read_encoded_ushort_array(&mut reader, active_count)?
            .into_iter()
            .map(|value| value as i16)
            .collect();
        let position_z_range = position_z_start..reader.offset();
        let angles_start = reader.offset();
        let angles = read_encoded_ushort_array(&mut reader, active_count)?;
        let angles_range = angles_start..reader.offset();

        let logo_tables_present = format_version >= LOGO_TABLE_VERSION;
        let (
            race_logo_indices,
            race_logo_indices_range,
            race_logo_names,
            event_logo_indices,
            event_logo_indices_range,
            event_logo_names,
        ) = if logo_tables_present {
            let race_count = flags
                .iter()
                .filter(|flags| **flags & 0x0300 == 0x0100)
                .count();
            let event_count = flags
                .iter()
                .filter(|flags| **flags & 0x0300 == 0x0200)
                .count();
            let race_start = reader.offset();
            let race_indices = read_encoded_ushort_array(&mut reader, race_count)?;
            let race_range = race_start..reader.offset();
            let race_names = read_prefab_names(&mut reader)?;
            let event_start = reader.offset();
            let event_indices = read_encoded_ushort_array(&mut reader, event_count)?;
            let event_range = event_start..reader.offset();
            (
                race_indices,
                Some(race_range),
                race_names,
                event_indices,
                Some(event_range),
                read_prefab_names(&mut reader)?,
            )
        } else {
            (Vec::new(), None, Vec::new(), Vec::new(), None, Vec::new())
        };
        let consumed = reader.offset();

        Ok((
            Self {
                format_version,
                flags,
                flags_range,
                prefab_indices,
                prefab_indices_range,
                prefab_names,
                position_x,
                position_x_range,
                position_z,
                position_z_range,
                angles,
                angles_range,
                race_logo_indices,
                race_logo_indices_range,
                race_logo_names,
                event_logo_indices,
                event_logo_indices_range,
                event_logo_names,
                logo_tables_present,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Creates an editor for existing active-prop assignments and transforms.
    pub fn into_editor(self) -> PropManagerEditor {
        PropManagerEditor {
            original_flags: self.flags.clone(),
            original_prefab_indices: self.prefab_indices.clone(),
            original_position_x: self.position_x.clone(),
            original_position_z: self.position_z.clone(),
            original_angles: self.angles.clone(),
            original_race_logo_indices: self.race_logo_indices.clone(),
            original_event_logo_indices: self.event_logo_indices.clone(),
            data: self,
        }
    }

    /// Returns all 65,535 serialized flags for one-based PropInstance slots.
    pub fn flags(&self) -> &[u16] {
        &self.flags
    }

    /// Returns PropInfo indices in active-prop buffer order.
    pub fn prefab_indices(&self) -> &[u16] {
        &self.prefab_indices
    }

    /// Returns PropInfo names in their original prefab-table order.
    pub fn prefab_names(&self) -> &[Option<String>] {
        &self.prefab_names
    }

    /// Returns signed X coordinates in active-prop buffer order.
    pub fn position_x(&self) -> &[i16] {
        &self.position_x
    }

    /// Returns signed Z coordinates in active-prop buffer order.
    pub fn position_z(&self) -> &[i16] {
        &self.position_z
    }

    /// Returns angle values in active-prop buffer order.
    pub fn angles(&self) -> &[u16] {
        &self.angles
    }

    /// Returns whether the versioned race-team and event logo scopes exist.
    pub fn logo_tables_present(&self) -> bool {
        self.logo_tables_present
    }

    /// Returns race-team logo indices in the order selected by PropInstance flags.
    pub fn race_logo_indices(&self) -> &[u16] {
        &self.race_logo_indices
    }

    /// Returns race-team logo names in their original prefab-table order.
    pub fn race_logo_names(&self) -> &[Option<String>] {
        &self.race_logo_names
    }

    /// Returns event logo indices in the order selected by PropInstance flags.
    pub fn event_logo_indices(&self) -> &[u16] {
        &self.event_logo_indices
    }

    /// Returns event logo names in their original prefab-table order.
    pub fn event_logo_names(&self) -> &[Option<String>] {
        &self.event_logo_names
    }

    /// Writes the original known PropManager prefix exactly.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }
}

impl PropManagerEditor {
    /// Returns the inner DataSerializer version.
    pub fn format_version(&self) -> u32 {
        self.data.format_version
    }

    /// Replaces raw flag bits for an existing PropInstance slot.
    ///
    /// `index` is zero-based, so zero represents serialized slot one. The
    /// value must preserve the slot's zero/nonzero activity state. From
    /// version 121026 onward, it must also preserve the `0x0300` logo-kind
    /// bits so the race-team and event-logo tables keep their existing
    /// lengths. Other flag meanings are not modeled.
    pub fn set_flag_bits(&mut self, index: usize, value: u16) -> Result<(), CrpError> {
        let current = *self.data.flags.get(index).ok_or_else(|| {
            invalid_prop_manager(index, "PropInstance flag index is out of range")
        })?;
        if (current == 0) != (value == 0) {
            return Err(invalid_prop_manager(
                index,
                "PropInstance flag edits must preserve the slot's active state",
            ));
        }
        if self.data.logo_tables_present && current & 0x0300 != value & 0x0300 {
            return Err(invalid_prop_manager(
                index,
                "PropInstance flag edits must preserve the logo-kind bits",
            ));
        }
        self.data.flags[index] = value;
        Ok(())
    }

    /// Returns mutable PropInfo assignments in active-prop order.
    pub fn prefab_indices_mut(&mut self) -> &mut [u16] {
        &mut self.data.prefab_indices
    }

    /// Returns mutable signed X coordinates in active-prop order.
    pub fn position_x_mut(&mut self) -> &mut [i16] {
        &mut self.data.position_x
    }

    /// Returns mutable signed Z coordinates in active-prop order.
    pub fn position_z_mut(&mut self) -> &mut [i16] {
        &mut self.data.position_z
    }

    /// Returns mutable angle values in active-prop order.
    pub fn angles_mut(&mut self) -> &mut [u16] {
        &mut self.data.angles
    }

    /// Returns mutable race-team logo assignments in PropInstance flag order.
    /// The slice is empty when the version-121026 logo tables are absent.
    pub fn race_logo_indices_mut(&mut self) -> &mut [u16] {
        &mut self.data.race_logo_indices
    }

    /// Returns mutable event logo assignments in PropInstance flag order.
    /// The slice is empty when the version-121026 logo tables are absent.
    pub fn event_logo_indices_mut(&mut self) -> &mut [u16] {
        &mut self.data.event_logo_indices
    }

    /// Writes edited columns while retaining all unedited source bytes.
    /// Returns an error if the source column ranges or record counts are inconsistent.
    pub fn encode(&self) -> Result<Vec<u8>, CrpError> {
        let count = self.original_prefab_indices.len();
        if self.data.flags.len() != PROP_INSTANCE_COUNT
            || self.original_flags.len() != PROP_INSTANCE_COUNT
            || self.data.flags.len() != self.original_flags.len()
            || self.data.prefab_indices.len() != count
            || self.data.position_x.len() != count
            || self.data.position_z.len() != count
            || self.data.angles.len() != count
            || self.original_position_x.len() != count
            || self.original_position_z.len() != count
            || self.original_angles.len() != count
            || self.data.race_logo_indices.len() != self.original_race_logo_indices.len()
            || self.data.event_logo_indices.len() != self.original_event_logo_indices.len()
        {
            return Err(invalid_prop_manager(
                0,
                "active-prop column lengths do not match the source",
            ));
        }

        let mut bytes = self.data.serialized_prefix.clone();
        let mut replacements = Vec::new();
        if self.data.flags != self.original_flags {
            replacements.push(encode_ushort_column(
                &bytes,
                &self.data.flags_range,
                &self.data.flags,
                "flags",
            )?);
        }
        if self.data.prefab_indices != self.original_prefab_indices {
            replacements.push(encode_ushort_column(
                &bytes,
                &self.data.prefab_indices_range,
                &self.data.prefab_indices,
                "prefab indexes",
            )?);
        }
        if self.data.position_x != self.original_position_x {
            let values = self
                .data
                .position_x
                .iter()
                .map(|value| *value as u16)
                .collect::<Vec<_>>();
            replacements.push(encode_ushort_column(
                &bytes,
                &self.data.position_x_range,
                &values,
                "x positions",
            )?);
        }
        if self.data.position_z != self.original_position_z {
            let values = self
                .data
                .position_z
                .iter()
                .map(|value| *value as u16)
                .collect::<Vec<_>>();
            replacements.push(encode_ushort_column(
                &bytes,
                &self.data.position_z_range,
                &values,
                "z positions",
            )?);
        }
        if self.data.angles != self.original_angles {
            replacements.push(encode_ushort_column(
                &bytes,
                &self.data.angles_range,
                &self.data.angles,
                "angles",
            )?);
        }
        if self.data.race_logo_indices != self.original_race_logo_indices {
            let range = self.data.race_logo_indices_range.as_ref().ok_or_else(|| {
                invalid_prop_manager(
                    0,
                    "race-team logo assignments are unavailable in this version",
                )
            })?;
            replacements.push(encode_ushort_column(
                &bytes,
                range,
                &self.data.race_logo_indices,
                "race-team logo indexes",
            )?);
        }
        if self.data.event_logo_indices != self.original_event_logo_indices {
            let range = self.data.event_logo_indices_range.as_ref().ok_or_else(|| {
                invalid_prop_manager(0, "event logo assignments are unavailable in this version")
            })?;
            replacements.push(encode_ushort_column(
                &bytes,
                range,
                &self.data.event_logo_indices,
                "event logo indexes",
            )?);
        }

        replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let mut previous_start = bytes.len();
        for (range, replacement) in replacements {
            if range.end > previous_start {
                return Err(invalid_prop_manager(
                    range.start,
                    "edited PropManager field ranges overlap",
                ));
            }
            validate_range(&bytes, &range, range.len(), "edited PropManager field")?;
            bytes.splice(range.clone(), replacement);
            previous_start = range.start;
        }
        Ok(bytes)
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.data.serialized_prefix
    }
}

fn encode_ushort_column(
    source: &[u8],
    range: &Range<usize>,
    values: &[u16],
    field: &str,
) -> Result<(Range<usize>, Vec<u8>), CrpError> {
    if range.start > range.end || range.end > source.len() {
        return Err(invalid_prop_manager(
            range.start,
            format!("{field} byte range is invalid"),
        ));
    }
    let mut encoded = Vec::new();
    write_encoded_ushort_array(&mut encoded, values);
    Ok((range.clone(), encoded))
}

fn validate_range(
    source: &[u8],
    range: &Range<usize>,
    expected_len: usize,
    field: &str,
) -> Result<(), CrpError> {
    if range.start > range.end || range.end > source.len() || range.len() != expected_len {
        return Err(invalid_prop_manager(
            range.start,
            format!("{field} byte range does not match its decoded values"),
        ));
    }
    Ok(())
}

fn invalid_prop_manager(offset: usize, reason: impl Into<String>) -> CrpError {
    CrpError::InvalidAssetPayload {
        name: "PropManager+Data".to_owned(),
        offset,
        reason: reason.into(),
    }
}

fn read_prefab_names(reader: &mut Reader<'_>) -> Result<Vec<Option<String>>, CrpError> {
    let count = usize::from(reader.read_u16_be()?);
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        names.push(reader.read_unique_string()?);
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::{PROP_INSTANCE_COUNT, PropManagerDataPrefix};
    use crate::encoded_array::write_encoded_ushort_array;

    const LEGACY_FIXTURE: &[u8] =
        include_bytes!("../tests/fixtures/prop-manager-data-v116020-desert.bin");

    fn zero_flags() -> Vec<u8> {
        let mut bytes = vec![0xff; 511];
        bytes.push(0xfe);
        bytes
    }

    fn empty_prefix(version: u32) -> Vec<u8> {
        let mut bytes = zero_flags();
        bytes.extend_from_slice(&[0, 0]); // PropInfo name count
        if version >= 121_026 {
            bytes.extend_from_slice(&[0, 0, 0, 0]); // race and event name counts
        }
        bytes
    }

    fn two_logo_prefix() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[3, 1, 0, 2, 0, 0, 0, 0, 0]);
        bytes.extend(std::iter::repeat_n(0xff, 511));
        bytes.push(0xfa); // 123 zero flags complete the fixed 65,535 positions.
        bytes.extend_from_slice(&[1, 0, 0x11, 0, 0x22, 0, 0]); // info indices and names
        bytes.extend_from_slice(&[1, 0xff, 0xff, 0, 2]); // signed X positions
        bytes.extend_from_slice(&[1, 0, 3, 0, 4]); // signed Z positions
        bytes.extend_from_slice(&[1, 1, 0, 2, 0]); // angles
        bytes.extend_from_slice(&[0, 0, 0x44, 0, 0]); // race logo index and names
        bytes.extend_from_slice(&[0, 0, 0x55, 0, 0]); // event logo index and names
        bytes
    }

    #[test]
    fn game_sample_decodes_and_preserves_prop_manager_prefix() {
        let (props, consumed) =
            PropManagerDataPrefix::decode_prefix(116_020, LEGACY_FIXTURE).unwrap();
        assert_eq!(consumed, LEGACY_FIXTURE.len());
        assert_eq!(props.flags().len(), 65_535);
        assert_eq!(
            props.flags().iter().filter(|flags| **flags != 0).count(),
            574
        );
        assert_eq!(props.prefab_indices().len(), 574);
        assert_eq!(props.prefab_names().len(), 899);
        assert_eq!(props.prefab_names().iter().flatten().count(), 47);
        assert_eq!(props.position_x().len(), 574);
        assert_eq!(props.position_z().len(), 574);
        assert_eq!(props.angles().len(), 574);
        assert!(!props.logo_tables_present());
        assert!(props.race_logo_indices().is_empty());
        assert!(props.event_logo_indices().is_empty());
        assert_eq!(props.encode(), LEGACY_FIXTURE);
    }

    #[test]
    fn empty_prop_manager_layout_respects_logo_table_gate() {
        let legacy = empty_prefix(121_025);
        let (old, used) = PropManagerDataPrefix::decode_prefix(121_025, &legacy).unwrap();
        assert_eq!(used, legacy.len());
        assert!(!old.logo_tables_present());
        assert_eq!(old.encode(), legacy);

        let current = empty_prefix(121_026);
        let (new, used) = PropManagerDataPrefix::decode_prefix(121_026, &current).unwrap();
        assert_eq!(used, current.len());
        assert!(new.logo_tables_present());
        assert_eq!(new.race_logo_names(), &[]);
        assert_eq!(new.event_logo_names(), &[]);
        assert_eq!(new.encode(), current);
    }

    #[test]
    fn newer_prop_manager_layout_reads_logo_indices_by_flag_kind() {
        let bytes = two_logo_prefix();
        assert_eq!(bytes.len(), 553);
        let (props, used) = PropManagerDataPrefix::decode_prefix(121_026, &bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert!(props.logo_tables_present());
        assert_eq!(props.flags()[..4], [0x0100, 0x0200, 0, 0]);
        assert_eq!(props.prefab_indices(), &[0x11, 0x22]);
        assert_eq!(props.position_x(), &[-1, 2]);
        assert_eq!(props.position_z(), &[3, 4]);
        assert_eq!(props.angles(), &[0x0100, 0x0200]);
        assert_eq!(props.race_logo_indices(), &[0x44]);
        assert_eq!(props.event_logo_indices(), &[0x55]);
        assert_eq!(props.encode(), bytes);
    }

    #[test]
    fn prop_manager_editor_reencodes_changed_logo_assignments_exactly() {
        let bytes = two_logo_prefix();
        let (props, _) = PropManagerDataPrefix::decode_prefix(121_026, &bytes).unwrap();
        let mut editor = props.into_editor();
        editor.race_logo_indices_mut()[0] = 0x1234;
        editor.event_logo_indices_mut()[0] = 0xabcd;

        let mut expected = bytes.clone();
        expected[544..546].copy_from_slice(&[0x12, 0x34]);
        expected[549..551].copy_from_slice(&[0xab, 0xcd]);
        let encoded = editor.encode().unwrap();
        assert_eq!(encoded, expected);

        let (updated, consumed) = PropManagerDataPrefix::decode_prefix(121_026, &encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(updated.race_logo_indices(), &[0x1234]);
        assert_eq!(updated.event_logo_indices(), &[0xabcd]);
        assert_eq!(updated.race_logo_names(), &[]);
        assert_eq!(updated.event_logo_names(), &[]);
    }

    #[test]
    fn prop_manager_logo_assignment_resize_preserves_later_scopes() {
        let mut flags = vec![0_u16; PROP_INSTANCE_COUNT];
        flags[..4].fill(0x0100);
        flags[4] = 0x0200;
        let mut bytes = Vec::new();
        write_encoded_ushort_array(&mut bytes, &flags);
        write_encoded_ushort_array(&mut bytes, &[1, 2, 3, 4, 5]);
        bytes.extend_from_slice(&[0, 0]); // Empty PropInfo names.
        write_encoded_ushort_array(&mut bytes, &[0; 5]); // X positions.
        write_encoded_ushort_array(&mut bytes, &[0; 5]); // Z positions.
        write_encoded_ushort_array(&mut bytes, &[0; 5]); // Angles.
        write_encoded_ushort_array(&mut bytes, &[5, 10, 15, 20]);
        bytes.extend_from_slice(&[0, 0]); // Empty race-team names.
        write_encoded_ushort_array(&mut bytes, &[30]);
        bytes.extend_from_slice(&[0, 0]); // Empty event names.

        let (props, consumed) = PropManagerDataPrefix::decode_prefix(121_026, &bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(props.race_logo_indices(), &[5, 10, 15, 20]);
        assert_eq!(props.event_logo_indices(), &[30]);
        let race_range = props.race_logo_indices_range.clone().unwrap();
        let mut editor = props.into_editor();
        editor.race_logo_indices_mut().fill(0);

        let mut expected = bytes.clone();
        expected.splice(race_range, [0x83]); // Four repeated zero indexes.
        let encoded = editor.encode().unwrap();
        assert_eq!(encoded, expected);
        assert_eq!(encoded.len(), bytes.len() - 3);

        let (updated, consumed) = PropManagerDataPrefix::decode_prefix(121_026, &encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(updated.race_logo_indices(), &[0; 4]);
        assert_eq!(updated.event_logo_indices(), &[30]);
    }

    #[test]
    fn prop_manager_editor_reencodes_only_changed_active_columns() {
        let mut flags = vec![0_u16; 65_535];
        flags[..3].fill(1);
        let mut encoded_flags = Vec::new();
        write_encoded_ushort_array(&mut encoded_flags, &flags);
        let mut source = encoded_flags.clone();
        write_encoded_ushort_array(&mut source, &[10, 20, 30]);
        source.extend_from_slice(&[0, 0]); // Empty PropInfo name scope.
        write_encoded_ushort_array(&mut source, &[10, 20, 30]);
        write_encoded_ushort_array(&mut source, &[0xffff, 2, 3]);
        write_encoded_ushort_array(&mut source, &[10, 20, 30]);

        let (props, _) = PropManagerDataPrefix::decode_prefix(116_020, &source).unwrap();
        let mut editor = props.into_editor();
        assert_eq!(editor.encode().unwrap(), source);
        editor.prefab_indices_mut().fill(0);
        editor.position_x_mut().fill(0);
        editor.position_z_mut().copy_from_slice(&[5, 6, 7]);
        editor.angles_mut().fill(0);

        let mut expected = encoded_flags;
        expected.extend_from_slice(&[0x82]); // Three zero prefab indices.
        expected.extend_from_slice(&[0, 0]); // Unchanged empty name scope.
        expected.extend_from_slice(&[0x82]); // Three zero x positions.
        expected.extend_from_slice(&[2, 0, 5, 0, 6, 0, 7]); // Signed z values as UShort.
        expected.extend_from_slice(&[0x82]); // Three zero angles.
        let encoded = editor.encode().unwrap();
        assert_eq!(encoded, expected);

        let (updated, used) = PropManagerDataPrefix::decode_prefix(116_020, &encoded).unwrap();
        assert_eq!(used, encoded.len());
        assert_eq!(updated.prefab_indices(), &[0, 0, 0]);
        assert_eq!(updated.position_x(), &[0, 0, 0]);
        assert_eq!(updated.position_z(), &[5, 6, 7]);
        assert_eq!(updated.angles(), &[0, 0, 0]);
    }

    #[test]
    fn prop_manager_editor_edits_flags_without_changing_sparse_layout() {
        let bytes = two_logo_prefix();
        let (props, _) = PropManagerDataPrefix::decode_prefix(121_026, &bytes).unwrap();
        let mut editor = props.into_editor();
        editor.set_flag_bits(0, 0x0101).unwrap();
        editor.set_flag_bits(1, 0x0202).unwrap();
        assert!(editor.set_flag_bits(0, 0x0200).is_err());
        assert!(editor.set_flag_bits(2, 1).is_err());
        assert!(editor.set_flag_bits(65_535, 1).is_err());

        let encoded = editor.encode().unwrap();
        assert_eq!(&encoded[..9], &[3, 0x01, 0x01, 0x02, 0x02, 0, 0, 0, 0]);
        assert_eq!(&encoded[9..], &bytes[9..]);
        let (updated, consumed) = PropManagerDataPrefix::decode_prefix(121_026, &encoded).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(updated.flags()[..4], [0x0101, 0x0202, 0, 0]);
        assert_eq!(updated.prefab_indices(), &[0x11, 0x22]);
        assert_eq!(updated.race_logo_indices(), &[0x44]);
        assert_eq!(updated.event_logo_indices(), &[0x55]);

        let (legacy, _) = PropManagerDataPrefix::decode_prefix(116_020, LEGACY_FIXTURE).unwrap();
        let active_index = legacy.flags().iter().position(|flags| *flags != 0).unwrap();
        let current_value = legacy.flags()[active_index];
        let original_prefab_indices = legacy.prefab_indices().to_vec();
        let changed_value = if current_value == 1 {
            2
        } else {
            current_value ^ 1
        };
        let mut legacy_editor = legacy.into_editor();
        legacy_editor
            .set_flag_bits(active_index, changed_value)
            .unwrap();
        let legacy_encoded = legacy_editor.encode().unwrap();
        let (legacy_updated, _) =
            PropManagerDataPrefix::decode_prefix(116_020, &legacy_encoded).unwrap();
        assert_eq!(legacy_updated.flags()[active_index], changed_value);
        assert_eq!(legacy_updated.prefab_indices(), original_prefab_indices);
    }

    #[test]
    fn prop_manager_editor_keeps_newer_logo_scopes_unchanged() {
        let bytes = two_logo_prefix();
        let (props, _) = PropManagerDataPrefix::decode_prefix(121_026, &bytes).unwrap();
        let mut editor = props.into_editor();
        editor.position_x_mut()[0] = 7;

        let mut expected = bytes.clone();
        expected[528..533].copy_from_slice(&[1, 0, 7, 0, 2]);
        let encoded = editor.encode().unwrap();
        assert_eq!(encoded, expected);
        let (updated, _) = PropManagerDataPrefix::decode_prefix(121_026, &encoded).unwrap();
        assert_eq!(updated.position_x(), &[7, 2]);
        assert_eq!(updated.race_logo_indices(), &[0x44]);
        assert_eq!(updated.event_logo_indices(), &[0x55]);
    }

    #[test]
    fn truncated_prop_manager_prefix_returns_an_error() {
        let bytes = empty_prefix(121_026);
        assert!(PropManagerDataPrefix::decode_prefix(121_026, &bytes[..bytes.len() - 1]).is_err());
        assert!(PropManagerDataPrefix::decode_prefix(116_020, &LEGACY_FIXTURE[..500]).is_err());
    }
}
