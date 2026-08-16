# 調査 — #1106: 表示ゲートの連言④（`visible_rows = 0`）で隠れた行を Enter が起動する

## issue の要約

`layout::present_results` の 4 連言のうち、**連言④（窓高さ > 0）が偽になる経路にだけ起動側の対応物が無い**。
`appearance.visible_rows = 0` のとき `results_window_height(0, _)` が `0.0` を返して `ResultsPresentation::Hidden`
になるが、`state.results()` は非空のまま・`plain_results_hidden` も偽なので、**results 窓に 1 行も出ていない
状態で Enter / Shift+Enter / クリックが行を起動する**。#1077 が塞いだのは連言③の経路だけだった。

出所は #1077 の `code-reviewer` レビュー（2026-08-16）。#1077 の受け入れ条件のどれにも要らずスコープ外にした。

## 一次証拠で裏取りした事実

| # | 命題 | 測り方 | 結果 |
|---|---|---|---|
| P1 | `visible_rows = 0` は本体で到達可能 | `Config::validate` を LSP `findReferences`（32 件） | **真**。本体（`src-tauri`）からの呼び出しは **0 件**。live な呼び出し元は `snotra-settings/src/app.rs:211` と `snotra-settings/src/tabs/backup.rs:274` だけで、残る 30 件は `snotra-core/src/config.rs` の `mod tests`。`ConfigError::VisibleRowsZero`（`config.rs:1066`）は設定 UI の保存前検証にしか効かず、`config.toml` の手編集を止めない。**3b が独立に補強**: `Config::load_from_dir_reporting` にも `config_watcher::apply_config_change` にも `validate()` は無く、`migrate_legacy_count_params` / `resolve_count_param_defaults` はどちらも `Option::get_or_insert*`（None のときだけ埋める）ゆえ `Some(0)` は生き残る |
| P2 | 連言④が偽になるのは `max_results == 0` のときだけ | `layout::Metrics::from_config`（`layout.rs:49-56`）を読む | **真**。`row_height = (f + path_size(f) + row_padding + 4.0).max(24.0)` で**下限 24.0** を持ち、`font_size: u32` ゆえ負も NaN も入らない。`results_window_height` の他方の入力から `0.0` は出ない。`layout.rs:117-121` / `:400` の全称主張（「返すのは `max_results == 0` のときだけ」）は**現時点で真**であり、この issue では検算の対象にならない |
| P3 | 起動の入口 | `activate_or_execute` を LSP `findReferences` | 呼び出し元は 4 か所——`view.rs:1175`（クリック逆流 `take_clicked_for`）、`launcher_controller.rs:1418`（`on_enter`）、同 `:579`（`shift_activate` の instant / Tool 委譲）、同 `:622`（同 `tools <= 1` 委譲）。#1077 と同じく `shift_activate` の `tools >= 2` 枝だけが `activate_or_execute` を通らず `SearchState::enter_tool` を直接呼ぶ |
| P4 | 連言④は③と射程が違う | `SPEC.md`「4.5 最大列挙数」 | **真**。「この高さは `results` に描くすべてのビューへ一様に適用される——通常結果・フォルダ展開・ツール選択メニュー・インスタントコマンド行」。連言③（`plain_results_hidden`）は `Results ∧ !instant_rows ∧ indexing` の carve-out を持つが、**④に carve-out は無い**。ゆえに④偽では tool ビュー・instant 行・folder 展開の行も 1 行も見えない |
| P5 | `max_results` は起動側へ配られていない | `effective_visible_rows` を LSP `findReferences`（14 件） | **真**。`src-tauri` 側の読み点は `window_coordinator.rs:750-751`（`fn max_results`）ただ 1 つで、その doc が「**読み点の制約を持たない**ため `DriveResultsInputs` へは載せず driver の内側で読む（#749）」と明記する。起動側（`launcher_controller`）はこの値を持たない |
| P6 | (b)（`effective_visible_rows` の下限 1 clamp）は却下済みの族 | ADR / SPEC / doc の 3 出典 | **真**。① `ADR-results-fixed-height` 却下 5「最大表示件数が 0 のとき 1 行を床にする」——「0 にしたら消える」という素直な読みから外れる・床を置けば機構が 1 つ増える。② `SPEC.md`「4.5 最大列挙数」が「最大表示件数が 0（`config.toml` の手編集でのみ到達する）のときも出さない」と**仕様として**書いている。③ `layout.rs:117-121`「`0.0` は『hide せよ』の契約値である。**0 を作ってはならないし、消してもならない**」。clamp は「作れなくする」側でこの契約に触れる |
| P7 | `plain_results_hidden` は④を含まない | `search_state.rs:727` | **真**。引数は `(view_kind, instant_rows, indexing)` の 3 つで、`max_results` も `row_height` も入らない |

