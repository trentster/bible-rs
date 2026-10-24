//! `cargo run -p xtask -- build-data`
//!
//! Downloads the public-domain USFM sources pinned below from eBible.org (into `.sources/`),
//! verifies their SHA-256, converts them with the same USFM importer users get, and writes
//! `data/translations/<id>.bbl`. Set `XTASK_ACCEPT_UPDATED_SOURCES=1` to accept a newer upstream
//! file (its new hash is printed so the pin can be updated).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use bible_core::format;
use bible_core::model::{Origin, TranslationBuilder, TranslationMeta};
use bible_core::usfm;
use sha2::{Digest, Sha256};

mod wordnet;
mod zld;

struct Source {
    id: &'static str,
    abbrev: &'static str,
    name: &'static str,
    ebible_id: &'static str,
    sha256: &'static str,
    license: &'static str,
    copyright: &'static str,
}

const SOURCES: &[Source] = &[
    Source {
        id: "kjv",
        abbrev: "KJV",
        name: "King James Version",
        ebible_id: "eng-kjv2006",
        sha256: "77bbe9c78f4f588b1465e49ddad986293a09e7f4b140abe7248232c8c5ae2a5b",
        license: "Public Domain (outside the United Kingdom)",
        copyright: "King James Version (1769 standardized text). Public domain outside the United Kingdom, where \
                    the Crown's letters patent apply.",
    },
    Source {
        id: "web",
        abbrev: "WEB",
        name: "World English Bible",
        ebible_id: "engwebp",
        sha256: "6b47e6553ce0bd71902fe4b4ba71a5d7d862928e2ce47936acdda345bc6c5dd0",
        license: "Public Domain",
        copyright: "The World English Bible is in the public domain. \u{201c}World English Bible\u{201d} is a \
                    trademark of eBible.org.",
    },
    Source {
        id: "asv",
        abbrev: "ASV",
        name: "American Standard Version",
        ebible_id: "eng-asv",
        sha256: "161ac2da2b0668cd7c6d3a2bffbf13aeb5c58b9a0ba3bfab0f576b232cb4adc8",
        license: "Public Domain",
        copyright: "American Standard Version (1901). Public domain.",
    },
    Source {
        id: "bsb",
        abbrev: "BSB",
        name: "Berean Standard Bible",
        ebible_id: "engbsb",
        sha256: "872c04668a29fe674c0013b6ebcbbcc81bc30dd0d9ae9f6e85bc53489749b37d",
        license: "Public Domain (dedicated April 30, 2023)",
        copyright: "The Holy Bible, Berean Standard Bible, BSB is produced in cooperation with Bible Hub, \
                    Discovery Bible, OpenBible.com, and the Berean Bible Translation Committee. This text of \
                    God's Word has been dedicated to the public domain.",
    },
    Source {
        id: "ylt",
        abbrev: "YLT",
        name: "Young's Literal Translation",
        ebible_id: "engylt",
        sha256: "987b38d903546e16af66caa69cab40c5abc1959db9ea64168531a497abc59d5c",
        license: "Public Domain",
        copyright: "Young's Literal Translation (Robert Young, 1898). Public domain.",
    },
    Source {
        id: "dby",
        abbrev: "DBY",
        name: "Darby Translation",
        ebible_id: "engDBY",
        sha256: "ebb1952e7ee657e6e45065178257009185ba89230de3c59d88e740ac60fd1a70",
        license: "Public Domain",
        copyright: "The Holy Scriptures, a New Translation from the Original Languages by J. N. Darby (1890). \
                    Public domain.",
    },
    Source {
        id: "wbs",
        abbrev: "WBS",
        name: "Webster Bible",
        ebible_id: "engwebster",
        sha256: "16748b10c224e0ec0acdf183b76741ba618e3b8f1c3a15f1a6464c04331a6b78",
        license: "Public Domain",
        copyright: "The Holy Bible, Containing the Old and New Testaments, in the Common Version, with \
                    Amendments of the Language by Noah Webster, LL.D. (1833). Public domain.",
    },
];

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    match args.first().map(String::as_str) {
        Some("build-data") => build_data(&root, &args[1..]),
        Some("missing") => {
            // Lists verses present in the first translation but absent from the others.
            let load = |id: &str| -> Result<bible_core::Translation> {
                format::decode(&std::fs::read(root.join(format!("data/translations/{id}.bbl")))?)
            };
            let base = load(&args[1])?;
            for other in &args[2..] {
                let t = load(other)?;
                let mut missing = Vec::new();
                for c in bible_core::ChapterIdx::all() {
                    for v in base.verses_in(c) {
                        if t.find(v.reference()).is_none() {
                            missing.push(v.reference().to_string());
                        }
                    }
                }
                println!("{other}: {} missing: {}", missing.len(), missing.join(", "));
            }
            Ok(())
        }
        Some("dump") => {
            let t = format::decode(&std::fs::read(root.join(format!("data/translations/{}.bbl", args[1])))?)?;
            let r = bible_core::reference::parse(&args[2]).context("bad reference")?;
            let c = r.chapter_idx();
            println!("{} — {:?}", c.display(), t.meta.books[c.book().index()]);
            for v in t.verses_in(c) {
                let marks: Vec<String> = v
                    .marks
                    .iter()
                    .map(|m| {
                        format!(
                            "{:?}{}@{}{}",
                            m.kind,
                            m.level,
                            m.offset,
                            if m.kind.has_text() {
                                format!("“{}”", t.mark_text(m))
                            } else {
                                String::new()
                            }
                        )
                    })
                    .collect();
                let spans: Vec<String> =
                    v.spans.iter().map(|s| format!("{:?}[{}..{}]", s.style, s.start, s.end)).collect();
                println!("{:>3} {}\n      marks {:?} spans {:?}", v.number, v.text, marks, spans);
            }
            Ok(())
        }
        Some("verse-table") => {
            let t = format::decode(&std::fs::read(root.join("data/translations/kjv.bbl"))?)?;
            let counts: Vec<String> =
                bible_core::ChapterIdx::all().map(|c| t.last_verse(c).to_string()).collect();
            for line in counts.chunks(30) {
                println!("    {},", line.join(", "));
            }
            Ok(())
        }
        Some("import") => {
            // Imports a file and compares it verse by verse with a bundled translation.
            let started = std::time::Instant::now();
            let imp = bible_core::import::import_path(Path::new(&args[1]), &Default::default())?;
            let t = imp.translation;
            println!(
                "{} — {} ({}) — {} verses in {:?}; {} warnings",
                imp.format.name(),
                t.meta.name,
                t.meta.abbrev,
                t.verse_count(),
                started.elapsed(),
                imp.warnings.len()
            );
            if let Some(base) = args.get(2) {
                let b = format::decode(&std::fs::read(root.join(format!("data/translations/{base}.bbl")))?)?;
                let (mut same, mut diff, mut missing) = (0, 0, 0);
                for c in bible_core::ChapterIdx::all() {
                    for v in b.verses_in(c) {
                        match t.find(v.reference()) {
                            Some(o) if o.plain_text() == v.plain_text() => same += 1,
                            Some(o) => {
                                if diff < 5 {
                                    println!(
                                        "  {}\n    {}\n    {}",
                                        v.reference(),
                                        v.plain_text(),
                                        o.plain_text()
                                    );
                                }
                                diff += 1
                            }
                            None => missing += 1,
                        }
                    }
                }
                println!("vs {base}: {same} identical, {diff} different, {missing} missing");
            }
            for r in ["ps 23:1", "jn 3:16", "gen 1:1", "rev 22:21"] {
                let r = bible_core::reference::parse(r).unwrap();
                if let Some(v) = t.find(r.verse_ref()) {
                    let marks: Vec<String> =
                        v.marks.iter().map(|m| format!("{:?}{}", m.kind, m.level)).collect();
                    let spans: Vec<String> = v.spans.iter().map(|s| format!("{:?}", s.style)).collect();
                    println!("{}: {} {marks:?} {spans:?}", r.display(), v.text);
                }
            }
            Ok(())
        }
        Some("build-lexicon") => build_lexicon(&root, &args[1..]),
        Some("build-wordnet") => build_wordnet(&root),
        Some("fetch-speech") => fetch_speech(&root),
        Some("fetch") => {
            for s in selected(&args[1..]) {
                fetch(&root, s)?;
            }
            Ok(())
        }
        _ => {
            eprintln!(
                "usage: cargo run -p xtask -- build-data [ids…] | build-lexicon [ids…] | build-wordnet | fetch-speech | fetch [ids…]"
            );
            std::process::exit(2)
        }
    }
}

