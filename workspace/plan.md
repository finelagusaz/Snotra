# plan — issue #835 検索結果ウィンドウの高さを常に visible_rows 分確保する

ブランチ: `feat/results-fixed-height`

## 目的

`results` 窓の高さを実件数フィットから **`visible_rows` 分の固定高**へ変える。候補の増減で窓が伸縮しなくなり、#743 で起きた「階層が変わっていないように見える」誤読の実因を断つ。**#646 PR2 決定 7 と #675（下端クランプ）の 2 つを覆す仕様変更**である。

**これは #646 PR2 以前の仕様への回帰でもある** — 当時の SPEC §4.5 は「結果ウィンドウの高さは最大表示件数に基づく固定高とする。ヒット数が最大表示件数未満でも高さは維持され…」であった（`docs/superpowers/plans/2026-07-25-646-pr2-results-window-split.md:740` が旧文を引用している）。ADR にこの往復を記録する。

## 受け入れ条件（issue #835 より）

1. 候補が `visible_rows` 未満でも表示領域が `visible_rows` 分で一定になる
2. 0 件時の扱いを決め、`SPEC.md` §4.5 と results 可視性の連言に反映する
3. 作業領域下端でのクランプと矛盾しない（下端に収まらない場合の挙動を明記する）
4. `layout.rs` の純粋核テストで高さ算出を固定する

## 人間が確定した設計判断（2026-08-04・モック提示後）

| 論点 | 決定 | 逐語 |
|---|---|---|
| 0 件の扱い | 窓を出さない | 「表示しない（推奨）」 |
| 下端に収まらないとき | **クランプしない**（サイズは変えずはみ出させる） | 「results が画面下部に数行消えてもサイズは変えない、でいいと思う。はみ出て不自由ならユーザーが好きに位置を動かす」 |
| `visible_rows = 0`（手編集） | 窓を出さない | 「窓を出さない（推奨）」 |
| 最下端でバーを出すと results が 1 行も見えなくなる件 | **受容する** | 「受容する（クランプ完全撤去）」 |

## 設計の決定

### D1. 高さ算出から `result_count` を落とす

```rust
/// 結果窓の高さ（#835）。**`visible_rows` 分の固定高**・padding 8。
/// `max_results == 0` は 0.0（呼び出し側が hide する契約）。
pub fn results_window_height(max_results: u32, row_height: f64) -> f64 {
    if max_results == 0 {
        0.0
    } else {
        max_results as f64 * row_height + 8.0
    }
}
```

引数から `result_count` を落とすことで、**呼び出し点の移行漏れをコンパイラが検出する**。`max_results == 0` を 0.0 に倒すのは、その値が `config.toml` の手編集で到達可能だからである（`layout.rs:207-211` の doc）。ここを落とすと 8px のスリット窓が出る。

### D2. 0 件の hide を独立した連言へ移す

```rust
pub fn present_results(i: ResultsInputs) -> ResultsPresentation {
    let desired_height = results_window_height(i.max_results, i.row_height);
    if i.main_visible && !i.plain_hidden && i.result_count > 0 && desired_height > 0.0 {
        ResultsPresentation::Visible { desired_height }
    } else {
        ResultsPresentation::Hidden
    }
}
```

現在は連言②「結果が空でない」を `desired_height > 0.0` が代行しており、固定高さ化でこの代行が壊れる。**`SPEC.md` §8.6 の連言図（549 行）は元から 4 項を別々に書いており、実装だけが 2 項を融合していた** — この変更で図と実装が 1 対 1 になる。#752（②と④を区別できるようにした変更）の延長であり逆行ではない。

### D2-b. 真理値表の到達不能行が 4 行減る（テスト doc の論証が偽になる）

`present_results_truth_table_distinguishes_all_four_conjuncts` の doc（`layout.rs:600-602`）は「**16 行のうち 4 行は到達不能である。**「②false ∧ ④true」は生の入力から構成できない（`result_count = 0` なら高さも 0 になる）」と論証している。**D1 でこの前提が崩れる** — 高さが `max_results` だけで決まるため、`result_count = 0 ∧ max_results = 8` は「②false ∧ ④true」を**構成できる**。

出力は壊れない（D2 の `result_count > 0` が正しく Hidden へ倒す）が、論証はそのまま偽になる。**doc を書き換え、空いた 4 行を実測するケースを足す**（614 行のコメント `// ②f ④f` も `// ②f ④t` へ）。#752 が「②と④を区別できるようにする」ことを眼目にした変更である以上、区別できる組み合わせが増えたなら測る側に足すのが筋である。

