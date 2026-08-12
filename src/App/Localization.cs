using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Windows.Forms;
using AwayPhotoRawEditor.Controls;

namespace AwayPhotoRawEditor.App;

/// <summary>
/// Lightweight application localization. Traditional Chinese source text is used
/// as the stable key so existing controls can be translated recursively without
/// changing preset/storage identifiers.
/// </summary>
public static class L
{
    private readonly record struct Tr(string En, string Ja, string Ko, string Hans, string De, string Fr, string Es);

    public static AppLanguage CurrentLanguage { get; private set; } = AppLanguage.TraditionalChinese;

    public static CultureInfo CurrentCulture => CurrentLanguage switch
    {
        AppLanguage.English => CultureInfo.GetCultureInfo("en-US"),
        AppLanguage.Japanese => CultureInfo.GetCultureInfo("ja-JP"),
        AppLanguage.Korean => CultureInfo.GetCultureInfo("ko-KR"),
        AppLanguage.SimplifiedChinese => CultureInfo.GetCultureInfo("zh-CN"),
        AppLanguage.German => CultureInfo.GetCultureInfo("de-DE"),
        AppLanguage.French => CultureInfo.GetCultureInfo("fr-FR"),
        AppLanguage.Spanish => CultureInfo.GetCultureInfo("es-ES"),
        _ => CultureInfo.GetCultureInfo("zh-TW")
    };

    public static void SetLanguage(AppLanguage language)
    {
        if (!Enum.IsDefined(language)) language = AppLanguage.TraditionalChinese;
        CurrentLanguage = language;
        CultureInfo.CurrentUICulture = CurrentCulture;
    }

    public static string T(string source)
    {
        if (CurrentLanguage == AppLanguage.TraditionalChinese || !_text.TryGetValue(source, out var tr))
            return source;
        return CurrentLanguage switch
        {
            AppLanguage.English => tr.En,
            AppLanguage.Japanese => tr.Ja,
            AppLanguage.Korean => tr.Ko,
            AppLanguage.SimplifiedChinese => tr.Hans,
            AppLanguage.German => tr.De,
            AppLanguage.French => tr.Fr,
            AppLanguage.Spanish => tr.Es,
            _ => source
        };
    }

    public static string F(string source, params object?[] args) =>
        string.Format(CurrentCulture, T(source), args);

    /// <summary>Context-specific text for labels whose Chinese source word is ambiguous.</summary>
    public static string Pick(string zh, string en, string ja, string ko, string hans, string de, string fr, string es) => CurrentLanguage switch
    {
        AppLanguage.English => en,
        AppLanguage.Japanese => ja,
        AppLanguage.Korean => ko,
        AppLanguage.SimplifiedChinese => hans,
        AppLanguage.German => de,
        AppLanguage.French => fr,
        AppLanguage.Spanish => es,
        _ => zh
    };

    public static string LanguageDisplayName(AppLanguage language) => language switch
    {
        AppLanguage.TraditionalChinese => "繁體中文（台灣）",
        AppLanguage.English => "English (United States)",
        AppLanguage.Japanese => "日本語（日本）",
        AppLanguage.Korean => "한국어（대한민국）",
        AppLanguage.SimplifiedChinese => "简体中文（中国）",
        AppLanguage.German => "Deutsch (Deutschland)",
        AppLanguage.French => "Français (France)",
        AppLanguage.Spanish => "Español (España)",
        _ => "繁體中文（台灣）"
    };

    /// <summary>Translate standard and custom-drawn control captions recursively.</summary>
    public static void Apply(Control root)
    {
        if (!string.IsNullOrEmpty(root.Text)) root.Text = T(root.Text);
        if (root is SectionPanel section) section.Title = T(section.Title);
        if (root is AdjustmentSlider slider) slider.Label = T(slider.Label);
        if (root is TopTab tab) tab.Tabs = tab.Tabs.Select(T).ToArray();
        foreach (Control child in root.Controls) Apply(child);
        root.Invalidate();
    }

