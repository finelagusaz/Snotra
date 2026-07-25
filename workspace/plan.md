# plan: #628 — 可視アイドル再描画の源を計器で名指し、扱いを決める

前提は `workspace/research.md`。要旨: **項目 1（`fill_mesh` 最適化）は SU6.5 の実測（`raster_ms` p95 4.68ms）により実施しない**。本計画は**項目 2（可視アイドル再描画）だけ**を対象とし、SU6.5 決定 6 と同じ「測ってから決める」刻みで進む。

**plan-review 後の訂正**（詳細は末尾セルフレビュー）: research.md の「アイドルでは `request_repaint` が発火しない」は**結果表示中には成立しない**——`view.rs:850` の `wake_results` が `drive_results_window` 末尾で毎フレーム無条件に results 窓を起こす（`grep request_repaint` に出ない同概念・別名）。ゆえに測定は**空クエリと結果表示中の 2 条件**で行う。

## 変更ファイル一覧

| ファイル | 変更 | Phase |
|---|---|---|
| `snotra-egui-runtime/src/runtime.rs` | `EguiWindow::render` に repaint 原因トレース（`SNOTRA_EGUI_REPAINT_TRACE`）。`EguiWindow` に `repaint_trace_prev: Option<Instant>` | 1 |
| `snotra-egui-runtime/src/repaint.rs` | 原因列の整形を純関数 `format_repaint_causes` として置き、単体テストを付ける | 1 |
| （実測結果次第）`src-tauri/src/egui_shell/view.rs` | `set_visuals` 直前に `visuals.text_cursor.blink = false` | 3-A |
| （実測結果次第）`SPEC.md` §11（523 行付近） | キャレット非点滅を parity gap として明記 | 3-A |
| `PERFORMANCE.md` | 計測節へ egui 計器 env（`SNOTRA_EGUI_PAINT_TRACE` / `SNOTRA_EGUI_REPAINT_TRACE`）と可視アイドル実測値を記録 | 4 |
| `docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md:30` | SU1 行の follow-up 表記へ #628 の決着を反映 | 4 |

**触らない**（測る前に配線を変えない）: `raster.rs` 全体・`repaint.rs` の worker 本体 / `RepaintScheduler` / `WindowWaker`・`runtime.rs` の `RedrawRequested` arm・`renderer.rs`（既存 `SNOTRA_EGUI_PAINT_TRACE` をそのまま使う）・`view.rs:850` の `wake_results`（level-triggered だが**実測が名指しするまで触らない**）・`mod.rs` の `position_results_below_main`。

## 実装順序

### Phase 1 — 計器を入れる（コード変更はここだけが確定分）

**1a. `repaint.rs` に純関数を足す**

```rust
/// repaint 原因列を 1 行の trace 文字列へ（`file:line reason` を `; ` 区切り）。
/// 空列は `-`（「原因が無い」と「トレースが壊れた」を出力で区別するため）。
pub(crate) fn format_repaint_causes(causes: &[egui::RepaintCause]) -> String {
    if causes.is_empty() {
        return "-".to_owned();
    }
    causes.iter().map(|c| c.to_string()).collect::<Vec<_>>().join("; ")
}
```

`RepaintCause` は `egui` の公開 re-export（`egui-0.35.0/src/lib.rs:465`）で、`Display` は `{file}:{line} {reason}`（`context.rs:267-270`）。全フィールド `pub`・`#[non_exhaustive]` なしゆえテストから構築できる。

テスト（`repaint.rs` の `mod tests`）:
- `format_repaint_causes(&[])` == `"-"`
- 2 件で `; ` 区切り・各要素が `file:line` を含む

**1b. `runtime.rs::render()` に計器を配線**

`let output = self.context.run_ui(...)` の**直後**に、env ゲートの内側で:

```rust
// 可視アイドルの周期 repaint 源を名指しする計器（#628）。env 未設定なら Instant も
// causes の clone も取らない（計測器が測定対象を汚さない・renderer.rs と同規範）。
// window= は #646 PR2 の 2 窓（main/results）を区別するため。since_prev_ms の初回は NaN。
if std::env::var_os("SNOTRA_EGUI_REPAINT_TRACE").is_some() {
    let now = std::time::Instant::now();
    let since = self
        .repaint_trace_prev
        .replace(now)
        .map(|prev| (now - prev).as_secs_f64() * 1000.0)
        .unwrap_or(f64::NAN);
    eprintln!(
        "SNOTRA_EGUI_REPAINT window={} focused={} since_prev_ms={since:.1} causes={}",
        self.window.label(),
        self.context.input(|i| i.focused),
        crate::repaint::format_repaint_causes(&self.context.repaint_causes()),
    );
}
```

