import { type Component, createSignal, Show, onMount, onCleanup } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";
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
import SettingsGeneral from "./SettingsGeneral";
import SettingsSearch from "./SettingsSearch";
import SettingsIndex from "./SettingsIndex";
import SettingsVisual from "./SettingsVisual";
import SettingsOpener from "./SettingsOpener";

const TABS: { id: TabId; label: string }[] = [
  { id: "general", label: "全般" },
  { id: "search", label: "検索" },
  { id: "index", label: "インデックス・表示" },
  { id: "visual", label: "ビジュアル" },
  { id: "opener", label: "オープナー" },
];

const SettingsWindow: Component = () => {
  const [showDiscardBanner, setShowDiscardBanner] = createSignal(false);
  let tablistRef: HTMLDivElement | undefined;
  let allowClose = false;

  onMount(() => {
    loadDraft();
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // ホットキー入力中は window-close を抑止（SettingsGeneral で clearHotkey を処理）
        if (document.activeElement?.classList.contains("hotkey-input")) return;
        e.preventDefault();
        if (hasChanges()) {
          setShowDiscardBanner(true);
          return;
        }
        void getCurrentWindow().close();
      }
    };
    window.addEventListener("keydown", handler);
    onCleanup(() => window.removeEventListener("keydown", handler));

    let unlistenClose: (() => void) | undefined;
    onCleanup(() => unlistenClose?.());
    getCurrentWindow().onCloseRequested((event) => {
      if (allowClose) {
        allowClose = false;
        return;
      }
      if (hasChanges()) {
        event.preventDefault();
        setShowDiscardBanner(true);
      }
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
          <Show when={draft()} fallback={<div class="settings-loading">設定を読み込み中...</div>}>
            <>
              {activeTab() === "general" && <SettingsGeneral />}
              {activeTab() === "search" && <SettingsSearch />}
              {activeTab() === "index" && <SettingsIndex />}
              {activeTab() === "visual" && <SettingsVisual />}
              {activeTab() === "opener" && <SettingsOpener />}
            </>
          </Show>
        </div>

        <Show when={showDiscardBanner()}>
          <div class="settings-discard-banner">
            <span class="settings-discard-message">未保存の変更があります。</span>
            <button
              type="button"
              class="btn-primary"
              onClick={async () => {
                await saveDraft();
                if (!hasChanges()) {
                  setShowDiscardBanner(false);
                  void getCurrentWindow().close();
                }
              }}
            >
              保存して閉じる
            </button>
            <button
              type="button"
              onClick={() => {
                allowClose = true;
                setShowDiscardBanner(false);
                void getCurrentWindow().close();
              }}
            >
              破棄して閉じる
            </button>
            <button type="button" onClick={() => setShowDiscardBanner(false)}>
              キャンセル
            </button>
          </div>
        </Show>

        <div class="settings-footer">
          <button
            class="btn-primary"
            classList={{ "has-changes": canSave() }}
            disabled={!canSave()}
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
