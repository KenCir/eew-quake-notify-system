# EEW Quake Notify System

Project DM-D.S.S から地震情報・緊急地震速報関連の情報を受信し、ローカルデスクトップ通知と VOICEVOX 読み上げを行う、個人利用向けの Rust 製バックグラウンドシステムです。

このアプリケーションはローカルマシン上での通知と読み上げだけを目的にしています。Discord、Slack、LINE、Webhook、メール、remote push、public feed、relay など、地震情報や EEW をローカルマシン外へ再配信する機能は実装しません。

## 利用上の注意

このアプリケーションは、個人のローカル環境で Project DM-D.S.S の地震情報および緊急地震速報関連情報を受信し、ローカルデスクトップ通知と VOICEVOX 読み上げを行うためのものです。

受信した地震情報、緊急地震速報、その他の Project DM-D.S.S 配信データを、Discord、Slack、LINE、Webhook、メール、remote push、public feed、relay などで外部へ再配信する用途には使用しません。

本アプリケーションは防災情報を補助的に確認するための個人用ツールです。実際の避難判断や安全確保では、気象庁、自治体、公共機関などの公式情報も必ず確認してください。

## プライバシー

このアプリケーションは、設定ファイルと環境変数をローカル環境で読み込みます。Project DM-D.S.S の OAuth client secret、access token、受信した地震情報、通知履歴、読み上げ内容を、このアプリケーション自身が外部サービスへ送信することはありません。

Project DM-D.S.S への接続では、OAuth client credentials flow により access token を取得し、Socket Start v2 および WebSocket 接続に利用します。OAuth client secret や token はログに出力しません。

VOICEVOX を利用する場合、既定ではローカルホスト `http://127.0.0.1:50021` の VOICEVOX HTTP API に読み上げ文を送信します。`tts.voicevox_url` を外部ホストに変更した場合、その送信先の管理責任は利用者にあります。

## 現在の構成

- Project DM-D.S.S API v2 の Socket Start で WebSocket URL を取得
- `dmdata.v2` WebSocket protocol で受信
- WebSocket `ping` に対する `pong` 応答
- JSON telegram payload の DTO と内部 `EarthquakeEvent` への正規化
- 重複・更新判定
- ローカルデスクトップ通知
- VOICEVOX HTTP API による読み上げ
- 再接続 backoff / jitter
- config ファイルによる設定
- `--test-notify` / `--test-speech` / `--test-alert`

## 必要なもの

- Rust toolchain
- Project DM-D.S.S OAuth client
- VOICEVOX
  - 既定では `http://127.0.0.1:50021` のローカル VOICEVOX HTTP API を使います。
- デスクトップ通知が使える OS / desktop environment

## セットアップ

`config.toml.example` を元に `config.toml` を作成します。

```powershell
Copy-Item config.toml.example config.toml
```

Project DM-D.S.S の OAuth client ID / client secret は config に直接書かず、環境変数に入れます。既定の環境変数名は `DMDATA_CLIENT_ID` と `DMDATA_CLIENT_SECRET` です。

```powershell
$env:DMDATA_CLIENT_ID = "CId.xxxxx"
$env:DMDATA_CLIENT_SECRET = "CSt.xxxxx"
```

VOICEVOX を起動し、必要に応じて `config.toml` の `tts.voicevox_url` と `tts.speaker` を変更してください。

## 実行

通常起動:

```powershell
cargo run -- --config config.toml
```

デスクトップ通知だけをテスト:

```powershell
cargo run -- --config config.toml --test-notify
```

VOICEVOX 読み上げだけをテスト:

```powershell
cargo run -- --config config.toml --test-speech
```

通知と読み上げを含むアプリ内 pipeline をテスト:

```powershell
cargo run -- --config config.toml --test-alert
```

## 設定リファレンス

設定ファイルは TOML 形式です。サンプルは `config.toml.example` を参照してください。

### `[dmdata]`

Project DM-D.S.S 接続に関する設定です。

