import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { OutputPanel } from "./OutputPanel";
import type { Diagnostic, OutputLine } from "../types";

afterEach(cleanup);

const lines: OutputLine[] = [
  { stream: "stdout", line: "uno" },
  { stream: "stdout", line: "dos" },
];

const diagnostics: Diagnostic[] = [
  { severity: "error", message: "fallo", file: "A.java", line: 3, column: null },
];

describe("OutputPanel", () => {
  it("desplaza la salida hasta el final al recibir líneas", () => {
    Object.defineProperty(window.HTMLElement.prototype, "scrollHeight", {
      configurable: true,
      value: 777,
    });
    const { container } = render(
      <OutputPanel
        diagnostics={[]}
        diagnosticsStale={false}
        lines={lines}
        running={false}
        tab="output"
        onTabChange={() => undefined}
      />,
    );
    const body = container.querySelector<HTMLElement>(".output-body");
    expect(body).not.toBeNull();
    expect(body!.scrollTop).toBe(777);
    delete (window.HTMLElement.prototype as { scrollHeight?: number }).scrollHeight;
  });

  it("marca los diagnósticos obsoletos tras editar", () => {
    const { container, rerender } = render(
      <OutputPanel
        diagnostics={diagnostics}
        diagnosticsStale={false}
        lines={[]}
        running={false}
        tab="problems"
        onTabChange={() => undefined}
      />,
    );
    expect(container.querySelector(".stale-dot")).toBeNull();

    rerender(
      <OutputPanel
        diagnostics={diagnostics}
        diagnosticsStale
        lines={[]}
        running={false}
        tab="problems"
        onTabChange={() => undefined}
      />,
    );
    const dot = container.querySelector(".stale-dot");
    expect(dot).not.toBeNull();
    expect(dot!.getAttribute("title")).toMatch(/editado/i);
  });

  it("no marca obsoletos los diagnósticos cuando no hay ninguno", () => {
    const { container } = render(
      <OutputPanel
        diagnostics={[]}
        diagnosticsStale
        lines={[]}
        running={false}
        tab="problems"
        onTabChange={() => undefined}
      />,
    );
    expect(container.querySelector(".stale-dot")).toBeNull();
  });
});
