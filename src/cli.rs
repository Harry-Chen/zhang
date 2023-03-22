use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::core::ledger::Ledger;
use crate::transformers::zhang::ZhangTransformer;
use crate::{exporter, importer};

#[derive(Parser, Debug)]
#[clap(about, version, author)]
pub enum Opts {
    /// import sources accounting file as zhang data
    #[clap(subcommand)]
    Importer(ImportOpts),

    /// zhang parser
    Parse(ParseOpts),

    /// export to target file
    #[clap(subcommand)]
    Exporter(ExportOpts),

    /// start an internal server with frontend ui
    Server(ServerOpts),
}

#[derive(Subcommand, Debug)]
pub enum ImportOpts {
    Wechat { file: PathBuf, config: PathBuf },
}

#[derive(Args, Debug)]
pub struct ParseOpts {
    /// base path of zhang project
    pub path: PathBuf,

    /// the endpoint of main zhang file.
    #[clap(short, long, default_value = "main.zhang")]
    pub endpoint: String,

    /// indicate cache database file path, using tempfile if not present
    #[clap(long)]
    pub database: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum ExportOpts {
    Beancount {
        file: PathBuf,
        #[clap(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Args, Debug)]
pub struct ServerOpts {
    /// base path of zhang project
    pub path: PathBuf,

    /// the endpoint of main zhang file.
    #[clap(short, long, default_value = "main.zhang")]
    pub endpoint: String,

    /// serve port
    #[clap(short, long, default_value_t = 8000)]
    pub port: u16,

    /// indicate cache database file path, use memory database if not present
    #[clap(long)]
    pub database: Option<PathBuf>,

    /// whether the server report version info for anonymous statistics
    #[clap(long)]
    pub no_report: bool,
}

impl Opts {
    pub async fn run(self) {
        match self {
            Opts::Importer(importer) => importer.run(),
            Opts::Parse(parse_opts) => {
                Ledger::load_with_database::<ZhangTransformer>(
                    parse_opts.path,
                    parse_opts.endpoint,
                    parse_opts.database,
                )
                .await
                .expect("Cannot load ledger");
            }
            Opts::Exporter(opts) => opts.run().await,
            Opts::Server(opts) => crate::server::serve(opts).await.expect("cannot serve"),
        }
    }
}

impl ImportOpts {
    pub fn run(self) {
        let result = match self {
            ImportOpts::Wechat { file, config } => importer::wechat::run(file, config),
        };
        match result {
            Ok(_) => {}
            Err(error) => {
                eprintln!("{}", error)
            }
        }
    }
}

impl ExportOpts {
    pub async fn run(self) {
        let result = match self {
            ExportOpts::Beancount { file, output } => exporter::beancount::run(file, output).await,
        };
        match result {
            Ok(_) => {}
            Err(error) => {
                eprintln!("{}", error)
            }
        }
    }
}
