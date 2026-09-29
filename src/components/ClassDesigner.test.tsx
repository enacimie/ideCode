import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectSnapshot, SourceEntry } from "../types";
import { ClassDesigner } from "./ClassDesigner";

const { generateCode, writeGeneratedFiles } = vi.hoisted(() => ({
  generateCode: vi.fn(),
  writeGeneratedFiles: vi.fn(),
}));

vi.mock("../backend", () => ({
  backend: { generateCode, writeGeneratedFiles },
}));

vi.mock("mermaid", () => ({
  default: {
    initialize: vi.fn(),
    render: vi.fn(async () => ({ svg: "<svg></svg>" })),
  },
}));

const project: ProjectSnapshot = { root: "/tmp/proyecto", language: "java", files: [] };

const writtenEntry: SourceEntry = {
  path: "/tmp/proyecto/Perro.java",
  relative: "Perro.java",
  name: "Perro.java",
  content: "public class Perro {\n}\n",
};

afterEach(cleanup);

beforeEach(() => {
  localStorage.clear();
  generateCode.mockReset();
  writeGeneratedFiles.mockReset();
  generateCode.mockResolvedValue([
    { relative: "Perro.java", content: "public class Perro {\n}\n" },
  ]);
});

function addClassNamed(name: string) {
  fireEvent.click(screen.getByRole("button", { name: "+ Clase" }));
  fireEvent.change(screen.getByLabelText("Nombre"), { target: { value: name } });
}