`EguiWindow` に `repaint_trace_prev: Option<std::time::Instant>` を足す（`new()` で `None`。構築点は `runtime.rs:265` の 1 箇所のみ）。

**計器の 3 つの注意（読み違えると原因を取り違える）**:

- `repaint_causes()` が返すのは `prev_causes`——`begin_pass_repaint_logic` が pass 冒頭で `causes` と swap するため（egui 0.35 `context.rs:98-105`）、`run_ui` 直後に読む値は**1 つ前の pass で積まれた原因**である。定常アイドルでは同じ源が繰り返すので判別には足りるが、**読み手が 1 フレームずれを知っている必要がある**（コメントにも書く）
- **遅延ゼロの `request_repaint()` は 2 フレーム生む**（`request_repaint_after` が `outstanding = 1` を立て、次 pass 冒頭の `begin_pass_repaint_logic` が callback をもう 1 回撃つ・`context.rs:110-146`）。件数照合で「1 要求 = 1 フレーム」を前提にしない
- **`focused=` は Phase 2 の false-negative を塞ぐための必須項目である**——egui はフォーカスがあるときだけ点滅 repaint を出す（`builder.rs:864-869`）。Phase 2 は測定のため `auto_hide_on_focus_lost=false` にして**製品には存在しない「可視かつ非フォーカス」状態を作る**ので、観測中に窓がフォーカスを失うと、疑っている当の源が黙り、ログは「眠っている」ように見える。フィールドが無いと判定 (a) を誤って引く
- **`request_repaint_after(d)` は実際には `d - predicted_dt` で起きる**（`context.rs:148-151`）。本 runtime は `RawInput::predicted_dt` を一度も書かない（`input.rs:29` の `take` が screen_rect / time / viewport しか埋めない・確認済み）ため既定 1/60 秒のままで、**予約はつねに約 16.7ms 早く発火する**。観測した cadence が「予約値ちょうど」でなくても異常ではない

**計器を置かない場所と、その理由**（A-4「`RedrawRequested` 到着」と A-6「実描画」の差分）:
`render()` 冒頭の `if !self.visible` 早期 return（`runtime.rs:300-310`）は、**crate 内に `visible` を false にする経路が無く現在到達不能**であることがコード自身に記録されている。かつ hidden 窓では `render()` 自体が呼ばれない（SU5 実測の不変条件）。ゆえに「到着したが描かなかったフレーム」は現状存在せず、`RedrawRequested` arm 側への計器追加は**行わない**（YAGNI）。この前提が崩れる変更（runtime 側で描画抑止を入れる等）を将来入れるなら、そのとき arm 側の計数を足す。

Run: `cargo test -p snotra-egui-runtime && cargo clippy -p snotra-egui-runtime --all-targets -- -D warnings`（PostToolUse hook が自動実行。沈黙 = 合格）

commit: `chore(egui-runtime): repaint 原因トレースを追加（#628 可視アイドルの計測）`

### Phase 2 — 実測（人手スモーク・ユーザーへ依頼）

**準備（欠くと測定が成立しない）**:

1. `config.toml` の `auto_hide_on_focus_lost` を一時的に `false` にする（放置中に窓が消えると 60 秒の可視アイドルが取れない。SU6.5 の G1 手順が同じ理由で同じ退避をしている）。**測定後に必ず戻す**
2. インデックス構築の完了を待ってから観測窓に入る（構築完了は世代検知経由で単発の repaint を生む・`view.rs:1182-1188`）
3. マウスカーソルを窓の上に置いたまま静止させない（hover 由来のフレームが混じる）

```powershell
cargo build --release -p snotra
$env:SNOTRA_EGUI_REPAINT_TRACE=1; $env:SNOTRA_EGUI_PAINT_TRACE=1; $env:SNOTRA_TRACE=1
.\target\release\snotra.exe 2>&1 | Tee-Object -FilePath $env:TEMP\628-idle.log
```

**観測条件（3 つ・順に実施）**:

| # | 状態 | 見たいもの |
|---|---|---|
| 1 | 表示 → **空クエリ**のまま 60 秒放置（results 窓は `results_should_show` で hidden） | main 単独のアイドル cadence |
| 2 | 結果が出るクエリを打鍵 → 打鍵完了後 **60 秒放置**（results 窓 visible） | `wake_results`（`view.rs:850`）の毎フレーム連鎖で 2 窓ぶんになるか |
| 3 | Alt+Q で非表示 → **30 秒放置** | hidden 停止の裏取り（SU6.5 G3(a) の再確認） |

