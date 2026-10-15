//! Backup folder detection with interpretable rules.
//!
//! A folder is a backup candidate when:
//! * its name has a strong keyword (backup, respaldo, bkp, bk, copia de
//!   seguridad), or
//! * its name has a weak keyword (copia, archive, histórico, mis archivos…)
//!   AND some date component, or
//! * it sits inside a keyword-only container (e.g. `D:\Respaldos`) AND its
//!   own name carries a date.
//!
//! Dates come from the name first. Ambiguous numbers (`03-04`) are flagged for
//! review instead of guessed. Content dates are only a labelled fallback.

use crate::query::PathResolver;
use crate::{Result, safety};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};

const STRONG: &[&str] = &["backup", "backups", "respaldo", "respaldos", "bkp", "bk", "bak", "bck"];
const WEAK: &[&str] = &["copia", "copias", "archive", "archives", "archivado", "historico", "historicos", "historical"];

fn month_of(tok: &str) -> Option<u32> {
    Some(match tok {
        "enero" | "ene" | "january" | "jan" => 1,
        "febrero" | "feb" | "february" => 2,
        "marzo" | "mar" | "march" => 3,
        "abril" | "abr" | "april" | "apr" => 4,
        "mayo" | "may" => 5,
        "junio" | "jun" | "june" => 6,
        "julio" | "jul" | "july" => 7,
        "agosto" | "ago" | "august" | "aug" => 8,
        "septiembre" | "setiembre" | "sep" | "sept" | "set" | "september" => 9,
        "octubre" | "oct" | "october" => 10,
        "noviembre" | "nov" | "november" => 11,
        "diciembre" | "dic" | "december" | "dec" => 12,
        _ => return None,
    })
}

fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'Á' | 'À' | 'Ä' | 'Â' => 'a',
            'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'Ó' | 'Ò' | 'Ö' | 'Ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
            'ñ' | 'Ñ' => 'n',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

/// Splits into alphabetic and numeric runs: "BACKUP_2024_08" → [backup, 2024, 08],
/// "bk02" → [bk, 02]. Separators between numbers are kept to detect patterns.
fn tokens(name: &str) -> Vec<(String, char)> {
    // (token, separator that followed it)
    let s = fold(name);
    let mut out: Vec<(String, char)> = Vec::new();
    let mut cur = String::new();
    let mut cur_digit = false;
    for c in s.chars() {
        let is_alnum = c.is_ascii_alphanumeric();
        if is_alnum {
            let d = c.is_ascii_digit();
            if !cur.is_empty() && d != cur_digit {
                out.push((std::mem::take(&mut cur), '\0'));
            }
            cur_digit = d;
            cur.push(c);
        } else {
            if !cur.is_empty() {
                out.push((std::mem::take(&mut cur), c));
            } else if let Some(last) = out.last_mut()
                && last.1 == '\0'
            {
                last.1 = c;
            }
        }
    }
    if !cur.is_empty() {
        out.push((cur, '\0'));
    }
    out
}

