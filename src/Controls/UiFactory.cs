using System.Drawing;
using System.Linq;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;

namespace AwayPhotoRawEditor.Controls;

/// <summary>Factory for dark-styled standard WinForms inputs (combo / text / check / label).</summary>
public static class UiFactory
{
    public static ComboBox Combo(params string[] items)
    {
        var c = new ComboBox
        {
            DropDownStyle = ComboBoxStyle.DropDownList,
            FlatStyle = FlatStyle.Flat,
            DrawMode = DrawMode.OwnerDrawFixed,
            BackColor = Theme.PanelBg3,
            ForeColor = Theme.Text,
            Font = Theme.Normal,
            Height = 24,
            ItemHeight = 20
        };
        c.Items.AddRange(items.Select(L.T).Cast<object>().ToArray());
        c.DrawItem += (s, e) =>
        {
            if (e.Index < 0) return;
            bool sel = (e.State & DrawItemState.Selected) != 0;
            using var bg = new SolidBrush(sel ? Theme.Accent : Theme.PanelBg3);
            e.Graphics.FillRectangle(bg, e.Bounds);
            TextRenderer.DrawText(e.Graphics, c.Items[e.Index]?.ToString() ?? "", Theme.Normal,
                e.Bounds, sel ? Color.White : Theme.Text,
                TextFormatFlags.Left | TextFormatFlags.VerticalCenter);
        };
        return c;
    }

    public static TextBox Text(string text = "")
    {
        var t = new TextBox
        {
            BorderStyle = BorderStyle.FixedSingle,
            BackColor = Theme.PanelBg3,
            ForeColor = Theme.Text,
            Font = Theme.Normal,
            Text = text
        };
        MatchDpiFont(t);
        return t;
    }

    /// <summary>原生 EDIT 類控制項（TextBox/NumericUpDown）的字型不會隨 DPI 放大，
    /// owner-draw 的下拉選單（TextRenderer 依 DC DPI 換算）則會，導致兩者字級不一致。
    /// handle 建立後依 DeviceDpi 放大字型，讓文字大小與下拉選單一致。</summary>
    private static void MatchDpiFont(Control c)
    {
        c.HandleCreated += (s, _) =>
        {
            var ctl = (Control)s!;
            float scale = ctl.DeviceDpi / 96f;
            if (scale > 1.01f) ctl.Font = Theme.UI(9f * scale);
        };
    }

    public static CheckBox Check(string text, bool @checked = false)
    {
        return new CheckBox
        {
            Text = text,
            ForeColor = Theme.Text,
            BackColor = Theme.PanelBg,
            Font = Theme.Normal,
            FlatStyle = FlatStyle.Flat,
            AutoSize = true,
            Checked = @checked
        };
    }

    public static NumericUpDown Numeric(decimal min, decimal max, decimal value, int decimals = 0, decimal increment = 1)
    {
        var n = new NumericUpDown
        {
            Minimum = min,
            Maximum = max,
            Value = value < min ? min : value > max ? max : value,
            DecimalPlaces = decimals,
            Increment = increment,
            BackColor = Theme.PanelBg3,
            ForeColor = Theme.Text,
            BorderStyle = BorderStyle.FixedSingle,
            Font = Theme.Normal,
            TextAlign = HorizontalAlignment.Left
        };
        MatchDpiFont(n);
        return n;
    }

    public static Label Label(string text, Color? color = null)
    {
        return new Label
        {
            Text = text,
            ForeColor = color ?? Theme.TextDim,
            BackColor = Theme.PanelBg,
            Font = Theme.Normal,
            AutoSize = false,
            TextAlign = ContentAlignment.MiddleLeft
        };
    }
}
