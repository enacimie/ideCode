import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Menu, MenuItem, MenuSeparator } from "./Menu";

afterEach(cleanup);

function renderMenu(onSelect = vi.fn()) {
  render(
    <Menu label="Archivo">
      {(close) => (
        <>
          <MenuItem
            onSelect={() => {
              onSelect("abrir");
              close();
            }}
          >
            Abrir carpeta…
          </MenuItem>
          <MenuSeparator />
          <MenuItem
            hint="Ctrl+S"
            disabled
            onSelect={() => {
              onSelect("guardar");
              close();
            }}
          >
            Guardar
          </MenuItem>
        </>
      )}
    </Menu>,
  );
  return { onSelect };
}

function open() {
  fireEvent.click(screen.getByRole("button", { name: /Archivo/ }));
}

describe("Menu", () => {
  it("empieza cerrado y se abre al pulsar", () => {
    renderMenu();
    expect(screen.queryByRole("menu")).toBeNull();
    open();
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Abrir carpeta…" })).toBeTruthy();
  });

  it("ejecuta la acción y se cierra", () => {
    const { onSelect } = renderMenu();
    open();
    fireEvent.click(screen.getByRole("menuitem", { name: "Abrir carpeta…" }));
    expect(onSelect).toHaveBeenCalledWith("abrir");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("se cierra con Escape", () => {
    renderMenu();
    open();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("se cierra al pulsar fuera", () => {
    renderMenu();
    open();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("no abre si está deshabilitado", () => {
    render(
      <Menu label="Archivo" disabled>
        {() => <MenuItem onSelect={() => undefined}>Abrir</MenuItem>}
      </Menu>,
    );
    open();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("respeta los elementos deshabilitados", () => {
    renderMenu();
    open();
    const guardar = screen.getByRole("menuitem", { name: /Guardar/ }) as HTMLButtonElement;
    expect(guardar.disabled).toBe(true);
  });

  it("navega con las flechas entre los elementos", () => {
    render(
      <Menu label="Archivo">
        {() => (
          <>
            <MenuItem onSelect={() => undefined}>Uno</MenuItem>
            <MenuItem onSelect={() => undefined}>Dos</MenuItem>
          </>
        )}
      </Menu>,
    );
    open();

    const uno = screen.getByRole("menuitem", { name: "Uno" });
    const dos = screen.getByRole("menuitem", { name: "Dos" });
    expect(document.activeElement).toBe(uno);

    fireEvent.keyDown(document, { key: "ArrowDown" });
    expect(document.activeElement).toBe(dos);

    fireEvent.keyDown(document, { key: "ArrowDown" });
    expect(document.activeElement).toBe(uno);

    fireEvent.keyDown(document, { key: "ArrowUp" });
    expect(document.activeElement).toBe(dos);

    fireEvent.keyDown(document, { key: "Home" });
    expect(document.activeElement).toBe(uno);

    fireEvent.keyDown(document, { key: "End" });
    expect(document.activeElement).toBe(dos);
  });
});
