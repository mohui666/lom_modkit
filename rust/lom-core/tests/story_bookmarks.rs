use lom_core::story_bookmarks::{self, TimelineEntry};
use serde_json::json;

#[test]
fn bookmarks_read_in_newest_order_and_ignore_incomplete_writes() {
    let root = tempfile::tempdir().unwrap();
    for (name, key, time) in [
        ("mark-old.json", "D_OLD", "2026-11-05T12:00:00Z"),
        ("mark-new.json", "D_NEW", "2026-11-05T12:01:00Z"),
    ] {
        std::fs::write(
            root.path().join(name),
            serde_json::to_vec(&json!({
                "schema":1,"script":"ch_demo","text_key":key,"text":"原版对白", "game_dir":"game",
                "year":1,"month":4,"stage":1,"captured_utc":time
            }))
            .unwrap(),
        )
        .unwrap();
    }
    std::fs::write(root.path().join("mark-writing.json.tmp"), b"{").unwrap();
    let marks = story_bookmarks::read(root.path()).unwrap();
    assert_eq!(marks.len(), 2);
    assert_eq!(marks[0].text_key, "D_NEW");
    assert!(marks[0].caption().contains("第1年4月上旬"));
    assert!(story_bookmarks::read(&root.path().join("missing"))
        .unwrap()
        .is_empty());
    std::fs::write(root.path().join("mark-bad.json"), b"{}").unwrap();
    assert!(story_bookmarks::read(root.path()).is_err());
}

#[test]
fn timeline_dates_match_ranges_across_years_and_keep_unknown_times_explicit() {
    let row = TimelineEntry {
        script: "DEMO".into(),
        title: "事件".into(),
        series: "Main".into(),
        key: "demo".into(),
        main: true,
        next_key: "".into(),
        condition: "".into(),
        place: "校場".into(),
        when_label: "跨年区间".into(),
        from_round: Some(34),
        to_round: Some(38),
        active: true,
    };
    assert!(row.matches_date(1, 12, false));
    assert!(row.matches_date(2, 1, false));
    assert!(!row.matches_date(1, 1, false));
    assert!(!row.matches_date(2, 2, false));
    let unknown = TimelineEntry {
        from_round: None,
        to_round: None,
        ..row
    };
    assert!(!unknown.matches_date(1, 4, false));
    assert!(unknown.matches_date(1, 4, true));
    assert!(unknown.matches_date(0, 0, false));
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("timeline.json"),
        serde_json::to_vec(&json!({
            "schema":2,"game_dir":"game","game_version":"1.0.6001.0","missions":[unknown]
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        story_bookmarks::read_timeline(root.path())
            .unwrap()
            .unwrap()
            .missions
            .len(),
        1
    );
}
