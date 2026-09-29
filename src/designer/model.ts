import type { ClassModel, FieldModel, MethodModel, Visibility } from "../types";

export type RelationKind =
  "extends" | "implements" | "association" | "aggregation" | "composition" | "dependency";

export const RELATION_LABELS: Record<RelationKind, string> = {
  extends: "Herencia",
  implements: "Implementa",
  association: "Asociación",
  aggregation: "Agregación",
  composition: "Composición",
  dependency: "Dependencia",
};

export type DesignerField = {
  id: string;
  name: string;
  ty: string;
  visibility: Visibility;
  isStatic: boolean;
  isFinal: boolean;
};

export type DesignerParam = {
  id: string;
  name: string;
  ty: string;
};

export type DesignerMethod = {
  id: string;
  name: string;
  returnTy: string;
  visibility: Visibility;
  isStatic: boolean;
  isAbstract: boolean;
  params: DesignerParam[];
};

export type DesignerNode = {
  id: string;
  name: string;
  kind: "class" | "interface";
  isAbstract: boolean;
  x: number;
  y: number;
  fields: DesignerField[];
  methods: DesignerMethod[];
};

export type DesignerEdge = {
  id: string;
  from: string;
  to: string;
  kind: RelationKind;
  label: string;
};

export type DesignerState = {
  nodes: DesignerNode[];
  edges: DesignerEdge[];
};

const STORAGE_KEY = "idecode.designer.v1";

let sequence = 0;

export function newId(prefix: string): string {
  sequence += 1;
  return `${prefix}-${Date.now().toString(36)}-${sequence}`;
}

export function emptyDesign(): DesignerState {
  return { nodes: [], edges: [] };
}

const IDENTIFIER = /^[\p{L}_][\p{L}\p{N}_]*$/u;

export function isIdentifier(name: string): boolean {
  return IDENTIFIER.test(name);
}

export function defaultName(state: DesignerState, kind: "class" | "interface"): string {
  const base = kind === "class" ? "Clase" : "Interfaz";
  let candidate = 1;
  const taken = new Set(state.nodes.map((node) => node.name));
  while (taken.has(`${base}${candidate}`)) {
    candidate += 1;
  }
  return `${base}${candidate}`;
}

export function createNode(state: DesignerState, kind: "class" | "interface"): DesignerNode {
  return {
    id: newId(kind === "class" ? "clase" : "interfaz"),
    name: defaultName(state, kind),
    kind,
    isAbstract: false,
    x: 40 + (state.nodes.length % 4) * 60,
    y: 40 + (state.nodes.length % 4) * 40,
    fields: [],
    methods: [],
  };
}

export function createField(): DesignerField {
  return {
    id: newId("campo"),
    name: "",
    ty: "",
    visibility: "private",
    isStatic: false,
    isFinal: false,
  };
}

export function createMethod(): DesignerMethod {
  return {
    id: newId("metodo"),
    name: "",
    returnTy: "void",
    visibility: "public",
    isStatic: false,
    isAbstract: false,
    params: [],
  };
}

export function parseParams(text: string): DesignerParam[] {
  const params: DesignerParam[] = [];
  let current = "";
  let depth = 0;
  for (const char of text) {
    if (char === "<") depth++;
    if (char === ">") depth = Math.max(0, depth - 1);
    if ((char === "," || char === ";") && depth === 0) {
      params.push(parseParamChunk(current));
      current = "";
    } else {
      current += char;
    }
  }
  params.push(parseParamChunk(current));
  return params.filter((param) => param.name.length > 0);
}

function parseParamChunk(chunk: string): DesignerParam {
  const trimmed = chunk.trim();
  const separator = trimmed.indexOf(":");
  if (separator < 0) {
    return { id: newId("param"), name: trimmed, ty: "" };
  }
  return {
    id: newId("param"),
    name: trimmed.slice(0, separator).trim(),
    ty: trimmed.slice(separator + 1).trim(),
  };
}

export function formatParams(params: DesignerParam[]): string {
  return params.map((param) => (param.ty ? `${param.name}: ${param.ty}` : param.name)).join(", ");
}

export function toClassModels(state: DesignerState): ClassModel[] {
  const nameOf = new Map(state.nodes.map((node) => [node.id, node.name]));
  return state.nodes.map((node) => {
    const fields: FieldModel[] = node.fields
      .filter((field) => field.name.trim().length > 0)
      .map((field) => ({
        name: field.name.trim(),
        ty: field.ty.trim(),
        targets: [],
        multiplicity: "one",
        visibility: field.visibility,
        is_static: field.isStatic,
        is_final: field.isFinal,
        line: 1,
      }));
    const methods: MethodModel[] = node.methods
      .filter((method) => method.name.trim().length > 0)
      .map((method) => ({
        name: method.name.trim(),
        return_ty: method.returnTy.trim(),
        visibility: method.visibility,
        is_static: method.isStatic,
        is_abstract: method.isAbstract,
        is_constructor: false,
        is_async: false,
        type_parameters: [],
        throws: [],
        parameters: method.params
          .filter((param) => param.name.trim().length > 0)
          .map((param) => ({ name: param.name.trim(), ty: param.ty.trim() })),
        line: 1,
      }));
    const related = (kind: RelationKind): string[] =>
      state.edges
        .filter((edge) => edge.kind === kind && edge.from === node.id)
        .map((edge) => nameOf.get(edge.to) ?? "")
        .filter((name) => name.length > 0);
    return {
      name: node.name,
      simple_name: node.name,
      outer: null,
      kind: node.kind,
      package: null,
      is_abstract: node.isAbstract,
      type_parameters: [],
      extends: related("extends"),
      implements: related("implements"),
      enum_constants: [],
      fields,
      methods,
      functions: [],
      uses: [],
      line: 1,
      file: "",
    } satisfies ClassModel;
  });
}

