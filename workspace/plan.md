# plan — issue #1194: モーダル移動ループ中のクランプ抑止と、位置を動かす経路の特定

ブランチ: `fix/keyboard-move-clamp-1194` / 分類: **仕様変更**（`SPEC.md` §8.2 の as-built を変える）

調査は `workspace/research.md`、敵対的調査の全文は `workspace/adversarial-1194.txt`。

## 目的

`clamp_main_into_work_area` の抑止条件を「ポインタが押されていない」から「**OS のモーダル移動ループ中でない**」へ差し替え、キーボード移動（`Alt+Space` → `M` → 矢印）がマウスドラッグと同じ保証を得るようにする。同時に、1 度置いた計装で Q2（確定後に窓を 27〜28 px 動かしている経路）と Q3（逸脱の 2 仮説）に答える。

## 受け入れ条件（issue から逐語）

- **Q1**: キーボード移動中はクランプが働かず、確定後の最初のフレームで 1 度だけ戻る（＝マウスドラッグと同じ保証）。マウスドラッグの挙動が退行していないことを対照つきで実測している
- **Q2**: 27〜28 px を動かしている経路を名指しできている（直すかどうかは、名指しできてから決める）
- **Q3**: 逸脱の 2 仮説（「ループ側の `SetWindowPos` と競り合っている」／「フレームが疎で戻りが遅い」）のどちらかを実測で棄却している
- 採らなかった候補と却下理由を ADR へ残している

### ⚠️ Q1 の「1 度だけ戻る」は、述語を直すだけでは閉じないかもしれない

**Q2 の答えが Q1 の受け入れ条件の真偽を決める。** 27〜28 px の動かし手が「確定後の最初のクランプフレーム」**より後**に効くなら、ユーザーは跳ねを 2 度見ることになり、最終位置は「クランプ後の位置 − 27 px」＝作業領域の内側からさらにずれた位置になる。**そのとき保証は「1 度だけ」ではなくなる。**

ゆえに **Phase 1（計装と測定）を Phase 2（実装）より前に置く順序は必須である**——issue が「まず計装を 1 度置いて」と書いているのはこの依存のためだと読む。測定で順序が判明したら、Q1 の実装が追加で何を要するかがそこで決まる。**Q1 を「述語の差し替えだけ」と見積もらない。**（独立導出レビューの最重大所見を採用。機序は `window_coordinator.rs:790-792` の既存 ⚠️ 記述で裏づけた）

**多モニターの扱いは人間裁定（2026-08-27）で確定済み**——単一モニターの代替判別子で進める。移動中の窓 `bottom` が作業領域の下端（1020）で止まるか、カーソル限界（1133〜1134）まで出られるか、を判別子とする。多モニターの封鎖は演繹のまま残し、`SPEC.md` の当該記述も「見込まれる」の強さに保つ。

## 設計（未確定を潰した結果・根拠は「未確定」節）

### 判定は `any_down()` を**置換**する

```rust
// window_coordinator.rs（新規・#[cfg(windows)]）
pub(crate) fn main_in_modal_move_loop(app: &tauri::AppHandle) -> bool
```

スレッド id は **`GetWindowThreadProcessId(main_hwnd, None)`** で取る（`GetCurrentThreadId()` **ではない**）。`GetGUIThreadInfo(tid, &mut gui)` を撃ち、`gui.flags.contains(GUI_INMOVESIZE) && gui.hwndMoveSize == main_hwnd` を返す。取得に失敗したら **`false` を返す**（＝クランプする側へ倒す。「復帰が働かない」より「働きすぎる」ほうが今日の挙動に近く、退行の幅が小さい）。

**スレッド id を窓から引くことが、未検証の前提を 1 つ消す。** `GetCurrentThreadId()` を使う形は「モーダルループはイベントループスレッドで走る」という**測っていない前提**の上に立つ。窓の所有スレッドを直接問えば、その前提は要らなくなる（`GetWindowThreadProcessId` は `windows-0.62.2/.../WindowsAndMessaging/mod.rs:1184`・同じ feature に在る）。独立導出レビューの指摘を採用した。

`view.rs:1279` のガードは次になる。

```rust
if !crate::egui_shell::main_in_modal_move_loop(&app) {
    crate::egui_shell::clamp_main_into_work_area(&app, metrics.bar_height);
}
```

**ドラッグも同じモーダルループへ入るので、この 1 つの述語が両方を覆う**——tao の `drag_window` は `WM_NCLBUTTONDOWN(HTCAPTION)` を post するだけで（`tao-0.35.3/src/platform_impl/windows/window.rs:529-558`）、以降は OS の `SC_MOVE` ループである。ゆえに `any_down()` を残す必要が無い。**副産物として `ADR-main-window-clamp-on-pointer-release` 却下 5 が「受容する残余」と記録した押下フラグの固着が構造的に消える**（`any_down()` を読まなくなるため）。

### 計装は既存の `[trace]` に乗せ、新しい env も新しい行も足さない

**新設するのは 1 行だけである**——クランプの判断を刻む `egui_main:clamp` イベント（`window_coordinator.rs`）。他はすべて既にある。

| 観測したいもの | 計器 | 新設 |
|---|---|---|
| フレームの発火時刻・間隔 | `egui_frame`（`view.rs:1320`。`seq` / `ts_ms` は `trace()` が全行に付ける） | 不要 |
| フレームを起こした `RedrawRequested` の受信 | `SNOTRA_EGUI_WAKE_TRACE`（`snotra-egui-runtime/src/runtime.rs:281-286`） | 不要 |
| クランプの判断と発火（移動中フラグ・発火前後の位置） | `egui_main:clamp` | **新設 1 行** |
| 窓の `top` / `bottom` / `EXTENDED_FRAME_BOUNDS` の時系列 | 外部の測定スクリプト（`SnotraSmoke.psm1` + epoch ms） | 製品コード 0 行 |

