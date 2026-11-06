# #1306 計画レビュー（2 回目・Alt+F12 差分のみ）

## 要対処

1. 兄弟の経路が 1 本残る: `fallback_invalid_hotkey`（`snotra-core/src/config/migrate.rs:200-213`）。`HotkeyConfig::default()`（Alt+Q）へ書き戻す。計画は「対象外」と書くが、計画自身の理由（同じ `match` の兄弟で既定値が食い違うと、復旧時だけ実機と衝突する）がそのまま当たる。
   - 発火例: 分離プロファイルの `config.toml` に不正なホットキー（`Hyper+Q` など）、またはシステムショートカットが書かれている。`Loaded` 腕の `apply_migrations()`（`io.rs:52`）が Alt+Q へ直して保存し、分離プロファイルが実機と衝突する。
   - 対処案: `apply_migrations` に既定のホットキーを渡さず、`load_from_dir_reporting` 側で `defaults.hotkey` を使う。または `fallback_invalid_hotkey` の戻し先を引数化する。最小は、対象外にする理由を「初回起動ではない」から「衝突しうるが残余として受容」へ書き直し、SPEC の残余にも明記すること。
   - 方針を決める際は、`HotkeyConfig::default()` のリテラルが 2 関数に 1 か所ずつという前提（下の 2）も同時に見直す。

2. この差分で偽になる doc を計画が拾っていない: `snotra-core/src/hotkey.rs:30-32`。「既定ホットキーのリテラルは `default_hotkey_modifier` / `default_hotkey_key` の各 1 か所だけ」と断言している。`ISOLATED_PROFILE_HOTKEY_*` 定数が入ると、リテラルは 2 組になる。計画の「触る範囲」表は hotkey.rs の項目に doc の修正を含めていない。あわせて、`io.rs` 冒頭の `//!` と `load_from_dir_reporting` の rustdoc に「既定値は引数で受ける」を足す必要がある。
   - なお `snotra-core/CLAUDE.md` は「`Config::default()` の明示初期化…」「既定へ戻すときは `normalized_default()`」と書く。分離の既定が別にあることの注記先を決めること。

## 軽微

1. 設定アプリの「初期設定に戻す」（`snotra-settings/src/app.rs:255` は `Config::normalized_default()` を draft に代入）は Alt+Q に戻る。計画は対象外と明記済み（裁定の範囲）。ただし帰結は、「保存すると config_watcher（`src-tauri/src/config_watcher.rs:116-134`）が Alt+Q を再登録し、実機と衝突する。登録失敗の通知は出るが、実機側のホットキーが奪われる／奪うのは先に登録した側で、後発の分離側が失敗する」形になる。この点の記述（残余）には「分離側が失敗通知を出す」と書いておくとよい。`app.rs:736-928` の `normalized_default()` を使うテスト群は、ホットキーの値に依存しないので影響しない（`hotkey_toggle` のみ参照）。
2. config_watcher の再読込（`config_watcher.rs:72-76` は `Config::load_reporting` を渡す）は、計画の `load_reporting` 変更で自動的に分離の既定を返す。`ReadFailed` は適用されない（`should_apply_config_change`）。`RecoveredFromCorrupt` は分離の既定 Alt+F12 を適用する。穴は無いと確認した。ただし、テスト側（`config_watcher.rs:260,308,323,343,363`）は `Config::default()` を直接使う（Alt+Q）。ホットキーの値に依存するアサーションが無いかは、実装時に確認すること（私は未確認）。
3. 他の製品コードの `Config::default()` 直呼びは `src-tauri/src/state.rs:151`・`commands/system.rs:37`・`snotra-core/src/engine.rs:364` で、いずれもテスト/フォールバックの Engine 初期値。ホットキーは登録に使われないので、衝突経路にはならない（`main.rs:168` と `snotra-settings/src/main.rs:25` の 2 つの入口は、どちらも `load_reporting` を通る）。
4. 新しい分離既定をテストに足すとき、`config/io/tests.rs:353-388` など既存の `Alt+Q` アサーション（`key = "Q"` の fixture）は、実プロファイル経路の期待値として残る。新テストは env を書かずに `load_from_dir_reporting(dir, defaults)` へ注入する設計なので、並列テストへ影響しない（計画の意図どおり）。

## 未検証

1. ⚠️ `RegisterHotKey` と F12: Microsoft の文書は「F12 キーはデバッガ用に常に予約されているので、ホットキーとして登録してはならない」と書く。修飾（Alt）付きで登録できるかは、リポジトリ内に実測も記録も見つからない（`git grep F12` は `platform/hotkey.rs:94` の VK 表・`hotkey.rs` の parser・古い `docs/superpowers/plans/…:747` の例だけ）。登録に失敗すると、分離プロファイルの起動が毎回「登録失敗」通知を出す。計画では Phase 6（実機スモーク A2）まで確かめない。**Phase 1 の前に、`Alt+F12` を実機で `RegisterHotKey` する最小の測定を置くべきである**（失敗するなら、`Alt+F11` など別の鍵へ選び直しで、計画全体が変わる）。デバッガ常駐の開発機では結果が変わりうる点も添える。
2. `validate.rs` / `is_system_shortcut`: `hotkey.rs:163` を読んだ限り、Alt 単独のシステム衝突は F4・Space・Tab のみで、`Alt+F12` は通る。`validate.rs` の本体は未読（`hotkey.rs` の `is_system_shortcut` 経由のはず）。
3. smoke の注入: VK は `18`（Alt）と `123`（F12 = 0x7B。`platform/hotkey.rs:94` と一致）。`smoke-egui.ps1:259` は `[byte]` へ変換するので範囲は問題ない。ただし `smoke-egui.ps1:253-257` の「単一の出所」コメントと `occupy-hotkey.ps1` の写像が F12 を持つかは未確認（計画は写像を直すと書いている）。キー注入が F12 を debugger 予約として落とさないかは、1 と同じ測定で確かめる。
4. `SnotraSmoke.psm1` の seed が `[hotkey]` を必ず書くか（書かなければ serde の欠落補完で Alt+Q になる・`config/tests.rs:82,352` が当てる経路）。計画は seed の既定を F12 にするとあるが、`[hotkey]` セクションを常に出力することの確認は私は未実施。
