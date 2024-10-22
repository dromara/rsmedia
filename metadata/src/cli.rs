use clap::{Parser};

/// Media file metadata for human consumption.
#[derive(Parser, Debug)]
#[command(version = env!("CARGO_PKG_VERSION"), author = "Zhiming Wang <metadata@zhimingwang.org>")]
pub struct Cli {
    /// Include file checksum(s)
    #[arg(short, long)]
    pub checksum: bool,

    /// Print metadata tags, except mundane ones
    #[arg(short, long)]
    pub tags: bool,

    /// Print all metadata tags
    #[arg(short = 'A', long = "all-tags")]
    pub  all_tags: bool,

    /// Media file(s)
    #[arg(required = true)]
    pub files: Vec<String>,
}