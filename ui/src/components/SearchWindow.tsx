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
import * as api from "../lib/invoke";

async function hideAllWindows() {
  getCurrentWindow().hide();
  const rw = await WebviewWindow.getByLabel("results");
  if (rw) {
    rw.hide();
  }
}

const SearchWindow: Component = () => {
  let inputRef: HTMLInputElement | undefined;

  function setInputRef(el: HTMLInputElement) {
    inputRef = el;
    el.focus();
  }

  onMount(() => {
    refreshResults();
    listen("window-shown", () => {
      requestAnimationFrame(() => {
        inputRef?.focus();
      });
    });
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
        }
        break;
      case "Enter":
        // /o command opens settings
        if (!folderState() && query().trim() === "/o") {
          api.openSettings().catch((err) => console.error("openSettings failed:", err));
          setQuery("");
          hideAllWindows();
        } else {
          activateSelected();
        }
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
    <div class="search-bar" onKeyDown={handleKeyDown}>
      <Show
        when={!indexing()}
        fallback={<div class="indexing-message">インデックス構築中...</div>}
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
