// Backup and restore commands (src-tauri/src/backup.rs): the Data menu, Alt+Y.
import { invoke } from "@tauri-apps/api/core";
import type { CompanyInfo } from "./types";

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
  }
}

/** A company's backup settings on this computer. */
export interface BackupPrefs {
  /** null: the default folder. */
  folder: string | null;
  automatic: boolean;
  on_close: boolean;
  daily: boolean;
  keep: number;
}

export interface LastBackup {
  at: string;
  path: string;
  automatic: boolean;
  encrypted: boolean;
  size: number;
  entries: number;
  last_hash: string | null;
  records_rev: number;
  /** "manual", "close", "daily", or the risky action it came before. */
  reason: string;
}

export interface BackupStatus {
  prefs: BackupPrefs;
  folder: string;
  default_folder: string;
  last: LastBackup | null;
  changed_since: boolean;
  /** No backup for 7 days (or never) while the books changed. */
  remind: boolean;
  can_change: boolean;
}

export interface BackupSummary {
  company: string;
  company_id: string | null;
  entries: number;
  last_hash: string | null;
  records: number;
  vouchers: number;
  first_voucher: string | null;
  last_voucher: string | null;
  made_at: string;
  automatic: boolean;
  encrypted: boolean;
  app_version: string;
}

export interface BackupDone {
  path: string;
  file: string;
  folder: string;
  size: number;
  summary: BackupSummary;
  deleted: string[];
}

export type Relation = "no_company_open" | "other_company" | "same" | "older" | "newer" | "diverged";

export interface RestorePreview {
  summary: BackupSummary;
  file: string;
  size: number;
  relation: Relation;
  later_entries: number;
  can_replace: boolean;
  open_company: string | null;
}

export interface Restored {
  path: string;
  /** The replaced company, reopened; null for a new company file. */
  info: CompanyInfo | null;
  kept: string | null;
  safety_backup: string | null;
}

export const backupApi = {
  status: () => call<BackupStatus>("backup_status"),
  setPrefs: (prefs: BackupPrefs) => call<BackupStatus>("backup_set_prefs", { prefs }),
  /** `path`: a file picked in the save dialog (phones); null: the backup folder. */
  now: (path: string | null, password: string | null) => call<BackupDone>("backup_now", { path, password }),
  encrypted: (path: string) => call<boolean>("backup_encrypted", { path }),
  inspect: (path: string, password: string | null) => call<RestorePreview>("backup_inspect", { path, password }),
  restore: (path: string, password: string | null, mode: "new" | "replace", confirm: string | null) =>
    call<Restored>("backup_restore", { path, password, mode, confirm }),
};
