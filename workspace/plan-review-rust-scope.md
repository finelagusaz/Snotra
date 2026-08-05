# plan.md Phase 2 レビュー — 変更の射程（誰が壊れるか）

対象: `snotra-egui-runtime` の trace ハッチ 7 箇所を `var_os(...).is_some()`（緩い）から
`src-tauri/src/trace.rs:20` の `env_flag`（`1|true|yes|on` のみ真）へ寄せる変更。

---

## 0. 母集団の確定（先に訂正）

**「6 種類の env」は誤りで、正しくは 5 名前 × 7 箇所である。**
`workspace/research.md:91-96` の表は 6 行あるが、`SNOTRA_EGUI_WAKE_TRACE` が
`repaint.rs:197` と `runtime.rs:279` の 2 行を占めるため、名前は 5 つしかない。
`plan.md:128` の作業項目「上記 6 種の env を設定している箇所を grep し」は行数を名前数と
取り違えている。**カバレッジの穴は生じない**（表は 5 名前すべてを名指している）が、
作業項目の母集団は 5 名前 / 7 箇所と読み替える必要がある。

| # | env 名 | 読み手（変更対象） |
|---|---|---|
| 1 | `SNOTRA_EGUI_INPUT_TRACE` | `input.rs:34`（**唯一 `OnceLock` キャッシュ付き**） |
| 2 | `SNOTRA_EGUI_PAINT_TRACE` | `renderer.rs:76` |
| 3 | `SNOTRA_EGUI_WAKE_TRACE` | `repaint.rs:197` / `runtime.rs:279` |
| 4 | `SNOTRA_EGUI_REPAINT_TRACE` | `runtime.rs:456` |
| 5 | `SNOTRA_EGUI_IME_TRACE` | `windows_ime.rs:100` / `:209` |

**射程外だが同じ表層形を持つ env**（既に `env_flag` を通る、または flag でないため今回無関係）:
`SNOTRA_EGUI_FAKE_UPDATE` / `SNOTRA_EGUI_FAKE_UPDATE_FAILED`（`egui_shell/mod.rs:187,198`）・
`SNOTRA_FAKE_INITIAL_HOTKEY_FAILURE`（`platform/mod.rs:288`）は既に `env_flag` 経由。
`SNOTRA_ICON_DIAG_PATHS`（`icon.rs:423`）は真偽 flag ではない。

---

## 1〜2. 設定箇所の全列挙と、変更後の真偽

### 実行した検索コマンド（絶対必要: `.github/` `.claude/` は dot ディレクトリゆえ ripgrep の既定走査から外れる）

```bash
# (A) 全ファイル（hidden / gitignore 含む）から 5 名前を走査 — 47 hit / 17 file
rg -n --hidden --no-ignore -g '!.git/*' -g '!target/*' -g '!node_modules/*' \
   "SNOTRA_EGUI_(INPUT|PAINT|WAKE|REPAINT|IME)_TRACE"

# (B) 「代入して設定している」形だけを走査（$env: / set / export / SetEnvironmentVariable / NAME=）
rg -n --hidden --no-ignore -g '!.git/*' -g '!target/*' -g '!node_modules/*' \
   '(\$env:|set |export |SetEnvironmentVariable\()[^\n]*SNOTRA_EGUI_(INPUT|PAINT|WAKE|REPAINT|IME)_TRACE|SNOTRA_EGUI_(INPUT|PAINT|WAKE|REPAINT|IME)_TRACE\s*='

# (C) 間接経路（Start-SnotraProcess -ExtraVariables 経由の env 注入）の実引数
rg -n --hidden --no-ignore "ExtraVariables" scripts/

# (D) .github / .claude を明示的に指した確認
rg -n "SNOTRA_" .github/    # → e2e.yml:68 のコメント 1 行のみ（SNOTRA_CONFIG_DIR の説明）
rg -n "SNOTRA_" .claude/    # → No matches found
```

### 設定箇所の全件（(B) と (C) の出力が母集団）