/// A dictionary bundled with the app, converted from the module CrossWire ships.
///
/// The conversion is a build step: the app only ever reads the tab-separated file this writes.
struct Dictionary {
    id: &'static str,
    name: &'static str,
    url: &'static str,
    sha256: &'static str,
    /// The module's own directory inside the archive, holding `<id>.idx` and friends.
    dir: &'static str,
    license: &'static str,
    copyright: &'static str,
}

const DICTIONARIES: &[Dictionary] = &[Dictionary {
    id: "easton",
    name: "Easton's Bible Dictionary",
    url: "https://www.crosswire.org/ftpmirror/pub/sword/packages/rawzip/Easton.zip",
    sha256: "f6dd054554764e2e97d5d189a697eb26039054578a9ccf98ce668ab810341c6e",
    dir: "modules/lexdict/zld/easton",
    license: "Public Domain",
    copyright: "Easton's Bible Dictionary (1897) by Matthew George Easton, who died in 1894. Its \
                distribution licence states \"Public Domain\". Text from the Christian Classics \
                Ethereal Library, packaged by the CrossWire Bible Society.",
}];

fn selected(ids: &[String]) -> Vec<&'static Source> {
    SOURCES.iter().filter(|s| ids.is_empty() || ids.iter().any(|i| i == s.id)).collect()
}

