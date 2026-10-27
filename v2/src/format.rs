//! Bounded structural checks and encoded-payload witnesses. No decoding/re-encoding.
use crate::{Result, MAX_FILE};
use sha2::{Digest, Sha256};

const PARTS: usize = 65_536;
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    pub kind: &'static str,
    pub witness: [u8; 32],
    pub dimensions: Option<(u32, u32)>,
    pub seconds: Option<f64>,
}
fn bad<T>(kind: &str) -> Result<T> {
    Err(format!("Malformed {kind} structure"))
}
fn be16(b: &[u8]) -> usize {
    u16::from_be_bytes(b[..2].try_into().unwrap()) as usize
}
fn be32(b: &[u8]) -> usize {
    u32::from_be_bytes(b[..4].try_into().unwrap()) as usize
}
fn le32(b: &[u8]) -> usize {
    u32::from_le_bytes(b[..4].try_into().unwrap()) as usize
}
fn add(hash: &mut Sha256, label: &[u8], bytes: &[u8]) {
    hash.update((label.len() as u64).to_le_bytes());
    hash.update(label);
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
fn parsed(
    kind: &'static str,
    hash: Sha256,
    dimensions: Option<(u32, u32)>,
    seconds: Option<f64>,
) -> Parsed {
    Parsed {
        kind,
        witness: hash.finalize().into(),
        dimensions,
        seconds,
    }
}
fn unavailable<T>(layout: &str) -> Result<T> {
    Err(format!("Witness unavailable: {layout}"))
}
/// Finite optional evidence, never a format-family admission list. Confirmed
/// malformed structures/work limits still fail; native ExifTool owns other layouts.
pub fn witness(b: &[u8]) -> Result<Option<Parsed>> {
    match inspect(b) {
        Ok(parsed) => Ok(Some(parsed)),
        Err(error) if error.starts_with("Witness unavailable:") => Ok(None),
        Err(error) => Err(error),
    }
}
pub fn inspect(b: &[u8]) -> Result<Parsed> {
    if b.len() as u64 > MAX_FILE {
        return Err("File exceeds 256 MiB structural-validation limit".into());
    }
    if b.starts_with(b"\xff\xd8") {
        jpeg(b)
    } else if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(b)
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        gif(b)
    } else if b.starts_with(b"RIFF") {
        riff(b)
    } else if b.starts_with(b"fLaC") {
        flac(b)
    } else if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        tiff(b)
    } else if b.starts_with(b"%PDF-") {
        pdf(b)
    } else if b.starts_with(b"ID3")
        || (b.first() == Some(&255) && b.get(1).is_some_and(|v| v & 0xe0 == 0xe0))
    {
        mp3(b)
    } else if b.len() >= 12 && matches!(&b[4..8], b"ftyp" | b"moov" | b"mdat" | b"wide" | b"free") {
        bmff(b)
    } else {
        unavailable("format outside finite structural witnesses")
    }
}
pub fn equivalent(before: &[u8], after: &[u8]) -> Result<Parsed> {
    let a = inspect(before)?;
    let b = inspect(after)?;
    if a.kind != b.kind {
        return Err("Metadata tool changed the file format; original retained".into());
    }
    if a.kind == "PDF" {
        if !after.starts_with(pdf_base(before)?) {
            return Err("PDF base revision must remain an exact byte prefix (ExifTool update-tail replacement); original retained".into());
        }
    } else if a.witness != b.witness {
        return Err(
            "Encoded content or required decode information changed; original retained".into(),
        );
    }
    Ok(b)
}
fn jpeg(b: &[u8]) -> Result<Parsed> {
    let mut h = Sha256::new();
    let mut p = 2;
    let mut dim = None;
    let mut scans = 0;
    for _ in 0..PARTS {
        if p >= b.len() || b[p] != 255 {
            return bad("JPEG marker");
        }
        while p < b.len() && b[p] == 255 {
            p += 1;
        }
        let marker = *b.get(p).ok_or("Truncated JPEG marker")?;
        p += 1;
        if marker == 0xd9 {
            if scans == 0 || dim.is_none() {
                return bad("JPEG end");
            }
            if p != b.len() {
                return unavailable("JPEG trailer outside scan witness");
            }
            return Ok(parsed("JPEG", h, dim, None));
        }
        if marker == 0 || marker == 0xd8 || (0xd0..=0xd7).contains(&marker) {
            return bad("JPEG marker order");
        }
        if marker == 1 {
            add(&mut h, &[marker], &[]);
            continue;
        }
        if p + 2 > b.len() {
            return bad("JPEG segment");
        }
        let len = be16(&b[p..]);
        if len < 2 || len > b.len() - p {
            return bad("JPEG segment length");
        }
        let data = &b[p + 2..p + len];
        if matches!(marker,0xc0..=0xc3|0xc5..=0xc7|0xc9..=0xcb|0xcd..=0xcf) {
            if data.len() < 6 || data.len() != 6 + data[5] as usize * 3 {
                return bad("JPEG frame");
            }
            let height = be16(&data[1..]) as u32;
            let width = be16(&data[3..]) as u32;
            if width == 0 || height == 0 {
                return bad("JPEG dimensions");
            }
            dim = Some((width, height));
        }
        // APP14 Adobe is required decode information; JFIF density, EXIF orientation and ICC
        // are deliberately removable because V1 explicitly requests all= + ICC=.
        if !(0xe0..=0xed).contains(&marker) && marker != 0xef && marker != 0xfe {
            add(&mut h, &[marker], data);
        }
        p += len;
        if marker == 0xda {
            if dim.is_none() || data.len() < 4 {
                return bad("JPEG scan");
            }
            scans += 1;
            let start = p;
            while p < b.len() {
                if b[p] != 255 {
                    p += 1;
                    continue;
                }
                let mut q = p + 1;
                while q < b.len() && b[q] == 255 {
                    q += 1;
                }
                let next = *b.get(q).ok_or("Truncated JPEG entropy marker")?;
                if next == 0 || (0xd0..=0xd7).contains(&next) {
                    p = q + 1;
                } else {
                    break;
                }
            }
            if p == b.len() {
                return bad("JPEG missing EOI");
            }
            add(&mut h, b"entropy", &b[start..p]);
        }
    }
    bad("JPEG work limit")
}
fn png(b: &[u8]) -> Result<Parsed> {
    let mut p = 8;
    let mut h = Sha256::new();
    let mut dim = None;
    let mut data = false;
    let mut ended_data = false;
    for i in 0..PARTS {
        if p + 12 > b.len() {
            return bad("PNG chunk");
        }
        let len = be32(&b[p..]);
        if len > b.len() - p - 12 {
            return bad("PNG chunk length");
        }
        let tag = &b[p + 4..p + 8];
        let bytes = &b[p + 8..p + 8 + len];
        if !tag.iter().all(u8::is_ascii_alphabetic) || tag[2] & 32 != 0 {
            return bad("PNG chunk type");
        }
        if crc32fast::hash(&b[p + 4..p + 8 + len]) != be32(&b[p + 8 + len..]) as u32 {
            return bad("PNG CRC");
        }
        if i == 0 && tag != b"IHDR" {
            return bad("PNG missing IHDR");
        }
        match tag {
            b"IHDR" => {
                if dim.is_some() || bytes.len() != 13 {
                    return bad("PNG IHDR");
                }
                let w = be32(bytes) as u32;
                let hh = be32(&bytes[4..]) as u32;
                if w == 0
                    || hh == 0
                    || w > 0x7fff_ffff
                    || hh > 0x7fff_ffff
                    || bytes[10] != 0
                    || bytes[11] != 0
                    || bytes[12] > 1
                {
                    return bad("PNG IHDR fields");
                }
                let depth = bytes[8];
                let color = bytes[9];
                if !match color {
                    0 => [1, 2, 4, 8, 16].contains(&depth),
                    2 | 4 | 6 => [8, 16].contains(&depth),
                    3 => [1, 2, 4, 8].contains(&depth),
                    _ => false,
                } {
                    return bad("PNG bit depth");
                }
                dim = Some((w, hh));
                add(&mut h, tag, bytes);
            }
            b"IDAT" => {
                if ended_data {
                    return bad("PNG non-contiguous IDAT");
                }
                data = true;
                h.update(bytes);
            }
            b"IEND" => {
                if len != 0 || !data || p + 12 != b.len() {
                    return bad("PNG IEND");
                }
                return Ok(parsed("PNG", h, dim, None));
            }
            _ => {
                if data {
                    ended_data = true;
                }
                if matches!(tag, b"PLTE" | b"tRNS" | b"acTL" | b"fcTL" | b"fdAT")
                    || tag[0] & 32 == 0
                {
                    add(&mut h, tag, bytes);
                }
                // ExifTool may remove colour metadata as V1 all= requests it.
            }
        }
        p += len + 12;
    }
    bad("PNG work limit")
}
fn subblocks(b: &[u8], p: &mut usize) -> Result<()> {
    for _ in 0..PARTS {
        let len = *b.get(*p).ok_or("Truncated GIF block")? as usize;
        *p += 1;
        if len == 0 {
            return Ok(());
        }
        if len > b.len() - *p {
            return bad("GIF block length");
        }
        *p += len;
    }
    bad("GIF block work limit")
}
fn gif(b: &[u8]) -> Result<Parsed> {
    if b.len() < 14 {
        return bad("GIF header");
    }
    let mut p = 13;
    let mut h = Sha256::new();
    add(&mut h, b"screen", &b[6..13]);
    let w = u16::from_le_bytes(b[6..8].try_into().unwrap()) as u32;
    let hh = u16::from_le_bytes(b[8..10].try_into().unwrap()) as u32;
    if w == 0 || hh == 0 {
        return bad("GIF dimensions");
    }
    if b[10] & 128 != 0 {
        p += 3 * (1 << ((b[10] & 7) + 1));
    }
    if p > b.len() {
        return bad("GIF colour table");
    }
    add(&mut h, b"palette", &b[13..p]);
    let mut images = 0;
    for _ in 0..PARTS {
        let start = p;
        let marker = *b.get(p).ok_or("GIF missing trailer")?;
        p += 1;
        match marker {
            0x3b => {
                if p != b.len() || images == 0 {
                    return bad("GIF trailer");
                }
                return Ok(parsed("GIF", h, Some((w, hh)), None));
            }
            0x2c => {
                if p + 9 > b.len() {
                    return bad("GIF image descriptor");
                }
                let packed = b[p + 8];
                p += 9;
                if packed & 128 != 0 {
                    p += 3 * (1 << ((packed & 7) + 1));
                }
                if p >= b.len() || !(2..=8).contains(&b[p]) {
                    return bad("GIF LZW");
                }
                p += 1;
                subblocks(b, &mut p)?;
                add(&mut h, b"image", &b[start..p]);
                images += 1;
            }
            0x21 => {
                let label = *b.get(p).ok_or("GIF extension label")?;
                p += 1;
                subblocks(b, &mut p)?;
                if label == 0xf9
                    || label == 0x01
                    || (label == 0xff
                        && b[start..p]
                            .windows(11)
                            .any(|x| x == b"NETSCAPE2.0" || x == b"ANIMEXTS1.0"))
                {
                    add(&mut h, b"control", &b[start..p]);
                }
            }
            _ => return bad("GIF block marker"),
        }
    }
    bad("GIF work limit")
}
fn riff(b: &[u8]) -> Result<Parsed> {
    if b.len() < 12 || le32(&b[4..]) + 8 != b.len() {
        return bad("RIFF size");
    }
    let kind = match &b[8..12] {
        b"WEBP" => "WebP",
        b"WAVE" => "WAV",
        _ => return unavailable("RIFF layout other than WebP/WAV"),
    };
    let mut p = 12;
    let mut h = Sha256::new();
    let mut dim = None;
    let mut payload = false;
    let mut seconds = None;
    let mut rate = 0;
    for _ in 0..PARTS {
        if p == b.len() {
            if !payload {
                return bad("RIFF missing content");
            }
            return Ok(parsed(kind, h, dim, seconds));
        }
        if p + 8 > b.len() {
            return bad("RIFF chunk");
        }
        let n = le32(&b[p + 4..]);
        if n > b.len() - p - 8 {
            return bad("RIFF chunk length");
        }
        let tag = &b[p..p + 4];
        let v = &b[p + 8..p + 8 + n];
        if kind == "WebP" {
            match tag {
                b"VP8X" => {
                    if v.len() != 10 {
                        return bad("WebP VP8X");
                    }
                    let mut x = v.to_vec();
                    x[0] &= !0x2c;
                    if x[0] != 0 {
                        add(&mut h, tag, &x);
                    }
                    dim = Some((
                        1 + u32::from_le_bytes([v[4], v[5], v[6], 0]),
                        1 + u32::from_le_bytes([v[7], v[8], v[9], 0]),
                    ));
                }
                b"VP8 " | b"VP8L" | b"ALPH" | b"ANIM" | b"ANMF" => {
                    add(&mut h, tag, v);
                    payload |= matches!(tag, b"VP8 " | b"VP8L" | b"ANMF");
                }
                b"EXIF" | b"XMP " | b"ICCP" => (),
                _ => add(&mut h, tag, v),
            }
        } else {
            match tag {
                b"fmt " => {
                    if n < 16 {
                        return bad("WAV format");
                    }
                    rate = le32(&v[8..]);
                    if rate == 0 {
                        return bad("WAV byte rate");
                    }
                    add(&mut h, tag, v);
                }
                b"data" => {
                    if rate == 0 {
                        return bad("WAV fmt before data");
                    }
                    payload = true;
                    seconds = Some(n as f64 / rate as f64);
                    add(&mut h, tag, v);
                }
                b"fact" => add(&mut h, tag, v),
                _ => (),
            }
        }
        p += 8 + n + (n & 1);
        if p > b.len() {
            return bad("RIFF padding");
        }
    }
    bad("RIFF work limit")
}
fn flac(b: &[u8]) -> Result<Parsed> {
    let mut p = 4;
    let mut h = Sha256::new();
    let mut seconds = None;
    for i in 0..128 {
        if p + 4 > b.len() {
            return bad("FLAC block");
        }
        let tag = b[p];
        let n = be32(&[0, b[p + 1], b[p + 2], b[p + 3]]);
        p += 4;
        if n > b.len() - p {
            return bad("FLAC block length");
        }
        if i == 0 {
            if tag & 127 != 0 || n != 34 {
                return bad("FLAC STREAMINFO");
            }
            let v = &b[p..p + n];
            add(&mut h, b"streaminfo", v);
            let packed = u64::from_be_bytes(v[10..18].try_into().unwrap());
            let rate = packed >> 44;
            let samples = packed & 0x0f_ffff_ffff;
            if rate == 0 {
                return bad("FLAC sample rate");
            }
            seconds = Some(samples as f64 / rate as f64);
        } else if tag & 127 == 0 || tag & 127 == 127 {
            return bad("FLAC block type");
        }
        p += n;
        if tag & 128 != 0 {
            if p + 2 > b.len() || b[p] != 255 || b[p + 1] & 0xfe != 0xf8 {
                return bad("FLAC audio sync");
            }
            add(&mut h, b"frames", &b[p..]);
            return Ok(parsed("FLAC", h, None, seconds));
        }
    }
    bad("FLAC block limit")
}
fn mp3(b: &[u8]) -> Result<Parsed> {
    let mut p = 0;
    let mut end = b.len();
    if b.starts_with(b"ID3") {
        if b.len() < 10
            || !(2..=4).contains(&b[3])
            || b[4] == 255
            || b[6..10].iter().any(|x| x & 128 != 0)
        {
            return bad("ID3 header");
        }
        let n = b[6..10].iter().fold(0usize, |a, x| (a << 7) | *x as usize);
        p = 10 + n + if b[3] == 4 && b[5] & 16 != 0 { 10 } else { 0 };
        if p > end {
            return bad("ID3 size");
        }
    }
    if end >= 128 && &b[end - 128..end - 125] == b"TAG" {
        end -= 128;
    }
    if end >= 32 && &b[end - 32..end - 24] == b"APETAGEX" {
        let size = le32(&b[end - 20..]);
        if size < 32 || size > end - p {
            return bad("APE footer");
        }
        end -= size;
        if end >= 32 && &b[end - 32..end - 24] == b"APETAGEX" {
            end -= 32;
        }
    }
    let start = p;
    let mut seconds = 0.;
    let mut frames = 0;
    while p < end && frames < PARTS {
        if p + 4 > end || b[p] != 255 || b[p + 1] & 0xe0 != 0xe0 {
            return bad("MPEG audio sync");
        }
        let version = (b[p + 1] >> 3) & 3;
        let layer = (b[p + 1] >> 1) & 3;
        let br = (b[p + 2] >> 4) as usize;
        let sr = ((b[p + 2] >> 2) & 3) as usize;
        if version == 1 || layer == 0 || br == 0 || br == 15 || sr == 3 {
            return bad("MPEG audio header/free bitrate");
        }
        let rates = [44100, 48000, 32000];
        let rate = rates[sr]
            / if version == 3 {
                1
            } else if version == 2 {
                2
            } else {
                4
            };
        let tables = [
            [
                0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448,
            ],
            [
                0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
            ],
            [
                0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
            ],
        ];
        let low = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
        let low1 = [
            0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256,
        ];
        let kbps = if version == 3 {
            tables[(3 - layer) as usize][br]
        } else if layer == 3 {
            low1[br]
        } else {
            low[br]
        };
        let pad = ((b[p + 2] >> 1) & 1) as usize;
        let (length, samples) = if layer == 3 {
            ((12 * kbps * 1000 / rate + pad) * 4, 384)
        } else if layer == 1 && version != 3 {
            (72 * kbps * 1000 / rate + pad, 576)
        } else {
            (144 * kbps * 1000 / rate + pad, 1152)
        };
        if length < 4 || length > end - p {
            return bad("MPEG audio frame length");
        }
        seconds += samples as f64 / rate as f64;
        p += length;
        frames += 1;
    }
    if frames == 0 || p != end {
        return bad("MPEG audio frame limit");
    }
    let mut h = Sha256::new();
    add(&mut h, b"frames", &b[start..end]);
    Ok(parsed("MP3", h, None, Some(seconds)))
}
fn tiff(b: &[u8]) -> Result<Parsed> {
    let little = b.starts_with(b"II");
    let u16at = |p: usize| -> Result<usize> {
        let v = b.get(p..p + 2).ok_or("TIFF u16 bounds")?;
        Ok(if little {
            u16::from_le_bytes(v.try_into().unwrap())
        } else {
            u16::from_be_bytes(v.try_into().unwrap())
        } as usize)
    };
    let u32at = |p: usize| -> Result<usize> {
        let v = b.get(p..p + 4).ok_or("TIFF u32 bounds")?;
        Ok(if little {
            u32::from_le_bytes(v.try_into().unwrap())
        } else {
            u32::from_be_bytes(v.try_into().unwrap())
        } as usize)
    };
    let mut next = u32at(4)?;
    let mut seen = std::collections::HashSet::new();
    let mut h = Sha256::new();
    let mut dim = None;
    let mut work = 0usize;
    for _ in 0..128 {
        if next == 0 {
            if seen.is_empty() {
                return unavailable("TIFF without image IFD");
            }
            return Ok(parsed("TIFF", h, dim, None));
        }
        if !seen.insert(next) {
            return bad("TIFF IFD cycle");
        }
        let n = u16at(next)?;
        if n > 8192 || next.checked_add(6 + n * 12).is_none_or(|x| x > b.len()) {
            return bad("TIFF IFD bounds");
        }
        let mut values = std::collections::BTreeMap::new();
        let mut offsets = Vec::new();
        let mut lengths = Vec::new();
        for i in 0..n {
            let p = next + 2 + i * 12;
            let tag = u16at(p)?;
            let ty = u16at(p + 2)?;
            let count = u32at(p + 4)?;
            let unit = match ty {
                1 | 2 | 6 | 7 => 1,
                3 | 8 => 2,
                4 | 9 | 11 | 13 => 4,
                5 | 10 | 12 => 8,
                _ => return unavailable("TIFF field type outside finite witness"),
            };
            let size = count.checked_mul(unit).ok_or("TIFF field size overflow")?;
            let start = if size <= 4 { p + 8 } else { u32at(p + 8)? };
            work = work.checked_add(size).ok_or("TIFF work overflow")?;
            if work > b.len() * 2 {
                return bad("TIFF repeated/overlapping field work limit");
            }
            let v = b
                .get(start..start.checked_add(size).ok_or("TIFF data overflow")?)
                .ok_or("TIFF field bounds")?;
            let numeric = || -> Result<Vec<usize>> {
                if count > PARTS {
                    return bad("TIFF numeric count");
                }
                (0..count)
                    .map(|i| match ty {
                        3 => u16at(start + 2 * i),
                        4 => u32at(start + 4 * i),
                        _ => bad("TIFF numeric type"),
                    })
                    .collect()
            };
            match tag {
                256 | 257 => {
                    values.insert(tag, numeric()?);
                }
                273 | 324 => offsets = numeric()?,
                279 | 325 => lengths = numeric()?,
                // Required decode information. File-position offsets excluded.
                258 | 259 | 262 | 266 | 277 | 278 | 284 | 317 | 320 | 322 | 323 | 338 | 339
                | 347 | 512 | 515 | 517 | 518 | 519 | 520 | 521 | 529 | 530 | 531 | 532 => {
                    add(&mut h, &(tag as u16).to_le_bytes(), v);
                }
                330 => return unavailable("TIFF SubIFD"),
                _ => (),
            }
        }
        if let (Some(w), Some(hh)) = (
            values.get(&256).and_then(|v| v.first()),
            values.get(&257).and_then(|v| v.first()),
        ) {
            if *w == 0 || *hh == 0 {
                return bad("TIFF dimensions");
            }
            if dim.is_none() {
                dim = Some((*w as u32, *hh as u32));
            }
            add(
                &mut h,
                b"dimensions",
                &[(*w as u64).to_le_bytes(), (*hh as u64).to_le_bytes()].concat(),
            );
        }
        if offsets.is_empty() && lengths.is_empty() {
            return unavailable("TIFF image layout without ordinary strips/tiles");
        }
        if offsets.is_empty() || offsets.len() != lengths.len() {
            return bad("TIFF strip/tile arrays");
        }
        for (offset, len) in offsets.into_iter().zip(lengths) {
            work = work.checked_add(len).ok_or("TIFF payload work overflow")?;
            if work > b.len() * 2 {
                return bad("TIFF repeated payload work limit");
            }
            let v = b
                .get(offset..offset.checked_add(len).ok_or("TIFF strip overflow")?)
                .ok_or("TIFF strip bounds")?;
            add(&mut h, b"strip", v);
        }
        next = u32at(next + 2 + n * 12)?;
    }
    bad("TIFF IFD limit")
}
fn pdf_base(b: &[u8]) -> Result<&[u8]> {
    let tail = &b[b.len().saturating_sub(64)..];
    let end = b"%EndExifToolUpdate ";
    if let Some(at) = tail.windows(end.len()).rposition(|v| v == end) {
        let suffix = std::str::from_utf8(&tail[at + end.len()..])
            .map_err(|_| "Invalid PDF update footer")?;
        let fields: Vec<_> = suffix.split_ascii_whitespace().collect();
        if fields.len() != 4
            || fields[1] != "startxref"
            || fields[3] != "%%EOF"
            || fields[2].parse::<usize>().is_err()
        {
            return bad("PDF ExifTool footer");
        }
        let offset = fields[0]
            .parse::<usize>()
            .map_err(|_| "PDF update offset overflow")?;
        if offset > b.len() || !b[offset..].starts_with(b"%BeginExifToolUpdate") {
            return bad("PDF ExifTool update offset");
        }
        let base = &b[..offset];
        if !base
            .iter()
            .rev()
            .skip_while(|c| c.is_ascii_whitespace())
            .take(5)
            .copied()
            .eq(b"FOE%%".iter().copied())
        {
            return bad("PDF base EOF");
        }
        Ok(base)
    } else {
        Ok(b)
    }
}
fn pdf(b: &[u8]) -> Result<Parsed> {
    if b.len() < 20
        || !b[..8].starts_with(b"%PDF-1.") && !b[..8].starts_with(b"%PDF-2.")
        || !b[b.len().saturating_sub(1024)..]
            .windows(5)
            .any(|w| w == b"%%EOF")
    {
        return bad("PDF header/EOF");
    }
    // The authentic writer may replace its previous ExifTool update tail.
    // Every byte of the original base revision must survive; never redaction.
    Ok(parsed("PDF", Sha256::new(), None, None))
}
fn bmff(b: &[u8]) -> Result<Parsed> {
    struct Walk {
        h: Sha256,
        count: usize,
        offset_entries: usize,
        payload: bool,
        payload_hash: Sha256,
        dim: Option<(u32, u32)>,
    }
    fn boxes(
        b: &[u8],
        start: usize,
        end: usize,
        depth: usize,
        w: &mut Walk,
        ranges: &[(usize, usize)],
    ) -> Result<()> {
        if depth > 16 {
            return bad("ISO BMFF depth");
        }
        let mut p = start;
        while p < end {
            w.count += 1;
            if w.count > PARTS || p + 8 > end {
                return bad("ISO BMFF box limit/header");
            }
            let tag = &b[p + 4..p + 8];
            let mut size = be32(&b[p..]);
            let mut header = 8;
            if size == 1 {
                if p + 16 > end {
                    return bad("ISO BMFF extended size");
                }
                size = usize::try_from(u64::from_be_bytes(b[p + 8..p + 16].try_into().unwrap()))
                    .map_err(|_| "ISO BMFF size overflow")?;
                header = 16;
            }
            if size == 0 {
                size = end - p;
            }
            if size < header || size > end - p {
                return bad("ISO BMFF bounds");
            }
            let q = p + header;
            let stop = p + size;
            match tag {
                b"ftyp" => {
                    if stop - q < 8 {
                        return bad("ISO BMFF brand length");
                    }
                    if !matches!(
                        &b[q..q + 4],
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
                            | b"qt  "
                            | b"M4V "
                            | b"M4A "
                            | b"M4B "
                            | b"M4P "
                            | b"MSNV"
                    ) {
                        return unavailable("ISO BMFF brand outside ordinary MP4/MOV witness");
                    }
                }
                b"mvex" | b"moof" | b"traf" => return unavailable("fragmented ISO BMFF"),
                b"mdhd" | b"mvhd" => {
                    if stop - q < 4 {
                        return bad("ISO BMFF timing header");
                    }
                    let offset = match b[q] {
                        0 => 12,
                        1 => 20,
                        _ => return bad("ISO BMFF header version"),
                    };
                    let required = if b[q] == 0 { 8 } else { 12 };
                    if stop - q < offset + required {
                        return bad("ISO BMFF timing fields");
                    }
                    add(&mut w.h, tag, &b[q + offset..q + offset + required]);
                }
                b"mdat" => {
                    w.payload = true;
                    add(&mut w.payload_hash, b"mdat", &b[q..stop]);
                }
                b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"edts" | b"dinf" => {
                    boxes(b, q, stop, depth + 1, w, ranges)?
                }
                b"stsd" | b"stts" | b"ctts" | b"stsc" | b"stsz" | b"stz2" | b"stss" | b"sdtp"
                | b"elst" => add(&mut w.h, tag, &b[q..stop]),
                b"tkhd" => {
                    if size < header + 8 {
                        return bad("ISO BMFF tkhd");
                    }
                    let version = *b.get(q).ok_or("Missing track version")?;
                    let offset = match version {
                        0 => 12,
                        1 => 20,
                        _ => return bad("ISO BMFF track version"),
                    };
                    let length = if version == 0 { 28 } else { 32 };
                    if stop - q < offset + length + 44 {
                        return bad("ISO BMFF track header length");
                    }
                    add(
                        &mut w.h,
                        b"track-timing",
                        &b[q + offset..q + offset + length],
                    );
                    let width = be32(&b[stop - 8..]) as u32 >> 16;
                    let height = be32(&b[stop - 4..]) as u32 >> 16;
                    if width > 0 && height > 0 {
                        w.dim = Some((width, height));
                    }
                    add(&mut w.h, b"track-dimensions", &b[stop - 8..stop]);
                }
                // Chunk offsets are rewritten by ExifTool when moov moves. Their
                // targets are independently checked against mdat bounds below.
                b"stco" | b"co64" => {
                    if stop - q < 8 {
                        return bad("ISO BMFF offset table");
                    }
                    let n = be32(&b[q + 4..]);
                    let unit = if tag == b"stco" { 4 } else { 8 };
                    if n > PARTS || 8 + n * unit != stop - q {
                        return bad("ISO BMFF offset count");
                    }

                    w.offset_entries += n;
                    if w.offset_entries > 1_000_000 {
                        return bad("ISO BMFF aggregate offset-entry work limit");
                    }
                    for i in 0..n {
                        let at = q + 8 + i * unit;
                        let offset = if unit == 4 {
                            be32(&b[at..]) as u64
                        } else {
                            u64::from_be_bytes(b[at..at + 8].try_into().unwrap())
                        };
                        let index = ranges.partition_point(|(_, end)| *end as u64 <= offset);
                        let (start, end) = ranges
                            .get(index)
                            .ok_or("ISO BMFF chunk offset outside mdat")?;
                        if offset < *start as u64 || offset >= *end as u64 {
                            return bad("ISO BMFF chunk offset outside mdat");
                        }
                        let relative = offset - *start as u64;
                        add(
                            &mut w.h,
                            b"chunk",
                            &[(index as u64).to_le_bytes(), relative.to_le_bytes()].concat(),
                        );
                    }
                }
                _ => (),
            }
            p = stop;
        }
        Ok(())
    }
    fn mdat_ranges(b: &[u8]) -> Result<Vec<(usize, usize)>> {
        let mut out = Vec::new();
        let mut p = 0;
        let mut count = 0;
        while p < b.len() {
            count += 1;
            if count > PARTS || p + 8 > b.len() {
                return bad("BMFF top-level");
            }
            let mut n = be32(&b[p..]);
            let mut header = 8;
            if n == 1 {
                if p + 16 > b.len() {
                    return bad("BMFF wide box");
                }
                n = usize::try_from(u64::from_be_bytes(b[p + 8..p + 16].try_into().unwrap()))
                    .map_err(|_| "BMFF overflow")?;
                header = 16;
            }
            if n == 0 {
                n = b.len() - p;
            }
            if n < header || n > b.len() - p {
                return bad("BMFF box size");
            }
            if &b[p + 4..p + 8] == b"mdat" {
                out.push((p + header, p + n));
            }
            p += n;
        }
        Ok(out)
    }
    let mut walk = Walk {
        h: Sha256::new(),
        count: 0,
        offset_entries: 0,
        payload: false,
        payload_hash: Sha256::new(),
        dim: None,
    };
    let ranges = mdat_ranges(b)?;
    boxes(b, 0, b.len(), 0, &mut walk, &ranges)?;
    if !walk.payload {
        return bad("ISO BMFF missing mdat");
    }
    add(&mut walk.h, b"encoded-data", &walk.payload_hash.finalize());
    Ok(parsed("MP4/MOV", walk.h, walk.dim, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chunk(tag: &[u8], data: &[u8]) -> Vec<u8> {
        let mut v = (data.len() as u32).to_be_bytes().to_vec();
        v.extend(tag);
        v.extend(data);
        v.extend(crc32fast::hash(&v[4..]).to_be_bytes());
        v
    }
    pub fn png_fixture() -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        b.extend(chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]));
        b.extend(chunk(b"tEXt", b"Author\0Owned QA"));
        b.extend(chunk(
            b"IDAT",
            &[120, 156, 99, 248, 207, 192, 0, 0, 3, 1, 1, 0],
        ));
        b.extend(chunk(b"IEND", &[]));
        b
    }
    #[test]
    fn crc_and_payload() {
        let b = png_fixture();
        assert_eq!(inspect(&b).unwrap().dimensions, Some((1, 1)));
        let mut c = b.clone();
        c[20] ^= 1;
        assert!(inspect(&c).unwrap_err().contains("CRC"));
        let mut c = b.clone();
        let p = c.windows(4).position(|w| w == b"IDAT").unwrap();
        c[p + 5] ^= 1;
        let n = be32(&c[p - 4..]);
        let crc = crc32fast::hash(&c[p..p + 4 + n]);
        c[p + 4 + n..p + 8 + n].copy_from_slice(&crc.to_be_bytes());
        assert!(equivalent(&b, &c).is_err());
    }
    #[test]
    fn rejects_every_truncated_png() {
        let b = png_fixture();
        for n in 0..b.len() {
            assert!(inspect(&b[..n]).is_err(), "prefix {n}");
        }
    }
    #[test]
    fn rejects_forged_lengths() {
        for b in [
            b"RIFF\xff\xff\xff\xffWAVE".as_slice(),
            b"II*\0\xff\xff\xff\xff",
            b"\xff\xd8\xff\xe1\xff\xff",
            b"fLaC\x80\xff\xff\xff",
        ] {
            assert!(inspect(b).is_err());
        }
    }
}
