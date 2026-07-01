# EEW Quake Notify System

Project DM-D.S.S から地震情報・緊急地震速報関連の情報を受信し、ローカルデスクトップ通知と VOICEVOX 読み上げを行う、個人利用向けの Rust 製バックグラウンドシステムです。

このアプリケーションはローカルマシン上での通知と読み上げだけを目的にしています。Discord、Slack、LINE、Webhook、メール、remote push、public feed、relay など、地震情報や EEW をローカルマシン外へ再配信する機能は実装しません。

## Terms of Use

利用上の注意は [TERMS.md](TERMS.md) を参照してください。

## Privacy Policy

プライバシーポリシーは [PRIVACY.md](PRIVACY.md) を参照してください。

## 現在の構成

- Project DM-D.S.S API v2 の Socket Start で WebSocket URL を取得
- `dmdata.v2` WebSocket protocol で受信
- WebSocket `ping` に対する `pong` 応答
- `Ctrl+C` などの終了時に Socket Close と状態保存を実行
- JSON telegram payload の DTO と内部 `EarthquakeEvent` への正規化
- 重複・更新判定
- ローカルデスクトップ通知
- VOICEVOX HTTP API による読み上げ
- 再接続 backoff / jitter
- 二重起動防止用のローカル lock file
- console / file log 出力
- config ファイルによる設定
- `--test-notify` / `--test-speech` / `--test-alert` / `--replay-fixture` / `--validate-config` / `--doctor`

## 必要なもの

- Rust toolchain
- Project DM-D.S.S OAuth client
  - OAuth scope は、WebSocket 接続開始用の `socket.start` と終了処理用の `socket.close` が必要です。
  - 受信する区分に応じて `telegram.get.earthquake`, `eew.get.warning`, `eew.get.forecast` なども許可してください。
- VOICEVOX
  - 既定では `http://127.0.0.1:50021` のローカル VOICEVOX HTTP API を使います。
- デスクトップ通知が使える OS / desktop environment

## セットアップ

`config.toml.example` を元に `config.toml` を作成します。

```powershell
Copy-Item config.toml.example config.toml
```

Project DM-D.S.S の OAuth client ID / client secret は `config.toml` に直接書くか、環境変数に入れます。`config.toml` に実 credential を書く場合は git 管理しないでください。

config に直接書く例:

```toml
[dmdata]
client_id = "CId.xxxxx"
client_secret = "CSt.xxxxx"
```

環境変数を使う例:

```powershell
$env:DMDATA_CLIENT_ID = "CId.xxxxx"
$env:DMDATA_CLIENT_SECRET = "CSt.xxxxx"
```

VOICEVOX を起動し、必要に応じて `config.toml` の `tts.voicevox_url` と `tts.speaker` を変更してください。

設定ファイルだけを検証:

```powershell
cargo run -- --config config.toml --validate-config
```

ローカル運用診断を実行:

```powershell
cargo run -- --config config.toml --doctor
```

`--doctor` は config、state/log directory、single-instance lock、VOICEVOX API 到達性を確認します。Project DM-D.S.S の live API には接続しません。

## 実行

通常起動:

```powershell
cargo run -- --config config.toml
```

終了するときは `Ctrl+C` を送ってください。通常起動では shutdown signal を受けると Project DM-D.S.S の Socket Close API を呼び出し、WebSocket close frame を送信し、処理済みイベントの dedup state を保存してから終了します。

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

過去データ fixture を使って、WebSocket 受信後と同じ正規化・通知・読み上げ pipeline をテスト:

```powershell
cargo run -- --config config.toml --replay-fixture test-data/eew-warning_20260626
```

`--replay-fixture` は、Project DM-D.S.S WebSocket v2 の `type: "data"` メッセージ、またはコントロールパネルから取得した `telegrams.json` と個別電文 JSON/XML を含むディレクトリを読み込みます。コントロールパネル由来のデータはテスト用に WebSocket `data` 相当へ組み立てますが、本番受信では公式ドキュメントに記載された WebSocket message schema を優先します。

## Windows での常駐起動

現段階では Windows Service 化は推奨しません。Service セッションではデスクトップ通知と音声再生がユーザーセッションと同じように動かない可能性があるためです。

常駐運用する場合は、Windows タスクスケジューラで「ユーザーのログオン時」に起動するタスクを作成してください。release build の exe を使う場合は、補助スクリプトで登録できます。

