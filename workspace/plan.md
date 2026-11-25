# 計画: issue #1526（性能の残り 3 件を測ってから要否を決める）

## 目的と受け入れ条件

- A（`skip_name` を Substring/Prefix へ）・B（表示計測の `ms` に入る trace）・C（`compute_wave1` の kana 分岐の並列化）のそれぞれに、測定値と採否が `PERFORMANCE.md` に記録されている
- 採ったもの（A・B）は実装され、採らないもの（C）は理由が記録されている
- C の前提の記述（`build.rs` の `compute_wave1` のコメントと `PERFORMANCE.md` の #1056 節）が、現行の経路に合っている

調査は `workspace/research.md`。測定は承認の前に済ませた（下の「未確定」の各項に値を置く）。

## 変更するファイル

- `snotra-core/src/search/scoring.rs` — `score_one_entry` の `skip_name` から `mode == SearchMode::Fuzzy` を外す。直上のコメントの「3 つの条件はどれも落とせない」の `Fuzzy` の項と「測っていない」を、実測と全モードの論証に直す
- `snotra-core/src/search/tests/path.rs` — `skipping_name_scoring_changes_nothing_over_real_index` を Prefix/Substring/Fuzzy の全モードで突き合わせる形へ広げる。`fuzzy_name_match_survives_when_display_name_contains_path_separator` を全モードのループへ広げ（失敗メッセージにモード名を入れる）、名前を `name_match_survives_when_display_name_contains_path_separator_in_every_mode` に改める。639〜646・666・694 行付近の doc と assert 文言の「Fuzzy スコアリング」「部分列」を直す
- A で偽になる現在形の記述（「Fuzzy だけ」「部分列」を前提にした文）を直す。母集団は `git grep -n "Fuzzy スコアリング\|skip_name\|any_name_has_path_sep\|部分列" -- snotra-core PERFORMANCE.md docs` で数えた（2026-11-25）。該当: `snotra-core/src/search.rs`（`any_name_has_path_sep` の doc・`caches_candidates` の doc の `skip_name` への言及）、`snotra-core/src/query.rs`（68〜76 行の `contains_path_sep` の doc）、`snotra-core/src/search/query_plan.rs`（`norm_query_has_path_sep` の doc）、`snotra-core/src/search/build.rs`（346・357 行のコメント）、`snotra-core/tests/path_query_cost.rs`（239 行「あれは Fuzzy を要求する」）。論証は「部分列」ではなく「Fuzzy は部分列、Prefix は接頭辞、Substring は連続一致。どれも needle の各文字が名前に現れることを要求する」の形で、1 か所（`scoring.rs` の `skip_name` のコメント）を正本にし、他はそこを指す。`any_name_has_path_sep` が表示名・`lower_names`・`lower_file_names` の 3 本の blob を測っていること（`build.rs` の `contains_path_sep(..names_blob())` の行）を正本のコメントに書く。`has_path_sep` と `norm_query_has_path_sep` の取り違えの適用範囲（今の nucleo で外延的に一致）が全モードに及ぶことも同じ箇所に書く
- `PERFORMANCE.md` の #1057 節（見出し「採用: パスクエリで name の Fuzzy スコアリングを行わない」）は見出しを変えない（当時の決定として真のまま。見出しは `query.rs`・`build.rs`・`scoring.rs` から引かれている）。節の冒頭に、全モードへ広げたことと新しい節への参照を 1 行添える
- `src-tauri/src/main.rs` — `setup_hotkey_listener` で `let t0 = Instant::now();` を `trace_main("hotkey:listener_enter", ..)` の後へ移す（post の前に取ることは保つ）。理由をコメントに書く
- `src-tauri/src/egui_shell/window_coordinator.rs` — `show_egui_main` の `egui_show:ime_control` の trace を、`egui_show:done` の trace の後へ移す（IME の送信の位置は変えない）
- `snotra-core/src/search/build.rs` — `compute_wave1` の kana 分岐のコメント（「この分岐が通るのは cache-miss（全走査）の内側だけ」）を、現行の経路に合わせて直す
- `PERFORMANCE.md` — A・B・C の節を足す。#1056 節の「`compute_wave1` の kana 分岐は逐次のまま残してある（通るのは cache-miss ＝ 全走査の内側だけ…）」を直す。#1446 節の `egui_show:done` の値に、trace の書き込みを含む計測だった旨を添える

## 実装の順序

