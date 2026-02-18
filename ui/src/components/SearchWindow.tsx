import { type Component, onMount } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
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

  onMount(() => {
    refreshResults();
    inputRef?.focus();
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
      <input
        ref={inputRef}
        type="text"
        class="search-input"
        placeholder={placeholderText()}
        value={inputValue()}
        onInput={handleInput}
        autofocus
      />
    </div>
  );
};

export default SearchWindow;
