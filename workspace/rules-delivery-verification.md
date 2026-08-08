# `.claude/rules/comments.md` 配送の実測（2026-08-08・新鮮な context のサブエージェント）

セッション内で最初に行った tool 呼び出しが手順 1 の `Read` である（`.claude/rules/` は手順 3 完了後まで一切開いていない）。

## 1. 手順 1（`snotra-core/src/config.rs` を `limit: 15` で Read）直後に配送された system-reminder

`Contents of ...` 行の逐語引用（この Read 直後の system-reminder に現れた全 3 件・省略なし）:

- `Contents of C:\workspace\Snotra\snotra-core\CLAUDE.md:`
- `Contents of C:\workspace\Snotra\.claude\rules\comments.md:`
- `Contents of C:\workspace\Snotra\.claude\rules\snotra-core.md:`

（同じ system-reminder ブロックには MCP サーバ指示・利用可能な agent 型の一覧も含まれていたが、`Contents of <path>` の形で本文が配送されたファイルは上の 3 件だけである。`.claude/rules/snotra-core-search.md` は配送されなかった＝`config.rs` は当該 rule の `paths` に一致しない。）

## 2. 手順 3（`snotra-egui-runtime/src/repaint.rs` を `limit: 15` で Read）直後に配送された system-reminder

`Contents of ...` 行の逐語引用（全 1 件）:

- `Contents of C:\workspace\Snotra\snotra-egui-runtime\CLAUDE.md:`

`comments.md` の**再配送は無かった**（この Read で新規に配送されたのは `snotra-egui-runtime/CLAUDE.md` のみ）。**手順 3 は判定 2 の測定として不成立である** — 手順 1 が `comments.md` を配送した時点で、同一セッション内での再配送は重複排除に隠れて観測できなくなっていた。この穴は下記「2b」の追加測定で塞いだ。

## 2b. 判定 2 の測定（新鮮な context のサブエージェント・重複排除の影響を受けない経路）

判定 2 は同一セッションでは原理的に測れないため、`comments.md` を一度も配送されていない**新鮮な context のサブエージェント**を 1 体起動し、その**最初の tool 呼び出し**として `C:\workspace\Snotra\snotra-egui-runtime\src\repaint.rs` を `limit: 15` で Read させた（`.claude/rules/` を事前に開かない制約を明示的に渡した）。返ってきた `Contents of ...` 行は逐語で 2 件:

```
Contents of C:\workspace\Snotra\snotra-egui-runtime\CLAUDE.md:
Contents of C:\workspace\Snotra\.claude\rules\comments.md:
```

配送本文の先頭は `# コメントの書き方（ルーター）` → 空行 → `正本は ...` で、`---` frontmatter は含まれていなかった（本セッションの手順 1 での配送形と一致）。

## 3. 判定 1（既存 rule が隠されていないか）

**`BOTH`** — 手順 1 で `snotra-core.md` と `comments.md` の両方が配送された。既存 rule は隠されていない（加えて crate の `CLAUDE.md` も従来どおり配送されている）。

## 4. 判定 2（今までカバーの無かった crate へ届くか）

**`DELIVERED`** — 根拠は上記「2b」の新鮮な context での実測（`snotra-egui-runtime/src/repaint.rs` の Read で `comments.md` が配送された）。本セッションの手順 3 だけでは重複排除により観測できず、そちらは測定として不成立だった。

補強として、手順 3 完了後に読んだ `.claude/rules/comments.md` の frontmatter は次を持つ:

```
---
paths:
  - "snotra-core/**/*.rs"
  - "snotra-egui-runtime/**/*.rs"
  - "snotra-settings/**/*.rs"
  - "src-tauri/**/*.rs"
---
```

`snotra-egui-runtime/src/repaint.rs` は 2 番目の glob に一致する（これはファイル内容からの推論であり、配送の観測は「2b」が持つ）。

## 5. 根拠（逐語）

手順 1 直後の system-reminder に現れた `Contents of` 行（再掲・逐語）:

```
Contents of C:\workspace\Snotra\snotra-core\CLAUDE.md:
Contents of C:\workspace\Snotra\.claude\rules\comments.md:
Contents of C:\workspace\Snotra\.claude\rules\snotra-core.md:
```

手順 3 直後の system-reminder に現れた `Contents of` 行（逐語）:

```
Contents of C:\workspace\Snotra\snotra-egui-runtime\CLAUDE.md:
```

配送された `comments.md` 本文の先頭 2 行（逐語。配送本文は `---` frontmatter を**含まず**、`#` 見出しから始まっていた＝これは配送されたブロックそのものの観測である。2 行目は空行なので 3 行目も添える）:

```
# コメントの書き方（ルーター）

正本は `docs/comment-guidelines.md`。本 rule は「どこを読むか・何を実行するか」だけを示す（要約を置かない）。
```

ディスク上の `comments.md`（手順 3 後に読んだもの）の 9・11 行目と一致する＝同一ファイルが配送されている。

## 6. ⚠️ 確信の持てない所見

- **指示された手順（同一セッションで手順 1 → 手順 3）は判定 2 を測れない設計だった。** 手順 1 が `comments.md` を配送した時点で観測経路が閉じるため、手順 3 の「再配送なし」は (a) glob 不一致 と (b) 重複排除 を区別できない。判定 2 は**サブエージェントによる別 context の測定**（「2b」）で決着させており、本セッションの手順 3 単体を根拠に `NOT_DELIVERED` と読んではならない。
- 呼び出し側が挙げた「harness が起動時に `.claude/rules/` を読み、セッション途中に作られた新ファイルを見ていない」という説明は、**両判定で当たらないことが分かっている**（`comments.md` は本セッションの手順 1 と、後から起動した別 context の両方で実際に配送された）。
- **重複排除の粒度（ファイル単位か本文ハッシュ単位か、rule ごとか一括か）は観測していない。** 手順 3 で `comments.md` が現れなかった機序を「重複排除」と呼んでいるが、その実装は測っていない（`comments.md` が配送されうる状況で現れなかった、という事実だけが観測である）。
- **2b はサブエージェントの報告であり、その system-reminder を私自身は見ていない。** 委譲先の逐語引用を信頼している（一次証拠を直接見た測定は判定 1 側だけである）。ただし 2b の報告は本セッションの手順 1 で私が直接見た配送形（frontmatter なし・同じ先頭行）と一致しており、独立に整合する。
- **`snotra-settings/**/*.rs` と `src-tauri/**/*.rs` の 2 つの glob は一度も測っていない。** frontmatter に書かれているだけである（今回の依頼の射程外）。
- 手順 1 では `snotra-core-search.md` が配送されなかった。これは `config.rs` が当該 rule の `paths` 外である帰結と読めるが、その rule の frontmatter を確認していないため断定しない（本タスクの射程外）。
- **付随観測**: 手順 3 の後に `.claude/rules/comments.md` 自身を Read したところ、`Contents of C:\workspace\Snotra\.claude\rules\safety-nets.md:` が配送された（`safety-nets.md` の `paths` が rules 自身を含むため）。測定の 2 手順は完了後だったので判定には影響しないが、記録として残す。