1. A: テストを先に広げる（Red の確認は変異で行う・下記）→ `scoring.rs` を直す
2. B: `main.rs` と `window_coordinator.rs` の 2 か所
3. C: `build.rs` のコメント
4. `PERFORMANCE.md`
5. 検証

## 不変条件と異常系

- A: 結果（集合・順序）は全モードで不変。前提 `!any_name_has_path_sep` は構築時の実測で、外れた入力はスコアリングを受ける（既存のまま）。ガードは `norm_query_has_path_sep`（`has_path_sep` ではない）のまま
- A: needle は Prefix が `starts_with(plan.norm_query)`、Substring が `Finder::new(norm_query)`。どちらも `norm_query` を見るので、ガードと同じ値である（research.md）
- B: 製品（`SNOTRA_TRACE` 無効）の挙動は変わらない。`ms` の定義は「listener の入口（post の前）から show の完了まで」のまま、trace の書き込みだけを外す。`ShowAfterAltRelease` の経路の `t0` は変えない
- B: `b.send_command(TurnOffIme)` の位置は動かさない。trace のために `bool` を持ち回し、`bridge.lock()` のガードを `egui_show:done` の後まで持ち越さない。`heap-trace` feature のビルドでは `egui_show:done` の行のヒープ標本の時点が変わる（`PERFORMANCE.md` の B の節に注記する）
- B: `egui_show:ime_control` は、IME の送信をした show でだけ出ることを保つ。`egui_show:done` の後に出るようになる。この 2 つのイベントの順序を読むスクリプトは無い（`scripts/` の grep で `ime_control` 0 件・`listener_enter` 0 件）
- 計測ハーネスを走らせるときは実 `config.toml`・`history.bin`・`index.bin` を退避し、終わったらハッシュで無変化を確かめる
- 一時の計装・スパイクはコミットしない（測定の後に戻し済み。`git status` は `workspace/` だけ）

## テスト方針と検証コマンド

- A の検知器（`/implement`「3b. 委譲へ渡すもの」の委譲先が変異を当てる）:
  - 変異 1（対象を壊す向き）: `skip_name` から前提 `!self.any_name_has_path_sep` を外す → `name_match_survives_when_display_name_contains_path_separator_in_every_mode` が 3 モードの回で落ちること
  - 変異 2（検査が新しい分岐を見ているかを壊す向き）: `skip_name = mode != SearchMode::Fuzzy && plan.norm_query_has_path_sep`（Substring/Prefix だけ、前提なしで飛ばす）→ 同じテストが Substring/Prefix の回で落ち、Fuzzy の回は通ること
  - 広げたテストは実装前の版でも緑である（挙動を変えない最適化なので Red にはならない）。新しい分岐へ届くことは変異 2 で示す。`has_path_sep` と `norm_query_has_path_sep` の取り違えは、今の nucleo では外延的に一致するので検知器を置けない（`scoring.rs` の既存コメントのとおり・変異に使わない）
- 検知器の役割を書き分ける: 変異 1/2 を落とすのは合成 fixture の `name_match_survives_when_display_name_contains_path_separator_in_every_mode`（表示名 `c:\tool`・target `D:\other\app.exe`・クエリ `c:\tool`）だけである。`skipping_name_scoring_changes_nothing_over_real_index` は実索引で前提（区切りを含む名前が 0 件）が成り立つことと、その下で 3 モードの結果が一致することを見るだけで、変異は落とせない。これを release で実行し、3 モード × 実索引で一致すること、件数が出力に出ることを確かめる（`#[ignore]`・実索引依存）
- 本実装の差分が、A の測定に使った一時版と同じ（`mode == SearchMode::Fuzzy &&` を外すだけ）であることを `git diff` で確かめる
- A の額の再確認: `snotra-core/tests/path_query_cost.rs` の `measure_path_query_frame_cost_at_operating_point` を release で走らせ、Substring の区切りクエリが下の A の B 列と同じ帯にあることを見る（実ファイルを退避して）
- B の確認: release ビルドを専用プロファイル（Ctrl+F12 と Alt+F12・`ime_off_on_show = true`・`SNOTRA_TRACE=1`・stderr をファイル）で起動し、各 25 回の表示で `egui_show:done` の `ms` の p50 が、下の B の「`ms` − 区間内の trace」の帯（Ctrl 14.4 ms 前後・Alt 13.2 ms 前後）へ下がること。`egui_show:ime_control` が 25 回とも `egui_show:done` の後に出ること。駆動は smoke の共有モジュール（`scripts/lib/SnotraSmoke.psm1`）で組む一時スクリプト（コミットしない）
- `docs/build-commands.md`「変更後の検証チェックリスト」のカテゴリ A（Rust）と、ホットキーの表示経路に触れるのでカテゴリ C（`smoke:startup` / `smoke:egui`）
- `npm run governance:check`

