//! Device-local, on-demand Hunspell dictionaries.
//!
//! Each supported locale maps to immutable source blobs in a pinned public
//! repository.  Download and integrity verification happen off the UI thread.
//! The Archive, its Git repository, and authored Documents are never touched.
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

const SOURCE_COMMIT: &str = "8cfea406b505e4d7df52d5a19bce525df98c54ab";
const SOURCE_URL: &str = "https://raw.githubusercontent.com/wooorm/dictionaries";

struct Entry {
    locale: &'static str,
    folder: &'static str,
    aff: &'static str,
    dic: &'static str,
    license: &'static str,
}

// Git blob IDs are checksums of the *exact bytes*, not mutable URLs.
// Licenses are preserved verbatim next to the dictionaries.
const CATALOG: &[Entry] = &[
    Entry {
        locale: "it_IT",
        folder: "it",
        aff: "3154d0846a6330b0c66c4e919ae20b44c842fc1c",
        dic: "0f224a23da400b3aeea4e9201be7361b7983e79a",
        license: "14383a439732fa2d748d422ca4f4bd7e958cbb19",
    },
    Entry {
        locale: "fr_FR",
        folder: "fr",
        aff: "5c24e468f8344f8d044a4107c887535c5a8a5e38",
        dic: "bd1e9b0fed954db8f07c7c9d20888afd55f35305",
        license: "1b156be880e815ce234885128b37f2203cc774fa",
    },
    Entry {
        locale: "en_GB",
        folder: "en-GB",
        aff: "9e7cb1e2f1c2cc3faff5a7ae2d5c041091017bdb",
        dic: "85edde560296b643498da7292eabd09740682dd8",
        license: "663781aee8593b882e79d01d28e7e54d2ec64947",
    },
    Entry {
        locale: "en_US",
        folder: "en",
        aff: "9e7cb1e2f1c2cc3faff5a7ae2d5c041091017bdb",
        dic: "b41fd203e6daf76934db99dec6572a230516368a",
        license: "6b5b49088c4ae6f871bdb1c741b15fd837868572",
    },
    Entry {
        locale: "de_DE",
        folder: "de",
        aff: "825f3fa5fcc57afcd253f76568d756ef44713ebe",
        dic: "5bb091ebbb23e18812c28f9e72a7ee0907fee6e3",
        license: "3702ed2564d457d437533173a38cdc844843026b",
    },
    Entry {
        locale: "es_ES",
        folder: "es",
        aff: "4bab8067f3bbcce50c9d5ea48243911681beb062",
        dic: "719bc8ca190a1866485ddf088d20733b64e3e4da",
        license: "5747bc9585ed346daaa6a049a9d2b26b3bb2ad0d",
    },
];

#[derive(Debug, Clone)]
pub enum DictionaryLocation {
    System,
    Managed(PathBuf),
}

pub struct DictionaryManager {
    available: HashMap<String, DictionaryLocation>,
    active: HashSet<String>,
    sender: Sender<(String, Result<DictionaryLocation, String>)>,
    receiver: Receiver<(String, Result<DictionaryLocation, String>)>,
}

impl Default for DictionaryManager {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            available: HashMap::new(),
            active: HashSet::new(),
            sender,
            receiver,
        }
    }
}

impl DictionaryManager {
    pub fn location(&self, locale: &str) -> Option<&DictionaryLocation> {
        self.available.get(locale)
    }

    /// Returns true only if the dictionary has already been verified in this
    /// session. Otherwise starts at most one background job per locale.
    pub fn ensure(&mut self, locale: &str) -> Result<bool, String> {
        if self.available.contains_key(locale) {
            return Ok(true);
        }
        let entry =
            catalog(locale).ok_or_else(|| format!("no dictionary available for {locale}"))?;
        if self.active.insert(locale.to_owned()) {
            let sender = self.sender.clone();
            let language = locale.to_owned();
            let result = thread::Builder::new()
                .name(format!("carta-dictionary-{locale}"))
                .spawn(move || {
                    let outcome = resolve_dictionary(entry);
                    let _ = sender.send((language, outcome));
                });
            if let Err(error) = result {
                self.active.remove(locale);
                return Err(format!("cannot start dictionary download: {error}"));
            }
        }
        Ok(false)
    }

    /// Called from the shared App maintenance tick; never waits for network I/O.
    pub fn poll(&mut self) -> Vec<(String, Result<(), String>)> {
        let mut completed = Vec::new();
        for (language, outcome) in self.receiver.try_iter() {
            self.active.remove(&language);
            let status = match outcome {
                Ok(location) => {
                    self.available.insert(language.clone(), location);
                    Ok(())
                }
                Err(error) => Err(error),
            };
            completed.push((language, status));
        }
        completed
    }
}

fn catalog(locale: &str) -> Option<&'static Entry> {
    CATALOG.iter().find(|entry| entry.locale == locale)
}

fn data_directory() -> Result<PathBuf, String> {
    if let Some(root) = env::var_os("XDG_DATA_HOME") {
        let root = PathBuf::from(root);
        if root.is_absolute() {
            return Ok(root.join("carta/dictionaries"));
        }
    }
    if cfg!(windows) {
        if let Some(root) = env::var_os("LOCALAPPDATA") {
            return Ok(PathBuf::from(root).join("Carta/dictionaries"));
        }
    }
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or("HOME / XDG_DATA_HOME unavailable for Carta dictionary cache")?;
    Ok(PathBuf::from(home).join(".local/share/carta/dictionaries"))
}

