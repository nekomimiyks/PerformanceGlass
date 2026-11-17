# Code signing policy（コード署名方針）

## 現在の状態

GitHub Releasesで配布するEXEは未署名です。

## プロジェクトと担当者

- プロジェクト: [PerformanceGlass](https://github.com/nekomimiyks/PerformanceGlass)
- ライセンス: [MIT](../LICENSE)
- ソース変更・レビュー担当: [nekomimiyks](https://github.com/nekomimiyks)
- 署名承認担当: [nekomimiyks](https://github.com/nekomimiyks)

外部からの変更は保守担当者がレビューします。署名サービスを利用する場合は、署名要求ごとに承認担当者が内容を確認して手動承認します。必要な多要素認証の要件は、サービス利用開始前に所有者が確認します。

## 署名対象とビルド

署名対象は、このリポジトリのソースからGitHub Actionsの[Windows MSVC build](https://github.com/nekomimiyks/PerformanceGlass/actions/workflows/windows-build.yml)で生成する自作の `PerformanceGlass.exe` です。製品名は `Performance Glass`、製品バージョンは `Cargo.toml` のバージョンと一致させます。他プロジェクトのバイナリをこのプロジェクトの証明書で署名しません。

承認後は、署名されたファイルを再検証してから、配布ZIP・EXE・ハッシュ一覧を更新します。署名鍵や認証情報をソースへ保存しません。

## プライバシーとシステム変更

アプリは計測情報・プロセス名・設定などの情報を外部のネットワークへ送信しません。CPU温度の連携もローカルのWMI名前空間だけを読み取ります。

履歴はメモリ上に保持し、終了時に消えます。個人設定は `%LOCALAPPDATA%\PerformanceGlass\settings.txt` に保存します。自動起動のレジストリ登録は、利用者が設定を有効にした場合だけ行います。セキュリティーソフトの設定変更やドライバーの自動導入は行いません。

削除する場合は、自動起動を無効にしてアプリを終了し、展開したフォルダーを削除してください。設定も不要なら、上記の設定フォルダーを削除できます。
