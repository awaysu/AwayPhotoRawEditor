using System.Drawing;
using System.Windows.Forms;

namespace AwayPhotoRawEditor.Controls;

/// <summary>
/// 全域 DPI 縮放。整份 UI 的版面常數都是以 96 DPI（100%）設計的像素值，
/// 透過 <see cref="S(int)"/> 乘上系統縮放後才交給 WinForms。
///
/// 字型<b>不要</b>再經過這裡：<c>Theme.UI()</c> 以「點」為單位，GDI+ 本來就會依 DPI
/// 換算成像素，重複縮放會變成 1.5×1.5。這裡只負責座標與尺寸。
///
/// DPI 於 <see cref="Init"/>（Program.Main，建立任何視窗之前）取一次就固定，
/// 對應 csproj 的 <c>SystemAware</c>：整個 process 的 DPI 不會中途改變，
/// 拖到不同縮放的第二螢幕時由 Windows 拉伸（略糊但尺寸正確），
/// 換來的是不需要在執行期重算整份版面。
/// </summary>
public static class Ui
{
    /// <summary>系統 DPI，預設 96（100%）。</summary>
    public static int Dpi { get; private set; } = 96;

    /// <summary>縮放倍率：150% 為 1.5。</summary>
    public static float Scale { get; private set; } = 1f;

    /// <summary>非 100% 縮放時為 true。</summary>
    public static bool IsScaled => Scale > 1.001f || Scale < 0.999f;

    /// <summary>字型點數的修正倍率。GDI+ 已經依系統 DPI 把「點」換算成像素，所以正常情況
    /// 恆為 1；只有 <see cref="ForceScale"/> 讓 <see cref="Scale"/> 偏離系統 DPI 時才不是 1，
    /// 用來把字型補到指定的縮放。</summary>
    public static float FontScale { get; private set; } = 1f;

    /// <summary>系統實際 DPI（<see cref="ForceScale"/> 不會改動它）。</summary>
    private static int _systemDpi = 96;

    /// <summary>系統縮放（150% = 1.5）。<see cref="Scale"/> 可能因為螢幕放不下而低於它。</summary>
    public static float SystemScale => _systemDpi / 96f;

    /// <summary>主視窗版面「全部區塊都看得到」所需的 client 高度（96 DPI 設計值）：
    /// 上方列 60 + 右欄（padding 4 + 直方圖 158 + 14 + 照片資訊 285 + 14 + 工具 355 + 14 + 8）
    /// + 底部 全部重設/恢復上一步 96 ≈ 1008，抓 1010 當門檻。
    /// 左欄（60 + 縮圖列 158 + 776 = 994）比右欄稍矮，所以以右欄為準。</summary>
    private const int DesignClientHeight = 1010;
    private const int DesignClientWidth = 1100;   // = MainForm 的設計最小寬度

    /// <summary>螢幕（工作區）在不裁切版面的前提下容得下的最大縮放，下限 1.0。
    ///
    /// 級距取 0.05：0.25 級距會浪費螢幕（例如 1920×1200 開 150% 明明放得下 1.10 倍，
    /// 卻只能退到 1.00）。線條類尺寸都走 <see cref="SMin"/>，非整數倍也不會消失。</summary>
    public static float AutoFitScale()
    {
        var wa = Screen.PrimaryScreen?.WorkingArea ?? new Rectangle(0, 0, 1920, 1080);
        // 最大化視窗的 client 區約等於工作區再扣掉外框
        float byHeight = (wa.Height - 8f) / DesignClientHeight;
        float byWidth = (wa.Width - 8f) / DesignClientWidth;
        float fit = Math.Min(byHeight, byWidth);
        float stepped = (float)Math.Floor(fit * 20) / 20f;
        return Math.Clamp(stepped, 1f, 4f);
    }

