//! Embedded UI translations. No resource files, network, or per-frame locale queries.
use std::sync::atomic::{AtomicUsize, Ordering};
use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Language {
    Ja,
    En,
    ZhHans,
    ZhHant,
    Ko,
    De,
    Fr,
    Es,
    Pt,
    It,
    Ru,
    Id,
}
pub const LANGUAGES: [Language; 12] = [
    Language::Ja,
    Language::En,
    Language::ZhHans,
    Language::ZhHant,
    Language::Ko,
    Language::De,
    Language::Fr,
    Language::Es,
    Language::Pt,
    Language::It,
    Language::Ru,
    Language::Id,
];
static LANGUAGE: AtomicUsize = AtomicUsize::new(Language::En as usize);
impl Language {
    pub fn from_tag(tag: &str) -> Option<Self> {
        let tag = tag.replace('_', "-").to_ascii_lowercase();
        let mut parts = tag.split('-');
        let base = parts.next()?;
        Some(match base {
            "ja" => Self::Ja,
            "en" => Self::En,
            "ko" => Self::Ko,
            "de" => Self::De,
            "fr" => Self::Fr,
            "es" => Self::Es,
            "pt" => Self::Pt,
            "it" => Self::It,
            "ru" => Self::Ru,
            "id" => Self::Id,
            "zh" => {
                let parts: Vec<_> = parts.collect();
                if parts.contains(&"hant") {
                    Self::ZhHant
                } else if parts.contains(&"hans") {
                    Self::ZhHans
                } else if parts.iter().any(|p| matches!(*p, "tw" | "hk" | "mo")) {
                    Self::ZhHant
                } else {
                    Self::ZhHans
                }
            }
            _ => return None,
        })
    }
    pub fn tag(self) -> &'static str {
        [
            "ja", "en", "zh-Hans", "zh-Hant", "ko", "de", "fr", "es", "pt", "it", "ru", "id",
        ][self as usize]
    }
    pub fn native_name(self) -> &'static str {
        [
            "日本語",
            "English",
            "简体中文",
            "繁體中文",
            "한국어",
            "Deutsch",
            "Français",
            "Español",
            "Português",
            "Italiano",
            "Русский",
            "Bahasa Indonesia",
        ][self as usize]
    }
}
fn preferred_language<'a>(tags: impl IntoIterator<Item = &'a str>) -> Language {
    tags.into_iter()
        .find_map(Language::from_tag)
        .unwrap_or(Language::En)
}
fn detect() -> Language {
    unsafe {
        let mut count = 0;
        let mut length = 0;
        if GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut length,
        ) == 0
            || length == 0
            || length > 65536
        {
            return Language::En;
        }
        let mut buffer = vec![0u16; length as usize];
        if GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            buffer.as_mut_ptr(),
            &mut length,
        ) == 0
        {
            return Language::En;
        }
        let tags = String::from_utf16_lossy(&buffer);
        preferred_language(tags.split('\0').filter(|s| !s.is_empty()))
    }
}
pub fn init(args: &[String]) {
    // CLI override is only for isolated demo/UI checks. Normal use applies
    // the saved menu preference when loading Settings in ui::run.
    let isolated = args
        .iter()
        .any(|s| matches!(s.as_str(), "--demo" | "--test-ui"));
    let override_language = isolated
        .then(|| {
            args.iter()
                .position(|s| s == "--language")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| Language::from_tag(s))
        })
        .flatten();
    select(override_language);
}
pub fn select(preference: Option<Language>) {
    LANGUAGE.store(
        preference.unwrap_or_else(detect) as usize,
        Ordering::Relaxed,
    );
}
pub fn language() -> Language {
    LANGUAGES[LANGUAGE.load(Ordering::Relaxed)]
}
pub fn tr(key: &str) -> &str {
    translate(language(), key)
}
pub fn translate(language: Language, key: &str) -> &str {
    let row = match key {
        "言語" => &["言語", "Language", "语言", "語言", "언어", "Sprache", "Langue", "Idioma", "Idioma", "Lingua", "Язык", "Bahasa"] as &[&str; 12],
        "Windowsに合わせる（自動）" => &["Windowsに合わせる（自動）", "Follow Windows (automatic)", "跟随 Windows（自动）", "跟隨 Windows（自動）", "Windows 설정에 따름 (자동)", "Windows folgen (automatisch)", "Suivre Windows (automatique)", "Seguir Windows (automático)", "Seguir Windows (automático)", "Segui Windows (automatico)", "Как в Windows (автоматически)", "Ikuti Windows (otomatis)"] as &[&str; 12],
        "トレイに格納" => &["トレイに格納","Minimize to tray","最小化到托盘","縮小至通知區域","알림 영역으로 최소화","In Infobereich minimieren","Réduire dans la zone de notification","Minimizar a la bandeja","Minimizar para a bandeja","Riduci nell'area di notifica","Свернуть в область уведомлений","Minimalkan ke baki"] as &[&str; 12],
        "{}前" => &["{}前","{} ago","{}前","{}前","{} 전","vor {}","il y a {}","hace {}","há {}","{} fa","{} назад","{} lalu"] as &[&str; 12],
        "専用" => &["専用","dedicated","专用","專用","전용","dediziert","dédiée","dedicada","dedicada","dedicata","выделенная","khusus"] as &[&str; 12],
        "受信" => &["受信","RX","接收","接收","수신","Empfang","Réception","Recepción","Receb.","Ricez.","Приём","Terima"] as &[&str; 12],
        "送信" => &["送信","TX","发送","傳送","송신","Senden","Envoi","Envío","Envio","Invio","Отпр.","Kirim"] as &[&str; 12],
        "通信の目盛り上限" => &["通信の目盛り上限","Network scale maximum","网络刻度上限","網路刻度上限","네트워크 눈금 상한","Netzwerkskala maximal","Maximum de l'échelle réseau","Máximo de la escala de red","Máximo da escala de rede","Massimo scala rete","Максимум шкалы сети","Maksimum skala jaringan"] as &[&str; 12],
        "送受信合計" => &["送受信合計","RX + TX","收发合计","傳送與接收合計","송수신 합계","Empfang + Senden","Réception + envoi","Recepción + envío","Recebimento + envio","Ricezione + invio","Приём + отправка","Terima + kirim"] as &[&str; 12],
        "メモリ" => &["メモリ","Memory","内存","記憶體","메모리","Arbeitsspeicher","Mémoire","Memoria","Memória","Memoria","Память","Memori"] as &[&str; 12],
        "ディスク" => &["ディスク","Disk","磁盘","磁碟","디스크","Datenträger","Disque","Disco","Disco","Disco","Диск","Disk"] as &[&str; 12],
        "1分" => &["1分","1 min","1分钟","1分鐘","1분","1 Min.","1 min","1 min","1 min","1 min","1 мин","1 mnt"] as &[&str; 12],
        "10分" => &["10分","10 min","10分钟","10分鐘","10분","10 Min.","10 min","10 min","10 min","10 min","10 мин","10 mnt"] as &[&str; 12],
        "60分" => &["60分","60 min","60分钟","60分鐘","60분","60 Min.","60 min","60 min","60 min","60 min","60 мин","60 mnt"] as &[&str; 12],
        "6時間" => &["6時間","6 h","6小时","6小時","6시간","6 Std.","6 h","6 h","6 h","6 h","6 ч","6 jam"] as &[&str; 12],
        "12時間" => &["12時間","12 h","12小时","12小時","12시간","12 Std.","12 h","12 h","12 h","12 h","12 ч","12 jam"] as &[&str; 12],
        "使用率" => &["使用率","Usage","使用率","使用率","사용률","Auslastung","Utilisation","Uso","Uso","Utilizzo","Загрузка","Penggunaan"] as &[&str; 12],
        "速度" => &["速度","Speed","速度","速度","속도","Takt","Fréquence","Velocidad","Velocidade","Velocità","Частота","Kecepatan"] as &[&str; 12],
        "温度" => &["温度","Temp.","温度","溫度","온도","Temp.","Temp.","Temp.","Temp.","Temp.","Темп.","Suhu"] as &[&str; 12],
        "使用量" => &["使用量","Used","已用","已使用","사용량","Belegt","Utilisée","En uso","Em uso","In uso","Занято","Terpakai"] as &[&str; 12],
        "読み込み" => &["読み込み","Read","读取","讀取","읽기","Lesen","Lecture","Lectura","Leitura","Lettura","Чтение","Baca"] as &[&str; 12],
        "書き込み" => &["書き込み","Write","写入","寫入","쓰기","Schreiben","Écriture","Escritura","Gravação","Scrittura","Запись","Tulis"] as &[&str; 12],
        "稼働率" => &["稼働率","Activity","活动率","使用中時間","활성 시간","Aktivität","Activité","Actividad","Atividade","Attività","Активность","Aktivitas"] as &[&str; 12],
        "受信 Mbps" => &["受信 Mbps","RX Mbps","接收 Mbps","接收 Mbps","수신 Mbps","Empfang Mbps","Réception Mbps","Recepción Mbps","Receb. Mbps","Ricez. Mbps","Приём Mbps","Terima Mbps"] as &[&str; 12],
        "送信 Mbps" => &["送信 Mbps","TX Mbps","发送 Mbps","傳送 Mbps","송신 Mbps","Senden Mbps","Envoi Mbps","Envío Mbps","Envio Mbps","Invio Mbps","Отпр. Mbps","Kirim Mbps"] as &[&str; 12],
        "接続状態" => &["接続状態","Status","连接状态","連線狀態","연결 상태","Status","État","Estado","Estado","Stato","Состояние","Status"] as &[&str; 12],
        "未接続" => &["未接続","Disconnected","未连接","未連線","연결 안 됨","Getrennt","Déconnecté","Desconectado","Desconectado","Disconnesso","Нет соединения","Terputus"] as &[&str; 12],
        "GPUメモリ" => &["GPUメモリ","GPU memory","GPU 内存","GPU 記憶體","GPU 메모리","GPU-Speicher","Mémoire GPU","Memoria GPU","Memória GPU","Memoria GPU","Память GPU","Memori GPU"] as &[&str; 12],
        "設定 ···" => &["設定 ···","Settings ···","设置 ···","設定 ···","설정 ···","Optionen ···","Réglages ···","Ajustes ···","Opções ···","Opzioni ···","Настройки ···","Setelan ···"] as &[&str; 12],
        "見本データ" => &["見本データ","Demo data","演示数据","示範資料","데모 데이터","Demodaten","Démonstration","Demostración","Demonstração","Dati demo","Демо-данные","Data demo"] as &[&str; 12],
        "使用率 %" => &["使用率 %","Usage %","使用率 %","使用率 %","사용률 %","Auslastung %","Utilisation %","Uso %","Uso %","Utilizzo %","Загрузка %","Penggunaan %"] as &[&str; 12],
        "通信 Mbps" => &["通信 Mbps","Network Mbps","网络 Mbps","網路 Mbps","네트워크 Mbps","Netz Mbps","Réseau Mbps","Red Mbps","Rede Mbps","Rete Mbps","Сеть Mbps","Jaringan Mbps"] as &[&str; 12],
        "現在" => &["現在","Now","现在","現在","현재","Jetzt","Maintenant","Ahora","Agora","Ora","Сейчас","Kini"] as &[&str; 12],
        "計測を準備しています…" => &["計測を準備しています…","Preparing metrics…","正在准备计测…","正在準備計測…","측정 준비 중…","Messung wird vorbereitet…","Préparation des mesures…","Preparando mediciones…","Preparando medições…","Preparazione misure…","Подготовка измерений…","Menyiapkan pengukuran…"] as &[&str; 12],
        "（見本）" => &["（見本）"," (demo)","（演示）","（示範）"," (데모)"," (Demo)"," (démo)"," (demo)"," (demo)"," (demo)"," (демо)"," (demo)"] as &[&str; 12],
        "時間" => &["時間","h","小时","小時","시간","h","h","h","h","h","ч","j"] as &[&str; 12],
        "分" => &["分","min","分钟","分鐘","분","min","min","min","min","min","мин","mnt"] as &[&str; 12],
        "秒" => &["秒","s","秒","秒","초","s","s","s","s","s","с","dtk"] as &[&str; 12],
        "ウィンドウを表示" => &["ウィンドウを表示","Show window","显示窗口","顯示視窗","창 표시","Fenster anzeigen","Afficher la fenêtre","Mostrar ventana","Mostrar janela","Mostra finestra","Показать окно","Tampilkan jendela"] as &[&str; 12],
        "常に手前に表示 (&T)" => &["常に手前に表示 (&T)","Always on top (&T)","置顶 (&T)","一律置頂 (&T)","항상 위에 표시 (&T)","Immer im Vordergrund (&T)","Toujours au premier plan (&T)","Siempre visible (&T)","Sempre no topo (&T)","Sempre in primo piano (&T)","Поверх всех окон (&T)","Selalu di atas (&T)"] as &[&str; 12],
        "グラフだけ表示 (ダブルクリック / F11)" => &["グラフだけ表示 (ダブルクリック / F11)","Graph only (double-click / F11)","仅显示图表（双击 / F11）","只顯示圖表（按兩下 / F11）","그래프만 표시 (두 번 클릭 / F11)","Nur Diagramm (Doppelklick / F11)","Graphique seul (double-clic / F11)","Solo gráfico (doble clic / F11)","Só gráfico (clique duplo / F11)","Solo grafico (doppio clic / F11)","Только график (двойной щелчок / F11)","Grafik saja (klik ganda / F11)"] as &[&str; 12],
        "Windowsログオン時に起動" => &["Windowsログオン時に起動","Start at Windows sign-in","登录 Windows 时启动","登入 Windows 時啟動","Windows 로그인 시 시작","Bei Windows-Anmeldung starten","Démarrer à la connexion Windows","Iniciar al entrar en Windows","Iniciar ao entrar no Windows","Avvia all'accesso a Windows","Запуск при входе в Windows","Mulai saat masuk Windows"] as &[&str; 12],
        "終了 (&X)" => &["終了 (&X)","Exit (&X)","退出 (&X)","結束 (&X)","종료 (&X)","Beenden (&X)","Quitter (&X)","Salir (&X)","Sair (&X)","Esci (&X)","Выход (&X)","Keluar (&X)"] as &[&str; 12],
        "クリックして表示" => &["クリックして表示","Click to show","单击以显示","按一下以顯示","클릭하여 표시","Zum Anzeigen klicken","Cliquer pour afficher","Clic para mostrar","Clique para mostrar","Fai clic per mostrare","Нажмите, чтобы показать","Klik untuk tampilkan"] as &[&str; 12],
        "履歴: {}" => &["履歴: {}","History: {}","历史记录：{}","歷程記錄：{}","기록: {}","Verlauf: {}","Historique : {}","Historial: {}","Histórico: {}","Cronologia: {}","История: {}","Riwayat: {}"] as &[&str; 12],
        "線を表示: {}" => &["線を表示: {}","Show line: {}","显示曲线：{}","顯示曲線：{}","선 표시: {}","Linie anzeigen: {}","Afficher la courbe : {}","Mostrar línea: {}","Mostrar linha: {}","Mostra linea: {}","Показать линию: {}","Tampilkan garis: {}"] as &[&str; 12],
        "読込" => &["読込","Read","读取","讀取","읽기","Lesen","Lecture","Lectura","Leitura","Lettura","Чтение","Baca"] as &[&str; 12],
        "書込" => &["書込","Write","写入","寫入","쓰기","Schreiben","Écriture","Escritura","Gravação","Scrittura","Запись","Tulis"] as &[&str; 12],
        "通信の目盛り上限 {} Mbps（送受信合計）" => &["通信の目盛り上限 {} Mbps（送受信合計）","Network scale: {} Mbps (RX + TX)","网络刻度上限 {} Mbps（接收与发送合计）","網路刻度上限 {} Mbps（接收與傳送總計）","네트워크 눈금 상한 {} Mbps (수신 + 송신)","Netzskala: {} Mbps (Empfang + Senden)","Échelle réseau : {} Mbps (réception + envoi)","Escala de red: {} Mbps (recepción + envío)","Escala de rede: {} Mbps (recebimento + envio)","Scala rete: {} Mbps (ricezione + invio)","Шкала сети: {} Mbps (приём + отправка)","Skala jaringan: {} Mbps (terima + kirim)"] as &[&str; 12],
        "ディスク: 稼働率は最大、読み書きは全物理ディスク合計" => &["ディスク: 稼働率は最大、読み書きは全物理ディスク合計","Disk: peak activity; reads/writes summed across physical disks","磁盘：最高活动率；读取和写入为所有物理磁盘合计","磁碟：最高使用中時間；讀取與寫入為所有實體磁碟總計","디스크: 최대 활성 시간, 읽기/쓰기는 모든 물리 디스크 합계","Datenträger: höchste Aktivität; Lesen/Schreiben aller physischen Datenträger summiert","Disque : activité maximale ; lectures/écritures cumulées des disques physiques","Disco: actividad máxima; lectura/escritura sumada de los discos físicos","Disco: atividade máxima; leitura/gravação somadas dos discos físicos","Disco: attività massima; letture/scritture sommate sui dischi fisici","Диск: максимальная активность; чтение/запись суммарно по физическим дискам","Disk: aktivitas tertinggi; baca/tulis dijumlahkan dari semua disk fisik"] as &[&str; 12],
        "自動起動を変更できません。Windowsの「設定 → アプリ → スタートアップ」も確認してください。" => &["自動起動を変更できません。Windowsの「設定 → アプリ → スタートアップ」も確認してください。","Cannot change startup. Check Windows Settings → Apps → Startup.","无法更改启动设置。请检查 Windows 设置 → 应用 → 启动。","無法變更啟動設定。請檢查 Windows 設定 → 應用程式 → 啟動。","시작 설정을 변경할 수 없습니다. Windows 설정 → 앱 → 시작 프로그램을 확인하세요.","Autostart kann nicht geändert werden. Windows-Einstellungen → Apps → Autostart prüfen.","Impossible de modifier le démarrage. Vérifiez Paramètres Windows → Applications → Démarrage.","No se puede cambiar el inicio. Revise Configuración de Windows → Aplicaciones → Inicio.","Não foi possível alterar a inicialização. Verifique Configurações do Windows → Aplicativos → Inicialização.","Impossibile modificare l'avvio. Controlla Impostazioni Windows → App → Avvio.","Не удалось изменить автозапуск. Проверьте Параметры Windows → Приложения → Автозагрузка.","Tidak dapat mengubah startup. Periksa Pengaturan Windows → Aplikasi → Startup."] as &[&str; 12],
        "自動起動の設定処理中です。少し待ってから再度お試しください。" => &["自動起動の設定処理中です。少し待ってから再度お試しください。","Startup settings are busy. Try again shortly.","正在处理启动设置。请稍后重试。","正在處理啟動設定。請稍後再試。","시작 설정을 처리 중입니다. 잠시 후 다시 시도하세요.","Autostart wird verarbeitet. Bitte gleich erneut versuchen.","Modification du démarrage en cours. Réessayez dans un instant.","Procesando el inicio. Inténtelo de nuevo en un momento.","Processando a inicialização. Tente novamente em instantes.","Impostazioni di avvio in corso. Riprova tra poco.","Настройка автозапуска занята. Повторите попытку позже.","Pengaturan startup sedang diproses. Coba lagi sebentar."] as &[&str; 12],
        "自動起動の設定を開けません" => &["自動起動の設定を開けません","Cannot open startup settings","无法打开启动设置","無法開啟啟動設定","시작 설정을 열 수 없습니다","Autostart-Einstellungen können nicht geöffnet werden","Impossible d'ouvrir les paramètres de démarrage","No se puede abrir la configuración de inicio","Não foi possível abrir as configurações de inicialização","Impossibile aprire le impostazioni di avvio","Не удалось открыть настройки автозапуска","Tidak dapat membuka pengaturan startup"] as &[&str; 12],
        "自動起動を変更できません" => &["自動起動を変更できません","Cannot change startup settings","无法更改启动设置","無法變更啟動設定","시작 설정을 변경할 수 없습니다","Autostart kann nicht geändert werden","Impossible de modifier le démarrage","No se puede cambiar el inicio","Não foi possível alterar a inicialização","Impossibile modificare l'avvio","Не удалось изменить автозапуск","Tidak dapat mengubah startup"] as &[&str; 12],
        "アプリの起動ロックを作成できません。" => &["アプリの起動ロックを作成できません。","Cannot create the application startup lock.","无法创建应用启动锁。","無法建立應用程式啟動鎖。","앱 시작 잠금을 만들 수 없습니다.","Die Anwendungssperre kann nicht erstellt werden.","Impossible de créer le verrou de l'application.","No se puede crear el bloqueo de inicio de la aplicación.","Não foi possível criar o bloqueio de inicialização do aplicativo.","Impossibile creare il blocco di avvio dell'app.","Не удалось создать блокировку запуска приложения.","Tidak dapat membuat kunci startup aplikasi."] as &[&str; 12],
        "ウィンドウを登録できません" => &["ウィンドウを登録できません","Cannot register the window","无法注册窗口","無法註冊視窗","창을 등록할 수 없습니다","Fenster kann nicht registriert werden","Impossible d'enregistrer la fenêtre","No se puede registrar la ventana","Não foi possível registrar a janela","Impossibile registrare la finestra","Не удалось зарегистрировать окно","Tidak dapat mendaftarkan jendela"] as &[&str; 12],
        "ウィンドウを作成できません" => &["ウィンドウを作成できません","Cannot create the window","无法创建窗口","無法建立視窗","창을 만들 수 없습니다","Fenster kann nicht erstellt werden","Impossible de créer la fenêtre","No se puede crear la ventana","Não foi possível criar a janela","Impossibile creare la finestra","Не удалось создать окно","Tidak dapat membuat jendela"] as &[&str; 12],
        _ => return key,
    };
    row[language as usize]
}
pub fn message(key: &str, value: &str) -> String {
    tr(key).replacen("{}", value, 1)
}
pub fn font_name() -> &'static str {
    match language() {
        Language::Ja => "Yu Gothic UI",
        Language::ZhHans => "Microsoft YaHei UI",
        Language::ZhHant => "Microsoft JhengHei UI",
        Language::Ko => "Malgun Gothic",
        _ => "Segoe UI",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_language_tags_and_fallback() {
        for tag in ["zh-TW", "zh-HK", "zh-MO", "zh-Hant", "zh-Hant-HK"] {
            assert_eq!(Language::from_tag(tag), Some(Language::ZhHant));
        }
        for tag in ["zh-CN", "zh-SG", "zh", "zh-Hans-TW"] {
            assert_eq!(Language::from_tag(tag), Some(Language::ZhHans));
        }
        assert_eq!(Language::from_tag("EN_us"), Some(Language::En));
        assert_eq!(
            preferred_language(["ar-SA", "fr-CA", "ja-JP"]),
            Language::Fr
        );
        assert_eq!(preferred_language(["ar-SA"]), Language::En);
        assert_eq!(preferred_language(["ja-JP", "en-US"]), Language::Ja);
        assert_eq!(Language::from_tag("pt-BR"), Some(Language::Pt));
    }
    #[test]
    fn every_translation_is_present_and_keeps_placeholders() {
        for lang in LANGUAGES {
            for key in ["言語", "Windowsに合わせる（自動）"] {
                assert!(!translate(lang, key).is_empty());
                if lang != Language::Ja {
                    assert_ne!(translate(lang, key), key);
                }
            }
            assert!(!lang.native_name().is_empty());
        }
        let keys = ["トレイに格納", "{}前", "専用", "受信", "送信", "通信の目盛り上限", "送受信合計", "メモリ", "ディスク", "1分", "10分", "60分", "6時間", "12時間", "使用率", "速度", "温度", "使用量", "読み込み", "書き込み", "稼働率", "受信 Mbps", "送信 Mbps", "接続状態", "未接続", "GPUメモリ", "設定 ···", "見本データ", "使用率 %", "通信 Mbps", "現在", "計測を準備しています…", "（見本）", "時間", "分", "秒", "ウィンドウを表示", "常に手前に表示 (&T)", "グラフだけ表示 (ダブルクリック / F11)", "Windowsログオン時に起動", "終了 (&X)", "クリックして表示", "履歴: {}", "線を表示: {}", "読込", "書込", "通信の目盛り上限 {} Mbps（送受信合計）", "ディスク: 稼働率は最大、読み書きは全物理ディスク合計", "自動起動を変更できません。Windowsの「設定 → アプリ → スタートアップ」も確認してください。", "自動起動の設定処理中です。少し待ってから再度お試しください。", "自動起動の設定を開けません", "自動起動を変更できません", "アプリの起動ロックを作成できません。", "ウィンドウを登録できません", "ウィンドウを作成できません"];
        for lang in LANGUAGES {
            assert_eq!(Language::from_tag(lang.tag()), Some(lang));
            for key in keys {
                let value = translate(lang, key);
                assert!(!value.trim().is_empty(), "{lang:?}: {key}");
                assert_eq!(
                    key.matches("{}").count(),
                    value.matches("{}").count(),
                    "{lang:?}: {key}"
                );
                if lang != Language::Ja {
                    assert!(
                        !value
                            .chars()
                            .any(|c| ('\u{3040}'..='\u{30ff}').contains(&c)),
                        "{lang:?}: {key}"
                    );
                }
            }
        }
    }
}
