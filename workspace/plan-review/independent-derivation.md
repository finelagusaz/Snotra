# #836 独立再導出 — フォルダ展開中に現在のフォルダを画面に示す

読んだもの: `gh issue view 836 --comments`（SSOT）／`src-tauri/src/egui_shell/{view,search_state,strings,layout,notify,results_view,mod}.rs`／`SPEC.md` §4.5–4.8・§6・§8.6・§11／`docs/build-commands.md`／`scripts/{manual-smoke,smoke-egui}.ps1`／`scripts/governance-check.mjs`／`.claude/rules/src-tauri.md`／撤去済みフロント（`git show 15933af^:ui/src/lib/i18n.ts` と `.../components/SearchWindow.tsx`）／egui 0.35.0 の `src/widgets/text_edit/builder.rs`。
`workspace/plan.md` ほか他人の分解は読んでいない。

---

## 0. 結論（要約）

- 描画面は**入力欄の hint（プレースホルダ）ただ 1 つ**にする。status 行は使わない。撤去済み WebView2 版が `placeholder={t("search.placeholder.folder", {dir})}` そのものであり、issue 本人がその画面を「あるべき姿」として貼っている（2026-07-28 コメント）ため、これは推測でなく SSOT からの導出である。
- 表示するのは**現在フォルダの名前（リーフ）**であってフルパスではない（§3.2。issue コメントの文言「（フォルダ名）内を検索…」に従う。フルパス版は撤去済みコードの側の事実で、両者は食い違っている）。
- 触るコードは **3 ファイル**（`strings.rs` / `view.rs` / `search_state.rs`）、触る文書は **1 ファイル**（`SPEC.md` §6）。`layout.rs`・`notify.rs`・results 窓側は一切触らない。
- 新規の恒久チェックリスト・恒久検査機構は**すべて却下**（§5）。
- 最大の罠は「egui の hint は buf が空のときしか描かれない」を欠陥と誤認して 2 面目を足すこと＝#700 の再演（§6）。

---

## 1. 変更集合（ファイル → シンボル → 何をするか）

### 1.1 コード（必須）

| ファイル | シンボル | 何をするか |
|---|---|---|
| `src-tauri/src/egui_shell/strings.rs` | **新規** `pub fn folder_hint(l: Language, dir: &str) -> String` | Ja: `format!("{dir} 内を検索...")` / En: `format!("Search in {dir}...")`。**末尾は ASCII の `...`**（既存 `search_hint` の `検索...` と同じ。`…` を混ぜない）。引数 `dir` は**表示名**（リーフ）であり、整形の責務のみを持つ |
| 同上 | `mod tests` に 1 テスト追加 | 両言語で `dir` が挿入されること・`…`（U+2026）を含まないこと・**JA は dir が先頭 / En は dir が末尾**（この非対称が §3.2 の判断根拠）を固定する |
| `src-tauri/src/egui_shell/search_state.rs` | **新規** `pub(crate) fn folder_display_name(dir: &str) -> &str` | 純粋核。フルパス → 表示名（リーフ）。**ルートは自分自身を返す**（`C:\` → `C:\`、`\\server\share` → `\\server\share`）。末尾の `\` を剥がしてから最後のセグメントを取る（`compute_parent_dir` と同じ正規化の考え方）。`mod tests` に境界テスト（通常・末尾 `\` 付き・ドライブルート・UNC 共有ルート・空文字）を足す。**置き場所が `strings.rs` でなく純粋核なのは、これが文言でなくパス演算だから**（`compute_parent_dir` の隣） |
| `src-tauri/src/egui_shell/view.rs` | `SearchWindowView::update()` の `let hint` チェーン（現 351–357 行付近） | `in_tool` → **`in_folder`（新）** → 既定、の 3 分岐にする。型は `&'static str` → **`String`**。folder 分岐は `self.controller.state().folder_current_dir()` を読み、`Some(dir)` なら `folder_hint(l, folder_display_name(dir))`、`None`（view_kind=Folder では表現不能）なら `search_hint(l).to_string()` へ倒す（panic しない） |
| `src-tauri/src/egui_shell/search_state.rs` | `SearchState::folder_current_dir` | `#[allow(dead_code)]` を**削除**（消費者ができるため不要になる）。**rustdoc を書き換える**——現在の 2 主張「driver は生の accessor を直接呼ばない」「folder 中の hint 文脈提示は §6 で任意扱い・見送り」は本 PR で**両方とも偽になる**。新 doc は「`view.rs` が folder 中の入力欄 hint に使う（SPEC §6.7）」 |

