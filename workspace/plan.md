# 実装計画 — #1134 列挙失敗行に、前の世代のフォルダアイコンが残って描かれる

ブランチ: `fix/error-row-icon-key`
調査: `workspace/research.md`（採用案は C: `SearchResult::icon_key` が `is_error` のとき `None` を返す）

## 1. 目的

`SPEC.md`「3.4 アイコン」が既に定めている「列挙失敗行は抽出せず placeholder も描かない」を、**要求側の 1 か所の条件ではなくキーの導出そのもの**で成立させる。これにより、前の世代で抽出したテクスチャがエラー行に引かれる経路（#1134）を表現不能にする。

## 2. 受け入れ条件

1. `is_error: true` の行は、`icon` が `FromPath` でも `Explicit` でも `SearchResult::icon_key()` が `None` を返す。
2. `icon_textures` に**既にそのキーのテクスチャが在る状態**から、`visible_icon_keys` → `retain_visible` → `icon_for_row` の連鎖を通しても、エラー行は `None` しか得ない。
3. 剪定を通していない（テクスチャが残っている）マップからでも、エラー行は `icon_for_row` で引けない。——**受け入れ条件 2 を保持側の挙動に依存させない**ため、独立して測る。
4. 通常行（`FromPath` / `Explicit`）の挙動は不変。`Skip` 行の挙動も不変。
5. `wanted_icon_keys` は引き続きエラー行を要求に載せない（既存テストが通り続ける）。
6. 描画側の「エラー行に placeholder を描かない」（`None if !result.is_error`）は残る。

## 3. 変更ファイルと対象シンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `snotra-core/src/ui_types.rs` | `SearchResult::icon_key` | `is_error` のとき `None` を返す（本体 1 か所）＋ doc 改稿 |
| `snotra-core/src/ui_types.rs` | `mod tests` | 導出そのものの検知器を 1 本追加 |
| `src-tauri/src/egui_shell/icon_textures.rs` | `wanted_icon_keys` | 冗長化した `if r.is_error { continue }` を撤去＋ doc 改稿（#1134 で撤去を約束した段落を含む） |
| `src-tauri/src/egui_shell/icon_textures.rs` | `visible_icon_keys` | doc 改稿（「広い側の代償」が消えたことの反映） |
| `src-tauri/src/egui_shell/icon_textures.rs` | ファイル冒頭の「行 → アイコンキーの読み」ブロックコメント | doc 改稿（不変条件の担い手が `icon_key` であることを明記） |
| `src-tauri/src/egui_shell/icon_textures.rs` | `mod tests` | 連鎖の検知器を 1 本追加＋既存テスト `wanted_icon_keys_never_loads_icons_for_error_rows` のコメント改稿 |
| `src-tauri/src/egui_shell/results_view.rs` | `request_icons_for_results` の導出コメント（:178-180） | `icon_key` が「どのキーか」だけでなく「そもそもキーを持つか」も決めることを反映（**コードは変えない**） |

| `docs/adr/ADR-error-row-icon-key-in-derivation.md` | （新規） | 却下した代替案 4 件とその理由（`workspace/` の削除で失う否定の知識・#593） |

`SPEC.md` は変更しない（→ §7）。

**`results_view.rs:366-368` のコメント（「通常の欠落のみ placeholder。エラー行には…描かない」）は真のまま残す**——不変条件 I2 の担い手であり、今回の変更で偽にならない。

### 変更で偽になる散文の走査（概念ラベルでの grep）

`エラー行` / `列挙失敗` を `*.md` と `*.rs` の全体へ当てた（識別子 `is_error` とは別に、概念ラベルで 1 回）。偽になるのは **`icon_textures.rs` の 4 箇所（:82 / :93 / :122 / :262）だけ**であり、いずれも上の一覧に載っている。

