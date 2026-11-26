use rusqlite::Connection;
use std::{fs, path::PathBuf, process::Command, time::SystemTime};

#[test]
fn import_stores_a_message_for_its_session() {
    let dir = TestDir::new();
    fs::write(
        dir.0.join("session.jsonl"),
        concat!(
            r#"{"type":"session","version":3,"id":"test-session"}"#,
            "\n",
            r#"{"type":"message","id":"entry-1","parentId":null,"message":{"role":"user","content":"Hello"}}"#,
            "\n",
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pi-rag"))
        .args(["import", "session.jsonl"])
        .current_dir(&dir.0)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success() && stderr.is_empty(), "{stderr}");

    let conn = Connection::open(dir.0.join("pi-rag.db")).unwrap();
    let row: (String, String, String) = conn
        .query_row(
            "SELECT pi_session_id, pi_entry_id, text
             FROM messages JOIN sessions ON sessions.id = messages.session_id",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        row,
        ("test-session".into(), "entry-1".into(), "Hello".into())
    );
}

#[test]
fn search_matches_a_literal_substring_in_message_text() {
    let dir = TestDir::new();
    let conn = Connection::open(dir.0.join("pi-rag.db")).unwrap();
    conn.execute_batch(include_str!("../src/schema.sql"))
        .unwrap();
    conn.execute_batch(
        "INSERT INTO sessions (id, pi_session_id, source_path)
         VALUES (1, 'test-session', 'session.jsonl');
         INSERT INTO messages (session_id, pi_entry_id, text) VALUES
         (1, 'entry-1', 'used 50%_today'),
         (1, 'entry-2', 'used 50Xtoday');",
    )
    .unwrap();
    drop(conn);

    for (query, expected) in [
        ("50%_", "[test-session / entry-1] used 50%_today\n"),
        ("not found", ""),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_pi-rag"))
            .args(["search", query])
            .current_dir(&dir.0)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success() && stderr.is_empty(), "{stderr}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
            expected
        );
    }
}

#[test]
fn search_fails_without_creating_a_missing_database() {
    let dir = TestDir::new();
    let output = Command::new(env!("CARGO_BIN_EXE_pi-rag"))
        .args(["search", "hello"])
        .current_dir(&dir.0)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(!dir.0.join("pi-rag.db").exists());
}

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("pi-rag-test-{}-{timestamp}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
