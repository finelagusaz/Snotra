# plan-review — レイヤー C: ガバナンス文書・スキル（#666 段 3）

## 1. 問題なし

1. G1（`src-tauri/CLAUDE.md` モジュール索引の双方向照合）: Phase 4 第 1 項目（`plan.md:82`）で `font.rs` / `launcher_controller.rs` を「モジュール構成」の `egui_shell/` 一覧へ追加すると明記されており、`checkModuleIndex`（`scripts/governance-check.mjs:90-127`）の逆方向照合（実ファイル→索引本文のバッククォート出現）を満たす。
2. G2（`docs/architecture.md` へのファイル単位表の再導入禁止）: 計画は表行を追加せず既存散文の書き換えのみ（`plan.md:84`）。`checkArchitectureTable`（`governance-check.mjs:132-143`）は発火しない。
3. G7（`.claude/rules/*.md` の paths glob）: `.claude/skills/**` 変更が `safety-nets.md:8` の paths に該当し、計画は `/norm-review` を明示的に起動している（`plan.md:87`）。
4. G10（恒久規範の面積 ratchet）: 対象はルート直下 `CLAUDE.md`/`AGENTS.md`（`governance-check.mjs:542`）のみで、計画が触るのは `src-tauri/CLAUDE.md`（クレート固有・非対象）。state-check の `description` フィールド（frontmatter）は変更対象外（`plan.md:86` は本文 L40 のみ）ゆえ skill description 課税面（`governance-check.mjs:624-652`）にも影響しない。
5. `src-tauri/CLAUDE.md`「フォント登録」節の指し先差し替え（`view.rs` → `font.rs`、`plan.md:83`）は Phase 1 で `font.rs` に該当テスト 7 件 + `//!` を先に作ってから Phase 4 で実行される順序（`plan.md:40-50` → `82-90`）ゆえ、差し替え後の指し先は実在するようになる。
6. SPEC.md 更新不要の判定（`plan.md:164`）は実測と一致。`SPEC.md:92,420` の `egui_shell` 言及 2 箇所はいずれも `icon_textures.rs` と `create` で、`view.rs`/`SearchWindowView` への言及はゼロ（grep 実測）。
7. `.claude/skills/state-check/SKILL.md` の対象行（`SKILL.md:40`）は Step 1 の「対象コードを読む」ロケータ文であり、判定ロジック本体（Step 2〜5・`SKILL.md:42-98`）は同ファイル中に一切 `view.rs` を再言及しない。計画の「判定ロジック・チェック項目は 1 文字も変えない」（`plan.md:86`）は構造的に成立する。
8. 新設設計書 `docs/superpowers/specs/2026-07-27-666-...-design.md` は `governanceDocs()`（`governance-check.mjs:799-808`）・`headingRefDocs()`（`governance-check.mjs:815-819`）の双方で `docs/superpowers/` 前置を明示除外しており、G3/G4/G11 いずれの対象にも入らない（#589 で非規範化済みの前提どおり）。

## 2. 軽微な懸念

1. `/norm-review` の上限巡数を「1 巡」にしている（`plan.md:87`）が、`SKILL.md:21`「2 巡が既定」からの逸脱である。理由（「変更は事実の索引であって判定を足さないため」）は書かれており規約違反ではないが、規定の default からの明示的な引き下げとして実装時に再確認する価値がある。
2. `docs/architecture.md` のうち L156・L174 は mermaid フェンス内（`linesOutsideFences` により G3/G2 の走査対象外・`governance-check.mjs:59-71`）にあり、CI では検知されない。L156「`participant View as egui_shell/view.rs (main)`」は分割後、そのシーケンス内の `View->>Eng: engine.search` 等（実質は controller が担う `run_search` 系）を 1 参加者名で束ねたままになる。計画の Phase 4「4 箇所を実態へ」（`plan.md:84`）はこの mermaid 内の責務境界の書き分けまで指定していない。
3. `egui_shell/mod.rs` の「view.rs が消費する」系コメントは、計画が名指しする `L13–16`・`L57–65`（`plan.md:62`）の外にも複数ある。実測（`mod.rs:17-19` の `UpdaterPhase`/`ToastKind`/`UpdaterUi`、`mod.rs:67` の `ui_strings`、`mod.rs:94` の `EguiShellState.reset_pending` 実装コメント「view が消費して state.reset()」、`mod.rs:108-109` の `pending_hotkey_failure` コメント「view が消費時に lang() live-read」）はいずれも Phase 2 で controller 側メソッド（`consume_reset_pending` / hotkey 失敗消費 / `handle_toast_action`）へ移る事実の消費者名を含み、計画が引用する 2 レンジには入っていない。plan.md:16 の「re-export コメントの消費者名を更新」は範囲として広すぎる/狭すぎるどちらの読み方も可能で、実装時に取りこぼしうる。

