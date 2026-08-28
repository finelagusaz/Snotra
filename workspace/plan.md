# 実装計画: issue #988 — `//` の intra-doc link 形の triage

## 目的

issue #988 を、現在の正本（#1176 が定めた「名指しと正本の指名」）のもとで再判定し、**残った実作業だけを当てる**。

処遇はユーザーが 2026-08-28 に決めた——**「(b) の triage だけへ縮める」**。issue が課した「(a) 素のバッククォートへ落とす」「20 件を 0 件にする」「その後に検知器の採否を決める」は、いずれも #1176 が前提を反転させたため実行しない。

**この計画は件数を減らすことを約束しない。** 修正後も 2 件は `` [`X`] `` の形のまま（完全修飾になるだけ）であり、issue の grep 述語には引き続き一致する。成果物は下の triage 表と、着地しない 2 件の修正である。

## 受け入れ条件

1. 20 件すべてについて (b) の該当可否と理由が表として記録されている
2. 着地しない 2 件が、完全修飾パスで着地する形になっている
3. `cargo doc --workspace --no-deps --document-private-items` が exit 0（現状維持の確認）
4. issue の (b) 予想と食い違う 1 箇所が、ユーザーの裁定を受けている

## triage の判別式

`docs/comment-guidelines.md`「配置基準（3 層）」が `//` の面で禁じるのは**保証の要る参照**である。「保証が要る」を実際に判定できる形へ落とす。

**その名指しが隣接する実行コードにも現れるなら、改名はコンパイラが隣の行を直す。** 散文にしか無い名指しだけが、黙って浮く。これが「保証が要るか」の実体である。

この判別式は実データで裏づけられた——**着地しない 2 件は、隣接コードのアンカーが無く、かつモジュールを跨ぐ 2 件と完全に一致する。** 同ファイル・同モジュールの散文専用アンカー（`Timeline::mark` など）は着地しており、跨ぐものだけが浮いていた。

## triage の結果（20 件）

`#[cfg(test)]` 側の 5 件は**原理的に (b) の対象外**である。「名指しと正本の指名」の表が `#[cfg(test)]` の `///` を「着地を検査する: ×」と宣言しており、doc コメントへ移しても保証を 1 つも買えない。

| 位置 | 参照 | 隣接コードのアンカー | 着地 | (b) 該当 |
|---|---|---|---|---|
| `snotra-core/src/binfmt.rs:174` | `crate::indexer::index_built_at_in` | 無し（跨ぐ） | ○ | 否——既に完全修飾で、浮いても綴りが自明 |
| `snotra-core/src/indexer/cache.rs:634` | `DerivedColumns::into_cached_masks` | method 名のみ（型名は散文専用・跨ぐ） | **×** | **修正対象** |
| `snotra-core/src/indexer/columns.rs:125` | `CachedLower` | 有り（`CachedLower::Collapsed`） | ○ | 否 |
| `snotra-core/src/indexer/columns.rs:137` | `IndexMaterial` | 無し（跨ぐ） | **×** | **修正対象** |
| `snotra-core/src/indexer/columns.rs:139` | `derive_entry_collapsed` | 有り（同関数内で呼ぶ） | ○ | 否（**issue の予想と食い違う**・下記） |
| `snotra-core/src/indexer/columns.rs:236` | `derive_entry_lowers` | 有り（直下で呼ぶ） | ○ | 否（同上） |
| `snotra-core/src/indexer/columns.rs:237` | `derive_entry_collapsed` | 有り（直下で呼ぶ） | ○ | 否（同上） |
| `snotra-core/src/indexer/scan.rs:118` | `root_roles` | 有り（直前で呼ぶ） | ○ | 否 |
| `snotra-core/src/index_tree.rs:262` | `capped_capacity` | 有り（直下で呼ぶ） | ○ | 否 |
| `snotra-core/src/index_tree.rs:695` | `CHAIN_CAP` | 有り（9 行下で使う） | ○ | 否 |
| `snotra-core/src/search/build.rs:473` | `KANA_CHUNK` | 有り（直下で使う） | ○ | 否 |
| `snotra-core/src/search/build.rs:534` | `wave1_from_tree` | 有り（直下で呼ぶ） | ○ | 否 |
| `snotra-core/src/search/path_store.rs:277` | `Self::sorted_prefix_len` | 有り（7 行下で読む） | ○ | 否 ⚠️ |
| `snotra-core/src/search/scoring.rs:287` | `PathStore` | 無し（同モジュール・`use` 済み） | ○ | 否 |
| `src-tauri/src/startup.rs:357` | `Timeline::mark` | 無し（同ファイル） | ○ | 否 |
| `src-tauri/.../activation/tests.rs:251` | `method_header` | 有り | ○ | **対象外**（`#[cfg(test)]`） |
| `src-tauri/.../activation/tests.rs:710` | `sources` | 有り | ○ | **対象外**（同上） |
| `src-tauri/.../activation/tests.rs:713` | `sole_file_with` | 有り | ○ | **対象外**（同上） |
| `src-tauri/.../activation/tests.rs:719` | `method_header` | 有り | ○ | **対象外**（同上） |
| `src-tauri/src/egui_shell/view.rs:1357` | `assert_read_once_in_this_file` | 有り | ○ | **対象外**（テストモジュール内） |