**1 フレームぶんを 1 組で刻む**: `in_move_size` / `hwnd_is_main` / **クランプ前の位置** / `fired` / **クランプ後の位置**（`seq` と `ts_ms` は `trace()` が全行に付けるので全順序は無料）。

**「他人が動かした」はこの組の連続性から名指せる**——前フレームの*クランプ後*と今フレームの*クランプ前*が食い違い、その間に自分の `set_position` が無ければ、動かしたのは自分ではない。**この推論が成立するには位置が観測できる必要があるので、出力条件は次にする**:

> `in_move_size` が前フレームと変わった、**または** クランプ前の位置が前フレームのクランプ後と違う、**または** 発火した

**当初案の「判断が変わったとき・発火したときだけ」は誤りだった**——位置だけが変わったフレームで黙るので、まさに名指したい他人の移動が系列から消える。**沈黙の意味は「位置は動いておらず、何も撃たなかった」**である。行数は O(事象) のまま。

**この行は足場ではなく恒久の製品 trace である。** 理由: 却下 5 が「クランプが黙って死んでも検知手段が無い」と記録した受容残余へ、初めて検知手段を与える行だからである（`ADR-no-test-only-injection-in-product-code` の射程は「計測・検査のためだけの注入点」であり、この行は製品の観測可能性そのものを上げる）。ゆえに撤去条件を持たず、`AGENTS.md` の足場トリガー行は適用されない。

### `SPEC.md` は「ドラッグ中」を「モーダル移動ループ中」へ一般化する

`SPEC.md:476`（ポインタ非押下フレームでの復帰）・`:477`（**「ドラッグしている間は拘束しない」＝一般化する当の規則**）・`:478`（キーボードは拘束される、という as-built）・`:481`（幅設定の項に埋まった第 4 の写し）を、**1 つの規則へ畳む**。写しを増やさず、抑止の主体を「ポインタの押下」から「OS の移動ループ」へ移す。

**これは Phase 2 の最初に行う**——分類が仕様変更である以上、`AGENTS.md`「開発ワークフロー」1 の `SPEC.md` → コード → ドキュメントの順に従う。Phase 1 の計装は挙動を変えないので、この順序の外側にある。

## 変更ファイル一覧と対象シンボル

| ファイル | 対象 | 変更 |
|---|---|---|
| `src-tauri/src/egui_shell/window_coordinator.rs` | `main_in_modal_move_loop`（新規。`#[cfg(windows)]` と `#[cfg(not(windows))]` のスタブを対で置く——`clamp_main_into_work_area` の `:817` / `:841` と同じ形） | `GetGUIThreadInfo` による判定。rustdoc に射程・失敗時の倒し方・`hwndMoveSize` を使う理由 |
| 同 | `clamp_main_into_work_area` | `egui_main:clamp` trace の追加。rustdoc の「キーボード移動は保証の外にある」ブロック（`:754` / `:758` / `:763` / `:769` / `:794` / `:803`）を実測後の記述へ差し替え |
| `src-tauri/src/egui_shell/mod.rs` | `:43` の `pub(crate) use window_coordinator::{...}` | 新しい述語を再 export（`view.rs` は `crate::egui_shell::` 経由で呼ぶ） |
| `src-tauri/src/egui_shell/view.rs` | `:1279` のガードと `:1269` / `:1285-1286` のコメント | 述語の差し替え。**`:1285` の「クランプの `!any_down()` の外に置く」も文言が偽になる**。呼び出し順の制約（`drive_results_window` より前）は変えない |
| `SPEC.md` | §8.2「表示中の作業領域への復帰（#738）」の **`:476` / `:477` / `:478` / `:481`** | 「ポインタの押下」「ドラッグ」を主体にした 4 項を「モーダル移動ループ中は拘束しない」へ畳む。**`:477` が一般化する当の規則、`:481`（幅設定の項）は独立した第 4 の写しであり、落とすと「クランプはポインタが押されていないフレームで常に働き」が残る** |
| `docs/adr/ADR-modal-move-loop-clamp-suppression.md` | 新規 | 採用案と却下した候補。`ADR-main-window-clamp-on-pointer-release` を短縮名で引く |
| `src-tauri/CLAUDE.md` | **2 か所**: (1)「モジュール構成」`:51` の `window_coordinator.rs` の項——「呼ぶのは `view.rs` だが**ポインタ非押下のフレームに限る**」が偽になる。(2)「イベント駆動 wake の不変条件（#532 SU5）」 | (1) は述語名の差し替え。(2) はモーダルループ中のフレームの出所（`WM_PAINT` → `RedrawRequested` 直結）を測定結果に応じて 1 文で足す |
| `scripts/manual-smoke.ps1` | `:114` | 判定基準（バーが作業領域の下端を超えていたら FAIL）は**変わらない**が、添えた理由「ポインタを離した時点で…戻す」が偽になる。**理由の一句だけを直す** |

**写しの母集団には、ファイルの grep に入らないものが 2 つある**（独立導出 §4.3）:

- **PR 本文** — squash で main の commit message になるのに `git grep` には入らない（#1056）。**「ポインタを離したら戻る」を PR 本文へ書くと、それが 5 枚目の写しになる**
- **新 ADR 自身** — 書いている最中に「押下で抑止している」を**現在形で書かない**（凍結された時点で偽になる）

