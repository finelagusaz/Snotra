import { type Component, onMount, onCleanup } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import SearchWindow from "./components/SearchWindow";
import {
  resetForShow,
  setSelected,
  activateSelectedByIndex,
  initIndexingState,
  setHotkeyFailureNotice,
} from "./stores/search";
import { applyTheme } from "./lib/theme";
import { t } from "./lib/i18n";
import type { BootstrapPayload, VisualConfig } from "./lib/types";
import * as api from "./lib/invoke";
import { perfMarkRenderDone } from "./lib/perf";
import { trace } from "./lib/trace";
import type { ResultsDataPayload, ResultsVisibilityPayload, ResultsRenderDonePayload } from "./lib/searchEvents";
import { createResultsWindowController } from "./lib/resultsWindowController";

const MainApp: Component = () => {
  const win = getCurrentWindow();
  const unlistenFns: Array<() => void> = [];

  onMount(async () => {
    const controller = createResultsWindowController(win);

    const hideMainAndResults = async () => {
      controller.updateMainVisible(false);
      controller.updateResultsVisible(false);
      api.notifyMainHidden().catch(() => {});
      await win.hide();
      const rw = await controller.getResultsWindow();
      if (rw) {
        await rw.hide();
      }
    };

    const registerAutoHideOnFocusLost = async () => {
      let blurTimer: ReturnType<typeof setTimeout> | undefined;
      let blurCancelled = false;
      const unlistenFocus = await win.onFocusChanged(({ payload: focused }) => {
        if (!focused) {
          blurCancelled = false;
          blurTimer = setTimeout(async () => {
            try {
              if (blurCancelled) return;
              // WS_EX_NOACTIVATE を設定しても WebView2 が SetForegroundWindow() を呼ぶため
              // results クリック時も foreground が変わる。プロセス ID で自アプリ内操作を判定する。
              const mainForeground = await api.isMainForeground();
              if (blurCancelled) return;
              if (!mainForeground) {
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
    const [unlistenWindowShown, unlistenDataChanged, unlistenVisibilityChanged, unlistenResultClicked, unlistenRenderDone, unlistenResultDoubleClicked, unlistenResultHovered, unlistenPlatformEvent] =
      await Promise.all([
        listen("window-shown", () => {
          trace("app:event:window_shown");
          controller.updateMainVisible(true);
          resetForShow();
        }),
        listen<ResultsDataPayload>("results-data-changed", (event) => {
          void controller.handleDataChanged(event.payload);
        }),
        listen<ResultsVisibilityPayload>("results-visibility-changed", (event) => {
          void controller.handleVisibilityChanged(event.payload);
        }),
        listen<number>("result-clicked", async (event) => {
          trace("app:event:result_clicked", { index: event.payload });
          const launched = await activateSelectedByIndex(event.payload);
          trace("app:event:result_clicked:done", { index: event.payload, launched });
          if (launched) {
            void hideMainAndResults();
          } else {
            console.warn("Failed to launch clicked result", { index: event.payload });
          }
        }),
        listen<ResultsRenderDonePayload>("results-render-done", (event) => {
          perfMarkRenderDone(event.payload.requestId);
        }),
        listen<number>("result-double-clicked", (event) => {
          setSelected(event.payload);
        }),
        listen<number>("result-hovered", (event) => {
          setSelected(event.payload);
        }),
        listen<{ event: string; hotkey: string }>("platform-event", async (ev) => {
          const p = ev.payload;
          if (p.event === "initial-hotkey-failed") {
            trace("app:event:platform_event:initial_hotkey_failed");
            try {
              controller.updateMainVisible(true);
              await win.show();
              // Sync Rust-side visibility flag so hotkey toggle works correctly.
              api.notifyMainShown().catch(() => {});
            } catch (e) {
              console.warn("platform-event: failed to show window on initial-hotkey-failed:", e);
            }
            resetForShow();
            // Set notice after resetForShow() to avoid clearLaunchNotice() race.
            setHotkeyFailureNotice(t("notice.hotkey.initial_failed", { hotkey: p.hotkey }));
          }
        }),
      ]);

    unlistenFns.push(
      unlistenWindowShown,
      unlistenDataChanged,
      unlistenVisibilityChanged,
      unlistenResultClicked,
      unlistenRenderDone,
      unlistenResultDoubleClicked,
      unlistenResultHovered,
      unlistenPlatformEvent,
    );

    const initiallyVisible = await win.isVisible();
    controller.updateMainVisible(initiallyVisible);
    if (initiallyVisible) {
      resetForShow();
    }
    unlistenFns.push(await initIndexingState());

    const unlistenMainResized = await win.onResized(({ payload: sz }) => {
      const logicalSize = sz.toLogical(controller.getCachedScaleFactor());
      controller.updateMainSize(logicalSize.width, logicalSize.height);
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

      // Update cached position and sync results window position
      controller.updateMainPosition(logicalPos);
      void controller.handleMainMoved(logicalPos);
    });
    unlistenFns.push(unlistenMainMoved);

    // Listen for visual config changes
    const unlistenVisual = await listen<VisualConfig>("visual-config-changed", (event) => {
      applyTheme(event.payload);
    });
    unlistenFns.push(unlistenVisual);

    // Listen for max_results config changes
    const unlistenMaxResults = await listen<number>("max-results-changed", (event) => {
      controller.updateMaxResults(event.payload);
    });
    unlistenFns.push(unlistenMaxResults);

    // Listen for hotkey registration failure (config change case)
    const unlistenHotkeyFailed = await listen<string>("hotkey-registration-failed", (event) => {
      setHotkeyFailureNotice(t("notice.hotkey.change_failed", { hotkey: event.payload }));
    });
    unlistenFns.push(unlistenHotkeyFailed);

    // Load bootstrap payload and apply theme (non-fatal on failure)
    let bootstrap: BootstrapPayload | null = null;
    try {
      bootstrap = await api.getBootstrapPayload();
      applyTheme(bootstrap.visual);
      controller.updateMaxResults(bootstrap.appearance.max_results);
    } catch (e) {
      console.error("Failed to load bootstrap payload:", e);
    }

    if (bootstrap?.general.auto_hide_on_focus_lost) {
      await registerAutoHideOnFocusLost();
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

  return <SearchWindow />;
};

export default MainApp;