- `SPEC.md:87` / `:275` / `:290-295`、`launcher_controller.rs` の 6 箇所、`results_view.rs:366` / `:410`、`search_state.rs` の 3 箇所は**真のまま**（起動抑止・絞り込み非適用・レイアウトの話であり、アイコンキーに触れていない）。
- `docs/superpowers/plans/` と `docs/superpowers/specs/` の該当行は**日付つきの歴史的記録**であり更新しない（ADR と同じ扱い）。

### コミットの粒度

**Phase 1〜4 は 1 コミットで着地させる**（`AGENTS.md`「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」と同じ理由）。Phase 1 だけのコミットは赤いテストを main の履歴へ残し、Phase 2 だけのコミットは `wanted_icon_keys` の `continue` 撤去と `icon_key` の修正が分かれて中間状態を作る。

## 4. 実装順序

### Phase 1 — Red（検知器を先に落とす）

- [x] 1. `src-tauri/src/egui_shell/icon_textures.rs` の `mod tests` へ連鎖の検知器を追加する。

```
error_row_never_resolves_a_texture_from_a_previous_generation
  ── 前提: icons に "C:\\Windows" のダミー値を入れておく（前の世代で通常のフォルダ行
     として抽出済みの状態を模す）
  ── 行: is_error=true / path="C:\\Windows" / icon=FromPath の 1 行（`folder::error_result` と同形）
  ── (a) visible_icon_keys(&rows) がそのキーを含まない
  ── (b) retain_visible 後に icons が空である
  ── (c) **剪定を通していない**別のマップからでも icon_for_row が None を返す
  ── (d) icon=Explicit のエラー行でも (c) が成り立つ
```

- [x] 2. `snotra-core/src/ui_types.rs` の `mod tests` へ導出そのものの検知器を追加する（`icon_key_is_none_for_error_rows`。`FromPath` と `Explicit` の両方）。
- [x] 3. 2 本とも**落ちることを確認する**（`cargo test -p snotra-core -q` / `cargo test -p snotra -q`）。

### Phase 2 — Green（最小実装）

- [x] 4. `SearchResult::icon_key` の先頭で `is_error` なら `None` を返す。
- [x] 5. `wanted_icon_keys` の `if r.is_error { continue }` を撤去する。
- [x] 6. Phase 1 の 2 本と既存テスト（とくに `wanted_icon_keys_never_loads_icons_for_error_rows`）が緑になることを確認する。

### Phase 3 — 散文を実装へ合わせる

- [x] 7. `icon_key` の doc を改稿し、`is_error` を折り込む理由（`SPEC.md`「3.4 アイコン」）を書く。
- [x] 8. `wanted_icon_keys` の doc から「要求を止めても絵が出ない保証にはならない」の段落と、`is_error` 行を弾く責務の記述を撤去する（#1134 の撤去約束）。**責務は「重複排除と抽出要否の判定」である旨を簡潔に書く**——`icon_key` が担い手であることをここで名指ししない（2026-08-19 のユーザー裁定。逐語: "重複排除と抽出要否の判定である旨を簡潔に書いてほしい"）。導出が 1 つであることはファイル冒頭のブロックコメント（Phase 3-10）が持つ。
- [x] 9. `visible_icon_keys` の doc から「広い側の代償」の記述を撤去し、`is_error` で狭めない理由（通常行の往復回避）だけを残す。
- [x] 10. ファイル冒頭のブロックコメントへ、`icon_key` が「どのキーか」だけでなく「そもそもキーを持つか」も決めることを書く。
- [x] 11. `results_view.rs` のアイコン枝のコメントと、既存テスト `wanted_icon_keys_never_loads_icons_for_error_rows` のコメントから、「描画側に `is_error` ガードが無いのでここが最後の砦」という趣旨の前提を外す。
- [x] 12. 全称表現を使わない（`AGENTS.md`「検証の作法」）。「〜する経路は存在しない」ではなく「エラー行はキーを持たないので、要求・保持・引きのいずれもエラー行を扱わない」と肯定形で書く。

### Phase 4 — 検証と変異注入