**`ADR-main-window-clamp-on-pointer-release` は編集しない**——ADR は凍結された歴史である（`ADR-adr-frozen-history`）。却下 5 の残余が閉じたことは新 ADR 側に書く。

**`Cargo.toml` は変更しない**——`Win32_UI_WindowsAndMessaging` は既に有効で、`GetGUIThreadInfo` / `GUITHREADINFO` / `GUI_INMOVESIZE` はすべてその feature に入っている（実測済み・「未確定」1）。

## 実装順序

### Phase 1 — 計装と測定（Q2・Q3 を 1 つの系列で答える）

- [ ] `window_coordinator.rs` に `egui_main:clamp` trace を足す（1 フレーム 1 組・出力条件は「設計」節。フィールド: `in_move_size` / `hwnd_is_main` / クランプ前の位置 / `fired` / クランプ後の位置）
- [ ] 測定スクリプトを**リポジトリの外**（`C:/tmp/snotra-1194/`）に書く。`SnotraSmoke.psm1` の関数だけで組み、`Alt+Space` → `M` → `↓`×200 → `Enter` を注入しながら `GetWindowRect` の `top`/`bottom` と `DwmGetWindowAttribute` の `EXTENDED_FRAME_BOUNDS` を epoch ms つきで刻む
- [ ] 対照ビルド（クランプ呼び出しを殺した 1 行パッチ）と本体ビルドの両方で 5 反復以上を測る。**測り終えたらパッチを戻して release を再ビルドする**
- [ ] **U1 を裁定する**: `top` が動いたのか高さが縮んだのか。「窓高は不変の純平行移動」は #1173 が測っていない前提であり、この 1 回の系列で真偽が決まる
- [ ] **Q2 に答える**: 27〜28 px の書き手を名指しする。`egui_main:clamp` が出ていなければクランプではなく、`SNOTRA_EGUI_WAKE_TRACE` と `egui_frame` の系列に対応するフレームが無ければ製品コードでもない（＝ OS 側）
- [ ] **Q3 に答える**: `egui_frame` の `interval_us` 系列と `egui_main:clamp` の発火位置から、「競り合い」と「フレームが疎」のどちらかを棄却する
- [ ] **U3 を測る**: `Enter`（`WM_EXITSIZEMOVE`）の**後**にフレームが来るか。来ないなら wake を置く必要がある
- [ ] 生ログは `C:/tmp/` に留め、**リポジトリへ入れるのは経路を数えた派生表だけ**にする（`[trace]` は利用者の実パスを逐語で載せる・#999）
- [ ] 測定条件（機体名・実 `config.toml` の `font_size` / `window_width`・scale・作業領域）を派生表に併記する

### Phase 2 — 仕様の更新と判定の実装

**`SPEC.md` を先に直す**（`AGENTS.md`「開発ワークフロー」1: 仕様変更は `SPEC.md` → コード → ドキュメントの順）。

- [ ] `SPEC.md` §8.2 の `:476` / `:477` / `:478` / `:481` を「モーダル移動ループ中は拘束しない」へ畳む（**4 か所ある。`:481` は幅設定の項に埋まっている**）
- [ ] `main_in_modal_move_loop` を実装し、rustdoc を書く。**書く内容は 4 つ**: (1) 失敗時に `false`（＝クランプする側）へ倒す理由、(2) `GUI_INMOVESIZE` **かつ** `hwndMoveSize == main` の連言にする理由——`GUITHREADINFO` は同型 `HWND` を 6 つ持ち取り違えが型で守られないため、連言が取り違えの爆風半径を縛る部品である（`/symmetric-check` 2c）、(3) `GUI_INMOVESIZE` は **move だけでなく size のループでも立つ**という射程（main は `resizable(false)` ゆえ通常は到達しないが、述語は区別しない）、(4) `#[cfg(not(windows))]` のスタブは `false` を返す
- [ ] `view.rs:1279` のガードを差し替え、直上のコメントを更新する（呼び出し順の制約は据え置き）
- [ ] **U3 の測定結果に従って、確定後の wake を置くか置かないかを決め、決めた側の理由を rustdoc へ書く**（置くなら「モーダルループ中のフレームで `ctx.request_repaint()` を撃ち、フラグが偽へ落ちた最初のフレームまで鎖を繋ぐ」。`src-tauri/CLAUDE.md`「armed 期限」の「時間経過で解消する不成立にだけ再要求してよい」との関係も同じ rustdoc に書く）

### Phase 3 — 対照つき検証

