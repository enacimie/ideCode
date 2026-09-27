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

export type Backend = {
  listLanguages(): Promise<LanguageInfo[]>;
  listExamples(): Promise<ExampleInfo[]>;
  selectProject(): Promise<ProjectSnapshot | null>;
  currentProject(): Promise<ProjectSnapshot | null>;
  loadExample(example: string): Promise<ProjectSnapshot>;
  readSource(path: string): Promise<string>;
  writeSource(path: string, content: string): Promise<void>;
  createSource(name: string): Promise<SourceEntry>;
  compile(): Promise<BuildResult>;
  run(
    entry: string | null,
    args: string[],
    onChunk: (chunk: RunChunk) => void,
  ): Promise<RunOutcome>;
  classDiagram(): Promise<DiagramResult>;
  watchCloseRequests(isDirty: () => boolean, onRequest: () => void): Promise<() => void>;
  closeWindow(): Promise<void>;
};
