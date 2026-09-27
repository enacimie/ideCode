import { describe, expect, it } from "vitest";
import { classNameFromNodeId, collectMemberTargets } from "./diagramNodes";
import type { ClassModel } from "../types";

function module(name: string): ClassModel {
  return {
    name,
    simple_name: name,
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
    functions: [
      { name: "saludar", return_ty: "", is_async: false, parameters: [], line: 3 },
      { name: "main", return_ty: "", is_async: false, parameters: [], line: 8 },
    ],
    uses: [],
    line: 1,
    file: `${name}.py`,
  };
}

describe("classNameFromNodeId", () => {
  it("reads the class name Mermaid puts in a node id", () => {
    expect(classNameFromNodeId("idecode-diagram-abc-classId-Animal-0")).toBe("Animal");
    expect(classNameFromNodeId("idecode-diagram-abc-classId-Caja-2")).toBe("Caja");
    expect(classNameFromNodeId("x-classId-Clase2-10")).toBe("Clase2");
  });

  it("refuses ids that are not class nodes", () => {
    expect(classNameFromNodeId("idecode-diagram-abc")).toBeNull();
    expect(classNameFromNodeId("")).toBeNull();
    expect(classNameFromNodeId("x-classId--0")).toBeNull();
  });
});

describe("collectMemberTargets", () => {
  it("maps free-function rows to their line", () => {
    const rows = [
      document.createElementNS("http://www.w3.org/2000/svg", "g"),
      document.createElementNS("http://www.w3.org/2000/svg", "g"),
    ];
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.setAttribute("class", "methods-group");
    rows.forEach((row) => {
      row.setAttribute("class", "label");
      group.appendChild(row);
    });
    const node = document.createElementNS("http://www.w3.org/2000/svg", "g");
    node.appendChild(group);

    const targets = collectMemberTargets(node, module("utilidades"));
    expect(targets.map((target) => target.line)).toEqual([3, 8]);
    expect(targets).toHaveLength(2);
  });
});
