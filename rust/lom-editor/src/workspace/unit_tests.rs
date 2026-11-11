use super::*;
#[test]
fn copy_retargets_nested_edges_without_rewriting_prose() {
    let mut v = json!({"id":"copy","text":"old","options":[{"goto":"old","text":"old"}],"_editor":{"sections":[{"start":"old","end":"old"}]}});
    retarget(&mut v, &BTreeMap::from([("old".into(), "new".into())]));
    assert_eq!(v["options"][0]["goto"], "new");
    assert_eq!(v["text"], "old");
    assert_eq!(v["_editor"]["sections"][0]["start"], "new");
}
#[test]
fn bulk_edit_keeps_code_and_identifiers() {
    let mut v = json!({"id":"hello","title":"hello","nodes":[{"id":"hello","type":"say","text":"hello hello"},{"id":"lua","type":"raw","code":"hello"}]});
    assert_eq!(replace_prose(&mut v, "hello", "goodbye"), 3);
    assert_eq!(v["id"], "hello");
    assert_eq!(v["nodes"][1]["code"], "hello");
}
#[test]
fn all_catalog_node_forms_have_defaults() {
    let c = Catalog::new();
    let schemas = c.schema["NODE_SCHEMAS"].as_object().unwrap();
    assert_eq!(schemas.len(), 72);
    for kind in schemas.keys() {
        let node = c.new_node(kind, "n1".into());
        assert_eq!(node["type"], kind.as_str());
        assert_eq!(node["id"], "n1");
        assert!(node.is_object());
    }
}
