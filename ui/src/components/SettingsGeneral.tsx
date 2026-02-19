import type { Component } from "solid-js";
import { draft, updateDraft } from "../stores/settings";
import SettingRow from "./SettingRow";
import ToggleSwitch from "./ToggleSwitch";

const SettingsGeneral: Component = () => {
  const d = () => draft()!;

  return (
    <div class="settings-section">
      <div class="settings-group">
        <div class="settings-group-title">ホットキー</div>
        <div class="settings-group-content">
          <SettingRow label="修飾キー" block>
            <input
              type="text"
              value={d().hotkey.modifier}
              onInput={(e) =>
                updateDraft((c) => {
                  c.hotkey.modifier = e.currentTarget.value;
                })
              }
            />
          </SettingRow>
          <SettingRow label="キー" block>
            <input
              type="text"
              value={d().hotkey.key}
              onInput={(e) =>
                updateDraft((c) => {
                  c.hotkey.key = e.currentTarget.value;
                })
              }
            />
          </SettingRow>
          <SettingRow
            label="トグル動作"
            description="ホットキーで表示中のウィンドウを非表示にします"
          >
            <ToggleSwitch
              checked={d().general.hotkey_toggle}
              onChange={(v) =>
                updateDraft((c) => {
                  c.general.hotkey_toggle = v;
                })
              }
            />
          </SettingRow>
        </div>
      </div>

      <div class="settings-group">
        <div class="settings-group-title">動作</div>
        <div class="settings-group-content">
          <SettingRow
            label="起動時に表示"
            description="アプリ起動時に検索ウィンドウを自動表示します"
          >
            <ToggleSwitch
              checked={d().general.show_on_startup}
              onChange={(v) =>
                updateDraft((c) => {
                  c.general.show_on_startup = v;
                })
              }
            />
          </SettingRow>
          <SettingRow
            label="フォーカス喪失時に非表示"
            description="他のウィンドウをクリックした時に自動で隠します"
          >
            <ToggleSwitch
              checked={d().general.auto_hide_on_focus_lost}
              onChange={(v) =>
                updateDraft((c) => {
                  c.general.auto_hide_on_focus_lost = v;
                })
              }
            />
          </SettingRow>
          <SettingRow
            label="トレイアイコン"
            description="システムトレイにアイコンを表示します"
          >
            <ToggleSwitch
              checked={d().general.show_tray_icon}
              onChange={(v) =>
                updateDraft((c) => {
                  c.general.show_tray_icon = v;
                })
              }
            />
          </SettingRow>
          <SettingRow
            label="表示時にIMEオフ"
            description="ウィンドウ表示時にIMEを自動でオフにします"
          >
            <ToggleSwitch
              checked={d().general.ime_off_on_show}
              onChange={(v) =>
                updateDraft((c) => {
                  c.general.ime_off_on_show = v;
                })
              }
            />
          </SettingRow>
        </div>
      </div>
    </div>
  );
};

export default SettingsGeneral;
