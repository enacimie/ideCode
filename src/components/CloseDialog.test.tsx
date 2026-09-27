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
});
