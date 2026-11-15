# 計画: #1014 — `drain_index` の `begin_in` → `rebuild_and_save` の順序をソーステキスト検査で固定する

調査は `workspace/research.md`。バグ/仕様変更の判定: **どちらでもない（検査の追加）**。製品の挙動・`SPEC.md` の記述は変えない。

## 目的と受け入れ条件

`scan_log.rs` の `//!`「2 行 1 組」が定める「走査の**前**に `start` 行」を、`drain_index` の呼び出し順として CI で固定する。

1. `src-tauri/src/indexing.rs` の `mod tests` に順序検査 `drain_index_appends_start_before_the_scan` が在り、素の実装で緑
2. 順序を壊す変異 M1（research.md「実測」）で、**その検査が assert で**赤（コンパイルエラーではない）。既存の他テストは緑のまま（= 単独で検出する）
3. 2 を **LF と CRLF の両方**の `indexing.rs` で実測し、素の実装の CRLF でも緑であること（CI の windows-latest は CRLF で checkout しうる）
4. 行末コメントに綴りを残す変異 M2 でも赤（前処理が効いていることの実測）
5. 母集団が見えているかを壊す変異 M3（アンカー `fn drain_index(` を改名する）で、helper の assert により赤
6. CI（本 PR）で緑。CI の赤と連続 N 回は要求しない（ユーザー裁定・research.md「要求の裁定」）。その理由を PR 本文に書く

## 変更するファイル

| ファイル | 対象 | 変更 |
|---|---|---|
| `src-tauri/src/indexing.rs` | `mod tests` | テスト `drain_index_appends_start_before_the_scan` を 1 本足す。既存の `top_level_fn_body` を使う（変更しない） |

`SPEC.md`・`docs/`・`scan_log.rs` の `//!`・`src-tauri/CLAUDE.md`：変更不要（挙動を変えない・ファイルを足さない・`//!` は不変条件を述べるだけで検知器の所在を書いていない）。

## 検査の仕様

```rust
#[test]
fn drain_index_appends_start_before_the_scan() {
    let body = top_level_fn_body(include_str!("indexing.rs"), "fn drain_index(", "begin_index_drain(");
    // 各行の `//` 以降を落とす（行頭コメント・行末コメントの両方）。drain_index 本体に文字列リテラルは無い
    let code: String = body.lines().map(|l| l.split_once("//").map_or(l, |(c, _)| c)).collect::<Vec<_>>().join("\n");
    let begin = "scan_log::begin_in(";
    let scan = "indexer::rebuild_and_save(";
    assert_eq!(code.matches(begin).count(), 1, …);
    assert_eq!(code.matches(scan).count(), 1, …);
    assert!(code.find(begin) < code.find(scan), "…走査の後で start 行を書いている…");
}
```

- assert メッセージは「何が壊れたか」と「`scan_log` の `//!`『2 行 1 組』が正本」を名指す
- doc（`///`）に書くこと（`docs/comment-guidelines.md`「テストコードのコメント」に従う。数え上げは書かない）:
  - 守るもの: `start` 行が走査の前に書かれること。既存スイートはこの順序を縛っていなかった（#1014 で M1 が素通し）
  - **縛るのはテキストの順序であって実行順ではない**。受容残余: ブロックコメント・文字列に綴りを残し実呼び出しを別名で後ろへ置く／走査の前にクロージャを作って後で呼ぶ／死んだ分岐・`cfg` で無効化する、のような**検査を欺く書き換え**は通す。偶発的な並べ替え・本体外への移設（綴り 0 回）・重複は赤
  - 「ちょうど 1 回」は位置比較の述語を母集団の広がり・狭まりに対して赤へ倒すため（`ADR-source-text-probe-predicate-polarity`）
  - 母集団の壊れ方は `top_level_fn_body` が塞ぐ（正本はその doc。写さない）

## 実装の順序

1. 変異前の基準件数を取る（`cargo test -p snotra -q`）
2. 検査を足す → 素の実装で緑（製品コードは既に正しいので、Red は手順 3 の M1 で見る）
3. 変異 M1・M2・M3 を当てて測り、戻す（下「テスト方針」）
4. CRLF で素の実装・M1 を測る
5. カテゴリ A の検証

## 不変条件と異常系

- 製品コード（`drain_index` 本体）を 1 バイトも変えない——`git diff --stat` で `indexing.rs` の差分が `mod tests` 内だけであること
- 既存の `top_level_fn_body` とその回帰テスト 4 本を変えない（`ADR-source-text-probe-helper-locality` の爆風の論点）
- 変異は作業ツリーに残さない（各測定後に `git diff` で検査の追加だけが残ることを確かめる）

