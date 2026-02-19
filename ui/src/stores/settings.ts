import { createSignal } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import type { Config } from "../lib/types";
import * as api from "../lib/invoke";

const [draft, setDraft] = createSignal<Config | null>(null);
const [savedConfig, setSavedConfig] = createSignal<Config | null>(null);
const [status, setStatus] = createSignal("");

const [activeTab, setActiveTab] = createSignal<
  "general" | "search" | "index" | "visual"
>("general");

function hasChanges(): boolean {
  const d = draft();
  const s = savedConfig();
  if (!d || !s) return false;
  return JSON.stringify(d) !== JSON.stringify(s);
}

async function loadDraft() {
  try {
    const config = await api.getConfig();
    setDraft(structuredClone(config));
    setSavedConfig(structuredClone(config));
    setStatus("");
  } catch (e) {
    console.error("Failed to load config:", e);
    setStatus("設定の読み込みに失敗しました");
  }
}

// Reload config each time the settings window is shown.
// The window is pre-created and hidden on close, so onMount only fires once.
listen("settings-shown", () => {
  loadDraft();
});

listen("indexing-complete", () => {
  if (status() === "保存しました（インデックスを再構築中…）") {
    setStatus("保存しました（インデックスの再構築が完了しました）");
  }
});

function updateDraft(updater: (c: Config) => void) {
  const d = draft();
  if (!d) return;
  const clone = structuredClone(d);
  updater(clone);
  setDraft(clone);
}

async function saveDraft() {
  const d = draft();
  if (!d) return;
  try {
    const result = await api.saveConfig(d);
    setSavedConfig(structuredClone(d));
    setStatus(
      result.reindex_started
        ? "保存しました（インデックスを再構築中…）"
        : "保存しました",
    );
  } catch (e) {
    setStatus(`保存に失敗: ${e}`);
  }
}

export {
  draft,
  setDraft,
  status,
  setStatus,
  activeTab,
  setActiveTab,
  hasChanges,
  loadDraft,
  updateDraft,
  saveDraft,
};
