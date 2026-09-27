import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FileTree } from "./FileTree";

afterEach(cleanup);

function createFile(extension: string, typed: string) {
  const onCreate = vi.fn();
  render(
    <FileTree
      files={[]}
      activePath={null}
      dirty={new Set<string>()}
      extension={extension}
      onSelect={() => undefined}
      onCreate={onCreate}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "+ Nuevo" }));
  const input = screen.getByRole("textbox");
  fireEvent.change(input, { target: { value: typed } });
  fireEvent.submit(input.closest("form")!);
  return onCreate;
}

describe("FileTree", () => {
  it("añade la extensión del lenguaje activo", () => {
    expect(createFile("java", "Perro")).toHaveBeenCalledWith("Perro.java");
  });

  it("no duplica una extensión ya presente", () => {
    expect(createFile("java", "Perro.JAVA")).toHaveBeenCalledWith("Perro.JAVA");
  });

  it("respeta el nombre cuando no hay extensión conocida", () => {
    expect(createFile("", "Perro")).toHaveBeenCalledWith("Perro");
  });

  it("muestra la extensión esperada en el marcador de posición", () => {
    render(
      <FileTree
        files={[]}
        activePath={null}
        dirty={new Set<string>()}
        extension="py"
        onSelect={() => undefined}
        onCreate={() => undefined}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "+ Nuevo" }));
    expect(screen.getByPlaceholderText("Nombre.py")).toBeTruthy();
  });
});

describe("FileTree con colisiones de nombre", () => {
  it("muestra la ruta relativa cuando dos archivos se llaman igual", () => {
    const files = [
      { path: "/p/a/Main.java", relative: "a/Main.java", name: "Main.java", content: "" },
      { path: "/p/b/Main.java", relative: "b/Main.java", name: "Main.java", content: "" },
      { path: "/p/Unico.java", relative: "Unico.java", name: "Unico.java", content: "" },
    ];
    render(
      <FileTree
        files={files}
        activePath={null}
        dirty={new Set<string>()}
        extension="java"
        onSelect={() => undefined}
        onCreate={() => undefined}
      />,
    );
    expect(screen.getByText("a/Main.java")).toBeTruthy();
    expect(screen.getByText("b/Main.java")).toBeTruthy();
    expect(screen.getByText("Unico.java")).toBeTruthy();
    expect(screen.queryByText("Main.java")).toBeNull();
  });
});