fn fetch(root: &Path, s: &Source) -> Result<PathBuf> {
    let dir = root.join(".sources");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(format!("{}_usfm.zip", s.ebible_id));
    if !file.exists() {
        let url = format!("https://ebible.org/Scriptures/{}_usfm.zip", s.ebible_id);
        eprintln!("downloading {url}");
        let status = Command::new("curl").args(["-fsSL", "-o"]).arg(&file).arg(&url).status()?;
        if !status.success() {
            bail!("download failed: {url}");
        }
    }
    let digest = hex(&Sha256::digest(std::fs::read(&file)?));
    if digest != s.sha256 {
        if std::env::var_os("XTASK_ACCEPT_UPDATED_SOURCES").is_some() {
            eprintln!("warning: {} changed upstream; new sha256 = {digest}", s.ebible_id);
        } else {
            bail!(
                "{} sha256 mismatch: expected {}, got {digest}. The upstream file changed; review it, then \
                 update the pin (or set XTASK_ACCEPT_UPDATED_SOURCES=1).",
                file.display(),
                s.sha256
            );
        }
    }
    Ok(file)
}

fn build_data(root: &Path, ids: &[String]) -> Result<()> {
    let out_dir = root.join("data/translations");
    std::fs::create_dir_all(&out_dir)?;
    for s in selected(ids) {
        let zip_path = fetch(root, s)?;
        let digest = hex(&Sha256::digest(std::fs::read(&zip_path)?));
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip_path)?)?;
        let mut names: Vec<String> =
            archive.file_names().flatten().filter(|n| n.ends_with(".usfm")).map(|n| n.into_owned()).collect();
        names.sort();
        let meta = TranslationMeta {
            id: s.id.into(),
            abbrev: s.abbrev.into(),
            name: s.name.into(),
            language: "en".into(),
            license: s.license.into(),
            copyright: s.copyright.into(),
            source: format!(
                "https://ebible.org/Scriptures/{}_usfm.zip (sha256 {digest}); details: https://ebible.org/{}/",
                s.ebible_id, s.ebible_id
            ),
            origin: Origin::Bundled,
            ..Default::default()
        };
        let mut b = TranslationBuilder::new(meta);
        for name in &names {
            let mut f = archive.by_name(name)?;
            let mut text = String::new();
            std::io::Read::read_to_string(&mut f, &mut text).with_context(|| name.clone())?;
            usfm::UsfmParser::new(&mut b).parse(&text)?;
        }
        let warnings = std::mem::take(&mut b.warnings);
        let t = b.finish();
        let bytes = format::encode(&t, 19)?;
        let path = out_dir.join(format!("{}.bbl", s.id));
        std::fs::write(&path, &bytes)?;
        println!(
            "{:<4} {:>6} verses  {:>5.2} MB text → {:>5.2} MB  ({} warnings){}",
            s.abbrev,
            t.verse_count(),
            t.total_text_bytes() as f64 / 1e6,
            bytes.len() as f64 / 1e6,
            warnings.len(),
            if t.meta.red_letters { "  red letters" } else { "" }
        );
        for w in warnings.iter().take(5) {
            println!("      {w}");
        }
    }
    Ok(())
}

