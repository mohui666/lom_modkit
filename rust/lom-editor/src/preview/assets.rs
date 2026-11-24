use super::{egui, opaque_bounds, Rect, Value};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
};

type AssetMap = BTreeMap<String, Vec<u8>>;
pub(super) type AssetSnapshot = Arc<AssetMap>;
type ImageLocation = (String, Arc<Value>);

struct Content {
    path: String,
    metadata: Arc<Value>,
}

impl Content {
    fn read(path: &str, bytes: &[u8]) -> Option<Self> {
        if !path.ends_with("/content.json") {
            return None;
        }
        Some(Self {
            path: path.to_owned(),
            metadata: Arc::new(serde_json::from_slice(bytes).ok()?),
        })
    }

    fn user_image(&self, character: bool, portrait: &str) -> Option<ImageLocation> {
        let main = self.metadata["files"]["main"].as_str();
        let file = if character {
            self.metadata["portraits"][portrait]
                .as_str()
                .or_else(|| self.metadata["portraits"]["normal"].as_str())
                .or(main)
        } else {
            main
        }?;
        let path = Path::new(&self.path).parent()?.join(file);
        Some((
            path.to_string_lossy().replace('\\', "/"),
            self.metadata.clone(),
        ))
    }

    fn portrait_file(&self, file: &str) -> ImageLocation {
        let directory = self.path.strip_suffix("content.json").unwrap();
        (format!("{directory}{file}"), self.metadata.clone())
    }
}

pub(super) struct ProjectImages {
    pub(super) snapshot: AssetSnapshot,
    content: BTreeMap<String, Content>,
    overrides: BTreeMap<String, BTreeMap<String, ImageLocation>>,
}

impl ProjectImages {
    pub(super) fn new(snapshot: AssetSnapshot) -> Self {
        let mut content = BTreeMap::new();
        let mut overrides: BTreeMap<String, BTreeMap<String, ImageLocation>> = BTreeMap::new();
        for (path, bytes) in &*snapshot {
            let Some(entry) = Content::read(path, bytes) else {
                continue;
            };
            if path.starts_with("assets/user/character/") {
                if let Some(target) = entry.metadata["extends"].as_str() {
                    let portraits = overrides.entry(target.to_owned()).or_default();
                    for (slot, file) in entry.metadata["portraits"]
                        .as_object()
                        .into_iter()
                        .flatten()
                    {
                        if let Some(file) = file.as_str() {
                            portraits
                                .entry(slot.clone())
                                .or_insert_with(|| entry.portrait_file(file));
                        }
                    }
                }
            }
            if let Some(id) = entry.metadata["id"].as_str() {
                content.entry(id.to_owned()).or_insert(entry);
            }
        }
        Self {
            snapshot,
            content,
            overrides,
        }
    }
}

pub(super) fn user_image(
    assets: &AssetMap,
    index: Option<&ProjectImages>,
    id: &str,
    character: bool,
    portrait: &str,
) -> Option<ImageLocation> {
    match index {
        Some(index) => index.content.get(id)?.user_image(character, portrait),
        None => assets
            .iter()
            .filter_map(|(path, bytes)| Content::read(path, bytes))
            .find(|entry| entry.metadata["id"] == id)?
            .user_image(character, portrait),
    }
}

pub(super) fn portrait_override(
    assets: &AssetMap,
    index: Option<&ProjectImages>,
    target: &str,
    portrait: &str,
) -> Option<ImageLocation> {
    match index {
        Some(index) => index.overrides.get(target)?.get(portrait).cloned(),
        None => assets
            .iter()
            .filter(|(path, _)| path.starts_with("assets/user/character/"))
            .filter_map(|(path, bytes)| Content::read(path, bytes))
            .filter(|entry| entry.metadata["extends"] == target)
            .find_map(|entry| {
                let file = entry.metadata["portraits"][portrait].as_str()?;
                Some(entry.portrait_file(file))
            }),
    }
}

pub(super) const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, PartialEq)]
pub(super) enum AssetSource<'a> {
    Project(&'a [u8]),
    Library(PathBuf),
}

impl<'a> AssetSource<'a> {
    pub(super) fn bytes(self) -> std::io::Result<Cow<'a, [u8]>> {
        match self {
            Self::Project(bytes) => Ok(Cow::Borrowed(bytes)),
            Self::Library(path) => std::fs::read(path).map(Cow::Owned),
        }
    }
}

