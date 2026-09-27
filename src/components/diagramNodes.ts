import type { ClassModel } from "../types";

export function classNameFromNodeId(id: string): string | null {
  const marker = "-classId-";
  const at = id.lastIndexOf(marker);
  if (at === -1) return null;
  const name = id.slice(at + marker.length).replace(/-\d+$/, "");
  return name.length > 0 ? name : null;
}

export type MemberTarget = {
  element: Element;
  line: number;
};

export function collectMemberTargets(node: Element, model: ClassModel): MemberTarget[] {
  const targets: MemberTarget[] = [];
  const dataLines = [
    ...model.enum_constants.map((constant) => constant.line),
    ...model.fields.map((field) => field.line),
  ];
  const dataRows = Array.from(node.querySelectorAll("g.members-group > g.label"));
  dataRows.forEach((element, index) => {
    const line = dataLines[index];
    if (line !== undefined) {
      targets.push({ element, line });
    }
  });

  const methodRows = Array.from(node.querySelectorAll("g.methods-group > g.label"));
  const callables = [...model.methods, ...model.functions];
  methodRows.forEach((element, index) => {
    const line = callables[index]?.line;
    if (line !== undefined) {
      targets.push({ element, line });
    }
  });

  return targets;
}
