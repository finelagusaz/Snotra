# 計画 — issue #1444 hide の trim を自プロセスと設定サイドカーに絞る

分類: バグ（Snotra から起動したアプリへ `EmptyWorkingSet` が掛かる意図しない副作用）。`SPEC.md` に working set の記述は無いので変えない。調査は `workspace/research.md`。

## 目的と受け入れ条件

- hide の `EmptyWorkingSet` を、自プロセスと設定サイドカー（`SettingsProcessState` の子）にだけ掛ける。`EmptyWorkingSet` そのものはやめない
- 受け入れ:
  1. Snotra から起動したアプリの `WorkingSet64` が、Snotra の hide の前後で落ちない（下の計測。main のビルドでは落ちることを先に確かめる＝陰性対照）
  2. Snotra 自身の非表示時の working set が今までどおり削られる（同じ計測で Snotra 自身も見る）
  3. 設定サイドカーが存命中は、今までどおりそれも削る（同じ計測でサイドカーも見る）

## 設計

`trim_idle_working_set` を次の形にする。引数は `root_pid: u32` から `app: &tauri::AppHandle` に替える（設定サイドカーを `SettingsProcessState` から引くため）。

1. 自プロセス: `GetCurrentProcess()`（擬似ハンドル・閉じなくてよい）に `EmptyWorkingSet`
2. 設定サイドカー: `app.try_state::<SettingsProcessState>()` → `try_lock()`。取れて `Some(child)` なら、**guard を持ったまま** `child.as_raw_handle()` を `HANDLE` にして `EmptyWorkingSet` を呼ぶ（Child の drop は exit ハンドラと監視スレッドで起きるので、guard を放してから使うと閉じたハンドルを叩きうる・3b の F3）。取れなければ見送る（best-effort。`launch_settings_process` が spawn の間ロックを持つので、`lock()` で待つとイベントループが止まる）
3. 子孫は辿らない。Toolhelp のスナップショットも撮らない

非 Windows の no-op 版も同じシグネチャにする。`collect_descendant_pids` とそのテスト 6 本は呼び手が無くなるので消す（`-D warnings` の下では `dead_code` で落ちる）。

## 変更するファイルと対象シンボル

1. `src-tauri/src/working_set.rs`
   - `//!` を書き直す（対象が「自プロセスと設定サイドカー」であること、子孫を辿らない理由＝Snotra から起動したアプリや設定サイドカーが開いたブラウザ等も子孫に入るから、#1444）。`docs/comment-guidelines.md` が `//!` の模範例として名指しているので、様式（短く・理由を書く・best-effort と panic しない旨を残す）を保つ
   - `trim_idle_working_set`（Windows / 非 Windows の両方）
   - `collect_descendant_pids` と `mod tests` を削除
   - `HandleGuard` は不要になる（擬似ハンドルは閉じず、Child のハンドルは Child が持つ）ので削除
2. `src-tauri/src/egui_shell/window_coordinator.rs` の `hide_egui_main` — 呼び出しを `trim_idle_working_set(app)` にし、直前のコメント（「子孫 BFS は設定プロセス…も巻き込みうる」）と、上の「Toolhelp スナップショット + プロセスツリー BFS の合計」を直す
3. `src-tauri/Cargo.toml` — `windows` の features から `Win32_System_Diagnostics_ToolHelp` を外す（他に使う箇所なし・3b で確認）。外して `cargo build -p snotra` が通ることで使っていないことを確かめる
4. `src-tauri/CLAUDE.md` のモジュール索引「`working_set.rs` — 非表示アイドル時のプロセスツリー working set 回収」→「非表示アイドル時の working set 回収（自プロセスと設定サイドカー）」
5. `PERFORMANCE.md`「アイドルメモリ（working set）の削減（2026-06-01〜02 計測）」冒頭の注記「`EmptyWorkingSet` の採用判断（下記）だけは現行でも生きている」に、「適用先は #1444 で自プロセスと設定サイドカーに絞った」を足す。節の本文（WebView2 期の記録）は直さない。あわせて本件の計測値を新しい記録として 1 節足す

触らないもの: `scripts/measure-memory.ps1`（冒頭の「ツリーに乗りうる子は設定サイドカーだけ」は本件の前から偽だが、`scripts/` はセーフティネットなので別 issue とし、ユーザーに伝える）、`SPEC.md`。

## 実装の順序

