import type { Component } from "solid-js";
import { draft, updateDraft } from "../stores/settings";

const SettingsSearch: Component = () => {
  const d = () => draft()!;

  return (
    <div class="settings-section">
      <label>
        通常検索モード
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
      </label>
      <label>
        フォルダ内検索モード
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
      </label>
      <label class="checkbox">
        <input
          type="checkbox"
          checked={d().search.show_hidden_system}
          onChange={(e) =>
            updateDraft((c) => {
              c.search.show_hidden_system = e.currentTarget.checked;
            })
          }
        />
        隠しファイル・システムファイルを表示
      </label>
    </div>
  );
};

export default SettingsSearch;
