# plan: issue #697 — hidden 中の paint 抑止機構の実測 + トートロジーテスト + 決定 5 の記録

前提は `workspace/research.md` と `workspace/frame-scheduling-design.md`（契約④の接地作業・実施順序 §8 の 1 番）。type:refactor / size:S。**挙動変更なし**——入るのは env ゲート内の計器・コメント・テスト削除のみ。

plan-review（2026-07-26・偵察 2 体 + 独立導出 1 体 = `workspace/plan-review-2b.md`）の指摘を反映済み。初版からの主な変更: 書き換え対象文書を 2 → 5 箇所へ拡張・受信計器に `runtime_id` を付記・計器 env を新設・`PERFORMANCE.md` 計器一覧の更新を追加。

## 変更ファイル一覧

| ファイル | 変更 | Phase |
|---|---|---|
| `snotra-egui-runtime/src/repaint.rs` | worker の `proxy.send_event` 直前に送信側計器 1 行（`SNOTRA_EGUI_WAKE_TRACE` ゲート内・`window_id` 付き） | 1 |
| `snotra-egui-runtime/src/runtime.rs` | ① `RedrawRequested` arm 冒頭（`window_id_map` 引き当て前）に受信側計器 1 行（同ゲート内・tao `window_id` + 引き当て結果 `runtime_id`〔`Option` のまま〕を出す）② 到達可能性注記の「（本ブランチ b9a9caf）」を PR #677 へ言い換え（既に同注記が「#671 サイクル PR A」と呼んでいるため「（PR #677）」で置換し重複呼称を避ける）③ 測定後 (A): `visible` ガード注記の「未測定・推測」を実測結果へ ④ 測定後 (A): テスト `hidden_window_is_not_painted` を削除 | 1, 3 |
| `PERFORMANCE.md` | egui 計器一覧（自己宣言された計器の正本）へ `SNOTRA_EGUI_WAKE_TRACE` を 1 行追加 | 1 |
| `src-tauri/src/egui_shell/mod.rs` | 測定後 (A): `wake_main` doc の「OS/tao 層にあると推測されており未測定」を実測結果（日付・#697）へ。あわせて `wake_results` doc に「削ると壊れる理由は `drive_results_window` 側コメント」の 1 行ポインタ（Phase 4） | 3, 4 |
| `src-tauri/src/egui_shell/layout.rs` | 測定後 (A): `results_should_show` doc の「機構は未同定・未測定」を実測済み（#697）へ（「命題に依存しない設計」という主張自体は保持） | 3 |
| `src-tauri/CLAUDE.md` | 測定後 (A): 「hidden 中は `update()` が走らない（実測・SU5 要石）」に同定した機構と #697 を括弧内追記（挙動主張は不変・小変更） | 3 |
| `docs/superpowers/specs/2026-07-25-egui-window-ownership-and-event-delivery-design.md` | 測定後 (A): §7 残余 2・残余 3 に errata 追記（「#697 で測定済み / 処分済み」各 1〜2 行。本文は改変しない——日付付き spec は歴史記録・#646 spec の errata 前例に倣う） | 3 |
| `src-tauri/src/egui_shell/view.rs` | `drive_results_window` 末尾（現行 :857）の無条件 `wake_results` に「削ると壊れる理由」コメント（決定 5 の記録・1〜4 行） | 4 |
| （スクラッチパッド）`measure-697.ps1` + ログ | 測定スクリプトと生ログ。**リポジトリへは入れない**。結果サマリは issue #697 へコメント | 2 |

## 実装順序

### Phase 1 — 計器 2 本 + 計器正本（`snotra-egui-runtime` / `PERFORMANCE.md`・挙動変更なし）

1. `repaint.rs` worker ループ内、`proxy.send_event(...)` の**直前**:
   ```rust
   if std::env::var_os("SNOTRA_EGUI_WAKE_TRACE").is_some() {
       eprintln!("SNOTRA_EGUI_WAKE_SEND window_id={window_id:?}");
   }
   ```
