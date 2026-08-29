import AppKit
import AwayRawCore

// Entry point. The palette and the cached fonts are decided once, here, before any view
// exists — every custom-drawn control reads them from statics, and rebuilding them at
// runtime would mean re-laying out the whole window, so a change goes through a relaunch.

let settings = AppSettings.current
// Diagnostics: AWPR_UI_LANGUAGE / AWPR_UI_STYLE override the saved values in memory only,
// so screenshots of every language and theme can be taken without touching settings.xml.
let env = ProcessInfo.processInfo.environment
if let raw = env["AWPR_UI_LANGUAGE"],
   let lang = AppLanguage.allCases.first(where: { $0.rawValue.caseInsensitiveCompare(raw) == .orderedSame }) {
    settings.uiLanguage = lang
}
if let raw = env["AWPR_UI_STYLE"],
   let style = UiStyle.allCases.first(where: { $0.rawValue.caseInsensitiveCompare(raw) == .orderedSame }) {
    settings.interfaceStyle = style
}
Theme.setStyle(settings.interfaceStyle)
Theme.rebuildFonts(settings.fontSizes)
L.setLanguage(settings.uiLanguage)

let app = NSApplication.shared
app.setActivationPolicy(.regular)

let delegate = AppDelegate()
app.delegate = delegate
app.run()
