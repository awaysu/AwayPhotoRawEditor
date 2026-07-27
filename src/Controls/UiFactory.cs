using System.Drawing;
using System.Windows.Forms;

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
        c.Items.AddRange(items);
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
        return new TextBox
        {
            BorderStyle = BorderStyle.FixedSingle,
            BackColor = Theme.PanelBg3,
            ForeColor = Theme.Text,
            Font = Theme.Normal,
            Text = text
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
        return new NumericUpDown
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
