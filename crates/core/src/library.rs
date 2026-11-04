//! The set of installed translations, whatever their origin, behind one interface.
//!
//! * **Bundled** — public-domain `.bbl` blobs compiled into the binary.
//! * **Imported** — `.bbl` files the user converted from their own OSIS/USFM/Zefania/SWORD
//!   files, stored in `<data dir>/translations/`.
//!
//! That is the whole list: Lamplit has no online translations, so a library never holds text it
//! would have to fetch. Translations decode lazily (once, ≈10 ms) and can be warmed on a
//! background thread.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, bail};

use crate::books::ChapterIdx;
use crate::format;
use crate::import::Imported;
use crate::model::{Origin, Translation, TranslationMeta};

pub enum EntryKind {
    Bundled(&'static [u8]),
    Imported(PathBuf),
}

pub struct Entry {
    pub meta: TranslationMeta,
    pub kind: EntryKind,
    local: OnceLock<std::result::Result<Arc<Translation>, String>>,
}

impl Entry {
    pub fn is_local(&self) -> bool {
        true
    }

    /// Decodes (once) and returns a local translation.
    pub fn load(&self) -> Result<Arc<Translation>> {
        let r = self.local.get_or_init(|| {
            let bytes: std::borrow::Cow<'_, [u8]> = match &self.kind {
                EntryKind::Bundled(b) => std::borrow::Cow::Borrowed(*b),
                EntryKind::Imported(p) => match std::fs::read(p) {
                    Ok(b) => std::borrow::Cow::Owned(b),
                    Err(e) => return Err(format!("reading {}: {e}", p.display())),
                },
            };
            format::decode(&bytes)
                .map(|mut t| {
                    // Keep the listing metadata (it carries the origin) authoritative.
                    t.meta.origin = self.meta.origin.clone();
                    Arc::new(t)
                })
                .map_err(|e| format!("{e:#}"))
        });
        r.clone().map_err(anyhow::Error::msg)
    }

    pub fn loaded(&self) -> Option<Arc<Translation>> {
        self.local.get().and_then(|r| r.as_ref().ok().cloned())
    }

    pub fn copyright(&self) -> String {
        self.meta.copyright.clone()
    }
}

pub enum Chapter {
    Ready(Arc<Translation>),
    /// The chapter's text couldn't be read or decoded.
    Failed(String),
    /// The translation doesn't contain this chapter (e.g. a New Testament only import).
    Missing,
}

type Notify = Arc<dyn Fn() + Send + Sync>;

pub struct Library {
    data_dir: PathBuf,
    entries: Vec<Arc<Entry>>,
    notify: Notify,
}

