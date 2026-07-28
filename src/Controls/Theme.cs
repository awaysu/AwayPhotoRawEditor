using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;

namespace AwayPhotoRawEditor.Controls;

/// <summary>
/// Central switchable palette, fonts and small drawing helpers shared by all
/// custom-drawn controls. ClassicDark preserves the original UI exactly.
/// </summary>
public static class Theme
{
    public sealed record Palette(
        Color WindowBg, Color PanelBg, Color PanelBg2, Color PanelBg3, Color ViewerBg,
        Color Border, Color BorderLight,
        Color Text, Color TextDim, Color TextFaint,
        Color Accent, Color AccentHover, Color AccentDim,
        Color SliderTrack, Color SliderFill, Color SliderKnob,
        Color EditedBadge, Color CopyBadge, Color Warn);

    private static readonly Palette ClassicDark = new(
        // Original palette: keep every value unchanged for backwards visual parity.
        Color.FromArgb(0x1E, 0x1E, 0x1E),
        Color.FromArgb(0x25, 0x25, 0x25),
        Color.FromArgb(0x2D, 0x2D, 0x2D),
        Color.FromArgb(0x3A, 0x3A, 0x3A),
        Color.FromArgb(0x14, 0x14, 0x14),
        Color.FromArgb(60, 60, 62),
        Color.FromArgb(82, 82, 86),
        Color.FromArgb(224, 224, 228),
        Color.FromArgb(150, 150, 158),
        Color.FromArgb(110, 110, 118),
        Color.FromArgb(45, 120, 220),
        Color.FromArgb(60, 140, 240),
        Color.FromArgb(38, 90, 160),
        Color.FromArgb(64, 64, 70),
        Color.FromArgb(150, 150, 158),
        Color.FromArgb(230, 230, 235),
        Color.FromArgb(70, 170, 90),
        Color.FromArgb(210, 160, 60),
        Color.FromArgb(220, 90, 70));

    private static readonly Palette WarmPaper = new(
        // A tactile, daylight-friendly studio UI. The viewer stays charcoal so
        // surrounding luminance remains controlled while judging photographs.
        Color.FromArgb(0xE9, 0xE5, 0xDE),
        Color.FromArgb(0xF5, 0xF2, 0xEC),
        Color.FromArgb(0xDD, 0xD8, 0xCF),
        Color.FromArgb(0xFF, 0xFD, 0xF9),
        Color.FromArgb(0x18, 0x17, 0x15),
        Color.FromArgb(0xC9, 0xC1, 0xB6),
        Color.FromArgb(0xAA, 0xA0, 0x94),
        Color.FromArgb(0x2C, 0x2A, 0x27),
        Color.FromArgb(0x70, 0x6B, 0x64),
        Color.FromArgb(0x99, 0x92, 0x88),
        Color.FromArgb(0xC8, 0x5C, 0x32),
        Color.FromArgb(0xDE, 0x70, 0x43),
        Color.FromArgb(0x98, 0x42, 0x24),
        Color.FromArgb(0xD3, 0xCC, 0xC2),
        Color.FromArgb(0x82, 0x79, 0x70),
        Color.FromArgb(0x35, 0x32, 0x2E),
        Color.FromArgb(0x3C, 0x91, 0x62),
        Color.FromArgb(0xD1, 0x91, 0x32),
        Color.FromArgb(0xC8, 0x4F, 0x42));

    private static Palette _palette = ClassicDark;

    public static UiStyle CurrentStyle { get; private set; } = UiStyle.ClassicDark;
    public static Color WindowBg => _palette.WindowBg;
    public static Color PanelBg => _palette.PanelBg;
    public static Color PanelBg2 => _palette.PanelBg2;
    public static Color PanelBg3 => _palette.PanelBg3;
    public static Color ViewerBg => _palette.ViewerBg;
    public static Color Border => _palette.Border;
    public static Color BorderLight => _palette.BorderLight;
    public static Color Text => _palette.Text;
    public static Color TextDim => _palette.TextDim;
    public static Color TextFaint => _palette.TextFaint;
    public static Color Accent => _palette.Accent;
    public static Color AccentHover => _palette.AccentHover;
    public static Color AccentDim => _palette.AccentDim;
    public static Color SliderTrack => _palette.SliderTrack;
    public static Color SliderFill => _palette.SliderFill;
    public static Color SliderKnob => _palette.SliderKnob;
    public static Color EditedBadge => _palette.EditedBadge;
    public static Color CopyBadge => _palette.CopyBadge;
    public static Color Warn => _palette.Warn;

