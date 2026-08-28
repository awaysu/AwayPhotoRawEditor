import AppKit
import AwayRawCore

/// Opening the modal dialogs and handling what they change.
extension MainViewController {

    func showSettings() {
        let d = SettingsWindowController()
        d.onSaved = { [weak self] needsRestart in
            guard let self else { return }
            self.loader.syncFromSettings()
            self.strip.showNumbers = self.settings.showThumbnailNumber
            self.strip.refreshBadges()
            if needsRestart { self.promptRelaunch() }
        }
        d.show(over: view.window)
    }

    func showFontSizeDialog() {
        let d = FontSizeWindowController()
        d.onSaved = { [weak self] in self?.promptRelaunch() }
        d.show(over: view.window)
    }

    func showAbout() {
        AboutWindowController().show(over: view.window)
    }

    func showPresetEditor() {
        let d = PresetEditorWindowController()
        d.onClosed = { [weak self] in self?.presetPanel.reload() }
        d.show(over: view.window)
    }

    /// Fonts and the palette are cached statics rebuilt at launch, so changing them
    /// relaunches — the same approach the Windows build takes with Application.Restart.
    func promptRelaunch() {
        let a = NSAlert()
        a.messageText = L.t("需要重新啟動")
        a.informativeText = L.t("變更介面設定後需要重新啟動程式才會套用。")
        a.addButton(withTitle: L.t("立即重新啟動"))
        a.addButton(withTitle: L.t("稍後"))
        guard a.runModal() == .alertFirstButtonReturn else { return }
        saveCurrentIfDirty()
        savePreviewList()
        relaunchApp()
    }

    private func relaunchApp() {
        let url = Bundle.main.bundleURL
        let config = NSWorkspace.OpenConfiguration()
        config.createsNewApplicationInstance = true
        NSWorkspace.shared.openApplication(at: url, configuration: config) { _, _ in
            DispatchQueue.main.async { NSApp.terminate(nil) }
        }
    }
}
