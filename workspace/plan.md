# plan: #836 フォルダ展開中に現在のフォルダを画面に示す

**方針**: フォルダ展開中の現在ディレクトリを、**入力欄のプレースホルダ（egui の `hint_text`）だけ**に出す。撤去済み WebView2 版の as-built（`{dir} 内を検索...` / `Search in {dir}...` を `<input placeholder>` へ）と同型で、文言も codepoint 一致させる。根拠と一次証拠は `workspace/research.md`。

**描画面は 1 つである**（#700）。status 行（`overlay_kind` の indexing / launching / notice）は一切触らず、`main_window_height` も変わらない。フォルダ展開中の hint は「案内（お知らせ）」ではなく**フォルダ内絞り込み入力欄の本来のプレースホルダ**であり、SPEC §4.7 が禁じる「同じ情報に 2 面」にはあたらない。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `src-tauri/src/egui_shell/strings.rs` | `folder_hint(l: Language, dir: &str) -> String` を追加（`launch_failed` 等と同じ「引数を取る文言は `String` 返し」の先例に倣う）。codepoint 一致のユニットテストを 1 本追加 |
| `src-tauri/src/egui_shell/view.rs` | `hint` の束縛を `&str` → `String` にし、3 分岐へ（tool / folder / それ以外） |
| `src-tauri/src/egui_shell/search_state.rs` | `folder_current_dir()` の `#[allow(dead_code)]` を外し、doc の「driver は生の accessor を直接呼ばない」という**偽になる断定**を書き換える |
| `SPEC.md` | §6 の**末尾へ `### 6.7 フォルダ展開中の現在地表示` を追記**（挿入ではない・理由は下記）。§4.7 の #700 の箇条へ §6.7 への参照を 1 つ足す |

**触らない（根拠つき）**:

- `layout.rs` / `main_window_height` — hint は入力欄の内側に描かれ行を積まない。窓高の式に入力が増えない
- `notify.rs` / `overlay_kind` — folder 文脈は排他ラダーに入れない。folder × indexing / launching / notice / toast の 4 共起はすべて従来どおり status 行・toast 行が描き、hint とは独立（hint は buf が空のときだけ描かれるので重なりもしない）
- `results_view.rs` / `results_window.rs` / `RowsSnapshot` — 結果窓に現在地を積まない（積むと面が 2 つになる）。`truncate_middle` も呼ばない（未確定 (b) の裁定）
- `launcher_controller.rs` — 状態遷移も検索も変えない。`state()` 越しに `folder_current_dir()` へ届くので新しい読み口も要らない
- `SPEC.md` §8.6（状態遷移図）・§18.5 — モード集合もガードも遷移も変えない。表示だけの変更
- `SPEC.md` §11 as-built — 現 594 行は入力欄の**色の受け取り機構と寸法**の記述であって hint の**内容**の一覧ではない（`tool_select_hint` も列挙されていないことを実読で確認）。文言の内容を足すと §6.7 の写しになる
- `docs/architecture.md` / `src-tauri/CLAUDE.md` のモジュール構成節 — ファイルの追加・削除が無く、`strings.rs` の責務宣言（「UI 文言テーブル」）は不変
- `docs/superpowers/plans/2026-07-23-su3-m2-folder.md`（当時「hint も folder 中は現在地を出す（任意）」と書いた計画書）— **歴史文書であり書き換えない**
- `scripts/manual-smoke.ps1` の `$items`（常設の目視項目） — **ラウンド 1 でいったん「項目 14 を追加」としたが、ラウンド 2 の独立再導出が YAGNI と名指ししたので撤回した。** 既存 13 項目を実読すると、いずれも**横断不変条件**（フォーカス奪取・hide の順序・クリック逆流の読み点・位置復元・フォント hot-reload・キャレット・ベースライン・通知期限）＝**どの変更でも壊れうるもの**であり、機能単位の受け入れ確認ではない。新機能のために先回りで常設項目を足すと、以後すべての PR に恒久コストが乗る。#836 の目視は**この PR 限りの目視表**（PR 本文・下記「カテゴリ D の目視項目」）で足りる。**副次的な利得**: `manual-smoke.ps1:6` の「省略時は全 13 項目」という件数リテラル（自動検知が無く、`governance:check` も PostToolUse も `.ps1` を見ない）を腐らせずに済む
- `snotra-core/src/ui_types.rs:19` の `FolderExpansionState` — **#532 SU7 で撤去された WebView2 フロントへの IPC DTO の残滓**（`#[serde(rename_all = "camelCase")]` がその出自を示す）。`current_dir` / `saved_results` / `saved_selected` / `saved_query` は `search_state.rs:72` の `FolderFrame` と**同概念・別名**で、リポジトリ全体での出現は定義の 1 件のみ（`grep -rn FolderExpansionState . --include=*.rs --include=*.md --include=*.toml` = 1 件・`/dry-check` 実測）。lib crate の `pub` ゆえ `dead_code` は発火しない。**撤去は #833 と同種の別作業ゆえ #836 では触らない**が、「現在のディレクトリ」を担う型が 2 つあることは本 issue の実装者が踏みうる罠なので記録する（処遇は下記「裁定によって受容した残余」の最終項）

