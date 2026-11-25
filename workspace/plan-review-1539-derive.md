# #1539 の独立導出（plan-review 用）

- 導出の基準: worktree `issue-1529-c319d1`・ブランチ `fix/trace-single-write`・HEAD `40470781`（会話開始時点の snapshot）。
- `workspace/` 配下（plan.md・research.md・adversarial-1539.txt）は読んでいない。検索は具体ディレクトリ／ファイル、`git grep` は `':!workspace'` 付きで行った。計画・調査の内容は目にしていない。
- 行番号は HEAD 時点。

## 1. 変更が要るファイルとシンボル（受け入れ条件ごと）

### AC1: trace 1 行の書き込みが stderr への 1 回の書き込みになる

- `src-tauri/src/trace.rs:66` `trace()` 末尾の `eprintln!("[trace] {line}");` が唯一の出口。
  `main.rs:51` `trace_main` と `commands/mod.rs:33` `trace_command` は委譲するだけの薄い wrapper で、直接の呼び出し（`egui_shell/view.rs:1323` の `crate::trace::trace("egui_frame", …)` など）も同じ関数を通る。**直す場所は 1 か所**。
  - 形: `let s = format!("[trace] {line}\n");`（メモリ上で組むので `write_str` が何回でも費用は無い）→ `std::io::stderr().write_all(s.as_bytes())`（`Stderr::write_all` は内部で lock を取り、1 回の `write_all` 呼び出しにする）。
  - テストのために「組む」と「書く」を分ける: 例 `fn emit_line(w: &mut impl Write, line: &serde_json::Value) -> io::Result<()>`（または `format_line(&Value) -> String` + 書き込み）。`trace()` はそれを `stderr()` に対して呼ぶ。
- `src-tauri/src/trace.rs:1-15` の `//!`: 「JSON formatting」と書いた責務に「1 行を先に組み、1 回で書く」を足す（理由は数値ではなく機序: `Stderr` が無バッファで serde_json の `Display` が字句ごとに書くこと。額は `PERFORMANCE.md` を指す）。
- `src-tauri/src/trace.rs:40-43` `trace()` の doc: 「Emit one `[trace]` JSON line to stderr」に「1 回の書き込みで」を足す程度。
- 書き込み失敗の扱いを決める（⚠️ 下記 3. 不変条件 d）。

### AC2: 計測ハーネスと同じリダイレクト条件で、1 行の所要を前後で測って記録する

- 条件の正本: `scripts/lib/SnotraSmoke.psm1:332` `Start-SnotraProcess`（`:369` で `RedirectStandardError`）。`scripts/bench-startup.ps1:134-138` も同じ形で stderr をファイルへ。
- 測り方の候補（どれもコミットしない一時の計装）:
  1. `trace()` の書き込みを `Instant` で計時し、次の行（または同じ行の別欄）へ添える版を A（eprintln）/B（1 回書き）で作り、`Start-SnotraProcess -StandardErrorPath` で起動、ホットキー注入で既知の行（`egui_show:*`・`hotkey:listener_enter`）を数十本出して p50/p90/max を取る（#1526 の「2i」と同じ手法・`PERFORMANCE.md:720-770`）。
  2. 製品側の観測で裏を取る: #1526 の交互測定（2i 相当）を再現して、前面化 trace を区間に戻した版の `ms` − 直した版の `ms` が縮むか。
  3. issue 本文の小さな Rust probe は「同じ形の別プログラム」であって製品ではない（AGENTS.md「主張は代理ではなく対象そのもので測ってから書く」）。製品バイナリでの測定を最低 1 つ置くべき。
- 交互に並べる・同日同機体・機体名（HAL9000 など）・release・標本数を書く（`PERFORMANCE.md:31-45`「この文書へ記録するときの規約」、`PERFORMANCE.md:3698-3700`「数 % の差を版の A/B で読むときは…」の交互順）。
- 記録先: `PERFORMANCE.md` に新しい日付つきの節（例「採用: trace 1 行を 1 回で書く（#1539）」）。置き場所は #1526 の節（`PERFORMANCE.md:720`）の近くが自然。計装を撤去する前に値を着地させる（同規約）。
- cmd の `2>file` 条件は「ハーネスと同じ条件」ではないので、載せるなら補助扱い。

### AC3: trace の所要を前提にした `PERFORMANCE.md` の記述を直す

主対象は `PERFORMANCE.md`「計測と受け入れ基準」の trace の箇条（`:3662-3684`）。詳細は 2. の全件表。

## 2. 直した後に偽になる現在形の記述（全件）

