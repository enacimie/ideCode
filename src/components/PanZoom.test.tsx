import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PanZoom } from "./PanZoom";

afterEach(cleanup);

function mockSize(element: HTMLElement, width: number, height: number) {
  for (const key of ["offsetWidth", "clientWidth"]) {
    Object.defineProperty(element, key, { value: width, configurable: true });
  }
  for (const key of ["offsetHeight", "clientHeight"]) {
    Object.defineProperty(element, key, { value: height, configurable: true });
  }
}

function setup(contentWidth = 800, contentHeight = 400) {
  const utils = render(
    <PanZoom>
      <div>contenido</div>
    </PanZoom>,
  );
  const host = utils.container.querySelector<HTMLElement>(".panzoom-host")!;
  const content = utils.container.querySelector<HTMLElement>(".panzoom-content")!;
  mockSize(host, 400, 400);
  mockSize(content, contentWidth, contentHeight);
  return { ...utils, host, content };
}

describe("PanZoom", () => {
  it("acerca con la rueda alrededor del cursor", () => {
    const { host, content } = setup();
    fireEvent.wheel(host, { deltaY: -200, clientX: 50, clientY: 40 });
    expect(content.style.transform).toMatch(/scale\(1\.34/);
  });

  it("desplaza arrastrando con el puntero", () => {
    const { host, content } = setup();
    fireEvent.pointerDown(host, { clientX: 10, clientY: 10 });
    fireEvent.pointerMove(host, { clientX: 35, clientY: 25 });
    fireEvent.pointerUp(host, { clientX: 35, clientY: 25 });
    expect(content.style.transform).toBe("translate(25px, 15px) scale(1)");
  });

  it("ajusta el contenido al panel con el botón Ajustar", () => {
    const { host, content } = setup();
    fireEvent.click(screen.getByRole("button", { name: "Ajustar" }));
    expect(content.style.transform).toBe("translate(0px, 100px) scale(0.5)");
    expect(host.querySelector(".panzoom-controls")!.textContent).toContain("50 %");
  });

  it("los botones + y − zooman paso a paso", () => {
    const { content } = setup();
    fireEvent.click(screen.getByRole("button", { name: "Acercar" }));
    expect(content.style.transform).toMatch(/scale\(1\.25\)/);
    fireEvent.click(screen.getByRole("button", { name: "Alejar" }));
    expect(content.style.transform).toMatch(/scale\(1\)/);
  });

  it("suprime el clic cuando el arrastre termina sobre un elemento", () => {
    const onClick = vi.fn();
    const { container } = render(
      <PanZoom>
        <button type="button" onClick={onClick}>
          nodo
        </button>
      </PanZoom>,
    );
    const host = container.querySelector<HTMLElement>(".panzoom-host")!;
    const button = screen.getByRole("button", { name: "nodo" });
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
    fireEvent.pointerDown(host, { clientX: 0, clientY: 0 });
    fireEvent.pointerMove(host, { clientX: 30, clientY: 0 });
    fireEvent.pointerUp(host, { clientX: 30, clientY: 0 });
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("no se mueve con arrastres por debajo del umbral", () => {
    const { host, content } = setup();
    fireEvent.pointerDown(host, { clientX: 10, clientY: 10 });
    fireEvent.pointerMove(host, { clientX: 11, clientY: 11 });
    fireEvent.pointerUp(host, { clientX: 11, clientY: 11 });
    expect(content.style.transform).toBe("translate(0px, 0px) scale(1)");
  });
});