- [ ] **キーボード側**: 判定差し替え後に Phase 1 と同じスクリプトを回し、移動中に `bottom` がカーソル限界（1133〜1134）まで出られること、`Enter` 後の最初のフレームで 1 度だけ戻ることを 5 反復以上で確かめる
- [ ] **`/symmetric-check` の要求を満たす**: `in_move_size` が両向きに立つことを `egui_main:clamp` の系列で実測する（真になる時点と偽へ落ちる時点の両方が系列に現れること）。**真の側が一度も現れなかったら、`GetGUIThreadInfo` そのものより先にスレッド同一性を疑う**——未確定 1 の実測は `idthread = 0`（フォアグラウンドスレッド）で行っており、製品は `GetWindowThreadProcessId(main_hwnd, None)` が返す**窓の所有スレッド**を渡す。両者が一致するかは測っていない
- [ ] **マウスドラッグ側**: 差し替え前後で挙動が変わっていないことを対照つきで測る。**押下したままの窓矩形が外側に留まり、離した後に戻る**——却下 5 の表と同じ形の判別子を使う。**`SnotraSmoke.psm1` にマウス注入は無く、足さない**（「未確定」6）ので手作業で実施し、`egui_main:clamp` の系列で裏づける。実行前に、フォアグラウンド窓へ入力を撃つ旨とキーボードから手を離す時間を人へ明示的に求める
- [ ] **ドラッグ確定から復帰までの時間差を測る**——`WM_EXITSIZEMOVE` と合成 `WM_LBUTTONUP` の間で復帰が今日より早まる（`/state-check` の要注意行）。人間裁定「ドラッグ操作を終えたら戻る」を満たすことを確かめ、測った差を rustdoc へ書く
- [ ] **ドラッグ対照の結果で述語の形（`any_down()` の置換のまま／`!any_down() && !in_modal_move_loop` の連言へ倒す）を確定し、どちらの結果でも理由を新 ADR へ 1 節書く**
- [ ] **新しい述語のフレーム内コストを裏づける**——`egui_frame` の `update_us` を差し替え前後で比べる。`GetGUIThreadInfo` は可視中のフレームごとに 1 回増える呼び出しであり、専用の計器は要らない（未確定 1 の残り半分）
- [ ] `/race-check`・`/symmetric-check`・`/state-check` を**修正差分にも**再実行する（`AGENTS.md` の fix-forward 行）

### Phase 4 — 文書の同期

- [ ] `src-tauri/CLAUDE.md`「モジュール構成」`:51` の「ポインタ非押下のフレームに限る」を差し替える
- [ ] `scripts/manual-smoke.ps1:114` の理由の一句を差し替える（判定基準そのものは変えない）
- [ ] `clamp_main_into_work_area` の rustdoc の「キーボード移動は保証の外にある」ブロックを、実測後の記述へ差し替える（#1173 の表は残し、今回の測定を追加する。**測定条件を書く**）
- [ ] `docs/adr/ADR-modal-move-loop-clamp-suppression.md` を書く。**含める節は 5 つ**: 採用案／却下した候補（`WM_MOVING` フック＝サブクラス化——却下理由の再測結果つき）／却下 5 の受容残余が閉じたこと／**`.claude/rules/src-tauri.md:21`「Win32 を呼ぶ経路の新設は `PlatformBridge` 経由を既定とする」に逆らう理由**——問うているのが窓の所有スレッド自身の状態であり、platform スレッドへ委ねると答えが変質する／ドラッグ対照の結果で確定した述語の形
- [ ] `src-tauri/CLAUDE.md`「イベント駆動 wake の不変条件」へ、モーダルループ中のフレームの出所を測定結果に応じて 1 文で足す
- [ ] `npm run governance:check` を通す

## 不変条件と異常系

| 不変条件 | 壊れ方 | 検知手段 |
|---|---|---|
| クランプの呼び出しは `drive_results_window` より**前** | results が 1 フレームだけ旧位置へ追従 | `view.rs` の既存コメントが正本。順序は変えない |
| `check_show_bar_rect` はガードの**外**（`was_reset_frame` のみ） | show 直後にポインタが押されていた回の検証機会が落ちる | 既存構造を変えない（今回ガードの中身だけを差し替える） |
| `WorkArea::clamp` の算術は変えない | 新しい算術が 2 本目の導出になる | ユニットテスト 7 件（既存） |
| 「移動中である」の真偽が**両向きに立つ** | 真のまま固着してクランプが黙って死ぬ／偽のままで抑止が効かない | `egui_main:clamp` の系列（Phase 3）+ `/symmetric-check` |
| main の位置を書く経路は 2 つのまま | 3 つ目が増えると #878 の集計に 1 行加わる | `git grep` による数え上げ（`.claude/worktrees/` 除外） |

**異常系**: `GetGUIThreadInfo` が失敗したら `false`（＝クランプする）。`get_window("main")` が取れなければクランプしない（既存の倒し方と同じ）。**この 2 つは倒す向きが逆である**——前者は「移動中か分からない」ので今日の挙動へ、後者は「対象が無い」ので何もしない側へ倒す。rustdoc に書く。

## テスト方針と検証コマンド

**`main_in_modal_move_loop` は OS の状態を読む述語であり、純粋核テストで固定できない。** ゆえに検証は実機の対照実験が担う（Phase 1・3）。`WorkArea::clamp` の算術は触らないので既存のユニットテストがそのまま守る。

- カテゴリ A（`.rs` 変更）: `cargo fmt --all -- --check` / `cargo check --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test -p snotra` / `cargo doc --workspace --no-deps --document-private-items`
- カテゴリ C（ウィンドウ表示順に触る）: `npm test` / `npm run test:powershell` / `npm run smoke:startup` / `npm run smoke:egui`。**実バイナリを起動する検査の前に `cargo build -p snotra` を打つ**
- カテゴリ C の補足: `scripts/lib/SnotraTraceInvariants.psm1` の不変条件は `egui_results:*` / `egui_search:settled` だけを見る。**`egui_main:clamp` の新設では壊れないが、既存イベント名を変えれば壊れる**（今回は変えない）
- カテゴリ D: **`smoke:manual` の当該 1 項目だけ該当する**。`scripts/manual-smoke.ps1:114` が「バーが作業領域の下端を超えていたら FAIL」を目視項目として持ち、今回その判定の根拠になっている挙動を変える。**overflow / clipping / フォントレンダリングは変えないので D の全項目には広げない**（#1106）。実施は「エージェントが目視項目を自分で実施するとき」に従って自分で行い、Phase 1・3 の対照実験がその実体を兼ねる
- カテゴリ F（`SPEC.md`・ADR・`CLAUDE.md` 変更）: `npm run governance:check`
- **`cargo doc --workspace --no-deps --document-private-items` は手で走らせる**——doc コメントを大きく書き換えるのに PostToolUse hook は発火せず、intra-doc link 切れは CI でしか出ない（`docs/build-commands.md` カテゴリ A）

