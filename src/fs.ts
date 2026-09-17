// Thin wrapper over the filesystem commands (src-tauri/src/fs.rs), mirroring
// db.ts. All are confined to pinned project roots on the Rust side.
import { invoke } from "@tauri-apps/api/core";

export type EntryKind = "dir" | "file";

export interface Entry {
  name: string;
  kind: EntryKind;
  size: number;
}

export interface FileText {
  text: string;
  size: number;
  /** Modification time, ms since the epoch; a save must present it back. */
  mtime: number;
  /** A UTF-8 BOM was stripped; the save restores it. */
  bom: boolean;
}

export interface Written {
  size: number;
  mtime: number;
}

export interface WriteOptions {
  crlf: boolean;
  bom: boolean;
  /** `null` skips the conflict check (Overwrite). */
  expectedMtime: number | null;
}

export const fs = {
  list: (dir: string) => invoke<Entry[]>("fs_list", { dir }),
  read: (path: string) => invoke<FileText>("fs_read", { path }),
  write: (path: string, text: string, o: WriteOptions) =>
    invoke<Written>("fs_write", { path, text, crlf: o.crlf, bom: o.bom, expectedMtime: o.expectedMtime }),
};
