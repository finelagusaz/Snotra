# SU5 — updater + 通知 primitive + 起動 async 化（#532 Phase 2）設計

- 種別: 設計 spec（brainstorm 成果物。次工程は plan）
- 日付: 2026-07-24
- 親: #532（Phase 2 ロードマップ `2026-07-21-phase2-softbuffer-migration-roadmap.md` SU5 行）・#631
- レビュー: 3 レンズ subagent（並行性 / WebView2 parity / 状態機械）+ codex 敵対探索を実施済み。発見は本文に反映（→「レビューで確定した否定の知識」）

## スコープ

コア 4 点。IME 再変換（WANT・独自 IMM32 実装が必要）は **defer**（切替をブロックしない・SU6 以降か flip 後に独立検討）。

1. 汎用通知 primitive（一時 overlay + 持続 toast の二描画面・状態モデル共通）
2. updater（check / download / install・3 モード・exit 合流）
3. #631 起動 async 化 + single-flight + 失敗通知（`activate` / `execute_tool_selected` / `execute_instant_selected`）
4. flush-on-Enter 乖離の解消（trailing 窓内 Enter の stale 起動）

付随: egui 経路の最小 i18n テーブル（SU5 文言が必要とするため。→ D 節）。

## 決定事項（brainstorm + レビューで確定）

1. **通知 primitive は「状態モデル共通 + 二描画面」**。一時系＝検索バー overlay（高さ不変・自動クリア）、持続系＝バー直下 52px toast 行（ボタン付き・高さ加算）。WebView2 の外観 parity（flip 基準 2）を守る
2. **起動 async 化の担体は per-launch 専用スレッド + フレーム drain**。`spawn_blocking` は dead UNC のプール飽和ゆえ不採用（folder 展開 #636 と同じ論拠）。常駐 launcher スレッド 1 本はワーカー死亡で全滞留ゆえ不採用
3. **launching ガードは打鍵のみ**（`handleInput` parity）。Escape / blur / Alt+Q の手動 hide は launching 中も通す。「完了待ち」なのは成功時の**自動 hide だけ**（初版ドラフトの「hide は起動完了待ち・入力ガードで抑止」は WebView2 実コードで反証された——`handleKeyDown` に launching ガードは無い）
4. **blur 自動非表示は launching 中も通す**（parity）。起動対象が modal で focus を奪い blur-hide → 失敗通知が hidden 中に期限切れで見えない穴は、WebView2 も同型（resetForShow が通知を消す）ゆえ**既知の限界として受容**
5. **履歴記録（`record_and_save`）は worker スレッド側**で成功時に行う（WebView2 が backend 側で UI 可視性と無関係に記録するのと parity）。hide 中に完了した起動の記録消失 gap を閉じる。instant は記録しない（現行 parity）
6. **遅着結果の無害化は per-launch channel で行う（世代 token 不要）**。`LaunchInFlight` が rx を所有し、`launching = None` で Receiver ごと drop → 遅着 send は Err で消える
7. **launch 突入時に results をクリアする**（`withLaunchLifecycle` の await 前 `clearResults()` parity）。クエリは保持。launching 中は 52px collapse・↑↓/クリックは空リストゆえ自然に inert。失敗時は同期 `run_search` で再取得（`runRefresh` parity）+ 失敗通知
8. **timeout（4 秒）は「失敗」でなく「結果不明」**として通知する。起動という副作用は取り消せない（abandoned が完走する `spawn_blocking` と同じ・parity）。timeout 後の再実行による二重起動可能性も WebView2 と同型の受容済みリスク
9. **updater の保存順序は plan 工程で決める（未決・両案併記）**。→ B 節
10. **egui 文言の言語は config `general.language` を起動時に一回読む**静的解決。hot-reload（`language-changed` 追従）は SU6（config 反映の本丸）へ送る

## A. 通知 primitive（`src-tauri/src/egui_shell/notify.rs` 新設・純粋核)

`layout.rs`（`Debouncer`）と同じ様式: 時刻は driver が注入する純粋核。egui/Win32 非依存・ユニットテスト対象。

