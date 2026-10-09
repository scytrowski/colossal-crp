use crate::CrpError;
use crate::data_serializer::Reader;

pub(crate) const GUIDE_TRIGGER_BASE_PREFIX_LEN: usize = 11;
pub(crate) const GUIDE_INSTANCE_BODY_LEN: usize = 17;

/// The common field prefix serialized by `GuideTriggerBase`.
///
/// It contains two big-endian `u32` frame values, two `u8` counters, and the
/// raw byte used by DataSerializer for the disabled bool. The prefix has no
/// inner-version gates. `decode` consumes 11 bytes and leaves any subtype
/// fields to the caller. It does not locate this prefix in an object queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideTriggerBasePrefix {
    next_repeat: u32,
    activation_frame: u32,
    activation_count: u8,
    display_count: u8,
    disabled_byte: u8,
    serialized_prefix: Vec<u8>,
}

/// Editable values for a `GuideTriggerBase` prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideTriggerBaseEditor {
    original: GuideTriggerBasePrefix,
    next_repeat: u32,
    activation_frame: u32,
    activation_count: u8,
    display_count: u8,
    disabled: bool,
}

/// One queued guide that targets a network segment or building instance.
///
/// `NetSegmentInstanceGuide`, `BuildingInstanceGuide`, and its subclass
/// `BuildingOnFireGuide` all serialize this 17-byte body: the 11-byte
/// `GuideTriggerBase` prefix, a big-endian build index, and a big-endian
/// 16-bit instance identifier. The body has no inner-version gates. This
/// codec parses one body but does not locate it in the shared object queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideInstanceBody {
    base: GuideTriggerBasePrefix,
    build_index: u32,
    instance_id: u16,
    serialized_body: Vec<u8>,
}

/// Editable values for a queued segment or building guide body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideInstanceEditor {
    original: GuideInstanceBody,
    base: GuideTriggerBaseEditor,
    build_index: u32,
    instance_id: u16,
}

