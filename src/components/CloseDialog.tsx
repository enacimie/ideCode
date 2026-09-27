import "./CloseDialog.css";

type Props = {
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
};

export function CloseDialog({ onSave, onDiscard, onCancel }: Props) {
  return (
    <div className="close-backdrop">
      <div className="close-dialog" role="dialog" aria-modal="true" aria-labelledby="close-title">
        <h2 id="close-title">Cambios sin guardar</h2>
        <p>Tienes archivos modificados. ¿Qué quieres hacer antes de cerrar?</p>
        <div className="close-actions">
          <button type="button" className="primary" onClick={onSave}>
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
