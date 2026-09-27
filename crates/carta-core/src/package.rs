use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::{Archive, Error};

const TEMPORARY_REGISTRY: &str = "carta-external-temporaries.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReport {
    destination: PathBuf,
    entries: usize,
    checkpoint_created: bool,
}

impl PackageReport {
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    pub fn entries(&self) -> usize {
        self.entries
    }

    pub fn checkpoint_created(&self) -> bool {
        self.checkpoint_created
    }
}

impl Archive {
    pub fn package(&self, destination: impl AsRef<Path>) -> Result<PackageReport, Error> {
        Archive::validate(&self.root)?;
        let destination = safe_export_destination(&self.root, destination.as_ref())?;
        let checkpoint_created = crate::history::create_package_checkpoint(&self.root)?.is_some();
        let strip_sync_remote = self.sync_remote()?.is_some();

        let temporary = temporary_sibling(&destination, "package")?;
        let mut guard = TemporaryFile::register(&self.root, temporary.clone())?;
        let entries = write_package(&self.root, &temporary, strip_sync_remote)?;
        validate_package(&temporary)?;
        replace_destination(&temporary, &destination)?;
        guard.finish()?;
        sync_parent(&destination)?;
        Ok(PackageReport {
            destination,
            entries,
            checkpoint_created,
        })
    }
}

fn write_package(
    root: &Path,
    destination: &Path,
    strip_sync_remote: bool,
) -> Result<usize, Error> {
    let file = File::options()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| Error::io(destination, error))?;
    let mut zip = ZipWriter::new(file);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("mimetype", stored)
        .map_err(|error| Error::zip(destination, error))?;
    zip.write_all(carta_format::MIMETYPE)
        .map_err(|error| Error::io(destination, error))?;

    let mut entries = vec![
        root.join("carta.json"),
        root.join("volumes"),
        root.join("works"),
        root.join(".git"),
    ];
    for entry in fs::read_dir(root).map_err(|error| Error::io(root, error))? {
        let path = entry.map_err(|error| Error::io(root, error))?.path();
        let name = path.file_name().and_then(|name| name.to_str());
        if matches!(
            name,
            Some("mimetype" | "carta.json" | "volumes" | "works" | ".git")
        ) || is_transient(&path, root)
        {
            continue;
        }
        entries.push(path);
    }
    entries.sort_by_key(|path| relative_name(root, path));

    let mut count = 1;
    for path in entries {
        count += append_path(
            &mut zip,
            root,
            &path,
            destination,
            strip_sync_remote,
        )?;
    }
    let file = zip
        .finish()
        .map_err(|error| Error::zip(destination, error))?;
    file.sync_all()
        .map_err(|error| Error::io(destination, error))?;
    Ok(count)
}

fn append_path(
    zip: &mut ZipWriter<File>,
    root: &Path,
    path: &Path,
    package: &Path,
    strip_sync_remote: bool,
) -> Result<usize, Error> {
    if is_transient(path, root) {
        return Ok(0);
    }
    let metadata = fs::symlink_metadata(path).map_err(|error| Error::io(path, error))?;
    if metadata.file_type().is_symlink() {
        return Err(Error::UnsupportedPackageSymlink(path.to_path_buf()));
    }
    let name =
        relative_name(root, path).ok_or_else(|| Error::NonUtf8PackagePath(path.to_path_buf()))?;
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    if metadata.is_dir() {
        zip.add_directory(format!("{name}/"), options)
            .map_err(|error| Error::zip(package, error))?;
        let mut children = fs::read_dir(path)
            .map_err(|error| Error::io(path, error))?
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|error| Error::io(path, error))
            })
            .collect::<Result<Vec<_>, _>>()?;
        children.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
        let mut count = 1;
        for child in children {
            count += append_path(zip, root, &child, package, strip_sync_remote)?;
        }
        Ok(count)
    } else if metadata.is_file() {
        let is_git_config = name == ".git/config";
        zip.start_file(name, options)
            .map_err(|error| Error::zip(package, error))?;
        if strip_sync_remote && is_git_config {
            let config = packaged_git_config(root, path)?;
            zip.write_all(&config)
                .map_err(|error| Error::io(package, error))?;
        } else {
            let mut input = File::open(path).map_err(|error| Error::io(path, error))?;
            std::io::copy(&mut input, zip).map_err(|error| Error::io(path, error))?;
        }
        Ok(1)
    } else {
        Err(Error::UnsafeDestination(path.to_path_buf()))
    }
}

