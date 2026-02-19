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
    </div>
  );
};

export default SettingsSearch;
