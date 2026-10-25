//! What the first lookup costs, and what a later one costs.
//!
//! The bundled tables are zstd-compressed and unpacked on demand, inside the first `define()` call,
//! so this is the number that decides whether that choice was honest: it is paid once, when a reader
//! first asks what a word means, and never on the startup path.
//!
//! `cargo run --release -p bible-core --features bundled --example first_lookup`

fn ms(d: std::time::Duration) -> String {
    format!("{:.2} ms", d.as_secs_f64() * 1000.0)
}

fn main() {
    // WordNet: the first call unpacks its table, its forms table and the spelling table, then
    // indexes 127,981 words.
    let start = std::time::Instant::now();
    let found = bible_core::lexicon::define("feast");
    let wordnet_first = start.elapsed();

    // Easton's answers separately, so its table is unpacked only when a Bible word needs it.
    let start = std::time::Instant::now();
    let name = bible_core::lexicon::define("Melchisedec");
    let easton_first = start.elapsed();

    // Everything after that is a lookup in a built index.
    let mut warm = std::time::Duration::MAX;
    for word in ["feast", "propitiation", "went", "spake", "Nazarene", "Melchisedec"] {
        for _ in 0..50 {
            let start = std::time::Instant::now();
            let _ = bible_core::lexicon::define(word);
            warm = warm.min(start.elapsed());
        }
    }

    println!("first lookup, WordNet table (unpack + index + find)  {}", ms(wordnet_first));
    println!("first lookup, Easton's table (unpack + index + find)  {}", ms(easton_first));
    println!("warm lookup, fastest of 300                              {:?}", warm);
    println!(
        "feast answered with {} entry, {} line(s) of meaning",
        found.entries.len(),
        found.entries.first().map_or(0, |e| e.body.lines().count())
    );
    println!("Melchisedec answered with {} entry", name.entries.len());
}