| env 名 | 設定箇所 `file:line` | 渡している値 | 変更後の真偽 | 影響 |
|---|---|---|---|---|
| `SNOTRA_EGUI_INPUT_TRACE` | `scripts/repro-pester-flake.ps1:129` | `'1'`（`-InputTrace` 指定時のみ） | **真のまま** | 影響なし |
| `SNOTRA_EGUI_INPUT_TRACE` | `scripts/repro-pester-flake.ps1:145`（`finally` の復元） | **`$null` → 実体は空文字**（元が未設定のとき） | **真 → 偽** | **これが唯一の実挙動変化であり、本計画の目的そのもの**（意図された修正・望ましい方向） |
| `SNOTRA_EGUI_PAINT_TRACE` | `docs/superpowers/plans/2026-07-24-su6.5-flip-hardening.md:919` | `$env:SNOTRA_EGUI_PAINT_TRACE=1` | **真のまま** | 影響なし（かつ歴史文書＝日付付き plan） |
| `SNOTRA_EGUI_PAINT_TRACE` | `.superpowers/sdd/review-2975baf..64e9348.diff:1028` | 同上（diff の写し） | **真のまま** | 影響なし（レビュー成果物の凍結記録） |
| `SNOTRA_EGUI_WAKE_TRACE` | — | — | — | **設定箇所なし**（(A)(B) の全 hit が読み手・散文・研究メモ） |
| `SNOTRA_EGUI_REPAINT_TRACE` | — | — | — | **設定箇所なし**（同上） |
| `SNOTRA_EGUI_IME_TRACE` | — | — | — | **設定箇所なし**（同上） |

### 「無い」の裏取り（射程ごと）

| 射程 | 結果 | 根拠 |
|---|---|---|
| `.github/workflows/` | **設定なし** | (D) の `rg -n "SNOTRA_" .github/` は `e2e.yml:68` のコメント 1 行のみ。`pester-flake-repro.yml:78` は `-InputTrace` を**スクリプトのパラメータとして**渡しており（`${{ github.event.inputs.input_trace == 'true' && '-InputTrace' \|\| '' }}`）env を直接設定しない（`rg -n 'InputTrace\|repro-pester' .github/workflows/` で実測） |
| `.claude/` 配下 | **設定なし** | (D) の `rg -n "SNOTRA_" .claude/` が `No matches found` |
| `package.json` / `CONTRIBUTING.md` | **設定なし** | (A) の 17 file の一覧に不出現 |
| Rust のテストコード | **設定なし** | (A) の全 hit のうち `.rs` は `input.rs` / `renderer.rs` / `repaint.rs` / `runtime.rs` / `windows_ime.rs` の**読み手 5 ファイルのみ**。`#[cfg(test)]` ブロック内の hit はゼロ |
| 間接経路 `Start-SnotraProcess -ExtraVariables` | **5 名前のいずれも通らない** | (C) の実引数呼び出し点は `scripts/smoke-egui.ps1:529` の 1 つだけで、渡すのは `SNOTRA_EGUI_FAKE_UPDATE = '1'`。予約名検査（`SnotraSmoke.psm1:309`）が守るのは `SNOTRA_CONFIG_DIR` / `SNOTRA_TRACE` の 2 つのみ |
| `SNOTRA_PESTER_*`（`plan.md:100` の同型対象） | **読み手は PowerShell のみ** | `rg -g '*.rs' "SNOTRA_PESTER"` が `0 matches / 90 files searched`。plan の「実害なし・同型」の判断は裏取りできる |

**結論**: 変更後に真偽が変わるのは `repro-pester-flake.ps1:145` の復元が作る空文字ただ 1 経路であり、
それは本計画が消したい欠陥そのものである。**既存の意図的な設定はすべて `=1` で、挙動は不変**。
`plan.md:114` の「変わるのは空文字・`0`・`false` を渡していた経路だけである」という主張は、
リポジトリ内の**設定箇所**に対しては**正しい**（リポジトリ外の残余は ⚠️ 1 を参照）。

