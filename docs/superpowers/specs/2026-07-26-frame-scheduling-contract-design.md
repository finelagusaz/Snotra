# snotra-egui-runtime フレームスケジューリング契約（設計・2026-07-26 合意）

対象 issue: #737（フレームレート上限なし・448fps）/ #711（blur 猶予の 1 回きり予約）/ #714（再検索時のスクロールアニメーション）/ #697 項目 1（hidden 中の paint 抑止機構が未測定）。

**進捗**: §8 の全 5 段が 2026-07-26 完了——1 番（#697・実測 (A) 確定）/ 2 番（#714・PR #742）/ 3 番（#737・契約②の実装 + A/B 実測）/ 4 番（#711・契約③の実装）/ **5 番（規範の CLAUDE.md への移設。§3 の冒頭を参照——本書はこれ以降、導出の歴史記録である）**。

**残る追跡**: なし。**#745（`unfocus_at` が契約④の backstop の外にいる）は解消済み**——blur 猶予を `BlurGrace` 状態機械へ畳み、reset-on-show でクリアするようにした（残る受容残余は「その呼び出しの消失を捕まえる検査が無い」ことで、機械化は #930）。**#746（`blur_should_hide` の `!settings_running` が SPEC に無い逸脱）も解消済み**——項を撤去し、§5 errata が記す「`Idle` の根拠の唯一の例外」も同時に消えた（現在 `blur_grace_action` の時計非依存な入力は `focused` / `auto_hide` の 2 つで、どちらも wake の持ち主がいる）。

これらは独立の欠陥ではなく、「**再描画を誰が・いつ要求し、どこで打ち切り、来なかったらどうするのか**」という runtime の設計契約が未文書のまま、呼び出し点ごとに場当たりで決まっていることの断片である。本書はその契約を 5 か条に固定し、各 issue をその実装として位置づける。

## 1. 現状の棚卸し — フレームが生まれる全経路

`render()`（`runtime.rs`）は `RedrawRequested` でのみ走る。`RedrawRequested` の発生源は 2 系統:

**系統 A: `RepaintScheduler` worker 経由（合流点あり）**

worker（`repaint.rs`）は窓ごとに 1 スレッドで、`pending: Option<Instant>` に「最も早い deadline」だけを保持し、期限到来で `RequestRedraw` を 1 発送る。ここへ流れ込む要求は:

| 要求元 | 経路 | 遅延 |
|---|---|---|
| egui 内部（ポインタ移動・キャレット点滅・アニメーション） | `wants_repaint_after` → repaint callback | ZERO〜点滅周期 |
| view の `ctx.request_repaint()` / `request_repaint_after()` | 同上 | ZERO / 任意 |
| paint 失敗リトライ（`runtime.rs` の指数バックオフ） | 同上 | 16ms〜 |
| 入力イベント（`on_window_event` が true を返す） | plugin が直接 `scheduler.request(ZERO)` | ZERO |
| 外部 wake（`wake_main` / `wake_results`・worker スレッド） | `WindowWaker::wake()` = `request(ZERO)` | ZERO |
| 活性化直後の初回フレーム | `scheduler.request(ZERO)` | ZERO |

**系統 B: OS 由来（合流点を通らない）**

expose・リサイズ等で tao が直接 `RedrawRequested` を配送する。頻度は低く、描き直しの必要が実在するため、制御の対象にしない。

- **errata（2026-07-26・#737 独立導出）**: 系統 B には第 3 の経路がある——IME preedit 更新時の `InvalidateRect`（`windows_ime.rs`）→ `WM_PAINT` → OS 由来 `RedrawRequested`。IME 打鍵レートに有界で、上限（契約②）の対象外として受容する。

**抑止・終端経路（フレームが生まれない側）**

上の 2 系統は「**renderer 初期化済み・非破棄・イベントループ稼働中**の窓」を前提にしている。前提が崩れる経路は次のとおりで、いずれも**要求が消えるのではなく、要求しても永久に何も起きない**（errata 2026-07-26・Codex 敵対的レビュー）:

| 経路 | 何が起きるか |
|---|---|
| 活性化時の softbuffer surface 初期化失敗（`runtime.rs` の `attach_pending_windows`） | その窓は `active` へ入らず scheduler も初回要求も作られない。**`attach()` が既に返した `WindowWaker` は恒久的に no-op** になる（受信側が落ちるため） |
| `Destroyed`（`runtime.rs` の `WindowEvent` arm） | `active` から除去。以後の wake は no-op |
| proxy 切断（`send_event` が `Err`） | worker が `break` して停止 |
| hidden（契約④） | 要求は proxy まで届くが tao/OS 層が配送しない（#697 実測） |

**最初の 1 つは「初期化に失敗した窓は、生きた handle を持ったまま二度と描かない」**という形であり、呼び出し側からは wake が効かないことと区別できない。今日の製品は窓 2 つとも起動時に活性化するため実害の報告は無いが、**§1 の表だけを読んで「wake すれば描かれる」と結論してはならない**。

**現状の帰結**: 系統 A に流量制御が無い。egui はポインタ移動中 `Some(Duration::ZERO)` を返し続け（egui 0.35 `input_state/mod.rs:652`・設計として正しい）、worker は言われるままに配送する → 448fps / 1 コア 84.7%（#737 実測）。

## 2. 消費側の時限処理 — 「毎フレーム再要求」へ統一済み（#711 以前は blur だけが非対称）

| 箇所 | 流儀 | 脆さ |
|---|---|---|
| 検索 debounce（`view.rs` の `is_armed` 節） | **armed の間、毎フレーム残余を再要求** | なし（coalescing 耐性あり、とコメントで明記済み） |
| 一時通知期限（`view.rs` の `notice.poll` / `notice.remaining`） | poll + 表示中は残余を再要求 | なし（debounce と同型） |
| blur 猶予（`view.rs` の `unfocus_at` → `lifecycle::blur_grace_action`） | **armed の間、毎フレーム残余を再要求**（`BlurAction::Rearm`・#711 で是正） | なし（#711 以前は「1 回きりの予約」で、割り込みで予約が消えると hide が次の無関係な入力まで宙吊りだった） |
| 起動タイムアウト（`drain_launch` → `LAUNCH_TIMEOUT - elapsed`） | launching 中に毎フレーム残余を再要求 | なし |

**4 箇所すべてが「毎フレーム再要求」で揃った**（#711 で blur が合流）。契約③はこの多数派の流儀を昇格させたものであり、新しい機構は発明していない。

**armed 期限はこの 4 つで全数である**（`src-tauri/src/egui_shell/` で「`Instant` を `self.` に持ち期限を判定するもの」を走査・#711 の `/symmetric-check`）。他の `Instant` は計測トレース用で期限を持たない。

## 3. 契約（5 か条）

