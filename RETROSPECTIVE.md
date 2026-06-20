# Retrospective — instantCommandMode 持続ラッチを interpKind 純粋導出へ（#374 二軸モデルの完成: 調査 → plan-review → 段階的 TDD → state-check → code-reviewer → SSOT 化）

## よかったこと

### 段階的 TDD が「latch ＝ 純粋導出」を実証してから latch を抜いた
Phase 0 で先にテスト assertion を `instantCommandMode()` → `interpKind()` へ移行し、latch がまだ生きている状態で緑を確認＝「interpKind は latch の無損失な再パッケージ」という #374 の主張をテストが裏書きした。その上で Phase 1（導出化・latch 残置）→ 2（latch 撤去）と刻み、各段階 228 テスト緑で挙動不変を連続担保。挙動不変リファクタを可逆・安全に進められた。

### レビューの「要対処」を旧/新どちらの不変条件で裁いているか検証して誤警報を見抜いた
plan-review の Agent 2 が「309 ガード置換は IPC-stale で破綻（要対処）」と提起した。だが検証すると、Agent 2 は「`instantCommandMode` が固着する」という**旧不変条件で新世界を裁いて**いた——latch を消した後は固着すべき latch が存在せず、当の stale シナリオでは掃除対象（timer/items）が空＝no-op、interpKind も "plain" で誤起動が防がれる。レビュー所見を鵜呑みにせず、変更後の不変条件に照らし直す価値の好例。

### code-reviewer の「述語二重化」を受けて SSOT を移管した
latch は「モードフラグ」であると同時に instant 検出述語の唯一の評価点（SSOT）でもあった。消した結果 `interpKind` と query effect に述語が二重化（Medium 1）。`isInstantPrefix` ヘルパへ抽出し SSOT を移管。「散在を消したつもりが別の散在を生む」を自己検出し、development-principles の原則を自分の変更に適用できた。→ 一般則を development-principles へ抽出済み。

### SPEC が実装の行き先を既に示していた
SPEC §8.6 L437-438 は instant 遷移を `Input [query startsWith prefix]` と最初から query 派生で記述していた。純粋導出はラッチという状態プロキシを外し、実装を SPEC の概念モデルに一致させた＝SPEC 更新不要どころか実装が仕様に追いついた。仕様が「あるべき導出形」を既に持っていることがある。

---

## 伸びしろ

### 「latch が担う SSOT 役割」を plan 段階で問えていなかった
code-reviewer が拾った述語二重化は、plan 時点で「この latch を消すと、latch が単一情報源だった判定はどこへ散るか」を問えば設計段階で防げた。状態削除の plan では「その状態が暗黙に担う SSOT 役割の移管先」を必ず設計項目に入れる。→ development-principles に反映済み。

### 全消費者 grep を research で見たが、テストモックの導出構造まで降りていなかった
research で `SearchWindow.test.tsx` の `instantCommandMode` 参照は把握したが、`mockInterpKind` が `mockInstantCommandMode` から**導出**している構造は plan-review（Agent 3）まで降りて初めて明示化された。テストモックも「派生アクセサがどの下位モックから導出されるか」まで research 段階で追うと、移行手順の精度が上がる。