### D3. 下端クランプ（#675）の機構を撤去する

人間の決定（上表）に従い、`results` の高さは作業領域に関係なく常に `visible_rows` 分とする。収まらない分は画面外へはみ出す。

**撤去は 5 シンボルに連鎖する**（`-D warnings` 下で `dead_code` になるため保持できない。呼び出し点は grep で数え上げ済み）:

| # | シンボル | 位置 | 撤去理由 |
|---|---|---|---|
| 1 | `layout::clamp_results_height` | `layout.rs:95` | 呼び出し点は `window_coordinator.rs:845` の 1 つだけ |
| 2 | `layout::available_below` | `layout.rs:132` | 呼び出し点は `results_available_height` の 1 つだけ |
| 3 | `window_coordinator::results_available_height` | `window_coordinator.rs:715` / `725`（2 cfg） | 呼び出し点は `window_coordinator.rs:847` の 1 つだけ |
| 4 | `monitor::window_monitor_work_area` | `src-tauri/src/monitor.rs:96` | 呼び出し点は 3 の 1 つだけ（**連鎖 dead_code**） |
| 5 | `ResultsWindow::scale_factor` | `results_window.rs:252` | 呼び出し点は 3 の 1 つだけ（**連鎖 dead_code**） |

`layout::results_top_y` と `position_results_below_main` は**残す**（位置決めに要る。クランプとは別責務）。ただし **`position_results_below_main` の戻り値 `Option<i32>` は落とす**（symmetric-check）——上端 y を消費していたのはクランプだけであり、撤去後は 2 つの呼び出し点（`mod.rs:346` の `Moved` リスナーは既に捨てている・`window_coordinator.rs:832`）のどちらも使わない。`Option` は型に `#[must_use]` が付かないため**捨てても警告は出ない**（`mod.rs:346` が現に捨てているのが証拠）＝ コンパイラは教えてくれない。手で落とす。

### D3-b. 固定高は results 窓に描く全ビューへ一様に適用される

`result_count` は通常検索の件数ではなく **`results` 窓に出る行数**である（ツール選択は「結果リストをツール一覧で置換」する・`search_state.rs:306`。フォルダ展開・instant 行も同じ `results` を使う）。ゆえに固定高はツール選択メニュー（候補 2〜3 件）やフォルダ展開にも一様に効き、どのビューでも窓の高さは `visible_rows` 分になる。**これは意図した帰結である**（「窓の高さが状況で変わらない」が本 issue の目的そのもの）。`SPEC.md` §4.5 にこの射程を明記する。

### D4. 「高さ 0 ⇒ hide」契約の正本を移す

この契約の doc は現在 `clamp_results_height`（`layout.rs:90` の「0.0 は `present_results` が hide と読む契約値」）にあり、撤去で消える。**`results_window_height` の doc へ移す**。`ResultsPresentation` を struct にしない理由の doc（`layout.rs:218`）もこの参照を持つため、同時に書き換える。

### D5. `icon_cache_cap` は変更しない

`snotra-core/src/config.rs:626` の導出は `max(visible_rows, result_limit, recent_limit) × 5` で `result_count` を入力に持たない。本変更は「何行分の高さを取るか」だけを変え `visible_rows` の値は変えないため、cap は変更前後で同一である（issue 論点 4 の回答・根拠は `research.md`）。

## 変更ファイル一覧と対象シンボル

