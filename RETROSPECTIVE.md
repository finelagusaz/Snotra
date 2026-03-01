# Retrospective — アイコン転送 base64 → バイナリ IPC 移行

## よかったこと

### 失敗の根本原因を eprintln 診断で確定してから代替案を選んだ

Custom URI Scheme が「なんとなく動かない」ではなく「WebView2 の `SetCustomSchemeRegistrations` 未宣言により `AddWebResourceRequestedFilter` にリクエストが届いていない」と一文で特定できていた（revert コミットに記録済み）。この根本原因が明確だったことで、代替案（`tauri::ipc::Response`）の有効性を迷わず判断できた。

### バイナリ IPC への移行が最小変更で完結した

変更ファイルは 8 件だが、変更の本質は「base64 encode/decode の除去」と「キャッシュ型変更」の2点のみ。パイプラインの構造（`SHGetFileInfoW` → BGRA → PNG）、キャッシュの永続化（`icons.bin`）、フロントのフィルタロジックはすべて据え置きで、転送層だけ入れ替えられた。

### /simplify で tracedInvoke 見落としを検出できた

`getIconPng` が唯一 `tracedInvoke` を使わずパフォーマンス計測ブランチとして致命的な抜けだったが、コードレビューエージェントが検出した。実装完了後に `/simplify` を走らせる習慣が機能した。

---

## 伸びしろ

### YAGNI 判断が揺れた（bgra_to_png_bytes の統合→再分割）

revert コミットが「`bgra_to_png_bytes` 抽出は維持」と明記していたにもかかわらず、`/simplify` で「唯一の呼び出し元が1つ＝YAGNI」として統合した。その後バイナリ IPC 実装で再び分割の有用性が出てきたケースで、判断の一貫性を保てなかった。「意図的に維持した分割」かどうかをコミットメッセージから読み取るチェックが必要。

### ObjectURL の cleanup 計画を実装前に明示しなかった

`URL.createObjectURL` を導入する際、破棄の「場所・構造・理由」を事前に計画せず実装を先に書いた。結果として `/simplify` のレビューで指摘を受けてから `onCleanup` を追加する流れになった。CLAUDE.md の「リソース管理は生成/破棄ペアで計画する」原則を適用できていなかった。

---

## ネクストアクション

- [ ] 動作確認: `npm run tauri dev` でアイコン表示・非表示設定・インデックス再構築後の再抽出を手動確認
- [ ] 次のボトルネック: `Mutex<IconCache>` を保持したまま `SHGetFileInfoW` を呼ぶ直列化問題（PERFORMANCE.md §3 に記録済み）を Issue 化するか判断する
- [ ] 「意図的に維持した構造」はコミットメッセージに `[intentional]` 等の印を付ける運用を検討する
