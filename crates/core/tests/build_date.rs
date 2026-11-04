#[allow(dead_code)]
#[path = "../build.rs"]
mod build_script;

use build_script::build_date;

#[test]
fn the_date_given_comes_before_git() {
    let git = || Some("2026-11-01".to_string());
    assert_eq!(build_date(Some("2026-11-03".into()), git), "2026-11-03");
    assert_eq!(
        build_date(Some(" 2026-11-03\n".into()), || None),
        "2026-11-03"
    );
    assert_eq!(build_date(None, git), "2026-11-01", "a build with git");
    assert_eq!(
        build_date(Some(String::new()), git),
        "2026-11-01",
        "an empty argument is no date"
    );
    assert_eq!(build_date(Some("today".into()), git), "2026-11-01");
    assert_eq!(build_date(Some("2026-11-3".into()), || None), "unknown");
    assert_eq!(build_date(None, || None), "unknown");
}
