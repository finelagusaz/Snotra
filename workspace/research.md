# Research: ResultsWindow アイコンキャッシュ LRU 化 (#164)

## issue の要約

ResultsWindow.tsx の `iconCache` が無制限に成長し、Blob URL が適切に解放されない。長時間運用時のメモリ増加を抑えるため、LRU キャッシュ化と Blob URL の適切な解放を行う。

## 対応内容（issue より）

1. フロント側 iconCache を件数上限付き LRU に変更
2. shouldShow=false または結果クリア時に未使用 Blob URL を `URL.revokeObjectURL()` する
3. アイコン取得を初回テキスト描画後に遅延実行する

## 関連コード

### 主要ファイル: `ui/src/components/ResultsWindow.tsx`

- **`iconCache` シグナル** (L11-13): `Map<string, string>` — パス→Blob URL。無制限成長。
- **`iconUrls` Set** (L71): 全 Blob URL を追跡。`revokeAllIconUrls()` で一括解放。
- **`parseBinaryBatch()`** (L37-61): バイナリ→Blob URL 変換。`iconUrls.add(url)` で追跡。
- **`fetchIcons()`** (L73-97): `results-data-changed` 受信時に呼ばれる。キャッシュにないパスのアイコンをバッチ取得。
- **`revokeAllIconUrls()`** (L64-69): 全 URL 解放。現在は `onCleanup` と `show-icons-changed(false)` でのみ呼ばれる。

### イベントリスニング

- **`results-data-changed`** (L170-180): 結果配列変更時。`fetchIcons` を呼ぶ。
- **`results-selection-changed`** (L184-190): 選択変更のみ。
- **`results-visibility-changed`**: **未リスン**。非表示時の Blob URL 解放ポイントがない。

### 発行側: `ui/src/stores/search.ts`

- `emitVisibilityChanged(reason)` は `shouldShow: false` で emit。以下のタイミング:
  - コマンド実行時 (L86, L142, L254)
  - インデックス中 (L160)
  - launch 開始時 (L414, L444, L521, L545)

## 既存パターン

- `revokeAllIconUrls()` は既に存在し、全解放パターンがある
- `iconUrls` Set で Blob URL を追跡するパターンも既存
- SolidJS シグナルの `Map` 更新は `new Map(cache)` でイミュータブルに行う（L92-96）

## 技術的制約

- **Blob URL のライフサイクル**: `URL.createObjectURL()` で作成した URL は `URL.revokeObjectURL()` で解放しないとメモリリーク
- **SolidJS リアクティブ**: `iconCache` は `createSignal` で管理。Map を更新するたびに新しい Map を作る必要がある
- **LRU 実装**: 外部ライブラリなし。`Map` の挿入順序を利用して簡易 LRU を実現可能（Map は挿入順でイテレーションする）
- **遅延実行**: `fetchIcons` を `requestAnimationFrame` で1フレーム遅延させれば、テキスト描画後にアイコン取得を開始できる
- **非表示時の解放戦略**: `results-visibility-changed` をリッスンして全 Blob URL を解放

## 妥当性チェック

### 1. LRU キャッシュのサイズ上限

issue は「件数上限付き LRU」と言っている。適切な上限値は？
- 最大表示件数はデフォルト 8（`config.toml` で変更可能）
- 検索を繰り返すとキャッシュが蓄積する。100〜200 件が妥当か
- **結論**: 定数 `MAX_ICON_CACHE_SIZE = 200` で十分。設定値にする必要はない（YAGNI）

### 2. `results-visibility-changed` リスン追加の影響

- 現在 ResultsWindow はこのイベントをリスンしていない
- 追加しても他のリスナーに影響なし（results ウィンドウ内で閉じる）
- **結論**: 安全に追加可能

### 3. 遅延実行の方法

issue は「アイコン取得を初回テキスト描画後に遅延実行する」と言っている。
- 現在 `fetchIcons` は `results-data-changed` ハンドラ内で即座に呼ばれる
- `requestAnimationFrame` でテキスト描画フレーム後に遅延させるのが最もシンプル
- **結論**: `fetchIcons` 呼び出しを `requestAnimationFrame` でラップ

### 4. 非表示時の解放範囲

- `results-visibility-changed` は結果を非表示にするとき（`shouldShow: false`）
- 非表示時は全 Blob URL を解放してよいか？→ 次に表示されるときは新しい検索結果なので、古いキャッシュは不要。全解放で問題ない
- ただし iconCache の Map もクリアする必要がある（revoke 済み URL を参照させないため）
- **結論**: 非表示時は `revokeAllIconUrls()` + `setIconCache(new Map())` で全解放

### 5. LRU eviction 時の Blob URL 解放

- LRU からエントリを追い出すとき、対応する Blob URL を `revokeObjectURL` する必要がある
- `iconUrls` Set からも除去する必要がある
- **結論**: eviction 関数で revoke + Set 除去を行う

## 未解決の疑問

なし。issue の要求は明確で、既存パターンの拡張で対応可能。
