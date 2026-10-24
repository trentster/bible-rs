# Read Aloud voice: Phonon-31m

The speaking voice is **not** committed to Git. `cargo run -p xtask -- fetch-speech` downloads it to
`data/speech/` and verifies each file's SHA-256 against the pin in `xtask/src/main.rs`.

| File | Size | Source |
| --- | --- | --- |
| `model.q8.gguf` | 61.5 MB | [gradium/phonon-31m-en](https://huggingface.co/gradium/phonon-31m-en) |
| `config.json` | 0.3 MB | same |
| `tokenizer.json` | 0.3 MB | same |
| `default-voice.safetensors` | 0.2 MB | same |
| `russell.safetensors` | 0.26 MB | [gradium/phonon-voices](https://huggingface.co/gradium/phonon-voices), US male |
| `spencer.safetensors` | 0.26 MB | same, UK male |
| `harper.safetensors` | 0.26 MB | same, US female |
| `freya.safetensors` | 0.26 MB | same, UK female |

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

We stretch each voice's output to the pace Mark approved — **152.5 words per minute of speech** —
with our own WSOLA implementation in `crates/core/src/speak/stretch.rs`. Because the voices read at
different native rates (177–201 wpm), each carries its own measured figure in
`crates/core/src/speak/voices.rs` and the factor is solved per voice, so changing voice does not
change the pace. That changes timing, not the weights; the audio is still the model's own
performance.