## 関連ファイル・シンボル

- `src-tauri/src/egui_shell/layout.rs` — `results_window_height`（`:143`）/ `present_results`（`:401`）/ `ResultsInputs`（`:343`）/ `Metrics::from_config`（`:49`）
- `src-tauri/src/egui_shell/launcher_controller.rs` — `activate_or_execute`（`:535`）/ `shift_activate`（`:576`）/ `on_enter`（`:1380`）/ `activate`（`:224`）。**ソーステキスト検査 2 本**が `mod tests` にある——`activation_uses_the_frame_indexing_value_not_a_live_read`（`:1498`）と `activation_entry_points_consult_the_display_gate`（`:1533`）。母集団の切り出しは `method_body(src, anchor, canary)` で、終端の取り逃しは `method_body_is_line_ending_agnostic`（`:1472`・#1108 の同型は別件）が見る
- `src-tauri/src/egui_shell/window_coordinator.rs` — `max_results`（`:747`）/ `FrameIndexing`（`:111`）/ `read_indexing`（`:125`）/ `DriveResultsInputs`（`:776`）
- `src-tauri/src/egui_shell/search_state.rs` — `plain_results_hidden`（`:727`）
- `src-tauri/src/egui_shell/view.rs` — `read_visual` / `metrics`（`:568-569`）/ `indexing` の唯一の読み点（`:928-929`）/ `on_enter` の呼び出し（`:1097`）/ 連言③の読み点（`:1123`）/ クリック逆流の消費（`:1170-1187`）/ `indexing_is_read_exactly_once_per_frame`（`:1325`）
- `snotra-core/src/config.rs` — `AppearanceConfig::effective_visible_rows`（`:364`）/ `validate`（`:1061`）/ `ConfigError::VisibleRowsZero`（`:1065`）
- `SPEC.md`「4.5 最大列挙数」「4.7 結果表示制御（2 窓構成）」「8.6 状態遷移」
- `docs/adr/ADR-activation-gate-placement.md`（#1077 の否定の知識）/ `docs/adr/ADR-results-fixed-height.md`（#835）

## 再利用できる既存パターン

- **ゲートの置き場所**: `ADR-activation-gate-placement` 決定——`activate_or_execute` と `shift_activate` の冒頭。
  `start_launch`（却下 1）・`activate`（却下 2）へは置かない。**却下 1 の「数は減らない」理由は今回もそのまま当たる**
  （`tools >= 2` 枝が `activate_or_execute` を通らない）
- **判定は表示側と同じ述語を呼ぶ**（同 ADR 決定）。同義の別式を作らない
- **フレーム内 1 回読みの値を型で配る**: `FrameIndexing`（構築子が `window_coordinator` に private・却下 4/5）
- **検知器はソーステキスト検査**: `launcher_controller.rs` にテスト席が無い（`LauncherController` の構築が
  `AppHandle` と engine lock を要求する）ため、呼び出し点の脱落は `mod tests` の 2 本が固定する
- **実機再現の足場**: `SNOTRA_CONFIG_DIR` の使い捨てプロファイル（`docs/build-commands.md`「別プロファイルで
  起動するための env ハッチ（`SNOTRA_CONFIG_DIR`）」）＋ `SNOTRA_TRACE=1`。実 config を読みも書きもしない
- **trace の不変条件は不在で書く**（`src-tauri/CLAUDE.md`「trace の presence 検査は状態の検査ではない」）

## 技術的制約