2. `runtime.rs` の `Event::RedrawRequested(window_id)` arm 冒頭、`window_id_map` 引き当てより**前**（引き当て失敗で握りつぶされる経路も観測対象・issue コメント指定）。ただし引き当て自体は非破壊参照なので、**引き当て結果も同じ行に出して窓の帰属を付ける**（plan-review 指摘: 帰属なしでは判定表が「別窓由来の RECV」で誤読しうる。送信側の `window_id` は runtime 層 `WindowId` なので、この `runtime_id` と直接照合できる）:
   ```rust
   if std::env::var_os("SNOTRA_EGUI_WAKE_TRACE").is_some() {
       eprintln!(
           "SNOTRA_EGUI_WAKE_RECV window_id={window_id:?} runtime_id={:?}",
           context.window_id_map.get(window_id)
       );
   }
   ```
3. 同 `runtime.rs` の到達可能性注記「（本ブランチ b9a9caf）により」→「（PR #677）により」（`b9a9caf` は現リポジトリ全履歴で解決しないハッシュと確認済み。#677 のタイトル「RuntimeFrame::hide_window 削除」と注記内容の一致も確認済み。測定と独立・先にやってよい）
4. `PERFORMANCE.md` の計器一覧へ `SNOTRA_EGUI_WAKE_TRACE`（何を出すか・目的・#697）を 1 行追加
5. **env を新設する理由**: #628 の前例は計器族ごとに別 env（`SNOTRA_EGUI_REPAINT_TRACE` / `SNOTRA_EGUI_PAINT_TRACE`）。既存 env に相乗りすると、`measurement.md` の既存プロトコル（REPAINT 行数を数える）の出力に未知の行が混ざる
6. 計器は**恒久設置**（#628 と同じ扱い。env 未設定時のコストは `var_os` 1 回。#737 のフレーム上限実装時の検証にもそのまま使う）
7. 検証: post-edit hook（clippy + crate tests・沈黙=合格）+ `npm run smoke:egui` 1 回（runtime のイベント経路に触れたため・カテゴリ C）+ `npm run governance:check`（`PERFORMANCE.md` はガバナンス文書・カテゴリ F・hook は走らない）。smoke の trace パーサは `[trace]` prefix の JSON 行のみを読むため新しい生 stderr 行に寛容（plan-review で実測確認済み）

### Phase 2 — 実機測定（コード変更なし・実測 1 回・Phase 1 のコミット後に固定コードで行う〔#489〕）

スクリプトはスクラッチパッドに置き、`scripts/smoke-egui.ps1` の部品（keybd_event 注入・`hotkey:registered` からの VK 導出・stderr 捕捉）を流用する。**ホットキー注入は smoke の実績ある経路**——注入で show が観測できない場合のみ、実打鍵の協働（#558 の流儀）へフォールバックする。

プロトコル（区間は stderr の `[trace]` イベントで区切る）:

1. `cargo build --release`（#628 実測と同条件。`predicted_dt = 0`〔PR #709〕後のコードであることを結果に明記）
2. `SNOTRA_TRACE=1` + `SNOTRA_EGUI_WAKE_TRACE=1` + `SNOTRA_EGUI_REPAINT_TRACE=1` で起動、stderr をログへ
3. **区間 V（可視・陽性対照）**: ホットキー注入で show → 3 秒放置（フォーカスありのキャレット点滅が毎パス `request_repaint_after(≤0.5s)` を撃つ）。SEND・RECV・REPAINT が全部 >0 を確認——**計器が生きている証拠。これ無しでは「hidden で 0」が計器故障と区別できない**。RECV と REPAINT の件数一致もクロスチェック（`visible` 恒真ゆえ受信すれば必ず REPAINT が伴う）
4. **区間 H1（hidden・無刺激）**: Escape 注入で hide（`egui_hide:done` で区間開始）→ 10 秒放置。hide 直前に積まれたキャレット点滅の deadline（≤0.5s）が満期を迎える——config 刺激と独立な自然刺激
5. **区間 H2（hidden・config 刺激）**: `%APPDATA%\Snotra\config.toml` を**同一内容で atomic 書き戻し**→ 10 秒放置。`CONFIG_APPLIED` は load 成功なら値の差分と無関係に emit（`config_watcher.rs:162`・plan-review で早期 return 全 4 経路を確認済み）、`IndexInputs` 差分も空ゆえ `indexing-*` の混入なし。**実 config の値は一切変えない**（独立導出は visual-only キー変更を提案したが、無変更書き戻しで同じ emit が得られることをコードで確認済み——値を触らない側が安全）
6. プロセス kill（スクリプトに finally 相当を置き、途中失敗でも回収）、ログ解析

