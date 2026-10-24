# bible-rs: notes for agents

## Tests and screenshots

- Offscreen screenshots and scripted interaction use `crates/app/examples/shot.rs`
  (`BIBLE_RS_SHOT_SCRIPT`, `BIBLE_RS_SHOT_STEP_DT`). No screen-recording permission needed.
- `egui_kittest` simulates one frame every **250 ms** by default, so anything gated on click
  timing — double-click, drag momentum — is a silent false negative unless `BIBLE_RS_SHOT_STEP_DT`
  is set (try `0.016`). Before calling such a failure a regression, re-run with a realistic clock.

## Licensing

- Code is MIT ([LICENSE](LICENSE)), wherever it lives.
- The object specs in `objects/` and the images rendered from their 3D models
  (`site/static/ark-of-the-covenant/`, and the copy in `crates/app/web/ark-of-the-covenant/`) are
  CC BY 4.0 ([objects/LICENSE](objects/LICENSE), full text in
  [LICENSES/CC-BY-4.0.txt](LICENSES/CC-BY-4.0.txt)). So is anything new in `objects/`, glTF models
  included. A spec's `license` field is `"CC-BY-4.0"`, which the schema requires.
- Credit them as: "Ark of the Covenant specs and 3D models © 2026 Micky (github.com/michaelshimeles) / bible-rs contributors, licensed CC BY 4.0, https://github.com/michaelshimeles/bible-rs".
- Bundled Bible translations are public domain. Never add copyrighted translation text, even in
  tests ([LICENSES/README.md](LICENSES/README.md)).
