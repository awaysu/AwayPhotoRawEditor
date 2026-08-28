import AppKit
import AwayRawCore

/// The modal progress sheet used for cache generation and export. Cancellable; the
/// completion message is only shown when the work actually finished.
final class ProgressWindowController: NSObject {

    let panel: NSPanel
    private let titleLabel = NSTextField(labelWithString: "")
    private let subtitleLabel = NSTextField(labelWithString: "")
    private let bar = NSProgressIndicator()
    private let detailLabel = NSTextField(labelWithString: "")
    private let cancelButton = FlatButton(title: "取消")

    private(set) var cancelled = false
    private weak var host: NSWindow?
    private var finishing = false

    var onCancel: (() -> Void)?

    init(title: String, subtitle: String = "") {
        panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 420, height: 190),
                        styleMask: [.titled], backing: .buffered, defer: false)
        super.init()

        panel.title = ""
        panel.isFloatingPanel = true
        panel.appearance = Theme.appearance
        let content = FlippedView(frame: panel.contentLayoutRect)
        content.wantsLayer = true
        content.layer?.backgroundColor = Theme.panelBg.cgColor
        panel.contentView = content

        titleLabel.stringValue = title
        titleLabel.font = Theme.progressTitle
        titleLabel.textColor = Theme.text
        titleLabel.alignment = .center
        titleLabel.frame = NSRect(x: 20, y: 18, width: 380, height: 24)
        content.addSubview(titleLabel)

        subtitleLabel.stringValue = subtitle
        subtitleLabel.font = Theme.normal
        subtitleLabel.textColor = Theme.textDim
        subtitleLabel.alignment = .center
        subtitleLabel.maximumNumberOfLines = 2
        subtitleLabel.lineBreakMode = .byWordWrapping
        subtitleLabel.frame = NSRect(x: 20, y: 46, width: 380, height: 38)
        content.addSubview(subtitleLabel)

        bar.style = .bar
        bar.isIndeterminate = false
        bar.minValue = 0
        bar.maxValue = 1
        bar.frame = NSRect(x: 20, y: 94, width: 380, height: 14)
        content.addSubview(bar)

        detailLabel.font = Theme.small
        detailLabel.textColor = Theme.textFaint
        detailLabel.alignment = .center
        detailLabel.lineBreakMode = .byTruncatingMiddle
        detailLabel.frame = NSRect(x: 20, y: 114, width: 380, height: 18)
        content.addSubview(detailLabel)

        cancelButton.onClick = { [weak self] in
            guard let self else { return }
            self.cancelled = true
            self.onCancel?()
            self.close()
        }
        cancelButton.frame = NSRect(x: 160, y: 142, width: 100, height: 30)
        content.addSubview(cancelButton)
    }

    func show(over window: NSWindow?) {
        host = window
        guard let window else { panel.makeKeyAndOrderFront(nil); return }
        window.beginSheet(panel)
    }

    func update(fraction: Double, message: String = "") {
        bar.doubleValue = min(max(fraction, 0), 1)
        if !message.isEmpty { detailLabel.stringValue = message }
    }

    /// Show a completion message briefly, then close. A cancelled or failed run closes
    /// straight away instead.
    func finish(message: String) {
        guard !cancelled, !finishing else { close(); return }
        finishing = true
        bar.doubleValue = 1
        detailLabel.stringValue = ""
        subtitleLabel.stringValue = message
        cancelButton.isEnabledButton = false
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { [weak self] in self?.close() }
    }

    func close() {
        if let host, panel.isSheet { host.endSheet(panel) }
        panel.orderOut(nil)
    }
}
