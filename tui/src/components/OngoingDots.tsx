import { useState, useEffect } from "react";
import { Text } from "ink";
import { colors } from "../lib/theme.js";

interface OngoingDotsProps {
  count?: number;
}

/**
 * Single shared frame counter — all OngoingDots instances share one timer
 * to avoid triggering N separate re-render intervals.
 */
let globalFrame = 0;
let listenerCount = 0;
let globalTimer: ReturnType<typeof setInterval> | null = null;
const listeners = new Set<() => void>();

function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  listenerCount++;
  if (!globalTimer) {
    globalTimer = setInterval(() => {
      globalFrame = (globalFrame + 1) % 7;
      for (const fn of listeners) fn();
    }, 300);
  }
  return () => {
    listeners.delete(cb);
    listenerCount--;
    if (listenerCount <= 0 && globalTimer) {
      clearInterval(globalTimer);
      globalTimer = null;
      listenerCount = 0;
    }
  };
}

/**
 * Animated pulsing dots indicator — terminal equivalent of the web's
 * CSS-animated OngoingDots. Shares a single global timer across all instances.
 */
export function OngoingDots({ count = 5 }: OngoingDotsProps) {
  const [frame, setFrame] = useState(globalFrame);

  useEffect(() => {
    return subscribe(() => setFrame(globalFrame));
  }, []);

  const dots: string[] = [];
  for (let i = 0; i < count; i++) {
    dots.push(i === frame % (count + 2) || i === (frame % (count + 2)) - 1 ? "●" : "·");
  }

  return (
    <Text color={colors.ongoing} bold>
      {dots.join("")}
    </Text>
  );
}