## 実装順序

### フェーズ 1 — 文言（`strings.rs`）

- [ ] `folder_hint(l: Language, dir: &str) -> String` を追加する。Ja `format!("{dir} 内を検索...")` / En `format!("Search in {dir}...")`
- [ ] **文字列は実物のソースを自分で開いて写す**——`git show 15933af^:ui/src/lib/i18n.ts` の 42 行目・76 行目。**この計画書の文字列は引用であって正本ではない**（`strings.rs` の `//!` が「計画書・レビュー引用の文字列を写さず実物を開け」と要求している。過去に「…」vs「...」・末尾ピリオドの差が実物突合だけで捕まった）。確認済みの事実: 三点は ASCII ピリオド 3 個・Ja は `{dir}` の直後に半角スペース 1 個・En は `{dir}` の直後に空白なし
- [ ] doc コメントに「`{dir}` はフルパスである（フォルダ名ではない）」と一次証拠（復元した `SearchWindow.tsx:277` が `fs.currentDir` を渡していた）を書く
- [ ] `strings.rs` の `//!` へ **「この表の文言は描画面ごとに 3 系統へ分かれる」**旨を 1 文足す——(1) 入力欄のプレースホルダ（`search_hint` / `tool_select_hint` / `folder_hint`） (2) status 行（`indexing_hint` / `launching` / `launch_failed` / `launch_timeout` / `hotkey_*`） (3) toast 行（`update_*`）。**`indexing_hint` は名前に `hint` を持つが status 行の文言である**（#700 で移設された際に関数名が残った）。この一文が無いと、`hint` で grep した実装者が現在地を status 行へ配線する（ラウンド 1・2 の独立再導出が共に「最も踏みやすい罠」と名指しした経路）。
      **「2 系統」と書かない**（ラウンド 3 の訂正）——status 行と toast 行は独立に同時描画されうる別の面である（`SPEC.md:185`「status 行と toast 行は独立に積む（同時成立時にどちらも隠さない）」・`view.rs` が別々に `allocate_exact_size` する）。2 系統と書くと、将来 toast の文言を status 行のラダーへ配線する誤りを誘う
