import { createSignal } from "solid-js";
import type { Config } from "../lib/types";
import { isHotkeyInvalid } from "../lib/hotkeyValidation";
import * as api from "../lib/invoke";
import { t } from "../lib/i18n";

const [draft, setDraft] = createSignal<Config | null>(null);
let statusClearTimer: ReturnType<typeof setTimeout> | undefined;
const [savedConfig, setSavedConfig] = createSignal<Config | null>(null);
const [status, setStatus] = createSignal("");

export type TabId = "general" | "search" | "index" | "visual" | "opener";

const [activeTab, setActiveTab] = createSignal<TabId>("general");

// Cache the JSON.stringify result to avoid redundant serialization
// when hasChanges is called multiple times within the same reactive cycle.
let cachedDraftJson = "";
let cachedSavedJson = "";
let cachedDraftRef: Config | null = null;
let cachedSavedRef: Config | null = null;

function hasChanges(): boolean {
  const d = draft();
  const s = savedConfig();
  if (!d || !s) return false;
  if (d !== cachedDraftRef) {
    cachedDraftJson = JSON.stringify(d);
    cachedDraftRef = d;
  }
  if (s !== cachedSavedRef) {
    cachedSavedJson = JSON.stringify(s);
    cachedSavedRef = s;
  }
  return cachedDraftJson !== cachedSavedJson;
}

function canSave(): boolean {
  if (!hasChanges()) return false;
  const d = draft();
  if (!d) return false;
  return !isHotkeyInvalid(d.hotkey.modifier, d.hotkey.key);
}

async function loadDraft() {
  try {
    const config = await api.getConfig();
    setDraft(structuredClone(config));
    setSavedConfig(structuredClone(config));
    setStatus("");
  } catch (e) {
    console.error("Failed to load config:", e);
    setStatus(t("status.load_failed"));
  }
}


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
    setStatus(t("status.saved"));
    clearTimeout(statusClearTimer);
    statusClearTimer = setTimeout(() => setStatus(""), 3000);
  } catch (e) {
    if (e === "hotkey_registration_failed") {
      setStatus(t("status.save_failed.hotkey"));
    } else {
      setStatus(t("status.save_failed.generic", { error: String(e) }));
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
