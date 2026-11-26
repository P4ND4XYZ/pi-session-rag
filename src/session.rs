use rusqlite::{Connection, params};
use serde_json::Value;

use serde::Deserialize;

use std::{
    collections::HashMap,
    error::Error,
    fs::File,
    io::{self, BufRead, BufReader},
    path::Path,
};

use crate::db::DB_PATH;

#[derive(Deserialize, Debug)]
struct SessionHeader {
    #[serde(rename = "type")]
    kind: String,
    version: u32,
    id: String,
}

#[derive(Deserialize)]
pub struct Entry {
    #[serde(rename = "type")]
    kind: String,
    id: String,
    #[serde(rename = "parentId")]
    parent_id: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    role: String,
    content: Content,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

pub struct Session {
    header: SessionHeader,
    records: HashMap<String, Entry>,
    last_entry_id: Option<String>,
}

impl Session {
    pub fn new(path: &std::path::Path) -> io::Result<Self> {
        Self::from_records(read_records(path)?)
    }

    fn pi_session_id(&self) -> &str {
        &self.header.id
    }

    fn from_records(records: Vec<Value>) -> io::Result<Self> {
        let mut records = records.into_iter();
        let header_value = records
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing session header"))?;
        let header: SessionHeader = serde_json::from_value(header_value)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("line 1: {err}")))?;
        if header.kind != "session" || header.version != 3 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "line 1: expected a Pi v3 session header",
            ));
        }

        let mut entries = HashMap::new();
        let mut last_entry_id = None;
        for (index, record) in records.enumerate() {
            let line = index + 2;
            let entry: Entry = serde_json::from_value(record).map_err(|err| {
                io::Error::new(io::ErrorKind::InvalidData, format!("line {line}: {err}"))
            })?;
            if entry.kind == "session" {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("line {line}: unexpected session header"),
                ));
            }
            let id = entry.id.clone();
            if entries.insert(id.clone(), entry).is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("line {line}: duplicate entry ID {id}"),
                ));
            }
            last_entry_id = Some(id);
        }

        Ok(Self {
            header,
            records: entries,
            last_entry_id,
        })
    }

    pub fn branch(&self) -> io::Result<Vec<&Entry>> {
        let mut branch = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut current_id = self.last_entry_id.as_deref();

        while let Some(id) = current_id {
            if !visited.insert(id) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("cycle detected at entry {id}"),
                ));
            }

            let entry = self.records.get(id).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("cycle detected at entry {id}"),
                )
            })?;

            branch.push(entry);
            current_id = entry.parent_id.as_deref();
        }

        branch.reverse();

        Ok(branch)
    }

    fn messages_for_import(&self) -> io::Result<Vec<(String, String)>> {
        let branch = self.branch()?;
        let mut messages = Vec::new();

        for e in branch {
            let Some(message) = e.message.as_ref() else {
                continue;
            };

            if !matches!(message.role.as_str(), "user" | "assistant") {
                continue;
            }

            let text = match &message.content {
                Content::Text(text) => text.clone(),
                Content::Blocks(blocks) => blocks
                    .iter()
                    .filter(|block| block.kind == "text")
                    .filter_map(|block| block.text.as_deref())
                    .filter(|text| !text.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
            };

            if text.trim().is_empty() {
                continue;
            }

            messages.push((e.id.clone(), text));
        }

        Ok(messages)
    }

    pub fn import_session(path: &Path) -> Result<(), Box<dyn Error>> {
        let session = Session::new(path)?;
        let messages = session.messages_for_import()?;

        let mut connection = Connection::open(DB_PATH)?;
        connection.execute_batch(include_str!("schema.sql"))?;

        let tx = connection.transaction()?;
        tx.execute(
            "INSERT INTO sessions (pi_session_id, source_path) VALUES (?1, ?2)",
            params![session.pi_session_id(), path.to_string_lossy().as_ref()],
        )?;
        let session_id = tx.last_insert_rowid();

        for (pi_entry_id, text) in messages {
            tx.execute(
                "INSERT INTO messages (session_id, pi_entry_id, text) VALUES (?1, ?2, ?3)",
                params![session_id, pi_entry_id, text],
            )?;
        }
        tx.commit()?;

        Ok(())
    }
}

fn read_records(path: &std::path::Path) -> io::Result<Vec<Value>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stores_header_separately_and_tracks_the_last_entry_in_file_order() {
        let session = Session::from_records(vec![
            json!({"type": "session", "version": 3, "id": "s"}),
            json!({"type": "message", "id": "z", "parentId": null}),
            json!({"type": "model_change", "id": "a", "parentId": "z"}),
        ])
        .unwrap();

        assert_eq!(session.header.id, "s");
        assert_eq!(session.records.len(), 2);
        assert!(!session.records.contains_key("s"));
        assert_eq!(session.records["a"].parent_id.as_deref(), Some("z"));
        assert_eq!(session.last_entry_id.as_deref(), Some("a"));
    }

    #[test]
    fn header_only_session_has_no_last_entry() {
        let session =
            Session::from_records(vec![json!({"type": "session", "version": 3, "id": "s"})])
                .unwrap();

        assert!(session.records.is_empty());
        assert_eq!(session.last_entry_id, None);
    }

    #[test]
    fn branch_returns_only_the_last_entries_ancestors_in_root_order() {
        let session = Session::from_records(vec![
            json!({"type": "session", "version": 3, "id": "s"}),
            json!({"type": "message", "id": "root", "parentId": null}),
            json!({"type": "model_change", "id": "model", "parentId": "root"}),
            json!({"type": "message", "id": "other", "parentId": "model"}),
            json!({"type": "message", "id": "last", "parentId": "model"}),
        ])
        .unwrap();

        let ids: Vec<_> = session
            .branch()
            .unwrap()
            .into_iter()
            .map(|entry| entry.id.as_str())
            .collect();

        assert_eq!(ids, ["root", "model", "last"]);
    }

    #[test]
    fn branch_is_empty_for_a_header_only_session() {
        let session =
            Session::from_records(vec![json!({"type": "session", "version": 3, "id": "s"})])
                .unwrap();

        assert!(session.branch().unwrap().is_empty());
    }

    #[test]
    fn branch_rejects_a_missing_parent() {
        let session = Session::from_records(vec![
            json!({"type": "session", "version": 3, "id": "s"}),
            json!({"type": "message", "id": "last", "parentId": "missing"}),
        ])
        .unwrap();

        let err = session.branch().err().expect("missing parent must fail");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn branch_rejects_a_parent_cycle() {
        let session = Session::from_records(vec![
            json!({"type": "session", "version": 3, "id": "s"}),
            json!({"type": "message", "id": "a", "parentId": "b"}),
            json!({"type": "message", "id": "b", "parentId": "a"}),
        ])
        .unwrap();

        let err = session.branch().err().expect("parent cycle must fail");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn rejects_missing_invalid_or_unsupported_headers() {
        for records in [
            vec![],
            vec![json!({"type": "session", "id": "s"})],
            vec![json!({"type": "message", "version": 3, "id": "a"})],
            vec![json!({"type": "session", "version": 2, "id": "s"})],
        ] {
            assert_eq!(
                Session::from_records(records).err().unwrap().kind(),
                io::ErrorKind::InvalidData,
            );
        }
    }

    #[test]
    fn rejects_duplicate_entry_ids() {
        let result = Session::from_records(vec![
            json!({"type": "session", "version": 3, "id": "s"}),
            json!({"type": "message", "id": "a", "parentId": null}),
            json!({"type": "message", "id": "a", "parentId": "a"}),
        ]);

        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::InvalidData);
    }
}
