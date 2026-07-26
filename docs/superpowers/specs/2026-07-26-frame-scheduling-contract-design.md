# snotra-egui-runtime フレームスケジューリング契約（設計・2026-07-26 合意）

対象 issue: #737（フレームレート上限なし・448fps）/ #711（blur 猶予の 1 回きり予約）/ #714（再検索時のスクロールアニメーション）/ #697 項目 1（hidden 中の paint 抑止機構が未測定）。

**進捗**: §8 の 1 番（#697・実測 (A) 確定）・2 番（#714・PR #742）・3 番（#737・契約②の実装 + A/B 実測で上限を確認）は 2026-07-26 完了。残りは 4〜5 番（#711 → 契約 5 か条の CLAUDE.md 転記）。

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

**現状の帰結**: 系統 A に流量制御が無い。egui はポインタ移動中 `Some(Duration::ZERO)` を返し続け（egui 0.35 `input_state/mod.rs:652`・設計として正しい）、worker は言われるままに配送する → 448fps / 1 コア 84.7%（#737 実測）。

## 2. 消費側の時限処理 — 現在 3 つの流儀が混在

| 箇所 | 流儀 | 脆さ |
|---|---|---|
| 検索 debounce（`view.rs` の `is_armed` 節） | **armed の間、毎フレーム残余を再要求** | なし（coalescing 耐性あり、とコメントで明記済み） |
| 一時通知期限（`view.rs` の `notice.poll` / `notice.remaining`） | poll + 表示中は残余を再要求 | なし（debounce と同型） |
| blur 猶予 100ms（`view.rs` の `unfocus_at`） | **1 回きりの予約**（`request_repaint_after(100ms)` を張るだけ） | 予約フレームが早着・消失すると hide が次の無関係な入力まで宙吊り（#711） |
| 起動タイムアウト（`drain_launch` → `LAUNCH_TIMEOUT - elapsed`） | launching 中に毎フレーム残余を再要求 | なし |

4 箇所中 3 箇所が既に「毎フレーム再要求」であり、blur 猶予だけが対称性を欠く。**新しい機構を発明する必要はなく、既にある多数派の流儀を契約へ昇格させれば足りる。**

## 3. 契約（5 か条）

採択されたら `snotra-egui-runtime/CLAUDE.md` の不変条件節へ転記し、`src-tauri/CLAUDE.md` の「イベント駆動 wake の不変条件」から参照する。

### ① フレームは要求から生まれる（イベント駆動）

runtime は勝手にフレームを回さない。フレームを必要とする状態変化を起こした側が、必ず要求を出す（既存規範の再掲。`ctx.request_repaint()` / `WindowWaker::wake()`）。

### ② 配送には下限間隔がある（フレーム上限・#737）

worker は dispatch の間隔に `min_interval` の下限を守る。dispatch 時刻は `max(最も早い deadline, 前回 dispatch + min_interval)`。

- `min_interval` = 窓が載っているモニターのリフレッシュレートの逆数。取得失敗時は 1/60 秒
- **入力起因のフレームにも一律に効く**。追加される応答遅延の上限は `min_interval`（144Hz なら ≤ 7ms）であり、体感を損なわない
- 系統 B（OS 由来）は対象外。paint 失敗リトライは既に 16ms 以上で、実質影響なし

### ③ 予約は「フレーム 1 枚以上」を約束し、「条件成立」を約束しない

`request_repaint_after(d)` は d 経過後にフレームが**少なくとも 1 枚**来ることだけを約束する（可視中に限る・④）。そのフレームで条件（猶予経過・debounce 満了）が成立している保証は無い——deadline は coalescing で早着しうるし、フレームは別の理由でも来る。

**ゆえに、条件待ち（armed）の消費側は、条件が成立するか解除されるまで毎フレーム残余を再要求する。** 検索 debounce・通知期限・起動タイムアウトは既にこの形。blur 猶予をこれへ揃える（#711 案 A）。

### ④ hidden 中のフレームは約束されない