function sanitize(name: string): string {
  const cleaned = name.replace(/[^\p{L}\p{N}_]/gu, "_");
  return cleaned.length > 0 ? cleaned : "_";
}

export function visibilityMarker(visibility: Visibility): string {
  switch (visibility) {
    case "public":
      return "+";
    case "protected":
      return "#";
    case "package":
      return "~";
    case "private":
      return "-";
  }
}

export function buildMermaid(state: DesignerState): string {
  const lines: string[] = ["classDiagram", "    direction TB"];
  const used = new Map<string, number>();
  const nameOf = new Map(state.nodes.map((node) => {
    const base = sanitize(node.name);
    const count = used.get(base) ?? 0;
    used.set(base, count + 1);
    return [node.id, count > 0 ? `${base}_${count}` : base];
  }));

  for (const node of state.nodes) {
    const name = nameOf.get(node.id) ?? sanitize(node.name);
    lines.push(`    class ${name} {`);
    if (node.kind === "interface") {
      lines.push("        <<interface>>");
    } else if (node.isAbstract) {
      lines.push("        <<abstract>>");
    }
    for (const field of node.fields) {
      if (!field.name.trim()) continue;
      const type = field.ty.trim() ? `${field.ty.trim()} ` : "";
      lines.push(
        `        ${visibilityMarker(field.visibility)}${type}${sanitize(field.name)}${field.isStatic ? "$" : ""}`,
      );
    }
    for (const method of node.methods) {
      if (!method.name.trim()) continue;
      const params = method.params
        .filter((param) => param.name.trim())
        .map((param) =>
          param.ty.trim() ? `${param.ty.trim()} ${param.name.trim()}` : param.name.trim(),
        )
        .join(", ");
      const returns = method.returnTy.trim() ? ` ${method.returnTy.trim()}` : "";
      const marker = method.isStatic ? "$" : method.isAbstract ? "*" : "";
      lines.push(
        `        ${visibilityMarker(method.visibility)}${sanitize(method.name)}(${params})${returns}${marker}`,
      );
    }
    lines.push("    }");
  }

  for (const edge of state.edges) {
    const from = nameOf.get(edge.from);
    const to = nameOf.get(edge.to);
    if (!from || !to) continue;
    const label = edge.label.trim() ? ` : ${edge.label.trim().replace(/\s+/g, " ")}` : "";
    switch (edge.kind) {
      case "extends":
        lines.push(`    ${to} <|-- ${from}`);
        break;
      case "implements":
        lines.push(`    ${to} <|.. ${from}`);
        break;
      case "association":
        lines.push(`    ${from} --> ${to}${label}`);
        break;
      case "aggregation":
        lines.push(`    ${from} o-- ${to}${label}`);
        break;
      case "composition":
        lines.push(`    ${from} *-- ${to}${label}`);
        break;
      case "dependency":
        lines.push(`    ${from} ..> ${to}${label}`);
        break;
    }
  }

  return `${lines.join("\n")}\n`;
}

