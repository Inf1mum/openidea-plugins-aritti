//! Read-only classic ZIP subset. No filesystem writes or archive extraction.
use openidea_plugin::Error;
use std::{collections::BTreeSet, io::Read};
pub const MAX_ARCHIVE: usize = 8 * 1024 * 1024;
const MAX_META: usize = 512 * 1024;
const MAIN: &str = "META-INF/spring-configuration-metadata.json";
const EXTRA: &str = "META-INF/additional-spring-configuration-metadata.json";
fn bad() -> Error {
    Error::Invalid("Unsupported, malformed or oversized JAR/metadata archive.".into())
}
fn u16at(b: &[u8], at: usize) -> Result<usize, Error> {
    Ok(u16::from_le_bytes(b.get(at..at + 2).ok_or_else(bad)?.try_into().map_err(|_| bad())?) as usize)
}
fn u32at(b: &[u8], at: usize) -> Result<usize, Error> {
    Ok(u32::from_le_bytes(b.get(at..at + 4).ok_or_else(bad)?.try_into().map_err(|_| bad())?) as usize)
}
fn sig(b: &[u8], at: usize, s: &[u8]) -> bool {
    b.get(at..at + 4) == Some(s)
}
pub fn metadata(bytes: &[u8]) -> Result<Vec<(String, String)>, Error> {
    if bytes.len() < 22 || bytes.len() > MAX_ARCHIVE {
        return Err(bad());
    }
    let eocd = (bytes.len().saturating_sub(4118)..=bytes.len() - 22)
        .rev()
        .find(|&at| sig(bytes, at, b"PK\x05\x06") && u16at(bytes, at + 20).is_ok_and(|n| at + 22 + n == bytes.len()))
        .ok_or_else(bad)?;
    let count = u16at(bytes, eocd + 10)?;
    let central_size = u32at(bytes, eocd + 12)?;
    let central = u32at(bytes, eocd + 16)?;
    if u16at(bytes, eocd + 4)? != 0
        || u16at(bytes, eocd + 6)? != 0
        || u16at(bytes, eocd + 8)? != count
        || count == 0
        || count > 2048
        || central_size > 1024 * 1024
        || central.checked_add(central_size) != Some(eocd)
    {
        return Err(bad());
    }
    let mut at = central;
    let mut seen = BTreeSet::new();
    let mut expanded = 0usize;
    let mut metadata_expanded = 0usize;
    let mut out = vec![];
    let mut spans = vec![];
    for _ in 0..count {
        if !sig(bytes, at, b"PK\x01\x02") {
            return Err(bad());
        }
        let flags = u16at(bytes, at + 8)?;
        let method = u16at(bytes, at + 10)?;
        let crc = u32at(bytes, at + 16)? as u32;
        let compressed = u32at(bytes, at + 20)?;
        let size = u32at(bytes, at + 24)?;
        let names = u16at(bytes, at + 28)?;
        let extra = u16at(bytes, at + 30)?;
        let comment = u16at(bytes, at + 32)?;
        let local = u32at(bytes, at + 42)?;
        if local >= central {
            return Err(bad());
        }
        if u16at(bytes, at + 6)? > 20
            || u16at(bytes, local + 4)? > 20
            || flags & !0x080e != 0
            || !matches!(method, 0 | 8)
            || names == 0
            || names > 256
            || extra > 4096
            || comment > 4096
            || u16at(bytes, at + 34)? != 0
            || compressed == u32::MAX as usize
            || size == u32::MAX as usize
            || local >= central
        {
            return Err(bad());
        }
        let end = at.checked_add(46 + names + extra + comment).filter(|end| *end <= eocd).ok_or_else(bad)?;
        let name = std::str::from_utf8(bytes.get(at + 46..at + 46 + names).ok_or_else(bad)?).map_err(|_| bad())?;
        if name.starts_with('/')
            || name.contains(['\\', ':'])
            || name.chars().any(char::is_control)
            || name.trim_end_matches('/').split('/').any(|p| p.is_empty() || p == "." || p == "..")
            || !seen.insert(name.to_string())
        {
            return Err(bad());
        }
        expanded = expanded.checked_add(size).filter(|n| *n <= 64 * 1024 * 1024).ok_or_else(bad)?;
        if !sig(bytes, local, b"PK\x03\x04")
            || u16at(bytes, local + 6)? != flags
            || u16at(bytes, local + 8)? != method
            || u16at(bytes, local + 26)? != names
        {
            return Err(bad());
        }
        let local_extra = u16at(bytes, local + 28)?;
        if local_extra > 4096 {
            return Err(bad());
        }
        let start = local.checked_add(30 + names + local_extra).ok_or_else(bad)?;
        let stop = start.checked_add(compressed).filter(|n| *n <= central).ok_or_else(bad)?;
        let region_stop = if flags & 8 != 0 {
            let descriptor = stop + if sig(bytes, stop, b"PK\x07\x08") { 4 } else { 0 };
            if u32at(bytes, descriptor)? != crc as usize
                || u32at(bytes, descriptor + 4)? != compressed
                || u32at(bytes, descriptor + 8)? != size
                || descriptor + 12 > central
            {
                return Err(bad());
            }
            descriptor + 12
        } else {
            stop
        };
        spans.push((local, region_stop));
        if bytes.get(local + 30..local + 30 + names) != Some(name.as_bytes()) {
            return Err(bad());
        }
        if flags & 8 == 0
            && (u32at(bytes, local + 14)? != crc as usize || u32at(bytes, local + 18)? != compressed || u32at(bytes, local + 22)? != size)
        {
            return Err(bad());
        }
        if matches!(name, MAIN | EXTRA) {
            if size > MAX_META
                || compressed == 0
                || size > compressed.saturating_mul(100)
                || ((u32at(bytes, at + 38)? as u32 >> 16) & 0xf000) == 0xa000
            {
                return Err(bad());
            }
            metadata_expanded += size;
            if metadata_expanded > 768 * 1024 {
                return Err(bad());
            }
            let payload = &bytes[start..stop];
            let data = if method == 0 {
                if size != compressed {
                    return Err(bad());
                }
                payload.to_vec()
            } else {
                let mut decoder = flate2::bufread::DeflateDecoder::new(payload);
                let mut data = vec![];
                decoder.by_ref().take((MAX_META + 1) as u64).read_to_end(&mut data).map_err(|_| bad())?;
                if decoder.total_in() != compressed as u64 {
                    return Err(bad());
                }
                data
            };
            if data.len() != size || crc32fast::hash(&data) != crc {
                return Err(bad());
            }
            out.push((name.to_string(), String::from_utf8(data).map_err(|_| bad())?));
        }
        at = end;
    }
    spans.sort_unstable();
    if spans.first().is_none_or(|s| s.0 != 0) || spans.windows(2).any(|w| w[0].1 > w[1].0) {
        return Err(bad());
    }
    if at != eocd || out.is_empty() {
        return Err(bad());
    }
    out.sort_by_key(|(name, _)| name == EXTRA);
    Ok(out)
}

