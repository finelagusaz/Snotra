# research — issue #481（Phase 1b: block-main-commit の削除）

## issue の要約

`block-main-commit`（PreToolUse hook）を `.claude/settings.json` から削除し、それが根拠となっていた CLAUDE.md の運用ルール 2 本を消す。main 保護は #480 で `.githooks/` + GitHub ruleset へ移った。

**削除の理由は「不要になった」ではなく「そもそも守れていなかった」。** PowerShell tool に発火せず、`git -C` を検出せず、`push` を語彙に持たず、`tool_input` 全体を grep するため誤爆する。

## 前提（トリガー）の裏取り — 実測済み

| 確認項目 | 結果 |
|---|---|
| #480 の状態 | `MERGED`（`10cc510`） |
| `main` の tip | `10cc510` |
| `main` のツリーに `.githooks/pre-commit` | 存在する（`create mode 100755`） |
| `core.hooksPath` | `.githooks` |
| **本物の main で PowerShell tool から `git commit --allow-empty`** | `exit 1` / `BLOCKED: main への直接コミットは禁止です。` |

**Layer 1 は稼働している。** ゆえに `block-main-commit` を削除しても安全網は空にならない（issue の非目標だった状態を回避できている）。

## 関連コード

### 1. `.claude/settings.json`

```
PreToolUse[0].hooks[0]  ← 削除対象（block-main-commit）
PreToolUse[0].hooks[1]  ← PR 前 push チェック。残す
PostToolUse[0]          ← 不変
enabledPlugins          ← 不変
```

**重要な発見**: `.claude/settings.json` に **`block-main-commit` という文字列は存在しない**。hook オブジェクトは `type` と `command` しか持たず、`block-main-commit` は CLAUDE.md で我々が付けた呼び名にすぎない。したがって「名前で grep して消す」ことはできず、**配列の第 0 要素を位置で特定する**。

削除対象の実体（`settings.json:9`）:

```
input=$(cat); if echo "$input" | grep -qE 'git\s+(commit|merge|rebase)' && [ "$(git branch --show-current)" = 'main' ]; then ... exit 2; fi
```

### 2. `.claude/hooks/post-edit.mjs:242-243`

```js
/**
 * `.claude/settings.json` が壊れると、PostToolUse だけでなく block-main-commit を
 * 含む全 hook が停止する。パースは実質 0ms なので、hook 系を編集したら必ず見る。
 */
```

削除後は虚偽になる。**振る舞いは変えない**（コメントのみ）。

### 3. `CLAUDE.md`（5 箇所）

| 行 | 内容 | 処置 |
|---|---|---|
| 16 | 最重要ルール 2「`git` コマンドを `&&` でチェーンしない」 | **narrow**（`gh pr create` 限定）。番号 1〜4 は保つ |
| 41 | 「`git` コマンドをチェーンしない」— 根拠は `block-main-commit` の誤爆 | **narrow**（同上） |
| 42 | 「main の同期は `git pull --ff-only`」— 根拠は `block-main-commit` が `merge --ff-only` を弾くこと | **削除**（`pre-merge-commit` が本物の判定をする） |
| 49 | フック表の直前。「`block-main-commit` は漏れがあり…Phase 1b で削除予定」 | 最後の一文を削除 |
| 53 | フック表の `block-main-commit` 行 | **削除** |

### 4. `.claude/skills/**`（ユーザー判断で scope に追加）

narrow すると、一般形の禁止を引用している側が孤立する。

| ファイル:行 | 内容 |
|---|---|
| `start-issue/SKILL.md:38` | 「以下の git コマンドは**チェーンせず**…（CLAUDE.md「Git/GitHub 運用」）」← 引用が陳腐化する |
| `start-issue/SKILL.md:117` | 「以下は**チェーンせず**…」（引用なし） |
| `implement/SKILL.md:81` | 「（`git add` と `git commit` はチェーンせず独立した呼び出しで実行する）」 |

`.githooks/pre-rebase` は**実行時に実ツリーで**判定するため、`git checkout x && git rebase main` はもう誤発火しない。一般形の禁止は根拠を失う。

**残る本物の誤爆は `gh pr create` のみ**（#482 まで）:

```
if grep -qE 'gh\s+pr\s+create'; then up=$(git rev-parse ... @{u}); if [ -z "$up" ] ...
```

この hook はコマンド実行の**前**に `@{u}` を評価する。ゆえに `git push -u origin HEAD && gh pr create` は必ずブロックされる。

### 5. 影響を受けないもの（確認済み）

- `.claude/hooks/post-edit.test.mjs` — `block-main-commit` への参照 **0 件**。テスト変更不要
- `.claude/skills/start-issue/SKILL.md:45` の `git pull --ff-only` — main の同期コマンドとして正しい。**回避策ではなくなっただけ**なので残す
- `AGENTS.md` — `block-main-commit` / チェーン禁止への参照なし
- `SPEC.md` — アプリの仕様。エージェント運用は対象外。**更新不要**
- `docs/build-commands.md` — 参照なし
- 他の issue が「最重要ルール **4**」を番号で参照している（#473/#475/#476/#477/#479）→ **番号 1〜4 を保つ**

## 既存パターン

- Phase 1a（#480）で `.githooks/` を導入し、`.gitattributes`・`npm prepare`・16 テストで固めた
- Phase 1a の CLAUDE.md 変更は **追記のみ**だった。今回が初めての削除
- Phase 1a のレビューが確立した規律: **主張の確度を測定の等級に合わせる**（実測 / read-back のみ / 視界外）

## 技術的制約

- **`.claude/settings.json` の編集は file watcher が即座に拾う**。壊れた JSON を書いた瞬間、`block-main-commit` だけでなく **PostToolUse を含む全 hook が停止する**
- そして `post-edit.mjs` の `validateSettings`（JSON 検証）は、**停止する側の hook の中にある**。settings.json が壊れたとき PostToolUse が発火するかは **未実測**（`post-edit.test.mjs` に `validateSettings` の直接テストは無い）
- したがって「沈黙 = 合格」に頼ってはならない。**JSON の妥当性は明示的に検証する**
- `.claude/settings.json` の変更は CLAUDE.md 最重要ルール 4 の対象。**エージェントが書いた計画への承認は、この操作の許可と同一ではない**（Phase 1a で harness の classifier がサブエージェントへの委譲を正しく拒否した）

## 未解決の疑問

- **無し。** 唯一の要求の曖昧さ（スキル 2 本の扱い）はユーザーに確認済み → 「スキルも narrow に合わせる」