    /// <summary>決定介面縮放。必須在 <c>ApplicationConfiguration.Initialize()</c> 之後、
    /// 建立任何 Form/Control 之前呼叫。
    ///
    /// <paramref name="userPercent"/> 為 0 表示「自動」：取系統縮放，但不超過螢幕容得下的
    /// 大小（<see cref="AutoFitScale"/>）。這很重要——版面是照 100% 螢幕設計的，1920×1080
    /// 開 150% 時工作區只有 1020px，硬套 1.5 倍會讓細節/風格檔/工具整塊被裁掉。</summary>
    public static void Init(int userPercent = 0)
    {
        try
        {
            using var g = Graphics.FromHwnd(IntPtr.Zero);
            int dpi = (int)Math.Round(g.DpiX);
            if (dpi is >= 72 and <= 480) Dpi = dpi;   // 超出範圍視為讀取失敗
        }
        catch { /* 取不到就維持 96，等同舊行為 */ }
        _systemDpi = Dpi;

        float target = userPercent >= 100
            ? userPercent / 100f                      // 使用者指定固定倍率
            : Math.Min(SystemScale, AutoFitScale());   // 自動：跟系統走，但不超過螢幕放得下的
        SetScale(Math.Clamp(target, 1f, 4f));
    }

    /// <summary>目前縮放是否已超過螢幕容得下的大小（此時左右欄必須開捲軸，否則會被裁掉）。</summary>
    public static bool ExceedsScreen => Scale > AutoFitScale() + 0.001f;

    private static void SetScale(float scale)
    {
        Scale = scale;
        Dpi = (int)Math.Round(96 * scale);
        // GDI+ 已依系統 DPI 把「點」換算成像素，字型只需要補上與系統縮放的差額。
        FontScale = scale / SystemScale;
    }

    /// <summary>診斷用：強制指定縮放（`AWPR_UI_SCALE` 環境變數），不必真的去改系統顯示設定
    /// 就能截出 125% / 200% 的版面。字型也一併補正，否則字會跟版面對不上。</summary>
    public static void ForceScale(float scale)
    {
        if (scale is < 0.5f or > 5f) return;
        SetScale(scale);
    }

    // ---- 純量 ----

    /// <summary>設計像素 → 實際像素。</summary>
    public static int S(int px) => (int)Math.Round(px * Scale);

    /// <summary>設計像素 → 實際像素（浮點，用於繪圖座標）。</summary>
    public static float S(float px) => px * Scale;

    /// <summary>設計像素 → 實際像素，但至少 <paramref name="min"/> 實際像素
    /// （框線、分隔線這類縮到 0 就消失的東西用）。</summary>
    public static int SMin(int px, int min = 1) => Math.Max(min, S(px));

    // ---- 結構 ----

    public static Size Sz(int w, int h) => new(S(w), S(h));
    public static Point Pt(int x, int y) => new(S(x), S(y));
    public static Rectangle Rect(int x, int y, int w, int h) => new(S(x), S(y), S(w), S(h));
    public static RectangleF RectF(float x, float y, float w, float h) => new(S(x), S(y), S(w), S(h));
    public static Padding Pad(int all) => new(S(all));
    public static Padding Pad(int left, int top, int right, int bottom) =>
        new(S(left), S(top), S(right), S(bottom));

    // ---- 控制項 ----

    /// <summary>以 96 DPI 設計座標擺放控制項（等同 SetBounds，但四個值都會縮放）。</summary>
    public static void Place(Control c, int x, int y, int w, int h) =>
        c.SetBounds(S(x), S(y), S(w), S(h));

    /// <summary>設計尺寸縮放後，再夾進主螢幕工作區。
    ///
    /// 版面是以 100% 螢幕設計的：像編輯風格檔 598×732 在 1920×1080 上只佔 68% 高，
    /// 但同一台螢幕開 150% 後需要 1098px，比工作區（約 1020px）還高，視窗會被切掉。
    /// 這裡先夾住尺寸，配合視窗的 <c>AutoScroll</c> 讓超出的部分可以捲動。</summary>
    public static Size FitWorkArea(int designW, int designH)
    {
        var wa = Screen.PrimaryScreen?.WorkingArea ?? new Rectangle(0, 0, 1920, 1080);
        int maxW = Math.Max(320, wa.Width - S(24));    // 留給左右外框
        int maxH = Math.Max(240, wa.Height - S(48));   // 留給標題列與外框
        return new Size(Math.Min(S(designW), maxW), Math.Min(S(designH), maxH));
    }
}
