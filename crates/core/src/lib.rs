//! Core of bible-rs: the verse-indexed translation model, the `.bbl` binary format, importers,
//! full-text search, reference parsing and local study storage. No GUI code lives here.

pub mod books;
pub mod bundled;
pub mod content;
pub mod copy;
pub mod credits;
pub mod format;
pub mod highlight;
#[cfg(feature = "import")]
pub mod import;
pub mod lexicon;
#[cfg(feature = "native")]
pub mod library;
pub mod model;
// The browser reader parses the NLT API's HTML with this. Nothing else does, so it is behind a
// feature only that reader turns on: the Mac and phone builds have no online translations, and
// carry none of this.
#[cfg(feature = "nlt")]
pub mod nlt_html;
#[cfg(feature = "native")]
pub mod original;
pub mod plans;
pub mod reference;
pub mod related;
pub mod search;
#[cfg(feature = "native")]
pub mod speak;
pub mod study;
pub mod sync;
pub mod usfm;
pub mod xml;

pub use books::{BookId, ChapterIdx, Testament, VerseRef};
pub use model::{Translation, TranslationMeta};
