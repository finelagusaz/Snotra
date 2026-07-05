import { type Component, onCleanup, onMount, Show, Switch, Match } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import {
  query,
  setQuery,
  results,
  selected,
  folderState,
  folderFilter,
  setFolderFilter,
  moveSelectionUp,
  moveSelectionDown,
  exitFolderExpansion,
  navigateFolderUp,
  enterFolderExpansion,
  activateSelected,
  indexing,
  launching,
  launchNotice,
  clearLaunchNotice,
  enterToolSelection,
  exitToolSelection,
  toolSelectionState,
  noResults,
  viewKind,
  allowsFolderNav,
} from "../stores/search";
import { hideMainWindow } from "../lib/commands";
import { computeParentDir } from "../lib/folderNav";
import { perfMarkInput } from "../lib/perf";
import { trace } from "../lib/trace";
import { t } from "../lib/i18n";

const SearchWindow: Component = () => {
  let inputRef: HTMLInputElement | undefined;
  const focusRetryTimers: ReturnType<typeof setTimeout>[] = [];
  let focusRafHandle: number | undefined;

  function focusInputSoon() {
    // Two-frame defer absorbs first-show native show/focus race on cold start.
    // Retries at 120ms/280ms cover longer delays (WebView2 init etc.).
    const t0 = performance.now();
    focusRafHandle = requestAnimationFrame(() => {
      const t1 = performance.now();
      focusRafHandle = requestAnimationFrame(() => {
        focusRafHandle = undefined;
        const t2 = performance.now();
        inputRef?.focus();
        const t3 = performance.now();
        trace("ui:focus_input_done", {
          raf1_ms: Math.round((t1 - t0) * 100) / 100,
          raf2_ms: Math.round((t2 - t1) * 100) / 100,
          focus_ms: Math.round((t3 - t2) * 100) / 100,
          total_ms: Math.round((t3 - t0) * 100) / 100,
        });
      });
    });
  }

  function clearFocusRetryTimers() {
    if (focusRafHandle !== undefined) {
      cancelAnimationFrame(focusRafHandle);
      focusRafHandle = undefined;
    }
    for (const timer of focusRetryTimers) {
      clearTimeout(timer);
    }
    focusRetryTimers.length = 0;
  }

  function focusInputWithRetries() {
    clearFocusRetryTimers();
    focusInputSoon();
    focusRetryTimers.push(setTimeout(() => focusInputSoon(), 120));
    focusRetryTimers.push(setTimeout(() => focusInputSoon(), 280));
  }

  function setInputRef(el: HTMLInputElement) {
    inputRef = el;
    focusInputSoon();
  }

  onMount(() => {
    // Record initial visibility state for WebView2 throttle investigation
    trace("ui:initial_visibility", {
      state: document.visibilityState,
      hidden: document.hidden,
    });

    // Track visibility changes to detect WebView2 suspend/resume
    const onVisibilityChange = () => {
      trace("ui:visibilitychange", {
        state: document.visibilityState,
        hidden: document.hidden,
      });
    };
    document.addEventListener("visibilitychange", onVisibilityChange);

    let unlistenWindowShown: (() => void) | undefined;
    let unlistenFocusChanged: (() => void) | undefined;
    void listen("window-shown", () => {
      const t0 = performance.now();
      trace("ui:window_shown");
      requestAnimationFrame(() => {
        trace("ui:window_shown:first_raf", {
          ms: Math.round((performance.now() - t0) * 100) / 100,
        });
      });
      focusInputWithRetries();
    }).then((unlisten) => {
      unlistenWindowShown = unlisten;
    }).catch((e) => console.warn("SearchWindow: failed to listen window-shown:", e));
    void getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        trace("ui:focus_changed", { focused });
        if (focused) {
          focusInputWithRetries();
        } else {
          clearFocusRetryTimers();
        }
      })
      .then((unlisten) => {
        unlistenFocusChanged = unlisten;
      })
      .catch((e) => console.warn("SearchWindow: failed to listen focus-changed:", e));

    // Fallback for startup timing: if first window-shown was emitted
    // before this listener mounted, focus once when already visible.
    void (async () => {
      try {
        if (await getCurrentWindow().isVisible()) {
          focusInputWithRetries();
        }
      } catch (e) {
        console.warn("SearchWindow: failed to check initial visibility:", e);
      }
    })();

    onCleanup(() => {
      clearFocusRetryTimers();
      document.removeEventListener("visibilitychange", onVisibilityChange);
      unlistenWindowShown?.();
      unlistenFocusChanged?.();
    });
  });

  function handleKeyDown(e: KeyboardEvent) {
    trace("ui:key_down", {
      key: e.key,
      alt: e.altKey,
      ctrl: e.ctrlKey,
      shift: e.shiftKey,
      folderMode: folderState() !== null,
      query: query(),
    });
    // Prevent system beep when Alt-modified character keys slip in during
    // focus transitions.  Do NOT inject the character into the DOM here —
    // doing so bypasses IME composition and corrupts the first keystroke
    // under the default ime_off_on_show=false configuration.
    // The Rust-side send_alt_key_up() is the primary mitigation; this guard
    // is a last-resort fallback that silently drops the Alt+char event.
    if (e.altKey && !e.ctrlKey && e.key.length === 1) {
      trace("ui:key_down:blocked_alt_char", { key: e.key });
      e.preventDefault();
      return;
    }

    switch (e.key) {
      case "Escape":
        trace("ui:key_action", { action: "escape" });
        if (!exitToolSelection() && !exitFolderExpansion()) {
          hideMainWindow();
        }
        e.preventDefault();
        break;
      case "ArrowUp":
        trace("ui:key_action", { action: "arrow_up" });
        moveSelectionUp();
        e.preventDefault();
        break;
      case "ArrowDown":
        trace("ui:key_action", { action: "arrow_down" });
        moveSelectionDown();
        e.preventDefault();
        break;
      case "ArrowRight": {
        trace("ui:key_action", { action: "arrow_right" });
        // 優先度の導出は allowsFolderNav() 側に集約済み（command は結果空で展開不能のため含めない）
        if (!allowsFolderNav()) break;
        const r = results()[selected()];
        if (r?.isFolder) {
          enterFolderExpansion(r.path);
          e.preventDefault();
        }
        break;
      }
      case "ArrowLeft":
        trace("ui:key_action", { action: "arrow_left" });
        if (!allowsFolderNav()) break;
        if (viewKind() === "folder") {
          navigateFolderUp();
          e.preventDefault();
        } else {
          const r = results()[selected()];
          if (r && !r.isError) {
            const parent = computeParentDir(r.path);
            if (parent) {
              enterFolderExpansion(parent);
              e.preventDefault();
            }
          }
        }
        break;
      case "Enter":
        trace("ui:key_action", { action: "enter", shift: e.shiftKey });
        if (e.shiftKey && allowsFolderNav()) {
          // Shift+Enter: ツール選択メニューを表示（0/1 ツール時は通常起動にフォールバック）
          const r = results()[selected()];
          if (r && !r.isError) {
            void enterToolSelection(r).then((launched) => {
              trace("ui:key_action:enter_done", { launched, shift: true });
              if (launched) void hideMainWindow();
            });
          }
        } else {
          void activateSelected().then((launched) => {
            trace("ui:key_action:enter_done", { launched });
            if (launched) void hideMainWindow();
          });
        }
        e.preventDefault();
        break;
    }
  }

  function handleInput(e: InputEvent) {
    // 入力可否は軸1(view)+overlay(launching)のみに依存。ツール選択中は無効化（C2対策）。
    // インスタントコマンド中(interp=instant)は受理する＝interp は読まない（綻び2）。
    if (viewKind() === "tool") return;
    if (launching()) return;
    const value = (e.target as HTMLInputElement).value;
    // インデックス構築中も setQuery は常に呼ぶ。IPC をスキップするガードは refreshResults() 側にある。
    // これにより、構築完了後の runRefresh() が最新の query() 値で即座に検索できる。
    trace("ui:input", { value, folderMode: folderState() !== null });
    perfMarkInput();
    clearLaunchNotice();
    if (viewKind() === "folder") {
      setFolderFilter(value);
    } else {
      setQuery(value);
    }
  }

  function inputValue(): string {
    // モード判定は viewKind 経由（優先度の再導出を避ける）。frame アクセスは storage を直読。
    const vk = viewKind();
    if (vk === "tool") {
      // ツール選択中はターゲットのファイル名を表示（readonly）
      const ts = toolSelectionState();
      if (ts) {
        const parts = ts.targetPath.split(/[\\/]/);
        return parts[parts.length - 1] ?? ts.targetPath;
      }
    }
    return vk === "folder" ? folderFilter() : query();
  }

  function placeholderText(): string {
    const vk = viewKind();
    if (vk === "tool") {
      return t("search.placeholder.tool_select");
    }
    if (vk === "folder") {
      const fs = folderState();
      if (fs) {
        return t("search.placeholder.folder", { dir: fs.currentDir });
      }
    }
    return t("search.placeholder.default");
  }

  return (
    <div class="search-bar" data-tauri-drag-region onKeyDown={handleKeyDown}>
      <input
        ref={setInputRef}
        type="text"
        class="search-input"
        placeholder={placeholderText()}
        value={inputValue()}
        onInput={handleInput}
        autofocus
      />
      <Switch>
        <Match when={indexing()}>
          <div class="search-overlay indexing-message" data-tauri-drag-region>
            {t("search.status.indexing")}
          </div>
        </Match>
        <Match when={launching() || launchNotice()}>
          <div
            class="search-overlay indexing-message"
            classList={{ "indexing-message--error": !launching() && launchNotice() !== null }}
            data-tauri-drag-region
          >
            {launching() ? t("search.status.launching") : launchNotice() ?? ""}
          </div>
        </Match>
        <Match when={noResults()}>
          <span class="no-results-hint">{t("search.status.no_results")}</span>
        </Match>
      </Switch>
    </div>
  );
};

export default SearchWindow;
