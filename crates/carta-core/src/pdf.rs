use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use carta_format::{DocumentId, WorkId};

use crate::package::{
    replace_destination, safe_export_destination, sync_parent, temporary_sibling, TemporaryFile,
};
use crate::{Archive, Error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfExportOptions {
    pandoc: PathBuf,
    pdf_engine: String,
}

impl PdfExportOptions {
    pub fn new(pandoc: impl Into<PathBuf>, pdf_engine: impl Into<String>) -> Self {
        Self {
            pandoc: pandoc.into(),
            pdf_engine: pdf_engine.into(),
        }
    }

    pub fn pandoc(&self) -> &Path {
        &self.pandoc
    }

    pub fn pdf_engine(&self) -> &str {
        &self.pdf_engine
    }
}

impl Default for PdfExportOptions {
    fn default() -> Self {
        Self::new("pandoc", "lualatex")
    }
}

impl Archive {
    pub fn export_document_pdf(
        &self,
        id: DocumentId,
        destination: impl AsRef<Path>,
    ) -> Result<(), Error> {
        self.export_document_pdf_with(id, destination, &PdfExportOptions::default())
    }

    pub fn export_document_pdf_with(
        &self,
        id: DocumentId,
        destination: impl AsRef<Path>,
        options: &PdfExportOptions,
    ) -> Result<(), Error> {
        self.export_pdf(
            &self.export_document_markdown(id)?,
            destination.as_ref(),
            options,
        )
    }

    pub fn export_work_pdf(&self, id: WorkId, destination: impl AsRef<Path>) -> Result<(), Error> {
        self.export_work_pdf_with(id, destination, &PdfExportOptions::default())
    }

    pub fn export_work_pdf_with(
        &self,
        id: WorkId,
        destination: impl AsRef<Path>,
        options: &PdfExportOptions,
    ) -> Result<(), Error> {
        self.export_pdf(
            &self.export_work_markdown(id)?,
            destination.as_ref(),
            options,
        )
    }

    fn export_pdf(
        &self,
        markdown: &str,
        destination: &Path,
        options: &PdfExportOptions,
    ) -> Result<(), Error> {
        let destination = safe_export_destination(&self.root, destination)?;
        let temporary = temporary_sibling(&destination, "pdf")?;
        let mut guard = TemporaryFile::new(temporary.clone());
        let mut child = Command::new(&options.pandoc)
            .args([
                "--from=commonmark",
                "--to=pdf",
                &format!("--pdf-engine={}", options.pdf_engine),
                "--output",
            ])
            .arg(&temporary)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| Error::PandocUnavailable {
                program: options.pandoc.clone(),
                source,
            })?;
        let input_result = child
            .stdin
            .take()
            .expect("Pandoc stdin is piped")
            .write_all(markdown.as_bytes());
        let output = child
            .wait_with_output()
            .map_err(|source| Error::PandocUnavailable {
                program: options.pandoc.clone(),
                source,
            })?;
        if !output.status.success() {
            return Err(Error::PdfExportFailed {
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        input_result.map_err(|error| Error::io(&temporary, error))?;
        if !fs::metadata(&temporary).is_ok_and(|metadata| metadata.is_file()) {
            return Err(Error::PdfOutputMissing);
        }
        File::options()
            .write(true)
            .open(&temporary)
            .and_then(|file| file.sync_all())
            .map_err(|error| Error::io(&temporary, error))?;
        replace_destination(&temporary, &destination)?;
        guard.disarm();
        sync_parent(&destination)
    }
}
