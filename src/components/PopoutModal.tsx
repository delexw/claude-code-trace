import { useState, useEffect, useCallback, useRef, type ReactNode } from "react";

interface PopoutModalProps {
  onClose: () => void;
  header: ReactNode;
  children: ReactNode;
  initialWidth?: number;
  initialHeight?: number;
}

export function PopoutModal({
  onClose,
  header,
  children,
  initialWidth,
  initialHeight,
}: PopoutModalProps) {
  const [size, setSize] = useState(() => ({
    width: initialWidth ?? Math.round(window.innerWidth * 0.8),
    height: initialHeight ?? Math.round(window.innerHeight * 0.8),
  }));
  const resizing = useRef<{ startX: number; startY: number; startW: number; startH: number } | null>(null);
  const justResized = useRef(false);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  const onResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    resizing.current = { startX: e.clientX, startY: e.clientY, startW: size.width, startH: size.height };
    justResized.current = true;

    const onMouseMove = (ev: MouseEvent) => {
      if (!resizing.current) return;
      const newW = Math.max(400, resizing.current.startW + (ev.clientX - resizing.current.startX));
      const newH = Math.max(300, resizing.current.startH + (ev.clientY - resizing.current.startY));
      setSize({ width: newW, height: newH });
    };

    const onMouseUp = () => {
      resizing.current = null;
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
      setTimeout(() => { justResized.current = false; }, 100);
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, [size]);

  const handleOverlayClick = useCallback(() => {
    if (justResized.current) return;
    onClose();
  }, [onClose]);

  return (
    <div className="popout-overlay" onClick={handleOverlayClick}>
      <div
        className="popout-modal"
        style={{ width: size.width, height: size.height }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="popout-modal__header">
          {header}
          <button className="popout-modal__close" onClick={onClose}>{"\u2715"}</button>
        </div>
        <div className="popout-modal__body">
          {children}
        </div>
        <div className="popout-modal__resize-handle" onMouseDown={onResizeStart} />
      </div>
    </div>
  );
}
