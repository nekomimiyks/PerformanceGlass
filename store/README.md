# Microsoft Store向けMSIX

Partner Centerで予約済みの製品ID: `9PJ34ZZHSD01`。登録先はMSIXアプリです。2026年11月18日（日本時間）に初回申請を送信し、Partner Centerで「認定中」を確認しました。登録とStore審査・公開は別の工程で、現時点では未公開です。

初回申請には `0.1.10.0` のx64パッケージ、日本語の説明、通常表示／グラフ専用表示のライト／ダーク計4枚の画像を登録しています。年齢区分は3歳以上／全年齢対象（地域の区分方式により異なる）です。認定に合格すると無料で自動公開される設定です。

パッケージはGitHub Actionsでビルドし、単体テスト・UI検証とMakeAppxの検証を実行しています。実際のMSIX登録環境でも、計測、DPI別のサイズ変更、グラフ専用表示と自動起動状態の取得を確認しました。

`AppxManifest.xml` の発行元とパッケージ名はPartner Centerの「製品ID」に基づいています。公開前に同ページとの一致を確認してください。

Windows SDKがある環境でMSVC版のビルド後、次を実行します。

```powershell
.\build-msix.ps1
```

出力は `dist/PerformanceGlass-<version>-x64.msix` です。MakeAppxで検証し、署名せずStoreへ提出するパッケージを生成します。Microsoft Storeでの認定後、配布パッケージにMicrosoft側の署名が適用されます。GitHubの単体EXEを署名する手順ではありません。

Store版の自動起動はMSIXのStartupTaskで登録し、初期状態は無効です。通常版と同じメニューから切り替えられます。Windows側で利用者や管理者が無効化した場合には、その制限を尊重します。設定処理は専用の待機スレッドで行い、ウィンドウ操作やグラフ更新をブロックしません。ログオン時は `--tray` を渡し、通知領域から起動します。

アイコンは `Assets/` のPNGを使用します。[プライバシーポリシー](../docs/privacy.md)と既存の4枚のデモ画面を掲載情報に使用できます。
