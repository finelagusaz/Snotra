# research — #1085: Claude Code の RA 診断をどう扱うか、実効設定を実測してから決める

## issue の要約

#1083（PR #1086）で Claude Code 用の rust-analyzer（RA）を repo 所有の project-scope LSP plugin
（`.claude/lsp/`）へ分離し、`initializationOptions` で `checkOnSave=false` と
`workspace.symbol.search`（`all_symbols` / `limit=512`）を渡した。**診断の扱いだけを先送りにしてある。**

先送りの理由は 2 つ。(1) 抑制の機構が 2 層ある（Claude Code 側の `diagnostics: false` は
publishDiagnostics の**注入だけ**を止め navigation は保つ／RA 側の `diagnostics.enable=false` は
**計算しない**）。(2) 費用対効果に疑問がある——`<new-diagnostics>` の量は編集由来 3〜9 件に対し
底値 96 件で、しかもその底値は CLI のアーティファクトの公算が高い。

判断基準はユーザーが #1083 の承認時に示したもの:
> 重要なのはエージェント（この場合はあなた）が LSP から**過不足なく**情報を受け取ることにある

本 issue は**まず測る**ことを求めており、判断はその結果を見てから行う。

## 測定対象と、その素性

測ったのは **claude.exe（PID 7868）が起動した RA インスタンス**（PID 5952 shim → 19196 本体・
起動 `13:00:47`）と、**同じ `initializationOptions` で私が別途起動した RA**（M4 の治具）である。

**この RA は #1085 のために起動されたものではない。** 起動時刻は PR #1086 の検証時間帯と重なり、
以後 1 度も再起動されていない。#1085 の実測窓（14:00〜14:40）まで同一プロセスを使い回している。
結論（設定が届いている）には影響しないが、「このタスク用に新規起動した」と読まれないよう明記する。

CLI（`rust-analyzer diagnostics .`）は測定手段に採らない。`cfg(test)` を立てず ratoml も読まないため、
測りたい対象を測らない（#1082 で実測済み）。RA のバージョンは `1.97.1 (8bab26f4 2026-07-14)`。

## 実測（2026-08-14 14:00〜14:40 JST）

### M0. 計器の排他性 — RA は 1 系統

| PID | 実体 | 親 |
|---|---|---|
| 5952 | `C:\Users\Eoh\.cargo\bin\rust-analyzer.exe`（rustup shim） | 7868 `claude.exe` |
| 19196 | `...\.rustup\toolchains\stable-...\bin\rust-analyzer.exe` | 5952 |
| 9352 | proc-macro-srv | 19196 |

`rust-analyzer.exe` という名を持つプロセスはこの 1 系統だけである（敵対枠が独立に再取得して一致）。

**ただし「別クライアントは居ない」とまでは言えない**（敵対枠の指摘・採用）。母集団をプロセス名だけで
取っており、`Zed.exe`（PID 19708・起動 13:52:18）が現に稼働していて workspace 履歴に Snotra が
記録されている。測定窓の間 Zed が一度も RA を起動しなかったことは検証していない。

**それでも M1 の推論の向きは安全である。** 主張は「mtime が**動かなかった**」であり、
別クライアントが増えても mtime は**動く方向にしか**効かない。第三者の書き込みは我々の RA の
書き込みを隠せない。ゆえに M0 の弱さは M1 の結論を脅かさない（M1b がこれを独立に裏付ける）。

### M1. 挙動プローブ 1（`checkOnSave=false` が届いたか）— **合格**

`target/flycheck0/stdout` の mtime は `12:09:44`（現 RA の起動 13:00:47 より前）で、
以下 5 回の `.rs` 編集を跨いで**1 度も動かなかった**。

1. コメント 1 語の書き換え（`env.rs`）
2. `fn is_enabled` → `is_enabled_probe1085` の改名（cargo は E0425 / E0432 で落ちた）
3. 未リンクの新規ファイル `probe1085.rs` の作成
4. 構文エラーの注入（`env.rs` の閉じ括弧欠落）
5. 構文修復 + 改名の再注入