集計:

```powershell
$re = 'SNOTRA_EGUI_REPAINT window=(\S+) since_prev_ms=(\S+) causes=(.*)'
$rows = Select-String -Path $env:TEMP\628-idle.log -Pattern $re | ForEach-Object {
    [pscustomobject]@{ Window = $_.Matches[0].Groups[1].Value
                       Ms     = $_.Matches[0].Groups[2].Value
                       Causes = $_.Matches[0].Groups[3].Value }
}
$rows | Group-Object Window | ForEach-Object { "{0}: frames={1}" -f $_.Name, $_.Count }
$rows | Group-Object Window, Causes | Sort-Object Count -Descending | Select-Object -First 8 Count, Name
$rows | Where-Object { $_.Ms -ne 'NaN' } | ForEach-Object { [double]$_.Ms } | Measure-Object -Average -Minimum -Maximum
```

**ログの読み方（誤帰属がこの作業の唯一の失敗様式である）**:

- **`focused=false` の行が観測窓に混じった run は無効**とし、測り直す。上記のとおり、その状態では疑っている源が黙る。判定 (a) は**すべての行が `focused=true` の run からしか引けない**
- **`window=results` で `causes=-` の行は egui の repaint 源ではない**——`position_results_below_main` が毎フレーム `SetWindowPos` を撃つ（`view.rs:837` → `mod.rs:549-568`）ため、OS 由来の再描画が egui の要求なしで届く。周期源の判定には数えない（数えると増幅器を源と取り違える）
- `since_prev_ms` の巨大値は hide またぎ・観測窓の境界であり、cadence の集計から除く

**判定（この分岐が Phase 3 を決める）**:

| 観測 | 結論 | 次 |
|---|---|---|
| (a) 条件 1・2 とも数フレームで収束（周期発火なし・**全行 `focused=true`**） | 現象は `MvpView` 撤去（#702）とともに消滅 | Phase 4-a: 実測を issue へ記録して close |
| (b) ~500ms 周期・causes が `text_selection/visuals.rs:313` | **キャレット点滅**が源と確定 | Phase 3（ユーザー判断・下記） |
| (c) 条件 2 だけ frames が倍（results 窓が main に追随） | `wake_results` の level-triggered が増幅器 | 源（= main 側の周期要因）を先に決着させてから、増幅の是非を別途判断 |
| (d) 上記以外の周期源（別の `file:line`） | 名指しされた箇所を個別に判断 | 計画を更新してから着手 |

**条件 3 の合否**: `SNOTRA_EGUI_REPAINT` と `SNOTRA_EGUI_PAINT` の行が**ともに 0**。0 でなければ SU6.5 G3(a) の回帰であり、#628 の扱いを flip 後の回帰として昇格させる。

### Phase 3 — (b) だった場合のみ・**ユーザー判断は済んでいる: B（受容）**

**2026-07-26 決定: 分岐 B。** 実測が (b)（源はキャレット点滅）を示したら、**コード変更はせず**に受容し、issue へ「源 = キャレット点滅・2fps・CPU 1% 未満・parity 維持のため受容」と記録して閉じる。A（点滅停止）は parity を捨てる見返りが「将来の検知器」であり、いま必要なものではない（YAGNI）。以下 A/B の記述は判断の根拠として残す。


キャレットが点滅する限りアイドル休眠はできない（点滅 = 定期再描画）。両立しないため、どちらを採るかは UX の判断であり実装からは決まらない。

- **分岐 A「眠らせる」**: `view.rs` の `ctx.set_visuals(visuals)`（1219 行）直前へ `visuals.text_cursor.blink = false;`。可視アイドルの repaint は数フレームで 0 へ収束する（`set_visuals` は次フレームから効くため「即時 0」ではない）。**副作用の範囲**: Windows では `Visuals::ime_composition.legacy_visuals` が既定 true（`egui-0.35.0/src/style.rs:1659-1663`）で、Snotra はこれを触っていない。ゆえに IME 変換中のキャレットも同じ `paint_text_cursor` 経路を通り、**変換中キャレットも非点滅になる**（変換範囲の下線描画は別経路ゆえ不変）。旧 WebView2 の `<input>` は OS 既定で点滅していたため、これは flip 後に生じる意図的な parity gap であり `SPEC.md` **§11（523 行付近・ビジュアル節）** へ明記する（挙動変更 ⇒ 仕様変更扱い・AGENTS.md ステップ 0）。§4.8（196 行）はキャレット**位置**の話であり追記先ではない
- **分岐 B「受容する」**: 2fps の repaint を受容し、issue のチェックボックス 2 を「源はキャレット点滅・受容」として close。根拠は実測 CPU（`raster_ms` 4.68ms × 2fps ≒ 1% 未満）と、点滅キャレットが標準的な入力欄の挙動であること