- [x] 13. カテゴリ A（`docs/build-commands.md`）を実行する。
- [x] 14. doc コメントを触るので `cargo doc --workspace --no-deps --document-private-items` を手で打つ（hook 非発火・カテゴリ A の必須行）。
- [x] 15. `.rs` のコメントが `SPEC.md` の見出しを参照するので `npm run governance:check` を実行する。
- [x] 16. **変異注入**: `icon_key` の `is_error` 判定を一時的に戻し、Phase 1 の 2 本が**赤になること**を実測する（緑のままなら検知器が何も測っていない）。注入は必ず巻き戻す。
- [x] 17. **`wanted_icon_keys` 側の変異も測る**: `icon_key` を直したうえで、既存テスト `wanted_icon_keys_never_loads_icons_for_error_rows` が**導出経由で**通っていることを、`icon_key` の `is_error` を戻したときに赤くなることで確かめる（16 と同じ注入で同時に観測できるなら 1 回でよい。観測できないなら別途測る）。

### Phase 5 — レビューと fix-forward（実装中に判明した作業・その場で追記）

- [x] 18. `/symmetric-check` を実行する（テクスチャの生成/破棄の対称性）。**結果**: `results_view.rs:649-654` で `icon_textures` / `icon_attempts` / `icon_pending` が**同じ `visible` 集合**で刈られており、`icon_attempts` が増えるのは `Missing` のときだけ（:588）。成功して落ちたキーは試行回数を持たないので積み直せる——対称性は保たれている。適用漏れ 0 件。
- [x] 19. `code-reviewer` ラウンド 1。**コード欠陥 0 件**、散文 6 件（High 1 / ⚠️ High の対 1 / Medium 1 / Low 3）を全件採用して修正。引用された規約 2 件（`docs/comment-guidelines.md:24` / :31）は自分で当たって逐語で実在を確認した。
- [x] 20. ラウンド 2（同一エージェントを `SendMessage` で継続）。6 件の解消を現物で確認、**新規 Medium 1 件**——「現在形は消えたが置換先が**未観測の過去形**」（issue #1134 自身が「実機で再現はしていない」と書いているのに「#1134 の観測」と書いた）。採用して過去の行為へ倒した。あわせて依頼した変異 3 種の実測で、ラウンド 1 の「母集団はこの 2 本が持つ」が**1 対 1 の対応**だったとレビュア自身が訂正した。
- [x] 21. ラウンド 3（母集団を修正差分の 1 文に限定）。**自分の監査で同型の再生を 1 件発見**——「その経路が**残っていること**を辿った」の目的語が現在形のままで、修正後は偽だった。3 度目の再生を止めるため置換ではなく**歴史の記述そのものを落とし**、機序は #1134 への参照へ委ねた。**ラウンド 3 の判定は一つ前の版に当たっていた**（`stat` の mtime 10:24:37 > 相手の読み 10:23。こちらが「触らない」と言った後に編集した落ち度）ので、現行版を対象にラウンド 3b を依頼し**新規 0 件**を得た。
- [x] 22. `docs/adr/ADR-error-row-icon-key-in-derivation.md` を起こす（`workspace/` の削除で失う**否定の知識**＝却下した代替案 4 件とその理由）。
- [x] 23. **旧構造を再現する変異で対照を測る**（レビュアが「読み取りによる裁定」と断った論点を自分で測った）。要求側の条件を戻し導出の折り込みを外すと、**`wanted_icon_keys_never_loads_icons_for_error_rows` は緑のまま `error_row_never_resolves_…` だけが赤**（2026-08-19 実測）。**旧テスト一式が緑のままバグが生きていた**ことの直接の証拠であり、連鎖検知器を別に置く根拠。結果を当該テストの doc へ記録した。注入はバックアップから復元し、痕跡 grep 0 件・全テスト緑を確認済み。**この 1 段落はラウンド 3b の後に足したので、4 カテゴリ（新しい現在形・数え上げ・双条件・未観測の観測）を自分で当てて 0 件を確認した**（過去形・実測済み・数を持たない・双条件でない）。

## 5. 不変条件と異常系