1. 計測の器を作り、main のビルドで陰性対照（起動したアプリの WS が hide 後に落ちる）を取る
2. `working_set.rs` を直し、呼び出し・Cargo.toml・文書を直す
3. 同じ器で本ブランチを測る

## 不変条件と異常系

- 自プロセスの trim は毎回走る（`GetCurrentProcess` は失敗しない）。`EmptyWorkingSet` の失敗は黙って捨てる
- 設定サイドカーの trim は、`SettingsProcessState` が無い・ロックが取れない・`None` のときは見送る。どれも「メモリが減らない」だけで機能に影響しない
- Child のハンドルは guard を持っている間だけ使う
- panic しない（`unwrap` / `expect` / 添字を使わない。`try_lock` の `Err`〈毒を含む〉は見送りへ倒す）
- 境界条件と検証:
  - 設定サイドカーなし → 自プロセスだけ（計測の通常の回）
  - 設定サイドカーあり → 両方（計測でサイドカーを開いた回）
  - ロック競合 → 見送り（`launch_settings_process` の spawn・監視スレッドの `try_wait`・hotkey listener の読みのいずれかと hide が重なったとき。決定的に再現できないので、コードを読んで確かめる・`/race-check`）
  - Snotra から起動したアプリ → 触らない（計測）

## テスト方針と検証コマンド

- 純粋核が無くなる（`collect_descendant_pids` を消す）ので、新しい単体テストは置かない。`trim_idle_working_set` は Win32 と `AppHandle` に依存し、ユニットテストの前提にしない（`.claude/rules/src-tauri.md`）。挙動は下の計測で確かめる
- 計測（受け入れ 1〜3。スクリプトはスクラッチに置き、リポジトリに入れない）:
  - `scripts/lib/SnotraSmoke.psm1` の `New-SnotraVerificationProfile`（分離プロファイル・ホットキー Alt+F12）と `Start-SnotraProcess` を使い、debug ビルドの Snotra を起こす。scan パスは一時フォルダで、そこに `winver.exe` の写しを別名（例 `wstrimprobe.exe`）で置く
  - ホットキーで表示 → 名前を打つ → Enter で起動させる（ShellExecuteW 経路で Snotra の子になる。親 PID で確かめる）。起動後の hide を待ち、数秒おいて起動したプロセスの `WorkingSet64` を読む → ホットキーで表示して Escape で隠す → 再び読む。これを 3 回繰り返す
  - 同じ回で Snotra 自身の `WorkingSet64`（hide 前と後）も読む（受け入れ 2）
  - 設定サイドカー: 同じ手順の中で設定を開き（設定ウィンドウを開く操作は実装時に `SPEC.md` で確かめる）、Snotra の hide の前後でサイドカーの `WorkingSet64` を読む（受け入れ 3）
  - main（陰性対照）と本ブランチで 1 回ずつ。期待: main では起動したアプリの WS が hide 後に大きく落ち、本ブランチでは落ちない。Snotra 自身とサイドカーは両方で落ちる
  - 常駐の Snotra とは別のプロファイル・別ホットキーなので並走できる（`New-SnotraVerificationProfile` の doc・#1306）。常駐を止めない
- 検証: `docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリ A（`.rs` と `Cargo.toml`）。hide の経路を触るのでカテゴリ C の `smoke:egui` も回す（常駐の Snotra がいると動かないので、そのときユーザーに終了を頼む）
- 変異注入（`/implement`「3b. 委譲へ渡すもの」の委譲先が行う）: 単体テストが無いので、変異の検知は計測でしか測れない。委譲先には変異注入ではなくコードレビュー（とくに guard の寿命・`try_lock` の毒の扱い・`as_raw_handle` の型変換）を頼む

## 文書の更新

- `SPEC.md`: 不要
- `src-tauri/CLAUDE.md`: 索引の 1 行（上記 4）
- `PERFORMANCE.md`: 注記 1 行と計測の記録 1 節（上記 5）

## 作業項目

### Phase 1 — 陰性対照
- [x] スクラッチに計測スクリプトを書き、main の debug ビルドで受け入れ 1〜3 の値を取る（起動したアプリの WS が hide 後に落ちることを確かめる） — main で起動したプロセス（cmd.exe の写し・親は Snotra）の WS が 6.24 → 0 MiB。winver の写しはすぐ終了するので使えなかった

### Phase 2 — 実装
- [x] `working_set.rs` の `trim_idle_working_set` を自プロセス + 設定サイドカーへ書き換え、`collect_descendant_pids`・`HandleGuard`・テストを削除し、`//!` を書き直す
- [x] `window_coordinator.rs` の呼び出しと 2 か所のコメント
- [x] `src-tauri/Cargo.toml` から `Win32_System_Diagnostics_ToolHelp` を外す
- [x] `src-tauri/CLAUDE.md` の索引

