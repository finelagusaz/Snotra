import { type Component, onMount, Switch, Match } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import SearchWindow from "./components/SearchWindow";
import ResultsWindow from "./components/ResultsWindow";
import SettingsWindow from "./components/SettingsWindow";
import { resetForShow, setSelected, activateSelected, initIndexingState } from "./stores/search";
import { applyTheme } from "./lib/theme";
import type { VisualConfig } from "./lib/types";
import * as api from "./lib/invoke";

const RESULTS_GAP = 4;
const RESULT_ROW_HEIGHT = 30;
const RESULTS_PADDING = 8;

const App: Component = () => {
  const windowLabel = getCurrentWindow().label;

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

    // Listen for visual config changes (all windows)
    listen<VisualConfig>("visual-config-changed", (event) => {
      applyTheme(event.payload);
    });

    // Load config and apply theme (non-fatal on failure)
    let config: Awaited<ReturnType<typeof api.getConfig>> | null = null;
    try {
      config = await api.getConfig();
      applyTheme(config.visual);
    } catch (e) {
      console.error("Failed to load config/apply theme:", e);
    }

    if (label === "main" && config) {
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
      let latestMoveEvent = 0;
      win.onMoved(({ payload: pos }) => {
        const moveEvent = ++latestMoveEvent;
        // Save position (debounced)
        clearTimeout(moveTimer);
        moveTimer = setTimeout(() => {
          void (async () => {
            const sf = await win.scaleFactor();
            const logicalPos = pos.toLogical(sf);
            if (moveEvent !== latestMoveEvent) return;
            await api.saveSearchPlacement(Math.round(logicalPos.x), Math.round(logicalPos.y));
          })();
        }, 500);

        // Immediately sync results window position
        void (async () => {
          const sf = await win.scaleFactor();
          const logicalPos = pos.toLogical(sf);
          const rw = await WebviewWindow.getByLabel("results");
          if (!rw || moveEvent !== latestMoveEvent) return;

          const size = await win.innerSize();
          if (moveEvent !== latestMoveEvent) return;
          const logicalH = size.toLogical(sf).height;
          await rw.setPosition(
            new LogicalPosition(logicalPos.x, logicalPos.y + logicalH + RESULTS_GAP),
          );
        })();
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

        // Use current main window width (may have been updated via settings)
        const currentSize = await win.innerSize();
        const currentSf = await win.scaleFactor();
        const currentWidth = currentSize.toLogical(currentSf).width;

        // Resize results window based on count
        const resultsHeight = Math.min(count * RESULT_ROW_HEIGHT + RESULTS_PADDING * 2, 400);
        await rw.setSize(new LogicalSize(currentWidth, resultsHeight));

        // Position results below main
        const mainPos = await win.outerPosition();
        const mainSize = await win.innerSize();
        const sf = await win.scaleFactor();
        const logicalMainPos = mainPos.toLogical(sf);
        const logicalH = mainSize.toLogical(sf).height;
        await rw.setPosition(
          new LogicalPosition(logicalMainPos.x, logicalMainPos.y + logicalH + RESULTS_GAP),
        );

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
          await win.setSize(new LogicalSize(size.width, size.height));
        }
        if (placement) {
          await win.setPosition(new LogicalPosition(placement.x, placement.y));
        }
      } catch (e) {
        console.error("Settings placement restore error:", e);
      }

      // Save position on move (debounced)
      let moveTimer: ReturnType<typeof setTimeout> | undefined;
      win.onMoved(({ payload: pos }) => {
        clearTimeout(moveTimer);
        moveTimer = setTimeout(() => {
          void (async () => {
            const sf = await win.scaleFactor();
            const logicalPos = pos.toLogical(sf);
            await api.saveSettingsPlacement(Math.round(logicalPos.x), Math.round(logicalPos.y));
          })();
        }, 500);
      });

      // Save size on resize (debounced)
      let resizeTimer: ReturnType<typeof setTimeout> | undefined;
      win.onResized(({ payload: sz }) => {
        clearTimeout(resizeTimer);
        resizeTimer = setTimeout(() => {
          void (async () => {
            const sf = await win.scaleFactor();
            const logicalSize = sz.toLogical(sf);
            await api.saveSettingsSize(Math.round(logicalSize.width), Math.round(logicalSize.height));
          })();
        }, 500);
      });
    }
  });

  return (
    <Switch fallback={<div style="padding: 16px">Unknown window: {windowLabel}</div>}>
      <Match when={windowLabel === "settings"}>
        <SettingsWindow />
      </Match>
      <Match when={windowLabel === "results"}>
        <ResultsWindow />
      </Match>
      <Match when={windowLabel === "main"}>
        <SearchWindow />
      </Match>
    </Switch>
  );
};

export default App;
