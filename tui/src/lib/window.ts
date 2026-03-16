/**
 * Stable windowing for TUI lists — only scrolls when the selection
 * reaches the edge of the visible window (2-item margin).
 * Prevents "shaking" caused by center-following recalculation.
 */

const windowStarts = new Map<string, number>();

export function stableWindow(
  id: string,
  selectedIndex: number,
  totalItems: number,
  windowSize: number,
): { start: number; end: number } {
  const maxStart = Math.max(0, totalItems - windowSize);
  let start = windowStarts.get(id) ?? 0;

  // Clamp stored start to valid range
  start = Math.min(start, maxStart);

  // Only scroll if selection goes outside the visible window (with 1-item margin)
  if (selectedIndex < start + 1) {
    start = Math.max(0, selectedIndex - 1);
  } else if (selectedIndex >= start + windowSize - 1) {
    start = Math.min(maxStart, selectedIndex - windowSize + 2);
  }

  windowStarts.set(id, start);
  const end = Math.min(totalItems, start + windowSize);
  return { start, end };
}