fn resolve_dictionary(entry: &Entry) -> Result<DictionaryLocation, String> {
    // The spell checker still uses Hunspell. Missing executables cannot be
    // fixed by downloading lexical files; do not waste network requests.
    let system = Command::new("hunspell")
        .args(["-a", "-d", entry.locale])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| {
            format!(
                "Hunspell executable unavailable ({error}); install or use a bundled Carta package"
            )
        })?;
    let dir = data_directory()?;
    let cache = dir.join(format!("{}-{}", entry.locale, SOURCE_COMMIT));
    if cached_dictionary_valid(&cache, entry)? {
        return Ok(DictionaryLocation::Managed(cache.join(entry.locale)));
    }
    if system.status.success() && system.stdout.starts_with(b"@(#)") {
        return Ok(DictionaryLocation::System);
    }
    install_dictionary(&dir, &cache, entry)?;
    Ok(DictionaryLocation::Managed(cache.join(entry.locale)))
}

fn cached_dictionary_valid(cache: &Path, entry: &Entry) -> Result<bool, String> {
    match fs::symlink_metadata(cache) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("dictionary cache metadata: {e}")),
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            return Err(format!("unsafe dictionary cache path: {}", cache.display()));
        }
        Ok(_) => {}
    }
    for (name, expected) in [
        (format!("{}.aff", entry.locale), entry.aff),
        (format!("{}.dic", entry.locale), entry.dic),
        ("LICENSE.txt".to_owned(), entry.license),
    ] {
        let path = cache.join(name);
        if !path.is_file()
            || fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
            || git_blob_id(&path)? != expected
        {
            return Err(format!("cached {locale} dictionary is incomplete or corrupted at {path}; move that cache folder aside and retry", locale=entry.locale, path=cache.display()));
        }
    }
    Ok(true)
}

fn install_dictionary(root: &Path, cache: &Path, entry: &Entry) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|error| format!("create dictionary cache: {error}"))?;
    let staging = tempfile::Builder::new()
        .prefix(".carta-download-")
        .tempdir_in(root)
        .map_err(|error| format!("create dictionary staging: {error}"))?;

    for (remote, local, expected) in [
        ("index.aff", format!("{}.aff", entry.locale), entry.aff),
        ("index.dic", format!("{}.dic", entry.locale), entry.dic),
        ("license", "LICENSE.txt".to_owned(), entry.license),
    ] {
        let destination = staging.path().join(local);
        let url = format!(
            "{SOURCE_URL}/{SOURCE_COMMIT}/dictionaries/{}/{remote}",
            entry.folder
        );
        let output = Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--connect-timeout",
                "8",
                "--max-time",
                "40",
                "--max-filesize",
                "8000000",
                "--output",
            ])
            .arg(&destination)
            .arg(&url)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("HTTPS downloader unavailable ({error}); Carta needs curl"))?;
        if !output.status.success() {
            return Err(format!(
                "download of {} ({}) failed: {}",
                entry.locale,
                remote,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let actual = git_blob_id(&destination)?;
        if actual != expected {
            return Err(format!(
                "dictionary {} failed integrity verification ({remote})",
                entry.locale
            ));
        }
        fs::File::open(&destination)
            .and_then(|file| file.sync_all())
            .map_err(|error| format!("sync dictionary file: {error}"))?;
    }
    fs::rename(staging.path(), cache)
        .map_err(|error| format!("publish dictionary cache: {error}"))?;
    Ok(())
}

fn git_blob_id(path: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .arg("hash-object")
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Git integrity verifier unavailable: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot verify downloaded dictionary: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_unique_safe_languages_and_pinned_blobs() {
        let mut seen = HashSet::new();
        for entry in CATALOG {
            assert!(seen.insert(entry.locale));
            assert_eq!(entry.locale.len(), 5);
            assert!(entry.locale.as_bytes()[..2]
                .iter()
                .all(u8::is_ascii_lowercase));
            assert_eq!(entry.locale.as_bytes()[2], b'_');
            assert!(entry.locale.as_bytes()[3..]
                .iter()
                .all(u8::is_ascii_uppercase));
            assert!(entry
                .folder
                .chars()
                .all(|c| c.is_ascii_alphabetic() || c == '-'));
            for sha in [entry.aff, entry.dic, entry.license] {
                assert_eq!(sha.len(), 40);
                assert!(sha.bytes().all(|c| c.is_ascii_hexdigit()));
            }
        }
        assert!(catalog("../../").is_none());
    }

    #[test]
    fn verifies_cache_bytes_and_rejects_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("fixture.dic");
        fs::write(&file, b"2\nbonjour\ncarta\n").unwrap();
        let sha = git_blob_id(&file).unwrap();
        assert_eq!(sha.len(), 40);
        fs::write(&file, b"2\nbonjour\ncarte\n").unwrap();
        assert_ne!(sha, git_blob_id(&file).unwrap());
    }

    #[test]
    fn never_downloads_on_manager_creation() {
        let manager = DictionaryManager::default();
        assert!(manager.available.is_empty());
        assert!(manager.active.is_empty());
    }
}
