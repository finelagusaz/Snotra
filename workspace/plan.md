# 実装計画: #751 — style 経由 3 値を同じフレームに届ける

種別: **バグ修正**（`SPEC.md` §11 の意図「色は config から」は変わらない。文書化されたフローも
状態遷移も変えないので `SPEC.md` 同期は不要——`AGENTS.md` 開発ワークフロー 1 の判定）。

## 目的

`[visual]` の**色だけ**を変えた config 適用フレームで、`extreme_bg_color` /
`selection.bg_fill` / `weak_text_color` の 3 値が**そのフレームの TextEdit 描画に届く**ようにする。

## 受け入れ条件

1. `SearchWindowView::update` が `ctx.set_visuals` を呼ばず、`ui.visuals_mut()` で 3 値を適用する
2. 同一 pass の子 Ui（TextEdit が置かれる `egui::Frame` の内側）が新しい 3 値を読むことを、
   headless の `Context::run_ui` テストが固定する
3. カテゴリ D（実機）: **設定ウィンドウを開いたまま** `input_background_color` **だけ**を
   非既定色へ変えて保存すると、main の入力欄背景が**キー入力なしで**新色になる。
   **前提条件つき**——修正前のバイナリで同じ手順を先に踏み、症状（入力欄だけ旧色）が
   **再現することを確認してから**修正後を測る。理由は下記「カテゴリ D の手順」
4. `cargo fmt` / `cargo clippy -D warnings` / `cargo test` が緑（カテゴリ A）
5. 修正で嘘になる既存コメント・CLAUDE.md の記述がすべて更新されている

## 変更ファイル一覧と対象シンボル

| ファイル | 対象 | 内容 |
|---|---|---|
| `src-tauri/src/egui_shell/view.rs` | `SearchWindowView::update`（現 `:353-361`） | `ctx.style_of(ctx.theme()).visuals.clone()` + `ctx.set_visuals` を廃し、`ui.visuals_mut()` へ 3 値を代入。**位置は現在のまま**（最初の子 Ui 構築より前） |
| 同 | module doc `:9-22` | 「本ファイルが直接呼ぶのは `ctx.set_visuals` と `frame.set_clear_color`」→ `ui.visuals_mut()` と `frame.set_clear_color` へ。「`ui.visuals_mut()` は全域 grep で 0 件（2026-07-28 実測）」の一文を削除。「style 経由 3 値の #751 制約」を解消済みの記述へ |
| 同 | `:340-342` | `set_clear_color` の「下の `set_visuals` が抱える制約とは無縁」を、非対称が消えた事実へ書き換え |
| 同 | `:584-585` | 「hint の色は `set_visuals` の `weak_text_color` が正本」→ `ui.visuals_mut()` の |
| 同 | `mod tests`（`:888-947`） | テスト 1 本を追加（下記「テスト方針」） |
| `src-tauri/CLAUDE.md` | 「テーマ色・font・行高の読みは 1 フレーム 1 回」節末尾 | 「style 経由の 3 値が抱える #751 の制約とは別経路」の言い換え + 新しい順序不変条件の明記 |

**触らない**: `docs/superpowers/specs/2026-07-2[4578]-*` / `docs/superpowers/plans/*` の #751 言及。
日付付き設計書＝**凍結された歴史**である（#896 の方針）。`visual.rs`（導出は壊れていない）、
`snotra-egui-runtime/`（runtime 側にフックは要らない）、`snotra-settings/`（別プロセス・静的テーマ）。

## 実装順序

### Phase 0: 未確定の測定（実装前）

- [x] 「未確定」節 1 を測る（測定手順は同節・結果は `workspace/measure-751-repaint.md`）

### Phase 1: 実装

- [ ] `view.rs` の適用ブロックを `ui.visuals_mut()` へ置換する
- [ ] 置換点に**順序の不変条件**をコメントで明記する（「最初のウィジェット／子 Ui 構築より前」・
      検知器が無いことも書く）
- [ ] テスト 1 本（`ui_visuals_mut_reaches_child_ui_in_the_same_pass`）を `mod tests` へ追加する
- ~~Phase 0 の結果が「フレームが走らない」だった場合のみ `ctx.request_repaint()` を足す~~
  → **実施しない。** Phase 0 で「走る」と実測した（下記「未確定」節）

### Phase 2: 文書

- [ ] `view.rs` の module doc と 3 箇所のコメントを更新する
- [ ] `src-tauri/CLAUDE.md` の該当記述を更新する

### Phase 3: 検証

- [ ] カテゴリ A: `cargo fmt --check` / `cargo clippy --workspace --all-targets -- -D warnings` /
      `cargo test --workspace`
