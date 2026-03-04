import { type Component, createMemo, onCleanup, onMount, Show } from "solid-js";
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
} from "../stores/search";
import { hideAllWindows } from "../lib/commands";
import { perfMarkInput } from "../lib/perf";
import { trace } from "../lib/trace";
import { t } from "../lib/i18n";

const SearchWindow: Component = () => {
  let inputRef: HTMLInputElement | undefined;
  const focusRetryTimers: ReturnType<typeof setTimeout>[] = [];

  function focusInputSoon() {
    // Two-frame defer avoids first-show races with native show/focus timing.
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        inputRef?.focus();
      });
    });
  }

  function clearFocusRetryTimers() {
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
    let unlistenWindowShown: (() => void) | undefined;
    let unlistenFocusChanged: (() => void) | undefined;
    void listen("window-shown", () => {
      trace("ui:window_shown");
      focusInputWithRetries();
    }).then((unlisten) => {
      unlistenWindowShown = unlisten;
    });
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
      });

    // Fallback for startup timing: if first window-shown was emitted
    // before this listener mounted, focus once when already visible.
    void (async () => {
      if (await getCurrentWindow().isVisible()) {
        focusInputWithRetries();
      }
    })();

    onCleanup(() => {
      clearFocusRetryTimers();
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
    // Prevent system beep when Alt-modified character keys slip in during focus transitions.
    if (e.altKey && !e.ctrlKey && e.key.length === 1) {
      trace("ui:key_down:blocked_alt_char", { key: e.key });
      e.preventDefault();
      return;
    }

    switch (e.key) {
      case "Escape":
        trace("ui:key_action", { action: "escape" });
        if (!exitToolSelection() && !exitFolderExpansion()) {
          hideAllWindows();
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
        if (toolSelectionState()) break;
        const r = results()[selected()];
        if (r?.isFolder) {
          enterFolderExpansion(r.path);
          e.preventDefault();
        }
        break;
      }
      case "ArrowLeft":
        trace("ui:key_action", { action: "arrow_left" });
        if (toolSelectionState()) break;
        if (folderState()) {
          navigateFolderUp();
          e.preventDefault();
        } else {
          const r = results()[selected()];
          if (r && !r.isError) {
            let parent = r.path.replace(/\\[^\\]+$/, "");
            if (/^[A-Za-z]:$/.test(parent)) {
              parent += "\\";
            }
            if (parent && parent !== r.path) {
              enterFolderExpansion(parent);
              e.preventDefault();
            }
          }
        }
        break;
      case "Enter":
        trace("ui:key_action", { action: "enter", shift: e.shiftKey });
        if (e.shiftKey && !toolSelectionState()) {
          // Shift+Enter: ツール選択メニューを表示（0/1 ツール時は通常起動にフォールバック）
          const r = results()[selected()];
          if (r && !r.isError) {
            void enterToolSelection(r).then((launched) => {
              trace("ui:key_action:enter_done", { launched, shift: true });
              if (launched) void hideAllWindows();
            });
          }
        } else {
          void activateSelected().then((launched) => {
            trace("ui:key_action:enter_done", { launched });
            if (launched) void hideAllWindows();
          });
        }
        e.preventDefault();
        break;
    }
  }

  function handleInput(e: InputEvent) {
    // ツール選択中は入力を無効化（C2対策）
    if (toolSelectionState()) return;
    // インデックス構築中は入力を無視
    if (indexing()) return;
    const value = (e.target as HTMLInputElement).value;
    trace("ui:input", { value, folderMode: folderState() !== null });
    perfMarkInput();
    clearLaunchNotice();
    if (folderState()) {
      setFolderFilter(value);
    } else {
      setQuery(value);
    }
  }

  function inputValue(): string {
    const ts = toolSelectionState();
    if (ts) {
      // ツール選択中はターゲットのファイル名を表示（readonly）
      const parts = ts.targetPath.split(/[\\/]/);
      return parts[parts.length - 1] ?? ts.targetPath;
    }
    return folderState() ? folderFilter() : query();
  }

  function placeholderText(): string {
    if (toolSelectionState()) {
      return t("search.placeholder.tool_select");
    }
    const fs = folderState();
    if (fs) {
      return t("search.placeholder.folder", { dir: fs.currentDir });
    }
    return t("search.placeholder.default");
  }

  const noResults = createMemo(() =>
    results().length === 0 &&
    !indexing() &&
    query().length > 0 &&
    !toolSelectionState() &&
    !folderState()
  );

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
      <Show when={indexing()}>
        <div class="search-overlay indexing-message" data-tauri-drag-region>
          {t("search.status.indexing")}
        </div>
      </Show>
      <Show when={launching() || launchNotice()}>
        <div
          class="search-overlay indexing-message"
          classList={{ "indexing-message--error": !launching() && launchNotice() !== null }}
          data-tauri-drag-region
        >
          {launching() ? t("search.status.launching") : launchNotice() ?? ""}
        </div>
      </Show>
      <Show when={noResults()}>
        <span class="no-results-hint">{t("search.status.no_results")}</span>
      </Show>
    </div>
  );
};

export default SearchWindow;
