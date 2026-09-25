use std::error::Error as StdError;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use carta_core::{Archive, CheckpointKind, DocumentId, PdfExportOptions, ValidationErrors, WorkId};
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "carta",
    version,
    about = "Scriptable Carta Space archive tools"
)]
struct Cli {
    /// Archive working directory used by commands other than create.
    #[arg(short, long, global = true, default_value = ".")]
    archive: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Create {
        path: PathBuf,
    },
    Validate,
    Inspect,
    Documents,
    Works,
    Checkpoint {
        #[arg(long, value_enum, default_value_t = CheckpointType::Manual)]
        kind: CheckpointType,
        #[arg(long)]
        note: Option<String>,
    },
    History,
    Import {
        source: PathBuf,
    },
    Search {
        query: String,
    },
    Export(ExportArgs),
    Package {
        destination: PathBuf,
    },
    Trash(TrashArgs),
    Restore(RestoreArgs),
    Wipe(WipeArgs),
}

#[derive(Debug, Args)]
struct ExportArgs {
    #[command(subcommand)]
    command: ExportCommand,
}

#[derive(Debug, Subcommand)]
enum ExportCommand {
    MarkdownDocument { document: DocumentId },
    MarkdownWork { work: WorkId },
    PdfDocument(PdfDocumentArgs),
    PdfWork(PdfWorkArgs),
}

#[derive(Debug, Args)]
struct PdfDocumentArgs {
    document: DocumentId,
    destination: PathBuf,
    #[command(flatten)]
    renderer: RendererArgs,
}

#[derive(Debug, Args)]
struct PdfWorkArgs {
    work: WorkId,
    destination: PathBuf,
    #[command(flatten)]
    renderer: RendererArgs,
}

#[derive(Debug, Args)]
struct RendererArgs {
    #[arg(long, default_value = "pandoc")]
    pandoc: PathBuf,
    #[arg(long, default_value = "lualatex")]
    pdf_engine: String,
}

#[derive(Debug, Args)]
struct TrashArgs {
    #[command(subcommand)]
    command: TrashCommand,
}

#[derive(Debug, Subcommand)]
enum TrashCommand {
    Inventory,
    Impact {
        document: DocumentId,
    },
    Document {
        document: DocumentId,
        #[arg(long, required = true)]
        confirm: bool,
    },
    Work {
        work: WorkId,
        #[arg(long, required = true)]
        confirm: bool,
    },
}

#[derive(Debug, Args)]
struct RestoreArgs {
    #[command(subcommand)]
    command: RestoreCommand,
}

#[derive(Debug, Subcommand)]
enum RestoreCommand {
    Document {
        document: DocumentId,
    },
    Work {
        work: WorkId,
        #[arg(long)]
        restore_documents: bool,
        /// Explicit unique replacement title when the historical title conflicts.
        #[arg(long)]
        title: Option<String>,
    },
}

#[derive(Debug, Args)]
struct WipeArgs {
    #[command(subcommand)]
    command: WipeCommand,
}

