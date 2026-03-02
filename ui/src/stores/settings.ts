import { createSignal } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import type { Config } from "../lib/types";
import { isHotkeyInvalid } from "../lib/hotkeyValidation";
import * as api from "../lib/invoke";

const [draft, setDraft] = createSignal<Config | null>(null);
let statusClearTimer: ReturnType<typeof setTimeout> | undefined;
const [savedConfig, setSavedConfig] = createSignal<Config | null>(null);
const [status, setStatus] = createSignal("");

const [activeTab, setActiveTab] = createSignal<
  "general" | "search" | "index" | "visual" | "opener"
>("general");

function hasChanges(): boolean {
  const d = draft();
  const s = savedConfig();
  if (!d || !s) return false;
  return JSON.stringify(d) !== JSON.stringify(s);
}

function isHotkeyValid(): boolean {
  const d = draft();
  if (!d) return false;
  return !isHotkeyInvalid(d.hotkey.modifier, d.hotkey.key);
}

function canSave(): boolean {
  return hasChanges() && isHotkeyValid();
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
    await api.saveConfig(d);
    setSavedConfig(structuredClone(d));
    setStatus("保存しました");
    clearTimeout(statusClearTimer);
    statusClearTimer = setTimeout(() => setStatus(""), 3000);
  } catch (e) {
    if (e === "hotkey_registration_failed") {
      setStatus("保存に失敗: ホットキーの登録に失敗しました（他のアプリが同じキーを使用している可能性があります）");
    } else {
      setStatus(`保存に失敗: ${e}`);
    }
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
  canSave,
  loadDraft,
  updateDraft,
  saveDraft,
};
