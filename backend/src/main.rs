mod book_data;
mod reference;

fn main() {
    let full_ref = "James 1:1-4;Hosea4;Lk6:1-14;7,9:1-9,10:16";
    // let full_ref = "Beginning";
    let references = reference::parse_references(full_ref);
    // println!("{references:#?}");
    for reference in references {
        let reference = reference.expect("Invalid reference");
        println!(
            "{:?} {}:{}-{}",
            reference.book, reference.chapter, reference.verses.0, reference.verses.1
        );
    }
}
