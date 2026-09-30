import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import App from "./App";
import { webBackend } from "./backend/webBackend";

afterEach(cleanup);

beforeEach(async () => {
  await webBackend.loadExample("zoologico");
});

async function loadDemo() {
  render(<App />);
  await waitFor(() => expect(screen.getByText("Main.java")).toBeTruthy(), { timeout: 60000 });
}

describe("App", () => {
  it("carga el proyecto de demostración y lista sus archivos", async () => {
    await loadDemo();
    expect(screen.getByText("Animal.java")).toBeTruthy();
    expect(screen.getByText("Perro.java")).toBeTruthy();
    expect(screen.getByText("Veterinario.java")).toBeTruthy();
  });

  it("muestra el lenguaje detectado en la barra", async () => {
    await loadDemo();
    expect(screen.getByText(/^java · /)).toBeTruthy();
    expect(screen.getByText("Clase principal")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Compilar" })).toBeTruthy();
  });

  it("informa de que el diagrama necesita la app de escritorio", async () => {
    await loadDemo();
    fireEvent.click(screen.getByRole("button", { name: "Diagrama de clases" }));
    await waitFor(() =>
      expect(screen.getByText(/solo está disponible en la aplicación de escritorio/i)).toBeTruthy(),
    );
  });

  it("informa de que compilar necesita la app de escritorio", async () => {
    await loadDemo();
    fireEvent.click(screen.getByRole("button", { name: "Compilar" }));
    await waitFor(() =>
      expect(screen.getByText(/solo está disponible en la aplicación de escritorio/i)).toBeTruthy(),
    );
  });

  it("muestra el aviso de escritorio al ejecutar en el navegador", async () => {
    await loadDemo();
    fireEvent.click(screen.getByRole("button", { name: "Ejecutar" }));
    await waitFor(() =>
      expect(screen.getByText(/solo está disponible en la aplicación de escritorio/i)).toBeTruthy(),
    );
  });

  it("agrupa las acciones de archivo en el menú", async () => {
    await loadDemo();
    fireEvent.click(screen.getByRole("button", { name: /Archivo/ }));
    expect(screen.getByRole("menuitem", { name: /Abrir carpeta/ })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: /^\s*Zoologico\s*6 archivos\s*$/ })).toBeTruthy();
    expect(
      screen.getByRole("menuitem", { name: /^\s*Zoologico Python\s*6 archivos\s*$/ }),
    ).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: /Guardar/ })).toBeTruthy();
  });

  it("carga un ejemplo desde el menú", async () => {
    await loadDemo();
    fireEvent.click(screen.getByRole("button", { name: /Archivo/ }));
    fireEvent.click(screen.getByRole("menuitem", { name: /^\s*Zoologico\s*6 archivos\s*$/ }));
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(screen.getByText("Main.java")).toBeTruthy();
  });

  it("cambia a Python y adapta el botón de compilar", async () => {
    await loadDemo();
    fireEvent.click(screen.getByRole("button", { name: /Archivo/ }));
    fireEvent.click(
      screen.getByRole("menuitem", { name: /^\s*Zoologico Python\s*6 archivos\s*$/ }),
    );
    await waitFor(() => expect(screen.getByText("main.py")).toBeTruthy());
    expect(screen.getByText(/^python · /)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Comprobar sintaxis" })).toBeTruthy();
    expect(screen.getByText("Módulo principal")).toBeTruthy();
    expect(screen.queryByText("Clase principal")).toBeNull();
  });

  it("cambia a Kotlin y adapta las etiquetas", async () => {
    await loadDemo();
    fireEvent.click(screen.getByRole("button", { name: /Archivo/ }));
    fireEvent.click(
      screen.getByRole("menuitem", { name: /^\s*Zoologico Kotlin\s*6 archivos\s*$/ }),
    );
    await waitFor(() => expect(screen.getByText("Main.kt")).toBeTruthy());
    expect(screen.getByText(/^kotlin · /)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Compilar" })).toBeTruthy();
    expect(screen.getByText("Archivo principal")).toBeTruthy();
  });

  it("guarda con Ctrl+S desde cualquier parte", async () => {
    await loadDemo();
    fireEvent.keyDown(window, { key: "s", ctrlKey: true });
    await waitFor(() => expect(screen.getByText("Cambios guardados.")).toBeTruthy());
  });
});