| ファイル | 対象 | 変更 |
|---|---|---|
| `SPEC.md` §4.5 | 172-173 行 | 実件数フィット → 固定高。**173 行（#675 のクランプ）を削除**し、はみ出しを受容する旨へ書き換え。**射程（通常結果・フォルダ展開・ツール選択・instant 行のすべて）も明記**（D3-b） |
| `SPEC.md` §4.7 | 184 行 | 「実件数フィット（§4.5）」→「`visible_rows` 分の固定高（§4.5）」 |
| `SPEC.md` §8.6 | 555 行の表 | 連言項「窓高さ > 0」の説明を更新し、「結果が空でない」との分離を明示 |
| `src-tauri/src/egui_shell/layout.rs` | `results_window_height` | D1 + D4（doc の契約を引き取る） |
| `src-tauri/src/egui_shell/layout.rs` | `present_results` | D2。`result_count > 0` を連言へ追加。doc の「クランプは driver が行う」（215 行）と `clamp_results_height` 参照（218 行）を書き換え |
| `src-tauri/src/egui_shell/layout.rs` | `results_top_y` の doc（114 行） | `available_below` を引き合いに出す段落を書き換え |
| `src-tauri/src/egui_shell/layout.rs` | `clamp_results_height` / `available_below` | **削除**（D3-1・D3-2） |
| `src-tauri/src/egui_shell/layout.rs` | テスト 511-514 / 519-537 / **591-602 の doc** / 606 / 614 / 650-680 / 712-725 | 期待値更新・クランプ系テスト削除・**真理値表 doc の到達可能性の論証を書き換え**（D2-b） |
| `src-tauri/src/egui_shell/window_coordinator.rs` | `results_available_height`（2 cfg） | **削除**（D3-3） |
| `src-tauri/src/egui_shell/window_coordinator.rs` | `drive_results_window` **833-849** | `applied_height` を廃し `desired_height` をそのまま `set_size` へ。**クランプ前提の doc は 833 行から始まる**（「作業領域の下端でクランプする（#675）」〜「別名にしてある」まで一続き）ので段落ごと消す |
| `src-tauri/src/egui_shell/window_coordinator.rs` | `position_results_below_main` の doc **665-675** | 戻り値を返す理由（「高さのクランプに上端が要る」「計算した値を捨てる関数は、次の利用者に写しを書かせる」）が戻り値の撤去と矛盾する。**上端の適用点がここ 1 つである**という主張だけを残す |
| `src-tauri/src/monitor.rs` | `window_monitor_work_area` | **削除**（D3-4）。77 行の doc（`point_monitor_work_area` が「こちらを使わない」と名指す段落）も書き換え。**`HWND` の import が未使用になる**（`POINT` は他 3 関数が使うので残す） |
| `src-tauri/src/egui_shell/results_window.rs` | `scale_factor` | **削除**（D3-5） |
| `src-tauri/src/egui_shell/mod.rs` | 38 行のコメント | `results_available_height` の名を消す |
| `src-tauri/src/egui_shell/mod.rs` | **315 行のコメント** | `results` 窓生成時の「初期値。実高は main が実件数フィットで設定」→ 固定高の表現へ |
| `scripts/lib/SnotraTraceInvariants.psm1` | **16 行（H4 の説明）** | 「`egui_results:show` の `rows` が 0 なら異常 ｜ 「高さ 0 ⇔ hide」の契約違反（`layout::present_results`）」の**契約名**を「件数 0 ⇒ hide」（連言②）へ改める。**判定ロジック自体は変更後も真**（0 件なら hide するので show の `rows` は必ず > 0）——直すのは名指しだけである |
| `src-tauri/CLAUDE.md` | 「モジュール構成」の `layout.rs` 項・`monitor.rs` 項 | 消えたシンボル名を索引から外す |
| `docs/architecture.md` | 82 行 | `clamp_results_height` の名を外す |
| `scripts/manual-smoke.ps1` | 項目 7（99-109 行） | **本変更で偽になる**。「作業領域の下端で高さがクランプされる」→「下端に収まらなくても高さが変わらない」へ書き換え。`inv` の I6（可視判定はクランプ前・`set_size` はクランプ後）も消える（**I6 の言及はこの 1 か所だけ**・grep 実測） |
| `docs/adr/ADR-results-fixed-height.md` | 新規 | 決定 7 と #675 を覆す理由・0 件を独立連言にした理由・#646 PR2 以前への回帰である事実・受容する残余 |

**変更しないと判断したもの（根拠つき）**

- `docs/adr/ADR-results-presentation-two-stage.md` / `ADR-main-window-clamp-on-pointer-release.md` — **ADR は決定時点の記録であり、後の決定で書き換えない**。撤去した事実は新 ADR が持ち、そこから旧 ADR を参照する
- `layout::results_top_y` / `position_results_below_main` / `monitor::point_monitor_work_area` — 位置決めの経路であり、クランプとは別責務（D3）
- `snotra-core/src/config.rs` — D5

## 実装順序

### Phase 1: SPEC.md（仕様が先）

- [ ] §4.5 を固定高へ書き換える（0 件は非表示・**クランプしない**・はみ出しを受容する理由）
- [ ] §4.7 の 184 行の参照を更新する
- [ ] §8.6 の連言表 555 行を更新する
- [ ] `npm run governance:check`

### Phase 2: 高さ算出と連言（Red → Green）