### 一時レーン（overlay）

- 単一スロット `TransientNotice { message, duration }`。新規 set は旧通知を上書き（`clearLaunchNotice` → set と同型）
- duration は per-message（WebView2 は 2400ms=起動失敗/timeout・3000ms=indexing 中の設定不可・5000ms=hotkey 失敗の 3 値。SU5 で使うのは 2400 系。5000 系は SU6 のホットキー反映で合流）
- `poll(elapsed)` で期限切れクリア。view は残余時間で `request_repaint_after` を予約（**可視中のみ有効**。→ C 節の hidden 要石）

### 持続レーン（updater toast）

- `UpdaterUiState`（managed `Arc<Mutex<...>>`）から導出: `Checking / UpToDate / Available { version, can_install, update } / Installing / InstallFailed { message }` + `dismissed`
- **`Available` は plugin の `Update` 本体を保持する**（version 文字列だけでは `downloadAndInstall()` を呼べない。WebView2 の `pendingUpdate` 相当）
- **dismissed は managed 側に置く**。view-local に置くと reset-on-show で [閉じる] 済み toast が復活する。dismiss はセッション中恒久（次チェックは次回起動・parity）
- **Installing 中は [今すぐ更新] / [閉じる] とも disabled**（`UpdateToast` parity）。`Available → Installing` 遷移は mutex 内で原子的に行い、install と dismiss の競合を表現不能にする

### 描画

- **overlay 優先順は `indexing > 起動中 > 通知`**（`SearchWindow.tsx` の Switch 先頭一致 parity。初版の「起動中 > 通知 > indexing」は逆で誤り。instant は indexing 中も実行可能ゆえ同時 true が到達可能）
- **overlay は painted label で描く（`hint_text` 不可）**。hint は空クエリ時のみ描画されるが、launching 中は query 非空（起動対象文字列を保持）
- toast 行は検索バー直下・**モード非依存**（folder/tool/instant 中も表示・§20.3）。`HeightParams` の `has_update_toast` / `update_toast_height` は SU4 で先行導入済み（テスト `toast_adds_height` あり）——driver（view.rs の `has_update_toast: false` 固定）を実値配線するだけ
- toast 表示中の show は「52px collapse → view が次フレームで拡張」の 1 フレームスナップを受容（結果展開と同じ既存トレードオフ・as-built で注記）

### reset-on-show（reset_pending 消費ブロック）の追加責務

- **一時通知クリア + `launching = None` を追加**（`resetForShow` の `clearLaunchNotice()` + `setLaunching(false)` parity）
- **持続 toast と dismissed は触らない**（`resetForShow` は `updateInfo` に触れない・parity）

## B. updater

- **check**: setup 時（egui フラグ経路）、`auto_update != disabled` なら tauri async runtime で `UpdaterExt::check()` を一回。結果を `UpdaterUiState` へ書き、**完了時に `ctx.request_repaint()` を呼ぶ**（可視中に完了した場合、次の操作まで toast が現れない穴を塞ぐ。スパイク実装と同じ）。check 失敗は trace のみ（`console.warn` parity）
- **モード gating**: `full` のみ [今すぐ更新]、`check_only` は通知のみ、`disabled` は check しない（`MainApp.tsx` / スパイク parity）。config は起動時一回読み（hot-reload なし・parity）
- **install**: [今すぐ更新] → `Available → Installing`（原子遷移）→（保存ステップ・下記未決）→ `downloadAndInstall`（async）→ 完了後は既存の exit 合流点（`exit-requested` → history/icon flush → exit(0)。`app.restart()` 不使用・§20.4）。失敗は `InstallFailed` 表示 + Installing 解除（`updaterError` parity）
- **スパイクからの転用範囲は check + モード gating のみ**。スパイク（`snotra-egui-mvp`）は install を実装していない（check + download 検証のみ）。install → 保存 → exit の列は**新規・未検証**であり、実装時スモーク項目とする

### 未決: 保存順序（plan 工程で決定）