## フェーズと作業項目

### フェーズ 1: A（`skip_name` を全モードへ）

- [ ] `search/tests/path.rs` の 2 本を全モードへ広げる（改名を含む）
- [ ] `scoring.rs` の `skip_name` から `mode == SearchMode::Fuzzy` を外し、コメントを正本として直す
- [ ] `search.rs`・`query.rs`・`query_plan.rs`・`build.rs`・`tests/path_query_cost.rs` の「Fuzzy だけ」「部分列」を前提にした記述を直す（正本を指す形へ）
- [ ] 実索引の突き合わせ（release・`#[ignore]`）を 3 モードで通す

### フェーズ 2: B（表示計測から trace の書き込みを外す）

- [ ] `main.rs` の `t0` を `listener_enter` の trace の後へ移し、理由をコメントに書く
- [ ] `window_coordinator.rs` の `egui_show:ime_control` の trace を `egui_show:done` の後へ移す
- [ ] 専用プロファイルで 2 経路 × 25 回を測り、`ms` の p50 が下がったことを確かめる

### フェーズ 3: C と記録

- [ ] `build.rs` の `compute_wave1` の kana 分岐のコメントを直す
- [ ] `PERFORMANCE.md` に A・B・C の節を足し、#1056 節と #1446 節を直す
- [ ] カテゴリ A・C と `npm run governance:check` を通す

## 用語

新しく導入する用語は無い。

## 未確定（実装前に潰す）

