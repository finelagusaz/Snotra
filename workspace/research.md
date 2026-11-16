# 調査 — issue #1444

## 要約

`src-tauri/src/working_set.rs` の `trim_idle_working_set(root_pid)` は、Toolhelp のスナップショットから `root_pid`（= Snotra 自身）の子孫をすべて集め、それぞれに `EmptyWorkingSet` を掛ける。呼び出し元は `egui_shell/window_coordinator.rs` の `hide_egui_main` の末尾の 1 か所だけで、main の hide のたびに走る。

Snotra から起動したアプリは子孫に入りうる——`commands/launch.rs` の `launch_item_core` は `ShellExecuteW` を Snotra のプロセス内で呼び、オープナー経由の `std::process::Command::new`（`launch.rs` の 65 行・268 行付近）は常に子を作る。起動成功の直後に hide が走るので、起動したアプリは起動中に working set を失い、以後 Snotra を隠すたびに失う（コードからの導出・実機未計測）。

直し方（issue の案）: `EmptyWorkingSet` の対象を自プロセスと設定サイドカー（`SettingsProcessState` が持つ子）に絞る。Toolhelp のスナップショットも要らなくなる。`EmptyWorkingSet` そのものはやめない。

分類: バグ（意図しない対象へ副作用が及んでいる）。`SPEC.md` に working set の記述は無い（`git grep` で 0 件）。

## 関連するファイル・シンボル

- `src-tauri/src/working_set.rs`
  - `collect_descendant_pids`（純関数・BFS）とそのテスト 6 本（`mod tests`）
  - `trim_idle_working_set(root_pid)`（Windows）: `CreateToolhelp32Snapshot` → `collect_descendant_pids` → 各 pid に `OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA)` + `EmptyWorkingSet`。`HandleGuard`（RAII）
  - 非 Windows の no-op 版
  - `//!`（「プロセスツリー（自プロセス + 子孫。設定サイドカー存命中はそれも含む）」）——`docs/comment-guidelines.md` が `//!` の模範例として名指している
- `src-tauri/src/egui_shell/window_coordinator.rs` の `hide_egui_main` — 呼び出し `crate::working_set::trim_idle_working_set(std::process::id())` と、その直前のコメント（「子孫 BFS は設定プロセス…も巻き込みうる」）
- `src-tauri/src/commands/window.rs`
  - `pub type SettingsProcessState = Mutex<Option<Child>>`
  - `launch_settings_process`: ロックを取ったまま `Command::new(...).spawn()` し、`Some(child)` を入れる。監視スレッドが 250 ms ごとにロックを取り `try_wait`、終了で `None` にする
- `src-tauri/src/main.rs` — `.manage(SettingsProcessState::default())`、hotkey 経路（430 行付近）と exit ハンドラ（575 行付近）が `SettingsProcessState` を読む
- `src-tauri/CLAUDE.md` のモジュール索引「`working_set.rs` — 非表示アイドル時のプロセスツリー working set 回収」
- `PERFORMANCE.md`「アイドルメモリ（working set）の削減（2026-06-01〜02 計測）」と「採用: hide 時の `EmptyWorkingSet`（issue #355 / PR #360）」（「プロセスツリー全体へ能動適用」）。節の冒頭に「WebView2 期の記録」「`EmptyWorkingSet` の採用判断だけは現行でも生きている」とある
- 計測スクリプト: `scripts/measure-memory.ps1`（子孫を BFS で合算）、`scripts/measure-memory-stages.ps1`

## 再利用できる既存パターン

- `HandleGuard`（同ファイル内の RAII）
- 自プロセスの handle は `GetCurrentProcess()`（擬似ハンドル・閉じなくてよい）で取れる。`OpenProcess` が要らない
- 設定サイドカーは `std::process::Child` を `SettingsProcessState` に持つ。Windows の `Child` は `AsRawHandle` でプロセスハンドル（`CreateProcess` が返す全権のハンドル）を渡せる。`Child` を持っている間はハンドルが開いているので、PID の再利用で別のプロセスを掴むことが無い（pid から `OpenProcess` する形ではこの保証が無い）

## 技術的な制約

