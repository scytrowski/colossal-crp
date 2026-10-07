use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::read_encoded_ushort_array;

const PROP_INSTANCE_COUNT: usize = 65_535;
const LOGO_TABLE_VERSION: u32 = 121_026;

/// Known fields from `PropManager+Data`, excluding its loading-profiler tags.
///
/// The decoder supports the legacy layout below inner version 121026 and the
/// logo-table layout from version 121026. It retains the exact known bytes for
/// lossless rewriting; changing fields and canonical serialization are not
/// currently exposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropManagerDataPrefix {
    format_version: u32,
    flags: Vec<u16>,
    prefab_indices: Vec<u16>,
    prefab_names: Vec<Option<String>>,
    position_x: Vec<i16>,
    position_z: Vec<i16>,
    angles: Vec<u16>,
    race_logo_indices: Vec<u16>,
    race_logo_names: Vec<Option<String>>,
    event_logo_indices: Vec<u16>,
    event_logo_names: Vec<Option<String>>,
    logo_tables_present: bool,
    serialized_prefix: Vec<u8>,
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
        let flags = read_encoded_ushort_array(&mut reader, PROP_INSTANCE_COUNT)?;
        let active_count = flags.iter().filter(|flags| **flags != 0).count();
        let prefab_indices = read_encoded_ushort_array(&mut reader, active_count)?;
        let prefab_names = read_prefab_names(&mut reader)?;
        let position_x = read_encoded_ushort_array(&mut reader, active_count)?
            .into_iter()
            .map(|value| value as i16)
            .collect();
        let position_z = read_encoded_ushort_array(&mut reader, active_count)?
            .into_iter()
            .map(|value| value as i16)
            .collect();
        let angles = read_encoded_ushort_array(&mut reader, active_count)?;

        let logo_tables_present = format_version >= LOGO_TABLE_VERSION;
        let (race_logo_indices, race_logo_names, event_logo_indices, event_logo_names) =
            if logo_tables_present {
                let race_count = flags
                    .iter()
                    .filter(|flags| **flags & 0x0300 == 0x0100)
                    .count();
                let event_count = flags
                    .iter()
                    .filter(|flags| **flags & 0x0300 == 0x0200)
                    .count();
                let race_indices = read_encoded_ushort_array(&mut reader, race_count)?;
                let race_names = read_prefab_names(&mut reader)?;
                let event_indices = read_encoded_ushort_array(&mut reader, event_count)?;
                (
                    race_indices,
                    race_names,
                    event_indices,
                    read_prefab_names(&mut reader)?,
                )
            } else {
                (Vec::new(), Vec::new(), Vec::new(), Vec::new())
            };
        let consumed = reader.offset();

        Ok((
            Self {
                format_version,
                flags,
                prefab_indices,
                prefab_names,
                position_x,
                position_z,
                angles,
                race_logo_indices,
                race_logo_names,
                event_logo_indices,
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
    use super::PropManagerDataPrefix;

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
    fn truncated_prop_manager_prefix_returns_an_error() {
        let bytes = empty_prefix(121_026);
        assert!(PropManagerDataPrefix::decode_prefix(121_026, &bytes[..bytes.len() - 1]).is_err());
        assert!(PropManagerDataPrefix::decode_prefix(116_020, &LEGACY_FIXTURE[..500]).is_err());
    }
}
