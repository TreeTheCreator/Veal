// Thin wrapper over the read-only filesystem commands (src-tauri/src/fs.rs),
// mirroring db.ts. Both are confined to pinned project roots on the Rust side.
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
}

export const fs = {
  list: (dir: string) => invoke<Entry[]>("fs_list", { dir }),
  read: (path: string) => invoke<FileText>("fs_read", { path }),
};
