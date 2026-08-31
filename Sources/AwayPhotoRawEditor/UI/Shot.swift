import AppKit
import AwayRawCore

/// Headless screenshots, the counterpart of the Windows build's `--shot` / `--dlgshot`.
/// The window draws itself into a bitmap, so this needs no Screen Recording permission
/// and works over SSH or in CI.
///
///   AwayPhotoRawEditor --shot <folder> <out.png> [waitMs] [WxH]      (AWPR_SHOT_TOOL=crop|gradient|heal)
///   AwayPhotoRawEditor --dlgshot <export|settings|presets|about|fonts|firstrun|progress> <out.png>
enum Shot {

    /// Set while a diagnostic run is in progress. Suppresses the shortcuts that would
    /// otherwise steal real keyboard focus, and stops the run from writing LastFolder —
    /// a screenshot must not pollute the user's settings.
    nonisolated(unsafe) static var headless = false

    static func parse(_ args: [String]) -> Bool {
        args.contains("--shot") || args.contains("--dlgshot") || args.contains("--uitest")
    }

    /// Render a view hierarchy to a PNG.
    @discardableResult
    static func capture(_ view: NSView, to path: String) -> Bool {
        view.layoutSubtreeIfNeeded()
        guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return false }
        view.cacheDisplay(in: view.bounds, to: rep)
        guard let data = rep.representation(using: .png, properties: [:]) else { return false }
        do {
            try data.write(to: URL(fileURLWithPath: path))
            let scale = rep.pixelsWide / max(1, Int(view.bounds.width))
            FileHandle.standardOutput.write(Data(
                "wrote \(path)  \(rep.pixelsWide)x\(rep.pixelsHigh) (@\(scale)x)\n".utf8))
            return true
        } catch {
            FileHandle.standardError.write(Data("shot failed: \(error)\n".utf8))
            return false
        }
    }

    /// The window at a chosen client size, so short-window layouts can be checked too.
    static func run(controller: MainViewController, window: NSWindow, args: [String]) {
        headless = true

        if let i = args.firstIndex(of: "--uitest"), i + 1 < args.count {
            window.setContentSize(NSSize(width: 1500, height: 1040))
            controller.view.frame = NSRect(x: 0, y: 0, width: 1500, height: 1040)
            controller.view.layoutSubtreeIfNeeded()
            UITest.run(controller: controller, folder: args[i + 1],
                       report: i + 2 < args.count ? args[i + 2] : nil)
            return
        }
        if let i = args.firstIndex(of: "--dlgshot"), i + 2 < args.count {
            runDialog(kind: args[i + 1], path: args[i + 2], controller: controller, window: window)
            return
        }
        guard let i = args.firstIndex(of: "--shot"), i + 2 < args.count else {
            NSApp.terminate(nil); return
        }
        let folder = args[i + 1]
        let out = args[i + 2]
        let waitMs = (i + 3 < args.count ? Int(args[i + 3]) : nil) ?? 2500
        // Size the view rather than the window: macOS clamps a window to the screen, so
        // going through the window would make the result depend on the display it ran on.
        let size = (i + 4 < args.count ? parseSize(args[i + 4]) : nil)
                   ?? NSSize(width: 1500, height: 1040)
        window.setContentSize(size)
        controller.view.frame = NSRect(origin: .zero, size: size)
        controller.view.layoutSubtreeIfNeeded()

        if !folder.isEmpty, folder != "-" {
            controller.openFolder(folder)
        }
        // Give the cache generation and the first render time to land.
        DispatchQueue.main.asyncAfter(deadline: .now() + Double(waitMs) / 1000.0) {
            controller.view.frame = NSRect(origin: .zero, size: size)
            controller.view.layoutSubtreeIfNeeded()
            // AWPR_SHOT_TOOL=crop|gradient|heal photographs the viewer with that tool's
            // overlay (a gradient is added so its handles show). The handle geometry is
            // the part of the UI no headless assertion can judge.
            if let tool = ProcessInfo.processInfo.environment["AWPR_SHOT_TOOL"] {
                switch tool {
                case "crop": controller.toolsPanel.selectTool(.crop)
                case "gradient": controller.toolsPanel.selectTool(.gradient); controller.addGradient()
                case "heal": controller.toolsPanel.selectTool(.heal)
                default: break
                }
                controller.view.layoutSubtreeIfNeeded()
                controller.viewer.needsDisplay = true
            }
            let ok = capture(controller.view, to: out)
            // exit() rather than terminate(): background cache work can otherwise keep
            // the run alive long after the picture is written.
            exit(ok ? 0 : 1)
        }
    }

    private static func parseSize(_ s: String) -> NSSize? {
        let parts = s.lowercased().split(separator: "x")
        guard parts.count == 2, let w = Double(parts[0]), let h = Double(parts[1]) else { return nil }
        return NSSize(width: w, height: h)
    }

    private static func runDialog(kind: String, path: String,
                                  controller: MainViewController, window: NSWindow) {
        // Build the dialog and photograph its content view directly — a sheet cannot be
        // captured offscreen, but its content view can.
        let target: NSView?
        switch kind {
        case "export":
            let d = ExportWindowController(settings: ExportSettings.load(), count: 3)
            target = d.panelContentView
        case "settings":
            target = SettingsWindowController().content
        case "presets":
            target = PresetEditorWindowController().content
        case "about":
            target = AboutWindowController().content
        case "fonts":
            target = FontSizeWindowController().content
        case "firstrun":
            target = FirstRunLanguageController().content
        case "progress":
            let p = ProgressWindowController(title: L.t("產生快取（縮圖＋預覽）"),
                                             subtitle: L.t("第一次產生快取與縮圖檔案需要一些時間\n請稍等..."))
            p.update(fraction: 0.42, message: "DSCF6897.RAF")
            target = p.panelContentView
        default:
            FileHandle.standardError.write(Data("unknown dialog: \(kind)\n".utf8))
            target = nil
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
            let ok = target.map { capture($0, to: path) } ?? false
            exit(ok ? 0 : 1)
        }
    }
}

extension ExportWindowController {
    var panelContentView: NSView? { panel.contentView }
}

extension ProgressWindowController {
    var panelContentView: NSView? { panel.contentView }
}