export function validateDesign(state: DesignerState): string[] {
  const problems: string[] = [];
  const nodeById = new Map(state.nodes.map((node) => [node.id, node]));
  const seen = new Set<string>();

  for (const node of state.nodes) {
    const name = node.name.trim();
    if (!name) {
      problems.push("Hay una clase sin nombre.");
      continue;
    }
    if (!isIdentifier(name)) {
      problems.push(`«${name}» no es un identificador válido.`);
    }
    if (seen.has(name)) {
      problems.push(`Hay dos clases llamadas «${name}».`);
    }
    seen.add(name);

    const fieldNames = new Set<string>();
    for (const field of node.fields) {
      if (!field.name.trim()) {
        problems.push(`«${name}» tiene un campo sin nombre.`);
        continue;
      }
      if (!isIdentifier(field.name.trim())) {
        problems.push(`«${name}.${field.name.trim()}» no es un identificador válido.`);
      }
      if (!field.ty.trim()) {
        problems.push(`El campo «${name}.${field.name.trim()}» necesita un tipo.`);
      }
      if (fieldNames.has(field.name.trim())) {
        problems.push(`«${name}» repite el campo «${field.name.trim()}».`);
      }
      fieldNames.add(field.name.trim());
    }

    const signatures = new Set<string>();
    for (const method of node.methods) {
      if (!method.name.trim()) {
        problems.push(`«${name}» tiene un método sin nombre.`);
        continue;
      }
      if (!isIdentifier(method.name.trim())) {
        problems.push(`«${name}.${method.name.trim()}» no es un identificador válido.`);
      }
      if (!method.returnTy.trim()) {
        problems.push(`El método «${name}.${method.name.trim()}» necesita un tipo de retorno.`);
      }
      for (const param of method.params) {
        if (!param.name.trim()) {
          problems.push(`«${name}.${method.name.trim()}» tiene un parámetro sin nombre.`);
        } else if (!param.ty.trim()) {
          problems.push(
            `El parámetro «${param.name.trim()}» de «${name}.${method.name.trim()}» necesita un tipo.`,
          );
        }
      }
      const signature = `${method.name.trim()}/${method.params
        .map((param) => param.ty.trim())
        .join(",")}`;
      if (signatures.has(signature)) {
        problems.push(
          `«${name}» repite el método «${method.name.trim()}» con el mismo número de parámetros.`,
        );
      }
      signatures.add(signature);
    }

    if (node.kind === "interface" && node.isAbstract) {
      problems.push(`«${name}» es una interfaz; no necesita marcarse como abstracta.`);
    }
    if (node.kind === "class" && !node.isAbstract) {
      for (const method of node.methods) {
        if (method.isAbstract && method.name.trim()) {
          problems.push(
            `«${name}» tiene el método abstracto «${method.name.trim()}» sin marcarse como clase abstracta.`,
          );
        }
      }
    }
  }

  const extendsByNode = new Map<string, string[]>();
  for (const edge of state.edges) {
    const source = nodeById.get(edge.from);
    const target = nodeById.get(edge.to);
    if (!source || !target) continue;
    if (edge.from === edge.to) {
      problems.push(
        `«${source.name}» no puede relacionarse consigo misma (${RELATION_LABELS[edge.kind]}).`,
      );
      continue;
    }
    if (edge.kind === "extends") {
      const list = extendsByNode.get(edge.from) ?? [];
      list.push(edge.to);
      extendsByNode.set(edge.from, list);
      if (source.kind === "class" && target.kind === "interface") {
        problems.push(
          `«${source.name}» hereda de «${target.name}», que es una interfaz; usa «Implementa».`,
        );
      }
      if (source.kind === "interface" && target.kind === "class") {
        problems.push(`«${source.name}» es una interfaz y solo puede extender interfaces.`);
      }
    }
    if (edge.kind === "implements" && target.kind === "class") {
      problems.push(
        `«${source.name}» implementa «${target.name}», que es una clase; «Implementa» solo aplica a interfaces.`,
      );
    }
  }

  for (const node of state.nodes) {
    if ((extendsByNode.get(node.id) ?? []).length > 1) {
      problems.push(`«${node.name}» hereda de varias clases; Java y Kotlin solo permiten una.`);
    }
  }

  const visiting = new Set<string>();
  const done = new Set<string>();
  const hasCycle = (id: string): boolean => {
    if (visiting.has(id)) return true;
    if (done.has(id)) return false;
    visiting.add(id);
    for (const edge of state.edges) {
      if (edge.from !== id) continue;
      if (edge.kind !== "extends" && edge.kind !== "implements") continue;
      if (hasCycle(edge.to)) return true;
    }
    visiting.delete(id);
    done.add(id);
    return false;
  };
  for (const node of state.nodes) {
    if (hasCycle(node.id)) {
      problems.push(`La jerarquía de «${node.name}» forma un ciclo de herencia.`);
      break;
    }
  }

  return problems;
}

function isDesignerState(value: unknown): value is DesignerState {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as { nodes?: unknown; edges?: unknown };
  if (!Array.isArray(candidate.nodes) || !Array.isArray(candidate.edges)) return false;
  return candidate.nodes.every(isDesignerNode) && candidate.edges.every(isDesignerEdge);
}

function isDesignerNode(value: unknown): boolean {
  if (typeof value !== "object" || value === null) return false;
  const node = value as Record<string, unknown>;
  return (
    typeof node.id === "string" &&
    typeof node.name === "string" &&
    (node.kind === "class" || node.kind === "interface") &&
    typeof node.isAbstract === "boolean" &&
    typeof node.x === "number" &&
    typeof node.y === "number" &&
    Array.isArray(node.fields) &&
    Array.isArray(node.methods)
  );
}

function isDesignerEdge(value: unknown): boolean {
  if (typeof value !== "object" || value === null) return false;
  const edge = value as Record<string, unknown>;
  return (
    typeof edge.id === "string" &&
    typeof edge.from === "string" &&
    typeof edge.to === "string" &&
    typeof edge.kind === "string" &&
    typeof edge.label === "string"
  );
}

export function loadDesign(): DesignerState | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    return isDesignerState(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

export function saveDesign(state: DesignerState): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    /* almacenamiento no disponible */
  }
}