| 項目 | 必須 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `socket_start_url` | いいえ | `https://api.dmdata.jp/v2/socket` | Socket Start v2 endpoint。通常は変更しません。 |
| `token_endpoint_url` | いいえ | `https://manager.dmdata.jp/account/oauth2/v1/token` | OAuth token endpoint。通常は変更しません。 |
| `websocket_url` | いいえ | なし | 開発・検証用の WebSocket URL 上書き。設定した場合は Socket Start を呼ばず、この URL に接続します。ticket や secret を含む値を git 管理しないでください。 |
| `auth_mode` | いいえ | `"access_token"` | 認証方式。通常運用では `"client_credentials"` を使います。既に取得済みの `ATn.` access token を直接使う場合だけ `"access_token"` にします。 |
| `api_token_env` | いいえ | `DMDATA_API_TOKEN` | `auth_mode = "access_token"` のときに access token を読む環境変数名。token 本体ではありません。 |
| `client_id_env` | いいえ | `DMDATA_CLIENT_ID` | `auth_mode = "client_credentials"` のときに OAuth client ID を読む環境変数名。 |
| `client_secret_env` | いいえ | `DMDATA_CLIENT_SECRET` | `auth_mode = "client_credentials"` のときに OAuth client secret を読む環境変数名。 |
| `oauth_scopes` | いいえ | `[]` | OAuth token request に渡す scope の明示指定。空なら `classifications` から `socket.start` などを自動生成します。 |
| `classifications` | いいえ | `["telegram.earthquake", "eew.forecast", "eew.warning"]` | Socket Start に渡す配信区分。地震・EEW 関連だけを購読します。 |
| `types` | いいえ | `["VXSE43", "VXSE44", "VXSE45", "VXSE51", "VXSE52", "VXSE53", "VXSE62"]` | 受信対象の telegram type。最大 30 件までです。 |
| `test` | いいえ | `"no"` | 試験データの扱い。現在は `"no"` または `"including"` を受け付けます。 |
| `app_name` | いいえ | `"eew-quake-notify"` | Socket Start に渡すアプリ名。24 bytes 以下にしてください。 |
| `format_mode` | いいえ | `"json"` | 受信 payload 形式。現在の実装は `"json"` のみ対応です。 |
| `reconnect_initial_ms` | いいえ | `1000` | 再接続 backoff の初期待機時間、ミリ秒。 |
| `reconnect_max_ms` | いいえ | `30000` | 再接続 backoff の最大待機時間、ミリ秒。 |

`types` の既定値は地震情報・震度速報・緊急地震速報関連を想定しています。購読可能な分類や telegram type は Project DM-D.S.S の公式リファレンスを確認してください。

### `[notify]`

ローカルデスクトップ通知に関する設定です。

| 項目 | 必須 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `desktop_enabled` | いいえ | `true` | `false` にするとデスクトップ通知を送信しません。 |
| `min_intensity` | いいえ | なし | 通知する最小震度。例: `"3"`, `"4"`, `"5弱"`, `"5強"`。未指定なら震度による抑制をしません。取消報は震度に関係なく通知対象です。 |

### `[tts]`

VOICEVOX 読み上げに関する設定です。

| 項目 | 必須 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | いいえ | `true` | `false` にすると読み上げを行いません。 |
| `engine` | いいえ | `"voicevox"` | 現在は `"voicevox"` のみ対応です。 |
| `voicevox_url` | いいえ | `http://127.0.0.1:50021` | VOICEVOX HTTP API の URL。 |
| `speaker` | いいえ | `1` | VOICEVOX speaker ID。 |
| `min_intensity` | いいえ | なし | 読み上げる最小震度。書式は `notify.min_intensity` と同じです。取消報は震度に関係なく読み上げ対象です。 |

### `[log]`

ログ出力に関する設定です。

| 項目 | 必須 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `level` | いいえ | `"info"` | `tracing` のログレベル。例: `"debug"`, `"info"`, `"warn"`。 |

## 設定例

```toml
[dmdata]
socket_start_url = "https://api.dmdata.jp/v2/socket"
token_endpoint_url = "https://manager.dmdata.jp/account/oauth2/v1/token"
auth_mode = "client_credentials"
api_token_env = "DMDATA_API_TOKEN"
client_id_env = "DMDATA_CLIENT_ID"
client_secret_env = "DMDATA_CLIENT_SECRET"
oauth_scopes = []
classifications = ["telegram.earthquake", "eew.forecast", "eew.warning"]
types = ["VXSE43", "VXSE44", "VXSE45", "VXSE51", "VXSE52", "VXSE53", "VXSE62"]
test = "no"
app_name = "eew-quake-notify"
format_mode = "json"
reconnect_initial_ms = 1000
reconnect_max_ms = 30000

[notify]
desktop_enabled = true
min_intensity = "3"

[tts]
enabled = true
engine = "voicevox"
voicevox_url = "http://127.0.0.1:50021"
speaker = 1
min_intensity = "3"

[log]
level = "info"
```

## 開発時の検証

コード変更後は次を実行します。

```powershell
cargo fmt
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

live の Project DM-D.S.S service に依存する test は避け、payload は fixture JSON で検証します。

## 参考リンク

- [Project DM-D.S.S API v2 reference](https://dmdata.jp/docs/reference/api/v2/)
- [Project DM-D.S.S GitHub repository](https://github.com/pdmdss/dmdata.jp)
- [VOICEVOX](https://voicevox.hiroshiba.jp/)

Project DM-D.S.S の API 仕様、endpoint、認証方式、message schema、rate limit は変更される可能性があります。DMDATA 連携を変更するときは、実装前に公式ドキュメントを確認してください。
