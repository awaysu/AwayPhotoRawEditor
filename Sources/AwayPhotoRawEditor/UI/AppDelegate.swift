import AppKit
import AwayRawCore

final class AppDelegate: NSObject, NSApplicationDelegate {

    var window: NSWindow!
    var controller: MainViewController!

    func applicationDidFinishLaunching(_ notification: Notification) {
        // First run: the language is chosen before anything else exists, so every caption
        // is built in the chosen language from the start. Diagnostic runs skip it.
        let diagnostic = Shot.parse(Array(CommandLine.arguments.dropFirst()))
        Shot.headless = diagnostic
        if settings.isFirstRun && !diagnostic {
            let picker = FirstRunLanguageController()
            picker.onChosen = { lang in
                let s = AppSettings.current
                s.uiLanguage = lang
                s.save()
                L.setLanguage(lang)
                NSApp.stopModal()
            }
            picker.panel.center()
            picker.show(over: nil)
            NSApp.runModal(for: picker.panel)
        }

        buildMainMenu()

        controller = MainViewController()
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1500, height: 1040),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable],
                          backing: .buffered, defer: false)
        window.title = "AwayPhotoRawEditor"
        window.appearance = Theme.appearance
        window.contentViewController = controller
        window.minSize = NSSize(width: 1100, height: 720)
        // No state restoration: the app rebuilds its own state from settings.xml, and
        // opting out also keeps macOS from putting up its modal "reopen windows?" alert
        // after a crash — which a headless diagnostic run could never dismiss.
        window.isRestorable = false
        window.delegate = self
        window.center()
        window.makeKeyAndOrderFront(nil)
        // The Windows build starts maximized; the closest equivalent here is filling the
        // visible frame rather than true full screen, which would hide the menu bar.
        if let screen = window.screen ?? NSScreen.main {
            window.setFrame(screen.visibleFrame, display: true)
        }
        // Diagnostic screenshot modes take over from here and quit when done.
        let args = Array(CommandLine.arguments.dropFirst())
        if Shot.parse(args) {
            Shot.run(controller: controller, window: window, args: args)
            return
        }

        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool { true }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        controller?.saveCurrentIfDirty()
        controller?.savePreviewList()
        return .terminateNow
    }

    // ---- menu bar --------------------------------------------------------

    private func buildMainMenu() {
        let main = NSMenu()

        // Application menu
        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: L.t("關於") + " AwayPhotoRawEditor",
                        action: #selector(showAbout), keyEquivalent: "")
            .target = self
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: L.t("設定…"), action: #selector(showSettings), keyEquivalent: ",")
            .target = self
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: L.t("隱藏") + " AwayPhotoRawEditor",
                        action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        appMenu.addItem(withTitle: L.t("結束") + " AwayPhotoRawEditor",
                        action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        main.addItem(appItem)

        // File
        let fileItem = NSMenuItem()
        let fileMenu = NSMenu(title: L.t("檔案"))
        fileMenu.addItem(withTitle: L.t("開啟資料夾…"), action: #selector(openFolder), keyEquivalent: "o")
            .target = self
        fileMenu.addItem(withTitle: L.t("重新整理資料夾  (F5)"), action: #selector(refreshFolder), keyEquivalent: "r")
            .target = self
        fileMenu.addItem(withTitle: L.t("關閉資料夾"), action: #selector(closeFolder), keyEquivalent: "w")
            .target = self
        fileMenu.addItem(.separator())
        fileMenu.addItem(withTitle: L.t("匯出目前照片"), action: #selector(exportCurrent), keyEquivalent: "e")
            .target = self
        let exportAll = fileMenu.addItem(withTitle: L.t("匯出全部照片"),
                                         action: #selector(exportAll), keyEquivalent: "e")
        exportAll.keyEquivalentModifierMask = [.command, .shift]
        exportAll.target = self
        fileItem.submenu = fileMenu
        main.addItem(fileItem)

        // Edit
        let editItem = NSMenuItem()
        let editMenu = NSMenu(title: L.t("編輯"))
        editMenu.addItem(withTitle: L.t("恢復上一步"), action: #selector(undo), keyEquivalent: "z")
            .target = self
        let redo = editMenu.addItem(withTitle: L.t("重做"), action: #selector(redoEdit), keyEquivalent: "z")
        redo.keyEquivalentModifierMask = [.command, .shift]     // ⌘⇧Z, the Mac's Ctrl+Y
        redo.target = self
        editMenu.addItem(.separator())
        editMenu.addItem(withTitle: L.t("複製照片設定"), action: #selector(copySettings), keyEquivalent: "c")
            .target = self
        editMenu.addItem(withTitle: L.t("貼上照片設定"), action: #selector(pasteSettings), keyEquivalent: "v")
            .target = self
        editMenu.addItem(.separator())
        editMenu.addItem(withTitle: L.t("全選"), action: #selector(selectAll), keyEquivalent: "a")
            .target = self
        editMenu.addItem(.separator())
        editMenu.addItem(withTitle: L.t("全部重設"), action: #selector(resetAll), keyEquivalent: "")
            .target = self
        editMenu.addItem(withTitle: L.t("編輯風格檔…"), action: #selector(presetEditor), keyEquivalent: "")
            .target = self
        editItem.submenu = editMenu
        main.addItem(editItem)

        // View
        let viewItem = NSMenuItem()
        let viewMenu = NSMenu(title: L.t("檢視"))
        viewMenu.addItem(withTitle: L.t("適合"), action: #selector(zoomFit), keyEquivalent: "0")
            .target = self
        viewMenu.addItem(withTitle: "100%", action: #selector(zoom100), keyEquivalent: "1")
            .target = self
        viewMenu.addItem(withTitle: "200%", action: #selector(zoom200), keyEquivalent: "2")
            .target = self
        viewMenu.addItem(.separator())
        // Backslash matches the Windows shortcut for the before/after toggle.
        viewMenu.addItem(withTitle: L.t("對照原圖"), action: #selector(toggleOriginal), keyEquivalent: "\\")
            .target = self
        viewItem.submenu = viewMenu
        main.addItem(viewItem)

        NSApp.mainMenu = main
    }

    @objc func showAbout() { controller.showAbout() }
    @objc func showSettings() { controller.showSettings() }
    @objc func openFolder() { controller.pickFolder() }
    @objc func refreshFolder() { controller.refreshFolder() }
    @objc func closeFolder() { controller.closeFolder() }
    @objc func exportCurrent() { controller.exportCurrent() }
    @objc func exportAll() { controller.exportAll() }
    @objc func undo() { controller.doUndo() }
    @objc func redoEdit() { controller.doRedo() }
    @objc func resetAll() { controller.resetAllAdjustments() }
    @objc func presetEditor() { controller.showPresetEditor() }
    @objc func zoomFit() { controller.viewer.zoomToFit() }
    @objc func zoom100() { controller.viewer.zoom100() }
    @objc func zoom200() { controller.viewer.zoom200() }
    @objc func toggleOriginal() { controller.toggleShowOriginal() }
    @objc func selectAll() { controller.strip.selectAll() }

    @objc func copySettings() {
        guard let item = controller.current else { return }
        controller.copiedSettings = controller.adj
        controller.copySourceKey = item.key
        for it in controller.items { it.isCopySettingsSource = (it.key == item.key) }
        controller.strip.refreshBadges()
    }

    @objc func pasteSettings() {
        let dummy = NSMenuItem()
        dummy.representedObject = controller.current
        controller.menuPasteSettings(dummy)
    }
}

extension AppDelegate: NSWindowDelegate {
    func windowWillClose(_ notification: Notification) {
        controller?.saveCurrentIfDirty()
        controller?.savePreviewList()
    }
}

extension NSView {
    /// Force a redraw of an entire subtree — used after a language change, where every
    /// caption is pulled at draw time.
    func subviewsNeedDisplay() {
        needsDisplay = true
        for v in subviews { v.subviewsNeedDisplay() }
    }
}
