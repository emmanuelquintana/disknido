//! SQLite index. One file index (`entries`) holding every scanned root; a
//! scan replaces only the roots it walks. Scans, duplicates, operations and
//! user marks persist across scans.
//! Migrations are versioned through `PRAGMA user_version`.

use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

const MIGRATIONS: &[&str] = &[
    // v1
    r#"
    CREATE TABLE scans (
        id INTEGER PRIMARY KEY,
        started_at INTEGER NOT NULL,
        finished_at INTEGER,
        status TEXT NOT NULL,          -- running | done | cancelled | failed | interrupted
        mode TEXT NOT NULL,
        roots TEXT NOT NULL,           -- JSON array of root paths
        files INTEGER NOT NULL DEFAULT 0,
        dirs INTEGER NOT NULL DEFAULT 0,
        bytes INTEGER NOT NULL DEFAULT 0,
        errors INTEGER NOT NULL DEFAULT 0
    );
    -- Current index. parent_id = 0 for scan roots; children always have a
    -- larger id than their parent (ids are assigned while walking).
    CREATE TABLE entries (
        id INTEGER PRIMARY KEY,
        parent_id INTEGER NOT NULL,
        name TEXT NOT NULL,
        is_dir INTEGER NOT NULL,
        size INTEGER NOT NULL,
        mtime INTEGER NOT NULL,
        ctime INTEGER NOT NULL,
        attrs INTEGER NOT NULL,
        ext TEXT,
        total_size INTEGER,
        file_count INTEGER,
        newest_mtime INTEGER,
        oldest_mtime INTEGER
    );
    CREATE INDEX entries_parent ON entries(parent_id);
    CREATE INDEX entries_file_size ON entries(is_dir, size);
    CREATE INDEX entries_ext ON entries(ext) WHERE ext IS NOT NULL;
    CREATE TABLE scan_errors (
        scan_id INTEGER NOT NULL,
        path TEXT NOT NULL,
        message TEXT NOT NULL
    );
    CREATE TABLE hash_cache (
        path TEXT PRIMARY KEY,
        size INTEGER NOT NULL,
        mtime INTEGER NOT NULL,
        file_key TEXT NOT NULL,
        partial TEXT,
        full TEXT
    );
    CREATE TABLE dup_groups (
        id INTEGER PRIMARY KEY,
        run_at INTEGER NOT NULL,
        size INTEGER NOT NULL,
        hash TEXT NOT NULL,
        verified INTEGER NOT NULL      -- 1 = byte-by-byte confirmed
    );
    CREATE TABLE dup_files (
        group_id INTEGER NOT NULL REFERENCES dup_groups(id) ON DELETE CASCADE,
        path TEXT NOT NULL,
        mtime INTEGER NOT NULL,
        file_key TEXT NOT NULL,
        protected_reason TEXT
    );
    CREATE INDEX dup_files_group ON dup_files(group_id);
    CREATE TABLE operations (
        id INTEGER PRIMARY KEY,
        kind TEXT NOT NULL,            -- organize | compress | recycle | empty_recycle_bin
        created_at INTEGER NOT NULL,
        finished_at INTEGER,
        status TEXT NOT NULL,          -- running | done | partial | failed | cancelled | interrupted
        summary TEXT NOT NULL,         -- JSON
        bytes INTEGER NOT NULL DEFAULT 0,
        free_before INTEGER,
        free_after INTEGER,
        reversible INTEGER NOT NULL DEFAULT 0,
        error TEXT
    );
    CREATE TABLE operation_items (
        op_id INTEGER NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
        seq INTEGER NOT NULL,
        src TEXT NOT NULL,
        dst TEXT,
        status TEXT NOT NULL,          -- pending | running | done | failed | skipped
        error TEXT,
        PRIMARY KEY (op_id, seq)
    );
    CREATE TABLE marks (
        path TEXT PRIMARY KEY,         -- lower-cased full path
        mark TEXT NOT NULL,            -- important | ignored | favorite | excluded
        created_at INTEGER NOT NULL
    );
    CREATE TABLE backup_overrides (
        path TEXT PRIMARY KEY,         -- lower-cased full path
        year INTEGER,
        month INTEGER,
        no_date INTEGER NOT NULL DEFAULT 0,
        excluded INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    "#,
    // v2: per-root replacement. Root rows (parent_id = 0) record the scan
    // that produced them; other rows leave it NULL. A v1 index came entirely
    // from the most recent scan.
    r#"
    ALTER TABLE entries ADD COLUMN scan_id INTEGER;
    UPDATE entries SET scan_id = (SELECT MAX(id) FROM scans) WHERE parent_id = 0;
    "#,
    // v3: catalog of archives (what is inside each ZIP and where it lives).
    // Survives "reset index": it is the user's record, not analysis data.
    r#"
    CREATE TABLE archives (
        id INTEGER PRIMARY KEY,
        path TEXT NOT NULL,
        path_key TEXT NOT NULL UNIQUE,     -- lower-cased path
        search_key TEXT NOT NULL,          -- lower-cased, accents folded
        source TEXT,                       -- folder it was made from, when known
        origin TEXT NOT NULL,              -- created | imported
        created_at INTEGER NOT NULL,
        files INTEGER NOT NULL,
        bytes INTEGER NOT NULL,            -- uncompressed total
        archive_bytes INTEGER NOT NULL,
        verified INTEGER NOT NULL          -- 1 = contents checked with BLAKE3 when created
    );
    CREATE TABLE archive_entries (
        archive_id INTEGER NOT NULL REFERENCES archives(id) ON DELETE CASCADE,
        rel_path TEXT NOT NULL,            -- path inside the archive, '/' separated
        name TEXT NOT NULL,                -- last component
        search_key TEXT NOT NULL,          -- rel_path lower-cased, accents folded
        size INTEGER NOT NULL,
        mtime INTEGER,
        blake3 TEXT
    );
    CREATE INDEX archive_entries_archive ON archive_entries(archive_id);
    "#,
    // v4: extension totals per indexed root, filled while scanning, so the
    // distribution never needs a GROUP BY over millions of rows.
    r#"
    CREATE TABLE ext_stats (
        root_id INTEGER NOT NULL,
        ext TEXT NOT NULL,                 -- '' = no extension
        files INTEGER NOT NULL,
        bytes INTEGER NOT NULL,
        PRIMARY KEY (root_id, ext)
    );
    "#,
];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA temp_store=MEMORY;")
}

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current: usize = conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))?;
        tx.commit()?;
    }
    Ok(())
}

