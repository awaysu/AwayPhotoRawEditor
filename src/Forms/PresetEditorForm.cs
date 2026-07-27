using System;
using System.Drawing;
using System.Linq;
using System.Windows.Forms;
using AwayPhotoRawEditor.Controls;
using AwayPhotoRawEditor.Models;
using AwayPhotoRawEditor.Panels;
using AwayPhotoRawEditor.Storage;

namespace AwayPhotoRawEditor.Forms;

/// <summary>
/// 編輯風格檔：左側為所有風格檔清單（內建 + 自訂），右側用 基本調整/色彩/細節 面板
/// 編輯所選風格檔的值。修改在切換選取／關閉視窗時自動存回 PresetStore（內建風格檔
/// 改回與預設完全相同時自動移除覆寫）。可用自訂名稱新增風格檔；「恢復預設」先確認，
/// 然後刪除所有自訂並把所有內建風格檔恢復為預設值。
/// </summary>
public sealed class PresetEditorForm : Form
{
    private readonly ListBox _list;
    private readonly TextBox _nameBox;
    private readonly BasicAdjustPanel _basic = new();
    private readonly ColorPanel _color = new();
    private readonly DetailPanel _detail = new();

    private string? _cur;                    // name of the preset being edited
    private ImageAdjustments? _adj;          // working values the panels are bound to
    private ImageAdjustments? _baseline;     // values at load time (skip save when unchanged)

    public PresetEditorForm()
    {
        Text = "編輯風格檔";
        BackColor = Theme.WindowBg;
        ForeColor = Theme.Text;
        Font = Theme.Normal;
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false; MinimizeBox = false;
        StartPosition = FormStartPosition.CenterParent;
        ClientSize = new Size(598, 732);

        var header = new Label { Text = "編輯風格檔", Font = Theme.Header, ForeColor = Theme.Text, Left = 16, Top = 14, Width = 300, Height = 24 };

        _list = new ListBox
        {
            BackColor = Theme.PanelBg2, ForeColor = Theme.Text, BorderStyle = BorderStyle.FixedSingle,
            DrawMode = DrawMode.OwnerDrawFixed, ItemHeight = 30, IntegralHeight = false
        };
        _list.SetBounds(16, 50, 240, 468);
        _list.DrawItem += DrawListItem;
        _list.SelectedIndexChanged += (_, _) => { if (_list.SelectedItem is string n && n != _cur) SelectPreset(n); };

        var nameLbl = UiFactory.Label("新增自訂風格檔", Theme.TextDim);
        nameLbl.SetBounds(16, 528, 240, 20);
        _nameBox = UiFactory.Text();
        _nameBox.SetBounds(16, 552, 152, 28);
        var addBtn = new FlatButton { Text = "新增" };
        addBtn.SetBounds(174, 551, 82, 28);
        addBtn.Click += (_, _) => AddCustom();

        var hint = UiFactory.Label("「新增」以目前顯示的設定建立\n修改會自動儲存", Theme.TextFaint);
        hint.Font = Theme.Small;
        hint.SetBounds(16, 588, 240, 36);

        // right column: the same three adjust panels as the main window
        // (SectionPanel defaults to Dock.Top — switch to absolute placement first)
        _basic.Dock = _color.Dock = _detail.Dock = DockStyle.None;
        _basic.Location = new Point(272, 50);
        _color.Location = new Point(272, 305);
        _detail.Location = new Point(272, 525);
        _color.SetTemperatureMode(true);   // 風格檔一律以 Kelvin 儲存
        _color.HideWhiteBalanceRow();      // 沒有目標照片，滴管/拍攝時設定不適用

        var resetBtn = new FlatButton { Text = "恢復預設" };
        resetBtn.SetBounds(16, 686, 240, 32);
        resetBtn.Click += (_, _) => ResetAllPresets();

        var closeBtn = new FlatButton { Text = "關閉", Primary = true };
        closeBtn.SetBounds(502, 686, 80, 32);
        closeBtn.Click += (_, _) => Close();

        Controls.AddRange(new Control[] { header, _list, nameLbl, _nameBox, addBtn, hint, _basic, _color, _detail, resetBtn, closeBtn });

        ReloadList();
    }

    protected override void OnFormClosing(FormClosingEventArgs e)
    {
        CommitCurrent();
        base.OnFormClosing(e);
    }

    // ---- list --------------------------------------------------------------

