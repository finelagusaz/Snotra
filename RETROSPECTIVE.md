# Retrospective — ResultsWindow アイコンキャッシュ LRU 化 (#164)

## よかったこと

### 計画が的確で実装フェーズの手戻りがゼロだった
`/start-issue` で作成した plan.md の4フェーズ構成（LRU クラス → ResultsWindow 統合 → テスト → CLAUDE.md）がそのまま実装順序として機能した。SolidJS のリアクティブ戦略（`iconCacheVersion` カウンタ）の設計判断も計画段階で確定できており、Phase 2 の実装でブレが発生しなかった。

### `iconUrls` Set 廃止による二重管理の解消
旧実装の `iconCache`（Map シグナル）+ `iconUrls`（Set）の二重管理を `LruIconCache` に一元化した判断は正しかった。URL のライフサイクル管理が1クラスに集約され、見通しがよくなった。

### アルゴリズムレビューで実害のない最適化と実害のある問題を切り分けられた
`evict()` の while→if 変更は軽微だが綺麗な改善。`iconCacheVersion` の全行再評価は構造的トレードオフだが表示件数が少なく実害なしと正しく判定し、過剰な最適化を避けた。

---

## 伸びしろ

### 追跡機構の移行時に「生成→登録」間の早期リターンパスを見落とした
コードレビューで Critical が検出された。`iconUrls` Set を廃止した際、`parseBinaryBatch`（URL 生成）→ stale guard → `cache.set()`（URL 登録）の間にある早期リターンパスで、生成済み URL が revoke されない経路を見落とした。

**根本原因**: 計画のセルフレビューで「`iconUrls.add(url)` の削除: cache.set 内で管理に移行」と書いたが、「旧 `iconUrls` が追跡していた全コードパス」を列挙しなかった。stale 棄却パスでは URL が `iconUrls` に add された後に棄却されても `revokeAllIconUrls()` で回収されていた — この暗黙の安全網が消えたことに気づかなかった。

**教訓**: リソース追跡機構を別の機構に移行するときは、旧機構が保護していた全パス（正常パス + エラーパス + 早期リターンパス）を列挙し、新機構でも同等の保護があることを確認する。`ui/CLAUDE.md` に Blob URL 管理の不変条件として反映済み。

---

## ネクストアクション

- [ ] PR を作成してマージする
