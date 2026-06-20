# リリース手順

## 署名鍵（minisign / Tauri updater）

Snotra の自動更新（Tauri updater / minisign）に使う Ed25519 署名鍵の管理ルール。

### 現行正本鍵

- **鍵 ID**: `D0897078B7A5555D`（v0.17.1 = 2026-06-14 で導入）
- **パスワード**: 空（`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` Secret は不要）
- **バックアップ**: 開発機 Dropbox — `001 TRANS/snotra/snotra-updater.key`（秘密鍵）と `.pub`（公開鍵）

### 不変条件

`TAURI_SIGNING_PRIVATE_KEY`（GitHub Secret: `finelagusaz/Snotra`）と `src-tauri/tauri.conf.json` の `plugins.updater.pubkey` は**必ず同一鍵ペア**であること。ズレると updater が次のエラーで必ず失敗する:

```
The signature was created with a different key than the one provided
```

pubkey の埋め込み箇所は `tauri.conf.json` の1か所のみ。

### 退役・再利用禁止の鍵 ID

| 鍵 ID | 用途 | 退役理由 |
|---|---|---|
| `78EC8C56B5BB75E4` | 旧 CI 署名鍵（v0.14.0–0.17.0 署名）| pubkey と不一致だった |
| `27FC787084AE0C82` | 旧・誤った埋め込み pubkey | #289 で設定、不正 |
| `85BD5242469DD021` | 最初期の pubkey（`snotra.key.pub`）| 初期実験鍵 |

### 旧バージョンからの移行

≤0.17.0 のインストールは旧 pubkey を焼き込んでおり自動更新不可。v0.17.1+ を1回だけ手動再インストールすれば以降は自動更新が回復する。

---

## リリースワークフロー

### 基本フロー

1. `create-release.yml`（workflow_dispatch、input=version）を実行 → git タグ `v<version>` を切り draft release を作成
2. `release.yml` が自動発火 → Windows ビルド・インストーラ・`latest.json` を draft release にアップロード
3. **draft のまま留まる**（`release.yml` の `draft: true` が明示指定されているため）
4. リリース担当が `latest.json` の署名を検証してから GitHub 上で手動 publish
5. publish して初めて updater が "latest" として配信を開始する

### 踏みやすい罠

#### `draft: true` を消すと即時公開バグが再発する

`release.yml` の softprops アクションは `draft` 指定がないと `draft: false`（即時公開）がデフォルト。#373 以前（v0.17.1 で顕在化）はこれで即時公開されていた。`release.yml` 内の `draft: true` は**絶対に削除しない**こと。

#### tauri signer は `node_modules/.bin` を直叩きする

`npm run tauri -- signer generate --ci -w <path>` も `npx tauri signer generate --ci -w <path>` も、npm/npx が `--ci`（→`--cidr`）・`-w`（→`--workspace`）を**自分のフラグとして横取り**して tauri に渡さない。

正しい起動法:
```powershell
./node_modules/.bin/tauri signer generate --ci --write-keys "<path>"
```

### 署名検証ハーネス

pynacl（libsodium = minisign と同一の Ed25519）で `latest.json` の署名を実検証できる。

- minisign の "ED" 署名は **blake2b-512 prehash 後に Ed25519 検証**
- 鍵 ID はランダムな 8 バイトタグ（鍵から導出されない）なので、**ID 一致だけでなくフル署名検証まで行う**こと
