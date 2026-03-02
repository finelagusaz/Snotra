import type { Component } from "solid-js";
import { createSignal, For, Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import { draft, updateDraft, setStatus } from "../stores/settings";
import SettingsEditableList from "./SettingsEditableList";
import SettingsEditorActions from "./SettingsEditorActions";
import SettingsEditorModal from "./SettingsEditorModal";
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

  return (
    <div class="settings-section">
      <div class="settings-group">
        <div class="settings-group-title">スキャンパス</div>
        <div class="settings-group-content">
          <SettingsEditableList
            hasItems={d().paths.scan.length > 0}
            emptyMessage="スキャンパスはまだ登録されていません"
            onAdd={openCreateModal}
          >
            <For each={d().paths.scan}>
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
          </SettingsEditableList>
        </div>
      </div>

      <SettingsEditorModal
        open={modalMode() !== null}
        title={modalMode() === "edit" ? "スキャンパスを編集" : "スキャンパスを追加"}
        titleId="scan-path-modal-title"
        onClose={closeModal}
        initialFocusEl={() => pathInputRef}
      >
        <div class="settings-editor-form">
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
            <span class="settings-editor-form-hint">同じパスは自動的に統合されます</span>
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
          <div class="settings-editor-form-toggle">
            <ToggleSwitch
              checked={editIncludeFolders()}
              onChange={(v) => setEditIncludeFolders(v)}
            />
            <span>フォルダを含める</span>
          </div>
          <SettingsEditorActions
            left={
              modalMode() === "edit" ? (
                <button type="button" class="btn-danger" onClick={removeScanPath}>
                  削除
                </button>
              ) : undefined
            }
            right={
              <>
                <button type="button" onClick={closeModal}>
                  キャンセル
                </button>
                <button type="button" class="btn-primary" onClick={saveScanPath}>
                  保存
                </button>
              </>
            }
          />
        </div>
      </SettingsEditorModal>
    </div>
  );
};

export default SettingsIndex;