## `SPEC.md`・関連文書の更新要否

| 文書 | 要否 | 理由 |
|---|---|---|
| `SPEC.md` §8.2 | **要** | as-built を変える（分類が仕様変更である根拠そのもの） |
| `clamp_main_into_work_area` の rustdoc | **要** | #1173 の表と「代替判定はまだ無い」の記述が古くなる |
| `docs/adr/ADR-modal-move-loop-clamp-suppression.md`（新規） | **要** | 受け入れ条件が明示的に要求 |
| `ADR-main-window-clamp-on-pointer-release` | **不要**（編集しない） | 凍結された歴史（`ADR-adr-frozen-history`） |
| `src-tauri/CLAUDE.md` | **要**（測定結果に応じて 1 文） | モーダルループ中のフレームの出所は「通常フレームは勝手に回らない」の例外として読者に効く |
| `PERFORMANCE.md` | **不要** | 今回測るのは位置と順序であって性能ではない。測定値は rustdoc へ着地させる |
| `docs/architecture.md` | **不要** | 横断パターンを変えない（窓の幾何の所有は #878 の課題のまま。今回触らない） |

## `/race-check` の扱い — **計画段階では起動しない**

skill 本文が「**計画段階では起動しない**（経緯は #784）」と明記しており、母集団は `npm run race:boundaries` が**差分**から導く。いま `.rs` の差分は 0 件（`git status` は `workspace/` の untracked のみ）なので、走らせても母集団が空になり、「検査を実行した」と「検査対象があった」の区別が付かない。

**ゆえに Phase 3 の作業項目として実装差分に対して実行する**（issue の「検証」欄が指定する `/race-check` はここで満たす）。計画段階でこの skill を根拠に「安全」と書かない。

## `/symmetric-check` の結果

### 2a. コードパスの対称ペア

| 候補 | 判定 | 根拠 |
|---|---|---|
| `WM_ENTERSIZEMOVE` / `WM_EXITSIZEMOVE`（述語が真になる時点 / 偽へ落ちる時点） | **[適用]** | 両向きが立つことを実測する必要がある。真の側は未測定（未確定 1 で偽の側だけ測れている）→ Phase 3 の作業項目に入れた |
| `clamp_main_into_work_area` / `position_on_target_monitor`（show 側の配置） | **[不要]** | ケース A: main 自身が移動ループ中 → main は既に可視ゆえ show は走らない。ケース B: 別窓（settings）が移動ループ中 → `hwndMoveSize != main` ゆえ述語は偽、クランプは通常どおり走る。**`hwndMoveSize` まで見る設計がこのケースを構造的に閉じている** |
| `clamp_main_into_work_area` / `read_placement_relative`（hide 時の保存） | **[不要]（ただし挙動は変わる）** | ホットキー hide は `on_event_loop` 経由（`main.rs:461`）で、モーダルループの入れ子メッセージポンプが配送しうる。**今日は移動中もクランプが働くので内側の座標が保存されるが、変更後は外側の座標が保存されうる。** それでも `position_on_target_monitor` が読み込んだ placement を `target_wa.clamp(...)` に通す（`window_coordinator.rs:257`）ため、**次の show で内側へ戻る＝自己修復する**。#755/#801 の「hide がずれた位置を保存して恒久化した」形にはならない |
| `check_show_bar_rect` | **[不要]** | ガードの外（`was_reset_frame` のみ）にあり、今回ガードの中身しか変えない（`view.rs:1285` のコメント文言だけ更新する） |

### 2b. リソースライフサイクルの対称ペア

新しいリソースは増えない。`GUITHREADINFO` はスタック上の POD で解放経路を持たない。**唯一の新しい状態は `egui_main:clamp` の「前フレームの判断」memo** で、`SearchWindowView` の寿命に閉じ、挙動を決めない（trace の量を絞るためだけ）ので reset-on-show の対象外。

### 2c. 同型ペアの取り違え（**要対処**）

`GUITHREADINFO` は同じ型 `HWND` のフィールドを **6 つ**持つ（`hwndActive` / `hwndFocus` / `hwndCapture` / `hwndMenuOwner` / `hwndMoveSize` / `hwndCaret`。`windows-0.62.2/.../WindowsAndMessaging/mod.rs:3696-3705` で実測）。**`hwndMoveSize` を `hwndActive` と取り違えても型・コンパイル・既存テストのすべてが通る。**

- **配線の起点**: `GetGUIThreadInfo` が埋めた構造体のフィールド選択。ここで初めて意味が分かれるので、**型は守っていない**
- **区別できる観測**: 取り違えると、main がフォアグラウンドに居るだけで述語が真になり、**クランプが黙って死ぬ**（#738 の修正が無音で無効化される）。`egui_main:clamp` の系列に「移動していないのに `in_move_size = true`」が現れることが唯一の検知手段である
- **構造側の緩和（設計に織り込み済み）**: 述語を `flags.contains(GUI_INMOVESIZE) && hwndMoveSize == main` の**連言**にしてあるため、フィールドを取り違えても `GUI_INMOVESIZE` が立っていない限り真にならない。**取り違えの影響は「どの窓が移動中でも main が移動中と読む」まで縮む**——連言は装飾ではなく、この取り違えの爆風半径を縛る部品である。**rustdoc にそう書く**

