import type { Component } from "solid-js";
import { createEffect, createSignal, For, Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import { draft, updateDraft, setStatus } from "../stores/settings";
import SettingRow from "./SettingRow";
import ToggleSwitch from "./ToggleSwitch";

function normalizeScanPathKey(path: string): string {
  let key = path.trim().replace(/\//g, "\\").toLowerCase();
  if (key.endsWith("\\") && !/^[a-z]:\\$/.test(key)) {
    key = key.replace(/\\+$/, "");
  }
  return key;
}

function parseExtensions(raw: string): string[] {
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

function mergeExtensions(target: { extensions: string[] }, exts: string[]): void {
  for (const ext of exts) {
    const norm = ext.startsWith(".") ? ext.toLowerCase() : `.${ext.toLowerCase()}`;
    if (!target.extensions.some((e) => e.toLowerCase() === norm)) {
      target.extensions.push(norm);
    }
  }
  target.extensions.sort();
}

const SettingsIndex: Component = () => {
  const d = () => draft()!;

  const [modalMode, setModalMode] = createSignal<"create" | "edit" | null>(null);
  const [editingIndex, setEditingIndex] = createSignal<number | null>(null);
  const [editPath, setEditPath] = createSignal("");
  const [editExtensions, setEditExtensions] = createSignal("");
  const [editIncludeFolders, setEditIncludeFolders] = createSignal(false);
  let pathInputRef: HTMLInputElement | undefined;

  function findDuplicateIndex(path: string, excludeIndex: number | null): number {
    const key = normalizeScanPathKey(path);
    if (!key) return -1;
    return d().paths.scan.findIndex(
      (sp, i) => i !== excludeIndex && normalizeScanPathKey(sp.path) === key
    );
  }

  createEffect(() => {
    if (modalMode() === null) return;
    queueMicrotask(() => {
      pathInputRef?.focus();
      pathInputRef?.select();
    });
  });

  function resetForm() {
    setEditPath("");
    setEditExtensions("");
    setEditIncludeFolders(false);
  }

  function closeModal() {
    setModalMode(null);
    setEditingIndex(null);
    resetForm();
  }

  function openCreateModal() {
    setEditingIndex(null);
    resetForm();
    setModalMode("create");
  }

  function openEditModal(index: number) {
    const scan = d().paths.scan[index];
    if (!scan) return;
    setEditingIndex(index);
    setEditPath(scan.path);
    setEditExtensions(scan.extensions.join(", "));
    setEditIncludeFolders(scan.include_folders);
    setModalMode("edit");
  }

  function saveScanPath() {
    const path = editPath();
    const extensions = parseExtensions(editExtensions());
    const includeFolders = editIncludeFolders();

    if (modalMode() === "edit") {
      const idx = editingIndex();
      if (idx === null) return;

      const dupIdx = findDuplicateIndex(path, idx);
      if (dupIdx >= 0) {
        updateDraft((c) => {
          mergeExtensions(c.paths.scan[dupIdx], extensions);
          if (includeFolders) c.paths.scan[dupIdx].include_folders = true;
          c.paths.scan.splice(idx, 1);
        });
        setStatus("重複するパスを統合しました");
        closeModal();
        return;
      }

      updateDraft((c) => {
        c.paths.scan[idx].path = path;
        c.paths.scan[idx].extensions = extensions;
        c.paths.scan[idx].include_folders = includeFolders;
      });
      closeModal();
      return;
    }

    const dupIdx = findDuplicateIndex(path, null);
    if (dupIdx >= 0) {
      updateDraft((c) => {
        mergeExtensions(c.paths.scan[dupIdx], extensions);
        if (includeFolders) c.paths.scan[dupIdx].include_folders = true;
      });
      setStatus("既存のパスに統合しました");
      closeModal();
      return;
    }

    updateDraft((c) => {
      c.paths.scan.push({ path, extensions, include_folders: includeFolders });
    });
    closeModal();
  }

  function removeScanPath() {
    const idx = editingIndex();
    if (idx === null) return;
    updateDraft((c) => {
      c.paths.scan.splice(idx, 1);
    });
    closeModal();
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

  function handleModalKeyDown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    closeModal();
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
          <div class="scan-path-list">
            <For
              each={d().paths.scan}
              fallback={
                <div class="scan-path-list-empty">
                  スキャンパスはまだ登録されていません
                </div>
              }
            >
              {(scan, idx) => (
                <div class="scan-path-item scan-path-item--editable">
                  <div class="scan-path-item-main">
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
                  <button
                    type="button"
                    class="scan-path-edit-button"
                    onClick={() => openEditModal(idx())}
                  >
                    編集
                  </button>
                </div>
              )}
            </For>
          </div>
          <div class="scan-path-list-actions">
            <button type="button" onClick={openCreateModal}>
              追加
            </button>
          </div>
        </div>
      </div>

      <Show when={modalMode() !== null}>
        <div class="settings-modal-backdrop" onClick={closeModal}>
          <div
            class="settings-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="scan-path-modal-title"
            onClick={(e) => e.stopPropagation()}
            onKeyDown={handleModalKeyDown}
          >
            <div class="settings-modal-header">
              <div id="scan-path-modal-title" class="settings-modal-title">
                {modalMode() === "edit" ? "スキャンパスを編集" : "スキャンパスを追加"}
              </div>
              <button
                type="button"
                class="settings-modal-close"
                onClick={closeModal}
                aria-label="閉じる"
              >
                x
              </button>
            </div>

            <div class="scan-path-form">
              <label>
                パス
                <div class="scan-path-input-row">
                  <input
                    ref={pathInputRef}
                    type="text"
                    value={editPath()}
                    onInput={(e) => setEditPath(e.currentTarget.value)}
                    placeholder="C:\..."
                  />
                  <button type="button" class="btn-browse" onClick={browsePath}>
                    参照...
                  </button>
                </div>
                <span class="scan-path-form-hint">同じパスは自動的に統合されます</span>
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
              <div class="scan-path-form-actions scan-path-form-actions--modal">
                <Show when={modalMode() === "edit"}>
                  <button type="button" class="btn-danger" onClick={removeScanPath}>
                    削除
                  </button>
                </Show>
                <div class="scan-path-form-actions-primary">
                  <button type="button" onClick={closeModal}>
                    キャンセル
                  </button>
                  <button type="button" class="btn-primary" onClick={saveScanPath}>
                    保存
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default SettingsIndex;
