# 実装計画: #999 — results 窓の表示後に注入した打鍵が届かなくなる（調査）

一次調査は `workspace/research.md`、敵対的レビューの原文は `workspace/adversarial-999.txt`。

## 目的

**#999 は判定を求めており、修正を求めていない。** 製品の欠陥だった場合は別 issue を立てると issue 自身が定めている。
この計画は**測って結論を出すまで**を所有する。製品コード（`.rs`）は 1 行も変更しない。

## 受け入れ条件

1. **層の特定**: `research.md` の判定表のどの行に当たったかが、生ログの逐語引用つきで確定している
   （`take` の `ts_ms` 階差 / `rx_key` の有無 / `drop_key` の有無 / `SNOTRA_SMOKE_INJECT` との突き合わせ）
2. **契機の特定**: 1 の層に応じた沈黙の契機が名指しされている（できなかった場合は、**何を測れば決まるか**が残る）
3. **既存検証への影響の判定**: `smoke:egui` / `check:colors` が同じ条件へ入りうるかに答えが付いている
4. **帰属の結論**: 製品の欠陥か注入経路固有かが、根拠つきで書かれている
5. 上の 4 点が issue へ貼れる 1 枚（`workspace/conclusion-999.md`）にまとまっている

## 変更ファイル一覧と対象シンボル

| ファイル | 種別 | 内容 |
|---|---|---|
| `workspace/repro-999.ps1` | 新規（足場） | 再現ハーネス。`SnotraSmoke.psm1` を import して使う |
| `workspace/evidence-999/` | 新規（生ログ） | 実行ごとの stderr / ホスト出力 / 突き合わせ表 |
| `workspace/research.md` | 更新 | 判定表のどの行に当たったか、契機、⚠️ 付きの残余 |
| `workspace/conclusion-999.md` | 新規 | issue へ貼る 4 点の結論（**貼るのは別の指示を待つ**） |
| `workspace/plan.md` | 更新 | 判定が出た時点で、消えた枝を削除して書き換える |

**測定フェーズ（Phase 1〜2）では、製品コード（`snotra-egui-runtime/` / `src-tauri/` / `snotra-core/`）と
`scripts/` を触らない。** 計器は既に在り（`SNOTRA_EGUI_INPUT_TRACE`）、追加の計装は要らない。

**Phase 3 の「結論の着地先」が repo のファイルになる場合は、その時点で上の表へ追加し、
リスク判定をやり直す**（下の「セルフレビュー」の通常判定は測定フェーズの射程で立てている）。
着地が `workspace/conclusion-999.md` と新規 issue で閉じるなら、表も判定もそのままでよい。

### 足場の撤去条件（AGENTS.md「一時的な足場」トリガー）

`workspace/repro-999.ps1` は **`workspace/` に置く**。`/retrospective` のサイクル終了処理が
`workspace/` ごと撤去する既存の機構に載るため、**足場自身が撤去の合図を持つ必要がない**
（`scripts/` へ置くと「#999 が閉じたら消す」という自己参照の撤去条件になり、閉じるのが当の PR のとき発火しない
——`scaffold-removal-condition-self-reference`）。この理由を `.SYNOPSIS` の 1 行として script 自身に書く。

## 実装順序

### Phase 1 — ハーネスを書く

`workspace/repro-999.ps1`。`scripts/lib/SnotraSmoke.psm1` の既存関数だけで組む（写しを作らない）。

- `Assert-SnotraSessionUnlocked`（画面ロック中は窓が描かれない・#866）
- `Resolve-SnotraCargoExecutable -Profile release`（#996 は release で測った）。
  **`release` は小文字で渡す**——`$Profile` は `Join-Path <target_dir> "$Profile/snotra.exe"` の
  パス片としてそのまま使われる（既定は `'debug'`・実読）
- プロファイル用意: **実 config ディレクトリを丸ごと複製し、`auto_update = "disabled"` へ倒す**（D-2）。
  **窓の高さに効くキーを `evidence-999/<run>/profile.txt` へ書き出してから起動する**
- `Start-SnotraProcess -Trace -ExtraVariables @{ SNOTRA_EGUI_INPUT_TRACE = '1' } -StandardErrorPath <log>`
  **両系統を同時に立てる**（`research.md` 制約 2b）