    private static readonly Dictionary<string, Tr> _text = new(StringComparer.Ordinal)
    {
        // Common / settings
        ["設定"] = new("Settings", "設定", "설정", "设置", "Einstellungen", "Paramètres", "Configuración"),
        ["設定…"] = new("Settings…", "設定…", "설정…", "设置…", "Einstellungen…", "Paramètres…", "Configuración…"),
        ["套用"] = new("Apply", "適用", "적용", "应用", "Anwenden", "Appliquer", "Aplicar"),
        ["確定"] = new("OK", "OK", "확인", "确定", "OK", "OK", "Aceptar"),
        ["取消"] = new("Cancel", "キャンセル", "취소", "取消", "Abbrechen", "Annuler", "Cancelar"),
        ["關閉"] = new("Close", "閉じる", "닫기", "关闭", "Schließen", "Fermer", "Cerrar"),
        ["一般選項"] = new("General", "一般", "일반", "常规", "Allgemein", "Général", "General"),
        ["介面風格"] = new("Interface style", "インターフェーススタイル", "인터페이스 스타일", "界面风格", "Oberflächenstil", "Style d'interface", "Estilo de interfaz"),
        ["語言"] = new("Language", "言語", "언어", "语言", "Sprache", "Langue", "Idioma"),
        ["介面大小"] = new("UI size", "UI サイズ", "UI 크기", "界面大小", "UI-Größe", "Taille de l'UI", "Tamaño de UI"),
        // 帶冒號當獨立 key，避免與「常用位置」的「下載」（下載資料夾）混淆
        ["下載:"] = new("Download:", "ダウンロード:", "다운로드:", "下载:", "Download:", "Téléchargement :", "Descarga:"),
        ["授權："] = new("License: ", "ライセンス: ", "라이선스: ", "许可: ", "Lizenz: ", "Licence : ", "Licencia: "),
        ["歡迎自由修改成你自己的版本，只希望你能在你的「關於」視窗中提及來源是這裡（AwayPhotoRawEditor / Awaysu）。"] = new(
            "You are welcome to modify this into your own version — I only ask that you credit the original source (AwayPhotoRawEditor / Awaysu) in your About dialog.",
            "自由に改変してご自身のバージョンを作って構いません。ただし「バージョン情報」に出典（AwayPhotoRawEditor / Awaysu）を記載してください。",
            "자유롭게 수정해 자신의 버전을 만들어도 됩니다. 다만 정보 창에 출처(AwayPhotoRawEditor / Awaysu)를 밝혀 주세요.",
            "欢迎自由修改成你自己的版本，只希望你能在你的“关于”窗口中提及来源是这里（AwayPhotoRawEditor / Awaysu）。",
            "Sie dürfen dies frei zu einer eigenen Version ändern — ich bitte nur darum, die Quelle (AwayPhotoRawEditor / Awaysu) in Ihrem Info-Dialog zu nennen.",
            "Vous pouvez librement en faire votre propre version — je demande seulement de créditer la source (AwayPhotoRawEditor / Awaysu) dans votre fenêtre À propos.",
            "Puedes modificarlo libremente para crear tu propia versión — solo te pido que menciones la fuente (AwayPhotoRawEditor / Awaysu) en tu ventana Acerca de."),
        ["自動（依螢幕大小）"] = new(
            "Automatic (fit screen)",
            "自動（画面に合わせる）",
            "자동 (화면에 맞춤)",
            "自动（适应屏幕）",
            "Automatisch (Bildschirm)",
            "Automatique (écran)",
            "Automático (pantalla)"),
        ["變更介面大小後將自動重新啟動程式"] = new(
            "Restarts after a UI size change",
            "UI サイズ変更後に自動で再起動します",
            "UI 크기 변경 후 앱 재시작",
            "更改界面大小后将自动重新启动程序",
            "Neustart nach Größenwechsel",
            "Redémarre après changement de taille",
            "Se reinicia al cambiar el tamaño"),
        ["變更語言後將自動重新啟動程式"] = new(
            "Restarts after a language change",
            "言語変更後に自動で再起動します",
            "언어 변경 후 앱 재시작",
            "更改语言后将自动重新启动程序",
            "Neustart nach Sprachwechsel",
            "Redémarre après changement de langue",
            "Se reinicia al cambiar el idioma"),
        ["點選預覽即可切換，套用後立即生效"] = new(
            "Choose a preview; changes apply immediately",
            "プレビューを選択すると、適用後すぐに反映されます",
            "미리보기를 선택하면 적용 즉시 반영됩니다",
            "点击预览即可切换，应用后立即生效",
            "Vorschau anklicken – gilt sofort",
            "Cliquez sur un aperçu – effet immédiat",
            "Vista previa – efecto inmediato"),
        ["經典深色"] = new("Classic Dark", "クラシックダーク", "클래식 다크", "经典深色", "Klassisch Dunkel", "Sombre classique", "Oscuro clásico"),
        ["暖白相紙"] = new("Warm Paper", "ウォームペーパー", "웜 페이퍼", "暖白相纸", "Warmes Papier", "Papier chaud", "Papel cálido"),
        ["低亮度專業工作區\n藍色重點操作"] = new(
            "Low-light workspace\nBlue accents",
            "暗い作業環境\nブルーアクセント",
            "어두운 작업 공간\n블루 포인트",
            "低亮度专业工作区\n蓝色重点操作",
            "Dunkler Arbeitsbereich\nBlaue Akzente",
            "Espace sombre\nAccents bleus",
            "Espacio oscuro\nAcentos azules"),
        ["明亮暖灰工作區\n陶土橘重點操作"] = new(
            "Bright warm-gray\nTerracotta accents",
            "明るい暖色グレー\nテラコッタ",
            "밝은 웜 그레이\n테라코타 포인트",
            "明亮暖灰工作区\n陶土橘重点操作",
            "Helles Warmgrau\nTerrakotta-Akzente",
            "Gris chaud lumineux\nAccents terracotta",
            "Gris cálido claro\nAcentos terracota"),
        ["使用 LibRaw"] = new("Use LibRaw", "LibRawを使用", "LibRaw 사용", "使用 LibRaw", "LibRaw verwenden", "Utiliser LibRaw", "Usar LibRaw"),
        ["高精度 RAW 處理流程 (16-bit / float)"] = new(
            "High-precision RAW pipeline (16-bit / float)",
            "高精度RAW処理（16-bit / float）",
            "고정밀 RAW 처리 (16-bit / float)",
            "高精度 RAW 处理流程 (16-bit / float)",
            "Hochpräzise RAW-Pipeline (16-bit / float)",
            "Pipeline RAW haute précision (16 bits / float)",
            "Proceso RAW de alta precisión (16 bits / float)"),
        ["在縮圖左上顯示編號 (#1, #2 …)"] = new(
            "Show numbers on thumbnails (#1, #2 …)",
            "サムネイル左上に番号を表示（#1、#2 …）",
            "썸네일 왼쪽 위에 번호 표시 (#1, #2 …)",
            "在缩略图左上显示编号 (#1, #2 …)",
            "Nummern auf Miniaturen anzeigen (#1, #2 …)",
            "Afficher les numéros sur les vignettes (#1, #2 …)",
            "Mostrar números en las miniaturas (#1, #2 …)"),
        ["顯示捲軸（視窗過矮時左右欄可捲動）"] = new(
            "Show scrollbars when side panels do not fit",
            "ウィンドウが低いときにサイドパネルのスクロールバーを表示",
            "창 높이가 부족할 때 사이드 패널 스크롤바 표시",
            "显示滚动条（窗口过矮时左右栏可滚动）",
            "Scrollleisten anzeigen, wenn die Seitenleisten nicht passen",
            "Afficher les barres de défilement si nécessaire",
            "Mostrar barras de desplazamiento si es necesario"),
        ["libraw.dll 已載入"] = new("libraw.dll loaded", "libraw.dll 読み込み済み", "libraw.dll 로드됨", "libraw.dll 已加载", "libraw.dll geladen", "libraw.dll chargé", "libraw.dll cargado"),
        ["libraw.dll 未找到（將退回 WIC / 嵌入預覽）"] = new(
            "libraw.dll not found (using WIC / embedded preview)",
            "libraw.dllが見つかりません（WIC／埋め込みプレビューを使用）",
            "libraw.dll을 찾을 수 없음 (WIC / 내장 미리보기 사용)",
            "libraw.dll 未找到（将回退 WIC / 内嵌预览）",
            "libraw.dll nicht gefunden (WIC / eingebettete Vorschau)",
            "libraw.dll introuvable (WIC / aperçu intégré)",
            "libraw.dll no encontrado (WIC / vista previa integrada)"),
        ["exiftool.exe 已載入"] = new("exiftool.exe loaded", "exiftool.exe 読み込み済み", "exiftool.exe 로드됨", "exiftool.exe 已加载", "exiftool.exe geladen", "exiftool.exe chargé", "exiftool.exe cargado"),
        ["exiftool.exe 未找到（將退回 WIC metadata）"] = new(
            "exiftool.exe not found (using WIC metadata)",
            "exiftool.exeが見つかりません（WICメタデータを使用）",
            "exiftool.exe를 찾을 수 없음 (WIC 메타데이터 사용)",
            "exiftool.exe 未找到（将回退 WIC 元数据）",
            "exiftool.exe nicht gefunden (WIC-Metadaten)",
            "exiftool.exe introuvable (métadonnées WIC)",
            "exiftool.exe no encontrado (metadatos WIC)"),

        // Main editor and panels
        ["📁  開啟資料夾"] = new("📁  Open Folder", "📁  フォルダーを開く", "📁  폴더 열기", "📁  打开文件夹", "📁  Ordner öffnen", "📁  Ouvrir", "📁  Abrir carpeta"),
        ["尚未選擇資料夾"] = new("No folder selected", "フォルダーが選択されていません", "선택한 폴더 없음", "尚未选择文件夹", "Kein Ordner ausgewählt", "Aucun dossier sélectionné", "Ninguna carpeta seleccionada"),
        ["匯出全部照片"] = new("Export All Photos", "すべての写真を書き出す", "모든 사진 내보내기", "导出全部照片", "Alle Fotos exportieren", "Tout exporter", "Exportar todas las fotos"),
        ["匯出目前照片"] = new("Export Current Photo", "現在の写真を書き出す", "현재 사진 내보내기", "导出当前照片", "Foto exportieren", "Exporter la photo", "Exportar foto"),
        ["基本／色彩／細節 重設"] = new("Reset Basic / Color / Detail", "基本／カラー／ディテールをリセット", "기본 / 색상 / 디테일 초기화", "基本／色彩／细节 重置", "Basis / Farbe / Details zurücksetzen", "Réinit. base / couleur / détails", "Restablecer básico / color / detalle"),
        ["適合"] = new("Fit", "フィット", "맞춤", "适合", "Einpassen", "Ajuster", "Ajustar"),
        ["對照原圖"] = new("Original", "元画像", "원본", "对照原图", "Original", "Original", "Original"),
        ["全部重設"] = new("Reset All", "すべてリセット", "모두 초기화", "全部重置", "Alles zurücksetzen", "Tout réinitialiser", "Restablecer todo"),
        ["恢復上一步"] = new("Undo", "元に戻す", "실행 취소", "撤销", "Rückgängig", "Annuler", "Deshacer"),
        ["基本調整"] = new("Basic", "基本補正", "기본 보정", "基本调整", "Grundeinstellungen", "Réglages de base", "Ajustes básicos"),
        ["曝光"] = new("Exposure", "露出", "노출", "曝光", "Belichtung", "Exposition", "Exposición"),
        ["對比"] = new("Contrast", "コントラスト", "대비", "对比度", "Kontrast", "Contraste", "Contraste"),
        ["亮部"] = new("Highlights", "ハイライト", "하이라이트", "高光", "Lichter", "Hautes lumières", "Iluminaciones"),
        ["暗部"] = new("Shadows", "シャドウ", "섀도", "阴影", "Tiefen", "Ombres", "Sombras"),
        ["白色"] = new("Whites", "白レベル", "화이트", "白色", "Weiß", "Blancs", "Blancos"),
        ["黑色"] = new("Blacks", "黒レベル", "블랙", "黑色", "Schwarz", "Noirs", "Negros"),
        ["色彩"] = new("Color", "カラー", "색상", "色彩", "Farbe", "Couleur", "Color"),
        ["白平衡選擇器"] = new("White Balance Picker", "ホワイトバランス選択", "화이트 밸런스 선택", "白平衡选择器", "Weißabgleich-Pipette", "Pipette balance des blancs", "Selector de balance de blancos"),
        ["拍攝時設定"] = new("As Shot", "撮影時の設定", "촬영 시 설정", "拍摄时设置", "Wie aufgenommen", "Telle quelle", "Según disparo"),
        ["色溫"] = new("Temperature", "色温度", "색온도", "色温", "Temperatur", "Température", "Temperatura"),
        ["色調"] = new("Tint", "色かぶり補正", "색조", "色调", "Tonung", "Teinte", "Matiz"),
        ["鮮豔度"] = new("Vibrance", "自然な彩度", "생동감", "鲜艳度", "Dynamik", "Vibrance", "Intensidad"),
        ["飽和度"] = new("Saturation", "彩度", "채도", "饱和度", "Sättigung", "Saturation", "Saturación"),
        ["細節"] = new("Detail", "ディテール", "디테일", "细节", "Details", "Détails", "Detalle"),
        ["銳利度"] = new("Sharpening", "シャープ", "선명도", "锐化", "Schärfen", "Netteté", "Enfoque"),
        ["暗角"] = new("Vignette", "周辺光量", "비네팅", "暗角", "Vignette", "Vignettage", "Viñeta"),
        ["降噪"] = new("Noise Reduction", "ノイズ軽減", "노이즈 감소", "降噪", "Rauschreduzierung", "Réduction du bruit", "Reducción de ruido"),
        ["直方圖"] = new("Histogram", "ヒストグラム", "히스토그램", "直方图", "Histogramm", "Histogramme", "Histograma"),
        ["照片資訊"] = new("Photo Info", "写真情報", "사진 정보", "照片信息", "Fotoinfo", "Infos photo", "Info de foto"),
        ["工具"] = new("Tools", "ツール", "도구", "工具", "Werkzeuge", "Outils", "Herramientas"),
        ["裁切"] = new("Crop", "切り抜き", "자르기", "裁剪", "Zuschneiden", "Recadrer", "Recortar"),
        ["漸層"] = new("Gradient", "グラデーション", "그라데이션", "渐变", "Verlauf", "Dégradé", "Degradado"),
        ["修護"] = new("Heal", "修復", "복구", "修复", "Reparieren", "Corriger", "Corregir"),
        ["比例"] = new("Ratio", "比率", "비율", "比例", "Seitenverhältnis", "Ratio", "Proporción"),
        ["原始"] = new("Original", "オリジナル", "원본", "原始", "Original", "Original", "Original"),
        ["自訂"] = new("Custom", "カスタム", "사용자 지정", "自定义", "Benutzerdefiniert", "Personnalisé", "Personalizado"),
        ["角度"] = new("Angle", "角度", "각도", "角度", "Winkel", "Angle", "Ángulo"),
        ["廣角變形"] = new("Lens Distortion", "レンズ歪み", "렌즈 왜곡", "广角变形", "Objektivverzerrung", "Distorsion d'objectif", "Distorsión de lente"),
        ["照片左轉90度"] = new("Rotate Left 90°", "左に90°回転", "왼쪽으로 90° 회전", "照片左转90度", "90° nach links drehen", "Rotation 90° à gauche", "Girar 90° a la izquierda"),
        ["照片右轉90度"] = new("Rotate Right 90°", "右に90°回転", "오른쪽으로 90° 회전", "照片右转90度", "90° nach rechts drehen", "Rotation 90° à droite", "Girar 90° a la derecha"),
        ["裁切重設"] = new("Reset Crop", "切り抜きをリセット", "자르기 초기화", "裁剪重置", "Zuschnitt zurücksetzen", "Réinitialiser le recadrage", "Restablecer recorte"),
        ["新增線性漸層"] = new("Add Linear Gradient", "線形グラデーションを追加", "선형 그라데이션 추가", "新增线性渐变", "Linearen Verlauf hinzufügen", "Ajouter un dégradé linéaire", "Añadir degradado lineal"),
        ["漸層重設（清除全部）"] = new("Reset Gradients (Clear All)", "グラデーションをすべて消去", "그라데이션 모두 지우기", "渐变重置（清除全部）", "Verläufe zurücksetzen (alle löschen)", "Réinit. dégradés (tout effacer)", "Restablecer degradados (borrar todo)"),
        ["仿製"] = new("Clone", "コピー", "복제", "仿制", "Klonen", "Cloner", "Clonar"),
        ["修補"] = new("Inpaint", "修復", "인페인트", "修补", "Ausbessern", "Retoucher", "Retocar"),
        ["大小"] = new("Size", "サイズ", "크기", "大小", "Größe", "Taille", "Tamaño"),
        ["修護重設"] = new("Reset Healing", "修復をリセット", "복구 초기화", "修复重置", "Reparatur zurücksetzen", "Réinitialiser la correction", "Restablecer corrección"),
        ["風格檔種類"] = new("Presets", "プリセット", "프리셋", "预设种类", "Vorgaben", "Préréglages", "Preajustes"),
        ["套用該風格檔"] = new("Apply Preset", "プリセットを適用", "프리셋 적용", "应用该预设", "Vorgabe anwenden", "Appliquer le préréglage", "Aplicar preajuste"),

        // EXIF
        ["相機"] = new("Camera", "カメラ", "카메라", "相机", "Kamera", "Appareil", "Cámara"),
        ["鏡頭"] = new("Lens", "レンズ", "렌즈", "镜头", "Objektiv", "Objectif", "Objetivo"),
        ["光圈"] = new("Aperture", "絞り", "조리개", "光圈", "Blende", "Ouverture", "Apertura"),
        ["快門"] = new("Shutter", "シャッター", "셔터", "快门", "Verschluss", "Obturateur", "Obturador"),
        ["焦段"] = new("Focal Length", "焦点距離", "초점 거리", "焦距", "Brennweite", "Focale", "Distancia focal"),
        ["曝光補償"] = new("Exposure Bias", "露出補正", "노출 보정", "曝光补偿", "Belichtungskorr.", "Correction d'expo.", "Compensación exp."),
        ["白平衡"] = new("White Balance", "ホワイトバランス", "화이트 밸런스", "白平衡", "Weißabgleich", "Balance des blancs", "Balance de blancos"),
        ["測光"] = new("Metering", "測光", "측광", "测光", "Messung", "Mesure", "Medición"),
        ["日期"] = new("Date", "撮影日時", "날짜", "日期", "Datum", "Date", "Fecha"),
        ["尺寸"] = new("Dimensions", "サイズ", "크기", "尺寸", "Abmessungen", "Dimensions", "Dimensiones"),
        ["檔案大小"] = new("File Size", "ファイルサイズ", "파일 크기", "文件大小", "Dateigröße", "Taille du fichier", "Tamaño de archivo"),

        // Folder picker
        ["選擇相片資料夾"] = new("Select Photo Folder", "写真フォルダーを選択", "사진 폴더 선택", "选择照片文件夹", "Fotoordner auswählen", "Choisir le dossier de photos", "Seleccionar carpeta de fotos"),
        ["選擇此資料夾"] = new("Select This Folder", "このフォルダーを選択", "이 폴더 선택", "选择此文件夹", "Ordner auswählen", "Choisir ce dossier", "Elegir esta carpeta"),
        ["常用位置"] = new("Locations", "よく使う場所", "빠른 위치", "常用位置", "Orte", "Emplacements", "Ubicaciones"),
        ["桌面"] = new("Desktop", "デスクトップ", "바탕 화면", "桌面", "Desktop", "Bureau", "Escritorio"),
        ["圖片"] = new("Pictures", "ピクチャ", "사진", "图片", "Bilder", "Images", "Imágenes"),
        ["下載"] = new("Downloads", "ダウンロード", "다운로드", "下载", "Downloads", "Téléchargements", "Descargas"),
        ["文件"] = new("Documents", "ドキュメント", "문서", "文档", "Dokumente", "Documents", "Documentos"),
        ["本機"] = new("This PC", "PC", "내 PC", "此电脑", "Dieser PC", "Ce PC", "Este equipo"),
        ["↑  上一層"] = new("↑  Up", "↑  上へ", "↑  상위 폴더", "↑  上一级", "↑  Nach oben", "↑  Dossier parent", "↑  Subir"),
        ["已選擇："] = new("Selected: ", "選択済み：", "선택됨: ", "已选择：", "Ausgewählt: ", "Sélectionné : ", "Seleccionado: "),
        ["請選擇資料夾"] = new("Select a folder", "フォルダーを選択してください", "폴더를 선택하세요", "请选择文件夹", "Bitte einen Ordner auswählen", "Sélectionnez un dossier", "Seleccione una carpeta"),
        ["無法開啟："] = new("Could not open: ", "開けません：", "열 수 없음: ", "无法打开：", "Kann nicht geöffnet werden: ", "Impossible d'ouvrir : ", "No se puede abrir: "),
        ["（此資料夾沒有子資料夾）"] = new(
            "(This folder has no subfolders)",
            "（このフォルダーにサブフォルダーはありません）",
            "(이 폴더에 하위 폴더가 없습니다)",
            "（此文件夹没有子文件夹）",
            "(Dieser Ordner hat keine Unterordner)",
            "(Ce dossier n'a pas de sous-dossiers)",
            "(Esta carpeta no tiene subcarpetas)"),

        // Export
        ["匯出設定"] = new("Export Settings", "書き出し設定", "내보내기 설정", "导出设置", "Exporteinstellungen", "Paramètres d'exportation", "Ajustes de exportación"),
        ["匯出照片"] = new("Export Photos", "写真を書き出す", "사진 내보내기", "导出照片", "Fotos exportieren", "Exporter les photos", "Exportar fotos"),
        ["匯出相片"] = new("Export Photos", "写真を書き出す", "사진 내보내기", "导出照片", "Fotos exportieren", "Exporter les photos", "Exportar fotos"),
        ["共 {0} 張相片將被轉存"] = new(
            "{0} photos will be exported",
            "{0}枚の写真を書き出します",
            "사진 {0}장을 내보냅니다",
            "共 {0} 张照片将被转存",
            "{0} Fotos werden exportiert",
            "{0} photos seront exportées",
            "Se exportarán {0} fotos"),
        ["儲存位置"] = new("Destination", "保存先", "저장 위치", "保存位置", "Speicherort", "Destination", "Destino"),
        ["同原始照片目錄"] = new("Same as original photo", "元の写真と同じフォルダー", "원본 사진 폴더", "同原始照片目录", "Wie Originalfoto", "Même dossier que l'original", "Igual que el original"),
        ["自己選擇"] = new("Choose a folder", "フォルダーを選択", "폴더 선택", "自行选择", "Ordner wählen", "Choisir un dossier", "Elegir carpeta"),
        ["瀏覽"] = new("Browse", "参照", "찾아보기", "浏览", "Wählen…", "Parcourir", "Examinar"),
        ["次資料夾"] = new("Subfolder", "サブフォルダー", "하위 폴더", "子文件夹", "Unterordner", "Sous-dossier", "Subcarpeta"),
        ["儲存至次資料夾"] = new("Save to subfolder", "サブフォルダーに保存", "하위 폴더에 저장", "保存至子文件夹", "In Unterordner", "Sous-dossier", "En subcarpeta"),
        ["重新命名"] = new("Rename", "名前の変更", "이름 바꾸기", "重命名", "Umbenennen", "Renommer", "Renombrar"),
        ["按照原始檔案"] = new("Use original filename", "元のファイル名", "원본 파일 이름", "按照原始文件", "Originaldateiname", "Nom de fichier d'origine", "Nombre de archivo original"),
        ["日期時間（IMG 年月日時分秒＋序號）"] = new(
            "Date and time (IMG + timestamp + sequence)",
            "日時（IMG＋年月日時分秒＋連番）",
            "날짜 및 시간 (IMG + 타임스탬프 + 순번)",
            "日期时间（IMG 年月日时分秒＋序号）",
            "Datum und Uhrzeit (IMG + Zeitstempel + Nummer)",
            "Date et heure (IMG + horodatage + numéro)",
            "Fecha y hora (IMG + marca de tiempo + número)"),
        ["數字開始（IMG00001）"] = new("Sequence (IMG00001)", "連番（IMG00001）", "순번 (IMG00001)", "数字开始（IMG00001）", "Fortlaufend (IMG00001)", "Séquence (IMG00001)", "Secuencia (IMG00001)"),
        ["存檔遇到相同檔名"] = new("When a file already exists", "同名ファイルがある場合", "같은 이름의 파일이 있을 때", "保存遇到相同文件名", "Wenn die Datei bereits existiert", "Si le fichier existe déjà", "Si el archivo ya existe"),
        ["檔名接續 \"_數字\"，例如 _1, _2..."] = new(
            "Append a number, e.g. _1, _2…",
            "番号を追加（例：_1、_2…）",
            "번호 추가 (예: _1, _2…)",
            "文件名接 \"_数字\"，例如 _1, _2...",
            "Nummer anhängen, z. B. _1, _2…",
            "Ajouter un numéro, ex. _1, _2…",
            "Añadir un número, p. ej. _1, _2…"),
        ["直接覆蓋"] = new("Overwrite", "上書き", "덮어쓰기", "直接覆盖", "Überschreiben", "Écraser", "Sobrescribir"),
        ["格式與尺寸"] = new("Format and Size", "形式とサイズ", "형식 및 크기", "格式与尺寸", "Format und Größe", "Format et taille", "Formato y tamaño"),
        ["符合寬度高度(像素)"] = new("Fit Width/Height (px)", "幅・高さ上限(px)", "가로·세로 최대(px)", "符合宽度高度(像素)", "Max. Breite/Höhe (px)", "Larg./haut. max (px)", "Ancho/alto máx. (px)"),
        ["解析度（像素/英寸）"] = new("Resolution (pixels/inch)", "解像度（ピクセル/インチ）", "해상도 (픽셀/인치)", "分辨率（像素/英寸）", "Auflösung (Pixel/Zoll)", "Résolution (px/pouce)", "Resolución (píx./pulgada)"),
        ["JPEG 品質"] = new("JPEG Quality", "JPEG画質", "JPEG 품질", "JPEG 质量", "JPEG-Qualität", "Qualité JPEG", "Calidad JPEG"),
        ["保存 EXIF（相機 / 鏡頭 / 拍攝資訊）"] = new(
            "Preserve EXIF (camera / lens / capture info)",
            "EXIFを保持（カメラ／レンズ／撮影情報）",
            "EXIF 유지 (카메라 / 렌즈 / 촬영 정보)",
            "保留 EXIF（相机 / 镜头 / 拍摄信息）",
            "EXIF behalten (Kamera / Objektiv / Aufnahme)",
            "Conserver l'EXIF (appareil / objectif / capture)",
            "Conservar EXIF (cámara / objetivo / captura)"),
        ["轉檔完成後開啟檔案總管顯示"] = new(
            "Show exported files in File Explorer",
            "完了後にエクスプローラーで表示",
            "완료 후 파일 탐색기에서 보기",
            "转存完成后打开文件资源管理器显示",
            "Exportierte Dateien im Explorer anzeigen",
            "Afficher les fichiers exportés dans l'Explorateur",
            "Mostrar los archivos exportados en el Explorador"),
        ["浮水印"] = new("Watermark", "透かし", "워터마크", "水印", "Wasserzeichen", "Filigrane", "Marca de agua"),
        ["標誌"] = new("Watermark", "透かし", "워터마크", "水印", "Wasserzeichen", "Filigrane", "Marca de agua"),
        ["儲存風格檔"] = new("Save Preset", "プリセットを保存", "프리셋 저장", "保存预设", "Vorgabe speichern", "Enregistrer le préréglage", "Guardar preajuste"),
        ["啟用浮水印"] = new("Enable watermark", "透かしを有効化", "워터마크 사용", "启用水印", "Aktivieren", "Activer", "Activar"),
        ["文字"] = new("Text", "文字", "텍스트", "文字", "Text", "Texte", "Texto"),
        ["字體"] = new("Font", "フォント", "글꼴", "字体", "Schrift", "Police", "Fuente"),
        ["顏色"] = new("Color", "色", "색상", "颜色", "Farbe", "Couleur", "Color"),
        ["透明度"] = new("Opacity", "透明度", "투명도", "透明度", "Opazität", "Opacité", "Opacidad"),
        ["位置"] = new("Position", "位置", "위치", "位置", "Position", "Position", "Posición"),
        ["邊緣"] = new("Margin", "余白", "여백", "边距", "Rand", "Marge", "Margen"),
        ["左上"] = new("Top Left", "左上", "왼쪽 위", "左上", "Oben links", "En haut à gauche", "Arriba izquierda"),
        ["右上"] = new("Top Right", "右上", "오른쪽 위", "右上", "Oben rechts", "En haut à droite", "Arriba derecha"),
        ["左下"] = new("Bottom Left", "左下", "왼쪽 아래", "左下", "Unten links", "En bas à gauche", "Abajo izquierda"),
        ["右下"] = new("Bottom Right", "右下", "오른쪽 아래", "右下", "Unten rechts", "En bas à droite", "Abajo derecha"),
        ["藍色"] = new("Blue", "青", "파란색", "蓝色", "Blau", "Bleu", "Azul"),
        ["黃色"] = new("Yellow", "黄", "노란색", "黄色", "Gelb", "Jaune", "Amarillo"),
        ["綠色"] = new("Green", "緑", "초록색", "绿色", "Grün", "Vert", "Verde"),
        ["紅色"] = new("Red", "赤", "빨간색", "红色", "Rot", "Rouge", "Rojo"),
        ["灰色"] = new("Gray", "グレー", "회색", "灰色", "Grau", "Gris", "Gris"),
        ["橙色"] = new("Orange", "オレンジ", "주황색", "橙色", "Orange", "Orange", "Naranja"),
        ["儲存設定"] = new("Save Settings", "設定を保存", "설정 저장", "保存设置", "Speichern", "Enregistrer", "Guardar"),
        ["儲存設定並開始轉存"] = new("Save and Export", "保存して書き出す", "저장 후 내보내기", "保存设置并开始转存", "Exportieren", "Exporter", "Guardar y exportar"),

        // Presets
        ["風格檔"] = new("Preset", "プリセット", "프리셋", "预设", "Vorgabe", "Préréglage", "Preajuste"),
        ["預設時設定"] = new("Default", "初期設定", "기본값", "默认设置", "Standard", "Par défaut", "Predeterminado"),
        ["風景"] = new("Landscape", "風景", "풍경", "风景", "Landschaft", "Paysage", "Paisaje"),
        ["人像"] = new("Portrait", "ポートレート", "인물", "人像", "Porträt", "Portrait", "Retrato"),
        ["鮮豔"] = new("Vivid", "ビビッド", "선명하게", "鲜艳", "Lebendig", "Éclatant", "Vívido"),
        ["黑白"] = new("Black & White", "モノクロ", "흑백", "黑白", "Schwarzweiß", "Noir et blanc", "Blanco y negro"),
        ["柔和"] = new("Soft", "ソフト", "부드럽게", "柔和", "Weich", "Doux", "Suave"),
        ["自訂1"] = new("Custom 1", "カスタム1", "사용자 지정 1", "自定义1", "Benutzerdefiniert 1", "Personnalisé 1", "Personalizado 1"),
        ["自訂2"] = new("Custom 2", "カスタム2", "사용자 지정 2", "自定义2", "Benutzerdefiniert 2", "Personnalisé 2", "Personalizado 2"),
        ["自訂3"] = new("Custom 3", "カスタム3", "사용자 지정 3", "自定义3", "Benutzerdefiniert 3", "Personnalisé 3", "Personalizado 3"),
        ["編輯風格檔"] = new("Edit Presets", "プリセットを編集", "프리셋 편집", "编辑预设", "Vorgaben bearbeiten", "Modifier les préréglages", "Editar preajustes"),
        ["編輯風格檔…"] = new("Edit Presets…", "プリセットを編集…", "프리셋 편집…", "编辑预设…", "Vorgaben bearbeiten…", "Modifier les préréglages…", "Editar preajustes…"),
        ["新增自訂風格檔"] = new("New Custom Preset", "新規カスタムプリセット", "새 사용자 프리셋", "新增自定义预设", "Neue eigene Vorgabe", "Nouveau préréglage personnalisé", "Nuevo preajuste personalizado"),
        ["新增"] = new("Add", "追加", "추가", "新增", "Hinzufügen", "Ajouter", "Añadir"),
        ["「新增」以目前顯示的設定建立\n修改會自動儲存"] = new(
            "“Add” uses the settings shown\nChanges are saved automatically",
            "「追加」は現在の設定を使用します\n変更は自動保存されます",
            "‘추가’는 현재 설정을 사용합니다\n변경 내용은 자동 저장됩니다",
            "“新增”以当前显示的设置创建\n修改会自动保存",
            "„Hinzufügen“ nutzt die angezeigten Werte\nÄnderungen werden automatisch gespeichert",
            "« Ajouter » utilise les réglages affichés\nModifications enregistrées automatiquement",
            "“Añadir” usa los ajustes mostrados\nLos cambios se guardan automáticamente"),
        ["恢復預設"] = new("Restore Defaults", "初期設定に戻す", "기본값 복원", "恢复默认", "Standard wiederherstellen", "Restaurer les valeurs par défaut", "Restaurar predeterminados"),
        ["（自訂）"] = new(" (Custom)", "（カスタム）", " (사용자 지정)", "（自定义）", " (eigene)", " (personnalisé)", " (personalizado)"),
        ["請先輸入自訂風格檔名稱"] = new(
            "Enter a name for the custom preset first",
            "カスタムプリセット名を入力してください",
            "사용자 프리셋 이름을 먼저 입력하세요",
            "请先输入自定义预设名称",
            "Bitte zuerst einen Namen für die Vorgabe eingeben",
            "Saisissez d'abord un nom de préréglage",
            "Escriba primero un nombre para el preajuste"),
        ["已有名為「{0}」的風格檔，請換一個名稱"] = new(
            "A preset named “{0}” already exists. Choose another name.",
            "「{0}」というプリセットは既にあります。別の名前を指定してください。",
            "‘{0}’ 프리셋이 이미 있습니다. 다른 이름을 사용하세요.",
            "已有名为“{0}”的预设，请换一个名称",
            "Eine Vorgabe namens „{0}“ existiert bereits. Bitte anderen Namen wählen.",
            "Un préréglage nommé « {0} » existe déjà. Choisissez un autre nom.",
            "Ya existe un preajuste llamado “{0}”. Elija otro nombre."),
        ["將刪除所有自訂風格檔，並把所有內建風格檔恢復為預設值。\n確定要恢復預設？"] = new(
            "All custom presets will be deleted and built-in presets restored.\nRestore defaults?",
            "すべてのカスタムプリセットを削除し、内蔵プリセットを初期状態に戻します。\nよろしいですか？",
            "모든 사용자 프리셋을 삭제하고 기본 프리셋을 초기 상태로 복원합니다.\n계속할까요?",
            "将删除所有自定义预设，并把所有内置预设恢复为默认值。\n确定要恢复默认？",
            "Alle eigenen Vorgaben werden gelöscht und die integrierten zurückgesetzt.\nFortfahren?",
            "Tous les préréglages personnalisés seront supprimés et les intégrés restaurés.\nContinuer ?",
            "Se eliminarán todos los preajustes personalizados y se restaurarán los integrados.\n¿Continuar?"),
        ["備份全部"] = new("Back Up All", "すべてバックアップ", "전체 백업", "备份全部", "Sichern", "Sauvegarder", "Respaldar"),
        ["還原全部"] = new("Restore All", "すべて復元", "전체 복원", "还原全部", "Wiederherstellen", "Tout restaurer", "Restaurar todo"),
        ["風格檔備份"] = new("Preset Backup", "プリセットのバックアップ", "프리셋 백업", "预设备份", "Vorgaben-Backup", "Sauvegarde des préréglages", "Copia de preajustes"),
        ["已備份全部風格檔至：\n{0}"] = new(
            "All presets backed up to:\n{0}",
            "すべてのプリセットをバックアップしました：\n{0}",
            "모든 프리셋을 백업했습니다:\n{0}",
            "已备份全部预设至：\n{0}",
            "Alle Vorgaben gesichert nach:\n{0}",
            "Tous les préréglages sauvegardés vers :\n{0}",
            "Todos los preajustes guardados en:\n{0}"),
        ["這不是有效的風格檔備份檔"] = new(
            "This is not a valid preset backup file",
            "有効なプリセットバックアップファイルではありません",
            "유효한 프리셋 백업 파일이 아닙니다",
            "这不是有效的预设备份文件",
            "Keine gültige Vorgaben-Sicherungsdatei",
            "Fichier de sauvegarde de préréglages non valide",
            "No es un archivo de copia de preajustes válido"),
        ["還原將以備份內容取代現有的所有風格檔設定。\n確定要還原？"] = new(
            "Restoring will replace all current presets with the backup.\nContinue?",
            "復元するとバックアップの内容で現在のプリセットがすべて置き換えられます。\n続行しますか？",
            "복원하면 현재 프리셋이 모두 백업 내용으로 대체됩니다.\n계속할까요?",
            "还原将以备份内容替换现有的所有预设。\n确定要还原？",
            "Beim Wiederherstellen werden alle aktuellen Vorgaben durch das Backup ersetzt.\nFortfahren?",
            "La restauration remplacera tous les préréglages actuels.\nContinuer ?",
            "Restaurar reemplazará todos los preajustes actuales.\n¿Continuar?"),
        ["已從備份還原風格檔"] = new(
            "Presets restored from backup",
            "バックアップからプリセットを復元しました",
            "백업에서 프리셋을 복원했습니다",
            "已从备份还原预设",
            "Vorgaben aus Backup wiederhergestellt",
            "Préréglages restaurés depuis la sauvegarde",
            "Preajustes restaurados desde la copia"),
        ["備份失敗："] = new("Backup failed: ", "バックアップ失敗：", "백업 실패: ", "备份失败：", "Sicherung fehlgeschlagen: ", "Échec de la sauvegarde : ", "Error de copia: "),
        ["還原失敗："] = new("Restore failed: ", "復元失敗：", "복원 실패: ", "还原失败：", "Wiederherstellung fehlgeschlagen: ", "Échec de la restauration : ", "Error al restaurar: "),

        // Menus, viewer and status
        ["開啟資料夾…"] = new("Open Folder…", "フォルダーを開く…", "폴더 열기…", "打开文件夹…", "Ordner öffnen…", "Ouvrir un dossier…", "Abrir carpeta…"),
        ["關閉資料夾"] = new("Close Folder", "フォルダーを閉じる", "폴더 닫기", "关闭文件夹", "Ordner schließen", "Fermer le dossier", "Cerrar carpeta"),
        ["關閉資料夾並刪除快取縮圖"] = new(
            "Close Folder and Delete Cache",
            "フォルダーを閉じてキャッシュを削除",
            "폴더 닫기 및 캐시 삭제",
            "关闭文件夹并删除缓存缩略图",
            "Ordner schließen und Cache löschen",
            "Fermer le dossier et supprimer le cache",
            "Cerrar carpeta y borrar caché"),
        ["還原已隱藏的照片"] = new("Restore Hidden Photos", "非表示の写真を復元", "숨긴 사진 복원", "恢复已隐藏的照片", "Ausgeblendete Fotos wiederherstellen", "Restaurer les photos masquées", "Restaurar fotos ocultas"),
        ["還原已隱藏的照片（{0} 張）"] = new(
            "Restore Hidden Photos ({0})",
            "非表示の写真を復元（{0}枚）",
            "숨긴 사진 복원 ({0}장)",
            "恢复已隐藏的照片（{0} 张）",
            "Ausgeblendete Fotos wiederherstellen ({0})",
            "Restaurer les photos masquées ({0})",
            "Restaurar fotos ocultas ({0})"),
        ["重新整理資料夾  (F5)"] = new("Refresh Folder  (F5)", "フォルダーを更新  (F5)", "폴더 새로 고침  (F5)", "刷新文件夹  (F5)", "Ordner aktualisieren  (F5)", "Actualiser le dossier  (F5)", "Actualizar carpeta  (F5)"),
        ["紀錄"] = new("Recent Folders", "最近のフォルダー", "최근 폴더", "打开记录", "Zuletzt verwendet", "Dossiers récents", "Carpetas recientes"),
        ["（尚無開啟紀錄）"] = new("(No recent folders)", "（履歴はありません）", "(최근 폴더 없음)", "（尚无打开记录）", "(Keine Einträge)", "(Aucun dossier récent)", "(No hay carpetas recientes)"),
        ["清除紀錄"] = new("Clear History", "履歴を消去", "기록 지우기", "清除记录", "Verlauf löschen", "Effacer l'historique", "Borrar historial"),
        ["支援RAW檔相機列表"] = new("Supported RAW Cameras", "対応RAWカメラ一覧", "지원 RAW 카메라 목록", "支持RAW文件相机列表", "Unterstützte RAW-Kameras", "Appareils RAW pris en charge", "Cámaras RAW compatibles"),
        ["關於"] = new("About", "このアプリについて", "정보", "关于", "Info", "À propos", "Acerca de"),
        ["結束"] = new("Exit", "終了", "종료", "退出", "Beenden", "Quitter", "Salir"),
        ["全選"] = new("Select All", "すべて選択", "모두 선택", "全选", "Alles auswählen", "Tout sélectionner", "Seleccionar todo"),
        ["反向選擇"] = new("Invert Selection", "選択を反転", "선택 반전", "反向选择", "Auswahl umkehren", "Inverser la sélection", "Invertir selección"),
        ["取消全選"] = new("Deselect All", "選択を解除", "모두 선택 해제", "取消全选", "Auswahl aufheben", "Tout désélectionner", "Deseleccionar todo"),
        ["複製照片設定"] = new("Copy Photo Settings", "写真の設定をコピー", "사진 설정 복사", "复制照片设置", "Fotoeinstellungen kopieren", "Copier les réglages de la photo", "Copiar ajustes de la foto"),
        ["貼上照片設定"] = new("Paste Photo Settings", "写真の設定を貼り付け", "사진 설정 붙여넣기", "粘贴照片设置", "Fotoeinstellungen einfügen", "Coller les réglages de la photo", "Pegar ajustes de la foto"),
        ["建立副本"] = new("Create Virtual Copy", "仮想コピーを作成", "가상 사본 만들기", "创建虚拟副本", "Virtuelle Kopie erstellen", "Créer une copie virtuelle", "Crear copia virtual"),
        ["隱藏且不輸出"] = new("Hide (No Export)", "非表示（書き出さない）", "숨기기(내보내지 않음)", "隐藏且不输出", "Ausblenden (kein Export)", "Masquer (pas d'export)", "Ocultar (sin exportar)"),
        ["取消隱藏"] = new("Unhide", "非表示を解除", "숨기기 해제", "取消隐藏", "Einblenden", "Ne plus masquer", "Mostrar de nuevo"),
        ["不顯示隱藏"] = new("Don't Show Hidden", "非表示を表示しない", "숨긴 항목 표시 안 함", "不显示隐藏", "Ausgeblendete verbergen", "Ne pas afficher les masquées", "No mostrar ocultas"),
        ["顯示全部"] = new("Show All", "すべて表示", "모두 표시", "显示全部", "Alle anzeigen", "Tout afficher", "Mostrar todo"),
        ["刪除檔案"] = new("Delete File", "ファイルを削除", "파일 삭제", "删除文件", "Datei löschen", "Supprimer le fichier", "Eliminar archivo"),
        ["匯出"] = new("Export", "書き出し", "내보내기", "导出", "Exportieren", "Exporter", "Exportar"),
        ["套用風格檔"] = new("Apply Preset", "プリセットを適用", "프리셋 적용", "应用预设", "Vorgabe anwenden", "Appliquer le préréglage", "Aplicar preajuste"),
        ["刪除此線性漸層"] = new("Delete This Linear Gradient", "この線形グラデーションを削除", "이 선형 그라데이션 삭제", "删除此线性渐变", "Diesen linearen Verlauf löschen", "Supprimer ce dégradé linéaire", "Eliminar este degradado lineal"),
        ["選擇一張相片開始編輯"] = new(
            "Select a photo to start editing",
            "写真を選択して編集を開始",
            "편집할 사진을 선택하세요",
            "选择一张照片开始编辑",
            "Foto auswählen, um zu beginnen",
            "Sélectionnez une photo pour commencer",
            "Seleccione una foto para empezar"),
        ["點擊中性灰色區域設定白平衡"] = new(
            "Click a neutral gray area to set white balance",
            "ニュートラルグレーの部分をクリックしてホワイトバランスを設定",
            "중성 회색 영역을 클릭하여 화이트 밸런스 설정",
            "点击中性灰色区域设置白平衡",
            "Auf neutrales Grau klicken für den Weißabgleich",
            "Cliquez sur un gris neutre pour la balance des blancs",
            "Haga clic en un gris neutro para el balance de blancos"),
        ["未使用LibRaw讀取"] = new("LibRaw not in use", "LibRaw未使用", "LibRaw 사용 안 함", "未使用LibRaw读取", "LibRaw nicht verwendet", "LibRaw non utilisé", "LibRaw no usado"),
        ["LibRaw 讀取中"] = new("Decoded with LibRaw", "LibRawで読み込み", "LibRaw로 디코딩", "LibRaw 读取中", "Mit LibRaw dekodiert", "Décodé avec LibRaw", "Decodificado con LibRaw"),
        ["LibRaw 已啟用"] = new("LibRaw enabled", "LibRaw有効", "LibRaw 사용", "LibRaw 已启用", "LibRaw aktiviert", "LibRaw activé", "LibRaw activado"),
        ["算圖失敗："] = new("Render failed: ", "レンダリング失敗：", "렌더링 실패: ", "渲染失败：", "Rendern fehlgeschlagen: ", "Échec du rendu : ", "Error de renderizado: "),
        ["無法讀取資料夾："] = new("Could not read folder: ", "フォルダーを読み込めません：", "폴더를 읽을 수 없음: ", "无法读取文件夹：", "Ordner kann nicht gelesen werden: ", "Impossible de lire le dossier : ", "No se puede leer la carpeta: "),
        ["產生快取（縮圖＋預覽）"] = new("Building Cache (Thumbnails + Previews)", "キャッシュを作成（サムネイル＋プレビュー）", "캐시 생성 (썸네일 + 미리보기)", "生成缓存（缩略图＋预览）", "Cache erstellen (Miniaturen + Vorschau)", "Création du cache (vignettes + aperçus)", "Creando caché (miniaturas + vistas previas)"),
        ["第一次產生快取與縮圖檔案需要一些時間\n請稍等..."] = new(
            "The first cache and thumbnail build may take a while.\nPlease wait…",
            "初回のキャッシュとサムネイル作成には時間がかかります。\nしばらくお待ちください…",
            "첫 캐시와 썸네일 생성에는 시간이 걸릴 수 있습니다.\n잠시 기다려 주세요…",
            "首次生成缓存与缩略图文件需要一些时间\n请稍候...",
            "Der erste Cache-Aufbau kann etwas dauern.\nBitte warten…",
            "La première création du cache peut prendre du temps.\nVeuillez patienter…",
            "La primera creación de la caché puede tardar.\nEspere, por favor…"),
        ["完成，可以開始編輯"] = new("Done — ready to edit", "完了しました。編集を開始できます", "완료 — 편집할 수 있습니다", "完成，可以开始编辑", "Fertig — bereit zum Bearbeiten", "Terminé — prêt à modifier", "Listo para editar"),
        ["載入中… "] = new("Loading… ", "読み込み中… ", "불러오는 중… ", "加载中… ", "Wird geladen… ", "Chargement… ", "Cargando… "),
        ["載入失敗："] = new("Load failed: ", "読み込み失敗：", "불러오기 실패: ", "加载失败：", "Laden fehlgeschlagen: ", "Échec du chargement : ", "Error al cargar: "),
        ["儲存調整失敗："] = new("Could not save adjustments: ", "調整を保存できません：", "보정 내용을 저장할 수 없음: ", "保存调整失败：", "Anpassungen nicht gespeichert: ", "Impossible d'enregistrer les réglages : ", "No se pueden guardar los ajustes: "),
        ["此相片沒有可用的拍攝白平衡資訊"] = new(
            "This photo has no usable as-shot white balance data",
            "この写真には撮影時のホワイトバランス情報がありません",
            "이 사진에는 사용 가능한 촬영 시 화이트 밸런스 정보가 없습니다",
            "此照片没有可用的拍摄白平衡信息",
            "Kein Weißabgleich der Aufnahme verfügbar",
            "Aucune balance des blancs d'origine disponible",
            "No hay balance de blancos de captura disponible"),
        ["已複製相片設定"] = new("Photo settings copied", "写真の設定をコピーしました", "사진 설정을 복사했습니다", "已复制照片设置", "Fotoeinstellungen kopiert", "Réglages copiés", "Ajustes copiados"),
        ["尚未複製任何設定"] = new("No settings have been copied", "設定はまだコピーされていません", "복사된 설정이 없습니다", "尚未复制任何设置", "Keine Einstellungen kopiert", "Aucun réglage copié", "No se ha copiado ningún ajuste"),
        ["已貼上相片設定"] = new("Photo settings pasted", "写真の設定を貼り付けました", "사진 설정을 붙여넣었습니다", "已粘贴照片设置", "Fotoeinstellungen eingefügt", "Réglages collés", "Ajustes pegados"),
        ["已建立虛擬副本"] = new("Virtual copy created", "仮想コピーを作成しました", "가상 사본을 만들었습니다", "已创建虚拟副本", "Virtuelle Kopie erstellt", "Copie virtuelle créée", "Copia virtual creada"),
        ["已隱藏（不輸出）"] = new("Hidden (won't export)", "非表示にしました（書き出し対象外）", "숨김(내보내기 제외)", "已隐藏（不输出）", "Ausgeblendet (kein Export)", "Masquée (pas d'export)", "Oculta (sin exportar)"),
        ["已取消隱藏"] = new("Unhidden", "非表示を解除しました", "숨기기 해제됨", "已取消隐藏", "Wieder eingeblendet", "Photo réaffichée", "Visible de nuevo"),
        ["沒有已隱藏的照片"] = new("No hidden photos", "非表示の写真はありません", "숨긴 사진이 없습니다", "没有已隐藏的照片", "Keine ausgeblendeten Fotos", "Aucune photo masquée", "No hay fotos ocultas"),
        ["已還原 {0} 張隱藏的照片"] = new(
            "Restored {0} hidden photos",
            "非表示の写真を{0}枚復元しました",
            "숨긴 사진 {0}장을 복원했습니다",
            "已恢复 {0} 张隐藏的照片",
            "{0} ausgeblendete Fotos wiederhergestellt",
            "{0} photos masquées restaurées",
            "{0} fotos ocultas restauradas"),
        ["已刪除檔案"] = new("File deleted", "ファイルを削除しました", "파일을 삭제했습니다", "已删除文件", "Datei gelöscht", "Fichier supprimé", "Archivo eliminado"),
        ["刪除失敗："] = new("Delete failed: ", "削除失敗：", "삭제 실패: ", "删除失败：", "Löschen fehlgeschlagen: ", "Échec de la suppression : ", "Error al eliminar: "),
        ["已套用風格檔：{0}"] = new("Preset applied: {0}", "プリセットを適用：{0}", "프리셋 적용: {0}", "已应用预设：{0}", "Vorgabe angewendet: {0}", "Préréglage appliqué : {0}", "Preajuste aplicado: {0}"),
        ["已重新整理資料夾"] = new("Folder refreshed", "フォルダーを更新しました", "폴더를 새로 고쳤습니다", "已刷新文件夹", "Ordner aktualisiert", "Dossier actualisé", "Carpeta actualizada"),
        ["目前沒有可匯出的照片。"] = new("There is no current photo to export.", "書き出す写真がありません。", "내보낼 현재 사진이 없습니다.", "当前没有可导出的照片。", "Kein aktuelles Foto zum Exportieren.", "Aucune photo actuelle à exporter.", "No hay foto actual para exportar."),
        ["沒有可匯出的相片。"] = new("There are no photos to export.", "書き出す写真がありません。", "내보낼 사진이 없습니다.", "没有可导出的照片。", "Keine Fotos zum Exportieren.", "Aucune photo à exporter.", "No hay fotos para exportar."),
        ["匯出失敗"] = new("Export Failed", "書き出し失敗", "내보내기 실패", "导出失败", "Export fehlgeschlagen", "Échec de l'exportation", "Error de exportación"),
        ["匯出已取消"] = new("Export canceled", "書き出しをキャンセルしました", "내보내기 취소됨", "导出已取消", "Export abgebrochen", "Exportation annulée", "Exportación cancelada"),
        ["已匯出 {0} 張相片"] = new("Exported {0} photos", "{0}枚の写真を書き出しました", "사진 {0}장을 내보냈습니다", "已导出 {0} 张照片", "{0} Fotos exportiert", "{0} photos exportées", "{0} fotos exportadas"),
        ["匯出完成"] = new("Export complete", "書き出し完了", "내보내기 완료", "导出完成", "Export abgeschlossen", "Exportation terminée", "Exportación completada"),
        ["刪除照片檔案"] = new("Delete Photo File", "写真ファイルを削除", "사진 파일 삭제", "删除照片文件", "Fotodatei löschen", "Supprimer le fichier photo", "Eliminar archivo de foto"),
        ["確定刪除檔案？（會移到資源回收桶）\n{0}"] = new(
            "Delete this file? It will be moved to the Recycle Bin.\n{0}",
            "このファイルを削除しますか？ごみ箱に移動します。\n{0}",
            "이 파일을 삭제할까요? 휴지통으로 이동합니다.\n{0}",
            "确定删除文件？（会移到回收站）\n{0}",
            "Datei löschen? Sie wird in den Papierkorb verschoben.\n{0}",
            "Supprimer ce fichier ? Il sera placé dans la corbeille.\n{0}",
            "¿Eliminar este archivo? Se moverá a la papelera.\n{0}"),
        ["關閉資料夾並刪除此資料夾的快取與縮圖檔案？\n（編輯設定會保留，下次開啟會重新產生快取）"] = new(
            "Close this folder and delete its cache and thumbnails?\n(Edit settings are kept; cache will be rebuilt next time.)",
            "このフォルダーを閉じ、キャッシュとサムネイルを削除しますか？\n（編集設定は保持され、次回再作成されます）",
            "이 폴더를 닫고 캐시와 썸네일을 삭제할까요?\n(편집 설정은 유지되며 다음에 다시 생성됩니다.)",
            "关闭文件夹并删除此文件夹的缓存与缩略图文件？\n（编辑设置会保留，下次打开会重新生成缓存）",
            "Ordner schließen und Cache/Miniaturen löschen?\n(Bearbeitungen bleiben erhalten; der Cache wird neu erstellt.)",
            "Fermer le dossier et supprimer son cache et ses vignettes ?\n(Les réglages sont conservés ; le cache sera recréé.)",
            "¿Cerrar la carpeta y borrar su caché y miniaturas?\n(Los ajustes se conservan; la caché se regenerará.)"),
        ["刪除快取縮圖"] = new("Delete Cache", "キャッシュを削除", "캐시 삭제", "删除缓存缩略图", "Cache löschen", "Supprimer le cache", "Borrar caché"),
        ["已關閉資料夾並刪除快取縮圖（{0} 個檔案）"] = new(
            "Folder closed and {0} cache files deleted",
            "フォルダーを閉じ、キャッシュファイルを{0}個削除しました",
            "폴더를 닫고 캐시 파일 {0}개를 삭제했습니다",
            "已关闭文件夹并删除缓存缩略图（{0} 个文件）",
            "Ordner geschlossen, {0} Cache-Dateien gelöscht",
            "Dossier fermé, {0} fichiers de cache supprimés",
            "Carpeta cerrada, {0} archivos de caché eliminados"),
        ["資料夾已不存在：\n{0}"] = new(
            "Folder no longer exists:\n{0}",
            "フォルダーが見つかりません：\n{0}",
            "폴더가 더 이상 존재하지 않습니다:\n{0}",
            "文件夹已不存在：\n{0}",
            "Ordner existiert nicht mehr:\n{0}",
            "Le dossier n'existe plus :\n{0}",
            "La carpeta ya no existe:\n{0}"),

        // Progress / about / engine errors
        ["準備中…"] = new("Preparing…", "準備中…", "준비 중…", "准备中…", "Wird vorbereitet…", "Préparation…", "Preparando…"),
        ["取消中…"] = new("Canceling…", "キャンセル中…", "취소 중…", "取消中…", "Abbruch…", "Annulation…", "Cancelando…"),
        ["可以開始編輯"] = new("Ready to edit", "編集できます", "편집할 수 있습니다", "可以开始编辑", "Bereit zum Bearbeiten", "Prêt à modifier", "Listo para editar"),
        ["作者:"] = new("Author:", "作者：", "제작자:", "作者:", "Autor:", "Auteur :", "Autor:"),
        ["版本："] = new("Version: ", "バージョン：", "버전: ", "版本：", "Version: ", "Version : ", "Versión: "),
        ["編譯時間："] = new("Build time: ", "ビルド日時：", "빌드 시간: ", "编译时间：", "Build-Zeit: ", "Compilé le : ", "Compilado el: "),
        ["第三方元件:"] = new("Third-party components:", "サードパーティコンポーネント：", "서드파티 구성 요소:", "第三方组件:", "Drittanbieter-Komponenten:", "Composants tiers :", "Componentes de terceros:"),
        ["無法解碼影像"] = new("Could not decode image", "画像をデコードできません", "이미지를 디코딩할 수 없습니다", "无法解码图像", "Bild kann nicht dekodiert werden", "Impossible de décoder l'image", "No se puede decodificar la imagen"),
        ["無法讀取影像：{0}"] = new("Could not read image: {0}", "画像を読み込めません：{0}", "이미지를 읽을 수 없음: {0}", "无法读取图像：{0}", "Bild kann nicht gelesen werden: {0}", "Impossible de lire l'image : {0}", "No se puede leer la imagen: {0}"),
        ["匯出「{0}」失敗：{1}"] = new("Failed to export “{0}”: {1}", "「{0}」の書き出しに失敗：{1}", "‘{0}’ 내보내기 실패: {1}", "导出“{0}”失败：{1}", "Export von „{0}“ fehlgeschlagen: {1}", "Échec de l'exportation de « {0} » : {1}", "Error al exportar “{0}”: {1}")
    };
}
