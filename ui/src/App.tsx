import { type Component, onMount, onCleanup, Switch, Match } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import SearchWindow from "./components/SearchWindow";
import ResultsWindow from "./components/ResultsWindow";
import SettingsWindow from "./components/SettingsWindow";
import AboutWindow from "./components/AboutWindow";
import {
  resetForShow,
  setSelected,
  activateSelectedByPath,
  initIndexingState,
} from "./stores/search";
import { applyTheme } from "./lib/theme";
import type { BootstrapPayload, VisualConfig } from "./lib/types";
import * as api from "./lib/invoke";
import { perfMarkRenderDone } from "./lib/perf";
import { trace } from "./lib/trace";

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
  const unlistenFns: Array<() => void> = [];

  onMount(async () => {
    const win = getCurrentWindow();
    const label = win.label;
    let resultsWindowPromise: Promise<WebviewWindow | null> | undefined;
    let lastResultsSize: { width: number; height: number } | undefined;
    let lastResultsPosition: { x: number; y: number } | undefined;
    let latestResultsRequestId = 0;
    let registerAutoHideOnFocusLost: (() => void) | undefined;

    const getResultsWindow = async () => {
      if (!resultsWindowPromise) {
        trace("app:results_window:ensure:start");
        try {
          const created = await api.ensureWindow("results");
          trace("app:results_window:ensure:ok", { created });
          resultsWindowPromise = WebviewWindow.getByLabel("results");
          trace("app:results_window:get_by_label", {
            exists: resultsWindowPromise !== null,
          });
        } catch (e) {
          trace("app:results_window:ensure:error", { error: String(e) });
          throw e;
        }
      }
      return resultsWindowPromise;
    };

    if (label === "main") {
      let cachedScaleFactor = 1;
      let cachedMainLogicalHeight = 52;
      let positionApplyInFlight = false;
      let pendingResultsPosition: { x: number; y: number } | undefined;

      const hideMainAndResults = async () => {
        await win.hide();
        const rw = await getResultsWindow();
        if (rw) {
          await rw.hide();
        }
      };

      const queueResultsPosition = (rw: WebviewWindow, nextPosition: { x: number; y: number }) => {
        pendingResultsPosition = nextPosition;
        if (positionApplyInFlight) {
          return;
        }
        positionApplyInFlight = true;
        void (async () => {
          while (pendingResultsPosition) {
            const target = pendingResultsPosition;
            pendingResultsPosition = undefined;
            await rw.setPosition(new LogicalPosition(target.x, target.y));
            lastResultsPosition = target;
          }
          positionApplyInFlight = false;
        })().catch((e) => {
          console.error("Failed to sync results window position:", e);
          positionApplyInFlight = false;
        });
      };

      registerAutoHideOnFocusLost = () => {
        let blurTimer: ReturnType<typeof setTimeout> | undefined;
        let blurCancelled = false;
        win.onFocusChanged(({ payload: focused }) => {
          if (!focused) {
            blurCancelled = false;
            blurTimer = setTimeout(async () => {
              try {
                if (blurCancelled) return;
                const sw = await WebviewWindow.getByLabel("settings");
                const aw = await WebviewWindow.getByLabel("about");
                if (blurCancelled) return;
                const settingsVisible = sw && await sw.isVisible();
                const aboutVisible = aw && await aw.isVisible();
                if (blurCancelled) return;
                if (!settingsVisible && !aboutVisible) {
                  await hideMainAndResults();
                }
              } catch (e) {
                console.warn("auto-hide focus check failed:", e);
              }
            }, 100);
          } else {
            blurCancelled = true;
            clearTimeout(blurTimer);
          }
        });
      };

      // Wait for all critical listeners to be attached before first reset/show.
      const [unlistenWindowShown, unlistenResultsCountChanged, unlistenResultClicked, unlistenRenderDone, unlistenResultDoubleClicked, unlistenPlatformEvent] =
        await Promise.all([
          listen("window-shown", () => {
            trace("app:event:window_shown");
            resetForShow();
          }),
          listen<ResultsCountChangedPayload>("results-count-changed", async (event) => {
            const { count, requestId } = event.payload;
            trace("app:event:results_count_changed", {
              count,
              requestId,
              latestResultsRequestId,
            });
            if (requestId < latestResultsRequestId) return;
            latestResultsRequestId = requestId;
            const isStale = () => requestId !== latestResultsRequestId;

            if (count === 0) {
              const rw = await WebviewWindow.getByLabel("results");
              if (!rw || isStale()) return;
              const visible = await rw.isVisible();
              if (isStale()) return;
              if (visible) {
                trace("app:results_window:hide", { reason: "count_zero", requestId });
                await rw.hide();
              }
              return;
            }
            const rw = await getResultsWindow();
            if (!rw || isStale()) return;

            // Use current main window width (may have been updated via settings)
            const [currentSize, currentSf, mainPos, mainVisible] = await Promise.all([
              win.innerSize(),
              win.scaleFactor(),
              win.outerPosition(),
              win.isVisible(),
            ]);
            if (isStale()) return;

            const mainSize = currentSize;
            const sf = currentSf;
            const currentWidth = currentSize.toLogical(currentSf).width;
            cachedScaleFactor = currentSf;
            cachedMainLogicalHeight = currentSize.toLogical(currentSf).height;

            // Resize results window based on count
            const resultsHeight = Math.min(count * RESULT_ROW_HEIGHT + RESULTS_PADDING * 2, 400);
            if (
              !lastResultsSize ||
              lastResultsSize.width !== currentWidth ||
              lastResultsSize.height !== resultsHeight
            ) {
              await rw.setSize(new LogicalSize(currentWidth, resultsHeight));
              if (isStale()) return;
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
              if (isStale()) return;
              lastResultsPosition = nextPosition;
            }

            if (!mainVisible) {
              const visible = await rw.isVisible();
              if (isStale()) return;
              if (visible) {
                await rw.hide();
              }
              return;
            }

            const visible = await rw.isVisible();
            if (isStale()) return;
            if (!visible) {
              trace("app:results_window:show", { requestId, count });
              await rw.show();
              if (isStale()) {
                await rw.hide();
                return;
              }
              void api.setWindowNoActivate();
            }
          }),
          listen<string>("result-clicked", async (event) => {
            trace("app:event:result_clicked", { path: event.payload });
            const launched = await activateSelectedByPath(event.payload);
            trace("app:event:result_clicked:done", { path: event.payload, launched });
            if (launched) {
              void hideMainAndResults();
            } else {
              console.warn("Failed to launch clicked result", { path: event.payload });
            }
          }),
          listen<ResultsRenderDonePayload>("results-render-done", (event) => {
            perfMarkRenderDone(event.payload.requestId);
          }),
          listen<number>("result-double-clicked", (event) => {
            setSelected(event.payload);
          }),
          listen<string>("platform-event", async (event) => {
            if (event.payload === "initial-hotkey-failed") {
              trace("app:event:platform_event:initial_hotkey_failed");
              try {
                await win.show();
              } catch (e) {
                console.warn("platform-event: failed to show window on initial-hotkey-failed:", e);
              }
              resetForShow();
            }
          }),
        ]);

      unlistenFns.push(
        unlistenWindowShown,
        unlistenResultsCountChanged,
        unlistenResultClicked,
        unlistenRenderDone,
        unlistenResultDoubleClicked,
        unlistenPlatformEvent,
      );

      if (await win.isVisible()) {
        resetForShow();
      }
      void initIndexingState();

      void (async () => {
        try {
          const [sf, size] = await Promise.all([win.scaleFactor(), win.innerSize()]);
          cachedScaleFactor = sf;
          cachedMainLogicalHeight = size.toLogical(sf).height;
        } catch (e) {
          console.warn("Failed to initialize main window geometry cache:", e);
        }
      })();

      win.onResized(({ payload: sz }) => {
        const logicalSize = sz.toLogical(cachedScaleFactor);
        cachedMainLogicalHeight = logicalSize.height;
      });

      // Sync results window position when main moves
      let moveTimer: ReturnType<typeof setTimeout> | undefined;
      let latestMoveEvent = 0;
      win.onMoved(({ payload: pos }) => {
        const moveEvent = ++latestMoveEvent;
        const logicalPos = pos.toLogical(cachedScaleFactor);
        // Save position (debounced)
        clearTimeout(moveTimer);
        moveTimer = setTimeout(() => {
          void (async () => {
            if (moveEvent !== latestMoveEvent) return;
            await api.saveSearchPlacement(Math.round(logicalPos.x), Math.round(logicalPos.y));
          })();
        }, 500);

        // Immediately sync results window position
        void (async () => {
          const rw = await getResultsWindow();
          if (!rw || moveEvent !== latestMoveEvent) {
            return;
          }
          const nextPosition = {
            x: logicalPos.x,
            y: logicalPos.y + cachedMainLogicalHeight + RESULTS_GAP,
          };
          if (
            lastResultsPosition &&
            lastResultsPosition.x === nextPosition.x &&
            lastResultsPosition.y === nextPosition.y
          ) {
            return;
          }
          queueResultsPosition(rw, nextPosition);
        })();
      });

    }

    // Listen for visual config changes (all windows)
    const unlistenVisual = await listen<VisualConfig>("visual-config-changed", (event) => {
      applyTheme(event.payload);
    });
    unlistenFns.push(unlistenVisual);

    // Load bootstrap payload and apply theme (non-fatal on failure)
    let bootstrap: BootstrapPayload | null = null;
    try {
      bootstrap = await api.getBootstrapPayload();
      applyTheme(bootstrap.visual);
    } catch (e) {
      console.error("Failed to load bootstrap payload:", e);
    }

    if (label === "main" && bootstrap?.general.auto_hide_on_focus_lost) {
      registerAutoHideOnFocusLost?.();
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

  onCleanup(() => {
    for (const unlisten of unlistenFns) {
      try {
        unlisten();
      } catch (e) {
        console.warn("Failed to cleanup listener:", e);
      }
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
      <Match when={windowLabel === "about"}>
        <AboutWindow />
      </Match>
    </Switch>
  );
};

export default App;