fn packaged_git_config(root: &Path, source: &Path) -> Result<Vec<u8>, Error> {
    let bytes = fs::read(source).map_err(|error| Error::io(source, error))?;
    let mut temporary =
        tempfile::NamedTempFile::new().map_err(|error| Error::io(source, error))?;
    temporary
        .write_all(&bytes)
        .map_err(|error| Error::io(temporary.path(), error))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| Error::io(temporary.path(), error))?;
    let temporary_path = temporary
        .path()
        .to_str()
        .ok_or_else(|| Error::NonUtf8TemporaryPath(temporary.path().to_path_buf()))?;
    crate::history::git_output(
        root,
        "remove device-local sync remote from portable package",
        &[
            "config",
            "--file",
            temporary_path,
            "--remove-section",
            "remote.carta-sync",
        ],
    )?;
    fs::read(temporary.path()).map_err(|error| Error::io(temporary.path(), error))
}

fn validate_package(path: &Path) -> Result<(), Error> {
    let file = File::open(path).map_err(|error| Error::io(path, error))?;
    let mut zip = ZipArchive::new(file).map_err(|error| Error::zip(path, error))?;
    if zip.is_empty() {
        return Err(Error::InvalidPackage("ZIP is empty".to_owned()));
    }
    {
        let mut mimetype = zip.by_index(0).map_err(|error| Error::zip(path, error))?;
        if mimetype.name() != "mimetype" || mimetype.compression() != CompressionMethod::Stored {
            return Err(Error::InvalidPackage(
                "mimetype is not the first uncompressed member".to_owned(),
            ));
        }
        let mut bytes = Vec::new();
        mimetype
            .read_to_end(&mut bytes)
            .map_err(|error| Error::io(path, error))?;
        if bytes != carta_format::MIMETYPE {
            return Err(Error::InvalidPackage(
                "mimetype content is not exact".to_owned(),
            ));
        }
    }

    let unpacked = tempfile::tempdir().map_err(|error| Error::io(path, error))?;
    zip.extract(unpacked.path())
        .map_err(|error| Error::zip(path, error))?;
    Archive::validate(unpacked.path()).map_err(Error::from)?;
    let unpacked_archive = Archive::open(unpacked.path())?;
    if unpacked_archive.is_dirty()? {
        return Err(Error::InvalidPackage(
            "packaged current state differs from its checkpoint".to_owned(),
        ));
    }
    unpacked_archive.history()?;
    Ok(())
}

pub(crate) fn safe_export_destination(root: &Path, destination: &Path) -> Result<PathBuf, Error> {
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|_| Error::InvalidDestinationParent(parent.to_path_buf()))?;
    if !parent.is_dir() {
        return Err(Error::InvalidDestinationParent(parent));
    }
    let name = destination
        .file_name()
        .ok_or_else(|| Error::UnsafeDestination(destination.to_path_buf()))?;
    let destination = parent.join(name);
    let root = root
        .canonicalize()
        .map_err(|error| Error::io(root, error))?;
    if destination.starts_with(&root) {
        return Err(Error::DestinationInsideArchive(destination));
    }
    match fs::symlink_metadata(&destination) {
        Ok(metadata) if !metadata.is_file() => Err(Error::UnsafeDestination(destination)),
        Ok(_) => Ok(destination),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(destination),
        Err(error) => Err(Error::io(&destination, error)),
    }
}

pub(crate) fn temporary_sibling(destination: &Path, kind: &str) -> Result<PathBuf, Error> {
    let parent = destination
        .parent()
        .expect("safe destinations have a parent");
    let name = destination
        .file_name()
        .expect("safe destinations have a name")
        .to_str()
        .ok_or_else(|| Error::NonUtf8TemporaryPath(destination.to_path_buf()))?;
    Ok(parent.join(format!(".carta-{kind}-{name}-{}.tmp", uuid::Uuid::now_v7())))
}

pub(crate) fn replace_destination(temporary: &Path, destination: &Path) -> Result<(), Error> {
    fs::rename(temporary, destination).map_err(|error| Error::io(destination, error))
}

pub(crate) fn sync_parent(destination: &Path) -> Result<(), Error> {
    let parent = destination
        .parent()
        .expect("safe destinations have a parent");
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| Error::io(parent, error))
}

fn relative_name(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()?
        .to_str()
        .map(|name| name.replace('\\', "/"))
}

