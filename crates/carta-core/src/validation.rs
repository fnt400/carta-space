use std::collections::BTreeMap;
use std::fs::{self, DirEntry, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use carta_format::{ArchiveMetadata, DocumentId, DocumentMetadata, WorkId, WorkMetadata, MIMETYPE};

use crate::{DocumentInfo, ValidationErrors, ValidationIssue, ValidationIssueKind, Volume, Work};

pub(crate) struct ScannedArchive {
    pub metadata: ArchiveMetadata,
    pub documents: BTreeMap<DocumentId, DocumentInfo>,
    pub works: BTreeMap<WorkId, Work>,
}

pub(crate) fn scan_archive(root: &Path) -> Result<ScannedArchive, ValidationErrors> {
    let mut scanner = Scanner::default();

    if !scanner.require_directory(root) {
        return Err(ValidationErrors::new(scanner.issues));
    }

    scanner.validate_mimetype(&root.join("mimetype"));
    let metadata = scanner.read_archive_metadata(&root.join("carta.json"));

    let volumes = root.join("volumes");
    let works_path = root.join("works");
    let git = root.join(".git");
    let volumes_exists = scanner.require_directory(&volumes);
    let works_exists = scanner.require_directory(&works_path);
    if scanner.require_directory(&git) {
        scanner.validate_git_working_tree(root, &git);
    }

    let documents = if volumes_exists {
        scanner.scan_volumes(&volumes)
    } else {
        BTreeMap::new()
    };
    let works = if works_exists {
        scanner.scan_works(&works_path, &documents)
    } else {
        BTreeMap::new()
    };

    if scanner.issues.is_empty() {
        Ok(ScannedArchive {
            metadata: metadata.expect("metadata exists when validation has no issues"),
            documents,
            works,
        })
    } else {
        Err(ValidationErrors::new(scanner.issues))
    }
}

#[derive(Default)]
struct Scanner {
    issues: Vec<ValidationIssue>,
}

impl Scanner {
    fn issue(&mut self, path: impl Into<PathBuf>, kind: ValidationIssueKind) {
        self.issues.push(ValidationIssue::new(path, kind));
    }

    fn require_directory(&mut self, path: &Path) -> bool {
        self.require_type(path, "directory", |metadata| metadata.is_dir())
    }

    fn require_file(&mut self, path: &Path) -> bool {
        self.require_type(path, "regular file", |metadata| metadata.is_file())
    }

    fn require_type(
        &mut self,
        path: &Path,
        expected: &'static str,
        predicate: impl FnOnce(&fs::FileType) -> bool,
    ) -> bool {
        match fs::symlink_metadata(path) {
            Ok(metadata) if predicate(&metadata.file_type()) => true,
            Ok(metadata) => {
                self.issue(
                    path,
                    ValidationIssueKind::WrongEntryType {
                        expected,
                        actual: file_type_name(&metadata.file_type()),
                    },
                );
                false
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.issue(path, ValidationIssueKind::MissingEntry);
                false
            }
            Err(error) => {
                self.issue(path, ValidationIssueKind::Io(error.to_string()));
                false
            }
        }
    }

    fn validate_mimetype(&mut self, path: &Path) {
        if !self.require_file(path) {
            return;
        }
        match fs::read(path) {
            Ok(content) if content == MIMETYPE => {}
            Ok(_) => self.issue(path, ValidationIssueKind::InvalidMimetype),
            Err(error) => self.issue(path, ValidationIssueKind::Io(error.to_string())),
        }
    }

    fn validate_git_working_tree(&mut self, root: &Path, git_directory: &Path) {
        let output = Command::new("git")
            .current_dir(root)
            .args(["--git-dir=.git", "--work-tree=."])
            .args(["rev-parse", "--is-inside-work-tree"])
            .output();

        match output {
            Ok(output)
                if output.status.success()
                    && String::from_utf8_lossy(&output.stdout).trim() == "true" => {}
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let detail = stderr.trim();
                let detail = if detail.is_empty() {
                    format!("git rev-parse exited with {}", output.status)
                } else {
                    detail.to_owned()
                };
                self.issue(
                    git_directory,
                    ValidationIssueKind::InvalidGitWorkingTree(detail),
                );
            }
            Err(error) => self.issue(
                git_directory,
                ValidationIssueKind::InvalidGitWorkingTree(format!("could not run Git: {error}")),
            ),
        }
    }

    fn read_archive_metadata(&mut self, path: &Path) -> Option<ArchiveMetadata> {
        if !self.require_file(path) {
            return None;
        }
        match File::open(path)
            .map(BufReader::new)
            .map_err(carta_format::FormatError::from)
            .and_then(ArchiveMetadata::read_from)
        {
            Ok(metadata) => Some(metadata),
            Err(error) => {
                self.issue(path, ValidationIssueKind::InvalidFormat(error.to_string()));
                None
            }
        }
    }

    fn scan_volumes(&mut self, volumes: &Path) -> BTreeMap<DocumentId, DocumentInfo> {
        let mut documents = BTreeMap::new();
        for year_entry in self.read_entries(volumes) {
            let year_path = year_entry.path();
            let Some(year_name) = entry_name(&year_entry) else {
                self.issue(
                    &year_path,
                    ValidationIssueKind::MalformedReservedEntry(
                        "year name is not UTF-8".to_owned(),
                    ),
                );
                continue;
            };
            let Some(year) = parse_year(&year_name) else {
                self.issue(
                    &year_path,
                    ValidationIssueKind::MalformedReservedEntry(
                        "year must contain exactly four ASCII digits".to_owned(),
                    ),
                );
                continue;
            };
            if !entry_is_directory(&year_entry) {
                self.issue(
                    &year_path,
                    ValidationIssueKind::MalformedReservedEntry(
                        "year entry must be a directory, not a file or symlink".to_owned(),
                    ),
                );
                continue;
            }

            for month_entry in self.read_entries(&year_path) {
                let month_path = month_entry.path();
                let Some(month_name) = entry_name(&month_entry) else {
                    self.issue(
                        &month_path,
                        ValidationIssueKind::MalformedReservedEntry(
                            "month name is not UTF-8".to_owned(),
                        ),
                    );
                    continue;
                };
                let Some(month) = parse_month(&month_name) else {
                    self.issue(
                        &month_path,
                        ValidationIssueKind::MalformedReservedEntry(
                            "month must be two ASCII digits from 01 through 12".to_owned(),
                        ),
                    );
                    continue;
                };
                if !entry_is_directory(&month_entry) {
                    self.issue(
                        &month_path,
                        ValidationIssueKind::MalformedReservedEntry(
                            "month entry must be a directory, not a file or symlink".to_owned(),
                        ),
                    );
                    continue;
                }

                self.scan_month(&month_path, Volume::from_parts(year, month), &mut documents);
            }
        }
        documents
    }

    fn scan_month(
        &mut self,
        month_path: &Path,
        volume: Volume,
        documents: &mut BTreeMap<DocumentId, DocumentInfo>,
    ) {
        for entry in self.read_entries(month_path) {
            let path = entry.path();
            let Some(name) = entry_name(&entry) else {
                self.issue(
                    &path,
                    ValidationIssueKind::MalformedReservedEntry(
                        "document directory name is not UTF-8".to_owned(),
                    ),
                );
                continue;
            };
            let directory_id = match DocumentId::from_str(&name) {
                Ok(id) => id,
                Err(error) => {
                    self.issue(
                        &path,
                        ValidationIssueKind::MalformedReservedEntry(error.to_string()),
                    );
                    continue;
                }
            };
            if !entry_is_directory(&entry) {
                self.issue(
                    &path,
                    ValidationIssueKind::MalformedReservedEntry(
                        "document entry must be a directory, not a file or symlink".to_owned(),
                    ),
                );
                continue;
            }

            let metadata_path = path.join("meta.json");
            let content_path = path.join("content.md");
            let metadata = self.read_document_metadata(&metadata_path);
            self.validate_content(&content_path);

            let Some(metadata) = metadata else {
                continue;
            };
            if directory_id != metadata.id() {
                self.issue(
                    &metadata_path,
                    ValidationIssueKind::DirectoryIdMismatch {
                        directory_id: name,
                        metadata_id: metadata.id().to_string(),
                    },
                );
            }
            if metadata.created().year() != i32::from(volume.year())
                || metadata.created().month() != u32::from(volume.month())
            {
                self.issue(
                    &metadata_path,
                    ValidationIssueKind::VolumeMismatch {
                        expected_year: metadata.created().year(),
                        expected_month: metadata.created().month(),
                        actual_year: volume.year(),
                        actual_month: volume.month(),
                    },
                );
            }

            let info = DocumentInfo {
                metadata,
                volume,
                path: path.clone(),
            };
            if documents.insert(directory_id, info).is_some() {
                self.issue(path, ValidationIssueKind::DuplicateDocument(directory_id));
            }
        }
    }

    fn read_document_metadata(&mut self, path: &Path) -> Option<DocumentMetadata> {
        if !self.require_file(path) {
            return None;
        }
        match File::open(path)
            .map(BufReader::new)
            .map_err(carta_format::FormatError::from)
            .and_then(DocumentMetadata::read_from)
        {
            Ok(metadata) => Some(metadata),
            Err(error) => {
                self.issue(path, ValidationIssueKind::InvalidFormat(error.to_string()));
                None
            }
        }
    }

    fn validate_content(&mut self, path: &Path) {
        if !self.require_file(path) {
            return;
        }
        match fs::read_to_string(path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                self.issue(path, ValidationIssueKind::InvalidUtf8);
            }
            Err(error) => self.issue(path, ValidationIssueKind::Io(error.to_string())),
        }
    }

    fn scan_works(
        &mut self,
        works_path: &Path,
        documents: &BTreeMap<DocumentId, DocumentInfo>,
    ) -> BTreeMap<WorkId, Work> {
        let mut works = BTreeMap::new();
        for entry in self.read_entries(works_path) {
            let path = entry.path();
            let Some(name) = entry_name(&entry) else {
                self.issue(
                    &path,
                    ValidationIssueKind::MalformedReservedEntry(
                        "work directory name is not UTF-8".to_owned(),
                    ),
                );
                continue;
            };
            let directory_id = match WorkId::from_str(&name) {
                Ok(id) => id,
                Err(error) => {
                    self.issue(
                        &path,
                        ValidationIssueKind::MalformedReservedEntry(error.to_string()),
                    );
                    continue;
                }
            };
            if !entry_is_directory(&entry) {
                self.issue(
                    &path,
                    ValidationIssueKind::MalformedReservedEntry(
                        "work entry must be a directory, not a file or symlink".to_owned(),
                    ),
                );
                continue;
            }

            let metadata_path = path.join("work.json");
            let Some(metadata) = self.read_work_metadata(&metadata_path) else {
                continue;
            };
            if directory_id != metadata.id() {
                self.issue(
                    &metadata_path,
                    ValidationIssueKind::DirectoryIdMismatch {
                        directory_id: name,
                        metadata_id: metadata.id().to_string(),
                    },
                );
            }
            for document in metadata.documents() {
                if !documents.contains_key(document) {
                    self.issue(
                        &metadata_path,
                        ValidationIssueKind::DanglingDocumentReference {
                            work: metadata.id(),
                            document: *document,
                        },
                    );
                }
            }

            let work = Work {
                metadata,
                path: path.clone(),
            };
            if works.insert(directory_id, work).is_some() {
                self.issue(path, ValidationIssueKind::DuplicateWork(directory_id));
            }
        }
        works
    }

    fn read_work_metadata(&mut self, path: &Path) -> Option<WorkMetadata> {
        if !self.require_file(path) {
            return None;
        }
        match File::open(path)
            .map(BufReader::new)
            .map_err(carta_format::FormatError::from)
            .and_then(WorkMetadata::read_from)
        {
            Ok(metadata) => Some(metadata),
            Err(error) => {
                self.issue(path, ValidationIssueKind::InvalidFormat(error.to_string()));
                None
            }
        }
    }

    fn read_entries(&mut self, path: &Path) -> Vec<DirEntry> {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) => {
                self.issue(path, ValidationIssueKind::Io(error.to_string()));
                return Vec::new();
            }
        };

        let mut result = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => result.push(entry),
                Err(error) => self.issue(path, ValidationIssueKind::Io(error.to_string())),
            }
        }
        result.sort_by_key(DirEntry::file_name);
        result
    }
}

fn entry_name(entry: &DirEntry) -> Option<String> {
    entry.file_name().into_string().ok()
}

fn entry_is_directory(entry: &DirEntry) -> bool {
    entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
}

fn parse_year(value: &str) -> Option<u16> {
    if value.len() != 4 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn parse_month(value: &str) -> Option<u8> {
    if value.len() != 2 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let month = value.parse().ok()?;
    (1..=12).contains(&month).then_some(month)
}

fn file_type_name(file_type: &fs::FileType) -> &'static str {
    if file_type.is_dir() {
        "directory"
    } else if file_type.is_file() {
        "regular file"
    } else if file_type.is_symlink() {
        "symlink"
    } else {
        "special file"
    }
}