⚠️ `path_store.rs:277` の `Self::sorted_prefix_len` は field（124 行）と `pub(super) fn`（308 行）が同名である。rustdoc が曖昧さを警告する形かどうかは測っていない。(b) 非該当なので doc へ移さず、この曖昧さは顕在化しない。

### issue の予想と食い違う 1 箇所（ユーザーの裁定を仰ぐ）

issue 本文は次を (b) の候補として名指している。

> **(b) が正しい箇所が混ざっているはずである**（例: `indexer.rs:813` と `:1626-1627` は「per-entry の導出は 1 か所を通ること」という**崩れたら潰れ方がずれる不変条件**を運んでおり、局所的な why ではない）

現在の `indexer/columns.rs:139` と `:236-237` である。**この triage はいずれも (b) 非該当と判定した。** 理由は 2 つ。

1. **不変条件はコードが固定している。** 両側とも同じコメントの数行下で当の関数を実際に呼んでおり、改名すればコンパイラが隣接行を直す。散文だけが浮く形にならない
2. **コメントは関数本体の途中にあり、その位置でしか意味を持たない**（「ここに列ごとの別実装を書き起こしてはならない」）。`///` へ移すと、禁止が掛かる場所から離れる

`extend_cached_masks` の `///`（233〜236 行）は既に契約側を記述している。不変条件を `derive_entry_collapsed` の `///` へ一意性の宣言として足すことは可能だが、`//` の行が禁じられているわけではない以上、**必須ではない**。

## 変更ファイル一覧と対象シンボル

| ファイル | 対象 | 変更 |
|---|---|---|
| `snotra-core/src/indexer/columns.rs` | `derive_columns` 冒頭の `//`（137 行付近） | `` [`IndexMaterial`] `` → `` [`crate::indexer::IndexMaterial`] `` |
| `snotra-core/src/indexer/cache.rs` | `save_cache_sorted` 末尾付近の `//`（634 行付近） | `` [`DerivedColumns::into_cached_masks`] `` → `` [`crate::indexer::columns::DerivedColumns::into_cached_masks`] `` |

**行番号で辿らない**（`.claude/rules/snotra-core.md`・#588）。シンボル名で grep して位置を確定する。

### 修正形の選択（`use` を足す案は閉じた）

対象は `//` コメントである。コメント本文のためだけに `use` を足すと rustc から見て未使用であり、この workspace は `-D warnings` ゆえビルドが落ちる。ゆえに**完全修飾パスをコメントへ書く**しかない。リポジトリ内の先例は `binfmt.rs:174` の `` [`crate::indexer::index_built_at_in`] `` である。

## 不変条件と異常系

- **件数は減らない。** 修正後も両者はリンク形のままで、issue の grep 述語に一致し続ける。「20 件 → 18 件」を成果として書かない
- **`cargo doc` は変更前も変更後も緑である。** `//` は rustdoc が読まないため、この修正は現在の検査結果を動かさない。買うのは「(b) を後から選んだときに壊れない」ことと「読者が辿れる指名」だけである
- コメントの散文（何を主張しているか）は変えない。変えるのはリンクのパスだけ

## テスト方針と検証コマンド

- `cargo doc --workspace --no-deps --document-private-items` — exit 0（`docs/build-commands.md` カテゴリ A）
- `cargo build --workspace --all-targets` — `-D warnings` 下で通ること
- PostToolUse hook が `.rs` 編集で自動実行する検査は**沈黙 = 合格**
- **修正が効いたことの確認**: 2 箇所を一時的に `///` の位置へ写して `cargo doc` が緑になることを測る（`unresolved link` が出ないこと）。測ったら元へ戻し、`git status` が clean であることを確認する