- **不変条件 I1**: エラー行はアイコンキーを持たない。担い手は `SearchResult::icon_key` ただ 1 つ。検知器は Phase 1 の 2 本。
- **不変条件 I2**: エラー行に placeholder を描かない。担い手は `draw_result_row` の `None if !result.is_error`。**I1 とは別の事実**であり、I1 を入れても消してはならない。検知器は無い（描画層・受容する残余。`AGENTS.md` が言う「その検査対象外は受容する残余」に当たる）。
- **不変条件 I3**: 通常行は従来どおりキーを持つ。検知器は既存の `icon_key_*` テスト群（`ui_types.rs` / `instant.rs`）と `wanted_icon_keys_*` 群。
- 異常系: `icon` が `Explicit` のエラー行（`folder::error_result` は作らないが型としては作れる）でもキーを持たないこと。Phase 1 の検知器で両方を測る。
- 破棄経路: `retain_visible` がエラー行の世代でテクスチャを 1 件落とす。**復帰経路は既存の `wanted_icon_keys` → worker であり、新設しない**。しかも復帰時の抽出は `commands/icon.rs` の `load_icon_pngs` が `IconCache`（`icons.bin` 裏付け）から返すため、シェル呼び出し（`SHGetFileInfoW`）にはならない（`icon.rs:49-69` を読んで確認済み）。

## 5b. 該当した条件別チェック（`AGENTS.md`）

### 「`Option` / フラグなど、どの分岐が選ばれるかを決める値の出所を変更」

下流 1 段を辿って「この値で初めて走る行」を列挙した（`research.md` §7 の 3 件）。検知器は Phase 1、**呼び忘れを再現する変異で落ちることまで**を Phase 4-16/17 で測る。

### 「重複した読み・冗長に見える状態を束ねる/消す」

消すのは `wanted_icon_keys` の `if r.is_error { continue }` 1 件。トリガーが要求する「**後で**読まれる/立つことに依存していないか」の書き出し:

- この `continue` が守っているのは「エラー行を worker に積まない」ことだけである。**後で読まれる状態を作らない**（`wanted` に載らない＝`icon_pending` にも入らない＝ drain も来ない）。
- 消した後にこの行の役割を引き受けるのは、同じ関数内の `let Some(key) = r.icon_key() else { continue }` である。**位置は同じループの 1 行下**であり、間に副作用は無い（`icon_key` は `&self` の読みだけ）。
- 消すことで「エラー行が `wanted` に載る」ようになる経路は、`icon_key` が `Some` を返す場合に限る。それは Phase 2-4 が塞ぐ当のものであり、**同じコミットで塞ぐ**（`AGENTS.md`「新 API の導入と呼び出し点の移行は 1 タスクに束ねる」と同じ理由）。

### 「検査・検証手段を新設する」

`docs/development-principles.md`「検証の層と、層と層の隙間」に従い、**穴が層の境界に空く**ことを先に書いておく。

| 層 | 何を測るか | 今回置く検知器 |
|---|---|---|
| 導出（`snotra-core`） | エラー行がキーを持たない | Phase 1-2（`icon_key_is_none_for_error_rows`） |
| 連鎖（`src-tauri` 純粋核） | 保持・剪定・引きがエラー行を扱わない | Phase 1-1（連鎖の検知器） |
| 描画（`results_view`） | エラー行に placeholder を描かない | **無し**（既存コードを変えないため、置いても今回の差分を検算しない） |

**境界に残る隙間を名指しする**: 「`icon_for_row` が返した `None` が `draw_result_row` の `None` 枝へ届いていること」は、どの層も見ない。**#1134 はまさにこの形の隙間だった**（要求側は測られ、描画側は測られ、その間が測られていなかった）。今回はその隙間を「導出が 1 つであること」で埋めるが、**検知器では埋めていない**——受容する残余であり、`icon_for_row` の呼び出し点が `results_view.rs:484` の 1 箇所であることが根拠である。

## 6. テスト方針と検証コマンド

```
cargo test -p snotra-core -q
cargo test -p snotra -q
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps --document-private-items
npm run governance:check
```