fn is_transient(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut components = relative.components();
    let in_git = matches!(components.next(), Some(Component::Normal(name)) if name == ".git");
    relative.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        name.to_str().is_some_and(|name| {
            if in_git {
                name.starts_with("carta-transaction-")
                    || name.starts_with("carta-wipe-index-")
                    || name == "carta-conflicts"
                    || name == TEMPORARY_REGISTRY
            } else {
                name.starts_with(".carta-")
            }
        })
    })
}

pub(crate) struct TemporaryFile {
    root: PathBuf,
    path: PathBuf,
    armed: bool,
}

impl TemporaryFile {
    pub(crate) fn register(root: &Path, path: PathBuf) -> Result<Self, Error> {
        register_temporary(root, &path)?;
        Ok(Self {
            root: root.to_path_buf(),
            path,
            armed: true,
        })
    }

    pub(crate) fn finish(&mut self) -> Result<(), Error> {
        unregister_temporary(&self.root, &self.path)?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
            let _ = unregister_temporary(&self.root, &self.path);
        }
    }
}

pub(crate) fn cleanup_registered_temporaries(root: &Path) -> Result<usize, Error> {
    let paths = read_registry(root)?;
    let mut removed = 0;
    for path in &paths {
        validate_registered_path(root, path)?;
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => {
                fs::remove_file(path).map_err(|error| Error::io(path, error))?;
                removed += 1;
            }
            Ok(_) => return Err(Error::UnsafeDestination(path.clone())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io(path, error)),
        }
    }
    if !paths.is_empty() || registry_path(root).exists() {
        write_registry(root, &[])?;
        let path = registry_path(root);
        match fs::remove_file(&path) {
            Ok(()) => sync_git_directory(root)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io(path, error)),
        }
    }
    Ok(removed)
}

fn register_temporary(root: &Path, path: &Path) -> Result<(), Error> {
    validate_registered_path(root, path)?;
    let mut paths = read_registry(root)?;
    if !paths.iter().any(|candidate| candidate == path) {
        paths.push(path.to_path_buf());
        paths.sort();
        write_registry(root, &paths)?;
    }
    Ok(())
}

fn unregister_temporary(root: &Path, path: &Path) -> Result<(), Error> {
    let mut paths = read_registry(root)?;
    paths.retain(|candidate| candidate != path);
    if paths.is_empty() {
        let registry = registry_path(root);
        match fs::remove_file(&registry) {
            Ok(()) => sync_git_directory(root),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Error::io(registry, error)),
        }
    } else {
        write_registry(root, &paths)
    }
}

fn read_registry(root: &Path) -> Result<Vec<PathBuf>, Error> {
    let path = registry_path(root);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(Error::io(path, error)),
    };
    let strings: Vec<String> = serde_json::from_slice(&bytes)
        .map_err(|error| Error::MalformedTemporaryRegistry(error.to_string()))?;
    Ok(strings.into_iter().map(PathBuf::from).collect())
}

fn write_registry(root: &Path, paths: &[PathBuf]) -> Result<(), Error> {
    let strings = paths
        .iter()
        .map(|path| {
            path.to_str()
                .map(str::to_owned)
                .ok_or_else(|| Error::NonUtf8TemporaryPath(path.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let bytes = serde_json::to_vec(&strings)
        .map_err(|error| Error::MalformedTemporaryRegistry(error.to_string()))?;
    let git = root.join(".git");
    let temporary = git.join(format!(
        ".{TEMPORARY_REGISTRY}-{}.tmp",
        uuid::Uuid::now_v7()
    ));
    let destination = registry_path(root);
    let result = (|| {
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| Error::io(&temporary, error))?;
        file.write_all(&bytes)
            .map_err(|error| Error::io(&temporary, error))?;
        file.sync_all()
            .map_err(|error| Error::io(&temporary, error))?;
        fs::rename(&temporary, &destination).map_err(|error| Error::io(&destination, error))?;
        sync_git_directory(root)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn validate_registered_path(root: &Path, path: &Path) -> Result<(), Error> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::NonUtf8TemporaryPath(path.to_path_buf()))?;
    let root = root
        .canonicalize()
        .map_err(|error| Error::io(root, error))?;
    if !path.is_absolute()
        || path.starts_with(root)
        || !name.starts_with(".carta-")
        || !name.ends_with(".tmp")
    {
        return Err(Error::UnsafeDestination(path.to_path_buf()));
    }
    Ok(())
}

fn registry_path(root: &Path) -> PathBuf {
    root.join(".git").join(TEMPORARY_REGISTRY)
}

fn sync_git_directory(root: &Path) -> Result<(), Error> {
    let git = root.join(".git");
    File::open(&git)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| Error::io(git, error))
}
