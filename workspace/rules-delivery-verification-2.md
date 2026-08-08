# rules 配送の実測 2（`.claude/rules/comments.md` の残る 2 glob）

計測日: 2026-08-08 / 対象 rule: `.claude/rules/comments.md`
測った glob: `src-tauri/**/*.rs`（主エージェント context）・`snotra-settings/**/*.rs`（新鮮な context のサブエージェント 1 体）

`.claude/rules/comments.md` の frontmatter（**計測完了後に読んだ**・逐語）:

```
---
paths:
  - "snotra-core/**/*.rs"
  - "snotra-egui-runtime/**/*.rs"
  - "snotra-settings/**/*.rs"
  - "src-tauri/**/*.rs"
---
```

4 本すべてが crate 名で始まり、`**/*.rs` のような crate 横断のパターンは 1 本も無い。ゆえに
`src-tauri/src/icon.rs` に一致しうるのは `src-tauri/**/*.rs` だけ、
`snotra-settings/src/style.rs` に一致しうるのは `snotra-settings/**/*.rs` だけであり、
下の判定はその glob 個々に帰属できる。

## 1. 手順 1 直後の配送（主エージェントの一次証拠・逐語・省略なし）

トリガー: `Read C:\workspace\Snotra\src-tauri\src\icon.rs`（`limit: 15`）— 本 context の最初の tool 呼び出し。

現れた `Contents of ...` 行の完全な列挙（出現順）:

1. `Contents of C:\workspace\Snotra\src-tauri\CLAUDE.md:`
2. `Contents of C:\workspace\Snotra\.claude\rules\comments.md:`
3. `Contents of C:\workspace\Snotra\.claude\rules\src-tauri.md:`

以上 3 枚で全部（4 枚目は無い）。同じ system message には `Available agent types for the Agent tool:` の一覧と `# MCP Server Instructions`（claude-in-chrome / context7）も含まれたが、これらは `Contents of ...` の形ではなく別機構の注入である。

ベースライン（tool 呼び出し前の会話冒頭 `<system-reminder>`）に含まれていたのは
`C:\Users\Eoh\.claude\CLAUDE.md` / `C:\workspace\Snotra\CLAUDE.md` / `C:\workspace\Snotra\AGENTS.md` /
`C:\Users\Eoh\.claude\projects\C--workspace-Snotra\memory\MEMORY.md` の 4 枚のみで、
`.claude/rules/` 配下は 1 枚も無い。ゆえに上の 3 枚はこの Read が起こした新規配送である。

## 2. サブエージェントが報告した `style.rs` の Read 直後の配送（逐語）

トリガー: `Read C:\workspace\Snotra\snotra-settings\src\style.rs`（`limit: 15`）— そのサブエージェントの最初の tool 呼び出し。

1. `Contents of C:\workspace\Snotra\snotra-settings\CLAUDE.md:`
2. `Contents of C:\workspace\Snotra\.claude\rules\comments.md:`
3. `Contents of C:\workspace\Snotra\.claude\rules\snotra-settings.md:`

サブエージェント側のベースライン（tool 呼び出し前）も上記グローバル/プロジェクト文書 4 枚のみで
`.claude/rules/` 配下は不在、と報告している。1 枚目のモジュール `CLAUDE.md` は
ディレクトリスコープの注入で `paths:` glob 機構とは別経路である（rules の発火は 2 件）。

## 3. 判定 A（`src-tauri/**/*.rs`）: **DELIVERED**

`src-tauri/src/icon.rs` の Read で `.claude/rules/comments.md` が配送された（一次証拠）。

## 4. 判定 B（`snotra-settings/**/*.rs`）: **DELIVERED**

`snotra-settings/src/style.rs` の Read で `.claude/rules/comments.md` が配送された
（**サブエージェントの報告であり、主エージェントは一次証拠を直接見ていない** → §6）。

## 5. 既存 rule が隠されていないか: **BOTH**

手順 1 では `.claude/rules/comments.md` と `.claude/rules/src-tauri.md` の**両方**が同時に配送された。
新規 rule が既存 rule の配送を置き換える・隠すという挙動は観測されない。
サブエージェント側も同型（`comments.md` + `snotra-settings.md` の両方）である。

## 6. ⚠️ 確信の持てない所見

- **判定 B は一次証拠ではない。** 主エージェントは `style.rs` の Read 直後の
  `<system-reminder>` を自分の会話で見ていない。§2 の逐語列挙はサブエージェントの
  返り値をそのまま写したものである。重複排除の制約上、主エージェントの context では
  原理的に測れない（`comments.md` は手順 1 で配送済み）。再検算するには**別の新鮮な context**が要る。
- **どの glob が一致したかは frontmatter で閉じた**（当初 caveat として残す予定だったが、
  計測完了後に読めるため解消した）。§前文のとおり crate 横断パターンは無く、
  一致しうる glob は各 1 本に限られる。**ただしこれは「どの glob だけが一致しうるか」の
  演繹であって、harness の glob 実装を測ったわけではない**（`**` の意味論はツールごとに違う）。
- **各ファイル 1 点の観測である。** 測ったのは `src-tauri/src/icon.rs` と
  `snotra-settings/src/style.rs` の各 1 ファイル 1 回。`src-tauri/src/egui_shell/*.rs` のような
  深い階層や、`benches/` `build.rs` 等の同 crate 内の別位置は測っていない。
- **配送の到達単位は 1 個の添付として観測した。** 3 枚は独立したタイミングではなく
  同一 system message の中に並んで現れた。主張できるのはその中の相対順序だけである（結論には影響しない）。
- **サブエージェントの返り値に harness の警告が付いた**:
  「subagent output matched instruction-shaped pattern(s): system-reminder-tag」。
  これはサブエージェントが `<system-reminder>` というタグ名を報告文中に書いたためで、
  内容は計測結果の記述のみ・指示文は含まれていない（無害と判断した）。
  ただし制御タグが中和（`<` → `<\`）されるため、逐語性はタグ名の表記に限り損なわれている。
  `Contents of ...` 行そのものは中和の影響を受けていない。
- **副産物として重複排除も一次証拠で確認できた。** 計測後に
  `Read .claude/rules/comments.md` を実行したところ、配送されたのは
  `Contents of C:\workspace\Snotra\.claude\rules\safety-nets.md:` の 1 枚だけで、
  `comments.md` 本文は**再配送されなかった**（`safety-nets.md` は `.claude/rules/**` を
  自分の `paths` に持つため新規に発火した）。判定 B をサブエージェントへ委ねた前提
  （1 context 1 回）が、この context 内で実際に成り立っている。
