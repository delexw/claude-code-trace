import { Text } from "ink";
import { colors } from "../lib/theme.js";

/** Static green dot — no animation, no timer, no re-renders. */
export function OngoingDot() {
  return (
    <Text color={colors.ongoing} bold>
      ●
    </Text>
  );
}

/** Static "active" indicator — no animation to avoid re-render shaking. */
export function BrailleSpinner() {
  return (
    <Text color={colors.ongoing} bold>
      ● active
    </Text>
  );
}