fn is_num(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn year_ok(y: i32) -> bool {
    (1990..=2100).contains(&y)
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedDate {
    pub year: Option<i32>,
    pub month: Option<u32>,
    /// Numbers that could be read more than one way; needs user review.
    pub ambiguous: bool,
    pub evidence: Vec<String>,
}

impl ParsedDate {
    pub fn has_any(&self) -> bool {
        self.year.is_some() || self.month.is_some() || self.ambiguous
    }
}

pub fn parse_date(name: &str) -> ParsedDate {
    let toks = tokens(name);
    let mut d = ParsedDate::default();
    let mut years: Vec<i32> = Vec::new();
    let mut months: Vec<u32> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let t = toks[i].0.as_str();
        let next = toks.get(i + 1).map(|x| x.0.as_str());
        let next2 = toks.get(i + 2).map(|x| x.0.as_str());
        if is_num(t) {
            let n: i64 = t.parse().unwrap_or(-1);
            match t.len() {
                8 => {
                    // YYYYMMDD
                    let (y, m) = (t[..4].parse::<i32>().unwrap_or(0), t[4..6].parse::<u32>().unwrap_or(0));
                    if year_ok(y) && (1..=12).contains(&m) {
                        years.push(y);
                        months.push(m);
                        d.evidence.push(format!("«{t}» = {y}-{m:02}"));
                    }
                }
                6 => {
                    // YYYYMM, or MMYYYY
                    let (y, m) = (t[..4].parse::<i32>().unwrap_or(0), t[4..].parse::<u32>().unwrap_or(0));
                    let (m2, y2) = (t[..2].parse::<u32>().unwrap_or(0), t[2..].parse::<i32>().unwrap_or(0));
                    if year_ok(y) && (1..=12).contains(&m) {
                        years.push(y);
                        months.push(m);
                        d.evidence.push(format!("«{t}» = {y}-{m:02}"));
                    } else if year_ok(y2) && (1..=12).contains(&m2) {
                        years.push(y2);
                        months.push(m2);
                        d.evidence.push(format!("«{t}» = {y2}-{m2:02}"));
                    }
                }
                4 if year_ok(n as i32) => {
                    let y = n as i32;
                    years.push(y);
                    // YYYY-MM[-DD]
                    if let Some(nx) = next.filter(|s| is_num(s) && s.len() <= 2) {
                        let m: u32 = nx.parse().unwrap_or(0);
                        if (1..=12).contains(&m) {
                            months.push(m);
                            d.evidence.push(format!("«{y}-{nx}» = año-mes"));
                            i += 1;
                            if next2.is_some_and(|s| is_num(s) && s.len() <= 2) {
                                i += 1; // day
                            }
                        }
                    } else {
                        d.evidence.push(format!("año «{y}»"));
                    }
                }
                1 | 2 => {
                    let a = n as u32;
                    match (next, next2) {
                        // A-B-YYYY: day/month order unknown unless one side > 12
                        (Some(b), Some(y))
                            if is_num(b) && b.len() <= 2 && is_num(y) && y.len() == 4 && year_ok(y.parse().unwrap_or(0)) =>
                        {
                            let b: u32 = b.parse().unwrap_or(0);
                            years.push(y.parse().unwrap());
                            if a <= 12 && b <= 12 && a != b && a >= 1 && b >= 1 {
                                d.ambiguous = true;
                                d.evidence.push(format!("«{a:02}-{b:02}-{y}»: el mes puede ser {a} o {b}"));
                            } else if (1..=12).contains(&b) && a > 12 {
                                months.push(b);
                                d.evidence.push(format!("«{a:02}-{b:02}-{y}» = día-mes-año"));
                            } else if (1..=12).contains(&a) {
                                months.push(a);
                                d.evidence.push(format!("«{a:02}-{b:02}-{y}» = mes-día-año"));
                            }
                            i += 2;
                        }
                        // MM-YYYY
                        (Some(y), _) if is_num(y) && y.len() == 4 && year_ok(y.parse().unwrap_or(0)) && (1..=12).contains(&a) => {
                            years.push(y.parse().unwrap());
                            months.push(a);
                            d.evidence.push(format!("«{t}-{y}» = mes-año"));
                            i += 1;
                        }
                        // NN-NN without year: day/month? month/year(2 digits)? Unknown.
                        (Some(b), _) if is_num(b) && b.len() <= 2 => {
                            d.ambiguous = true;
                            d.evidence.push(format!("«{t}-{b}» es ambiguo"));
                            i += 1;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        } else if let Some(m) = month_of(t) {
            months.push(m);
            d.evidence.push(format!("mes «{t}»"));
        }
        i += 1;
    }
    years.sort();
    years.dedup();
    months.sort();
    months.dedup();
    if years.len() > 1 || months.len() > 1 {
        d.ambiguous = true;
        d.evidence.push("Varias fechas distintas en el nombre".into());
    } else {
        d.year = years.first().copied();
        d.month = months.first().copied();
    }
    d
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum KeywordStrength {
    Strong,
    Weak,
    None,
}

pub fn keyword_strength(name: &str) -> (KeywordStrength, Vec<String>) {
    let toks: Vec<String> = tokens(name).into_iter().map(|t| t.0).collect();
    let mut found = Vec::new();
    let joined = toks.join(" ");
    if joined.contains("copia de seguridad") || joined.contains("copias de seguridad") {
        found.push("copia de seguridad".into());
        return (KeywordStrength::Strong, found);
    }
    for t in &toks {
        if STRONG.contains(&t.as_str()) {
            found.push(t.clone());
        }
    }
    if !found.is_empty() {
        return (KeywordStrength::Strong, found);
    }
    for t in &toks {
        if WEAK.contains(&t.as_str()) {
            found.push(t.clone());
        }
    }
    if joined.contains("mis archivos") {
        found.push("mis archivos".into());
    }
    if found.is_empty() { (KeywordStrength::None, found) } else { (KeywordStrength::Weak, found) }
}

/// Name made only of keywords (e.g. "Backups", "Respaldos", "Copias de seguridad"):
/// a container of backups rather than one backup.
pub fn is_container_name(name: &str) -> bool {
    let (k, _) = keyword_strength(name);
    if k != KeywordStrength::Strong {
        return false;
    }
    tokens(name).iter().all(|(t, _)| {
        STRONG.contains(&t.as_str()) || matches!(t.as_str(), "copia" | "copias" | "de" | "seguridad" | "mis" | "my")
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    High,
    Medium,
    Review,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub keywords: Vec<String>,
    pub strength: KeywordStrength,
    pub date: ParsedDate,
    pub confidence: Confidence,
    pub reasons: Vec<String>,
}

/// Decides whether a folder name (given whether an ancestor is a backup
/// container) is a backup, and how sure we are.
pub fn detect(name: &str, in_container: bool) -> Option<Detection> {
    let (strength, keywords) = keyword_strength(name);
    let date = parse_date(name);
    let complete = date.year.is_some() && date.month.is_some() && !date.ambiguous;
    let mut reasons = Vec::new();
    let accepted = match strength {
        KeywordStrength::Strong => {
            reasons.push(format!("El nombre contiene «{}»", keywords.join("», «")));
            !is_container_name(name)
        }
        KeywordStrength::Weak if date.has_any() => {
            reasons.push(format!("«{}» junto con una fecha", keywords.join("», «")));
            true
        }
        KeywordStrength::None if in_container && date.has_any() => {
            reasons.push("Carpeta con fecha dentro de una carpeta de respaldos".into());
            true
        }
        _ => false,
    };
    if !accepted {
        return None;
    }
    reasons.extend(date.evidence.iter().cloned());
    let confidence = if date.ambiguous || date.year.is_none() {
        Confidence::Review
    } else if complete && strength == KeywordStrength::Strong {
        Confidence::High
    } else {
        Confidence::Medium
    };
    Some(Detection { keywords, strength, date, confidence, reasons })
}

/// Name of the nearest ancestor folder that looks like a backup.
pub fn backup_ancestor(path: &Path) -> Option<String> {
    let mut in_container = false;
    let parent = path.parent()?;
    for c in parent.components() {
        if let Component::Normal(s) = c {
            let name = s.to_string_lossy();
            if is_container_name(&name) {
                in_container = true;
                continue;
            }
            if detect(&name, in_container).is_some() {
                return Some(name.into_owned());
            }
        }
    }
    None
}

/// `<drive>\<root>\YYYY\YYYY-MM\<name>` — already organised by DiskNido.
pub fn organized_layout(path: &Path, root_name: &str) -> Option<(i32, u32)> {
    let comps: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let n = comps.len();
    if n < 4 || !comps[n - 4].eq_ignore_ascii_case(root_name) {
        return None;
    }
    let y: i32 = comps[n - 3].parse().ok()?;
    let (ys, ms) = comps[n - 2].split_once('-')?;
    let m: u32 = ms.parse().ok()?;
    (ys == comps[n - 3] && year_ok(y) && (1..=12).contains(&m)).then_some((y, m))
}

fn structural_folder(path: &Path, root_name: &str) -> bool {
    let comps: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    // `<root>`, `<root>\YYYY`, `<root>\YYYY\YYYY-MM`
    let n = comps.len();
    (1..=3).any(|k| {
        n >= k
            && comps[n - k].eq_ignore_ascii_case(root_name)
            && comps[n - k + 1..].iter().all(|c| !c.is_empty() && c.chars().all(|ch| ch.is_ascii_digit() || ch == '-'))
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupItem {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub volume: String,
    pub size: u64,
    pub file_count: u64,
    pub newest_mtime: Option<i64>,
    pub oldest_mtime: Option<i64>,
    pub year: Option<i32>,
    pub month: Option<u32>,
    /// name | override | content | none
    pub date_source: String,
    pub ambiguous: bool,
    pub confidence: Confidence,
    pub reasons: Vec<String>,
    pub organized: bool,
    pub no_date: bool,
    pub protected_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Override {
    pub year: Option<i32>,
    pub month: Option<u32>,
    #[serde(default)]
    pub no_date: bool,
    #[serde(default)]
    pub excluded: bool,
}

fn load_overrides(conn: &Connection) -> Result<HashMap<String, Override>> {
    let mut st = conn.prepare("SELECT path, year, month, no_date, excluded FROM backup_overrides")?;
    let v = st.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, Override { year: r.get(1)?, month: r.get(2)?, no_date: r.get(3)?, excluded: r.get(4)? }))
    })?;
    Ok(v.collect::<rusqlite::Result<_>>()?)
}

pub fn set_override(conn: &Connection, path: &Path, o: &Override) -> Result<()> {
    if let Some(m) = o.month
        && !(1..=12).contains(&m)
    {
        return Err(crate::Error::Refused("Mes no válido".into()));
    }
    conn.execute(
        "INSERT INTO backup_overrides(path,year,month,no_date,excluded) VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(path) DO UPDATE SET year=excluded.year, month=excluded.month, no_date=excluded.no_date, excluded=excluded.excluded",
        params![safety::path_key(path), o.year, o.month, o.no_date, o.excluded],
    )?;
    Ok(())
}

/// All backup candidates in the current index (top-most only).
pub fn list(conn: &Connection, root_name: &str) -> Result<Vec<BackupItem>> {
    let overrides = load_overrides(conn)?;
    let mut res = PathResolver::load(conn)?;
    let mut st = conn.prepare(
        "SELECT id, parent_id, name, COALESCE(total_size,0), COALESCE(file_count,0), newest_mtime, oldest_mtime, attrs
         FROM entries WHERE is_dir=1 AND parent_id<>0 ORDER BY id",
    )?;
    type Row = (i64, i64, String, i64, i64, Option<i64>, Option<i64>, u32);
    let rows: Vec<Row> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut inside: HashSet<i64> = HashSet::new(); // ids within an accepted backup
    let mut containers: HashSet<i64> = HashSet::new();
    let mut out = Vec::new();
    for (id, parent, name, size, count, newest, oldest, attrs) in rows {
        if inside.contains(&parent) {
            inside.insert(id);
            continue;
        }
        if attrs & crate::fsx::ATTR_REPARSE_POINT != 0 && !crate::fsx::is_cloud_only_attrs(attrs) {
            continue;
        }
        let in_container = containers.contains(&parent);
        let Some(path) = res.dir_path(id) else { continue };
        if structural_folder(&path, root_name) || is_container_name(&name) {
            containers.insert(id);
            if organized_layout(&path, root_name).is_none() {
                continue;
            }
        }
        let organized = organized_layout(&path, root_name);
        let det = detect(&name, in_container);
        if det.is_none() && organized.is_none() {
            if in_container {
                containers.insert(id); // e.g. D:\Respaldos\Laptop\2024-01
            }
            continue;
        }
        let key = safety::path_key(&path);
        let ov = overrides.get(&key).cloned().unwrap_or_default();
        if ov.excluded {
            inside.insert(id);
            continue;
        }
        inside.insert(id);
        let (mut year, mut month, mut source) = (None, None, "none");
        let (mut ambiguous, mut confidence, mut reasons) = (false, Confidence::Review, Vec::new());
        if let Some(d) = &det {
            year = d.date.year;
            month = d.date.month;
            ambiguous = d.date.ambiguous;
            confidence = d.confidence.clone();
            reasons = d.reasons.clone();
            if year.is_some() || month.is_some() {
                source = "name";
            }
        }
        if let Some((y, m)) = organized {
            year = Some(y);
            month = Some(m);
            source = "name";
            ambiguous = false;
            confidence = Confidence::High;
            reasons.push("Ya organizado por año y mes".into());
        }
        if ov.year.is_some() || ov.month.is_some() || ov.no_date {
            year = ov.year;
            month = ov.month;
            source = "override";
            ambiguous = false;
            confidence = Confidence::High;
            reasons.push("Fecha asignada por ti".into());
        } else if year.is_none()
            && !ambiguous
            && let Some(ts) = newest
        {
            // Labelled fallback only; never overrides a date in the name.
            if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
                use chrono::Datelike;
                if month.is_none() {
                    month = Some(dt.month());
                }
                year = Some(dt.year());
                source = "content";
                reasons.push(format!("Fecha inferida del archivo más reciente ({}-{:02})", dt.year(), dt.month()));
            }
        }
        out.push(BackupItem {
            id,
            volume: path.to_string_lossy().get(..2).unwrap_or("").to_ascii_uppercase(),
            protected_reason: safety::protected_reason(&path, false),
            path: path.to_string_lossy().into_owned(),
            name,
            size: size as u64,
            file_count: count as u64,
            newest_mtime: newest,
            oldest_mtime: oldest,
            year,
            month,
            date_source: source.into(),
            ambiguous,
            confidence,
            reasons,
            organized: organized.is_some(),
            no_date: ov.no_date,
        });
    }
    out.sort_by(|a, b| {
        (a.volume.clone(), a.year, a.month, a.name.clone()).cmp(&(b.volume.clone(), b.year, b.month, b.name.clone()))
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ym(name: &str) -> (Option<i32>, Option<u32>, bool) {
        let d = parse_date(name);
        (d.year, d.month, d.ambiguous)
    }

    #[test]
    fn parses_named_months() {
        assert_eq!(ym("Backup Enero 2024"), (Some(2024), Some(1), false));
        assert_eq!(ym("Backup Febrero 2024"), (Some(2024), Some(2), false));
        assert_eq!(ym("Copia PC Junio 2023"), (Some(2023), Some(6), false));
        assert_eq!(ym("backup octubre 2026"), (Some(2026), Some(10), false));
        assert_eq!(ym("Mis Archivos Diciembre 2025"), (Some(2025), Some(12), false));
        assert_eq!(ym("Jan 2025"), (Some(2025), Some(1), false));
        assert_eq!(ym("Backup Sept 2021"), (Some(2021), Some(9), false));
        assert_eq!(ym("Respaldo Dic 2020"), (Some(2020), Some(12), false));
        assert_eq!(ym("BACKUP February 2019"), (Some(2019), Some(2), false));
        assert_eq!(ym("Respaldo Setiembre 2022"), (Some(2022), Some(9), false));
    }

    #[test]
    fn parses_numeric_formats() {
        assert_eq!(ym("BACKUP_2024_08"), (Some(2024), Some(8), false));
        assert_eq!(ym("bk-02-2025"), (Some(2025), Some(2), false));
        assert_eq!(ym("Respaldo Laptop 2023-11"), (Some(2023), Some(11), false));
        assert_eq!(ym("Desktop Backup 01-2024"), (Some(2024), Some(1), false));
        assert_eq!(ym("2025_01"), (Some(2025), Some(1), false));
        assert_eq!(ym("202501"), (Some(2025), Some(1), false));
        assert_eq!(ym("backup 20250115"), (Some(2025), Some(1), false));
        assert_eq!(ym("Respaldo 2024-12"), (Some(2024), Some(12), false));
        assert_eq!(ym("backup 2024-03-15"), (Some(2024), Some(3), false));
        assert_eq!(ym("Respaldo 2026"), (Some(2026), None, false));
        assert_eq!(ym("Respaldos 2025"), (Some(2025), None, false));
    }

    #[test]
    fn ambiguous_dates_are_flagged() {
        assert!(ym("Backup 03-04").2);
        assert!(ym("Backup 03-04-2025").2);
        assert_eq!(ym("Backup 25-04-2025"), (Some(2025), Some(4), false));
        assert!(ym("Backup 2023 a 2024").2);
        assert_eq!(ym("Copia Marzo"), (None, Some(3), false));
    }

    #[test]
    fn detection_rules() {
        assert_eq!(detect("Backup Enero 2025", false).unwrap().confidence, Confidence::High);
        assert_eq!(detect("Respaldo Marzo", false).unwrap().confidence, Confidence::Review);
        assert_eq!(detect("Respaldo 2026", false).unwrap().confidence, Confidence::Medium);
        assert_eq!(detect("Copia Marzo", false).unwrap().confidence, Confidence::Review);
        assert_eq!(detect("Backup 03-04", false).unwrap().confidence, Confidence::Review);
        // weak keyword without date is not a backup
        assert!(detect("Copia", false).is_none());
        assert!(detect("copia de fotos", false).is_none());
        assert!(detect("Proyecto Final", false).is_none());
        assert!(detect("Fotos 2024", false).is_none());
        // strong keyword
        assert!(detect("Copia de seguridad iPhone", false).is_some());
        assert!(detect("bkp-laptop", false).is_some());
        // container folders are not backups themselves
        assert!(detect("Backups", false).is_none());
        assert!(detect("Respaldos", false).is_none());
        // dated folder inside a container
        assert!(detect("2023-11", true).is_some());
        assert!(detect("2023-11", false).is_none());
        // words containing keywords are not keywords
        assert!(detect("Bakery 2024", false).is_none());
        assert!(detect("Backgrounds 2024", false).is_none());
    }

    #[test]
    fn same_month_backups_stay_distinct() {
        let a = detect("Backup PC Trabajo Enero 2025", false).unwrap();
        let b = detect("Backup PC Personal Enero 2025", false).unwrap();
        assert_eq!((a.date.year, a.date.month), (b.date.year, b.date.month));
    }

    #[cfg(windows)]
    #[test]
    fn organized_layout_is_recognised() {
        assert_eq!(organized_layout(Path::new(r"D:\Backups\2025\2025-01\Backup Enero 2025"), "Backups"), Some((2025, 1)));
        assert_eq!(organized_layout(Path::new(r"D:\Backups\2025\2024-01\X"), "Backups"), None);
        assert_eq!(organized_layout(Path::new(r"D:\Otros\2025\2025-01\X"), "Backups"), None);
        assert_eq!(organized_layout(Path::new(r"D:\Archivo\Backups\2024\2024-12\X"), "Backups"), Some((2024, 12)));
        assert!(structural_folder(Path::new(r"D:\Archivo\Backups\2024"), "Backups"));
        assert!(structural_folder(Path::new(r"D:\Backups\2024\2024-01"), "Backups"));
        assert!(!structural_folder(Path::new(r"D:\Backups\2024\2024-01\Backup Enero"), "Backups"));
    }

    #[cfg(windows)]
    #[test]
    fn ancestor_detection() {
        assert_eq!(
            backup_ancestor(Path::new(r"E:\Respaldos\Backup Enero 2024\fotos\a.jpg")).as_deref(),
            Some("Backup Enero 2024")
        );
        assert_eq!(backup_ancestor(Path::new(r"E:\Respaldos\2023-11\a.jpg")).as_deref(), Some("2023-11"));
        assert_eq!(backup_ancestor(Path::new(r"D:\Fotos\a.jpg")), None);
    }
}
