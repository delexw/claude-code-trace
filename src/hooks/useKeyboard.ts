import { useEffect, useRef } from "react";

export function useKeyboard(keyMap: Record<string, () => void>) {
  const keyMapRef = useRef(keyMap);
  // Refresh after commit rather than during render — writing a ref while rendering is
  // unsafe under concurrent rendering, and the map is only read from the keydown
  // listener, which cannot fire before the commit anyway.
  useEffect(() => {
    keyMapRef.current = keyMap;
  });

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) {
        return;
      }
      const action = keyMapRef.current[e.key];
      if (action) {
        e.preventDefault();
        action();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);
}
