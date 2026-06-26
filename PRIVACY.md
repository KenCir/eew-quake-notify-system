# Privacy Policy

このアプリケーションは、設定ファイルと環境変数をローカル環境で読み込みます。Project DM-D.S.S の OAuth client secret、access token、受信した地震情報、通知履歴、読み上げ内容を、このアプリケーション自身が外部サービスへ送信することはありません。

Project DM-D.S.S への接続では、OAuth client credentials flow により access token を取得し、Socket Start v2 および WebSocket 接続に利用します。OAuth client secret や token はログに出力しません。

VOICEVOX を利用する場合、既定ではローカルホスト `http://127.0.0.1:50021` の VOICEVOX HTTP API に読み上げ文を送信します。`tts.voicevox_url` を外部ホストに変更した場合、その送信先の管理責任は利用者にあります。

設定ファイル、環境変数、ログ、受信データの保存先やバックアップ先については、利用者自身のローカル環境の管理方針に従って保護してください。
