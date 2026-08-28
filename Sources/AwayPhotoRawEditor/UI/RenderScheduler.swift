import AppKit
import AwayRawCore

/// Debounced, cancellable background render pump. Slider events call `schedule()`; a
/// timer coalesces bursts, then a snapshot job (captured on the main thread) runs on a
/// worker. A monotonically increasing version guarantees only the newest render reaches
/// the UI — a stale result never overwrites a newer one.
final class RenderScheduler {

    typealias Job = (CancelToken) throws -> CGImage?

    /// Builds a render job on the main thread — snapshot the adjustments here.
    /// Returning nil means there is nothing to render.
    var jobFactory: (() -> Job?)?
    var completed: ((CGImage) -> Void)?
    var failed: ((Error) -> Void)?
    var debounce: TimeInterval = 0.07

    private var timer: Timer?
    private var version: Int = 0
    private var token: CancelToken?
    private let queue = DispatchQueue(label: "awpr.render", qos: .userInitiated)

    func schedule(immediate: Bool = false) {
        version += 1
        timer?.invalidate()
        timer = nil
        if immediate {
            fire()
        } else {
            timer = Timer.scheduledTimer(withTimeInterval: debounce, repeats: false) { [weak self] _ in
                self?.timer = nil
                self?.fire()
            }
        }
    }

    func cancelPending() {
        timer?.invalidate()
        timer = nil
        token?.cancel()
    }

    private func fire() {
        // Always called on the main thread (timer callback or an immediate schedule from
        // an event handler), so it is safe to snapshot UI state here.
        let v = version
        guard let job = jobFactory?() else { return }

        token?.cancel()
        let t = CancelToken()
        token = t

        queue.async { [weak self] in
            do {
                guard let img = try job(t) else { return }
                guard !t.isCancelled else { return }
                DispatchQueue.main.async {
                    guard let self, self.version == v else { return }
                    self.completed?(img)
                }
            } catch is CancellationError {
                // superseded — nothing to report
            } catch {
                DispatchQueue.main.async { [weak self] in self?.failed?(error) }
            }
        }
    }
}