## `SPEC.md`・関連文書の更新要否

- `SPEC.md`: **不要**。挙動を変えない
- `docs/comment-guidelines.md`: **不要**。この計画は現在の条項に従うだけで、条項を変えない
- `docs/adr/`: 下記の未確定 1 を参照

## フェーズ

### Phase 1 — 着地しない 2 件の修正

- [ ] `snotra-core/src/indexer/columns.rs` の `derive_columns` 冒頭の `` [`IndexMaterial`] `` を完全修飾へ書き換える
- [ ] `snotra-core/src/indexer/cache.rs` の `` [`DerivedColumns::into_cached_masks`] `` を完全修飾へ書き換える
- [ ] 2 箇所を一時的に doc コメントの位置へ写して `cargo doc` が `unresolved link` を出さないことを測り、元へ戻す
- [ ] `cargo doc --workspace --no-deps --document-private-items` exit 0
- [ ] `cargo build --workspace --all-targets` exit 0

### Phase 2 — 記録

- [ ] `workspace/research.md` と本計画の triage 表が、実装後の実際の行内容と一致することを grep で確かめる

## 未確定（実装前に潰す）

- [x] **ADR の陳腐化した一行を、この PR で扱うか。** → **扱わない（何もしない）。** ユーザーは 2026-08-28 に選択肢としては「この issue の射程に含める」を選んだが、その後の調査で選択の前提が崩れたため下の 3 点を提示し直し、推奨（何もしない）を含む計画ごと承認を得た。**`docs/adr/` は変更しない。** 以下は裁定の根拠として残す
  - `docs/adr/ADR-adr-frozen-history.md`「決定」がリポジトリの契約として「歴史は、消えることに対してだけ守り、**変わることに対しては守らない**」を定め、同 ADR 自身が先行 ADR の正当化を覆したときに**編集しなかった**ことを「本契約の初適用である」と宣言している
  - **覆りは既に前向きに記録されている。** `docs/adr/ADR-folded-code-span-detector.md`「却下 3 との関係」が却下 3 と当該の受容する残余を逐語で引き、`ADR-folded-canonical-reference-detector` が却下理由を「陳腐化していた」と宣言している
  - ゆえに古い ADR へ追記すると、`AGENTS.md`「文書に事実の写しを増やす変更」が禁じる**写しになる**
  - **推奨は「何もしない」**（記録は既に正しい場所に在る）。ユーザーが追記を望むなら `ADR-governance-meta-demotion.md` の「訂正（日付・凍結規約により書き換えず追記する）」の形に倣う

## 人間レビュー

- [x] 承認済み — 2026-08-28 / 問い: "`workspace/plan.md` へ注釈を追加していただくか、明示的に承認していただければ実装へ進みます。"（あわせて ADR の推奨「何もしない」と、issue の予想と食い違う `indexer/columns.rs` の (b) 非該当判定を明示して裁定を求めた） / 回答: "OK"

注釈は無し。承認の射程には次の 2 つの明示的な判断が含まれる。

1. **ADR は変更しない**（覆りは `ADR-folded-code-span-detector` に記録済みで、追記は写しになる）
2. **`indexer/columns.rs:139` と `:236-237` は (b) 非該当**（issue 本文の予想と食い違う唯一の箇所）

## セルフレビュー

- リスク: 通常
- plan-review: 未実施（通常リスク）。`AGENTS.md`「条件別チェック」の該当行なし——コメントのリンクパスのみの変更であり、対称ペア・状態遷移・永続形式・関数の新規定義/改名・並行性・網羅性・ガバナンス文書のいずれにも当たらない
- エージェント数: 1（Step 3b の敵対的調査 1 体）
- 要対処: 3b の所見 3 件を裁定し、うち 2 件を採用して `research.md` を訂正（初稿の「腐った参照は 0 件」は誤りだった）、1 件は所見のみ採用して機序を退けた
- 未検証: `path_store.rs:277` の `Self::sorted_prefix_len` における field と method の同名による rustdoc の曖昧さ（(b) 非該当ゆえ顕在化しない）。#1176 が数えた「18 件」と今日の「20 件」の差（処遇に効かない）
