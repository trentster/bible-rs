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

/// Where the voice comes from: a release asset on the project's own mirror, one file per request so
/// a slow line makes progress that can be seen.
const BUNDLE: &str = "https://github.com/trentster/bible-rs/releases/download/speech-1";

/// Every file the voice is made of, and the digest it must have.
///
/// The same pins `xtask` enforces when it fetches a developer's copy — the app and the build tool
/// agree on what the voice is.
const FILES: &[(&str, &str)] = &[
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
const TOTAL_BYTES: u64 = 66_073_850;

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
    fn every_file_the_voice_needs_is_pinned() {
        // A file missing from this list would be fetched by nobody and the voice would not start.
        assert_eq!(FILES.len(), 8);
        for (name, digest) in FILES {
            assert_eq!(digest.len(), 64, "{name} needs a full sha-256");
            assert!(digest.chars().all(|c| c.is_ascii_hexdigit()), "{name}");
        }
    }

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
}
