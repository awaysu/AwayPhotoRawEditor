import AppKit
import AwayRawCore

/// Colour hint painted along a slider track so the direction of travel reads at a glance.
/// Only used where the colour meaning is unambiguous — the tonal sliders (曝光/亮部/…)
/// keep the plain track + fill, since a grayscale ramp is invisible on the dark theme and
/// just adds noise.
enum SliderGradient {
    case none, temperature, tint, saturation
}

/// Central switchable palette, fonts and small drawing helpers shared by every
/// custom-drawn control. ClassicDark reproduces the Windows palette exactly.
enum Theme {

    struct Palette {
        let windowBg, panelBg, panelBg2, panelBg3, viewerBg: NSColor
        let border, borderLight: NSColor
        let text, textDim, textFaint: NSColor
        let accent, accentHover, accentDim: NSColor
        let sliderTrack, sliderFill, sliderKnob: NSColor
        let editedBadge, copyBadge, warn: NSColor
    }

    static func rgb(_ r: Int, _ g: Int, _ b: Int) -> NSColor {
        NSColor(srgbRed: CGFloat(r) / 255, green: CGFloat(g) / 255, blue: CGFloat(b) / 255, alpha: 1)
    }

    static let classicDark = Palette(
        windowBg:    rgb(0x1E, 0x1E, 0x1E),
        panelBg:     rgb(0x25, 0x25, 0x25),
        panelBg2:    rgb(0x2D, 0x2D, 0x2D),
        panelBg3:    rgb(0x3A, 0x3A, 0x3A),
        viewerBg:    rgb(0x14, 0x14, 0x14),
        border:      rgb(60, 60, 62),
        borderLight: rgb(82, 82, 86),
        text:        rgb(224, 224, 228),
        textDim:     rgb(150, 150, 158),
        textFaint:   rgb(110, 110, 118),
        accent:      rgb(45, 120, 220),
        accentHover: rgb(60, 140, 240),
        accentDim:   rgb(38, 90, 160),
        sliderTrack: rgb(64, 64, 70),
        sliderFill:  rgb(150, 150, 158),
        sliderKnob:  rgb(230, 230, 235),
        editedBadge: rgb(70, 170, 90),
        copyBadge:   rgb(210, 160, 60),
        warn:        rgb(220, 90, 70))

    /// A tactile, daylight-friendly studio UI. The viewer stays charcoal so surrounding
    /// luminance remains controlled while judging photographs.
    static let warmPaper = Palette(
        windowBg:    rgb(0xE9, 0xE5, 0xDE),
        panelBg:     rgb(0xF5, 0xF2, 0xEC),
        panelBg2:    rgb(0xDD, 0xD8, 0xCF),
        panelBg3:    rgb(0xFF, 0xFD, 0xF9),
        viewerBg:    rgb(0x18, 0x17, 0x15),
        border:      rgb(0xC9, 0xC1, 0xB6),
        borderLight: rgb(0xAA, 0xA0, 0x94),
        text:        rgb(0x2C, 0x2A, 0x27),
        textDim:     rgb(0x70, 0x6B, 0x64),
        textFaint:   rgb(0x99, 0x92, 0x88),
        accent:      rgb(0xC8, 0x5C, 0x32),
        accentHover: rgb(0xDE, 0x70, 0x43),
        accentDim:   rgb(0x98, 0x42, 0x24),
        sliderTrack: rgb(0xD3, 0xCC, 0xC2),
        sliderFill:  rgb(0x82, 0x79, 0x70),
        sliderKnob:  rgb(0x35, 0x32, 0x2E),
        editedBadge: rgb(0x3C, 0x91, 0x62),
        copyBadge:   rgb(0xD1, 0x91, 0x32),
        warn:        rgb(0xC8, 0x4F, 0x42))

    private(set) static var palette = classicDark
    private(set) static var currentStyle: UiStyle = .classicDark

