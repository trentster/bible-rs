//! Reading a SWORD `zLD` lexicon module, and turning its markup into plain dictionary text.
//!
//! CrossWire ships Easton's and Webster's as `zLD` modules: an index of entries, a key file, and
//! zlib blocks of XML. This reads one once, at build time, so the app never carries a zLD reader.
//!
//! The layout, as the module's own bytes show it:
//!
//! - `<name>.idx` — 8 bytes an entry: `u32` offset into `.dat`, `u32` length there.
//! - `<name>.dat` — an entry a record: `key\r\n`, then `u32` block, `u32` index within the block.
//! - `<name>.zdx` — 8 bytes a block: `u32` offset into `.zdt`, `u32` compressed length.
//! - `<name>.zdt` — zlib blocks. Uncompressed, a block is `u32` count, then count × (`u32`
//!   offset, `u32` length) pointing past the table, then the entry text itself.
//!
//! All numbers are little-endian. Blocks are 2 bytes apart in `.zdt`, which this reader never needs
//! to know because `.zdx` gives each block's offset and length outright.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};

/// Every complete entry of a module, as `(key, markup)`.
pub fn entries(idx: &[u8], dat: &[u8], zdx: &[u8], zdt: &[u8]) -> Result<Vec<(String, String)>> {
    if idx.len() % 8 != 0 {
        bail!("a zLD index is a whole number of 8-byte entries, not {} bytes", idx.len());
    }
    let mut blocks: HashMap<u32, Vec<u8>> = HashMap::new();
    let mut out = Vec::with_capacity(idx.len() / 8);
    for record in idx.chunks_exact(8) {
        let start = le(record, 0) as usize;
        let len = le(record, 4) as usize;
        let end = start.checked_add(len).context("an entry runs past the end of .dat")?;
        let rec = dat.get(start..end).with_context(|| format!("entry at {start} is not in .dat"))?;
        let (key, rest) = rec.split_once_key()?;
        let block = le(rest, 0);
        let index = le(rest, 4);
        let data = blocks.entry(block).or_insert_with(|| inflate_block(zdx, zdt, block).unwrap_or_default());
        let key = String::from_utf8_lossy(key);
        let markup = block_entry(data, index)
            .with_context(|| format!("entry {} in block {block} is missing", key.trim()))?;
        out.push((key.trim().to_string(), markup));
    }
    Ok(out)
}

/// The key a `.dat` record starts with, and the eight bytes that follow it.
trait SplitKey {
    fn split_once_key(&self) -> Result<(&[u8], &[u8])>;
}

impl SplitKey for [u8] {
    fn split_once_key(&self) -> Result<(&[u8], &[u8])> {
        // Records end their key with CRLF and pad the short ones out to eight bytes of numbers.
        let at = self.windows(2).position(|w| w == b"\r\n").context("a .dat record has no key")?;
        let rest = &self[at + 2..];
        if rest.len() < 8 {
            bail!("a .dat record's key is not followed by a block and index");
        }
        Ok((&self[..at], &rest[..8]))
    }
}

/// One block of a module, decompressed.
fn inflate_block(zdx: &[u8], zdt: &[u8], block: u32) -> Result<Vec<u8>> {
    let at = block as usize * 8;
    let start = le(zdx, at) as usize;
    let len = le(zdx, at + 4) as usize;
    let end = start.checked_add(len).context("a block runs past the end of .zdt")?;
    let raw = zdt.get(start..end).with_context(|| format!("block {block} is not in .zdt"))?;
    // These blocks are zlib streams: ZlibDecoder takes the header and the checksum for us.
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(raw), &mut out)
        .context("a compressed block would not decompress")?;
    Ok(out)
}

/// The text of entry `index` in a decompressed block.
fn block_entry(block: &[u8], index: u32) -> Option<String> {
    let count = le(block, 0);
    if index >= count {
        return None;
    }
    let at = 4 + index as usize * 8;
    let start = le(block, at) as usize;
    let len = le(block, at + 4) as usize;
    let end = start.checked_add(len)?;
    Some(String::from_utf8_lossy(block.get(start..end)?).into_owned())
}

fn le(bytes: &[u8], at: usize) -> u32 {
    bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0)
}

/// The headword and plain text of an entry, with its markup removed.
///
/// The module is TEI: `<entryFree n="Aaron"><title>Aaron</title><p>…</p></entryFree>`, with
/// `<ref osisRef="Bible:Exod.4.14">Ex. 4:14</ref>` for scripture links. A reader wants the words,
/// not the tags, so the reference keeps its own text and everything else goes.
pub fn plain(markup: &str) -> (String, String) {
    let headword = between(markup, "<title>", "</title>")
        .or_else(|| attribute(markup, "n=\""))
        .map(|s| clean(&tidy(&unescape(&s))))
        .unwrap_or_default();
    let body = match markup.find("</title>") {
        Some(at) => &markup[at + "</title>".len()..],
        None => markup,
    };
    let body = body.replace("<p>", "\n").replace("<p/>", "\n").replace("</p>", "\n");
    let body = strip_tags(&body);
    let mut paragraphs: Vec<String> =
        body.split('\n').map(|p| clean(&tidy(&unescape(p)))).filter(|p| !p.is_empty()).collect();
    paragraphs.dedup();
    (headword, paragraphs.join("\n"))
}