**通知経路が生きていることを独立に確かめてある**（凍結が「RA が編集を見ていない」で説明されないため）。
`documentSymbol(env.rs)` が改名後の `is_enabled_probe1085` を返し、構文エラー注入時には
`<new-diagnostics>` が実際に届いた。

### M1b. `checkOnSave` の A/B（**計器が本当に `checkOnSave` を測っているか**）— **合格・反復あり**

M1 には対抗仮説があった（敵対枠の争点 2）: **Claude Code のクライアントが `textDocument/didSave` を
送らないなら、`checkOnSave` の値に関わらず flycheck は走らない**——その場合 mtime は計器として無意味。

自前の stdio クライアント（M4 の治具）で RA を 2 条件で起動し、同じ `target/` を共有させて対照した。

| 条件 | `initializationOptions` | `flycheck0/stdout` |
|---|---|---|
| A | `.lsp.json` と同一（`checkOnSave: false`） | **動かない**（`14:33:38` のまま） |
| B | `{}`（RA 既定＝`checkOnSave` 真） | **動く**（`14:39:37` へ・サイズも変化） |

**偶然の観測から設計された対照へ変えた**——最初の B 条件は Q1 のために走らせたもので、後片付けの
`git status` で mtime が動いていることに気づいた。その後 A → B の順で意図的に反復し、再現した。

これで対抗仮説は落ちる。**私の治具は `didSave` を 1 度も送っていない**（送るのは `didOpen` だけ）。
それでも B では flycheck が走った。ゆえに flycheck の起動は保存通知に依存せず、
**A と B を分けているのは `checkOnSave` の値そのものである**。

### M2. 挙動プローブ 2（入れ子形の `initializationOptions` が受理されたか）— **合格**

`workspaceSymbol("config")` が **114 件**を返し、`Function`（`config_dir` / `compute_config_hash` /
`read_config` 等）と `Module` / `Constant` / `EnumMember` が混ざった。

**「既定でも Function が返りうる」という対抗仮説は実在する**（敵対枠の指摘・採用）。RA の
`handle_workspace_symbol` には「型検索が 0 件のときだけ `only_types` を外して再検索する」
フォールバックがある（`crates/rust-analyzer/src/handlers/request.rs`）。ゆえに一般論だけでは足りない。

**これを退けるのは PR #1086 の実測記録である**: 変更**前**（既定 `only_types`）に同じクエリ `"config"` が
**14 件・全部 Struct / Enum** を返していた。型検索が 0 件でなかった以上フォールバックは発火しえない。
ゆえに 114 件・Function 混在は `kind = "all_symbols"` の効果としか説明できない。

**`limit = 512` は独立には証明できていない**——114 件は既定の 128 を下回るため差が現れない。

### M3. 実セッションでの `<new-diagnostics>` — **構文エラーしか届かない**

| 変異 | cargo（PostToolUse hook） | `<new-diagnostics>` |
|---|---|---|
| コメント 1 語 | 沈黙＝合格 | 0 件 |
| 改名（ファイル未開状態） | E0432 / E0425 | 0 件 |
| 改名（**開いた状態**） | E0432 / E0425 | 0 件 |
| 未リンクファイル作成（構文正当） | 沈黙＝合格（cargo に見えない） | 0 件 |
| 構文エラー（リンク済み `env.rs`） | fmt / clippy / test が 3 本とも失敗 | **1 件** `syntax-error` |
| 構文エラー（**未リンク** `probe1085.rs`） | 沈黙＝合格（cargo に見えない） | **2 件** `syntax-error` |

**注入経路は生きている。** そして届くのは構文診断だけで、意味診断（`unresolved-*` / `inactive-code` /
`unlinked-file`）は 1 件も届いていない。最下行は重要である——**cargo が完全に見ていないファイルの
構文エラーを LSP が届けた**。ここだけは LSP が hook の上に足している検出力である。

### M4. RA へ直接聞く（自前の stdio LSP クライアント）— Q1 の一次証拠

間接プローブを重ねても機序は決まらないので、`initializationOptions` を同一にした RA を別途起動し、
`didOpen` して `publishDiagnostics` の**生の配列**を読んだ（治具は scratchpad の `ra-probe.mjs`。
稼働中の Claude Code の RA には触れていない）。