検索語: `10 ?ms`・`ミリ秒`・`1 行あたり`・`1 行の書き込み`・`書き込みの所要`・`eprintln`・`trace 2 本`・`trace を書く`・`trace の書き込み`・`trace の所要`・`trace 1 行`・`trace を吐`・`trace の費用`・`書く間`・`stderr への 1 行`・`stderr 1 行`・`汚染`・`受け手の速さ`・`trace を何本`・`二峰`・`10〜18 ms`・`update_us`・`egui_frame`。
範囲: `PERFORMANCE.md`・`SPEC.md`・`docs/architecture.md`・`docs/build-commands.md`・`docs/development-principles.md`・`docs/glossary.md`・`src-tauri`・`snotra-egui-runtime`・`snotra-core`・`scripts`・`.claude`・`AGENTS.md`・`CLAUDE.md`（`docs/superpowers/` は除いた。下記 2c）。

### 2a. 直す（現在形で、新しい値で偽になる／指す先が消える）

| 位置 | 記述 | 理由 |
|---|---|---|
| `PERFORMANCE.md:3662-3664` | 「**フレームループ中の trace は 1 行あたり約 10 ms かかる。**フレーム時間を trace で測るなら、絶対値ではなく A/B の差分で読む」 | 現在形の費用の主張。issue の測定（ハーネス条件で p50 2.5 ms → 0.12 ms）が正しければ 1〜2 桁偽になる。日付つき（#1004/#1032・2026-08-11）の観察は「当時の版で」と限定して残し、現在の額は新しい測定へ差し替える |
| `PERFORMANCE.md:3664-3669` | #1004 の 10〜18 ms 観察・#1032 の 50〜96% 控除 | 日付つきの過去の観察なので本文は残してよいが、「別々の測り方が同じ規模を出している」が現在の費用の根拠として読まれる形になっている。「1 回書きにする前（〜#1539）の版で」と時制を限定する |
| `PERFORMANCE.md:3669-3670` | 「`update_us` のような量は…**『そのフレームで trace を何本吐いたか』を強く反映する**。**60fps の予算 16,700 µs は trace 2 本で超える**」 | 現在形の帰結。0.12 ms/行なら 2 本で 16.7 ms は超えない。偽になる |
| `PERFORMANCE.md:3671-3674` | 「絶対値では合否を判定できない——汚染は A/B の両側へ等しく乗る」 | 汚染の規模が変わる。「合否を判定できない」が今も言えるかは新しい測定次第（⚠️ 揺れ・最大値の裾が残るなら残してよい） |
| `PERFORMANCE.md:3675-3680`（未調停の反例） | 起動段の ON/OFF A/B で差が雑音だったことと「上の規模」の不一致 | 「上の規模」が変わるので、反例の位置づけが変わる（調停される可能性がある）。少なくとも「上の規模なら見えるはず」の前提を書き直す |
| `PERFORMANCE.md:3678-3680` | 「区間ごとに trace を吐かない。…素直に 3 本吐けば、測定行為そのものが **30 ms 級**を足して答えを歪める」 | 30 ms 級は 10 ms/行 × 3 の帰結。額が偽になる。規則（区間ごとに吐かない）自体は揺れ・裾の点で残る余地がある |
| `PERFORMANCE.md:555-557` | 「`update_us` には trace の書き込みが含まれる。絶対値を実挙動と読んではならない…費用とその帰結の正本は『計測と受け入れ基準』」 | 含まれること自体は真のまま。読み方の強さは正本に追随するので、正本を直せば足りる可能性が高い（⚠️ 「絶対値を実挙動と読んではならない」を残すかは測定次第） |
| `PERFORMANCE.md:735-736` | 「`listener_enter` の書き込みは約 2 ms と約 10 ms の二峰に分かれ、『計測と受け入れ基準』の『1 行あたり約 10 ms』は上の峰に当たる」 | 前半は 2026-11-25 の日付つき測定で残す。後半は**引用先の文言**を指しており、3662 を直すと「存在しない文言へ読者を送る」ので直す（`.claude/rules/governance-docs.md`「条項を足す・消す・移すとき」の判定） |
| `PERFORMANCE.md:768` | 「回ごとに揺れる原因（trace 1 行の書き込みが回によって約 2〜10 ms で変わることとの関係）は確かめていない」 | 節は日付つき記録だが「確かめていない」は現在の状態の主張。#1539 が出どころ（字句ごとの書き込み）を突き止めたなら、後日注記を添えるのが筋（⚠️ #1539 は「揺れの出どころ」を代理 probe で示しただけで、製品の `ms` の揺れが消えたかは AC2 の測定次第） |
| `PERFORMANCE.md:3730-3734` | 「`PERFORMANCE.md`『計測と受け入れ基準』が記す 1 行あたりの費用との突き合わせは未調停である」 | 2026-08-09 の節の中だが「未調停」は現在形の状態。費用の記述を直すと参照先の内容が変わる。最小限なら現状維持でも偽にはならない（指す先の節は残る）が、3675 の反例と対で調停するなら直す（⚠️ 判断） |
| `src-tauri/src/egui_shell/window_coordinator.rs:511-512` | 「前に書くと、trace 1 行の書き込み（**ミリ秒単位**）が `ms` の区間に入る」 | 0.12 ms（ハーネス条件）・0.014 ms（cmd）なら「ミリ秒単位」は偽。規則（`egui_show:done` の後に書く）は依然正しい（揺れ・裾を区間に入れない）。額の語を落とし、`PERFORMANCE.md` を指す形へ |
| `src-tauri/src/main.rs:143-145` `ForegroundOutcome` の doc | 同上「trace 1 行の書き込み（**ミリ秒単位**）」 | 同上 |
| `src-tauri/src/main.rs:566-568` `setup_hotkey_listener` 内の `t0` のコメント | 同上「trace 1 行の書き込み（**ミリ秒単位**）」 | 同上 |
| `src-tauri/src/trace.rs:13-15` `//!` | 「Using trace as an instrument has caveats — how much time sits between two lines…」 | 額は書いておらず偽にはならない。正本を指す形なので、正本を直せば足りる。新しく足す機序の一文だけ追加 |