**ただし読み手はもう 1 つあり、そちらは変更されない** —— `SnotraSmoke.psm1:664` の
`if ($env:SNOTRA_EGUI_INPUT_TRACE)`。**ここが新しい食い違いを生む**（→ 要対処 2）。

---

## 3. 人間が手で叩く手順として文書に書かれている形

`$env:X = ''` や `set X=` の形は**リポジトリ内に 1 件も無い**（上の (B) の出力が全件）。
文書に現れる手順形は 2 件だけで、どちらも `=1`：

- `docs/superpowers/plans/2026-07-24-su6.5-flip-hardening.md:919` — `$env:SNOTRA_EGUI_MAIN=1; $env:SNOTRA_EGUI_PAINT_TRACE=1; $env:SNOTRA_TRACE=1`
- `.superpowers/sdd/review-2975baf..64e9348.diff:1028` — 上の diff の写し

**変更後に嘘になる文書は無い。** ただし逆向きの問題がある（→ 要対処 3）:
**生きた規範文書のどこにも「どんな値で点くか」が書かれていない**。
`PERFORMANCE.md:250-253` はこれらの計器の自称する正本（「**このリストが計器の正本である**」）だが、
値の受理仕様に一言も触れず、しかも 5 名前のうち 3 つしか載せていない。

---

## 4. `env_flag` の写しを `snotra-egui-runtime` へ置く判断の妥当性

### 前提の実測

| 事実 | 根拠 |
|---|---|
| `snotra-egui-runtime` は `snotra-core` に依存していない | `snotra-egui-runtime/Cargo.toml:12-20`（arboard / egui / log / softbuffer / tauri 系 / thiserror） |
| `snotra-core` は依存 12 本を持つ（rayon・nucleo-matcher・wana_kana・uuid・chrono・toml 等） | `snotra-core/Cargo.toml:9-21` |
| **`src-tauri` は既に `snotra-egui-runtime` に依存している** | `src-tauri/Cargo.toml:14` `snotra-egui-runtime = { path = "../snotra-egui-runtime" }` |

### 計画が挙げなかった第 3 の選択肢がある

`plan.md:108` と U4（`plan.md:167`）は選択肢を 2 つ（写し vs `snotra-core` へ寄せる）しか
並べておらず、「依存辺を増やさない」を写しの理由にしている。しかし**依存の向きを逆にすれば
依存辺は 1 本も増えない**：

> **案 C: SSOT を `snotra-egui-runtime` に置き（`pub fn env_flag`）、`src-tauri/src/trace.rs::env_flag` が
> それへ委譲する。**新規の依存辺はゼロ（`src-tauri/Cargo.toml:14` が既に在る）。

**計画が写しを正当化するために挙げた理由（依存辺を増やさない）は、この向きには当たらない。**
これは好みの差ではなく、`Cargo.toml` という一次資料が計画の前提を否定している。

### 3 案の比較

| 案 | 新規の依存辺 | 「正本を 1 か所」規範との折り合い | 残余 |
|---|---|---|---|
| **A: 写し（計画の案）** | 0 | **抵触する。** `AGENTS.md`「条件別チェック」の「文書に事実の写しを増やす変更 → 正本を 1 か所に定め他は参照へ」は正面から当たる。計画の緩和策は「双方の doc で互いを名指し」だが、**それには検知手段が無い**（→ 要対処 2） | 受理値集合の分岐が沈黙で起きる |
| **B: `snotra-core` へ寄せる** | **1 本（重い）** | 満たす | 描画接着層に rayon / wana_kana / uuid の依存ツリーを持ち込む。**8 行の述語のために払う代償として過大で、計画が却下したのは妥当** |
| **C: `snotra-egui-runtime` を SSOT に、`src-tauri` が委譲** | **0** | **満たす。** 正本 1 か所、参照は言語機構（`use`）が担い、改名は compile-fail が捕まえる | 汎用の env 述語を「Snotra 専用の接着層」に置く責務のねじれ。`lib.rs`（公開 API）が関数 1 本ぶん太る |

