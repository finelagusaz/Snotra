# #1443 の独立導出（plan.md / research.md / adversarial-* は読んでいない）

基準: ブランチ `perf/debounce-skip-idle-trailing`・HEAD `5880ea33`（作業ツリーに `workspace/` 以外の差分なし）。コードは読むだけ。

## 0. 前提の裏取り（issue の「事実」）

- `Debouncer::on_input` は `fire_leading = leading && !armed; armed = true`（`src-tauri/src/egui_shell/layout.rs:518-522`）。leading でも armed を立てる → `poll` は `armed && elapsed >= interval` だけで発火（同 :526-532）。**leading の後に入力が無くても trailing が撃たれる**ことをコードで確認。
- 呼び出し側: leading は `search_flow.rs:269-271`（`run_search_with(&prefix)`）、trailing は `search_flow.rs:297-299`（`run_search()`）。どちらも Results∧Plain なら `issue_search` → `search_tx.send`（`search_flow.rs:93-98`）。
- 同一クエリの重複排除は無い: `SearchDispatch::issue` は無条件に `next_seq += 1`（`search_dispatch.rs:30-37`）、worker は `coalesce` で「最後の 1 件」を選ぶだけ（`search_worker.rs:27-29, 44`）。
- interval の正本は `LauncherController::new_search_debounce`（`launcher_controller.rs:134-136`・50 ms・leading=true）。Debouncer の production 利用者はこの 1 つだけ（`grep -rn "Debouncer::new" src-tauri/src` を実行 → コードは `launcher_controller.rs:135` と `layout.rs` のテスト 3 件、他は doc の名指し）。

## 1. 変更すべきファイルとシンボル

### 必須（コード）

