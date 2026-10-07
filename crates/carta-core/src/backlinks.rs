use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use carta_format::{ArchiveId, DocumentId};
use serde::{Deserialize, Serialize};

use crate::archive::atomic_replace;
use crate::{
    extract_markdown_links, Backlink, CartaLinkTarget, DocumentInfo, Error, MarkdownLink,
};

const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct FileFingerprint {
    len: u64,
    modified_secs: u64,
    modified_nanos: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CachedLink {
    link: MarkdownLink,
    context: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CachedSource {
    document: DocumentId,
    fingerprint: Option<FileFingerprint>,
    label: String,
    links: Vec<CachedLink>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    sources: Vec<CachedSource>,
}

#[derive(Debug)]
pub(crate) struct BacklinkIndex {
    sources: BTreeMap<DocumentId, CachedSource>,
    inverse: HashMap<CartaLinkTarget, Vec<Backlink>>,
    cache_path: Option<PathBuf>,
    dirty: bool,
}

impl BacklinkIndex {
    pub(crate) fn load(
        documents: &BTreeMap<DocumentId, DocumentInfo>,
        cache_path: Option<PathBuf>,
    ) -> Self {
        let cached = cache_path
            .as_deref()
            .and_then(load_cache)
            .filter(|cache| cache.version == CACHE_VERSION);
        let mut previous: BTreeMap<_, _> = cached
            .as_ref()
            .map(|cache| {
                cache
                    .sources
                    .iter()
                    .cloned()
                    .map(|source| (source.document, source))
                    .collect()
            })
            .unwrap_or_default();
        let mut changed = cached.is_none();
        let mut sources = BTreeMap::new();

        for (id, info) in documents {
            let fingerprint = fingerprint(&info.path.join("content.md"));
            let source = match previous.remove(id) {
                Some(source)
                    if fingerprint.is_some() && source.fingerprint == fingerprint =>
                {
                    source
                }
                _ => {
                    changed = true;
                    CachedSource::from_info(info, fingerprint)
                }
            };
            sources.insert(*id, source);
        }
        if !previous.is_empty() {
            changed = true;
        }

        let mut index = Self {
            sources,
            inverse: HashMap::new(),
            cache_path,
            dirty: changed,
        };
        index.rebuild_inverse();
        index.flush();
        index
    }

    pub(crate) fn enable_cache(&mut self, cache_root: PathBuf, archive: ArchiveId) {
        self.cache_path = Some(
            cache_root
                .join("backlinks-v1")
                .join(format!("{archive}.json")),
        );
        self.dirty = true;
        self.flush();
    }

    pub(crate) fn refresh(&mut self, documents: &BTreeMap<DocumentId, DocumentInfo>) {
        let mut previous = std::mem::take(&mut self.sources);
        let mut sources = BTreeMap::new();
        let mut changed = false;

        for (id, info) in documents {
            let fingerprint = fingerprint(&info.path.join("content.md"));
            let source = match previous.remove(id) {
                Some(source)
                    if fingerprint.is_some() && source.fingerprint == fingerprint =>
                {
                    source
                }
                _ => {
                    changed = true;
                    CachedSource::from_info(info, fingerprint)
                }
            };
            sources.insert(*id, source);
        }
        if !previous.is_empty() {
            changed = true;
        }

        self.sources = sources;
        self.rebuild_inverse();
        self.dirty |= changed;
    }

    pub(crate) fn update_document(&mut self, info: &DocumentInfo) {
        let id = info.id();
        if let Some(previous) = self.sources.remove(&id) {
            self.remove_source_from_inverse(id, &previous);
        }
        let source = CachedSource::from_info(
            info,
            fingerprint(&info.path.join("content.md")),
        );
        self.add_source_to_inverse(id, &source);
        self.sources.insert(id, source);
        self.dirty = true;
    }

    pub(crate) fn backlinks(&self, target: CartaLinkTarget) -> Vec<Backlink> {
        self.inverse.get(&target).cloned().unwrap_or_default()
    }

    pub(crate) fn flush(&mut self) {
        if !self.dirty || self.cache_path.is_none() {
            return;
        }
        if self.persist().is_ok() {
            self.dirty = false;
        }
    }

    fn rebuild_inverse(&mut self) {
        self.inverse.clear();
        let sources: Vec<_> = self
            .sources
            .iter()
            .map(|(id, source)| (*id, source.clone()))
            .collect();
        for (id, source) in sources {
            self.add_source_to_inverse(id, &source);
        }
    }

    fn remove_source_from_inverse(&mut self, id: DocumentId, source: &CachedSource) {
        let targets: HashSet<_> = source
            .links
            .iter()
            .filter_map(|cached| cached.link.carta_target())
            .collect();
        for target in targets {
            let empty = if let Some(entries) = self.inverse.get_mut(&target) {
                entries.retain(|backlink| backlink.source() != id);
                entries.is_empty()
            } else {
                false
            };
            if empty {
                self.inverse.remove(&target);
            }
        }
    }

    fn add_source_to_inverse(&mut self, id: DocumentId, source: &CachedSource) {
        let mut touched = HashSet::new();
        for cached in &source.links {
            let Some(target) = cached.link.carta_target() else {
                continue;
            };
            touched.insert(target);
            self.inverse.entry(target).or_default().push(Backlink::new(
                id,
                cached.link.clone(),
                source.label.clone(),
                cached.context.clone(),
            ));
        }
        for target in touched {
            if let Some(entries) = self.inverse.get_mut(&target) {
                entries.sort_by_key(|backlink| {
                    (backlink.source(), backlink.link().source_range().start)
                });
            }
        }
    }

    fn persist(&self) -> Result<(), Error> {
        let Some(path) = self.cache_path.as_deref() else {
            return Ok(());
        };
        let cache = CacheFile {
            version: CACHE_VERSION,
            sources: self.sources.values().cloned().collect(),
        };
        let bytes = serde_json::to_vec(&cache).map_err(|error| {
            Error::io(path, io::Error::new(io::ErrorKind::InvalidData, error))
        })?;
        let parent = path
            .parent()
            .expect("backlink cache path always has a parent");
        fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
        atomic_replace(path, &bytes)
    }
}

impl CachedSource {
    fn from_info(info: &DocumentInfo, fingerprint: Option<FileFingerprint>) -> Self {
        let content = std::str::from_utf8(&info.content_bytes)
            .expect("validated Document content must remain UTF-8");
        let links = extract_markdown_links(content)
            .into_iter()
            .filter(|link| link.carta_target().is_some())
            .map(|link| {
                let context = line_context(content, &link);
                CachedLink { link, context }
            })
            .collect();
        Self {
            document: info.id(),
            fingerprint,
            label: info.derived_label(content),
            links,
        }
    }
}

fn line_context(content: &str, link: &MarkdownLink) -> String {
    let range = link.source_range();
    let start = content[..range.start].rfind('\n').map_or(0, |index| index + 1);
    let end = content[range.end..]
        .find('\n')
        .map_or(content.len(), |index| range.end + index);
    content[start..end].trim().to_owned()
}

fn fingerprint(path: &Path) -> Option<FileFingerprint> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(FileFingerprint {
        len: metadata.len(),
        modified_secs: modified.as_secs(),
        modified_nanos: modified.subsec_nanos(),
    })
}

fn load_cache(path: &Path) -> Option<CacheFile> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}