**既定の推奨は B**（YAGNI・parity 維持・変更ゼロ）。A を採るなら SPEC 同期 + 目視スモーク（`cargo run -p snotra` でキャレットが実線で出る・入力できる・IME 変換中も破綻しない）をセットで行う。

**blur→hide への影響は無い**（plan-review で挙がった懸念の反証）: egui は viewport がフォーカスを持つときだけキャレットを描き点滅 repaint を出す（`text_edit/builder.rs:864-869` の `if viewport_has_focus`）。blur 後は点滅由来のフレームがそもそも供給されていないため、点滅停止が `view.rs:1305-1321` の 100ms 猶予経路を変えることはない。ただし**その猶予経路が「次のフレームが来ること」に依存し再要求を持たない**という脆さ自体は実在する（`request_repaint_after(100ms)` は predicted_dt 分だけ早く起きるため `grace_elapsed` が false になりうる）。**#628 の対象外**とし、Phase 2 のログで blur→hide 間のフレーム間隔が観測できたら別 issue として起票する。

### Phase 4 — 決着の記録

- **issue #628**: 実測値（窓別 frames / cadence / causes・hidden 0 件）と、項目 1 を実施しない根拠をコメント。チェックボックスは**未チェックのまま理由付きで**扱う。項目 1 の根拠は 3 点:
  1. SU6.5 G3(b) の実測（3680 フレームで `raster_ms` p95 4.68ms・予算超過 0 件）
  2. issue が前提にした「900×588 の全画面 CentralPanel 背景」は #646 PR2 の 2 窓分割で消滅（main は bar 高固定・`layout.rs` の `main_window_height`）
  3. 単色 fast path は**挙動非保存**である——現行は単色三角形でも重心 `b0+b1+b2`（浮動小数で 1.0 にならない）で色を再構成して `as u8` 切り捨てするため、fast path 化は一部ピクセルの値を黙って変える。`raster.rs` の固定は 2 ピクセルのみでこの差分を検出しない。測定上の必要が無いのに全ピクセルのベースライン差分検証を要する変更は入れない
- **再オープン条件**（issue に 1 行で残す・独立導出の発見）: `visible_rows` / `window_width` に上限が無く（`snotra-core/src/config.rs:989,992-993` は下限・0 のみ検証）、`font_size` は行高にも効く（`layout.rs` の `Metrics::from_config`）。SU6.5 の 3680 フレームは**既定 config 1 点**の測定である。極端 config で `raster_ms` p95 が 16.7ms を超えたら再オープンし、まず `inv_area` の乗算化だけを入れて再測する
- **ロードマップ**: `docs/superpowers/specs/2026-07-21-phase2-softbuffer-migration-roadmap.md:30` の SU1 行 follow-up 表記を「#628（→ 2026-07 実測で決着・項目 1 は不要／項目 2 は \<結論\>）」へ更新
- **`PERFORMANCE.md`**: 計測節（244 行付近「ランタイムの計測は `SNOTRA_TRACE=1`」）へ、egui 計器 env 2 つ（`SNOTRA_EGUI_PAINT_TRACE` / `SNOTRA_EGUI_REPAINT_TRACE`）と本サイクルの可視アイドル実測値を記す。**`docs/build-commands.md` には書かない**（同じ事実を 2 か所に置かない・AGENTS.md「文書に事実の写しを増やす変更は正本を 1 か所に」。perf 計器の正本は `PERFORMANCE.md`）。governance:check は env を検査対象にしない（`scripts/governance-check.mjs` の G5/G9 は npm script と cargo コマンドの照合のみ）ため、この追記は自動検査に守られない記述である
- **PR マージ直前の auto-close 確認（手順として書く。周辺知識に委ねない）**: 分岐 (b)/(c)/(d) では **#628 は open のまま残す**。PR テンプレートが `Closes` を埋めるため、`gh pr view <PR> --json closingIssuesReferences` を**マージ直前に取り直し**、意図しない issue が入っていないことを確認する（入っていれば本文を編集して一覧から消えるまで繰り返す）。マージ後は `gh issue view 628 --json state` が `OPEN` であることも確認する（ルート `CLAUDE.md`「Git/GitHub 運用」の手順 1・2・4）
- **計器の去就**: `SNOTRA_EGUI_REPAINT_TRACE` は残す。理由は `SNOTRA_EGUI_PAINT_TRACE`（SU6.5）・`SNOTRA_EGUI_IME_TRACE` と同じ——env 未設定時のコストが 0 で、次に「なぜ再描画が止まらない」を問うときに再実装が要らない。**恒久計器として扱い**、コメントに #628 の由来を書く