fn build_lexicon(root: &Path, ids: &[String]) -> Result<()> {
    let out_dir = root.join("data/lexicon");
    std::fs::create_dir_all(&out_dir)?;
    for d in DICTIONARIES.iter().filter(|d| ids.is_empty() || ids.iter().any(|i| i == d.id)) {
        let zip_path = fetch_dictionary(root, d)?;
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip_path)?)?;
        let idx = module_file(&mut archive, d, "idx")?;
        let dat = module_file(&mut archive, d, "dat")?;
        let zdx = module_file(&mut archive, d, "zdx")?;
        let zdt = module_file(&mut archive, d, "zdt")?;
        let entries = zld::entries(&idx, &dat, &zdx, &zdt)?;
        let mut out = format!("#bible-rs-lexicon v1\n#name\t{}\n#licence\t{}\n", d.name, d.license);
        let mut bodyless = 0;
        for (key, markup) in &entries {
            let (headword, body) = zld::plain(markup);
            let headword = if headword.is_empty() { key.clone() } else { headword };
            if body.is_empty() {
                bodyless += 1;
            }
            // The format is a line an entry: tabs and newlines inside a definition become a space
            // and the two characters `\n`, which is what the reader turns back into a line break.
            let headword: String = headword.replace(['\t', '\n'], " ");
            let body = body.replace('\t', " ").replace('\n', "\\n");
            out.push_str(&format!("{}\t{headword}\t{body}\n", d.id));
        }
        let path = write_compressed(&out_dir, &format!("{}.tsv", d.id), out.as_bytes())?;
        println!(
            "{:<28} {:>5} entries  {} with no definition  {:>5.2} MB → {}",
            d.name,
            entries.len(),
            bodyless,
            out.len() as f64 / 1e6,
            path.display()
        );
    }
    Ok(())
}

/// One file of a module, from inside its pinned archive.
fn module_file(archive: &mut zip::ZipArchive<std::fs::File>, d: &Dictionary, ext: &str) -> Result<Vec<u8>> {
    let path = format!("{}/{}.{ext}", d.dir, d.id);
    let mut f = archive.by_name(&path).with_context(|| format!("{path} is not in the {} module", d.id))?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut f, &mut bytes)?;
    Ok(bytes)
}

fn fetch_dictionary(root: &Path, d: &Dictionary) -> Result<PathBuf> {
    let dir = root.join(".sources");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(format!("{}.zip", d.id));
    if !file.exists() {
        eprintln!("downloading {}", d.url);
        let status = Command::new("curl").args(["-fsSL", "-o"]).arg(&file).arg(d.url).status()?;
        if !status.success() {
            bail!("download failed: {}", d.url);
        }
    }
    let digest = hex(&Sha256::digest(std::fs::read(&file)?));
    if digest != d.sha256 {
        if std::env::var_os("XTASK_ACCEPT_UPDATED_SOURCES").is_some() {
            eprintln!("warning: {} changed upstream; new sha256 = {digest}", d.id);
        } else {
            bail!(
                "{} sha256 mismatch: expected {}, got {digest}. The upstream file changed; review it, \
                 then update the pin (or set XTASK_ACCEPT_UPDATED_SOURCES=1).",
                file.display(),
                d.sha256
            );
        }
    }
    Ok(file)
}

/// Open English WordNet: the primary dictionary, where ordinary words come from.
const WORDNET: (&str, &str) = (
    "https://github.com/globalwordnet/english-wordnet/releases/download/2025-edition/english-wordnet-2025-json.zip",
    "7d749f6e2c39e6970e4997839dcf6e42fd281f3c2fae0171d2192bae8cfa4b51",
);

fn build_wordnet(root: &Path) -> Result<()> {
    let (url, sha) = WORDNET;
    let sources = root.join(".sources");
    std::fs::create_dir_all(&sources)?;
    let archive = sources.join("oewn-2025-json.zip");
    if !archive.exists() {
        eprintln!("downloading {url}");
        let status = Command::new("curl").args(["-fsSL", "-o"]).arg(&archive).arg(url).status()?;
        if !status.success() {
            bail!("download failed: {url}");
        }
    }
    let digest = hex(&Sha256::digest(std::fs::read(&archive)?));
    if digest != sha {
        bail!(
            "{} sha256 mismatch: expected {sha}, got {digest}. The release changed; review it, then \
             update the pin.",
            archive.display()
        );
    }
    let (lexicon, forms) = wordnet::convert(&archive)?;
    let out = root.join("data/lexicon");
    std::fs::create_dir_all(&out)?;
    write_compressed(&out, "wordnet.tsv", lexicon.as_bytes())?;
    write_compressed(&out, "wordnet-forms.tsv", forms.as_bytes())?;
    println!(
        "Open English WordNet  {:>6} words {:>6} forms  {:>5.2} MB → {}",
        lexicon.lines().count().saturating_sub(3),
        forms.lines().count().saturating_sub(1),
        lexicon.len() as f64 / 1e6,
        out.join("wordnet.tsv").display()
    );
    Ok(())
}

