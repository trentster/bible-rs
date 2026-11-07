//! What the voice is made of, and where it comes from.
//!
//! The app ships without a voice. On the reader's own instruction it fetches the model once — sixty
//! odd megabytes, from a pinned release asset, each file checked against its SHA-256 before it is
//! allowed anywhere near the speaker — and after that nothing is fetched, ever.
//!
//! The pin list lives here rather than in either frontend, because the Mac downloads with `ureq`
//! and the phone downloads with Apple's `URLSession`: two ways of fetching, but they must fetch the
//! same eight files and check the same eight digests. One list is the only way that stays true.

use std::path::Path;

/// Where the voice comes from: a release asset on the project's own mirror, one file per request so
/// a slow line makes progress that can be seen.
pub const BUNDLE_URL: &str = "https://github.com/trentster/lamplit/releases/download/speech-1";

/// Every file the voice is made of, and the digest it must have.
///
/// The same pins `xtask` enforces when it fetches a developer's copy — the apps and the build tool
/// agree on what the voice is.
pub const FILES: &[(&str, &str)] = &[
    ("model.q8.gguf", "a7d9aad2cd6dc66473f24a08bc6d2939030ef4d751ef82349361fbef3272dbae"),
    ("config.json", "7062ea376e6b158543c8abec8e5af67df171c49ad70eb7e2a0fb25c16f265e06"),
    ("tokenizer.json", "691a00068fa7b989f2f3e5ae5daa2d1a5bb319f2d5360deafcfb19a8bb6e7936"),
    ("default-voice.safetensors", "f739e8c4c76ee7f92e7c94a82002748c8cd22e99582ac49f7b9b94cd63786e18"),
    ("russell.safetensors", "beae9e4030efce0a935fcb0f807dc0784d598bcaa87ad4aacb047436b1eaa411"),
    ("spencer.safetensors", "cc9ac1107b0ffce282374a19beb7bfa95847724e6b7afbac991b4ae4277d5e6d"),
    ("harper.safetensors", "6b3da6e2d796a20c51af892c482bd6509fb88267a221aff83c870843dee2a123"),
    ("freya.safetensors", "eb3cd47418349e618408c16bea7153988b3ad04b41a6033039dd474a9a4dcd23"),
];

/// What the whole download weighs, for a progress figure the reader can judge.
pub const TOTAL_BYTES: u64 = 66_073_850;

/// The file a named voice is stored in.
pub fn file_for(voice_name: &str) -> String {
    format!("{}.safetensors", voice_name.to_ascii_lowercase())
}

/// Whether every file the voice needs is in `dir`. `model.q8.gguf` is the largest and arrives
/// first, so asking after that one file alone called a half-finished download installed: the app
/// said Ready, Listened to nothing, and never tried again. All eight, or it is not there.
pub fn complete(dir: &Path) -> bool {
    FILES.iter().all(|(name, _)| dir.join(name).exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_the_voice_needs_is_pinned() {
        // A file missing from this list would be fetched by nobody and the voice would not start.
        assert_eq!(FILES.len(), 8);
        for (name, digest) in FILES {
            assert_eq!(digest.len(), 64, "{name} needs a full sha-256");
            assert!(digest.chars().all(|c| c.is_ascii_hexdigit()), "{name}");
        }
    }

    #[test]
    fn the_pins_agree_with_the_build_tools() {
        // The apps and `xtask` must not drift apart: one fetches a developer's copy, the others a
        // reader's, and they have to be the same voice.
        let xtask =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../xtask/src/main.rs"))
                .expect("xtask source");
        for (name, digest) in FILES {
            assert!(
                xtask.contains(digest),
                "{name}'s digest is not in xtask's pins, so the two would fetch different voices"
            );
        }
    }

    #[test]
    fn every_shipped_voice_has_its_file() {
        for voice in crate::speak::VOICES {
            let file = file_for(voice.name);
            assert!(
                FILES.iter().any(|(name, _)| *name == file),
                "{} has no pinned file, so it could never speak",
                voice.name
            );
        }
    }

    /// A directory of its own, removed when the test ends.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("bible-rs-bundle-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("a scratch directory");
            Self(dir)
        }

        fn touch(&self, name: &str) {
            std::fs::write(self.0.join(name), b"x").expect("a file");
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_download_that_stopped_after_the_model_is_not_installed() {
        let dir = Scratch::new("partial");
        // model.q8.gguf is the first file fetched, and the only one the old check looked for.
        dir.touch("model.q8.gguf");
        assert!(!complete(&dir.0), "seven files short is not an installed voice");
        dir.touch("config.json");
        dir.touch("tokenizer.json");
        assert!(!complete(&dir.0), "five files short is not an installed voice");
    }

    #[test]
    fn all_eight_files_are_an_installed_voice() {
        let dir = Scratch::new("whole");
        for (name, _) in FILES {
            dir.touch(name);
        }
        assert!(complete(&dir.0));
        assert_eq!(FILES.len(), 8, "the pin list is what 'installed' means");
    }
}
