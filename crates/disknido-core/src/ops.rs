//! Operation journal. Every mutating operation writes its intent before
//! touching the disk and records each item's outcome, so a crash leaves a
//! readable trail and nothing destructive is ever replayed automatically.

use crate::Result;
use rusqlite::{Connection, params};
use serde::Serialize;

pub fn begin(
    conn: &Connection,
    kind: &str,
    summary: &serde_json::Value,
    free_before: Option<u64>,
    reversible: bool,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO operations(kind,created_at,status,summary,free_before,reversible) VALUES(?1,?2,'running',?3,?4,?5)",
        params![kind, crate::now_unix(), summary.to_string(), free_before.map(|v| v as i64), reversible],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn add_item(conn: &Connection, op: i64, seq: i64, src: &str, dst: Option<&str>) -> Result<()> {
    conn.execute(
        "INSERT INTO operation_items(op_id,seq,src,dst,status) VALUES(?1,?2,?3,?4,'pending')",
        params![op, seq, src, dst],
    )?;
    Ok(())
}

pub fn item_status(conn: &Connection, op: i64, seq: i64, status: &str, error: Option<&str>) -> Result<()> {
    conn.execute("UPDATE operation_items SET status=?3, error=?4 WHERE op_id=?1 AND seq=?2", params![op, seq, status, error])?;
    Ok(())
}

pub fn finish(conn: &Connection, op: i64, status: &str, bytes: u64, free_after: Option<u64>, error: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE operations SET status=?2, finished_at=?3, bytes=?4, free_after=?5, error=?6 WHERE id=?1",
        params![op, status, crate::now_unix(), bytes as i64, free_after.map(|v| v as i64), error],
    )?;
    Ok(())
}

/// Outcome of a whole operation from its items.
pub fn status_from(done: usize, failed: usize, cancelled: bool) -> &'static str {
    match (done, failed, cancelled) {
        (_, _, true) => "cancelled",
        (0, f, _) if f > 0 => "failed",
        (_, f, _) if f > 0 => "partial",
        _ => "done",
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpItem {
    pub seq: i64,
    pub src: String,
    pub dst: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub id: i64,
    pub kind: String,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    pub status: String,
    pub summary: serde_json::Value,
    pub bytes: u64,
    pub free_before: Option<u64>,
    pub free_after: Option<u64>,
    pub reversible: bool,
    pub error: Option<String>,
    pub items: Vec<OpItem>,
}

pub fn history(conn: &Connection, limit: i64) -> Result<Vec<Operation>> {
    let mut st = conn.prepare(
        "SELECT id,kind,created_at,finished_at,status,summary,bytes,free_before,free_after,reversible,error
         FROM operations ORDER BY id DESC LIMIT ?1",
    )?;
    let ops: Vec<Operation> = st
        .query_map([limit], |r| {
            let summary: String = r.get(5)?;
            Ok(Operation {
                id: r.get(0)?,
                kind: r.get(1)?,
                created_at: r.get(2)?,
                finished_at: r.get(3)?,
                status: r.get(4)?,
                summary: serde_json::from_str(&summary).unwrap_or(serde_json::Value::Null),
                bytes: r.get::<_, i64>(6)? as u64,
                free_before: r.get::<_, Option<i64>>(7)?.map(|v| v as u64),
                free_after: r.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                reversible: r.get(9)?,
                error: r.get(10)?,
                items: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut ist = conn.prepare("SELECT seq,src,dst,status,error FROM operation_items WHERE op_id=?1 ORDER BY seq LIMIT 500")?;
    ops.into_iter()
        .map(|mut op| {
            op.items = ist
                .query_map([op.id], |r| {
                    Ok(OpItem { seq: r.get(0)?, src: r.get(1)?, dst: r.get(2)?, status: r.get(3)?, error: r.get(4)? })
                })?
                .collect::<rusqlite::Result<_>>()?;
            Ok(op)
        })
        .collect()
}

pub fn items(conn: &Connection, op: i64) -> Result<Vec<OpItem>> {
    let mut st = conn.prepare("SELECT seq,src,dst,status,error FROM operation_items WHERE op_id=?1 ORDER BY seq")?;
    let v = st.query_map([op], |r| {
        Ok(OpItem { seq: r.get(0)?, src: r.get(1)?, dst: r.get(2)?, status: r.get(3)?, error: r.get(4)? })
    })?;
    Ok(v.collect::<rusqlite::Result<_>>()?)
}

/// After a crash: settle items left "running" by looking at the disk.
/// For moves, the item is done if the destination exists and the source
/// does not; otherwise it is reported as not done. Nothing is re-executed.
pub fn reconcile_interrupted(conn: &Connection) -> Result<usize> {
    let mut st = conn.prepare(
        "SELECT i.op_id, i.seq, i.src, i.dst, o.kind FROM operation_items i JOIN operations o ON o.id=i.op_id
         WHERE i.status='running'",
    )?;
    let rows: Vec<(i64, i64, String, Option<String>, String)> =
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<rusqlite::Result<_>>()?;
    for (op, seq, src, dst, kind) in &rows {
        let src_exists = std::path::Path::new(src).exists();
        if kind == "compress"
            && let Some(d) = dst
        {
            // Our own unfinished archive; the originals were never touched.
            let _ = std::fs::remove_file(format!("{d}.partial"));
        }
        let status = match (kind.as_str(), dst) {
            ("organize", Some(d)) if !src_exists && std::path::Path::new(d).exists() => "done",
            ("recycle", _) if !src_exists => "done",
            _ => "failed",
        };
        item_status(conn, *op, *seq, status, (status == "failed").then_some("Interrumpido: la aplicación se cerró"))?;
    }
    Ok(rows.len())
}
