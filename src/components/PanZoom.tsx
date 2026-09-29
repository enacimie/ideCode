import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import "./PanZoom.css";

const MIN_SCALE = 0.1;
const MAX_SCALE = 4;
const DRAG_THRESHOLD = 3;

type View = { k: number; tx: number; ty: number };

type Props = {
  children: ReactNode;
  className?: string;
  fitKey?: unknown;
  controls?: boolean;
};

function zoomAt(view: View, mx: number, my: number, factor: number): View {
  const k = Math.min(MAX_SCALE, Math.max(MIN_SCALE, view.k * factor));
  const ratio = k / view.k;
  return { k, tx: mx - (mx - view.tx) * ratio, ty: my - (my - view.ty) * ratio };
}

export function PanZoom({ children, className, fitKey = null, controls = true }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const [view, setView] = useState<View>({ k: 1, tx: 0, ty: 0 });
  const viewRef = useRef(view);
  viewRef.current = view;
  const interacted = useRef(false);
  const pan = useRef<{ x: number; y: number; tx: number; ty: number } | null>(null);
  const moved = useRef(false);

  const fit = useCallback(() => {
    const host = hostRef.current;
    const content = contentRef.current;
    if (!host || !content) return;
    const cw = content.offsetWidth;
    const ch = content.offsetHeight;
    const hw = host.clientWidth;
    const hh = host.clientHeight;
    if (cw <= 0 || ch <= 0 || hw <= 0 || hh <= 0) return;
    const k = Math.min(MAX_SCALE, Math.max(MIN_SCALE, Math.min(hw / cw, hh / ch)));
    setView({ k, tx: (hw - cw * k) / 2, ty: (hh - ch * k) / 2 });
  }, []);

  useEffect(() => {
    interacted.current = false;
    fit();
  }, [fitKey, fit]);

  useEffect(() => {
    const content = contentRef.current;
    if (!content || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (!interacted.current) fit();
    });
    observer.observe(content);
    return () => observer.disconnect();
  }, [fit]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    function onWheel(event: WheelEvent) {
      event.preventDefault();
      const rect = host!.getBoundingClientRect();
      interacted.current = true;
      setView((current) =>
        zoomAt(
          current,
          event.clientX - rect.left,
          event.clientY - rect.top,
          Math.exp(-event.deltaY * 0.0015),
        ),
      );
    }
    host.addEventListener("wheel", onWheel, { passive: false });
    return () => host.removeEventListener("wheel", onWheel);
  }, []);

  function handlePointerDown(event: ReactPointerEvent<HTMLDivElement>) {
    pan.current = {
      x: event.clientX,
      y: event.clientY,
      tx: viewRef.current.tx,
      ty: viewRef.current.ty,
    };
    moved.current = false;
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Entornos sin soporte de captura de puntero (jsdom).
    }
  }

  function handlePointerMove(event: ReactPointerEvent<HTMLDivElement>) {
    const start = pan.current;
    if (!start) return;
    const dx = event.clientX - start.x;
    const dy = event.clientY - start.y;
    if (!moved.current && Math.abs(dx) + Math.abs(dy) <= DRAG_THRESHOLD) return;
    moved.current = true;
    interacted.current = true;
    setView((current) => ({ ...current, tx: start.tx + dx, ty: start.ty + dy }));
  }

  function endPan(event: ReactPointerEvent<HTMLDivElement>) {
    pan.current = null;
    try {
      event.currentTarget.releasePointerCapture(event.pointerId);
    } catch {
      // Idem.
    }
  }

  function handleClickCapture(event: ReactMouseEvent<HTMLDivElement>) {
    if (moved.current) {
      event.stopPropagation();
      event.preventDefault();
      moved.current = false;
    }
  }

  function zoomBy(factor: number) {
    const host = hostRef.current;
    if (!host) return;
    interacted.current = true;
    setView((current) => zoomAt(current, host.clientWidth / 2, host.clientHeight / 2, factor));
  }

  function actualSize() {
    const host = hostRef.current;
    const content = contentRef.current;
    if (!host || !content) return;
    interacted.current = true;
    setView({
      k: 1,
      tx: (host.clientWidth - content.offsetWidth) / 2,
      ty: (host.clientHeight - content.offsetHeight) / 2,
    });
  }

  return (
    <div
      ref={hostRef}
      className={`panzoom-host${className ? ` ${className}` : ""}`}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={endPan}
      onPointerCancel={endPan}
      onClickCapture={handleClickCapture}
    >
      {controls && (
        <div className="panzoom-controls" onPointerDown={(event) => event.stopPropagation()}>
          <button type="button" title="Acercar" aria-label="Acercar" onClick={() => zoomBy(1.25)}>
            +
          </button>
          <button type="button" title="Alejar" aria-label="Alejar" onClick={() => zoomBy(0.8)}>
            −
          </button>
          <button
            type="button"
            title="Ajustar al panel"
            onClick={() => {
              interacted.current = true;
              fit();
            }}
          >
            Ajustar
          </button>
          <button type="button" title="Tamaño real" onClick={actualSize}>
            {Math.round(view.k * 100)} %
          </button>
        </div>
      )}
      <div
        ref={contentRef}
        className="panzoom-content"
        style={{ transform: `translate(${view.tx}px, ${view.ty}px) scale(${view.k})` }}
      >
        {children}
      </div>
    </div>
  );
}