> **規範の正本はこの文書ではない**（2026-07-26・§8 の 5 番完了）。本書は日付付きの設計書＝**導出の歴史記録**であり、生きている規範は次の 2 箇所にある。食い違ったら向こうが正しい:
>
> - **配送側の保証**（②・③の機構・抑止経路）: `snotra-egui-runtime/CLAUDE.md`「不変条件」
> - **消費側の規範**（①・③の再要求・④）: `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」
>
> **転記は「5 か条を貼る」形にしなかった。** ①と④は既に `src-tauri/CLAUDE.md` にあり（#697 の実測も反映済み）、②は runtime のモジュール構成にあった——欠けていたのは③だけである。写しを増やさず、欠けた分だけを所在へ足した（`AGENTS.md`「文書に事実の写しを増やす変更 → 正本を 1 か所に定め他は参照へ」）。**⑤は規範に置いていない**——`results_view.rs` の `RowScroll` enum と網羅 match が構造で強制しており、規範に写すと機構で吸収済みのものを二重に課すことになる（#593 の階梯）。

### ① フレームは要求から生まれる（イベント駆動）

runtime は勝手にフレームを回さない。フレームを必要とする状態変化を起こした側が、必ず要求を出す（既存規範の再掲。`ctx.request_repaint()` / `WindowWaker::wake()`）。

### ② 配送には下限間隔がある（フレーム上限・#737）

worker は dispatch の間隔に `min_interval` の下限を守る。dispatch 時刻は `max(最も早い deadline, 前回 dispatch + min_interval)`。

- `min_interval` = 窓が載っているモニターのリフレッシュレートの逆数。取得失敗時は 1/60 秒
- **入力起因のフレームにも一律に効く**。追加される応答遅延の上限は `min_interval`（144Hz なら ≤ 7ms）であり、体感を損なわない
- 系統 B（OS 由来）は対象外。paint 失敗リトライは既に 16ms 以上で、実質影響なし
- **`min_interval` の変更は次の dispatch から完全反映される**。gate は dispatch 時点の値で「前回 dispatch + `min_interval`」を固定するため、**リフレッシュレートが下がった直後の 1 回だけ旧値（短い側）の下限で配送されうる**（144Hz → 60Hz のモニター跨ぎ等）。次の dispatch で自己回復するため是正しない（errata 2026-07-26・Codex 敵対的レビュー。「下限を常に守る」と読めた初版への限定）

### ③ 予約は「フレームが来ること」を約束しない

`request_repaint_after(d)` が約束するのは**要求を 1 つ登録すること**だけであり、**d 経過後にフレームが来ることではない**。worker は最も早い deadline だけを単一スロット `pending` で保持し（`repaint.rs` の coalescing）、dispatch 時に `pending.take()` で**予約全体を空にする**。ゆえに、より早い要求（入力・外部 wake・アニメーション）が 1 つでも割り込むと、**両者は 1 回の dispatch へ畳まれ、後の deadline は黙って消える**。

具体例: t=0 に 100ms 後を予約し、t=10ms に `config-applied` の `wake_main`（ZERO 遅延・可視性を見ない）が入ると、10ms のフレームで予約が消費され、**100ms 時点のフレームは来ない**。

**ゆえに、条件待ち（armed）の消費側は、条件が成立するか解除されるまで毎フレーム残余を再要求する。** 検索 debounce・通知期限・起動タイムアウトは既にこの形。blur 猶予をこれへ揃える（#711）。

**再要求してよいのは「時間経過で解消する不成立」だけである**（`grace_elapsed == false` 等）。時計と無関係な入力（フォーカス・設定値・他プロセスの生死）による不成立で再要求すると `request_repaint_after(ZERO)` の永久スピンになり、②で潰した消費を別の扉から再導入する。**それらの変化は、変えた側が wake する責務を負う**（契約①）——負っていない経路は契約①の違反であって、③で埋め合わせてはならない。

- **errata（2026-07-26・#711 の plan-review + Codex 敵対的レビュー）**: 本条項の初版は「d 経過後にフレームが**少なくとも 1 枚**来ることだけを約束する」「deadline は coalescing で**早着しうる**」と書いていた。**前者は偽**（上記のとおり予約は消えうる）、後者は機構の取り違え（早着ではなく消失）。**規範（毎フレーム再要求）は変わらないが、その根拠はまるごと差し替わっている**——初版の根拠だけを読んだ実装者は「早く来るだけなら 1 回の予約でも足りる」と誤読しうる。

### ④ hidden 中のフレームは約束されない

hide を跨ぐ時限状態は reset-on-show を backstop にする（既存規範の再掲・`src-tauri/CLAUDE.md`）。「hidden 中は `update()` が走らない」の抑止機構は **#697 で実測済み**——worker は `RequestRedraw` を送るが hidden な窓には `RedrawRequested` が配送されない（抑止は tao/OS 層。`runtime.rs` の `visible` ガードは到達不能のまま受け口として残る）。§6 は実施済みの測定手順の記録として残す。

**backstop の既知の穴（当時）**: `unfocus_at` / `was_focused`（blur 猶予）は reset-on-show でクリアされておらず、この条項の対象でありながら backstop の外にいた。**#745 で解消済み**——2 フィールドを `BlurGrace` 状態機械へ畳み、`consume_reset_pending` が `reset()` を呼ぶ。

### ⑤ 連続アニメーションの消費には②が天井を与える。不連続遷移にアニメーション経路を使わない

egui のアニメーション（scroll・フェード）は ZERO 遅延要求を出し続けるため、②の上限がその消費の天井になる。**収束そのものは egui 側の性質であって本契約は保証しない**——#710 の実測では観測した 482 バーストすべてが終端した（最長 1.97 秒）が、これは実測であり全称の保証ではない（errata 2026-07-26・Codex 敵対的レビュー。初版の見出しは「有限時間に収束する」と保証の形で書いていた）。一方、**内容が総入れ替えになる不連続遷移**（再検索による結果集合の世代交代）に「現在位置から寄せる」アニメーション経路を流用しない——別のリストへの切り替えに、前のリストの位置は持ち越さない（#714。選択移動 ↑↓ のアニメーションは維持）。

## 4. 設計: フレーム上限（契約②）の実装

**実装位置は worker ループ（`repaint.rs`）一択とする。** 理由:

- 全要求（系統 A）の唯一の合流点であり、呼び出し側 8 箇所に一切触れない
- `render()` 側で間引く案は、イベントループの起床コスト（proxy send → `RedrawRequested` 配送）を払った後に捨てることになり、無駄が残る

### 機構

```rust
// worker ループ（概念スケッチ）
let mut next_allowed: Instant = Instant::now();      // 前回 dispatch + min_interval
// SchedulerMessage::Request { deadline } 受信時:
//   pending = min(pending, deadline) は現行どおり（最も早い希望を保持）
// 期限到来時:
//   dispatch_at = max(pending, next_allowed);        // ← 追加はこの 1 行が本体
//   now < dispatch_at なら pending = dispatch_at として待ち直す
//   dispatch したら next_allowed = now + min_interval
```

現行の coalescing（早い方を採る）と直交し、既存テスト（`wake_before_activation_is_queued` 等）の契約を壊さない。

- **errata（2026-07-26・#737 実装）**: 上のスケッチの「`pending = dispatch_at` として待ち直す」は採らなかった——後着の早い Request が pending を引き戻すたび早発 wake が生じる（plan-review の独立導出）。実装は `pending` の意味論を変えず、**待ち時間計算（`recv_timeout` の目標時刻）に max を閉じた**。理由の恒久記録は `repaint.rs` の worker コメント。

### `min_interval` の供給

- `RepaintScheduler` に `Arc<AtomicU64>`（interval ナノ秒）を持たせ、worker は dispatch のたびに読む
- 値の更新は plugin（`runtime.rs`）が行う: 活性化時に 1 回 + `Moved` / `ScaleFactorChanged` 受信時に再取得（モニター跨ぎ追従）+ `Focused(true)` 受信時に**全窓**再取得（静止中の OS 設定変更・抜き差しの backstop。results は focusable(false) で Focused を受けないため全窓に適用する——実装時の /symmetric-check で追加）
- 取得は窓 HWND → `MonitorFromWindow` → `EnumDisplaySettingsW(ENUM_CURRENT_SETTINGS)` の `dmDisplayFrequency`。crate は既に IMM32（`windows_ime.rs`）で Win32 依存を持つため、依存の性格は変わらない
- 失敗時（0 / 1 が返る・API 失敗）は 60Hz へフォールバック。VRR パネルは最大値が返るが、上限の趣旨（表示されないフレームを描かない）に反しない

### 採らない案とその理由

- **config キー（#737 案 2）**: 4 点セット（default / settings UI / watcher / 後方互換）のコストに対し、「ユーザーが fps を判断したい」という要求が実在しない。必要が立ってから足せる形（AtomicU64 の供給元を増やすだけ）にはなっている
- **固定 60fps（#737 案 3）**: 144Hz 実機（実測環境そのもの)で体感を落とす。モニター値取得は前例（`monitor.rs`）があり、コスト差が小さい

### 期待値の見積もり

448fps → 144fps で描画コストは比例減（84.7% × 144/448 ≈ 27%）。1.64ms/frame の paint コスト自体は不変なので、**上限だけでは「移動中に重い」の根治ではない**——それでも 6 割減であり、#737 の受け入れ条件（有意に下がる）は満たせる見込み。残余は #714 の修正（無駄なアニメーションフレームの除去）と合算で評価する。

## 5. 設計: blur 猶予の再要求（契約③・#711 案 A）

`view.rs` の `unfocus_at` 節を debounce と同型へ:

**出荷形**（#711 **当時**。初版スケッチとの差は下の errata。第 4 引数 `settings_running` は #746 で撤去済みで、現行は 3 引数である）:

```rust
// lifecycle.rs（純粋核・elapsed は呼び出し側が 1 回だけ読んで渡す）
fn blur_grace_action(elapsed, focused, auto_hide, settings_running) -> BlurAction {
    if blur_should_hide(focused, elapsed >= BLUR_GRACE, auto_hide, settings_running) {
        BlurAction::Hide
    } else if elapsed < BLUR_GRACE {
        BlurAction::Rearm(BLUR_GRACE - elapsed)   // 減算はこの分岐内だけ（underflow 不能）
    } else {
        BlurAction::Idle                          // 時間では解消しない → 再要求しない
    }
}