- `Set-SnotraForegroundWindow` → hotkey → 1 文字クエリ → `Wait-SnotraTraceEvent "egui_results:show"`
- 直後に Down×N → Escape。**#996 の測定スクリプトの形（表示直後に本番操作へ入る）を再現する**
- 生ログとホスト側出力の両方を `workspace/evidence-999/<run>/` へ落とす。
  注入時刻は `Send-SnotraKey ... 6>> $injectLog`（情報ストリーム・D-3 で実測済み）
- `finally` で `Stop-SnotraProcessAndWait`

**判定を script に持たせない。** 出すのは生ログと突き合わせ表であり、読むのは人間である
（`visual-input-metrics.ps1` と同じ流儀——合否を出さない道具）。

### Phase 2 — 測る

- 計器 **OFF → ON** を 1 組として**交互に**回す（規則は D-1 で確定済み）。
  OFF 側は「#996 の 6/6 が今も再現するか」の対照であり、**ON 側だけ回して 0/N なら結論が出ない**
  （`ab-baseline-needs-drift-control` / `PERFORMANCE.md:2721`「率を測る回と機序を測る回は別の回に」）
- **`SNOTRA_TRACE`（`-Trace`）は OFF の回も常時立てる。** トグルするのは `SNOTRA_EGUI_INPUT_TRACE` だけである
  ——両方切ると OFF 回が盲目になり、`egui_results:show` で止まったことすら見えない
- 各実行の生ログを `evidence-999/` へ残す。**測る前に、いま持っている分を出力先へ書く**

### Phase 3 — 判定して書く

- 判定表のどの行に当たったかを `research.md` へ記録し、**そこで消えた仮説を削除する**
- 受け入れ条件 3（既存検証への影響）に答える。`research.md` の静的所見（`smoke:egui` は
  results 表示後の打鍵条件へ**既に入っている**）に、機序が規模依存かどうかの判断を足す
- `workspace/conclusion-999.md` に 4 点を書く

## 不変条件と異常系

- **実 config を書き換えない。** `Start-SnotraProcess` の予約名ガード（`SNOTRA_CONFIG_DIR` は
  `-ConfigDir` 経由のみ）に必ず載せる。`-ExtraVariables` へ `SNOTRA_CONFIG_DIR` を渡すと throw される（実装済み）
- **`SNOTRA_EGUI_INPUT_TRACE` に空文字を渡さない。** 空文字は未設定として扱われる（`env.rs`・ADR）。
  `Invoke-SnotraEnvironment` の復元は `Remove-Item` なので漏れないが、**明示的な `'1'` を渡す**
- **強制終了は `flush_persistent_state` を飛ばす。** #996 で実際に起きた。異常系でも
  `Stop-SnotraProcessAndWait` を通し、飛ばした回はその旨をログに残す
- **`take` の `frame=` は窓ごとの独立カウンタである。** 非単調を「巻き戻り」と読まない（`research.md`）
- **計器つきの実行を「率」の証拠に使わない。** 分類と機序にだけ使う

## テスト方針と検証コマンド

製品コードを変えないため cargo 系の検証は不要。触るのは `workspace/` のみ。

- `npm run governance:check` — `workspace/` は `*.md` を含むため念のため 1 回
- ハーネス自体の接地: **最初の 1 回は「成功する形」で回す**（`egui_hide:done` まで到達する経路を
  一度観測してから本番条件へ入る）。**観測できないことと、ハーネスが壊れていることを区別できないため**
  （`run-new-verification-path-before-reporting`）

## SPEC.md・関連文書の更新要否

- **`SPEC.md`: 不要。** 挙動を変えない
- **`PERFORMANCE.md`: 不要。** 性能値を新たに主張しない
- **結論の着地先は 1 か所に決める**（Phase 3 の作業項目）。候補は
  `docs/superpowers/specs/2026-08-05-caret-test-mechanism-design.md` の **U-B**（フレーム不回転の機序・射程外と宣言済み）、
  `scripts/lib/SnotraSmoke.psm1` の `Send-SnotraKey` の doc（注入経路固有だった場合）、
  新規 issue（製品の欠陥だった場合）。**どれになるかは判定次第だが、「1 か所へ書く」こと自体は必ず行う**

