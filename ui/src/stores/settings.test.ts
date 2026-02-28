import { beforeEach, describe, expect, it, vi } from "vitest";

// ── Tauri 依存をモック ────────────────────────────────────────────────────────

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({ close: vi.fn(async () => {}) })),
}));

vi.mock("../lib/invoke", () => ({
  getConfig: vi.fn(async () => ({})),
  saveConfig: vi.fn(async () => ({ reindex_started: false })),
}));

// ── モック確立後にインポート ─────────────────────────────────────────────────

import * as api from "../lib/invoke";
import type { Config } from "../lib/types";
import {
  draft,
  hasChanges,
  canSave,
  loadDraft,
  updateDraft,
  saveDraft,
} from "../stores/settings";

// ── テスト用ベース Config ────────────────────────────────────────────────────

function makeConfig(overrides: Partial<Config> = {}): Config {
  return {
    hotkey: { modifier: "Alt", key: "Q" },
    general: {
      hotkey_toggle: true,
      show_on_startup: false,
      auto_hide_on_focus_lost: true,
      show_tray_icon: false,
      ime_off_on_show: false,
    },
    appearance: {
      max_results: 8,
      window_width: 600,
      top_n_history: 200,
      max_history_display: 8,
      show_icons: false,
    },
    visual: {
      preset: "obsidian",
      background_color: "#282828",
      input_background_color: "#383838",
      text_color: "#e0e0e0",
      selected_row_color: "#505050",
      hint_text_color: "#808080",
      font_family: "Segoe UI",
      font_size: 15,
    },
    paths: { scan: [] },
    search: {
      normal_mode: "fuzzy",
      folder_mode: "fuzzy",
      show_hidden_system: false,
      history_normalization: "disabled",
      fuzzy_history_cap_ratio: 0.3,
    },
    openers: [],
    ...overrides,
  };
}

// ── セットアップ ──────────────────────────────────────────────────────────────

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.getConfig).mockResolvedValue(makeConfig());
  vi.mocked(api.saveConfig).mockResolvedValue({ reindex_started: false });
});

// ── loadDraft ─────────────────────────────────────────────────────────────────

describe("loadDraft", () => {
  it("getConfig の結果で draft が初期化される", async () => {
    await loadDraft();

    expect(draft()).toEqual(makeConfig());
  });

  it("loadDraft 直後は hasChanges が false", async () => {
    await loadDraft();

    expect(hasChanges()).toBe(false);
  });

  it("loadDraft 直後は canSave が false（変更なし）", async () => {
    await loadDraft();

    expect(canSave()).toBe(false);
  });
});

// ── updateDraft ───────────────────────────────────────────────────────────────

describe("updateDraft + hasChanges", () => {
  it("オープナールールを追加すると hasChanges が true になる", async () => {
    await loadDraft();

    updateDraft((c) => {
      c.openers.push({ target: "folder", tools: [] });
    });

    expect(hasChanges()).toBe(true);
    expect(draft()!.openers).toHaveLength(1);
  });

  it("オープナーツールを追加すると draft に反映される", async () => {
    await loadDraft();
    updateDraft((c) => {
      c.openers.push({
        target: "folder",
        tools: [{ name: "My Tool", exe: "C:\\tool.exe", args: "/arg" }],
      });
    });

    const opener = draft()!.openers[0];
    expect(opener.target).toBe("folder");
    expect(opener.tools[0].name).toBe("My Tool");
    expect(opener.tools[0].exe).toBe("C:\\tool.exe");
    expect(opener.tools[0].args).toBe("/arg");
  });

  it("ルールを削除すると hasChanges が true になる", async () => {
    vi.mocked(api.getConfig).mockResolvedValue(
      makeConfig({ openers: [{ target: "folder", tools: [] }] }),
    );
    await loadDraft();

    updateDraft((c) => {
      c.openers.splice(0, 1);
    });

    expect(hasChanges()).toBe(true);
    expect(draft()!.openers).toHaveLength(0);
  });

  it("updateDraft は draft を structuredClone で更新する（元オブジェクトを変更しない）", async () => {
    await loadDraft();
    const before = draft()!;

    updateDraft((c) => {
      c.openers.push({ target: "folder", tools: [] });
    });

    // 変更前の参照は変わっていない
    expect(before.openers).toHaveLength(0);
    // draft() は新しい参照になっている
    expect(draft()).not.toBe(before);
  });
});

// ── canSave ───────────────────────────────────────────────────────────────────

describe("canSave", () => {
  it("変更あり + ホットキー有効 → true", async () => {
    await loadDraft(); // Alt+Q は有効
    updateDraft((c) => {
      c.openers.push({ target: "folder", tools: [] });
    });

    expect(canSave()).toBe(true);
  });

  it("変更なし → false（ホットキー有効でも）", async () => {
    await loadDraft();

    expect(canSave()).toBe(false);
  });

  it("modifier 空 → ホットキー無効のため変更あっても false", async () => {
    vi.mocked(api.getConfig).mockResolvedValue(
      makeConfig({ hotkey: { modifier: "", key: "Q" } }),
    );
    await loadDraft();
    updateDraft((c) => {
      c.openers.push({ target: "folder", tools: [] });
    });

    expect(hasChanges()).toBe(true);
    expect(canSave()).toBe(false);
  });

  it("key 空 → ホットキー無効のため変更あっても false", async () => {
    vi.mocked(api.getConfig).mockResolvedValue(
      makeConfig({ hotkey: { modifier: "Alt", key: "" } }),
    );
    await loadDraft();
    updateDraft((c) => {
      c.openers.push({ target: "folder", tools: [] });
    });

    expect(canSave()).toBe(false);
  });
});

// ── saveDraft ─────────────────────────────────────────────────────────────────

describe("saveDraft", () => {
  it("saveConfig が draft の内容で呼ばれる", async () => {
    await loadDraft();
    updateDraft((c) => {
      c.openers.push({ target: "folder", tools: [{ name: "T", exe: "t.exe", args: "" }] });
    });
    const expected = draft();

    await saveDraft();

    expect(api.saveConfig).toHaveBeenCalledWith(expected);
  });

  it("saveConfig 成功後 hasChanges が false に戻る", async () => {
    await loadDraft();
    updateDraft((c) => {
      c.openers.push({ target: "folder", tools: [] });
    });
    expect(hasChanges()).toBe(true);

    await saveDraft();

    expect(hasChanges()).toBe(false);
  });

  it("saveConfig 失敗時は hasChanges が true のまま", async () => {
    vi.mocked(api.saveConfig).mockRejectedValue(new Error("network error"));
    await loadDraft();
    updateDraft((c) => {
      c.openers.push({ target: "folder", tools: [] });
    });

    await saveDraft();

    expect(hasChanges()).toBe(true);
  });
});
