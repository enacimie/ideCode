import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { backend } from "../backend";
import {
  RELATION_LABELS,
  buildMermaid,
  createField,
  createMethod,
  createNode,
  emptyDesign,
  formatParams,
  loadDesign,
  newId,
  parseParams,
  saveDesign,
  toClassModels,
  validateDesign,
  visibilityMarker,
  type DesignerEdge,
  type DesignerField,
  type DesignerMethod,
  type DesignerNode,
  type DesignerState,
  type RelationKind,
} from "../designer/model";
import type { GeneratedFile, ProjectSnapshot, SourceEntry, Visibility } from "../types";
import "./ClassDesigner.css";

const NODE_WIDTH = 220;
const HEADER_HEIGHT = 26;
const LINE_HEIGHT = 18;
const CANVAS_WIDTH = 2200;
const CANVAS_HEIGHT = 1500;
const RELATIONS: RelationKind[] = [
  "extends",
  "implements",
  "association",
  "aggregation",
  "composition",
  "dependency",
];
const VISIBILITIES: Visibility[] = ["public", "protected", "package", "private"];
const VISIBILITY_LABELS: Record<Visibility, string> = {
  public: "público",
  protected: "protegido",
  package: "paquete",
  private: "privado",
};

function messageOf(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

function nodeHeight(node: DesignerNode): number {
  const fields = node.fields.filter((field) => field.name.trim()).length;
  const methods = node.methods.filter((method) => method.name.trim()).length;
  const separators = (fields > 0 ? 1 : 0) + (methods > 0 ? 1 : 0);
  return HEADER_HEIGHT + (fields + methods) * LINE_HEIGHT + separators * 6 + 8;
}

function clipToRect(
  centerX: number,
  centerY: number,
  width: number,
  height: number,
  fromX: number,
  fromY: number,
): { x: number; y: number } {
  const dx = fromX - centerX;
  const dy = fromY - centerY;
  if (dx === 0 && dy === 0) {
    return { x: centerX, y: centerY };
  }
  const scaleX = dx !== 0 ? width / 2 / Math.abs(dx) : Number.POSITIVE_INFINITY;
  const scaleY = dy !== 0 ? height / 2 / Math.abs(dy) : Number.POSITIVE_INFINITY;
  const scale = Math.min(scaleX, scaleY);
  return { x: centerX + dx * scale, y: centerY + dy * scale };
}

type Props = {
  project: ProjectSnapshot;
  onWritten: (entries: SourceEntry[]) => void;
  onClose: () => void;
};

type Selection = { type: "node" | "edge"; id: string } | null;

export function ClassDesigner({ project, onWritten, onClose }: Props) {
  const [design, setDesign] = useState<DesignerState>(() => loadDesign() ?? emptyDesign());
  const [selection, setSelection] = useState<Selection>(null);
  const [language, setLanguage] = useState(
    project.language === "kotlin" || project.language === "python" ? project.language : "java",
  );
  const [tab, setTab] = useState<"diagrama" | "codigo">("diagrama");
  const [files, setFiles] = useState<GeneratedFile[] | null>(null);
  const [codeError, setCodeError] = useState<string | null>(null);
  const [mermaidError, setMermaidError] = useState<string | null>(null);
  const [connectMode, setConnectMode] = useState(false);
  const [connectFrom, setConnectFrom] = useState<string | null>(null);
  const [relationKind, setRelationKind] = useState<RelationKind>("extends");
  const [conflicts, setConflicts] = useState<string[] | null>(null);
  const [message, setMessage] = useState<{ kind: "success" | "error"; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const svgRef = useRef<SVGSVGElement>(null);
  const previewRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ id: string; offsetX: number; offsetY: number } | null>(null);

  const problems = useMemo(() => validateDesign(design), [design]);
  const selectedNode =
    selection?.type === "node"
      ? (design.nodes.find((node) => node.id === selection.id) ?? null)
      : null;
  const selectedEdge =
    selection?.type === "edge"
      ? (design.edges.find((edge) => edge.id === selection.id) ?? null)
      : null;
  const nameOf = (id: string) => design.nodes.find((node) => node.id === id)?.name ?? "?";

  useEffect(() => {
    const timer = setTimeout(() => saveDesign(design), 400);
    return () => clearTimeout(timer);
  }, [design]);

  useEffect(() => {
    if (tab !== "diagrama") return;
    let cancelled = false;
    setMermaidError(null);
    const id = `idecode-designer-${Math.random().toString(36).slice(2)}`;
    void (async () => {
      try {
        const module = await import("mermaid");
        const mermaid = module.default;
        mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: "neutral" });
        const { svg } = await mermaid.render(id, buildMermaid(design));
        if (cancelled || !previewRef.current) return;
        previewRef.current.innerHTML = svg;
      } catch (reason) {
        if (!cancelled) {
          setMermaidError(messageOf(reason));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [tab, design]);

  useEffect(() => {
    if (tab !== "codigo") return;
    if (problems.length > 0 || design.nodes.length === 0) {
      setFiles(null);
      setCodeError(null);
      return;
    }
    const timer = setTimeout(() => {
      void backend
        .generateCode(toClassModels(design), language)
        .then((generated) => {
          setFiles(generated);
          setCodeError(null);
        })
        .catch((reason: unknown) => {
          setFiles(null);
          setCodeError(messageOf(reason));
        });
    }, 250);
    return () => clearTimeout(timer);
  }, [tab, design, language, problems]);

  function deleteSelection() {
    if (!selection) return;
    setDesign((current) => {
      if (selection.type === "node") {
        return {
          nodes: current.nodes.filter((node) => node.id !== selection.id),
          edges: current.edges.filter(
            (edge) => edge.from !== selection.id && edge.to !== selection.id,
          ),
        };
      }
      return { ...current, edges: current.edges.filter((edge) => edge.id !== selection.id) };
    });
    setSelection(null);
  }

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const target = event.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
      if (event.key === "Escape") {
        setConnectMode(false);
        setConnectFrom(null);
        return;
      }
      if ((event.key === "Delete" || event.key === "Backspace") && selection) {
        event.preventDefault();
        deleteSelection();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  function toCanvasPoint(event: ReactPointerEvent) {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) {
      return { x: 0, y: 0 };
    }
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  }

  function handleConnectClick(nodeId: string) {
    if (!connectFrom) {
      setConnectFrom(nodeId);
      return;
    }
    if (connectFrom === nodeId) {
      setConnectFrom(null);
      return;
    }
    const exists = design.edges.some(
      (edge) => edge.from === connectFrom && edge.to === nodeId && edge.kind === relationKind,
    );
    if (!exists) {
      const edge: DesignerEdge = {
        id: newId("rel"),
        from: connectFrom,
        to: nodeId,
        kind: relationKind,
        label: "",
      };
      setDesign((current) => ({ ...current, edges: [...current.edges, edge] }));
      setSelection({ type: "edge", id: edge.id });
    }
    setConnectFrom(null);
  }

  function handleNodePointerDown(event: ReactPointerEvent, node: DesignerNode) {
    event.stopPropagation();
    if (connectMode) {
      handleConnectClick(node.id);
      return;
    }
    const point = toCanvasPoint(event);
    dragRef.current = { id: node.id, offsetX: point.x - node.x, offsetY: point.y - node.y };
    setSelection({ type: "node", id: node.id });
  }

  function handlePointerMove(event: ReactPointerEvent) {
    const drag = dragRef.current;
    if (!drag) return;
    const point = toCanvasPoint(event);
    const x = Math.max(0, Math.min(CANVAS_WIDTH - NODE_WIDTH, point.x - drag.offsetX));
    const y = Math.max(0, Math.min(CANVAS_HEIGHT - 80, point.y - drag.offsetY));
    setDesign((current) => ({
      ...current,
      nodes: current.nodes.map((node) => (node.id === drag.id ? { ...node, x, y } : node)),
    }));
  }

  function addNode(kind: "class" | "interface") {
    const node = createNode(design, kind);
    setDesign((current) => ({ ...current, nodes: [...current.nodes, node] }));
    setSelection({ type: "node", id: node.id });
  }

  function updateNode(id: string, patch: Partial<DesignerNode>) {
    setDesign((current) => ({
      ...current,
      nodes: current.nodes.map((node) => (node.id === id ? { ...node, ...patch } : node)),
    }));
  }

  function updateField(nodeId: string, fieldId: string, patch: Partial<DesignerField>) {
    setDesign((current) => ({
      ...current,
      nodes: current.nodes.map((node) =>
        node.id === nodeId
          ? {
              ...node,
              fields: node.fields.map((field) =>
                field.id === fieldId ? { ...field, ...patch } : field,
              ),
            }
          : node,
      ),
    }));
  }

  function updateMethod(nodeId: string, methodId: string, patch: Partial<DesignerMethod>) {
    setDesign((current) => ({
      ...current,
      nodes: current.nodes.map((node) =>
        node.id === nodeId
          ? {
              ...node,
              methods: node.methods.map((method) =>
                method.id === methodId ? { ...method, ...patch } : method,
              ),
            }
          : node,
      ),
    }));
  }

  async function handleWrite(overwrite: boolean) {
    setBusy(true);
    setMessage(null);
    try {
      const generated = await backend.generateCode(toClassModels(design), language);
      setFiles(generated);
      const outcome = await backend.writeGeneratedFiles(generated, overwrite);
      if (outcome.conflicts.length > 0) {
        setConflicts(outcome.conflicts);
        return;
      }
      setConflicts(null);
      setMessage({
        kind: "success",
        text: `${outcome.written.length === 1 ? "Archivo generado" : `${outcome.written.length} archivos generados`} en la carpeta del proyecto.`,
      });
      onWritten(outcome.written);
    } catch (reason) {
      setMessage({ kind: "error", text: messageOf(reason) });
    } finally {
      setBusy(false);
    }
  }

  function renderNode(node: DesignerNode) {
    const height = nodeHeight(node);
    const fields = node.fields.filter((field) => field.name.trim());
    const methods = node.methods.filter((method) => method.name.trim());
    const isSelected = selection?.type === "node" && selection.id === node.id;
    const isConnectSource = connectFrom === node.id;
    const classes = ["designer-node"];
    if (isSelected) classes.push("selected");
    if (isConnectSource) classes.push("connect-source");

    let baseline = HEADER_HEIGHT + 6 + LINE_HEIGHT - 5;
    const fieldLines = fields.map((field) => {
      const text = `${visibilityMarker(field.visibility)}${field.ty.trim() ? `${field.ty.trim()} ` : ""}${field.name.trim()}${field.isStatic ? " $" : ""}${field.isFinal ? " (final)" : ""}`;
      const y = baseline;
      baseline += LINE_HEIGHT;
      return (
        <text key={field.id} className="node-member" x={8} y={y}>
          {text}
        </text>
      );
    });
    if (methods.length > 0) {
      baseline += 6;
    }
    const methodLines = methods.map((method) => {
      const params = method.params
        .filter((param) => param.name.trim())
        .map((param) =>
          param.ty.trim() ? `${param.ty.trim()} ${param.name.trim()}` : param.name.trim(),
        )
        .join(", ");
      const text = `${visibilityMarker(method.visibility)}${method.name.trim()}(${params})${method.returnTy.trim() ? ` ${method.returnTy.trim()}` : ""}${method.isStatic ? " $" : method.isAbstract ? " *" : ""}`;
      const y = baseline;
      baseline += LINE_HEIGHT;
      return (
        <text key={method.id} className="node-member" x={8} y={y}>
          {text}
        </text>
      );
    });

    return (
      <g
        key={node.id}
        transform={`translate(${node.x}, ${node.y})`}
        onPointerDown={(event) => handleNodePointerDown(event, node)}
        style={{ cursor: connectMode ? "crosshair" : "grab" }}
      >
        <rect className={classes.join(" ")} width={NODE_WIDTH} height={height} rx={6} />
        <line
          className="node-separator"
          x1={0}
          y1={HEADER_HEIGHT}
          x2={NODE_WIDTH}
          y2={HEADER_HEIGHT}
        />
        {fields.length > 0 && methods.length > 0 && (
          <line
            className="node-separator"
            x1={0}
            y1={HEADER_HEIGHT + 6 + fields.length * LINE_HEIGHT + 3}
            x2={NODE_WIDTH}
            y2={HEADER_HEIGHT + 6 + fields.length * LINE_HEIGHT + 3}
          />
        )}
        <text
          className={node.kind === "interface" ? "node-title interface" : "node-title"}
          x={NODE_WIDTH / 2}
          y={17}
          textAnchor="middle"
        >
          {node.name || "(sin nombre)"}
        </text>
        {fieldLines}
        {methodLines}
      </g>
    );
  }

  function renderEdge(edge: DesignerEdge) {
    const from = design.nodes.find((node) => node.id === edge.from);
    const to = design.nodes.find((node) => node.id === edge.to);
    if (!from || !to) return null;
    const fromHeight = nodeHeight(from);
    const toHeight = nodeHeight(to);
    const fromCenter = { x: from.x + NODE_WIDTH / 2, y: from.y + fromHeight / 2 };
    const toCenter = { x: to.x + NODE_WIDTH / 2, y: to.y + toHeight / 2 };
    const start = clipToRect(
      fromCenter.x,
      fromCenter.y,
      NODE_WIDTH,
      fromHeight,
      toCenter.x,
      toCenter.y,
    );
    const end = clipToRect(
      toCenter.x,
      toCenter.y,
      NODE_WIDTH,
      toHeight,
      fromCenter.x,
      fromCenter.y,
    );
    const isSelected = selection?.type === "edge" && selection.id === edge.id;
    const dashed = edge.kind === "implements" || edge.kind === "dependency";
    const markerEnd =
      edge.kind === "extends" || edge.kind === "implements"
        ? "url(#designer-triangle)"
        : edge.kind === "association" || edge.kind === "dependency"
          ? "url(#designer-arrow)"
          : undefined;
    const markerStart =
      edge.kind === "aggregation"
        ? "url(#designer-diamond-open)"
        : edge.kind === "composition"
          ? "url(#designer-diamond-filled)"
          : undefined;
    const label = edge.label.trim();

    return (
      <g key={edge.id}>
        <line
          className={dashed ? "designer-edge dashed" : "designer-edge"}
          x1={start.x}
          y1={start.y}
          x2={end.x}
          y2={end.y}
          markerEnd={markerEnd}
          markerStart={markerStart}
          style={isSelected ? { stroke: "var(--accent)", strokeWidth: 2 } : undefined}
        />
        <line
          className="designer-edge-hit"
          x1={start.x}
          y1={start.y}
          x2={end.x}
          y2={end.y}
          onPointerDown={(event) => {
            event.stopPropagation();
            setSelection({ type: "edge", id: edge.id });
          }}
        />
        {label && (
          <text
            className="edge-label"
            x={(start.x + end.x) / 2}
            y={(start.y + end.y) / 2 - 5}
            textAnchor="middle"
          >
            {label}
          </text>
        )}
      </g>
    );
  }

  const canWrite = !busy && problems.length === 0 && design.nodes.length > 0;

  return (
    <section className="designer">
      <header className="designer-header">
        <strong>Diseñador de clases</strong>
        <span className="designer-root" title={project.root}>
          {project.root}
        </span>
        <label className="designer-field">
          <span>Código en</span>
          <select value={language} onChange={(event) => setLanguage(event.target.value)}>
            <option value="java">Java</option>
            <option value="kotlin">Kotlin</option>
            <option value="python">Python</option>
          </select>
        </label>
        <div className="designer-tabs">
          <button
            type="button"
            className={tab === "diagrama" ? "active" : ""}
            onClick={() => setTab("diagrama")}
          >
            Diagrama
          </button>
          <button
            type="button"
            className={tab === "codigo" ? "active" : ""}
            onClick={() => setTab("codigo")}
          >
            Código
          </button>
        </div>
        <button type="button" onClick={onClose}>
          Cerrar
        </button>
      </header>

      <div className="designer-toolbar">
        <button type="button" onClick={() => addNode("class")}>
          + Clase
        </button>
        <button type="button" onClick={() => addNode("interface")}>
          + Interfaz
        </button>
        <label className="designer-field">
          <span>Relación</span>
          <select
            value={relationKind}
            onChange={(event) => setRelationKind(event.target.value as RelationKind)}
          >
            {RELATIONS.map((kind) => (
              <option key={kind} value={kind}>
                {RELATION_LABELS[kind]}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className={connectMode ? "primary" : ""}
          onClick={() => {
            setConnectMode(!connectMode);
            setConnectFrom(null);
          }}
        >
          {connectMode ? (connectFrom ? "Elige destino…" : "Elige origen…") : "Conectar"}
        </button>
        <button type="button" onClick={deleteSelection} disabled={!selection}>
          Eliminar
        </button>
        <span className="designer-hint">
          Arrastra las clases · Supr borra la selección · Esc sale de «Conectar»
        </span>
      </div>

      <div className="designer-body">
        <div className="designer-canvas">
          <svg
            ref={svgRef}
            width={CANVAS_WIDTH}
            height={CANVAS_HEIGHT}
            onPointerDown={() => {
              setSelection(null);
              setConnectFrom(null);
            }}
            onPointerMove={handlePointerMove}
            onPointerUp={() => {
              dragRef.current = null;
            }}
          >
            <defs>
              <marker
                id="designer-arrow"
                markerWidth="10"
                markerHeight="10"
                refX="9"
                refY="5"
                orient="auto"
              >
                <path d="M0,0 L10,5 L0,10 z" fill="#475569" />
              </marker>
              <marker
                id="designer-triangle"
                markerWidth="13"
                markerHeight="13"
                refX="12"
                refY="6.5"
                orient="auto"
              >
                <path d="M0,0 L13,6.5 L0,13 z" fill="#ffffff" stroke="#475569" />
              </marker>
              <marker
                id="designer-diamond-open"
                markerWidth="16"
                markerHeight="12"
                refX="1"
                refY="6"
                orient="auto"
              >
                <path d="M1,6 L8,1 L15,6 L8,11 z" fill="#ffffff" stroke="#475569" />
              </marker>
              <marker
                id="designer-diamond-filled"
                markerWidth="16"
                markerHeight="12"
                refX="1"
                refY="6"
                orient="auto"
              >
                <path d="M1,6 L8,1 L15,6 L8,11 z" fill="#475569" />
              </marker>
            </defs>
            {design.edges.map(renderEdge)}
            {design.nodes.map(renderNode)}
          </svg>
        </div>

        <aside className="designer-inspector">
          {selectedNode && (
            <div className="inspector-section">
              <h3>{selectedNode.kind === "interface" ? "Interfaz" : "Clase"}</h3>
              <label className="designer-field">
                <span>Nombre</span>
                <input
                  value={selectedNode.name}
                  onChange={(event) => updateNode(selectedNode.id, { name: event.target.value })}
                />
              </label>
              <label className="designer-field">
                <span>Tipo</span>
                <select
                  value={selectedNode.kind}
                  onChange={(event) =>
                    updateNode(selectedNode.id, {
                      kind: event.target.value as "class" | "interface",
                      isAbstract: false,
                    })
                  }
                >
                  <option value="class">Clase</option>
                  <option value="interface">Interfaz</option>
                </select>
              </label>
              {selectedNode.kind === "class" && (
                <label className="designer-check">
                  <input
                    type="checkbox"
                    checked={selectedNode.isAbstract}
                    onChange={(event) =>
                      updateNode(selectedNode.id, { isAbstract: event.target.checked })
                    }
                  />
                  <span>Abstracta</span>
                </label>
              )}

              <h4>
                Campos
                <button
                  type="button"
                  onClick={() =>
                    updateNode(selectedNode.id, { fields: [...selectedNode.fields, createField()] })
                  }
                >
                  + Campo
                </button>
              </h4>
              {selectedNode.fields.map((field) => (
                <div className="member-row" key={field.id}>
                  <input
                    placeholder="nombre"
                    value={field.name}
                    onChange={(event) =>
                      updateField(selectedNode.id, field.id, { name: event.target.value })
                    }
                  />
                  <input
                    placeholder="tipo"
                    value={field.ty}
                    onChange={(event) =>
                      updateField(selectedNode.id, field.id, { ty: event.target.value })
                    }
                  />
                  <select
                    value={field.visibility}
                    onChange={(event) =>
                      updateField(selectedNode.id, field.id, {
                        visibility: event.target.value as Visibility,
                      })
                    }
                  >
                    {VISIBILITIES.map((visibility) => (
                      <option key={visibility} value={visibility}>
                        {VISIBILITY_LABELS[visibility]}
                      </option>
                    ))}
                  </select>
                  <label className="designer-check" title="estático">
                    <input
                      type="checkbox"
                      checked={field.isStatic}
                      onChange={(event) =>
                        updateField(selectedNode.id, field.id, { isStatic: event.target.checked })
                      }
                    />
                    <span>S</span>
                  </label>
                  <label className="designer-check" title="final">
                    <input
                      type="checkbox"
                      checked={field.isFinal}
                      onChange={(event) =>
                        updateField(selectedNode.id, field.id, { isFinal: event.target.checked })
                      }
                    />
                    <span>F</span>
                  </label>
                  <button
                    type="button"
                    onClick={() =>
                      updateNode(selectedNode.id, {
                        fields: selectedNode.fields.filter(
                          (candidate) => candidate.id !== field.id,
                        ),
                      })
                    }
                  >
                    ✕
                  </button>
                </div>
              ))}

              <h4>
                Métodos
                <button
                  type="button"
                  onClick={() =>
                    updateNode(selectedNode.id, {
                      methods: [...selectedNode.methods, createMethod()],
                    })
                  }
                >
                  + Método
                </button>
              </h4>
              {selectedNode.methods.map((method) => (
                <div className="method-block" key={method.id}>
                  <div className="member-row">
                    <input
                      placeholder="nombre"
                      value={method.name}
                      onChange={(event) =>
                        updateMethod(selectedNode.id, method.id, { name: event.target.value })
                      }
                    />
                    <input
                      placeholder="retorno"
                      value={method.returnTy}
                      onChange={(event) =>
                        updateMethod(selectedNode.id, method.id, { returnTy: event.target.value })
                      }
                    />
                    <select
                      value={method.visibility}
                      onChange={(event) =>
                        updateMethod(selectedNode.id, method.id, {
                          visibility: event.target.value as Visibility,
                        })
                      }
                    >
                      {VISIBILITIES.map((visibility) => (
                        <option key={visibility} value={visibility}>
                          {VISIBILITY_LABELS[visibility]}
                        </option>
                      ))}
                    </select>
                    <label className="designer-check" title="estático">
                      <input
                        type="checkbox"
                        checked={method.isStatic}
                        onChange={(event) =>
                          updateMethod(selectedNode.id, method.id, {
                            isStatic: event.target.checked,
                          })
                        }
                      />
                      <span>S</span>
                    </label>
                    <label className="designer-check" title="abstracto">
                      <input
                        type="checkbox"
                        checked={method.isAbstract}
                        disabled={selectedNode.kind === "interface"}
                        onChange={(event) =>
                          updateMethod(selectedNode.id, method.id, {
                            isAbstract: event.target.checked,
                          })
                        }
                      />
                      <span>A</span>
                    </label>
                    <button
                      type="button"
                      onClick={() =>
                        updateNode(selectedNode.id, {
                          methods: selectedNode.methods.filter(
                            (candidate) => candidate.id !== method.id,
                          ),
                        })
                      }
                    >
                      ✕
                    </button>
                  </div>
                  <input
                    className="params-input"
                    placeholder="parámetros: nombre: tipo, edad: int"
                    value={formatParams(method.params)}
                    onChange={(event) =>
                      updateMethod(selectedNode.id, method.id, {
                        params: parseParams(event.target.value),
                      })
                    }
                  />
                </div>
              ))}

              <button type="button" className="danger" onClick={deleteSelection}>
                Eliminar {selectedNode.kind === "interface" ? "interfaz" : "clase"}
              </button>
            </div>
          )}

          {selectedEdge && (
            <div className="inspector-section">
              <h3>Relación</h3>
              <p className="edge-summary">
                {nameOf(selectedEdge.from)} → {nameOf(selectedEdge.to)}
              </p>
              <label className="designer-field">
                <span>Tipo</span>
                <select
                  value={selectedEdge.kind}
                  onChange={(event) =>
                    setDesign((current) => ({
                      ...current,
                      edges: current.edges.map((edge) =>
                        edge.id === selectedEdge.id
                          ? { ...edge, kind: event.target.value as RelationKind }
                          : edge,
                      ),
                    }))
                  }
                >
                  {RELATIONS.map((kind) => (
                    <option key={kind} value={kind}>
                      {RELATION_LABELS[kind]}
                    </option>
                  ))}
                </select>
              </label>
              <label className="designer-field">
                <span>Etiqueta</span>
                <input
                  value={selectedEdge.label}
                  onChange={(event) =>
                    setDesign((current) => ({
                      ...current,
                      edges: current.edges.map((edge) =>
                        edge.id === selectedEdge.id ? { ...edge, label: event.target.value } : edge,
                      ),
                    }))
                  }
                />
              </label>
              <button type="button" className="danger" onClick={deleteSelection}>
                Eliminar relación
              </button>
            </div>
          )}

          {!selectedNode && !selectedEdge && (
            <div className="inspector-section">
              <h3>Cómo se usa</h3>
              <ol className="designer-help">
                <li>Añade clases e interfaces con los botones «+ Clase» e «+ Interfaz».</li>
                <li>
                  Arrástralas para colocarlas y selecciónalas para editar sus campos y métodos.
                </li>
                <li>
                  Elige un tipo de relación, pulsa «Conectar» y haz clic en el origen y luego en el
                  destino.
                </li>
                <li>Revisa el diagrama y el código generado en la parte inferior.</li>
                <li>«Escribir en el proyecto» crea los archivos en la carpeta abierta.</li>
              </ol>
              <p className="designer-note">
                La herencia y la implementación generan código; las asociaciones, agregaciones,
                composiciones y dependencias se reflejan solo en el diagrama.
              </p>
            </div>
          )}
        </aside>
      </div>

      <div className="designer-preview">
        {message && <p className={`designer-message ${message.kind}`}>{message.text}</p>}
        {tab === "diagrama" ? (
          mermaidError ? (
            <p className="designer-error">{mermaidError}</p>
          ) : (
            <div className="preview-host" ref={previewRef} />
          )
        ) : problems.length > 0 ? (
          <ul className="designer-problems">
            {problems.map((problem) => (
              <li key={problem}>{problem}</li>
            ))}
          </ul>
        ) : codeError ? (
          <p className="designer-error">{codeError}</p>
        ) : files ? (
          <pre className="designer-code">
            {files.map((file) => `── ${file.relative} ──\n${file.content}`).join("\n")}
          </pre>
        ) : (
          <p className="designer-empty">Dibuja al menos una clase para ver el código generado.</p>
        )}
        <div className="designer-footer">
          {problems.length > 0 && (
            <button
              type="button"
              className="designer-problems-badge"
              onClick={() => setTab("codigo")}
            >
              {problems.length} {problems.length === 1 ? "problema" : "problemas"}
            </button>
          )}
          {conflicts && (
            <span className="designer-conflicts">
              Ya existen: {conflicts.join(", ")}.
              <button type="button" disabled={busy} onClick={() => void handleWrite(true)}>
                Sobrescribir
              </button>
              <button type="button" disabled={busy} onClick={() => setConflicts(null)}>
                Cancelar
              </button>
            </span>
          )}
          <button
            type="button"
            className="primary"
            disabled={!canWrite}
            onClick={() => void handleWrite(false)}
          >
            {busy ? "Escribiendo…" : "Escribir en el proyecto"}
          </button>
        </div>
      </div>
    </section>
  );
}