## `/state-check` の結果

**直交性マトリクス（Step 2）は非該当。** 差し替えるガードは窓の幾何を守る述語であり、UI モード（`FolderExpansionMode` / `ToolSelectionMode` / `QueryIntent` / `indexing` / `launching`）のどれも読まないし、どれからも読まれない。**リセット経路（Step 3）も非該当**——新しい状態シグナルを増やさない（唯一の memo は trace の量を絞る値で、挙動を決めない）。

**Step 5（SPEC §8.6 との整合）も非該当**: §8.6 は状態遷移図であり、クランプ・ポインタ・作業領域のいずれにも言及しない（`SPEC.md:506` 以降を走査して 0 件）。今回変えるのは §8.2 である。

**該当したのは Step 4「ガードの過剰阻害」（バグパターン 4）だけである。** 述語の真偽が今日と食い違うコンテキストを列挙した。

| コンテキスト | 今日（`!any_down()`） | 変更後（`!in_modal_move_loop`） | 判定 |
|---|---|---|---|
| キーボード移動中 | クランプする（**これが欠陥**） | しない | **[意図した変更]** |
| マウスドラッグ中 | しない | しない | [整合]（tao の `drag_window` は `WM_NCLBUTTONDOWN(HTCAPTION)` を post し OS の同じループへ入る） |
| **ドラッグ確定〜合成 `WM_LBUTTONUP` が egui へ届くまで** | しない（押下フラグがまだ真） | **する** | **[要注意・意図した変更]** `WM_EXITSIZEMOVE` でフラグが落ちてから tao が `WM_LBUTTONUP` を post するまでの区間（`tao .../event_loop.rs:1036-1043`）。復帰が**今日より数フレーム早くなる**。人間裁定「ドラッグ操作を終えたら戻る」は満たすが、**Phase 3 のドラッグ対照でこの時間差を測る** |
| 窓の**リサイズ**モーダルループ中（`Alt+Space` → `S`） | ポインタ次第 | しない | **[整合]** main は `resizable(false)` で生成される（`egui_shell/mod.rs:351`。`decorations:false` と対で `:325` の doc が明記）ため、通常の経路では size ループへ入らない。**ただし `GUI_INMOVESIZE` は move と size を区別しない**ので、「size では立たない」とは書かず**射程として rustdoc に残す** |
| ポインタを押しているが窓は動かしていない（行の上・入力欄の上） | しない | **する** | [整合] 窓が動いていない以上クランプは同値判定で撃たない（`window_coordinator.rs:832` の「同値なら撃たない」）。**窓が既に外側にある場合のみ差が出るが、その状態はドラッグ直後にしか作れず、上の行と同じ区間である** |
| 押下フラグが固着した状態（`PointerGone` / フォーカス喪失） | **クランプが黙って死ぬ**（却下 5 の受容残余） | 正常に働く | **[改善]** 残余が閉じる |
| 初回起動・インデックス構築中・エラーリカバリ | 述語は移動ループを読むだけで、これらの状態を読まない | 同左 | [整合] 阻害しない |

**要対処 → 計画へ反映済み**: 「ドラッグ確定〜合成 `WM_LBUTTONUP`」の区間で復帰が早まることを Phase 3 のドラッグ対照の測定項目に含め、リサイズループの射程を rustdoc に書く。

## 未確定（実装前に潰す）

- [x] **1. `GetGUIThreadInfo` を追加 feature なしで呼べるか** — **実測で確定**。`src-tauri` と同じ単一 feature `Win32_UI_WindowsAndMessaging` だけを持つ使い捨て crate（リポジトリ外）でコンパイル・実行し、`call result = Ok(())` / `cbSize = 72` / `flags = GUITHREADINFO_FLAGS(0)` / `in_move_size = false` / `hwndMoveSize = HWND(0x0)` を得た。**偽の側（移動していないとき false）はこれで測れている。真の側は Phase 3 の `/symmetric-check` で測る**
- [x] **2. `ADR-main-window-clamp-on-pointer-release` 却下 1 の却下理由が今も真か** — **測り直して真**。tao 0.35.3 の `WindowExtWindows` が公開するのは `hwnd()` / `hinstance()` / `set_enable` / `set_taskbar_icon` / `set_overlay_icon` / `theme` / `reset_dead_keys` / `begin_resize_drag` であり、**wndproc フック・サブクラス化の公開 API は無い**（`platform/windows.rs` を走査して 0 件）。`WM_MOVING` を捕まえるには今も `hwnd()` から自前で `SetWindowSubclass` する必要があり、却下理由 1（tao 所有の wndproc への介入）はそのまま成立する。**ただし理由 2（「離した時点の復帰で満たされる」）の射程はドラッグに限られていたことが #1173 で判明した**——キーボード移動には「離す」が無い。**今回の変更は拘束を外す方向なので `WM_MOVING` は不要であり、この射程の欠けは採否を変えない。**新 ADR にこの再測結果を書く
- [x] **3. 判定は `any_down()` の置換か、連言か** — **置換で実装する**。tao の `drag_window` は `WM_NCLBUTTONDOWN(HTCAPTION)` を post するだけで（`window.rs:529-558` を読んで確認）、以降は OS の `SC_MOVE` モーダルループである。ゆえに 1 つの述語が両経路を覆う。**Phase 3 のドラッグ対照で差が出たら連言へ倒す**——これは分岐する作業項目ではなく、「測って、どちらでも ADR に理由を 1 節書く」1 項目として Phase 3 に置いた
- [x] **4. 計装は恒久 trace か足場か** — **恒久**。却下 5 が「クランプが黙って死んでも検知手段が無い」と記録した受容残余へ検知手段を与える行であり、計測のためだけの注入点ではない（`ADR-no-test-only-injection-in-product-code` の射程外）。ゆえに撤去条件を持たない。**量は「判断が変わったとき・発火したときだけ」に絞って O(事象) にする**
- [x] **5. どの機体・どの config で測るか** — **GPDWINMINI・使い捨てプロファイル（`SNOTRA_CONFIG_DIR`）で測り、seed した `config.toml` の内容を派生表に書く**。#1173 / PR #1193 は「使い捨てプロファイル」としか書かず入力値を記録しておらず、既定（`font_size = 15` → `bar_height = 43.0` 論理）と実 config（`font_size = 13` → `41.0`）のどちらだったか一次資料から復元できない。**過去を特定するのではなく、今回の記録を欠かさない**（`two-dev-machines-unlabeled-in-perf-doc`）
- [x] **6. マウスドラッグ側の対照をどう測るか** — **手作業で実施し、`SnotraSmoke.psm1` にはマウス注入を足さない**。モジュールに無い操作を自前 P/Invoke で足すと画面ロック検出（#866）の外へ出る面が 1 つ増える（`docs/build-commands.md:116`）。得られるのは 1 回の対照実験であり、面を増やす代価に見合わない。手作業の結果は `egui_main:clamp` の系列が裏づける

