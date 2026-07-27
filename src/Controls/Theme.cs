using System.Drawing;

namespace AwayPhotoRawEditor.Controls;

/// <summary>
/// Central dark-theme palette, fonts and small drawing helpers shared by all
/// custom-drawn controls.
/// </summary>
public static class Theme
{
    // Surfaces (per spec: window #1E1E1E, panels #252525, bars #2D2D2D, viewer #141414)
    public static readonly Color WindowBg = Color.FromArgb(0x1E, 0x1E, 0x1E);
    public static readonly Color PanelBg = Color.FromArgb(0x25, 0x25, 0x25);
    public static readonly Color PanelBg2 = Color.FromArgb(0x2D, 0x2D, 0x2D);
    public static readonly Color PanelBg3 = Color.FromArgb(0x3A, 0x3A, 0x3A);
    public static readonly Color ViewerBg = Color.FromArgb(0x14, 0x14, 0x14);
    public static readonly Color Border = Color.FromArgb(60, 60, 62);
    public static readonly Color BorderLight = Color.FromArgb(82, 82, 86);

    // Text
    public static readonly Color Text = Color.FromArgb(224, 224, 228);
    public static readonly Color TextDim = Color.FromArgb(150, 150, 158);
    public static readonly Color TextFaint = Color.FromArgb(110, 110, 118);

    // Accent
    public static readonly Color Accent = Color.FromArgb(45, 120, 220);
    public static readonly Color AccentHover = Color.FromArgb(60, 140, 240);
    public static readonly Color AccentDim = Color.FromArgb(38, 90, 160);

    // Sliders
    public static readonly Color SliderTrack = Color.FromArgb(64, 64, 70);
    public static readonly Color SliderFill = Color.FromArgb(150, 150, 158);
    public static readonly Color SliderKnob = Color.FromArgb(230, 230, 235);

    // Badges / status
    public static readonly Color EditedBadge = Color.FromArgb(70, 170, 90);
    public static readonly Color CopyBadge = Color.FromArgb(210, 160, 60);
    public static readonly Color Warn = Color.FromArgb(220, 90, 70);

    // Fonts
    public const string FontFamily = "Microsoft JhengHei UI";
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