- [ ] テストを先に書き換える（`results_window_height` / `present_results` の新期待値）— **落ちることを確認する**
- [ ] `results_window_height` を D1 の形にし、doc に D4 の契約を書く
- [ ] `present_results` に `result_count > 0` を足し、doc（215・218 行）を書き換える
- [ ] 606 行・650-680 行の既存テストを新しい式へ合わせる
- [ ] 真理値表テストの doc（591-602 行）の到達可能性の論証を書き換え、空いた 4 行のケースを足す（D2-b・614 行のコメントも）
- [ ] `cargo test -p snotra`（**`--lib` を付けない** — `src-tauri` は `[lib]` を持たない）

### Phase 3: クランプ機構の撤去

**この Phase は分割せず 1 コミットにする。** `clamp_results_height` の呼び出しを外した瞬間に `available_below` → `results_available_height` → `window_monitor_work_area` → `ResultsWindow::scale_factor` の 4 段が同時に `dead_code` になるため、途中でコミットすると `-D warnings` で落ちる（「各 Phase の検証 green 後にコミット」の運用は Phase 境界でのみ成立する）。

**撤去範囲が計画より増えたら、黙って広げずユーザーへ報告する。** `dead_code` の 3 段目以降はコンパイラでしか分からない（計画の連鎖は grep による静的な 2 段確認である）。

- [ ] `window_coordinator.rs` の `drive_results_window` から `clamp_results_height` 呼び出しを外す（`applied_height` → `desired_height`・842-844 行の doc も削除）
- [ ] `position_results_below_main` の戻り値 `Option<i32>` を落とし、`window_coordinator.rs:832` の `let top_y =` を外す（D3・**コンパイラは教えない**）
- [ ] `results_available_height`（2 cfg）を削除する
- [ ] `layout::clamp_results_height` / `available_below` とそのテストを削除する
- [ ] `monitor::window_monitor_work_area` を削除し、`point_monitor_work_area` の doc（77 行）を書き換え、未使用になる `HWND` の import を外す
- [ ] `ResultsWindow::scale_factor` を削除する
- [ ] `mod.rs:38` のコメントから `results_available_height` を外し、`mod.rs:315` の「実件数フィット」を書き換える
- [ ] `position_results_below_main` の doc（665-675 行）から戻り値の理由を外す
- [ ] `layout::results_top_y` の doc（114 行）から `available_below` の引き合いを外す
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`（**dead_code の取り残しはここで出る**）

### Phase 4: ADR とドキュメント

- [ ] `docs/adr/ADR-results-fixed-height.md` を書く
- [ ] `src-tauri/CLAUDE.md` の `layout.rs` 項・`monitor.rs` 項から消えたシンボル名を外す
- [ ] `docs/architecture.md:82` から `clamp_results_height` を外す
- [ ] `scripts/manual-smoke.ps1` の項目 7 を「下端に収まらなくても高さが変わらない」へ書き換える（`inv` も更新）
- [ ] `scripts/lib/SnotraTraceInvariants.psm1:16` の H4 の契約名を「件数 0 ⇒ hide」へ改める（**判定ロジックは変えない**）
- [ ] `npm run governance:check`

### Phase 5: 検証

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test -p snotra`
- [ ] `npm run smoke:egui`（表示経路の変更ゆえカテゴリ C 該当）
- [ ] 目視（カテゴリ D）— 0 件 / 1 件 / `visible_rows` 未満 / 超過 / **バーを画面下端へ置いた状態**の 5 状態

## 不変条件と異常系

| 不変条件 | 検知手段 |
|---|---|
| 「高さ 0 ⇒ hide」の契約 | `present_results` の `desired_height > 0.0` を残す + ユニットテスト（doc の正本は D4 で `results_window_height` へ移す） |
| `max_results == 0` で窓を出さない | ユニットテスト（新規） |
| 0 件で窓を出さない | ユニットテスト（新規・連言②を独立に測る） |
| 連言①（main 可視）・③（carve-out）の効力 | 既存テスト（650-680 の回帰群）が通ること |
| results 窓の 3 操作（show/hide/topmost）は raw Win32 のまま | 本変更は `set_size` に渡す値だけを変える |
| クランプ機構の取り残しが無い | `cargo clippy -- -D warnings` の `dead_code`（Phase 3 の最後） |

**受容する残余（人間が明示的に承認・2026-08-04）**

1. バーを作業領域の最下端に置くと `results` が丸ごと画面外へ出て、行が 1 つも見えない。位置を動かせば直る（#675 が置いた床を失う帰結）
2. 画面外へはみ出した行は、キーボードで選択を下へ動かしたとき画面外のまま選択されうる。`ScrollArea` は「窓の中で可視」にするだけで、窓が画面内にあるかを知らない
3. 混在 DPI 環境で `results` 窓の scale を読む経路が消える。固定高は `row_height`（論理 px）から導き、tao が `results` 窓の scale で物理へ戻すため、**読む必要が無くなったことによる撤去**である（挙動の後退ではない）

