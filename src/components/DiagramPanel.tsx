import { useEffect, useRef, useState } from "react";
import type { ClassModel } from "../types";
import { classNameFromNodeId, collectMemberTargets } from "./diagramNodes";
import "./DiagramPanel.css";

type Props = {
  chart: string;
  classes: ClassModel[];
  stale: boolean;
  onSelect: (className: string, line: number) => void;
  onRefresh: () => void;
  onClose: () => void;
};

export function DiagramPanel({ chart, classes, stale, onSelect, onRefresh, onClose }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;

  useEffect(() => {
    let cancelled = false;
    setError(null);
    const id = `idecode-diagram-${Math.random().toString(36).slice(2)}`;
    const models = new Map(classes.map((element) => [element.name, element]));

    void (async () => {
      try {
        const module = await import("mermaid");
        const mermaid = module.default;
        mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: "neutral" });
        const { svg } = await mermaid.render(id, chart);
        if (cancelled || !host.current) return;
        host.current.innerHTML = svg;

        for (const node of Array.from(host.current.querySelectorAll<SVGGElement>("g.node"))) {
          const name = classNameFromNodeId(node.getAttribute("id") ?? "");
          const model = name ? models.get(name) : undefined;
          if (!name || !model) continue;

          node.classList.add("clickable");
          const tooltip = document.createElementNS("http://www.w3.org/2000/svg", "title");
          tooltip.textContent = `Abrir ${name} en el editor`;
          node.insertBefore(tooltip, node.firstChild);
          node.addEventListener("click", () => onSelectRef.current(name, model.line));

          for (const target of collectMemberTargets(node, model)) {
            target.element.classList.add("clickable");
            target.element.addEventListener("click", (event) => {
              event.stopPropagation();
              onSelectRef.current(name, target.line);
            });
          }
        }
      } catch (reason) {
        if (!cancelled) {
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [chart, classes]);

  return (
    <aside className="diagram-panel">
      <div className="diagram-header">
        <span>Diagrama de clases</span>
        {stale && <span className="diagram-stale">Desactualizado</span>}
        {classes.length > 0 && (
          <span className="diagram-hint">
            Haz clic en una clase, atributo o método para abrirlo
          </span>
        )}
        <span className="diagram-actions">
          <button type="button" onClick={onRefresh}>
            Regenerar
          </button>
          <button type="button" onClick={onClose}>
            Cerrar
          </button>
        </span>
      </div>
      {error ? (
        <p className="diagram-error">{error}</p>
      ) : (
        <div className="diagram-host" ref={host} />
      )}
    </aside>
  );
}