| 開いたファイル | 条件 | 生の publish 配列 |
|---|---|---|
| `probe1085.rs`（未リンク・構文正当） | A（我々の設定） | **0 件** |
| `env.rs`（リンク済み・正当） | A | **0 件** |
| `probe1085.rs`（未リンク・**構文エラーあり**） | A | **2 件・すべて `syntax-error`** |
| `probe1085.rs` + `env.rs`（どちらも正当） | **B（RA 既定）** | **どちらも 0 件** |

`cachePriming` の終了まで待った上での結果である（`env.rs` は B で 2 度 publish されており、
意味解析のパスも走った上で 0 件だった）。

読み取れること。

1. **`unlinked-file` は RA が publish していない。** 未リンクファイルを明示的に `didOpen` し、
   同じ配列に `syntax-error` が 2 件入っている状況で、`unlinked-file` だけが不在である。
   ゆえに「注入側の severity フィルタ」「観測窓の外し」「開いていないから計算されない」の
   どれでもない——**RA が出していない**（Q1 候補 b）。
2. **原因は #1083 の設定ではない。** 条件 B（RA 既定）でも結果は同一だった。
3. **`inactive-code` 94 件は LSP には存在しない。** `env.rs` は `#[cfg(test)] mod tests` を持ち、
   CLI ならこれが `inactive-code` になるが、LSP 側は A / B とも 0 件だった。
   `ra-diagnostics-noise-is-baseline-not-edits` の「CLI のアーティファクト」という見立てが
   **LSP 側の一次証拠で裏づいた**（CLI は `cfg(test)` を立てないため・#1082）。

**`unlinked-file` が出ない機序は未確定である。** RA の `ide-diagnostics/src/lib.rs` を読むと
`unlinked_file` は「そのファイルの `module` が `None` のとき」に無条件で積まれ、ハンドラ側にも
早期 return は見当たらない。つまり読み筋では発火するはずで、観測と合わない。**推測を書かない**
——分かっているのは「出ていない」ことと「我々の設定が原因ではない」ことの 2 つだけである。

### M5. 足 2 を塞ぐ判定式を、実運用点の入力で評価した

設計を確定する前に判定式そのものを測った（`evaluate-predicate-on-real-config` /
`AGENTS.md`「計画に書いた判定ロジックは、実装前に代表入力で実行して測る」）。
試作は scratchpad の `unlinked-proto.mjs`（リポジトリへは入れない）。

判定は「crate ルート（`lib.rs` / `main.rs`）から `mod` 宣言を辿り、`<crate>/src/**/*.rs` のうち
到達しないものを挙げる」。`#[path = "..."]` と `mod.rs` を解決する。

| 入力 | 結果 |
|---|---|
| 現状のリポジトリ | **95 / 95 到達・未到達 0 件**（誤検出なし） |
| 変異: 新規 `.rs` を作り `mod` を書かない | **赤**（1 件・実際の回帰の姿） |
| 変異: 既存の `mod env;` を消す | **赤**（1 件・宣言側の回帰） |
| 復元後 | 緑 |

**誤検出が 0 なのは自明ではない。** `snotra-egui-runtime/src/ime.rs` に
`#[cfg(windows)] #[path = "windows_ime.rs"] mod platform;` という実例があり、
「`mod <stem>;` がどこかに在ること」という素朴な述語なら `windows_ime.rs` を誤検出する。
`mod.rs` 5 枚も同様に素朴な述語を壊す。

**まだ測っていない向きがある**——「検査を殺す変異」（母集団が空になれば無条件に緑）。
これは変異を足す側からは原理的に見えない（`ADR-claude-code-ra-lsp-plugin-delivery`
「検知器自身のカバー範囲を検算して見つかった足」で 14 中 3 件の素通りが実際に見つかっている）。
**実装では fail-closed を設計に埋め、その向きの変異も注入する**（計画の受け入れ条件）。

## 決定に効く形へまとめると

- **量の問題は存在しない。** 実セッションで届く診断は構文エラーだけで、正常な編集では 0 件である。
  抑制して減らせるものが無い。issue が心配した底値 96 件は LSP には最初から無い。
