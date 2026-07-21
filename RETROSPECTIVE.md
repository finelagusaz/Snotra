# Retrospective — #532 採用ゲート検証（#579/#580/#581/#582）+ #399 フォントバグ修正

## よかったこと

### 「一手ずつ提示 → 選択 → 実測 → 記録」の刻みが 4 ゲートで機能した
コールドスタート内訳・異 DPI・IME・署名更新の各ゲートで、計測手順を提示 → ユーザーが選択 → 実測 → issue コメントへ記録、のリズムを守った。ユーザーの好むテンポ（記憶 [[issue-532-softbuffer-pivot]]）に沿い、破壊的/外向きの操作（GUI 実行・issue close・鍵削除）は必ず着手前に名指しで合意した。

### トレースベースの客観化が目視を裏取りした
DPI トレース（`ScaleFactorChanged`/monitor 表）・IME イベントトレース（`Ime`/`KeyboardInput`）・framebuffer PPM ダンプ（→PNG→Read）で、視覚検証を客観信号で補強した。特に #582 は Ime/Key トレースが「Esc は変換中 IME 経路でキャンセル・Escape の KeyboardInput は 0 件」「IME 処理キーは `Process text=None` ＝二重投入なし」を厳密に確定させた。#579 の framebuffer ダンプは修正の before/after を pixel で撮れた。

### 既知教訓の grep が突破口、advisor が記録を較正した
#399（`snotra-settings/CLAUDE.md`「混在は単一フォントで」）を grep で発見して root cause を確定（ユーザーの「settings で対処した」という記憶とコード文書が一致）。記録の投稿前には advisor で過剰主張（ppp 丸め機構の断定・「settings と同じ」・p95 の標本数）を是正し、「観測できたことだけを書く」較正を効かせた。

### 同一パターン全コードパス検索で波及しつつ「やりすぎ」を回避した
#399 を `soft_host` で発見後 grep で列挙し、同じ可視バグを負う `soft_main`（softbuffer）へ限定波及。glow/wgpu bin は同 append パターンでも sub-pixel AA で不可視かつ #532 で不採用の旧レンダラゆえ非対象とし、面積を増やさなかった。教訓は使用箇所（`snotra-egui-mvp/CLAUDE.md` 不変条件）へ置いて発見可能性を是正した。

---

## 伸びしろ

### 視覚バグで第一印象に anchor した（#399 デバッグ）
composing 文字「t」の「太さ」を症状と読み違え、forced-ppp 再現まで走らせたが、太さは active clause の正常な強調だった。ユーザーの「ローマ字のベースラインがずれる」の一言で真の症状（フォント間ベースライン差）が判明した。視覚バグは症状語を先に正確化し（失敗モードを選ばせる）、第一印象で仮説を固めない。教訓はメモリ [[debug-visual-render-precise-symptom]] に記録済み。

### GUI probe を止めずに再ビルドして exe ロック、パイプが exit を隠した
実行中の検証 probe が exe を掴んだまま `cargo build` してリンク失敗（Windows `os error 5`）、しかも `| tail` が cargo の失敗を exit 0 に見せた。CLAUDE.md「壊れた出力から推論しない」で mtime 矛盾を捕捉し旧バイナリ検証を回避できた（既存機構が機能）。再ビルド前に probe を停止し、ビルド成否はパイプでマスクせず exit code で見る——機構が覆う near-miss ゆえ規範追加はせず習慣として持つ。