pub fn schema_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
}

/// Marks leftovers from a crash so nothing looks like it is still running.
pub fn recover_interrupted(conn: &Connection) -> rusqlite::Result<usize> {
    let a = conn.execute("UPDATE scans SET status='interrupted' WHERE status='running'", [])?;
    let b = conn.execute(
        "UPDATE operations SET status='interrupted', finished_at=?1 WHERE status='running'",
        params![crate::now_unix()],
    )?;
    Ok(a + b)
}

/// "Restablecer índice": drops analysis results. Never touches user files.
pub fn reset_index(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "DELETE FROM entries; DELETE FROM ext_stats; DELETE FROM scan_errors; DELETE FROM scans; DELETE FROM hash_cache;
         DELETE FROM dup_files; DELETE FROM dup_groups;",
    )
}

pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get(0)).optional()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn migrations_are_idempotent() {
        let c = super::open_in_memory().unwrap();
        super::migrate(&c).unwrap();
        assert_eq!(super::schema_version(&c).unwrap(), super::MIGRATIONS.len() as i64);
    }

    #[test]
    fn v1_index_roots_get_the_last_scan() {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch(super::MIGRATIONS[0]).unwrap();
        c.execute_batch(
            "PRAGMA user_version = 1;
             INSERT INTO scans(id,started_at,status,mode,roots) VALUES(1,1,'done','full','[]'),(2,2,'done','full','[]');
             INSERT INTO entries(id,parent_id,name,is_dir,size,mtime,ctime,attrs) VALUES(1,0,'D:\\',1,0,0,0,0),(2,1,'a',0,5,0,0,0);",
        )
        .unwrap();
        super::migrate(&c).unwrap();
        let ids: Vec<Option<i64>> = c
            .prepare("SELECT scan_id FROM entries ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(ids, vec![Some(2), None]);
    }
}