| ファイル | シンボル | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/layout.rs` | `struct Debouncer`（:485） | フィールドを 1 つ足す（例 `trailing_pending: bool`＝「leading の後に、まだ検索へ出していない入力があった」）。**`Debouncer` の中に置く**こと——`consume_reset_pending` が `Debouncer` を丸ごと作り直す（`frame_stages.rs:53`）ので、reset-on-show のクリアが構造で付いてくる。`LauncherController` 側に置くと `consume_reset_pending` の一覧へ足す義務が生じる（`search_state.rs:620-633` の警告と同型の残余） |
| 同 | `Debouncer::new`（:491） | 新フィールドを `false` で初期化 |
| 同 | `Debouncer::on_input`（:518） | `fire_leading` のとき `trailing_pending = false`、そうでなければ `true`（leading=false モードでは常に `true`）。`armed = true` は**そのまま**（下記 §6 の落とし穴 c） |
| 同 | `Debouncer::poll`（:526） | `armed && elapsed >= interval` で **armed は必ず落とし**、返り値は `trailing_pending`（落とす前に読み、`false` に戻す）。**armed を落とさずに false を返す実装は永久スピン**（§3 repaint） |
| 同 | `Debouncer::cancel`（:537） | `trailing_pending = false` も落とす（衛生。`poll` は `armed` を先に見るので実害は無いが、対称性のため） |
| 同 | `Debouncer::is_armed`（:510） | 振る舞いは不変。doc のみ（§2） |
| 同 `mod tests` | `leading_fires_on_burst_start_then_trailing`（:685） | 2 回目の `on_input` があるので現状のまま緑のはず。名前・メッセージは「leading の後に入力があれば trailing」へ寄せる程度 |
| 同 `mod tests` | 新規テスト（例 `leading_only_burst_does_not_fire_trailing`） | 受け入れ条件 3。中身と落とし穴は §5 |
| 同 `mod tests` | `no_leading_mode_only_trailing`（:701）/ `cancel_disarms_pending_trailing`（:708） | 変更不要（新実装でも緑のはず）。leading=false モードは production に呼び手が無い（`Debouncer::new(.., false)` は test の 1 件のみ）が、契約として維持 |

**`search_flow.rs` のコード変更は不要。** `poll_search_debounce`（:295-309）は `poll` の返り値で `run_search` を呼び、`is_armed()` で再要求するだけなので、Debouncer の契約変更がそのまま効く。`on_input_changed` の `request_repaint_after(interval)`（:272）は**残す**——leading だけのバーストでも、50 ms 後に `poll` が armed を落とすフレームが要る（`input_idle` の反転 → results の wake → アイコン要求、`results_view.rs:37` の doc の「armed → 非 armed の遷移で wake」）。

### 推奨（doc・コメント）——§2 の表を参照

## 2. 偽になる記述 / 真のまま残る記述

### 偽になる（直す）

| 場所 | 現在の記述 | なぜ偽になるか |
|---|---|---|
| `src-tauri/src/egui_shell/layout.rs:482-484`（`Debouncer` の型 doc） | 「on_input: … leading なら true を返し、以後 armed」「poll: armed かつ elapsed>=interval で trailing 発火し disarm」 | poll は armed を落とすが、発火は leading 後の入力があったときだけ |
| `layout.rs:504`（`is_armed` doc） | 「armed のまま（trailing 未発火）か」 | armed は「interval が未経過」であって「trailing が予約されている」ではなくなる。repaint 再要求の用途の説明は真のまま |
| `layout.rs:510-511`（`on_input` doc） | 「いずれにせよ armed にして trailing を予約する」 | leading の入力は trailing を予約しない |
| `layout.rs:519`（`poll` doc） | 「前回入力からの経過が interval 以上なら trailing 発火し disarm」 | 同上 |
| `src-tauri/src/egui_shell/search_dispatch.rs:9`（`Settled` doc） | 「打鍵起点には 50 ms の trailing debounce 待ちが必ず入るため、片方では worker 往復の費用を読めない」 | 孤立打鍵では settled は leading のものだけになり、`since_key ≈ since_dispatch`。「必ず」が偽（厳密には leading の settled について以前から偽だったが、孤立打鍵の最後の settled が trailing だったので実用上真だった）。両方を出す理由は「連打では trailing 待ちが入る」へ狭めれば真 |
| `src-tauri/src/egui_shell/mod.rs:103` | 「検索 debounce（leading + trailing）で消費する」 | ⚠️ 境界。trailing が条件付きになるだけで、語としては真。直さなくてよい |
| `search_flow.rs:300-301`（`poll_search_debounce` 内） | 「armed のまま = trailing 未発火」 | armed = 窓が未経過。trailing は撃たれないこともある |
| `search_flow.rs:296` | 「連打が収まって interval 経過したら最終クエリで検索し直す」 | ⚠️ 境界。「leading の後に打鍵があった場合だけ」と補えば正確 |
| `search_state.rs:91-93`（`FolderFrame::unsettled_at_entry` doc） | 「`Debouncer::poll` の trailing 発火で最後の打鍵から interval のうちに落ちる」 | armed が落ちるのは trailing の発火ではなく poll（発火しない場合も落ちる）。結論（減衰しない控え・安全側の過剰近似）は真のまま |
| `search_state.rs:603-611`（`is_unsettled` doc） | 「解けるのは trailing が発火するか（poll・最後の打鍵から interval）」 | 同上。「poll が armed を下ろすか」へ |
| `docs/architecture.md:167` | 「Debouncer（leading 発火 + trailing 50ms を arm）」 | trailing は leading 後の入力があったときだけ |
| `docs/architecture.md:210` | 「poll_search_debounce（trailing）」 | ⚠️ シーケンス図上は「条件付き」と注記する程度 |
| `docs/architecture.md:239` | 「発火しうるウィンドウは『打鍵 → 50 ms』から『打鍵 → 50 ms + worker の走査 … 』へ広がった」 | ⚠️ 連打（trailing が撃たれる）では真のまま。孤立打鍵では上端が `max(打鍵+50ms, leading の accept)` に縮む。上界としては真なので「偽」ではなく過大。直すなら「trailing を撃つバーストでは」と範囲を付ける |
| `scripts/smoke-egui.ps1:484` | 「debounce(50ms) の trailing を跨がせる」 | ⚠️ 120 ms の間隔の目的（各打鍵を別バーストにする）は真。孤立打鍵では trailing が撃たれなくなるので語だけが古くなる。「debounce の窓を跨がせる」へ |

### 歴史の記録（直さない・判断は ⚠️）

- `docs/superpowers/specs/2026-08-10-search-worker-design.md:10`「走るのはバースト先頭と静止 50 ms 後の 2 回」・`:39`「打鍵起点には 50 ms の trailing debounce 待ちが必ず入る」——変更後は両方偽になる。issue 本文が名指ししている。`docs/superpowers/` は設計時点の記録として扱われている（`docs/architecture.md:243` が同 spec §4.5 を「役目を終えた」と外から注記する流儀）。直すなら本文ではなく、外から指す側で注記する方が流儀に合う。⚠️ 管轄（どのスキルがこのディレクトリを持つか）は未確認
- `docs/superpowers/plans/2026-07-22-su3-m1-core-search.md:477-572, 973-1024` ほか plans 群——実装計画の履歴。直さない
- `PERFORMANCE.md:500-513`（2026-08-11 の A 側計測「leading と trailing の 2 回の検索が乗る」）——日付付き記録。直さない（同 :37 の規約「既存の記述へ遡及して補完しない」）
- `PERFORMANCE.md:2843`「単発の打鍵では leading と trailing の結果着で 2 回走る」——#1279 の記録内の但し書き。現在形だが記録の一部。⚠️ 遡及しない規約に従えば据え置き。#1443 の測定（§5）を新しい節として足すなら、そこで「この但し書きは #1443 以降 1 回」と外から指せる

### 真のまま残る（確認済み）

- `PERFORMANCE.md:12`「leading edge（初回即時発火）+ trailing 50ms」——trailing の条件が狭まるだけで、方針の記述として真
- `search_state.rs:1112-1135`（`unsettled_covers_in_flight_after_trailing_fired`）・`:1190-1209`（`folder_entry_captures_armed_not_only_in_flight` の「trailing 未発火のまま」）——テストは `is_unsettled` の入力を直接与えるので、Debouncer の変更では値が変わらない。「trailing 未発火で armed だけ立つ」状態は、leading 後の窓（今後は trailing を撃たない窓）でも非 leading 入力後の窓でも生じるので記述は真
- `search_state.rs:629, 637`（reset を忘れると poll が trailing を撃つ／armed は folder 中の trailing poll で落ちる）——前者は `trailing_pending` が立っている場合について真、後者は「poll で落ちる」の意味で真
- `search_flow.rs:55, 82, 150, 276`・`view.rs:1110, 1113`・`activation.rs:436` ——trailing が撃たれる場合についての記述で、真のまま
- `docs/superpowers/specs/2026-07-22-su3-search-experience-design.md`（決定 7「leading + trailing 50ms」）・`2026-07-23-su4-icons-visual-theme-design.md`（settled/trailing で spawn）——歴史の設計記録。直さない
- `layout.rs:520` `poll` の `#[must_use]` メッセージ「trailing 発火の信号を落とすと…最終クエリでは検索が走らない」——真（返り値が真のときの意味は変わらない）
- `layout.rs:519` `on_input` の `#[must_use]`——真
- `layout.rs:534-536` `cancel` doc——真
- `search_flow.rs:26-29` `clear_search` doc「クリア後に stale trailing 検索が発火」を防ぐ——真（打鍵が続いていれば trailing は予約されうる）
- `activation.rs:516` 「armed trailing がツール一覧を上書きしないよう掃除」——真
- `activation.rs:594-602` on_enter の flush 判定——式もコメントも真
- `search_state.rs:597-602`「armed だけでは足りない…trailing 発火の直後は armed==false かつ in-flight あり」——trailing が撃たれた場合について真
- `search_state.rs:603-607`「pending_seq == 0 だが未反映…armed がそれを覆う」——真（armed の意味を変えないので）
- `results_view.rs:37-41`（`input_idle` doc）——真。armed が下りる契機（poll / cancel / new）は不変
- `icon_textures.rs:389-433`（`icon_gate_keeps_input_idle_semantics`）——`view.rs:1165` の `let input_idle = !self.controller.is_search_armed();` を固定する。変更後も同じ式なので緑
- `launcher_controller.rs:127-136`（`new_search_debounce` doc「`Debouncer::new` は常に `armed: false` で建てる」）——新フィールドも false で建てれば真
- `lifecycle.rs:112-118`（armed 期限 2 つとの時刻の持ち方の差）——真
- `scripts/lib/SnotraTraceInvariants.psm1:397`「`Debouncer`（leading）がバースト中の再発火を抑える」——真
- `docs/adr/ADR-row-replacement-choke-point.md:34`（`armed || pending != 0` の armed は規約）——真
- `src-tauri/CLAUDE.md:32`「`layout.rs` — … debounce の純粋核」——真。モジュール構成の変化なし
- `SPEC.md:818`（コマンド完全一致で debounce をキャンセル）——真
- `view.rs:1113`「trailing debounce の poll と再 arm（armed の間は毎フレーム残余を要求し直す）」——真
- `view.rs:681`「debounce leading で同期検索」——**既に偽**（#1004 で worker 化済み）。#1443 の変更とは無関係。⚠️ ついでに直すかは範囲判断