- 実機再現は行わない（issue 本文も「机上で経路を辿っただけ」と自認。権限不足ディレクトリを検索結果へ出して右カーソルで展開する手順が要る）。**接地は Phase 1 の連鎖検知器と Phase 4 の変異注入で行う**——`AGENTS.md`「主張は代理ではなく対象そのもので測ってから書く」に対し、対象は「連鎖の出力」であり、それは単体で測れる。
- kittest（描画層）の検知器は置かない。I2 の担い手は既存コードで変更しないため、置いても今回の差分を検算しない。

## 7. SPEC.md・関連文書の更新要否

- **`SPEC.md`: 変更しない。** §3.4 の「列挙失敗行（§6.6）はこの規則の対象外である。`path` が実在ディレクトリを指していても抽出せず、placeholder も描かない」は**観測される挙動を正しく記述しており**、今回の変更で偽にならない。機構がどこに在るか（`icon_key` か要求側か）は `SPEC.md` の管轄ではない（`AGENTS.md`「3層分担」: 意図は SPEC、実装事実はコード）。
- **`docs/architecture.md` / 各 `CLAUDE.md`: 変更しない。** アイコンキーの導出は横断パターンとして記録されておらず（`src-tauri/CLAUDE.md` の `icon_textures.rs` 行は「責務は `//!`」とだけ書く）、ファイルの追加・削除も無い。
- **`RETROSPECTIVE.md`: 触らない**（サイクル末の `/retrospective` の管轄）。

## 未確定（実装前に潰す）

- [x] 敵対的調査（`workspace/adversarial-1134.txt`）の所見を受け取り、採否と理由を `research.md` §9 へ反映した。**壊せた項目は 0 件**。⚠️ 3 件のうち行番号のズレは採用し、**相手の数値を写さず自分で測って**訂正した（実測: `retain_visible` 68 / `icon_for_row` 132 / アイコン枝 350-370）。残る 2 件（C 実装後にしか測れない・実機未再現）は Phase 1 / Phase 4 の検知器と、受容する残余として処理する。
- [x] 命題「エラー行が出る世代の行はエラー行 1 行だけである」——**真**。`spawn_folder_load` の失敗枝（`launcher_controller.rs:830-833`）が `snotra_core::folder::error_result(dir)`（要素 1 の `Vec`）を `FolderMsg::Failed` へ載せ、`run_search_with` の Folder 枝（:870）が `set_results(err.clone())` で行を丸ごと差し替える。`set_results` の製品側呼び出し点は `launcher_controller.rs` の 10 箇所で、エラー行を運ぶのは :870 だけである。
- [x] 命題「`icon_key` の製品コードの呼び出し元は 3 箇所だけ」——**真**。LSP（rust-analyzer）は本セッションで応答しなかったため grep へ落とした。`.icon_key(` の 3 箇所（`icon_textures.rs:107` / `:125` / `:136`）に加え、**メソッド参照形**（`SearchResult::icon_key` を値として渡す形）も走査したが、当たったのは doc コメント 3 件のみ（`ui_types.rs:22` / `icon_textures.rs:74` / `results_view.rs:179`）。`icon_textures` の HashMap を `icon_for_row` を通さず引く箇所も無い。
- [x] C の代償の実体——`commands/icon.rs` の `load_icon_pngs`（:49-69）が `IconCache` を先に引くため、**再抽出はキャッシュヒットであって `SHGetFileInfoW` の呼び直しではない**。費用は worker 往復 1 回と PNG decode 1 回に閉じる。

（`results_view.rs` のコメント改稿が `SPEC.md` の見出し参照を壊さないことの確認は、書いた後にしか測れないため Phase 4-15 の作業項目とした。）

## 人間レビュー

- [x] 承認済み — 2026-08-19 / 問い: "採用案 C（`icon_key` が `is_error` を折り込む）で進めてよいか、それとも B（`icon_for_row` 側で見る）へ倒すか" / 回答: "C で進めて"

## plan-review 結果

