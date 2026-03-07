# Retrospective — Alt 残留ビープ音の根本修正 (#155, PR #161)

## よかったこと

### 多角的調査で根本原因を3層に分解できた
当初は「SendInput のタイミングを直せば解消する」と想定していたが、手動テストで効果がないことを確認し、3方向の並行調査（HWND 階層、SendInput タイミング、WebView2 コールドスタート）に切り替えた。結果として問題が3層（SendInput レース、レンダラスロットル、WM_SYSCHAR ネイティブ処理）で構成されていることを特定し、正しい層（ネイティブ側の AcceleratorKeyPressed）に対策を打てた。

### 公式 API を使った根本対策にたどり着いた
HWND サブクラスや WH_GETMESSAGE フックなど複雑な代替案を調査した上で、WebView2 公式 API の `AcceleratorKeyPressed` が最適解であることを検証できた。Tauri の `with_webview()` → `controller()` 経由でアクセスできることも確認し、安定性の高い実装になった。

### 段階的アプローチで無駄な実装を防いだ
MenuMaskKey → タイミング修正 → AcceleratorKeyPressed と段階的に進め、各段階で手動テストして効果を確認した。旧フェーズ 2（RAF 1フレーム化）、旧フェーズ 3（Alt ガード文字救済）は P1 の根本対策で不要になったため見送り、YAGNI を遵守した。

---

## 伸びしろ

### 初手の仮説が浅く、2回の手戻りが発生した
1. 最初の仮説: 「SendInput で Alt key-up を送れば解決する」→ MenuMaskKey を実装
2. 手動テストで効果なし → `send_alt_key_up()` を `show+set_focus` の後に移動
3. 再度手動テストで効果なし → 多角的調査で AcceleratorKeyPressed にたどり着く

**根本原因**: `WM_SYSCHAR` → `DefWindowProc` → `MessageBeep` の経路を最初に特定していれば、「SendInput でキー状態をクリアする」アプローチが層を間違えていることに気づけた。**通知音のファイル名を先に特定し、そこから逆引きで発生経路を特定する**という手順を最初から踏んでいれば、手戻りを1回減らせた。

### Win32 の非同期性への理解が不足していた
`SetForegroundWindow` が部分的に非同期であること、`SendInput` のルーティングがキュー取り出し時に決定されることを知らなかった。Raymond Chen のブログ記事で初めて理解した。Win32 API を使う場合、「同期に見えて非同期な API」のリストを事前に把握しておくべきだった。

---

## ネクストアクション

- [ ] PR #161 をマージ
- [ ] Issue #159（ホットキー登録失敗通知）の実装
- [ ] Issue #160（ホットキーバリデーション見直し）の実装