impl GuideTriggerBasePrefix {
    /// Decodes one 11-byte `GuideTriggerBase` prefix.
    ///
    /// Returns the decoded prefix and the number of bytes consumed. Truncated
    /// input returns an error. The original bytes are retained for exact
    /// rewriting, including noncanonical bool bytes.
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let next_repeat = reader.read_u32_be()?;
        let activation_frame = reader.read_u32_be()?;
        let activation_count = reader.read_u8()?;
        let display_count = reader.read_u8()?;
        let disabled_byte = reader.read_u8()?;
        let consumed = reader.offset();
        debug_assert_eq!(consumed, GUIDE_TRIGGER_BASE_PREFIX_LEN);
        Ok((
            Self {
                next_repeat,
                activation_frame,
                activation_count,
                display_count,
                disabled_byte,
                serialized_prefix: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the next-repeat frame.
    pub fn next_repeat(&self) -> u32 {
        self.next_repeat
    }

    /// Returns the activation frame.
    pub fn activation_frame(&self) -> u32 {
        self.activation_frame
    }

    /// Returns the number of activations.
    pub fn activation_count(&self) -> u8 {
        self.activation_count
    }

    /// Returns the number of displays.
    pub fn display_count(&self) -> u8 {
        self.display_count
    }

    /// Returns whether the saved bool is true; only raw byte `01` is true.
    pub fn disabled(&self) -> bool {
        self.disabled_byte == 1
    }

    /// Returns the original bool byte, including noncanonical values.
    pub fn disabled_byte(&self) -> u8 {
        self.disabled_byte
    }

    /// Returns the original 11-byte prefix unchanged.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_prefix.clone()
    }

    /// Creates an editor that retains original bytes until a value changes.
    pub fn into_editor(self) -> GuideTriggerBaseEditor {
        GuideTriggerBaseEditor {
            next_repeat: self.next_repeat,
            activation_frame: self.activation_frame,
            activation_count: self.activation_count,
            display_count: self.display_count,
            disabled: self.disabled(),
            original: self,
        }
    }
}

impl GuideTriggerBaseEditor {
    /// Returns mutable access to the next-repeat frame.
    pub fn next_repeat_mut(&mut self) -> &mut u32 {
        &mut self.next_repeat
    }

    /// Returns mutable access to the activation frame.
    pub fn activation_frame_mut(&mut self) -> &mut u32 {
        &mut self.activation_frame
    }

    /// Returns mutable access to the activation count.
    pub fn activation_count_mut(&mut self) -> &mut u8 {
        &mut self.activation_count
    }

    /// Returns mutable access to the display count.
    pub fn display_count_mut(&mut self) -> &mut u8 {
        &mut self.display_count
    }

    /// Returns mutable access to the disabled state.
    pub fn disabled_mut(&mut self) -> &mut bool {
        &mut self.disabled
    }

    /// Writes the 11-byte prefix in big-endian field order.
    ///
    /// An unchanged prefix is returned byte for byte. If another field changes
    /// while the bool's decoded value stays the same, its original raw byte is
    /// retained. Changing the bool writes canonical `00` or `01`.
    pub fn encode(&self) -> Vec<u8> {
        if self.next_repeat == self.original.next_repeat
            && self.activation_frame == self.original.activation_frame
            && self.activation_count == self.original.activation_count
            && self.display_count == self.original.display_count
            && self.disabled == self.original.disabled()
        {
            return self.original.encode();
        }

        let mut output = Vec::with_capacity(GUIDE_TRIGGER_BASE_PREFIX_LEN);
        output.extend_from_slice(&self.next_repeat.to_be_bytes());
        output.extend_from_slice(&self.activation_frame.to_be_bytes());
        output.push(self.activation_count);
        output.push(self.display_count);
        let disabled_byte = if self.disabled == self.original.disabled() {
            self.original.disabled_byte
        } else {
            u8::from(self.disabled)
        };
        output.push(disabled_byte);
        output
    }

    pub(crate) fn original_prefix_bytes(&self) -> &[u8] {
        &self.original.serialized_prefix
    }
}

impl GuideInstanceBody {
    /// Decodes one 17-byte body used by segment and building instance guides.
    ///
    /// Returns the decoded body and the number of bytes consumed. Any bytes
    /// after the body are left for the caller. Truncated input returns an
    /// error, and the original bytes are retained for exact rewriting.
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let (base, base_length) = GuideTriggerBasePrefix::decode(bytes)?;
        let mut reader = Reader::new(&bytes[base_length..]);
        let build_index = reader.read_u32_be()?;
        let instance_id = reader.read_u16_be()?;
        let consumed = base_length + reader.offset();
        debug_assert_eq!(consumed, GUIDE_INSTANCE_BODY_LEN);
        Ok((
            Self {
                base,
                build_index,
                instance_id,
                serialized_body: bytes[..consumed].to_vec(),
            },
            consumed,
        ))
    }

    /// Returns the common trigger fields.
    pub fn base(&self) -> &GuideTriggerBasePrefix {
        &self.base
    }

    /// Returns the build index.
    pub fn build_index(&self) -> u32 {
        self.build_index
    }

    /// Returns the segment or building identifier.
    pub fn instance_id(&self) -> u16 {
        self.instance_id
    }

    /// Returns the original body unchanged.
    pub fn encode(&self) -> Vec<u8> {
        self.serialized_body.clone()
    }

    /// Creates an editor that retains original bytes until a field changes.
    pub fn into_editor(self) -> GuideInstanceEditor {
        GuideInstanceEditor {
            base: self.base.clone().into_editor(),
            build_index: self.build_index,
            instance_id: self.instance_id,
            original: self,
        }
    }
}

impl GuideInstanceEditor {
    /// Returns mutable access to the common trigger fields.
    pub fn base_mut(&mut self) -> &mut GuideTriggerBaseEditor {
        &mut self.base
    }

    /// Returns mutable access to the build index.
    pub fn build_index_mut(&mut self) -> &mut u32 {
        &mut self.build_index
    }

    /// Returns mutable access to the segment or building identifier.
    pub fn instance_id_mut(&mut self) -> &mut u16 {
        &mut self.instance_id
    }