## 3. 要対処

1. 設計書 §1.1「全 68 項目・例外ゼロ」（`design.md:34`）の内訳が合計 68 にならない。`inherent メソッド 25`（`design.md:14`）に対し、§1.1 の分類は `launcher_controller.rs` メソッド 23（`design.md:39`）+ `view.rs` メソッド 1（`window_width`・`design.md:48`）= 24 で、`new` がどちらの列挙にも現れない。`plan.md:61`（Phase 2）は `SearchWindowView::new` を `LauncherController::new` を包む形で残すと運用上は決めているが、これは設計書 §1.1 の分類表自体には反映されておらず、「規則 R(段 3) を字義通り適用して 68 項目を割り切った」という主張の検算が 1 項目分成立しない。設計書 §1.1 のメソッド列挙に `new` の帰属（またはメソッド 1→2 への分裂の明記）を足すべき。
2. 規則 R(段 3) の 3 条項（`design.md:24-30`）には ADR-0008 の「移設する関数がその中でしか使わないヘルパーは一緒に運ぶ」に相当する条項が無い。にもかかわらず `font.rs` へは `configure_japanese_font`（唯一の外部複数消費者を持つ項目・規則 3 が発火する「1 件」）だけでなく、内部専用ヘルパー 8 件（`JP_FONT_BYTES`/`CJK_PROBE`/`font_covers_cjk`/`font_definitions`/`ResolvedFont`/`USER_FONTS`/`resolve_font_family`/`jp_font_bytes`）も一括で移す（`design.md:52-54`、`plan.md:12`）。この 8 件は単独では規則 1（層の不変条件）に従えば「1 フレームの pass への入出力」＝描画面層＝`view.rs` に属するはずで、規則 3（外部消費者不到達）にも規則 2（両層消費）にも文字通りは該当しない。設計書冒頭の「ADR-0008 規則 R の『複数のモジュールから消費されるものは残す』と同じ論法」（`design.md:32`）という説明はこの 8 件の移設理由にはならない（この文言はモジュール自体の独立新設理由であって、ヘルパーの同伴理由ではない）。**規則 R(段 3) に「ヘルパー同伴」条項を明記するか、この 8 件の分類根拠を §1.1 に個別追記すべき**——「例外ゼロ」を謳う規則の検算対象として看過できない。
3. 計画は ADR に一切言及していない（`plan.md` 全文に "ADR" の記載なし）。段 1（#749）は同種の「規則を 1 本立てて例外ゼロで説明する」判断を `docs/adr/0008-window-coordinator-split-rule.md` という独立 ADR に残した。段 3 は設計書自身が「母集団が桁違いに大きい」（`design.md:22`）と明言し、却下案も 8 件（§3.1–3.8）と段 1（却下 8 件・ADR-0008 全体）より多いにもかかわらず、この否定の知識は非規範化された `docs/superpowers/specs/` にしか残らない（governance-check の G3/G4/G11 の対象外・上記「問題なし」8）。`AGENTS.md`「意思決定記録（否定の知識）」の基準（否定の知識が生じた決定は ADR）に照らし、段 1 より対象が大きい段 3 で ADR を作らない判断が計画に明記されていない——**意図的な後回し（例: 段 3 完了後にまとめて 1 本書く）か、単純な記載漏れかを計画へ追記させるべき**。

## 4. 未検証（理由）

1. `cargo doc --workspace --no-deps --document-private-items` を実際に実行した際の intra-doc link 切れの実測は行っていない（本レビューはガバナンス文書・スキル層に限定されており、Rust ビルド検証は他レイヤーの担当）。
2. `egui_shell/mod.rs` 全体（120 行超）のうち、今回読んだ範囲（L1–120）を超える箇所に追加の「view.rs が消費する」コメントが残っていないかは未確認（ファイル残り部分・`results_view.rs`/`window_coordinator.rs`/`results_window.rs`/`visual.rs`/`commands/launch.rs`/`commands/instant.rs` の該当行そのものの文面は `plan.md` の記述と `grep` の一致のみ確認し、各ファイルを個別に開いての前後文脈確認はしていない）。
3. `.claude/skills/state-check/SKILL.md` 以外の skill（`cache-check`/`race-check`/`symmetric-check` 等）が `view.rs` を名指ししていないかの網羅的 grep は行っていない（`state-check` 以外は本タスクの対象外と判断したため）。