判定表（H1 + H2 の hidden 区間・main 窓帰属の行で数える）:

| SEND | RECV | 結論 | 続き |
|---|---|---|---|
| ≥1 | 0 | **(A) tao/OS 層が hidden HWND への配送を落とす** — 予測どおり。H1（キャレット）と H2（config）の両方で SEND が出れば確度が上がる | Phase 3 へ |
| 0 | 0 | (B) worker が送っていない（deadline 管理側の欠陥） | **止まる**: 結果を報告し、調査を別 issue へ起票。Phase 3 は行わない |
| ≥1 | ≥1（REPAINT も >0） | (C) hidden 中も `update()` が走る＝不変条件そのものの反証（挙動引用 11 箇所が誤りになる大事故） | **止まる**: 報告 + `visible` ガード実効化の設計を別 issue へ。文書反映は行わない |

- 区間 V で SEND/RECV/REPAINT のいずれかが 0 → **測定不成立**として止まる（計器か手順の欠陥）
- H2 で `[trace]` に config 適用系イベントが出ない → 刺激未達（watcher の debounce・ReadFailed リトライを疑い、書き戻しを再試行）

### Phase 3 — 接地の書き戻し（結果 (A) の場合のみ・doc とテストのみ・5 箇所 + テスト削除）

書き換え対象は独立導出の間接参照分類（`plan-review-2b.md` §4）に従う——**機構を主張している 5 箇所だけを書き換え、挙動だけを引用する 11 箇所は触らない**（不変条件は測定後も真のまま）。日付付き spec/plans は凍結、唯一の例外が 2026-07-25 spec への errata **追記**。

1. `mod.rs` `wake_main` doc: 「——ただしその抑止は wake 経路ではなく OS/tao 層にあると推測されており未測定」→「（2026-07-XX 実測・#697: worker は `RequestRedraw` を送るが、hidden な窓には `RedrawRequested` が配送されない。抑止は tao/OS 層）」の趣旨へ
2. `runtime.rs` `visible` ガード注記: 「未測定」の理由付けを実測結果で更新（ガード自体は到達不能のまま残す——「将来 runtime 側での抑止が必要になったときの受け口」という現行の理由は実測後も成立）
3. `layout.rs` `results_should_show` doc: 「機構は未同定・未測定」→ 実測済み（#697）。「命題に依存しない設計」は保持
4. `src-tauri/CLAUDE.md`: 「（実測・SU5 要石）」→「（実測・SU5 要石。機構は tao/OS 層の配送抑止・#697）」の趣旨で括弧内追記
5. spec 2026-07-25 §7 残余 2・残余 3: errata 追記（各 1〜2 行・本文不変）
6. テスト `hidden_window_is_not_painted` を**削除**。根拠は測定結果と独立（恒真テストはどの結果でも実挙動を守れない・独立導出の判断と一致）だが、実行は Phase 3 に置く——(B)/(C) では作業全体が停止し、処分は後続 issue に載るため。**コード内の参照は 0 件**（`super::{MAX_PAINT_RETRIES, retry_delay}` のみ import・plan-review で確認済み）。spec §7-3 の名指し参照は上記 5 の errata が「処分済み」と閉じる——**「参照 0 件」の確認範囲はコードに限る**（spec は errata で閉じるのが正しい形であり、grep 0 件を合格条件にしない）
7. 測定サマリ（プロトコル・件数・判定）を issue #697 へコメント
8. 検証: post-edit hook（*.rs）+ `npm run governance:check`（CLAUDE.md / spec / PERFORMANCE.md の編集・カテゴリ F）