/// What sits between two markers.
fn between(s: &str, open: &str, close: &str) -> Option<String> {
    let start = s.find(open)? + open.len();
    let end = s[start..].find(close)? + start;
    Some(s[start..end].to_string())
}

/// A quoted attribute's value, given the text just before the quote.
fn attribute(s: &str, before: &str) -> Option<String> {
    let start = s.find(before)? + before.len();
    let end = s[start..].find('"')? + start;
    Some(s[start..end].to_string())
}

/// The text with every `<tag …>` removed.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// The five XML entities and the numeric ones these modules use.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let Some(end) = rest[at..].find(';').map(|e| e + at) else {
            out.push_str(&rest[at..]);
            return out;
        };
        let entity = &rest[at + 1..end];
        let c = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if entity.starts_with("#x") => {
                u32::from_str_radix(&entity[2..], 16).ok().and_then(char::from_u32)
            }
            _ if entity.starts_with('#') => entity[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match c {
            Some(c) => out.push(c),
            None => out.push_str(&rest[at..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// One line with its runs of space collapsed.
fn tidy(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text without control characters, which is how a module pads the end of an entry.
fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block as the format has it: a count, a table, then the entries.
    fn block(items: &[&str]) -> Vec<u8> {
        let mut table = Vec::new();
        let mut data = Vec::new();
        let start = 4 + items.len() * 8;
        for s in items {
            table.extend_from_slice(&((start + data.len()) as u32).to_le_bytes());
            table.extend_from_slice(&(s.len() as u32).to_le_bytes());
            data.extend_from_slice(s.as_bytes());
        }
        let mut out = (items.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(&table);
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn finds_an_entry_by_its_index_in_the_block() {
        let b = block(&["<entryFree n=\"A\">A</entryFree>", "<entryFree n=\"B\">B</entryFree>"]);
        assert_eq!(block_entry(&b, 0).unwrap(), "<entryFree n=\"A\">A</entryFree>");
        assert_eq!(block_entry(&b, 1).unwrap(), "<entryFree n=\"B\">B</entryFree>");
        assert_eq!(block_entry(&b, 2), None, "there is no third entry");
    }

    #[test]
    fn a_key_record_gives_up_its_block_and_index() {
        let mut rec = b"AARON\r\n".to_vec();
        rec.extend_from_slice(&0u32.to_le_bytes());
        rec.extend_from_slice(&1u32.to_le_bytes());
        let (key, rest) = rec.split_once_key().unwrap();
        assert_eq!(key, b"AARON");
        assert_eq!((le(rest, 0), le(rest, 4)), (0, 1));
    }

    #[test]
    fn a_record_without_its_numbers_is_refused() {
        assert!(b"AARON\r\n\x00\x00".split_once_key().is_err());
    }

    #[test]
    fn eastons_markup_becomes_a_headword_and_words() {
        let markup = "<entryFree n=\"Aaron\">\n<title>Aaron</title>\n<p>a teacher, or lofty, the eldest son of \
                      Amram and Jochebed (<ref osisRef=\"Bible:Exod.6.20\">Ex. 6:20</ref>).</p>\n<p>See \
                      <ref osisRef=\"Bible:Num.17.8\">Num. 17:8</ref>.</p>\n</entryFree>";
        let (headword, body) = plain(markup);
        assert_eq!(headword, "Aaron");
        assert_eq!(
            body,
            "a teacher, or lofty, the eldest son of Amram and Jochebed (Ex. 6:20).\nSee Num. 17:8."
        );
        assert!(!body.contains('<'), "no markup survives: {body}");
    }

    #[test]
    fn entities_and_run_together_spaces_are_cleaned_up() {
        let (_, body) = plain("<title>X</title><p>one   &amp;  two &lt;three&gt; &#8212; four</p>");
        assert_eq!(body, "one & two <three> — four");
    }

    #[test]
    fn the_nul_that_pads_an_entry_never_reaches_the_reader() {
        // Each entry in a module is padded with a NUL, inside its recorded length.
        let (headword, body) = plain("<title>Rest</title><p>quietness.\u{0}</p>\u{0}");
        assert_eq!(headword, "Rest");
        assert_eq!(body, "quietness.");
    }

    #[test]
    fn an_entry_with_no_body_still_gives_its_headword() {
        let (headword, body) = plain("<entryFree n=\"Zuzims\"><title>Zuzims</title></entryFree>");
        assert_eq!(headword, "Zuzims");
        assert_eq!(body, "");
    }
}
