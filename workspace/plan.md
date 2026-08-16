# 実装計画 — #1106: 連言④（`visible_rows = 0`）で隠れた行の起動を塞ぐ

調査は `workspace/research.md`（一次証拠の表 P1〜P7・敵対的調査の採否）。

## 目的

`appearance.visible_rows = 0` のとき results 窓は 1 行も描かないのに、Enter / Shift+Enter / クリックが
行を起動する。#1077 が連言③（`plain_results_hidden`）へ入れた「画面に出ていない行は起動にも使えない」を、
連言④（窓高さ > 0）にも効かせる。

## 受け入れ条件

1. `visible_rows = 0` のとき、results 窓に出ていない行に対する **Enter / Shift+Enter / クリックが起動しない**
2. 同じ条件で **Shift+Enter の `tools >= 2` 枝（ツール選択モードへの入場）も起こらない**
   （#1077 が対象にした 2 つと揃える。「起動の入口」の母集団はこの 2 つと定義する——3b の ⚠️ 2 番への回答）
3. **止めるのは操作だけである**——行データ・選択は消さず、→ / ← のフォルダ突入も止めない（#1077 と同じ射程）
4. `visible_rows > 0` のときの挙動は一切変わらない
5. 連言④の判定式が**表示側と起動側で 1 つである**（同義の別式を作らない）
6. `visible_rows` の値をフレーム内で読む点が **1 つである**（表示ゲートと起動ゲートが食い違わない）
7. 呼び出し点の脱落をソーステキスト検査が捕まえ、**呼び忘れを再現する変異で実際に落ちる**

## 設計の決定

### D1. 述語は `layout::results_area_collapsed(max_results: u32) -> bool` を新設し、`results_window_height` の早期 return がそれを使う

```rust
/// 連言④（`SPEC.md`「4.5 最大列挙数」）が偽になる条件。**起動側もこの述語を見る**（#1106）。
pub fn results_area_collapsed(max_results: u32) -> bool {
    max_results == 0
}
```

`results_window_height` の冒頭を `if results_area_collapsed(max_results) { return 0.0; }` へ書き換える。
これで**「`0.0` を返す条件」そのものが共有され**、片方だけ変わる将来が構造的に消える。

- **`present_results` は現状のまま `desired_height > 0.0` を見る**（述語へ置き換えない）。あちらは高さ由来の
  直接の表現であり、将来 `row_height` 側から 0 が出る形へ変わっても**安全側へ倒れる**。述語へ置き換えると
  その安全性が床（`Metrics::row_height` の `.max(24.0)`）の存在に依存する
- 述語が `row_height` を引数に持たないことの根拠は P2（production で `row_height ≥ 24.0`）で、
  それを固定する既存テストは `layout.rs:685` の `metrics_row_floor_is_24`。**述語の doc からこれを名指す**

**却下**: 述語を `!(results_window_height(max, row) > 0.0)` の形にして起動側にも `row_height` を渡す案。
仮定は不要になるが、起動側の引数が 2 つ増え、`LauncherController` が `Metrics` を持たない現状に
`row_height: f64` を通すことになる（隣の `f64` 引数との取り違えを型が塞がない）。得るものが仮定 1 つの除去だけで、
その仮定はテスト 1 本が固定している。

### D2. 値は `view.rs` がフレーム冒頭で 1 回読み、`FrameVisibleRows` として配る

`FrameIndexing`（#1077）と同型にする。

- `window_coordinator.rs` に `pub(crate) struct FrameVisibleRows(u32)`（フィールド private・`Copy`）と、
  唯一の構築点 `pub(super) fn read_visible_rows(app) -> FrameVisibleRows` を置く。中身は既存の `fn max_results`
- **`drive_results_window` の内側の `max_results(app)` 呼び出しを撤去し、`DriveResultsInputs` へ載せる**。
  これで `visible_rows` を読む点は view.rs の 1 か所になる（**構造的な担保**であり、規範ではない）
- `DriveResultsInputs` の doc と `fn max_results` の doc（#749「読み点の制約を持たない」）を、
  **制約が生まれたこと**へ改める。#1077 が `indexing` について同じ転換をした先例に倣う