## 作業項目

### Phase 1 — ハーネス

- [ ] `workspace/repro-999.ps1` を書く（既存の `SnotraSmoke.psm1` 関数のみ・判定を持たせない・撤去理由を `.SYNOPSIS` へ）
- [ ] ハーネスを「成功する形」で 1 回回し、`egui_hide:done` まで到達することを確認する（接地）

### Phase 2 — 測定

- [ ] 計器 OFF / ON を交互に回し、各回の生ログを `workspace/evidence-999/<run>/` へ残す
- [ ] `SNOTRA_SMOKE_INJECT` と `rx_key` / `drop_key` / `take` を 1 本の時系列へ並べた突き合わせ表を作る
- [ ] `take` の `ts_ms` 階差を出し、沈黙区間の直前〜最中に 100ms を大きく超える間隔が集中するかを見る

### Phase 3 — 判定

- [ ] 判定表のどの行に当たったかを `workspace/research.md` へ記録し、消えた仮説を削除する
- [ ] `smoke:egui` / `check:colors` が同じ条件へ入りうるかに答えを付ける（規模依存かどうかの判断を含む）
- [ ] 製品の欠陥か注入経路固有かの帰属を、根拠つきで書く
- [ ] `workspace/conclusion-999.md`（issue へ貼る 4 点）を書く
- [ ] 結論の着地先を 1 か所決め、そこへ書く
- [ ] `workspace/plan.md` を判定後の形へ書き換える（消えた枝を削除する）

## 未確定（実装前に潰す）

- [x] **D-1: 計器 ON/OFF の割り付けと反復の規則** — 決定: **OFF → ON を 1 組として交互に回す**
  （まとめて OFF を先に回す形は採らない——ドリフトが A/B の差へ紛れる）。**`SNOTRA_TRACE` は両方の回で常時 ON**、
  トグルするのは `SNOTRA_EGUI_INPUT_TRACE` だけ。
  **事前に確定させる読み**: ON 側が 0/N になったら、それは「直った」ではなく
  **「タイミング依存である」という所見**であり、H3′（重いフレーム）を支持する証拠に数える
  （`env.rs` の doc・caret spec が「計器は率だけでなく喪失の現れ方も変える」と定める）。
  OFF 側でも再現しなかった場合は、環境差（D-2 の逸脱を先に戻す）を疑う
- [x] **D-2: 実索引 313,028 件の用意** — 決定: **(a) 実 config ディレクトリを丸ごと複製する**
  （`index.bin` ごと持ち込む＝ #996 の条件への忠実さを基準に採る）。実 config は**読むだけ**で、
  `-ConfigDir` は複製先を指す。件数・`rows` が #996 の値に届くかは Phase 1 の接地断言で確かめる。
  **既知の逸脱 1 件**: 複製後の `config.toml` の `auto_update` を `disabled` へ倒す
  （ユーザー判断 2026-08-26: 「計測の目的を考えると高さを変える設定は明示的にしたほうがいい。disable にしよう」）。
  実チェックはネットワーク依存の雑音を持ち込むうえ、**toast は窓の高さを変え**、
  **toast 窓それ自体が focus 事象の発生源**であり、H1（`held_since_focus_gain`）の検定を汚す。
  **OFF 側でも再現しなかったときは、まずこの 1 行を戻す**

  **同じ理由で、複製した config のうち窓の高さに効くキーは暗黙にしない**——実測に入る前に
  `evidence-999/<run>/profile.txt` へ値を書き出す（`auto_update` / `[appearance]` の
  `window_width`・`show_icons`・フォント関係）。**丸ごと複製は「何が効いているか」を隠す形なので、
  高さを動かす入力だけは明示的に読める形で残す。**
