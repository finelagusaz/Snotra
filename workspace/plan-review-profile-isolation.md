# 計画レビュー: #1306 プロファイル分離（独立レビュー・観点 1 状態とガード / 観点 2 列挙の完全性）

## 要対処

なし（計画の骨格は成り立つ）。下の軽微のうち M1・M2 は実装前に計画へ 1 行足すことを勧める。

## 軽微

- M1 identifier 差し替えは成り立つ（確認済み）。`tauri::Context::config_mut()` は公開 API（tauri-2.12.5 `src/lib.rs:438`、rustdoc に「app を build する前に config を書き換える」用途の例がある）。single-instance 2.5.5 は `setup` で `app.config().identifier` を読み、`{id}-sic/-siw/-sim` を作る（`platform_impl/windows.rs` の `init`）。差し替えは `generate_context!()`（`src-tauri/src/main.rs:245`）と `Builder::run(context)`（同 371）の間なので setup より前。identifier を読む他の経路（`app_data_dir` 等）は製品コードに無い（grep 0 件）。`semver` feature が有効なら id に `_<version>` が付くが、鍵の後ろに付くだけで衝突しない。
- M2 自動起動の書き込み経路は `autostart::enable/disable` だけ（`general.rs:44,46` が唯一の呼び出し元、`app.rs:702` が唯一の `is_enabled` 呼び出し）。本体 `snotra.exe` は autostart を呼ばない。ガード方針に漏れは見つからない。設定アプリは Snotra から env 継承で起動される（`commands/window.rs`）ので鍵が一致する。`--first-run` の経路（`main.rs:405`）は autostart に触れず阻害されない。
- M3 更新機構（`egui_shell/mod.rs:233` `spawn_update_check`）が計画の列挙に無い。seed は `auto_update = "disabled"` を注入する（`SnotraSmoke.psm1:263-273`）ので smoke 系は安全。だが手動起動の分離プロファイル（`cargo run` で config.toml なし）は `AutoUpdateMode` 既定が Full（`schema.rs:129`）のため実リリースを確認しにいき、トーストからインストールを押すと実機のインストールを上書きしうる。計画の「対象外」節（残余）に 1 行足すとよい。
- M4 `Resolve-SnotraExistingProcess -ExecutablePath` の根拠「同じ分離プロファイルの残留は同じ exe から起動されている」は偽になりうる。プロファイルは固定（`target/smoke-egui/profile` 等）で、exe は `-Profile debug/release`・`-ExePath` で変わる。debug の残留がある状態で release を起動すると、パスが違うので絞り込みから漏れ、single-instance で後発が黙って終了する（#872 と同じ失敗）。軽減: `finally` で自プロセスを落としている点、同じ target 配下の残留は稀な点。注記か、絞り込み条件を「パスが一致、または同じ repo の target 配下」へ広げる検討を。
- M5 計画は `Wait-SnotraWindow` の最初の 1 窓問題を拾っている（`SnotraSmoke.psm1:926` は `FindWindowW` 1 回で、実機の窓が同題 'Snotra' だと所有者不一致のまま待ち続ける）。この修正は必須。`Get-SnotraForegroundWindow` 系や他の `FindWindowW` 呼び出しは無い（grep で 926 のみ）ので追加漏れは無い。
- M6 鍵の導出の偽になる入力（いずれも計画の「対象外」か許容範囲）: (1) `\\?\C:\x` と `C:\x` は `std::path::absolute` が接頭辞を畳まないため別鍵（対象外へ 1 語足すとよい）。(2) 大文字小文字は `to_lowercase` で畳むが NTFS の大文字小文字区別ディレクトリは対象外。(3) 相対パスは起動時の CWD に依存。`config_dir` 自体が CWD 依存なので整合する。(4) 空白だけの上書き `" "` は「空でない」ので分離扱いになり、`absolute` が末尾空白を落として CWD を指す。実害は小さい。実プロファイルと同一視（鍵なし）される誤りの経路は見つからなかった（比較は `base.join("Snotra")` 同士の同一正規化）。
- M7 「変更しない散文」の判定: `scripts/measure-memory-stages.ps1:121,158,205`（`Stop-Process -Name snotra` で実機を落とす）・`manual-smoke.ps1:230`・`measure-memory.ps1:47` は実プロファイル前提で、計画は意図的に不変としている。判定は妥当だが、`measure-memory-stages.ps1:45` の `18,81` 例示は実機既定として正しい点も含め誤りは見つからなかった。
- M8 `smoke-egui.ps1:153,470` の `msedgewebview2` プロセス数の前後比較は実機と独立で、今回の変更の対象外（既存の性質）。

## 未検証

- ⚠️ U1 seed の既定ホットキーを `Ctrl+Alt+Q` にすると、smoke-egui の「押しっぱなしの Z」検査（`smoke-egui.ps1:540-600`）と `ShowAfterAltRelease` で、show 時に Ctrl がまだ押下中になりうる。チョードは down: Ctrl, Alt, Q／up: 逆順なので、Alt 解放で show が確定する時点では Ctrl は先に上がらず保持されている。egui 側が Ctrl+Z を「文字入力なし」と扱って検査が赤になる、あるいは held-key 取り込みの前提が変わる恐れがある。実機で一度回して確かめるべき（計画 Phase 6 の D で実測されるが、失敗時の代替案を用意しておくとよい）。また Ctrl+Alt は AltGr 扱いのキーボード配列があり、OS の入力経路が変わる。
- ⚠️ U2 src-tauri のテストが `register_hotkey`（`platform/hotkey.rs:162`）を実際に撃つか未確認。撃つなら実機の `Alt+Q` と衝突しうる。計画は cargo テスト経路のホットキーを列挙していない。
- ⚠️ U3 `cargo test` が実プロファイルへ書く経路（research A3）は `search/tests` の `Config::load()` と src-tauri fixture の 2 系統とされているが、`snotra-settings` の kittest（`en_harness`）や `snotra-egui-runtime` のテストが `Config::load` / `config_path` を呼ぶ経路は今回の範囲では全件確認していない。計画の検証（空ディレクトリを `SNOTRA_CONFIG_DIR` にして `cargo test --workspace` を回し何も作られないこと）が検出器になるので、それを必ず実施すること。
- ⚠️ U4 二重起動時（同一プロファイルの 2 回目）は `main.rs:167` の config 読み込みが single-instance 判定より前に走る（計画の A4 残余と同じ）。分離プロファイルでは害が小さいが、first-run の seed 作成（`config.toml` 無し）が 2 回目でも走る点は未測定。
- 未実施: 実ソースで確認したのは tauri 2.12.5 の `config_mut` と single-instance 2.5.5 の identifier 利用箇所。mutex 名の文字種制限（`\` を含む名前は失敗する）は計画の記述を信用しており、ソースの `CreateMutexW` 呼び出しで鍵が `[0-9a-f]` のみであることは確認できたが、Windows の名前空間挙動の実測はしていない。