補足（実装上の細部・いずれもコンパイラが教えてくれる範囲）:

- `hint` を `String` にしても `egui::RichText::new(hint)` は `impl Into<String>` ゆえ通る。
- `folder_current_dir()` は `Option<&str>` を返す。`format!` で即座に所有権を作れば、後段の `self.controller.on_input_changed(...)`（`&mut self`）との借用衝突は起きない。**`&str` を hint 変数に保持したまま持ち回らない**こと。
- `hint` の分岐順は `buf` の分岐順（`in_tool` → `in_folder` → 既定）と**同一**にする。同じ述語に対する 2 本の並行チェーンであり、順序がずれるとツール名と folder hint が同時に出る。

### 1.2 ドキュメント（必須）

| ファイル | 箇所 | 何をするか |
|---|---|---|
| `SPEC.md` | **新設 `### 6.7 現在フォルダの提示`**（6.1–6.6 の直後） | 受け入れ条件 3。as-built を明文化する（内容は下記） |

`### 6.7` に書くべき as-built（**「見えないことがある」条件まで書く**——これを落とすと将来の読者が同じ症状を再び「バグ」として起票する）:

1. フォルダ展開中、入力欄の hint に**現在のフォルダ名**（フルパスの末尾セグメント。ドライブルート `C:\` / UNC 共有ルートはそれ自身）を `{フォルダ名} 内を検索...` として表示する。**描画面はこの hint ただ 1 つ**で、status 行（§4.7）は使わない。**同名のフォルダは hint だけでは区別できない**——フルパスは候補行の下段が示す（§4.5）
2. hint は**フォルダ内フィルタが空のときだけ**見える（入力欄に文字があるときはその文字列が表示される）。1 文字打つと hint は消える
3. `←` / `→` の直後、**列挙結果が届く前**から新しいディレクトリを示す（`current_dir` の書き換えは同期・候補の差し替えは非同期。§6.1 の as-built と対）
4. 列挙に失敗した場合（§6.6）も hint は表示され続ける
5. ツール選択中（§18.5）は hint がツール選択用の文言に切り替わる（優先度 tool > folder と一致）
6. ツール選択から Escape で folder へ戻ると、退避していたフォルダ内フィルタが復元されるため、hint が見えない状態で戻ることがある

**連番の制約**: `governance-check.mjs` の `G-spec-sections` が `### N.x` の連続性を検査する。6.6 の次は **6.7** でなければならない（6.8 を作ると落ちる）。§6 の中に足す限り `## N.` 側の連続性には影響しない。

### 1.3 検証手順（変更集合の一部として明示）

- `strings.rs` の新規ユニットテスト（上記）。**これが本 PR の唯一の自動検出器である。**
- カテゴリ D（目視）を**この PR 限り**で実施し、記録を PR 本文へ残す（恒久項目は足さない・§5）。目視項目は §4 に列挙。

### 1.4 「変更しない」と判断したもの（根拠つき）

| 対象 | 触らない根拠 |
|---|---|
| `layout.rs`（`Metrics` / `main_window_height`） | hint は入力欄の内側に描かれ、行を 1 つも増やさない。窓高の引数（`status_height` / `toast_height`）は不変 |
| `notify.rs`（`OverlayKind` / `overlay_kind`） | status 行の優先ラダーに新 variant を足さない（案 B を却下したため） |
| `results_view.rs` / `results_window.rs` / `RowsSnapshot` / `window_coordinator.rs` | results 窓は別 Context で `RowsSnapshot` 経由でしか main の状態を見ない。hint は main 窓内で完結し、snapshot に新フィールドが要らない |
| `launcher_controller.rs` | 新しい状態も遷移も増えない。`view.rs` が既存の `state()` 越しに読むだけ |
| `SPEC.md` §4.7（#700 の 2 文） | **意図的に触らない。** §4.7 が「描画面は status 行ただ 1 つ」と言っているのは*案内*（indexing / 起動中 / 一時通知）についてであり、フォルダの hint は案内ではなく本来のプレースホルダである。§4.7 は偽にならない。ここへ写しを足すと「文書に事実の写しを増やす変更」（AGENTS.md）に該当する。正本は §6.7 の 1 か所 |
| `src-tauri/CLAUDE.md` / `docs/architecture.md` | ファイルの追加削除が無く、モジュール索引・責務散文とも不変（`strings.rs` は「UI 文言テーブル」のまま） |
| `snotra-core/` 全体 | 文言は UI 層に置く規約（`strings.rs` の `//!`）。core は「UI 表示文字列を持たない」 |
| `scripts/smoke-egui.ps1` | trace イベント名・hotkey 登録・表示経路のいずれも変えない（AGENTS.md の該当トリガー不成立） |
| `scripts/manual-smoke.ps1` の `$items` | §5 で YAGNI 判定 |
| `snotra-core/src/ui_types.rs` の `FolderExpansionState` | 死んだ DTO だが本 issue の射程外（§2 で名指し・別掃除） |