ロードマップの「保存優先＝`downloadAndInstall` 復帰後に保存を置かない」（#580/#532 申し送り）と、現行コード + SPEC §20.4（保存は `downloadAndInstall` の**後**、quit_app 経由）は食い違う。決定は **tauri-plugin-updater の `downloadAndInstall` / NSIS の終了挙動を一次資料（context7 / plugin ソース）で調べてから**行う。

- **案 1（前に足して後も残す）**: exit listener から**保存専用ルーチンを切り出し**（exit-requested の flush 列は保存 + exit(0) の不可分列であり、そのままでは保存のみに再利用できない——parity レビューで確認済み）、install 前に先行 flush。完了後は従来どおり exit 合流（再 flush）。二重 flush は `NEXT_SAVE_SEQUENCE` の単調ガードで実測安全（最新 seq 勝ち・テスト実証済み）。SPEC とは追加的ハードニングとして両立
- **案 2（現行順のまま）**: `downloadAndInstall` → exit 合流のみ。新規切り出し不要で最小。installer が exit listener 完走前にプロセスを終えるリスクは現状同様に受容

plugin が「`downloadAndInstall` 復帰前にプロセスを終わらせうる」なら案 1 必須、「復帰する（現行 WebView2 コードが `quitApp()` へ到達している）」なら案 2 で足りる。

## C. #631 起動 async 化 + single-flight + flush-on-Enter

### 対象

`activate`（通常 + opener 先頭ツール）・`execute_tool_selected`・`execute_instant_selected`。instant は engine ロック内の action 抽出のみ UI スレッドに残し、clipboard 読み + 変数展開 + 実行を worker へ。

### 機構

- view が `launching: Option<LaunchInFlight> { started_at, rx, 後処理文脈 }` を持つ
- **不変条件 1: channel は per-launch**。`LaunchInFlight` が rx を所有し、破棄（reset / timeout）で遅着結果ごと自然消滅する。folder 展開の「view 寿命の共有 channel + 世代 token」を**コピーしない**（token が要るのは共有 channel だから。per-launch なら不要）
- **不変条件 2: launching-drain は reset_pending 消費の後に置く**（folder/icon drain と同じ位置）。前に置くと show 直後フレームで stale Ok が reset より先に処理され、再 show した窓を emit_hide が撃つ
- worker: 実行 → 成功なら `record_and_save`（worker 側・決定 5）→ send + `ctx.request_repaint()`
- drain: `Ok` → `clear_search` + `emit_hide`。`Failed` → 同期 `run_search` 再取得 + 一時レーンへ失敗通知（hide しない）。**started_at から 4 秒経過** → 「結果不明」通知 + `launching = None`（遅着は rx drop で消滅）
- 突入時: results クリア（決定 7）+ 「起動中…」painted overlay + 打鍵ガード

### single-flight

- dispatch chokepoint（Enter / クリックの起動分岐）で `launching.is_some()` なら拒否。egui は入力をフレーム内で fresh 消費するため、拒否された Enter が後で再生されるキューは無い（実測確認済み）
- ガードの位置は「打鍵 = `handleInput` 相当」と「起動 dispatch」の 2 点のみ。hide 系（Escape/blur/Alt+Q）はガードしない（決定 3・4）

### hidden 中の drain（要石・実装時スモーク）

egui_shell 経路では runtime の visible ガードが効かない構造のため、hidden 中に `update()` が走るかは tao が hidden 窓へ RedrawRequested を配るか一点に懸かる（未実測）。**走らない場合**: timeout は hidden 中観測されず、遅着結果は次 show まで宙吊り → reset-on-show の `launching = None`（A 節）が backstop となりどちらに転んでも安全。「deadline は `request_repaint_after` で確実に観測」が真なのは**可視中のみ**。実装時に「遅い起動 → 即 hide → 再 show」のスモークで決着させる。

### flush-on-Enter

- 述語: `view_kind == Results ∧ interp == Plain ∧ search_debounce.is_armed()`（folder は同期フィルタ・instant/command は cancel 済みゆえ armed にならない——この述語で誤発火が構造的に起きない）
- 動作: Enter dispatch の前に `cancel()` → 同期 `run_search` → 選択を `clamp` → dispatch（`resolveActivationTarget` の `flushPendingRefresh` → `clampSelectedIndex` と同型・parity 確認済み）

