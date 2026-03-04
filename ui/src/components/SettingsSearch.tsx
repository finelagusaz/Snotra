import type { Component } from "solid-js";
import { draft, updateDraft } from "../stores/settings";
import { t } from "../lib/i18n";
import SettingRow from "./SettingRow";
import ToggleSwitch from "./ToggleSwitch";

const SettingsSearch: Component = () => {
  const d = () => draft()!;

  return (
    <div class="settings-section">
      <div class="settings-group">
        <div class="settings-group-title">{t("settings.search.group.mode")}</div>
        <div class="settings-group-content">
          <SettingRow
            label={t("settings.search.normal_mode.label")}
            description={t("settings.search.normal_mode.description")}
          >
            <select
              value={d().search.normal_mode}
              onChange={(e) =>
                updateDraft((c) => {
                  c.search.normal_mode = e.currentTarget.value;
                })
              }
            >
              <option value="prefix">{t("settings.search.mode.prefix")}</option>
              <option value="substring">{t("settings.search.mode.substring")}</option>
              <option value="fuzzy">{t("settings.search.mode.fuzzy")}</option>
            </select>
          </SettingRow>
          <SettingRow
            label={t("settings.search.folder_mode.label")}
            description={t("settings.search.folder_mode.description")}
          >
            <select
              value={d().search.folder_mode}
              onChange={(e) =>
                updateDraft((c) => {
                  c.search.folder_mode = e.currentTarget.value;
                })
              }
            >
              <option value="prefix">{t("settings.search.mode.prefix")}</option>
              <option value="substring">{t("settings.search.mode.substring")}</option>
              <option value="fuzzy">{t("settings.search.mode.fuzzy")}</option>
            </select>
          </SettingRow>
        </div>
      </div>

      <div class="settings-group">
        <div class="settings-group-title">{t("settings.search.group.visibility")}</div>
        <div class="settings-group-content">
          <SettingRow
            label={t("settings.search.show_hidden.label")}
            description={t("settings.search.show_hidden.description")}
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
        <div class="settings-group-title">{t("settings.search.group.history")}</div>
        <div class="settings-group-content">
          <SettingRow
            label={t("settings.search.history_size.label")}
            description={t("settings.search.history_size.description")}
          >
            <input
              type="number"
              min="10"
              max="1000"
              value={d().appearance.top_n_history}
              onInput={(e) =>
                updateDraft((c) => {
                  c.appearance.top_n_history =
                    Math.max(10, Math.min(1000, parseInt(e.currentTarget.value) || 200));
                })
              }
              style={{ width: "80px" }}
            />
          </SettingRow>
        </div>
      </div>

      <div class="settings-group">
        <div class="settings-group-title">{t("settings.search.group.history_score")}</div>
        <div class="settings-group-content">
          <SettingRow
            label={t("settings.search.history_normalization.label")}
            description={t("settings.search.history_normalization.description")}
          >
            <select
              value={d().search.history_normalization}
              onChange={(e) =>
                updateDraft((c) => {
                  c.search.history_normalization = e.currentTarget.value;
                })
              }
            >
              <option value="disabled">{t("settings.search.normalization.disabled")}</option>
              <option value="fuzzy_relative_cap">{t("settings.search.normalization.fuzzy_relative_cap")}</option>
            </select>
          </SettingRow>
          <SettingRow
            label={t("settings.search.history_cap.label")}
            description={t("settings.search.history_cap.description")}
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
