import Foundation

/// Lightweight application localization. Traditional Chinese source text is the stable
/// key, so preset and storage identifiers never change when the display language does —
/// only the presentation layer is translated. The table is a direct port of the Windows
/// build's, entry for entry.
///
/// German, French and Spanish run long: any translation that has to fit a fixed-width
/// control was already shortened on Windows and is carried over verbatim here.
public enum L {

    public struct Tr {
        let en, ja, ko, hans, de, fr, es: String
        init(_ en: String, _ ja: String, _ ko: String, _ hans: String,
             _ de: String, _ fr: String, _ es: String) {
            self.en = en; self.ja = ja; self.ko = ko; self.hans = hans
            self.de = de; self.fr = fr; self.es = es
        }
    }

    nonisolated(unsafe) public private(set) static var currentLanguage: AppLanguage = .traditionalChinese

    public static var currentLocale: Locale {
        switch currentLanguage {
        case .english: return Locale(identifier: "en_US")
        case .japanese: return Locale(identifier: "ja_JP")
        case .korean: return Locale(identifier: "ko_KR")
        case .simplifiedChinese: return Locale(identifier: "zh_CN")
        case .german: return Locale(identifier: "de_DE")
        case .french: return Locale(identifier: "fr_FR")
        case .spanish: return Locale(identifier: "es_ES")
        case .traditionalChinese: return Locale(identifier: "zh_TW")
        }
    }

    public static func setLanguage(_ language: AppLanguage) {
        currentLanguage = language
    }

    /// Translate one piece of source text.
    public static func t(_ source: String) -> String {
        guard currentLanguage != .traditionalChinese, let tr = table[source] else { return source }
        switch currentLanguage {
        case .english: return tr.en
        case .japanese: return tr.ja
        case .korean: return tr.ko
        case .simplifiedChinese: return tr.hans
        case .german: return tr.de
        case .french: return tr.fr
        case .spanish: return tr.es
        case .traditionalChinese: return source
        }
    }

    /// Translate and substitute {0}, {1}, … the way the C# `string.Format` calls did.
    public static func f(_ source: String, _ args: Any?...) -> String {
        var s = t(source)
        for (i, a) in args.enumerated() {
            let v: String
            switch a {
            case let x as String: v = x
            case let x as Int: v = String(x)
            case let x as Double: v = DotNetXml.string(x)
            case .some(let x): v = "\(x)"
            case .none: v = ""
            }
            s = s.replacingOccurrences(of: "{\(i)}", with: v)
        }
        return s
    }

    /// Context-specific text for labels whose Chinese source word is ambiguous.
    public static func pick(_ zh: String, _ en: String, _ ja: String, _ ko: String,
                            _ hans: String, _ de: String, _ fr: String, _ es: String) -> String {
        switch currentLanguage {
        case .english: return en
        case .japanese: return ja
        case .korean: return ko
        case .simplifiedChinese: return hans
        case .german: return de
        case .french: return fr
        case .spanish: return es
        case .traditionalChinese: return zh
        }
    }

    public static func languageDisplayName(_ language: AppLanguage) -> String {
        switch language {
        case .traditionalChinese: return "繁體中文（台灣）"
        case .english: return "English (United States)"
        case .japanese: return "日本語（日本）"
        case .korean: return "한국어（대한민국）"
        case .simplifiedChinese: return "简体中文（中国）"
        case .german: return "Deutsch (Deutschland)"
        case .french: return "Français (France)"
        case .spanish: return "Español (España)"
        }
    }

    /// The full body text behind Settings → "RAW 處理精度" → 說明. The Traditional Chinese
    /// original is itself the key (every key in the table is source text; an earlier
    /// Windows version mistakenly keyed it on the dialog title, so the Chinese UI showed
    /// only the heading).
    public static let rawPrecisionHelp = """
    8-bit：RAW 解碼成每色 256 階再處理，預覽快取每張約 3–5 MB。一般調整足夠。

    16-bit：RAW 解碼成每色 65536 階（以浮點處理），預覽快取每張約 35 MB、第一次開資料夾較慢。大幅拉曝光或暗部時不會出現色階斷層（banding）。

    兩者的運算與輸出都一樣（float 運算、8-bit 輸出），差別只在 RAW 解碼保留多少資訊；對 JPG 等非 RAW 沒有影響。

    變更後請用「關閉資料夾並刪除快取縮圖」重新產生預覽快取，舊快取不會自動更新（注意：macOS 版會連同調整設定一起刪除整個 RAW_TEMP）。
    """

    // MARK: - Translation table (ported entry-for-entry from the Windows build)

    // An array merged into a dictionary rather than a dictionary literal: a literal with a
    // duplicate key traps at first use, and `t` only touches the table for the seven
    // non-Chinese languages — so such a duplicate crashed every user except the
    // Traditional Chinese ones while every Chinese-language test passed. With this the
    // later entry simply wins.
    static let table: [String: Tr] = Dictionary(entries, uniquingKeysWith: { _, last in last })

