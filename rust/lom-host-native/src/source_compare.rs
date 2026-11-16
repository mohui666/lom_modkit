//! Classification over the JSON tape *after* C# has validated/patched Mono
//! numbers and HexNumber escapes. No serialization or numeric reparsing.
use crate::json::Node;
use std::{
    collections::{HashMap, HashSet},
    mem::size_of,
    slice,
};

#[repr(C)]
pub(crate) struct DocumentView {
    pub source: *const u16,
    pub nodes: *const Node,
    pub text: *const u16,
    pub source_len: i32,
    pub node_count: i32,
    pub text_len: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Mark {
    pub id_start: i32,
    pub id_len: i32,
    pub flags: u32,
}
#[repr(C)]
pub struct Header {
    pub status: u32,
    pub logic_changed: u32,
    pub original_verified: u32,
    pub target_start: i32,
    pub target_len: i32,
    pub source_len: i32,
    pub mark_count: i32,
    pub source: *const u16,
    pub marks: *const Mark,
}
#[repr(C)]
struct Owned {
    header: Header,
    source: Vec<u16>,
    marks: Vec<Mark>,
}
type Result<T> = std::result::Result<T, u32>;
type Object<'a> = HashMap<&'a [u16], usize>;
const CAST_OBJECT: u32 = 80;
const CAST_STRING: u32 = 90;
const NULL: u32 = 22;
const NULL_KEY: u32 = 23;
const NULL_DICTIONARY: u32 = 24;

struct Reader<'a> {
    source: &'a [u16],
    nodes: &'a [Node],
    text: &'a [u16],
}
fn ascii(value: &[u16], key: &str) -> bool {
    value.len() == key.len() && value.iter().zip(key.bytes()).all(|(&a, b)| a == b as u16)
}
impl<'a> Reader<'a> {
    fn node(&self, index: usize) -> Result<&Node> {
        self.nodes.get(index).ok_or(0)
    }
    fn location(&self, start: i32, len: i32) -> Result<&'a [u16]> {
        if len < 0 {
            return Err(0);
        }
        let (data, at) = if start >= 0 {
            (self.source, start as usize)
        } else {
            (self.text, (-1i64 - start as i64) as usize)
        };
        data.get(at..at.checked_add(len as usize).ok_or(0u32)?)
            .ok_or(0)
    }
    fn string(&self, index: usize) -> Result<Option<&'a [u16]>> {
        let n = self.node(index)?;
        if n.kind == 0 {
            Ok(None)
        } else if n.kind == 4 {
            Ok(Some(self.location(n.start, n.len)?))
        } else {
            Err(CAST_STRING + n.kind as u32)
        }
    }
    fn children(&self, index: usize) -> Result<Vec<usize>> {
        let n = self.node(index)?;
        let end = usize::try_from(n.next).map_err(|_| 0u32)?;
        if end <= index || end > self.nodes.len() {
            return Err(0);
        }
        let mut at = index + 1;
        let mut result = Vec::new();
        while at < end {
            result.push(at);
            let next = usize::try_from(self.node(at)?.next).map_err(|_| 0u32)?;
            if next <= at || next > end {
                return Err(0);
            }
            at = next;
        }
        Ok(result)
    }
    fn object(&self, index: usize) -> Result<Object<'a>> {
        let n = self.node(index)?;
        if n.kind == 0 {
            return Err(NULL);
        }
        if n.kind != 6 {
            return Err(CAST_OBJECT + n.kind as u32);
        }
        let mut result = HashMap::new();
        for at in self.children(index)? {
            let n = self.node(at)?;
            result.insert(self.location(n.key_start, n.key_len)?, at); // last write wins
        }
        Ok(result)
    }
    fn ordered_values(&self, index: usize) -> Result<Vec<usize>> {
        let fields = self.object(index)?;
        let mut seen = HashSet::new();
        let mut values = Vec::new();
        // Dictionary overwrites retain the first key's slot. Invalid language
        // objects must therefore fail in that same insertion order.
        for at in self.children(index)? {
            let n = self.node(at)?;
            let key = self.location(n.key_start, n.key_len)?;
            if seen.insert(key) {
                values.push(fields[key]);
            }
        }
        Ok(values)
    }
    fn get(&self, object: &Object<'a>, key: &str) -> Option<usize> {
        object
            .iter()
            .find_map(|(&name, &value)| ascii(name, key).then_some(value))
    }
    fn required(&self, object: &Object<'a>, key: &str) -> Result<usize> {
        self.get(object, key).ok_or(match key {
            "schema" => 30,
            "target" => 31,
            "origins" => 32,
            "nodes" => 33,
            "code" => 34,
            "id" => 35,
            "type" => 36,
            "node" => 37,
            _ => 0,
        })
    }
    fn is_true(&self, value: Option<usize>) -> bool {
        value
            .and_then(|i| self.nodes.get(i))
            .is_some_and(|n| n.kind == 2)
    }
    fn origin(&self, object: &Object<'a>) -> Result<i32> {
        let Some(editor) = self.get(object, "_editor") else {
            return Ok(-1);
        };
        if self.node(editor)?.kind != 6 {
            return Ok(-1);
        }
        let editor = self.object(editor)?;
        let Some(index) = self.get(&editor, "vanilla_origin") else {
            return Ok(-1);
        };
        let n = self.node(index)?;
        if n.kind != 3
            || !n.number.is_finite()
            || n.number < 0.0
            || n.number > i32::MAX as f64
            || n.number.trunc() != n.number
        {
            return Ok(-1);
        }
        Ok(n.number as i32)
    }
    fn equal_value(&self, left: usize, right: usize) -> Result<bool> {
        let (a, b) = (self.node(left)?, self.node(right)?);
        if a.kind != b.kind {
            return Ok(false);
        }
        match a.kind {
            0..=2 => Ok(true),
            3 => Ok(a.number == b.number || a.number.is_nan() && b.number.is_nan()),
            4 => Ok(self.location(a.start, a.len)? == self.location(b.start, b.len)?),
            5 => {
                let (x, y) = (self.children(left)?, self.children(right)?);
                if x.len() != y.len() {
                    return Ok(false);
                }
                for (a, b) in x.into_iter().zip(y) {
                    if !self.equal_value(a, b)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            6 => self.equal_objects(&self.object(left)?, &self.object(right)?, 0, false, false),
            _ => Err(0),
        }
    }
    fn equal_objects(
        &self,
        left: &Object<'a>,
        right: &Object<'a>,
        edges: u32,
        say: bool,
        root: bool,
    ) -> Result<bool> {
        let skip = |key: &[u16]| {
            root && (ascii(key, "id")
                || ascii(key, "_editor")
                || ["goto", "success", "failure"]
                    .iter()
                    .enumerate()
                    .any(|(i, k)| edges & (1 << i) != 0 && ascii(key, k))
                || say
                    && ["text", "font_size", "voice", "mood_icon", "mood_character"]
                        .iter()
                        .any(|k| ascii(key, k)))
        };
        if left.keys().filter(|k| !skip(k)).count() != right.keys().filter(|k| !skip(k)).count() {
            return Ok(false);
        }
        for (&key, &value) in left {
            if skip(key) {
                continue;
            }
            let Some(&other) = right.get(key) else {
                return Ok(false);
            };
            if !self.equal_value(value, other)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
fn transient(kind: &[u16]) -> bool {
    [
        "say",
        "wait",
        "choice",
        "flag_check",
        "stat_check",
        "affinity_check",
        "item_check",
        "talent_check",
        "quest_check",
        "persistent_check",
    ]
    .iter()
    .any(|k| ascii(kind, k))
}
fn valid_target(target: &[u16]) -> bool {
    !target.is_empty()
        && target.len() <= 64
        && !target.starts_with(&[77, 79, 68, 95])
        && target
            .iter()
            .all(|&u| matches!(u, 65..=90 | 97..=122 | 48..=57 | 95 | 45))
}
fn utf8_bytes(text: &[u16]) -> usize {
    let mut count = 0;
    let mut at = 0;
    while at < text.len() {
        let u = text[at];
        if (0xD800..=0xDBFF).contains(&u)
            && at + 1 < text.len()
            && (0xDC00..=0xDFFF).contains(&text[at + 1])
        {
            count += 4;
            at += 2;
        } else {
            count += if u < 0x80 {
                1
            } else if u < 0x800 {
                2
            } else {
                3
            };
            at += 1;
        }
    }
    count
}
fn detour<'a>(
    r: &Reader<'a>,
    actual: &'a [u16],
    expected: &[u16],
    nodes: &HashMap<&'a [u16], Object<'a>>,
    following: &HashMap<&'a [u16], Option<&'a [u16]>>,
    original: &HashSet<&'a [u16]>,
) -> Result<bool> {
    let mut pending = vec![Some(actual)];
    let mut seen = HashSet::new();
    while let Some(id) = pending.pop() {
        let Some(id) = id else {
            return Err(NULL_KEY);
        };
        if id == expected {
            continue;
        }
        if !seen.insert(id) || original.contains(id) {
            return Ok(false);
        }
        let Some(node) = nodes.get(id) else {
            return Ok(false);
        };
        let Some(kind) = r.string(r.required(node, "type")?)? else {
            return Ok(false);
        };
        if ascii(kind, "end") && r.is_true(r.get(node, "resume_original")) {
            if let Some(resume) = r.get(node, "resume_node") {
                if r.node(resume)?.kind == 4 {
                    pending.push(r.string(resume)?);
                    continue;
                }
            }
        }
        if !transient(kind) {
            return Ok(false);
        }
        let mut targets = Vec::new();
        for key in ["goto", "success", "failure"] {
            if let Some(target) = r.get(node, key) {
                if r.node(target)?.kind == 4 {
                    targets.push(r.string(target)?);
                }
            }
        }
        if let Some(options) = r.get(node, "options") {
            if r.node(options)?.kind == 5 {
                for option in r.children(options)? {
                    let option = r.object(option)?;
                    if let Some(target) = r.get(&option, "goto") {
                        if r.node(target)?.kind == 4 {
                            targets.push(r.string(target)?);
                        }
                    }
                }
            }
        }
        if targets.is_empty() {
            let Some(next) = following.get(id) else {
                return Ok(false);
            };
            targets.push(*next);
        }
        pending.extend(targets);
    }
    Ok(true)
}

fn classify(r: &Reader<'_>) -> Result<(u32, u32, i32, i32, Vec<u16>, Vec<Mark>)> {
    if r.node(0)?.kind != 6 {
        return Err(1);
    }
    let story = r.object(0)?;
    let Some(meta) = r.get(&story, "vanilla_import") else {
        return Err(1);
    };
    if r.node(meta)?.kind != 6 {
        return Err(10);
    }
    let meta = r.object(meta)?;
    let schema = r.node(r.required(&meta, "schema")?)?;
    if schema.kind != 3 || !(schema.number == 1.0 || schema.number == 2.0) {
        return Err(10);
    }
    let external = r.is_true(r.get(&meta, "external"));
    let mut logic = external;
    let target_index = r.required(&meta, "target")?;
    let target_node = r.node(target_index)?;
    if target_node.kind != 4 || !valid_target(r.location(target_node.start, target_node.len)?) {
        return Err(11);
    }
    let origins_index = r.required(&meta, "origins")?;
    let nodes_index = r.required(&story, "nodes")?;
    if r.node(origins_index)?.kind != 5 || r.node(nodes_index)?.kind != 5 {
        return Err(12);
    }
    let origins = r.children(origins_index)?;
    let node_indices = r.children(nodes_index)?;
    if origins.is_empty() || origins.len() > 16384 {
        return Err(12);
    }
    let mut source = Vec::new();
    let mut too_large = false;
    for &index in &origins {
        let origin = r.object(index)?;
        if let Some(code) = r.string(r.required(&origin, "code")?)? {
            if source.len() + code.len() <= 4 * 1024 * 1024 && !too_large {
                source.extend_from_slice(code);
            } else {
                too_large = true;
            }
        }
    }
    if too_large || utf8_bytes(&source) > 4 * 1024 * 1024 {
        return Err(13);
    }
    let mut list = Vec::new();
    let mut by_id = HashMap::new();
    let mut following = HashMap::new();
    let mut original_ids = HashSet::new();
    for &index in &node_indices {
        let node = r.object(index)?;
        let id_index = r.required(&node, "id")?;
        let id = r.string(id_index)?.ok_or(NULL_KEY)?;
        list.push((node, id, id_index));
    }
    for i in 0..list.len() {
        let (node, id, _) = &list[i];
        by_id.insert(*id, node.clone());
        if i + 1 < list.len() {
            following.insert(*id, Some(list[i + 1].1));
        }
        let origin = r.origin(node)?;
        if origin >= 0 && (origin as usize) < origins.len() {
            original_ids.insert(*id);
        }
    }
    let mut claimed = HashSet::new();
    let mut previous = -1;
    let mut marks = Vec::new();
    for (node, id, id_index) in &list {
        let index = r.origin(node)?;
        let native = index >= 0 && (index as usize) < origins.len() && claimed.insert(index);
        let mut baseline = None;
        let mut edges = 0;
        if native {
            if index <= previous {
                logic = true;
            }
            previous = index;
            let origin = r.object(origins[index as usize])?;
            let baseline_index = r.required(&origin, "node")?;
            if r.node(baseline_index)?.kind == 0 {
                return Err(if schema.number == 2.0 {
                    NULL
                } else {
                    NULL_DICTIONARY
                });
            }
            let original = r.object(baseline_index)?;
            if schema.number == 2.0 {
                for (bit, key) in ["goto", "success", "failure"].iter().enumerate() {
                    if let (Some(a), Some(b)) = (r.get(&original, key), r.get(node, key)) {
                        if r.node(a)?.kind == 4
                            && r.node(b)?.kind == 4
                            && detour(
                                r,
                                r.string(b)?.ok_or(NULL)?,
                                r.string(a)?.ok_or(NULL)?,
                                &by_id,
                                &following,
                                &original_ids,
                            )?
                        {
                            edges |= 1 << bit;
                        }
                    }
                }
            }
            if r.equal_objects(&original, node, edges, false, true)? {
                let n = r.node(*id_index)?;
                marks.push(Mark {
                    id_start: n.start,
                    id_len: n.len,
                    flags: 1,
                });
                continue;
            }
            let kind = r.string(r.required(&original, "type")?)?;
            if kind.is_some_and(|k| ascii(k, "raw")) {
                logic = true;
            }
            baseline = Some(original);
        }
        let kind = r.string(r.required(node, "type")?)?;
        let mut persistent = kind.is_none_or(|k| !transient(k));
        if kind.is_some_and(|k| ascii(k, "end")) && r.is_true(r.get(node, "resume_original")) {
            persistent = false;
        } else {
            if kind.is_some_and(|k| ascii(k, "say"))
                && !native
                && !r.is_true(r.get(node, "inherit_dialog"))
            {
                persistent = true;
            }
            if kind.is_some_and(|k| ascii(k, "say"))
                && native
                && !r.equal_objects(baseline.as_ref().ok_or(0u32)?, node, edges, true, true)?
            {
                persistent = true;
            }
        }
        let n = r.node(*id_index)?;
        let _ = id;
        marks.push(Mark {
            id_start: n.start,
            id_len: n.len,
            flags: if persistent { 2 } else { 0 },
        });
    }
    if claimed.len() != origins.len() {
        logic = true;
    }
    if r.get(&story, "mood").is_some_and(|i| r.nodes[i].kind == 1) {
        logic = true;
    }
    if let Some(localized) = r.get(&story, "localization") {
        if r.node(localized)?.kind == 6 {
            let localized = r.object(localized)?;
            if let Some(translations) = r.get(&localized, "translations") {
                for language in r.ordered_values(translations)? {
                    for &key in r.object(language)?.keys() {
                        if let Some(dot) = key.iter().position(|&u| u == 46) {
                            if dot > 0 && !key.starts_with(&[115, 116, 111, 114, 121, 46]) {
                                for mark in &mut marks {
                                    if r.location(mark.id_start, mark.id_len)? == &key[..dot] {
                                        mark.flags &= !1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok((
        logic as u32,
        external as u32,
        target_node.start,
        target_node.len,
        source,
        marks,
    ))
}

/// Classify one complete validated document in a single call. The result owns
/// only concatenated source and marks; target/id spans still reference view.
/// # Safety
/// view and its arrays are valid immutable parser-owned buffers for this call.
/// Returned header must be freed exactly once with lom_source_compare_free_v1.
#[no_mangle]
pub(crate) unsafe extern "C" fn lom_source_compare_v1(view: *const DocumentView) -> *mut Header {
    if view.is_null() || !view.is_aligned() {
        return std::ptr::null_mut();
    }
    let v = unsafe { &*view };
    if v.source_len < 0
        || v.text_len < 0
        || v.node_count <= 0
        || v.nodes.is_null()
        || !v.nodes.is_aligned()
        || v.source_len > 0 && (v.source.is_null() || !v.source.is_aligned())
        || v.text_len > 0 && (v.text.is_null() || !v.text.is_aligned())
        || v.node_count as usize > isize::MAX as usize / size_of::<Node>()
        || v.source_len as usize > isize::MAX as usize / 2
        || v.text_len as usize > isize::MAX as usize / 2
    {
        return std::ptr::null_mut();
    }
    let r = Reader {
        source: if v.source_len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(v.source, v.source_len as usize) }
        },
        nodes: unsafe { slice::from_raw_parts(v.nodes, v.node_count as usize) },
        text: if v.text_len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(v.text, v.text_len as usize) }
        },
    };
    let mut owned = Owned {
        header: Header {
            status: 0,
            logic_changed: 0,
            original_verified: 0,
            target_start: 0,
            target_len: 0,
            source_len: 0,
            mark_count: 0,
            source: std::ptr::null(),
            marks: std::ptr::null(),
        },
        source: Vec::new(),
        marks: Vec::new(),
    };
    match classify(&r) {
        Err(status) => owned.header.status = status,
        Ok((logic, verified, start, len, source, marks)) => {
            owned.header.status = 2;
            owned.header.logic_changed = logic;
            owned.header.original_verified = verified;
            owned.header.target_start = start;
            owned.header.target_len = len;
            owned.source = source;
            owned.marks = marks;
        }
    }
    owned.header.source_len = owned.source.len() as i32;
    owned.header.mark_count = owned.marks.len() as i32;
    owned.header.source = owned.source.as_ptr();
    owned.header.marks = owned.marks.as_ptr();
    Box::into_raw(Box::new(owned)).cast()
}
/// # Safety
/// result is the nonnull owned result of exactly one classify call, not freed.
#[no_mangle]
pub(crate) unsafe extern "C" fn lom_source_compare_free_v1(result: *mut Header) {
    if !result.is_null() {
        drop(unsafe { Box::from_raw(result.cast::<Owned>()) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validated_numbers_keep_distinct_bits_and_nested_metadata() {
        let input: Vec<u16> = r#"{"vanilla_import":{"schema":2,"target":"S1","origins":[{"code":"正文","node":{"type":"say","value":1.0000000000000002,"nested":{"id":3}}}]},"nodes":[{"id":"a","type":"say","value":1.0000000000000004,"nested":{"id":3},"_editor":{"vanilla_origin":0}}]}"#.encode_utf16().collect();
        let mut document = crate::json::parse_document(&input);
        assert!(document.error.is_none());
        // Runtime conversion happens in C#. These fixed bit patterns test the
        // already-validated tape contract, not Rust decimal parsing policy.
        for v in &document.validations {
            let raw = &input[v.start as usize..(v.start + v.len) as usize];
            document.nodes[v.target as usize].number = if ascii(raw, "2") {
                2.0
            } else if ascii(raw, "3") {
                3.0
            } else if ascii(raw, "0") {
                0.0
            } else if ascii(raw, "1.0000000000000002") {
                f64::from_bits(0x3ff0000000000001)
            } else {
                f64::from_bits(0x3ff0000000000002)
            };
        }
        let r = Reader {
            source: &input,
            nodes: &document.nodes,
            text: &document.text,
        };
        let (_, _, _, _, source, marks) = classify(&r).unwrap();
        assert_eq!(source, "正文".encode_utf16().collect::<Vec<_>>());
        assert_eq!(marks[0].flags, 2);
        let changed = document
            .nodes
            .iter()
            .position(|n| n.kind == 3 && n.number.to_bits() == 0x3ff0000000000002)
            .unwrap();
        document.nodes[changed].number = f64::from_bits(0x3ff0000000000001);
        let r = Reader {
            source: &input,
            nodes: &document.nodes,
            text: &document.text,
        };
        assert_eq!(classify(&r).unwrap().5[0].flags, 1);
    }

    #[test]
    fn source_result_owns_only_outputs_and_uses_matching_abi() {
        let input: Vec<u16> = "null".encode_utf16().collect();
        let document = crate::json::parse_document(&input);
        let view = DocumentView {
            source: input.as_ptr(),
            nodes: document.nodes.as_ptr(),
            text: document.text.as_ptr(),
            source_len: input.len() as i32,
            node_count: document.nodes.len() as i32,
            text_len: document.text.len() as i32,
        };
        unsafe {
            assert!(lom_source_compare_v1(std::ptr::null()).is_null());
            let result = lom_source_compare_v1(&view);
            assert_eq!((*result).status, 1);
            lom_source_compare_free_v1(result);
            lom_source_compare_free_v1(std::ptr::null_mut());
        }
        assert_eq!(
            size_of::<DocumentView>(),
            if cfg!(target_pointer_width = "64") {
                40
            } else {
                24
            }
        );
        assert_eq!(
            size_of::<Header>(),
            if cfg!(target_pointer_width = "64") {
                48
            } else {
                36
            }
        );
        assert_eq!(size_of::<Mark>(), 12);
        assert_eq!(utf8_bytes(&[0xd800, 0xdc00, 0xdc00]), 7);
    }
}