- [ ] `npm run check:colors`（背景経路の非回帰。**入力欄は測っていない**ことを承知の上で走らせる）
- [ ] カテゴリ D（**修正前**）: 現行バイナリで症状が再現することを確認する（判別力の担保）
- [ ] カテゴリ D（**修正後**）: 受け入れ条件 3 を実機で確認する（手順は下記「カテゴリ D の手順」）
- [ ] `npm run governance:check`（`*.md` を変更するため）

## 不変条件と異常系

- **新設**: 3 値の適用は「main 窓の pass で、最初のウィジェット／子 Ui が作られるより前」に
  なければならない。**検知手段は無い**——コンパイラ・ユニットテスト・`check:colors`・smoke の
  どれも捕まえない。**受容する残余**として呼び出し点のコメントと `src-tauri/CLAUDE.md` に名指しする
- **維持**: 値の読みは 1 フレーム 1 lock（#673 spec 決定 4）。`visual` snapshot を `self.` へ保持しない
- **維持**: 背景色は style を経由しない（`set_clear_color`）。今回の変更で**この 2 経路の到達
  タイミングが揃う**（従来は背景だけ同フレーム・3 値は次フレーム）
- **異常系**: `ui.visuals_mut()` は失敗しない（`Arc::make_mut` の CoW）。config の parse 失敗は
  `visual_snapshot` の `hex_or` が既定色へ落とす（既存挙動・変更なし）
- **消える不変条件**: 「ctx の global style に 3 値が載っている」。載せる必要が無くなるので、
  global style は egui 既定のまま固定される。新しく egui コンテナ（`Area` / `Window` /
  `CentralPanel` / popup / tooltip）を main 窓へ足す変更は、その Ui へ自分で visuals を渡すこと

## テスト方針と検証コマンド

`view.rs` の `mod tests` へ**対のテスト**を置く（先例: 同 mod の
`restored_search_inserts_next_input_at_query_end` が `ctx.run_ui` で headless に走っている）。

`ui_visuals_mut_reaches_child_ui_in_the_same_pass`
— `ctx.run_ui` の callback 内で `ui.visuals_mut()` へ 3 値を書き、その後 `egui::Frame::new().show`
で作った子 Ui が `text_edit_bg_color()` / `selection.bg_fill` / `weak_text_color()` の 3 値とも
新しい色を返すことを**最初の pass で**assert する（= 修正が依存する当のもの）。
doc コメントに機序（`context.rs:780-807` の root Ui 生成順 / `ui.rs:108-136` の `global_style()`
snapshot / `ui.rs:236` の子への `Arc::clone`）を書く。

**対のテスト（`ctx.set_visuals` は当該 pass に届かない）は置かない。** それは egui の**現在の
制限**を固定する主張であり、egui が直したら緑のビルドが赤になる——依存していない命題の
保守税になる。こちらのテストは egui が `ui.visuals_mut()` の伝播を壊したときに正しく落ちる。

**このテストは `view.rs` の呼び出し点そのものは守らない**（守るのは egui の機序だけ）。
上の「新設した不変条件に検知器が無い」と同じ残余であり、テストの doc コメントに書く。

