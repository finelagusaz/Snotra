# governance-docs レビュー（ラウンド 3・最終）

対象: `SPEC.md`（§6.7 新設・§4.7 参照追加）・検証手順全般

## 問題なし

- **フェーズ番号の繰り上げに壊れた相互参照は無い**。`plan.md` 中の「フェーズ N」出現を全数 grep（`フェーズ 1`〜`フェーズ 5`・計 7 箇所）。`plan.md:133` の「フェーズ 4 のとおり」は §4「SPEC 同期」フェーズを正しく指し、`plan.md:143` の「フェーズ 2 のコードコメント」は §2「表示（view.rs）」を正しく指す。旧フェーズ 6（検証）を指す取り残しは無い
- **§6.7 追記は既存の序数参照を壊さない**。§6 末尾（§6.6 の直後）への追記であり、`docs/adr/ADR-folder-nav-selection-first-row.md:13,25`（§6.1）・`docs/superpowers/plans/2026-07-23-su3-m2-folder.md:224,259,697,976`（§6.1/§6.3/§6.6）・`docs/superpowers/specs/2026-07-22-su3-search-experience-design.md:102,121,132,133,157`（§6.1/§6.3/§6.4/§6.6）を実際に grep して、既存番号が 1 つもずれないことを確認した。`scripts/governance-check.mjs:206-250`（G-spec-sections）の子セクション連続性チェック（`n.(prevSub+1)`）も §6.6→§6.7 で満たす
- **カテゴリ F（`npm run governance:check`）の要否判定は正しい**。`SPEC.md` は `docs/build-commands.md` カテゴリ F の対象（`*.md`）に該当し、PostToolUse hook は `.md` へ検査を割り当てないため沈黙は「合格」ではなく「何も走っていない」（同ファイル線 124）。また `SPEC.md` は `scripts/governance-check.mjs:677` の `ALWAYS_LOADED_FILES = ["CLAUDE.md", "AGENTS.md"]` に含まれないため、G-area-budget（恒久規範の面積 ratchet）の対象外であることも確認した——§6.7 追記が面積超過を誘発する心配は無い
- **§8.6・§11・§15.4 は変更不要という判断は実読で裏付けられる**。§8.6（状態遷移図）はモード集合・ガードのみを記述し hint の文言や表示条件には触れない。§11 as-built（`SPEC.md:586-595`）は hint の**色の受け取り機構**（`Visuals::weak_text_color`）と**寸法**を記述するだけで、既存の `tool_select_hint` の文言内容すら列挙していない（実読で確認）。§15.4（フォルダ展開中はスラッシュコマンドを無視）は本変更と無関係。§18.5（`SPEC.md:750-756`）の優先度 `toolSelectionState !== null > folderState !== null > 通常モード` は plan.md §6.7(b) の「tool > folder > results」主張と一対一で一致する
- **`SPEC.md` はセーフティネットの `paths` 対象外**。`.claude/rules/safety-nets.md` の `paths`（`.claude/hooks/**`・`.githooks/**`・`.claude/settings.json`・`.github/workflows/**`・`.claude/rules/**`・`.claude/skills/**`・`scripts/*.mjs`）に `SPEC.md` は含まれないため、safety-nets 手順（フォールトインジェクション等）を計画が挙げていないのは妥当

## 軽微な懸念

- なし

## 要対処

- **フェーズ 5「カテゴリ D」の記述が、撤回済みの旧計画（`scripts/manual-smoke.ps1` へ項目追加する版）の言い回しを引きずっている（`plan.md:92`）**。
  > カテゴリ D（人間が実施・エージェントは実行できない）: `cargo run -p snotra` で下の目視項目を確認し、`npm run smoke:manual -- -PostToPr` か出力の貼り付けで PR へ残す

  「下の目視項目」は文脈上「テスト方針」節の「**カテゴリ D の目視項目**」（`plan.md:117-129`・#836 専用の 11 項目）を指す。しかし `npm run smoke:manual -- -PostToPr` が読み書きするのは `scripts/manual-smoke.ps1:6,38-41` の `$items`（固定 13 項目・ラウンド2の YAGNI 裁定により未変更のまま）だけであり、この 11 項目を実行することも記録することも構造的にできない（`$items` に無い項目は `-Only` でも選べない。実読で確認）。
  一方 `plan.md:117` は「PR 本文の目視表へ書く」と明記しており、記録手段が手動転記であることは他所に書かれている。だが**フェーズ 5 の実行チェックリスト**（`gh pr create` 前に埋める、実質のゲートになる箇所）はこの手動転記を指示せず、`smoke:manual -PostToPr` を挙げるだけなので、実装者がフェーズ 5 だけを辿ると「標準 13 項目の smoke を回して post すれば #836 の目視も完了した」と誤認しうる。その場合 AC1（この issue の核）の**唯一の検知手段**（`plan.md:114`「実際に描かれること…カテゴリ D 目視。これが唯一の検知手段である」）が PR に一切残らないまま `gh pr create` を通過できる。
  **対処案**: `plan.md:92` を「`cargo run -p snotra` で下の 11 項目を確認し、結果を PR 本文の目視表へ手で書く（`smoke:manual` の対象外・別枠）。あわせて通常の `npm run smoke:manual -- -PostToPr`（標準 13 項目）も実施する」のように 2 つの記録経路を明示的に分離する。

## 未検証

- `docs/architecture.md` / `src-tauri/CLAUDE.md` のモジュール構成節が実際に `strings.rs` の責務宣言（「UI 文言テーブル」）と一致しているかは実読していない（「触らない」節の主張の裏取りは他レイヤーのスコープと判断した）
- コード側フェーズ（1〜3・`strings.rs` / `view.rs` / `search_state.rs`）の実装詳細・不変条件の正しさは governance-docs の担当外のため評価していない
- 計画本文の「肥大」評価は SPEC.md・検証手順に関わる記述に絞った。フェーズ 1〜3 のコードコメント量やコード側の未確定事項の要否は対象外とした
