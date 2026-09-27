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
    const entry = await webBackend.createSource("Perro.kt");
    expect(entry.content).toBe("class Perro {\n}\n");
  });

  it("anuncia Kotlin entre los lenguajes", async () => {
    const languages = await webBackend.listLanguages();
    const kotlin = languages.find((language) => language.id === "kotlin");
    expect(kotlin?.entryLabel).toBe("Archivo principal");
    expect(kotlin?.compileLabel).toBe("Compilar");
  });
});