## テスト方針と検証コマンド

純粋核テストで固定する（受け入れ条件 4）。新規・更新するケース:

| ケース | 期待 |
|---|---|
| `results_window_height(8, row)` | `8.0 * row + 8.0` |
| `results_window_height(0, row)` | `0.0` |
| `present_results`（0 件・max 8） | `Hidden` |
| `present_results`（1 件・max 8） | `Visible { 8 行分 }` |
| `present_results`（20 件・max 8） | `Visible { 8 行分 }` |
| `present_results`（3 件・max 0） | `Hidden` |
| `present_results`（3 件・`main_visible = false`） | `Hidden` |
| `present_results`（3 件・`plain_hidden = true`） | `Hidden` |
| `present_results`（**0 件・max 8** ＝ 新たに構成可能になる「②false ∧ ④true」） | `Hidden`（D2-b） |

```
cargo test -p snotra            # --lib は付けない
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
npm run governance:check
npm run smoke:egui
```

## 未確定（実装前に潰す）

（なし — 3 論点すべて 2026-08-04 に人間が確定。上表「人間が確定した設計判断」を参照）

## セルフレビュー

- リスク: **高**（SPEC の状態述語＝可視性の連言を変更・過去決定 2 件〔#646 PR2 決定 7 と #675〕の反転・セーフティネット〔手動スモーク項目〕の変更）
- plan-review: 独立レビュー 1 体（Step 2 の計画準拠レビュー・`general-purpose` / sonnet）。**Step 2b〔独立導出〕は選ばなかった** — 「クランプ撤去」は issue に書かれておらず会話で人間が裁定した事項なので、issue の WHAT だけを渡す導出では的が外れる
- エージェント数: 1（成果物 `workspace/plan-review-835-clamp-removal.md`）
- 併走した check スキル: `/state-check`（要対処 1）・`/symmetric-check`（要対処 1）
- 要対処: **6 件すべて反映済み**
  1. `/state-check` — 固定高がツール選択・フォルダ展開・instant 行にも一様に効く射程が未記述 → D3-b を新設し SPEC §4.5 に明記
  2. `/symmetric-check` — `position_results_below_main` の戻り値の消費者がゼロになる（**`Option` は型に `#[must_use]` が付かず警告が出ない**） → D3 と Phase 3 に追加
  3. 独立レビュー — `mod.rs:315` の「実件数フィット」コメント
  4. 独立レビュー — 真理値表テストの doc（`layout.rs:600-602`）の到達不能性の論証が偽になる → D2-b を新設し、空いた 4 行のケース追加も計画へ
  5. 独立レビュー — `position_results_below_main` の doc（665-675）が戻り値の存在理由を説明しており撤去と矛盾
  6. 独立レビュー — `scripts/lib/SnotraTraceInvariants.psm1:16` の H4 が「高さ 0 ⇔ hide」を契約名として持つ（判定ロジックは変更後も真・名指しだけ直す）
- 軽微（反映済み）: `monitor.rs` の `HWND` import が未使用になる／`drive_results_window` のクランプ前提コメントは 833 行から始まる（計画の引用は 842-844 だけだった）
- 自己照合（Step 1 の 7 点）: 7 点目「変更で偽になる散文を概念ラベルでも grep」で `scripts/manual-smoke.ps1` 項目 7（#675 の挙動を検証する手動スモーク）を発見 → 変更ファイル一覧と Phase 4 へ追加
- 未検証: **コンパイラでの裏取り**（`dead_code` / `unused_imports` の連鎖が 3 段目を持たないこと）は実装時 Phase 3 の `cargo clippy -- -D warnings` で行う。静的には grep で 1 呼び出し点ずつ実測済み
- 並行作業: worktree `chore/round2-findings`（locked）が在るが、触るファイルが本変更と重ならないことを `git worktree list` で確認した

## 人間レビュー

- [x] 承認済み — 2026-08-04 / 問い: "**この計画で実装へ進んでよろしいでしょうか。** 承認をいただければ `workspace/` をコミット・プッシュし、`/implement` へ引き渡せます。" / 回答: "実装に進んでよい"

セーフティネット 2 ファイル（`scripts/manual-smoke.ps1` 項目 7・`scripts/lib/SnotraTraceInvariants.psm1` の H4）に触ること、および撤去規模が当初計画の倍近くになることを名指しで提示したうえでの承認である（ルート `CLAUDE.md`「最重要ルール」2）。
