import type {
  BuildResult,
  ClassModel,
  DiagramResult,
  ExampleInfo,
  GeneratedFile,
  LanguageInfo,
  ProjectSnapshot,
  RunChunk,
  RunOutcome,
  SourceEntry,
  WriteOutcome,
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
  generateCode(classes: ClassModel[], language: string): Promise<GeneratedFile[]>;
  writeGeneratedFiles(files: GeneratedFile[], overwrite: boolean): Promise<WriteOutcome>;
  watchCloseRequests(isDirty: () => boolean, onRequest: () => void): Promise<() => void>;
  closeWindow(): Promise<void>;
};