impl Library {
    pub fn open(data_dir: &Path, notify: Notify) -> Self {
        let mut lib = Library { data_dir: data_dir.to_path_buf(), entries: Vec::new(), notify };
        #[cfg(feature = "bundled")]
        for (_, bytes) in crate::bundled::BUNDLED {
            if let Ok(meta) = format::read_meta(bytes) {
                lib.entries.push(Arc::new(Entry {
                    meta,
                    kind: EntryKind::Bundled(bytes),
                    local: OnceLock::new(),
                }));
            }
        }
        if let Ok(dir) = std::fs::read_dir(crate::import::imports_dir(data_dir)) {
            let mut paths: Vec<PathBuf> = dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "bbl"))
                .collect();
            paths.sort();
            for p in paths {
                if let Some(meta) = read_meta_file(&p)
                    && !lib.contains(&meta.id)
                {
                    lib.entries.push(Arc::new(Entry {
                        meta,
                        kind: EntryKind::Imported(p),
                        local: OnceLock::new(),
                    }));
                }
            }
        }
        lib
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn entries(&self) -> &[Arc<Entry>] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&Arc<Entry>> {
        self.entries.iter().find(|e| e.meta.id == id)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    pub fn load(&self, id: &str) -> Result<Arc<Translation>> {
        self.get(id).with_context(|| format!("no translation {id:?}"))?.load()
    }

    /// Decodes local translations on a background thread so switching is instant later.
    pub fn warm(&self, ids: &[String]) {
        let entries: Vec<Arc<Entry>> =
            ids.iter().filter_map(|id| self.get(id).cloned()).filter(|e| e.is_local()).collect();
        if entries.is_empty() {
            return;
        }
        let notify = self.notify.clone();
        std::thread::Builder::new()
            .name("warm-translations".into())
            .spawn(move || {
                for e in entries {
                    if let Ok(t) = e.load() {
                        t.search_index();
                    }
                }
                notify();
            })
            .ok();
    }

    /// The text of one chapter, without blocking on the network.
    pub fn chapter(&self, id: &str, c: ChapterIdx) -> Chapter {
        let Some(e) = self.get(id) else {
            return Chapter::Failed(format!("translation {id} is not installed"));
        };
        match e.load() {
            Ok(t) if t.has_chapter(c) => Chapter::Ready(t),
            Ok(_) => Chapter::Missing,
            Err(err) => Chapter::Failed(format!("{err:#}")),
        }
    }

    /// Saves an imported translation into the data directory and registers it.
    pub fn install(&mut self, imported: Imported) -> Result<String> {
        let mut t = imported.translation;
        let base = if t.meta.id.is_empty() { "imported".to_string() } else { t.meta.id.clone() };
        let mut id = base.clone();
        let mut n = 2;
        while self.contains(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        t.meta.id = id.clone();
        let dir = crate::import::imports_dir(&self.data_dir);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{id}.bbl"));
        let tmp = path.with_extension("bbl.tmp");
        std::fs::write(&tmp, format::encode(&t, 9)?)?;
        std::fs::rename(&tmp, &path)?;
        let meta = t.meta.clone();
        let entry = Entry { meta, kind: EntryKind::Imported(path), local: OnceLock::new() };
        let _ = entry.local.set(Ok(Arc::new(t)));
        self.entries.push(Arc::new(entry));
        Ok(id)
    }

    /// Removes an imported translation (bundled ones cannot be removed).
    pub fn remove(&mut self, id: &str) -> Result<()> {
        let Some(pos) = self.entries.iter().position(|e| e.meta.id == id) else {
            bail!("no translation {id}")
        };
        match &self.entries[pos].kind {
            EntryKind::Bundled(_) => bail!("bundled translations cannot be removed"),
            EntryKind::Imported(p) => {
                std::fs::remove_file(p).with_context(|| format!("removing {}", p.display()))?;
            }
        }
        self.entries.remove(pos);
        Ok(())
    }

    pub fn is_bundled(&self, id: &str) -> bool {
        self.get(id).is_some_and(|e| e.meta.origin == Origin::Bundled)
    }
}

fn read_meta_file(p: &Path) -> Option<TranslationMeta> {
    use std::io::Read;
    let mut f = std::fs::File::open(p).ok()?;
    // The header (magic, version, JSON metadata) is at the front; 64 KiB is plenty.
    let mut buf = vec![0u8; 64 << 10];
    let n = f.read(&mut buf).ok()?;
    buf.truncate(n);
    format::read_meta(&buf).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::books::{BookId, VerseRef};
    use crate::import::{Format, Imported};
    use crate::model::TranslationBuilder;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "bible-rs-lib-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn imported(abbrev: &str) -> Imported {
        let meta = TranslationMeta {
            id: abbrev.to_lowercase(),
            abbrev: abbrev.into(),
            name: format!("{abbrev} test"),
            origin: Origin::Imported { format: "USFM".into(), file_name: "x".into() },
            ..Default::default()
        };
        let mut b = TranslationBuilder::new(meta);
        b.begin_book(BookId(42));
        b.begin_chapter(3);
        b.begin_verse(16, 16);
        b.text("Imported text.");
        Imported { translation: b.finish(), format: Format::Usfm, warnings: vec![], skipped_books: vec![] }
    }

    #[test]
    fn bundled_imported_and_removal() {
        let dir = tmp();
        let mut lib = Library::open(&dir, Arc::new(|| {}));
        assert!(lib.contains("kjv") && lib.contains("web") && lib.contains("bsb"));
        assert!(lib.remove("kjv").is_err());
        let id = lib.install(imported("KJV")).unwrap();
        assert_eq!(id, "kjv-2", "ids never collide with bundled translations");
        let jn3 = ChapterIdx::new(BookId(42), 3).unwrap();
        assert!(matches!(lib.chapter(&id, jn3), Chapter::Ready(_)));
        assert!(matches!(lib.chapter(&id, ChapterIdx(0)), Chapter::Missing));
        // Reopening finds the import on disk.
        let lib2 = Library::open(&dir, Arc::new(|| {}));
        let t = lib2.load(&id).unwrap();
        assert_eq!(t.find(VerseRef::new(BookId(42), 3, 16)).unwrap().text, "Imported text.");
        assert!(matches!(t.meta.origin, Origin::Imported { .. }));
        lib.remove(&id).unwrap();
        assert!(!Library::open(&dir, Arc::new(|| {})).contains(&id));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
