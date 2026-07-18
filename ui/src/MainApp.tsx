import { type Component, onMount, onCleanup, createSignal, createEffect, Show, untrack } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { listen } from "@tauri-apps/api/event";
import type { Update } from "@tauri-apps/plugin-updater";
import SearchWindow from "./components/SearchWindow";
import ResultsSection from "./components/ResultsSection";
import UpdateToast from "./components/UpdateToast";
import {
  resetForShow,
  setSelected,
  activateSelectedByIndex,
  initIndexingState,
  setHotkeyFailureNotice,
  shouldShowResults,
  interpKind,
  setInstantCommandPrefix,
} from "./stores/search";
import { hideMainWindow } from "./lib/commands";
import { createOwnedTimer } from "./lib/ownedTimer";
import { applyTheme } from "./lib/theme";
import { t, setLanguage, type Lang } from "./lib/i18n";
import type { BootstrapPayload, UpdateAvailablePayload, VisualConfig } from "./lib/types";
import * as api from "./lib/invoke";
import { trace } from "./lib/trace";
import { computeWindowHeight } from "./lib/windowHeight";

function readLayoutConst(name: string, fallback: number): number {
  const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const px = parseFloat(raw);
  return isNaN(px) ? fallback : px;
}

const MainApp: Component = () => {
  const SEARCH_BAR_HEIGHT = readLayoutConst("--search-bar-height", 52);
  const RESULT_ROW_HEIGHT = readLayoutConst("--result-row-height", 30);
  const RESULTS_PADDING = readLayoutConst("--results-padding", 8);
  const UPDATE_TOAST_HEIGHT = readLayoutConst("--update-toast-height", 52);
  const win = getCurrentWindow();
  const unlistenFns: Array<() => void> = [];
  const blurTimer = createOwnedTimer(100);
  const moveTimer = createOwnedTimer(500);
  const [mainVisible, setMainVisible] = createSignal(false);
  // フォーカス喪失時自動非表示のゲート（#576）。bootstrap と "auto-hide-focus-lost-changed"
  // イベントで更新され、常時登録の onFocusChanged ハンドラがこれを参照する。既定 false は
  // bootstrap 到着前に非表示を走らせない（旧: リスナー未登録＝非表示にならない挙動と等価）。
  const [autoHideOnFocusLost, setAutoHideOnFocusLost] = createSignal(false);
  const [maxResults, setMaxResults] = createSignal(8);
  const [showIcons, setShowIcons] = createSignal(true);
  const [cachedWidth, setCachedWidth] = createSignal(600);
  const [iconCacheSize, setIconCacheSize] = createSignal(200);
  const [updateInfo, setUpdateInfo] = createSignal<UpdateAvailablePayload | null>(null);
  const [updaterInstalling, setUpdaterInstalling] = createSignal(false);
  const [updaterError, setUpdaterError] = createSignal<string | null>(null);
  let pendingUpdate: Update | null = null;

  onMount(async () => {
    const hideMain = async () => {
      if (!mainVisible()) return;
      // eager に results を畳み Blob URL を hide 前に解放する（signal 駆動）。
      // hide→notify(trim) の順序は hideMainWindow に集約（#361）。
      setMainVisible(false);
      await hideMainWindow();
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
              // 不変条件: suspend 後に到達しうる show 経路は必ず Rust 側の
              // show_main_and_emit（resume_webview を含む）を通ること。この win.show() は
              // 起動直後（初回 suspend 発生前）にしか発火しないため resume 不要の例外。
              // 別のタイミングから再利用する場合は Rust 側の show 経路へ寄せ直すこと。
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
      setCachedWidth(logical.width);
    } catch (e) {
      console.warn("Failed to initialize window geometry cache:", e);
    }

    const unlistenMainResized = await win.onResized(({ payload: sz }) => {
      const logicalSize = sz.toLogical(cachedScaleFactor);
      setCachedWidth(logicalSize.width);
    });
    unlistenFns.push(unlistenMainResized);

    // Save window position (debounced). Rust side reads position directly
    // from HWND and converts to monitor-relative coordinates.
    const unlistenMainMoved = await win.onMoved(() => {
      moveTimer.arm(() => {
        void (async () => {
          await api.saveSearchPlacement();
        })();
      });
    });
    unlistenFns.push(unlistenMainMoved);

    // フォーカス喪失時の自動非表示リスナーを常時登録し、autoHideOnFocusLost シグナルを
    // ゲートとして参照する（#576）。登録失敗は局所 catch で隔離し、後続の初期化
    // （他イベント購読・bootstrap 適用）を巻き添えにしない。
    try {
      const unlistenFocus = await win.onFocusChanged(({ payload: focused }) => {
        if (!focused) {
          if (!autoHideOnFocusLost()) return; // 設定オフなら何もしない
          blurTimer.arm(() => {
            void (async () => {
              try {
                // 統合後は results ウィンドウが同一ウィンドウ内のため、
                // is_main_foreground によるプロセス ID 比較は不要。
                await hideMain();
              } catch (e) {
                console.warn("auto-hide focus check failed:", e);
              }
            })();
          });
        } else {
          blurTimer.cancel();
        }
      });
      unlistenFns.push(unlistenFocus);
    } catch (e) {
      console.warn("auto-hide focus listener registration failed:", e);
    }

    // Listen for auto_hide_on_focus_lost config changes (#576)
    const unlistenAutoHideConfig = await listen<boolean>("auto-hide-focus-lost-changed", (event) => {
      setAutoHideOnFocusLost(event.payload);
    });
    unlistenFns.push(unlistenAutoHideConfig);

    // Listen for visual config changes
    const unlistenVisual = await listen<VisualConfig>("visual-config-changed", (event) => {
      applyTheme(event.payload);
    });
    unlistenFns.push(unlistenVisual);

    // Listen for visible_rows config changes (event name kept as "max-results-changed" for IPC stability, #388)
    const unlistenMaxResults = await listen<number>("max-results-changed", (event) => {
      setMaxResults(event.payload);
    });
    unlistenFns.push(unlistenMaxResults);

    // Listen for show_icons config changes
    const unlistenShowIcons = await listen<boolean>("show-icons-changed", (event) => {
      setShowIcons(event.payload);
    });
    unlistenFns.push(unlistenShowIcons);

    // Listen for hotkey registration failure (config change case)
    const unlistenHotkeyFailed = await listen<string>("hotkey-registration-failed", (event) => {
      setHotkeyFailureNotice(t("notice.hotkey.change_failed", { hotkey: event.payload }));
    });
    unlistenFns.push(unlistenHotkeyFailed);

    // Listen for language changes
    const unlistenLang = await listen<string>("language-changed", (event) => {
      setLanguage(event.payload as Lang);
    });
    unlistenFns.push(unlistenLang);

    // Listen for instant command prefix changes
    const unlistenInstantPrefix = await listen<string>("instant-prefix-changed", (event) => {
      setInstantCommandPrefix(event.payload);
    });
    unlistenFns.push(unlistenInstantPrefix);

    // Listen for result_limit changes (event name kept as "top-n-history-changed" for IPC stability, #388)
    const unlistenTopN = await listen<number>("top-n-history-changed", (event) => {
      setIconCacheSize(event.payload);
    });
    unlistenFns.push(unlistenTopN);

    // Load bootstrap payload and apply theme (non-fatal on failure)
    let bootstrap: BootstrapPayload | null = null;
    try {
      bootstrap = await api.getBootstrapPayload();
      setLanguage(bootstrap.language as Lang);
      applyTheme(bootstrap.visual);
      setMaxResults(bootstrap.appearance.visible_rows);
      setShowIcons(bootstrap.appearance.show_icons);
      setInstantCommandPrefix(bootstrap.instant_command_prefix);
      setIconCacheSize(bootstrap.result_limit);
      // フォーカス喪失時自動非表示の初期値（#576）。以後の変更は
      // "auto-hide-focus-lost-changed" イベントが追従する。bootstrap 失敗時は
      // 既定 false のままとし、非表示を走らせない。
      setAutoHideOnFocusLost(bootstrap.general.auto_hide_on_focus_lost);
    } catch (e) {
      console.error("Failed to load bootstrap payload:", e);
    }

    // 起動時に更新チェック（auto_update が disabled 以外の場合）
    // plugin-updater は動的 import で遅延読込し初期バンドルから除外する
    if (bootstrap && bootstrap.general.auto_update !== "disabled") {
      const canInstall = bootstrap.general.auto_update === "full";
      import("@tauri-apps/plugin-updater").then(({ check }) =>
        check().then((update) => {
          if (update) {
            if (canInstall) {
              pendingUpdate = update;
            }
            setUpdateInfo({ version: update.version, can_install: canInstall });
          }
        })
      ).catch((e) => {
        console.warn("Update check failed:", e);
      });
    }
  });

  // ウィンドウサイズを結果の表示/非表示に応じて動的変更。
  // cachedWidth は高さ計算に影響しないため untrack で依存から外す
  // （ドラッグリサイズ中の冗長な setSize IPC を防止）。
  // 高さが変化した場合のみ setSize を発行する。
  let prevSetHeight = 0;
  createEffect(() => {
    const height = computeWindowHeight({
      shouldShowResults: shouldShowResults(),
      maxResults: maxResults(),
      hasUpdateToast: updateInfo() !== null,
      searchBarHeight: SEARCH_BAR_HEIGHT,
      resultRowHeight: RESULT_ROW_HEIGHT,
      resultsPadding: RESULTS_PADDING,
      updateToastHeight: UPDATE_TOAST_HEIGHT,
    });
    if (height === prevSetHeight) return;
    prevSetHeight = height;
    const width = untrack(cachedWidth);
    void win.setSize(new LogicalSize(width, height));
  });

  // 設定がオフへ切り替わった瞬間に保留中の非表示タイマーを止める（#576）。
  // 「フォーカス喪失 → blurTimer.arm 直後に設定オフ」という 100ms 未満の窓でも
  // pending タイマーを即キャンセルし、意図しない非表示を防ぐ。
  createEffect(() => {
    if (!autoHideOnFocusLost()) blurTimer.cancel();
  });

  onCleanup(() => {
    for (const unlisten of unlistenFns) {
      try {
        unlisten();
      } catch (e) {
        console.warn("Failed to cleanup listener:", e);
      }
    }
    blurTimer.cancel();
    moveTimer.cancel();
  });

  async function handleUpdateInstall() {
    if (!pendingUpdate) return;
    setUpdaterInstalling(true);
    setUpdaterError(null);
    try {
      await pendingUpdate.downloadAndInstall();
      // NSIS インストーラにプロセス終了・再起動を委ねる。
      // app.restart() は新プロセスを spawn してから exit するため、
      // 新プロセスがファイルをロックし NSIS の上書きが失敗する。
      await api.quitApp();
      // アプリが終了するためここには到達しない
    } catch (e) {
      console.error("Update install failed:", e);
      setUpdaterInstalling(false);
      setUpdaterError(e instanceof Error ? e.message : String(e));
    }
  }

  function handleClickResult(index: number) {
    trace("app:event:result_clicked", { index });
    void activateSelectedByIndex(index).then((launched) => {
      trace("app:event:result_clicked:done", { index, launched });
      if (launched) {
        setMainVisible(false);
        void hideMainWindow();
      } else {
        console.warn("Failed to launch clicked result", { index });
      }
    });
  }

  function handleDoubleClickResult(index: number) {
    setSelected(index);
  }

  return (
    <>
      <SearchWindow />
      <Show when={updateInfo() !== null}>
        <UpdateToast
          version={updateInfo()!.version}
          canInstall={updateInfo()!.can_install}
          installing={updaterInstalling()}
          error={updaterError() !== null}
          errorDetail={updaterError()}
          onInstall={handleUpdateInstall}
          onDismiss={() => setUpdateInfo(null)}
        />
      </Show>
      <ResultsSection
        visible={shouldShowResults() && mainVisible()}
        showIcons={showIcons()}
        skipIcons={interpKind() === "instant"}
        maxResults={maxResults()}
        iconCacheSize={iconCacheSize()}
        onClickResult={handleClickResult}
        onDoubleClickResult={handleDoubleClickResult}
      />
    </>
  );
};

export default MainApp;