- [ ] ユニットテスト `folder_hint_matches_webview2_parity` を追加し、Ja / En の**完全一致**を固定する（`hotkey_change_failed_matches_i18n` と同じ様式）。入力は通常のパスに加えドライブルート `C:\` と UNC `\\srv\share` を含める

### フェーズ 2 — 表示（`view.rs`）

- [ ] `let hint: &str = if in_tool { … } else { … }`（現行 341-357 行）を次の形へ置き換える:

```rust
let hint: String = if in_tool {
    crate::egui_shell::ui_strings::tool_select_hint(l).to_string()
} else if let Some(dir) = self.controller.state().folder_current_dir() {
    // **`in_folder` ではなく Option を直接分岐させる**。`in_folder`（= view_kind()==Folder）で
    // 分岐すると「Folder なのに dir が無い」到達不能な else 側を `unwrap_or` 等で埋める必要が
    // 出る。Option で分岐すれば**その腕が構造的に存在しない**——到達不能な行を「検出器」に
    // 見せかけずに済む。
    //
    // **同値は片側だけである**: `view_kind()==Folder ⟹ folder.is_some()` は成り立つが、逆は
    // 成り立たない——tool が folder の上に積まれた状態（`enter_tool` は folder frame を残す）
    // では `folder.is_some()` かつ `view_kind()==Tool` である（search_state.rs:209-217 が
    // tool を先に見る。既存テスト `escape_ladder_tool_then_folder_then_hide` がその状態を
    // 実際に構成している）。**この分岐が正しいのは `in_tool` を先に見ているからであって、
    // 同値だからではない**（不変条件 4）。
    crate::egui_shell::ui_strings::folder_hint(l, dir)
} else {
    crate::egui_shell::ui_strings::search_hint(l).to_string()
};
```

- [ ] `hint` は**所有する `String` にする**。`&str` のまま `self.controller.state()` の借用を跨がせると、後段の `&mut self.controller` 呼び出し（`on_input_changed`）で E0502 になる（`update()` 冒頭のコメントが同型の罠を既に記録している）
- [ ] 直上のコメント（「**hint は indexing で差し替えない**（#700）」）を残したまま、フォルダ文脈がなぜ #700 に抵触しないかを 1 段落足す。**`indexing_hint()` は名前に反して status 行の文言である**ことも書く（`hint` で grep した実装者が現在地を status 行へ配線する罠・本変更で最も踏みやすい）
- [ ] **hint を決める位置を動かさない**（不変条件 7）。`in_tool` / `in_folder` の算出位置（現 341-342 行）は `on_nav_keys` より後でなければならない

### フェーズ 3 — 死んだ accessor の解除（`search_state.rs`）

- [ ] `folder_current_dir()` から `#[allow(dead_code)]` を外す（`folder_gen()` 側の allow は**外さない**——消費者は増えない）
- [ ] doc を書き換える。現行の「driver は …生の accessor は直接呼ばない（… §6 で任意扱い・#532 SU3 M2 Task 3 で見送り）」は**この変更で偽になる**（コンパイラが検出しない種類の腐り）。新 doc に書くこと: (i) `view.rs` の hint が唯一の消費者であること（#836・SPEC §6.7） (ii) `enter_folder` / `navigate_folder` が書いた直後の値＝**列挙の到着を待たない**こと (iii) `view_kind() == Folder` のとき必ず `Some` であること
- [ ] doc の書き方は既存様式（バッククォートのコードスパン）に揃える。rustdoc の `[...]` リンクを新たに導入しない（`broken_intra_doc_links` が deny・`cargo doc` は hook 非発火ゆえ手で回す）

### フェーズ 4 — SPEC 同期

- [ ] `SPEC.md` §6 の**末尾に `### 6.7 フォルダ展開中の現在地表示` を追記**する。**挿入ではなく追記である**——§6.1 は `docs/adr/ADR-folder-nav-selection-first-row.md:13,25` から、§6.3 / §6.6 は `docs/superpowers/plans/2026-07-23-su3-m2-folder.md:224,259,697,976` と `docs/superpowers/specs/2026-07-22-su3-search-experience-design.md:121,132,133,157` から**序数で参照されている**（grep 実測）。番号をずらすとこれらが黙って腐る（序数参照は `governance:check` の検出対象外）
- [ ] §6.7 に書く内容（**すべて条件つきで書く**・不変条件 1）:
      (a) フォルダ展開中は検索入力欄のプレースホルダを「`<現在のディレクトリのフルパス>` 内を検索...」に差し替える
      (b) ツール選択が上に積まれている間は tool のプレースホルダが勝つ（優先度 tool > folder > results＝§18.5 と一対一）
      (c) ラベルは打鍵フレームで同期に更新され、**行の到着より先に**変わる（列挙未着・列挙失敗〔§6.6〕でも遷移先を示し続ける）
      (d) **現在地の描画面はこのプレースホルダただ 1 つである**（結果行のパスは項目ごとのデータであって現在地のラベルではない。#700 の「同じ情報に描画面を 2 つ持たない」の適用）
      (e) 受容残余: フォルダ内の絞り込み文字列を 1 文字でも打つと消える（`←` / `→` は絞り込みをクリアするので、階層移動の直後は必ず見える）
      (f) 受容残余: 幅を超えるパスは末尾が `…` になる
- [ ] `SPEC.md` §4.7 の #700 の箇条（現 186 行）末尾へ、**案内（indexing / 起動中 / 一時通知）と文脈（フォルダ展開中の現在地）を区別する一文**と §6.7 への参照を足す。実体は §6.7 に置き、写しを作らない。**この一文が無いと本変更が既存規範に違反しているように読める**
- [ ] セクション番号は §6.7 の追記のみで、既存番号を 1 つも動かさない（`.claude/rules/spec.md`「セクション番号整合」）

