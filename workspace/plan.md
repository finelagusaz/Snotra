# plan: #949 テーマ 3 値の適用を `search_input_ui` へ吸収し、順序の不変条件に検知手段を与える

## 目的

#751 が新設した**順序**の不変条件（適用は visuals を読む最初の操作より前）を、規範から
**構造 + テスト**へ移す。適用を 3 値の唯一の消費者を描く `search_input_ui` の入口へ吸収すれば
位置は関数の入口に固定され、同一 pass の子 `Ui` を実コードのまま観測して守れる。

## 受け入れ条件

1. 3 値の適用が `search_input_ui` の入口に在り、`SearchWindowView::update` から消えている
2. 新テストが `search_input_ui` を実コードのまま **1 pass だけ**走らせ、`hint` クロージャが
   受け取る子 `Ui` の 3 値を実測して期待値と一致することを assert する
3. 次の 5 変異それぞれでそのテストが**落ちる**ことを実測する（変異ごとに「実際に起きうる編集の
   姿と一致するか」を 1 行書いてから測る・`.claude/rules/safety-nets.md`「効いていることは、
   フォールトインジェクションで一度は実測する」）
4. `src-tauri/CLAUDE.md` の「**この順序に検知手段は無い**」が実態と合う記述へ更新され、
   新 ADR が `ADR-visuals-application-target` の却下 4 を覆したことを記録する
5. 描画結果が変わらない（カテゴリ A 全緑 + 非既定色での目視）

### issue の「これが落とすもの」に対する訂正（実測に基づく）

issue は 3 つの回帰を挙げるが、うち **`ctx.set_visuals` へ戻す形は新テストの守備範囲に数えない**
——#900 の `disallowed-methods` が既に塞いでおり、`-D warnings` 下ではテストが走る前に
コンパイルが赤くなる（`src-tauri/clippy.toml:51-57` 実読）。代わりに issue が挙げていない
**「適用を `update()` へ戻す（移設の巻き戻し）」**を守備範囲へ加える——これが新テストの
最も価値ある検知である。

## 技術的前提（すべて実測・実読で確認済み）

| 前提 | 確認方法 | 結果 |
|---|---|---|
| 子 `Ui` の**生成より後ろ**の適用は届かない | 一時テストを `view.rs` へ足して `cargo test -p snotra tmp949`（2026-08-06・測定後に revert 済み） | 観測値は egui 既定色 `(#0A0A0A, #005C80, #545454_99)` で期待値と不一致 ＝ **順序の検知が成立する** |
| 子 `Ui` の**生成より前**の適用は届く | 既存テスト `ui_visuals_mut_reaches_child_ui_in_the_same_pass`（`view.rs:1378`）が `Frame::new().show` の子で実証。`search_input_ui` の `.inner_margin()` は `Frame` の余白であって style 継承に触れない | 届く |
| 3 値の消費者は `search_input_ui` の TextEdit だけ | `view.rs` 本体で `ui.label` / `ui.button` / `ui.add` が 0 件（grep 実測。テストモジュール内 3 件のみ）。status overlay・toast は `ui.painter()` へ色を明示渡し。results は別 `Context` | 移設しても描画結果は変わらない |
| `ctx.set_visuals` への巻き戻しは clippy が塞ぐ | `src-tauri/clippy.toml:50-58` 実読（7 メソッド） | テストで二重に守る必要が無い |
| SPEC.md の同期は不要 | `SPEC.md:633` と `:649` が「機序は `view.rs` の TextEdit 構築部コメントが正本」と**散文形**で指す。そのコメントは移設後も同じファイルに残り、挙動（どの色がどこに出るか）も変わらない | 不要 |

## 変更ファイル一覧と対象シンボル