1. **`max_results` を起動側へ配ると `window_coordinator::max_results` の doc（#749「読み点の制約を持たない」）が偽になる。**
   起動ゲートが同じ値を見る以上、読み点の制約は**生まれる**。#1077 が `indexing` について同じ転換をした先例がある
   （`view.rs` が 1 フレーム 1 回読み `FrameIndexing` で配る）。ただし `indexing` が `AtomicBool` の live-read
   だったのに対し、`visible_rows` は `read_config` 越しの config live-read で、変わる契機は `config_watcher` の適用に限られる
2. **`row_height` も起動側は持たない。** 連言④を `results_window_height(max, row) > 0.0` の形で共有するなら
   `row_height`（`VisualSnapshot` 由来・`metrics.row_height`）も渡す必要がある。P2 により `max_results == 0` だけで
   同値だが、**述語を rows だけの式で書くと同義の別式になる**——ADR の「同義の別式を作らない」と衝突しうる
3. **④は全ビューに効く（P4）ため、ゲートは `activate_or_execute` の冒頭（tool / instant の dispatch より前）に置く必要がある。**
   `plain_results_hidden` の後ろに並べるだけでは足りない（あちらは Results ビュー専用の carve-out を持つ）
4. **これは仕様変更である。** issue は「SPEC の記述は現状のままで正しい」と書くが、それは**現状の挙動**への評価であり、
   直せば文書化された挙動が変わる。`AGENTS.md`「開発ワークフロー」の 1 に従い `SPEC.md` → コード → ドキュメントの順に同期する
5. **`Config::validate` を本体の適用経路へ足す形は採らない**（P1 の裏返し）。それは設定の拒否であり、
   ADR-results-fixed-height 却下 5 が守った「0 にしたら消える」の読みを壊す（＝ (b) と同じ族）
6. **`on_enter` の空チェックは連言②の側であり、④とは独立である**（`!self.state.results().is_empty()`）。
   ④偽では行は非空のままなのでこのチェックは通る

## 未解決の疑問（plan.md の未確定欄で潰す）

- **Q1（実測）**: `visible_rows = 0` の使い捨てプロファイルで、`egui_results:show` が 1 度も出ないまま
  `egui_launch` が出るか。**修正が入った後では原理的に測れない**（#1077 のフェーズ 0 と同型）
- **Q2（設計）**: 連言④の値を起動側へどう配るか——(i) `view.rs` がフレーム冒頭で 1 回読み newtype で配る
  （`FrameIndexing` と同型・機構が 1 つ増える）、(ii) 起動側で `read_config` し食い違いを受容残余として明記、
  (iii) 連言④を `layout.rs` の名前つき述語へ切り出し `present_results` と起動側の両方が呼ぶ（値の運び方とは直交）
- **Q3（射程）**: → / ← のフォルダ突入を止めるか。#1077 が止めなかった理由は「突入すれば Folder ビューになり
  行は可視へ戻る」だったが、**④偽ではその理由が成り立たない**（窓高は 0 のまま）。それでも「止めるのは起動と
  tool 入場だけ」を維持するなら、理由を計画に書く
- **Q4（文書）**: 規範の置き場所は `SPEC.md`「4.5 最大列挙数」の 0 行 bullet か「4.7 結果表示制御（2 窓構成）」の
  ゲート節か。正本 1 か所の原則で決める
- **Q5（要求判断・Step 5c でユーザーへ）**: そもそも直すか。`visible_rows = 0` は設定 UI では作れず
  `validate()` がエラーに数える値であり、「やりすぎでは」の指摘がありうる。判断材料は Q1 の実測

## 敵対的調査（Step 3b）の所見と採否

母集団は本文書の全主張（P1〜P7・関連ファイルの行番号・技術的制約 1〜6・Q1〜Q5）。1 体（general-purpose / sonnet）。
返り値の全文は `workspace/adversarial-1106.txt`。

### 壊せた項目（1 件）

| 所見 | 採否 |
|---|---|
| `ConfigError::VisibleRowsZero` の行番号は `:1065` ではなく `:1066`（`:1065` は `if` 条件式の行） | **採用**。上表を訂正した。命題 P1 の真偽には影響しない |