### フェーズ 5 — 検証

- [ ] カテゴリ A: `cargo check --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` / `cargo doc --workspace --no-deps --document-private-items`（doc コメントを触るため `cargo doc` は手動実行が要る——hook は発火しない＝沈黙は合格ではない）
- [ ] カテゴリ F: `npm run governance:check`（`SPEC.md` を編集するため。**`*.md` の編集で PostToolUse は沈黙する＝「何も走らなかった」**）
- [ ] `/dry-check`（新規関数 `folder_hint` の定義に対する `AGENTS.md` 条件別チェック表のトリガー。呼び出し元 grep はレビュー済み）
- [ ] カテゴリ D（人間が実施・エージェントは実行できない）: `cargo run -p snotra` で起動し、下の**「カテゴリ D の目視項目」11 件**を確認して**判定を PR 本文の目視表へ手で書く**。
      **`npm run smoke:manual -- -PostToPr` はこの 11 件を記録しない**——`scripts/manual-smoke.ps1` の `$items` は常設 13 項目の固定リストであり、#836 の項目は（未確定 (g) の裁定により）そこへ足さないため。**AC1 の唯一の検証証跡がこの手書きの表である**ことを忘れない
- [ ] 常設 13 項目の側は従来どおり `npm run smoke:manual -- -PostToPr` で回す（本 PR で内容は変わらないが、`view.rs` を触るため項目 1・11 は回帰の観測点になる）
- [ ] **diff の 4 条件チェック**（不変条件 2「描画面は 1 つ」の検知手段。この不変条件は壊れても**コンパイルもテストも通る**ため、diff を読む手続きが唯一の検出器である。ラウンド 3 の独立再導出の提案を採用）: (1) 新しい描画点（`ui.painter()` / `allocate_exact_size`）が 0 件 (2) `OverlayKind` の variant が増えていない (3) `layout.rs` の diff が空 (4) 新規の文言関数がちょうど 1 本。**1 つでも破れていたら status 行案へ滑っている**
- [ ] カテゴリ C は**意味で非該当**（ウィンドウ生成／表示順・ホットキー・スラッシュコマンドのいずれにも触れない・`.claude/rules/src-tauri.md` #558）。ただし `e2e.yml` は `src-tauri/**` の paths で自動起動するので、**走った結果は中身まで読む**（緑が「検査が走った」を意味しない・#686）

## 不変条件

