import { type Component, Show, onMount, onCleanup } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import {
  type TabId,
  draft,
  status,
  activeTab,
  setActiveTab,
  hasChanges,
  canSave,
  loadDraft,
  saveDraft,
} from "../stores/settings";
import * as api from "../lib/invoke";
import { t } from "../lib/i18n";
import SettingsGeneral from "./SettingsGeneral";
import SettingsSearch from "./SettingsSearch";
import SettingsIndex from "./SettingsIndex";
import SettingsVisual from "./SettingsVisual";
import SettingsOpener from "./SettingsOpener";

const TABS: { id: TabId; label: string }[] = [
  { id: "general", label: t("settings.tab.general") },
  { id: "search", label: t("settings.tab.search") },
  { id: "index", label: t("settings.tab.index") },
  { id: "visual", label: t("settings.tab.visual") },
  { id: "opener", label: t("settings.tab.opener") },
];

const SettingsWindow: Component = () => {
  let tablistRef: HTMLDivElement | undefined;

  onMount(() => {
    loadDraft();

    // Reload config each time the settings window is shown.
    // The window is pre-created and hidden on close, so onMount only fires once.
    let unlistenShown: (() => void) | undefined;
    onCleanup(() => unlistenShown?.());
    void listen("settings-shown", () => {
      loadDraft();
    }).then((fn) => { unlistenShown = fn; });

    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // ホットキー入力中は window-close を抑止（SettingsGeneral で clearHotkey を処理）
        if (document.activeElement?.classList.contains("hotkey-input")) return;
        e.preventDefault();
        if (hasChanges()) {
          return; // バナーは hasChanges() で既に表示、footer ボタンで操作
        }
        void api.hideSettings();
      }
    };
    window.addEventListener("keydown", handler);
    onCleanup(() => window.removeEventListener("keydown", handler));

    let unlistenClose: (() => void) | undefined;
    onCleanup(() => unlistenClose?.());
    getCurrentWindow().onCloseRequested(async (event) => {
      event.preventDefault();
      if (hasChanges()) {
        return; // バナー表示、footer ボタンで操作
      }
      void api.hideSettings();
    }).then(fn => { unlistenClose = fn; });
  });

  return (
    <div class="settings-window">
      <div class="settings-sidebar">
        <div class="sidebar-nav" role="tablist" aria-orientation="vertical" ref={tablistRef}>
          {TABS.map((tab, i) => (
            <button
              role="tab"
              aria-selected={activeTab() === tab.id}
              classList={{ active: activeTab() === tab.id }}
              onClick={() => setActiveTab(tab.id)}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") {
                  const next = (i + 1) % TABS.length;
                  setActiveTab(TABS[next].id);
                  (tablistRef?.querySelectorAll('[role="tab"]')[next] as HTMLElement | undefined)?.focus();
                  e.preventDefault();
                } else if (e.key === "ArrowUp") {
                  const next = (i - 1 + TABS.length) % TABS.length;
                  setActiveTab(TABS[next].id);
                  (tablistRef?.querySelectorAll('[role="tab"]')[next] as HTMLElement | undefined)?.focus();
                  e.preventDefault();
                }
              }}
            >
              {tab.label}
            </button>
          ))}
        </div>
      </div>

      <div class="settings-main">
        <div class="settings-content">
          <Show when={draft()} fallback={<div class="settings-loading">{t("settings.loading")}</div>}>
            <>
              {activeTab() === "general" && <SettingsGeneral />}
              {activeTab() === "search" && <SettingsSearch />}
              {activeTab() === "index" && <SettingsIndex />}
              {activeTab() === "visual" && <SettingsVisual />}
              {activeTab() === "opener" && <SettingsOpener />}
            </>
          </Show>
        </div>

        <Show when={hasChanges()}>
          <div class="settings-discard-banner">
            <span class="settings-discard-message">{t("settings.unsaved_changes")}</span>
          </div>
        </Show>

        <div class="settings-footer">
          <button
            class="btn-primary"
            classList={{ "has-changes": canSave() }}
            disabled={!canSave()}
            onClick={saveDraft}
          >
            {hasChanges() ? t("settings.save") : t("settings.no_changes")}
          </button>
          <Show when={hasChanges()}>
            <button
              type="button"
              onClick={() => void api.hideSettings()}
            >
              {t("settings.discard")}
            </button>
          </Show>
          <Show when={status()}>
            <span class="settings-status">{status()}</span>
          </Show>
        </div>
      </div>
    </div>
  );
};

export default SettingsWindow;
