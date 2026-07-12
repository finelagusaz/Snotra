# plan: issue #524 — is_dir() を Engine ロックの外へ

## 変更ファイル一覧

1 ファイルのみ: `src-tauri/src/commands/launch.rs`

- `resolve_opener`（242-247）: `let is_folder = ... is_dir()` を `let engine = state.engine.lock()` の**前**へ移動。doc コメントを「is_dir（FS I/O）はロック外・ロック内は純 CPU のみ」と正確化（現状の「ロックは即解放される」は is_dir 込みで不正確 — issue 指摘）
- `resolve_all_openers`（285-290）: 同じ順序変更
- `launch_item` 内の行内コメント（launch.rs:200「ロック取得 → 即解放」）: 修正後は正確になるが、241 行の doc コメントと文言を揃えて一瞥する（Explore 監査の軽微指摘 A）

## 実装順序

単一フェーズ（2 関数の同型 2 行移動 + コメント修正）。

## 不変条件

- **ロック内は純 CPU のみ**（`find_matching_tools` + 小文字列 clone）— 本変更が確立する新しい不変条件。doc コメントに明記する
- `find_matching_tools(path, is_folder, openers)` の入力意味論は不変（is_dir の評価タイミングがロック取得前に移るだけ。TOCTOU 窓は既存の is_dir→ShellExecuteW 間の窓と同種で悪化なし）
- 失敗モード: `is_dir()` は存在しないパスで false を返すのみ（panic しない）。順序変更で新たな失敗経路は生じない
- 対称ペア: `resolve_opener` / `resolve_all_openers` は同型ペア — **両方同時に**変更する（片方だけの適用漏れが本 issue の再発形）

## テスト方針

- 新規ユニットテストなし: 2 関数は `AppState`（tauri managed state 前提）依存で、src-tauri の Win32/state 依存コードはユニットテスト前提にしない（`.claude/rules/src-tauri.md`）。順序はコードレビューで構造的に検証可能
- 検証: PostToolUse hook の clippy + 既存テスト（カテゴリ A）。`find_matching_tools` 自体の挙動は snotra-core 側の既存テストが保持

## SPEC.md 更新要否

不要（外部挙動・IPC 契約に変更なし。ロック保持時間の内部改善）。

## plan-review 結果の反映（Step 5a）

- **独立導出との差分（HOW の不一致 1 件・解決済み)**: 独立導出は「openers clone（snapshot）→ ロック外で is_dir + find_matching_tools」を提案。本計画は「is_dir をロック前へ移動、find_matching_tools（純関数・opener.rs で確認済み）はロック内に残す」。どちらもロック保持中の FS I/O を排除するが、本計画は clone も import 追加も不要で厳密に小さい。is_dir は engine 状態に依存しないため snapshot の必然性がなく、本計画を維持する
- **一致（完全性の能動的証拠）**: 変更対象 2 箇所・呼び出し元 3 経路・「src-tauri の engine guard 内 FS I/O は他にない」（独立導出は engine.lock() 全 20 箇所を監査）・履歴保存（record_and_save）は別カテゴリで #526 スコープ・SPEC 更新不要 — すべて独立に再一致
- **残存する既知の限界（独立導出の指摘を採録）**: 修正後も is_dir 自体の最大 21 秒ブロックは呼び出しスレッドに残る。特に `resolve_all_openers` はトレイの Win32 メッセージループスレッドで走るため、トレイメニュー構築は依然固まりうる。engine ロックの道連れ（全機能フリーズ）だけが本 issue のスコープ。この残余は resolve_opener の doc コメントに一文残す

## セルフレビュー

1. 対称コードパス: `resolve_opener`/`resolve_all_openers` の 2 箇所を grep で網羅確認済み（`src-tauri/src` に guard 内 `is_dir()` は他になし）
2. 影響範囲: 呼び出し元 3 経路（launch_item / launch_item_with_state / tray.rs:431)は関数シグネチャ不変のため無影響
3. 境界条件: 死んだ UNC（is_dir 長時間ブロック）でもロックを取らずに待つことが本修正の眼目。存在しないパス → false → フォルダ用ルール非マッチ（従来同様）
4. リソース管理: 新規リソースなし
5. 既存パターン整合: FolderListContext の「I/O をロック外へ」原則に合致。snapshot 導入は不要と判断（is_dir は engine 状態に依存しない）
6. YAGNI: openers の clone・世代管理・spawn_blocking 化は導入しない（research.md の残余に記録)
7. シンプル化: issue 記載案（openers clone）より単純な「順序変更のみ」を採用
8. 破壊不変条件: 「ロック内は純 CPU のみ」を doc コメントで明文化。検知手段は将来 guard 内 I/O が再導入されたときのレビュー規範（機構ではない）