## D. 最小 i18n テーブル（egui 経路）

- 現状: egui 経路の文言はハードコード日本語（「検索…」「インデックス構築中...」「ツールを選択…」）で言語切替なし。WebView2 は `i18n.ts`（JP/EN）
- SU5: Rust 側に言語別テーブル（JP/EN・`TranslationKey` 相当の enum か定数群）を新設し、config `general.language` を**起動時に一回**読んで解決（決定 10）。対象は SU5 新規文言（起動失敗 / 結果不明 / 起動中 / updater toast 一式）+ 既存ハードコード 3 ヒントの移行（同じ機構に載せるだけ・安価）
- 文言は WebView2 の `i18n.ts` と同文言（`notice.launch.*` / `update.*` キー対応）
- hot-reload（`language-changed` 追従）は SU6 の config 反映で拡張

## E. テスト・SPEC 同期

### テスト（純粋核・Red→Green）

- `notify.rs`: 期限切れ・上書き・レーン独立・reset 相当の選択的クリア（一時 + launching は消え、toast + dismissed は残る）
- timeout 述語（`started_at` 注入で 4 秒判定）・flush-on-Enter 述語・single-flight 拒否
- updater 遷移: `Available → Installing` の原子性・Installing 中 dismiss 拒否・モード gating（スパイクの `parse_update_mode` テスト転用）
- `HeightParams` toast 加算は既存テスト（`toast_adds_height`）あり → driver 配線の検証を追加

### 実装時スモーク（テストで書けない項目）

1. hidden 中 `update()` 挙動（遅い起動 → 即 hide → 再 show で遅着・timeout の観測列）
2. install 列 end-to-end（ローカル可能範囲。署名付き実更新は #580 核心＝SU7）

### SPEC 同期（仕様変更扱い・Step 0）

- §20: egui 経路の as-built を追記（保存順序は決定後に明文化・§20.3 に toast のモード非依存描画と高さ加算・§20.4 との関係）
- §19.6: 起動保護（4 秒 timeout）を経路非依存の記述へ整え、egui 経路の per-launch スレッド + drain を as-built で記す。timeout=「結果不明」の意味論を明記
- §8.6: launching / 一時通知 / toast は**状態ノードにしない**。`indexing` と同じ overlay として note で記述（「手動 hide は launching 中も成立し、成功時の自動 hide のみ完了後」）

## レビューで確定した否定の知識（なぜ B 案でないか）

- **世代 token を導入しない**: per-launch channel の rx 所有で遅着は構造的に消える。token は共有 channel の補償機構であり、ここでは複雑さだけが残る
- **`spawn_blocking` を使わない**: dead UNC がプールを飽和させ icon/index を巻き込む（#636 と同根拠）
- **blur-hide を launching でゲートしない**: parity 逸脱の割に、守れるのは「hidden 中に期限切れる失敗通知」だけで、それは WebView2 も同様に失う
- **「exit-requested の flush 列を保存のみに再利用」はできない**: あの列は保存 + exit(0) の不可分列。保存専用が要るなら切り出しが必須（案 1 のコスト）
- **install 前 flush と in-flight 起動の直列化は不要**: 履歴保存は `NEXT_SAVE_SEQUENCE` 単調ガードで最新 seq 勝ちが実測済み（`delayed_older_prepared_save_cannot_overwrite_newer_snapshot`）

## 受容済みの限界（既知・意図的）

- timeout 後の遅着二重起動可能性・起動副作用の取り消し不能（WebView2 abandoned `spawn_blocking` と同型）
- blur-hide 中に期限切れる失敗通知（WebView2 同型・決定 4）
- timeout 連打による abandoned worker スレッドの一時蓄積（`spawn_blocking` 版と同型のリスク・ShellExecuteW は OS レベルで最終的に復帰する）
- toast 表示中 show の 1 フレーム高さスナップ
- launching 中の in-flight 表示差（WebView2=空リスト + 起動中 / egui も同じへ寄せる・決定 7 で解消済み）
