//! Getting the voice onto the machine: one download, verified, then never again.
//!
//! The app ships without a voice. On the reader's own instruction it fetches the model once — sixty
//! odd megabytes, from a pinned release asset, each file checked against its SHA-256 before it is
//! allowed anywhere near the speaker — and after that nothing is fetched, ever. Reading stays offline
//! and private; this one request, made by a person pressing a button, is the only time the app talks
//! to the network on the voice's behalf.
//!
//! A file that does not match its digest is refused and thrown away rather than written, so a
//! tampered or half-delivered download cannot become a voice.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use sha2::{Digest, Sha256};

// The voice's pins live in `bible-core`, not here: the Mac fetches with `ureq` and the phone with
// Apple's `URLSession`, and two lists would eventually disagree about what the voice is.
use bible_core::speak::bundle::{self, BUNDLE_URL as BUNDLE, FILES, TOTAL_BYTES};

/// Whether every file the voice needs is in `dir`. `model.q8.gguf` is the largest and arrives first,
/// so asking after that one file alone called a half-finished download installed: the app said
/// Ready, Listened to nothing, and never tried again. All eight, or it is not there.
pub(crate) fn complete(dir: &std::path::Path) -> bool {
    bundle::complete(dir)
}

/// Where an installed voice lives: beside the study database, so a development copy
/// (`BIBLE_RS_HOME`) never mixes with a real one.
pub fn dir() -> PathBuf {
    crate::data_dir().join("speech")
}

/// Whether this machine can speak at all: a voice fetched into the data directory, one sitting
/// beside the app, or — for a developer — the repository's own folder. The honest question is not
/// "did the download run" but "is there a voice to use".
pub fn installed() -> bool {
    super::model_dir().is_some()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Nothing on this machine yet, and nothing being fetched.
    Missing,
    /// Under way: how much of it, in bytes, so the reader can tell it is moving.
    Fetching { done: u64, of: u64 },
    /// The download did not finish, and why in words a reader can act on.
    Failed(String),
    /// Fetched and checked.
    Ready,
}

/// The one download there can be. Kept for the life of the process so closing the settings page
/// does not throw away a download that is three quarters done.
pub struct Install {
    status: Mutex<Status>,
}

static INSTALL: OnceLock<Arc<Install>> = OnceLock::new();

pub fn shared() -> Arc<Install> {
    INSTALL
        .get_or_init(|| {
            Arc::new(Install {
                status: Mutex::new(if installed() { Status::Ready } else { Status::Missing }),
            })
        })
        .clone()
}

impl Install {
    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or(Status::Missing)
    }

    fn set(&self, status: Status) {
        if let Ok(mut held) = self.status.lock() {
            *held = status;
        }
    }

    /// Fetches the voice on a thread of its own, so the page keeps drawing while it arrives.
    pub fn start(self: &Arc<Self>) {
        if installed() || matches!(self.status(), Status::Fetching { .. }) {
            return;
        }
        self.set(Status::Fetching { done: 0, of: TOTAL_BYTES });
        let this = self.clone();
        std::thread::spawn(move || match fetch_into(&dir(), &this) {
            Ok(()) => this.set(Status::Ready),
            Err(why) => this.set(Status::Failed(why)),
        });
    }
}

/// Downloads every file, checking each against its digest before it is written.
fn fetch_into(dir: &Path, progress: &Install) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("could not make a place for the voice: {e}"))?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(Some(Duration::from_secs(30 * 60)))
        .user_agent(concat!("bible-rs/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut done = 0u64;
    for (name, want) in FILES {
        let url = format!("{BUNDLE}/{name}");
        let response =
            agent.get(&url).call().map_err(|e| format!("could not reach the voice download: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("the voice download answered {}", response.status()));
        }
        let mut reader = response.into_body().into_reader();
        let mut bytes = Vec::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let read = reader.read(&mut buf).map_err(|e| format!("{name}: {e}"))?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buf[..read]);
            progress.set(Status::Fetching { done: done + bytes.len() as u64, of: TOTAL_BYTES });
        }
        // Checked before it is written: a file that does not match never becomes a voice.
        let got = hex(&Sha256::digest(&bytes));
        if got != *want {
            return Err(format!("{name} arrived damaged and was thrown away"));
        }
        let temp = dir.join(format!(".{name}.arriving"));
        std::fs::write(&temp, &bytes).map_err(|e| format!("could not save {name}: {e}"))?;
        std::fs::rename(&temp, dir.join(name)).map_err(|e| format!("could not put {name} in place: {e}"))?;
        done += bytes.len() as u64;
        progress.set(Status::Fetching { done, of: TOTAL_BYTES });
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_digest_is_read_as_hex() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
    }

    #[test]
    fn the_pins_agree_with_the_build_tools() {
        // The app and `xtask` must not drift apart: one fetches a developer's copy, the other a
        // reader's, and they have to be the same voice.
        let xtask = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../xtask/src/main.rs"))
            .expect("xtask source");
        for (name, digest) in FILES {
            assert!(
                xtask.contains(digest),
                "{name}'s digest is not in xtask's pins, so the two would fetch different voices"
            );
        }
    }

    /// A directory of its own, removed when the test ends.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("bible-rs-voice-{tag}-{}", std::process::id()));
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