- リスク: **高**（`SearchResult::icon_key` は `snotra-core` の `pub fn` であり、意味の変更は crate 間インターフェースの変更に当たる）
- レビュー方式: 計画準拠レビュー 1 体（`/plan-review` Step 2。観点を 2 つに絞って渡した）
- エージェント数: 1（Step 3b の敵対枠 1 体と合わせて計 2 体）
- 成果物: `workspace/plan-review-1134-icon-key.md`

### 要対処

なし。

### 軽微

なし。

### 未検証

- Phase 4-16 / 4-17 の**実行結果そのもの**（対象コードが未着地のため。実装後に実測する——計画の作業項目に入っている）。
- レビュアは `cargo test` を走らせていない（本人が申告）。現行ソースを読んで Phase 1 の (a)(b)(c)(d) を手でトレースし、4 つとも修正前に落ちることを確認したという報告である。

### 主エージェントによる再照合（要対処が 0 件でも、根拠は自分で測る）

| レビュアの主張 | 自分で当てた手段 | 結果 |
|---|---|---|
| `snotra-egui-runtime` / `snotra-settings` は `SearchResult` を触らない | `grep -rn "SearchResult" snotra-egui-runtime/src snotra-settings/src` | ヒット 0。真 |
| 既存テストの `row()` ヘルパーは `is_error: false` 固定 | `icon_textures.rs:225` を直読 | 真 |
| 「層の境界の隙間を検知器なしで受容する」は既存の `icon_gate_keeps_input_idle_semantics` と同じ正当化パターン | `grep -rn` で当該テストの実在を確認（`icon_textures.rs:378`） | 実在する。**ただし正当化の根拠として採るのは「`icon_for_row` の呼び出し点が 1 箇所であること」であって、先例の存在ではない**（先例は機序の説明であり、所見そのものではない） |

### 判断

- 実装着手: **可**（人間の承認を得たのち）

## セルフレビュー

- リスク: **高**
- plan-review: 独立レビュー 1 体（Step 2・計画準拠）
- エージェント数: **2**（Step 3b の敵対枠 1 体 + plan-review 1 体）
- 要対処: **0 件**。敵対枠の ⚠️ 3 件のうち行番号ズレは `research.md` へ反映済み（自分で測り直した）。残る 2 件は Phase 1 / Phase 4 の検知器と、受容する残余として処理した
- 未検証: Phase 4-16 / 4-17 の実行結果（実装後にしか測れない）。実機再現（issue 本文も未実施・接地は連鎖検知器で行う）

### Step 5a の 5 点照合（主エージェント自身）

1. **issue の全要件に作業項目が対応する** — 機構の決定（§6 で 4 案を比較し C を採用）/ 修正（Phase 2）/ `wanted_icon_keys` の doc 段落の撤去（Phase 3-8。issue が「閉じたら更新する」と名指ししたもの）/ 連鎖の検知器（Phase 1-1。issue が「要求側だけでは足りない」と名指ししたもの）。**4 件すべてに対応がある**
2. **境界条件と検証** — `FromPath` のエラー行 / `Explicit` のエラー行 / 通常行 / `Skip` 行 / テクスチャが既に在る状態 / 剪定を通していないマップ。Phase 1 の (a)〜(d) と既存テスト群が各 1 件以上を持つ
3. **新しい状態・リソースの正常/失敗/破棄経路** — 新設は無い。唯一の破棄（`retain_visible` がテクスチャを 1 件落とす）には既存の復帰経路があり、しかも `IconCache` 経由でシェル呼び出しにならないことを実測で確認した（§5 の破棄経路）
4. **より単純な既存パターンで置き換えられないか** — 案 A（描画側 1 行）は確かに単純だが、**検知器が単体で置けなくなる**ため却下した（§6 に理由を記録）
5. **壊してはならない不変条件に検知手段がある** — I1 は検知器 2 本 + 変異注入、I3 は既存テスト群。**I2 だけは検知器を持たない**（描画層・コードを変えないため置いても今回の差分を検算しない）——受容する残余として §5b に明示した
