using System.Drawing;
using System.Threading;
using System.Threading.Tasks;

namespace AwayPhotoRawEditor.Rendering;

/// <summary>
/// Debounced, cancellable background render pump. Slider events call
/// <see cref="Schedule"/>; a WinForms timer coalesces bursts, then a snapshot job
/// (captured on the UI thread) runs on a worker. A monotonically increasing
/// version guarantees only the newest render reaches the UI — stale results are
/// discarded so an old render never overwrites a newer one.
/// </summary>
public sealed class RenderScheduler
{
    public delegate Bitmap? RenderJob(CancellationToken token);

    /// <summary>Builds a render job on the UI thread (snapshot adjustments here). Null = nothing to render.</summary>
    public Func<RenderJob?>? JobFactory { get; set; }
    public Action<Bitmap>? Completed { get; set; }
    public Action<Exception>? Failed { get; set; }
    public int DebounceMs { get; set; } = 70;

    private readonly System.Windows.Forms.Timer _timer;
    private SynchronizationContext? _ui;
    private long _version;
    private CancellationTokenSource? _cts;

    public RenderScheduler()
    {
        _timer = new System.Windows.Forms.Timer { Interval = DebounceMs };
        _timer.Tick += (_, _) => { _timer.Stop(); Fire(); };
    }

    public void Schedule(bool immediate = false)
    {
        Interlocked.Increment(ref _version);
        _timer.Stop();
        if (immediate) Fire();
        else { _timer.Interval = Math.Max(1, DebounceMs); _timer.Start(); }
    }

    public void CancelPending()
    {
        _timer.Stop();
        _cts?.Cancel();
    }

    private void Fire()
    {
        // Fire always runs on the UI thread (timer tick or immediate schedule from an
        // event handler), so capture the UI SynchronizationContext lazily here.
        _ui ??= SynchronizationContext.Current ?? new SynchronizationContext();

        long v = Interlocked.Read(ref _version);
        var job = JobFactory?.Invoke();   // UI thread: safe to snapshot state
        if (job is null) return;

        _cts?.Cancel();
        var cts = _cts = new CancellationTokenSource();
        var token = cts.Token;

        Task.Run(() =>
        {
            try
            {
                var bmp = job(token);
                if (bmp is null) return;
                if (token.IsCancellationRequested || Interlocked.Read(ref _version) != v) { bmp.Dispose(); return; }
                _ui.Post(_ =>
                {
                    if (Interlocked.Read(ref _version) == v) Completed?.Invoke(bmp);
                    else bmp.Dispose();
                }, null);
            }
            catch (OperationCanceledException) { }
            catch (Exception ex) { _ui.Post(_ => Failed?.Invoke(ex), null); }
        }, token);
    }
}
