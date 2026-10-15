//! "Limpieza inteligente": documented rules over known locations only.
//! Extensions alone (.tmp, .log, .bak) never make a file removable.
//! See docs/CLEANUP.md for the rationale of every rule.

use crate::fsx;
use crate::{Control, Error, Result, safety};
use serde::Serialize;
use std::path::{Path, PathBuf};

const DAY: i64 = 86_400;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    /// Documented temporary/regenerable data.
    High,
    /// Probably unneeded, review first.
    Medium,
    /// Old or large; usefulness unknown.
    Manual,
    /// Never offered for removal.
    Protected,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    /// Send selected files to the Recycle Bin.
    Recycle,
    /// Separate, confirmed "empty Recycle Bin".
    EmptyBin,
    /// Open Windows Storage Sense / Cleanup recommendations.
    OpenStorageSettings,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupFile {
    pub path: String,
    pub size: u64,
    pub mtime: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCategory {
    pub id: String,
    pub confidence: Confidence,
    pub action: Action,
    pub locations: Vec<String>,
    pub files: Vec<CleanupFile>,
    pub file_count: u64,
    pub bytes: u64,
    /// For EmptyBin / storage settings: an informational size.
    pub info_bytes: Option<u64>,
}

struct Rule {
    id: &'static str,
    confidence: Confidence,
    roots: Vec<PathBuf>,
    /// Minimum age (days since last modification).
    min_age_days: i64,
    min_size: u64,
    exts: Option<&'static [&'static str]>,
    exclude_exts: Option<&'static [&'static str]>,
    recursive: bool,
    /// Rule targets live under AppData.
    appdata: bool,
}

fn env(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

fn chromium_caches(local: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for vendor in [r"Google\Chrome", r"Microsoft\Edge", r"BraveSoftware\Brave-Browser"] {
        let ud = local.join(vendor).join("User Data");
        let Ok(items) = fsx::list_dir(&ud) else { continue };
        for it in items.into_iter().filter(|i| i.is_dir && !i.is_link()) {
            if it.name == "Default" || it.name.starts_with("Profile ") {
                for sub in [r"Cache\Cache_Data", "Code Cache", "GPUCache"] {
                    let p = ud.join(&it.name).join(sub);
                    if p.is_dir() {
                        out.push(p);
                    }
                }
            }
        }
    }
    let ff = local.join(r"Mozilla\Firefox\Profiles");
    if let Ok(items) = fsx::list_dir(&ff) {
        for it in items.into_iter().filter(|i| i.is_dir && !i.is_link()) {
            let p = ff.join(&it.name).join("cache2");
            if p.is_dir() {
                out.push(p);
            }
        }
    }
    out
}

const INSTALLERS: &[&str] = &["exe", "msi", "msix", "msixbundle", "appx"];

fn rules() -> Vec<Rule> {
    let mut v = Vec::new();
    let temp = std::env::temp_dir();
    let local = env("LOCALAPPDATA");
    let downloads = env("USERPROFILE").map(|h| h.join("Downloads"));
    v.push(Rule {
        id: "userTemp",
        confidence: Confidence::High,
        roots: vec![temp],
        min_age_days: 7,
        min_size: 0,
        exts: None,
        exclude_exts: None,
        recursive: true,
        appdata: true,
    });
    if let Some(local) = &local {
        v.push(Rule {
            id: "browserCache",
            confidence: Confidence::High,
            roots: chromium_caches(local),
            min_age_days: 0,
            min_size: 0,
            exts: None,
            exclude_exts: None,
            recursive: true,
            appdata: true,
        });
        v.push(Rule {
            id: "crashDumps",
            confidence: Confidence::Medium,
            roots: vec![local.join("CrashDumps")],
            min_age_days: 0,
            min_size: 0,
            exts: None,
            exclude_exts: None,
            recursive: false,
            appdata: true,
        });
        v.push(Rule {
            id: "errorReports",
            confidence: Confidence::Medium,
            roots: vec![local.join(r"Microsoft\Windows\WER\ReportArchive"), local.join(r"Microsoft\Windows\WER\ReportQueue")],
            min_age_days: 0,
            min_size: 0,
            exts: None,
            exclude_exts: None,
            recursive: true,
            appdata: true,
        });
    }
    if let Some(dl) = &downloads {
        v.push(Rule {
            id: "oldInstallers",
            confidence: Confidence::Manual,
            roots: vec![dl.clone()],
            min_age_days: 90,
            min_size: 0,
            exts: Some(INSTALLERS),
            exclude_exts: None,
            recursive: false,
            appdata: false,
        });
        v.push(Rule {
            id: "oldLargeDownloads",
            confidence: Confidence::Manual,
            roots: vec![dl.clone()],
            min_age_days: 180,
            min_size: 500 * 1024 * 1024,
            exts: None,
            exclude_exts: Some(INSTALLERS),
            recursive: false,
            appdata: false,
        });
    }
    v
}

fn matches(rule: &Rule, item: &fsx::RawItem, now: i64) -> bool {
    if item.is_dir || item.is_link() || item.is_cloud_only() {
        return false;
    }
    let ext = crate::scan::ext_of(&item.name);
    if let Some(list) = rule.exts
        && !ext.as_deref().is_some_and(|e| list.contains(&e))
    {
        return false;
    }
    if let Some(list) = rule.exclude_exts
        && ext.as_deref().is_some_and(|e| list.contains(&e))
    {
        return false;
    }
    item.size >= rule.min_size && now - item.mtime >= rule.min_age_days * DAY
}

fn collect(rule: &Rule, control: &Control, limit: usize) -> Result<(Vec<CleanupFile>, u64, u64)> {
    let now = crate::now_unix();
    let (mut files, mut count, mut bytes) = (Vec::new(), 0u64, 0u64);
    for root in &rule.roots {
        if !root.is_dir() || safety::protected_reason(root, rule.appdata).is_some() {
            continue;
        }
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            control.checkpoint()?;
            let Ok(items) = fsx::list_dir(&dir) else { continue }; // permission denied: skip
            for it in items {
                if it.is_dir && !it.is_link() && rule.recursive {
                    stack.push(dir.join(&it.name));
                } else if matches(rule, &it, now) {
                    count += 1;
                    bytes += it.size;
                    if files.len() < limit {
                        files.push(CleanupFile {
                            path: dir.join(&it.name).to_string_lossy().into_owned(),
                            size: it.size,
                            mtime: it.mtime,
                        });
                    }
                }
            }
        }
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.size));
    Ok((files, count, bytes))
}

