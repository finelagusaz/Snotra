# research: issue #524 — Engine ロック保持中の is_dir()

## issue の要約

`resolve_opener` / `resolve_all_openers` が `Mutex<Engine>` guard 保持中に `Path::is_dir()`（FS I/O）を実行する。死んだ UNC パスでは is_dir が最大 21 秒ブロックする実測値があり（issue #524 記載、2026-07-12 実測）、その間 Engine ロックを試みる全機能（検索・設定更新・インデックス swap）がフリーズする。

## 関連コード

| 箇所 | 内容 |
|---|---|
| `src-tauri/src/commands/launch.rs:242-247` | `resolve_opener` — guard 取得（243）→ `is_dir()`（244）→ `find_matching_tools`（245） |
| `src-tauri/src/commands/launch.rs:285-290` | `resolve_all_openers` — 同型（287 に `is_dir()`） |
| 呼び出し元 | `launch_item`（launch.rs:203、async IPC）/ `launch_item_with_state`（launch.rs:250、トレイ）/ `platform/tray.rs:431`（`resolve_all_openers`、トレイサブメニュー構築） |
| `find_matching_tools` | `snotra-core/src/opener.rs` — 純 CPU（config データ上のマッチング）。ロック内で問題なし |

grep 確認: `src-tauri/src` 内で guard 保持中に `is_dir()` を呼ぶのはこの 2 箇所のみ（他の `is_dir()` 出現なし）。

## 既存パターン

`FolderListContext`（`engine.rs:126-144`）が「snapshot → ロック外 I/O」の先例。ただし本件は **is_dir がロックを一切必要としない**（引数 `path` のみに依存し engine 状態を読まない）ため、snapshot 化すら不要 — **`is_dir()` をロック取得の前に移動するだけ**で解決する。issue の修正案（openers clone）より単純。

## 技術的制約

- `is_dir()` = `GetFileAttributesW` 系。同期 API で、死んだ UNC では SMB タイムアウトまでブロック（実測 1.25〜21 秒）。この待ち自体は「オープナールール解決にフォルダ判定が必要」という仕様上不可避で、本 issue のスコープは「待っている間に Engine ロックを道連れにしない」こと
- 残余（スコープ外）: `launch_item`（async fn）が `resolve_opener` を tokio worker 上で直接呼ぶため、死んだ UNC では worker スレッドが数秒ブロックする。multi-thread runtime のため他 IPC は別 worker で進行でき、全機能フリーズにはならない。`run_launch_blocking`（spawn_blocking）への移動は挙動・構造の変更が大きく、#524 の主害（ロック占有）解消後の必要性は実害次第 — 今回は見送る（YAGNI）

## 順序変更の安全性

`is_dir()` をロック前に移すことによる観測可能な差:
- ロック待ち中にファイルシステム状態が変わる TOCTOU 窓が「lock 後→lock 前」に移るが、元コードにも is_dir と実際の起動（ShellExecuteW、ロック外）の間に同種の窓が既にあり、悪化ではない
- `find_matching_tools` の入力（path, is_folder, openers）の意味論は不変

## 未解決の疑問

なし。
