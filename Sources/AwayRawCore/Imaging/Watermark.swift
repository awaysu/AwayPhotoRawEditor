import Foundation
import CoreGraphics
import CoreText

/// Step 11: the watermark overlay. Drawn with Core Text rather than AppKit so the whole
/// rendering path stays usable from the headless CLI.
public enum Watermark {

    /// Draw the watermark onto a rendered image. Returns the input unchanged when the
    /// watermark is disabled or empty.
    public static func apply(_ img: CGImage, _ ctx: ProcessContext) -> CGImage {
        guard let wm = ctx.watermark, wm.enabled,
              !wm.text.trimmingCharacters(in: .whitespaces).isEmpty else { return img }
        return draw(img, wm, scale: ctx.watermarkScale)
    }

    public static func draw(_ img: CGImage, _ wm: WatermarkSpec, scale: Double) -> CGImage {
        let w = img.width, h = img.height
        let size = max(4.0, wm.fontSize * scale)
        let margin = (Double(wm.margin) * scale).roundedHalfEven
        let alpha = 1.0 - Double(min(max(wm.transparency, 0), 100)) / 100.0

        guard let ctx = CGContext(data: nil, width: w, height: h,
                                  bitsPerComponent: 8, bytesPerRow: 0,
                                  space: ImageIOCodec.srgb,
                                  bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue |
                                              CGBitmapInfo.byteOrder32Big.rawValue)
        else { return img }
        ctx.draw(img, in: CGRect(x: 0, y: 0, width: w, height: h))

        // Fall back to the system font if the configured family is not installed —
        // the Windows build's font names (Arial, Microsoft JhengHei UI) will often not be.
        let font = CTFontCreateWithName(wm.fontName as CFString, size, nil)
        let (r, g, b) = wm.color.rgb
        let color = CGColor(srgbRed: r / 255, green: g / 255, blue: b / 255, alpha: alpha)

        // Core Text attribute keys rather than the AppKit ones, so this file stays
        // usable from the headless CLI.
        let attrs: [CFString: Any] = [
            kCTFontAttributeName: font,
            kCTForegroundColorAttributeName: color
        ]
        let attributed = CFAttributedStringCreate(nil, wm.text as CFString, attrs as CFDictionary)!
        let line = CTLineCreateWithAttributedString(attributed)
        let bounds = CTLineGetBoundsWithOptions(line, .useOpticalBounds)

        let tw = bounds.width, th = bounds.height
        let x: Double
        switch wm.position {
        case .topLeft, .bottomLeft: x = margin
        case .topRight, .bottomRight: x = Double(w) - tw - margin
        }
        // Core Graphics draws from the bottom-left; the spec places the margin from the
        // named edge, so top positions measure down from the top of the image.
        let y: Double
        switch wm.position {
        case .topLeft, .topRight: y = Double(h) - margin - th
        case .bottomLeft, .bottomRight: y = margin
        }

        ctx.textPosition = CGPoint(x: x - bounds.origin.x, y: y - bounds.origin.y)
        CTLineDraw(line, ctx)
        return ctx.makeImage() ?? img
    }
}