### Phase 4 — 決定 5 の記録（`src-tauri`・項目 1〜3 と独立・コメントのみ）

1. `view.rs:857` の `wake_results` 呼び出し直前に:
   ```rust
   // 決定 5（#673 spec）: この無条件 wake を edge 化してはならない。results は config 系
   // イベントを一切 listen せず（register_config_wake_listeners は wake_main のみ）、
   // visual-only の config 変更では RowsSnapshot が不変ゆえ snapshot 差分 wake も発火しない。
   // results が新しい色・フォント・行高を描く唯一の経路がこの level-triggered wake である。
   ```
2. `mod.rs` `wake_results` 定義 doc に「削ると壊れる理由は `drive_results_window` 側の呼び出し点コメント」の 1 行ポインタ（理由の本文は 1 に一元化・写しを作らない。独立導出 3-2 の採用）

## 不変条件

1. **計器は表示にも制御にも影響しない**: env 未設定時は `var_os` 判定のみ（受信側は可視中毎フレーム呼ばれるため、判定より重い処理をゲート外に置かない）。`window_id_map.get` は非破壊参照で、ゲートの**内側**で呼ぶ
2. **検査対象を変更しながら検査を走らせない**（#489）: Phase 2 の測定は Phase 1 のコミット後の固定コードで行う
3. **測定は「不在の観測」**: hidden 区間に main 帰属の RECV が現れないこと、を数える（presence 検査の罠・`src-tauri/CLAUDE.md` の trace 規範に合致）
4. **陽性対照なしの 0 を証拠にしない**: 区間 V の全計器 >0 が成立して初めて H1/H2 の 0 が意味を持つ
5. **失敗・異常系**: eprintln の失敗はプロセスに影響なし。測定スクリプトが途中で死んでも対象アプリは finally で kill。実 config は同一内容書き戻しのみで値を変えない（ReadFailed になっても watcher のバウンドリトライが拾い、適用側は保全される）
6. **結果 (B)/(C) ではスコープを広げない**: その場の修正はせず、報告 + 別 issue 起票（受け皿は新規作成——既存 issue への未確認送付はしない）

## テスト方針

- 新規ユニットテストなし（計器は trace のみ・コメントは非コード）
- テスト削除の根拠（恒真・実 render() の検査は dev-deps ゼロゆえ不可能・接地は実測 + doc へ移る）をコミットメッセージに記録。「既存テストが証明していた不変条件を孤立させない」——このテストが証明していたのは恒等述語のみで、実不変条件の根拠は本測定が新たに供給する
- post-edit hook: `*.rs` 編集で clippy + 各 crate テスト（沈黙=合格）
- `npm run smoke:egui` 1 回（Phase 1 後・カテゴリ C）+ `npm run governance:check`（Phase 1 と 3 の文書編集後・カテゴリ F）
- 実機検証: Phase 2 の測定自体が起動→show→hide→config 適用の全経路を通る（カテゴリ D 相当の実観測を兼ねる）

## SPEC.md 更新要否

不要。挙動変更なし。`SPEC.md` のフレームスケジューリング節は「非表示中はフレームが走らない」という**挙動**を書いており（機構は書いていない・独立導出の (b) 分類）、測定後も真のまま。機構の記録先は doc コメント / `src-tauri/CLAUDE.md` / spec errata であり Phase 3 が扱う。`workspace/frame-scheduling-design.md` 契約④の「測って接地する」条件が満たされる（契約の CLAUDE.md 転記は設計書 §8 の 5 番・本 issue のスコープ外）

