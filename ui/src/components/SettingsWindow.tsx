import { type Component, Show, onMount } from "solid-js";
import {
  draft,
  status,
  activeTab,
  setActiveTab,
  loadDraft,
  saveDraft,
} from "../stores/settings";
import SettingsGeneral from "./SettingsGeneral";
import SettingsSearch from "./SettingsSearch";
import SettingsIndex from "./SettingsIndex";
import SettingsVisual from "./SettingsVisual";

const SettingsWindow: Component = () => {
  onMount(() => {
    loadDraft();
  });

  return (
    <div class="settings-window">
      <div class="settings-tabs">
        <button
          classList={{ active: activeTab() === "general" }}
          onClick={() => setActiveTab("general")}
        >
          全般
        </button>
        <button
          classList={{ active: activeTab() === "search" }}
          onClick={() => setActiveTab("search")}
        >
          検索
        </button>
        <button
          classList={{ active: activeTab() === "index" }}
          onClick={() => setActiveTab("index")}
        >
          インデックス
        </button>
        <button
          classList={{ active: activeTab() === "visual" }}
          onClick={() => setActiveTab("visual")}
        >
          ビジュアル
        </button>
      </div>

      <div class="settings-content">
        <Show when={draft()}>
          {activeTab() === "general" && <SettingsGeneral />}
          {activeTab() === "search" && <SettingsSearch />}
          {activeTab() === "index" && <SettingsIndex />}
          {activeTab() === "visual" && <SettingsVisual />}
        </Show>
      </div>

      <div class="settings-footer">
        <div class="settings-actions">
          <button class="btn-primary" onClick={saveDraft}>
            保存
          </button>
        </div>
        <Show when={status()}>
          <span class="settings-status">{status()}</span>
        </Show>
      </div>
    </div>
  );
};

export default SettingsWindow;