**推奨は案 C。** 規範を満たし、依存辺を増やさず、受理値集合の分岐を**構造的に表現不能**にする
（`prefer-structural-over-documented-contract` の選好とも一致する）。
案 A を採るなら、**「検知器を持たない写し」であることを受容残余として明記した上で**採ること
——現在の計画本文は「doc で互いを名指しすれば足りる」と読める書き方で、そこが弱い。

### 置き場所として `input.rs` は不適切

`snotra-egui-runtime/CLAUDE.md`「モジュール構成」は `input.rs` を
**「Taoイベントからegui入力への純粋変換」**と定義する。汎用の env 述語はこの責務に属さず、
かつ**消費者 5 名前のうち 4 つは `input.rs` の外**（renderer / repaint / runtime / windows_ime）にある。
`input.rs` に置けば、renderer が「入力変換モジュール」から述語を import する形になる。

- 案 C を採るなら置き場所は `lib.rs`（公開 API の所在）か新規の小さいモジュール
- 案 A を採るなら新規ファイル（例: `env.rs`）が素直。ただし**`.rs` の追加は `AGENTS.md`「条件別チェック」の
  「ファイル（`.rs`）を追加/削除」トリガーに当たり**、`snotra-egui-runtime/CLAUDE.md`「モジュール構成」への
  索引行の追加と `npm run governance:check` が要る。計画の Phase 3 は `docs/build-commands.md` 1 行しか
  触らないので、この作業項目が抜けている

---

## 5. 既存テストと、`env_flag` の単体検査の置き場所

### 既存テストの実測

`snotra-egui-runtime` は 8 ファイルに `#[cfg(test)]` / `#[test]` が計 32 箇所ある
（`input.rs` 7 / `repaint.rs` 7 / `raster.rs` 5 / `ime.rs` 4 / `renderer.rs` 3 / `runtime.rs` 2 /
`surface.rs` 2 / `windows_ime.rs` 2）。**テストの器は十分に在る。**
`input.rs:477` の `mod tests` の 6 テストはいずれも純関数（キーマッピング・`take` の契約）で、
**env を触るテストは 1 本も無い**。

**`src-tauri/src/trace.rs` にはテストが 1 本も無い**（59 行全読・`#[cfg(test)]` 不在）。
つまり**受理値の意味論は、SSOT 側でも現在まったく測られていない**。案 A / C のどちらでも、
この検査を新設することが本 Phase の実質的な価値の一つになる。

### `OnceLock` でキャッシュした述語をどうテストするか — 既存の流儀

**計画の検証項目（`plan.md:120`）は、書かれたままでは実装できない。**
「未設定が偽」を単体検査で測るには env を消す必要があるが、両 crate は edition 2024 で、
`std::env::set_var` / `remove_var` は `unsafe` である。実測:

```
$ rustc --edition 2024 envtest.rs
error[E0133]: call to unsafe function `set_var` is unsafe and requires unsafe block
 --> envtest.rs:2:5
（rustc 1.94.0）
```

加えて `OnceLock` はプロセス内で 1 度しか評価されないため、
テストの実行順に依存する（`cargo test` は同一プロセス内でテストを並列実行する）。

**リポジトリ内の既存の流儀がこれをきれいに解いている**——`snotra-core` の `config_dir` 系
（正本は `snotra-core/CLAUDE.md`「モジュール構成」の `config.rs` 項）:

1. **判定核を env から切り離した純関数にする** — `config_dir_from(override, base)` は env を読まない
   （`config.rs` の rustdoc: 「**判定核 `config_dir_from(override, base)` は env を読まない**ので
   並列テストから安全に測れる」）
2. **結線（env を読むこと）は別のテストが「env を読むだけ」で pin する** —
   `config_dir_is_wired_to_dirs_config_dir_with_snotra_suffix`（`config.rs:1237-1248`）。
   doc（`:1235-1236`）が明言する: 「env を**読むだけ**（`set_var` しない）ので並列実行から安全。
   周囲の env がどちらでも対応する分岐を assert するため、**skip して黙る経路を持たない**」

### 提案する形（3 層に割る）

