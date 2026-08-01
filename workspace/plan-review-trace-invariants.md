# plan-review — #757 trace 不変条件（観点 1: SKIP 経路の網羅性 / 観点 2: D6 の挙動同値）

## 要対処

- **H1 が開く区間の境界イベント（`egui_hide:done`）自身が「要求レベル」で連続しうることが D2/D3 にも Phase 1 の Pester 計画にも無い。** `hide_egui_main`（`src-tauri/src/egui_shell/window_coordinator.rs:296-339`）は呼び出しごとに無条件で `save_placement_relative` → `window.hide()` → `main_visible=false` → `results.hide()`（**戻り値を無視して無条件に trace**・327-330 行のコメントが明記）→ working set trim → `trace_main("egui_hide:done", ...)`（339 行、**遷移の有無を問わず毎回発火**）を行う。呼び出し元は 2 系統ある——ホットキー `HideNow`（`main.rs:386-389`、`visible=true` のときのみ選ばれる）と `EGUI_HIDE_REQUESTED` リスナー（`egui_shell/mod.rs:399-403`、Escape/blur から emit）。`view.rs:210` の Escape 判定は `ctx.input(|i| i.key_pressed(...))`（エッジトリガー）なので**単発の Escape 押下では 1 回しか発火しない**が、ホットキー側の hide とリスナー側の hide が近接タイミングで両方走れば（例: hotkey トグルと blur 猶予〔`lifecycle.rs:62,76` `blur_should_hide`〕がほぼ同時に成立）、**`egui_show:done` を挟まずに `egui_hide:done` が 2 回連続する**入力が構成できる。
  D2 は H5 について「`egui_results:hide` のどちらの発火源も等しく separator とする」（plan.md:37）と非対称を明記するのに、**H1 が窓を開く側のイベント自身の連続については何も規定していない**。2 回目の `egui_hide:done` が窓の開始点を後方へ「上書き」する実装だと、1 回目と 2 回目の間に紛れ込んだ違反（＝main_visible ゲートが破れて `egui_results:show` が出た、まさに H1 が捕まえたいバグそのもの）の `seq` が新しい窓開始点より前になり、**評価対象から silently 外れうる**。Phase 1 のチェックリスト（plan.md:103-111）にも「連続 `egui_hide:done`」を種にしたケースが無い（連続を扱っているのは H5 用の「連続 `egui_results:hide`」だけ）。判定不能が PASS ではなく**「違反そのものが消える」**という、受け入れ条件 2 の列挙（該当イベント無し/rows読めない/main可視状態未観測/区間が閉じていない/traceが無い）のどれにも当たらない経路である。

- **D6「移行前後で判定が一致する」（plan.md:55）は、壊れた/途切れた行に対する検出力の点で崩れる。旧コードは JSON の妥当性を要求しない。** `smoke-egui.ps1:401-414` の orphan 検出は `Get-Content` の生行に対し `$_ -match '"event":"egui_results:show"'` という**部分文字列マッチ**であり、行全体が有効な JSON である必要が無い。新経路（`Read-SnotraTraceEvents`、`scripts/lib/SnotraSmoke.psm1:334-350`）は `ConvertFrom-Json` に**成功した行だけ**を返し、失敗した行は `catch` で黙って捨てる（348 行のコメントで明言）。
  `trace_main` が書く JSON は `src-tauri/src/trace.rs:49-57` の `json!({"seq":..,"ts_ms":..,"event":..,"data":..})` で、`Cargo.lock`（4478-4489 行、serde_json 1.0.151 の dependencies に `indexmap` が無い＝`preserve_order` feature 未有効）から、キーはデフォルトの `BTreeMap` 順＝**辞書順**（`data` → `event` → `seq` → `ts_ms`）でシリアライズされる。つまり `"event":"egui_results:show"` という部分文字列は行の**先頭ではなく `data` オブジェクトの後**に来る。行が `event` フィールドの完了より**後**・行末より**前**で途切れた場合（`rows` の値の途中、あるいは `seq`/`ts_ms` の途中で打ち切られた場合）、**旧コードの部分文字列マッチはヒットするが、新コードの `ConvertFrom-Json` は必ず失敗して該当イベントごと消える**。これは D6 自身が Phase 2b の最後で認めている変更（plan.md:132「壊れた行が捨てられる点は manual-smoke.ps1 側と同じ扱いにする」）と、55 行目の「現行と同値である」という言い切りが**同じ文書内で矛盾**している。オーケストレーション側（1500ms 静定待ち）が torn read の確率を下げてはいるが、ゼロにする仕組みではなく、D6 の「移行前後で同じく鳴ることを測る」（plan.md:131）フォールトインジェクションは、正常系の合成 trace だけでは**この検出力低下を再現できない**（意図的に行を切断した入力で確認しないと見えない）。

- **`Read-SnotraTraceEvents` が捨てた行は、受け入れ条件 2 が列挙する SKIP 理由のどれにも登録されない。** 「不変条件と異常系」節（plan.md:96）は「`[trace]` 行数と parse 成功件数の差を記録へ出す」とするが、これは `manual-smoke.ps1` の記録ヘッダに**補助情報として表示するだけ**で、`Test-SnotraTraceInvariants` の `Unjudgeable`/`SKIP` には反映されない設計（インターフェース節、plan.md:73-90）。もし**唯一の違反イベント行**がちょうど parse 失敗で落ちた場合、その区間・その不変条件は「違反が見つからなかった」ため `PASS`（H1 なら区間が閉じていれば `PASS`、閉じていなければ `SKIP`）と判定され、**記録の「trace判定」列は緑（または灰色）のまま**で、行数差はヘッダの別の場所にしか出ない——受け入れ条件 4「不一致は明示される」の対象にもならない。受け入れ条件 2 の「沈黙経路を列挙して全部塞ぐ」（plan.md:12）という自己申告の基準に対し、この経路は**列挙されておらず、塞がれてもいない**（見える化はしているが、判定への統合はしていない）。

## 軽微

- **`Sections` の個々の要素（`StartSeq`/`Id` 欠落）に対する耐性が Phase 1 のチェックリストに無い。** 「不変条件と異常系」節は「`Sections` が空」を例外安全ケースとして明記するが（plan.md:94, 111）、**空ではないが要素の形が壊れている**場合（例: `StartSeq` 欠落・非数値）は `Get-SnotraTraceProperty` を通せば安全に `$null` に倒せるはずだが、明示的なテストケースが無い。D1 の設計上、`Overall` 判定は `Sections` に依存せず `Events` 全体を舐めるため違反の見落としには直結しないが（`Sections` は帰属先の表示にしか使わない）、誤った区間へ帰属して報告が読みにくくなる可能性はある。

## 未検証

- なし
