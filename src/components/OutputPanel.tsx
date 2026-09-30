import { useEffect, useRef } from "react";
import type { Diagnostic, OutputLine } from "../types";
import "./OutputPanel.css";

export type OutputTab = "output" | "problems";

type Props = {
  diagnostics: Diagnostic[];
  diagnosticsStale: boolean;
  lines: OutputLine[];
  running: boolean;
  tab: OutputTab;
  onTabChange: (tab: OutputTab) => void;
};

export function OutputPanel({
  diagnostics,
  diagnosticsStale,
  lines,
  running,
  tab,
  onTabChange,
}: Props) {
  const errors = diagnostics.filter((diagnostic) => diagnostic.severity === "error").length;
  const problemsLabel =
    diagnostics.length === 0 ? "Problemas" : `Problemas (${errors}/${diagnostics.length})`;
  const body = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = body.current;
    if (!element) return;
    element.scrollTop = element.scrollHeight;
  }, [lines, diagnostics, tab]);

  return (
    <section className="output-panel">
      <div className="output-tabs">
        <button
          type="button"
          className={tab === "output" ? "active" : ""}
          onClick={() => onTabChange("output")}
        >
          Salida
        </button>
        <button
          type="button"
          className={tab === "problems" ? "active" : ""}
          onClick={() => onTabChange("problems")}
        >
          {problemsLabel}
          {diagnosticsStale && diagnostics.length > 0 && (
            <span className="stale-dot" title="Has editado desde la última comprobación" />
          )}
        </button>
        {running && <span className="running-badge">ejecutando…</span>}
      </div>

      <div className="output-body" ref={body} aria-live="polite">
        {tab === "output" &&
          (lines.length === 0 ? (
            <p className="muted">La salida del programa aparecerá aquí.</p>
          ) : (
            lines.map((line, index) => (
              <pre key={index} className={line.stream}>
                {line.line}
              </pre>
            ))
          ))}

        {tab === "problems" &&
          (diagnostics.length === 0 ? (
            <p className="muted">Sin problemas.</p>
          ) : (
            diagnostics.map((diagnostic, index) => (
              <pre key={index} className={`diagnostic ${diagnostic.severity}`}>
                {diagnostic.file
                  ? `${diagnostic.file}${diagnostic.line ? `:${diagnostic.line}` : ""}: `
                  : ""}
                {diagnostic.message}
              </pre>
            ))
          ))}
      </div>
    </section>
  );
}
