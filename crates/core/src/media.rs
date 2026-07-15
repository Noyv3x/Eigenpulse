use std::path::PathBuf;
use std::sync::OnceLock;

/// Maximum prefix read when identifying a Fitness demonstration object.
///
/// GIF needs six bytes, while ISO-BMFF `ftyp` and EBML headers are variable
/// length. Keeping one bounded probe size across upload, backup and restore
/// prevents those trust boundaries from disagreeing about the same bytes.
pub const MEDIA_FORMAT_PROBE_BYTES: usize = 4 * 1024;

/// Binary formats accepted for exercise demonstrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaFormat {
    Gif,
    Mp4,
    Webm,
}

impl MediaFormat {
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Gif => "gif",
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
        }
    }

    pub const fn extension(self) -> &'static str {
        self.media_type()
    }
}

/// Identify a supported exercise-media file from a bounded prefix.
///
/// This deliberately does more than check a marker at a fixed offset:
/// ISO-BMFF files must contain a complete, well-formed first `ftyp` box with a
/// video-compatible brand, and WebM files must contain a complete EBML header
/// whose DocType is exactly `webm`.
pub fn detect_media_format(probe: &[u8]) -> Option<MediaFormat> {
    if probe.starts_with(b"GIF87a") || probe.starts_with(b"GIF89a") {
        return Some(MediaFormat::Gif);
    }
    if is_mp4_ftyp(probe) {
        return Some(MediaFormat::Mp4);
    }
    if is_webm_ebml_header(probe) {
        return Some(MediaFormat::Webm);
    }
    None
}

fn is_mp4_ftyp(probe: &[u8]) -> bool {
    let Some(size32) = probe
        .get(..4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map(u32::from_be_bytes)
    else {
        return false;
    };
    if probe.get(4..8) != Some(b"ftyp") {
        return false;
    }

    let (box_size, header_size) = if size32 == 1 {
        let Some(size64) = probe
            .get(8..16)
            .and_then(|bytes| <[u8; 8]>::try_from(bytes).ok())
            .map(u64::from_be_bytes)
        else {
            return false;
        };
        let Ok(size) = usize::try_from(size64) else {
            return false;
        };
        (size, 16)
    } else {
        (size32 as usize, 8)
    };

    // The payload is major_brand + minor_version + zero or more compatible
    // brands. Size zero (to EOF), undersized and partially probed boxes are not
    // accepted because their declared structure cannot be verified here.
    let minimum_size = header_size + 8;
    if box_size < minimum_size || box_size > probe.len() || (box_size - minimum_size) % 4 != 0 {
        return false;
    }
    let brands = std::iter::once(&probe[header_size..header_size + 4])
        .chain(probe[header_size + 8..box_size].chunks_exact(4));
    let brands = brands.collect::<Vec<_>>();
    if brands.iter().any(|brand| is_still_image_brand(brand)) {
        return false;
    }
    brands.iter().any(|brand| is_mp4_video_brand(brand))
}

fn is_still_image_brand(brand: &[u8]) -> bool {
    matches!(
        brand,
        b"avif"
            | b"avis"
            | b"heic"
            | b"heix"
            | b"hevc"
            | b"hevx"
            | b"heim"
            | b"heis"
            | b"hevm"
            | b"hevs"
            | b"mif1"
            | b"msf1"
    )
}

fn is_mp4_video_brand(brand: &[u8]) -> bool {
    matches!(
        brand,
        b"isom"
            | b"iso2"
            | b"iso3"
            | b"iso4"
            | b"iso5"
            | b"iso6"
            | b"iso7"
            | b"iso8"
            | b"iso9"
            | b"mp41"
            | b"mp42"
            | b"avc1"
            | b"av01"
            | b"vp09"
            | b"hvc1"
            | b"hev1"
            | b"M4V "
            | b"M4VH"
            | b"M4VP"
            | b"3gp4"
            | b"3gp5"
            | b"3gp6"
            | b"3gp7"
            | b"3g2a"
            | b"3g2b"
            | b"dash"
            | b"msdh"
            | b"msix"
    )
}

fn is_webm_ebml_header(probe: &[u8]) -> bool {
    const EBML_ID: u64 = 0x1a45_dfa3;
    const DOC_TYPE_ID: u64 = 0x4282;

    let Some((id, id_len)) = parse_ebml_id(probe) else {
        return false;
    };
    if id != EBML_ID {
        return false;
    }
    let Some((payload_size, size_len)) = parse_ebml_size(&probe[id_len..]) else {
        return false;
    };
    let payload_start = id_len + size_len;
    let Some(payload_end) = payload_start.checked_add(payload_size) else {
        return false;
    };
    if payload_end > probe.len() {
        return false;
    }

    let mut cursor = payload_start;
    let mut found_doc_type = false;
    while cursor < payload_end {
        let Some((element_id, element_id_len)) = parse_ebml_id(&probe[cursor..payload_end]) else {
            return false;
        };
        cursor += element_id_len;
        let Some((element_size, element_size_len)) = parse_ebml_size(&probe[cursor..payload_end])
        else {
            return false;
        };
        cursor += element_size_len;
        let Some(element_end) = cursor.checked_add(element_size) else {
            return false;
        };
        if element_end > payload_end {
            return false;
        }
        if element_id == DOC_TYPE_ID {
            if found_doc_type || probe.get(cursor..element_end) != Some(b"webm") {
                return false;
            }
            found_doc_type = true;
        }
        cursor = element_end;
    }
    found_doc_type
}

fn parse_ebml_id(bytes: &[u8]) -> Option<(u64, usize)> {
    let first = *bytes.first()?;
    let length = first.leading_zeros() as usize + 1;
    if first == 0 || length > 4 || bytes.len() < length {
        return None;
    }
    let value = bytes[..length]
        .iter()
        .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte));
    Some((value, length))
}

