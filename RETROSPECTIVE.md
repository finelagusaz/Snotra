# Retrospective — `Config::load` データ損失の根治（#338）→ 復旧通知＋保存ガード（#343）

## よかったこと

### アドバーサリアルレビューが「同一作者のチェックの死角」を突いた
`/plan-review`・`/symmetric-check`・`code-reviewer` を通した #338 に対し、Codex アドバーサリアルレビュー（3周）が**自分のチェックでは出なかった実バグ**を検出した: (a) parse 失敗 arm だけ直して**兄弟 arm（read 失敗の `Err(_)` 一括 first-run 扱い）に同じ default 上書きが残っていた**、(b) `InvalidData`（非 UTF-8）を canonical path に残すと**後続 save で破損元が失われる非対称**、(c) 一時的 read 失敗後の後続 save での上書き（finding C）。データ損失系の変更では独立した批判的視点が効くことを実証。

### レビュー指摘を「事実確認」で取捨選択した
finding A（「Windows では既存 `.bak` への `std::fs::rename` が失敗する」）を鵜呑みにせず、`backup_invalid_overwrites_existing_bak` を**実機で実行して green を確認**＝反証（`MoveFileExW(REPLACE_EXISTING)` で置換）。逆に finding 2/B は採用。`receiving-code-review` どおり「盲従も性急な反論もせず実証で判断」できた。

### スコープ判断：同一不変条件は今ここ、別不変条件は follow-up
read 失敗の NotFound 分離は「`load` がユーザー設定を壊さない」という #338 と同じ不変条件なので #338 内で対処。finding C（save 側の別不変条件）と通知（UX）は `load_reporting` という同一 seam に乗るため **#343 に集約**。`size:S` を保ちつつ筋を通した。

### 注入 seam とフェーズ TDD が効いた
#338 で作った `load_from_dir`/`save_to_dir`（`config_dir` 注入）seam が #343 の `load_reporting` の土台になり、`config_path` 固定でも全分岐を統合テストできた。各フェーズで Red→Green（invalid-UTF-8 の default 上書きを Red で捕捉等）を回し、層分離（`LoadOutcome` は UI 文字列を持たない plain enum）も保てた。

---

## 伸びしろ

### 「兄弟分岐の同型バグ」を最初の実装で見落とした
#338 初版は parse 失敗 arm のみ直し、隣の read 失敗 arm に同じ破壊的 default 上書きを残した。「破壊的フォールバックを1分岐で直したら同じ `match` の全分岐の保全方針を揃える」を `snotra-core/CLAUDE.md` の読み込み失敗節に反映済み。

### ビルド済みバイナリ smoke でタイムスタンプに惑わされた
balloon smoke で `target/debug/snotra.exe`(00:48) が #343 を含むのに「stale では」と判断を往復した（cargo が deps バイナリを hardlink し直すためタイムスタンプが更新されない）。最終的に**変更固有文字列でバイナリを grep**して #343 入りを確定。教訓を `docs/build-commands.md` の smoke 節に反映済み。

### ワークフロー設定の潜在的デッドステップ
`/start-issue` Step 5a が `/plan-review` の起動を指示するのに、当該スキルが `disable-model-invocation` でモデルから呼べず、ワークフローが機能しない不整合があった（#342 で plan-review を起動可化して解消）。「他スキルの手順から呼ばれるスキルはモデル起動可でなければならない」。

### 引き継ぎバッファの無言欠落
`.gitignore` の bare `research.md`（全階層一致）が `workspace/research.md` を無視し、start-issue の handoff から静かに脱落していた（#344 で削除され untracked 化して初めて顕在化）。ルールを撤去して追跡対象化済み。

---

## ネクストアクション

- [ ] PR #345（#343 復旧通知＋保存ガード）をマージ（CI green 済み）
- [ ] `/health-check` でモジュール構成・SPEC.md 番号・メモリ・スキル・ルールの整合を確認（サイクル末）
- [ ] （任意）復旧バルーンを通知センターに永続化したい場合は WinRT/Tauri トースト API への置換を follow-up issue 化（#343 スコープ外。現状は `.bak`＋stderr が永続痕跡）
