# Microsoft Store向けMSIX

製品IDは `9PJ34ZZHSD01` です。[Microsoft Storeで開く](https://apps.microsoft.com/detail/9PJ34ZZHSD01)

Windows SDKがある環境でMSVC版のビルド後、次を実行します。

```powershell
.\build-msix.ps1
```

`dist/PerformanceGlass-<version>-x64.msix` にStore提出用パッケージを生成します。Microsoft Storeが配布するMSIXには、認定後にMicrosoftの署名が適用されます。

Store版のログオン時の自動起動は初期状態では無効です。右クリックメニューから切り替えられます。