## plan-review 結果

- リスク: **高**
- レビュー方式: **独立導出 1 体**（Step 2b。issue の WHAT だけを渡し、`workspace/` と `.claude/worktrees/` は読ませず grep からも除外させた）
- エージェント数: 1（3b の敵対的調査 1 体は別枠）
- 成果物: `workspace/plan-review-1194-derivation.md`。issue 番号・導出ファイル・シンボル・3 分類はすべて揃っており、再起動は不要だった

### 導出 ∖ plan（漏れ候補・**すべて採用して反映済み**）

- **`SPEC.md:477`「ドラッグしている間は拘束しない」** — 自分の grep では `:476` / `:478` / `:481` の 3 行しか挙げていなかった。**`:477` は一般化する当の規則そのもの**である。逐語を読んで確認し、4 行へ直した
- **スレッド id を `GetCurrentThreadId()` ではなく `GetWindowThreadProcessId(main_hwnd, None)` で取る** — `GetGUIThreadInfo(0, ..)` はフォアグラウンドスレッドであってこちらのスレッドではない。窓の所有スレッドを直接問えば「モーダルループはイベントループスレッドで走る」という**未検証の前提が要らなくなる**。`windows-0.62.2/.../mod.rs:1184` に実在を確認して採用
- **`.claude/rules/src-tauri.md:21`「Win32 を呼ぶ経路の新設は `PlatformBridge` 経由を既定とする」** — 自分は当てていなかった規範。逐語を確認し、**逆らう理由を新 ADR に書く**を作業項目へ入れた
- **計装の出力条件の欠陥** — 当初案「判断が変わったとき・発火したときだけ」では、**位置だけが変わったフレームで黙る**。それはまさに「他人が動かした」を名指したいフレームである。出力条件を「位置が前フレームのクランプ後と違うとき」を含む形へ直した
- **`/dry-check` + `findReferences`** — 関数の新規定義に当たるトリガー行を落としていた
- **カテゴリ D の判定** — 「該当しない」としていたが、`scripts/manual-smoke.ps1:114` が当該挙動の目視項目を現に持つ。**その 1 項目だけ該当**へ直した

### 判断の不一致（**根拠を突き合わせて降格**）

- **導出は計装を「足場」と前提し、撤去条件を計装自身の doc へ書くこと（`AGENTS.md` の足場トリガー行）を求めた** — **こちらは恒久 trace として採らない**。根拠は未確定 4: この行は却下 5 が「検知手段が無い」と記録した受容残余へ検知手段を与えるものであり、計測のためだけの注入点ではない。**ただし導出の付随指摘は採る**——撤去条件を「#1194 が閉じたら」にすると自己参照で発火しない（恒久ゆえ撤去条件を持たないので、この罠は構造的に避ける）／測定値の着地先は `PERFORMANCE.md` ではなく新 ADR と `clamp_main_into_work_area` の doc である（性能ではなく挙動ゆえ・`ADR-measurement-canon-in-code-doc` の趣旨）。**後者は計画の「文書の更新要否」表と一致していた**
- **導出の所見 2 の帰結「#1194 の後は『作業領域の外に確定した窓』が実在しうるので、掴んだ瞬間に内側へ跳ねて以後外へ戻せない」** — **機序（ドラッグ開始に 2 段の隙がある）は正しいが、帰結は前提が誤りである。** 受け入れ条件 Q1 自身が「確定後の最初のフレームで 1 度だけ戻る」を要求しており、**この変更は外側への確定を可能にしない**（移動中の拘束だけを外す）。ゆえに静止時のバー矩形は常に作業領域の内側にあり、ドラッグ開始の隙でクランプが走っても `window_coordinator.rs:832` の同値判定で撃たない。**所見は軽微へ降格し、降格の根拠をここに残す**。ただし Phase 3 のドラッグ対照はこの隙も観測範囲に含む

### 軽微