pub(super) struct Pixels {
    pub(super) image: egui::ColorImage,
    pub(super) gif: Option<crate::gif_preview::Player>,
    pub(super) opaque: Rect,
    pub(super) source_size: [usize; 2],
}

pub(super) fn decode(source: AssetSource<'_>, max_side: u32) -> Option<Pixels> {
    let bytes = source.bytes().ok()?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return None;
    }
    if lom_core::gif_image::is_gif(&bytes) {
        let decoded = lom_core::gif_image::decode(&bytes, Some(max_side.min(800))).ok()?;
        let source_size = [decoded.width as usize, decoded.height as usize];
        let opaque = opaque_bounds(decoded.frames[0].buffer());
        let gif = crate::gif_preview::Player::new(decoded);
        return Some(Pixels {
            image: (*gif.first()).clone(),
            gif: Some(gif),
            opaque,
            source_size,
        });
    }
    let image = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let source_size = [image.width() as usize, image.height() as usize];
    let image = if image.width() > max_side || image.height() > max_side {
        image.thumbnail(max_side, max_side)
    } else {
        image
    }
    .into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Some(Pixels {
        image: egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
        gif: None,
        opaque: opaque_bounds(&image),
        source_size,
    })
}

pub(super) enum LoadSource {
    Project(AssetSnapshot, String),
    Library(PathBuf),
}

struct Job {
    key: String,
    source: LoadSource,
    max_side: u32,
    ctx: egui::Context,
}

impl Job {
    fn decode(&self) -> Option<Pixels> {
        let source = match &self.source {
            LoadSource::Project(assets, path) => AssetSource::Project(assets.get(path)?),
            LoadSource::Library(path) => AssetSource::Library(path.clone()),
        };
        decode(source, self.max_side)
    }
}

pub(super) enum Load {
    Ready(Pixels),
    Pending,
    Missing,
}

pub(super) struct Loader {
    jobs: mpsc::Sender<Job>,
    results: mpsc::Receiver<(String, Option<Pixels>)>,
    queued: BTreeSet<String>,
    ready: BTreeMap<String, Pixels>,
    failed: BTreeSet<String>,
}

impl Loader {
    pub(super) fn new() -> Option<Self> {
        let (jobs, incoming) = mpsc::channel::<Job>();
        let (complete, results) = mpsc::channel();
        std::thread::Builder::new()
            .name("preview-images".into())
            .spawn(move || {
                while let Ok(job) = incoming.recv() {
                    let pixels = job.decode();
                    if complete.send((job.key, pixels)).is_err() {
                        break;
                    }
                    job.ctx.request_repaint();
                }
            })
            .ok()?;
        Some(Self {
            jobs,
            results,
            queued: BTreeSet::new(),
            ready: BTreeMap::new(),
            failed: BTreeSet::new(),
        })
    }

    pub(super) fn begin_frame(&mut self) {
        // Completed uploads are only retained for the next render pass. An image
        // from a view that was closed meanwhile must not keep a pixel buffer.
        self.ready.clear();
        while let Ok((key, result)) = self.results.try_recv() {
            self.queued.remove(&key);
            if let Some(pixels) = result {
                self.ready.insert(key, pixels);
            } else {
                self.failed.insert(key);
            }
        }
    }

    pub(super) fn clear_failures(&mut self) {
        self.failed.clear();
    }

    pub(super) fn load(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        source: LoadSource,
        max_side: u32,
    ) -> Load {
        if let Some(pixels) = self.ready.remove(key) {
            return Load::Ready(pixels);
        }
        if self.failed.contains(key) {
            return Load::Missing;
        }
        // Bound both the queued work and the unconsumed results. A completed
        // job requests a repaint so visible images skipped here get another turn.
        if self.queued.len() < 4 && !self.queued.contains(key) {
            if self
                .jobs
                .send(Job {
                    key: key.to_owned(),
                    source,
                    max_side,
                    ctx: ctx.clone(),
                })
                .is_err()
            {
                return Load::Missing;
            }
            self.queued.insert(key.to_owned());
        }
        Load::Pending
    }
}
