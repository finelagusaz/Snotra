## 問題なし

- **フェーズ 2 コードコメントの訂正文は事実と一致する。** `view_kind()`（`search_state.rs:209-217`）は `tool.is_some() → Tool` を最優先で見るため、`view_kind()==Folder ⟹ folder.is_some()` は成り立つが逆（`folder.is_some() ⟹ Folder`）は成り立たない。`enter_tool`（同 290-312）は `self.folder` に一切触れずフレームを残す。既存テスト `escape_ladder_tool_then_folder_then_hide`（同 909-934）は `enter_folder` → `enter_tool` の順で構成し、この時点で `assert_eq!(s.view_kind(), ViewKind::Tool)` を通しており、tool-on-folder（`folder.is_some() && view_kind()==Tool`）を実際に構成している。訂正文の 3 点（片側のみの含意・`enter_tool` が frame を残す・当該テストがその状態を構成する）はすべて実測どおり。
- **不変条件 3 の行番号・主張は正確。** `launcher_controller.rs:1044-1054`（`→` 分岐: `navigate_folder`/`enter_folder` による同期書き込み → `spawn_folder_load` の非同期呼び出し）、`:1061-1066`（`←` 分岐: `parent_dir()` → `navigate_folder` 同期書き込み → `spawn_folder_load` 非同期呼び出し）を実読して確認。どちらも「`current_dir` は同期で書かれ、列挙は非同期でその後に呼ばれる」という主張と一致する。
- **不変条件 4 は正確。** `on_escape`（`search_state.rs:330-351`）の `RestoredFromTool` 分岐は `self.folder_filter = t.saved_folder_filter` を実行する。`escape_ladder_tool_then_folder_then_hide` テストは `saved_folder_filter="fil"`（非空）での復元を実際に assert（`assert_eq!(s.folder_filter(), "fil")`）しており、非空なら hint が出ない（egui hint_text は buf 空時のみ描画）という帰結と整合する。「特例ではなく不変条件 1 の帰結」という位置づけも妥当。
- **フェーズ 3 の doc 記載内容 (i)(ii) は正確。** (i) 唯一の消費者が `view.rs` になる点はフェーズ 2 の変更内容と整合（現状 `folder_current_dir()` の非テスト呼び出し元は 0 件・grep 実測、フェーズ 2 で 1 件になる）。(ii) `enter_folder`/`navigate_folder` が `current_dir` を同期で書き、`spawn_folder_load` が非同期でその後に呼ばれる点は上記どおり実測一致。
- **doc (iii) の書き方自体は 1 の訂正と矛盾しない。** 「`view_kind() == Folder` のとき必ず `Some`」は真の含意（⟹）のみを主張しており、逆（`Some` のとき必ず `Folder`）を主張していない。round 2 が指摘した「偽の同値」の主張はしていない。
- **`#[allow(dead_code)]` 解除は `-D warnings` を通る見込みが高い。** フェーズ 2 で `view.rs` に `self.controller.state().folder_current_dir()` の実消費者が入るため（`launcher_controller.rs:157` の `pub(super) fn state(&self) -> &SearchState` 経由、既存の `view.rs:341-377` 付近で同型アクセスパターンが既に多数ある）、cfg(test) 以外の呼び出し元が生じ dead_code 警告は解消される。`folder_gen()` 側は grep で非テスト呼び出し元 0 件を確認済み・plan の「allow は外さない」判断と一致。
- **`FolderExpansionState`（`snotra-core/src/ui_types.rs:19`）の扱いは妥当。** `grep -rn FolderExpansionState . --include=*.rs --include=*.md --include=*.toml` は `ui_types.rs:19`（定義）の 1 件のみ（`workspace/plan.md` 自身がこの型名を引用しているため plan.md 側にも文字列としてヒットするが、コードとしての消費者ではない）。pub な lib crate 型のため `dead_code` は発火しない、という plan の説明も一致（`#[derive(..., Serialize, Deserialize)]` の pub struct はクレート内非消費でも lint されない）。撤去を本 issue の範囲外とする判断（送り先をユーザー裁定に委ねる）も #833 と同種の別作業という理由づけと整合し、過剰でも過小でもない。
- **hide→再 show（`reset()`）を跨いだ stale パスの経路は無い。** `consume_reset_pending`（`launcher_controller.rs:832-848`）は `self.state.reset()` を呼び、`reset()`（`search_state.rs:355-364`）は `self.folder = None` にする。この呼び出しは `view.rs` の `update()` 冒頭（hint 算出より前・#749 の順序制約）で毎フレーム行われるため、`folder_current_dir()` は reset 後は必ず `None` に落ち、hint は `search_hint` へフォールバックする。列挙未着・失敗・0 件の状態でも `current_dir` は同期で書かれているため hint は遷移先を示し続ける（stale ではなく「意図した先行表示」）。
- **このレイヤー（search-state-core）の記述量は過剰でない。** フェーズ 3 の変更は `#[allow(dead_code)]` 解除＋ doc 書き換えのみで、新しい状態・関数・テストを追加しない（不変条件 5 のとおり）。既存テスト（`enter_folder_saves_view_and_switches_kind` 等）が既に `folder_current_dir()` を状態側で assert 済みという判断も grep で裏取りできる（`search_state.rs:662,676,700,704`）。

## 軽微な懸念

- **doc (iii) は正しい方向だけを述べるが、round 2 が踏んだ「逆向きの誤読」への明示的な防御をこの accessor 自身の doc には持たせていない。** フェーズ 2 のコードコメント（`view.rs`）は「同値ではなく片側だけ」「tool-on-folder が反例」まで書くが、フェーズ 3 の doc 内容リスト (i)(ii)(iii) には反例への言及が無い。`folder_current_dir()` は `pub` で将来別の消費者が増えうる関数であり、その doc だけを読んだ実装者は「`Some` なら folder 表示中」という逆方向の誤読（まさに round 2 で一度実際に起きた誤り）を再び犯しうる。実装時に (iii) の文へ「逆（`Some` は `Tool` と共存しうる。tool-on-folder 参照）」の一文を足すことを推奨する。

## 要対処

なし

## 未検証

- **`SPEC.md` §6.7 の追記内容・序数参照・§4.7 との整合** — 担当レイヤー外（governance-docs / SPEC スカウトの担当）のため、序数参照先（`docs/adr/ADR-folder-nav-selection-first-row.md` 等の grep 裏取り）は実施していない。
- **`strings.rs` の `folder_hint` 文言・codepoint 一致・`view.rs` の 3 分岐実装そのもの** — 担当レイヤー外（strings/view スコープ）。`search_state.rs`/`launcher_controller.rs` 側から見た整合性のみ確認した。
- **`cargo clippy --workspace --all-targets -- -D warnings` の実行** — 静的読解のみで確認し、実際にビルド・clippy は実行していない（未実装のため実行不能）。dead_code 解除が通る見込みは呼び出し元の実在確認から導いたものであり、コンパイラでの実測ではない。
- **カテゴリ D の目視項目（1〜11）の妥当性** — 実機での確認が前提のため、コードレベルの整合性のみを見ており実際の描画結果は検証していない。