/// Phonon-31m, the Read Aloud voice: 31M parameters, q8_0, Apache-2.0 weights and runtime, and no
/// phonemizer anywhere. Fetched rather than committed — 62 MB in Git history is not worth it.
const SPEECH: &[(&str, &str, &str)] = &[
    (
        "model.q8.gguf",
        "https://huggingface.co/gradium/phonon-31m-en/resolve/main/model.q8.gguf",
        "a7d9aad2cd6dc66473f24a08bc6d2939030ef4d751ef82349361fbef3272dbae",
    ),
    (
        "config.json",
        "https://huggingface.co/gradium/phonon-31m-en/resolve/main/config.json",
        "7062ea376e6b158543c8abec8e5af67df171c49ad70eb7e2a0fb25c16f265e06",
    ),
    (
        "tokenizer.json",
        "https://huggingface.co/gradium/phonon-31m-en/resolve/main/tokenizer.json",
        "691a00068fa7b989f2f3e5ae5daa2d1a5bb319f2d5360deafcfb19a8bb6e7936",
    ),
    (
        "default-voice.safetensors",
        "https://huggingface.co/gradium/phonon-31m-en/resolve/main/default-voice.safetensors",
        "f739e8c4c76ee7f92e7c94a82002748c8cd22e99582ac49f7b9b94cd63786e18",
    ),
    (
        "russell.safetensors",
        "https://huggingface.co/gradium/phonon-voices/resolve/main/voices/_6Aslh2DxfmnRLmP.safetensors",
        "beae9e4030efce0a935fcb0f807dc0784d598bcaa87ad4aacb047436b1eaa411",
    ),
    (
        "spencer.safetensors",
        "https://huggingface.co/gradium/phonon-voices/resolve/main/voices/JOW0vchyswkQUMDS.safetensors",
        "cc9ac1107b0ffce282374a19beb7bfa95847724e6b7afbac991b4ae4277d5e6d",
    ),
    (
        "harper.safetensors",
        "https://huggingface.co/gradium/phonon-voices/resolve/main/voices/4SZHfMpw-p46Ywgs.safetensors",
        "6b3da6e2d796a20c51af892c482bd6509fb88267a221aff83c870843dee2a123",
    ),
    (
        "freya.safetensors",
        "https://huggingface.co/gradium/phonon-voices/resolve/main/voices/GgfEkEJtxZR7gnpy.safetensors",
        "eb3cd47418349e618408c16bea7153988b3ad04b41a6033039dd474a9a4dcd23",
    ),
];

fn fetch_speech(root: &Path) -> Result<()> {
    let out = root.join("data/speech");
    std::fs::create_dir_all(&out)?;
    let mut bytes = 0u64;
    for (name, url, sha) in SPEECH {
        let path = out.join(name);
        let digest = if path.exists() { hex(&Sha256::digest(std::fs::read(&path)?)) } else { String::new() };
        if digest != *sha {
            eprintln!("downloading {name}");
            let status = Command::new("curl").args(["-fsSL", "-o"]).arg(&path).arg(url).status()?;
            if !status.success() {
                bail!("download failed: {url}");
            }
            let got = hex(&Sha256::digest(std::fs::read(&path)?));
            if got != *sha {
                bail!(
                    "{name} sha256 mismatch: expected {sha}, got {got}. Review the release, then \n                     update the pin."
                );
            }
        }
        bytes += std::fs::metadata(&path)?.len();
    }
    let voices = SPEECH
        .iter()
        .filter(|(name, _, _)| name.ends_with(".safetensors") && !name.starts_with("default"))
        .count();
    println!("Phonon-31m + {voices} voices  {:>5.1} MB → {}", bytes as f64 / 1e6, out.display());
    Ok(())
}

/// Writes a bundled table compressed with zstd — the same compression the translations use, so the
/// app carries one decoder and the plain text never sits in the binary or the repo. The plain `.tsv`
/// is refused rather than left behind: two copies of 13 MB drift apart.
fn write_compressed(dir: &Path, name: &str, text: &[u8]) -> Result<std::path::PathBuf> {
    let path = dir.join(format!("{name}.zst"));
    let compressed = zstd::bulk::compress(text, 19)?;
    std::fs::write(&path, &compressed)?;
    let plain = dir.join(name);
    if plain.exists() {
        std::fs::remove_file(&plain)?;
    }
    Ok(path)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
