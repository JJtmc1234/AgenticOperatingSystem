use carl::providers::workspace::diff::simple_diff;

#[test]
fn adding_or_removing_the_final_newline_shows_the_changed_line() {
    for (before, after) in [("hello", "hello\n"), ("hello\n", "hello")] {
        let diff = simple_diff(before, after);
        assert!(diff.starts_with("@@ line 1 @@\n"), "{diff}");
        assert!(diff.contains("-hello\n"), "{diff}");
        assert!(diff.contains("+hello\n"), "{diff}");
        assert_eq!(
            diff.matches("\\ No newline at end of file").count(),
            1,
            "{diff}"
        );
    }
}

#[test]
fn changing_crlf_to_lf_visibly_identifies_the_line_ending() {
    let diff = simple_diff("same\nhello\r\ntail\n", "same\nhello\ntail\n");
    assert!(diff.starts_with("@@ line 2 @@\n"), "{diff}");
    assert!(diff.contains("-hello\n\\ Line ending: CRLF\n"), "{diff}");
    assert!(diff.contains("+hello\n"), "{diff}");
    assert!(
        !diff.contains('\r'),
        "the panel must show a label, not a control character"
    );
    assert!(!diff.contains("tail"), "{diff}");
}