#[derive(Debug, Subcommand)]
enum WipeCommand {
    Execute {
        document: DocumentId,
        /// Must exactly equal: WIPE <document-id> PERMANENTLY
        #[arg(long)]
        confirm: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CheckpointType {
    Automatic,
    Manual,
    Structural,
    Quit,
}

impl From<CheckpointType> for CheckpointKind {
    fn from(value: CheckpointType) -> Self {
        match value {
            CheckpointType::Automatic => Self::Automatic,
            CheckpointType::Manual => Self::Manual,
            CheckpointType::Structural => Self::Structural,
            CheckpointType::Quit => Self::Quit,
        }
    }
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("carta: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn StdError>> {
    if let Command::Create { path } = &cli.command {
        let archive = Archive::create(path)?;
        println!("{}", archive.root().display());
        return Ok(());
    }
    if matches!(&cli.command, Command::Validate) {
        return match Archive::validate(&cli.archive) {
            Ok(()) => {
                println!("valid");
                Ok(())
            }
            Err(errors) => {
                print_validation_errors(&errors);
                Err(Box::new(errors))
            }
        };
    }

    let mut archive = Archive::open(&cli.archive)?;
    match cli.command {
        Command::Create { .. } | Command::Validate => unreachable!(),
        Command::Inspect => {
            let version = archive.metadata().format_version();
            println!("archive\t{}", archive.metadata().archive_id());
            println!("format\t{}.{}", version.major(), version.minor());
            println!("created\t{}", archive.metadata().created());
            println!("documents\t{}", archive.documents().count());
            println!("works\t{}", archive.works().count());
            println!("dirty\t{}", archive.is_dirty()?);
        }
        Command::Documents => {
            for info in archive.documents() {
                let document = archive.read_document(info.id())?;
                println!(
                    "{}\t{}\t{:04}-{:02}\t{}",
                    info.id(),
                    info.created(),
                    info.volume().year(),
                    info.volume().month(),
                    document.derived_label().replace(['\t', '\n'], " ")
                );
            }
        }
        Command::Works => {
            for work in archive.works() {
                println!(
                    "{}\t{}\t{}\t{}",
                    work.id(),
                    work.created(),
                    work.documents().len(),
                    work.title().replace(['\t', '\n'], " ")
                );
            }
        }
        Command::Checkpoint { kind, note } => {
            match archive.checkpoint(kind.into(), note.as_deref())? {
                Some(checkpoint) => println!("{}", checkpoint.id()),
                None => println!("unchanged"),
            }
        }
        Command::History => {
            for checkpoint in archive.history()? {
                println!(
                    "{}\t{}\t{}\t{}",
                    checkpoint.id(),
                    checkpoint.created(),
                    checkpoint.kind().map_or("external", checkpoint_kind_name),
                    checkpoint.note().unwrap_or("").replace(['\t', '\n'], " ")
                );
            }
        }
        Command::Import { source } => {
            let bytes = fs::read(&source)?;
            println!("{}", archive.import_document_bytes(&bytes)?);
        }
        Command::Search { query } => {
            for result in archive.search(&query)? {
                println!(
                    "{}\t{}\t{}\t{}",
                    result.document(),
                    result.created(),
                    result.label().replace(['\t', '\n'], " "),
                    result.context().replace('\t', " ")
                );
            }
        }
        Command::Export(args) => match args.command {
            ExportCommand::MarkdownDocument { document } => {
                write_stdout(archive.export_document_markdown(document)?.as_bytes())?;
            }
            ExportCommand::MarkdownWork { work } => {
                write_stdout(archive.export_work_markdown(work)?.as_bytes())?;
            }
            ExportCommand::PdfDocument(args) => archive.export_document_pdf_with(
                args.document,
                args.destination,
                &PdfExportOptions::new(args.renderer.pandoc, args.renderer.pdf_engine),
            )?,
            ExportCommand::PdfWork(args) => archive.export_work_pdf_with(
                args.work,
                args.destination,
                &PdfExportOptions::new(args.renderer.pandoc, args.renderer.pdf_engine),
            )?,
        },
        Command::Package { destination } => {
            let report = archive.package(destination)?;
            println!("{}\t{}", report.destination().display(), report.entries());
        }
        Command::Trash(args) => match args.command {
            TrashCommand::Inventory => {
                let inventory = archive.trash_inventory()?;
                for document in inventory.documents() {
                    println!(
                        "document\t{}\t{}\t{}",
                        document.id(),
                        document.created(),
                        document.label()
                    );
                }
                for work in inventory.works() {
                    println!("work\t{}\t{}\t{}", work.id(), work.created(), work.title());
                }
            }
            TrashCommand::Impact { document } => {
                let impact = archive.document_trash_impact(document)?;
                println!("document\t{}", impact.document());
                for membership in impact.memberships() {
                    println!("work\t{}\t{}", membership.id(), membership.title());
                }
                for backlink in impact.inbound_links() {
                    println!(
                        "inbound-link\t{}\t{}",
                        backlink.source(),
                        backlink.link().destination()
                    );
                }
            }
            TrashCommand::Document { document, confirm } => {
                debug_assert!(confirm);
                println!("{}", archive.trash_document(document)?.id());
            }
            TrashCommand::Work { work, confirm } => {
                debug_assert!(confirm);
                println!("{}", archive.trash_work(work)?.id());
            }
        },
        Command::Restore(args) => match args.command {
            RestoreCommand::Document { document } => {
                println!("{}", archive.restore_trashed_document(document)?.id());
            }
            RestoreCommand::Work {
                work,
                restore_documents,
                title,
            } => println!(
                "{}",
                archive
                    .restore_trashed_work(work, restore_documents, title)?
                    .id()
            ),
        },
        Command::Wipe(args) => match args.command {
            WipeCommand::Execute { document, confirm } => {
                let expected = format!("WIPE {document} PERMANENTLY");
                if confirm != expected {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("confirmation must exactly equal {expected:?}"),
                    )
                    .into());
                }
                let plan = archive.plan_wipe_document(document)?;
                let token = plan.confirmation_token().to_owned();
                let report = archive.execute_wipe_document(&plan, &token)?;
                println!(
                    "rewritten-refs\t{}\nrewritten-commits\t{}\nremoved-artifacts\t{}",
                    report.rewritten_refs(),
                    report.rewritten_commits(),
                    report.removed_artifacts()
                );
            }
        },
    }
    Ok(())
}

fn checkpoint_kind_name(kind: CheckpointKind) -> &'static str {
    match kind {
        CheckpointKind::Automatic => "automatic",
        CheckpointKind::Manual => "manual",
        CheckpointKind::Structural => "structural",
        CheckpointKind::Quit => "quit",
    }
}

fn print_validation_errors(errors: &ValidationErrors) {
    for issue in errors.issues() {
        eprintln!("{}", issue);
    }
}

fn write_stdout(bytes: &[u8]) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(bytes)?;
    stdout.flush()
}
