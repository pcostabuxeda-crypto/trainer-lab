import { useEffect, useRef } from "react";
import { Button } from "./Button";

type Props = {
  title: string;
  message: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
};

/** Diàleg modal amb el <dialog> natiu: gestiona el focus i la tecla Esc. */
export function ConfirmDialog({ title, message, confirmLabel, onConfirm, onCancel }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    ref.current?.showModal();
  }, []);

  return (
    <dialog
      ref={ref}
      onCancel={onCancel}
      className="m-auto max-w-md rounded-xl border border-warn/40 bg-panel p-0 text-ink backdrop:bg-black/60"
    >
      <div className="p-5">
        <h2 className="font-semibold text-warn">{title}</h2>
        <p className="mt-2 text-sm text-muted">{message}</p>
      </div>
      <div className="flex justify-end gap-2 border-t border-line p-3">
        <Button onClick={onCancel} autoFocus>
          Cancel·la
        </Button>
        <Button variant="primary" onClick={onConfirm}>
          {confirmLabel}
        </Button>
      </div>
    </dialog>
  );
}
