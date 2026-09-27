import { isTauri } from "@tauri-apps/api/core";
import { tauriBackend } from "./tauriBackend";
import { webBackend } from "./webBackend";
import type { Backend } from "./types";

export const backend: Backend = isTauri() ? tauriBackend : webBackend;

export type { Backend } from "./types";
