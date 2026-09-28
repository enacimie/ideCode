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
import type { Backend } from "./types";

const modules = import.meta.glob("../../examples/*/*.{java,py,kt}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const DESKTOP_ONLY =
  "Esta función solo está disponible en la aplicación de escritorio. Arranca con «pnpm tauri dev».";

function languageOf(id: string): string {
  if (id.endsWith("_python")) return "python";
  if (id.endsWith("_kotlin")) return "kotlin";
  return "java";
}

type Demo = {
  id: string;
  name: string;
  files: SourceEntry[];
};

function exampleName(id: string): string {
  return id
    .split(/[-_]/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

const demos: Demo[] = (() => {
  const grouped = new Map<string, SourceEntry[]>();
  for (const [path, content] of Object.entries(modules)) {
    const parts = path.split("/");
    const name = parts.pop() ?? path;
    const id = parts.pop();
    if (!id) continue;
    const files = grouped.get(id) ?? [];
    files.push({ path: name, relative: name, name, content });
    grouped.set(id, files);
  }

  return Array.from(grouped.entries())
    .map(([id, files]) => ({
      id,
      name: exampleName(id),
      files: files.sort((a, b) => a.relative.localeCompare(b.relative)),
    }))
    .sort((a, b) => a.name.localeCompare(b.name));
})();

function projectFor(example: string): ProjectSnapshot | null {
  const demo = demos.find((candidate) => candidate.id === example);
  if (!demo) return null;
  return {
    root: `(demostración web · ${demo.name})`,
    language: languageOf(demo.id),
    files: demo.files,
  };
}

const EMPTY: ProjectSnapshot = { root: "(demostración web)", language: "java", files: [] };

let current: ProjectSnapshot = projectFor(demos[0]?.id ?? "") ?? EMPTY;

export const webBackend: Backend = {
  async listLanguages(): Promise<LanguageInfo[]> {
    return [
      {
        id: "java",
        name: "Java",
        extensions: ["java"],
        compileLabel: "Compilar",
        entryLabel: "Clase principal",
      },
      {
        id: "python",
        name: "Python",
        extensions: ["py"],
        compileLabel: "Comprobar sintaxis",
        entryLabel: "Módulo principal",
      },
      {
        id: "kotlin",
        name: "Kotlin",
        extensions: ["kt"],
        compileLabel: "Compilar",
        entryLabel: "Archivo principal",
      },
    ];
  },

  async listExamples(): Promise<ExampleInfo[]> {
    return demos.map((demo) => ({
      id: demo.id,
      name: demo.name,
      files: demo.files.length,
    }));
  },

  async selectProject(): Promise<ProjectSnapshot | null> {
    return current;
  },

  async currentProject(): Promise<ProjectSnapshot | null> {
    return current;
  },

  async loadExample(example: string): Promise<ProjectSnapshot> {
    const next = projectFor(example);
    if (!next) {
      throw new Error(`No existe el ejemplo «${example}».`);
    }
    current = next;
    return next;
  },

  async readSource(path: string): Promise<string> {
    return current.files.find((file) => file.path === path)?.content ?? "";
  },

  async writeSource(path: string, content: string): Promise<void> {
    const file = current.files.find((candidate) => candidate.path === path);
    if (file) {
      file.content = content;
    }
  },

  async createSource(name: string): Promise<SourceEntry> {
    const lower = name.toLowerCase();
    const stem = name.replace(/\.(java|py|kt)$/i, "");
    let content: string;
    if (lower.endsWith(".py")) {
      content = `def ${stem}():\n    pass\n`;
    } else if (lower.endsWith(".kt")) {
      content = `class ${stem} {\n}\n`;
    } else {
      content = `public class ${stem} {\n}\n`;
    }
    const entry: SourceEntry = { path: name, relative: name, name, content };
    current.files.push(entry);
    return entry;
  },

  async compile(): Promise<BuildResult> {
    return {
      success: false,
      diagnostics: [
        {
          severity: "error",
          message: DESKTOP_ONLY,
          file: null,
          line: null,
          column: null,
        },
      ],
    };
  },

  async run(
    _entry: string | null,
    _args: string[],
    onChunk: (chunk: RunChunk) => void,
  ): Promise<RunOutcome> {
    onChunk({ stream: "stderr", line: DESKTOP_ONLY });
    return { exit_code: null, timed_out: false };
  },

  async classDiagram(): Promise<DiagramResult> {
    throw new Error(DESKTOP_ONLY);
  },

  async generateCode(_classes: ClassModel[], _language: string): Promise<GeneratedFile[]> {
    throw new Error(DESKTOP_ONLY);
  },

  async writeGeneratedFiles(_files: GeneratedFile[], _overwrite: boolean): Promise<WriteOutcome> {
    throw new Error(DESKTOP_ONLY);
  },

  async watchCloseRequests(isDirty: () => boolean, _onRequest: () => void): Promise<() => void> {
    const handler = (event: BeforeUnloadEvent) => {
      if (isDirty()) {
        event.preventDefault();
      }
    };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  },

  async closeWindow(): Promise<void> {
    window.close();
  },
};
