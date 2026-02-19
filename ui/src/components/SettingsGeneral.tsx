import type { Component } from "solid-js";
import { draft, updateDraft } from "../stores/settings";

const SettingsGeneral: Component = () => {
  const d = () => draft()!;

  return (
    <div class="settings-section">
      <label>
        ホットキー修飾キー
        <input
          type="text"
          value={d().hotkey.modifier}
          onInput={(e) =>
            updateDraft((c) => {
              c.hotkey.modifier = e.currentTarget.value;
            })
          }
        />
      </label>
      <label>
        ホットキーキー
        <input
          type="text"
          value={d().hotkey.key}
          onInput={(e) =>
            updateDraft((c) => {
              c.hotkey.key = e.currentTarget.value;
            })
          }
        />
      </label>
      <label class="checkbox">
        <input
          type="checkbox"
          checked={d().general.hotkey_toggle}
          onChange={(e) =>
            updateDraft((c) => {
              c.general.hotkey_toggle = e.currentTarget.checked;
            })
          }
        />
        呼び出しキーで表示/非表示トグル
      </label>
      <label class="checkbox">
        <input
          type="checkbox"
          checked={d().general.show_on_startup}
          onChange={(e) =>
            updateDraft((c) => {
              c.general.show_on_startup = e.currentTarget.checked;
            })
          }
        />
        起動時にウィンドウ表示
      </label>
      <label class="checkbox">
        <input
          type="checkbox"
          checked={d().general.auto_hide_on_focus_lost}
          onChange={(e) =>
            updateDraft((c) => {
              c.general.auto_hide_on_focus_lost = e.currentTarget.checked;
            })
          }
        />
        フォーカス喪失時の自動非表示
      </label>
      <label class="checkbox">
        <input
          type="checkbox"
          checked={d().general.show_tray_icon}
          onChange={(e) =>
            updateDraft((c) => {
              c.general.show_tray_icon = e.currentTarget.checked;
            })
          }
        />
        タスクトレイアイコン表示
      </label>
      <label class="checkbox">
        <input
          type="checkbox"
          checked={d().general.ime_off_on_show}
          onChange={(e) =>
            updateDraft((c) => {
              c.general.ime_off_on_show = e.currentTarget.checked;
            })
          }
        />
        入力ウィンドウ表示時にIMEをオフ
      </label>
    </div>
  );
};

export default SettingsGeneral;