- [x] **D-3: `SNOTRA_SMOKE_INJECT` の捕まえ方** — 決定: **`Send-SnotraKey` の呼び出し点で
  情報ストリームを追記リダイレクトする**（`Send-SnotraKey -VirtualKey $vk 6>> $injectLog`）。
  **実測済み**（2026-08-26・pwsh 7・`-NoProfile`）: `& { Write-Host 'X' } 6>> f` はファイルへ落ち、
  `pwsh -NoProfile -Command "Write-Host 'X'" > g` も落ちた。**`Start-Transcript` は要らない**し、
  `Send-SnotraKey` の写しも作らずに済む（打鍵の実装は 1 か所のままである）
- [x] **D-4: 「再現しない」と言うまでの上限** — 決定: **規則を先に固定する**（数値は Phase 2 の出力）。
  OFF 側が m 回中 k 回再現したら、片側 95% の Clopper–Pearson 下限 `p_lo` を取り、
  **`N = ceil( ln(0.05) / ln(1 - p_lo) )`** 回だけ ON を回す。
  `k = m` のときは `p_lo = 0.05^(1/m)` で、**#996 と同じ 6/6 なら `p_lo ≈ 0.607` → `N = 4`**。
  **総試行は 20 組を上限とし、達しても再現しなければそれ自体が結論である**
  （「#996 の条件は現在の main では再現しない」）

## 人間レビュー

- [x] 承認済み — 2026-08-26 / 問い: "`workspace/plan.md` の承認をいただければ Step 6（workspace のコミット＆プッシュ）へ進みます — 他に注釈があれば先に反映します。" / 回答: "OK"
- 先行する注釈 1 件（D-2 へ反映済み） — 問い: "D-2 の `auto_update` を `disabled` へ倒す判断（#996 の条件からの意図的な逸脱）についてのご意向もいただけると助かります。" / 回答: "計測の目的を考えると高さを変える設定は明示的にしたほうがいいと思う。disalbeにしよう"

## セルフレビュー

- リスク: **通常**
  - 永続形式・並行性・状態遷移・ガバナンス文書・網羅性のいずれにも触れない（製品コード 0 行・`scripts/` 0 行）
  - 該当する `AGENTS.md` 条件別チェックは「一時的な足場の新設」のみで、撤去条件は上に書いた
  - `/persistence-check` `/race-check` `/state-check` `/symmetric-check` `/dry-check`: いずれも非該当
- plan-review: **未実施（通常リスク）／自己レビューのみ**
- エージェント数: **1**（Step 3b の敵対的調査 1 体・sonnet）
- 要対処: **3 件**（すべて `research.md` へ反映済み）
  1. H1 の根拠をタイミング論から構造的理由へ置換 → `research.md` H1 節
  2. 判定表の心拍二値 → `ts_ms` 階差へ変更し H3′ を新設 → `research.md` 判定表・H3′ 節
  3. `SNOTRA_TRACE` との二重要件 → `research.md` 制約 2b
- 未検証: **`load_icon_pngs` の内部実装と `Wait-SnotraTraceEvent` 本体**（敵対枠が未検証と申告）。
  前者は H3′ の定量化に要るが、**静的読解では定量化できない**と分かっているので Phase 2 の実測へ送る。
  後者は既存の smoke で日常的に使われている経路であり、この計画では新しい使い方をしない

### 自己照合（5 点）

1. **issue の全要件に作業項目が対応する** — issue の 4 点 ↔ 受け入れ条件 1〜4 ↔ Phase 2/3 の作業項目
2. **境界条件と検証** — 計器 ON/OFF（D-1）・索引規模（D-2）・ストリーム分離（D-3）・上限（D-4）を未確定欄で潰す
3. **新しい状態・リソース・プロセスの正常/失敗/破棄経路** — 起動したプロセスは `finally` の
   `Stop-SnotraProcessAndWait`。足場の撤去は `workspace/` ごと（上記）
4. **より単純な既存パターンで置き換えられないか** — 置き換えた。**計器の新設は不要**
   （`SNOTRA_EGUI_INPUT_TRACE` が既に 3 層を出す）。`scripts/` への新規スクリプトも不要
   （`workspace/` に置けば撤去条件の自己参照を避けられる）
5. **壊してはならない不変条件に検知手段がある** — 実 config を書き換えない ← `Start-SnotraProcess` の
   予約名ガード（throw する既存機構）。ハーネスが壊れていることと現象を取り違えない ← 接地の 1 回