検証コマンド（正本は `docs/build-commands.md`）:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run check:colors
npm run governance:check
```

### カテゴリ D の手順（受け入れ条件 3）

`docs/build-commands.md`「`[visual]` の色を変える変更は、**非既定色で**目視する」に従い、
`SNOTRA_CONFIG_DIR` で使い捨てプロファイルを使う。

**まず修正前のバイナリで手順 1〜5 を踏み、症状が再現することを確認する。** Phase 0 の測定では
1 回の config 書き換えで main のフレームが **2 枚**走った（書き換え方に固有の可能性が高い・
測定ログ参照）。**2 枚走る条件では現行コードも 2 枚目で自己修復する**ため、修正後だけを見ると
「修正が効いた」と「そもそも症状が出ない条件だった」を区別できない——**false green になる**。
再現しないなら、この検査は判別力を持たない。そのときは緑と報告せず、その事実を書く。

1. 使い捨てプロファイルへ config を seed し、`cargo run -p snotra` を起動する
2. ホットキーで main を表示する
3. 入力欄へ `/o` + Enter で設定 UI を開く（**開いたままにする**——これが症状の成立条件）
4. 設定 UI で `input_background_color` **だけ**を非既定色へ変え、保存する
   （font_size / font_family は変えない——変えると font 分岐の `request_repaint` が症状を隠す）
5. main の入力欄背景が**キー入力もマウス操作もなしに**新色へ変わることを見る
6. **設定ウィンドウを main に重ねない**（重なると `CopyFromScreen` は設定側の画素を撮るため、
   窓矩形キャプチャでの記録が取れなくなる）

## 未確定（実装前に潰す）

- [x] **`config-applied` の wake で main のフレームが実際に 1 枚走るか** — **走る（実測）**。
  `SNOTRA_EGUI_REPAINT_TRACE=1` で debug ビルドを起動し、main を可視のまま unfocused にして
  静穏化させてから `config.toml` の `input_background_color` **だけ**を書き換えたところ、
  `SNOTRA_EGUI_REPAINT window=main focused=false since_prev_ms=3404.7 causes=-` が現れた
  （3.4 秒アイドルだった main が書き換え直後に描き直された。`causes=-` ＝ egui 内部の
  repaint 要求ではない＝外部 wake）。
  → **Phase 1 の `request_repaint` 追加は実施しない。**
  測定手順・全出力・この測定が答えていないことは `workspace/measure-751-repaint.md`。
  （issue コメントの症状報告からは推論できなかった——報告者は入力欄の色だけを変えているため、
  「入力欄だけ旧色」はフレーム 0 枚とも整合してしまう）

## セルフレビュー

- リスク: 通常
- plan-review: 未実施（通常リスク）／自己レビューのみ
- エージェント数: 0
- 要対処: 6 件反映
  1. `ctx.set_visuals` 削除の可否（消費者側・このリポジトリ）→ `Area` / `Window` / `CentralPanel` /
     `Modal` / `ComboBox` / popup / tooltip / menu 系を `src-tauri/src/` 全域で grep し 0 件を確認
  2. `ctx.set_visuals` 削除の可否（消費者側・egui 内部）→ `global_style()` / `options.style()` の
     呼び出し点 21 件を egui 0.35.0 の `src/` 全域で列挙し、**3 値を読むものが 0 件**であることを
     確認（research.md「技術的制約」）
  3. 新設される順序不変条件に検知器が無い → 受容残余として名指しする項目を Phase 1 と
     「不変条件」節へ追加
  4. 修正で stale になる文書 6 箇所を洗い出し、変更ファイル一覧へ明示（凍結された歴史は触らない）
  5. カテゴリ D が false green になりうる（2 枚走る条件では現行コードも自己修復する）→
     「修正前の再現確認」を受け入れ条件 3 の前提条件として明記
  6. 「`ctx.set_visuals` は当該 pass に届かない」の対テストを**置かない**判断 → egui の現在の
     制限を固定する主張であり、直されたら緑が赤になる保守税になるため（テスト方針節）
- 未検証: なし（未確定 1 は実測で解消済み）。ただし**「現行コードでは 1 枚しか走らないから
  症状が出る」という機序の後半は未確認**である——測定では書き換え方に固有の理由で 2 枚走った。
  本 issue の判断に要るのは「0 枚ではない」ことだけなので、追わない（詳細は測定ログ）

### 自己レビュー 5 点（`/start-issue` Step 5a）

1. **issue の全要件に作業項目が対応するか** — 対応する。修正方向は issue の 3 案のうち第 1 案
   （`ui` を受ける形で TextEdit より前に適用）。第 2 案（runtime のフック）は
   `snotra-egui-runtime` を変えずに済むので却下、第 3 案（`request_repaint`）は
   issue 自身が「1 フレーム遅れは残る」と書く対症療法ゆえ採らない。「副次的な発見」は
   決着済みとして research.md に根拠を記録
2. **境界条件と検証** — (a) 適用点より前に描く要素があるか → `ui.interact`（ヒットテストのみ・
   描画も子 Ui 生成もしない）だけ、目視で確認。(b) results 窓（別 Context）に波及するか →
   `set_visuals` を呼んでいないので影響なし、grep 済み。(c) config parse 失敗 → `hex_or` の
   既存フォールバック（`visual.rs` のテストが固定）
3. **新しい状態・リソース・プロセス** — 追加しない（適用先を Context から Ui へ移すだけ）
4. **より単純な既存パターンで置き換えられないか** — これが最も単純な形である。
   `apply_visuals` 等の helper 抽出は呼び出し点が 1 つなので過剰（KISS）
5. **壊してはならない不変条件に検知手段があるか** — 「1 フレーム 1 lock」はテスト
   （`visual.rs`）が固定。**新設の順序不変条件には検知手段が無く、受容残余として明記する**

## 人間レビュー

- [x] 承認済み — 2026-08-03 / 問い: "この計画で実装へ進んでよろしいでしょうか。`workspace/plan.md` へ注釈を書き加えていただくか、明示的にご承認ください。" / 回答: "OK"