| ファイル | シンボル | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/view.rs` | `InputVisuals`（新規 `struct`） | 3 値（`input_bg` / `selection` / `hint`）を運ぶ |
| 〃 | `search_input_ui`（`:220`） | 第 2 引数に `InputVisuals` を追加。関数の**入口**で 3 値を適用。doc へ順序不変条件の正本を移す |
| 〃 | `SearchWindowView::update`（`:503-510`） | 適用 3 行を削除し、`InputVisuals` の構築へ置換（`:688` の呼び出しへ渡す） |
| 〃 | `update` の適用点コメント（`:480-498`） | 削除。#751 の機序（root `Ui` が pass 冒頭で `Arc` snapshot する）は `search_input_ui` の doc へ移す |
| 〃 | `update` の適用点コメント（`:499-502`） | **別扱い**。`panel_fill` / `window_fill` を意図的に設定していない記録（spec 決定 2 由来）は 3 値と独立の事実で `search_input_ui` の doc にそぐわないため**モジュール doc `//!` へ移す**。同じ段の「同じ grep が `ctx.set_visuals` を落としてよい根拠」は #900 が禁止を機構化した今は無用ゆえ**削除する** |
| 〃 | `update` の `search_input_ui` 呼び出し直前（`:687` 相当） | **新設**。旧 `:494-497` と同格の警告を置く——「ここより前で新しいウィジェット・子 `Ui` を作るなら、visuals を読まないことを確かめるか、自分で visuals を渡すこと」。移設で危険域が `ui.interact` 1 箇所から `search_input_ui` 呼び出しまでの全域へ**広がる**ため、警告の着地点が要る |
| 〃 | `read_visual` 近傍コメント（`:409-413`） | 「適用は別の位置に散る」の列挙で 3 値の行き先を `search_input_ui` へ更新 |
| 〃 | モジュール doc `//!`（`:9-24`） | 反映境界の記述で `ui.visuals_mut()` の呼び出し点を更新 |
| 〃 | TextEdit 構築部コメント（`:660`） | 「適用は `ui.visuals_mut()` 側」→ `search_input_ui` の入口 |
| 〃 | `ui_visuals_mut_reaches_child_ui_in_the_same_pass` の doc（`:1374-1376`） | 「順序不変条件には検知手段が無い」→ 新テストとの役割分担 |
| 〃 | `search_input_ui_applies_theme_values_to_child_ui_in_the_first_pass`（新規テスト） | 追加 |
| `src-tauri/CLAUDE.md` | 「モジュール構成」の `egui_shell/` 項（テーマ色の読みの段） | **同じ段の 2 文が偽になる**——「この順序に検知手段は無い」と「機構化されたのは禁止だけで、下の順序は依然として規範しか持たない」。検知器の所在（`search_input_ui` の doc）へ書き換え、却下案の参照に新 ADR を併記。**同段の `ui.interact` 例外の一文も更新する**——例外は `ui.interact` 1 箇所ではなく `search_input_ui` 呼び出し前の `update()` 全域になる |
| `docs/adr/ADR-visuals-order-detector-at-choke-point.md` | 新規 | 却下 4 を覆した記録。**旧 ADR は書き換えない**（`ADR-adr-frozen-history`） |

## 実装順序

**Phase 1（移設）→ Phase 2（テスト + 故障注入）**の順にする。新テストは移設後の
シグネチャに依存するため、移設前には書けない——「落ちるテストから」の役割は
**Phase 2 の故障注入が果たす**（変異を当てて赤を見てから戻す）。

## 不変条件と異常系

- **3 値の唯一の消費者は `search_input_ui` の TextEdit である**（上表で数え上げ済み）。
  移設は描画結果を変えない
- **移設は 3 値の到達範囲を縮め、未テーマ化の区間を広げる（新しい受容残余）**: 現状は `update()`
  冒頭適用ゆえ以後の全描画へ届くが、移設後は `search_input_ui` 以降に限られる。裏を返すと
  「テーマ未適用のまま `update()` が走る区間」は `ui.interact`（`:397`）1 箇所から
  `search_input_ui` 呼び出し（`:688`）までの全域へ**広がる**。現在その区間に visuals を読む
  描画は 1 つも無いので実害はゼロだが、`ADR-visuals-application-target` の帰結 2（main 窓へ
  新しい egui コンテナを足すなら自分で visuals を渡す）が**より広く当てはまる**ようになる
- **新しい検知器はこの拡大した区間の退行を捕まえない（明示すべき偽陰性）**: 新テストは
  `search_input_ui` を `ctx.run_ui` で単独に駆動するので、`update()` 側でその区間へ visuals を
  読むウィジェットを足す編集は**原理的に見えない**。守るのは「`search_input_ui` 内部の順序」
  だけである——**全称表現を前提条件とセットで書く**（`AGENTS.md`「検証の作法（全タスク共通）」）。
  残余は in-code の警告（`:687` 相当）・新 ADR・`src-tauri/CLAUDE.md` の 3 点で受ける