- 凍結 spec `docs/superpowers/specs/2026-07-24-646-two-window-ui-design.md:98`「ドラッグ中はネイティブ移動ループが回り egui フレームが止まる」を #1173 の実測が覆している。**凍結文書ゆえ編集しないが、この前提の上に設計しない**（逐語を確認済み）
- 述語を `read_frame_geom` / `read_bar_anchor` へ混ぜない（射程は「幾何」であって「入力状態」ではない）
- `view.rs:1285` は識別子だけ差し替え、命題（検出器をガードの外に置く理由）は書き換えない

### 未検証

- **`GUI_INMOVESIZE` が実機で立つこと**（真の側）。偽の側だけ実測済み（未確定 1）。Phase 3 で測る——**この 1 点が述語の成否を決める**
- 27〜28 px の移動とフラグが降りる順序（Phase 1 の測定が答える。上の ⚠️ 節）
- キーボード移動での多モニター封鎖（人間裁定により演繹のまま残す）
- egui 0.36 に代替手段があるか（調べていない）

### 判断

- **実装着手: 可**（人間の承認後）

## 人間レビュー

- [x] 承認済み — 2026-08-27 / 問い: "この計画で実装に着手してよろしいですか。" / 回答: "承認"

## セルフレビュー

主エージェント自身の 5 点照合:

1. **issue の全要件に作業項目が対応する** — Q1（Phase 2・3）／Q2（Phase 1）／Q3（Phase 1）／ADR（Phase 4）／マウスドラッグの非退行（Phase 3）。issue の「やること」4 項も対応: 計装 1 度（Phase 1）・却下 1 の再測（未確定 2）・`GetGUIThreadInfo` を先に検証（未確定 1）・ドラッグ側の対照（Phase 3）
2. **境界条件と検証** — `GetGUIThreadInfo` 失敗時／`get_window("main")` 不在／`hwndMoveSize` が他窓（settings）／フラグが真のまま固着／`WM_EXITSIZEMOVE` 後にフレームが来ない（U3）。各々に倒す向きか測定を割り当てた
3. **新しい状態・リソース・プロセスの正常/失敗/破棄経路** — 新しい永続状態は増えない。`GUITHREADINFO` はスタック上の POD で解放不要。**唯一の状態は `egui_main:clamp` の「前フレームの判断」memo**で、これは `SearchWindowView` の寿命に閉じ、reset-on-show で戻す必要が無い（trace の量を絞るためだけの値であり、挙動を決めない）
4. **より単純な既存パターンで置き換えられないか** — tao の `MARKER_IN_SIZE_MOVE` を使えれば OS 呼び出しすら要らないが、公開されていない（research C2）。`WM_MOVING` フックはサブクラス化が要る（未確定 2）。`GetGUIThreadInfo` が最も単純な既存機構である
5. **壊してはならない不変条件に検知手段がある** — 上の「不変条件と異常系」表。**「移動中である」の両向きだけが実機測定に依存し、純粋核テストを持てない**——これは受容する残余であり、`egui_main:clamp` の系列が唯一の検知手段である

適用した check スキル（`AGENTS.md` 条件別チェック表から）:

| スキル | 実施 | 結果 |
|---|---|---|
| `/symmetric-check` | **実施**（上に節） | 2c で**要対処 1 件**——`GUITHREADINFO` の同型 `HWND` 6 フィールドの取り違えが型で守られない。連言による緩和を rustdoc へ書く形で計画へ反映 |
| `/state-check` | **実施**（上に節） | Step 4 のみ該当。**要注意 2 件**——ドラッグ確定後の復帰が早まる／リサイズループでも述語が真になる。どちらも計画へ反映 |
| `/race-check` | **計画段階では起動しない**（skill 本文の指定・#784） | Phase 3 の作業項目として実装差分に対して実行する |
| `/dry-check` + LSP `findReferences` | **Phase 2 で実施** | `AGENTS.md`「関数・型を新規定義／改名／導入」の行に当たる（`main_in_modal_move_loop` の新設）。**新 API の導入と呼び出し点の移行は Phase 2 に束ねてある**——分けると `-D warnings` 下で未使用の新 API が `dead_code` で落ちる |
| `/persistence-check` | **非該当** | 永続形式・キーを変えない。`window.bin` に保存される値は変わりうるが形式は不変で、show 側の `target_wa.clamp` が自己修復する（`/symmetric-check` 2a） |

- リスク: **高**（OS のモーダルループをまたぐ状態・ガード条件の差し替え）
- plan-review: **独立導出 1 体**（Step 2b。理由: 設計上の危険は 3b の敵対的調査が既に叩いており、残る主なリスクは「散文が偽になるファイルの取りこぼし」＝網羅性である）
- エージェント数: 2（3b の敵対的調査 1 体 + 独立導出 1 体）
- 要対処と反映:
  - 3b から 3 件採用（U2 の「未特定」を降格・C5 の前提値訂正・副次仮説の棄却）。2 件は自分で閉じた（`commands/`・`platform/` の走査、`docs/superpowers/specs/` の日付による除外）
  - **自己照合 7（散文が偽になるファイル）で 4 件を発見**——`SPEC.md:476` と `:481`（`:478` 以外にポインタ／ドラッグ主体の記述が 3 つ）、`src-tauri/CLAUDE.md:51`（「モジュール構成」節。当初は別節しか挙げていなかった）、`scripts/manual-smoke.ps1:114`。**すべて変更ファイル一覧と作業項目へ反映済み**
- 未検証:
  - 「移動中である」の**真の側**（Phase 3 で測る。偽の側は未確定 1 で実測済み）
  - **多モニターでの封鎖**（人間裁定により演繹のまま残す）
  - **size ループでの挙動**（main は `resizable(false)` ゆえ通常の経路では到達しないが、述語は move と size を区別しない。射程として rustdoc に残す）