**却下**: 起動側で `read_config` し直し、食い違いを受容残余として明記する案。食い違いの帰結が
**この issue の症状そのもの**（見えない行が起動する）であり、1 フレームでも起きれば不可逆である。
`indexing` について #1077 が同じ理由で読み直しを止めたのと同じ判断。

**却下**: `visible_rows` を素の `u32` で配る案。`activate_or_execute(index: usize, ..)` とは型で分かれるが、
**将来 `u32` の引数が隣に増えたときに黙って壊れる**。newtype の費用は 10 行程度で、`FrameIndexing` の先例がある。

### D3. ゲートの置き場所は `activate_or_execute` の冒頭と `shift_activate` の `enter_tool` 枝の前

`ADR-activation-gate-placement` の決定と**同じ結論**を採る（`start_launch` へは置かない・`activate` へは置かない）。

**ただし理由は同じではない。** 同 ADR 却下 1 は 2 つの独立な理由を持つ——(a)「ガードの意味は行の選択の性質であり、
`start_launch` へ届く時点で行はもう無い」、(b)「却下しても数は減らない（`tools >= 2` 枝が `start_launch` を
通らない）」。**④の述語は `max_results` だけを取り、行の情報を一切使わないので (a) は当たらない。**
結論を支えるのは (b) だけである（`shift_activate` に個別のガードが要るのは変わらず、
③と④が同じ位置に並ぶ方が一緒に読まれる）。**この非対称を書き残す**——「そのまま踏襲」と書くと、
将来「行を見ない述語」を検討する人が (a) も効くと誤読する。
**ただし `plain_results_hidden` の隣ではなく、その手前に置く**——連言④には carve-out が無く（P4）、
tool ビュー・instant 行・folder 展開の行も同時に見えなくなるため、`view_kind` による dispatch より前で止める。

`shift_activate` に個別のガードが要る理由は #1077 と同じ（`tools >= 2` 枝が `activate_or_execute` を通らない）。

### D4. `visible_rows = 0` を到達不能にする案（issue の (b)）は採らない

3 出典で却下済みの族である（research.md P6）。`ADR-results-fixed-height` 却下 5 / `SPEC.md`「4.5 最大列挙数」/
`layout.rs` の「`0.0` は hide の契約値。**0 を作ってはならないし、消してもならない**」。
**新しい ADR は書かない**——否定の知識の正本が既にあり、参照で足りる（`.claude/rules/governance-docs.md`
「正本を 1 か所に定め他は参照へ」）。

### D5. → / ← のフォルダ突入は止めない

#1077 の理由（「突入すれば Folder ビューになり行は可視へ戻る」）は**④偽では成り立たない**（窓高は 0 のまま）。
それでも維持する理由は別にある: フォルダ突入は**行の起動ではなく現在地の移動**であり、可逆で、
状態を進めるだけである。#1077 が定めた射程（止めるのは起動と tool 入場）を④でも揃えることを優先する。
**この理由をコードの doc に書く**（#1077 の理由をそのまま写すと偽になるため）。

## 変更ファイルと対象シンボル

| ファイル | 対象 | 変更 |
|---|---|---|
| `SPEC.md` | 「4.5 最大列挙数」の 0 行 bullet | 「起動にも使えない」を 1 行足す。**§4.7 と §8.6 には写しを置かない**（§8.6 の表が既に連言④の正本を §4.5 と名指す） |
| `src-tauri/src/egui_shell/layout.rs` | `results_area_collapsed`（新設）/ `results_window_height` | 述語の新設と早期 return の書き換え |
| `src-tauri/src/egui_shell/window_coordinator.rs` | `FrameVisibleRows`（新設）/ `read_visible_rows`（新設）/ `max_results` / `DriveResultsInputs` / `drive_results_window` | 型の新設・読み点の移設・doc の更新 |
| `src-tauri/src/egui_shell/mod.rs` | re-export | `FrameVisibleRows` を `FrameIndexing` と同じ形で通す |
| `src-tauri/src/egui_shell/view.rs` | `update()` の冒頭 / `on_enter` 呼び出し（`:1097`）/ クリック逆流（`:1175`）/ `DriveResultsInputs` の構築（`:1285` 付近） | 1 回読み、起動側と driver へ配る |
| `src-tauri/src/egui_shell/launcher_controller.rs` | `on_enter` / `activate_or_execute` / `shift_activate` / `mod tests` の 2 検査 | 引数追加・ゲート追加・検査の拡張 |