### 壊せなかった項目（7 命題 + 制約 6 件 + 行番号 17 件）

P1〜P7 と技術的制約 1〜6 はいずれも独立の一次証拠で裏取りされ、反証されなかった。**独立に補強された点**を採用したもの:

- **P2**: production で `results_window_height` の `row_height` に届く経路は 1 本だけである——`ResultsInputs` の生きた構築点は `window_coordinator.rs:826`、その `i.row_height` は `view.rs:1285` の `metrics.row_height`、すなわち `Metrics::from_config`。`present_results` の production 呼び出しも `window_coordinator.rs:821` の 1 か所。`results_view.rs` の行高は描画専用でゲート判定に入らない
- **P3**: `.activate_or_execute(` の全文 grep が 4 件で尽きることを独立に確認。トレイメニュー経路（`launch_item_with_state` 等）は results 窓の行ではない別 UI 面、スラッシュコマンドは Enter を経ない（#1077 の PR 本文と一致）
- **P6**: ADR 却下 5 の「最大表示件数」は `visible_rows`（可視行数）であり `search.result_limit` との取り違えは無い
- **Q1(c)(d)**: trace 名 `egui_launch`（`launcher_controller.rs:246`）と `egui_results:show`（`window_coordinator.rs:855`）は実在する発火点を持つ。開発機の実 `config.toml` は `visible_rows = 8`（危険な値には未到達・読み取りのみ）

### 実質的な補足 1 件——所見は採り、機序は自分で測り直した

3b は「`SnotraSmoke.psm1` の `New-SnotraVerificationProfile` は `visible_rows` を既定で書かないので、実機再現には明示注入の 1 手が要る」と指摘し、機序として「`-AdditionalSections` で実現可能」と添えた。

**所見は採る。機序は誤りである**（`scripts/lib/SnotraSmoke.psm1:205-282` を自分で読んで測った）。同関数は `[appearance]` テーブルを**自分で発行する**（`window_width` / `show_icons`）。TOML はテーブルの再定義を許さないため、`-AdditionalSections` へ `[appearance]` を書くと **parse が落ちる**。`[general]` については同じ衝突を名指しで止める guard（`:229`）と専用パラメータ `-GeneralSection` があるが、**`[appearance]` にはどちらも無い**。

ゆえに Q1 の再現手順は **`New-SnotraVerificationProfile` を使わず `config.toml` を自分で 1 枚書く**（`docs/build-commands.md`「別プロファイルで起動するための env ハッチ（`SNOTRA_CONFIG_DIR`）」の形）。打鍵注入・trace 観測・窓の実測（`Send-SnotraKeyChord` / `Send-SnotraKey` / `Wait-SnotraTraceEvent` / `Wait-SnotraWindow`）は同モジュールの関数をそのまま再利用できる。計器側の改修（`-AppearanceSection` の新設）は**採らない**——セーフティネット（CI の e2e が使う共有ライブラリ）の変更であり、この issue の受け入れ条件に要らない。

### ⚠️（確信の持てない所見）の扱い

- `indexing_is_read_exactly_once_per_frame` の行番号「`:1316` 付近」は実際には `:1325`。**採用**し「付近」を消して実測値へ訂正した
- 「起動の入口」の母集団に `shift_activate` の `tools >= 2` 枝（`enter_tool` 直呼び）を含めるかの線引きが未定義、という指摘。**採用**——`plan.md` の受け入れ条件で「起動と tool 選択への入場の両方を止める」と明示的に定義する（#1077 が同じ 2 つを対象にしたのと揃える）

## #1077 のフェーズ 0 の形（PR #1107 本文より）

修正前の release バイナリを `SNOTRA_CONFIG_DIR` の使い捨てプロファイルで `SNOTRA_TRACE=1` 起動し、trace の時刻列で
`egui_results:hide` の 1.404 秒後の `egui_launch` を示した。**trace の presence だけに頼らず**
`Wait-SnotraWindow -Title 'Snotra Results'` の不成立で窓が実際に不可視であることも測っている。#1106 の再現も同じ形を採る
（ただし索引再構築も blur も要らない——`visible_rows = 0` は起動直後から連言④を偽にする）。
