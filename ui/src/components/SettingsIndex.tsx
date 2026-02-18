import type { Component } from "solid-js";
import { For, Show } from "solid-js";
import { draft, updateDraft } from "../stores/settings";

const SettingsIndex: Component = () => {
  const d = () => draft()!;

  function addScanPath() {
    updateDraft((c) => {
      c.paths.scan.push({ path: "", extensions: [], include_folders: false });
    });
  }

  function removeScanPath(idx: number) {
    updateDraft((c) => {
      c.paths.scan.splice(idx, 1);
    });
  }

  return (
    <div class="settings-section">
      <label>
        最大表示件数
        <input
          type="number"
          min="1"
          max="50"
          value={d().appearance.max_results}
          onInput={(e) =>
            updateDraft((c) => {
              c.appearance.max_results = parseInt(e.currentTarget.value) || 8;
            })
          }
        />
      </label>
      <label>
        ウィンドウ幅
        <input
          type="number"
          min="300"
          max="1200"
          value={d().appearance.window_width}
          onInput={(e) =>
            updateDraft((c) => {
              c.appearance.window_width =
                parseInt(e.currentTarget.value) || 600;
            })
          }
        />
      </label>
      <label>
        履歴保持数
        <input
          type="number"
          min="10"
          max="1000"
          value={d().appearance.top_n_history}
          onInput={(e) =>
            updateDraft((c) => {
              c.appearance.top_n_history =
                parseInt(e.currentTarget.value) || 200;
            })
          }
        />
      </label>
      <label class="checkbox">
        <input
          type="checkbox"
          checked={d().appearance.show_icons}
          onChange={(e) =>
            updateDraft((c) => {
              c.appearance.show_icons = e.currentTarget.checked;
            })
          }
        />
        アイコン表示
      </label>

      <h3>追加パス</h3>
      <For each={d().paths.additional}>
        {(path, idx) => (
          <div class="path-row">
            <input
              type="text"
              value={path}
              onInput={(e) =>
                updateDraft((c) => {
                  c.paths.additional[idx()] = e.currentTarget.value;
                })
              }
            />
            <button
              onClick={() =>
                updateDraft((c) => {
                  c.paths.additional.splice(idx(), 1);
                })
              }
            >
              削除
            </button>
          </div>
        )}
      </For>
      <button
        onClick={() =>
          updateDraft((c) => {
            c.paths.additional.push("");
          })
        }
      >
        追加パスを追加
      </button>

      <h3>スキャンパス</h3>
      <For each={d().paths.scan}>
        {(scan, idx) => (
          <div class="scan-path-block">
            <label>
              パス
              <input
                type="text"
                value={scan.path}
                onInput={(e) =>
                  updateDraft((c) => {
                    c.paths.scan[idx()].path = e.currentTarget.value;
                  })
                }
              />
            </label>
            <label>
              拡張子 (カンマ区切り)
              <input
                type="text"
                value={scan.extensions.join(", ")}
                onInput={(e) =>
                  updateDraft((c) => {
                    c.paths.scan[idx()].extensions = e.currentTarget.value
                      .split(",")
                      .map((s) => s.trim())
                      .filter((s) => s.length > 0);
                  })
                }
              />
            </label>
            <label class="checkbox">
              <input
                type="checkbox"
                checked={scan.include_folders}
                onChange={(e) =>
                  updateDraft((c) => {
                    c.paths.scan[idx()].include_folders =
                      e.currentTarget.checked;
                  })
                }
              />
              フォルダを含める
            </label>
            <button onClick={() => removeScanPath(idx())}>削除</button>
          </div>
        )}
      </For>
      <button onClick={addScanPath}>スキャンパスを追加</button>
    </div>
  );
};

export default SettingsIndex;
