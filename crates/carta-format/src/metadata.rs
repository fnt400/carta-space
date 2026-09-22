use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    ArchiveId, DocumentId, FormatError, Timestamp, WorkId, FORMAT_NAME, HISTORY_FORMAT,
    MARKDOWN_FORMAT,
};

pub type JsonExtensions = Map<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormatVersion {
    major: u64,
    minor: u64,
    #[serde(flatten)]
    extensions: JsonExtensions,
}

impl FormatVersion {
    pub fn draft_0_1() -> Self {
        Self {
            major: 0,
            minor: 1,
            extensions: JsonExtensions::new(),
        }
    }

    pub fn major(&self) -> u64 {
        self.major
    }

    pub fn minor(&self) -> u64 {
        self.minor
    }

    pub fn extensions(&self) -> &JsonExtensions {
        &self.extensions
    }

    fn validate(&self) -> Result<(), FormatError> {
        if (self.major, self.minor) != (0, 1) {
            return Err(FormatError::UnsupportedVersion {
                major: self.major,
                minor: self.minor,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchiveMetadata {
    format: String,
    format_version: FormatVersion,
    archive_id: ArchiveId,
    created: Timestamp,
    markdown: String,
    history: String,
    #[serde(flatten)]
    extensions: JsonExtensions,
}

impl ArchiveMetadata {
    pub fn new(archive_id: ArchiveId, created: Timestamp) -> Self {
        Self {
            format: FORMAT_NAME.to_owned(),
            format_version: FormatVersion::draft_0_1(),
            archive_id,
            created,
            markdown: MARKDOWN_FORMAT.to_owned(),
            history: HISTORY_FORMAT.to_owned(),
            extensions: JsonExtensions::new(),
        }
    }

    pub fn read_from(reader: impl Read) -> Result<Self, FormatError> {
        let metadata: Self = serde_json::from_reader(reader)?;
        metadata.validate()?;
        Ok(metadata)
    }

    pub fn write_to(&self, mut writer: impl Write) -> Result<(), FormatError> {
        self.validate()?;
        serde_json::to_writer_pretty(&mut writer, self)?;
        writer.write_all(b"\n")?;
        Ok(())
    }

    pub fn archive_id(&self) -> ArchiveId {
        self.archive_id
    }

    pub fn created(&self) -> Timestamp {
        self.created
    }

    pub fn format_version(&self) -> &FormatVersion {
        &self.format_version
    }

    pub fn extensions(&self) -> &JsonExtensions {
        &self.extensions
    }

    fn validate(&self) -> Result<(), FormatError> {
        if self.format != FORMAT_NAME {
            return Err(FormatError::InvalidFormatName {
                actual: self.format.clone(),
                expected: FORMAT_NAME,
            });
        }
        self.format_version.validate()?;
        if self.markdown != MARKDOWN_FORMAT {
            return Err(FormatError::InvalidMarkdownFormat {
                actual: self.markdown.clone(),
                expected: MARKDOWN_FORMAT,
            });
        }
        if self.history != HISTORY_FORMAT {
            return Err(FormatError::InvalidHistoryFormat {
                actual: self.history.clone(),
                expected: HISTORY_FORMAT,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentMetadata {
    id: DocumentId,
    created: Timestamp,
    #[serde(flatten)]
    extensions: JsonExtensions,
}

impl DocumentMetadata {
    pub fn new(id: DocumentId, created: Timestamp) -> Self {
        Self {
            id,
            created,
            extensions: JsonExtensions::new(),
        }
    }

    pub fn read_from(reader: impl Read) -> Result<Self, FormatError> {
        Ok(serde_json::from_reader(reader)?)
    }

    pub fn write_to(&self, mut writer: impl Write) -> Result<(), FormatError> {
        serde_json::to_writer_pretty(&mut writer, self)?;
        writer.write_all(b"\n")?;
        Ok(())
    }

    pub fn id(&self) -> DocumentId {
        self.id
    }

    pub fn created(&self) -> Timestamp {
        self.created
    }

    pub fn extensions(&self) -> &JsonExtensions {
        &self.extensions
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkMetadata {
    id: WorkId,
    created: Timestamp,
    title: String,
    documents: Vec<DocumentId>,
    #[serde(flatten)]
    extensions: JsonExtensions,
}

impl WorkMetadata {
    pub fn new(id: WorkId, created: Timestamp, title: String, documents: Vec<DocumentId>) -> Self {
        Self {
            id,
            created,
            title,
            documents,
            extensions: JsonExtensions::new(),
        }
    }

    pub fn read_from(reader: impl Read) -> Result<Self, FormatError> {
        Ok(serde_json::from_reader(reader)?)
    }

    pub fn write_to(&self, mut writer: impl Write) -> Result<(), FormatError> {
        serde_json::to_writer_pretty(&mut writer, self)?;
        writer.write_all(b"\n")?;
        Ok(())
    }

    pub fn id(&self) -> WorkId {
        self.id
    }

    pub fn created(&self) -> Timestamp {
        self.created
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn documents(&self) -> &[DocumentId] {
        &self.documents
    }

    pub fn extensions(&self) -> &JsonExtensions {
        &self.extensions
    }
}
