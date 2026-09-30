import { useEffect, useRef } from "react";
import "./CloseDialog.css";

type Props = {
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
};

export function CloseDialog({ onSave, onDiscard, onCancel }: Props) {
  const dialog = useRef<HTMLDivElement>(null);
  const primary = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    primary.current?.focus();
  }, []);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onCancel();
        return;
      }
      if (event.key !== "Tab" || !dialog.current) return;

      const focusables = Array.from(
        dialog.current.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"),
      );
      if (focusables.length === 0) return;

      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }

    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [onCancel]);

  return (
    <div className="close-backdrop">
      <div
        ref={dialog}
        className="close-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="close-title"
      >
        <h2 id="close-title">Cambios sin guardar</h2>
        <p>Tienes archivos modificados. ¿Qué quieres hacer antes de cerrar?</p>
        <div className="close-actions">
          <button ref={primary} type="button" className="primary" onClick={onSave}>
            Guardar y cerrar
          </button>
          <button type="button" className="danger" onClick={onDiscard}>
            Cerrar sin guardar
          </button>
          <button type="button" onClick={onCancel}>
            Cancelar
          </button>
        </div>
      </div>
    </div>
  );
}
