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
import { perfMarkRenderDone } from "./lib/perf";

const RESULTS_GAP = 4;
const RESULT_ROW_HEIGHT = 30;
const RESULTS_PADDING = 8;
type ResultsCountChangedPayload = {
  count: number;
  requestId: number;
};
type ResultsRenderDonePayload = {
  requestId: number;
};

const App: Component = () => {
  const windowLabel = getCurrentWindow().label;

  onMount(async () => {
    const win = getCurrentWindow();
    const label = win.label;
    let resultsWindowPromise: Promise<WebviewWindow | null> | undefined;
    let lastResultsSize: { width: number; height: number } | undefined;
    let lastResultsPosition: { x: number; y: number } | undefined;
    let latestResultsRequestId = 0;

    const getResultsWindow = async () => {
      if (!resultsWindowPromise) {
        resultsWindowPromise = WebviewWindow.getByLabel("results");
      }
      return resultsWindowPromise;
    };

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
              getResultsWindow().then((rw) => {
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
          const rw = await getResultsWindow();
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
      listen<ResultsCountChangedPayload>("results-count-changed", async (event) => {
        const { count, requestId } = event.payload;
        if (requestId < latestResultsRequestId) return;
        latestResultsRequestId = requestId;

        const rw = await getResultsWindow();
        if (!rw) return;

        if (count === 0) {
          if (await rw.isVisible()) {
            await rw.hide();
          }
          return;
        }

        // Use current main window width (may have been updated via settings)
        const [currentSize, currentSf, mainPos, mainVisible] = await Promise.all([
          win.innerSize(),
          win.scaleFactor(),
          win.outerPosition(),
          win.isVisible(),
        ]);
        if (requestId !== latestResultsRequestId) return;

        const mainSize = currentSize;
        const sf = currentSf;
        const currentWidth = currentSize.toLogical(currentSf).width;

        // Resize results window based on count
        const resultsHeight = Math.min(count * RESULT_ROW_HEIGHT + RESULTS_PADDING * 2, 400);
        if (
          !lastResultsSize ||
          lastResultsSize.width !== currentWidth ||
          lastResultsSize.height !== resultsHeight
        ) {
          await rw.setSize(new LogicalSize(currentWidth, resultsHeight));
          if (requestId !== latestResultsRequestId) return;
          lastResultsSize = { width: currentWidth, height: resultsHeight };
        }

        // Position results below main
        const logicalMainPos = mainPos.toLogical(sf);
        const logicalH = mainSize.toLogical(sf).height;
        const nextPosition = {
          x: logicalMainPos.x,
          y: logicalMainPos.y + logicalH + RESULTS_GAP,
        };
        if (
          !lastResultsPosition ||
          lastResultsPosition.x !== nextPosition.x ||
          lastResultsPosition.y !== nextPosition.y
        ) {
          await rw.setPosition(new LogicalPosition(nextPosition.x, nextPosition.y));
          if (requestId !== latestResultsRequestId) return;
          lastResultsPosition = nextPosition;
        }

        // Show if main is visible
        if (mainVisible) {
          if (!(await rw.isVisible())) {
            await rw.show();
          }
        }
      });

      // Listen for result-clicked from results window
      listen<number>("result-clicked", (event) => {
        setSelected(event.payload);
      });

      listen<ResultsRenderDonePayload>("results-render-done", (event) => {
        perfMarkRenderDone(event.payload.requestId);
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