## 3. 崩しうる既存の不変条件

前提: 提案は「armed の立ち/落ちは不変、poll の返り値だけを `trailing_pending` で絞る」。armed の式が不変なので、armed を読む消費者（`is_unsettled`・`input_idle`・`enter_folder` の控え・repaint の再要求）の入力は**値の列として同一**である。変わるのは「poll が true を返すフレーム」だけ。

| 不変条件 | 保たれる根拠 / 崩れる経路 |
|---|---|
| #1038 flush-on-Enter（`activation.rs:594-602`・`should_flush_on_enter`・`search_state.rs:648`） | `is_unsettled = armed \|\| pending != 0 \|\| restored_rows_stale`。armed は不変。leading の in-flight は `pending != 0` が覆う。**最終クエリ＝leading で出したクエリ**（leading の後に入力が無い）なので、leading の結果が採られれば最終クエリの結果である。flush の窓は「打鍵+50ms」と「leading の accept」の遅い方へ縮む（issue の第 3 の費用が減る方向）。偽陰性（未反映なのに false）が生じる経路は見つからなかった——下の「trailing が担っていた役目」を参照 |
| `is_unsettled` の armed disjunct が覆う「空クエリ・indexing・送信失敗の同期クリア後に armed だけ残る」 | armed は不変なので覆い続ける。ただし**解け方が変わる**: 以前は trailing が再実行して（indexing が明けていれば）検索を出し直した。今は leading が同期クリアした後 `trailing_pending == false` なら poll は何もせず armed だけ落ちる → `is_unsettled` は false、行は空のまま。indexing 由来は `consume_external_pending` の世代検知（`frame_stages.rs:84-93`）が再検索するので回復する（`state.rs:177-183` で `indexing=false` の直後に必ず `index_generation += 1`）。空クエリ由来は空のままが正しい。送信失敗由来は worker が死んでおり再送しても回復しない（`search_flow.rs:99-108`）。→ 保たれる |
| `results_view` の `input_idle`（`view.rs:1165`・`results_view.rs:37-42`） | `!is_armed()` のまま。armed は不変 → 値の列も不変。「armed → 非 armed の遷移で snapshot 差分 → wake 1 回」も、poll が armed を落とすフレームで起きる。**leading だけのバーストでは、そのフレームに新しい行が来ない**が、`RowsSnapshot::matches` が `input_idle` を比較に含む（`results_view.rs:75`）ので差分は生まれ、アイコン要求は起きる |
| folder 突入時の `unsettled_at_entry`（`search_state.rs:85-102, 351-360`・`folder_nav.rs:172, 207`） | 控える値 `is_unsettled(armed)` は不変。⚠️ 1 点だけ経路が変わる: 「打鍵（leading のみ）→ 50 ms 以内に `→` で突入 → 50 ms 以内に Escape」の往復では、以前は Results に戻った後の trailing が復元 query で再検索して行を新しくした。今は撃たれないので、復帰した行（`restored_rows_stale == true`）が次の打鍵まで残る。**ただし往復が 50 ms を超えれば以前から同じ**（trailing は Folder ビュー中に撃たれ、Folder 分岐の再フィルタで終わる・`search_flow.rs:72-80`）。Enter は `restored_rows_stale` で flush されるので誤起動はしない。人の操作で 50 ms 以内の往復は現実的でないので受容してよいと判断 |
| repaint の再要求（`search_flow.rs:302-308`・`src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」#711） | `is_armed()` の間だけ残余を再要求。armed は poll で必ず落ちる限り、時間経過で解消する不成立のまま。**崩れる経路は実装の誤りだけ**: poll が「false を返すとき armed を落とさない」形にすると、`remaining = interval.saturating_sub(elapsed) = 0` → `request_repaint_after(ZERO)` の永久スピン、かつ `is_unsettled` が恒久真（全 Enter が flush）、`input_idle` が恒久偽（アイコンが二度と取れない）。テストで固定すべき（§5） |
| reset-on-show（`frame_stages.rs:41-74`） | `search_debounce = Self::new_search_debounce()` で丸ごと作り直す。新フィールドを `Debouncer` の中に置けば構造で落ちる。外（`LauncherController`）に置くと、hide を跨いで `trailing_pending == true` が残り、show 直後の打鍵が leading で偽に戻すので実害は小さいが、`search_state.rs:620-633` が警告する「show を跨ぐ状態を足して一覧へ入れ忘れる」形になる |
| instant / command 突入の cancel（`search_flow.rs:274-291`・`activation.rs:517, 602`・`clear_search`） | `cancel` は armed を落とす。`poll` は armed を先に見るので trailing は撃たれない。新フィールドを cancel で落とさなくても正しいが、落とすのが対称 |
| `#[must_use]`（`src-tauri/CLAUDE.md`「処置を返す純粋核の強制」） | `poll` / `on_input` は既に `bool` を返すメソッド段の `#[must_use]` を持つ。シグネチャを変えない限り不変。返り値型を変える（enum 化など）なら型段へ |
| race（worker・channel） | Debouncer は UI スレッド内の純粋状態で、並行境界を足さない。worker へ出る要求の数が減るだけ。coalesce は「最後の 1 件」を選ぶので、要求が減っても選ばれるクエリは変わらない |

