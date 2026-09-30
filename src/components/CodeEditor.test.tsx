import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { CodeEditor } from "./CodeEditor";

afterEach(cleanup);

beforeAll(() => {
  const rect = {
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    bottom: 0,
    right: 0,
    width: 0,
    height: 0,
    toJSON: () => ({}),
  } as DOMRect;
  const list = {
    length: 0,
    item: () => null,
    [Symbol.iterator]: function* () {},
  } as unknown as DOMRectList;

  Range.prototype.getBoundingClientRect = () => rect;
  Range.prototype.getClientRects = () => list;
  Element.prototype.getClientRects = () => list;
});

function contentOf(container: HTMLElement): string {
  return container.querySelector(".cm-content")?.textContent ?? "";
}

describe("CodeEditor", () => {
  it("muestra el contenido inicial del archivo", () => {
    const { container } = render(
      <CodeEditor path="/p/A.java" name="A.java" value="class A {}" onChange={() => undefined} />,
    );
    expect(contentOf(container)).toContain("class A {}");
  });

  it("actualiza el documento cuando cambia el valor externo", () => {
    const { container, rerender } = render(
      <CodeEditor path="/p/A.java" name="A.java" value="uno" onChange={() => undefined} />,
    );
    rerender(<CodeEditor path="/p/A.java" name="A.java" value="dos" onChange={() => undefined} />);
    expect(contentOf(container)).toContain("dos");
    expect(contentOf(container)).not.toContain("uno");
  });

  it("enfoca el editor al revelar una línea", () => {
    const { container } = render(
      <CodeEditor
        path="/p/A.java"
        name="A.java"
        value={"linea1\nlinea2\nlinea3"}
        reveal={{ path: "/p/A.java", line: 2, nonce: 1 }}
        onChange={() => undefined}
      />,
    );
    expect(contentOf(container)).toContain("linea2");
    expect(document.activeElement?.classList.contains("cm-content")).toBe(true);
  });
});
