# plan — #870 フォルダ展開中の現在地を中間省略する

## 目的

フォルダ展開中のプレースホルダが、深い階層でも**いま居るフォルダ名（leaf）とドライブの両方**を示すようにする。egui 既定の末尾省略に任せている現状は、異なる 2 つのディレクトリを同一表示に潰す（#836 のカテゴリ D 実測・SHA256 完全一致）。

## 受け入れ条件（issue #870 の 6 項目 + 実装で足した 2 項目）

| # | 条件 | 検証手段 |
|---|---|---|
| 1 | 深い階層で `→` を打って潜ったとき、段ごとに main 窓の表示が変わる | カテゴリ D・各段キャプチャの SHA256 が全て相異 |
| 2 | 省略後も leaf（いま居るフォルダ名）が読める | カテゴリ D・キャプチャを目視 |
| 3 | 日本語 UI・英語 UI の双方で 1・2 が成り立つ | カテゴリ D を `-Lang ja` / `-Lang en` の 2 本 |
| 3b | **日本語 UI では接尾辞「 内を検索...」も残る**（現行はここが先に削れる。省略が `dir` にだけ当たるため構造的に残る） | カテゴリ D・ja のキャプチャを目視 |
| 4 | 収まるパスでは省略が入らない（現行の見え方を変えない） | ユニットテスト + カテゴリ D の第 1 階層 |
| 4b | **`…` が中間と末尾へ二重に付かない**（内幅の見積もりが足りていれば起きない。崩れた瞬間に条件 2 が静かに偽になる） | カテゴリ D・キャプチャを目視 |
| 5 | `SPEC.md` §6.7 と ADR の却下 3 を、実装と同じ変更で同期する | diff |
| 6 | 1〜4b をカテゴリ D の打鍵注入 + 窓矩形キャプチャで実測し、証跡を PR に残す | PR 本文 |

## 設計

### 純粋核（`layout.rs`）— 測定関数を注入する

固定部（接頭辞・接尾辞）の幅を**推定しない**。「候補を書式へ埋めた文字列の実幅」を返す関数を注入し、収まる最長の候補を探す。ADR 却下 3 の理由 2 はこれで消える。

```rust
/// 中間省略の下限。これ未満では `truncate_middle_chars` が原文を返す＝幅が非単調に跳ねる。
pub const MIN_MIDDLE_KEEP: usize = 4;

/// `s` を `max_chars` 字におよそ収める中間省略（`C:\a\…\app.exe`）。
pub fn truncate_middle_chars(s: &str, max_chars: usize) -> String;

/// `dir` を中間省略し、`measure` の返す実幅が `avail_px` に収まる**最長**の候補を返す。
/// `measure` は候補を書式へ埋めた文字列（hint 全体）の実幅を返す。
pub fn fit_middle_by_measure(dir: &str, avail_px: f32, measure: impl FnMut(&str) -> f32) -> String;
```

`fit_middle_by_measure` の骨子:

1. `measure(dir) <= avail_px` なら `dir` をそのまま返す（受け入れ条件 4）
2. `dir` の文字数 `n <= MIN_MIDDLE_KEEP` なら `dir` をそのまま返す（縮めようがない）
3. `[MIN_MIDDLE_KEEP, n-1]` を二分探索。**収まった候補だけを `best` へ記録する**——単調性が僅かに破れても（`…` が除いた 2 字より広い場合）、返す候補は必ず実測で収まっている
4. 1 件も収まらなければ `MIN_MIDDLE_KEEP` 字版を返す（これ以上短くできない。以降は egui の末尾省略に委ねる）

測定回数は `1 + ceil(log2(n))`（128 字で 8 回）。galley は egui がキャッシュするため定常フレームはハッシュ引き。

### 重複の扱い — `truncate_middle` は動かさず、中核だけ委譲させる

`results_view::truncate_middle` は px ベース API で呼び出し点が 1 つ、新しい用途は測定注入 API。**共有できるのは「char 数だけ受ける中核」だけ**である。ゆえに:

- `truncate_middle_chars` を `layout.rs` に**新設**する
- `results_view::truncate_middle` は**シグネチャ・挙動・テストをそのまま残し**、本体だけ `layout::truncate_middle_chars` への委譲へ差し替える（head/tail 分割の実装が 2 本並ぶのを避ける）
- `truncate_middle` 本体とテスト群の**移動はしない**——移すと #870 の diff に「結果行の省略ロジックが動いた」が混ざり、カテゴリ D の証跡レビューで原因の切り分けが利かなくなる。`egui_shell/` の責務記述（`src-tauri/CLAUDE.md`）の書き換えも巻き込む

### view.rs の分割 — 読み取りは外・書式化と省略は内

