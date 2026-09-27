import { useEffect, useRef, useState, type ReactNode } from "react";
import "./Menu.css";

type MenuProps = {
  label: string;
  disabled?: boolean;
  children: (close: () => void) => ReactNode;
};

export function Menu({ label, disabled, children }: MenuProps) {
  const [open, setOpen] = useState(false);
  const host = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;

    function onMouseDown(event: MouseEvent) {
      if (host.current && !host.current.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setOpen(false);
        trigger.current?.focus();
      }
    }

    document.addEventListener("mousedown", onMouseDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const first = host.current?.querySelector<HTMLButtonElement>(".menu-item:not(:disabled)");
    first?.focus();
  }, [open]);

  return (
    <div className="menu" ref={host}>
      <button
        ref={trigger}
        type="button"
        className="menu-trigger"
        disabled={disabled}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        {label}
        <span className="menu-caret" aria-hidden="true">
          ▾
        </span>
      </button>
      {open && (
        <div className="menu-popup" role="menu">
          {children(() => setOpen(false))}
        </div>
      )}
    </div>
  );
}

type MenuItemProps = {
  children: ReactNode;
  hint?: string;
  disabled?: boolean;
  onSelect: () => void;
};

export function MenuItem({ children, hint, disabled, onSelect }: MenuItemProps) {
  return (
    <button
      type="button"
      role="menuitem"
      className="menu-item"
      disabled={disabled}
      onClick={onSelect}
    >
      <span>{children}</span>
      {hint && <span className="menu-hint">{hint}</span>}
    </button>
  );
}

export function MenuSeparator() {
  return <span className="menu-separator" role="separator" />;
}

export function MenuLabel({ children }: { children: ReactNode }) {
  return <span className="menu-label">{children}</span>;
}

export function MenuEmpty({ children }: { children: ReactNode }) {
  return <span className="menu-empty">{children}</span>;
}
