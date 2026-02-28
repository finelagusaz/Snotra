import type { Component } from "solid-js";
import { createEffect, createSignal, For, Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import type { OpenerRule, OpenerTool } from "../lib/types";
import { draft, updateDraft } from "../stores/settings";

type FlatEntry = {
  ruleIndex: number;
  toolIndex: number;
  targetLabel: string;
  toolName: string;
  toolExe: string;
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
        toolExe: rule.tools[ti].exe,
      });
    }
  }
  return entries;
}

const SettingsOpener: Component = () => {
  const d = () => draft()!;

  const [selectedFlat, setSelectedFlat] = createSignal<number | null>(null);

  const [editTarget, setEditTarget] = createSignal("folder");
  const [editTargetExt, setEditTargetExt] = createSignal("");
  const [editToolName, setEditToolName] = createSignal("");
  const [editToolExe, setEditToolExe] = createSignal("");
  const [editToolArgs, setEditToolArgs] = createSignal("");

  const flatList = () => buildFlatList(d().openers);

  // 選択変更時にフォームを同期
  createEffect(() => {
    const fi = selectedFlat();
    if (fi === null) {
      setEditTarget("folder");
      setEditTargetExt("");
      setEditToolName("");
      setEditToolExe("");
      setEditToolArgs("");
      return;
    }
    const entry = flatList()[fi];
    if (!entry) {
      setSelectedFlat(null);
      return;
    }
    const rule = d().openers[entry.ruleIndex];
    const tool = rule?.tools[entry.toolIndex];
    if (!rule || !tool) {
      setSelectedFlat(null);
      return;
    }
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
  });

  function buildTarget(): string {
    return editTarget() === "folder" ? "folder" : `ext:${editTargetExt()}`;
  }

  // フラットリスト内でルールのツール数を合計し、指定ルールの先頭インデックスを返す
  function flatOffset(openers: OpenerRule[], ruleIndex: number): number {
    let offset = 0;
    for (let ri = 0; ri < ruleIndex; ri++) {
      offset += openers[ri].tools.length;
    }
    return offset;
  }

  function addEntry() {
    const target = buildTarget();
    const tool: OpenerTool = {
      name: editToolName(),
      exe: editToolExe(),
      args: editToolArgs(),
    };
    let newFlatIndex = 0;
    updateDraft((c) => {
      const existingRuleIdx = c.openers.findIndex((r) => r.target === target);
      if (existingRuleIdx >= 0) {
        c.openers[existingRuleIdx].tools.push(tool);
        newFlatIndex = flatOffset(c.openers, existingRuleIdx) + c.openers[existingRuleIdx].tools.length - 1;
      } else {
        c.openers.push({ target, tools: [tool] });
        newFlatIndex = flatOffset(c.openers, c.openers.length - 1);
      }
    });
    setSelectedFlat(newFlatIndex);
  }

  function updateEntry() {
    const fi = selectedFlat();
    if (fi === null) return;
    const entry = flatList()[fi];
    if (!entry) return;

    const newTarget = buildTarget();
    const newTool: OpenerTool = {
      name: editToolName(),
      exe: editToolExe(),
      args: editToolArgs(),
    };

    let newFlatIndex = fi;
    updateDraft((c) => {
      const oldRule = c.openers[entry.ruleIndex];
      if (oldRule.target === newTarget) {
        // 同じ対象: ツール情報のみ更新
        oldRule.tools[entry.toolIndex] = newTool;
      } else {
        // 対象変更: 旧ルールから削除 → 新ルールに追加
        oldRule.tools.splice(entry.toolIndex, 1);
        if (oldRule.tools.length === 0) {
          c.openers.splice(entry.ruleIndex, 1);
        }
        const newRuleIdx = c.openers.findIndex((r) => r.target === newTarget);
        if (newRuleIdx >= 0) {
          c.openers[newRuleIdx].tools.push(newTool);
          newFlatIndex = flatOffset(c.openers, newRuleIdx) + c.openers[newRuleIdx].tools.length - 1;
        } else {
          c.openers.push({ target: newTarget, tools: [newTool] });
          newFlatIndex = flatOffset(c.openers, c.openers.length - 1);
        }
      }
    });
    setSelectedFlat(newFlatIndex);
  }

  function deleteEntry() {
    const fi = selectedFlat();
    if (fi === null) return;
    const entry = flatList()[fi];
    if (!entry) return;
    updateDraft((c) => {
      c.openers[entry.ruleIndex].tools.splice(entry.toolIndex, 1);
      if (c.openers[entry.ruleIndex].tools.length === 0) {
        c.openers.splice(entry.ruleIndex, 1);
      }
    });
    setSelectedFlat(null);
  }

  function moveUp() {
    const fi = selectedFlat();
    if (fi === null) return;
    const entry = flatList()[fi];
    if (!entry || entry.toolIndex === 0) return;
    const { ruleIndex: ri, toolIndex: ti } = entry;
    updateDraft((c) => {
      const tools = c.openers[ri].tools;
      [tools[ti - 1], tools[ti]] = [tools[ti], tools[ti - 1]];
    });
    setSelectedFlat(fi - 1);
  }

  function moveDown() {
    const fi = selectedFlat();
    if (fi === null) return;
    const entry = flatList()[fi];
    if (!entry) return;
    const { ruleIndex: ri, toolIndex: ti } = entry;
    const toolCount = d().openers[ri]?.tools.length ?? 0;
    if (ti >= toolCount - 1) return;
    updateDraft((c) => {
      const tools = c.openers[ri].tools;
      [tools[ti], tools[ti + 1]] = [tools[ti + 1], tools[ti]];
    });
    setSelectedFlat(fi + 1);
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

  const currentEntry = () => {
    const fi = selectedFlat();
    return fi !== null ? (flatList()[fi] ?? null) : null;
  };

  const canMoveUp = () => {
    const entry = currentEntry();
    return entry !== null && entry.toolIndex > 0;
  };

  const canMoveDown = () => {
    const entry = currentEntry();
    if (!entry) return false;
    return entry.toolIndex < (d().openers[entry.ruleIndex]?.tools.length ?? 0) - 1;
  };

  return (
    <div class="settings-section">
      <div class="settings-group">
        <div class="settings-group-title">オープナールール</div>
        <div class="settings-group-content">
          <p style={{ "font-size": "0.85em", color: "var(--hint-text-color)", margin: "0 0 8px" }}>
            ファイル/フォルダを開く際に使用するツールを設定します。
            Enter で先頭ツールを起動、Shift+Enter でツール選択メニューを表示します。
          </p>

          {/* フラットリスト */}
          <div class="scan-path-list">
            <For each={flatList()}>
              {(entry, fi) => (
                <div
                  class="scan-path-item opener-item"
                  classList={{ selected: selectedFlat() === fi() }}
                  onClick={() => setSelectedFlat(fi())}
                >
                  <span class="opener-item-badge">{entry.targetLabel}</span>
                  <span class="opener-item-name">{entry.toolName}</span>
                </div>
              )}
            </For>
          </div>

          {/* 統合編集フォーム */}
          <div class="scan-path-form">
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
            <div class="scan-path-form-actions">
              <Show
                when={selectedFlat() !== null}
                fallback={<button onClick={addEntry}>追加</button>}
              >
                <button onClick={updateEntry}>更新</button>
                <button class="btn-danger" onClick={deleteEntry}>
                  削除
                </button>
                <button onClick={moveUp} disabled={!canMoveUp()} title="上へ">
                  ↑
                </button>
                <button onClick={moveDown} disabled={!canMoveDown()} title="下へ">
                  ↓
                </button>
                <button style={{ "margin-left": "auto" }} onClick={() => setSelectedFlat(null)}>
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

export default SettingsOpener;
