use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::{Archive, Error};

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

        let temporary = temporary_sibling(&destination, "package")?;
        let mut guard = TemporaryFile::new(temporary.clone());
        let entries = write_package(&self.root, &temporary)?;
        validate_package(&temporary)?;
        replace_destination(&temporary, &destination)?;
        guard.disarm();
        sync_parent(&destination)?;
        Ok(PackageReport {
            destination,
            entries,
            checkpoint_created,
        })
    }
}

fn write_package(root: &Path, destination: &Path) -> Result<usize, Error> {
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
        count += append_path(&mut zip, root, &path, destination)?;
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
            count += append_path(zip, root, &child, package)?;
        }
        Ok(count)
    } else if metadata.is_file() {
        zip.start_file(name, options)
            .map_err(|error| Error::zip(package, error))?;
        let mut input = File::open(path).map_err(|error| Error::io(path, error))?;
        std::io::copy(&mut input, zip).map_err(|error| Error::io(path, error))?;
        Ok(1)
    } else {
        Err(Error::UnsafeDestination(path.to_path_buf()))
    }
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
        .to_string_lossy();
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
            } else {
                name.starts_with(".carta-")
            }
        })
    })
}

pub(crate) struct TemporaryFile {
    path: PathBuf,
    armed: bool,
}

impl TemporaryFile {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}
