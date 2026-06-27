# plan.md — issue #382

## 種別判定（AGENTS.md ステップ 0）

**バグでも仕様変更でもない＝テスト堅牢化（flake 除去）**。
- アプリの挙動・IPC 契約・状態遷移・SPEC 記載のフローは一切変えない。E2E が検証する不変条件（初期状態で先頭行が selected）も不変で、その**検証の仕方だけ**を堅牢化する。
- → **SPEC.md 更新は不要**。

## 変更ファイル一覧

| ファイル | 変更内容 |
|---|---|
| `e2e/tauri.slash.e2e.ts` | L545-548 の掴み置き型を `driver.wait` 内 re-find 型へ置換（issue 提示コード）。同テスト L554-568 の既存型に揃える |
| `docs/build-commands.md` | 「E2E/スモーク運用メモ」（L71-83）に一般則を一行追記: 再レンダリングされうる要素（`.result-row` 等）の属性アサーションは掴み置きせず `driver.wait` 内で毎回 re-find する |

## 実装順序（フェーズ）

依存関係はなく独立。1 フェーズで完結。

1. `e2e/tauri.slash.e2e.ts` L545-548 を置換
2. `docs/build-commands.md` に一行追記

## 具体的な変更

### `e2e/tauri.slash.e2e.ts` L545-548

```ts
// before
// 初期状態: 先頭行が selected
const rows = await driver.findElements(By.css(".result-row"));
const firstClass = await rows[0].getAttribute("class");
expect(firstClass).toContain("selected");

// after
// 初期状態: 先頭行が selected（再レンダリングで stale 化しないよう wait 内で毎回 re-find）
await driver.wait(async () => {
  const r = await driver.findElements(By.css(".result-row"));
  if (r.length === 0) return false;
  return (await r[0].getAttribute("class"))?.includes("selected") ?? false;
}, 4_000, "初期状態で先頭行が selected にならない");
```

### `docs/build-commands.md`「E2E/スモーク運用メモ」末尾に追記

```md
- **再レンダリングされうる要素（`.result-row` 等）の属性アサーションは掴み置きせず `driver.wait` 内で毎回 re-find する**: `findElements` の戻り値を保持して `getAttribute` を読むと、検索デバウンス（leading + trailing 50ms）の trailing リフレッシュが結果リストを再レンダリングした瞬間に `StaleElementReferenceError` で flake する（#382）。安定要素（`.search-input`）は掴み置きでよい
```

## 不変条件

- **検証する不変条件は不変**: 「初期状態で先頭行（`r[0]`）が `selected` クラスを持つ」を引き続き検証する。before は掴み置き 1 回読み、after は最大 4 秒のポーリングで「いずれかの時点で `r[0]` が selected」を確認する。selected が安定状態である限り両者は同値で、after は trailing リフレッシュ後の再レンダリングも待てる点で真に堅牢。
- **タイムアウト 4_000ms は同テストの ↓/↑ 確認（L558, L568）と同一**。新しい待機定数を導入しない。
- **assertion セマンティクスの後退に注意**: `driver.wait` は条件成立で `true` を返すだけのため、`expect(...).toContain` のような明示 assert は無くなるが、タイムアウト時に第4引数のメッセージで throw するため**失敗検知は維持される**（L554-568 と同じ堅牢型）。むしろ「再レンダリング途中の一瞬の非 selected」を誤検知しなくなる。
- **失敗・異常時の挙動**: selected が一度も付かなければ 4 秒後に `"初期状態で先頭行が selected にならない"` で throw → テスト失敗として正しく顕在化。新たな状態フラグ・プロセス・リソースは導入しない（破棄ペアの懸念なし）。

## テスト方針