fn parse_ebml_size(bytes: &[u8]) -> Option<(usize, usize)> {
    let first = *bytes.first()?;
    let length = first.leading_zeros() as usize + 1;
    if first == 0 || length > 8 || bytes.len() < length {
        return None;
    }
    let marker = 1_u8 << (8 - length);
    let value = bytes[1..length]
        .iter()
        .fold(u64::from(first & !marker), |value, byte| {
            (value << 8) | u64::from(*byte)
        });
    let unknown = (1_u64 << (7 * length)) - 1;
    if value == unknown {
        return None;
    }
    Some((usize::try_from(value).ok()?, length))
}

/// Root for module-owned binary assets. Docker sets this to `/data/modules`;
/// local development defaults to `data/modules` beside the SQLite file.
pub fn module_data_root() -> PathBuf {
    std::env::var_os("EP_MODULE_DATA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/modules"))
}

/// Serialises media publication/deletion with backup archive creation.
pub fn module_data_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::{detect_media_format, MediaFormat};

    fn ftyp(major: &[u8; 4], compatible: &[[u8; 4]]) -> Vec<u8> {
        let size = 16 + compatible.len() * 4;
        let mut bytes = Vec::with_capacity(size);
        bytes.extend_from_slice(&(size as u32).to_be_bytes());
        bytes.extend_from_slice(b"ftyp");
        bytes.extend_from_slice(major);
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        for brand in compatible {
            bytes.extend_from_slice(brand);
        }
        bytes
    }

    fn ebml_header(doc_type: &[u8]) -> Vec<u8> {
        let payload_size = 3 + doc_type.len();
        assert!(payload_size < 127);
        let mut bytes = vec![0x1a, 0x45, 0xdf, 0xa3, 0x80 | payload_size as u8];
        bytes.extend_from_slice(&[0x42, 0x82, 0x80 | doc_type.len() as u8]);
        bytes.extend_from_slice(doc_type);
        bytes
    }

    #[test]
    fn recognizes_gif_signatures() {
        assert_eq!(detect_media_format(b"GIF87a..."), Some(MediaFormat::Gif));
        assert_eq!(detect_media_format(b"GIF89a..."), Some(MediaFormat::Gif));
        assert_eq!(detect_media_format(b"GIF88a..."), None);
    }

    #[test]
    fn parses_complete_video_ftyp_boxes() {
        assert_eq!(
            detect_media_format(&ftyp(b"isom", &[*b"iso6", *b"mp42"])),
            Some(MediaFormat::Mp4)
        );
        assert_eq!(
            detect_media_format(&ftyp(b"zzzz", &[*b"avc1"])),
            Some(MediaFormat::Mp4)
        );

        let mut extended = Vec::new();
        extended.extend_from_slice(&1_u32.to_be_bytes());
        extended.extend_from_slice(b"ftyp");
        extended.extend_from_slice(&24_u64.to_be_bytes());
        extended.extend_from_slice(b"mp42");
        extended.extend_from_slice(&0_u32.to_be_bytes());
        assert_eq!(detect_media_format(&extended), Some(MediaFormat::Mp4));
    }

    #[test]
    fn rejects_malformed_or_image_ftyp_boxes() {
        let mut truncated = ftyp(b"isom", &[*b"mp42"]);
        truncated.truncate(16);
        assert_eq!(detect_media_format(&truncated), None);

        let mut undersized = ftyp(b"isom", &[]);
        undersized[..4].copy_from_slice(&12_u32.to_be_bytes());
        assert_eq!(detect_media_format(&undersized), None);

        assert_eq!(detect_media_format(&ftyp(b"avif", &[*b"mif1"])), None);
        assert_eq!(
            detect_media_format(&ftyp(b"isom", &[*b"mp42", *b"heic"])),
            None
        );
        assert_eq!(detect_media_format(&ftyp(b"zzzz", &[*b"yyyy"])), None);
    }

    #[test]
    fn requires_a_complete_webm_ebml_header() {
        assert_eq!(
            detect_media_format(&ebml_header(b"webm")),
            Some(MediaFormat::Webm)
        );
        assert_eq!(detect_media_format(&ebml_header(b"matroska")), None);

        let mut truncated = ebml_header(b"webm");
        truncated.pop();
        assert_eq!(detect_media_format(&truncated), None);

        let mut unknown_header_size = ebml_header(b"webm");
        unknown_header_size[4] = 0xff;
        assert_eq!(detect_media_format(&unknown_header_size), None);
    }

    #[test]
    fn finds_webm_doctype_beyond_the_old_short_probe() {
        let mut payload = vec![0xec, 0x94]; // 20-byte EBML Void element.
        payload.extend_from_slice(&[0_u8; 20]);
        payload.extend_from_slice(&[0x42, 0x82, 0x84]);
        payload.extend_from_slice(b"webm");
        let mut bytes = vec![
            0x1a,
            0x45,
            0xdf,
            0xa3,
            0x80 | u8::try_from(payload.len()).unwrap(),
        ];
        bytes.extend_from_slice(&payload);

        assert_eq!(detect_media_format(&bytes[..16]), None);
        assert_eq!(detect_media_format(&bytes), Some(MediaFormat::Webm));
    }
}
