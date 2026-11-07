# Baseline before the macOS 26.6 + Xcode update — V1

Written on the machine that builds Lamplit, immediately before Oscar updates it. Its only job is to
make "report anything that changed" answerable: every number below is what the toolchain was at
`1f58785`, the last commit before the update.

## The toolchain, as measured

| Tool | Version |
| --- | --- |
| macOS | **26.2** (build 25C56) |
| Xcode / xcodebuild | **absent** — `xcode-select` points at `/Library/Developer/CommandLineTools` |
| cargo | 1.99.1 (5f94df478 2026-08-27) |
| rustc | 1.99.1 (58cf8f9c3 2026-10-13) |
| Node (site) | **v22.23.2**, pinned by `site/.nvmrc` (`22`) |
| zstd (via zstd-sys) | 2.1.2+zstd.1.5.7 |
| ptts | 0.4.0 |
| egui | 0.36.2 |

`rust-version` in the workspace is **1.95**; the toolchain is well above it, which is why a compiler
that arrives with the OS update is unlikely to break the build. If it does, the floor is the thing to
check first, not the code.

## What to run after the update, and what "unchanged" looks like

1. **Full Rust suite** — `cargo test -q --workspace --all-features`. At `1f58785`: **251 passed, 0
   failed** (app lib 57, core lib 141 passed + 1 ignored, the rest integration tests).
2. **Release build** — `ALLOW_DIRTY=1 scripts/release-macos.sh --ad-hoc`. Expect the arm64 and x86_64
   smoke reads (`read jn 3:16 ok`), a verified DMG, and the ad-hoc Gatekeeper rejection, which is
   expected and not a fault.
3. **Site check with Node 22** — `cd site && npm run check` then `npm run build`. Expect 0 errors and
   Vercel output written. The dependency tree refuses Node 26 outright, so confirm `node -v` first;
   `.nvmrc` is what saves it.
4. **Voice download into a fresh `BIBLE_RS_HOME`** — `cargo run -p bible-rs --example voice-check`.
   Expect all **eight** files verified in roughly 20 seconds (`model.q8.gguf` 64,512,352 B;
   `config.json` 2,851; `tokenizer.json` 277,967; `default-voice.safetensors` and the four named
   voices 256,136 each), then synthesis at far above realtime. A TLS error here means the `rustls`
   feature has gone missing again — see `DECISIONS.md`, "The one request needs a TLS stack".

## Why this file exists

An OS update moves four things at once — the C compiler behind the Rust toolchain, the linker, the
Metal driver, and the Node that ships with the command line tools. Any of them can change behaviour
without changing a line of ours, and "it worked before the update" is not evidence. The numbers above
are the evidence.