---

## 2. 間接参照の洗い出し（概念で分類）

### 2.1 同名・別概念（同じ字面が別のものを指す）

| 字面 | 概念 A | 概念 B | 罠 |
|---|---|---|---|
| `current_dir` | `egui_shell::search_state::FolderFrame::current_dir`（**本 PR の対象**・現行の唯一の真実） | `snotra_core::ui_types::FolderExpansionState::current_dir`（**死んだ DTO**。WebView2/IPC 時代の serde 型で、SU7 撤去後は生成も消費もされていない。`pub` ゆえ dead_code 警告も出ない） | grep `current_dir` で 2 件出る。B を直しても画面は 1 ピクセルも変わらない。**B は触らない**（撤去は別 issue の話） |
| `hint` | 入力欄のプレースホルダ文字列（本 PR） | `VisualSnapshot::hint`（config `hint_text_color` 由来の**色**。`visual.rs`） | 「hint を変える」で色側を触りうる。色は不変 |
| `folder_hint` / `search_hint` / `indexing_hint` | プレースホルダ（前 2 者） | `indexing_hint` だけは **status 行に描かれる案内**であってプレースホルダではない（#700 で移設済み。名前だけが hint のまま残っている） | 「hint 系は全部プレースホルダ」と読むと #700 を巻き戻す |
| `folder_filter` | 入力欄に映る**フォルダ内フィルタ**（`buf` の folder 分岐） | `ToolFrame::saved_folder_filter`（退避コピー） | Escape 復帰で hint が見えなくなる経路の説明に必要（§1.2 の 6） |

### 2.2 同概念・別名（同じものが別の名前で現れる＝間接参照）

