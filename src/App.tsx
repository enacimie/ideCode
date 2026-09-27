import { useEffect, useRef, useState } from "react";
import { backend } from "./backend";
import { splitArgs } from "./args";
import { CloseDialog } from "./components/CloseDialog";
import { CodeEditor } from "./components/CodeEditor";
import { DiagramPanel } from "./components/DiagramPanel";
import { FileTree } from "./components/FileTree";
import { OutputPanel, type OutputTab } from "./components/OutputPanel";
import { Toolbar } from "./components/Toolbar";
import type {
  Diagnostic,
  DiagramResult,
  ExampleInfo,
  LanguageInfo,
  Notice,
  OutputLine,
  ProjectSnapshot,
  RevealTarget,
} from "./types";
import "./App.css";

const RUN_TIMEOUT_SECONDS = 30;

function messageOf(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

export default function App() {
  const [project, setProject] = useState<ProjectSnapshot | null>(null);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [dirty, setDirty] = useState<Set<string>>(new Set());
  const [diagnostics, setDiagnostics] = useState<Diagnostic[]>([]);
  const [lines, setLines] = useState<OutputLine[]>([]);
  const [running, setRunning] = useState(false);
  const [outputTab, setOutputTab] = useState<OutputTab>("output");
  const [diagram, setDiagram] = useState<DiagramResult | null>(null);
  const [diagramStale, setDiagramStale] = useState(false);
  const [diagnosticsStale, setDiagnosticsStale] = useState(false);
  const [reveal, setReveal] = useState<RevealTarget | null>(null);
  const [args, setArgs] = useState("");
  const [entry, setEntry] = useState("");
  const [notice, setNotice] = useState<Notice | null>(null);
  const [languages, setLanguages] = useState<LanguageInfo[]>([]);
  const [examples, setExamples] = useState<ExampleInfo[]>([]);
  const [confirmClose, setConfirmClose] = useState(false);
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;
  const saveRef = useRef<() => void>(() => undefined);
  saveRef.current = () => void handleSave();

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        saveRef.current();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    let unwatch: (() => void) | undefined;
    let cancelled = false;
    void backend
      .watchCloseRequests(
        () => dirtyRef.current.size > 0,
        () => setConfirmClose(true),
      )
      .then((stop) => {
        if (cancelled) {
          stop();
        } else {
          unwatch = stop;
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
      unwatch?.();
    };
  }, []);

  useEffect(() => {
    void backend
      .listLanguages()
      .then(setLanguages)
      .catch(() => undefined);
    void backend
      .listExamples()
      .then(setExamples)
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!notice || notice.kind === "error") return;
    const timer = setTimeout(() => setNotice(null), 4000);
    return () => clearTimeout(timer);
  }, [notice]);

  useEffect(() => {
    void backend
      .currentProject()
      .then((snapshot) => {
        if (!snapshot) return;
        setProject(snapshot);
        setActivePath(snapshot.files[0]?.path ?? null);
      })
      .catch(() => undefined);
  }, []);

  const activeFile = project?.files.find((file) => file.path === activePath) ?? null;
  const language = languages.find((candidate) => candidate.id === project?.language);
  const extension = language?.extensions[0] ?? languages[0]?.extensions[0] ?? "";
  const compileLabel = language?.compileLabel ?? "Compilar";
  const entryLabel = language?.entryLabel ?? "Punto de entrada";
  const languageName = language?.name ?? "Java";

  function updateContent(path: string, content: string) {
    setProject((current) =>
      current
        ? {
            ...current,
            files: current.files.map((file) => (file.path === path ? { ...file, content } : file)),
          }
        : current,
    );
    setDirty((current) => new Set(current).add(path));
    setDiagramStale(true);
    setDiagnosticsStale(true);
  }

  async function saveAll() {
    if (!project) return;
    const pending = project.files.filter((file) => dirty.has(file.path));
    if (pending.length === 0) return;
    for (const file of pending) {
      await backend.writeSource(file.path, file.content);
    }
    const saved = pending.map((file) => file.path);
    setDirty((current) => {
      const next = new Set(current);
      for (const path of saved) {
        next.delete(path);
      }
      return next;
    });
  }

  async function handleOpen() {
    try {
      const snapshot = await backend.selectProject();
      if (!snapshot) return;
      setProject(snapshot);
      setActivePath(snapshot.files[0]?.path ?? null);
      setDirty(new Set());
      setDiagnostics([]);
      setDiagnosticsStale(false);
      setLines([]);
      setDiagram(null);
      setDiagramStale(false);
      setReveal(null);
      setNotice(null);
    } catch (error) {
      setNotice({ kind: "error", message: messageOf(error) });
    }
  }

  async function handleExample(example: string) {
    try {
      const snapshot = await backend.loadExample(example);
      setProject(snapshot);
      setActivePath(snapshot.files[0]?.path ?? null);
      setDirty(new Set());
      setDiagnostics([]);
      setDiagnosticsStale(false);
      setLines([]);
      setDiagram(null);
      setDiagramStale(false);
      setReveal(null);
      setNotice(null);
    } catch (error) {
      setNotice({ kind: "error", message: messageOf(error) });
    }
  }

  async function handleSave() {
    try {
      await saveAll();
      setNotice({ kind: "success", message: "Cambios guardados." });
    } catch (error) {
      setNotice({ kind: "error", message: messageOf(error) });
    }
  }

  async function handleCloseWithSave() {
    setConfirmClose(false);
    try {
      await saveAll();
      await backend.closeWindow();
    } catch (error) {
      setNotice({ kind: "error", message: messageOf(error) });
    }
  }

  async function handleCloseWithoutSave() {
    setConfirmClose(false);
    try {
      await backend.closeWindow();
    } catch (error) {
      setNotice({ kind: "error", message: messageOf(error) });
    }
  }

  async function handleCompile() {
    if (!project) return;
    setNotice(null);
    try {
      await saveAll();
      const result = await backend.compile();
      setDiagnostics(result.diagnostics);
      setDiagnosticsStale(false);
      if (result.success) {
        setOutputTab("output");
        setNotice({ kind: "success", message: "Compilación correcta." });
      } else {
        setOutputTab("problems");
        setNotice({ kind: "error", message: "La compilación tiene errores." });
      }
    } catch (error) {
      setNotice({ kind: "error", message: messageOf(error) });
    }
  }

  async function handleRun() {
    if (!project) return;
    setNotice(null);
    setLines([]);
    setOutputTab("output");
    setRunning(true);
    try {
      await saveAll();
      const outcome = await backend.run(entry.trim() || null, splitArgs(args), (chunk) => {
        setLines((current) => [...current, chunk]);
      });
      if (outcome.timed_out) {
        setNotice({
          kind: "error",
          message: `El programa superó ${RUN_TIMEOUT_SECONDS} s y se detuvo.`,
        });
      } else if (outcome.exit_code === 0) {
        setNotice({ kind: "success", message: "El programa terminó correctamente." });
      } else {
        setNotice({
          kind: "error",
          message: `El programa terminó con código ${outcome.exit_code ?? "desconocido"}.`,
        });
      }
    } catch (error) {
      setNotice({ kind: "error", message: messageOf(error) });
    } finally {
      setRunning(false);
    }
  }

  async function handleDiagram() {
    if (!project) return;
    try {
      await saveAll();
      const result = await backend.classDiagram();
      setDiagram(result);
      setDiagramStale(false);
    } catch (error) {
      setNotice({ kind: "error", message: messageOf(error) });
    }
  }

  function handleSelect(className: string, line: number) {
    if (!project || !diagram) return;
    const model = diagram.classes.find((candidate) => candidate.name === className);
    if (!model) return;
    const file = project.files.find((candidate) => candidate.relative === model.file);
    if (!file) return;
    setActivePath(file.path);
    setReveal({ path: file.path, line, nonce: Date.now() });
  }

  async function handleCreate(name: string) {
    try {
      const file = await backend.createSource(name);
      setProject((current) =>
        current
          ? {
              ...current,
              files: [...current.files, file].sort((a, b) => a.relative.localeCompare(b.relative)),
            }
          : current,
      );
      setActivePath(file.path);
      setDiagramStale(true);
    } catch (error) {
      setNotice({ kind: "error", message: messageOf(error) });
    }
  }

  return (
    <div className="app">
      <Toolbar
        project={project}
        examples={examples}
        running={running}
        compileLabel={compileLabel}
        entryLabel={entryLabel}
        args={args}
        entry={entry}
        onOpen={() => void handleOpen()}
        onExample={(example) => void handleExample(example)}
        onSave={() => void handleSave()}
        onCompile={() => void handleCompile()}
        onRun={() => void handleRun()}
        onDiagram={() => void handleDiagram()}
        onArgsChange={setArgs}
        onEntryChange={setEntry}
      />

      <div className="workspace">
        <FileTree
          files={project?.files ?? []}
          activePath={activePath}
          dirty={dirty}
          extension={extension}
          onSelect={setActivePath}
          onCreate={(name) => void handleCreate(name)}
        />

        <main className="center">
          {activeFile ? (
            <CodeEditor
              key={activeFile.path}
              path={activeFile.path}
              name={activeFile.name}
              value={activeFile.content}
              reveal={reveal}
              onChange={(value) => updateContent(activeFile.path, value)}
            />
          ) : (
            <div className="empty">
              <p>Abre una carpeta con tus archivos {languageName} para empezar.</p>
              <div className="empty-actions">
                <button type="button" className="primary" onClick={() => void handleOpen()}>
                  Abrir carpeta
                </button>
                <button
                  type="button"
                  onClick={() => void handleExample(examples[0].id)}
                  disabled={examples.length === 0}
                >
                  Cargar ejemplo
                </button>
              </div>
            </div>
          )}

          <OutputPanel
            diagnostics={diagnostics}
            diagnosticsStale={diagnosticsStale}
            lines={lines}
            running={running}
            tab={outputTab}
            onTabChange={setOutputTab}
          />
        </main>

        {diagram && (
          <DiagramPanel
            chart={diagram.mermaid}
            classes={diagram.classes}
            stale={diagramStale}
            onSelect={handleSelect}
            onRefresh={() => void handleDiagram()}
            onClose={() => setDiagram(null)}
          />
        )}
      </div>

      {notice && <div className={`notice ${notice.kind}`}>{notice.message}</div>}

      {confirmClose && (
        <CloseDialog
          onSave={() => void handleCloseWithSave()}
          onDiscard={() => void handleCloseWithoutSave()}
          onCancel={() => setConfirmClose(false)}
        />
      )}
    </div>
  );
}
