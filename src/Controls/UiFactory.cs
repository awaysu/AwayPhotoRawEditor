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
            Height = Ui.S(24),
            ItemHeight = Ui.S(20)
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
        return t;
    }

    // 註：舊版有個 MatchDpiFont，會在 handle 建立後把 TextBox/NumericUpDown 的字型再放大
    // DeviceDpi/96 倍——那是 PerMonitorV2 但完全沒做 DPI 縮放時的權宜之計。改成 SystemAware
    // 之後原生 EDIT 控制項的字型已經正確跟著系統 DPI 放大，再乘一次會變成 1.5×1.5，
    // 輸入框的字明顯大過旁邊的標籤與下拉選單，所以移除。

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
