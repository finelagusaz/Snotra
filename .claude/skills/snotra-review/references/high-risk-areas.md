# Snotra High-Risk Areas

## 目的

レビューで優先的に疑うべき高リスク領域をまとめる。
どれも「壊れると主要フローが欠ける」「発見が遅れる」「回帰範囲が広い」のいずれかを満たす。

## 1. 検索述語と増分検索

- incremental search
- kana / migemo
- slash command / instant command
- ranking と history boost

典型リスク:
- 述語拡張時に前回候補を誤再利用して false negative
- モード切替時に別モードの状態を引きずる

## 2. 初期化順と有効化順

- hotkey
- tray
- window event listener
- config watcher

典型リスク:
- リスナー準備前にイベント発火
- 起動時だけ壊れる race

## 3. 一時状態のライフサイクル

- 通知クリア timer
- `listen()` 購読
- UI 側の observer
- activation / launching guard

典型リスク:
- 前回ハンドルの破棄漏れ
- 二重購読や stale callback

## 4. 子プロセス管理

- `snotra-settings.exe` の起動
- 重複起動防止
- 終了監視
- exit 時 kill

典型リスク:
- 孤児プロセス
- alwaysOnTop 復元漏れ

## 5. 永続化

- history
- index
- icon cache

典型リスク:
- 部分書き込み
- 破損ファイルで起動失敗
- 保存失敗が主要フローに波及

## 6. UI と native window の境界

- 高さリセットと結果描画
- 表示/非表示
- E2E 可視判定

典型リスク:
- Rust 側と UI 側が別の真実源を持つ
- `visibilityState` を信用して誤検証

## 7. 多言語と設定反映

- OS 言語初期判定
- `language-changed`
- `instant-prefix-changed`
- `config.toml` watcher

典型リスク:
- 層ごとに反映タイミングがずれる
- 片側だけ古い設定で動く

## 8. レビュー時の優先順位

高リスク領域では次の順に見る。

1. 結果欠落や回復不能が起きるか
2. 正しさを守る fallback があるか
3. その fallback が全遷移で効くか
4. テストや trace で観測できるか
