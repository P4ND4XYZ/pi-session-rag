use serde_json::Value;

use std::{
    fs::File,
    io::{self, BufRead, BufReader},
};

pub fn read_records(path: &std::path::Path) -> io::Result<Vec<Value>> {
    let reader = BufReader::new(File::open(path)?);

    reader
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let line = line?;
            serde_json::from_str(&line).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}:{}: {err}", path.display(), index + 1),
                )
            })
        })
        .collect()
}

pub fn debug(path: &std::path::Path) -> io::Result<()> {
    let records = read_records(path)?;

    for (index, record) in records.iter().enumerate() {
        let line = index + 1;
        let record_type = record.get("type").and_then(Value::as_str);

        match record_type {
            Some("session") => {
                let version = record.get("version").and_then(Value::as_u64);
                match version {
                    Some(version) => println!("{line}: type=session, version={version}"),
                    None => println!("{line}: type=session, not found version"),
                }
            }
            Some(record_type) => println!("{line}: type={record_type}"),
            None => println!("{line}: not found type"),
        }
    }

    Ok(())
}
