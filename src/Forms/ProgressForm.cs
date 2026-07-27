using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;
using AwayPhotoRawEditor.Controls;

namespace AwayPhotoRawEditor.Forms;

/// <summary>
/// Dark, cancelable progress dialog. Runs an async job (given an IProgress and a
/// CancellationToken) while showing modally; shared by cache generation and export.
/// The bar eases toward its target and carries a subtle moving highlight.
/// </summary>
public sealed class ProgressForm : Form
{
    private readonly CancellationTokenSource _cts = new();
    private readonly Func<IProgress<(int done, int total, string msg)>, CancellationToken, Task> _work;
    private readonly string _title;
    private readonly string? _doneMessage;
    private readonly Label _msg;
    private readonly Panel _bar;
    private readonly FlatButton _cancel;
    private readonly System.Windows.Forms.Timer _anim;
    private double _frac;        // target fraction from the worker
    private double _dispFrac;    // eased, displayed fraction
    private int _shimmer;        // moving-highlight phase, in px
    private bool _indeterminate = true;

    public Exception? Error { get; private set; }
    public bool Canceled => _cts.IsCancellationRequested;

    /// <param name="subtitle">Optional persistent line shown under the header (e.g. a "please wait" note).</param>
    /// <param name="doneMessage">Optional message shown briefly (with a full bar) once the work completes.</param>
    public ProgressForm(string title, Func<IProgress<(int done, int total, string msg)>, CancellationToken, Task> work,
        string? subtitle = null, string? doneMessage = null)
    {
        _work = work;
        _title = title;
        _doneMessage = doneMessage;
        Text = "";   // the styled header already shows the title — avoid a duplicate caption
        FormBorderStyle = FormBorderStyle.FixedDialog;
        StartPosition = FormStartPosition.CenterParent;
        MaximizeBox = false; MinimizeBox = false;
        bool hasSub = !string.IsNullOrEmpty(subtitle);
        ClientSize = new Size(480, hasSub ? 224 : 176);
        BackColor = Theme.WindowBg;
        ForeColor = Theme.Text;
        Font = Theme.Normal;
        ControlBox = false;

        Controls.Add(BuildHeader());

        int msgTop = 68;
        if (hasSub)
        {
            // Centered, multi-line "please wait" note.
            var sub = new Label { Text = subtitle, ForeColor = Theme.Accent, Left = 20, Top = 60, Width = 440, Height = 44, BackColor = Theme.WindowBg, TextAlign = ContentAlignment.MiddleCenter };
            Controls.Add(sub);
            msgTop = 114;
        }

        _msg = new Label { Text = "準備中…", ForeColor = Theme.TextDim, Left = 20, Top = msgTop, Width = 440, Height = 20, AutoEllipsis = true, BackColor = Theme.WindowBg };
        _bar = new Panel { Left = 20, Top = msgTop + 28, Width = 440, Height = 18, BackColor = Theme.WindowBg };
        _bar.Paint += PaintBar;
        _cancel = new FlatButton { Text = "取消", Width = 90, Height = 30, Top = msgTop + 62, Left = 370 };
        _cancel.Click += (_, _) => { _cts.Cancel(); _cancel.Enabled = false; _cancel.Text = "取消中…"; };

        Controls.AddRange(new Control[] { _msg, _bar, _cancel });

        _anim = new System.Windows.Forms.Timer { Interval = 30 };
        _anim.Tick += (_, _) => AnimTick();
        _anim.Start();
    }

    private Panel BuildHeader()
    {
        var header = new Panel { Left = 0, Top = 0, Width = 480, Height = 52, BackColor = Theme.PanelBg2 };
        header.Paint += (_, e) =>
        {
            var g = e.Graphics;
            PaintHelpers.EnableHighQuality(g);
            using (var accent = new SolidBrush(Theme.Accent))
                g.FillRectangle(accent, 0, 0, 4, header.Height);
            using (var line = new Pen(Color.FromArgb(60, Theme.Accent)))
                g.DrawLine(line, 0, header.Height - 1, header.Width, header.Height - 1);
            TextRenderer.DrawText(g, _title, Theme.UI(12f, FontStyle.Bold),
                new Rectangle(20, 0, 448, header.Height), Theme.Text,
                TextFormatFlags.Left | TextFormatFlags.VerticalCenter);
        };
        return header;
    }