## 不変条件

| # | 不変条件 | 失敗・異常時の挙動 | 検知手段 |
|---|---|---|---|
| 1 | `RedrawRequested` は `on_event` で `WindowEvent` と別 arm のまま（egui 入力へ渡さない） | 渡すと再描画が自己永続する（#579 実測: 15 秒 2,000 フレーム） | Phase 2 の窓別 `frames` 件数（60s で数千行なら即座に露見）。arm 自体は本計画で触らない |
| 2 | hidden 中は paint 0 回（SU6.5 G3(a)・flip 基準の既取得分） | 眠らなければ電力回帰 | Phase 2 条件 3（30s で両 trace が 0 行） |
| 3 | 計器は env 未設定時に一切の追加コストを持たない（`Instant` も `Vec::clone` も `label()` も取らない） | 常時 clone すると計測器が測定対象を汚す | `var_os` ゲートの内側にすべてを置く（コードレビュー）+ `SNOTRA_EGUI_PAINT_TRACE` の `raster_ms` が SU6.5 実測（p95 4.68ms）から悪化しないこと |
| 4 | repaint worker は Drop で `Stop` 送信 → join（外部 `WindowWaker` 保持でも停止する） | 破れると窓破棄でスレッドが残る | `repaint.rs` の既存テスト。本計画は worker に触らない（純関数追加のみ） |
| 5 | `repaint_trace_prev` は状態を持つが**表示にも制御にも影響しない**（trace 専用） | 異常値（NaN・hide をまたぐ巨大値）でもログの数字が乱れるだけ | 初回 `NaN`・hide またぎは巨大値という規約をコメントに明記 |
| 6 | `results` 窓の可視判定（`layout::results_should_show`）と `wake_results` の連鎖は無変更 | 触ると測定対象そのものが変わる | Phase 2 の条件 2 が increment を数字で示す。実測前に触らない |

## テスト方針

- **追加**: `format_repaint_causes` の単体テスト 2 件（空列 → `"-"`、複数件 → `; ` 区切りで `file:line` を含む）
- **既存**: `cargo test -p snotra-egui-runtime`（repaint worker の 2 テストを含む）・`cargo clippy -p snotra-egui-runtime --all-targets -- -D warnings`
- **CI**: `snotra-egui-runtime/**` は `e2e.yml` の paths 対象（`docs/build-commands.md:44`）ゆえ PR で `smoke:startup` / `smoke:egui` が自動起動する。計器は `eprintln!` の別チャンネル（スモークは `[trace] {...}` JSON のみ解析・`scripts/smoke-egui.ps1:176-185`）ゆえ前提を壊さない
- **人手スモーク**: Phase 2（3 条件）。Phase 3-A を採る場合は追加で `cargo run -p snotra` の目視（実線キャレット・入力・IME 変換）
- **非対象**: `raster.rs` は無変更ゆえ既存テストのまま

## SPEC.md 更新要否

- Phase 1・2・4 のみで終わるなら**不要**（挙動不変・トレース追加のみ）
- Phase 3-A（点滅停止）を採るなら**必要**——`SPEC.md` §11（523 行付近・ビジュアル節）へ、キャレット非点滅（IME 変換中も含む）を WebView2 からの意図的な parity gap として明記する

## セルフレビュー

### plan-review（Step 5a）

**Rust runtime 層（Explore）**: 要対処なし。`EguiWindow` の構築点は 1 箇所（`runtime.rs:265`）・`crate::repaint::` パスと `pub(crate)` 可視性・`RepaintCause` の公開性と `Display` 形式・`repaint_causes()` が `&self` で足りること・borrow 衝突なし・env ゲート内にコストが収まることをいずれも一次資料で確認。軽微 2 件（NaN 規約をコード片のコメントへ / 引用行番号の微差）は本版で反映済み。

