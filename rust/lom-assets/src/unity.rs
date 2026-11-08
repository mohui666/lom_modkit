use anyhow::{bail, ensure, Context, Result};
use std::{collections::BTreeSet, path::Path};
use unity_rs_core::{
    loader::{AssetCollection, AssetLoadLimits},
    scene::resolve_object_reference,
    serialized::ObjectReference,
    studio::{Studio, StudioObject},
};

pub struct Reader<'a> {
    pub data: &'a [u8],
    pub at: usize,
}
impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }
    pub fn at(data: &'a [u8], at: usize) -> Self {
        Self { data, at }
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).context("资源范围溢出")?;
        let bytes = self.data.get(self.at..end).context("资源数据不完整")?;
        self.at = end;
        Ok(bytes)
    }
    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n)?;
        Ok(())
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into()?))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into()?))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.bytes(4)?.try_into()?))
    }
    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.bytes(8)?.try_into()?))
    }
    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.bytes(4)?.try_into()?))
    }
    pub fn ptr(&mut self) -> Result<ObjectReference> {
        Ok(ObjectReference {
            file_id: self.i32()?,
            path_id: self.i64()?,
        })
    }
    pub fn string(&mut self) -> Result<String> {
        let n = self.i32()?;
        ensure!(n >= 0, "资源字符串长度无效");
        let value = String::from_utf8(self.bytes(n as usize)?.to_vec())?;
        self.at = (self.at + 3) & !3;
        ensure!(self.at <= self.data.len(), "资源字符串不完整");
        Ok(value)
    }
    pub fn count(&mut self, limit: usize) -> Result<usize> {
        let n = self.i32()?;
        ensure!(n >= 0 && n as usize <= limit, "资源条目数无效：{n}");
        Ok(n as usize)
    }
    pub fn end(&self) -> Result<()> {
        ensure!(self.at == self.data.len(), "资源包含尚不支持的播放规则");
        Ok(())
    }
}

/// Load only the referenced serialized files. Streamed textures stay source-bound.
pub fn open(path: &Path) -> Result<Studio> {
    let root = path.parent().context("资源路径缺少父目录")?;
    let mut parts = Studio::open(path)?.into_collection().into_parts();
    let mut loaded: BTreeSet<String> = parts
        .serialized_files
        .iter()
        .map(|f| basename(&f.path).to_lowercase())
        .collect();
    let mut at = 0;
    while at < parts.serialized_files.len() {
        let dependencies: Vec<_> = parts.serialized_files[at]
            .file
            .externals
            .iter()
            .map(|x| x.path.clone())
            .collect();
        at += 1;
        for dependency in dependencies {
            let name = basename(&dependency);
            if loaded.contains(&name.to_lowercase()) {
                continue;
            }
            let target = root.join(&dependency);
            if !target.is_file() {
                continue;
            } // Engine built-ins need not be present for these readers.
            let other = Studio::open(&target)
                .with_context(|| format!("读取引用资源 {}", target.display()))?
                .into_collection()
                .into_parts();
            for file in other.serialized_files {
                if loaded.insert(basename(&file.path).to_lowercase()) {
                    parts.serialized_files.push(file);
                }
            }
            for resource in other.resources {
                if !parts.resources.iter().any(|x| x.path == resource.path) {
                    parts.resources.push(resource);
                }
            }
        }
    }
    let mut collection = AssetCollection::from_parts(parts);
    collection.rebuild_reference_index(AssetLoadLimits::default())?;
    collection.rebuild_resource_index(AssetLoadLimits::default())?;
    collection.resolve_object_metadata(AssetLoadLimits::default())?;
    collection.rebuild_sprite_atlas_index(AssetLoadLimits::default())?;
    Ok(Studio::from_collection(collection))
}
pub fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
pub fn resolve<'a>(
    studio: &'a Studio,
    owner: StudioObject<'_>,
    pointer: ObjectReference,
) -> Result<StudioObject<'a>> {
    let object = resolve_object_reference(studio.collection(), owner.file_index(), pointer)?
        .context("资源引用为空")?;
    studio
        .object_by_index(object.file_index, object.object_index)
        .context("资源引用不存在")
}
pub fn raw(object: StudioObject<'_>) -> Result<Vec<u8>> {
    Ok(object.read_raw(256 * 1024 * 1024)?)
}
pub fn script_type(studio: &Studio, object: StudioObject<'_>) -> Result<(String, String)> {
    ensure!(object.class_id() == 114, "资源不是脚本对象");
    let bytes = raw(object)?;
    let pointer = Reader::at(&bytes, 16).ptr()?;
    if pointer.file_id > 0 {
        let file = &studio.collection().serialized_files()[object.file_index()].file;
        if file
            .externals
            .get(pointer.file_id as usize - 1)
            .is_some_and(|e| e.path == "Library/unity default resources")
        {
            // Built-in Unity behaviours are not any of the Mortal/Lean data classes.
            return Ok((String::new(), String::new()));
        }
    }
    let script = resolve(studio, object, pointer)?.read_mono_script(Default::default())?;
    Ok((script.namespace, script.class_name))
}
pub fn mapping(
    studio: &Studio,
    owner: StudioObject<'_>,
    pointer: ObjectReference,
) -> Result<String> {
    let bytes = raw(resolve(studio, owner, pointer)?)?;
    let mut r = Reader::at(&bytes, 28);
    r.string()?;
    r.string()?;
    r.string()
}
pub fn unique<'a>(studio: &'a Studio, class: i32, name: &str) -> Result<StudioObject<'a>> {
    let mut items = studio
        .objects()
        .filter(|x| x.class_id() == class && x.name() == Some(name));
    let item = items.next().with_context(|| format!("找不到资源 {name}"))?;
    if items.next().is_some() {
        bail!("资源名称不唯一：{name}");
    }
    Ok(item)
}
