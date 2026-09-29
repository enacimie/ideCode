import { describe, expect, it } from "vitest";
import { webBackend } from "./webBackend";

function dispatchBeforeUnload(): boolean {
  const event = new Event("beforeunload", { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

describe("webBackend.watchCloseRequests", () => {
  it("retiene el cierre del navegador solo cuando hay cambios", async () => {
    let dirty = false;
    const stop = await webBackend.watchCloseRequests(
      () => dirty,
      () => undefined,
    );

    expect(dispatchBeforeUnload()).toBe(false);
    dirty = true;
    expect(dispatchBeforeUnload()).toBe(true);

    stop();
    expect(dispatchBeforeUnload()).toBe(false);
  });

  it("closeWindow no lanza en la demo web", async () => {
    await expect(webBackend.closeWindow()).resolves.toBeUndefined();
  });
});

describe("webBackend con Kotlin", () => {
  it("carga la demo de Kotlin con su lenguaje", async () => {
    const snapshot = await webBackend.loadExample("zoologico_kotlin");
    expect(snapshot.language).toBe("kotlin");
    expect(snapshot.files.some((file) => file.relative === "Main.kt")).toBe(true);
  });

  it("crea fuentes .kt con plantilla de clase", async () => {
    await webBackend.loadExample("zoologico_kotlin");
    const entry = await webBackend.createSource("NuevoAnimal.kt");
    expect(entry.content).toBe("class NuevoAnimal {\n}\n");
  });

  it("anuncia Kotlin entre los lenguajes", async () => {
    const languages = await webBackend.listLanguages();
    const kotlin = languages.find((language) => language.id === "kotlin");
    expect(kotlin?.entryLabel).toBe("Archivo principal");
    expect(kotlin?.compileLabel).toBe("Compilar");
  });
});

describe("webBackend.writeSource y createSource", () => {
  it("rechaza escribir en rutas inexistentes", async () => {
    await webBackend.loadExample("zoologico_kotlin");
    await expect(webBackend.writeSource("NoExiste.kt", "x")).rejects.toThrow(
      "No existe el archivo",
    );
  });

  it("actualiza el contenido de un archivo existente", async () => {
    const snapshot = await webBackend.loadExample("zoologico_kotlin");
    const target = snapshot.files[0].path;
    await webBackend.writeSource(target, "contenido nuevo");
    expect(await webBackend.readSource(target)).toBe("contenido nuevo");
  });

  it("rechaza duplicados, rutas y extensiones desconocidas", async () => {
    await webBackend.loadExample("zoologico_kotlin");
    await webBackend.createSource("UnicaPrueba.kt");
    await expect(webBackend.createSource("UnicaPrueba.kt")).rejects.toThrow("Ya existe");
    await expect(webBackend.createSource("sub/Clase.kt")).rejects.toThrow("rutas");
    await expect(webBackend.createSource("notas.txt")).rejects.toThrow("Extensión");
  });

  it("no contamina la demo al crear o escribir", async () => {
    await webBackend.loadExample("zoologico_kotlin");
    const before = (await webBackend.listExamples()).find(
      (example) => example.id === "zoologico_kotlin",
    )?.files;
    await webBackend.createSource("TemporalPrueba.kt");
    const after = (await webBackend.listExamples()).find(
      (example) => example.id === "zoologico_kotlin",
    )?.files;
    expect(after).toBe(before);
  });

  it("rechaza ejemplos inexistentes", async () => {
    await expect(webBackend.loadExample("no_existe")).rejects.toThrow("No existe el ejemplo");
  });
});
