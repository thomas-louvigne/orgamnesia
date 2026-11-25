use super::*;

#[test]
fn base64_as_the_terminal_reads_it() {
    assert_eq!(base64(b"Ma"), "TWE=");
    assert_eq!(base64(b"Man"), "TWFu");
    assert_eq!(base64("é".as_bytes()), "w6k=");
}