- **観測点を `hint` クロージャに置く代価**: 本来必要なのは「子 `Ui` の生成より前」だが、この
  観測点は「`hint` の呼び出しより前」まで縛る。適用を関数の入口へ置く限り偽陽性は出ないが、
  将来 `hint` より後ろへ適用を動かす正当な理由が出たら偽陽性になる。テストの doc に書く
- 異常系: 無い。実行時の失敗経路を増やさない純粋な内部構造の変更である

### 意図的に分けた構造（レビューへ先渡しする・ルート `CLAUDE.md`「サブエージェント委譲と worktree」）

- **`InputVisuals` を `SearchInputParams` へ吸収しない**: 前者は「関数が `ui` へ**適用**する値」、
  後者は「`TextEdit` へ**渡す**値」で、`InputVisuals` だけが順序不変条件を負う。issue が
  シグネチャの形を名指しで提案しており、それに従う
- **既存テスト `ui_visuals_mut_reaches_child_ui_in_the_same_pass` を残す**: 新テストが落ちた
  とき「egui が変わった」か「製品コードが変わった」かを切り分ける対照である。#872 が
  `focus_requested_before_text_edit_applies_same_frame_input`（egui の意味論）と kittest 検査
  （製品の並び）を併存させ、doc で役割分担を明記したのと同型

## テスト方針と検証コマンド

新テストは **kittest ではなく素の `ctx.run_ui` を使う**——初回 pass であることが症状の成立条件
そのものであり、`egui_kittest::Harness` は構築の時点で既に 1 フレーム走らせる（`view.rs:1320-1322`
の実測コメント）。既存 `ui_visuals_mut_reaches_child_ui_in_the_same_pass` と同じ形になる。

故障注入の 5 変異（各回 `cargo test -p snotra search_input_ui_applies_theme_values` で測る）:

| # | 変異 | 実際に起きうる編集の姿か |
|---|---|---|
| i | 適用を `Frame::show` の呼び出しより**後ろ**へ移す | ○ 順序の回帰そのもの（移設後の「visuals を読む最初の操作より後ろ」はこの形になる） |
| ii-a | `extreme_bg_color` の代入を消す | ○ 3 値のうち一部の適用漏れ |
| ii-b | `selection.bg_fill` の代入を消す | ○ 同上 |
| ii-c | `weak_text_color` の代入を消す | ○ 同上 |
| iii | 適用を `search_input_ui` から `update()` へ戻す | ○ 移設の巻き戻し（レビューで「元の位置の方が読みやすい」と言われる形） |

**判別力は実質 4 種類である**（独立レビュー指摘・軽微）。変異 iii は新テストが `search_input_ui` を
直接駆動する構造ゆえ、観測結果としては ii-a + ii-b + ii-c を同時に当てた状態と機械的に区別が
付かない。それでも別項として測るのは**当てる編集の姿が違う**からであり、「5 通りの異なる
コードパスを検査している」という含意は持たせない。

検証コマンド（`docs/build-commands.md`）:

- **カテゴリ A**: `cargo fmt --all -- --check` / `cargo check --workspace` /
  `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` /
  `cargo doc --workspace --no-deps --document-private-items`（doc コメントを触るため必須）
- **カテゴリ F**: `npm run governance:check`（`CLAUDE.md` と新 ADR を触るため）
- **カテゴリ D**: `npm run check:colors`（背景の自動判定）＋ **非既定色**で起動して入力欄背景・
  hint 色・選択色を目視する。**`check:colors` が自動判定するのは背景だけで 3 値は目視項目である**
  ——エージェントは `-Interactive` 起動と窓矩形キャプチャで実施できる（#836 / #870 の実績。
  画面ロック中は不可）。実施できなければ人間へ依頼し、PR 本文へ記録する

## SPEC.md・関連文書の更新要否

