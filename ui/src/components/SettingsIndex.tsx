import type { Component } from "solid-js";
import { createEffect, createSignal, For, Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import { draft, updateDraft } from "../stores/settings";
import SettingRow from "./SettingRow";
import ToggleSwitch from "./ToggleSwitch";

const SettingsIndex: Component = () => {
  const d = () => draft()!;

  const [selectedIndex, setSelectedIndex] = createSignal<number | null>(null);
  const [editPath, setEditPath] = createSignal("");
  const [editExtensions, setEditExtensions] = createSignal("");
  const [editIncludeFolders, setEditIncludeFolders] = createSignal(false);

  // Sync form fields when selection changes
  createEffect(() => {
    const idx = selectedIndex();
    if (idx === null) {
      setEditPath("");
      setEditExtensions("");
      setEditIncludeFolders(false);
    } else {
      const scan = d().paths.scan[idx];
      if (scan) {
        setEditPath(scan.path);
        setEditExtensions(scan.extensions.join(", "));
        setEditIncludeFolders(scan.include_folders);
      }
    }
  });

  function applyEdit() {
    const idx = selectedIndex();
    if (idx === null) return;
    updateDraft((c) => {
      c.paths.scan[idx].path = editPath();
      c.paths.scan[idx].extensions = editExtensions()
        .split(",")
        .map((s) => s.trim())
        .filter((s) => s.length > 0);
      c.paths.scan[idx].include_folders = editIncludeFolders();
    });
  }

  function addScanPath() {
    const path = editPath();
    const extensions = editExtensions()
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    const includeFolders = editIncludeFolders();
    updateDraft((c) => {
      c.paths.scan.push({ path, extensions, include_folders: includeFolders });
    });
    // Select the newly added item
    setSelectedIndex(d().paths.scan.length - 1);
  }

  function removeScanPath() {
    const idx = selectedIndex();
    if (idx === null) return;
    updateDraft((c) => {
      c.paths.scan.splice(idx, 1);
    });
    setSelectedIndex(null);
  }

  function startNew() {
    setSelectedIndex(null);
  }

  async function browsePath() {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: editPath() || undefined,
    });
    if (selected !== null) {
      setEditPath(selected as string);
    }
  }

  function formatExtensions(exts: string[]): string {
    return exts.join(", ");
  }

  return (
    <div class="settings-section">
      <div class="settings-group">
        <div class="settings-group-title">表示設定</div>
        <div class="settings-group-content">
          <SettingRow
            label="最大表示件数"
            description="検索結果に表示する最大候補数"
          >
            <input
              type="number"
              min="1"
              max="50"
              value={d().appearance.max_results}
              onInput={(e) =>
                updateDraft((c) => {
                  c.appearance.max_results =
                    parseInt(e.currentTarget.value) || 8;
                })
              }
              style={{ width: "80px" }}
            />
          </SettingRow>
          <SettingRow
            label="ウィンドウ幅"
            description="検索ウィンドウの横幅（論理ピクセル）"
          >
            <input
              type="number"
              min="300"
              max="1200"
              value={d().appearance.window_width}
              onInput={(e) =>
                updateDraft((c) => {
                  c.appearance.window_width =
                    parseInt(e.currentTarget.value) || 600;
                })
              }
              style={{ width: "80px" }}
            />
          </SettingRow>
          <SettingRow
            label="アイコン表示"
            description="検索結果にファイルアイコンを表示します"
          >
            <ToggleSwitch
              checked={d().appearance.show_icons}
              onChange={(v) =>
                updateDraft((c) => {
                  c.appearance.show_icons = v;
                })
              }
            />
          </SettingRow>
        </div>
      </div>

      <div class="settings-group">
        <div class="settings-group-title">履歴</div>
        <div class="settings-group-content">
          <SettingRow
            label="履歴保持数"
            description="保持する起動履歴の件数"
          >
            <input
              type="number"
              min="10"
              max="1000"
              value={d().appearance.top_n_history}
              onInput={(e) =>
                updateDraft((c) => {
                  c.appearance.top_n_history =
                    parseInt(e.currentTarget.value) || 200;
                })
              }
              style={{ width: "80px" }}
            />
          </SettingRow>
        </div>
      </div>

      <div class="settings-group">
        <div class="settings-group-title">スキャンパス</div>
        <div class="settings-group-content">
          {/* List */}
          <div class="scan-path-list">
            <For each={d().paths.scan}>
              {(scan, idx) => (
                <div
                  class="scan-path-item"
                  classList={{ selected: selectedIndex() === idx() }}
                  onClick={() => setSelectedIndex(idx())}
                >
                  <div class="scan-path-item-path">{scan.path || "(未設定)"}</div>
                  <div class="scan-path-item-meta">
                    <span class="scan-path-item-exts">
                      {formatExtensions(scan.extensions) || "(拡張子未指定)"}
                    </span>
                    <Show when={scan.include_folders}>
                      <span class="scan-path-item-folder-badge" title="フォルダを含む">&#x1F4C1;</span>
                    </Show>
                  </div>
                </div>
              )}
            </For>
          </div>

          {/* Edit form */}
          <div class="scan-path-form">
            <label>
              パス
              <div class="scan-path-input-row">
                <input
                  type="text"
                  value={editPath()}
                  onInput={(e) => setEditPath(e.currentTarget.value)}
                  placeholder="C:\..."
                />
                <button type="button" class="btn-browse" onClick={browsePath}>
                  参照...
                </button>
              </div>
            </label>
            <label>
              拡張子 (カンマ区切り)
              <input
                type="text"
                value={editExtensions()}
                onInput={(e) => setEditExtensions(e.currentTarget.value)}
                placeholder=".lnk, .exe"
              />
            </label>
            <div class="scan-path-form-toggle">
              <ToggleSwitch
                checked={editIncludeFolders()}
                onChange={(v) => setEditIncludeFolders(v)}
              />
              <span>フォルダを含める</span>
            </div>
            <div class="scan-path-form-actions">
              <Show
                when={selectedIndex() !== null}
                fallback={
                  <button onClick={addScanPath}>追加</button>
                }
              >
                <button onClick={applyEdit}>適用</button>
                <button class="btn-danger" onClick={removeScanPath}>
                  削除
                </button>
              </Show>
              <Show when={selectedIndex() !== null}>
                <button
                  style={{ "margin-left": "auto" }}
                  onClick={startNew}
                >
                  新規追加
                </button>
              </Show>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default SettingsIndex;