    public static let entries: [(String, Tr)] = [
        ("格式", Tr("Format", "形式", "형식", "格式", "Format", "Format", "Formato")),
        ("重做", Tr("Redo", "やり直し", "다시 실행", "重做", "Wiederholen", "Rétablir", "Rehacer")),
        ("檔案", Tr("File", "ファイル", "파일", "文件", "Datei", "Fichier", "Archivo")),
        ("編輯", Tr("Edit", "編集", "편집", "编辑", "Bearbeiten", "Édition", "Edición")),
        ("檢視", Tr("View", "表示", "보기", "查看", "Ansicht", "Affichage", "Vista")),
        ("隱藏", Tr("Hide", "隠す", "가리기", "隐藏", "Ausblenden", "Masquer", "Ocultar")),
        ("重新命名規則", Tr("Naming rule", "命名規則", "이름 규칙", "重命名规则", "Namensregel", "Règle de nommage", "Regla de nombres")),
        ("寬長最大", Tr("Long edge", "長辺", "긴 변", "宽长最大", "Lange Kante", "Grand côté", "Lado largo")),
        ("解析度", Tr("DPI", "解像度", "해상도", "分辨率", "DPI", "DPI", "PPP")),
        ("邊距", Tr("Margin", "余白", "여백", "边距", "Rand", "Marge", "Margen")),
        ("白", Tr("White", "白", "흰색", "白", "Weiß", "Blanc", "Blanco")),
        ("黑", Tr("Black", "黒", "검정", "黑", "Schwarz", "Noir", "Negro")),
        ("藍", Tr("Blue", "青", "파랑", "蓝", "Blau", "Bleu", "Azul")),
        ("黃", Tr("Yellow", "黄", "노랑", "黄", "Gelb", "Jaune", "Amarillo")),
        ("綠", Tr("Green", "緑", "초록", "绿", "Grün", "Vert", "Verde")),
        ("紅", Tr("Red", "赤", "빨강", "红", "Rot", "Rouge", "Rojo")),
        ("灰", Tr("Gray", "灰", "회색", "灰", "Grau", "Gris", "Gris")),
        ("橘", Tr("Orange", "橙", "주황", "橙", "Orange", "Orange", "Naranja")),
        ("尚未選擇照片", Tr("No photo selected", "写真が選択されていません", "선택된 사진 없음", "尚未选择照片", "Kein Foto ausgewählt", "Aucune photo sélectionnée", "Ninguna foto seleccionada")),
        ("需要重新啟動", Tr("Restart required", "再起動が必要です", "다시 시작 필요", "需要重新启动", "Neustart erforderlich", "Redémarrage requis", "Reinicio necesario")),
        ("變更介面設定後需要重新啟動程式才會套用。", Tr("The interface change takes effect after the app restarts.", "インターフェース設定の変更は再起動後に反映されます。", "인터페이스 변경은 앱을 다시 시작한 후 적용됩니다.", "更改界面设置后需要重新启动程序才会应用。", "Die Änderung wird nach einem Neustart wirksam.", "Le changement prendra effet après le redémarrage.", "El cambio se aplicará tras reiniciar.")),
        ("立即重新啟動", Tr("Restart now", "今すぐ再起動", "지금 다시 시작", "立即重新启动", "Jetzt neu starten", "Redémarrer maintenant", "Reiniciar ahora")),
        ("稍後", Tr("Later", "後で", "나중에", "稍后", "Später", "Plus tard", "Más tarde")),
        ("已選 {0} 張", Tr("{0} selected", "{0} 枚選択中", "{0}장 선택됨", "已选 {0} 张", "{0} ausgewählt", "{0} sélectionnées", "{0} seleccionadas")),
        ("刪除", Tr("Delete", "削除", "삭제", "删除", "Löschen", "Supprimer", "Eliminar")),
        ("此資料夾沒有支援的影像", Tr("This folder has no supported images", "このフォルダーに対応画像がありません", "이 폴더에 지원되는 이미지가 없습니다", "此文件夹没有支持的图像", "Dieser Ordner enthält keine unterstützten Bilder", "Ce dossier ne contient aucune image prise en charge", "Esta carpeta no contiene imágenes compatibles")),
        ("將刪除 {0} 內的縮圖與預覽快取（調整設定會保留）。", Tr("The thumbnail and preview cache in {0} will be deleted (adjustments are kept).", "{0} 内のサムネイルとプレビューキャッシュを削除します（調整は保持されます）。", "{0}의 썸네일과 미리보기 캐시를 삭제합니다 (조정은 유지됨).", "将删除 {0} 内的缩略图与预览缓存（调整设置会保留）。", "Der Miniatur- und Vorschau-Cache in {0} wird gelöscht (Anpassungen bleiben erhalten).", "Le cache des vignettes et aperçus de {0} sera supprimé (les réglages sont conservés).", "Se eliminará la caché de miniaturas y vistas previas de {0} (los ajustes se conservan).")),
        ("將刪除整個 {0} 資料夾，包含所有調整設定、隱藏狀態與虛擬副本。此操作無法在程式內復原。", Tr("The entire {0} folder will be deleted, including all adjustments, hidden flags and virtual copies. This cannot be undone from within the app.", "{0} フォルダー全体を削除します。すべての調整、非表示状態、仮想コピーも含まれます。アプリ内では元に戻せません。", "{0} 폴더 전체를 삭제합니다. 모든 조정, 숨김 상태, 가상 복사본이 포함됩니다. 앱 내에서 되돌릴 수 없습니다.", "将删除整个 {0} 文件夹，包含所有调整设置、隐藏状态与虚拟副本。此操作无法在程序内撤销。", "Der gesamte Ordner {0} wird gelöscht, einschließlich aller Anpassungen, Ausblendungen und virtuellen Kopien. Dies kann in der App nicht rückgängig gemacht werden.", "Le dossier {0} entier sera supprimé, y compris tous les réglages, les photos masquées et les copies virtuelles. Cette action est irréversible dans l'application.", "Se eliminará la carpeta {0} completa, incluidos todos los ajustes, fotos ocultas y copias virtuales. No se puede deshacer desde la aplicación.")),
        ("已關閉資料夾並刪除 {0}", Tr("Folder closed and {0} deleted", "フォルダーを閉じ、{0} を削除しました", "폴더를 닫고 {0}을(를) 삭제했습니다", "已关闭文件夹并删除 {0}", "Ordner geschlossen und {0} gelöscht", "Dossier fermé et {0} supprimé", "Carpeta cerrada y {0} eliminado")),
        ("無法刪除 {0}", Tr("Could not delete {0}", "{0} を削除できません", "{0}을(를) 삭제할 수 없습니다", "无法删除 {0}", "{0} konnte nicht gelöscht werden", "Impossible de supprimer {0}", "No se pudo eliminar {0}")),
        ("還原備份", Tr("Restore backup", "バックアップを復元", "백업 복원", "还原备份", "Sicherung wiederherstellen", "Restaurer la sauvegarde", "Restaurar copia")),
        ("無法新增", Tr("Cannot add", "追加できません", "추가할 수 없음", "无法新增", "Hinzufügen nicht möglich", "Ajout impossible", "No se puede añadir")),
        ("「{0}」是保留名稱，請換一個。", Tr("\"{0}\" is a reserved name; please choose another.", "「{0}」は予約済みの名前です。別の名前を選んでください。", "\"{0}\"은(는) 예약된 이름입니다. 다른 이름을 선택하세요.", "“{0}”是保留名称，请换一个。", "„{0}“ ist ein reservierter Name; bitte einen anderen wählen.", "« {0} » est un nom réservé ; choisissez-en un autre.", "«{0}» es un nombre reservado; elige otro.")),
        ("拖曳白點移動、黃點調範圍、藍點旋轉；右鍵刪除", Tr("Drag the white dot to move, yellow to set range, blue to rotate; right-click to delete", "白い点をドラッグで移動、黄で範囲、青で回転。右クリックで削除", "흰 점 드래그로 이동, 노란 점으로 범위, 파란 점으로 회전, 오른쪽 클릭으로 삭제", "拖动白点移动、黄点调范围、蓝点旋转；右键删除", "Weißen Punkt ziehen: verschieben, gelb: Bereich, blau: drehen; Rechtsklick löscht", "Point blanc : déplacer, jaune : étendue, bleu : rotation ; clic droit : supprimer", "Punto blanco: mover, amarillo: rango, azul: rotar; clic derecho: eliminar")),
        ("點擊畫面新增修護點；拖曳移動、右鍵刪除", Tr("Click the image to add a spot; drag to move, right-click to delete", "画像をクリックでスポット追加。ドラッグで移動、右クリックで削除", "이미지를 클릭해 스팟 추가, 드래그로 이동, 오른쪽 클릭으로 삭제", "点击画面新增修复点；拖动移动、右键删除", "Klick ins Bild fügt einen Punkt hinzu; ziehen verschiebt, Rechtsklick löscht", "Cliquez sur l'image pour ajouter un point ; glisser pour déplacer, clic droit pour supprimer", "Haz clic en la imagen para añadir un punto; arrastra para mover, clic derecho para eliminar")),
        ("基本 / 色彩 / 細節 重設", Tr("Reset Basic / Color / Detail", "基本／色彩／ディテールをリセット", "기본 / 색상 / 세부 재설정", "基本 / 色彩 / 细节 重置", "Basis / Farbe / Details zurücksetzen", "Réinitialiser Base / Couleur / Détails", "Restablecer Básico / Color / Detalle")),
        ("將 {0} 張照片改用新的色彩管線，滑桿值會換算成盡量接近目前的樣子。", Tr("{0} photo(s) will switch to the new colour pipeline; slider values are converted to look as close as possible.", "{0} 枚の写真を新しいカラーパイプラインに切り替えます。スライダー値はできるだけ近い見た目に換算されます。", "{0}장의 사진을 새 색상 파이프라인으로 전환합니다. 슬라이더 값은 최대한 비슷해 보이도록 변환됩니다.", "将 {0} 张照片改用新的色彩管线，滑块值会换算成尽量接近目前的样子。", "{0} Foto(s) wechseln auf die neue Farbpipeline; die Reglerwerte werden möglichst ähnlich umgerechnet.", "{0} photo(s) passeront au nouveau pipeline couleur ; les valeurs des curseurs seront converties au plus proche.", "{0} foto(s) pasarán al nuevo flujo de color; los valores se convertirán para verse lo más parecido posible.")),
        ("已經有名為「{0}」的風格檔。", Tr("A preset named \"{0}\" already exists.", "「{0}」という名前のプリセットは既にあります。", "\"{0}\" 프리셋이 이미 있습니다.", "已经有名为“{0}”的风格文件。", "Ein Preset namens „{0}“ existiert bereits.", "Un préréglage nommé « {0} » existe déjà.", "Ya existe un ajuste llamado «{0}».")),
        ("這個檔案不是風格檔備份。", Tr("This file is not a preset backup.", "このファイルはプリセットのバックアップではありません。", "이 파일은 프리셋 백업이 아닙니다.", "这个文件不是风格文件备份。", "Diese Datei ist keine Preset-Sicherung.", "Ce fichier n'est pas une sauvegarde de préréglages.", "Este archivo no es una copia de ajustes.")),
        ("將刪除所有自訂風格檔，並把內建風格檔恢復為預設值。", Tr("All custom presets will be deleted and the built-in ones restored to their defaults.", "すべてのカスタムプリセットを削除し、内蔵プリセットを既定値に戻します。", "모든 사용자 프리셋을 삭제하고 내장 프리셋을 기본값으로 되돌립니다.", "将删除所有自定义风格文件，并把内置风格文件恢复为默认值。", "Alle eigenen Presets werden gelöscht und die eingebauten zurückgesetzt.", "Tous les préréglages personnalisés seront supprimés et ceux intégrés restaurés.", "Se eliminarán todos los ajustes personalizados y se restaurarán los integrados.")),
        ("GPU 加速（Metal）", Tr("GPU acceleration (Metal)", "GPU アクセラレーション（Metal）", "GPU 가속 (Metal)", "GPU 加速（Metal）", "GPU-Beschleunigung (Metal)", "Accélération GPU (Metal)", "Aceleración GPU (Metal)")),
        ("算圖", Tr("Rendering", "レンダリング", "렌더링", "渲染", "Rendering", "Rendu", "Renderizado")),
        ("此電腦無法使用 Metal，一律以 CPU 算圖", Tr("Metal is unavailable on this Mac; rendering always uses the CPU", "この Mac では Metal を利用できないため、常に CPU で処理します", "이 Mac에서는 Metal을 사용할 수 없어 항상 CPU로 처리합니다", "此电脑无法使用 Metal，一律以 CPU 渲染", "Metal ist auf diesem Mac nicht verfügbar; es wird immer die CPU verwendet", "Metal n'est pas disponible sur ce Mac ; le rendu utilise toujours le processeur", "Metal no está disponible en este Mac; el renderizado siempre usa la CPU")),
        ("{0} MP 以下的預覽使用，全解析度匯出走 CPU", Tr("Used for previews up to {0} MP; full-resolution export uses the CPU", "{0} MP までのプレビューで使用、フル解像度の書き出しは CPU", "{0} MP 이하 미리보기에 사용, 전체 해상도 내보내기는 CPU", "{0} MP 以下的预览使用，全分辨率导出走 CPU", "Für Vorschauen bis {0} MP; Export in voller Auflösung nutzt die CPU", "Utilisé pour les aperçus jusqu'à {0} MP ; l'export pleine résolution utilise le processeur", "Se usa en vistas previas de hasta {0} MP; la exportación a resolución completa usa la CPU")),
        ("無法檢查更新，請稍後再試", Tr("Could not check for updates. Please try again later.", "アップデートを確認できませんでした。しばらくしてからもう一度お試しください。", "업데이트를 확인할 수 없습니다. 나중에 다시 시도해 주세요.", "无法检查更新，请稍后再试。", "Updates konnten nicht geprüft werden. Bitte später erneut versuchen.", "Impossible de vérifier les mises à jour. Réessayez plus tard.", "No se pudo buscar actualizaciones. Inténtalo más tarde.")),
        ("已是最新版本（{0}）", Tr("You are up to date ({0})", "最新バージョンです（{0}）", "최신 버전입니다 ({0})", "已是最新版本（{0}）", "Sie sind auf dem neuesten Stand ({0})", "Vous êtes à jour ({0})", "Ya tienes la última versión ({0})")),
        ("有新版本 v{0}", Tr("Version v{0} is available", "新しいバージョン v{0} があります", "새 버전 v{0} 이(가) 있습니다", "有新版本 v{0}", "Version v{0} ist verfügbar", "La version v{0} est disponible", "La versión v{0} está disponible")),
        ("設定", Tr("Settings", "設定", "설정", "设置", "Einstellungen", "Paramètres", "Configuración")),
        ("設定…", Tr("Settings…", "設定…", "설정…", "设置…", "Einstellungen…", "Paramètres…", "Configuración…")),
        ("套用", Tr("Apply", "適用", "적용", "应用", "Anwenden", "Appliquer", "Aplicar")),
        ("確定", Tr("OK", "OK", "확인", "确定", "OK", "OK", "Aceptar")),
        ("取消", Tr("Cancel", "キャンセル", "취소", "取消", "Abbrechen", "Annuler", "Cancelar")),
        ("關閉", Tr("Close", "閉じる", "닫기", "关闭", "Schließen", "Fermer", "Cerrar")),
        ("一般選項", Tr("General", "一般", "일반", "常规", "Allgemein", "Général", "General")),
        ("介面風格", Tr("Interface style", "インターフェーススタイル", "인터페이스 스타일", "界面风格", "Oberflächenstil", "Style d'interface", "Estilo de interfaz")),
        ("語言", Tr("Language", "言語", "언어", "语言", "Sprache", "Langue", "Idioma")),
        ("介面大小", Tr("UI size", "UI サイズ", "UI 크기", "界面大小", "UI-Größe", "Taille de l'UI", "Tamaño de UI")),
        ("下載:", Tr("Download:", "ダウンロード:", "다운로드:", "下载:", "Download:", "Téléchargement :", "Descarga:")),
        ("授權：", Tr("License: ", "ライセンス: ", "라이선스: ", "许可: ", "Lizenz: ", "Licence : ", "Licencia: ")),
        ("檢查更新", Tr("Check for updates", "更新を確認", "업데이트 확인", "检查更新", "Nach Updates suchen", "Rechercher des mises à jour", "Buscar actualizaciones")),
        ("檢查中…", Tr("Checking…", "確認中…", "확인 중…", "检查中…", "Wird gesucht…", "Recherche…", "Buscando…")),
        ("目前已是最新版本（{0}）。", Tr("You already have the latest version ({0}).", "すでに最新バージョン（{0}）です。", "이미 최신 버전({0})입니다.", "当前已是最新版本（{0}）。", "Sie haben bereits die neueste Version ({0}).", "Vous disposez déjà de la dernière version ({0}).", "Ya tienes la última versión ({0}).")),
        ("有新版本可以下載。\n\n目前版本：{0}\n最新版本：v{1}", Tr("A new version is available.\n\nCurrent version: {0}\nLatest version: v{1}", "新しいバージョンがあります。\n\n現在のバージョン: {0}\n最新バージョン: v{1}", "새 버전이 있습니다.\n\n현재 버전: {0}\n최신 버전: v{1}", "有新版本可以下载。\n\n当前版本：{0}\n最新版本：v{1}", "Eine neue Version ist verfügbar.\n\nAktuelle Version: {0}\nNeueste Version: v{1}", "Une nouvelle version est disponible.\n\nVersion actuelle : {0}\nDernière version : v{1}", "Hay una nueva versión disponible.\n\nVersión actual: {0}\nÚltima versión: v{1}")),
        ("要開啟下載頁面嗎？", Tr("Open the download page?", "ダウンロードページを開きますか？", "다운로드 페이지를 열까요?", "要打开下载页面吗？", "Download-Seite öffnen?", "Ouvrir la page de téléchargement ?", "¿Abrir la página de descarga?")),
        ("無法連線到更新伺服器，請稍後再試。", Tr("Could not reach the update server. Please try again later.", "更新サーバーに接続できませんでした。後でもう一度お試しください。", "업데이트 서버에 연결할 수 없습니다. 잠시 후 다시 시도해 주세요.", "无法连接到更新服务器，请稍后再试。", "Der Update-Server ist nicht erreichbar. Bitte später erneut versuchen.", "Impossible de joindre le serveur de mise à jour. Réessayez plus tard.", "No se pudo conectar con el servidor de actualizaciones. Inténtalo más tarde.")),
        ("歡迎自由修改成你自己的版本，只希望你能在你的「關於」視窗中提及來源是這裡（AwayPhotoRawEditor / Awaysu）。", Tr("You are welcome to modify this into your own version — I only ask that you credit the original source (AwayPhotoRawEditor / Awaysu) in your About dialog.", "自由に改変してご自身のバージョンを作って構いません。ただし「バージョン情報」に出典（AwayPhotoRawEditor / Awaysu）を記載してください。", "자유롭게 수정해 자신의 버전을 만들어도 됩니다. 다만 정보 창에 출처(AwayPhotoRawEditor / Awaysu)를 밝혀 주세요.", "欢迎自由修改成你自己的版本，只希望你能在你的“关于”窗口中提及来源是这里（AwayPhotoRawEditor / Awaysu）。", "Sie dürfen dies frei zu einer eigenen Version ändern — ich bitte nur darum, die Quelle (AwayPhotoRawEditor / Awaysu) in Ihrem Info-Dialog zu nennen.", "Vous pouvez librement en faire votre propre version — je demande seulement de créditer la source (AwayPhotoRawEditor / Awaysu) dans votre fenêtre À propos.", "Puedes modificarlo libremente para crear tu propia versión — solo te pido que menciones la fuente (AwayPhotoRawEditor / Awaysu) en tu ventana Acerca de.")),
        ("自動（依螢幕大小）", Tr("Automatic (fit screen)", "自動（画面に合わせる）", "자동 (화면에 맞춤)", "自动（适应屏幕）", "Automatisch (Bildschirm)", "Automatique (écran)", "Automático (pantalla)")),
        ("變更介面大小後將自動重新啟動程式", Tr("Restarts after a UI size change", "UI サイズ変更後に自動で再起動します", "UI 크기 변경 후 앱 재시작", "更改界面大小后将自动重新启动程序", "Neustart nach Größenwechsel", "Redémarre après changement de taille", "Se reinicia al cambiar el tamaño")),
        ("變更語言後將自動重新啟動程式", Tr("Restarts after a language change", "言語変更後に自動で再起動します", "언어 변경 후 앱 재시작", "更改语言后将自动重新启动程序", "Neustart nach Sprachwechsel", "Redémarre après changement de langue", "Se reinicia al cambiar el idioma")),
        ("點選預覽即可切換，套用後立即生效", Tr("Choose a preview; changes apply immediately", "プレビューを選択すると、適用後すぐに反映されます", "미리보기를 선택하면 적용 즉시 반영됩니다", "点击预览即可切换，应用后立即生效", "Vorschau anklicken – gilt sofort", "Cliquez sur un aperçu – effet immédiat", "Vista previa – efecto inmediato")),
        ("經典深色", Tr("Classic Dark", "クラシックダーク", "클래식 다크", "经典深色", "Klassisch Dunkel", "Sombre classique", "Oscuro clásico")),
        ("暖白相紙", Tr("Warm Paper", "ウォームペーパー", "웜 페이퍼", "暖白相纸", "Warmes Papier", "Papier chaud", "Papel cálido")),
        ("低亮度專業工作區\n藍色重點操作", Tr("Low-light workspace\nBlue accents", "暗い作業環境\nブルーアクセント", "어두운 작업 공간\n블루 포인트", "低亮度专业工作区\n蓝色重点操作", "Dunkler Arbeitsbereich\nBlaue Akzente", "Espace sombre\nAccents bleus", "Espacio oscuro\nAcentos azules")),
        ("明亮暖灰工作區\n陶土橘重點操作", Tr("Bright warm-gray\nTerracotta accents", "明るい暖色グレー\nテラコッタ", "밝은 웜 그레이\n테라코타 포인트", "明亮暖灰工作区\n陶土橘重点操作", "Helles Warmgrau\nTerrakotta-Akzente", "Gris chaud lumineux\nAccents terracotta", "Gris cálido claro\nAcentos terracota")),
        ("使用 LibRaw", Tr("Use LibRaw", "LibRawを使用", "LibRaw 사용", "使用 LibRaw", "LibRaw verwenden", "Utiliser LibRaw", "Usar LibRaw")),
        ("RAW 處理精度", Tr("RAW precision", "RAW処理精度", "RAW 처리 정밀도", "RAW 处理精度", "RAW-Genauigkeit", "Précision RAW", "Precisión RAW")),
        ("8-bit（省空間）", Tr("8-bit (saves space)", "8-bit（省スペース）", "8-bit (공간 절약)", "8-bit（省空间）", "8 Bit (platzsparend)", "8 bits (économise l'espace)", "8 bits (ahorra espacio)")),
        ("16-bit（高精度）", Tr("16-bit (high precision)", "16-bit（高精度）", "16-bit (고정밀)", "16-bit（高精度）", "16 Bit (hohe Genauigkeit)", "16 bits (haute précision)", "16 bits (alta precisión)")),
        ("說明", Tr("What's this?", "説明", "설명", "说明", "Erklärung", "Explication", "Explicación")),
        ("字體大小…", Tr("Font sizes…", "フォントサイズ…", "글꼴 크기…", "字体大小…", "Schriftgrößen…", "Tailles de police…", "Tamaños de fuente…")),
        ("預設比例", Tr("Default sizes", "既定サイズ", "기본 크기", "默认比例", "Standardgrößen", "Tailles par défaut", "Tamaños predeterminados")),
        ("套用風格檔時維持照片目前的色溫／色調", Tr("Keeps the photo's temperature / tint", "適用時は色温度／色かぶりを維持", "적용 시 색온도/틴트 유지", "套用风格文件时保持照片当前的色温／色调", "Temperatur/Tönung bleibt erhalten", "Température/teinte conservées", "Temperatura/tinte se conservan")),
        ("已自訂：一般 {0}px、區塊標題 {1}px、小字 {2}px", Tr("Custom: normal {0}px, section title {1}px, small {2}px", "カスタム：標準 {0}px、セクション見出し {1}px、小 {2}px", "사용자 지정: 일반 {0}px, 섹션 제목 {1}px, 작게 {2}px", "已自定义：常规 {0}px、区块标题 {1}px、小字 {2}px", "Angepasst: normal {0}px, Abschnittstitel {1}px, klein {2}px", "Personnalisé : normal {0}px, titre de section {1}px, petit {2}px", "Personalizado: normal {0}px, título de sección {1}px, pequeño {2}px")),
        ("使用 GPU 加速算圖（偵測不到或失敗時自動改用 CPU）", Tr("Use GPU acceleration (falls back to CPU when unavailable)", "GPUアクセラレーションを使用（使えない場合はCPUに戻す）", "GPU 가속 사용 (사용 불가 시 CPU로 대체)", "使用 GPU 加速运算（检测不到或失败时自动改用 CPU）", "GPU-Beschleunigung verwenden (sonst CPU)", "Accélération GPU (sinon CPU)", "Aceleración GPU (si no, CPU)")),
        ("GPU 算圖", Tr("GPU", "GPU", "GPU", "GPU 运算", "GPU", "GPU", "GPU")),
        ("GPU：未偵測到可用裝置，使用 CPU", Tr("GPU: no usable device, using CPU", "GPU：利用可能なデバイスなし、CPUを使用", "GPU: 사용 가능한 장치 없음, CPU 사용", "GPU：未检测到可用设备，使用 CPU", "GPU: kein nutzbares Gerät, CPU wird verwendet", "GPU : aucun périphérique utilisable, CPU utilisé", "GPU: sin dispositivo utilizable, se usa la CPU")),
        ("GPU：已停用（設定）", Tr("GPU: disabled (settings)", "GPU：無効（設定）", "GPU: 비활성화됨(설정)", "GPU：已停用（设置）", "GPU: deaktiviert (Einstellungen)", "GPU : désactivé (paramètres)", "GPU: desactivada (ajustes)")),
        ("GPU：", Tr("GPU: ", "GPU：", "GPU: ", "GPU：", "GPU: ", "GPU : ", "GPU: ")),
        ("高精度 RAW 處理流程 (16-bit / float)", Tr("High-precision RAW pipeline (16-bit / float)", "高精度RAW処理（16-bit / float）", "고정밀 RAW 처리 (16-bit / float)", "高精度 RAW 处理流程 (16-bit / float)", "Hochpräzise RAW-Pipeline (16-bit / float)", "Pipeline RAW haute précision (16 bits / float)", "Proceso RAW de alta precisión (16 bits / float)")),
        ("在縮圖左上顯示編號 (#1, #2 …)", Tr("Show numbers on thumbnails (#1, #2 …)", "サムネイル左上に番号を表示（#1、#2 …）", "썸네일 왼쪽 위에 번호 표시 (#1, #2 …)", "在缩略图左上显示编号 (#1, #2 …)", "Nummern auf Miniaturen anzeigen (#1, #2 …)", "Afficher les numéros sur les vignettes (#1, #2 …)", "Mostrar números en las miniaturas (#1, #2 …)")),
        ("顯示捲軸（視窗過矮時左右欄可捲動）", Tr("Show scrollbars when side panels do not fit", "ウィンドウが低いときにサイドパネルのスクロールバーを表示", "창 높이가 부족할 때 사이드 패널 스크롤바 표시", "显示滚动条（窗口过矮时左右栏可滚动）", "Scrollleisten anzeigen, wenn die Seitenleisten nicht passen", "Afficher les barres de défilement si nécessaire", "Mostrar barras de desplazamiento si es necesario")),
        ("libraw.dll 已載入", Tr("libraw.dll loaded", "libraw.dll 読み込み済み", "libraw.dll 로드됨", "libraw.dll 已加载", "libraw.dll geladen", "libraw.dll chargé", "libraw.dll cargado")),
        ("libraw.dll 未找到（將退回 WIC / 嵌入預覽）", Tr("libraw.dll not found (using WIC / embedded preview)", "libraw.dllが見つかりません（WIC／埋め込みプレビューを使用）", "libraw.dll을 찾을 수 없음 (WIC / 내장 미리보기 사용)", "libraw.dll 未找到（将回退 WIC / 内嵌预览）", "libraw.dll nicht gefunden (WIC / eingebettete Vorschau)", "libraw.dll introuvable (WIC / aperçu intégré)", "libraw.dll no encontrado (WIC / vista previa integrada)")),
        ("exiftool.exe 已載入", Tr("exiftool.exe loaded", "exiftool.exe 読み込み済み", "exiftool.exe 로드됨", "exiftool.exe 已加载", "exiftool.exe geladen", "exiftool.exe chargé", "exiftool.exe cargado")),
        ("exiftool.exe 未找到（將退回 WIC metadata）", Tr("exiftool.exe not found (using WIC metadata)", "exiftool.exeが見つかりません（WICメタデータを使用）", "exiftool.exe를 찾을 수 없음 (WIC 메타데이터 사용)", "exiftool.exe 未找到（将回退 WIC 元数据）", "exiftool.exe nicht gefunden (WIC-Metadaten)", "exiftool.exe introuvable (métadonnées WIC)", "exiftool.exe no encontrado (metadatos WIC)")),
        ("📁  開啟資料夾", Tr("📁  Open Folder", "📁  フォルダーを開く", "📁  폴더 열기", "📁  打开文件夹", "📁  Ordner öffnen", "📁  Ouvrir", "📁  Abrir carpeta")),
        ("尚未選擇資料夾", Tr("No folder selected", "フォルダーが選択されていません", "선택한 폴더 없음", "尚未选择文件夹", "Kein Ordner ausgewählt", "Aucun dossier sélectionné", "Ninguna carpeta seleccionada")),
        ("匯出全部照片", Tr("Export All Photos", "すべての写真を書き出す", "모든 사진 내보내기", "导出全部照片", "Alle Fotos exportieren", "Tout exporter", "Exportar todas las fotos")),
        ("匯出目前照片", Tr("Export Current Photo", "現在の写真を書き出す", "현재 사진 내보내기", "导出当前照片", "Foto exportieren", "Exporter la photo", "Exportar foto")),
        ("基本／色彩／細節 重設", Tr("Reset Basic / Color / Detail", "基本／カラー／ディテールをリセット", "기본 / 색상 / 디테일 초기화", "基本／色彩／细节 重置", "Basis / Farbe / Details zurücksetzen", "Réinit. base / couleur / détails", "Restablecer básico / color / detalle")),
        ("適合", Tr("Fit", "フィット", "맞춤", "适合", "Einpassen", "Ajuster", "Ajustar")),
        ("對照原圖", Tr("Original", "元画像", "원본", "对照原图", "Original", "Original", "Original")),
        ("全部重設", Tr("Reset All", "すべてリセット", "모두 초기화", "全部重置", "Alles zurücksetzen", "Tout réinitialiser", "Restablecer todo")),
        ("恢復上一步", Tr("Undo", "元に戻す", "실행 취소", "撤销", "Rückgängig", "Annuler", "Deshacer")),
        ("基本調整", Tr("Basic", "基本補正", "기본 보정", "基本调整", "Grundeinstellungen", "Réglages de base", "Ajustes básicos")),
        ("曝光", Tr("Exposure", "露出", "노출", "曝光", "Belichtung", "Exposition", "Exposición")),
        ("對比", Tr("Contrast", "コントラスト", "대비", "对比度", "Kontrast", "Contraste", "Contraste")),
        ("亮部", Tr("Highlights", "ハイライト", "하이라이트", "高光", "Lichter", "Hautes lumières", "Iluminaciones")),
        ("暗部", Tr("Shadows", "シャドウ", "섀도", "阴影", "Tiefen", "Ombres", "Sombras")),
        ("白色", Tr("Whites", "白レベル", "화이트", "白色", "Weiß", "Blancs", "Blancos")),
        ("黑色", Tr("Blacks", "黒レベル", "블랙", "黑色", "Schwarz", "Noirs", "Negros")),
        ("色彩", Tr("Color", "カラー", "색상", "色彩", "Farbe", "Couleur", "Color")),
        ("白平衡選擇器", Tr("White Balance Picker", "ホワイトバランス選択", "화이트 밸런스 선택", "白平衡选择器", "Weißabgleich-Pipette", "Pipette balance des blancs", "Selector de balance de blancos")),
        ("拍攝時設定", Tr("As Shot", "撮影時の設定", "촬영 시 설정", "拍摄时设置", "Wie aufgenommen", "Telle quelle", "Según disparo")),
        ("色溫", Tr("Temperature", "色温度", "색온도", "色温", "Temperatur", "Température", "Temperatura")),
        ("色調", Tr("Tint", "色かぶり補正", "색조", "色调", "Tonung", "Teinte", "Matiz")),
        ("鮮豔度", Tr("Vibrance", "自然な彩度", "생동감", "鲜艳度", "Dynamik", "Vibrance", "Intensidad")),
        ("飽和度", Tr("Saturation", "彩度", "채도", "饱和度", "Sättigung", "Saturation", "Saturación")),
        ("細節", Tr("Detail", "ディテール", "디테일", "细节", "Details", "Détails", "Detalle")),
        ("銳利度", Tr("Sharpening", "シャープ", "선명도", "锐化", "Schärfen", "Netteté", "Enfoque")),
        ("暗角", Tr("Vignette", "周辺光量", "비네팅", "暗角", "Vignette", "Vignettage", "Viñeta")),
        ("降噪", Tr("Noise Reduction", "ノイズ軽減", "노이즈 감소", "降噪", "Rauschreduzierung", "Réduction du bruit", "Reducción de ruido")),
        ("直方圖", Tr("Histogram", "ヒストグラム", "히스토그램", "直方图", "Histogramm", "Histogramme", "Histograma")),
        ("照片資訊", Tr("Photo Info", "写真情報", "사진 정보", "照片信息", "Fotoinfo", "Infos photo", "Info de foto")),
        ("工具", Tr("Tools", "ツール", "도구", "工具", "Werkzeuge", "Outils", "Herramientas")),
        ("裁切", Tr("Crop", "切り抜き", "자르기", "裁剪", "Zuschneiden", "Recadrer", "Recortar")),
        ("漸層", Tr("Gradient", "グラデーション", "그라데이션", "渐变", "Verlauf", "Dégradé", "Degradado")),
        ("修護", Tr("Heal", "修復", "복구", "修复", "Reparieren", "Corriger", "Corregir")),
        ("比例", Tr("Ratio", "比率", "비율", "比例", "Seitenverhältnis", "Ratio", "Proporción")),
        ("原始", Tr("Original", "オリジナル", "원본", "原始", "Original", "Original", "Original")),
        ("自訂", Tr("Custom", "カスタム", "사용자 지정", "自定义", "Benutzerdefiniert", "Personnalisé", "Personalizado")),
        ("角度", Tr("Angle", "角度", "각도", "角度", "Winkel", "Angle", "Ángulo")),
        ("廣角變形", Tr("Lens Distortion", "レンズ歪み", "렌즈 왜곡", "广角变形", "Objektivverzerrung", "Distorsion d'objectif", "Distorsión de lente")),
        ("照片左轉90度", Tr("Rotate Left 90°", "左に90°回転", "왼쪽으로 90° 회전", "照片左转90度", "90° nach links drehen", "Rotation 90° à gauche", "Girar 90° a la izquierda")),
        ("照片右轉90度", Tr("Rotate Right 90°", "右に90°回転", "오른쪽으로 90° 회전", "照片右转90度", "90° nach rechts drehen", "Rotation 90° à droite", "Girar 90° a la derecha")),
        ("裁切重設", Tr("Reset Crop", "切り抜きをリセット", "자르기 초기화", "裁剪重置", "Zuschnitt zurücksetzen", "Réinitialiser le recadrage", "Restablecer recorte")),
        ("新增線性漸層", Tr("Add Linear Gradient", "線形グラデーションを追加", "선형 그라데이션 추가", "新增线性渐变", "Linearen Verlauf hinzufügen", "Ajouter un dégradé linéaire", "Añadir degradado lineal")),
        ("漸層重設（清除全部）", Tr("Reset Gradients (Clear All)", "グラデーションをすべて消去", "그라데이션 모두 지우기", "渐变重置（清除全部）", "Verläufe zurücksetzen (alle löschen)", "Réinit. dégradés (tout effacer)", "Restablecer degradados (borrar todo)")),
        ("仿製", Tr("Clone", "コピー", "복제", "仿制", "Klonen", "Cloner", "Clonar")),
        ("修補", Tr("Inpaint", "修復", "인페인트", "修补", "Ausbessern", "Retoucher", "Retocar")),
        ("大小", Tr("Size", "サイズ", "크기", "大小", "Größe", "Taille", "Tamaño")),
        ("修護重設", Tr("Reset Healing", "修復をリセット", "복구 초기화", "修复重置", "Reparatur zurücksetzen", "Réinitialiser la correction", "Restablecer corrección")),
        ("風格檔種類", Tr("Presets", "プリセット", "프리셋", "预设种类", "Vorgaben", "Préréglages", "Preajustes")),
        ("套用該風格檔", Tr("Apply Preset", "プリセットを適用", "프리셋 적용", "应用该预设", "Vorgabe anwenden", "Appliquer le préréglage", "Aplicar preajuste")),
        ("相機", Tr("Camera", "カメラ", "카메라", "相机", "Kamera", "Appareil", "Cámara")),
        ("鏡頭", Tr("Lens", "レンズ", "렌즈", "镜头", "Objektiv", "Objectif", "Objetivo")),
        ("光圈", Tr("Aperture", "絞り", "조리개", "光圈", "Blende", "Ouverture", "Apertura")),
        ("快門", Tr("Shutter", "シャッター", "셔터", "快门", "Verschluss", "Obturateur", "Obturador")),
        ("焦段", Tr("Focal Length", "焦点距離", "초점 거리", "焦距", "Brennweite", "Focale", "Distancia focal")),
        ("曝光補償", Tr("Exposure Bias", "露出補正", "노출 보정", "曝光补偿", "Belichtungskorr.", "Correction d'expo.", "Compensación exp.")),
        ("白平衡", Tr("White Balance", "ホワイトバランス", "화이트 밸런스", "白平衡", "Weißabgleich", "Balance des blancs", "Balance de blancos")),
        ("測光", Tr("Metering", "測光", "측광", "测光", "Messung", "Mesure", "Medición")),
        ("日期", Tr("Date", "撮影日時", "날짜", "日期", "Datum", "Date", "Fecha")),
        ("尺寸", Tr("Dimensions", "サイズ", "크기", "尺寸", "Abmessungen", "Dimensions", "Dimensiones")),
        ("檔案大小", Tr("File Size", "ファイルサイズ", "파일 크기", "文件大小", "Dateigröße", "Taille du fichier", "Tamaño de archivo")),
        ("選擇相片資料夾", Tr("Select Photo Folder", "写真フォルダーを選択", "사진 폴더 선택", "选择照片文件夹", "Fotoordner auswählen", "Choisir le dossier de photos", "Seleccionar carpeta de fotos")),
        ("選擇儲存位置", Tr("Select Save Location", "保存先を選択", "저장 위치 선택", "选择保存位置", "Speicherort wählen", "Choisir l'emplacement", "Elegir ubicación")),
        ("桌面", Tr("Desktop", "デスクトップ", "바탕 화면", "桌面", "Desktop", "Bureau", "Escritorio")),
        ("無法開啟：", Tr("Could not open: ", "開けません：", "열 수 없음: ", "无法打开：", "Kann nicht geöffnet werden: ", "Impossible d'ouvrir : ", "No se puede abrir: ")),
        ("匯出設定", Tr("Export Settings", "書き出し設定", "내보내기 설정", "导出设置", "Exporteinstellungen", "Paramètres d'exportation", "Ajustes de exportación")),
        ("匯出照片", Tr("Export Photos", "写真を書き出す", "사진 내보내기", "导出照片", "Fotos exportieren", "Exporter les photos", "Exportar fotos")),
        ("匯出相片", Tr("Export Photos", "写真を書き出す", "사진 내보내기", "导出照片", "Fotos exportieren", "Exporter les photos", "Exportar fotos")),
        ("共 {0} 張相片將被轉存", Tr("{0} photos will be exported", "{0}枚の写真を書き出します", "사진 {0}장을 내보냅니다", "共 {0} 张照片将被转存", "{0} Fotos werden exportiert", "{0} photos seront exportées", "Se exportarán {0} fotos")),
        ("儲存位置", Tr("Destination", "保存先", "저장 위치", "保存位置", "Speicherort", "Destination", "Destino")),
        ("同原始照片目錄", Tr("Same as original photo", "元の写真と同じフォルダー", "원본 사진 폴더", "同原始照片目录", "Wie Originalfoto", "Même dossier que l'original", "Igual que el original")),
        ("自己選擇", Tr("Choose a folder", "フォルダーを選択", "폴더 선택", "自行选择", "Ordner wählen", "Choisir un dossier", "Elegir carpeta")),
        ("瀏覽", Tr("Browse", "参照", "찾아보기", "浏览", "Wählen…", "Parcourir", "Examinar")),
        ("次資料夾", Tr("Subfolder", "サブフォルダー", "하위 폴더", "子文件夹", "Unterordner", "Sous-dossier", "Subcarpeta")),
        ("儲存至次資料夾", Tr("Save to subfolder", "サブフォルダーに保存", "하위 폴더에 저장", "保存至子文件夹", "In Unterordner", "Sous-dossier", "En subcarpeta")),
        ("重新命名", Tr("Rename", "名前の変更", "이름 바꾸기", "重命名", "Umbenennen", "Renommer", "Renombrar")),
        ("按照原始檔案", Tr("Use original filename", "元のファイル名", "원본 파일 이름", "按照原始文件", "Originaldateiname", "Nom de fichier d'origine", "Nombre de archivo original")),
        ("日期時間（IMG 年月日時分秒＋序號）", Tr("Date and time (IMG + timestamp + sequence)", "日時（IMG＋年月日時分秒＋連番）", "날짜 및 시간 (IMG + 타임스탬프 + 순번)", "日期时间（IMG 年月日时分秒＋序号）", "Datum und Uhrzeit (IMG + Zeitstempel + Nummer)", "Date et heure (IMG + horodatage + numéro)", "Fecha y hora (IMG + marca de tiempo + número)")),
        ("數字開始（IMG00001）", Tr("Sequence (IMG00001)", "連番（IMG00001）", "순번 (IMG00001)", "数字开始（IMG00001）", "Fortlaufend (IMG00001)", "Séquence (IMG00001)", "Secuencia (IMG00001)")),
        ("存檔遇到相同檔名", Tr("When a file already exists", "同名ファイルがある場合", "같은 이름의 파일이 있을 때", "保存遇到相同文件名", "Wenn die Datei bereits existiert", "Si le fichier existe déjà", "Si el archivo ya existe")),
        ("檔名接續 \"_數字\"，例如 _1, _2...", Tr("Append a number, e.g. _1, _2…", "番号を追加（例：_1、_2…）", "번호 추가 (예: _1, _2…)", "文件名接 \"_数字\"，例如 _1, _2...", "Nummer anhängen, z. B. _1, _2…", "Ajouter un numéro, ex. _1, _2…", "Añadir un número, p. ej. _1, _2…")),
        ("直接覆蓋", Tr("Overwrite", "上書き", "덮어쓰기", "直接覆盖", "Überschreiben", "Écraser", "Sobrescribir")),
        ("格式與尺寸", Tr("Format and Size", "形式とサイズ", "형식 및 크기", "格式与尺寸", "Format und Größe", "Format et taille", "Formato y tamaño")),
        ("符合寬度高度(像素)", Tr("Fit Width/Height (px)", "幅・高さ上限(px)", "가로·세로 최대(px)", "符合宽度高度(像素)", "Max. Breite/Höhe (px)", "Larg./haut. max (px)", "Ancho/alto máx. (px)")),
        ("解析度（像素/英寸）", Tr("Resolution (pixels/inch)", "解像度（ピクセル/インチ）", "해상도 (픽셀/인치)", "分辨率（像素/英寸）", "Auflösung (Pixel/Zoll)", "Résolution (px/pouce)", "Resolución (píx./pulgada)")),
        ("JPEG 品質", Tr("JPEG Quality", "JPEG画質", "JPEG 품질", "JPEG 质量", "JPEG-Qualität", "Qualité JPEG", "Calidad JPEG")),
        ("保存 EXIF（相機 / 鏡頭 / 拍攝資訊）", Tr("Preserve EXIF (camera / lens / capture info)", "EXIFを保持（カメラ／レンズ／撮影情報）", "EXIF 유지 (카메라 / 렌즈 / 촬영 정보)", "保留 EXIF（相机 / 镜头 / 拍摄信息）", "EXIF behalten (Kamera / Objektiv / Aufnahme)", "Conserver l'EXIF (appareil / objectif / capture)", "Conservar EXIF (cámara / objetivo / captura)")),
        ("轉檔完成後開啟檔案總管顯示", Tr("Show exported files in File Explorer", "完了後にエクスプローラーで表示", "완료 후 파일 탐색기에서 보기", "转存完成后打开文件资源管理器显示", "Exportierte Dateien im Explorer anzeigen", "Afficher les fichiers exportés dans l'Explorateur", "Mostrar los archivos exportados en el Explorador")),
        ("浮水印", Tr("Watermark", "透かし", "워터마크", "水印", "Wasserzeichen", "Filigrane", "Marca de agua")),
        ("標誌", Tr("Watermark", "透かし", "워터마크", "水印", "Wasserzeichen", "Filigrane", "Marca de agua")),
        ("儲存風格檔", Tr("Save Preset", "プリセットを保存", "프리셋 저장", "保存预设", "Vorgabe speichern", "Enregistrer le préréglage", "Guardar preajuste")),
        ("啟用浮水印", Tr("Enable watermark", "透かしを有効化", "워터마크 사용", "启用水印", "Aktivieren", "Activer", "Activar")),
        ("文字", Tr("Text", "文字", "텍스트", "文字", "Text", "Texte", "Texto")),
        ("字體", Tr("Font", "フォント", "글꼴", "字体", "Schrift", "Police", "Fuente")),
        ("顏色", Tr("Color", "色", "색상", "颜色", "Farbe", "Couleur", "Color")),
        ("透明度", Tr("Opacity", "透明度", "투명도", "透明度", "Opazität", "Opacité", "Opacidad")),
        ("位置", Tr("Position", "位置", "위치", "位置", "Position", "Position", "Posición")),
        ("邊緣", Tr("Margin", "余白", "여백", "边距", "Rand", "Marge", "Margen")),
        ("左上", Tr("Top Left", "左上", "왼쪽 위", "左上", "Oben links", "En haut à gauche", "Arriba izquierda")),
        ("右上", Tr("Top Right", "右上", "오른쪽 위", "右上", "Oben rechts", "En haut à droite", "Arriba derecha")),
        ("左下", Tr("Bottom Left", "左下", "왼쪽 아래", "左下", "Unten links", "En bas à gauche", "Abajo izquierda")),
        ("右下", Tr("Bottom Right", "右下", "오른쪽 아래", "右下", "Unten rechts", "En bas à droite", "Abajo derecha")),
        ("藍色", Tr("Blue", "青", "파란색", "蓝色", "Blau", "Bleu", "Azul")),
        ("黃色", Tr("Yellow", "黄", "노란색", "黄色", "Gelb", "Jaune", "Amarillo")),
        ("綠色", Tr("Green", "緑", "초록색", "绿色", "Grün", "Vert", "Verde")),
        ("紅色", Tr("Red", "赤", "빨간색", "红色", "Rot", "Rouge", "Rojo")),
        ("灰色", Tr("Gray", "グレー", "회색", "灰色", "Grau", "Gris", "Gris")),
        ("橙色", Tr("Orange", "オレンジ", "주황색", "橙色", "Orange", "Orange", "Naranja")),
        ("儲存設定", Tr("Save Settings", "設定を保存", "설정 저장", "保存设置", "Speichern", "Enregistrer", "Guardar")),
        ("儲存設定並開始轉存", Tr("Save and Export", "保存して書き出す", "저장 후 내보내기", "保存设置并开始转存", "Exportieren", "Exporter", "Guardar y exportar")),
        ("風格檔", Tr("Preset", "プリセット", "프리셋", "预设", "Vorgabe", "Préréglage", "Preajuste")),
        ("預設時設定", Tr("Default", "初期設定", "기본값", "默认设置", "Standard", "Par défaut", "Predeterminado")),
        ("風景", Tr("Landscape", "風景", "풍경", "风景", "Landschaft", "Paysage", "Paisaje")),
        ("人像", Tr("Portrait", "ポートレート", "인물", "人像", "Porträt", "Portrait", "Retrato")),
        ("鮮豔", Tr("Vivid", "ビビッド", "선명하게", "鲜艳", "Lebendig", "Éclatant", "Vívido")),
        ("黑白", Tr("Black & White", "モノクロ", "흑백", "黑白", "Schwarzweiß", "Noir et blanc", "Blanco y negro")),
        ("柔和", Tr("Soft", "ソフト", "부드럽게", "柔和", "Weich", "Doux", "Suave")),
        ("自訂1", Tr("Custom 1", "カスタム1", "사용자 지정 1", "自定义1", "Benutzerdefiniert 1", "Personnalisé 1", "Personalizado 1")),
        ("自訂2", Tr("Custom 2", "カスタム2", "사용자 지정 2", "自定义2", "Benutzerdefiniert 2", "Personnalisé 2", "Personalizado 2")),
        ("自訂3", Tr("Custom 3", "カスタム3", "사용자 지정 3", "自定义3", "Benutzerdefiniert 3", "Personnalisé 3", "Personalizado 3")),
        ("編輯風格檔", Tr("Edit Presets", "プリセットを編集", "프리셋 편집", "编辑预设", "Vorgaben bearbeiten", "Modifier les préréglages", "Editar preajustes")),
        ("編輯風格檔…", Tr("Edit Presets…", "プリセットを編集…", "프리셋 편집…", "编辑预设…", "Vorgaben bearbeiten…", "Modifier les préréglages…", "Editar preajustes…")),
        ("新增自訂風格檔", Tr("New Custom Preset", "新規カスタムプリセット", "새 사용자 프리셋", "新增自定义预设", "Neue eigene Vorgabe", "Nouveau préréglage personnalisé", "Nuevo preajuste personalizado")),
        ("新增", Tr("Add", "追加", "추가", "新增", "Hinzufügen", "Ajouter", "Añadir")),
        ("「新增」以目前顯示的設定建立\n修改會自動儲存", Tr("“Add” uses the settings shown\nChanges are saved automatically", "「追加」は現在の設定を使用します\n変更は自動保存されます", "‘추가’는 현재 설정을 사용합니다\n변경 내용은 자동 저장됩니다", "“新增”以当前显示的设置创建\n修改会自动保存", "„Hinzufügen“ nutzt die angezeigten Werte\nÄnderungen werden automatisch gespeichert", "« Ajouter » utilise les réglages affichés\nModifications enregistrées automatiquement", "“Añadir” usa los ajustes mostrados\nLos cambios se guardan automáticamente")),
        ("恢復預設", Tr("Restore Defaults", "初期設定に戻す", "기본값 복원", "恢复默认", "Standard wiederherstellen", "Restaurer les valeurs par défaut", "Restaurar predeterminados")),
        ("（自訂）", Tr(" (Custom)", "（カスタム）", " (사용자 지정)", "（自定义）", " (eigene)", " (personnalisé)", " (personalizado)")),
        ("請先輸入自訂風格檔名稱", Tr("Enter a name for the custom preset first", "カスタムプリセット名を入力してください", "사용자 프리셋 이름을 먼저 입력하세요", "请先输入自定义预设名称", "Bitte zuerst einen Namen für die Vorgabe eingeben", "Saisissez d'abord un nom de préréglage", "Escriba primero un nombre para el preajuste")),
        ("已有名為「{0}」的風格檔，請換一個名稱", Tr("A preset named “{0}” already exists. Choose another name.", "「{0}」というプリセットは既にあります。別の名前を指定してください。", "‘{0}’ 프리셋이 이미 있습니다. 다른 이름을 사용하세요.", "已有名为“{0}”的预设，请换一个名称", "Eine Vorgabe namens „{0}“ existiert bereits. Bitte anderen Namen wählen.", "Un préréglage nommé « {0} » existe déjà. Choisissez un autre nom.", "Ya existe un preajuste llamado “{0}”. Elija otro nombre.")),
        ("將刪除所有自訂風格檔，並把所有內建風格檔恢復為預設值。\n確定要恢復預設？", Tr("All custom presets will be deleted and built-in presets restored.\nRestore defaults?", "すべてのカスタムプリセットを削除し、内蔵プリセットを初期状態に戻します。\nよろしいですか？", "모든 사용자 프리셋을 삭제하고 기본 프리셋을 초기 상태로 복원합니다.\n계속할까요?", "将删除所有自定义预设，并把所有内置预设恢复为默认值。\n确定要恢复默认？", "Alle eigenen Vorgaben werden gelöscht und die integrierten zurückgesetzt.\nFortfahren?", "Tous les préréglages personnalisés seront supprimés et les intégrés restaurés.\nContinuer ?", "Se eliminarán todos los preajustes personalizados y se restaurarán los integrados.\n¿Continuar?")),
        ("備份全部", Tr("Back Up All", "すべてバックアップ", "전체 백업", "备份全部", "Sichern", "Sauvegarder", "Respaldar")),
        ("還原全部", Tr("Restore All", "すべて復元", "전체 복원", "还原全部", "Wiederherstellen", "Tout restaurer", "Restaurar todo")),
        ("風格檔備份", Tr("Preset Backup", "プリセットのバックアップ", "프리셋 백업", "预设备份", "Vorgaben-Backup", "Sauvegarde des préréglages", "Copia de preajustes")),
        ("已備份全部風格檔至：\n{0}", Tr("All presets backed up to:\n{0}", "すべてのプリセットをバックアップしました：\n{0}", "모든 프리셋을 백업했습니다:\n{0}", "已备份全部预设至：\n{0}", "Alle Vorgaben gesichert nach:\n{0}", "Tous les préréglages sauvegardés vers :\n{0}", "Todos los preajustes guardados en:\n{0}")),
        ("這不是有效的風格檔備份檔", Tr("This is not a valid preset backup file", "有効なプリセットバックアップファイルではありません", "유효한 프리셋 백업 파일이 아닙니다", "这不是有效的预设备份文件", "Keine gültige Vorgaben-Sicherungsdatei", "Fichier de sauvegarde de préréglages non valide", "No es un archivo de copia de preajustes válido")),
        ("還原將以備份內容取代現有的所有風格檔設定。\n確定要還原？", Tr("Restoring will replace all current presets with the backup.\nContinue?", "復元するとバックアップの内容で現在のプリセットがすべて置き換えられます。\n続行しますか？", "복원하면 현재 프리셋이 모두 백업 내용으로 대체됩니다.\n계속할까요?", "还原将以备份内容替换现有的所有预设。\n确定要还原？", "Beim Wiederherstellen werden alle aktuellen Vorgaben durch das Backup ersetzt.\nFortfahren?", "La restauration remplacera tous les préréglages actuels.\nContinuer ?", "Restaurar reemplazará todos los preajustes actuales.\n¿Continuar?")),
        ("已從備份還原風格檔", Tr("Presets restored from backup", "バックアップからプリセットを復元しました", "백업에서 프리셋을 복원했습니다", "已从备份还原预设", "Vorgaben aus Backup wiederhergestellt", "Préréglages restaurés depuis la sauvegarde", "Preajustes restaurados desde la copia")),
        ("備份失敗：", Tr("Backup failed: ", "バックアップ失敗：", "백업 실패: ", "备份失败：", "Sicherung fehlgeschlagen: ", "Échec de la sauvegarde : ", "Error de copia: ")),
        ("還原失敗：", Tr("Restore failed: ", "復元失敗：", "복원 실패: ", "还原失败：", "Wiederherstellung fehlgeschlagen: ", "Échec de la restauration : ", "Error al restaurar: ")),
        ("開啟資料夾…", Tr("Open Folder…", "フォルダーを開く…", "폴더 열기…", "打开文件夹…", "Ordner öffnen…", "Ouvrir un dossier…", "Abrir carpeta…")),
        ("關閉資料夾", Tr("Close Folder", "フォルダーを閉じる", "폴더 닫기", "关闭文件夹", "Ordner schließen", "Fermer le dossier", "Cerrar carpeta")),
        ("關閉資料夾並刪除快取縮圖", Tr("Close Folder and Delete Cache", "フォルダーを閉じてキャッシュを削除", "폴더 닫기 및 캐시 삭제", "关闭文件夹并删除缓存缩略图", "Ordner schließen und Cache löschen", "Fermer le dossier et supprimer le cache", "Cerrar carpeta y borrar caché")),
        ("還原已隱藏的照片", Tr("Restore Hidden Photos", "非表示の写真を復元", "숨긴 사진 복원", "恢复已隐藏的照片", "Ausgeblendete Fotos wiederherstellen", "Restaurer les photos masquées", "Restaurar fotos ocultas")),
        ("還原已隱藏的照片（{0} 張）", Tr("Restore Hidden Photos ({0})", "非表示の写真を復元（{0}枚）", "숨긴 사진 복원 ({0}장)", "恢复已隐藏的照片（{0} 张）", "Ausgeblendete Fotos wiederherstellen ({0})", "Restaurer les photos masquées ({0})", "Restaurar fotos ocultas ({0})")),
        ("重新整理資料夾  (F5)", Tr("Refresh Folder  (F5)", "フォルダーを更新  (F5)", "폴더 새로 고침  (F5)", "刷新文件夹  (F5)", "Ordner aktualisieren  (F5)", "Actualiser le dossier  (F5)", "Actualizar carpeta  (F5)")),
        ("紀錄", Tr("Recent Folders", "最近のフォルダー", "최근 폴더", "打开记录", "Zuletzt verwendet", "Dossiers récents", "Carpetas recientes")),
        ("（尚無開啟紀錄）", Tr("(No recent folders)", "（履歴はありません）", "(최근 폴더 없음)", "（尚无打开记录）", "(Keine Einträge)", "(Aucun dossier récent)", "(No hay carpetas recientes)")),
        ("清除紀錄", Tr("Clear History", "履歴を消去", "기록 지우기", "清除记录", "Verlauf löschen", "Effacer l'historique", "Borrar historial")),
        ("支援RAW檔相機列表", Tr("Supported RAW Cameras", "対応RAWカメラ一覧", "지원 RAW 카메라 목록", "支持RAW文件相机列表", "Unterstützte RAW-Kameras", "Appareils RAW pris en charge", "Cámaras RAW compatibles")),
        ("關於", Tr("About", "このアプリについて", "정보", "关于", "Info", "À propos", "Acerca de")),
        ("結束", Tr("Exit", "終了", "종료", "退出", "Beenden", "Quitter", "Salir")),
        ("全選", Tr("Select All", "すべて選択", "모두 선택", "全选", "Alles auswählen", "Tout sélectionner", "Seleccionar todo")),
        ("反向選擇", Tr("Invert Selection", "選択を反転", "선택 반전", "反向选择", "Auswahl umkehren", "Inverser la sélection", "Invertir selección")),
        ("取消全選", Tr("Deselect All", "選択を解除", "모두 선택 해제", "取消全选", "Auswahl aufheben", "Tout désélectionner", "Deseleccionar todo")),
        ("複製照片設定", Tr("Copy Photo Settings", "写真の設定をコピー", "사진 설정 복사", "复制照片设置", "Fotoeinstellungen kopieren", "Copier les réglages de la photo", "Copiar ajustes de la foto")),
        ("貼上照片設定", Tr("Paste Photo Settings", "写真の設定を貼り付け", "사진 설정 붙여넣기", "粘贴照片设置", "Fotoeinstellungen einfügen", "Coller les réglages de la photo", "Pegar ajustes de la foto")),
        ("建立副本", Tr("Create Virtual Copy", "仮想コピーを作成", "가상 사본 만들기", "创建虚拟副本", "Virtuelle Kopie erstellen", "Créer une copie virtuelle", "Crear copia virtual")),
        ("隱藏且不輸出", Tr("Hide (No Export)", "非表示（書き出さない）", "숨기기(내보내지 않음)", "隐藏且不输出", "Ausblenden (kein Export)", "Masquer (pas d'export)", "Ocultar (sin exportar)")),
        ("升級處理版本", Tr("Upgrade Process Version", "処理バージョンを更新", "처리 버전 업그레이드", "升级处理版本", "Prozessversion aktualisieren", "Mettre à jour la version de traitement", "Actualizar versión de proceso")),
        ("已升級 {0} 張照片的處理版本", Tr("Upgraded the process version of {0} photo(s)", "{0} 枚の処理バージョンを更新しました", "{0}장의 처리 버전을 업그레이드했습니다", "已升级 {0} 张照片的处理版本", "Prozessversion von {0} Foto(s) aktualisiert", "Version de traitement mise à jour pour {0} photo(s)", "Versión de proceso actualizada en {0} foto(s)")),
        ("選取的照片已是最新處理版本", Tr("Selected photos already use the current process version", "選択した写真はすでに最新の処理バージョンです", "선택한 사진은 이미 최신 처리 버전입니다", "选取的照片已是最新处理版本", "Ausgewählte Fotos verwenden bereits die aktuelle Prozessversion", "Les photos sélectionnées utilisent déjà la version actuelle", "Las fotos seleccionadas ya usan la versión actual")),
        ("舊版處理", Tr("Legacy process", "旧処理バージョン", "이전 처리 버전", "旧版处理", "Alte Prozessversion", "Ancienne version", "Versión antigua")),
        ("升級後曝光與白平衡改以線性光計算，畫面可能略有變化。要升級選取的 {0} 張照片嗎？", Tr("After upgrading, exposure and white balance are computed in linear light and the image may change slightly. Upgrade the {0} selected photo(s)?", "更新後は露出とホワイトバランスがリニア光で計算され、見た目が少し変わることがあります。選択した {0} 枚を更新しますか？", "업그레이드 후 노출과 화이트 밸런스가 선형 광으로 계산되어 이미지가 약간 달라질 수 있습니다. 선택한 {0}장을 업그레이드할까요?", "升级后曝光与白平衡改以线性光计算，画面可能略有变化。要升级选取的 {0} 张照片吗？", "Nach der Aktualisierung werden Belichtung und Weißabgleich in linearem Licht berechnet; das Bild kann sich leicht ändern. {0} ausgewählte(s) Foto(s) aktualisieren?", "Après la mise à jour, l'exposition et la balance des blancs sont calculées en lumière linéaire ; l'image peut changer légèrement. Mettre à jour les {0} photo(s) sélectionnée(s) ?", "Tras actualizar, la exposición y el balance de blancos se calculan en luz lineal; la imagen puede cambiar ligeramente. ¿Actualizar las {0} foto(s) seleccionadas?")),
        ("取消隱藏", Tr("Unhide", "非表示を解除", "숨기기 해제", "取消隐藏", "Einblenden", "Ne plus masquer", "Mostrar de nuevo")),
        ("不顯示隱藏", Tr("Don't Show Hidden", "非表示を表示しない", "숨긴 항목 표시 안 함", "不显示隐藏", "Ausgeblendete verbergen", "Ne pas afficher les masquées", "No mostrar ocultas")),
        ("顯示全部", Tr("Show All", "すべて表示", "모두 표시", "显示全部", "Alle anzeigen", "Tout afficher", "Mostrar todo")),
        ("刪除檔案", Tr("Delete File", "ファイルを削除", "파일 삭제", "删除文件", "Datei löschen", "Supprimer le fichier", "Eliminar archivo")),
        ("匯出", Tr("Export", "書き出し", "내보내기", "导出", "Exportieren", "Exporter", "Exportar")),
        ("套用風格檔", Tr("Apply Preset", "プリセットを適用", "프리셋 적용", "应用预设", "Vorgabe anwenden", "Appliquer le préréglage", "Aplicar preajuste")),
        ("刪除此線性漸層", Tr("Delete This Linear Gradient", "この線形グラデーションを削除", "이 선형 그라데이션 삭제", "删除此线性渐变", "Diesen linearen Verlauf löschen", "Supprimer ce dégradé linéaire", "Eliminar este degradado lineal")),
        ("選擇一張相片開始編輯", Tr("Select a photo to start editing", "写真を選択して編集を開始", "편집할 사진을 선택하세요", "选择一张照片开始编辑", "Foto auswählen, um zu beginnen", "Sélectionnez une photo pour commencer", "Seleccione una foto para empezar")),
        ("點擊中性灰色區域設定白平衡", Tr("Click a neutral gray area to set white balance", "ニュートラルグレーの部分をクリックしてホワイトバランスを設定", "중성 회색 영역을 클릭하여 화이트 밸런스 설정", "点击中性灰色区域设置白平衡", "Auf neutrales Grau klicken für den Weißabgleich", "Cliquez sur un gris neutre pour la balance des blancs", "Haga clic en un gris neutro para el balance de blancos")),
        ("未使用LibRaw讀取", Tr("LibRaw not in use", "LibRaw未使用", "LibRaw 사용 안 함", "未使用LibRaw读取", "LibRaw nicht verwendet", "LibRaw non utilisé", "LibRaw no usado")),
        ("LibRaw 讀取中", Tr("Decoded with LibRaw", "LibRawで読み込み", "LibRaw로 디코딩", "LibRaw 读取中", "Mit LibRaw dekodiert", "Décodé avec LibRaw", "Decodificado con LibRaw")),
        ("LibRaw 已啟用", Tr("LibRaw enabled", "LibRaw有効", "LibRaw 사용", "LibRaw 已启用", "LibRaw aktiviert", "LibRaw activé", "LibRaw activado")),
        ("算圖失敗：", Tr("Render failed: ", "レンダリング失敗：", "렌더링 실패: ", "渲染失败：", "Rendern fehlgeschlagen: ", "Échec du rendu : ", "Error de renderizado: ")),
        ("無法讀取資料夾：", Tr("Could not read folder: ", "フォルダーを読み込めません：", "폴더를 읽을 수 없음: ", "无法读取文件夹：", "Ordner kann nicht gelesen werden: ", "Impossible de lire le dossier : ", "No se puede leer la carpeta: ")),
        ("產生快取（縮圖＋預覽）", Tr("Building Cache (Thumbnails + Previews)", "キャッシュを作成（サムネイル＋プレビュー）", "캐시 생성 (썸네일 + 미리보기)", "生成缓存（缩略图＋预览）", "Cache erstellen (Miniaturen + Vorschau)", "Création du cache (vignettes + aperçus)", "Creando caché (miniaturas + vistas previas)")),
        ("第一次產生快取與縮圖檔案需要一些時間\n請稍等...", Tr("The first cache and thumbnail build may take a while.\nPlease wait…", "初回のキャッシュとサムネイル作成には時間がかかります。\nしばらくお待ちください…", "첫 캐시와 썸네일 생성에는 시간이 걸릴 수 있습니다.\n잠시 기다려 주세요…", "首次生成缓存与缩略图文件需要一些时间\n请稍候...", "Der erste Cache-Aufbau kann etwas dauern.\nBitte warten…", "La première création du cache peut prendre du temps.\nVeuillez patienter…", "La primera creación de la caché puede tardar.\nEspere, por favor…")),
        ("完成，可以開始編輯", Tr("Done — ready to edit", "完了しました。編集を開始できます", "완료 — 편집할 수 있습니다", "完成，可以开始编辑", "Fertig — bereit zum Bearbeiten", "Terminé — prêt à modifier", "Listo para editar")),
        ("載入中… ", Tr("Loading… ", "読み込み中… ", "불러오는 중… ", "加载中… ", "Wird geladen… ", "Chargement… ", "Cargando… ")),
        ("載入失敗：", Tr("Load failed: ", "読み込み失敗：", "불러오기 실패: ", "加载失败：", "Laden fehlgeschlagen: ", "Échec du chargement : ", "Error al cargar: ")),
        ("儲存調整失敗：", Tr("Could not save adjustments: ", "調整を保存できません：", "보정 내용을 저장할 수 없음: ", "保存调整失败：", "Anpassungen nicht gespeichert: ", "Impossible d'enregistrer les réglages : ", "No se pueden guardar los ajustes: ")),
        ("此相片沒有可用的拍攝白平衡資訊", Tr("This photo has no usable as-shot white balance data", "この写真には撮影時のホワイトバランス情報がありません", "이 사진에는 사용 가능한 촬영 시 화이트 밸런스 정보가 없습니다", "此照片没有可用的拍摄白平衡信息", "Kein Weißabgleich der Aufnahme verfügbar", "Aucune balance des blancs d'origine disponible", "No hay balance de blancos de captura disponible")),
        ("已複製相片設定", Tr("Photo settings copied", "写真の設定をコピーしました", "사진 설정을 복사했습니다", "已复制照片设置", "Fotoeinstellungen kopiert", "Réglages copiés", "Ajustes copiados")),
        ("尚未複製任何設定", Tr("No settings have been copied", "設定はまだコピーされていません", "복사된 설정이 없습니다", "尚未复制任何设置", "Keine Einstellungen kopiert", "Aucun réglage copié", "No se ha copiado ningún ajuste")),
        ("已貼上相片設定", Tr("Photo settings pasted", "写真の設定を貼り付けました", "사진 설정을 붙여넣었습니다", "已粘贴照片设置", "Fotoeinstellungen eingefügt", "Réglages collés", "Ajustes pegados")),
        ("已建立虛擬副本", Tr("Virtual copy created", "仮想コピーを作成しました", "가상 사본을 만들었습니다", "已创建虚拟副本", "Virtuelle Kopie erstellt", "Copie virtuelle créée", "Copia virtual creada")),
        ("已隱藏（不輸出）", Tr("Hidden (won't export)", "非表示にしました（書き出し対象外）", "숨김(내보내기 제외)", "已隐藏（不输出）", "Ausgeblendet (kein Export)", "Masquée (pas d'export)", "Oculta (sin exportar)")),
        ("已取消隱藏", Tr("Unhidden", "非表示を解除しました", "숨기기 해제됨", "已取消隐藏", "Wieder eingeblendet", "Photo réaffichée", "Visible de nuevo")),
        ("沒有已隱藏的照片", Tr("No hidden photos", "非表示の写真はありません", "숨긴 사진이 없습니다", "没有已隐藏的照片", "Keine ausgeblendeten Fotos", "Aucune photo masquée", "No hay fotos ocultas")),
        ("已還原 {0} 張隱藏的照片", Tr("Restored {0} hidden photos", "非表示の写真を{0}枚復元しました", "숨긴 사진 {0}장을 복원했습니다", "已恢复 {0} 张隐藏的照片", "{0} ausgeblendete Fotos wiederhergestellt", "{0} photos masquées restaurées", "{0} fotos ocultas restauradas")),
        ("已刪除檔案", Tr("File deleted", "ファイルを削除しました", "파일을 삭제했습니다", "已删除文件", "Datei gelöscht", "Fichier supprimé", "Archivo eliminado")),
        ("刪除失敗：", Tr("Delete failed: ", "削除失敗：", "삭제 실패: ", "删除失败：", "Löschen fehlgeschlagen: ", "Échec de la suppression : ", "Error al eliminar: ")),
        ("已還原照片", Tr("Photo restored", "写真を元に戻しました", "사진을 복원했습니다", "已还原照片", "Foto wiederhergestellt", "Photo restaurée", "Foto restaurada")),
        ("無法還原照片：", Tr("Could not restore photo: ", "写真を元に戻せません：", "사진을 복원할 수 없습니다: ", "无法还原照片：", "Foto konnte nicht wiederhergestellt werden: ", "Impossible de restaurer la photo : ", "No se pudo restaurar la foto: ")),
        ("已套用風格檔：{0}", Tr("Preset applied: {0}", "プリセットを適用：{0}", "프리셋 적용: {0}", "已应用预设：{0}", "Vorgabe angewendet: {0}", "Préréglage appliqué : {0}", "Preajuste aplicado: {0}")),
        ("已重新整理資料夾", Tr("Folder refreshed", "フォルダーを更新しました", "폴더를 새로 고쳤습니다", "已刷新文件夹", "Ordner aktualisiert", "Dossier actualisé", "Carpeta actualizada")),
        ("目前沒有可匯出的照片。", Tr("There is no current photo to export.", "書き出す写真がありません。", "내보낼 현재 사진이 없습니다.", "当前没有可导出的照片。", "Kein aktuelles Foto zum Exportieren.", "Aucune photo actuelle à exporter.", "No hay foto actual para exportar.")),
        ("沒有可匯出的相片。", Tr("There are no photos to export.", "書き出す写真がありません。", "내보낼 사진이 없습니다.", "没有可导出的照片。", "Keine Fotos zum Exportieren.", "Aucune photo à exporter.", "No hay fotos para exportar.")),
        ("匯出失敗", Tr("Export Failed", "書き出し失敗", "내보내기 실패", "导出失败", "Export fehlgeschlagen", "Échec de l'exportation", "Error de exportación")),
        ("匯出已取消", Tr("Export canceled", "書き出しをキャンセルしました", "내보내기 취소됨", "导出已取消", "Export abgebrochen", "Exportation annulée", "Exportación cancelada")),
        ("已匯出 {0} 張相片", Tr("Exported {0} photos", "{0}枚の写真を書き出しました", "사진 {0}장을 내보냈습니다", "已导出 {0} 张照片", "{0} Fotos exportiert", "{0} photos exportées", "{0} fotos exportadas")),
        ("匯出完成", Tr("Export complete", "書き出し完了", "내보내기 완료", "导出完成", "Export abgeschlossen", "Exportation terminée", "Exportación completada")),
        ("刪除照片檔案", Tr("Delete Photo File", "写真ファイルを削除", "사진 파일 삭제", "删除照片文件", "Fotodatei löschen", "Supprimer le fichier photo", "Eliminar archivo de foto")),
        ("確定刪除檔案？（會移到資源回收桶）\n{0}", Tr("Delete this file? It will be moved to the Recycle Bin.\n{0}", "このファイルを削除しますか？ごみ箱に移動します。\n{0}", "이 파일을 삭제할까요? 휴지통으로 이동합니다.\n{0}", "确定删除文件？（会移到回收站）\n{0}", "Datei löschen? Sie wird in den Papierkorb verschoben.\n{0}", "Supprimer ce fichier ? Il sera placé dans la corbeille.\n{0}", "¿Eliminar este archivo? Se moverá a la papelera.\n{0}")),
        ("關閉資料夾並刪除此資料夾的快取與縮圖檔案？\n（編輯設定會保留，下次開啟會重新產生快取）", Tr("Close this folder and delete its cache and thumbnails?\n(Edit settings are kept; cache will be rebuilt next time.)", "このフォルダーを閉じ、キャッシュとサムネイルを削除しますか？\n（編集設定は保持され、次回再作成されます）", "이 폴더를 닫고 캐시와 썸네일을 삭제할까요?\n(편집 설정은 유지되며 다음에 다시 생성됩니다.)", "关闭文件夹并删除此文件夹的缓存与缩略图文件？\n（编辑设置会保留，下次打开会重新生成缓存）", "Ordner schließen und Cache/Miniaturen löschen?\n(Bearbeitungen bleiben erhalten; der Cache wird neu erstellt.)", "Fermer le dossier et supprimer son cache et ses vignettes ?\n(Les réglages sont conservés ; le cache sera recréé.)", "¿Cerrar la carpeta y borrar su caché y miniaturas?\n(Los ajustes se conservan; la caché se regenerará.)")),
        ("刪除快取縮圖", Tr("Delete Cache", "キャッシュを削除", "캐시 삭제", "删除缓存缩略图", "Cache löschen", "Supprimer le cache", "Borrar caché")),
        ("已關閉資料夾並刪除快取縮圖（{0} 個檔案）", Tr("Folder closed and {0} cache files deleted", "フォルダーを閉じ、キャッシュファイルを{0}個削除しました", "폴더를 닫고 캐시 파일 {0}개를 삭제했습니다", "已关闭文件夹并删除缓存缩略图（{0} 个文件）", "Ordner geschlossen, {0} Cache-Dateien gelöscht", "Dossier fermé, {0} fichiers de cache supprimés", "Carpeta cerrada, {0} archivos de caché eliminados")),
        ("資料夾已不存在：\n{0}", Tr("Folder no longer exists:\n{0}", "フォルダーが見つかりません：\n{0}", "폴더가 더 이상 존재하지 않습니다:\n{0}", "文件夹已不存在：\n{0}", "Ordner existiert nicht mehr:\n{0}", "Le dossier n'existe plus :\n{0}", "La carpeta ya no existe:\n{0}")),
        ("準備中…", Tr("Preparing…", "準備中…", "준비 중…", "准备中…", "Wird vorbereitet…", "Préparation…", "Preparando…")),
        ("取消中…", Tr("Canceling…", "キャンセル中…", "취소 중…", "取消中…", "Abbruch…", "Annulation…", "Cancelando…")),
        ("可以開始編輯", Tr("Ready to edit", "編集できます", "편집할 수 있습니다", "可以开始编辑", "Bereit zum Bearbeiten", "Prêt à modifier", "Listo para editar")),
        ("作者:", Tr("Author:", "作者：", "제작자:", "作者:", "Autor:", "Auteur :", "Autor:")),
        ("版本：", Tr("Version: ", "バージョン：", "버전: ", "版本：", "Version: ", "Version : ", "Versión: ")),
        ("編譯時間：", Tr("Build time: ", "ビルド日時：", "빌드 시간: ", "编译时间：", "Build-Zeit: ", "Compilé le : ", "Compilado el: ")),
        ("第三方元件:", Tr("Third-party components:", "サードパーティコンポーネント：", "서드파티 구성 요소:", "第三方组件:", "Drittanbieter-Komponenten:", "Composants tiers :", "Componentes de terceros:")),
        ("無法解碼影像", Tr("Could not decode image", "画像をデコードできません", "이미지를 디코딩할 수 없습니다", "无法解码图像", "Bild kann nicht dekodiert werden", "Impossible de décoder l'image", "No se puede decodificar la imagen")),
        ("無法讀取影像：{0}", Tr("Could not read image: {0}", "画像を読み込めません：{0}", "이미지를 읽을 수 없음: {0}", "无法读取图像：{0}", "Bild kann nicht gelesen werden: {0}", "Impossible de lire l'image : {0}", "No se puede leer la imagen: {0}")),
        ("匯出「{0}」失敗：{1}", Tr("Failed to export “{0}”: {1}", "「{0}」の書き出しに失敗：{1}", "‘{0}’ 내보내기 실패: {1}", "导出“{0}”失败：{1}", "Export von „{0}“ fehlgeschlagen: {1}", "Échec de l'exportation de « {0} » : {1}", "Error al exportar “{0}”: {1}")),
    ]
}