`folder_current_dir()` の**読み取り位置は現在のまま**（`view.rs:370-377` の前寄せ禁止コメントが掛かっているのは読み取り点）。書式化と省略だけを closure 内へ移す。

```rust
// closure の外（現在の hint 構築位置）
enum HintPlan<'a> { Tool, Folder(&'a str), Search }
let hint_plan = if in_tool { Tool } else if let Some(dir) = ...folder_current_dir() { Folder(dir) } else { Search };

// closure の内（TextEdit を組む直前）
let hint: String = match hint_plan {
    Tool => tool_select_hint(l).to_string(),
    Search => search_hint(l).to_string(),
    Folder(dir) => {
        let avail = (ui.available_width() - TEXT_EDIT_HINT_H_MARGIN).max(0.0);
        let shown = layout::fit_middle_by_measure(dir, avail, |cand| {
            ui.painter().layout_no_wrap(folder_hint(l, cand), bar_font.clone(), bar_theme.name_color).size().x
        });
        folder_hint(l, &shown)
    }
};
```

優先度ラダー（tool > folder > results）と `Option` 直接分岐は**そのまま保つ**（ADR 却下 4・5）。分岐の形を `enum` へ写しただけで、腕の数も順序も条件も変えない。

`TEXT_EDIT_HINT_H_MARGIN = 8.0` の根拠は egui 0.35 `builder.rs:135`（`Margin::symmetric(4, 2)`）と `:614`（`available_width = allocate_width - margin.sum().x`）。

## 変更ファイル一覧と対象シンボル

| ファイル | 変更 |
|---|---|
| `src-tauri/src/egui_shell/layout.rs` | 新設 `MIN_MIDDLE_KEEP` / `truncate_middle_chars` / `fit_middle_by_measure` + ユニットテスト。`//!` に「テキストの中間省略」を足す |
| `src-tauri/src/egui_shell/results_view.rs` | `truncate_middle` の本体を `layout::truncate_middle_chars` への委譲へ（シグネチャ・doc・テストは維持） |
| `src-tauri/src/egui_shell/view.rs` | `HintPlan` 新設・hint の書式化と省略を closure 内へ・`TEXT_EDIT_HINT_H_MARGIN` 定数・コメント更新 |
| `src-tauri/src/egui_shell/strings.rs` | `folder_hint` の doc コメント（「省略は呼び出し側で組まない」が偽になる）を書き換え |
| `SPEC.md` §6.7 | 最終行を差し替え（3b・4b を含む新しい挙動と、新規実測値） |
| `docs/adr/ADR-folder-location-display-surface.md` | 却下 3 →「一度受容し、観測点が返した答えで採用へ転じた」へ。「受容した残余」の該当項目も同期 |

## 実装順序（フェーズ）

### Phase 1 — 純粋核（TDD）

- [ ] `layout.rs` に `MIN_MIDDLE_KEEP` / `truncate_middle_chars` / `fit_middle_by_measure` の**失敗するテストを先に**書く（Red）
- [ ] 実装して通す（Green）
- [ ] `cargo test -p snotra egui_shell::layout` が緑

### Phase 2 — 重複の解消

- [ ] `results_view::truncate_middle` を `layout::truncate_middle_chars` への委譲へ差し替える
- [ ] 既存 2 テスト（`truncate_middle_shortens_long_path` 系）が**無変更のまま**緑

### Phase 3 — 配線

- [ ] `view.rs` に `HintPlan` を新設し、hint の書式化と省略を closure 内へ移す
- [ ] `view.rs:370-377` / `:415-417` のコメントを新しい構造に合わせて更新（**前寄せ禁止の理由は残す**）
- [ ] `strings.rs` の `folder_hint` doc を更新
- [ ] `cargo build -p snotra` / `clippy -D warnings` が緑

### Phase 4 — 文書同期と実測

- [ ] `SPEC.md` §6.7 の最終行を書き換える
- [ ] `docs/adr/ADR-folder-location-display-surface.md` の却下 3 と「受容した残余」を書き換える
- [ ] カテゴリ D 実測: `pwsh -File C:/tmp/snotra836-tools/drive-836.ps1 -Scenario deep -Lang en` と `-Lang ja`
- [ ] キャプチャの SHA256 が段ごとに全て相異（条件 1）
- [ ] キャプチャを目視して leaf が読める・`…` が二重に付いていない・ja では接尾辞が残る（条件 2・3b・4b）
- [ ] 実測値（省略が始まる階層と字数）を `SPEC.md` / ADR へ反映
- [ ] `npm run governance:check` が緑

## 不変条件と異常系

