use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::session::Session;

pub mod session;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Import { path: PathBuf },
    Search { query: String },
}

fn main() {
    match Cli::parse().command {
        Command::Import { path } => {
            println!("import : {}", path.display());
            if path.is_file() {
                match Session::new(&path) {
                    Ok(session) => {
                        let entry = session.branch();

                    }
                    Err(e) => {
                        eprintln!("{}", e)
                    }
                }
            } else {
                eprintln!("must be file.");
            }
        }
        Command::Search { query } => {
            println!("search : {}", query);
        }
    }
}