| 文書 | 要否 | 根拠 |
|---|---|---|
| `SPEC.md` | **不要** | 上の技術的前提の表（§11 の参照は散文形で、指す先も挙動も変わらない） |
| `src-tauri/CLAUDE.md` | 必要 | 「この順序に検知手段は無い」が偽になる |
| `docs/adr/`（新 ADR） | 必要 | 却下 4 を覆す決定の記録 |
| `docs/adr/ADR-visuals-application-target.md` | **書き換えない** | `ADR-adr-frozen-history`「ADR は凍結された歴史」 |
| `src-tauri/clippy.toml` | 不要 | 禁止の機構は変わらない。reason の「run_ui の中では ui.visuals_mut() を使う」は真のまま |
| `docs/architecture.md` | 不要 | 横断パターンの変更ではない |
| `docs/development-principles.md`「構造的設計原則と強制の階梯」 | **不要（原則は変わらない）が、新 ADR から正準形で引く** | 同節の「『検知手段が無い』と書きかけたら、検知可能になる設計へ変えられないか先に問う」の **(a) 責務を検査可能な層へ移す**が本 issue の設計そのものである（先例として #654 が既に挙がっている）。実例を書き足すと事実の写しが増えるので足さない |
| `docs/superpowers/specs/2026-07-28-config-background-color-design.md`（§7）/ `2026-07-27-666-launcher-controller-main-view-design.md`（§3.8） | 不要 | どちらも「#751 は未解決のまま残る」と書いたまま #751 の修正時に更新されていない ＝ ADR と同じく**歴史的記録**として扱われている慣行（grep 実測） |
| `src-tauri/src/egui_shell/results_view.rs:423` | 不要 | 「先例と理由は `view.rs` の `search_input_ui`」は移設で**強まる**方向であり偽にならない |
| `src-tauri/Cargo.toml:52-55`（kittest の dev-dependency コメント） | 不要 | 新テストは kittest ではなく素の `ctx.run_ui` を使うため、kittest が縛る対象（キャレット・focus・構築の並び）は変わらない |

## 作業項目

### Phase 1 — 移設

- [ ] `view.rs` に `InputVisuals`（`input_bg` / `selection` / `hint` の 3 フィールド）を定義する
- [ ] `search_input_ui` の第 2 引数へ `InputVisuals` を足し、**関数の入口**（`Frame::new()` の
      呼び出しより前）で 3 値を `ui.visuals_mut()` へ適用する
- [ ] `update()` の適用 3 行（`:503-510`）を削除し、`InputVisuals` の構築と受け渡しへ置換する
- [ ] `cargo test -p snotra` が既存 4 検査を含めて緑であることを確認する

### Phase 2 — 検知手段

- [ ] `search_input_ui_applies_theme_values_to_child_ui_in_the_first_pass` を追加する
      （`ctx.run_ui` 1 pass・`hint` クロージャで子 `Ui` の 3 値を記録して突き合わせる）
- [ ] 5 変異それぞれを当てて**落ちる**ことを実測し、測定結果（変異・exit code・失敗メッセージ）を
      記録する。**稼働中のテストを弱めない**——変異は当てたら必ず戻す
- [ ] 変異ごとに「実際に起きうる編集の姿と一致するか」を 1 行で判定し、一致しない変異があれば
      その変異を捨てて別の形を探す

### Phase 3 — 文書

- [ ] `search_input_ui` の doc へ順序不変条件の正本を移す（#751 の機序・観測点の代価・
      新テストが守る範囲と**守らない範囲**）
- [ ] `update()` 側の残骸コメントを整理する。**移設で「既知の正しさ」を落とさない**
      ——現行コメントが持つ命題を 1 つずつ移設先で数え上げてから消す。着地先は次のとおり:
  - [ ] `:409-413` の「適用は別の位置に散る」の列挙 → 3 値の行き先を `search_input_ui` へ更新
  - [ ] `:660` の「適用は `ui.visuals_mut()` 側」→ `search_input_ui` の入口
  - [ ] `:480-498`（#751 の機序と順序不変条件）→ `search_input_ui` の doc
  - [ ] `:499-502` のうち **`panel_fill` / `window_fill` を意図的に設定していない記録 →
        モジュール doc `//!`**（3 値と独立の事実で `search_input_ui` にそぐわない）。
        同段の「同じ grep が `ctx.set_visuals` を落としてよい根拠」は #900 の機構化で
        無用になったので**削除する**
- [ ] `update()` の `search_input_ui` 呼び出し直前へ、旧 `:494-497` と同格の警告を新設する
      （危険域が広がったことへの着地点。文言は「ここより前で新しいウィジェット・子 `Ui` を
      作るなら、visuals を読まないことを確かめるか、自分で visuals を渡すこと」）
- [ ] `ui_visuals_mut_reaches_child_ui_in_the_same_pass` の doc を役割分担の記述へ更新する
- [ ] モジュール doc `//!` の反映境界の記述を更新する（上の `panel_fill` の移設先でもある）
- [ ] `docs/adr/ADR-visuals-order-detector-at-choke-point.md` を新設する。含めるもの:
      決定 / 旧 ADR の却下 4 を覆すこと（旧 ADR は**書き換えない**）/ 却下した代替案 /
      **受容する残余 3 点**（未テーマ化区間の拡大・新テストがそれを見ないこと・観測点を
      `hint` クロージャに置く代価）/ `docs/development-principles.md`「構造的設計原則と強制の
      階梯」の (a)「責務を検査可能な層へ移す」を正準形で引く