| 不変条件 | 壊れたときの検知手段 |
|---|---|
| 優先度ラダー tool > folder > results が変わらない | `HintPlan` の腕の順序（`if in_tool` が先頭）。カテゴリ D の tool シナリオ |
| `folder_current_dir()` の読み取り点が前へ動かない | `view.rs:370-377` のコメントが要求する grep 検算（読み取り点から TextEdit 構築までに `self.controller.` の `&mut` 呼び出しが 1 本も無い）を実装後に実行する |
| 収まるパスでは 1 文字も変わらない | `fit_middle_by_measure` の第 1 段（`measure(dir) <= avail_px` → 原文返し）のユニットテスト |
| 返す候補は必ず実測で収まっている（1 件も収まらない場合を除く） | `best` に「収まった候補だけ」を記録する構造。フェイク measure のユニットテスト |
| `results_view::truncate_middle` の挙動が 1 ミリも変わらない | 既存テストを**無変更**で通す |
| 書式末尾の三点は ASCII `...` のまま | 既存 `folder_hint_uses_ascii_ellipsis_not_u2026` |

異常系:

- `dir` が空 / 1 字 → `n <= MIN_MIDDLE_KEEP` で原文返し
- `avail_px <= 0`（窓が極端に細い） → 全候補が収まらず `MIN_MIDDLE_KEEP` 字版。egui の末尾省略が引き継ぐ（現行と同じ見え方へ退化するだけ）
- `measure` が非単調（`…` が除いた 2 字より広い） → `best` 記録により返り値は必ず実測で収まる。最長でない候補を返しうるが、条件 1〜4 は壊れない
- CJK パス → 測定注入ゆえフォント実寸に乗る（`per_char_px` の推定を経由しない）

## テスト方針と検証コマンド

ユニットテスト（`layout.rs`・フェイク measure = ASCII 10px / CJK 20px / 固定部 30px）:

