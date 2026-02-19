import { type Component, createSignal, onMount, Switch, Match } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import SearchWindow from "./components/SearchWindow";
import ResultsWindow from "./components/ResultsWindow";
import SettingsWindow from "./components/SettingsWindow";
import { resetForShow, setSelected, activateSelected, initIndexingState } from "./stores/search";
import { applyTheme } from "./lib/theme";
import * as api from "./lib/invoke";

const RESULTS_GAP = 4;
const RESULT_ROW_HEIGHT = 30;
const RESULTS_PADDING = 8;

const App: Component = () => {
  const [windowLabel, setWindowLabel] = createSignal(getCurrentWindow().label);

  onMount(async () => {
    const win = getCurrentWindow();
    const label = win.label;

    // Register listeners before any await to avoid race conditions
    if (label === "main") {
      listen("window-shown", () => {
        resetForShow();
      });
      initIndexingState();
    }

    // Load config and apply theme (non-fatal on failure)
    let config: Awaited<ReturnType<typeof api.getConfig>> | null = null;
    try {
      config = await api.getConfig();
      applyTheme(config.visual);
    } catch (e) {
      console.error("Failed to load config/apply theme:", e);
    }

    if (label === "main" && config) {
      // Restore search window position
      const placement = await api.getSearchPlacement();
      if (placement) {
        await win.setPosition({
          type: "Logical",
          x: placement.x,
          y: placement.y,
        });
      }

      // Apply window width from config
      const [currentSize, scaleFactor] = await Promise.all([
        win.innerSize(),
        win.scaleFactor(),
      ]);
      const logicalSize = currentSize.toLogical(scaleFactor);
      const windowWidth = config.appearance.window_width > 0
        ? config.appearance.window_width
        : logicalSize.width;

      if (config.appearance.window_width > 0) {
        await win.setSize({
          type: "Logical",
          width: windowWidth,
          height: logicalSize.height,
        });
      }

      // Auto-hide on focus lost (with grace period for drag operations)
      if (config.general.auto_hide_on_focus_lost) {
        let blurTimer: ReturnType<typeof setTimeout> | undefined;
        win.onFocusChanged(({ payload: focused }) => {
          if (!focused) {
            blurTimer = setTimeout(() => {
              win.hide();
              WebviewWindow.getByLabel("results").then((rw) => {
                if (rw) rw.hide();
              });
            }, 100);
          } else {
            clearTimeout(blurTimer);
          }
        });
      }

      // Sync results window position when main moves
      let moveTimer: ReturnType<typeof setTimeout> | undefined;
      win.onMoved(({ payload: pos }) => {
        // Save position (debounced)
        clearTimeout(moveTimer);
        moveTimer = setTimeout(() => {
          api.saveSearchPlacement(pos.x, pos.y);
        }, 500);

        // Immediately sync results window position
        WebviewWindow.getByLabel("results").then((rw) => {
          if (rw) {
            win.innerSize().then((size) => {
              win.scaleFactor().then((sf) => {
                const logicalH = size.toLogical(sf).height;
                rw.setPosition({
                  type: "Logical",
                  x: pos.x,
                  y: pos.y + logicalH + RESULTS_GAP,
                });
              });
            });
          }
        });
      });

      // Listen for results-count-changed to show/hide/resize results window
      listen<number>("results-count-changed", async (event) => {
        const count = event.payload;
        const rw = await WebviewWindow.getByLabel("results");
        if (!rw) return;

        if (count === 0) {
          rw.hide();
          return;
        }

        // Resize results window based on count
        const resultsHeight = Math.min(count * RESULT_ROW_HEIGHT + RESULTS_PADDING * 2, 400);
        await rw.setSize({
          type: "Logical",
          width: windowWidth,
          height: resultsHeight,
        });

        // Position results below main
        const mainPos = await win.outerPosition();
        const mainSize = await win.innerSize();
        const sf = await win.scaleFactor();
        const logicalH = mainSize.toLogical(sf).height;
        await rw.setPosition({
          type: "Logical",
          x: mainPos.x,
          y: mainPos.y + logicalH + RESULTS_GAP,
        });

        // Show if main is visible
        const mainVisible = await win.isVisible();
        if (mainVisible) {
          rw.show();
        }
      });

      // Listen for result-clicked from results window
      listen<number>("result-clicked", (event) => {
        setSelected(event.payload);
      });

      // Listen for result-double-clicked from results window
      listen<number>("result-double-clicked", (event) => {
        setSelected(event.payload);
        activateSelected();
      });
    }

    if (label === "settings") {
      // Restore settings window position and size
      try {
        const [placement, size] = await api.getSettingsPlacement();
        if (size) {
          await win.setSize({
            type: "Logical",
            width: size.width,
            height: size.height,
          });
        }
        if (placement) {
          await win.setPosition({
            type: "Logical",
            x: placement.x,
            y: placement.y,
          });
        }
      } catch (e) {
        console.error("Settings placement restore error:", e);
      }

      // Save position on move (debounced)
      let moveTimer: ReturnType<typeof setTimeout> | undefined;
      win.onMoved(({ payload: pos }) => {
        clearTimeout(moveTimer);
        moveTimer = setTimeout(() => {
          api.saveSettingsPlacement(pos.x, pos.y);
        }, 500);
      });

      // Save size on resize (debounced)
      let resizeTimer: ReturnType<typeof setTimeout> | undefined;
      win.onResized(({ payload: sz }) => {
        clearTimeout(resizeTimer);
        resizeTimer = setTimeout(() => {
          api.saveSettingsSize(sz.width, sz.height);
        }, 500);
      });
    }
  });

  return (
    <Switch fallback={<div style="padding: 16px">Unknown window: {windowLabel()}</div>}>
      <Match when={windowLabel() === "settings"}>
        <SettingsWindow />
      </Match>
      <Match when={windowLabel() === "results"}>
        <ResultsWindow />
      </Match>
      <Match when={windowLabel() === "main"}>
        <SearchWindow />
      </Match>
    </Switch>
  );
};

export default App;