```powershell
.\scripts\install-task-scheduler.ps1 `
  -ExePath .\target\release\eew-quake-notify-system.exe `
  -ConfigPath .\config.toml
```

解除:

```powershell
.\scripts\uninstall-task-scheduler.ps1
```

手動でタスクを作る場合の設定:

- プログラム: `cargo`
- 引数: `run --release -- --config C:\path\to\config.toml`
- 開始: このリポジトリのディレクトリ

ビルド済み exe を使う場合は、プログラムに exe のパス、引数に `--config C:\path\to\config.toml` を指定します。

## Release Build

ローカルで Windows x64 向け release build を作る場合:

```powershell
cargo build --release
.\target\release\eew-quake-notify-system.exe --config config.toml.example --validate-config
```

実運用では `config.toml.example` をコピーして `config.toml` を作成し、OAuth client ID / client secret などを設定してください。`config.toml` は credential を含み得るため git 管理しません。

GitHub Release の zip には次を含めます。

- `eew-quake-notify-system.exe`
- `config.toml.example`
- `README.md`
- `TERMS.md`
- `PRIVACY.md`
- `scripts/install-task-scheduler.ps1`
- `scripts/uninstall-task-scheduler.ps1`

zip には `config.toml`, `state/`, `logs/` は含めません。

## GitHub Actions Release

このリポジトリは release-please を使って release PR、CHANGELOG、version bump、GitHub Release 作成を行う想定です。

運用手順:

1. `fix: ...` や `feat: ...` など Conventional Commits 形式で `main` に merge します。
2. `release-please` workflow が release PR を作成または更新します。
3. release PR を merge すると GitHub Release が作成されます。
4. 同じ workflow 内で Windows x64 release build を作成し、zip と `.sha256` を Release asset に添付します。

GitHub repository settings で、GitHub Actions が pull request を作成できるようにしてください。GHA は live の Project DM-D.S.S API には接続しません。

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
| `client_id` | いいえ | なし | `auth_mode = "client_credentials"` のときに使う OAuth client ID。設定されていれば `client_id_env` より優先します。 |
| `client_id_env` | いいえ | `DMDATA_CLIENT_ID` | `auth_mode = "client_credentials"` のときに OAuth client ID を読む環境変数名。 |
| `client_secret` | いいえ | なし | `auth_mode = "client_credentials"` のときに使う OAuth client secret。設定されていれば `client_secret_env` より優先します。 |
| `client_secret_env` | いいえ | `DMDATA_CLIENT_SECRET` | `auth_mode = "client_credentials"` のときに OAuth client secret を読む環境変数名。 |
| `oauth_scopes` | いいえ | `[]` | OAuth token request に渡す scope の明示指定。空なら `classifications` から必要 scope を自動生成します。常に `socket.start` と `socket.close` を含み、`telegram.earthquake` では `telegram.get.earthquake`、`eew.warning` では `eew.get.warning`、`eew.forecast` では `eew.get.forecast` を追加します。DM-D.S.S 側の OAuth client でも同じ scope を許可してください。 |
| `classifications` | いいえ | `["telegram.earthquake", "eew.warning"]` | Socket Start に渡す配信区分。既定では地震・津波関連と緊急地震速報（警報）だけを購読します。緊急地震速報（予報）を契約・許可済みの場合は `"eew.forecast"` を追加してください。 |
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
| `enabled_kinds` | いいえ | `["earthquake", "intensity_report", "eew_warning"]` | 通知対象の内部イベント種別。指定可能値は `earthquake`, `intensity_report`, `eew_warning`, `eew_forecast` です。既定では `eew_forecast` は通知しません。 |
| `min_intensity` | いいえ | なし | 通知する最小震度。例: `"3"`, `"4"`, `"5弱"`, `"5強"`。未指定なら震度による抑制をしません。取消報は震度に関係なく通知対象です。 |

### `[tts]`

VOICEVOX 読み上げに関する設定です。

`enabled = true` の場合、起動時に VOICEVOX API の `initialize_speaker` を呼び出し、設定された `speaker` を事前初期化します。緊急地震速報など即時性が必要な読み上げで、初回合成時の待ち時間を抑えるためです。

