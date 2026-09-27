import { useState, type FormEvent } from "react";
import type { SourceEntry } from "../types";
import "./FileTree.css";

type Props = {
  files: SourceEntry[];
  activePath: string | null;
  dirty: Set<string>;
  extension: string;
  onSelect: (path: string) => void;
  onCreate: (name: string) => void;
};

export function FileTree({ files, activePath, dirty, extension, onSelect, onCreate }: Props) {
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const suffix = extension ? `.${extension}` : "";

  const counts = new Map<string, number>();
  for (const file of files) {
    counts.set(file.name, (counts.get(file.name) ?? 0) + 1);
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) return;
    const missingSuffix = suffix !== "" && !trimmed.toLowerCase().endsWith(suffix.toLowerCase());
    onCreate(missingSuffix ? `${trimmed}${suffix}` : trimmed);
    setName("");
    setCreating(false);
  }

  return (
    <aside className="file-tree">
      <div className="file-tree-header">
        <span>Archivos</span>
        <button type="button" onClick={() => setCreating((current) => !current)}>
          + Nuevo
        </button>
      </div>
      {creating && (
        <form className="file-tree-form" onSubmit={submit}>
          <input
            autoFocus
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder={`Nombre${suffix}`}
          />
          <button type="submit">Crear</button>
        </form>
      )}
      <ul>
        {files.map((file) => (
          <li key={file.path}>
            <button
              type="button"
              className={file.path === activePath ? "active" : ""}
              title={file.relative}
              onClick={() => onSelect(file.path)}
            >
              <span className="file-name">
                {(counts.get(file.name) ?? 0) > 1 ? file.relative : file.name}
              </span>
              {dirty.has(file.path) && <span className="dirty-dot" title="Sin guardar" />}
            </button>
          </li>
        ))}
        {files.length === 0 && <li className="empty">Sin archivos</li>}
      </ul>
    </aside>
  );
}