- **抑制すると失うものは 1 つある**——**cargo が見ないファイル（未リンク `.rs`）の構文エラー**。
  M3 の最下行がその実例で、hook の fmt / clippy / test はここで沈黙した。小さいが、
  「過不足なく」の**不足**の側に落ちる唯一の実測値である。
- **「`unlinked-file` を失う」という先送りの根拠は空だった。** 抑制するかに関わらず、
  いま届いていない。ゆえに「採るなら代替検知をどこに置くか」という issue の設問は、
  **採否と切り離して**独立に存在する穴である（→ Q3）。

## 関連ファイル・モジュール・シンボル

すべて実在を確認済み。

| パス | 役割 |
|---|---|
| `.claude/lsp/snotra-rust-lsp/.lsp.json` | `initializationOptions`。`diagnostics` キーは**現在無い** |
| `.claude/lsp/snotra-rust-lsp/.claude-plugin/plugin.json` | plugin マニフェスト |
| `.claude/lsp/.claude-plugin/marketplace.json` | marketplace マニフェスト（`name: "snotra"`） |
| `.claude/settings.json` | `extraKnownMarketplaces`（相対パス `./.claude/lsp`）と `enabledPlugins` |
| `.claude/hooks/lsp-config.mjs` | 上記の不変条件を判定するカナリア。`RATOML_FORBIDDEN = ["checkOnSave", "diagnostics"]` |
| `.claude/hooks/lsp-config.test.mjs` | 同カナリアの故障注入テスト |
| `.claude/hooks/post-edit.mjs` | `.claude/lsp/` と `rust-analyzer.toml` の編集で `hook-selftest` を発火（166 行目） |
| `docs/hooks.md`「Claude Code の RA インスタンスと hook の分担」 | 分担の正本 |
| `docs/adr/ADR-claude-code-ra-lsp-plugin-delivery.md` | 却下案と受容する残余。診断の先送りもここに記録 |
| `rust-analyzer.toml` | `checkOnSave` / `diagnostics.*` を書いてはならない側（カナリアが機械で縛る） |

## 再利用できる既存パターン

- **#1059 の計画の形**（`RETROSPECTIVE.md`）: 判定で分岐する作業を作業項目に置かず、
  作業項目は**勝敗のどちらでも実行するもの**だけで構成する。本 issue は同じ形である。
- **カナリアの拡張点は既にある。** `lsp-config.mjs` は「抑制キーの消失」を判定する形をすでに持つ。
- **stdio クライアントによる RA への直接質問**（M4 の治具）は、セッション再起動なしに
  `initializationOptions` の効き目を測れる。今後の同種の問いに再利用できる。

## 技術的制約

- **plugin の設定はサーバ起動時にしか読まれない。** `diagnostics: false` を入れて navigation が
  生きているかを確かめるには**セッション再起動**が要る（→ Q2）。
- **LSP ツールに diagnostics 操作は無い。** 診断の観測経路は `<new-diagnostics>` の注入だけで、
  しかも**トランスクリプトに永続化されない**。観測したらその場で書き写すしかない。
- **`.claude/lsp/**` はセーフティネットである**（`.claude/rules/safety-nets.md` の `paths`）。
  変更はルート `CLAUDE.md`「最重要ルール（常に適用）」の 2 番により**合意してから**行う。
- **`claude plugin validate --strict` は `.lsp.json` を見ない**（#1083 で変異注入により実測）。
- **検証は worktree ではなくメインツリーで行う**（ADR の受容する残余 1）。
  `known_marketplaces.json` の `installLocation` はメインツリーを指していることを確認済み。

## 未解決の疑問

- **Q1（解決済み）: `unlinked-file` は届くか。** → **届かない。RA が publish していない**（M4）。
  我々の設定が原因ではない。**機序は未確定**であり、規範文書へ書くのは所見だけに留める。
- **Q2（残る）: `diagnostics: false` で navigation が生きているか。** 静的読解の傍証のみ。
  **抑制を採らないなら測る必要が無い**——採否の判断が先に立つ。