describe("ClassDesigner", () => {
  it("añade una clase, la renombra y previsualiza el código generado", async () => {
    render(<ClassDesigner project={project} onWritten={vi.fn()} onClose={vi.fn()} />);
    addClassNamed("Perro");

    fireEvent.click(screen.getByRole("button", { name: "Código" }));
    await waitFor(() => expect(generateCode).toHaveBeenCalled(), { timeout: 3000 });

    const [models, language] = generateCode.mock.calls[generateCode.mock.calls.length - 1];
    expect(language).toBe("java");
    expect(models.map((model: { name: string }) => model.name)).toContain("Perro");
    await waitFor(() => expect(screen.getByText(/Perro\.java/)).toBeTruthy(), { timeout: 3000 });
  });

  it("permite escribir parámetros con dos puntos y los lleva al modelo", async () => {
    render(<ClassDesigner project={project} onWritten={vi.fn()} onClose={vi.fn()} />);
    addClassNamed("Perro");
    fireEvent.click(screen.getByRole("button", { name: "+ Método" }));
    fireEvent.change(screen.getByPlaceholderText("nombre"), { target: { value: "comer" } });

    const params = screen.getByPlaceholderText(/parámetros/) as HTMLInputElement;
    fireEvent.change(params, { target: { value: "n" } });
    expect(params.value).toBe("n");
    fireEvent.change(params, { target: { value: "nombre:" } });
    expect(params.value).toBe("nombre:");
    fireEvent.change(params, { target: { value: "nombre: String, edad: int" } });
    expect(params.value).toBe("nombre: String, edad: int");

    fireEvent.click(screen.getByRole("button", { name: "Código" }));
    await waitFor(() => expect(generateCode).toHaveBeenCalled(), { timeout: 3000 });
    const [models] = generateCode.mock.calls[generateCode.mock.calls.length - 1];
    const method = models[0].methods[0];
    expect(method.name).toBe("comer");
    expect(method.parameters).toEqual([
      { name: "nombre", ty: "String" },
      { name: "edad", ty: "int" },
    ]);
  });

  it("escribe los archivos generados en el proyecto", async () => {
    writeGeneratedFiles.mockResolvedValue({ written: [writtenEntry], conflicts: [] });
    const onWritten = vi.fn();
    render(<ClassDesigner project={project} onWritten={onWritten} onClose={vi.fn()} />);
    addClassNamed("Perro");

    fireEvent.click(screen.getByRole("button", { name: "Escribir en el proyecto" }));
    await waitFor(() => expect(onWritten).toHaveBeenCalledWith([writtenEntry]));
    expect(writeGeneratedFiles).toHaveBeenLastCalledWith(expect.any(Array), false);
    expect(screen.getByText(/Archivo generado/)).toBeTruthy();
  });

  it("pregunta antes de sobrescribir archivos existentes", async () => {
    writeGeneratedFiles
      .mockResolvedValueOnce({ written: [], conflicts: ["Perro.java"] })
      .mockResolvedValueOnce({ written: [writtenEntry], conflicts: [] });
    const onWritten = vi.fn();
    render(<ClassDesigner project={project} onWritten={onWritten} onClose={vi.fn()} />);
    addClassNamed("Perro");

    fireEvent.click(screen.getByRole("button", { name: "Escribir en el proyecto" }));
    await waitFor(() => expect(screen.getByText(/Ya existen: Perro\.java/)).toBeTruthy());
    expect(onWritten).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Sobrescribir" }));
    await waitFor(() => expect(onWritten).toHaveBeenCalledWith([writtenEntry]));
    expect(writeGeneratedFiles).toHaveBeenLastCalledWith(expect.any(Array), true);
  });

  it("bloquea la escritura cuando el diseño tiene problemas", () => {
    render(<ClassDesigner project={project} onWritten={vi.fn()} onClose={vi.fn()} />);
    addClassNamed("Perro");
    fireEvent.click(screen.getByRole("button", { name: "+ Clase" }));
    fireEvent.change(screen.getByLabelText("Nombre"), { target: { value: "Perro" } });

    const write = screen.getByRole("button", { name: "Escribir en el proyecto" });
    expect(write.hasAttribute("disabled")).toBe(true);
    expect(screen.getByRole("button", { name: /problema/ })).toBeTruthy();
  });

  it("elimina la selección con la tecla Supr", () => {
    render(<ClassDesigner project={project} onWritten={vi.fn()} onClose={vi.fn()} />);
    addClassNamed("Perro");
    expect(screen.getByLabelText("Nombre")).toBeTruthy();

    fireEvent.keyDown(window, { key: "Delete" });
    expect(screen.queryByLabelText("Nombre")).toBeNull();
    expect(screen.getByText(/Cómo se usa/)).toBeTruthy();
  });

  function parseWorld(container: HTMLElement) {
    const world = container.querySelector<SVGGElement>(".designer-world")!;
    const match = /translate\(([-\d.e]+), ([-\d.e]+)\) scale\(([\d.e]+)\)/.exec(
      world.getAttribute("transform") ?? "",
    );
    expect(match).not.toBeNull();
    return { tx: Number(match![1]), ty: Number(match![2]), k: Number(match![3]) };
  }

  it("acerca el lienzo con la rueda y lo devuelve a su sitio con Ajustar", () => {
    const { container } = render(
      <ClassDesigner project={project} onWritten={vi.fn()} onClose={vi.fn()} />,
    );
    const svg = container.querySelector<SVGSVGElement>(".designer-canvas svg")!;
    expect(parseWorld(container)).toEqual({ tx: 0, ty: 0, k: 1 });

    fireEvent.wheel(svg, { deltaY: -200, clientX: 100, clientY: 100 });
    const zoomed = parseWorld(container);
    expect(zoomed.k).toBeCloseTo(Math.exp(0.3), 4);
    expect(zoomed.tx).toBeCloseTo(100 - 100 * zoomed.k, 4);
    expect(screen.getByText("135 %")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Alejar lienzo" }));
    expect(parseWorld(container).k).toBeLessThan(zoomed.k);

    fireEvent.click(screen.getByRole("button", { name: "Ajustar lienzo" }));
    expect(parseWorld(container)).toEqual({ tx: 0, ty: 0, k: 1 });
  });

  it("desplaza el lienzo arrastrando el fondo", () => {
    const { container } = render(
      <ClassDesigner project={project} onWritten={vi.fn()} onClose={vi.fn()} />,
    );
    const svg = container.querySelector<SVGSVGElement>(".designer-canvas svg")!;
    fireEvent.pointerDown(svg, { clientX: 50, clientY: 40 });
    fireEvent.pointerMove(svg, { clientX: 120, clientY: 90 });
    fireEvent.pointerUp(svg, { clientX: 120, clientY: 90 });
    expect(parseWorld(container)).toEqual({ tx: 70, ty: 50, k: 1 });
  });

  it("arrastra las clases con la vista ampliada usando coordenadas del mundo", () => {
    const { container } = render(
      <ClassDesigner project={project} onWritten={vi.fn()} onClose={vi.fn()} />,
    );
    addClassNamed("Perro");
    const svg = container.querySelector<SVGSVGElement>(".designer-canvas svg")!;
    const nodeGroup = container.querySelector<SVGGElement>(".designer-node")!.parentElement!;
    const before = /translate\(([-\d.e]+), ([-\d.e]+)\)/.exec(
      nodeGroup.getAttribute("transform") ?? "",
    )!;
    const x0 = Number(before[1]);
    const y0 = Number(before[2]);

    fireEvent.wheel(svg, { deltaY: -200, clientX: 0, clientY: 0 });
    const k = Math.exp(0.3);

    fireEvent.pointerDown(nodeGroup.firstElementChild!, { clientX: x0 * k, clientY: y0 * k });
    fireEvent.pointerMove(svg, { clientX: x0 * k + 67.5, clientY: y0 * k });
    fireEvent.pointerUp(svg, { clientX: x0 * k + 67.5, clientY: y0 * k });

    const after = /translate\(([-\d.e]+), ([-\d.e]+)\)/.exec(
      nodeGroup.getAttribute("transform") ?? "",
    )!;
    expect(Math.abs(Number(after[1]) - x0 - 50)).toBeLessThan(1);
    expect(Math.abs(Number(after[2]) - y0)).toBeLessThan(1);
  });
});
