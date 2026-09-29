import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DiagramPanel } from "./DiagramPanel";
import type { ClassModel } from "../types";

afterEach(cleanup);

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

const proto = (globalThis as { SVGElement?: { prototype: object } }).SVGElement?.prototype;
if (proto) {
  Object.assign(proto, {
    getBBox: (): Rect => ({ x: 0, y: 0, width: 120, height: 18 }),
    getComputedTextLength: (): number => 100,
  });
}

const classes: ClassModel[] = [
  {
    name: "Animal",
    simple_name: "Animal",
    outer: null,
    kind: "class",
    package: null,
    is_abstract: true,
    type_parameters: [],
    extends: [],
    implements: [],
    enum_constants: [],
    fields: [
      {
        name: "nombre",
        ty: "String",
        targets: ["String"],
        multiplicity: "one",
        visibility: "private",
        is_static: false,
        is_final: false,
        line: 2,
      },
    ],
    functions: [],
    methods: [
      {
        name: "hablar",
        return_ty: "String",
        visibility: "public",
        is_static: false,
        is_abstract: false,
        is_constructor: false,
        is_async: false,
        type_parameters: [],
        throws: [],
        parameters: [],
        line: 3,
      },
    ],
    uses: [],
    line: 1,
    file: "Animal.java",
  },
];

const chart = `classDiagram
    class Animal {
        -String nombre
        +String hablar()
    }
`;

async function renderPanel(options: { stale?: boolean; onRefresh?: () => void } = {}) {
  const onSelect = vi.fn();
  const { container } = render(
    <DiagramPanel
      chart={chart}
      classes={classes}
      stale={options.stale ?? false}
      onSelect={onSelect}
      onRefresh={options.onRefresh ?? (() => undefined)}
      onClose={() => undefined}
    />,
  );

  const node = await waitFor(
    () => {
      const found = container.querySelector<SVGGElement>('g.node[id*="classId-Animal-"]');
      if (!found) throw new Error("el diagrama aún no se ha renderizado");
      return found;
    },
    { timeout: 60000 },
  );

  return { onSelect, node };
}

function click(element: Element) {
  element.dispatchEvent(new MouseEvent("click", { bubbles: true }));
}

describe("DiagramPanel", () => {
  it("abre la clase al hacer clic en su nodo", async () => {
    const { onSelect, node } = await renderPanel();
    click(node);
    await waitFor(() => expect(onSelect).toHaveBeenCalledWith("Animal", 1));
  });

  it("abre el atributo al hacer clic en su fila", async () => {
    const { onSelect, node } = await renderPanel();
    const row = node.querySelector("g.members-group > g.label");
    expect(row).not.toBeNull();
    click(row!);
    await waitFor(() => expect(onSelect).toHaveBeenCalledWith("Animal", 2));
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it("abre el método al hacer clic en su fila", async () => {
    const { onSelect, node } = await renderPanel();
    const row = node.querySelector("g.methods-group > g.label");
    expect(row).not.toBeNull();
    click(row!);
    await waitFor(() => expect(onSelect).toHaveBeenCalledWith("Animal", 3));
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it("oculta el aviso de diagrama desactualizado cuando está al día", async () => {
    await renderPanel();
    expect(screen.queryByText("Desactualizado")).toBeNull();
  });

  it("avisa y permite regenerar un diagrama desactualizado", async () => {
    const onRefresh = vi.fn();
    await renderPanel({ stale: true, onRefresh });
    expect(screen.getByText("Desactualizado")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Regenerar" }));
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });

  it("muestra el error cuando mermaid no puede renderizar", async () => {
    const { container } = render(
      <DiagramPanel
        chart="esto no es @@ un diagrama"
        classes={[]}
        stale={false}
        onSelect={vi.fn()}
        onRefresh={() => undefined}
        onClose={() => undefined}
      />,
    );

    await waitFor(
      () => {
        const error = container.querySelector(".diagram-error");
        if (!error || !error.textContent) throw new Error("sin mensaje de error todavía");
        return error;
      },
      { timeout: 30000 },
    );
  });

  it("renderiza un diagrama con un módulo de funciones libres", async () => {
    const module: ClassModel = {
      name: "utilidades",
      simple_name: "utilidades",
      outer: null,
      kind: "module",
      package: null,
      is_abstract: false,
      type_parameters: [],
      extends: [],
      implements: [],
      enum_constants: [],
      fields: [],
      methods: [],
      functions: [{ name: "saludar", return_ty: "", is_async: false, parameters: [], line: 2 }],
      uses: [],
      line: 1,
      file: "utilidades.py",
    };
    const moduleChart = `classDiagram
    direction TB
    class utilidades {
        <<module>>
        saludar()
    }
`;
    const onSelect = vi.fn();
    const { container } = render(
      <DiagramPanel
        chart={moduleChart}
        classes={[module]}
        stale={false}
        onSelect={onSelect}
        onRefresh={() => undefined}
        onClose={() => undefined}
      />,
    );

    const node = await waitFor(
      () => {
        const found = container.querySelector<SVGGElement>('g.node[id*="classId-utilidades-"]');
        if (!found) throw new Error("el módulo aún no se ha renderizado");
        return found;
      },
      { timeout: 60000 },
    );

    const row = node.querySelector("g.methods-group > g.label");
    expect(row).not.toBeNull();
    click(row!);
    await waitFor(() => expect(onSelect).toHaveBeenCalledWith("utilidades", 2));
  });
});
