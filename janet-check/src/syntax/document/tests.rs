use super::*;

#[test]
fn maps_utf16_positions_to_bytes_and_back() {
    let doc = Document::new("(print \"й\")\n(os/clock)".to_string());
    // "й" is one UTF-16 unit but two bytes.
    let after = doc.offset(Position::new(0, 10));
    assert_eq!(&doc.text[after..], ")\n(os/clock)");
    assert_eq!(doc.position(after), Position::new(0, 10));
    assert_eq!(doc.offset(Position::new(1, 4)), 17);
    assert_eq!(doc.position(17), Position::new(1, 4));
}

#[test]
fn clamps_positions_past_the_end() {
    let doc = Document::new("(print \"й\")\n(os/clock)".to_string());
    assert_eq!(doc.offset(Position::new(0, 99)), 12);
    assert_eq!(doc.offset(Position::new(9, 0)), doc.text.len());
}
