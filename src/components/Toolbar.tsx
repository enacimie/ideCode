import type { ExampleInfo, ProjectSnapshot } from "../types";
import { Menu, MenuEmpty, MenuItem, MenuLabel, MenuSeparator } from "./Menu";
import "./Toolbar.css";

type Props = {
  project: ProjectSnapshot | null;
  examples: ExampleInfo[];
  running: boolean;
  compileLabel: string;
  entryLabel: string;
  args: string;
  entry: string;
  onOpen: () => void;
  onExample: (example: string) => void;
  onSave: () => void;
  onCompile: () => void;
  onRun: () => void;
  onDiagram: () => void;
  onArgsChange: (value: string) => void;
  onEntryChange: (value: string) => void;
};

export function Toolbar({
  project,
  examples,
  running,
  compileLabel,
  entryLabel,
  args,
  entry,
  onOpen,
  onExample,
  onSave,
  onCompile,
  onRun,
  onDiagram,
  onArgsChange,
  onEntryChange,
}: Props) {
  const disabled = !project || running;

  return (
    <header className="toolbar">
      <Menu label="Archivo">
        {(close) => (
          <>
            <MenuItem
              onSelect={() => {
                onOpen();
                close();
              }}
            >
              Abrir carpeta…
            </MenuItem>
            <MenuSeparator />
            <MenuLabel>Ejemplos</MenuLabel>
            {examples.length === 0 ? (
              <MenuEmpty>No hay ejemplos incluidos</MenuEmpty>
            ) : (
              examples.map((example) => (
                <MenuItem
                  key={example.id}
                  hint={example.files === 1 ? "1 archivo" : `${example.files} archivos`}
                  onSelect={() => {
                    onExample(example.id);
                    close();
                  }}
                >
                  {example.name}
                </MenuItem>
              ))
            )}
            <MenuSeparator />
            <MenuItem
              hint="Ctrl+S"
              disabled={!project}
              onSelect={() => {
                onSave();
                close();
              }}
            >
              Guardar
            </MenuItem>
          </>
        )}
      </Menu>

      <div className="toolbar-group">
        <button type="button" className="primary" onClick={onCompile} disabled={disabled}>
          {compileLabel}
        </button>
        <button type="button" className="primary" onClick={onRun} disabled={disabled}>
          {running ? "Ejecutando…" : "Ejecutar"}
        </button>
        <button type="button" onClick={onDiagram} disabled={disabled}>
          Diagrama de clases
        </button>
      </div>

      <label className="toolbar-field">
        <span>Argumentos</span>
        <input
          value={args}
          onChange={(event) => onArgsChange(event.target.value)}
          placeholder="uno dos"
          disabled={!project}
        />
      </label>

      <label className="toolbar-field">
        <span>{entryLabel}</span>
        <input
          value={entry}
          onChange={(event) => onEntryChange(event.target.value)}
          placeholder="automática"
          disabled={!project}
        />
      </label>

      {project && (
        <span className="toolbar-root" title={project.root}>
          {project.language} · {project.root}
        </span>
      )}
    </header>
  );
}
