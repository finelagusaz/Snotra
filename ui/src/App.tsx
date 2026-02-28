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
import type { ResultsSyncPayload } from "./lib/searchEvents";
import { createResultsWindowController } from "./lib/resultsWindowController";

type ResultsRenderDonePayload = {
  requestId: number;
};

const App: Component = () => {
  const windowLabel = getCurrentWindow().label;
  const unlistenFns: Array<() => void> = [];

  onMount(async () => {
    const win = getCurrentWindow();
    const label = win.label;
    let registerAutoHideOnFocusLost: (() => Promise<void>) | undefined;

    if (label === "main") {
      const controller = createResultsWindowController(win);

      const hideMainAndResults = async () => {
        await win.hide();
        const rw = await controller.getResultsWindow();
        if (rw) {
          await rw.hide();
        }
      };

      registerAutoHideOnFocusLost = async () => {
        let blurTimer: ReturnType<typeof setTimeout> | undefined;
        let blurCancelled = false;
        const unlistenFocus = await win.onFocusChanged(({ payload: focused }) => {
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
        unlistenFns.push(unlistenFocus);
      };

      // Wait for all critical listeners to be attached before first reset/show.
      const [unlistenWindowShown, unlistenResultsSync, unlistenResultClicked, unlistenRenderDone, unlistenResultDoubleClicked, unlistenPlatformEvent] =
        await Promise.all([
          listen("window-shown", () => {
            trace("app:event:window_shown");
            resetForShow();
          }),
          listen<ResultsSyncPayload>("results-sync", (event) => {
            void controller.handleResultsSync(event.payload);
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
        unlistenResultsSync,
        unlistenResultClicked,
        unlistenRenderDone,
        unlistenResultDoubleClicked,
        unlistenPlatformEvent,
      );

      if (await win.isVisible()) {
        resetForShow();
      }
      unlistenFns.push(await initIndexingState());

      const unlistenMainResized = await win.onResized(({ payload: sz }) => {
        const logicalSize = sz.toLogical(controller.getCachedScaleFactor());
        controller.handleMainResized(logicalSize.height);
      });
      unlistenFns.push(unlistenMainResized);

      // Sync results window position when main moves
      let moveTimer: ReturnType<typeof setTimeout> | undefined;
      let latestMoveEvent = 0;
      const unlistenMainMoved = await win.onMoved(({ payload: pos }) => {
        const moveEvent = ++latestMoveEvent;
        const logicalPos = pos.toLogical(controller.getCachedScaleFactor());
        // Save position (debounced)
        clearTimeout(moveTimer);
        moveTimer = setTimeout(() => {
          void (async () => {
            if (moveEvent !== latestMoveEvent) return;
            await api.saveSearchPlacement(Math.round(logicalPos.x), Math.round(logicalPos.y));
          })();
        }, 500);

        // Immediately sync results window position
        void controller.handleMainMoved(logicalPos);
      });
      unlistenFns.push(unlistenMainMoved);

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
      await registerAutoHideOnFocusLost?.();
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
      const unlistenSettingsMoved = await win.onMoved(({ payload: pos }) => {
        clearTimeout(moveTimer);
        moveTimer = setTimeout(() => {
          void (async () => {
            const sf = await win.scaleFactor();
            const logicalPos = pos.toLogical(sf);
            await api.saveSettingsPlacement(Math.round(logicalPos.x), Math.round(logicalPos.y));
          })();
        }, 500);
      });
      unlistenFns.push(unlistenSettingsMoved);

      // Save size on resize (debounced)
      let resizeTimer: ReturnType<typeof setTimeout> | undefined;
      const unlistenSettingsResized = await win.onResized(({ payload: sz }) => {
        clearTimeout(resizeTimer);
        resizeTimer = setTimeout(() => {
          void (async () => {
            const sf = await win.scaleFactor();
            const logicalSize = sz.toLogical(sf);
            await api.saveSettingsSize(Math.round(logicalSize.width), Math.round(logicalSize.height));
          })();
        }, 500);
      });
      unlistenFns.push(unlistenSettingsResized);
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