1. **hint は「絞り込み文字列が空のとき」だけ描かれる。** egui の `hint_text` は buf が空のときだけ描画する（`builder.rs:584` の `text.as_str().is_empty()`）。**この条件を落とした全称の記述を SPEC にもコード doc にも書かない**——「フォルダ展開中は常にディレクトリが見える」は実装より強い主張になる（`AGENTS.md`「全称表現は前提条件とセットで書く」）
2. **描画面は 1 つのままである。** folder 文脈を status 行へも results 窓へも出さない。status 行の内容（`overlay_kind`）とその優先順は一切変えない
3. **hint が示すのは「遷移先の」ディレクトリであって「列挙が完了した」ディレクトリではない。** `enter_folder` / `navigate_folder` は `current_dir` を**同期で**書き、`spawn_folder_load`（非同期）はその後に呼ばれる（`launcher_controller.rs:1044-1054, 1061-1066`）。ゆえに `folder_load_pending`（列挙未着）中も §6.6 の列挙失敗時も 0 件でも、hint は遷移先を示し続ける。**これは意図した挙動である**——#743 の誤読が起きた瞬間はまさにここで、「どこへ移ったか」を即時に示すことがこの issue の目的である
4. **tool-on-folder では tool の hint が出る。** `view_kind()` の優先順 tool > folder と、上のコード形（`in_tool` を先に見る）が一致する。Escape で folder へ戻ると `saved_folder_filter` が復元され、それが非空なら hint は出ない（不変条件 1 の帰結であって特例ではない。既存テスト `escape_ladder_tool_then_folder_then_hide` が状態側を固定している）
5. **新しい状態・フラグ・リソースを 1 つも導入しない。** 追加するのは純関数 1 本と既存 accessor の読み出しだけ。ゆえに「失敗・異常終了・予期しない順序」で壊れる状態が無く、`AtomicBool` / `thread::spawn` / 子プロセスの生成も無い
6. **色の指定を書かない。** hint の色は `Visuals::weak_text_color`（`view.rs:299` で `visual.hint` を設定済み）だけが効き、`RichText::color()` は egui が無条件に上書きする（#654）。新しい文言も自動で同じ色になる（SPEC §11）
7. **hint を決める位置を `on_nav_keys` より後から動かさない。** `update()` の冒頭からこの位置までに `view_kind()` / `current_dir` を変えうる `&mut self.controller` 呼び出しが **4 本**ある——`consume_reset_pending`（256 行 → `launcher_controller.rs:838` の `self.state.reset()` が `folder` を `None` にする）・`poll_async`（316 行 → `drain_launch` → `finish_launch` の `LaunchTag::Tool` × `LaunchStatus::Ok` 分岐が `self.state.reset()` を呼ぶ・`launcher_controller.rs:305-311`。**同関数の folder drain 部〔914-933 行〕ではない**——そちらは `folder_cache` / `folder_error` しか触らず `view_kind()` を変えない）・`on_escape_pressed`（322 行）・`on_nav_keys`（332 行・`enter_folder` / `navigate_folder`。**`enter_tool` はここではない**——`on_enter` → `shift_activate`〔`launcher_controller.rs:505`〕から呼ばれ、それは hint より後の 573 行で走るので今フレームの hint には影響しない）。前寄せすると hint は**遷移前**のディレクトリを描き、不変条件 3（＝AC1 の論拠）が壊れて #743 の症状がより見つけにくい形で再現する。
   **件数を断定して書かない。** この「4 本」はラウンド 2 で 3 本から訂正された数であり（`self.controller.` の全呼び出しを列挙して初めて `consume_reset_pending` が出た）、**次の編集でまた変わりうる**。コードコメントには件数ではなく**検算の手続き**を書く: 選んだ位置から TextEdit 構築までの間に `&mut self.controller` を取る呼び出しが 1 本も無いことを、`self.controller.` の grep から列挙して確かめる（現在の位置ではこのリストは空である）
8. **AC2（通常検索・ツール選択・インスタントコマンドの表示を壊さない）は構造で閉じる。** 理由は 2 つあり、どちらか一方でも十分: (i) instant / slash コマンドは軸 2（`QueryIntent`）であって軸 1（`ViewKind`）ではなく、どちらも `ViewKind::Results` に落ちて `search_hint` のまま——**既存 2 腕の値を 1 文字も変えない** (ii) instant モードの成立条件は「バッファが prefix で始まる」＝非空であり、そのとき egui は hint を描かない（不変条件 1）

## テスト方針

| 対象 | 手段 |
|---|---|
| 文言の codepoint 一致（Ja / En・ドライブルート・UNC） | `strings.rs` のユニットテスト（`cargo test -p snotra`） |
| 3 分岐の選択そのもの | **自動テストを置かない**（未確定 (c)(f) の裁定）。AC2 は不変条件 8 が構造で閉じ、選択の実体はカテゴリ D の目視が検知手段 |
| `folder_current_dir` の状態側 | **新規テストは不要**。既存の `enter_folder_*` / `navigate_folder_*` / `left_twice_climbs_two_levels` が階層移動後の `folder_current_dir()` を assert 済み（スカウト実測）。ただしこれらは**hint に届いていることの証拠ではない** |
| 実際に描かれること | カテゴリ D 目視。**これが唯一の検知手段である**（`hint_text` の描画は egui 内部で、ユニットテストから観測できない） |
| 既存挙動の非回帰 | `cargo test -p snotra` ＋ `e2e.yml` の smoke 2 本 |

**カテゴリ D の目視項目**（**この PR 限りの受け入れ確認**として PR 本文の目視表へ書く。`scripts/manual-smoke.ps1` の常設 13 項目は横断不変条件のための別枠であり、増やさない・理由は「触らない」節）:

1. 通常検索で候補を選び `→` — 入力欄が「`<展開先のフルパス>` 内を検索...」になる
2. そのまま 1 文字打つ — hint が消え、打った文字が見える（#700 の「打った文字が見えない」再発が無い）
3. Backspace で空に戻す — hint が戻る
4. `←` で親へ上がる — hint のパスが**行の入れ替わりより先に**親へ変わる（**#743 の誤読が起きた局面**）
5. `←` を連打してドライブルート `C:\` まで上がる — 無反応になっても hint は `C:\ 内を検索...` のまま
6. 空フォルダ／アクセス拒否フォルダ（§6.6）へ入る — results が消える／エラー行 1 行になっても hint に現在地が出ている
7. 深いパス（幅を超えるもの）で展開 — 末尾が `…` になる。**見え方が受容できるかを記録する**（受容できなければ follow-up issue を立てる。この PR は広げない）
8. Shift+Enter でツール選択へ入る — hint が「ツールを選択...」になる。Escape で folder へ戻ると、絞り込みが空なら再びパスが出る
9. 通常検索モード・インスタントコマンド（`@`）・`/` コマンド — hint は「検索...」のまま（回帰なし）
10. `general.language = "en"` で 1・4・7 を再確認 — `Search in <path>...`
11. フォルダ展開中に indexing / updater toast が出る局面 — status 行・toast 行が従来どおり出て、窓高が 1 行ぶんずつ積まれる（hint とは独立）

## SPEC.md 更新要否

**要**。挙動変更（画面に出る情報が増える）ゆえ `AGENTS.md`「3層分担」に従い同じ変更で整合させる。追記先は §6 末尾の新設 §6.7、参照を §4.7 へ 1 つ。文面はフェーズ 4 のとおり**表示条件つき**で書く。§8.6 / §11 / §15.4 は変更不要（根拠は「触らない」節）。

## 未確定（実装前に潰す）— ラウンド 3

- [x] **(a) egui の `hint_text` は溢れたときどうなるか** — **測った**（一次資料）: `egui-0.35.0/src/widgets/text_edit/builder.rs:675-680` が singleline に `TextWrapMode::Truncate` を与え（コメント「This wrap mode only affects the hint_text」）、同 `:584-600` が hint atom に `atom_shrink(true)` を付ける。`atom_layout.rs:382-393` で shrink atom が残余幅を受け、`epaint-0.35.0/src/text/text_layout_types.rs:663-671,700-708` の `TextWrapping::truncate_at_width` が `overflow_character: Some('…')` を持つ。→ **末尾省略（`…`）が既定で効く。ハードクリップではない**
- [x] **(e) `scripts/smoke-egui.ps1` が hint 文字列に依存していないか** — **裏取りした**: `hint` / `placeholder` / `検索` / `Search` を grep して 0 件（該当は検索クエリと SPEC §4.7 への言及コメントのみ）。governance-docs スカウトが `smoke-startup` 側でも独立に再現。smoke の前提は壊れない
- [x] **(b) 省略の向き（末尾 / 先頭 / 中央）** — **裁定: egui 既定の末尾省略をそのまま使い、自前の省略機構を書かない。**
      **却下した代替案**: `results_view::truncate_middle`（`results_view.rs:418`・`pub(crate)`・テスト済みであることを実測確認）を書式へ埋める前の `dir` へ当てる案。却下理由は 3 つ——(i) この関数は `per_char_px`（呼び出し側が渡す推定値）で幅を測る API で、CJK を過小評価する既知の粗さを持つ。接尾辞「 内を検索...」の実幅も別途推定が要る (ii) 既定幅 600px でのパス予算は概ね ASCII 55〜60 字で、**実運用のパスはたいてい収まる**（溢れは端の事例） (iii) WebView2 版は CSS クリップで `…` すら無く、egui 既定は parity より既に良い。
      **受容する残余**: 深い階層では末尾（＝いま居るフォルダ名）が削られる。**観測点はカテゴリ D の目視項目 7・10** で、そこで受容できないと判明したときの逃げ道が上の却下案である（follow-up issue を立て、この PR は広げない）
- [x] **(c) 3 分岐の hint 選択を純粋核へ出すか** — **裁定: 出さない。** hint の分岐は `view_kind()` の優先度ラダー（tool > folder > results）そのものであり、純粋核へ切り出すと `view_kind()` の **2 重導出**になる（`overlay_kind` は `view_kind` と独立な 3 入力を畳む関数であって、この形とは違う）。加えて `strings.rs` の依存が `Language` だけで済まなくなる。
      **なお `view_kind()==Folder ⟺ folder.is_some()` という同値は偽である**（tool-on-folder が反例。ラウンド 2 で訂正）。成り立つのは `⟹` の片側だけで、実装が正しいのは `in_tool` を先に見るからである（不変条件 4・フェーズ 2 のコードコメント）。
      **併せて却下**: `view_kind()` の 2 回読み（341-342 行）を 1 回へ束ねる改修。値は同一（間に `&mut self.controller` が入らないことを実読で確認）で、束ねは #836 の要件ではない（YAGNI）。**落としてはならないのは読み点の位置の方であり、それは不変条件 7 に格上げした**
- [x] **(f) AC2 の検証が目視だけでよいか** — **裁定: よい。** (c) を出さない以上テストの口が無く、かつ AC2 は不変条件 8 が構造で閉じる（instant / slash は `ViewKind::Results` へ落ち、既存 2 腕の値を 1 文字も変えない）。目視項目 9 が観測点

- [x] **(g) `scripts/manual-smoke.ps1` の常設目視項目に「14」を足すか**（ラウンド 1 で計画へ入り、ラウンド 2 の独立再導出が YAGNI と名指し） — **裁定: 足さない。** 既存 13 項目を実読した結果、いずれも**横断不変条件**（どの変更でも壊れうるもの）であって機能単位の受け入れ確認ではない。新機能のために先回りで常設項目を足すと以後すべての PR に恒久コストが乗る。#836 の目視は**この PR 限りの目視表**で足りる。**副次的な利得**: `manual-smoke.ps1:6` の「省略時は全 13 項目」という件数リテラル（`governance:check` も PostToolUse も `.ps1` を見ないため自動検知が無い・governance-docs スカウトの発見を自分で再照合して実在を確認）を腐らせずに済む。
      **将来この項目が要るとしたら**: #836 の hint が実際に一度回帰したとき（#700 → 目視項目 11 と同じ経路）。**先回りでは足さない**

### 裁定によって受容した残余

- [x] **AC1「フォルダ展開中、現在のディレクトリが画面から分かる」は完全には満たさない** — 絞り込み文字列を 1 文字でも打つと hint は消える。
      **裁定の出所**: ユーザーコメント（#836, 2026-07-28）「WebView2版はフォルダ展開すると **メインウィンドウに「（フォルダ名）内を検索…」と表示する**」＋添付スクリーンショット、およびユーザー指示「コメントに WebView2 版の挙動をメモしておいた。**参考にして**丁寧に plan.md を策定してほしい」（2026-07-29）。
      **却下した代替案 B（status 行）**: `overlay_kind` は排他ラダー（indexing > launching > notice）なので、folder 文脈をそこへ入れると indexing 中などに**やはり消える**——#700 が是正した失敗様態そのものになる。加えて folder 中ずっと `main_window_height` が 1 行伸び、フォルダの出入りのたびに main が伸縮する。
      **却下した代替案 C（results 窓のヘッダ行）**: 0 件・空フォルダでは `present_results` が非表示を返して results 窓ごと消えるため、**いちばん現在地を知りたい局面で出ない**。
      **受容する理由**: 無条件に AC1 を満たす候補は存在しない。そして `enter_folder` / `navigate_folder` は**どちらも `folder_filter.clear()` する**（`search_state.rs:243,254`）ので、`←` / `→` の直後は必ずバッファが空＝hint が描かれる。#743 の誤読が起きた瞬間をちょうど覆う。
- [x] **散文とスクリーンショットの食い違い（フォルダ名 vs フルパス）はフルパスを採る** — **裁定の出所**: 一次証拠 2 つ（添付スクリーンショットの `C:\Toolbox\ghost-launcher 内を検索...`、および `git show 15933af^:ui/src/components/SearchWindow.tsx:277` が `fs.currentDir` をそのまま渡していたこと）が一致するため。散文の「（フォルダ名）」は値の略記と解する。ラウンド 1・2 の独立再導出も同じ結論へ独立到達した。
      **ラウンド 3 の独立再導出だけが「フォルダ名（leaf）を採るべき」と反対した。その根拠は採らない**——同エージェントは「散文が SSOT だから」と述べているが、**サブエージェントは issue 添付のスクリーンショットを読めない**（画像であり `gh issue view` には現れない。本計画の作成者は画像を取得して直接確認した）。つまり反対意見は**一次証拠 2 つのうち 1 つを見ないまま出された**ものである。ただし同エージェントが挙げた副次的な論拠（末尾省略で leaf が失われうる）は妥当であり、未確定 (b) の受容残余として既に記録済み。
- [x] **末尾 `\` の揺れは正規化しない** — `compute_parent_dir` はドライブルートで `C:\`（末尾 `\` あり）を返し、フォルダ列挙は `C:\d\Cafe`（なし）を返すため、hint に `C:\ 内を検索...` と `C:\d\Cafe 内を検索...` が混ざる。**WebView2 も同じ値を出していたので parity としては正しい。** 正規化は #836 の要求外ゆえ射程に入れない（気になるなら別 issue）。
- [x] **`FolderExpansionState`（`snotra-core/src/ui_types.rs:19`）の撤去は #836 では行わない** — `/dry-check` が検出した SU7 残滓（消費者ゼロ・詳細は「触らない」節）。**送り先**: `/start-issue` の完了報告でユーザーへ提示し、「本 PR で消す / 別 issue を立てる / 放置する」の裁定を仰ぐ。**issue の起票は外向きの操作ゆえ Claude が独断で行わない**（`CLAUDE.md`「最重要ルール」2 と同種の判断）。**この項目は実装を止めない**——#836 の変更集合とは独立である。

## セルフレビュー

**回したラウンド数: 3（上限）。収束せず、上限で打ち切って引き渡す。**

- **収束条件 (i)（未確定欄に `- [ ]` が残っていない）: 成立。** 未確定は 7 項目（a〜g）＋受容残余 4 項目、すべて `- [x]`。裁定によるものには出所（ユーザー発言と日付・却下した代替案）を記した
- **収束条件 (ii)（`plan-snapshot.md` との差分が空）: 不成立。** ラウンド 3 のレビューが指摘した 4 件を最終ラウンドで直したため、**その 4 件の訂正だけがどのレビューも受けていない**。内訳:
  1. 不変条件 7 の `poll_async` の説明（「folder drain」→ 実際は `drain_launch` → `finish_launch` の Tool 成功分岐。**folder drain 部は `view_kind()` を変えない**）
  2. 同・`enter_tool` の呼び出し元の誤り（`on_nav_keys` ではなく `on_enter` → `shift_activate`）
  3. `strings.rs` の `//!` に足す文言の系統数（2 → **3**。status 行と toast 行は独立に同時描画されうる別の面）
  4. フェーズ 5 のカテゴリ D の記録経路（`smoke:manual -- -PostToPr` は #836 の 11 項目を記録しない。**PR 本文へ手で書く**のが AC1 の唯一の証跡）
