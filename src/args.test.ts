import { describe, expect, it } from "vitest";
import { splitArgs } from "./args";

describe("splitArgs", () => {
  it("returns nothing for empty input", () => {
    expect(splitArgs("")).toEqual([]);
    expect(splitArgs("   ")).toEqual([]);
  });

  it("splits on whitespace", () => {
    expect(splitArgs("uno dos   tres")).toEqual(["uno", "dos", "tres"]);
  });

  it("keeps quoted groups together", () => {
    expect(splitArgs('hola "dos palabras" mundo')).toEqual(["hola", "dos palabras", "mundo"]);
    expect(splitArgs("'otra frase' final")).toEqual(["otra frase", "final"]);
  });

  it("keeps an empty quoted argument", () => {
    expect(splitArgs('a "" b')).toEqual(["a", "", "b"]);
  });
});
