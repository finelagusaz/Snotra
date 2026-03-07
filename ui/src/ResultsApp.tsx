import { type Component, onMount, onCleanup } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import ResultsWindow from "./components/ResultsWindow";
import { applyTheme } from "./lib/theme";
import type { VisualConfig } from "./lib/types";
import { getBootstrapPayload } from "./lib/invoke";

const ResultsApp: Component = () => {
  const unlistenFns: Array<() => void> = [];

  onMount(async () => {
    // Listen for visual config changes
    const unlistenVisual = await listen<VisualConfig>("visual-config-changed", (event) => {
      applyTheme(event.payload);
    });
    unlistenFns.push(unlistenVisual);

    // Load bootstrap payload and apply theme (non-fatal on failure)
    try {
      const bootstrap = await getBootstrapPayload();
      applyTheme(bootstrap.visual);
    } catch (e) {
      console.error("Failed to load bootstrap payload:", e);
    }
  });

  onCleanup(() => {
    for (const unlisten of unlistenFns) {
      try {
        unlisten();
      } catch (e) {
        console.warn("Failed to cleanup listener:", e);
      }
    }
  });

  return <ResultsWindow />;
};

export default ResultsApp;