    /// <summary>重建清單：內建（「預設時設定」除外，它是全部重設不可編輯）+ 自訂。</summary>
    private void ReloadList(string? select = null)
    {
        _list.Items.Clear();
        foreach (var n in PresetProfile.BuiltInNames.Where(n => n != PresetProfile.DefaultName)) _list.Items.Add(n);
        foreach (var n in PresetStore.CustomNames()) _list.Items.Add(n);
        int i = select != null ? _list.Items.IndexOf(select) : -1;
        _list.SelectedIndex = i >= 0 ? i : 0;   // fires SelectedIndexChanged → SelectPreset
    }

    private void DrawListItem(object? s, DrawItemEventArgs e)
    {
        if (e.Index < 0) return;
        bool sel = (e.State & DrawItemState.Selected) != 0;
        using var bg = new SolidBrush(sel ? Theme.Accent : Theme.PanelBg2);
        e.Graphics.FillRectangle(bg, e.Bounds);
        string name = _list.Items[e.Index] as string ?? "";
        string text = PresetProfile.BuiltIn.ContainsKey(name) ? name : name + "（自訂）";
        TextRenderer.DrawText(e.Graphics, text, Font,
            new Rectangle(e.Bounds.X + 10, e.Bounds.Y, e.Bounds.Width - 12, e.Bounds.Height),
            sel ? Color.White : Theme.Text,
            TextFormatFlags.Left | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis);
    }

    // ---- edit / persist ----------------------------------------------------

    private void SelectPreset(string name)
    {
        CommitCurrent();
        _cur = name;
        _adj = EffectiveOf(name);
        _baseline = _adj.Clone();
        _basic.Bind(_adj);
        _color.Bind(_adj);
        _detail.Bind(_adj);
    }

    /// <summary>風格檔目前生效的值：優先用使用者覆寫/自訂，否則內建預設。</summary>
    private static ImageAdjustments EffectiveOf(string name)
    {
        var a = new ImageAdjustments();
        if (!PresetStore.ApplyCustom(name, a))
            PresetProfile.Get(name)?.ApplyTo(a);
        return a;
    }

    /// <summary>把目前編輯中的風格檔寫回 PresetStore；值沒變就不寫，
    /// 內建風格檔被改回與內建預設完全相同時改為移除覆寫。</summary>
    private void CommitCurrent()
    {
        if (_cur is null || _adj is null || _baseline is null) return;
        if (_adj.ValueEquals(_baseline)) return;

        if (PresetProfile.BuiltIn.ContainsKey(_cur))
        {
            var builtin = new ImageAdjustments();
            PresetProfile.Get(_cur)!.ApplyTo(builtin);
            if (_adj.ValueEquals(builtin))
            {
                PresetStore.Remove(_cur);
                _baseline = _adj.Clone();
                return;
            }
        }
        PresetStore.Save(_cur, _adj);
        _baseline = _adj.Clone();
    }

    private void AddCustom()
    {
        var name = _nameBox.Text.Trim();
        if (name.Length == 0)
        {
            MessageBox.Show(this, "請先輸入自訂風格檔名稱", "編輯風格檔", MessageBoxButtons.OK, MessageBoxIcon.Information);
            return;
        }
        if (name == PresetProfile.DefaultName || PresetProfile.BuiltIn.ContainsKey(name) || PresetStore.CustomNames().Contains(name))
        {
            MessageBox.Show(this, $"已有名為「{name}」的風格檔，請換一個名稱", "編輯風格檔", MessageBoxButtons.OK, MessageBoxIcon.Warning);
            return;
        }
        CommitCurrent();
        PresetStore.Save(name, _adj?.Clone() ?? new ImageAdjustments());
        _nameBox.Text = "";
        ReloadList(name);
    }

    /// <summary>恢復預設（先確認）：刪除所有自訂風格檔、清除所有內建覆寫。</summary>
    private void ResetAllPresets()
    {
        var r = MessageBox.Show(this,
            "將刪除所有自訂風格檔，並把所有內建風格檔恢復為預設值。\n確定要恢復預設？",
            "恢復預設", MessageBoxButtons.OKCancel, MessageBoxIcon.Warning);
        if (r != DialogResult.OK) return;

        _cur = null; _adj = null; _baseline = null;   // discard pending edits on purpose
        PresetStore.ResetAllToDefaults();
        ReloadList();
    }
}