    static var windowBg: NSColor    { palette.windowBg }
    static var panelBg: NSColor     { palette.panelBg }
    static var panelBg2: NSColor    { palette.panelBg2 }
    static var panelBg3: NSColor    { palette.panelBg3 }
    static var viewerBg: NSColor    { palette.viewerBg }
    static var border: NSColor      { palette.border }
    static var borderLight: NSColor { palette.borderLight }
    static var text: NSColor        { palette.text }
    static var textDim: NSColor     { palette.textDim }
    static var textFaint: NSColor   { palette.textFaint }
    static var accent: NSColor      { palette.accent }
    static var accentHover: NSColor { palette.accentHover }
    static var accentDim: NSColor   { palette.accentDim }
    static var sliderTrack: NSColor { palette.sliderTrack }
    static var sliderFill: NSColor  { palette.sliderFill }
    static var sliderKnob: NSColor  { palette.sliderKnob }
    static var editedBadge: NSColor { palette.editedBadge }
    static var copyBadge: NSColor   { palette.copyBadge }
    static var warn: NSColor        { palette.warn }

    static func paletteFor(_ style: UiStyle) -> Palette {
        style == .warmPaper ? warmPaper : classicDark
    }

    static func setStyle(_ style: UiStyle) {
        currentStyle = style
        palette = paletteFor(style)
    }

    /// The window appearance that matches the palette, so system-drawn chrome
    /// (scrollers, the title bar, focus rings) agrees with the custom drawing.
    static var appearance: NSAppearance? {
        NSAppearance(named: currentStyle == .warmPaper ? .aqua : .darkAqua)
    }

    // ---- slider track gradients -----------------------------------------

    /// Evenly spaced left-to-right colour stops for a gradient slider track. WarmPaper
    /// gets desaturated variants — full-strength colour looks garish on light paper.
    static func gradientStops(_ kind: SliderGradient) -> [NSColor] {
        let warm = currentStyle == .warmPaper
        switch kind {
        // Cool → warm. The Kelvin slider running warmer to the right is the opposite of
        // the physical colour temperature, but it is the convention since Lightroom and
        // what users expect.
        case .temperature:
            return warm ? [rgb(0x7E, 0x9C, 0xC4), rgb(0xC6, 0xBE, 0xB3), rgb(0xD8, 0xAC, 0x74)]
                        : [rgb(0x3A, 0x6E, 0xC8), rgb(0x78, 0x78, 0x80), rgb(0xD6, 0x96, 0x3E)]
        // Green → magenta.
        case .tint:
            return warm ? [rgb(0x8C, 0xB8, 0x94), rgb(0xC6, 0xBE, 0xB3), rgb(0xC6, 0x92, 0xBA)]
                        : [rgb(0x4E, 0xA8, 0x60), rgb(0x78, 0x78, 0x80), rgb(0xC0, 0x5E, 0xB4)]
        // Grey → saturated.
        case .saturation:
            return warm ? [rgb(0xB8, 0xB2, 0xA8), rgb(0xC8, 0x8A, 0x54)]
                        : [rgb(0x6E, 0x6E, 0x76), rgb(0xC8, 0x7A, 0x36)]
        case .none:
            return []
        }
    }

    // ---- fonts -----------------------------------------------------------
    // Cached: every drawRect uses them, so they must not be constructed per paint.
    // Changing a size goes through a relaunch, exactly as on Windows.

