import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { openUrl } from "./lib/openUrl";
import { applyFontScale, readStoredFontScale } from "./lib/fontScale";
import { App } from "./App";
import "./styles/global.css";

// Apply the persisted scale before React mounts. Virtualized lists measure
// themselves during mount and must see the final zoom from their first frame.
applyFontScale(readStoredFontScale());

// Intercept all link clicks and open external URLs in the system browser
document.addEventListener("click", (e) => {
  const anchor = (e.target as HTMLElement).closest("a");
  if (!anchor) return;
  const href = anchor.getAttribute("href");
  if (href && (href.startsWith("http://") || href.startsWith("https://"))) {
    e.preventDefault();
    openUrl(href);
  }
});

const rootEl = document.getElementById("root");
if (!rootEl) throw new Error("Missing #root element");

createRoot(rootEl).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
