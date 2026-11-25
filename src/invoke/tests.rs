use super::*;

/// Every code the backend sends (`backend/src/error.rs`) has its text in
/// both languages, except the output of a tool, shown as it is.
#[test]
fn every_backend_error_is_worded() {
    let source = include_str!("../../backend/src/error.rs");
    let codes: Vec<&str> = source.lines()
        .filter_map(|l| l.split_once("=> (\"")).filter_map(|(_, rest)| rest.split_once('"')).map(|(code, _)| code)
        .filter(|c| !matches!(*c, "io" | "tool_output"))
        .collect();
    assert!(codes.len() > 10, "{codes:?}");
    for code in codes {
        for lang in [Lang::Fr, Lang::En] {
            let err = BackendError { code: code.to_string(), detail: None, cause: None };
            assert_ne!(error_text(&err, lang), code, "no text for the backend error {code}");
        }
    }
}