| 概念 | 現れ方 |
|---|---|
| 「フォルダ展開中である」 | `SearchState::view_kind() == ViewKind::Folder` ／ `folder.is_some()` ／ `view.rs` のローカル `in_folder` ／ SPEC の `FolderExpansionMode`（§8.6 の mermaid）／ SPEC 散文の「フォルダ展開モード」。**述語の SSOT は `view_kind()` 1 つ**で、`view.rs` は既に `in_folder` に束ねてある。新しい述語を作らない |
| 「現在のフォルダ」 | `FolderFrame::current_dir`（フィールド）／ `folder_current_dir()`（accessor）／ `parent_dir()`（`compute_parent_dir` 越しの**間接消費**——現状これが唯一の消費者で、`#[allow(dead_code)]` が生の accessor に付いている理由）／ 旧フロントの `folderState().currentDir` |
| 「プレースホルダ文言」 | `strings.rs` の `*_hint` 関数群 ／ 旧 `i18n.ts` の `search.placeholder.*` キー ／ SPEC には**現時点で 1 行も無い**（既定文言 `検索...` すら SPEC 未記載。§6.7 で folder 分だけ書くのは非対称だが、issue の受け入れ条件がそう要求している） |
| 「省略記号」 | `results_view::truncate_middle` の `'…'`（中間省略）／ egui の `TextWrapping::overflow_character` 既定 `'…'`（末尾省略・hint に効く）／ `platform/tray.rs` の ASCII `"..."`（トレイラベル）。**3 系統が既に共存している**ので「統一」しない |
| 「パスの末尾セグメントを取る」 | 本 PR で足す `folder_display_name(dir)` ／ **`view.rs` の tool 分岐に既にあるインライン `rsplit(['\\', '/']).next()`**（§18.5 の対象ファイル名表示・現 358–372 行）。**同概念・別名の最有力候補**であり `/dry-check` が指摘しうる。**それでも束ねない**のが私の判断: folder 側はルート（`C:\` / `\\server\share`）で自分自身へ倒す規則と末尾 `\` の正規化を持つが、tool 側の対象は必ずファイルでその 2 規則を持たない。束ねると tool 側に不要な分岐が入る。**この非対称を PR 本文に 1 行書いて、レビューでの再指摘を先回りする** |

### 2.3 コンパイラが検出しない箇所（本ブリーフの主眼）

1. **`#[allow(dead_code)]` の取り残し**。不要になった `allow` は警告を出さない。`-D warnings` でも沈黙する。**削除しても何も壊れないし、残しても何も鳴らない**——目視でしか捕まらない。
2. **`folder_current_dir` の rustdoc が能動的に嘘になる**（「driver は生の accessor を直接呼ばない」）。rustdoc を読む検査は 1 つも無い（`G-vocab` は規範文書だけが対象、`cargo doc` はリンク切れだけ）。
3. **同 rustdoc の「§6 で任意扱い」は現時点で既に嘘である。** 実測: `SPEC.md` §6（216–258 行）に hint・プレースホルダ・「任意」の記述は 1 件も存在しない。issue が言う「現在『任意扱い』と書かれている箇所」は **SPEC ではなくこの rustdoc** である。受け入れ条件 3 は「§6 に as-built を足す」＋「rustdoc の偽の主張を消す」の**両方**で満たされる。
4. **`strings.rs` の `//!` が課している規約**: 「文言を足す・直すときは計画書・レビュー引用の文字列を写さず、実物のソースを開いて codepoint 単位で確認する」。本 PR の実物は `git show 15933af^:ui/src/lib/i18n.ts` の 42/76 行（`"{dir} 内を検索..."` / `"Search in {dir}..."`）で、**末尾は ASCII 3 点**。この文書からコピペせず、必ず実物を開くこと。
5. **`G-spec-sections`（連番）と `§N` 参照実在検査**。`### 6.7` を足すのは可、飛ばすのは不可。他文書から `SPEC §6.7` と書くならその節が実在していること（本 PR では他文書に書かない方針）。
6. **`G-vocab`**: 規範文書の散文に、ソースに無い camelCase 識別子を書くと落ちる。SPEC に `folderState` / `currentDir` のような旧フロント語彙を書かない（snake_case は検査対象外）。
7. **カテゴリ D の目視項目**は PR 本文の表が SSOT で `scripts/manual-smoke.ps1` の `$items` がその写し。**増やすなら両方**（→ 増やさないと判定した・§5）。

---

## 3. 設計上の選択肢と推奨

### 3.1 どの面に出すか

| 案 | 内容 | 判定 |
|---|---|---|
| **A. 入力欄の hint**（推奨） | `{dir} 内を検索...` | **採用。** 撤去済み WebView2 の実装そのもので、issue 本人が貼ったスクリーンショットの挙動と一致する。描画面は 1 つ。`layout.rs` に触れない＝窓高が変わらない |
| B. status 行 | `OverlayKind` に 4 つ目を足す | **却下。** ① folder 突入のたびに main が `toast_height`（既定 43px）伸び、results 窓は main の直下に置かれるので **`←`/`→` のたびに候補一覧が 43px 跳ねる**。② 優先ラダーに載せる以上、indexing（数分）・起動中・一時通知の間はディレクトリが**消える**——「常に見える」という B 唯一の利点が要件時に失われる。③ 変更集合が `layout.rs`＋`notify.rs`＋そのテストへ広がる |
| C. results 窓の先頭行 | ヘッダ行を足す | **却下。** `RowsSnapshot` に新フィールドが要り、行インデックス（選択・クリック逆流・`rows_generation` 照合）の意味がずれる。得るものに対して危険が大きすぎる |

**hint が「フィルタを打つと消える」ことは A の欠陥ではない。** WebView2 版の `placeholder` も同じ性質を持ち、それが issue の言う「あるべき姿」である。加えて #743 の誤読が起きた局面（`←` 連打）では `enter_folder` / `navigate_folder` がどちらも `folder_filter.clear()` を呼ぶ（実測: `search_state.rs` 236–256 行）ため、**ナビゲーション直後は必ず hint が見える**。要件の中心はそこで満たされる。ただし §1.2 の 2 として as-built に書き、暗黙にしない。

