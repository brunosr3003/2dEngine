import { type ReactNode } from 'react';

type Props = {
  open: boolean;
  title: string;
  children: ReactNode;
  err?: string;
  saving?: boolean;
  onClose: () => void;
  onSave: () => void;
};

export function Modal({ open, title, children, err, saving, onClose, onSave }: Props) {
  if (!open) return null;
  return (
    <div className="modal-bg" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal" role="dialog" aria-modal>
        <h2>{title}</h2>
        <div>{children}</div>
        <div className="modal-actions">
          <button className="ghost" onClick={onClose} disabled={saving}>Cancelar</button>
          <button className="primary" onClick={onSave} disabled={saving}>
            {saving ? 'Salvando…' : 'Salvar'}
          </button>
        </div>
        {err && <div className="err">{err}</div>}
      </div>
    </div>
  );
}
