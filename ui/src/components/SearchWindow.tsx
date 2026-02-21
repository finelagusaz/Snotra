import { type Component, onMount, Show } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
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
  refreshResults,
  indexing,
} from "../stores/search";
import { initCommands } from "../lib/commands";

async function hideAllWindows() {
  getCurrentWindow().hide();
  const rw = await WebviewWindow.getByLabel("results");
  if (rw) {
    rw.hide();
  }
}

const SearchWindow: Component = () => {
  let inputRef: HTMLInputElement | undefined;

  function focusInputSoon() {
    // Two-frame defer avoids first-show races with native show/focus timing.
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        inputRef?.focus();
      });
    });
  }

  function setInputRef(el: HTMLInputElement) {
    inputRef = el;
    focusInputSoon();
  }

  onMount(() => {
    initCommands(hideAllWindows);
    refreshResults();
    listen("window-shown", () => {
      focusInputSoon();
    });

    // Fallback for startup timing: if first window-shown was emitted
    // before this listener mounted, focus once when already visible.
    void (async () => {
      if (await getCurrentWindow().isVisible()) {
        focusInputSoon();
      }
    })();
  });

  function handleKeyDown(e: KeyboardEvent) {
    switch (e.key) {
      case "Escape":
        if (!exitFolderExpansion()) {
          hideAllWindows();
        }
        e.preventDefault();
        break;
      case "ArrowUp":
        moveSelectionUp();
        e.preventDefault();
        break;
      case "ArrowDown":
        moveSelectionDown();
        e.preventDefault();
        break;
      case "ArrowRight": {
        const r = results()[selected()];
        if (r?.isFolder) {
          enterFolderExpansion(r.path);
          e.preventDefault();
        }
        break;
      }
      case "ArrowLeft":
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
        activateSelected();
        e.preventDefault();
        break;
    }
  }

  function handleInput(e: InputEvent) {
    const value = (e.target as HTMLInputElement).value;
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
