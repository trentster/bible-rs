# Dictionaries: provenance and terms

Each dictionary is converted once, at build time, from the module the CrossWire Bible Society
publishes: `cargo run -p xtask -- build-lexicon`. The archive is pinned by SHA-256 in
`xtask/src/main.rs`; a changed file fails the build rather than sliding in unnoticed. The app never
contains a reader for the module format — only the plain text the conversion writes.

## Easton's Bible Dictionary (1897) — `easton`

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
