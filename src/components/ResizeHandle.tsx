import { useCallback, useRef } from "react";

interface ResizeHandleProps {
  /** Callback with the new width (px) of the panel to the LEFT of the handle */
  onResize: (width: number) => void;
  direction?: "horizontal" | "vertical";
}

export function ResizeHandle({ onResize, direction = "horizontal" }: ResizeHandleProps) {
  const handleRef = useRef<HTMLDivElement>(null);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const startX = e.clientX;
      const startY = e.clientY;
      const handle = handleRef.current;
      if (!handle) return;

      // Get the previous sibling's current size
      const prev = handle.previousElementSibling as HTMLElement | null;
      if (!prev) return;
      const startSize =
        direction === "horizontal" ? prev.getBoundingClientRect().width : prev.getBoundingClientRect().height;

      document.body.style.cursor = direction === "horizontal" ? "col-resize" : "row-resize";
      document.body.style.userSelect = "none";

      const onMove = (ev: MouseEvent) => {
        const delta = direction === "horizontal" ? ev.clientX - startX : ev.clientY - startY;
        const newSize = Math.max(100, startSize + delta);
        onResize(newSize);
      };

      const onUp = () => {
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      };

      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp);
    },
    [onResize, direction],
  );

  return (
    <div
      ref={handleRef}
      className={`resize-handle resize-handle--${direction}`}
      onMouseDown={onMouseDown}
    />
  );
}