**触らない**: `snotra-core/src/config.rs`（D4）、`search_state.rs`（`plain_results_hidden` は③の述語のまま）、
`scripts/lib/SnotraSmoke.psm1`（research.md「実質的な補足」の判断）。

## 実装順序

`AGENTS.md`「開発ワークフロー」1 に従い `SPEC.md` → コード → 文書の順。**各フェーズは検証 green 後にコミットする。**

### フェーズ 1 — `SPEC.md`

- [x] 「4.5 最大列挙数」の「0 件のときは窓を出さない…最大表示件数が 0 のときも出さない」bullet に、
      起動にも使えないことを足す。**係り先を曖昧にしないこと**——同 bullet は連言②（0 件）と
      連言④（最大表示件数が 0）を 1 文で扱っているが、**起動を止めるのは④だけである**
      （②では行が空なので起動する行が無く、規範を足す対象にならない）。②側に係ると読める書き方をしない
- [x] 「8.6 状態遷移図」の遷移ルール要約の行（`NormalMode -> ToolSelectionMode` の `Shift+Enter` が
      「§4.7 の表示ゲートにも従う」と書いてある行）へ、**§4.5 の連言④にも従う**ことを足す
      （`/state-check` Step 5 の要対処。**規則の本体は書かず参照に留める**——#1077 が §8.6 へ写しを
      置かなかったのと同じ扱い）。**2 つのゲートが独立であることが読み取れる形にする**——
      「§4.7 と §4.5 の**どちらか**が隠していれば入場しない」であって、両方の成立を要求するのではない
- [x] `npm run governance:check` が緑（カテゴリ F・全検査 passed / 検査 19 件）

### フェーズ 2 — `layout.rs` の述語

- [x] `results_area_collapsed` を新設し、doc に「連言④の正本」「起動側もこれを見る」「`row_height` を
      引数に持たない根拠は `metrics_row_floor_is_24`」を書く
- [x] `results_window_height` の早期 return を述語呼び出しへ書き換える（契約 doc「`0.0` は hide の契約値」は維持）
- [x] 既存テスト（`results_window_height(0, row) == 0.0` ほか）が緑。**新テスト
      `results_area_collapsed_matches_the_zero_height_contract` は Red を実測してから通した**——
      述語を `false` 固定にした状態で `assertion failed: results_area_collapsed(0)`（276 passed / 1 failed）。
      なお既存の高さテストはこの Red では落ちない（早期 return を外しても `0.0 * drawn_row == 0.0` で
      値が一致するため）——**新しい性質を測れるのは新テストだけである**

### フェーズ 3 — 型の新設・配線・ゲートを **1 コミットに束ねる**

**分割してはならない**（`/plan-review` Step 1 の 6「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」）。
分けると中間状態がビルドを通らない: `FrameVisibleRows` / `read_visible_rows` は呼び出し点が無ければ
`-D warnings` 下で `dead_code` になり、`DriveResultsInputs` にフィールドを足した時点で `view.rs` の構築点が
コンパイルエラーになり、`on_enter` のシグネチャ変更は `view.rs` の呼び出しと同時でなければ通らない。

`window_coordinator.rs`:
- [ ] `FrameVisibleRows`（private フィールド・`Copy`・`get()`）と唯一の構築点 `read_visible_rows` を
      `FrameIndexing` / `read_indexing` の隣へ置く
- [ ] `DriveResultsInputs` に `max_results: u32` を足し、`drive_results_window` 内の `max_results(app)` 呼び出しを撤去
- [ ] `fn max_results` の doc（#749「読み点の制約を持たない」）と `DriveResultsInputs` の doc を、
      制約が生まれたことへ改める。**制約の理由を名指すこと**——`DriveResultsInputs` の doc が既に語る
      2 種（`width` は main へ適用するのと同一フレームの同一値・`row_height` は `VisualSnapshot` 由来）の
      **どちらでもない第三の理由**である: **起動側のゲートと同じ 1 回の読みでなければ #1106 の症状が再発する**。
      理由を書かずに制約だけ書くと、次に読む人が既存 2 種のどちらかへ誤って分類する（#749 / #752 F2 で
      繰り返した欠陥クラス）