### 3.2 何を出すか＝フルパスかフォルダ名か（**ここが唯一の実質的な設計判断**）

**一次資料が 2 つあり、食い違っている。**

- **issue コメント（2026-07-28・SSOT）**: 「メインウィンドウに**「（フォルダ名）内を検索…」**と表示する」——**フォルダ名**
- **撤去済みコード**（`SearchWindow.tsx` 277 行 + `i18n.ts` の doc 例 `t("search.placeholder.folder", { dir: "C:\\Users" })`）: `fs.currentDir` すなわち**フルパス**

私はコードでなく **issue の文言に従う**。SSOT はコメントであり、コードは撤去済みの過去の事実にすぎない。加えて次の実測が、その選択を「好みの一致」以上のものにする。

**egui の省略は末尾省略である**（実測: `builder.rs` 573–680 行。singleline の `wrap_mode` は `TextWrapMode::Truncate`、hint の text atom に `atom_shrink(true)` が付く＝**何もしなくても `…` で末尾省略される**）。フルパスを渡すと:

- **En** `Search in {dir}...` → 溢れた瞬間に**パスの末尾＝いま居るフォルダ名**が消える
- **JA** `{dir} 内を検索...` → まずラベル「内を検索...」が消え、**さらに溢れれば同じくパスの末尾が消える**。JA が安全なのは溢れが数文字のときだけで、**深いパスでは両言語ともリーフを失う**
- しかも**日本語のフォルダ名は CJK グリフで Latin の 1.0–1.8 倍幅を食う**（`truncate_middle` の rustdoc が実測として記す係数）。`C:\Users\<user>\Documents\プロジェクト\設計資料` のようなありふれたパスは、私の粗い見積もり（既定幅 600px・`font_size` 15 で可用幅 ≈ 570px ≒ Latin 70 文字）よりずっと手前で溢れる

つまり **`←` を繰り返して深く潜るほどリーフが消える**——これは #743 の誤読が起きたまさにその局面である。

| 案 | 判定 |
|---|---|
| **B-1. フォルダ名（リーフ）だけを出す** | **採用。** SSOT の文言そのもの。**リーフを構造的に失えない**（そもそも溢れない）。測定も切り詰めも `view.rs` の幅計算も不要。ルート（`C:\` / `\\server\share`）は自分自身を返す規則で表現する。代償は**同名フォルダの区別がつかない**ことで、これは §6.7 の as-built に書いて既知とする（フルパスは候補行の下段が示す） |
| B-2. フルパス + 既存 `truncate_middle` で事前に中間省略 | **次点。** 作者がフルパス表示を望むならこれ（**egui の末尾省略に委ねない**——中間省略ならリーフが残り、かつ切り詰めがユニットテスト可能な純関数側に来る）。`per_char_px` は `results_view.rs` と同様に実 galley から実測する。代償は `view.rs` が幅を測る数行を持つこと |
| B-3. フルパスを渡して egui の末尾省略に委ねる | **却下。** 受け入れ条件 1 を**破りうる唯一の案**である（上記のとおり両言語でリーフが消える）。「既定幅なら大抵収まる」は日本語のパスでは成り立たない |

**新しい切り詰めヘルパーは書かないこと**——B-2 へ倒す場合も `results_view::truncate_middle` を使う（`/dry-check` の指摘対象になる）。

### 3.3 i18n

`strings.rs` に 1 関数追加。パス自体は翻訳しない。JA/En でパラメータ位置が違う（先頭 / 中間）ので、`&'static str` 定数ではなく `format!` を返す既存の `update_available` / `launch_failed` と同じ形にする。

---

## 4. 検証手順