    /// Writes the 17-byte body in big-endian field order.
    ///
    /// An unchanged body is returned byte for byte. Editing a suffix field
    /// retains any noncanonical bool byte in the common trigger prefix unless
    /// the disabled state itself changes.
    pub fn encode(&self) -> Vec<u8> {
        if self.build_index == self.original.build_index
            && self.instance_id == self.original.instance_id
            && self.base.encode() == self.original.base.encode()
        {
            return self.original.encode();
        }

        let mut output = self.base.encode();
        output.extend_from_slice(&self.build_index.to_be_bytes());
        output.extend_from_slice(&self.instance_id.to_be_bytes());
        output
    }

    pub(crate) fn original_body_bytes(&self) -> &[u8] {
        &self.original.serialized_body
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GUIDE_INSTANCE_BODY_LEN, GUIDE_TRIGGER_BASE_PREFIX_LEN, GuideInstanceBody,
        GuideTriggerBasePrefix,
    };

    const GUIDE_MANAGER_BODIES: &[u8] =
        include_bytes!("../tests/fixtures/guide-manager-bodies-v112004-wolf-creek.bin");
    const UNLOCK_UNUSED_GUIDE_BODIES: &[u8] =
        include_bytes!("../tests/fixtures/unlock-unused-guides-v112004-wolf-creek.bin");
    const BUILDING_MANAGER_GUIDE_BODIES: &[u8] =
        include_bytes!("../tests/fixtures/building-manager-guide-bodies-v112004-wolf-creek.bin");

    #[test]
    fn wolf_creek_guide_manager_bodies_match_the_base_prefix_layout() {
        assert_eq!(
            GUIDE_MANAGER_BODIES.len(),
            48 * GUIDE_TRIGGER_BASE_PREFIX_LEN
        );
        let mut disabled = Vec::new();
        for (index, bytes) in GUIDE_MANAGER_BODIES
            .chunks_exact(GUIDE_TRIGGER_BASE_PREFIX_LEN)
            .enumerate()
        {
            let (guide, consumed) = GuideTriggerBasePrefix::decode(bytes).unwrap();
            assert_eq!(consumed, GUIDE_TRIGGER_BASE_PREFIX_LEN);
            assert_eq!(guide.encode(), bytes);
            assert_eq!(guide.next_repeat(), 0);
            assert_eq!(guide.activation_frame(), 0);
            assert_eq!(guide.activation_count(), 0);
            assert_eq!(guide.display_count(), 0);
            if guide.disabled() {
                disabled.push(index);
            }
        }
        assert_eq!(disabled, [0, 6, 20, 28, 29, 30, 31, 32, 37]);
    }

    #[test]
    fn wolf_creek_unlock_manager_guides_match_generic_guide_bodies() {
        assert_eq!(
            UNLOCK_UNUSED_GUIDE_BODIES.len(),
            2 * GUIDE_TRIGGER_BASE_PREFIX_LEN
        );
        for bytes in UNLOCK_UNUSED_GUIDE_BODIES.chunks_exact(GUIDE_TRIGGER_BASE_PREFIX_LEN) {
            let (guide, consumed) = GuideTriggerBasePrefix::decode(bytes).unwrap();
            assert_eq!(consumed, GUIDE_TRIGGER_BASE_PREFIX_LEN);
            assert_eq!(guide.encode(), bytes);
            assert_eq!(guide.next_repeat(), 0);
            assert_eq!(guide.activation_frame(), 0);
            assert_eq!(guide.activation_count(), 0);
            assert_eq!(guide.display_count(), 0);
            assert!(!guide.disabled());
        }
    }

    #[test]
    fn guide_prefix_reads_and_writes_big_endian_fields() {
        let original = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 1];
        let (guide, consumed) = GuideTriggerBasePrefix::decode(&original).unwrap();
        assert_eq!(consumed, GUIDE_TRIGGER_BASE_PREFIX_LEN);
        assert_eq!(guide.next_repeat(), 0x0102_0304);
        assert_eq!(guide.activation_frame(), 0x0506_0708);
        assert_eq!(guide.activation_count(), 9);
        assert_eq!(guide.display_count(), 10);
        assert!(guide.disabled());

        let mut editor = guide.into_editor();
        *editor.activation_frame_mut() = 0x1122_3344;
        *editor.display_count_mut() = 0x55;
        *editor.disabled_mut() = false;
        assert_eq!(
            editor.encode(),
            [1, 2, 3, 4, 0x11, 0x22, 0x33, 0x44, 9, 0x55, 0]
        );
    }