- [ ] `mod.rs` の re-export を `FrameIndexing` と同じ形で足す

`view.rs`:
- [ ] `update()` の冒頭（`indexing` の読みの隣・`:928` 付近）で `read_visible_rows` を 1 回呼ぶ
- [ ] `on_enter`（`:1097`）・クリック逆流（`:1175`）へ渡す
- [ ] `DriveResultsInputs` の構築（`:1285` 付近）へ同じ値を載せる

`launcher_controller.rs`:
- [ ] `on_enter` / `activate_or_execute` / `shift_activate` のシグネチャへ `FrameVisibleRows` を足す
- [ ] `activate_or_execute` の**冒頭**（`plain_results_hidden` の手前・`view_kind` の dispatch より前）へ
      `if results_area_collapsed(visible_rows.get()) { return; }`
- [ ] `shift_activate` へ同じゲート。**位置は既存の `plain_results_hidden` ガードの直前**
      （instant / Tool ビューを `activate_or_execute` へ委譲する冒頭の分岐より**後**でよい——
      委譲先の冒頭ゲートが効くため。`folder_load_pending` より前）
- [ ] 両方の doc に、③との射程の違い（carve-out が無い）と D5（→ / ← を止めない理由）、
      および**復帰経路**（`/o` は Enter を経ないためこのゲートを通らない）を書く

### フェーズ 4 — 検知器

- [ ] `activation_entry_points_consult_the_display_gate` の母集団（`activate_or_execute` / `shift_activate`）へ
      `results_area_collapsed(` の要求を足す
- [ ] `activation_uses_the_frame_indexing_value_not_a_live_read` と同型で、起動の入口が `visible_rows` を
      自分で読み直していないこと（`read_config(` が本体に無いこと）を足す
- [ ] `view.rs` の `indexing_is_read_exactly_once_per_frame` と同型で、`visible_rows` の読みが 1 回だけであることを固定する
- [ ] **上記 3 本それぞれについて、呼び忘れ・読み直しを再現する変異を入れて実際に落ちることを確かめ、戻す**
      （AGENTS.md「`Option` / フラグ / enum variant …」行）

### フェーズ 5 — 検証

- [ ] `/race-check` を実行する（**計画段階では起動しない設計ゆえここへ置く**・#784）。該当トリガーは
      「フレーム内 live-read を追加/変更」——`visible_rows` の読み点を driver から view.rs へ移す
- [ ] `/dry-check` を実行する（該当トリガーは「関数・型を新規定義」——`results_area_collapsed` /
      `FrameVisibleRows` / `read_visible_rows`）
- [ ] `docs/build-commands.md`「変更後の検証チェックリスト」カテゴリ A（fmt / clippy / test）
- [ ] `cargo doc`（doc コメントを触るため・`.claude/rules/comments.md`）
- [ ] カテゴリ C（`smoke:startup` / `smoke:egui`）——表示経路を触るため
- [ ] **修正後の実機確認**: フェーズ 0（未確定 U1）と同じプロファイルで、`egui_launch` が**出ないこと**を測る
      （trace の不在で書く。`src-tauri/CLAUDE.md`「trace の presence 検査は状態の検査ではない」）
- [ ] `visible_rows = 8` のプロファイルで通常の起動が**変わらず動く**ことを測る（受け入れ条件 4 の接地）

## 不変条件と異常系

- **`visible_rows > 0` の経路は 1 行も挙動が変わらない**（述語は `max_results == 0` でのみ真）
- **行データと選択は消さない**——ゲートは `return` するだけで `set_results` を撃たない
- **`results_window_height` の `0.0` 契約は維持する**（`present_results` 側の式を変えない）
- **`FrameVisibleRows` の構築点は `read_visible_rows` ただ 1 つ**（フィールド private・#1077 却下 5 の形）
- 異常系: `AppState` 不在・config 読み失敗時は `read_config` の fallback（既定 8）へ落ち、**ゲートは効かない側**
  （＝現状の挙動）になる。既存の `max_results` と同じ挙動であり、新たな失敗経路を作らない

