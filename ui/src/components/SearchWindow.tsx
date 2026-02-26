import { type Component, onCleanup, onMount, Show } from "solid-js";
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
} from "../stores/search";
import { hideAllWindows } from "../lib/commands";
import { perfMarkInput } from "../lib/perf";
import { trace } from "../lib/trace";

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
        if (!exitFolderExpansion()) {
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
        const r = results()[selected()];
        if (r?.isFolder) {
          enterFolderExpansion(r.path);
          e.preventDefault();
        }
        break;
      }
      case "ArrowLeft":
        trace("ui:key_action", { action: "arrow_left" });
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
        trace("ui:key_action", { action: "enter" });
        void activateSelected().then((launched) => {
          trace("ui:key_action:enter_done", { launched });
          if (launched) void hideAllWindows();
        });
        e.preventDefault();
        break;
    }
  }

  function handleInput(e: InputEvent) {
    const value = (e.target as HTMLInputElement).value;
    trace("ui:input", { value, folderMode: folderState() !== null });
    perfMarkInput();
    if (folderState()) {
      setFolderFilter(value);
    } else {
      setQuery(value);
    }
  }

  function inputValue(): string {
    return folderState() ? folderFilter() : query();
  }

  function placeholderText(): string {
    const fs = folderState();
    if (fs) {
      return `${fs.currentDir} 内を検索...`;
    }
    return "検索...";
  }

  return (
    <div class="search-bar" data-tauri-drag-region onKeyDown={handleKeyDown}>
      <Show
        when={!indexing()}
        fallback={<div class="indexing-message" data-tauri-drag-region>インデックス構築中...</div>}
      >
        <input
          ref={setInputRef}
          type="text"
          class="search-input"
          placeholder={placeholderText()}
          value={inputValue()}
          onInput={handleInput}
          autofocus
        />
      </Show>
    </div>
  );
};

export default SearchWindow;
