import type { Component } from "solid-js";
import { createSignal, For, Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import { draft, updateDraft, setStatus } from "../stores/settings";
import { t } from "../lib/i18n";
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
        setStatus(t("settings.index.merged_duplicate"));
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
      setStatus(t("settings.index.merged_existing"));
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
        <div class="settings-group-title">{t("settings.index.group.scan")}</div>
        <div class="settings-group-content">
          <SettingsEditableList
            hasItems={d().paths.scan.length > 0}
            emptyMessage={t("settings.index.empty")}
            onAdd={openCreateModal}
          >
            <For each={d().paths.scan}>
              {(scan, idx) => (
                <div class="scan-path-item scan-path-item--editable">
                  <div class="scan-path-item-main">
                    <div class="scan-path-item-path">{scan.path || t("settings.index.path.unset")}</div>
                    <div class="scan-path-item-meta">
                      <span class="scan-path-item-exts">
                        {formatExtensions(scan.extensions) || t("settings.index.extensions.unset")}
                      </span>
                      <Show when={scan.include_folders}>
                        <span class="scan-path-item-folder-badge" title={t("settings.index.include_folders.badge")}>&#x1F4C1;</span>
                      </Show>
                    </div>
                  </div>
                  <button
                    type="button"
                    class="scan-path-edit-button"
                    onClick={() => openEditModal(idx())}
                  >
                    {t("settings.index.edit")}
                  </button>
                </div>
              )}
            </For>
          </SettingsEditableList>
        </div>
      </div>

      <SettingsEditorModal
        open={modalMode() !== null}
        title={modalMode() === "edit" ? t("settings.index.modal.edit_title") : t("settings.index.modal.add_title")}
        titleId="scan-path-modal-title"
        onClose={closeModal}
        initialFocusEl={() => pathInputRef}
      >
        <div class="settings-editor-form">
          <label>
            {t("settings.index.path.label")}
            <div class="scan-path-input-row">
              <input
                ref={pathInputRef}
                type="text"
                value={editPath()}
                onInput={(e) => setEditPath(e.currentTarget.value)}
                placeholder="C:\..."
              />
              <button type="button" class="btn-browse" onClick={browsePath}>
                {t("settings.index.browse")}
              </button>
            </div>
            <span class="settings-editor-form-hint">{t("settings.index.path.hint")}</span>
          </label>
          <label>
            {t("settings.index.extensions.label")}
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
            <span>{t("settings.index.include_folders.label")}</span>
          </div>
          <SettingsEditorActions
            left={
              modalMode() === "edit" ? (
                <button type="button" class="btn-danger" onClick={removeScanPath}>
                  {t("settings.index.delete")}
                </button>
              ) : undefined
            }
            right={
              <>
                <button type="button" onClick={closeModal}>
                  {t("settings.index.cancel")}
                </button>
                <button type="button" class="btn-primary" onClick={saveScanPath}>
                  {t("settings.index.save")}
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
