import { type Component, onMount, onCleanup, createSignal, createEffect } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { listen } from "@tauri-apps/api/event";
import SearchWindow from "./components/SearchWindow";
import ResultsSection from "./components/ResultsSection";
import {
  resetForShow,
  setSelected,
  activateSelectedByIndex,
  initIndexingState,
  setHotkeyFailureNotice,
  shouldShowResults,
} from "./stores/search";
import { applyTheme } from "./lib/theme";
import { t } from "./lib/i18n";
import type { BootstrapPayload, VisualConfig } from "./lib/types";
import * as api from "./lib/invoke";
import { trace } from "./lib/trace";

const SEARCH_BAR_HEIGHT = 52;
const RESULT_ROW_HEIGHT = 30;
const RESULTS_PADDING = 8; // .results-section bottom padding

const MainApp: Component = () => {
  const win = getCurrentWindow();
  const unlistenFns: Array<() => void> = [];
  const [mainVisible, setMainVisible] = createSignal(false);
  const [maxResults, setMaxResults] = createSignal(8);
  let cachedWidth = 600;

  onMount(async () => {
    const hideMain = async () => {
      setMainVisible(false);
      api.notifyMainHidden().catch(() => {});
      await win.hide();
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
              // 統合後は results ウィンドウが同一ウィンドウ内のため、
              // is_main_foreground によるプロセス ID 比較は不要。
              // blurCancelled debounce はドラッグ移動時の一時的フォーカス喪失対策として維持。
              await hideMain();
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
    const [unlistenWindowShown, unlistenWindowHidden, unlistenPlatformEvent] =
      await Promise.all([
        listen("window-shown", () => {
          trace("app:event:window_shown");
          setMainVisible(true);
          resetForShow();
        }),
        listen("window-hidden", () => {
          trace("app:event:window_hidden");
          setMainVisible(false);
        }),
        listen<{ event: string; hotkey: string }>("platform-event", async (ev) => {
          const p = ev.payload;
          if (p.event === "initial-hotkey-failed") {
            trace("app:event:platform_event:initial_hotkey_failed");
            try {
              setMainVisible(true);
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
      unlistenWindowHidden,
      unlistenPlatformEvent,
    );

    const initiallyVisible = await win.isVisible();
    setMainVisible(initiallyVisible);
    if (initiallyVisible) {
      resetForShow();
    }
    unlistenFns.push(await initIndexingState());

    // Cache scale factor for coordinate conversion
    let cachedScaleFactor = 1;
    try {
      cachedScaleFactor = await win.scaleFactor();
      const size = await win.innerSize();
      const logical = size.toLogical(cachedScaleFactor);
      cachedWidth = logical.width;
    } catch (e) {
      console.warn("Failed to initialize window geometry cache:", e);
    }

    const unlistenMainResized = await win.onResized(({ payload: sz }) => {
      const logicalSize = sz.toLogical(cachedScaleFactor);
      cachedWidth = logicalSize.width;
    });
    unlistenFns.push(unlistenMainResized);

    // Save window position (debounced)
    let moveTimer: ReturnType<typeof setTimeout> | undefined;
    let latestMoveEvent = 0;
    const unlistenMainMoved = await win.onMoved(({ payload: pos }) => {
      const moveEvent = ++latestMoveEvent;
      const logicalPos = pos.toLogical(cachedScaleFactor);
      clearTimeout(moveTimer);
      moveTimer = setTimeout(() => {
        void (async () => {
          if (moveEvent !== latestMoveEvent) return;
          await api.saveSearchPlacement(Math.round(logicalPos.x), Math.round(logicalPos.y));
        })();
      }, 500);
    });
    unlistenFns.push(unlistenMainMoved);

    // Listen for visual config changes
    const unlistenVisual = await listen<VisualConfig>("visual-config-changed", (event) => {
      applyTheme(event.payload);
    });
    unlistenFns.push(unlistenVisual);

    // Listen for max_results config changes
    const unlistenMaxResults = await listen<number>("max-results-changed", (event) => {
      setMaxResults(event.payload);
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
      setMaxResults(bootstrap.appearance.max_results);
    } catch (e) {
      console.error("Failed to load bootstrap payload:", e);
    }

    if (bootstrap?.general.auto_hide_on_focus_lost) {
      await registerAutoHideOnFocusLost();
    }
  });

  // ウィンドウ高さを結果の表示/非表示に応じて動的変更
  createEffect(() => {
    const show = shouldShowResults();
    const height = show ? SEARCH_BAR_HEIGHT + maxResults() * RESULT_ROW_HEIGHT + RESULTS_PADDING : SEARCH_BAR_HEIGHT;
    void win.setSize(new LogicalSize(cachedWidth, height));
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

  function handleClickResult(index: number) {
    trace("app:event:result_clicked", { index });
    void activateSelectedByIndex(index).then((launched) => {
      trace("app:event:result_clicked:done", { index, launched });
      if (launched) {
        setMainVisible(false);
        api.notifyMainHidden().catch(() => {});
        void win.hide();
      } else {
        console.warn("Failed to launch clicked result", { index });
      }
    });
  }

  function handleDoubleClickResult(index: number) {
    setSelected(index);
  }

  function handleHoverResult(index: number) {
    setSelected(index);
  }

  return (
    <>
      <SearchWindow />
      <ResultsSection
        visible={shouldShowResults() && mainVisible()}
        onClickResult={handleClickResult}
        onDoubleClickResult={handleDoubleClickResult}
        onHoverResult={handleHoverResult}
      />
    </>
  );
};

export default MainApp;