// view.rs
if let Some(at) = self.unfocus_at {
    match blur_grace_action(at.elapsed(), focused, self.auto_hide_enabled(), self.settings_running()) {
        BlurAction::Hide => { self.unfocus_at = None; self.emit_hide(); }
        // 契約③: 予約はフレームの到来を約束しない（単一スロット + take() で消えうる）
        BlurAction::Rearm(remaining) => ctx.request_repaint_after(remaining),
        BlurAction::Idle => {}
    }
}
```

- 猶予中にフレームが来るたび残余で予約し直すため、1 枚の早着・消失で宙吊りにならない
- `was_focused && !focused` エッジでの初回予約は残す（armed 化した直後のフレームを確実に起こす）
- 純粋核 `blur_should_hide` は不変。追試は「grace_elapsed=false のフレームで再要求が出ること」を view 層でどう固定するかが実装時の論点（view は実機依存ゆえ、猶予残余の算出を純関数へ切り出してテストする形が既存の型に合う）

#711 案 B（状態遷移化）・案 C（コメントで受容）は、案 A が 3 行で済み多数派の流儀と揃う以上、複雑さ・非対称の残存に見合わない。

- **errata（2026-07-26・#711 の plan-review 独立導出）**: 上のスケッチは `at.elapsed()` を **3 回**読んでいる（判定・`blur_should_hide` の引数・減算）。判定（`>= grace`）と減算（`grace - at.elapsed()`）の間に時計が進むと **`Duration` 減算が underflow して panic** し、release は `panic = "abort"`（ルート `Cargo.toml`）ゆえ**プロセスが落ちる**。**このフレームは猶予境界に着弾するよう予約されているため、`elapsed ≈ grace` はまさに典型のケースであり確率は低くない。** 実装は `elapsed` を**呼び出し側で 1 回だけ読んで純関数へ渡し**、減算を `elapsed < grace` の分岐内に閉じること（この「1 回読み」は load-bearing である）。
- **errata（同・Codex 敵対的レビューが指摘）**: スケッチの `else if !grace_elapsed` は、猶予明けで条件不成立（`auto_hide` off・設定サイドカー起動中）のとき何もしない形だが、**その判断の正しさは「それらの変化を起こす側が wake する」ことに依存している**（契約③の追記を参照）。**設定サイドカーの終了はこの責務を負っていない**——監視スレッド（`commands/window.rs`）は `settings_running` を false へ倒すのに wake しない。ゆえに `Idle` の根拠には**当時** 1 つ例外があった。**#746 で解消済み**（`!settings_running` の項自体が SPEC に無い逸脱であり、項ごと撤去した）。#711 はこの例外を残したまま契約③の本体だけを実装した。

## 6. 設計: hidden 抑止の接地（契約④・#697 項目 1）

契約④は現在**推測の上に立っている**。採択前に 1 回だけ測る:

1. `render()` 冒頭（`visible` ガードより前）に `SNOTRA_TRACE` ゲートの trace を 1 行足す
2. 実機で hide 中に `wake_main` を発火させ（config 変更で `config-applied` が飛ぶ）、`render()` に到達するか / どこで止まるかを読む
3. 結果で分岐:
   - **OS/tao 層で止まる**（到達しない）→ `wake_main` doc と `runtime.rs` の注記から「推測」の限定を外し、実測日付で確定。トートロジーテスト `hidden_window_is_not_painted` は削除（#697 項目 2 の「削る」側で確定）
   - **`render()` に到達する** → hidden 中も update が走っている＝既存規範の前提が崩れているので、`visible` ガードを実効化する設計へ進む（`RuntimeFrame` に hide を返す or 外部可視状態の注入。これは別 issue に切る）

## 7. #714 の位置づけ（契約⑤の view 側実装）

runtime には触れない。`results_view.rs` の世代検知（`last_scrolled_selected` リセットと同じ箇所）の真偽で、世代交代フレームの選択行だけ `scroll_to_me_animation(None, ScrollAnimation::none())`（per-call のアニメーション無効化・egui 0.35 で `scroll_to_me` の糖衣元）を使い、瞬時に選択行を可視化する。↑↓ ナビ（世代不変）の `scroll_to_me` は不変。

**errata（2026-07-26・#714 plan-review）**: 本節の初版は「`ScrollArea::vertical_scroll_offset(0.0)` を明示指定」だったが、実装前レビューで却下した——(1) `on_escape` の folder/tool 復帰と index 再構築後の再検索は selected≠0 のまま世代を進めるため、先頭 0 固定は選択行を見失わせる (2) バックグラウンド reindex の再検索は内容同一でも世代を加算し（`set_results` は比較せず無条件）、閲覧中の無操作先頭スナップを作る (3) builder offset は in-flight の `offset_target` を消さない。正しい目標は「先頭」ではなく「選択行が瞬時に見えていること」であり、契約⑤の意図はこの形でも満たされる。

## 8. 実施順序（測定干渉を踏まえる）

#737 の受け入れ条件は「同一プロトコルの実測で改善を示す」であり、#714 のアニメーションは操作中 fps の一部を占める。帰属を混ぜないため:

| 順 | 作業 | 理由 |
|---|---|---|
| 1 | §6 の測定（#697 項目 1） | 契約④の接地。他と独立・実機 1 回 |
| 2 | #714 修正 + `measurement.md` プロトコルで再測定 | 無駄フレーム源を先に除去し、上限の効果測定の基線を作る |
| 3 | #737 フレーム上限 + 同プロトコルで実測 | 基線に対する上限の寄与が単独で読める |
| 4 | #711 blur 再要求 | 挙動不変（潜在の頑健化）ゆえ測定と独立。いつでもよいが、契約③の文書化と同 PR が自然 |
| 5 | 契約 5 か条を `snotra-egui-runtime/CLAUDE.md` へ転記・`src-tauri/CLAUDE.md` から参照 | 実装がすべて契約に一致した状態で文書化 |

各段は独立に green へ持ち込める（#431 の Phase 分割）。2 と 3 は `snotra-egui-runtime` / `src-tauri` でファイル境界も分かれる。

## 9. 決定事項（2026-07-26 合意）

1. **上限値の既定 = モニター値 + 60Hz フォールバック・config キーなし**（§4 のとおり）。固定 60fps 案・config 案は採らない。config が必要になった場合も `AtomicU64` の供給元を増やすだけで載る形は §4 が確保している
2. **契約③は規範に留める**。blur 猶予を多数派の流儀（毎フレーム残余再要求）へ揃えるのみとし、共通 `Deadline` primitive の抽出は行わない。`docs/development-principles.md`「強制の階梯」に照らし、同型の 4 例目が出た時点で再検討する

## 10. 受け入れ条件（本設計全体）

1. 契約 5 か条が `snotra-egui-runtime/CLAUDE.md` に載り、既存コードがすべて一致している（blur 含む）
2. ポインタ移動中 fps ≤ モニターリフレッシュレート、1 コア占有の有意減（#737 AC1・`measurement.md` プロトコル）
3. 打鍵・ホバー応答の体感悪化なし（#737 AC2・`egui_search:dispatch` trace + 目視）
4. 再検索時に結果窓が瞬時に先頭表示（#714）
5. hidden 抑止の機構が実測で接地され、doc の「推測」限定が解消（#697 項目 1）— **達成済み**（2026-07-26・contract ④ を実測結果へ改稿）
