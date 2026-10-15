// Typed wrappers around Tauri commands. The backend re-validates every
// mutating request; these wrappers only shape the calls.
import { invoke } from "@tauri-apps/api/core";
import type * as T from "./types";

export const api = {
  listVolumes: () => invoke<T.Volume[]>("list_volumes"),
  startScan: (request: T.ScanRequest) => invoke<string>("start_scan", { request }),
  cancelTask: (taskId: string) => invoke<void>("cancel_task", { taskId }),
  pauseTask: (taskId: string, paused: boolean) => invoke<void>("pause_task", { taskId, paused }),
  overview: (rootName: string) => invoke<T.Overview>("overview", { rootName }),
  children: (parentId: number, offset: number, limit: number) =>
    invoke<[T.EntryView[], number]>("children", { parentId, offset, limit }),
  largest: (limit: number) => invoke<[T.EntryView[], T.EntryView[]]>("largest", { limit }),
  search: (needle: string, limit: number) => invoke<T.EntryView[]>("search", { needle, limit }),
  subtreeExtensions: (id: number) => invoke<T.Bucket[]>("subtree_extensions", { id }),
  scanErrors: (scanId: number) => invoke<{ path: string; message: string }[]>("scan_errors", { scanId }),
  oldFolders: (months: number, minBytes: number) => invoke<T.OldFolder[]>("old_folders", { months, minBytes }),
  setMark: (path: string, mark: string | null) => invoke<void>("set_mark", { path, mark }),
  startDuplicates: (request: { minSize: number; scopes: string[]; profile: T.Profile }) =>
    invoke<string>("start_duplicates", { request }),
  duplicateGroups: (offset: number, limit: number) =>
    invoke<[T.DupGroup[], number]>("duplicate_groups", { offset, limit }),
  recycleDuplicates: (groupId: number, keep: string, remove: string[]) =>
    invoke<T.RecycleResult>("recycle_duplicates", { groupId, keep, remove }),
  exportDuplicates: (path: string, anonymize: boolean) => invoke<void>("export_duplicates", { path, anonymize }),
  diskImages: (readHeaders: boolean) => invoke<T.DiskImage[]>("disk_images", { readHeaders }),
  imageOrphans: (selection: string[]) => invoke<string[]>("image_orphans", { selection }),
  recycleImages: (targets: { path: string; expected: { size: number; mtime: number } | null }[]) =>
    invoke<T.RecycleResult>("recycle_images", { targets }),
  backupsList: (rootName: string) => invoke<T.BackupItem[]>("backups_list", { rootName }),
  backupOverride: (path: string, value: T.BackupOverride) => invoke<void>("backup_override", { path, value }),
  organizePlan: (request: {
    items: T.PlanInput[];
    rootName: string;
    parents: Record<string, string>;
    layout: "yearMonth" | "year";
  }) => invoke<T.Plan>("organize_plan", { request }),
  organizeExecute: (items: T.PlanItem[]) => invoke<string>("organize_execute", { items }),
  organizeUndo: (opId: number) => invoke<T.ExecResult>("organize_undo", { opId }),
  compressPreview: (request: T.CompressRequest) => invoke<T.CompressPreview>("compress_preview", { request }),
  startCompress: (request: T.CompressRequest) => invoke<string>("start_compress", { request }),
  cleanupAnalyze: () => invoke<T.CleanupCategory[]>("cleanup_analyze"),
  cleanupRecycle: (ruleId: string, paths: string[]) => invoke<T.RecycleResult>("cleanup_recycle", { ruleId, paths }),
  emptyRecycleBin: () => invoke<T.BinInfo>("empty_recycle_bin"),
  history: (limit: number) => invoke<T.Operation[]>("history", { limit }),
  getSettings: () => invoke<unknown>("get_settings"),
  setSettings: (value: unknown) => invoke<void>("set_settings", { value }),
  resetIndex: () => invoke<void>("reset_index"),
  appInfo: () => invoke<T.AppInfo>("app_info"),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  openStorageSettings: () => invoke<void>("open_storage_settings"),
};

/** Error text from a rejected invoke (the backend sends plain strings). */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
