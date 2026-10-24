# Dictionaries: provenance and terms

Each dictionary is converted once, at build time, from the archive the publisher releases:
`cargo run -p xtask -- build-lexicon` for CrossWire modules and `cargo run -p xtask -- build-wordnet`
for WordNet. Every archive is pinned by SHA-256 in `xtask/src/main.rs`; a changed file fails the
build rather than sliding in unnoticed. The app never contains a reader for a module or release
format — only the plain text the conversion writes.

## Open English WordNet (2025 edition) — `wordnet`

The dictionary a reader gets for ordinary English words. It answers first; Easton's answers only
what WordNet has never heard of.

| | |
|---|---|
| Archive | `https://github.com/globalwordnet/english-wordnet/releases/download/2025-edition/english-wordnet-2025-json.zip` |
| SHA-256 | `7d749f6e2c39e6970e4997839dcf6e42fd281f3c2fae0171d2192bae8cfa4b51` (9,986,555 bytes) |
| Downloaded | 2026-10-24 |
| Licence | Creative Commons Attribution 4.0 International **and** the Princeton WordNet License |
| Attribution required | **Princeton WordNet** and the **Open English WordNet team** |

Verbatim, from the project's own `LICENSE.md`:

> This resource is derived from Princeton WordNet under the WordNet License
> and further developed under the Creative Commons Attribution 4.0 International License.
> You may share and adapt this resource providing attribution is given to both
> Princeton WordNet and the Open English Wordnet team.

Why it may be redistributed: CC BY 4.0 permits redistribution in any medium provided the attribution
above is given, and the Princeton WordNet License permits use, copying, modification and
distribution for any purpose, without fee or royalty. Both conditions are satisfiable by a bundled,
offline app, and neither is copyleft — the surrounding app stays MIT. The attribution line is
discharged on screen by the popover's `Source: WordNet` footnote. **Still to do:** a fuller credit
(project name, edition, link, "modified") belongs in an About/credits surface once the app has one;
that obligation is not yet fully met, and is recorded here rather than quietly dropped.

What the conversion changes — it is a *modified* form of the resource, as CC BY 4.0 requires us to
state:

- lexical entries and synsets are flattened to word + meanings; synset relations, examples, sense
  numbers and parts of the vocabulary outside noun/verb/adjective/adverb are dropped;
- the first **three** senses per word are kept, ordered so the part of speech the word is mostly
  used as comes first;
- one definition line per sense, prefixed `n.`, `v.`, `adj.` or `adv.`, and truncated at 160
  characters on a word boundary (marked with `…`) to stay one short line in the popover;
- irregular inflections are extracted into their own table (`wordnet-forms.tsv.zst`, 4,389 forms,
  form → lemma) so *went*, *feet* and *best* reach *go*, *foot* and *good*;
- the result is plain TSV, then zstd-compressed exactly as the translations are: `data/lexicon/wordnet.tsv.zst`, **2,307,859 bytes** for 127,981 words (13,242,890 bytes of text). Easton's is 738,281 bytes for its 3,963 entries; the 4,389 forms are 20,094 bytes. The plain `.tsv` is deleted by the build rather than left beside the compressed copy, so two versions cannot drift apart. The hand-edited `aliases.tsv` stays plain text: at 1,282 bytes it is not worth a stale-copy risk.

Unpacking is deferred: the tables are decompressed and indexed inside the **first `define()` call**, never on the startup path. Measured on the development Mac (release build): first lookup of an ordinary word **102 ms**, first lookup of a Bible word another **7.8 ms**, and every lookup after that **0.2 µs** off a built index.

No definitions are added, removed or rewritten.

## Easton's Bible Dictionary (1897) — `easton`

The fallback, for Bible names and places WordNet has never heard of. Only its opening sentence or
two is shown, trimmed at display time.

| | |
|---|---|
| Archive | `https://www.crosswire.org/ftpmirror/pub/sword/packages/rawzip/Easton.zip` |
| SHA-256 | `f6dd054554764e2e97d5d189a697eb26039054578a9ccf98ce668ab810341c6e` (1,437,674 bytes) |
| Downloaded | 2026-10-24 |
| Distribution licence | Public Domain |
| Text source | Christian Classics Ethereal Library (CCEL) |
| Module version | 2.0.1 (`SwordVersionDate=2008-11-29`), `ModDrv=zLD`, `SourceType=TEI` |

Verbatim, from the module's own `mods.d/easton.conf`:

> About=Easton's 1897 Bible Dictionary\par\par Public Domain -- Copy Freely\par\par These Dictionary
> topics are from M.G. Easton M.A., D.D., Illustrated Bible Dictionary, Third Edition, published by
> Thomas Nelson, 1897. Due to the nature of etext, the illustrated portions of the Dictionary have
> not been included.

> ## Copyright and Licensing related elements
> DistributionLicense=Public Domain
> TextSource=CCEL

Why it may be redistributed: the work is public domain in every jurisdiction where bible-rs is
offered. Matthew George Easton died in 1894, so the 1897 text is out of copyright by the life-plus
term everywhere; the digital edition states the same terms itself, and CCEL's own text file for the
work reads "Public Domain -- Copy Freely". The illustrations are not included in the electronic
text ("Due to the nature of etext, the illustrated portions of the Dictionary have not been
included"), so no image rights are involved.

What the conversion changes: the module's TEI markup is stripped to plain text, and its entry keys
become headwords. No words of the dictionary are added, removed or rewritten; paragraphs are
preserved as line breaks.

## The spelling table — `data/lexicon/aliases.tsv`

**Our own editorial data, not Easton's.** The King James text prints some names in a Greek form
(*Melchisedec*, *Noe*, *Osee*) that Easton's files under another headword (*Melchizedek*, *Noah*,
*Hosea*). The table records each such spelling, the headword it points to, and one verse where the
KJV prints it. It was assembled by hand from the bundled KJV text and Easton's headwords, is not
taken from any third party, and carries no claim beyond the project's own MIT licence. A test
checks every line against the bundled KJV, so a wrong reference fails the build.

Deliberately absent: *Corinthians*, *Philippians*, and other names that resemble a headword but
mean something else — the reader asking about the epistle is not asking about the city. Also
absent: *Elias*, *Esaias*, *Jonas*, *Timotheus*, *Zacharias*, *Rahab* and *Sodom*, which Easton's
already carries under those spellings.