- **対象テスト自体が E2E テスト**。追加のユニットテストは不要（テストコードの堅牢化であり、プロダクトコードの挙動変更がない）。
- **検証コマンド（`docs/build-commands.md` 変更後検証チェックリスト）**:
  - 変更ファイルは `.ts`（E2E）と `.md` のみ。`.rs` / `.tsx` / UI ロジックには触れない。
  - カテゴリ判定: E2E テストファイル変更 → **カテゴリ C 相当（E2E）**。PR に `e2e` ラベルを付与し `E2E & Smoke` workflow をグリーン確認（受け入れ条件）。
  - ローカル: `npm run e2e:tauri`（任意・flake は非決定的のため「失敗の消滅」は CI グリーン継続で確認）。
  - lint: E2E ファイルは ESLint/typecheck 対象。`.ts` 編集の PostToolUse typecheck フックで自動検証される。
- **SPEC.md 更新要否**: 不要（前述のとおり挙動変更なし）。

## セルフレビュー

### 5a. check スキル該当性

| スキル | 判定 | 根拠 |
|---|---|---|
| `/plan-review`（常時） | **スキップ（過剰）** | issue が完成形コードを提示・同一テスト内に堅牢型テンプレート既存・4 行の E2E 堅牢化。並列 fan-out は CLAUDE.md「実行バイアス」/憲章「やりすぎ歓迎」に照らし不釣り合い。希望時に実行可 |
| `/symmetric-check` | 非該当 | 対称ペア（show/hide・clicked/double-clicked・enter/exit）に触れない |
| `/cache-check` | 非該当 | キャッシュ/incremental 再利用ロジックに触れない |
| `/state-check` | 非該当 | selected 状態を*検証*するが UI モード・ガード・遷移を*変更*しない |
| `/race-check` | 実質非該当 | 追加する async クロージャは同テスト L554-568 と同形のテスト待機。プロダクション async の状態競合ではない |

### 5b. セルフレビューチェックリスト

1. **対称コードパス**: 対称ペアなし（テスト assertion の堅牢化のみ）。同テストの ↓/↑ 確認（L554-568）は既に堅牢型で、初期確認だけが非対称に旧型だった → 本修正で**型が対称に揃う**（むしろ既存の非対称を解消）。
2. **影響範囲の網羅性**: `.result-row` 掴み置き + `getAttribute` の全箇所を grep 済み。属性を読む掴み置きは L546-548 が唯一（L528/733/747/775 は `.length` のみ）。下流影響なし。
3. **境界条件**: `r.length === 0`（結果未表示）を `return false` で扱い、4 秒間ポーリング継続 → 直前の L539-543 で「2 行以上表示」を保証済みのため通常は即 true。タイムアウト時はメッセージ付き throw で失敗顕在化。
4. **リソース管理**: 新規リソース（listen/Observer/プロセス/フラグ）なし。生成/破棄ペアの懸念なし。
5. **既存パターンとの整合**: 新規パターン導入なし。同ファイル L554-568 の既存 `driver.wait` 内 re-find 型へ揃えるのみ。
6. **YAGNI 違反**: なし。修正は L545-548 の置換 + docs 一行のみ。スコープ厳守。
7. **シンプル化の挑戦**: 新たな状態・抽象を導入しない。むしろ「掴み置き 1 回読み」より「wait 内 re-find」の方が WebDriver の stale 失敗モードに対して構造的に堅牢で、判断は単純化される。「selected が一度も付かなければ throw」を設計段階で明記済み。
8. **破壊不変条件の明示**: 本変更は E2E テストの検証ロジックのみで、Win32 フック・ホットキー・IPC など「戻ってこない」系には触れない。検知手段 = CI `E2E & Smoke` グリーン（受け入れ条件）。リグレッションリスクは「アサーション弱体化」だが、タイムアウト throw で失敗検知が維持されることを不変条件として確認済み。

### 修正した点

- self-review の結果、計画自体に修正は不要（issue 提示の修正がそのまま最小・最適）。**docs 一行追記を「検討」から「実施」に確定**（一般則は再発防止価値が高く、E2E メモの既存記法に自然に収まるため）。

