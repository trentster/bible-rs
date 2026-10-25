//! Prints one chapter as flowing text, no verse numbers, for listening tests.

use bible_core::books::{BookId, ChapterIdx};

fn main() {
    let mut args = std::env::args().skip(1);
    let code = args.next().unwrap_or_else(|| "MAT".into());
    let number: u16 = args.next().unwrap_or_else(|| "10".into()).parse().unwrap();
    let book = BookId::from_code(&code).expect("book code");
    let chapter = ChapterIdx::new(book, number.try_into().unwrap()).expect("chapter");
    let text = bible_core::bundled::load("kjv").expect("kjv");
    let verses: Vec<String> = text.verses_in(chapter).map(|v| v.text.to_string()).collect();
    println!("{}", verses.join(" "));
}
