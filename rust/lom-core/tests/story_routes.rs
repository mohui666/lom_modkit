use lom_core::{
    story_bookmarks::Timeline,
    story_routes::{RouteGraph, ScriptRoutes},
};
use serde_json::json;
use std::collections::BTreeSet;

#[test]
fn route_stations_split_merge_preserve_conditions_and_locate_nested_chapters() {
    let missions:Vec<_>=[("A",0,true),("B",3,true),("C",3,true),("D",6,true),("SIDE",3,false)]
        .into_iter().map(|(key,round,main)|json!({
            "script":key,"title":key,"series":if main{"main"}else{"Side"},"key":key,"main":main,
            "next_key":"","condition":"入口条件","place":"校場","when":"日期","from_round":round,"to_round":round,"active":true
        })).collect();
    let timeline: Timeline = serde_json::from_value(
        json!({"schema":2,"game_dir":"game","game_version":"test","missions":missions}),
    )
    .unwrap();
    let routes:ScriptRoutes=serde_json::from_value(json!({"scripts":[
        {"script":"A","links":[{"target":"A_INNER","condition":""}],"missions":[],"unknown_jumps":0},
        {"script":"A_INNER","links":[],"missions":[{"series":"Main","key":"B","condition":"选项：向左"},{"series":"Main","key":"C","condition":"选项：向右"},{"series":"side","key":"SIDE","condition":"支线名称大小写错误"}],"unknown_jumps":1},
        {"script":"B","links":[],"missions":[{"series":"Main","key":"D","condition":""}],"unknown_jumps":0},
        {"script":"C","links":[],"missions":[{"series":"Main","key":"D","condition":""}],"unknown_jumps":0},
        {"script":"D","links":[],"missions":[],"unknown_jumps":0},
        {"script":"SIDE","links":[],"missions":[{"series":"Main","key":"D","condition":"支线完成"}],"unknown_jumps":0}
    ]})).unwrap();
    let graph = RouteGraph::build(&timeline, &routes);
    for (from, to) in [(0, 1), (0, 2), (1, 3), (2, 3), (4, 3)] {
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.from == from && edge.to == to && !edge.by_date));
    }
    assert!(!graph
        .edges
        .iter()
        .any(|edge| edge.from == 0 && edge.to == 4));
    assert_ne!(graph.stations[1].lane, graph.stations[2].lane);
    assert_eq!(graph.stations[0].lane, graph.stations[3].lane);
    assert!(graph.stations[4].lane > graph.main_bottom);
    assert_eq!(graph.owners["a_inner"], vec![0]);
    assert_eq!(graph.unknown_jumps, 1);
    assert!(graph
        .edges
        .iter()
        .any(|edge| edge.labels.iter().any(|label| label.contains("选项：向左"))));
    let occupied: BTreeSet<_> = graph
        .stations
        .iter()
        .map(|node| (node.column, node.lane))
        .collect();
    assert_eq!(occupied.len(), graph.stations.len());
    let mut routes = routes.clone();
    routes.scripts.push(serde_json::from_value(json!({"script":"OPENING","links":[],"missions":[{"series":"MaIn","key":"A","condition":""}],"unknown_jumps":0})).unwrap());
    let mark=serde_json::from_value(json!({"schema":1,"script":"OPENING","text_key":"D_START","text":"开篇对白","game_dir":"game","year":1,"month":1,"stage":1,"captured_utc":"2026-11-05T12:00:00Z"})).unwrap();
    let marked = RouteGraph::with_bookmarks(&timeline, &routes, &[mark]);
    let station = marked.owners["opening"][0];
    assert_eq!(
        marked.stations[station].entry.series,
        RouteGraph::MARKED_SERIES
    );
    assert_eq!(marked.stations[station].lane, 0);
    assert!(marked
        .edges
        .iter()
        .any(|edge| edge.from == station && edge.to == 0 && !edge.by_date));
}