    #[test]
    fn guide_editor_preserves_unusual_bool_bytes_until_the_state_changes() {
        let mut original = [0; GUIDE_TRIGGER_BASE_PREFIX_LEN];
        original[10] = 0x7f;
        let (guide, _) = GuideTriggerBasePrefix::decode(&original).unwrap();
        assert!(!guide.disabled());
        assert_eq!(guide.disabled_byte(), 0x7f);

        let mut editor = guide.into_editor();
        *editor.next_repeat_mut() = 1;
        assert_eq!(editor.encode()[10], 0x7f);
        *editor.disabled_mut() = true;
        assert_eq!(editor.encode()[10], 1);
    }

    #[test]
    fn guide_prefix_rejects_truncated_input() {
        for length in 0..GUIDE_TRIGGER_BASE_PREFIX_LEN {
            assert!(
                GuideTriggerBasePrefix::decode(&[0; GUIDE_TRIGGER_BASE_PREFIX_LEN][..length])
                    .is_err()
            );
        }
    }

    #[test]
    fn wolf_creek_building_manager_guide_bodies_follow_reference_order() {
        let extended = [
            0, 1, 2, 3, 4, 7, 9, 11, 12, 16, 17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 28, 29, 32,
            34, 37,
        ];
        assert_eq!(BUILDING_MANAGER_GUIDE_BODIES.len(), 568);
        let mut cursor = 0;
        for index in 0..38 {
            let body_length = if extended.contains(&index) {
                GUIDE_INSTANCE_BODY_LEN
            } else {
                GUIDE_TRIGGER_BASE_PREFIX_LEN
            };
            let body = &BUILDING_MANAGER_GUIDE_BODIES[cursor..cursor + body_length];
            if extended.contains(&index) {
                let (guide, consumed) = GuideInstanceBody::decode(body).unwrap();
                assert_eq!(consumed, GUIDE_INSTANCE_BODY_LEN);
                assert_eq!(guide.build_index(), 0);
                assert_eq!(guide.instance_id(), 0);
                assert_eq!(guide.encode(), body);
            } else {
                let (guide, consumed) = GuideTriggerBasePrefix::decode(body).unwrap();
                assert_eq!(consumed, GUIDE_TRIGGER_BASE_PREFIX_LEN);
                assert_eq!(guide.encode(), body);
            }
            cursor += body_length;
        }
        assert_eq!(cursor, BUILDING_MANAGER_GUIDE_BODIES.len());
    }

    #[test]
    fn guide_instance_body_decodes_and_edits_big_endian_fields() {
        let mut original = [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0x7f, // common trigger prefix
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, // build index and instance ID
        ];
        let (guide, consumed) = GuideInstanceBody::decode(&original).unwrap();
        assert_eq!(consumed, GUIDE_INSTANCE_BODY_LEN);
        assert_eq!(guide.base().next_repeat(), 0x0102_0304);
        assert_eq!(guide.base().disabled_byte(), 0x7f);
        assert_eq!(guide.build_index(), 0x1122_3344);
        assert_eq!(guide.instance_id(), 0x5566);
        assert_eq!(guide.encode(), original);

        let mut editor = guide.into_editor();
        *editor.build_index_mut() = 0xa1b2_c3d4;
        *editor.instance_id_mut() = 0xe5f6;
        let encoded = editor.encode();
        assert_eq!(&encoded[11..15], &0xa1b2_c3d4_u32.to_be_bytes());
        assert_eq!(&encoded[15..17], &0xe5f6_u16.to_be_bytes());
        assert_eq!(encoded[10], 0x7f);

        *editor.base_mut().disabled_mut() = true;
        original[10] = 1;
        assert_eq!(editor.encode()[10], 1);
        assert_eq!(&editor.encode()[..10], &original[..10]);
    }

    #[test]
    fn guide_instance_body_rejects_truncated_input() {
        for length in 0..GUIDE_INSTANCE_BODY_LEN {
            assert!(GuideInstanceBody::decode(&[0; GUIDE_INSTANCE_BODY_LEN][..length]).is_err());
        }
    }
}
