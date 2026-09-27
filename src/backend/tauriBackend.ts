import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  BuildResult,
  DiagramResult,
  ExampleInfo,
  LanguageInfo,
  ProjectSnapshot,
  RunChunk,
  RunOutcome,
  SourceEntry,
} from "../types";
import type { Backend } from "./types";

export const tauriBackend: Backend = {
  listLanguages(): Promise<LanguageInfo[]> {
    return invoke<LanguageInfo[]>("list_languages");
  },

  listExamples(): Promise<ExampleInfo[]> {
    return invoke<ExampleInfo[]>("list_examples");
  },

  selectProject(): Promise<ProjectSnapshot | null> {
    return invoke<ProjectSnapshot | null>("select_project");
  },

  currentProject(): Promise<ProjectSnapshot | null> {
    return invoke<ProjectSnapshot | null>("current_project");
  },

  loadExample(example: string): Promise<ProjectSnapshot> {
    return invoke<ProjectSnapshot>("load_example", { example });
  },

  readSource(path: string): Promise<string> {
    return invoke<string>("read_source", { path });
  },

  writeSource(path: string, content: string): Promise<void> {
    return invoke<void>("write_source", { path, content });
  },

  createSource(name: string): Promise<SourceEntry> {
    return invoke<SourceEntry>("create_source", { name });
  },

  compile(): Promise<BuildResult> {
    return invoke<BuildResult>("compile_project");
  },

  async run(
    entry: string | null,
    args: string[],
    onChunk: (chunk: RunChunk) => void,
  ): Promise<RunOutcome> {
    const unlisten = await listen<RunChunk>("run://output", (event) => onChunk(event.payload));
    try {
      return await invoke<RunOutcome>("run_project", { entry, args });
    } finally {
      unlisten();
    }
  },

  classDiagram(): Promise<DiagramResult> {
    return invoke<DiagramResult>("class_diagram");
  },

  async watchCloseRequests(_isDirty: () => boolean, onRequest: () => void): Promise<() => void> {
    return await listen("app://close-requested", () => onRequest());
  },

  async closeWindow(): Promise<void> {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().destroy();
  },
};
