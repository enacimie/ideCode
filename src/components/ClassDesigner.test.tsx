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
});