    /// <summary>Returns a palette for drawing theme previews without changing the app.</summary>
    public static Palette PaletteFor(UiStyle style) =>
        style == UiStyle.WarmPaper ? WarmPaper : ClassicDark;

    /// <summary>
    /// Switches the active palette. When a live root is supplied, colors captured
    /// by standard WinForms controls at construction time are remapped in-place;
    /// custom controls repaint from the dynamic palette automatically.
    /// </summary>
    public static void Apply(UiStyle style, Control? liveRoot = null)
    {
        if (!System.Enum.IsDefined(style)) style = UiStyle.ClassicDark;
        var old = _palette;
        var next = PaletteFor(style);
        CurrentStyle = style;
        _palette = next;

        if (liveRoot is null || old == next) return;
        RestyleTree(liveRoot, old, next);
        liveRoot.PerformLayout();
        liveRoot.Invalidate(true);
        liveRoot.Update();
    }

    private static void RestyleTree(Control control, Palette old, Palette next)
    {
        control.BackColor = Remap(control.BackColor, old, next);
        control.ForeColor = Remap(control.ForeColor, old, next);

        if (control is LinkLabel link)
        {
            link.LinkColor = Remap(link.LinkColor, old, next);
            link.ActiveLinkColor = Remap(link.ActiveLinkColor, old, next);
            link.VisitedLinkColor = Remap(link.VisitedLinkColor, old, next);
        }
        if (control is CardPanel card)
        {
            card.CardColor = Remap(card.CardColor, old, next);
            if (card.BorderColor is { } border)
                card.BorderColor = Remap(border, old, next);
        }

        foreach (Control child in control.Controls)
            RestyleTree(child, old, next);
        control.Invalidate();
    }

    private static Color Remap(Color color, Palette old, Palette next)
    {
        Color[] before =
        {
            old.WindowBg, old.PanelBg, old.PanelBg2, old.PanelBg3, old.ViewerBg,
            old.Border, old.BorderLight, old.Text, old.TextDim, old.TextFaint,
            old.Accent, old.AccentHover, old.AccentDim, old.SliderTrack,
            old.SliderFill, old.SliderKnob, old.EditedBadge, old.CopyBadge, old.Warn
        };
        Color[] after =
        {
            next.WindowBg, next.PanelBg, next.PanelBg2, next.PanelBg3, next.ViewerBg,
            next.Border, next.BorderLight, next.Text, next.TextDim, next.TextFaint,
            next.Accent, next.AccentHover, next.AccentDim, next.SliderTrack,
            next.SliderFill, next.SliderKnob, next.EditedBadge, next.CopyBadge, next.Warn
        };
        for (int i = 0; i < before.Length; i++)
            if (color.ToArgb() == before[i].ToArgb()) return after[i];
        return color;
    }

    // Fonts
    public static string FontFamily => L.CurrentLanguage switch
    {
        AppLanguage.English => "Segoe UI",
        AppLanguage.Japanese => "Yu Gothic UI",
        AppLanguage.Korean => "Malgun Gothic",
        AppLanguage.SimplifiedChinese => "Microsoft YaHei UI",
        AppLanguage.German or AppLanguage.French or AppLanguage.Spanish => "Segoe UI",
        _ => "Microsoft JhengHei UI"
    };
    public static Font UI(float size = 9f, FontStyle style = FontStyle.Regular) =>
        new(FontFamily, size, style);
    public static readonly Font Small = UI(8f);
    public static readonly Font Normal = UI(9f);
    public static readonly Font Header = UI(10f, FontStyle.Bold);
    public static readonly Font Mono = new("Consolas", 8.5f);

    public static readonly StringFormat CenterLeft = new()
    {
        Alignment = StringAlignment.Near,
        LineAlignment = StringAlignment.Center,
        Trimming = StringTrimming.EllipsisCharacter,
        FormatFlags = StringFormatFlags.NoWrap
    };

    public static readonly StringFormat Center = new()
    {
        Alignment = StringAlignment.Center,
        LineAlignment = StringAlignment.Center,
        Trimming = StringTrimming.EllipsisCharacter,
        FormatFlags = StringFormatFlags.NoWrap
    };
}