#[cfg(test)]
pub(crate) fn fixture(entries: &[(&str, &[u8], u16)]) -> Vec<u8> {
    use std::io::Write;
    fn n16(out: &mut Vec<u8>, v: u16) {
        out.extend(v.to_le_bytes());
    }
    fn n32(out: &mut Vec<u8>, v: u32) {
        out.extend(v.to_le_bytes());
    }
    let mut bytes = vec![];
    let mut central = vec![];
    for (name, data, method) in entries {
        let compressed = if *method == 8 {
            let mut encoder = flate2::write::DeflateEncoder::new(vec![], flate2::Compression::default());
            encoder.write_all(data).unwrap();
            encoder.finish().unwrap()
        } else {
            data.to_vec()
        };
        let offset = bytes.len() as u32;
        let crc = crc32fast::hash(data);
        bytes.extend(b"PK\x03\x04");
        for n in [20, 0, *method, 0, 0] {
            n16(&mut bytes, n);
        }
        for n in [crc, compressed.len() as u32, data.len() as u32] {
            n32(&mut bytes, n);
        }
        n16(&mut bytes, name.len() as u16);
        n16(&mut bytes, 0);
        bytes.extend(name.as_bytes());
        bytes.extend(&compressed);
        central.extend(b"PK\x01\x02");
        for n in [20, 20, 0, *method, 0, 0] {
            n16(&mut central, n);
        }
        for n in [crc, compressed.len() as u32, data.len() as u32] {
            n32(&mut central, n);
        }
        for n in [name.len() as u16, 0, 0, 0, 0] {
            n16(&mut central, n);
        }
        n32(&mut central, 0);
        n32(&mut central, offset);
        central.extend(name.as_bytes());
    }
    let at = bytes.len() as u32;
    let size = central.len() as u32;
    bytes.extend(central);
    bytes.extend(b"PK\x05\x06");
    for n in [0, 0, entries.len() as u16, entries.len() as u16] {
        n16(&mut bytes, n);
    }
    n32(&mut bytes, size);
    n32(&mut bytes, at);
    n16(&mut bytes, 0);
    bytes
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stored_and_deflated_metadata_only_no_nested_layout_guessing() {
        for method in [0, 8] {
            let bytes = fixture(&[(MAIN, b"{\"properties\":[]}", method), ("other.class", b"not read", method)]);
            assert_eq!(metadata(&bytes).unwrap(), [(MAIN.into(), "{\"properties\":[]}".into())]);
        }
        assert!(metadata(&fixture(&[("BOOT-INF/classes/META-INF/spring-configuration-metadata.json", b"{}", 0)])).is_err());
    }
    #[test]
    fn malformed_truncated_crc_duplicate_and_path_tricks_are_rejected() {
        let good = fixture(&[(MAIN, b"{\"properties\":[]}", 8)]);
        for length in [0, 1, 21, good.len() - 1] {
            assert!(metadata(&good[..length]).is_err());
        }
        for name in ["../evil", "/absolute", "a/../b", "a\\b", "a//b"] {
            assert!(metadata(&fixture(&[(MAIN, b"{}", 0), (name, b"x", 0)])).is_err());
        }
        assert!(metadata(&fixture(&[(MAIN, b"{}", 0), (MAIN, b"{}", 0)])).is_err());
        let mut bytes = good.clone();
        bytes[30 + MAIN.len()] ^= 1;
        assert!(metadata(&bytes).is_err());
        let mut bytes = good;
        bytes[8] = 99;
        assert!(metadata(&bytes).is_err());
    }
    #[test]
    fn compressed_bombs_encryption_multidisk_and_entry_bounds_fail_closed() {
        assert!(metadata(&fixture(&[(MAIN, &vec![b' '; MAX_META + 1], 8)])).is_err());
        assert!(metadata(&fixture(&[(MAIN, &vec![b' '; 100_000], 8)])).is_err());
        let mut b = fixture(&[(MAIN, b"{}", 0)]);
        let e = b.len() - 22;
        b[e + 4] = 1;
        assert!(metadata(&b).is_err());
        let mut b = fixture(&[(MAIN, b"{}", 0)]);
        b[6] = 1;
        assert!(metadata(&b).is_err());
        assert!(metadata(&vec![0; MAX_ARCHIVE + 1]).is_err());
        let mut b = fixture(&[(MAIN, b"{}", 0)]);
        let e = b.len() - 22;
        b[e + 8..e + 12].copy_from_slice(&[1, 8, 1, 8]);
        assert!(metadata(&b).is_err());
    }
    #[test]
    fn independently_generated_python_zipfile_archive_is_readable() {
        let result = metadata(include_bytes!("../tests/fixtures/config-metadata.zip")).unwrap();
        assert_eq!(result[0].0, MAIN);
        assert!(result[0].1.contains("fixture.independent"));
    }
}
