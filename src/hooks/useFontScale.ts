import { useState, useEffect, useCallback } from "react";
import {
  DEFAULT_FONT_SCALE,
  applyFontScale,
  clampFontScale,
  readStoredFontScale,
  storeFontScale,
} from "../lib/fontScale";

/**
 * Manage the global UI zoom level. Reads the persisted value on mount, applies
 * it to the document root, and persists every change. Returns the current scale
 * and a setter that clamps to the supported range.
 */
export function useFontScale(): [number, (scale: number) => void] {
  // Initialise from storage lazily so the first paint already uses the saved
  // zoom (guarded for non-DOM environments inside readStoredFontScale).
  const [scale, setScale] = useState<number>(() => readStoredFontScale() || DEFAULT_FONT_SCALE);

  useEffect(() => {
    applyFontScale(scale);
  }, [scale]);

  const updateScale = useCallback((next: number) => {
    const clamped = clampFontScale(next);
    setScale(clamped);
    storeFontScale(clamped);
  }, []);

  return [scale, updateScale];
}
