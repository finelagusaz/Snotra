import type { Component } from "solid-js";
import { createSignal, For, Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import type { OpenerRule, OpenerTool } from "../lib/types";
import { draft, updateDraft } from "../stores/settings";
import SettingsEditableList from "./SettingsEditableList";
import SettingsEditorActions from "./SettingsEditorActions";
import SettingsEditorModal from "./SettingsEditorModal";

type FlatEntry = {
  ruleIndex: number;
  toolIndex: number;
  targetLabel: string;
  toolName: string;
};

function buildFlatList(openers: OpenerRule[]): FlatEntry[] {
  const entries: FlatEntry[] = [];
  for (let ri = 0; ri < openers.length; ri++) {
    const rule = openers[ri];
    const targetLabel =
      rule.target === "folder"
        ? "フォルダ"
        : rule.target.startsWith("ext:")
          ? rule.target.slice(4)
          : rule.target;
    for (let ti = 0; ti < rule.tools.length; ti++) {
      entries.push({
        ruleIndex: ri,
        toolIndex: ti,
        targetLabel,
        toolName: rule.tools[ti].name || "(名前未設定)",
      });
    }
  }
  return entries;
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

  const flatList = () => buildFlatList(d().openers);

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
    const rule = d().openers[entry.ruleIndex];
    const tool = rule?.tools[entry.toolIndex];
    if (!rule || !tool) {
      return;
    }
    setEditingFlat(flatIndex);
    if (rule.target === "folder") {
      setEditTarget("folder");
      setEditTargetExt("");
    } else {
      setEditTarget("ext");
      setEditTargetExt(rule.target.startsWith("ext:") ? rule.target.slice(4) : rule.target);
    }
    setEditToolName(tool.name);
    setEditToolExe(tool.exe);
    setEditToolArgs(tool.args);
    setModalMode("edit");
  }

  function buildTarget(): string {
    return editTarget() === "folder" ? "folder" : `ext:${editTargetExt()}`;
  }

  function saveEntry() {
    const target = buildTarget();
    const tool: OpenerTool = {
      name: editToolName(),
      exe: editToolExe(),
      args: editToolArgs(),
    };

    if (modalMode() === "edit") {
      const fi = editingFlat();
      if (fi === null) return;
      const entry = flatList()[fi];
      if (!entry) return;

      updateDraft((c) => {
        const oldRule = c.openers[entry.ruleIndex];
        if (oldRule.target === target) {
          oldRule.tools[entry.toolIndex] = tool;
          return;
        }

        oldRule.tools.splice(entry.toolIndex, 1);
        if (oldRule.tools.length === 0) {
          c.openers.splice(entry.ruleIndex, 1);
        }
        const newRuleIdx = c.openers.findIndex((r) => r.target === target);
        if (newRuleIdx >= 0) {
          c.openers[newRuleIdx].tools.push(tool);
        } else {
          c.openers.push({ target, tools: [tool] });
        }
      });
      closeModal();
      return;
    }

    updateDraft((c) => {
      const existingRuleIdx = c.openers.findIndex((r) => r.target === target);
      if (existingRuleIdx >= 0) {
        c.openers[existingRuleIdx].tools.push(tool);
      } else {
        c.openers.push({ target, tools: [tool] });
      }
    });
    closeModal();
  }

  function deleteEntry() {
    const fi = editingFlat();
    if (fi === null) return;
    const entry = flatList()[fi];
    if (!entry) return;
    updateDraft((c) => {
      c.openers[entry.ruleIndex].tools.splice(entry.toolIndex, 1);
      if (c.openers[entry.ruleIndex].tools.length === 0) {
        c.openers.splice(entry.ruleIndex, 1);
      }
    });
    closeModal();
  }

  function moveUpAt(fi: number) {
    const entry = flatList()[fi];
    if (!entry || entry.toolIndex === 0) return;
    const { ruleIndex: ri, toolIndex: ti } = entry;
    updateDraft((c) => {
      const tools = c.openers[ri].tools;
      [tools[ti - 1], tools[ti]] = [tools[ti], tools[ti - 1]];
    });
    if (editingFlat() === fi) {
      setEditingFlat(fi - 1);
    }
  }

  function moveDownAt(fi: number) {
    const entry = flatList()[fi];
    if (!entry) return;
    const { ruleIndex: ri, toolIndex: ti } = entry;
    const toolCount = d().openers[ri]?.tools.length ?? 0;
    if (ti >= toolCount - 1) return;
    updateDraft((c) => {
      const tools = c.openers[ri].tools;
      [tools[ti], tools[ti + 1]] = [tools[ti + 1], tools[ti]];
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
    const entry = flatList()[fi] ?? null;
    return entry !== null && entry.toolIndex > 0;
  }

  function canMoveDownAt(fi: number): boolean {
    const entry = flatList()[fi] ?? null;
    if (!entry) return false;
    return entry.toolIndex < (d().openers[entry.ruleIndex]?.tools.length ?? 0) - 1;
  }

  return (
    <div class="settings-section">
      <div class="settings-group">
        <div class="settings-group-title">オープナールール</div>
        <div class="settings-group-content">
          <p style={{ "font-size": "0.85em", color: "var(--hint-text-color)", margin: "0 0 8px" }}>
            ファイル/フォルダを開く際に使用するツールを設定します。
            Enter で先頭ツールを起動、Shift+Enter でツール選択メニューを表示します。
          </p>

          <SettingsEditableList
            hasItems={flatList().length > 0}
            emptyMessage="オープナールールはまだ登録されていません"
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
                      編集
                    </button>
                    <button
                      type="button"
                      onClick={() => moveUpAt(fi())}
                      disabled={!canMoveUpAt(fi())}
                      title="上へ"
                    >
                      ↑
                    </button>
                    <button
                      type="button"
                      onClick={() => moveDownAt(fi())}
                      disabled={!canMoveDownAt(fi())}
                      title="下へ"
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
        title={modalMode() === "edit" ? "オープナールールを編集" : "オープナールールを追加"}
        titleId="opener-modal-title"
        onClose={closeModal}
        initialFocusEl={() => toolNameInputRef}
      >
        <div class="settings-editor-form">
          <label>
            対象
            <div style={{ display: "flex", gap: "8px", "align-items": "center" }}>
              <select
                value={editTarget()}
                onChange={(e) => setEditTarget(e.currentTarget.value)}
                style={{ flex: "0 0 auto" }}
              >
                <option value="folder">フォルダ</option>
                <option value="ext">拡張子</option>
              </select>
              <Show when={editTarget() === "ext"}>
                <input
                  type="text"
                  value={editTargetExt()}
                  onInput={(e) => setEditTargetExt(e.currentTarget.value)}
                  placeholder="png,jpg,gif"
                  style={{ flex: "1" }}
                />
              </Show>
            </div>
          </label>
          <label>
            名前
            <input
              ref={toolNameInputRef}
              type="text"
              value={editToolName()}
              onInput={(e) => setEditToolName(e.currentTarget.value)}
              placeholder="Total Commander"
            />
          </label>
          <label>
            実行ファイル
            <div class="scan-path-input-row">
              <input
                type="text"
                value={editToolExe()}
                onInput={(e) => setEditToolExe(e.currentTarget.value)}
                placeholder="C:\tools\app.exe"
              />
              <button type="button" class="btn-browse" onClick={browseExe}>
                参照...
              </button>
            </div>
          </label>
          <label>
            引数 (オプション)
            <input
              type="text"
              value={editToolArgs()}
              onInput={(e) => setEditToolArgs(e.currentTarget.value)}
              placeholder="/O /T"
            />
          </label>
          <SettingsEditorActions
            left={
              modalMode() === "edit" ? (
                <button type="button" class="btn-danger" onClick={deleteEntry}>
                  削除
                </button>
              ) : undefined
            }
            right={
              <>
                <button type="button" onClick={closeModal}>
                  キャンセル
                </button>
                <button type="button" class="btn-primary" onClick={saveEntry}>
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

export default SettingsOpener;