hide を跨ぐ時限状態は reset-on-show を backstop にする（既存規範の再掲・`src-tauri/CLAUDE.md`）。ただし現在「hidden 中は `update()` が走らない」の抑止機構は**推測のまま**であり（`runtime.rs` の `visible` ガードは到達不能・OS/tao 層の抑止と推定・#697 項目 1）、本契約の採択前に測って接地する（§6）。

### ⑤ 連続アニメーションは上限の下で有限時間に収束する。不連続遷移にアニメーション経路を使わない

egui のアニメーション（scroll・フェード）は ZERO 遅延要求を出し続けるため、②の上限がその消費の天井になる。一方、**内容が総入れ替えになる不連続遷移**（再検索による結果集合の世代交代）に「現在位置から寄せる」アニメーション経路を流用しない——別のリストへの切り替えに、前のリストの位置は持ち越さない（#714。選択移動 ↑↓ のアニメーションは維持）。

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
- 値の更新は plugin（`runtime.rs`）が行う: 活性化時に 1 回 + `Moved` / `ScaleFactorChanged` 受信時に再取得（モニター跨ぎ追従）
- 取得は窓 HWND → `MonitorFromWindow` → `EnumDisplaySettingsW(ENUM_CURRENT_SETTINGS)` の `dmDisplayFrequency`。crate は既に IMM32（`windows_ime.rs`）で Win32 依存を持つため、依存の性格は変わらない
- 失敗時（0 / 1 が返る・API 失敗）は 60Hz へフォールバック。VRR パネルは最大値が返るが、上限の趣旨（表示されないフレームを描かない）に反しない

### 採らない案とその理由

- **config キー（#737 案 2）**: 4 点セット（default / settings UI / watcher / 後方互換）のコストに対し、「ユーザーが fps を判断したい」という要求が実在しない。必要が立ってから足せる形（AtomicU64 の供給元を増やすだけ）にはなっている
- **固定 60fps（#737 案 3）**: 144Hz 実機（実測環境そのもの)で体感を落とす。モニター値取得は前例（`monitor.rs`）があり、コスト差が小さい

### 期待値の見積もり

448fps → 144fps で描画コストは比例減（84.7% × 144/448 ≈ 27%）。1.64ms/frame の paint コスト自体は不変なので、**上限だけでは「移動中に重い」の根治ではない**——それでも 6 割減であり、#737 の受け入れ条件（有意に下がる）は満たせる見込み。残余は #714 の修正（無駄なアニメーションフレームの除去）と合算で評価する。

## 5. 設計: blur 猶予の再要求（契約③・#711 案 A）

`view.rs` の `unfocus_at` 節を debounce と同型へ:

```rust
if let Some(at) = self.unfocus_at {
    let grace = Duration::from_millis(100);
    let grace_elapsed = at.elapsed() >= grace;
    if blur_should_hide(focused, grace_elapsed, ...) {
        self.unfocus_at = None;
        self.emit_hide();
    } else if !grace_elapsed {
        // 契約③: armed の間は毎フレーム残余を再要求（coalescing・早着への耐性）
        ctx.request_repaint_after(grace - at.elapsed());
    }
}
```

- 猶予中にフレームが来るたび残余で予約し直すため、1 枚の早着・消失で宙吊りにならない
- `was_focused && !focused` エッジでの初回予約は残す（armed 化した直後のフレームを確実に起こす）
- 純粋核 `blur_should_hide` は不変。追試は「grace_elapsed=false のフレームで再要求が出ること」を view 層でどう固定するかが実装時の論点（view は実機依存ゆえ、猶予残余の算出を純関数へ切り出してテストする形が既存の型に合う）

#711 案 B（状態遷移化）・案 C（コメントで受容）は、案 A が 3 行で済み多数派の流儀と揃う以上、複雑さ・非対称の残存に見合わない。

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
5. hidden 抑止の機構が実測で接地され、doc の「推測」限定が解消（#697 項目 1）