```rust
/// 純粋核。env を読まないので並列テストから安全に、網羅的に測れる。
fn parse_env_flag(value: Option<&str>) -> bool { ... }

/// 結線。env を 1 回読んで純粋核へ渡す。
pub fn env_flag(name: &str) -> bool { parse_env_flag(std::env::var(name).ok().as_deref()) }

/// キャッシュ。呼び出し側の責務（現行 `input.rs:29-35` / `trace.rs:30-33` と同じ形）。
```

- **`parse_env_flag` に対して網羅検査を置く**（`plan.md:120` の項目はここで全部満たせる）:
  真 = `Some("1")` / `Some("true")` / `Some("yes")` / `Some("on")` / `Some("ON ")` / `Some(" TrUe ")`、
  偽 = `Some("")` / `Some("0")` / `Some("false")` / `Some("onx")` / `Some("2")` / `None`。
  **`unsafe` も `OnceLock` も要らず、実行順にも依存しない**
- **`OnceLock` の側は測らないでよい**——キャッシュの形は既に `input.rs:30-32` の doc が理由込みで
  正当化しており、そこに新しい判定ロジックは無い
- 置き場所は `parse_env_flag` を定義したファイルの `mod tests`（この crate の全モジュールがそうしている）

---

## 6. `SNOTRA_TRACE` と 5 種の使い分けの文書化

**未文書である。**

- `PERFORMANCE.md:249` — `SNOTRA_TRACE=1` の構造化トレース（`src-tauri/src/trace.rs`）に触れる
- `PERFORMANCE.md:250-253` — egui/softbuffer の計器を「3 つの env」として列挙し、
  **「このリストが計器の正本である——`docs/build-commands.md` には置かない」**と自称する
- **両者の受理値の意味論が違うことは、どちらにも書かれていない**
- `workspace/research.md:98` が「同じリポジトリの中に 2 つの意味論があり、緩い側だけが空文字で点灯する」と
  書いているが、これは今回のサイクルの作業材料であって規範文書ではない

**ただし正しい是正は「使い分けを文書化する」ではない**——Phase 2 はその差そのものを消す。
消したあとに書くべきは「**全 6 名前が同じ受理値集合（`1|true|yes|on`）を持つ**」という 1 文であり、
それを置く場所は自称正本の `PERFORMANCE.md:250` である（→ 要対処 3）。

---

## 所見

### 要対処

**1. 7 箇所のうち 6 箇所はキャッシュを持たず、`env_flag` は ON のときに割り当てが 1 → 2 へ増える。
計画の変更表は「新 `env_flag` へ寄せる」としか書いておらず、この差を扱っていない。**

- `env_flag` は `std::env::var`（String を割り当てる）+ `.trim().to_ascii_lowercase()`（もう 1 つ割り当てる）。
  現行の `var_os(...).is_some()` は OsString の割り当て 1 つ。**OFF のときは両者ほぼ同じ**
  （どちらも None / Err で早期に返る）が、**ON のときは呼び出しごとに割り当てが 1 つ増える**
- 増える先が問題である。`renderer.rs:76` は `paint()` の中＝**毎フレーム**で、しかも
  直後のコメントが「計測器が測定対象を汚さない」ことを設計意図として明記している。
  `SNOTRA_EGUI_PAINT_TRACE` が ON の状態こそ、この計器が `tess_ms` / `raster_ms` を測っている状態である
- `runtime.rs:456` は毎フレーム、`repaint.rs:197` は送信ごと、`windows_ime.rs:100/209` は IME メッセージごと
- **`input.rs:30-32` の doc が、この懸念を既に言葉にしている**——「この述語は窓イベントごと
  （マウス移動を含む）とフレームごとに問われるため、`env::var_os` の割り当てを毎回払うと…」。
  同じ理屈が残り 6 箇所に逐語で当たるのに、`OnceLock` が置かれたのは `input.rs` だけである
- **求めるもの**: Phase 2 で 6 箇所にも `OnceLock` を置くか、置かない理由を明記する。
  どちらにせよ `PERFORMANCE.md:250` の「いずれも未設定なら計器のコストは 0」と
  `input.rs:30-32` の根拠が、変更後も真かを確かめること

