// Mirrors of the Rust structs returned by Tauri commands (camelCase).

export type Media = "ssd" | "hdd" | "unknown";
export type DriveKind = "fixed" | "removable" | "network" | "optical" | "ramDisk" | "unknown";
export type Profile = "low" | "balanced" | "max";

export interface Volume {
  root: string;
  letter: string;
  label: string;
  fileSystem: string;
  kind: DriveKind;
  media: Media;
  external: boolean;
  totalBytes: number;
  freeBytes: number;
  volumeId: string | null;
  diskNumber: number | null;
  available: boolean;
}

export interface ScanRequest {
  roots: string[];
  mode: "quick" | "full" | "custom";
  profile: Profile;
  exclusions: string[];
  includeHidden: boolean;
}

export interface ScanProgress {
  scanId: number;
  phase: "enumerating" | "aggregating" | "indexing" | "done" | "cancelled";
  root: string;
  current: string;
  files: number;
  dirs: number;
  bytes: number;
  errors: number;
  elapsedMs: number;
}

export interface EntryView {
  id: number;
  parentId: number;
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  fileCount: number;
  mtime: number;
  newestMtime: number | null;
  attrs: number;
  cloudOnly: boolean;
  isLink: boolean;
}

/** An indexed root with the scan that last walked it. */
export interface IndexedRoot extends EntryView {
  scanId: number | null;
  scannedAt: number | null;
  scanStatus: string | null;
}

export interface Bucket {
  key: string;
  files: number;
  bytes: number;
}

export interface ScanInfo {
  id: number;
  startedAt: number;
  finishedAt: number | null;
  status: string;
  mode: string;
  roots: string[];
  files: number;
  dirs: number;
  bytes: number;
  errors: number;
}

export interface BinInfo {
  items: number;
  bytes: number;
}

export interface Overview {
  lastScan: ScanInfo | null;
  roots: IndexedRoot[];
  categories: Bucket[];
  extensions: Bucket[];
  duplicateGroups: number;
  duplicateBytes: number;
  recycleBin: BinInfo;
}

export interface Findings {
  backups: number;
  backupBytes: number;
  images: number;
  imageBytes: number;
  oldFolders: number;
  oldFolderBytes: number;
}

export interface ArchiveInfo {
  id: number;
  path: string;
  source: string | null;
  origin: "created" | "imported";
  createdAt: number;
  files: number;
  bytes: number;
  archiveBytes: number;
  verified: boolean;
  exists: boolean;
}

export interface CatalogHit {
  archiveId: number;
  archivePath: string;
  relPath: string;
  name: string;
  size: number;
  mtime: number | null;
  blake3: string | null;
}

export interface OldFolder {
  entry: EntryView;
  oldestMtime: number | null;
  monthsIdle: number;
  topExtensions: Bucket[];
  protectedReason: string | null;
  mark: string | null;
  appReason: string | null;
}

export interface DupProgress {
  phase: "grouping" | "identity" | "partial" | "hashing" | "verifying" | "saving" | "done" | "cancelled";
  filesDone: number;
  filesTotal: number;
  bytesDone: number;
  bytesTotal: number;
  elapsedMs: number;
  current: string;
}

export interface DupSummary {
  groups: number;
  files: number;
  redundantBytes: number;
  hardLinksCollapsed: number;
  skipped: [string, string][];
  elapsedMs: number;
  status: string;
}

export interface DupFile {
  path: string;
  mtime: number;
  protectedReason: string | null;
}

export interface DupGroup {
  id: number;
  size: number;
  hash: string;
  verified: boolean;
  runAt: number;
  files: DupFile[];
  redundantBytes: number;
  suggestedKeep: number;
  suggestionReason: string;
}

export interface RecycleResult {
  opId: number;
  status: string;
  recycled: number;
  failed: number;
  bytesSentToBin: number;
  freeBefore: number | null;
  freeAfter: number | null;
  messages: string[];
}

export interface DiskImage {
  id: number;
  name: string;
  path: string;
  folder: string;
  volume: string;
  format: string;
  size: number;
  mtime: number;
  ctime: number;
  cloudOnly: boolean;
  kind: "gameConsole" | "operatingSystem" | "unknown";
  hint: string | null;
  evidence: string | null;
  volumeLabel: string | null;
  related: string[];
  missing: string[];
  setSize: number;
}