**src-tauri・文書層（Explore）**: 要対処 2 件をいずれも反映。
- SPEC 追記先の誤り（§4.8:196 はキャレット**位置**の話 → 正しくは §11:523 付近）→ 訂正済み
- Windows 既定 `legacy_visuals = true` ゆえ **IME 変換中キャレットも非点滅になる** → Phase 3-A の副作用として明記
- 軽微: index build 完了待ち・窓識別子・「即時 0 ではなく収束」→ すべて反映済み
- 付随発見（本計画のスコープ外）: `SPEC.md` §11 に WebView2 時代の記述（CSS カスタムプロパティ）が残存

**独立導出（Step 2b・Plan タイプ / plan.md と research.md を読ませず issue とコードだけから再導出）**:

- **漏れ（導出 ∖ plan）— 反映したもの**:
  1. `wake_results`（`view.rs:850`）が結果表示中は毎フレーム無条件 → 「アイドルでは発火しない」という research の前提を条件付きへ訂正し、測定条件を 2 つに分割（**`grep request_repaint` では到達しない同概念・別名**。本レビュー最大の収穫）
  2. trace に窓識別子が無い（2 窓時代）→ `window=` を追加
  3. 測定手順の `auto_hide_on_focus_lost` 退避が抜けていた → Phase 2 準備へ
  4. `predicted_dt` を runtime が書かないため予約は約 16.7ms 早く発火する（`input.rs:29` の `take` を実地確認）→ 読解上の注意へ
  5. 項目 1 を閉じる根拠として「fast path は挙動非保存」「再オープン条件（`visible_rows` / `window_width` に上限なし）」→ Phase 4 へ
  6. 計測 env がどの文書にも無い → `PERFORMANCE.md` へ 1 か所だけ記す（build-commands.md には書かない）
- **反証したもの**: 「点滅停止で blur→hide が壊れる」は成立しない——egui はフォーカスがあるときだけ点滅 repaint を出す（`builder.rs:864-869`）ため、blur 後のフレーム供給に点滅は寄与していない。猶予経路の脆さ自体は実在するが #628 の対象外（別 issue 候補として記録）
- **スコープ過剰（plan ∖ 導出）**: なし。むしろ導出側が挙げた `RedrawRequested` arm への計数追加を、`!visible` ガードが到達不能（`runtime.rs:300-310` のコード内記録）を根拠に**採らない**と判断した（YAGNI・理由を Phase 1 に明記）
- **一致（完全性の証拠）**: 項目 1 不実施の結論と根拠（SU6.5 G3(b) 実測）・項目 2 は「計器 + 実測 + 記録」であり最適化ではないこと・`repaint_causes()` が源特定の本体であること・off-by-one・「1 要求 = 2 フレーム」・触らない対象の集合・issue を close せず受け入れ条件を書き換えること——いずれも独立に再一致した

### 5b の 3 観点

1. **境界条件**:
   - 計器の**初回フレーム**（`repaint_trace_prev` が `None`）→ `since_prev_ms=NaN`（0 と紛れない）
   - **原因列が空**（`prev_causes` 空 = 直前 pass で誰も要求しなかった → 入力イベント起因のフレーム）→ `-` を出す
   - **hide → show をまたぐ `since_prev_ms`** → 非表示中の空白が巨大値として出る（正しい。眠っていた証拠になる）
   - **1 パス複数原因** → `; ` 区切りで全件出す（先頭だけ出すと源を取り違える）
   - **多パス（multipass）フレーム** → `repaint_causes()` は最後の pass の 1 つ前を返す。定常アイドルの判別には影響しないが件数照合は厳密にしない
   - **2 窓が同時に吐く** → `window=` で分離（条件 2 の判定はこれに依存する）
2. **シンプル化の挑戦**: 新しい状態は `Option<Instant>` 1 つ・trace 専用・読み手 1 か所。**「計器なしで仮説（キャレット点滅）を実装して様子を見る」は採らない**——観測 200-300ms と点滅の 500ms が合わず、源が別にある可能性が残る。`RepaintCause` を scheduler まで運ぶ拡張も採らない（callback は cause を運ばず、運ばせるには egui の型を跨ぐ改造が要る・YAGNI）
3. **破壊不変条件 + 検知手段**: 上表 1〜6。#579 の自己永続ループ（不変条件 1）は**この計器自身が最良の検知器**になる——アイドル 60s のフレーム数がそのまま指標である。Win32 フック・ホットキー・IPC には触れない
