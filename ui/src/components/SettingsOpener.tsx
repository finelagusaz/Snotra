import type { Component } from "solid-js";
import { createSignal, createMemo, For, Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import type { OpenerRule } from "../lib/types";
import type { GroupedOpenerEntry } from "../lib/openerGroups";
import {
  buildGroupedOpeners,
  cloneGroupedOpenerEntry,
  isSameGroupedOpener,
  mergeGroupedOpenerEntries,
  serializeGroupedOpeners,
} from "../lib/openerGroups";
import { normalizeOpenerTarget } from "../lib/openerTarget";
import { draft, updateDraft } from "../stores/settings";
import { t } from "../lib/i18n";
import SettingsEditableList from "./SettingsEditableList";
import SettingsEditorActions from "./SettingsEditorActions";
import SettingsEditorModal from "./SettingsEditorModal";

type FlatEntry = GroupedOpenerEntry & {
  targetLabel: string;
  toolName: string;
};

function buildFlatList(openers: OpenerRule[]): FlatEntry[] {
  return buildGroupedOpeners(openers).map((entry) => ({
    ...entry,
    targetLabel: entry.targetKind === "folder" ? t("settings.opener.target.folder") : entry.extensions.join(","),
    toolName: entry.tool.name || t("settings.opener.tool_name.unset"),
  }));
}

const SettingsOpener: Component = () => {
  const d = () => draft()!;

  const [modalMode, setModalMode] = createSignal<"create" | "edit" | null>(null);
  const [editingFlat, setEditingFlat] = createSignal<number | null>(null);

  const [editTarget, setEditTarget] = createSignal("folder");
  const [editTargetExt, setEditTargetExt] = createSignal("");
  const [editToolName, setEditToolName] = createSignal("");
  const [editToolExe, setEditToolExe] = createSignal("");
  const [editToolArgs, setEditToolArgs] = createSignal("");
  let toolNameInputRef: HTMLInputElement | undefined;

  const flatList = createMemo(() => buildFlatList(d().openers));

  function resetForm() {
    setEditTarget("folder");
    setEditTargetExt("");
    setEditToolName("");
    setEditToolExe("");
    setEditToolArgs("");
  }

  function closeModal() {
    setModalMode(null);
    setEditingFlat(null);
    resetForm();
  }

  function openCreateModal() {
    setEditingFlat(null);
    resetForm();
    setModalMode("create");
  }

  function openEditModal(flatIndex: number) {
    const entry = flatList()[flatIndex];
    if (!entry) {
      return;
    }
    setEditingFlat(flatIndex);
    if (entry.targetKind === "folder") {
      setEditTarget("folder");
      setEditTargetExt("");
    } else {
      setEditTarget("ext");
      setEditTargetExt(entry.extensions.join(","));
    }
    setEditToolName(entry.tool.name);
    setEditToolExe(entry.tool.exe);
    setEditToolArgs(entry.tool.args);
    setModalMode("edit");
  }

  function buildTarget(): string {
    return editTarget() === "folder"
      ? "folder"
      : normalizeOpenerTarget(`ext:${editTargetExt()}`);
  }

  function buildEntryFromForm(): GroupedOpenerEntry {
    const target = buildTarget();
    return {
      targetKind: target === "folder" ? "folder" : "ext",
      extensions:
        target === "folder"
          ? []
          : target
              .slice(4)
              .split(",")
              .filter((ext) => ext.length > 0),
      tool: {
        name: editToolName(),
        exe: editToolExe(),
        args: editToolArgs(),
      },
    };
  }

  function saveEntry() {
    const entries = flatList().map((entry) =>
      cloneGroupedOpenerEntry({
        targetKind: entry.targetKind,
        extensions: entry.extensions,
        tool: entry.tool,
      }),
    );
    const nextEntry = buildEntryFromForm();

    if (modalMode() === "edit") {
      const fi = editingFlat();
      if (fi === null) return;
      entries.splice(fi, 1);
      const mergeIndex = entries.findIndex((entry) => isSameGroupedOpener(entry, nextEntry));
      if (mergeIndex >= 0) {
        entries[mergeIndex] = mergeGroupedOpenerEntries(entries[mergeIndex], nextEntry);
      } else {
        entries.splice(Math.min(fi, entries.length), 0, nextEntry);
      }
    } else {
      const mergeIndex = entries.findIndex((entry) => isSameGroupedOpener(entry, nextEntry));
      if (mergeIndex >= 0) {
        entries[mergeIndex] = mergeGroupedOpenerEntries(entries[mergeIndex], nextEntry);
      } else {
        entries.push(nextEntry);
      }
    }

    updateDraft((c) => {
      c.openers = serializeGroupedOpeners(entries);
    });
    closeModal();
  }

  function deleteEntry() {
    const fi = editingFlat();
    if (fi === null) return;
    const entries = flatList().map((entry) =>
      cloneGroupedOpenerEntry({
        targetKind: entry.targetKind,
        extensions: entry.extensions,
        tool: entry.tool,
      }),
    );
    if (!entries[fi]) return;
    entries.splice(fi, 1);
    updateDraft((c) => {
      c.openers = serializeGroupedOpeners(entries);
    });
    closeModal();
  }

  function moveUpAt(fi: number) {
    if (fi <= 0) return;
    const entries = flatList().map((entry) =>
      cloneGroupedOpenerEntry({
        targetKind: entry.targetKind,
        extensions: entry.extensions,
        tool: entry.tool,
      }),
    );
    updateDraft((c) => {
      [entries[fi - 1], entries[fi]] = [entries[fi], entries[fi - 1]];
      c.openers = serializeGroupedOpeners(entries);
    });
    if (editingFlat() === fi) {
      setEditingFlat(fi - 1);
    }
  }

  function moveDownAt(fi: number) {
    const entries = flatList().map((entry) =>
      cloneGroupedOpenerEntry({
        targetKind: entry.targetKind,
        extensions: entry.extensions,
        tool: entry.tool,
      }),
    );
    if (fi < 0 || fi >= entries.length - 1) return;
    updateDraft((c) => {
      [entries[fi], entries[fi + 1]] = [entries[fi + 1], entries[fi]];
      c.openers = serializeGroupedOpeners(entries);
    });
    if (editingFlat() === fi) {
      setEditingFlat(fi + 1);
    }
  }

  async function browseExe() {
    const selected = await open({
      directory: false,
      multiple: false,
      filters: [{ name: "実行ファイル", extensions: ["exe", "bat", "cmd"] }],
      defaultPath: editToolExe() || undefined,
    });
    if (selected !== null) {
      setEditToolExe(selected as string);
    }
  }

  function canMoveUpAt(fi: number): boolean {
    return fi > 0;
  }

  function canMoveDownAt(fi: number): boolean {
    return fi < flatList().length - 1;
  }

  return (
    <div class="settings-section">
      <div class="settings-group">
        <div class="settings-group-title">{t("settings.opener.group.rules")}</div>
        <div class="settings-group-content">
          <p style={{ "font-size": "0.85em", color: "var(--hint-text-color)", margin: "0 0 8px" }}>
            {t("settings.opener.description")}
          </p>

          <SettingsEditableList
            hasItems={flatList().length > 0}
            emptyMessage={t("settings.opener.empty")}
            onAdd={openCreateModal}
          >
            <For each={flatList()}>
              {(entry, fi) => (
                <div class="scan-path-item opener-item scan-path-item--editable">
                  <div class="opener-item-main">
                    <span class="opener-item-badge">{entry.targetLabel}</span>
                    <span class="opener-item-name">{entry.toolName}</span>
                  </div>
                  <div class="scan-path-item-actions">
                    <button
                      type="button"
                      class="scan-path-edit-button"
                      onClick={() => openEditModal(fi())}
                    >
                      {t("settings.opener.edit")}
                    </button>
                    <button
                      type="button"
                      onClick={() => moveUpAt(fi())}
                      disabled={!canMoveUpAt(fi())}
                      title={t("settings.opener.move_up")}
                    >
                      ↑
                    </button>
                    <button
                      type="button"
                      onClick={() => moveDownAt(fi())}
                      disabled={!canMoveDownAt(fi())}
                      title={t("settings.opener.move_down")}
                    >
                      ↓
                    </button>
                  </div>
                </div>
              )}
            </For>
          </SettingsEditableList>
        </div>
      </div>

      <SettingsEditorModal
        open={modalMode() !== null}
        title={modalMode() === "edit" ? t("settings.opener.modal.edit_title") : t("settings.opener.modal.add_title")}
        titleId="opener-modal-title"
        onClose={closeModal}
        initialFocusEl={() => toolNameInputRef}
      >
        <div class="settings-editor-form">
          <label>
            {t("settings.opener.target.label")}
            <div style={{ display: "flex", gap: "8px", "align-items": "center" }}>
              <select
                value={editTarget()}
                onChange={(e) => setEditTarget(e.currentTarget.value)}
                style={{ flex: "0 0 auto" }}
              >
                <option value="folder">{t("settings.opener.target.folder")}</option>
                <option value="ext">{t("settings.opener.target.ext")}</option>
              </select>
              <Show when={editTarget() === "ext"}>
                <input
                  type="text"
                  value={editTargetExt()}
                  onInput={(e) => setEditTargetExt(e.currentTarget.value)}
                  placeholder=".png,.jpg,.gif"
                  style={{ flex: "1" }}
                />
              </Show>
            </div>
          </label>
          <label>
            {t("settings.opener.tool_name.label")}
            <input
              ref={toolNameInputRef}
              type="text"
              value={editToolName()}
              onInput={(e) => setEditToolName(e.currentTarget.value)}
              placeholder="Total Commander"
            />
          </label>
          <label>
            {t("settings.opener.exe.label")}
            <div class="scan-path-input-row">
              <input
                type="text"
                value={editToolExe()}
                onInput={(e) => setEditToolExe(e.currentTarget.value)}
                placeholder="C:\tools\app.exe"
              />
              <button type="button" class="btn-browse" onClick={browseExe}>
                {t("settings.opener.browse")}
              </button>
            </div>
          </label>
          <label>
            {t("settings.opener.args.label")}
            <input
              type="text"
              value={editToolArgs()}
              onInput={(e) => setEditToolArgs(e.currentTarget.value)}
              placeholder='-d {path}'
            />
            <span class="settings-editor-form-hint">
              {t("settings.opener.args.hint")}
            </span>
          </label>
          <SettingsEditorActions
            left={
              modalMode() === "edit" ? (
                <button type="button" class="btn-danger" onClick={deleteEntry}>
                  {t("settings.opener.delete")}
                </button>
              ) : undefined
            }
            right={
              <>
                <button type="button" onClick={closeModal}>
                  {t("settings.opener.cancel")}
                </button>
                <button type="button" class="btn-primary" onClick={saveEntry}>
                  {t("settings.opener.save")}
                </button>
              </>
            }
          />
        </div>
      </SettingsEditorModal>
    </div>
  );
};

export default SettingsOpener;