### 2b. 直さない（真のまま、または過去の記録）

| 位置 | 理由 |
|---|---|
| `PERFORMANCE.md:615-620`（#1032 節「`update_us` の 50〜96% は trace の書き込みであり、本番では 0 になる」） | 2026-08-11 の日付・条件つきの測定記録。当時の版について真。`PERFORMANCE.md`「この文書へ記録するときの規約」は時系列の採否ログとして扱う（同規約の最終段落）。書き換えると測定記録の顔をした推測が残る |
| `PERFORMANCE.md:720-770` の表・値（#1526・2026-11-25） | 日付・機体・条件つきの記録。前の版（eprintln）での値として真。ただし 735-736 の後半（引用先の文言）と 768 の「確かめていない」は 2a |
| `PERFORMANCE.md:3746`・`:3873-3893`（#1454・runner の hotkey_register が trace の書き込みだった） | 2026-11-16/17 の日付つき記録。過去の版で真。#1454 以後、区間は trace を含まない（`startup.rs:574-579`） |
| `src-tauri/src/startup.rs:574-579` `finish_at` の doc（「入れると区間が stderr の受け手の速さで動く」） | 額を書いていない。0.12 ms でも受け手の速さに依存するのは真。規則は残す |
| `src-tauri/src/platform/hotkey.rs:160-161` `Registration`（「時刻は trace を書く前に取る」） | 同上。額なし |
| `PERFORMANCE.md:3691`・`snotra-egui-runtime/src/input.rs:22-24, 98`（「runner では stderr 1 行が 17〜56ms」） | **別の出口**（`snotra-egui-runtime/src/input.rs:68` の `eprintln!("SNOTRA_EGUI_INPUT …")`）の runner 実測。#1539 の変更は `trace.rs` だけなので触れない。ただし同じ形（`eprintln!` に複数の `{}` → 複数回の書き込み）なので、issue の「確かめていないこと」2 点目に当たる（⚠️ 範囲外として PR 本文か別 issue へ） |
| `PERFORMANCE.md:3954-3964`「ホットキー表示レイテンシ（…2026-03-07）」（「~41ms はトレース I/O オーバーヘッド」） | 日付つきの過去の計測。現存しない経路（`show_main_and_emit`）の当時の値 |
| `docs/superpowers/plans/2026-07-25-…md:861`・`docs/superpowers/specs/2026-08-10-search-worker-design.md` | 書いた後は更新しない設計記録（AGENTS.md「ドキュメント参照」の `docs/design/` と同じ扱い）。行形式の記述は変わらないので偽にもならない |
| `src-tauri/CLAUDE.md:16`（`trace.rs` の責務） | 責務は変わらない |

### 2c. 検索の射程外として残ったもの

- `docs/superpowers/` は除外した（凍結記録）。中に「10 ms」系の主張があっても直す対象ではない、という前提に依っている。
- GitHub issue / PR 本文（#1004・#1032・#1526）は見ていない。

## 3. 出力形式（`[trace] ` + JSON + 改行）の消費者と不変条件