## テスト方針と検証コマンド

- `layout.rs`: `results_area_collapsed` の真理値表（0 で真・1..=50 で偽）。既存の
  `present_results_truth_table_distinguishes_all_four_conjuncts` は**そのまま緑であること**（④の意味を変えない）
- `launcher_controller.rs`: ソーステキスト検査 3 本（検知器フェーズ）。**挙動テストは書けない**——
  `LauncherController` の構築が `AppHandle` と engine lock を要求する（`ADR-activation-gate-placement` 却下 3）
- 検証コマンドは `docs/build-commands.md` を SSOT とする（本文書へ写さない）

## `SPEC.md`・関連文書の更新要否

| 文書 | 要否 | 理由 |
|---|---|---|
| `SPEC.md`「4.5 最大列挙数」 | **要** | 文書化された挙動が変わる＝仕様変更（`AGENTS.md`「『fix』でも文書化された挙動を変えたら仕様変更」） |
| `SPEC.md`「8.6 状態遷移図」 | **要（参照 1 行）** | 状態遷移図の `Shift+Enter [tools >= 2]` の辺にガードが増える。既存行が §4.7 だけを名指しているので §4.5 を併記する。**規則の本体は書かない** |
| `SPEC.md`「4.7 結果表示制御（2 窓構成）」 | 不要 | #1077 の規範は「**この規則で**隠れている行は」と indexing のゲートへ意図的に限定してある。④の規範は §4.5 側へ置き、限定を覆さない |
| `docs/adr/` | 不要 | D4 の否定の知識は `ADR-results-fixed-height` 却下 5 が、ゲートの置き場所は `ADR-activation-gate-placement` が既に持つ。**新規の否定の知識は D1・D2 の却下理由だけで、それはコードの doc と PR 本文が持つ** |
| `src-tauri/CLAUDE.md` | 不要 | モジュール構成のファイル一覧は変わらない（新規ファイル無し）。横断不変条件も増えない |
| `docs/architecture.md` | 不要 | 窓の駆動の構造は変わらない |
| `src-tauri/CLAUDE.md` の `layout.rs` / `window_coordinator.rs` のシンボル索引 | 不要 | あの索引は主要シンボルだけを載せる——`main_window_height` も `path_size` も `FrameIndexing` も載っていない（`plain_results_hidden` も `search_state.rs` の行に無い）。`results_area_collapsed` / `FrameVisibleRows` は同じ粒度の内側 |

## 状態モデルの検証（`/state-check`・計画レビュー版）

**新しいモードでもシグナルでもない**——ガードの条件は `effective_visible_rows() == 0` という config 値からの
純粋導出であり、状態を持たない。ゆえに**リセット経路（`consume_reset_pending` / Escape 連鎖 / 実行完了 /
モード離脱）の対象にならない**（`FrameVisibleRows` はフレームローカルで `self.` へ保持しない・D2）。

### 直交性（全モードと直交・ガードが勝つ）

| 組み合わせ | 同時成立 | 優先度と根拠 |
|---|---|---|
| × `FolderExpansionMode` | 直交 | ガードが勝つ（早期 return）。folder ビューの行も窓に出ない（`SPEC.md`「4.5 最大列挙数」の「すべてのビューへ一様に適用」）。→ / ← は止めない（D5） |
| × `ToolSelectionMode` | 直交 | ガードが勝つ。`execute_tool_selected` は `activate_or_execute` の内側の dispatch なので冒頭ガードが先に効く |
| × `QueryIntent::Instant` | 直交 | ガードが勝つ。**#1077 との違いはここである**——③は instant 行を carve-out するが、④は carve-out を持たない |
| × `QueryIntent::Command` | 直交 | **ガードは効かない**（整合）。スラッシュコマンドは Enter を経ず完全一致で走る（`SPEC.md`「15.1 概要」）ため `activate_or_execute` を通らない。#1077 と同じ |
| × `indexing` | 直交 | 両方真なら両方のガードが効く（どちらでも `return`）。順序は問わない |
| × `launching` | 直交 | ガードは新しい起動の入口だけを止める。in-flight の起動には触らない |

### 入力イベントの分岐