## 4. trailing が「同じクエリの再検索」以外に担っていた役目

`run_search`（trailing）が `run_search_with` を経て行うのは: `instant_prefix()` の読み直し（`search_flow.rs:58-61`）・`instant_rows_query = None`（:70）・view_kind ごとの分岐・Plain での `indexing` / 空クエリ / 送信失敗の判定。leading と trailing の間に**入力以外で**変わりうる入力を数える。

| 入力 | 変わる契機 | 以前の trailing の効果 | 変更後 | 判断 |
|---|---|---|---|---|
| `instant_command_prefix`（issue 既出） | config 変更（`config_watcher` → `CONFIG_APPLIED` は wake するだけで再検索しない・`mod.rs:519-533`） | 打鍵後 50 ms 以内に prefix が変わったときだけ、偶然再解釈された | 再解釈されない | 50 ms を超えた config 変更は以前から再解釈されない。偶然の 50 ms 窓を失うだけ。**問題にならない**（保証ではなかった）。Enter 時は `on_enter` が prefix を読み直して `is_plain` を決める（`activation.rs:597-598`） |
| 検索設定（fuzzy/substring 等の engine 側設定） | config 変更 | 50 ms 以内なら偶然新設定で再検索 | しない | 同上。⚠️ engine 設定変更が索引再構築（世代加算）を伴うかは未確認。伴うなら世代検知が再検索する |
| `indexing` | 索引構築の開始・終了 | 開始: 行を同期クリア。終了: 検索を出し直す | 開始: leading の結果が残る（表示は `plain_results_hidden` が隠す）。終了: 世代検知（`frame_stages.rs:84-93`）が再検索 | 保たれる。⚠️ 構築開始の直前に出た leading の結果が構築中に採られうるが、これは以前から（trailing より前に着けば）起きていた |
| view_kind | `→` で folder 突入 / Escape / tool 突入 | Folder 中なら再フィルタ（無害）、Tool 中は no-op | 撃たれない | 影響なし。folder の往復は §3 の ⚠️。Tool は入力欄が編集不可（`view.rs:714` `input_editable = !in_tool && !is_launching()`）で、突入時に `cancel`（`activation.rs:517`）するので、Tool の往復で trailing が担っていた役目は無い |
| in-flight の失効（`put_rows`） | `on_escape`（folder/tool 復帰）・`start_launch`・`clear_search`・`reset` | 失効した leading を trailing が出し直した | 出し直さない | `on_escape` の folder 復帰は §3 の ⚠️（50 ms 以内の往復のみ差）。`start_launch` は成功なら hide、失敗・timeout・切断は自前で `run_search`（`activation.rs:238, 267, 284`）。tool 突入は cancel 済み |
| `query` 自体 | `set_query` の呼び手は `on_input_changed`（打鍵）と `clear_search`（cancel 込み）だけ。`on_escape` は突入時と同じ query を戻す（`search_state.rs:513`） | — | — | 入力以外で query が変わる経路は無い（`grep -rn "set_query(\|self.query = " src-tauri/src`） |
| 行の世代（`rows_generation`） | trailing の結果着の `put_rows` | 打鍵ごとにもう 1 回世代が進み、#714 によりその世代交代フレームはスクロールがアニメーションなし、選択は clamp で維持 | 進まない | 改善方向。leading の結果着後 50 ms 以内に ↑↓ で動かした場合、以前は trailing 着で瞬時スクロールが 1 回挟まった。⚠️ 観測可能な差だが退行ではない |
| アイコン要求の wake | — | trailing の結果着も snapshot 差分を作った | leading の結果着と armed→非 armed の反転だけ | `input_idle` の反転で wake されるので足りる（§3） |
| 計時 trace の意味 | `egui_search:settled` の `since_key_us` | 孤立打鍵の最後の settled は trailing のもので、`since_key ≈ 50 ms + 走査` | leading のもので `since_key ≈ since_dispatch` | **計測の意味が変わる。** 過去の `since_key_us` と比較する A/B は成立しない（`docs/superpowers/specs/2026-08-10-search-worker-design.md:39` の前提が崩れる）。⚠️ この値を判定に使う検査は見つからなかった（`SnotraTraceInvariants.psm1` は H7 で `dispatch_seq` / `pending_seq` だけを見る） |
| 送信失敗（worker の死）の検知 | `send` の `Err` | 「検知は 1 要求ぶん遅れる」（`search_worker.rs:35-37`）——leading が死を招いても trailing の send が死を検知してクリアした | 次の打鍵まで検知されない | ⚠️ **見落としやすい役目。** worker が leading の要求を処理中に死ぬ（`AppState` 不在 / engine の毒）と、以前は 50 ms 後の trailing の send が `Err` を受けて行をクリアした。今は次の打鍵まで pending が残る。ただしその間 `pending != 0` なので `is_unsettled` が真→Enter は同期 flush（`engine.lock().unwrap()` は毒なら panic・release は abort）。worker の死は「現に到達しない」と doc 自身が書く経路（`search_worker.rs:51-53`）。実害は無視できるが、`search_worker.rs:35-37` の「1 要求ぶん遅れる」は「次の送信まで」へ読み替わり、その「次」が以前より遠くなる |