    private void PaintBar(object? sender, PaintEventArgs e)
    {
        var g = e.Graphics;
        PaintHelpers.EnableHighQuality(g);
        float h = _bar.Height, w = _bar.Width;
        PaintHelpers.FillRounded(g, new RectangleF(0, 0, w, h), h / 2, Theme.SliderTrack);

        int fillW = (int)Math.Round(w * Math.Clamp(_dispFrac, 0, 1));
        if (fillW > h)
        {
            using var clip = PaintHelpers.RoundedRect(new RectangleF(0, 0, fillW, h), h / 2);
            var save = g.Save();
            g.SetClip(clip);
            using (var grad = new LinearGradientBrush(new RectangleF(0, 0, fillW, h), Theme.AccentDim, Theme.AccentHover, LinearGradientMode.Horizontal))
                g.FillRectangle(grad, 0, 0, fillW, h);
            // moving highlight sweep
            int sx = _shimmer % (fillW + 120) - 60;
            using (var shine = new LinearGradientBrush(new RectangleF(sx, 0, 80, h),
                Color.FromArgb(0, 255, 255, 255), Color.FromArgb(70, 255, 255, 255), LinearGradientMode.Horizontal))
                g.FillRectangle(shine, sx, 0, 40, h);
            using (var shine2 = new LinearGradientBrush(new RectangleF(sx + 40, 0, 80, h),
                Color.FromArgb(70, 255, 255, 255), Color.FromArgb(0, 255, 255, 255), LinearGradientMode.Horizontal))
                g.FillRectangle(shine2, sx + 40, 0, 40, h);
            g.Restore(save);
        }

        string pct = _indeterminate ? "" : $"{(int)Math.Round(_dispFrac * 100)}%";
        if (pct.Length > 0)
            TextRenderer.DrawText(g, pct, Theme.Small, new Rectangle(0, 0, (int)w, (int)h),
                Color.White, TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
    }

    private void AnimTick()
    {
        _shimmer += 6;
        // Ease the displayed fill toward the reported target.
        double d = _frac - _dispFrac;
        if (Math.Abs(d) > 0.001) _dispFrac += d * 0.2;
        else _dispFrac = _frac;
        _bar.Invalidate();
    }

    protected override async void OnShown(EventArgs e)
    {
        base.OnShown(e);
        try
        {
            var prog = new Progress<(int done, int total, string msg)>(Report);
            await _work(prog, _cts.Token);
            // Briefly show a completion message (e.g. "可以開始編輯") before closing.
            if (!Canceled && !string.IsNullOrEmpty(_doneMessage) && !IsDisposed)
            {
                _indeterminate = false; _frac = 1;
                _msg.ForeColor = Theme.Accent;
                _msg.Text = _doneMessage;
                _cancel.Enabled = false;
                await Task.Delay(850, _cts.Token);
            }
        }
        catch (OperationCanceledException) { }
        catch (Exception ex) { Error = ex; }
        finally
        {
            // Guard: this runs in an async void; any escaping exception would crash the process.
            try
            {
                if (!IsDisposed)
                    DialogResult = Canceled ? DialogResult.Cancel : DialogResult.OK; // closes the modal dialog
            }
            catch { }
        }
    }

    private void Report((int done, int total, string msg) p)
    {
        _indeterminate = p.total <= 0;
        _frac = p.total > 0 ? (double)p.done / p.total : 0;
        _msg.Text = p.total > 0 ? $"{p.done} / {p.total}   {p.msg}" : p.msg;
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing) { _anim.Dispose(); _cts.Dispose(); }
        base.Dispose(disposing);
    }
}