消費者:
- `scripts/lib/SnotraSmoke.psm1:708-722` `ConvertFrom-SnotraTraceLine`: `'^\[trace\]\s+(.+)$'` で行を拾い `ConvertFrom-Json`。parse 失敗は「書き込み途中の末尾行」として捨てる。
- `scripts/lib/SnotraSmoke.psm1:728-778` `Read-SnotraTraceSnapshot`: `^\[trace\]\s+` の行数 − parse 成功数 = Dropped。`Get-Content` で行単位に読む。
- `scripts/lib/SnotraSmoke.psm1:683` `Read-SnotraTraceEvents`、`Wait-SnotraTraceCondition`（`smoke-startup.ps1:86-97` が「最初の `[trace]` 行」を待つ）。
- `scripts/lib/SnotraTraceInvariants.psm1:118-125`（`-DroppedLineCount` の意味は「`[trace]` で始まるのに parse できなかった行」）。
- テスト fixture: `scripts/lib/SnotraSmoke.Tests.ps1:536-552`、`scripts/lib/SnotraTraceInvariants.Tests.ps1:481-496`（キーはソート順 `data,event,seq,ts_ms`）。
- `scripts/bench-startup.ps1`（stderr ファイルを読む）、`scripts/smoke-egui.ps1` ほか smoke 一式。
- `snotra-egui-runtime/src/input.rs:50-54`: `[trace]` 接頭辞を使わない理由として、`[trace]` 行は必ず `seq` を持つこと・`ts_ms` が同じ時計であることを前提にしている。
- `src-tauri/src/heap_trace.rs:15, 38`: 出力先は既存の `[trace]` 行の `heap` 欄。

壊してはならない不変条件:
- a. **接頭辞はバイト列 `[trace] `**（角括弧 + 空白 1 つ）。正規表現は `\s+` なので空白は 1 つ以上で通るが、`input.rs` 側の区別と smoke の待ちが接頭辞に依存する。
- b. **1 行 = 1 イベント・末尾は `\n` 1 つ**。`eprintln!` は `\n`（`\r\n` ではない）。`format!("…\n")` で同じにする。`writeln!` 相当を残すなら同じ。`Get-Content` は `\n` でも `\r\n` でも割るので CRLF にしても読めるが、変える理由は無い。
- c. **JSON 本文の形は変えない**: `{line}` は `serde_json::Value` の `Display`（compact・`to_string` と同じ出力）。キー順は Map の実装（既定 `BTreeMap` でソート）で決まり、書き方の変更では動かない。`seq`・`ts_ms`・`event`・`data`・任意の `heap` 欄（`trace.rs:54-65`）は不変。
- d. **書き込みの失敗**: `eprintln!` は stderr への書き込みが失敗すると panic する（`std` の `print_to` の挙動。無効ハンドルは sink 扱いで失敗しない）。release は `windows_subsystem = "windows"`（`main.rs:8`）なので、リダイレクトなしでは stderr は無効ハンドル → sink。`write_all` に変えると、`let _ =` で捨てるか `expect` で panic を残すかを決める必要がある。trace は診断専用（`trace.rs:9-11`）なので、失敗で製品を落とさない（捨てる）ほうが筋だが、挙動の変更になる（⚠️ 決める点として計画に書くべき）。
- e. **スレッド間で行が混ざらない**: `trace()` は UI スレッド・platform スレッド・worker・Tauri listener から呼ばれる。`eprintln!` は 1 呼び出しの間 `Stderr` の再入 lock を握るので、行の途中に他スレッドの行は入らない。新しい形も「1 回の `write_all`」（`Stderr::write_all` が lock を取る）か、明示の `stderr().lock()` の下で書くこと。接頭辞と本文を 2 回に分けて書くと混ざる。
- f. `seq` は `fetch_add` で採番してから書くので、出力順と `seq` 順は今もスレッド間で一致を保証しない（消費者は `seq` で整列する・`SnotraTraceInvariants.psm1:118`）。変更で悪化も改善もしない。
- g. `SNOTRA_TRACE` 無効時は何も組まない（`trace.rs:45-47` の早期 return を `format!` より前に保つ）。無効時の費用 0 は `PERFORMANCE.md:3685`（計測コードのコスト 0 の記述は egui 側の env についてだが、trace も同じ前提で使われている）。
- h. 書き込み途中の末尾行を捨てる消費者の寛容（`SnotraSmoke.psm1:719-721`）は残る。1 回書きにすると、ファイルでは途中行が見える窓が狭まる方向にしか動かない。

## 4. AC1 をテストでどう縛るか・その検査が見ていないもの

