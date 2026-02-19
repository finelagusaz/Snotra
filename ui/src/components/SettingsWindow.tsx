import { type Component, Show, onMount } from "solid-js";
import {
  draft,
  status,
  activeTab,
  setActiveTab,
  hasChanges,
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
      <div class="settings-sidebar">
        <div class="sidebar-nav">
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
      </div>

      <div class="settings-main">
        <div class="settings-content">
          <Show when={draft()}>
            {activeTab() === "general" && <SettingsGeneral />}
            {activeTab() === "search" && <SettingsSearch />}
            {activeTab() === "index" && <SettingsIndex />}
            {activeTab() === "visual" && <SettingsVisual />}
          </Show>
        </div>

        <div class="settings-footer">
          <button
            class="btn-primary"
            classList={{ "has-changes": hasChanges() }}
            disabled={!hasChanges()}
            onClick={saveDraft}
          >
            {hasChanges() ? "保存" : "変更なし"}
          </button>
          <Show when={status()}>
            <span class="settings-status">{status()}</span>
          </Show>
        </div>
      </div>
    </div>
  );
};

export default SettingsWindow;