1. `fit_middle_leaves_short_dir_untouched` — 収まる `dir` は 1 文字も変わらない（条件 4）
2. `fit_middle_keeps_head_and_tail` — 128 字相当を縮めても先頭（`C:\`）と末尾（leaf）が残る（条件 2）
3. `fit_middle_result_actually_fits` — 返り値を測ると `avail_px` 以下（`best` 記録の核心）
4. `fit_middle_distinguishes_sibling_dirs` — 同じ親の下の 2 つの長い leaf が**相異なる文字列**になる（#870 の症状そのもの）
5. `truncate_middle_chars_width_is_monotonic_above_min_keep` — フェイク measure で `keep` を `MIN_MIDDLE_KEEP..=n` と 1 ずつ増やし、幅が**非減少**であることを検算（二分探索の正当性の根拠）
6. `truncate_middle_chars_returns_source_below_min_keep` — `keep < MIN_MIDDLE_KEEP` は原文（＝探索範囲から外す理由の固定）
7. `fit_middle_handles_cjk` — CJK 混在で、Latin 想定の推定より正しく縮む
8. `fit_middle_narrow_avail_falls_back_to_min_keep` — 極端に狭い幅で panic せず最短候補を返す

検証コマンド（`docs/build-commands.md` の SSOT に従う）:

- カテゴリ A: `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test`（PostToolUse hook が自動実行＝沈黙は合格）
- カテゴリ D: `pwsh -File C:/tmp/snotra836-tools/drive-836.ps1 -Scenario deep -Lang en` / `-Lang ja`（打鍵注入 + 窓矩形キャプチャ。**実行中はキーボード・マウスに触れない**。画面がロックされていれば #866 の検出器が実行前に止める）
- カテゴリ F: `npm run governance:check`（`SPEC.md`・ADR を触るため）

カテゴリ C（`smoke:startup` / `smoke:egui`）: **該当する**（表示経路の変更）。hint 文言に依存する trace イベント・hotkey は無い（`grep "内を検索\|Search in\|folder_hint" scripts/` は 0 件）ことを確認済みだが、`view.rs` の描画経路を触るため実行する。

## `SPEC.md`・関連文書の更新要否

**要**。これは「fix」ではなく**仕様変更**である——§6.7 の「アプリ側に省略の機構は持たない」が偽になる。`AGENTS.md`「3層分担」により `SPEC.md` → コード → ドキュメントを同じ変更に含める。

ADR は「却下した代替案と却下の理由」を持つ文書なので、**却下 3 を消さずに「観測点が返した答えで採用へ転じた」形で残す**（否定の知識が「なぜ一度却下したか」と「何が前提を覆したか」の両方を持つ）。あわせて、却下理由 1・2 が**リポジトリ内の呼び出し点を見ずに書かれていた**ことも記録する——これは再発しうる書き手の失敗様態である。

## 未確定（実装前に潰す）

- [x] **`truncate_middle` を `layout.rs` へ統合するか** — 統合**しない**。共有できるのは char 数を受ける中核だけで、px ベース API 本体は共有されない。移動はテスト群も動かし、#870 の diff に「結果行の省略が動いた」を混ぜてカテゴリ D 証跡の切り分けを壊す。`truncate_middle_chars` を新設し本体を委譲へ差し替える形で DRY を閉じる（`/dry-check` の判定根拠にこの行を使う）
- [x] **hint 構築をどこまで closure 内へ移すか** — **読み取りは外・書式化と省略は内**。`view.rs:370-377` の前寄せ禁止は `folder_current_dir()` の読み取り点に掛かっており、書式化を後ろへ動かすのは制約を壊さない。`HintPlan` で分岐だけ外に残す
- [x] **hint の内幅を何と見積もるか** — `ui.available_width() - 8.0`。根拠は egui 0.35 `builder.rs:135`（`Margin::symmetric(4, 2)`）と `:614`（`available_width = allocate_width - margin.sum().x`）。**`builder.rs:721-725` の frame expansion がこの内幅にどう効くかは読み切っていない**——推定で埋めず、受け入れ条件 4b（`…` が二重に付かない）としてカテゴリ D の実測で閉じる（Phase 4 の作業項目）
- [x] **SPEC / ADR の旧実測値（「71 字なら全部見え、94 字で削られる」）をどうするか** — 旧値は「末尾省略がどこから始まるか」の記録で、機構が変わる以上そのまま残せない。**Phase 4 のカテゴリ D で取り直す**。deep fixture は 4 段で概ね 32 / 73 / 96 / 116 / 128 字のパスを通るため、省略が始まる階層をそこから読める。取り直した値だけを書き、旧値は ADR の「一度受容したときの実測」として日付付きで残す

## セルフレビュー

- リスク: **通常**
  - 根拠: 永続形式・設定キー・公開 API・状態遷移の変更なし。worker / channel / listener / 共有状態 / async の変更なし。網羅性は要件でない。hook / CI / rules / skills は触らない。`SPEC.md` と ADR は触るが、`/plan-review` のリスク一覧が hook・CI・rules・skills と並べる「ガバナンス文書」は**エージェントの行動を律する機構の文書**であり、製品仕様（第 1 層）である `SPEC.md` はそれに当たらない
  - モジュール間インターフェースは `layout.rs` に純関数を 2 本足すだけで、既存の消費者との契約は変わらない（`results_view::truncate_middle` はシグネチャ・挙動・テストとも不変）
- plan-review: 未実施（通常リスク）／自己レビューのみ
- エージェント数: 0
- 5a の自己照合:
  1. **issue の全要件に作業項目が対応する** — 条件 1〜6 が Phase 4 の作業項目と表で一対一。加えて実装から導いた 3b・4b を足した
  2. **境界条件と検証** — 空 `dir` / 1 字 / `n <= MIN_MIDDLE_KEEP` / `avail_px <= 0` / 非単調な measure / CJK を「異常系」に列挙し、テスト 1〜8 で覆う
  3. **新しい状態・リソース・プロセス** — 無し。純関数 2 本と `enum` 1 つで、生成/破棄のライフサイクルを持つものを導入しない
  4. **より単純な既存パターンで置き換えられないか** — 「`per_char_px` を 1 回推定して `truncate_middle` を当てる」（issue 本文の素案）はより単純だが、平均幅で割る近似ゆえ**収まる保証が無い**。溢れれば egui が末尾を削り、条件 2（leaf が読める）が静かに偽になる。測定注入 + 二分探索は測定回数 8 回と引き換えに「返す候補は必ず収まる」を得る
  5. **壊してはならない不変条件の検知手段** — 「不変条件と異常系」の表に 6 件、それぞれ検知手段付き
- 条件別チェック（`AGENTS.md` の表）の当たり:
  - **関数・型を新規定義** → 呼び出し元 grep（新設なので 0）＋ `/dry-check`（Step 4a で実行）
  - **表示経路の変更** → `scripts/smoke-egui.ps1` の前提確認（実施済み: hint 文言に依存する trace イベント名・hotkey は無い）
  - **UI モード・ガード条件の追加/変更** → 該当なし。`HintPlan` は既存 3 分岐の写しで、腕・順序・条件を変えない（ADR 却下 4・5 が「切り出さない」理由を持つが、今回は純粋核へ切り出すのではなく**分岐の形だけ**を保ったまま書式化を後段へ送る）。`/state-check` は Step 4a で判定を書く
  - **並行境界** → 該当なし（`update()` 内の同期処理のみ）
- 未検証: hint 内幅に対する frame expansion の効き（受け入れ条件 4b としてカテゴリ D で閉じる。Phase 4 の作業項目）

## 人間レビュー

- [x] 承認済み — 2026-08-01 / 問い: "`workspace/plan.md` へ注釈を加えていただくか、承認をいただければ `/implement` へ渡します。" / 回答: "OK"
