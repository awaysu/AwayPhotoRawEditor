using System;
using System.Drawing;
using System.Linq;
using System.Windows.Forms;
using AwayPhotoRawEditor.App;
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
        // 這是最高的對話框：150% 以上時可能高過螢幕工作區，夾住尺寸並允許捲動。
        ClientSize = Ui.FitWorkArea(598, 732);
        AutoScroll = true;

        // 座標一律是 96 DPI 設計值。
        var header = new Label { Text = "編輯風格檔", Font = Theme.Header, ForeColor = Theme.Text };
        Ui.Place(header, 16, 14, 300, 24);

        _list = new ListBox
        {
            BackColor = Theme.PanelBg2, ForeColor = Theme.Text, BorderStyle = BorderStyle.FixedSingle,
            DrawMode = DrawMode.OwnerDrawFixed, ItemHeight = Ui.S(30), IntegralHeight = false
        };
        Ui.Place(_list, 16, 50, 240, 468);
        _list.DrawItem += DrawListItem;
        _list.SelectedIndexChanged += (_, _) => { if (_list.SelectedItem is string n && n != _cur) SelectPreset(n); };

        var nameLbl = UiFactory.Label("新增自訂風格檔", Theme.TextDim);
        Ui.Place(nameLbl, 16, 528, 240, 20);
        _nameBox = UiFactory.Text();
        Ui.Place(_nameBox, 16, 552, 152, 28);
        var addBtn = new FlatButton { Text = "新增" };
        Ui.Place(addBtn, 174, 551, 82, 28);
        addBtn.Click += (_, _) => AddCustom();

        var hint = UiFactory.Label("「新增」以目前顯示的設定建立\n修改會自動儲存", Theme.TextFaint);
        hint.Font = Theme.Small;
        Ui.Place(hint, 16, 588, 240, 36);

        // right column: the same three adjust panels as the main window
        // (SectionPanel defaults to Dock.Top — switch to absolute placement first)
        _basic.Dock = _color.Dock = _detail.Dock = DockStyle.None;
        _basic.Location = Ui.Pt(272, 50);
        _color.Location = Ui.Pt(272, 305);
        _detail.Location = Ui.Pt(272, 525);
        _color.SetTemperatureMode(true);   // 風格檔一律以 Kelvin 儲存
        _color.HideWhiteBalanceRow();      // 沒有目標照片，滴管/拍攝時設定不適用
        _color.ShowPresetWhiteBalanceNote(); // 滑桿照常顯示；套用時維持照片目前的色溫色調（2026-08-23 使用者決定）

        var backupBtn = new FlatButton { Text = "備份全部" };
        Ui.Place(backupBtn, 16, 646, 116, 32);
        backupBtn.Click += (_, _) => BackupAll();

        var restoreBtn = new FlatButton { Text = "還原全部" };
        Ui.Place(restoreBtn, 140, 646, 116, 32);
        restoreBtn.Click += (_, _) => RestoreAll();

        var resetBtn = new FlatButton { Text = "恢復預設" };
        Ui.Place(resetBtn, 16, 686, 240, 32);
        resetBtn.Click += (_, _) => ResetAllPresets();

        var closeBtn = new FlatButton { Text = "關閉", Primary = true };
        Ui.Place(closeBtn, 502, 686, 80, 32);
        closeBtn.Click += (_, _) => Close();

        Controls.AddRange(new Control[] { header, _list, nameLbl, _nameBox, addBtn, hint, _basic, _color, _detail, backupBtn, restoreBtn, resetBtn, closeBtn });

        ReloadList();
        L.Apply(this);
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
        string text = PresetProfile.BuiltIn.ContainsKey(name) ? L.T(name) : name + L.T("（自訂）");
        TextRenderer.DrawText(e.Graphics, text, Font,
            new Rectangle(e.Bounds.X + Ui.S(10), e.Bounds.Y, e.Bounds.Width - Ui.S(12), e.Bounds.Height),
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
            MessageBox.Show(this, L.T("請先輸入自訂風格檔名稱"), L.T("編輯風格檔"), MessageBoxButtons.OK, MessageBoxIcon.Information);
            return;
        }
        if (name == PresetProfile.DefaultName || PresetProfile.BuiltIn.ContainsKey(name) || PresetStore.CustomNames().Contains(name))
        {
            MessageBox.Show(this, L.F("已有名為「{0}」的風格檔，請換一個名稱", name), L.T("編輯風格檔"), MessageBoxButtons.OK, MessageBoxIcon.Warning);
            return;
        }
        CommitCurrent();
        PresetStore.Save(name, _adj?.Clone() ?? new ImageAdjustments());
        _nameBox.Text = "";
        ReloadList(name);
    }

    /// <summary>備份全部：把整份風格檔設定存成使用者選擇的 XML 檔（重灌後可用「還原全部」帶回）。</summary>
    private void BackupAll()
    {
        CommitCurrent();
        using var d = new SaveFileDialog
        {
            Title = L.T("風格檔備份"),
            Filter = L.T("風格檔備份") + " (*.xml)|*.xml",
            FileName = "AwayPhotoRawEditor_Presets.xml",
            DefaultExt = "xml",
            AddExtension = true
        };
        if (d.ShowDialog(this) != DialogResult.OK) return;
        try
        {
            PresetStore.ExportTo(d.FileName);
            MessageBox.Show(this, L.F("已備份全部風格檔至：\n{0}", d.FileName),
                L.T("風格檔備份"), MessageBoxButtons.OK, MessageBoxIcon.Information);
        }
        catch (Exception ex)
        {
            MessageBox.Show(this, L.T("備份失敗：") + ex.Message,
                L.T("風格檔備份"), MessageBoxButtons.OK, MessageBoxIcon.Error);
        }
    }

    /// <summary>還原全部（先確認）：以備份檔整份取代現有風格檔設定。</summary>
    private void RestoreAll()
    {
        using var d = new OpenFileDialog
        {
            Title = L.T("風格檔備份"),
            Filter = L.T("風格檔備份") + " (*.xml)|*.xml"
        };
        if (d.ShowDialog(this) != DialogResult.OK) return;
        var r = MessageBox.Show(this,
            L.T("還原將以備份內容取代現有的所有風格檔設定。\n確定要還原？"),
            L.T("風格檔備份"), MessageBoxButtons.OKCancel, MessageBoxIcon.Warning);
        if (r != DialogResult.OK) return;
        try
        {
            if (!PresetStore.ImportFrom(d.FileName))
            {
                MessageBox.Show(this, L.T("這不是有效的風格檔備份檔"),
                    L.T("風格檔備份"), MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }
            _cur = null; _adj = null; _baseline = null;   // 捨棄編輯中的暫存，以備份內容為準
            ReloadList();
            MessageBox.Show(this, L.T("已從備份還原風格檔"),
                L.T("風格檔備份"), MessageBoxButtons.OK, MessageBoxIcon.Information);
        }
        catch (InvalidOperationException)
        {
            MessageBox.Show(this, L.T("這不是有效的風格檔備份檔"),
                L.T("風格檔備份"), MessageBoxButtons.OK, MessageBoxIcon.Warning);
        }
        catch (Exception ex)
        {
            MessageBox.Show(this, L.T("還原失敗：") + ex.Message,
                L.T("風格檔備份"), MessageBoxButtons.OK, MessageBoxIcon.Error);
        }
    }

    /// <summary>恢復預設（先確認）：刪除所有自訂風格檔、清除所有內建覆寫。</summary>
    private void ResetAllPresets()
    {
        var r = MessageBox.Show(this,
            L.T("將刪除所有自訂風格檔，並把所有內建風格檔恢復為預設值。\n確定要恢復預設？"),
            L.T("恢復預設"), MessageBoxButtons.OKCancel, MessageBoxIcon.Warning);
        if (r != DialogResult.OK) return;

        _cur = null; _adj = null; _baseline = null;   // discard pending edits on purpose
        PresetStore.ResetAllToDefaults();
        ReloadList();
    }
}