- `hide_egui_main` はイベントループのスレッドで走る（同関数のコメント「臨界区間の外は…イベントループスレッド上で走る」）。`SettingsProcessState` の `Mutex` を `lock()` で待つと、`launch_settings_process` がロックを持ったまま `spawn` している間、イベントループが止まる。`try_lock` で取れなければ設定サイドカーの trim を見送る（best-effort）形にする
- `state.rs` の `AppState::read_config` の doc は `SettingsProcessState` などの `Mutex` を取る害に触れているが、射程は `read_config` に渡すクロージャの中（config の read guard を持っている間）だけである。`hide_egui_main` は read guard を持たないので、この規範の外（自分で読んで確かめた）
- `collect_descendant_pids` は呼び手が無くなる。`-D warnings` の下では `dead_code` で落ちるので、テストごと消す（`.claude/rules/rust.md`「公開面を狭めるときは…」）。`comment-guidelines.md` の模範例の名指しは `working_set.rs` の `//!` を指しているので、`//!` を書き直しても模範であり続けるように書く
- 設定サイドカー（`snotra-settings`）は `open::that` で子を作りうる（`app.rs` の Web・Mail リンク、`tabs/backup.rs` のデータフォルダ。3b の F2 で発覚し、自分で `git grep` して確かめた）。これらは「設定から開いたブラウザ等」であり、Snotra から起動したアプリと同じく trim すべきでない対象である。新しい形では対象外になる——意図どおり
- release は `panic = "abort"`（`//!` が `unwrap` / `expect` / 添字を使わないと書いている）

## 未解決の疑問

- 受け入れ「起動したアプリの `WorkingSet64` が hide の前後で落ちない」をどう測るか。Snotra からアプリを起動して hide する操作が要る。開発ビルドを `SNOTRA_CONFIG_DIR` の一時プロファイルで起こし、キー注入で起動させられるか、それとも人手で測るか
- `measure:memory` の値（Snotra 自身の非表示時 PrivWS）が変わらないこと——自プロセスへの trim は従来どおりなので変わらない見込み。測るか
- （解消）`try_lock` の形は `state.rs` の規範と食い違わない（上の制約）

## 敵対的調査（3b）の所見と採否

出力: `workspace/adversarial-1444.txt`。

- 壊せなかった: 争点 1（一時 PowerShell から ShellExecuteW で起動した 3 件とも ParentProcessId が呼び手。ただし常駐 Snotra の子の実測ではない）、3（擬似ハンドルと Child のハンドルは権限が足りる）、4（lock の停止は spawn 時間分でデッドロックではない。try_lock は有効だが必須ではない。read_config の規範の射程外）、5（`collect_descendant_pids` の呼び手は `trim_idle_working_set` だけ）
- 採用 F2: 設定サイドカーは `open::that` で子を作る（上の制約に反映）
- 採用 F3: Child の drop は別スレッド（exit ハンドラと監視スレッド）で起きる。raw handle を取り出してから guard を放すと、閉じたハンドルを叩きうる → `EmptyWorkingSet` が終わるまで guard を持つ
- 採用 F5（最重要）: 既存の計測（`measure:memory` / `measure:memory:stages`）はアプリを起動しないので、この変更の効果も副作用も通らない。受け入れ 1 は専用の測り方が要り、変更前のビルドで「起動したアプリの WorkingSet64 が hide 後に落ちる」ことを先に再現する（陰性対照）。受け入れ 3（サイドカーを今までどおり trim）もどの計測も通らない → 同じ測り方でサイドカーも見る
- 採用 F6: 残骸 = `src-tauri/Cargo.toml` の `Win32_System_Diagnostics_ToolHelp` feature（他に使う箇所なし）、`window_coordinator.rs` の 2 か所のコメント、`src-tauri/CLAUDE.md` の索引、`working_set.rs` の `//!`。`PERFORMANCE.md` の WebView2 期の節は履歴なので本文は直さず、節の冒頭の注記（現行に生きている判断の範囲）だけを直す
- F7（記録）: 実 config の opener は `folder:c:\workspace` の 1 件（wt.exe と Code.exe）。実運用の起動の大半は ShellExecuteW 経路
- F8（⚠️・不採用）: 設定サイドカーの trim が得か損かは測った記録が無い。issue の受け入れ 3 が維持を求めているので維持する。畳むなら別 issue
- F9（⚠️・記録）: 実害の額は未測定。F5 の測り方で一緒に数字を取る
- `scripts/measure-memory.ps1` 冒頭の「ツリーに乗りうる子は設定サイドカーだけ」は本件の前から偽（Snotra から起動したアプリも子になる）。`scripts/` はセーフティネットなので本件では触らず、ユーザーに伝える
