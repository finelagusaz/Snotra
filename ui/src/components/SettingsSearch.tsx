import type { Component } from "solid-js";
import { draft, updateDraft } from "../stores/settings";
import SettingRow from "./SettingRow";
import ToggleSwitch from "./ToggleSwitch";

const SettingsSearch: Component = () => {
  const d = () => draft()!;

  return (
    <div class="settings-section">
      <div class="settings-group">
        <div class="settings-group-title">検索方式</div>
        <div class="settings-group-content">
          <SettingRow
            label="通常検索モード"
            description="通常の検索時に使用するマッチング方式"
          >
            <select
              value={d().search.normal_mode}
              onChange={(e) =>
                updateDraft((c) => {
                  c.search.normal_mode = e.currentTarget.value;
                })
              }
            >
              <option value="prefix">前方一致</option>
              <option value="substring">部分一致</option>
              <option value="fuzzy">あいまい</option>
            </select>
          </SettingRow>
          <SettingRow
            label="フォルダ内検索モード"
            description="フォルダ内ファイルの検索時に使用するマッチング方式"
          >
            <select
              value={d().search.folder_mode}
              onChange={(e) =>
                updateDraft((c) => {
                  c.search.folder_mode = e.currentTarget.value;
                })
              }
            >
              <option value="prefix">前方一致</option>
              <option value="substring">部分一致</option>
              <option value="fuzzy">あいまい</option>
            </select>
          </SettingRow>
        </div>
      </div>

      <div class="settings-group">
        <div class="settings-group-title">表示</div>
        <div class="settings-group-content">
          <SettingRow
            label="隠しファイルを表示"
            description="Windowsの隠しファイルやシステムファイルを結果に含めます"
          >
            <ToggleSwitch
              checked={d().search.show_hidden_system}
              onChange={(v) =>
                updateDraft((c) => {
                  c.search.show_hidden_system = v;
                })
              }
            />
          </SettingRow>
        </div>
      </div>

      <div class="settings-group">
        <div class="settings-group-title">履歴スコア</div>
        <div class="settings-group-content">
          <SettingRow
            label="履歴正規化"
            description="履歴の使用回数をスコアに反映する方式"
          >
            <select
              value={d().search.history_normalization}
              onChange={(e) =>
                updateDraft((c) => {
                  c.search.history_normalization = e.currentTarget.value;
                })
              }
            >
              <option value="disabled">無効</option>
              <option value="fuzzy_relative_cap">上限付き相対</option>
            </select>
          </SettingRow>
          <SettingRow
            label="履歴スコア上限比率"
            description="スコアに加算できる履歴ボーナスの上限（0〜1、正規化が有効の場合のみ）"
          >
            <input
              type="number"
              min="0"
              max="1"
              step="0.05"
              value={d().search.fuzzy_history_cap_ratio}
              disabled={d().search.history_normalization === "disabled"}
              onInput={(e) => {
                const val = parseFloat(e.currentTarget.value);
                if (!Number.isNaN(val)) {
                  updateDraft((c) => {
                    c.search.fuzzy_history_cap_ratio = Math.max(0, Math.min(1, val));
                  });
                }
              }}
            />
          </SettingRow>
        </div>
      </div>
    </div>
  );
};

export default SettingsSearch;