縛り方:
- 「組む」と「書く」を分けたヘルパ（`emit_line<W: Write>(w, &Value)`）を `trace()` が使い、`#[cfg(test)]` に `write` の呼び出し回数を数える `Write` 実装（`write` を数え、`write_all` は既定実装のまま＝`write` が全量を受け取れば 1 回）を置いて、**呼び出し 1 回**・**出力が `[trace] ` で始まり `\n` 1 つで終わる**・**本文が `serde_json::from_str` で元の `Value` に戻る**を検査する。
- 変異で確かめる: ヘルパを `writeln!(w, "[trace] {line}")`（＝今の `eprintln!` と同じ形）に戻すと回数が数十になって落ちること、接頭辞を別書きにすると 2 回になって落ちること。
- 既存の `SnotraSmoke.Tests.ps1` / `SnotraTraceInvariants.Tests.ps1` は行形式の消費側を縛る（Rust 側の形を変えないことの裏）。

見ていないもの:
- **配線**: `trace()` が本当にそのヘルパを `stderr()` に対して呼んでいるか（別経路で `eprintln!` を残しても単体テストは緑）。`trace()` は `OnceLock` の env と global の `seq` を持つので単体テストに乗せにくい。補うなら smoke（実プロセス）での行形式の検査か、`trace.rs` の目視レビュー。
- **OS レベルの回数**: `Stderr::write_all` が Windows で `WriteFile` 1 回になるかは std の実装（ファイル・パイプは 1 回、コンソールは UTF-16 変換して分割しうる）。テストの `Write` 実装は std の `Stderr` ではない。
- **所要**: 速くなったかは AC2 の測定でしか見えない。
- **他の出口**: `snotra-egui-runtime` の `SNOTRA_EGUI_*` 計装（`input.rs:68`・`runtime.rs`・`repaint.rs:198`・`windows_ime.rs` などの `eprintln!`）と `[config]`・`[history]` 等の診断行は同じ「複数片の書き込み」の形のまま。テストは `trace.rs` だけを見る。
- **スレッド間の混線**（不変条件 e）: 単スレッドの数え上げでは出ない。
- **失敗時の挙動**（不変条件 d）: panic するか捨てるかは別のテストが要る（失敗する `Write` を渡す）。

## ⚠️ 確信の持てない所見

1. 不変条件 d（`eprintln!` の panic → `write_all` の失敗をどう扱うか）は挙動の変更になる。計画で明示的に決めるべき。std の「無効ハンドルは sink」挙動は記憶に基づく（`std/src/io/stdio.rs` の `handle_ebadf`）。ソースで確かめていない。
2. 2a の `PERFORMANCE.md:3671-3680`（絶対値で判定できない・区間ごとに吐かない・未調停の反例）を「直す」か「額だけ直して規則は残す」かは、AC2 の新しい測定の裾（max）次第。issue の probe で最大 14〜16 ms は eprintln 側の値で、1 回書き側の max は issue に書かれていない。
3. `PERFORMANCE.md:768`（揺れの原因は確かめていない）と `:3730-3734`（未調停）を後日注記で更新するかは判断。偽になるかどうかは新しい測定で揺れが消えるか次第。
4. AC2 の「計測ハーネスと同じリダイレクト条件」を issue の probe（別プログラム）で満たすのか、製品バイナリで測るのかは issue に書かれていない。代理ではなく製品で測るべきというのは AGENTS.md「検証の作法」からの導出。
5. `snotra-egui-runtime` の `eprintln!` 計装（同じ形）を範囲に入れるかは issue が「確かめていないこと」に置いている。この導出では範囲外とした。`PERFORMANCE.md:3691` の「17〜56ms」は別の出口の runner 実測なので直さない側に置いたが、同じ機序なら将来同じ直しで偽になる。
6. serde_json の `preserve_order` が feature unification で有効になっているかは確かめていない（`src-tauri/Cargo.toml:25` は `serde_json = "1"` のみ）。どちらでも書き方の変更でキー順は動かないので結論には影響しない。
7. `PERFORMANCE.md:3954-3964`（2026-03-07）を読んだ。「cold 77ms のうち ~41ms はトレース I/O オーバーヘッド」は日付つきの過去の記録で、当時の版（`show_main_and_emit`、現存しない経路）について書かれている → 書き換えない側。ただし、これも trace の I/O が大きいという前提を支えていた 1 件である。
8. std の `Stderr` の内部（無効ハンドルを sink にする `handle_ebadf`・`write_all` が lock を取ること）は、rust-src が入っておらずソースで確かめられなかった（`~/.rustup/toolchains/1.98-x86_64-pc-windows-msvc` に `library/std` が無い）。記憶に基づく。