/// Read-only analysis of every category.
pub fn analyze(control: &Control) -> Result<Vec<CleanupCategory>> {
    let mut out = Vec::new();
    for rule in rules() {
        let (files, file_count, bytes) = collect(&rule, control, 5_000)?;
        out.push(CleanupCategory {
            id: rule.id.into(),
            confidence: rule.confidence,
            action: Action::Recycle,
            locations: rule.roots.iter().map(|r| r.to_string_lossy().into_owned()).collect(),
            files,
            file_count,
            bytes,
            info_bytes: None,
        });
    }
    let bin = crate::recycle::bin_info();
    out.push(CleanupCategory {
        id: "recycleBin".into(),
        confidence: Confidence::Medium,
        action: Action::EmptyBin,
        locations: vec!["Papelera de reciclaje".into()],
        files: Vec::new(),
        file_count: bin.items,
        bytes: bin.bytes,
        info_bytes: Some(bin.bytes),
    });
    out.push(CleanupCategory {
        id: "windowsComponents".into(),
        confidence: Confidence::Protected,
        action: Action::OpenStorageSettings,
        locations: vec![std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())],
        files: Vec::new(),
        file_count: 0,
        bytes: 0,
        info_bytes: None,
    });
    Ok(out)
}

/// Validates that each path still belongs to the rule (location, age, size,
/// extension) before it may be recycled. Paths outside the rule are refused.
pub fn check_rule_paths(rule_id: &str, paths: &[PathBuf]) -> Result<Vec<crate::recycle::RecycleTarget>> {
    let rule = rules().into_iter().find(|r| r.id == rule_id).ok_or_else(|| Error::Refused("Categoría desconocida".into()))?;
    if rule.confidence == Confidence::Protected {
        return Err(Error::Refused("Categoría protegida".into()));
    }
    let now = crate::now_unix();
    let mut out = Vec::new();
    for p in paths {
        let parent = p.parent().ok_or_else(|| Error::Refused("Ruta no válida".into()))?;
        let in_root = rule
            .roots
            .iter()
            .any(|r| if rule.recursive { safety::is_within(parent, r) } else { safety::path_key(parent) == safety::path_key(r) });
        if !in_root {
            return Err(Error::Refused(format!("{} no pertenece a esta categoría", p.display())));
        }
        let (attrs, tag) = fsx::path_attrs(p).map_err(|_| Error::Refused(format!("Ya no existe: {}", p.display())))?;
        let md = std::fs::symlink_metadata(p)?;
        let item = fsx::RawItem {
            name: p.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            is_dir: md.is_dir(),
            size: md.len(),
            mtime: fsx::mtime_of(&md),
            ctime: 0,
            attrs,
            reparse_tag: tag,
        };
        if !matches(&rule, &item, now) {
            return Err(Error::Refused(format!("{} ya no cumple la regla", p.display())));
        }
        out.push(crate::recycle::RecycleTarget { path: p.clone(), expected: None });
    }
    Ok(out)
}

pub fn rule_allows_appdata(rule_id: &str) -> bool {
    rules().into_iter().any(|r| r.id == rule_id && r.appdata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_alone_is_not_evidence() {
        let rule = Rule {
            id: "t",
            confidence: Confidence::High,
            roots: vec![],
            min_age_days: 7,
            min_size: 0,
            exts: None,
            exclude_exts: None,
            recursive: true,
            appdata: true,
        };
        let now = crate::now_unix();
        let fresh =
            fsx::RawItem { name: "a.tmp".into(), is_dir: false, size: 10, mtime: now, ctime: 0, attrs: 0, reparse_tag: 0 };
        assert!(!matches(&rule, &fresh, now), "recent temp files are kept");
        let old = fsx::RawItem { mtime: now - 30 * DAY, ..fresh.clone() };
        assert!(matches(&rule, &old, now));
        let cloud = fsx::RawItem { attrs: fsx::ATTR_RECALL_ON_DATA_ACCESS, ..old.clone() };
        assert!(!matches(&rule, &cloud, now));
    }

    #[test]
    fn unknown_rule_is_refused() {
        assert!(check_rule_paths("nope", &[]).is_err());
        assert!(check_rule_paths("windowsComponents", &[]).is_err());
    }
}
