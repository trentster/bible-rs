//! Who made the text and the tools this app carries, and under what licence.
//!
//! One list, in the core, so the Mac and the phone show the same words. The App Store asks for a
//! licence record; so does the licence of almost everything here. Nothing in this file is a
//! formality: each entry names the holder, the licence, and where it came from.

/// One thing the app carries, and the terms it arrives under.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Credit {
    /// What it is, as a reader would recognise it: "STEPBible", "OpenBible.info cross references".
    pub work: &'static str,
    /// Who made it or holds it.
    pub holder: &'static str,
    /// The licence, in the holder's own naming.
    pub licence: &'static str,
    /// Where it lives.
    pub url: &'static str,
    /// How it is used here, in one line. Empty when the work's name says it.
    pub note: &'static str,
}

/// Everything the app carries that someone else made, in the order a reader would look for it.
pub const CREDITS: &[Credit] = &[
    Credit {
        work: "bible-rs",
        holder: "Micky (github.com/michaelshimeles)",
        licence: "MIT",
        url: "https://github.com/michaelshimeles/bible-rs",
        note: "Lamplit is a fork of bible-rs. The full MIT text is in LICENSES/ of the source.",
    },
    Credit {
        work: "Bible translations",
        holder: "eBible.org",
        licence: "Public domain",
        url: "https://ebible.org",
        note: "KJV, WEB, BSB, ASV, YLT, DBY and WBS, converted from their USFM editions.",
    },
    Credit {
        work: "STEPBible",
        holder: "STEPBible.org / Tyndale House, Cambridge",
        licence: "CC BY 4.0",
        url: "https://github.com/STEPBible/STEPBible-Data",
        note: "The Hebrew, Aramaic and Greek behind each verse, and their brief lexicons.",
    },
    Credit {
        work: "Easton's Bible Dictionary",
        holder: "M. G. Easton (1897), via CrossWire and CCEL",
        licence: "Public domain",
        url: "https://www.crosswire.org/sword/modules/",
        note: "The dictionary behind a word lookup.",
    },
    Credit {
        work: "Open English WordNet",
        holder: "Princeton WordNet and the Open English WordNet team",
        licence: "CC BY 4.0 and the Princeton WordNet License",
        url: "https://github.com/globalwordnet/english-wordnet",
        note: "Ordinary English words that the Bible dictionary does not carry.",
    },
    Credit {
        work: "OpenBible.info cross references",
        holder: "OpenBible.info (links mostly from the Treasury of Scripture Knowledge)",
        licence: "CC BY 4.0",
        url: "https://www.openbible.info/labs/cross-references/",
        note: "The verses offered as related to the one you are reading.",
    },
    Credit {
        work: "Phonon-31m",
        holder: "Gradium",
        licence: "Apache-2.0",
        url: "https://huggingface.co/gradium/phonon-31m-en",
        note: "The reading voice, downloaded once on request and never bundled.",
    },
    Credit {
        work: "ptts",
        holder: "the ptts authors",
        licence: "MIT or Apache-2.0",
        url: "https://crates.io/crates/ptts",
        note: "The runtime that speaks the text.",
    },
    Credit {
        work: "Literata",
        holder: "TypeTogether, for Google",
        licence: "SIL Open Font License 1.1",
        url: "https://fonts.google.com/specimen/Literata",
        note: "The scripture text.",
    },
    Credit {
        work: "Inter",
        holder: "Rasmus Andersson",
        licence: "SIL Open Font License 1.1",
        url: "https://rsms.me/inter/",
        note: "The interface.",
    },
    Credit {
        work: "Ark of the Covenant specs and 3D models",
        holder: "Micky (github.com/michaelshimeles) / bible-rs contributors",
        licence: "CC BY 4.0",
        url: "https://github.com/michaelshimeles/bible-rs",
        note: "Shown in the app's object view.",
    },
];

/// The whole list. The Mac's About panel and the phone's Settings both read this one.
pub fn all() -> &'static [Credit] {
    CREDITS
}

/// The one-line credit the Ark's licence requires verbatim, kept beside the list so the wording cannot
/// drift between the app and the repository's own notice.
pub const ARK_CREDIT: &str = "Ark of the Covenant specs and 3D models © 2026 Micky (github.com/michaelshimeles) / bible-rs contributors, licensed CC BY 4.0, https://github.com/michaelshimeles/bible-rs";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_credit_names_a_holder_a_licence_and_a_source() {
        for c in all() {
            assert!(!c.work.is_empty(), "{:?}", c);
            assert!(!c.holder.is_empty(), "{} needs a holder", c.work);
            assert!(!c.licence.is_empty(), "{} needs a licence", c.work);
            assert!(c.url.starts_with("https://"), "{} needs a source URL", c.work);
        }
    }

    #[test]
    fn the_datasets_that_ship_are_all_credited() {
        // Each of these is in the binary or downloaded by it: a missing record is a licence breach,
        // not an omission.
        let works: Vec<&str> = all().iter().map(|c| c.work).collect();
        for wanted in [
            "bible-rs",
            "STEPBible",
            "Easton's Bible Dictionary",
            "Open English WordNet",
            "OpenBible.info cross references",
            "Phonon-31m",
            "Bible translations",
        ] {
            assert!(works.contains(&wanted), "{wanted} is shipped but not credited");
        }
    }

    #[test]
    fn the_ark_wording_is_the_one_its_licence_asks_for() {
        assert!(ARK_CREDIT.contains("licensed CC BY 4.0"));
        assert!(ARK_CREDIT.contains("michaelshimeles/bible-rs"));
    }
}