### Phase 3 — 検証と記録
- [x] 同じスクリプトで本ブランチを測り、受け入れ 1〜3 を確かめる — 起動したプロセス 7.3 → 7.3 MiB（落ちない）・Snotra 自身 8.73 MiB（main 8.79）・サイドカーの hide 直後 13.51 / 13.34 MiB（main 13.59 / 13.59）。値は PERFORMANCE.md。受け入れ 2 は、issue が挙げる `measure:memory` の PrivWS ではなく Snotra 自身の `WorkingSet64` を同じ計測の中で読んだ（自プロセスへの trim は従来と同じ呼び出しなので、同じ回の中で main と並べるほうが直接比べられる）
  - ラウンド 1 のレビュー（sha 4ecebb6c・verify:tree green〈A・C・E・F〉）の M3 を受けて、サイドカーの陰性対照を取った: 本変更からサイドカーの trim だけを消した版で hide 直後 107.98 / 105.62 MiB、本変更 12.55 / 13.22 MiB。受け入れ 3 を確かめた
- [x] 変更後の検証チェックリスト（カテゴリ A と C）を通す
  - ラウンド 1: sha 4ecebb6c / verify:tree green（A・C・E・F）/ 指摘 High 2・Medium 3・Low 9。採用: H1（訳語「毒」→ `PoisonError`）・M1（`PERFORMANCE.md` の同じ引用ブロックの「プロセスツリーは 1 件」を直す）・M3（陰性対照で解消）・L1（drop する場所の列挙をやめる）・L2（`//!` から経緯と起動経路の列挙を削る）・L3（`window_coordinator.rs` の写しを削り、指す先の記述を `//!` に足す）・L4（回数の写しを削る）・L6（標本数と丸めを記録に書く）・変異 (d) の検知案（`&Child` を受ける `empty_child_working_set` に閉じ込める）。H2 は `working_set.rs` の中だけ「ロック」に替えた（リポジトリ全体の「錠」の統一は範囲外）。不採用・L5（計画どおりの設計）・変異 (a)(c) の検知器（守る経路が 1 つずつで保守だけが増える）。L9 は計画の文言だけの誤りで、コードの doc に「見送った回は次の hide まで削られない」を書いた
  - ラウンド 2: sha a712236e / verify:tree green（A・C・E・F）/ 前回の採用分はすべて解消 / 新規 Low 5（`//!` の「イベントループから呼んでよい理由」にロックを待たないことを足す・陰性対照と同じ回の本変更の値を PERFORMANCE.md へ・表の丸めの表記・plan の H2 の記録と競合の全称を直す）——すべて採用 / 変異 (d) の自然な形は借用検査が拒む（E0505）。raw handle を持ち出す形は今も検知なし
- [x] `PERFORMANCE.md` の注記と記録

## 未確定（実装前に潰す）

（なし）

## 人間レビュー

- [x] 承認済み — 2026-11-16 / 問い: "計画を承認なさいますか。それとも `workspace/plan.md` に注釈を書き込まれますか。" / 回答: "承認"

## セルフレビュー

- リスク: 通常（永続形式・設定・公開 API・モード・並行の新設なし。共有状態 `SettingsProcessState` は読むだけで、`try_lock` で待たない。`/race-check` は実装段階で回す）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査）
- 要対処: 3b の所見 5 件を反映（F2 サイドカーの子・F3 guard の寿命・F5 計測が変更の分岐を通らない → 専用の計測と陰性対照・F6 残骸の一覧・F8 は不採用で維持）
- 未検証: 起動したアプリへの実害の額（Phase 1 で測る）
- 散文の全件列挙: 未実施（通常リスク）。代わりに 3b が `collect_descendant_pids`・Toolhelp・「プロセスツリー」・「子孫」で残骸を数えた（F6）
- 5a の照合: issue の要件 3 つに計測が対応 / 境界条件 4 つに検証 / 新しい状態・リソースは無い（ハンドルは借りるだけ） / より単純な既存パターンは無い（子孫を辿らないぶん今より単純） / 不変条件（自プロセスを毎回 trim・起動したアプリに触らない）は計測で検知
