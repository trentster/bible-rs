# Read Aloud voice: Phonon-31m

The speaking voice is **not** committed to Git. `cargo run -p xtask -- fetch-speech` downloads it to
`data/speech/` and verifies each file's SHA-256 against the pin in `xtask/src/main.rs`.

| File | Size | Source |
| --- | --- | --- |
| `model.q8.gguf` | 61.5 MB | [gradium/phonon-31m-en](https://huggingface.co/gradium/phonon-31m-en) |
| `config.json` | 0.3 MB | same |
| `tokenizer.json` | 0.3 MB | same |
| `default-voice.safetensors` | 0.2 MB | same |
| `edwin.safetensors` | 0.26 MB | [gradium/phonon-voices](https://huggingface.co/gradium/phonon-voices), voice "Edwin" |

## Licence

Weights, tokenizer and voice embeddings are **Apache-2.0**; the `ptts` runtime is MIT/Apache-2.0.
Both permit redistribution and commercial use with attribution and a copy of the licence, which this
repository carries in `LICENSES/`. Nothing here is copyleft, and the model needs no phonemizer, so
no GPL component (espeak-ng, Piper) enters the app.

**Limit to record:** Phonon-**40m** and the multilingual models are **CC BY-NC 4.0**. Only the 31m
English model is Apache-2.0, so there is no licence-clean upgrade path to a larger Phonon.

Nothing is fetched at runtime. The model is downloaded once by a developer (`fetch-speech`) or
shipped inside the app bundle; the running app never makes a network request to speak.

## What we do to it

We time-stretch the model's output to **0.80 of its own pace** (190 → 153 words per minute) with our
own WSOLA implementation in `crates/core/src/speak/stretch.rs`. That changes timing, not the weights;
the audio is still the model's own performance.