## テスト方針と検証コマンド

変異の注入は `/implement`「3b. 委譲へ渡すもの」の委譲先が行う（主は同じ worktree へ注入しない）。

| 変異 | 内容 | 期待 |
|---|---|---|
| M1 | `let log = …config_dir().map(…begin_in…)` ブロックを `rebuild_and_save(…);` の直後へ移し、`begin_scan_progress(…)` を `begin_scan_progress(None)` にする（research.md の R0 と同じ形） | 新検査だけが assert で赤 |
| M2 | M1 に加え、`rebuild_and_save` より前の行の行末に `// scan_log::begin_in(` を書き足す | 新検査が赤（行末コメント除去が効く） |
| M3 | `fn drain_index(` を `fn drain_index_x(` に改名（呼び出し側も） | 新検査が helper の「見つからない」で赤 |
| CRLF | 素の実装 + 検査、M1 + 検査を、それぞれ `indexing.rs` を CRLF 化して | 前者緑・後者赤 |

赤の理由が assert かコンパイラかを毎回出力で確かめる。

検証（`docs/build-commands.md`「変更後の検証チェックリスト」カテゴリ A のうち該当分。コマンドはそこに従う）: `cargo fmt --check`・`cargo clippy --workspace --all-targets -- -D warnings`・`cargo test -p snotra`。`#[cfg(test)]` 下の doc は `cargo doc` で検査されない（`.claude/rules/comments.md`）ので、doc 内の intra-doc link は `[`top_level_fn_body`]` のような同モジュール内の既存の形に限る。

## フェーズと作業項目

### Phase 1 — 検査の追加

- [ ] 変異前の基準件数を記録する
- [ ] `drain_index_appends_start_before_the_scan` を足し、素の実装で緑を確認する

### Phase 2 — 発火の実測

- [ ] M1・M2・M3 を測り、結果（赤の理由を含む）を PR 本文用に記録する
- [ ] CRLF の素の実装と M1 を測る
- [ ] 作業ツリーに変異が残っていないことを確かめる

### Phase 3 — 検証

- [ ] カテゴリ A の該当コマンドを走らせる

## 未確定（実装前に潰す）

- [x] poll 型（issue 本文の設計）を採るか — 不採用。`mock_app` で組めるが `drain_index` の `R: Runtime` 一般化・dev-dep・`SNOTRA_CONFIG_DIR` の並列衝突対策が要り、なお時間競合が残る（research.md 制約 1・3b 命題 1）
- [x] 候補 B（反復の切り出し＋注入）を採るか — 不採用。製品コードを再構成するうえ、`drain_index` が切り出しを迂回する経路を捕まえない。A は M1 を assert で赤にすることを試作で実測済み（R2・R3）
- [x] CI の赤・連続 N 回 — ユーザー裁定で CRLF ローカル代替・課さない
- [x] `//` 除去の範囲 — 各行の `//` 以降を落とす。本体に文字列リテラルが無いことを grep で確認（`drain_index` 本体に `"` 0 件）
- [x] `begin_scan_progress` の順序も縛るか — 範囲外（issue の問いではない）。M1 に含まれる差し替えはコンパイル上の強制

## 人間レビュー

- [x] 承認済み — 2026-11-15 / 問い: "workspace/plan.md の計画を承認しますか？" / 回答: "承認する"

## セルフレビュー

- リスク: 通常（テスト 1 本の追加。永続形式・並行性・状態遷移・ガバナンス文書・インターフェースに触れない）
- plan-review: 未実施（通常リスク）
- エージェント数: 1（3b の敵対的調査のみ）
- check スキル: 該当なし（対称ペア・UI モード・永続形式・網羅性要件・ガバナンス文書のいずれにも当たらない。新規はテスト関数 1 本で、同等ロジックの重複は既存 helper を使うことで避けている）
- 要対処: 0 件（5a 照合: 要件 1〜3 ↔ 受け入れ 1〜6 対応／境界は LF・CRLF・行末コメント・母集団喪失に各 1 変異／新しい状態・リソースなし／より単純な既存パターン = 同ファイルの既存ソーステキスト検査そのもの／不変条件「製品コード不変」は `git diff` で検知）
- 未検証: CRLF の素の実装での緑（Phase 2 で測る）
- 散文の全件列挙: 未実施（通常リスク）
