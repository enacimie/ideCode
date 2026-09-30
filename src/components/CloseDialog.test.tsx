import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CloseDialog } from "./CloseDialog";

afterEach(cleanup);

describe("CloseDialog", () => {
  it("explica el cierre pendiente y ofrece las tres salidas", () => {
    const onSave = vi.fn();
    const onDiscard = vi.fn();
    const onCancel = vi.fn();
    render(<CloseDialog onSave={onSave} onDiscard={onDiscard} onCancel={onCancel} />);

    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(screen.getByText("Cambios sin guardar")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Guardar y cerrar" }));
    expect(onSave).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "Cerrar sin guardar" }));
    expect(onDiscard).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("enfoca la opción principal y permite cancelar con Escape", () => {
    const onCancel = vi.fn();
    render(<CloseDialog onSave={vi.fn()} onDiscard={vi.fn()} onCancel={onCancel} />);

    const save = screen.getByRole("button", { name: "Guardar y cerrar" });
    expect(document.activeElement).toBe(save);

    fireEvent.keyDown(document, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("atrapa el foco dentro del diálogo con el tabulador", () => {
    render(<CloseDialog onSave={vi.fn()} onDiscard={vi.fn()} onCancel={vi.fn()} />);
    const save = screen.getByRole("button", { name: "Guardar y cerrar" });
    const cancel = screen.getByRole("button", { name: "Cancelar" });

    cancel.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(save);

    save.focus();
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(cancel);
  });
});