- [x] A の額 — 2026-11-25・開発機・release・実 `index.bin`（784,020 件・PATH マージで +251 件）・実 config（Substring・migemo on・`include_path_env = true`）。`measure_path_query_frame_cost_at_operating_point` に Prefix の行を一時的に足して（測定後に戻した）、A（main）→ B（`mode == Fuzzy` を外した一時版）→ A'（main）を同一セッションで回した。実ファイル 4 本は前後でハッシュ一致。p50（µs）:

  | 構成 | クエリ | A | A' | **B** | B は min(A,A') より drift を超えて速いか |
  |---|---|---:|---:|---:|---|
  | マージあり / Substring | `c:\` | 10,156 | 10,190 | **9,747** | はい（−409・drift 34） |
  | | `c:\users` | 8,274 | 8,140 | **7,985** | はい（−155・drift 134） |
  | | `c:\users\` | 8,081 | 8,146 | **7,825** | はい（−256・drift 65） |
  | | `\program files\` | 4,856 | 5,006 | **4,767** | いいえ（−89・drift 150） |
  | | `\zzz-no-such-path\` | 4,561 | 4,652 | **4,404** | はい（−157・drift 91） |
  | マージなし / Substring | 5 本 | — | — | — | 5 / 5 はい（−54〜−256） |
  | マージあり / Prefix | 5 本 | — | — | — | 2 / 5 |
  | マージなし / Prefix | 5 本 | — | — | — | 1 / 5 |

  対照 `users`（区切りなし）は A/A'/B とも 38〜41 µs で帯の内側。**Substring は閾値を超えた（9 / 10）。Prefix は超えない（3 / 10・速くも遅くもない）**。削減は Substring の区切りクエリで 54〜409 µs（p50 の 1〜4%）
- [x] （裁定）A をどの範囲で採るか — Substring だけが閾値を超え、Prefix は差が出ない。額は小さい（1〜4%）。問い: 「`mode == Fuzzy` の条件を外して全モードにする／Substring だけ足す／採らない」のどれか（AskUserQuestion・2026-11-25）。回答: 「全モードで承認・実装へ（推奨）」。計画は全モードで書いてある
- [x] B の額 — 2026-11-25・開発機・release に一時の計装（区間内の各 `trace_main` を直接計時して `egui_show:done` へ添える・測定後に戻した）。専用プロファイル（`ime_off_on_show = true`）・`SNOTRA_TRACE=1`・stderr はファイル。各 25 回:

  | 経路 | `ms` p50（min〜max） | `listener_enter` の書き込み | `ime_control` の書き込み | 区間内の trace の合計 p50 | `ms` − trace p50 |
  |---|---:|---:|---:|---:|---:|
  | Ctrl+F12（`ShowNow`・25 回とも） | 19.3（17.7〜29.9） | p50 2.7（1.5〜12.6）・区間内 | p50 2.2（1.6〜4.1）・区間内 | **4.9（25%）** | 14.4 |
  | Alt+F12（`ShowAfterAltRelease`・25 回とも） | 15.6（13.3〜22.1） | p50 9.7（1.4〜11.0）・**区間外**（`t0` を取り直す） | p50 2.3（1.6〜3.0）・区間内 | **2.3（15%）** | 13.2 |

  閾値（5%）を両経路で超えた → 直す。`listener_enter` の書き込みは 1.4〜12.6 ms の二峰（約 2 ms と約 10 ms）で、`PERFORMANCE.md`「計測と受け入れ基準」の「1 行あたり約 10 ms」は上の峰に当たる
- [x] B の直し方 — `t0` を trace の後へ移す（post の前に取る意味は保つ）。`ime_control` は `done` の後へ移す。却下: 書き込み時間を計って控除する形（#1032 の形）は計時のコードが製品に残る。`ime_control` を `done` の payload へ畳む形は、イベント名が消えて過去の trace と読み比べにくくなるうえ、得は 1 行ぶんしか変わらない
- [x] C の額 — 2026-11-25・開発機・release・実索引 784,020 件・一時のテスト（測定後に戻した）。並列版の列が逐次版と全件一致することを確かめた上で、各 5 標本:

  | 計測 | 値（ms） |
  |---|---:|
  | `compute_wave1(.., true)`（現行・kana 逐次） | 341〜374 |
  | `compute_wave1(.., false)`（kana なし） | 124〜136 |
  | kana を塊ごとの並列にした版（`rayon::join` で並走） | 148〜162 |
  | kana 単独・逐次 | 313〜322 |
  | kana 単独・並列 | 55 |

  並列化の得は約 190 ms。ただし製品で `compute_wave1` に来るのは、保存先が引けない cache-miss と、派生文字列を持たない旧版からの昇格の 1 回だけである（research.md）。閾値（到達がこの 2 つに限られる限り額によらず採らない）により**採らない**。`to_kana` の単価からの見積り（2.4 s）は外れた（実データは ASCII が大半で、単独の逐次は 313 ms）

## 承認

- [x] 承認済み — 2026-11-25 / 問い: "#1526 の計画（workspace/plan.md）には人の裁定が 1 件あります。A（パスクエリで名前の照合を飛ばす `skip_name` をどのモードまで広げるか）の実測では、Substring（実 config の値）の区切りクエリ 10 本のうち 9 本が、ゆらぎの幅を超えて 54〜409 µs（1〜4%）速くなりました。Prefix は差が出ませんでした。B（表示計測から trace の書き込みを外す）は採り、C（kana 分岐の並列化）は採らずに記録します。A の範囲をどうし、計画を承認しますか？" / 回答: "全モードで承認・実装へ（推奨）"

## セルフレビュー

- リスク: 高（listener のコード〈`setup_hotkey_listener`〉を変更する）
- plan-review: 独立レビュー1体（Step 2・人の裁定が前提にあるため。結果 `workspace/plan-review-1526.md`）
- エージェント数: 1（ほかに 3b の敵対的調査 1 体）
- 要対処: 3 件 → すべて反映（A で偽になる記述の母集団を grep で数え直して変更ファイルへ追加・論証を「部分列」から 3 モード別の形へ・検知器の役割の書き分け）。軽微のうち `send_command` の位置と `heap-trace` の注記を不変条件へ反映。主エージェントの自己照合: issue の 3 件すべてに作業項目あり／境界（Alt 経路・Ctrl 経路・`ime_off_on_show` 偽・区切りを含む表示名・3 モード）に検証あり／新しい状態・リソースは無い／より単純な形は B の却下案で検討済み／不変条件（結果不変）に検知器あり
- 未検証: B の修正後の実機測定とカテゴリ C は実装時に行う
- check スキル: なし（`/race-check` は計画段では起動しない。実装時に B の差分へ当てる）
- 散文の全件列挙: `prose:sweep` は実行していない（`/plan-review` Step 1 を通さず Step 2 だけを回した）。代わりに `git grep` で A の語の母集団を数えた（上記「変更するファイル」）