- **配送（独立レビューの成立）: 3 ラウンドすべてで 4/4 実在。** 不成立エントリは 1 つも無い（`plan:ledger verify` の出力）
- **ラウンド 3 の「要対処」はすべて自分で根拠を開いて再照合した**（`launcher_controller.rs:297-311, 903-933, 483-505` / `scripts/manual-smoke.ps1` の `$items` / `SPEC.md:185`）。降格した項目は無い
- **5b の 3 観点**:
  1. **境界条件** — ドライブルート `C:\`・UNC `\\srv\share`（ユニットテスト）、空フォルダ・列挙失敗・tool-on-folder・非空フィルタ・幅超過・En/Ja（目視項目 5〜10）で網羅。**`dir` が非空であるという前提は書かない**（`format!` は total ゆえ空でも panic しない。到達可能性が未確認の入力に対して「検出器」めいたテストを足さない）
  2. **シンプル化の挑戦** — 新しい状態・フラグ・スレッド・子プロセスを 1 つも導入せず、追加は純関数 1 本と分岐 1 本。文言を `view.rs` へ直書きすればさらに 1 ファイル減るが、i18n の正本が `strings.rs` である規約と、codepoint 一致テストの継ぎ目を失う。**これ以上は削れない**
  3. **破壊不変条件と検知手段** — 「描画面は 1 つ」（不変条件 2）は**壊れてもコンパイルもテストも通る**唯一の破壊不変条件であり、検知手段としてフェーズ 5 に**diff の 4 条件チェック**を置いた。「hint の読み点」（不変条件 7）の検知手段は同条の grep 手続きと目視項目 4、「#700 の再発（打った文字が見えない）」は目視項目 2
- **判断が割れた点**: ラウンド 3 の独立再導出のみ「フルパスではなくフォルダ名（leaf）」と反対した。**サブエージェントは issue 添付のスクリーンショットを読めない**（画像）という非対称が原因で、一次証拠 2 つのうち 1 つを見ずに出された意見ゆえ採らない（詳細は受容残余の該当項）