export type Confidence = "high" | "medium" | "review";

export interface BackupItem {
  id: number;
  path: string;
  name: string;
  volume: string;
  size: number;
  fileCount: number;
  newestMtime: number | null;
  oldestMtime: number | null;
  year: number | null;
  month: number | null;
  dateSource: "name" | "override" | "content" | "none";
  ambiguous: boolean;
  confidence: Confidence;
  reasons: string[];
  organized: boolean;
  noDate: boolean;
  protectedReason: string | null;
}

export interface BackupOverride {
  year: number | null;
  month: number | null;
  noDate: boolean;
  excluded: boolean;
}

export interface PlanInput {
  path: string;
  year: number | null;
  month: number | null;
  noDate: boolean;
  /** [files, bytes] from the index: the preview does not walk the folder. */
  indexed?: [number, number];
}

export type PlanStatus = "ready" | "renamed" | "alreadyOrganized" | "needsDate" | "refused";

export interface PlanItem {
  src: string;
  dst: string;
  volume: string;
  year: number | null;
  month: number | null;
  files: number;
  bytes: number;
  status: PlanStatus;
  message: string | null;
}

export interface Plan {
  items: PlanItem[];
  totalFiles: number;
  totalBytes: number;
  ready: number;
  warnings: string[];
}

export interface ExecResult {
  opId: number;
  status: string;
  done: number;
  failed: number;
  messages: string[];
}

export interface CompressRequest {
  source: string;
  destDir: string;
  name: string | null;
  level: number;
}

export interface CompressPreview {
  source: string;
  dest: string;
  files: number;
  folders: number;
  bytes: number;
  alreadyCompressedShare: number;
  destFree: number | null;
  enoughSpace: boolean;
  cloudOnly: number;
  links: number;
  destExists: boolean;
  problems: string[];
}

export interface CompressProgress {
  phase: "reading" | "verifying" | "done";
  bytesDone: number;
  bytesTotal: number;
  current: string;
}

export interface CompressResult {
  opId: number;
  archive: string;
  files: number;
  sourceBytes: number;
  archiveBytes: number;
  potentialSaving: number;
  verified: boolean;
  notPreserved: string[];
  catalogId: number;
}

export interface BatchProgress {
  index: number;
  count: number;
  source: string;
  phase: "reading" | "verifying" | "checking" | "recycling";
  bytesDone: number;
  bytesTotal: number;
  current: string;
}

export interface BatchItem {
  source: string;
  result: CompressResult | null;
  error: string | null;
  /** The original folder is now in the Recycle Bin. */
  recycled: boolean;
  keptReason: string | null;
}

export interface BatchResult {
  items: BatchItem[];
  verified: number;
  failed: number;
  recycled: number;
  bytesInBin: number;
  saving: number;
}

export type CleanupConfidence = "high" | "medium" | "manual" | "protected";

export interface CleanupCategory {
  id: string;
  confidence: CleanupConfidence;
  action: "recycle" | "emptyBin" | "openStorageSettings";
  locations: string[];
  files: { path: string; size: number; mtime: number }[];
  fileCount: number;
  bytes: number;
  infoBytes: number | null;
}

export interface OpItem {
  seq: number;
  src: string;
  dst: string | null;
  status: string;
  error: string | null;
}

export interface Operation {
  id: number;
  kind: string;
  createdAt: number;
  finishedAt: number | null;
  status: string;
  summary: Record<string, unknown>;
  bytes: number;
  freeBefore: number | null;
  freeAfter: number | null;
  reversible: boolean;
  error: string | null;
  items: OpItem[];
}

export interface AppInfo {
  version: string;
  dataDir: string;
  dbBytes: number;
  schemaVersion: number;
}

export interface TaskProgressEvent<T = unknown> {
  taskId: string;
  kind: TaskKind;
  data: T;
}

export interface TaskDoneEvent<T = unknown> {
  taskId: string;
  kind: TaskKind;
  ok: boolean;
  cancelled: boolean;
  result: T;
  error: string | null;
}

export type TaskKind = "scan" | "duplicates" | "organize" | "compress";
