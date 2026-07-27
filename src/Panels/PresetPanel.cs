using System;
using System.Drawing;
using System.Windows.Forms;
using AwayPhotoRawEditor.Controls;
using AwayPhotoRawEditor.Models;
using AwayPhotoRawEditor.Storage;

namespace AwayPhotoRawEditor.Panels;

/// <summary>風格檔種類 (310x106): preset ComboBox + 套用該風格檔 button.
/// 風格檔內容的編輯/新增/恢復預設在「編輯風格檔」視窗 (PresetEditorForm)。</summary>
public sealed class PresetPanel : SectionPanel
{
    private readonly ComboBox _combo;

    public event Action<string>? ApplyPreset;

    public PresetPanel() : base("風格檔種類")
    {
        ManualLayout = true;
        Size = new Size(310, 106);

        _combo = UiFactory.Combo();
        _combo.SetBounds(12, 6, 286, 30);

        var apply = new FlatButton { Text = "套用該風格檔", Primary = true };
        apply.SetBounds(12, 44, 286, 26);
        apply.Click += (_, _) => { if (_combo.SelectedItem is string name) ApplyPreset?.Invoke(name); };

        ContentArea.Controls.AddRange(new Control[] { _combo, apply });
        ReloadNames();
    }

    /// <summary>重建下拉清單：內建風格檔 + 使用者自訂（編輯風格檔視窗新增的），保留目前選取。</summary>
    public void ReloadNames()
    {
        var prev = _combo.SelectedItem as string;
        _combo.Items.Clear();
        foreach (var n in PresetProfile.BuiltInNames) _combo.Items.Add(n);
        foreach (var n in PresetStore.CustomNames()) _combo.Items.Add(n);
        int i = prev != null ? _combo.Items.IndexOf(prev) : -1;
        _combo.SelectedIndex = i >= 0 ? i : 0;
    }
}