- [ ] `src-tauri/CLAUDE.md` の該当段を更新し、却下案の参照に新 ADR を併記する
      （偽になる 2 文と `ui.interact` 例外の一文・上の変更ファイル一覧を参照）

### Phase 4 — 検証

- [ ] カテゴリ A を全て実行する（`cargo doc` を含む）
- [ ] `npm run governance:check` を実行する
- [ ] `npm run check:colors` を実行する
- [ ] 非既定色で起動し、入力欄背景・hint 色・選択色を目視する（不可なら人間へ依頼し理由を記録）
- [ ] 実装差分を確定させる（`git diff` で移設漏れ・コメントの取り残しを読み直す）

## 未確定（実装前に潰す）

- [x] 子 `Ui` の生成より後ろへ置いた適用が届かないこと（＝順序の検知が成立すること）
      — 2026-08-06 に一時テストで**実測**。観測値 `(#0A_0A_0A_FF, #00_5C_80_FF, #54_54_54_99)`
      は egui 既定色であり期待値と不一致。測定後に一時テストは revert 済み（`git status` clean で確認）
- [x] 3 値の消費者が `search_input_ui` の TextEdit だけであること — grep で数え上げ済み
      （`view.rs` 本体の `ui.label` / `ui.button` / `ui.add` が 0 件）
- [x] `ctx.set_visuals` への巻き戻しを新テストの守備範囲に数えるか — **数えない**。
      `src-tauri/clippy.toml:50-58` を実読し、`-D warnings` 下でコンパイルが先に赤くなることを確認
- [x] `SPEC.md` の同期要否 — `SPEC.md:633` / `:649` を実読し**不要**と判定
- [x] 旧 ADR を書き換えるか — **書き換えない**。`ADR-adr-frozen-history` が「ADR は凍結ゆえ
      編集しない——それ自体が本契約の初適用である」と先例を残している

## セルフレビュー

- リスク: **高**（`/plan-review`「リスク判定」の「hook、CI、rules、skills、ガバナンス文書を
  変更する」に該当——`src-tauri/CLAUDE.md` と新 ADR）
- plan-review: 独立レビュー 1 体（Step 2・観点は「検知器の実効性」と「消える散文の着地」の 2 つ。
  後者は**逆向きの監査**枠でルート `CLAUDE.md`「サブエージェント委譲と worktree」に従い
  `git log -S` / `git blame` を道具として渡した）。結果は `workspace/plan-review-visuals-choke-point.md`
- エージェント数: 1
- 要対処: **2 件・いずれも根拠を再照合して成立**。両方を計画へ反映済み
  1. 移設で「未テーマ化の区間」が `ui.interact` 1 箇所から `search_input_ui` 呼び出しまでの
     全域へ広がるのに、旧 `:494-497` と同格の in-code 警告の着地先が無かった。かつ新テストは
     この退行を原理的に見ない → 変更ファイル一覧に警告の新設点を追加し、不変条件節へ
     偽陰性を明示、`src-tauri/CLAUDE.md` の `ui.interact` 例外の一文も更新対象へ加えた
  2. `:499-502` の 2 命題（`panel_fill` / `window_fill` 不使用の記録・`ctx.set_visuals` 撤去の
     grep 根拠）の着地先が無かった → 前者はモジュール doc `//!` へ移し、後者は #900 の
     機構化で無用ゆえ削除する、と着地先を明記した
- 軽微: 1 件（変異 iii は ii-a+b+c 同時と観測上区別できず判別力は実質 4 種類）→ 表へ注記済み
- 未検証: `.inner_margin()` 付き `Frame` の子への伝播は既存テスト（`Frame::new().show`）からの
  演繹である（`inner_margin` は余白であって style 継承に触れない）。Phase 1 の緑で自然に確定する

## 人間レビュー

- [x] 承認済み — 2026-08-06 / 問い: "**`workspace/plan.md` をご確認ください。** 注釈を追記いただくか、明示的にご承認いただければ Step 6（workspace のコミット）へ進みます。承認をいただくまで実装には入りません。" / 回答: "OK"