## 5. 受け入れ条件の測り方とテスト

### `egui_search:settled` を数える案の欠陥

`settled` は「採られた結果」であって「worker へ送った要求」ではない（`search_flow.rs:187-226`）。要求 1 件は次のどれかになる:

1. 採られる → `egui_search:settled`
2. 追い越されて捨てられる → `egui_search:dropped`（reason=`seq`）／Folder へ遷移済み → `dropped`（reason=`view`）
3. worker の `coalesce` で吸われる → **trace が出ない**（`search_worker.rs:44`）
4. そもそも送らない（空クエリ・`indexing`・送信失敗の同期クリア・`search_flow.rs:88-91, 103-108`）→ trace が出ない

ゆえに:

- **修正前でも settled が 1 になりうる（偽の合格）。** leading の走査が 50 ms を超えると、trailing の issue が pending を上書きし、leading の結果は `dropped(seq)` になる。実運用点では `c:\` の検索が p50 41,882 µs / max 49,942 µs（`PERFORMANCE.md:504-508`・2026-08-11）で 50 ms と拮抗しており、遅い機体・大きなコーパスでは修正前でも「settled 1 件」が普通に出る。worker が前の要求を処理中なら leading が coalesce で消える場合もある
- **indexing 中に打つと 0 件**（修正前後とも）。smoke は起動直後の索引構築が明けてから打つ必要がある
- 空クエリ（Backspace で消し切る）は修正前後とも 0 件

**正しい数え方: `dispatch_seq` の差分を使う。** `SearchDispatch::issue` は 1 要求ごとに `next_seq += 1`、`SearchState::reset` も dispatch を作り直さない（`search_state.rs:535-545` は `put_rows` だけ）ので、seq はプロセス寿命で単調・連番。孤立打鍵 k の settled の `dispatch_seq` と打鍵 k−1 の settled の `dispatch_seq` の差が、その打鍵で発行した要求の数（coalesce / dropped を含む）。修正前は 2、修正後は 1。smoke のパスクエリ注入（`scripts/smoke-egui.ps1:482-488`、`c` `:` `\` を 120 ms 間隔）が使える。**ただし判別力があるのは直前に interval を十分超える間隔がある打鍵だけ**——`c` の直前は Backspace の後 `Start-Sleep -Milliseconds 50`（:481）で、50 ms は debounce の interval ちょうどなので、`c` が Backspace の窓の中に落ちると非 leading になり、修正前後とも 1 要求になる（修正前でもバグが見えない）。判別に使うのは `:` と `\`（直前 120 ms）の 2 打鍵: 修正前は各 +2、修正後は各 +1。代替として `settled + dropped` を数える方法は coalesce を取りこぼすが、孤立打鍵（間隔 120 ms ≫ 走査）なら coalesce は起きないので実用上は同じ値になる。⚠️ どちらも「打鍵の間に別の発行源（世代検知の再検索・launch 失敗の再検索）が挟まらない」前提

⚠️ **trace の presence 検査は状態の検査ではない**（`src-tauri/CLAUDE.md`「trace の presence 検査は…」）。受け入れの 1 番は単発の実測で足りるが、常設の不変条件にするなら「孤立打鍵の区間で dispatch_seq の増分が 2 以上なら違反」の形にする。ただし上の前提（他の発行源が挟まらない）があるので、常設化は誤検知の余地がある——一回きりの検証に留める方が安い

### 連打の最後のクエリ（受け入れ 2）

`a` → `b`（50 ms 以内）→ 静止: 修正後も `b` の on_input は非 leading で `trailing_pending = true` → poll が真 → `b` で検索。settled の最後の `dispatch_seq` が `b` の入力の後に発行された seq であることで測れる（trace はクエリ文字列を出さない・`search_flow.rs:236-237` の方針）。

### Debouncer のユニットテスト（受け入れ 3）と落とし穴

追加するテストは、少なくとも次を**同じテストの中で**表明する:

```
let mut d = Debouncer::new(50ms, true);
assert!(d.on_input());              // leading
assert!(d.is_armed());              // (a) leading でも armed は立つ
assert!(!d.poll(30ms));             // 窓の中
assert!(d.is_armed());
// (c) 窓の中の 2 回目の on_input は leading にならない——ただしこれを同じ d で打つと
//     trailing_pending が立ち受け入れ 3 を測れなくなるので、別インスタンス／既存テストで表明する
assert!(!d.poll(50ms));             // (受け入れ 3) leading だけのバーストでは trailing を撃たない
assert!(!d.is_armed());             // (b) それでも armed は落ちる
assert!(d.on_input());              // 窓が明けた次の入力は leading
```

落とし穴（バグがあっても緑になる形）:

- **(a)/(c) を欠く**: 「leading では armed を立てない」実装は `!poll(50)` を通るが、窓の中の 2 打鍵目がまた leading になり、**連打で毎打鍵検索する**（debounce が消える）。`!poll` だけを見るテストはこれを通す。「leading 直後の 2 回目の `on_input` が false」を必ず表明する（既存 `leading_fires_on_burst_start_then_trailing` の `!d.on_input()` がこれを持つので、両テストが揃って初めて塞がる）。また armed が立たないと `is_unsettled` の armed disjunct が空クエリ／indexing の同期クリア後の窓を覆わなくなる
- **(b) を欠く**: 「false を返すが armed を落とさない」実装は `!poll(50)` を通るが、`poll_search_debounce` の再要求が `request_repaint_after(ZERO)` の永久スピンになり、`is_unsettled` 恒久真・`input_idle` 恒久偽（§3）。`!d.is_armed()` を表明する
- **leading=false モードの退行**: `trailing_pending` を「leading が撃たれなかったとき」ではなく「2 回目以降の on_input」で立てると、leading=false で単発入力の trailing が消える。既存 `no_leading_mode_only_trailing` が塞ぐ（変えずに残す）
- **cancel 後**: 既存 `cancel_disarms_pending_trailing` で足りる。加えるなら「非 leading 入力 → cancel → poll(100) が false」（`trailing_pending` を cancel で落とし忘れても armed が先に弾くので、このテストは実装の差を検出しない——それ自体は害が無いことの確認でしかない）
- **時刻**: `poll` は `elapsed_since_input`（最後の入力から）を受ける。leading 直後の非 leading 入力で elapsed が 0 に戻る呼び出し側の責務は `search_flow.rs:268`（`last_input_at = now` を `on_input` より前に）にあり、Debouncer のテストでは測れない

`LauncherController` の配線（`poll_search_debounce`）は AppHandle を要し単体テストが無い。配線を変えない以上、Debouncer のテストと smoke の seq 差分で足りる。

### 検証カテゴリ

`docs/build-commands.md`「変更後の検証チェックリスト」の A（cargo test / clippy / doc——doc を触るので `cargo doc` を手で）、doc に `docs/architecture.md` を含めるなら `npm run governance:check`。打鍵経路の挙動変更なので C（`smoke:egui`）が該当しうる（`.claude/rules/src-tauri.md`「検証カテゴリは…コードパスの意味で決める」）。⚠️ 本導出では smoke を走らせていない。

## 6. より単純な代替の検討

| 案 | 判断 | 根拠 |
|---|---|---|
| `run_search_with` で「同じクエリなら飛ばす」 | **却下（issue の禁止はコードで裏付く）** | 同じクエリで出し直すのが正しい呼び手が 4 種ある: 索引の世代交代（`frame_stages.rs:84-93`・索引が変わったので同じクエリでも結果が変わる）、launch 失敗・timeout・切断（`activation.rs:238, 267, 284`・runRefresh parity）、folder の drain（`folder_nav.rs:125`・Folder 分岐だがこの関数を通る）、`on_enter` の flush（別経路だが「最終クエリの結果を同期で作り直す」という同じ意図）。さらに `run_search_with` は Instant / Command / Folder の分岐も持ち、「クエリが同じ」は `view_kind`・prefix・`indexing`・folder_cache の変化を見ないので、Instant の行や folder の再フィルタまで止めてしまう |
| `SearchDispatch` / worker で「直前と同じクエリなら結果を再利用」 | 却下 | 索引の差し替え（#1442 の `PathCursor` と同種の問題）で古い結果を返す。世代をキーに含めれば成立するが、`SearchDispatch` は今クエリ文字列を持たず（`search_dispatch.rs:24-25`）、純粋核にクエリと索引世代を持ち込むことになる。費用対効果で Debouncer 案に劣る |
| `poll_search_debounce` で「leading で出したクエリと同じなら trailing を飛ばす」（controller が直前の発行クエリを覚える） | 次善 | trailing 経路だけに効くので issue の禁止には当たらない。A→B→A（`ab` → Backspace で `a`、50 ms 以内）でも正しく飛ばせる点は Debouncer 案より精密。ただし prefix・`indexing`・view の変化を同一性に含めないと §4 の役目を誤って消す、`reset`/`clear_search` で記憶を落とす義務が増える、純粋核の外の状態で単体テストできない。Debouncer 案の方が単純で、A→B→A の余分な 1 回は issue の主題（孤立打鍵）ではない |
| leading=false（trailing のみ） | 却下 | 毎打鍵 50 ms の遅延が戻る（`PERFORMANCE.md:12` の採用理由の逆） |
| leading では armed を立てない | 却下 | §5 の落とし穴 (a)/(c)。バースト中の全打鍵が leading になり、検索がむしろ増える |
| `is_unsettled` / `input_idle` を `armed && trailing_pending` へ絞る | 本 issue の範囲外 ⚠️ | 孤立打鍵の 50 ms 窓での不要な flush やアイコン待ちを減らせるが、armed disjunct が覆う「同期クリア後に armed だけ残る」状態（`search_state.rs:603-607`）の覆いが崩れる。issue も式は変えないと明記。別 issue にするなら `/state-check` 案件 |

## ⚠️ 所見のまとめ

1. 送信失敗（worker の死）の検知が「次の打鍵まで」遅れる（§4 最終行）。現に到達しない経路で実害は無いが、`search_worker.rs:35-37` の doc の含意が変わる
2. 50 ms 以内の folder 往復で、復帰した古い行が次の打鍵まで残る（§3）。50 ms 超の往復では以前から同じ
3. `egui_search:settled` の数え上げは修正前でも 1 になりうる（leading の走査が 50 ms と拮抗・coalesce）。`dispatch_seq` の差分で測る
4. `since_key_us` の意味が孤立打鍵で変わる（trailing 待ちが入らなくなる）。過去値との A/B 比較は成立しない
5. trailing の結果着による世代交代（瞬時スクロール 1 回）が消える。改善方向だが観測可能な差
6. engine 側の検索設定の変更が世代加算を伴うかは未確認（伴わなくても、以前も 50 ms 窓の偶然に頼っていただけ）
7. `docs/superpowers/specs/2026-08-10-search-worker-design.md:10, 39` は偽になるが、歴史記録として扱う流儀か要確認
8. `view.rs:681` は #1443 と無関係に既に偽（同期検索と書いている）
9. `search_dispatch.rs:9`（`Settled` doc）は製品コードの doc で、変更により偽になる（spec:39 と同じ主張の現役の写し）
10. smoke の `c` は直前 50 ms の sleep で debounce 窓の境界に乗り、修正前でも 1 要求になりうる。判別は `:` と `\` で行う

## 母集団の取り方（§2）

`git grep -n "trailing" -- src-tauri snotra-egui-runtime scripts docs .claude ':!docs/superpowers/plans' ':!.claude/worktrees'` を切り詰めずに実行し、全ヒットを §2 のいずれかへ振り分けた。`leading` / `armed` / `debounce` の散文は §0〜§3 の作業中に個別に読んだ範囲に留まる（⚠️ 網羅の検索ではない）。