- **Q3（解決済み・本 issue の範囲に取り込む）: `mod` 忘れの検知は今どこにあるか。**

  **当初「規範のみ」と書いたのは過大な主張だった。** `scripts/governance-check.mjs` の
  `G-module-index` が逆方向の照合を持ち、`<crate>/src/**/*.rs` の basename が
  その crate の `CLAUDE.md` にバッククォートで現れることを機械で強制している。
  足ごとに壊して測った結果は次のとおり（2026-08-14 実測）。

  | 足 | 変異 | `governance:check` |
  |---|---|---|
  | 1 | 新規 `.rs` を作り、**索引にも `mod` にも**書かない | **赤**（`G-module-index` が捕捉） |
  | 2 | 同じ `.rs` を**索引には書き**、`mod` 宣言だけ忘れる | **緑**（素通り） |

  **残る穴は足 2 だけである。** cargo からも LSP からも `governance:check` からも見えない。
  最悪の帰結は「`#[cfg(test)] mod tests` を持つファイルが 1 度もコンパイルされず、
  テストが黙って走らない」——このリポジトリが最も嫌う偽の緑である。

  これは `.claude/rules/safety-nets.md`「検出器のカバー範囲は、欠落のパターンごとに検算する」が
  名指しする形そのもの（#858: カナリアが守るのは 3 本中 1 本だけだった）である。
  **ユーザー判断により、この足を塞ぐことを本 issue の範囲に取り込む。**

## 敵対的調査（3b）の所見と採否

母集団は `research.md` の全主張。**壊せた項目は 0 件**（実測データは独立に再現され、改竄・誤読なし）。
⚠️ として 3 つの実証上の穴が返り、**3 件とも採用**した。

| # | 所見 | 採否 | 反映 |
|---|---|---|---|
| 1 | M0 の母集団がプロセス名だけで、`Zed.exe` が現に稼働している | **採用（弱める）** | M0 に明記。**ただし推論の向きが安全であること**（第三者は mtime を動かす方向にしか効かない）を自分で裁定して添えた。敵対枠はこの向きの分析までは書いていない |
| 2 | 「本セッションが起動した RA」は誤読を招く（実体は #1086 検証時から使い回し） | **採用** | 「測定対象と、その素性」節を新設して明記 |
| 3 | Q1 が RA 本体のソースという一次資料に当たっていない | **採用（そのうえで手段を変えた）** | 敵対枠は `RA_LOG` + セッション再起動を勧めたが、**再起動不要の stdio クライアント**（M4）を選んだ。所見（一次資料に当たっていない）は正しい。**添えられた機序の説明は採らなかった** |

**機序の説明は採らず、自分で測った。** 敵対枠は Q1 の第 4 の機序として
`main_loop.rs` の `update_diagnostics` が `mem_docs`（`didOpen` 済み文書）に限定される点を挙げた。
これは RA のソースとして正しいが、**この現象の原因ではない**——M4 で明示的に `didOpen` しても
`unlinked-file` は出ず、同じ配列に `syntax-error` は入っていた。加えて私の観測ひとつ
（同じ `env.rs` を開いた状態で、構文エラーは届き改名は届かない）が `mem_docs` 説では説明できない。
ルート `CLAUDE.md`「採るのは所見であって説明ではない」の実例をもう 1 件足したことになる。

**敵対枠が独立に確認し、こちらの前提と一致した測定環境**: project / user の `.claude/settings.json`、
`.lsp.json` の内容、`rust-analyzer.toml` に実キーとして `checkOnSave` / `diagnostics` が無いこと、
ユーザ ratoml（`%APPDATA%\rust-analyzer\rust-analyzer.toml`）が**存在しない**こと、
`known_marketplaces.json` の `installLocation` がメインツリーであること。

**未追跡で残す 1 件**: user 設定では `rust-analyzer-lsp@claude-plugins-official` が `true`、
project 設定では `false` である。優先順位（user < project）はスキーマの docstring が名言しており
（`claude-code-lsp-plugin-spec-measured`）、M2 の実測が「project 側が効いている」ことを
挙動で裏づけているため、追加の確認は不要と判断した。
