import { createSignal } from "solid-js";
import type { Config } from "../lib/types";
import * as api from "../lib/invoke";

const [draft, setDraft] = createSignal<Config | null>(null);
const [status, setStatus] = createSignal("");
const [activeTab, setActiveTab] = createSignal<
  "general" | "search" | "index" | "visual"
>("general");

async function loadDraft() {
  try {
    const config = await api.getConfig();
    setDraft(structuredClone(config));
    setStatus("");
  } catch (e) {
    console.error("Failed to load config:", e);
    setStatus("設定の読み込みに失敗しました");
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
    setStatus("保存しました");
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
  loadDraft,
  updateDraft,
  saveDraft,
};
