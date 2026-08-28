import AppKit
import AwayRawCore

// Entry point. The palette and the cached fonts are decided once, here, before any view
// exists — every custom-drawn control reads them from statics, and rebuilding them at
// runtime would mean re-laying out the whole window, so a change goes through a relaunch.

let settings = AppSettings.current
Theme.setStyle(settings.interfaceStyle)
Theme.rebuildFonts(settings.fontSizes)
L.setLanguage(settings.uiLanguage)

let app = NSApplication.shared
app.setActivationPolicy(.regular)

let delegate = AppDelegate()
app.delegate = delegate
app.run()