| イベント | ④偽（`visible_rows = 0`）での挙動 | 判定 |
|---|---|---|
| Enter | 止める | 明示（受け入れ条件 1） |
| Shift+Enter（`tools >= 2`） | 止める（ツール選択へ入場しない） | 明示（受け入れ条件 2） |
| Shift+Enter（`tools <= 1` / instant / Tool ビュー） | 止める（`activate_or_execute` へ委譲される） | 明示 |
| クリック / ダブルクリック | 止める（クリック逆流が `activate_or_execute` を通る）。ダブルクリックは独立挙動を持たない（`SPEC.md`「4.8 マウス操作」） | 明示 |
| → / ← | **止めない** | 明示（D5） |
| ↑↓・文字入力・Escape | 止めない | 明示（ガードの対象外。検索も選択も動く——見えないだけ） |

### バグパターン 4（ガードの過剰阻害）の検討 — 復帰手段は存在する

ガードが真になる全コンテキストは `effective_visible_rows() == 0` **ただ 1 つ**である。初回起動・config 読み失敗は
`read_config` の fallback（既定 8）へ落ちるため**ガードは偽**であり、初回フローを阻害しない。

`visible_rows = 0` にした利用者はあらゆる行の起動ができなくなるが、**`/o`（設定を開く）は Enter を経ず
完全一致で走るためこのガードを通らない**（`search_state.rs` の `find_slash_command` で `SlashCmd::OpenSettings`
を確認）。設定画面の `1..=50` clamp で正常値へ戻せる。`config.toml` の手編集も従来どおり効く。
**この復帰経路をコードの doc に書く**——ガードが「詰み」を作らないことの根拠であり、推論では辿り着けない。

## 未確定（実装前に潰す）

- [x] **U1: `visible_rows = 0` で「行が 1 行も出ていないのに Enter が起動する」を実機で測る（フェーズ 0）**
      — **2026-08-16 に再現した。結果は下の「フェーズ 0 の測定結果」。**
      — 修正が入った後では原理的に測れない（#1077 のフェーズ 0 と同型）。**A（`visible_rows = 8`）→
      B（`visible_rows = 0`）の対照 2 回**で測る。

      **プロファイルの作り方**: `SNOTRA_CONFIG_DIR` が指す新規ディレクトリへ `config.toml` を**自分で 1 枚書く**
      ——`New-SnotraVerificationProfile` は `[appearance]` を自分で発行し、`-AdditionalSections` での再定義は
      TOML の parse を落とす（research.md「実質的な補足」で実測）。中身:
      - `[hotkey]` — **実 config（`Ctrl+K`・2026-08-16 実測）と衝突しない値**にする（`Alt+Q` 等）。
        **衝突すると注入が実インスタンスへ届き、実インデックスの任意アイテムを起動しうる**うえ、
        テスト側 trace は沈黙して「再現せず」と誤読される。**起動後に自プロセスの trace で
        `hotkey:registered` を肯定的に確認してから注入する**（現時点で実 Snotra は未常駐——`Get-Process` で実測）
      - `[appearance]` — `visible_rows`（A: 8 / B: 0）と `window_width`
      - `[general]` — `auto_update = "disabled"` と **`auto_hide_on_focus_lost = false`**
        （#1107 が到達可能性の要と名指した値。フォーカスの揺れによる偽陰性を消す）
      - `[paths]` — **scratch の隔離ディレクトリ 1 つだけ**を指し、そこへ `a.txt` を 1 枚だけ置く。
        **予期される副作用: Enter が通れば関連付けのエディタが 1 枚開く**（テスト後に閉じる）

      **手順**: (1) 現行 `main` の release バイナリを `SNOTRA_TRACE=1` + `SNOTRA_CONFIG_DIR` で起動 →
      `hotkey:registered` を確認。(2) `Send-SnotraKeyChord` で hotkey → `Wait-SnotraTraceEvent egui_show:done`。
      (3) `Send-SnotraKey` で `a` を打鍵（seed 名と一致させる——一致しないと連言②が偽で Enter が空振りし、
      ④の検証にならない）。(4) `Wait-SnotraWindow -Title 'Snotra Results'` と trace の `egui_results:show` を見る。
      (5) Enter を注入し `egui_launch` を見る。

      **判定**:
      - **A（対照・`visible_rows = 8`）**: `egui_results:show` **あり** ＋ `egui_launch` **あり**。
        これが出なければ打鍵列が行を作れていないので、B の陰性は何も意味しない（**計器の接地**）
      - **B（本番・`visible_rows = 0`）**: `egui_results:show` **なし**・窓は OS 実測でも不可視 ＋
        `egui_launch` **あり** → **再現**
      - B で `egui_launch` が出なければ計画を書き換え、A/B の trace 内訳とともにユーザーへ報告する
        （受け入れ条件が消えるため）

      env が効いたことはプロファイル配下の `*.bin` 生成で肯定的に確かめる（`docs/build-commands.md` の作法）。
      **A の対照を置く理由**: ④偽では `egui_results:show` が原理的に出ないため、B 単独では
      「②偽（行が空）で起動しなかった」と「④のバグが無かった」を区別できない