**2. 3 人目の読み手 `SnotraSmoke.psm1:664` が緩いまま残り、計画は食い違いを消すのではなく
別の値クラスへ引っ越させる。計画の「両側を直す」（`plan.md:93`）は成り立たない。**

計画が言う「両側」は「env の退避・復元」と「Rust の述語」であって、
**PowerShell 側の読み手 `SnotraSmoke.psm1:664` は一度も触られない。**
PowerShell の `if ($env:X)` は**文字列の真偽**であり、空文字だけが偽である（実測）:

```
$ pwsh -NoProfile -Command '$env:T = "0"; if ($env:T) { ... }'
value=0     -> True
value=false -> True
value=empty -> False
```

値クラスごとに並べると:

| 値 | 現行 Rust | 現行 PS | 変更後 Rust | 変更後 PS | |
|---|---|---|---|---|---|
| `''`（空文字） | 真 | 偽 | **偽** | 偽 | **食い違い解消**（計画の目的・唯一の実害経路） |
| `'1'` | 真 | 真 | 真 | 真 | 不変 |
| `'0'` / `'false'` / 任意の値 | 真 | 真 | **偽** | **真** | **新たな食い違いが生まれる** |

変更前、`=0` では両方の読み手が ON で**一貫していた**。変更後は
**PowerShell が注入行を出す一方、Rust は 1 行も出さない。**

これは Phase 2 が同時に作ろうとしているものへ直接効く。`plan.md:109` は
`caret.err` の `SNOTRA_EGUI_INPUT` 行の有無を「証拠に基づく計器の有無」として `summary.md` へ出す設計である。
`=0` の下では、**PS は注入を記録し、証拠ベースの集計は「計器なし」と報告する**——
計画が消すと宣言した「意図と実態の食い違い」が、別の値クラスへ移動しただけになる。

**他に緩い読み手は無い**（`rg -n 'if \(\$env:' scripts/lib/*.psm1 scripts/*.ps1` の hit は
`psm1:664` と、無関係な `repro-pester-flake.ps1:211` の `GITHUB_STEP_SUMMARY` の 2 件のみ）。

**求めるもの**: 次のいずれか。
- `psm1:664` にも同じ厳しい述語を与える（`1|true|yes|on` を判定する PowerShell ヘルパ）。
  **その場合、受理値の正本は 3 言語 3 箇所に散る**——要対処 3 の「検知器の無い写し」がさらに重くなり、
  案 C（Rust 側を 1 本化）の価値が上がる
- あるいは `plan.md:93` の「両側を直す」を「**空文字だけ**を直す」へ narrowing し、
  `=0` 系の残余を明示的に受容する

**3. 写しを置く判断に検知器が無く、計画の緩和策（doc で互いを名指し）はそれを埋めない。**

- `governance:check` の G-heading-refs が照合するのは正準形 `` `<path>.md`「<見出し>」 `` だけで
  （`.claude/rules/governance-docs.md`）、**Rust のシンボル相互参照はその形ではない**
- `broken_intra_doc_links = "deny"`（`Cargo.toml:22`）も助けにならない——
  `snotra-egui-runtime` は依存していない crate の `pub(crate)` 関数へ intra-doc link を張れない
- したがって、将来どちらか一方の受理値集合に `"y"` を足しても**何も赤くならない**
- **要対処 2 を「PS 側も厳しくする」で解くと、正本は Rust 2 か所 + PowerShell 1 か所の
  計 3 言語 3 箇所へ散る。** 言語をまたぐ写しには intra-doc link も compile-fail も届かない
- **求めるもの**: 案 C（`src-tauri` が `snotra-egui-runtime` の `env_flag` へ委譲）を採るか、
  写しを採るなら「検知器を持たない写しである」ことを受容残余として計画へ明記する

**4. `PERFORMANCE.md:250-253` が計器の自称正本でありながら 5 名前中 3 つしか載せず、受理値集合も書いていない。
Phase 3 はここを触らない。**

