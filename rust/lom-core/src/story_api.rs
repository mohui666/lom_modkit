//! Controlled JSON authoring API, shared by the CLI and desktop tooling.
//! Every request works on a clone; an error never modifies the caller's story.
use crate::{editing, release};
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::OnceLock};
fn schema() -> &'static Value {
    static S: OnceLock<Value> = OnceLock::new();
    S.get_or_init(|| {
        serde_json::from_str(include_str!("../../lom-editor/data/authoring.json")).unwrap()
    })
}
pub fn new_node(kind: &str, id: &str) -> Result<Value> {
    static D: OnceLock<Value> = OnceLock::new();
    let defaults =
        D.get_or_init(|| serde_json::from_str(include_str!("../data/node-defaults.json")).unwrap());
    let mut node = defaults.get(kind).context("未知节点类型")?.clone();
    node["id"] = json!(id);
    Ok(node)
}
pub fn new_story(id: &str, title: &str, mood: bool) -> Result<Value> {
    ensure!(editing::valid_id(id), "剧情 ID 不合法");
    let mut story: Value = serde_json::from_str(include_str!("../data/new-story.json"))?;
    story["id"] = json!(id);
    story["title"] = json!(title);
    story["mood"] = json!(mood);
    Ok(story)
}
fn string<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str().with_context(|| format!("{k} 必须是字符串"))
}
fn default_string<'a>(v: &'a Value, k: &str, d: &'a str) -> Result<&'a str> {
    if v.get(k).is_none() {
        Ok(d)
    } else {
        string(v, k)
    }
}
fn index(story: &Value, id: &str) -> Result<usize> {
    story["nodes"]
        .as_array()
        .context("缺少 nodes")?
        .iter()
        .position(|n| n["id"] == id)
        .context("节点不存在")
}
fn next_id(story: &Value, kind: &str) -> String {
    (1..)
        .map(|i| format!("{kind}{i}"))
        .find(|id| {
            !story["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|n| n["id"] == *id)
        })
        .unwrap()
}
fn checked_fields(kind: &str, fields: &Value) -> Result<Value> {
    ensure!(
        schema()["NODE_SCHEMAS"].get(kind).is_some(),
        "未知节点类型: {kind}"
    );
    if fields.is_null() {
        return Ok(json!({}));
    }
    for (key, value) in fields.as_object().context("fields 必须是对象")? {
        if matches!(key.as_str(), "id" | "type" | "goto") {
            ensure!(value.is_string(), "{key} 必须是字符串");
            continue;
        }
        let field = schema()["NODE_SCHEMAS"][kind]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f[0] == *key)
            .with_context(|| format!("{kind} 不支持字段 {key}"))?;
        let f = field[2].as_str().unwrap();
        let ok = if [
            "int",
            "float",
            "percent_scale",
            "percent_cg_scale",
            "percent_position",
            "percent_offset",
            "percent_opacity",
            "discount_toggle",
            "bool_int",
        ]
        .contains(&f)
        {
            value.is_number()
        } else if f == "bool" {
            value.is_boolean()
        } else if [
            "options",
            "cases",
            "vars",
            "dice_options",
            "official_characters",
            "battle_faction_list",
            "combat_talents",
            "reward_entries",
            "reward_entries_optional",
            "custom_shop_items",
        ]
        .contains(&f)
        {
            value.is_array()
        } else {
            value.is_string()
        };
        ensure!(ok, "{kind}.{key} 类型不符合 {f}");
    }
    Ok(fields.clone())
}
fn normalize(node: &mut Value) -> Result<()> {
    if node["type"] == "branch" {
        let field = if node["source"] == "stat" {
            "flag"
        } else {
            "stat"
        };
        node.as_object_mut().unwrap().remove(field);
    }
    if matches!(node["type"].as_str(), Some("show" | "say")) {
        if let (Some(cid), Some(portrait)) = (
            node["character"].as_str().filter(|s| !s.is_empty()),
            node["portrait"].as_str().filter(|s| !s.is_empty()),
        ) {
            if let Some(id) = crate::content::parse_content_ref(cid)? {
                ensure!(
                    !portrait.is_empty()
                        && portrait.len() <= 64
                        && portrait
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                    "表情 ID 不合法"
                );
                let root = crate::content::default_repository_root()
                    .join("assets/user/character")
                    .join(id)
                    .join("content.json");
                if root.is_file() {
                    let meta = crate::load_json(root)?;
                    ensure!(
                        meta["portraits"].get(portrait).is_some(),
                        "用户角色没有表情 {portrait}"
                    );
                }
            } else {
                let data: Value =
                    serde_json::from_str(include_str!("../../../data/editor_data.json"))?;
                if let Some(char) = data["characters"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|c| c["id"] == cid)
                {
                    if let Some(portraits) = char["portraits"].as_array() {
                        ensure!(
                            portraits.iter().any(|p| p == portrait),
                            "{cid} 没有表情 {portrait}"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
fn make(story: &mut Value, kind: &str, fields: Value, after: Option<&str>) -> Result<Value> {
    let id = next_id(story, kind);
    let mut node = new_node(kind, &id)?;
    node.as_object_mut()
        .unwrap()
        .extend(fields.as_object().context("fields 必须为对象")?.clone());
    normalize(&mut node)?;
    let at = if let Some(id) = after {
        index(story, id)? + 1
    } else {
        story["nodes"].as_array().context("缺少 nodes")?.len()
    };
    story["nodes"]
        .as_array_mut()
        .unwrap()
        .insert(at, node.clone());
    if let Some(cid) = release::missing_stage_linear(story["nodes"].as_array().unwrap(), at) {
        let show = json!({"id":next_id(story,"show"),"type":"show","character":cid,"position":"M"});
        story["nodes"].as_array_mut().unwrap().insert(at, show);
    }
    Ok(node)
}
/// Execute one operation. `params.story` is the input; the response contains
/// both the operation result and the complete updated story.
pub fn execute(op: &str, params: &Value) -> Result<Value> {
    if op == "new_node" {
        return Ok(
            json!({"result":new_node(string(params,"node_type")?,string(params,"node_id")?)?,"after":null}),
        );
    }
    if op == "new_story" {
        let mood = params
            .get("mood")
            .map(|m| m.as_bool().context("mood 必须是布尔值"))
            .transpose()?
            .unwrap_or(false);
        let story = new_story(
            default_string(params, "story_id", "main")?,
            default_string(params, "title", "新剧情")?,
            mood,
        )?;
        return Ok(json!({"result":story,"after":null}));
    }
    let mut story = params["story"].clone();
    ensure!(story["nodes"].is_array(), "缺少 story.nodes");
    let after = params
        .get("after")
        .filter(|v| !v.is_null())
        .map(|v| v.as_str().context("after 必须是节点 ID"))
        .transpose()?;
    let result=match op{
        "get_node"=>story["nodes"][index(&story,string(params,"node_id")?)?].clone(),
        "list_nodes"=>json!(story["nodes"].as_array().unwrap().iter().map(|n|json!({"id":n["id"],"type":n["type"],"summary":n.get("text").or_else(||n.get("title")).or_else(||n.get("character")).cloned().unwrap_or(json!(""))})).collect::<Vec<_>>()),
        "add_node"=>{let kind=string(params,"node_type")?;make(&mut story,kind,checked_fields(kind,&params["fields"])?,after)?},
        "add_scene"=>{let view=string(params,"view")?;ensure!(!view.is_empty(),"view 不能为空");make(&mut story,"scene",json!({"view":view}),after)?},
        "add_say"=>{
            let text=string(params,"text")?;let mode=default_string(params,"mode","character")?;let portrait=default_string(params,"portrait","normal")?;
            ensure!(["character","think","narrative","center"].contains(&mode),"非法 say mode");
            let mut fields=json!({"text":text,"mode":mode,"portrait":portrait});
            if matches!(mode,"character"|"think"){let cid=string(params,"character")?;ensure!(!cid.is_empty(),"人物必填");fields["character"]=json!(cid);}
            if let Some(v)=params.get("voice").filter(|v|!v.is_null()){ensure!(v.as_str().is_some_and(|s|!s.trim().is_empty()),"voice 必须非空");fields["voice"]=v.clone();}
            let mut n=make(&mut story,"say",fields,after)?;
            if matches!(mode,"narrative"|"center"){n.as_object_mut().unwrap().remove("character");let i=index(&story,n["id"].as_str().unwrap())?;story["nodes"][i]=n.clone();}n
        },
        "add_death"=>{
            let text=string(params,"text")?;let id=string(params,"death_id")?;let next=default_string(params,"next","Title")?;
            ensure!(!text.trim().is_empty() && id.bytes().all(|b|b.is_ascii_digit()) && id.parse::<u128>().is_ok_and(|v|v>=900000) && next=="Title","死亡文本、ID 或返回目标不合法");
            let mut f=json!({"text":text,"death_id":id,"next":next});
            if let Some(title)=params.get("title").filter(|v|!v.is_null()){ensure!(title.is_string(),"title 必须是字符串");if title!=""{f["title"]=title.clone();}}
            let mut n=make(&mut story,"death",f,after)?;if n["title"].as_str().is_none_or(str::is_empty){n.as_object_mut().unwrap().remove("title");let i=index(&story,n["id"].as_str().unwrap())?;story["nodes"][i]=n.clone();}n
        },
        "add_choice"=>{
            let options=params["options"].as_array().context("options 必须为数组")?;ensure!((2..=4).contains(&options.len()),"需要 2~4 个选项");
            let opts=options.iter().map(|o|{ensure!(o.as_array().is_some_and(|a|a.len()==2),"选项需要 text/goto 二元组");ensure!(o[0].as_str().is_some_and(|s|!s.is_empty())&&o[1].is_string(),"选项 text/goto 类型不合法");Ok(json!({"text":o[0],"goto":o[1]}))}).collect::<Result<Vec<_>>>()?;
            make(&mut story,"choice",json!({"options":opts,"dialog":"Options"}),after)?
        },
        "add_dice"=>{
            let max=params["maximum"].as_i64().context("maximum 必须是整数")?;let bonus=params.get("bonus").map(|v|v.as_i64().context("bonus 必须是整数")).transpose()?.unwrap_or(0);
            let header=string(params,"header")?;ensure!((1..=9999).contains(&max)&&(-9999..=9999).contains(&bonus)&&!header.trim().is_empty()&&header.chars().count()<=80,"骰子参数不合法");
            let bands=params["bands"].as_array().context("bands 必须为数组")?;ensure!((2..=4).contains(&bands.len()),"需要 2~4 档");let mut prev=None;
            for (i,b)in bands.iter().enumerate(){let o=b.as_object().context("band 必须为对象")?;ensure!(string(b,"text").is_ok_and(|s|!s.trim().is_empty())&&string(b,"goto").is_ok_and(|s|!s.trim().is_empty()),"结果文本和跳转不能为空");ensure!(o.keys().all(|k|matches!(k.as_str(),"text"|"goto")||(k=="upper"&&i+1<bands.len())),"分段含未知字段");if i+1<bands.len(){let upper=b["upper"].as_i64().context("upper 必须是整数")?;ensure!(upper>=bonus&&upper<max+bonus&&prev.is_none_or(|p|upper>p),"分段必须递增且位于范围内");prev=Some(upper);}}
            let mut f=json!({"max":max,"header":header,"bands":bands,"bonus":bonus});
            for key in ["bonus_name","bonus_status"]{let v=default_string(params,key,"")?;ensure!(v.chars().count()<=80,"{key} 过长");if !v.is_empty(){f[key]=json!(v);}}
            make(&mut story,"dice",f,after)?
        },
        "update_node"|"delete_node"|"rename_node"|"move_node"|"set_start"=>{
            let id=string(params,"node_id")?;let i=index(&story,id)?;let mut node=story["nodes"][i].clone();
            match op{
                "update_node"=>{let f=checked_fields(node["type"].as_str().unwrap_or(""),&params["fields"])?;node.as_object_mut().unwrap().extend(f.as_object().unwrap().clone());normalize(&mut node)?;story["nodes"][i]=node.clone();if release::missing_stage_linear(story["nodes"].as_array().unwrap(),i).is_some(){release::ensure_stage(&mut story,node["id"].as_str().unwrap());}},
                "delete_node"=>{story["nodes"].as_array_mut().unwrap().remove(i);},
                "set_start"=>{story["start"]=json!(id);},
                "move_node"=>{let d=params["delta"].as_i64().context("delta 必须是整数")?;ensure!(d==1||d==-1,"delta 必须是 ±1");let dest=i as i64+d;ensure!(dest>=0&&(dest as usize)<story["nodes"].as_array().unwrap().len(),"移动越界");story["nodes"].as_array_mut().unwrap().swap(i,dest as usize);},
                "rename_node"=>{let new=string(params,"new_id")?.trim();ensure!(!new.is_empty()&&new.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'),"新节点 ID 不合法");ensure!(new==id||index(&story,new).is_err(),"新节点 ID 已存在");node["id"]=json!(new);story["nodes"][i]=node.clone();editing::retarget(&mut story,&BTreeMap::from([(id.into(),new.into())]));node=story["nodes"][i].clone();},_=>()
            }node
        },
        _=>bail!("未知操作 {op}")
    };
    Ok(json!({"result":result,"after":story}))
}
pub fn apply(story: &Value, operations: &Value) -> Result<Value> {
    let mut next = story.clone();
    for op in operations.as_array().context("操作必须为数组")? {
        ensure!(op.is_object(), "每条操作必须是对象");
        let mut p = op.clone();
        p["story"] = next;
        let r = execute(string(op, "op")?, &p)?;
        ensure!(
            r["after"].is_object(),
            "edit 不接受创建节点或新建剧情操作，请使用 author"
        );
        next = r["after"].clone();
    }
    Ok(next)
}
