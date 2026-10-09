use crate::CrpError;
use crate::data_serializer::Reader;
use crate::encoded_array::{read_encoded_uint_array, skip_encoded_ushort_array};
use crate::net_manager_data::{NetObjectHeader, read_net_object_header};

/// Object references extracted from an empty-record `DisasterManager+Data` body.
///
/// Active disaster records remain unsupported. The original field bytes and
/// unresolved milestone and warning-guide headers are retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DisasterManagerReferenceData {
    pub(crate) object_headers: Vec<NetObjectHeader>,
}

impl DisasterManagerReferenceData {
    pub(crate) fn decode(format_version: u32, bytes: &[u8]) -> Result<(Self, usize), CrpError> {
        let mut reader = Reader::new(bytes);
        let capacity = usize::from(reader.read_u16_be()?)
            .checked_add(1)
            .ok_or(CrpError::ArithmeticOverflow("DisasterManager slot count"))?;
        let flags = read_encoded_uint_array(&mut reader, capacity - 1)?;
        if flags.iter().any(|flag| flag & !0x8000 != 0) {
            return Err(reader.invalid("active DisasterManager records are unsupported"));
        }

        let milestone_count = usize::from(reader.read_u16_be()?);
        let last_spawn_frame_count = if format_version >= 291 {
            usize::from(reader.read_u16_be()?)
        } else {
            0
        };
        let warning_guide_count = if format_version >= 293 {
            usize::from(reader.read_u16_be()?)
        } else {
            0
        };
        let prefab_count = flags
            .iter()
            .filter(|flag| **flag & !0x8000 != 0)
            .count()
            .checked_add(milestone_count)
            .and_then(|count| count.checked_add(last_spawn_frame_count))
            .and_then(|count| count.checked_add(warning_guide_count))
            .ok_or(CrpError::ArithmeticOverflow("DisasterManager prefab count"))?;
        skip_encoded_ushort_array(&mut reader, prefab_count)?;
        skip_prefab_names(&mut reader)?;

        let mut object_headers = read_object_headers(&mut reader, milestone_count)?;
        let frame_bytes =
            last_spawn_frame_count
                .checked_mul(4)
                .ok_or(CrpError::ArithmeticOverflow(
                    "DisasterManager last-spawn frame bytes",
                ))?;
        reader.read_bytes(frame_bytes)?;
        object_headers.extend(read_object_headers(&mut reader, warning_guide_count)?);
        if format_version >= 282 {
            reader.read_u16_be()?;
        }
        reader.read_u16_be()?;
        let consumed = reader.offset();
        Ok((Self { object_headers }, consumed))
    }
}

fn skip_prefab_names(reader: &mut Reader<'_>) -> Result<(), CrpError> {
    let count = usize::from(reader.read_u16_be()?);
    for _ in 0..count {
        reader.read_unique_string()?;
    }
    Ok(())
}

fn read_object_headers(
    reader: &mut Reader<'_>,
    count: usize,
) -> Result<Vec<NetObjectHeader>, CrpError> {
    if count > reader.remaining() {
        return Err(reader.invalid(format!(
            "DisasterManager declares {count} object references but only {} bytes remain",
            reader.remaining()
        )));
    }
    (0..count).map(|_| read_net_object_header(reader)).collect()
}

#[cfg(test)]
mod tests {
    use super::DisasterManagerReferenceData;
    use crate::net_manager_data::NetObjectHeader;

    #[test]
    fn disaster_reference_fields_follow_version_gates_and_preserve_bytes() {
        for version in [281, 282, 290, 291, 292, 293] {
            let mut bytes = vec![0x00, 0x00, 0x00, 0x00]; // One slot, no milestones.
            if version >= 291 {
                bytes.extend_from_slice(&[0x00, 0x00]);
            }
            if version >= 293 {
                bytes.extend_from_slice(&[0x00, 0x01]);
            }
            if version >= 293 {
                bytes.extend_from_slice(&[0x00, 0x00, 0x05]); // One warning-guide prefab index.
            }
            bytes.extend_from_slice(&[0x00, 0x00]); // Empty prefab names.
            if version >= 293 {
                bytes.extend_from_slice(&[0xfe, 0xfe, 0x80, 0x00, 0x00]); // New warning guide type.
            }
            if version >= 282 {
                bytes.extend_from_slice(&[0x00, 0x06]); // Random-disaster cooldown.
            }
            bytes.extend_from_slice(&[0x12, 0x34]); // Profiler end index.

            let (decoded, consumed) =
                DisasterManagerReferenceData::decode(version, &bytes).unwrap();
            assert_eq!(consumed, bytes.len(), "version {version}");
            assert_eq!(
                decoded.object_headers,
                if version >= 293 {
                    vec![NetObjectHeader::NewType("".to_owned())]
                } else {
                    Vec::new()
                },
                "version {version}"
            );
        }
    }

    #[test]
    fn disaster_reference_decoder_rejects_active_records_and_truncation() {
        let bytes = [
            0x00, 0x01, // Two slots.
            0x00, 0x00, 0x00, 0x01, // One active flag.
        ];
        assert!(DisasterManagerReferenceData::decode(293, &bytes).is_err());

        let empty = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x34];
        for end in 0..empty.len() {
            assert!(
                DisasterManagerReferenceData::decode(290, &empty[..end]).is_err(),
                "{end}"
            );
        }
    }
}