| 項目 | 必須 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | いいえ | `true` | `false` にすると読み上げを行いません。 |
| `engine` | いいえ | `"voicevox"` | 現在は `"voicevox"` のみ対応です。 |
| `voicevox_url` | いいえ | `http://127.0.0.1:50021` | VOICEVOX HTTP API の URL。 |
| `speaker` | いいえ | `1` | VOICEVOX speaker ID。 |
| `enabled_kinds` | いいえ | `["earthquake", "intensity_report", "eew_warning"]` | 読み上げ対象の内部イベント種別。指定可能値は `notify.enabled_kinds` と同じです。既定では `eew_forecast` は読み上げません。 |
| `min_intensity` | いいえ | なし | 読み上げる最小震度。書式は `notify.min_intensity` と同じです。取消報は震度に関係なく読み上げ対象です。 |

### `[state]`

重複通知を避けるためのローカル状態ファイルに関する設定です。通常起動時のみ使われ、`--test-alert` や `--replay-fixture` では状態ファイルを更新しません。

| 項目 | 必須 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `enabled` | いいえ | `true` | `true` にすると処理済みイベントの dedup state を保存・復元します。 |
| `file_path` | いいえ | `state/dedup-state.json` | dedup state を保存する JSON ファイルのパス。secret は含みませんが、実行時生成物なので git 管理しません。 |
| `max_entries` | いいえ | `1000` | 保存するイベント履歴の最大件数。超えた場合は古いものから削除します。 |

### `[runtime]`

常駐プロセスのローカル実行制御に関する設定です。

| 項目 | 必須 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `single_instance` | いいえ | `true` | `true` にすると通常起動時に lock file を作成し、二重起動を防ぎます。 |
| `lock_file_path` | いいえ | `state/app.lock` | single instance 用 lock file のパス。実行時生成物なので git 管理しません。 |

### `[log]`

ログ出力に関する設定です。

| 項目 | 必須 | 既定値 | 説明 |
| --- | --- | --- | --- |
| `level` | いいえ | `"info"` | `tracing` のログレベル。例: `"debug"`, `"info"`, `"warn"`。 |
| `console_enabled` | いいえ | `true` | `true` にすると標準出力へログを出します。 |
| `file_enabled` | いいえ | `false` | `true` にすると file log を出します。 |
| `file_path` | いいえ | `logs/eew-quake-notify.log` | file log の出力先。secret、token、ticket 付き WebSocket URL は出力しません。 |

## 設定例

```toml
[dmdata]
socket_start_url = "https://api.dmdata.jp/v2/socket"
token_endpoint_url = "https://manager.dmdata.jp/account/oauth2/v1/token"
auth_mode = "client_credentials"
api_token_env = "DMDATA_API_TOKEN"
client_id = ""
client_id_env = "DMDATA_CLIENT_ID"
client_secret = ""
client_secret_env = "DMDATA_CLIENT_SECRET"
oauth_scopes = []
# eew.forecast は契約・OAuth scope 許可後に必要なら追加してください。
classifications = ["telegram.earthquake", "eew.warning"]
types = ["VXSE43", "VXSE44", "VXSE45", "VXSE51", "VXSE52", "VXSE53", "VXSE62"]
test = "no"
app_name = "eew-quake-notify"
format_mode = "json"
reconnect_initial_ms = 1000
reconnect_max_ms = 30000

[notify]
desktop_enabled = true
enabled_kinds = ["earthquake", "intensity_report", "eew_warning"]
min_intensity = "3"

[tts]
enabled = true
engine = "voicevox"
voicevox_url = "http://127.0.0.1:50021"
speaker = 1
enabled_kinds = ["earthquake", "intensity_report", "eew_warning"]
min_intensity = "3"

[state]
enabled = true
file_path = "state/dedup-state.json"
max_entries = 1000

[runtime]
single_instance = true
lock_file_path = "state/app.lock"

[log]
level = "info"
console_enabled = true
file_enabled = false
file_path = "logs/eew-quake-notify.log"
```

## 開発時の検証

コード変更後は次を実行します。

```powershell
cargo fmt
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

live の Project DM-D.S.S service に依存する test は避け、payload は fixture JSON で検証します。手元の過去データで通知経路を確認する場合は `--replay-fixture <path>` を使います。

## 参考リンク

- [Project DM-D.S.S API v2 reference](https://dmdata.jp/docs/reference/api/v2/)
- [Project DM-D.S.S GitHub repository](https://github.com/pdmdss/dmdata.jp)
- [VOICEVOX](https://voicevox.hiroshiba.jp/)

Project DM-D.S.S の API 仕様、endpoint、認証方式、message schema、rate limit は変更される可能性があります。DMDATA 連携を変更するときは、実装前に公式ドキュメントを確認してください。