## コミット構成（Phase 分割・#431）

1. `chore: workspace 調査・計画 (issue #697)` — 本ファイル群
2. `chore(egui-runtime): RequestRedraw の送受信計器を追加し、b9a9caf 参照を PR 番号へ言い換える (#697 項目 1・2 前半)` — Phase 1
3. （測定は成果物コミットなし・issue コメントへ）
4. `chore: hidden 抑止の実測結果を 5 箇所へ接地し、トートロジーテストを削る (#697 項目 1・2)` — Phase 3
5. `docs(egui): 決定 5 の「無条件 wake_results を削ると壊れる理由」を呼び出し点に記録する (#697 項目 3)` — Phase 4

## セルフレビュー

### 5a. plan-review の要対処と反映

| 指摘（要対処） | 反映 |
|---|---|
| 受信計器に窓の帰属が無く判定表が誤読しうる（runtime 偵察） | Phase 1-2: RECV 行に `runtime_id`（`window_id_map.get` の結果）を併記。送信側 `window_id` と同じ ID 空間で直接照合可能 |
| テスト削除で spec §7-3 の名指し参照が宙に浮く・「参照 0 件」は現時点で偽（runtime 偵察） | Phase 3-6: 確認範囲をコードに限定し、spec は errata で閉じる形へ変更 |
| `b9a9caf` は全履歴で解決しない・置換文面の重複呼称（runtime 偵察） | Phase 1-3: 「（PR #677）」で置換し「#671 サイクル PR A」との並記を避ける |
| spec §7 残余 2 が書き換え対象に無い（src-tauri 偵察） | 変更ファイル一覧 + Phase 3-5 に errata 追記を追加 |
| `src-tauri/CLAUDE.md:36` の先取り断定にトレーサビリティが付かない（src-tauri 偵察） | Phase 3-4 に括弧内追記を追加 |

独立導出との主要差分の解決: `PERFORMANCE.md` 計器一覧（採用・Phase 1-4）/ `layout.rs` doc（採用・Phase 3-3）/ env 新設（採用・前例整合）/ config 刺激は visual-only キー変更（不採用——無変更書き戻しで同じ emit が得られることを偵察がコードで確認済み・値を触らない側が安全）/ ホットキーは実打鍵必須（不採用——keybd_event 注入は smoke の実績経路。失敗時のみ協働へフォールバック）

### 5b. plan-review が扱わない 3 観点

1. **境界条件**: ①ホットキー注入で show が観測できない（→実打鍵フォールバック・区間 V 不成立なら測定中止）②config 書き戻しが ReadFailed（→watcher のバウンドリトライ・trace に適用イベントが出るまで再試行、出なければ測定不成立）③RECV に main 以外の帰属（results 窓・未知窓）が混ざる（→`runtime_id` で分離して数える）④H1 でキャレット deadline が出ない（focus 状態の想定外れ・→H2 の config 刺激が独立の主刺激なので判定は可能、H1 は補強材料と位置づけ）
2. **シンプル化**: 新規状態・フラグ・抽象は一切導入しない（計器 2 行 + コメント + errata + テスト削除）。共通 `Deadline` primitive は設計書決定 2 で不採用確定済み。測定スクリプトもリポジトリに残さない
3. **破壊不変条件 + 検知手段**: ①「計器がフレーム経路の挙動を変えない」→ ゲート外は `var_os` のみ・非破壊参照のみ。検知: post-edit hook の clippy/tests + smoke:egui + 区間 V のクロスチェック（RECV = REPAINT 件数）②「smoke の trace 解析を壊さない」→ パーサは `[trace]` prefix 限定と偵察が実測確認。検知: Phase 1 後の smoke:egui ③「実 config を壊さない」→ 同一内容 atomic 書き戻しのみ・ReadFailed 時は適用側の保全（#348）が効く。検知: 測定後に config.toml の内容一致を diff で確認
