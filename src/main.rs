use clap::{Parser, Subcommand, builder::NonEmptyStringValueParser};
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

use crate::{db::DB_PATH, session::Session};

mod db;
pub mod session;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Import {
        path: PathBuf,
    },
    Search {
        #[arg(value_parser = NonEmptyStringValueParser::new())]
        query: String,
    },
}

fn main() {
    match Cli::parse().command {
        Command::Import { path } => {
            println!("import : {}", path.display());
            if path.is_file() {
                match Session::import_session(&path) {
                    Ok(_) => {
                        println!("import ok.");
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
            if let Err(err) = search(&query) {
                eprintln!("search failed ({DB_PATH}): {err}");
                std::process::exit(1);
            }
        }
    }
}

fn search(query: &str) -> rusqlite::Result<()> {
    let conn = Connection::open_with_flags(DB_PATH, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut stmt = conn.prepare(
        "SELECT s.pi_session_id, m.pi_entry_id, m.text
         FROM messages AS m
         JOIN sessions AS s ON s.id = m.session_id
         WHERE instr(m.text, ?1) > 0
         ORDER BY m.session_id, m.rowid",
    )?;
    let mut rows = stmt.query([query])?;
    while let Some(row) = rows.next()? {
        let session_id: String = row.get(0)?;
        let entry_id: String = row.get(1)?;
        let text: String = row.get(2)?;
        println!("[{session_id} / {entry_id}] {text}");
    }
    Ok(())
}