- 欠けているのは `SNOTRA_EGUI_INPUT_TRACE` と `SNOTRA_EGUI_IME_TRACE`
- Phase 2 は「どんな値で点くか」を**変える**変更なのに、その事実を書く場所が計画に無い
  （Phase 3 が触るのは `docs/build-commands.md:210` の 1 行だけ）
- `governance:check` はこの欠落も、受理値の未記載も捕まえない（決定的な検査項目に含まれない）
- **求めるもの**: Phase 3 に `PERFORMANCE.md:250-253` の是正（2 名前の追加 + 受理値集合 1 文）を足す。
  これが Q6（未文書）への正しい着地でもある

**5. 置き場所 `input.rs` はモジュール責務に反し、新規ファイルにするなら計画に索引更新の作業項目が要る。**（詳細は §4 末尾）

**6. `plan.md:120` の検証項目（「未設定が偽」の単体検査）は、書かれたままでは実装できない。**
edition 2024 で `set_var` / `remove_var` が `unsafe`（E0133 を実測）。
§5 の 3 層分割（`parse_env_flag` 純粋核）へ書き換えれば全項目が `unsafe` なしで満たせる。

### 軽微

**7.** 「6 種類の env」（計画・ブリーフとも）は 5 名前 / 7 箇所の誤り（§0）。カバレッジの穴は生じない。

**8.** `src-tauri/src/trace.rs` にテストが 1 本も無く、`env_flag` の受理値は SSOT 側でも未測定。
案 C を採れば新設した検査が両者を一度に守る。

**9.** `env_flag` は `std::env::var` を使うため、**非 UTF-8 の値は偽**になる（`var_os` は真だった）。
Windows のこれらの flag で実害は考えにくいが、「任意の値 → 偽」の一部として認識しておくとよい。

### ⚠️（確信が持てない）

**⚠️ 1 — 開発者のローカルシェルに残った falsy な値は、リポジトリからは観測不能である。**
誰かが `$env:SNOTRA_EGUI_PAINT_TRACE = ''` や `= '0'` の形で計器を点けていた場合、
変更後に**黙って計器が消える**。実際にそうしている人が居るかはリポジトリ内の証拠では決められない
（`repro-pester-flake.ps1` の空文字と同じ形を人が手で作りうる）。
唯一の緩和は要対処 3 の `PERFORMANCE.md` への受理値集合の明記である。確信度: 機序は確実、発生の有無は不明。

**⚠️ 2 — grep は「動的に組み立てた変数名」が `Invoke-SnotraEnvironment` の `$variables` へ届く経路を否定できない。**
静的な `-ExtraVariables` の実引数は 1 か所しか無い（`smoke-egui.ps1:529`）ことを確認したが、
`$variables[$computedName] = ...` のような形は上の検索では捕まらない。
実際にそういう書き方は見当たらなかったので低確率と見るが、**この探し方では証明できない**。

**⚠️ 3 — 歴史文書に残る旧形（`var_os(...).is_some()`）が、将来の写し元になりうる。**
`docs/superpowers/plans/2026-07-24-su6.5-flip-hardening.md:866` と `.superpowers/sdd/*.diff` に
旧形のコードブロックが凍結されている。日付付き plan は歴史記録であって規範ではない（ADR と同じ扱い）ため
**是正すべきかは判断がつかない**。放置が正しい可能性が高いが、新しい計器を足す人がここを手本にする
経路は実在する。確信度: 低。

**⚠️ 4 — `input.rs` の `OnceLock` を残したまま `env_flag` へ寄せると、`input.rs` だけが
「プロセス起動時の値」を、他 6 箇所が「呼び出し時点の値」を見る非対称が残る。**
現在も同じ非対称は在る（変更で新しく生まれるものではない）ので Phase 2 の責任範囲か迷う。
実害があるのは「実行中に env が変わる」場合だけで、Windows のプロセス env では通常起きない。
要対処 1 でキャッシュを 6 箇所へ広げるなら、この非対称は自然に消える。