**カテゴリ A（必須・`docs/build-commands.md`）**

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p snotra                       # strings.rs / search_state.rs / view.rs は snotra crate
cargo doc --workspace --no-deps --document-private-items   # rustdoc を書き換えるため必須（hook 非発火）
```

- `.rs` 編集で PostToolUse hook が clippy と `cargo test -p snotra` を自動発火する（沈黙 = 合格）。**`cargo doc` は hook 非発火**なので手で打つ——本 PR は rustdoc を書き換えるため該当する。

**カテゴリ F（必須）**

```bash
npm run governance:check      # SPEC.md を触るため（G-spec-sections / G-references / G-vocab）
```

**カテゴリ C（トリガー不成立・ただし CI では走る）**

ウィンドウ生成・表示順・ホットキー・スラッシュコマンドのいずれにも触れないため、ローカル必須ではない。ただし `Smoke` workflow は `src-tauri/**` の **paths** で自動起動するため PR では走る。緑を「この機能が検証された」と読まないこと（smoke は hint を一切見ない）。

**カテゴリ D（目視・この PR 限り。PR 本文へ記録を残す）**

1. `cargo run -p snotra` → ホットキー → 1 文字打って候補を出す → `→` でフォルダを展開。**入力欄に `<展開したフォルダ名> 内を検索...` が出る**
2. 続けて `←` を数回。**打つたびに hint のフォルダ名が親のものへ変わる**（#743 の誤読が構造的に起きなくなったことの確認）。ドライブルートでは `C:\ 内を検索...` になり、そこで打ち止まる
3. **日本語名の深いフォルダ**（例: `C:\Users\<user>\Documents\プロジェクト\設計資料`）へ潜り、**hint が省略されずフォルダ名が最後まで読める**ことを見る（CJK 幅が効くのでここが最悪ケース。B-2 を選んだ場合はここで中間省略の `…` とリーフの残存を見る）。余裕があれば `general.language = "en"` でも同様に確認する
4. フィルタを 1 文字打つ → hint が消え打った文字が出る。Backspace で hint が戻る
5. 列挙に失敗するフォルダ（アクセス拒否・存在しない UNC）へ入り、**エラー行が出ても hint が出続ける**
6. folder 中に `Shift+Enter` でツール選択へ入る → hint が「ツールを選択...」に変わる → Escape で folder へ戻る（フィルタを打っていたなら hint は出ない＝as-built どおり）
7. **窓の高さが 1px も変わらない**（folder 突入前後で main の高さ・results 窓の位置が動かない）
8. 通常検索モードで hint が `検索...` のまま（回帰なし）。インスタントコマンド（`@`）中も同様

---

## 5. やりすぎ（YAGNI）と判定したもの

| 案 | 判定 | 理由 |
|---|---|---|
| **`scripts/manual-smoke.ps1` の `$items` へ恒久項目を追加** | **却下（最重要）** | 既存 13 項目はすべて**窓の協調不変条件**（I1–I13: 表示順・フォーカス・逆流・クランプ）で、自動検出器を持てないから人手に載せてある。本件の検出器は `strings.rs` のユニットテストと `view.rs` の 1 分岐であり、恒久項目は「毎サイクル人間の時間を課金し続けるが、ユニットテストが既に見ている性質を再確認するだけ」になる。項目表は PR 本文と script の**二重メンテ**でもある。この PR 限りの目視（§4）で足りる |
| **`governance-check.mjs` に新チェックを足す**（例: 「SPEC §6 と rustdoc の整合」） | **却下** | 母集団が 1 件しかない。1 件のために検査器を作ると、検査器自身が新しい保守対象になる（`.claude/rules/safety-nets.md` の射程へ入る） |
| **trace イベント（`egui_folder:dir` 等）の追加** | **却下** | 「trace の presence 検査は状態の検査ではない」（`src-tauri/CLAUDE.md`）。文字が描かれたことを trace は証明しない。smoke に足す口実にもならない |
| **`view.rs` のスクリーンショット回帰・`check:colors` 相当の自動判定の新設** | **却下** | 文字列の描画位置を pixel で判定する機構は本件に釣り合わない |
| **`FolderExpansionState`（`ui_types.rs`）の撤去を同 PR に含める** | **却下（別掃除）** | 死んでいるのは事実だが、`snotra-core` の公開 DTO 削除は本 issue の受け入れ条件のどれにも紐づかない。§2.1 で名指しするに留め、必要なら別 issue |
| **`folder_gen()` の `#[allow(dead_code)]` も一緒に外す/消す** | **却下** | 本 PR で消費者ができるのは `folder_current_dir` だけ。無関係な `allow` を触ると「意図的な非対称」を壊す |
| **status 行 or hint を config で切り替え可能にする** | **却下** | 誰も要求していない。設定キーは永続形式（`/persistence-check` の射程）を引き込む |
| **`strings.rs` の hint 群を enum テーブルへリファクタ** | **却下** | 1 関数追加のための構造変更 |

---

## 6. 実装者が踏みうる最大の罠と、それを避ける手続き

**罠**: egui の hint は **buf が空のときにしか描かれない**（`builder.rs` 584 行 `if text.as_str().is_empty() && !hint_text.is_empty()`）。実装者（またはレビュアー）はフィルタを 1 文字打った瞬間にディレクトリが消えるのを見て「受け入れ条件 1 を満たしていない」と読み、**非空フィルタ時だけ status 行にディレクトリを出す**フォールバックを足す。これが成立した瞬間、#700 とまったく同型の 2 面構成（同じ情報が入力欄と status 行を打鍵で飛び移る）が復活し、受け入れ条件 2 を破る。原理を唱えるだけでは止まらない——「情報が消える」という体験の方が強いからである。

**避ける手続き（diff の形で検算する。レビュアーが実際にやれるのはこれだけ）**:

この変更の diff は、次の 4 つを**すべて**満たしていなければならない。1 つでも破れていたら 2 面目が戻っている。

1. 新しい描画点は**ゼロ**。`allocate_exact_size` / `painter().text` / `painter().galley` の**追加が 1 件も無い**（既存の `hint` チェーンに分岐が 1 本増えるだけ）
2. `OverlayKind` の variant が増えていない。`overlay_kind` のシグネチャ（3 引数）が変わっていない
3. `main_window_height` の呼び出し引数（`has_status` / `has_toast`）が変わっていない。`layout.rs` の diff が空
4. `strings.rs` に増えた関数が **1 つだけ**（`folder_hint`）。「非空フィルタ用の別文言」が生えていない（`search_state.rs` に増える `folder_display_name` はパス演算であって文言ではない）

（この 4 条件は本 PR 限りの検算であって、恒久チェックリストとして機構化しない——§5。PR 本文にレビュー観点として 1 度書けば足りる。）

**副次の罠**（罠ではあるが上ほど致命的でない）: `has_status` を増やさずに新しい行を `ui` へ allocate すると、main 窓の高さは `bar_height` のままなので**描いた行が窓の外に落ちて見えない**。案 B を選ぶ人が必ず踏む。案 A を選べば構造的に踏めない——これも A を推す理由の 1 つ。

---

## 7. 自信の低い箇所・未検証の観点（正直に）

1. **溢れの境界幅を galley で測っていない**。「既定幅 600px・`font_size` 15 → 可用幅 ≈ 570px ≒ Latin 70 文字、CJK はその 1/1.0〜1.8」は**算術による見積もりであって実測ではない**（`AGENTS.md`「判定の中核は自分で測る」に照らせば弱い）。ただし §3.2 の結論は**境界がどこであれ成り立つ**——B-1 は溢れ自体が起きないため。境界値が効くのは B-2/B-3 を検討する場合だけで、そのときは §4 の目視 3 で実測すること。
2. **フルパスとフォルダ名のどちらを出すかは、一次資料が食い違っている**（§3.2）。私は issue コメントの文言（フォルダ名）を採ったが、作者が「WebView2 と同じフルパスがよい」と言えば B-2 へ倒す。**着手前に確認する価値がある唯一の問い**。
3. **`egui::RichText::new(String)` が現行の呼び出し形（`.font(bar_font)` 付き）でそのまま通るか**はソースの型定義から判断しただけで、コンパイルは通していない。
4. **`folder_hint` を JA/En 以外に増やす予定の有無**は未調査（`Language` は 2 値のみと確認済みなので現状は問題なし）。
5. **`#[allow(dead_code)]` を外したとき `cargo clippy -D warnings` が本当に無警告か**は未実行（`view.rs` から呼ばれる以上通るはずだが、`#[cfg(test)]` 側だけが呼ぶ状態を作ると落ちる——実装では必ず**非テストコードから**呼ぶこと）。
6. **`workspace/plan.md` を読んでいない**ため、他の分解が扱っている論点（例: 別の窓での提示・設定項目化）を私が「そもそも検討していない」可能性がある。これは指示どおりの独立性の代償である。
