import { describe, expect, it } from "vitest";
import type { ClassModel } from "../types";
import {
  buildMermaid,
  createField,
  createMethod,
  createNode,
  emptyDesign,
  formatParams,
  parseParams,
  toClassModels,
  validateDesign,
  type DesignerEdge,
  type DesignerState,
} from "./model";

function zoo(): DesignerState {
  const state = emptyDesign();
  const mascota = createNode(state, "interface");
  mascota.name = "Mascota";
  const ladrar = createMethod();
  ladrar.name = "ladrar";
  ladrar.returnTy = "String";
  mascota.methods.push(ladrar);

  state.nodes.push(mascota);
  const animal = createNode(state, "class");
  animal.name = "Animal";
  animal.isAbstract = true;
  const patas = createField();
  patas.name = "patas";
  patas.ty = "int";
  patas.visibility = "protected";
  animal.fields.push(patas);
  state.nodes.push(animal);

  const perro = createNode(state, "class");
  perro.name = "Perro";
  const nombre = createField();
  nombre.name = "nombre";
  nombre.ty = "String";
  perro.fields.push(nombre);
  state.nodes.push(perro);

  const edge = (from: string, to: string, kind: DesignerEdge["kind"]): DesignerEdge => ({
    id: `${from}-${to}-${kind}`,
    from,
    to,
    kind,
    label: "",
  });
  state.edges.push(edge(perro.id, animal.id, "extends"));
  state.edges.push(edge(perro.id, mascota.id, "implements"));
  return state;
}

describe("toClassModels", () => {
  it("convierte nodos y aristas en modelos del núcleo", () => {
    const models = toClassModels(zoo());
    expect(models.map((model) => model.name)).toEqual(["Mascota", "Animal", "Perro"]);

    const perro = models.find((model) => model.name === "Perro") as ClassModel;
    expect(perro.extends).toEqual(["Animal"]);
    expect(perro.implements).toEqual(["Mascota"]);
    expect(perro.fields[0]).toMatchObject({
      name: "nombre",
      ty: "String",
      visibility: "private",
      multiplicity: "one",
    });

    const mascota = models.find((model) => model.name === "Mascota") as ClassModel;
    expect(mascota.kind).toBe("interface");
    expect(mascota.methods[0]).toMatchObject({
      name: "ladrar",
      return_ty: "String",
      is_constructor: false,
      parameters: [],
    });

    const animal = models.find((model) => model.name === "Animal") as ClassModel;
    expect(animal.is_abstract).toBe(true);
  });

  it("ignora miembros sin nombre", () => {
    const state = emptyDesign();
    const node = createNode(state, "class");
    node.name = "Vacia";
    node.fields.push(createField());
    node.methods.push(createMethod());
    state.nodes.push(node);
    const models = toClassModels(state);
    expect(models[0].fields).toEqual([]);
    expect(models[0].methods).toEqual([]);
  });
});

describe("buildMermaid", () => {
  it("genera el diagrama con miembros y relaciones", () => {
    const mermaid = buildMermaid(zoo());
    expect(mermaid).toContain("classDiagram");
    expect(mermaid).toContain("class Mascota {");
    expect(mermaid).toContain("<<interface>>");
    expect(mermaid).toContain("+ladrar() String");
    expect(mermaid).toContain("<<abstract>>");
    expect(mermaid).toContain("#int patas");
    expect(mermaid).toContain("-String nombre");
    expect(mermaid).toContain("Animal <|-- Perro");
    expect(mermaid).toContain("Mascota <|.. Perro");
  });

  it("etiqueta las relaciones no jerárquicas", () => {
    const state = emptyDesign();
    const dueno = createNode(state, "class");
    dueno.name = "Dueno";
    const perro = createNode(state, "class");
    perro.name = "Perro";
    state.nodes.push(dueno, perro);
    state.edges.push({
      id: "e1",
      from: dueno.id,
      to: perro.id,
      kind: "composition",
      label: "tiene",
    });
    state.edges.push({ id: "e2", from: perro.id, to: dueno.id, kind: "dependency", label: "" });
    const mermaid = buildMermaid(state);
    expect(mermaid).toContain("Dueno *-- Perro : tiene");
    expect(mermaid).toContain("Perro ..> Dueno");
  });
});

describe("validateDesign", () => {
  it("acepta un diseño correcto", () => {
    expect(validateDesign(zoo())).toEqual([]);
  });

  it("detecta nombres inválidos, duplicados y ciclos", () => {
    const state = emptyDesign();
    const uno = createNode(state, "class");
    uno.name = "1Perro";
    state.nodes.push(uno);
    expect(validateDesign(state).join("\n")).toContain("identificador válido");

    const duplicado = emptyDesign();
    const a = createNode(duplicado, "class");
    a.name = "Perro";
    const b = createNode(duplicado, "class");
    b.name = "Perro";
    duplicado.nodes.push(a, b);
    expect(validateDesign(duplicado).join("\n")).toContain("dos clases llamadas");

    const ciclo = emptyDesign();
    const x = createNode(ciclo, "class");
    x.name = "X";
    const y = createNode(ciclo, "class");
    y.name = "Y";
    ciclo.nodes.push(x, y);
    ciclo.edges.push({ id: "e1", from: x.id, to: y.id, kind: "extends", label: "" });
    ciclo.edges.push({ id: "e2", from: y.id, to: x.id, kind: "extends", label: "" });
    expect(validateDesign(ciclo).join("\n")).toContain("ciclo de herencia");
  });

  it("detecta herencia y implementación incoherentes", () => {
    const state = emptyDesign();
    const interfaz = createNode(state, "interface");
    interfaz.name = "Mascota";
    const clase = createNode(state, "class");
    clase.name = "Perro";
    state.nodes.push(interfaz, clase);
    state.edges.push({ id: "e1", from: clase.id, to: interfaz.id, kind: "extends", label: "" });
    expect(validateDesign(state).join("\n")).toContain("usa «Implementa»");

    state.edges[0].kind = "implements";
    interfaz.kind = "class";
    expect(validateDesign(state).join("\n")).toContain("solo aplica a interfaces");
  });

  it("detecta métodos abstractos en clases concretas", () => {
    const state = emptyDesign();
    const clase = createNode(state, "class");
    clase.name = "Perro";
    const metodo = createMethod();
    metodo.name = "ladrar";
    metodo.returnTy = "String";
    metodo.isAbstract = true;
    clase.methods.push(metodo);
    state.nodes.push(clase);
    expect(validateDesign(state).join("\n")).toContain("sin marcarse como clase abstracta");
  });
});

describe("parseParams y formatParams", () => {
  it("convierte texto en parámetros y viceversa", () => {
    const params = parseParams("texto: String, edad:int, solo");
    expect(params).toHaveLength(3);
    expect(params[0]).toMatchObject({ name: "texto", ty: "String" });
    expect(params[1]).toMatchObject({ name: "edad", ty: "int" });
    expect(params[2]).toMatchObject({ name: "solo", ty: "" });
    expect(parseParams("  ")).toEqual([]);
    expect(formatParams(params.slice(0, 2))).toBe("texto: String, edad: int");
  });
});