    private(set) static var small = NSFont.systemFont(ofSize: 11)
    private(set) static var normal = NSFont.systemFont(ofSize: 12)
    private(set) static var sectionTitle = NSFont.boldSystemFont(ofSize: 13)
    private(set) static var mono = NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular)
    private(set) static var aboutBody = NSFont.systemFont(ofSize: 12)
    private(set) static var aboutTitle = NSFont.boldSystemFont(ofSize: 18)
    private(set) static var dialogTitle = NSFont.boldSystemFont(ofSize: 14)
    private(set) static var progressTitle = NSFont.boldSystemFont(ofSize: 13)
    private(set) static var logo = NSFont.boldSystemFont(ofSize: 17)
    private(set) static var menuGlyph = NSFont.systemFont(ofSize: 20)
    private(set) static var iconGlyph = NSFont.systemFont(ofSize: 13)
    private(set) static var folderGlyph = NSFont.systemFont(ofSize: 12)

    static func rebuildFonts(_ s: FontSizes) {
        func f(_ pt: Int) -> NSFont { NSFont.systemFont(ofSize: CGFloat(pt)) }
        func b(_ pt: Int) -> NSFont { NSFont.boldSystemFont(ofSize: CGFloat(pt)) }
        small = f(s.small)
        normal = f(s.normal)
        sectionTitle = b(s.sectionTitle)
        mono = NSFont.monospacedDigitSystemFont(ofSize: CGFloat(s.mono), weight: .regular)
        aboutBody = f(s.aboutBody)
        aboutTitle = b(s.aboutTitle)
        dialogTitle = b(s.dialogTitle)
        progressTitle = b(s.progressTitle)
        logo = b(s.logo)
        menuGlyph = f(s.menuGlyph)
        iconGlyph = f(s.iconGlyph)
        folderGlyph = f(s.folderGlyph)
    }

    // ---- drawing helpers -------------------------------------------------

    static func roundedRect(_ r: NSRect, _ radius: CGFloat) -> NSBezierPath {
        NSBezierPath(roundedRect: r, xRadius: radius, yRadius: radius)
    }

    static func fill(_ r: NSRect, _ color: NSColor, radius: CGFloat = 0) {
        color.setFill()
        if radius > 0 { roundedRect(r, radius).fill() } else { r.fill() }
    }

    static func stroke(_ r: NSRect, _ color: NSColor, radius: CGFloat = 0, width: CGFloat = 1) {
        color.setStroke()
        let path = radius > 0
            ? roundedRect(r.insetBy(dx: width / 2, dy: width / 2), radius)
            : NSBezierPath(rect: r.insetBy(dx: width / 2, dy: width / 2))
        path.lineWidth = width
        path.stroke()
    }

    /// Draw text, honouring the palette. Returns the size it occupied.
    @discardableResult
    static func draw(_ s: String, at p: NSPoint, font: NSFont, color: NSColor) -> NSSize {
        let attrs: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color]
        let str = NSAttributedString(string: s, attributes: attrs)
        str.draw(at: p)
        return str.size()
    }

    static func measure(_ s: String, font: NSFont) -> NSSize {
        NSAttributedString(string: s, attributes: [.font: font]).size()
    }

    /// Draw text right-aligned to `maxX`, vertically centred on `midY`.
    static func drawRight(_ s: String, maxX: CGFloat, midY: CGFloat, font: NSFont, color: NSColor) {
        let size = measure(s, font: font)
        draw(s, at: NSPoint(x: maxX - size.width, y: midY - size.height / 2), font: font, color: color)
    }

    /// Draw text left-aligned at `x`, vertically centred on `midY`.
    static func drawLeft(_ s: String, x: CGFloat, midY: CGFloat, font: NSFont, color: NSColor) {
        let size = measure(s, font: font)
        draw(s, at: NSPoint(x: x, y: midY - size.height / 2), font: font, color: color)
    }

    /// Shorten text with an ellipsis until it fits `maxWidth`.
    static func truncate(_ s: String, font: NSFont, maxWidth: CGFloat) -> String {
        if measure(s, font: font).width <= maxWidth { return s }
        var t = s
        while !t.isEmpty, measure(t + "…", font: font).width > maxWidth { t.removeLast() }
        return t + "…"
    }

    /// Draw text centred in a rect.
    static func drawCentered(_ s: String, in r: NSRect, font: NSFont, color: NSColor) {
        let size = measure(s, font: font)
        draw(s, at: NSPoint(x: r.midX - size.width / 2, y: r.midY - size.height / 2),
             font: font, color: color)
    }
}