## フェーズ 0 の測定結果（2026-08-16・現行 `main` の release バイナリ）

使い捨てプロファイル（`SNOTRA_CONFIG_DIR`）＋ `SNOTRA_TRACE=1`。実 config には触れていない。
ホットキーは `Alt+Q`（実 config の `Ctrl+K` と非衝突）。索引は seed の `zsnotra1106.txt` **1 件だけ**
（両側とも `index_entries: 1`）。A と B の差は `visible_rows` の値だけである。

| 観測 | A（`visible_rows = 8`・対照） | B（`visible_rows = 0`・本番） |
|---|---|---|
| `hotkey:registered` | `ok: true, vks: [18,81]` | `ok: true, vks: [18,81]` |
| `egui_show:done` | あり | あり |
| `egui_search:settled` | 2 回（`index_entries: 1`） | 2 回（`index_entries: 1`） |
| **`egui_results:show`** | **あり（`rows: 1`）** | **0 件** |
| **results 窓の可視性（OS 実測 `IsWindowVisible`）** | **可視** | **不可視** |
| **`egui_launch`** | **あり（`index: 0`）** | **あり（`index: 0`）** |
| `egui_results:hide` | あり（起動後） | 無し（一度も show していない） |

**判定: 再現。** B では results 窓が trace でも OS 実測でも一度も可視にならないまま、Enter が行 0 を起動した。

**行が非空だったことの直接の証拠は `egui_launch` そのものである**——`on_enter` は
`if !self.state.results().is_empty()` を通ってからでなければ起動の入口を呼ばず、`activate` の trace は
`results().get(index)` が `Some` のときにしか出ない。ゆえに B の陰性側（`egui_results:show` の不在）を
「②偽で空振りした」と読む余地は無い。

env が効いたことは両側ともプロファイル配下の `index.bin` 生成で肯定的に確認した。
**副作用**: seed の `.txt` が関連付けアプリで 2 枚開いている（A・B 各 1 枚）。閉じてかまわない。

## 人間レビュー

- [x] 承認済み — 2026-08-16 / 問い: "`workspace/plan.md` をご確認のうえ、**この計画で `/implement` へ渡してよいか**——注釈を入れたい箇所があれば `workspace/plan.md` へ直接追記いただくか、承認の可否をお聞かせください。承認をいただくまで実装には入りません。" / 回答: "OK /implement へ"

## plan-review 結果

- リスク: 高（状態遷移の変更）
- レビュー方式: 計画準拠レビュー 1 体（Step 2。網羅性が要件でないため Step 2b は採らない）
- エージェント数: 1（全文は `workspace/plan-review-1106-gate.md`）

### 要対処（2 件・いずれも根拠が成立したので反映済み）

- **`DriveResultsInputs` の doc に書くべき「制約の理由」が計画に無かった** — 同 doc が既に語る 2 種
  （`width` / `row_height`）の**どちらでもない第三の理由**である。再照合: `window_coordinator.rs` の
  `DriveResultsInputs` doc は現に 2 種しか語っておらず、計画は「制約が生まれたことへ改める」としか
  指示していなかった → 型の新設フェーズへ理由の明記を追加
- **D3 が引く `ADR-activation-gate-placement` 却下 1 の理由が、④には部分的にしか転移しない** —
  却下 1 の理由 (a)（「行の選択の性質」）は、行の情報を取らない④の述語には当たらない。結論は理由 (b)
  （`tools >= 2` 枝が `start_launch` を通らない）だけで支えられる。再照合: 同 ADR 却下 1 の本文で 2 理由の
  独立を確認 → D3 へ非対称を明記

### 軽微（3 件・いずれも反映済み）

- `SPEC.md`「4.5 最大列挙数」の該当 bullet が連言②と④を 1 文で扱うため、追記の係り先が曖昧になりうる
  → 「起動を止めるのは④だけ」と係り先を固定する指示を追加
- `SPEC.md`「8.6 状態遷移図」への参照追加が、独立な 2 ゲートを AND と読ませうる
  → 「どちらかが隠していれば入場しない」と読める形にする指示を追加
- `shift_activate` のゲート挿入位置が `activate_or_execute` ほど明記されていない
  → 「既存の `plain_results_hidden` の直前・`folder_load_pending` より前」と明記

### 未検証

- 観点 2 のレース窓の広さの実測 — `/race-check` を検証フェーズへ意図的に遅延している（計画段階では
  起動しない設計・#784）。計画自体の欠陥ではない
- `FrameVisibleRows` 等の実装コード — 未実装のため確認不能

### 判断

- 実装着手: **可**（人間の承認後）

## セルフレビュー

- リスク: **高**（`/plan-review`「リスク判定」の「永続形式・設定キー・公開 API・**状態遷移**を変更する」に
  該当する——`SPEC.md`「8.6 状態遷移図」の `Shift+Enter [tools >= 2]` の辺にガードを足すため。
  他の 5 条件は非該当: worker / channel / listener / 共有状態 / 非同期処理の変更なし・hook / CI / rules /
  skills / ガバナンス文書の変更なし・網羅性は要件でない・モジュール間インターフェースの新設なし〔`egui_shell`
  内に閉じる〕・`--deep` 指定なし）
- 該当した条件別チェック 3 件:
  - 「UI モード・ガード条件を追加/変更」→ `/state-check` **実行済み**（結果は上節。要対処 2 件を反映——
    SPEC「8.6 状態遷移図」への参照 1 行と、復帰経路の doc 化）
  - 「フレーム内 live-read を追加/変更」→ `/race-check`。**計画段階では起動しない設計**（#784）ゆえ検証フェーズ へ
  - 「関数・型を新規定義」→ 呼び出し元は LSP `findReferences` で列挙済み（research.md P3・P5）。
    `/dry-check` は実装後のため検証フェーズ へ
- plan-review: 未実施（`/plan-review`「リスク判定」の高リスク条件に非該当——永続形式の変更なし・
  並行性の新設なし・網羅性が要件でない・ガバナンス文書の移動/圧縮/分割なし）
- エージェント数: 1（Step 3b の敵対的調査）
- 自己レビュー 5 点の結果:
  1. issue の全要件に作業項目が対応する ✓（issue の (a) を採り、(b) は D4 で却下理由つき）
  2. 境界条件と検証 — `visible_rows = 0` / `1` / 既定 `8` の 3 点。0 と 1 は `layout.rs` の真理値表、
     8 は検証フェーズ の実機確認
  3. 新しい状態・リソース・プロセス — 無し（`FrameVisibleRows` は値型で生成/破棄を持たない）
  4. より単純な既存パターンで置き換えられないか — D1・D2 の却下欄に記載
  5. 壊してはならない不変条件の検知手段 — 「起動側が④を見る」は検知器フェーズ の検査 1、
     「1 回読み」は検査 3 と `read_visible_rows` の構造、「④の意味が変わらない」は既存の真理値表テスト
- 要対処: 計 4 件、すべて反映済み。3b から 2 件（`ConfigError::VisibleRowsZero` の行番号訂正・再現手順の
  機序訂正 → research.md）、`/state-check` から 2 件（`SPEC.md`「8.6 状態遷移図」への参照 1 行 → フェーズ 1、
  復帰経路（`/o`）の doc 化 → ゲートのフェーズ）
- 未検証: U1（実機再現）。5c より前に潰す
