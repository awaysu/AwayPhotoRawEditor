import AppKit
import AwayRawCore

/// 照片資訊 — the EXIF read-out. Drawn directly rather than built from labels: it is a
/// fixed two-column list and redrawing it is cheaper than reflowing a dozen text fields
/// on every photo change.
final class InfoPanel: SectionPanel {
    var exif: ExifData? { didSet { needsDisplay = true } }
    /// Shown after the dimensions when the photo still renders with the legacy maths.
    var isLegacyPipeline = false { didSet { needsDisplay = true } }

    convenience init() {
        self.init(title: "照片資訊")
    }

    private var rows: [(String, String)] {
        guard let e = exif else { return [] }
        return [
            ("相機", "\(e.cameraMake) \(e.cameraModel)".trimmingCharacters(in: .whitespaces)),
            ("鏡頭", e.lens),
            ("ISO", e.iso),
            ("光圈", e.aperture),
            ("快門", e.shutter),
            ("焦段", e.focalLength),
            ("曝光補償", e.exposureBias),
            ("白平衡", e.whiteBalance),
            ("測光", e.meteringMode),
            ("日期", e.dateTaken),
            ("尺寸", e.dimensionsDisplay),
            ("檔案大小", e.fileSizeDisplay),
        ]
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard exif != nil else {
            Theme.drawLeft(L.t("尚未選擇照片"), x: 12, midY: titleHeight + 20,
                           font: Theme.normal, color: Theme.textFaint)
            return
        }
        var y = titleHeight + 4
        let labelX: CGFloat = 12
        // The value column starts after the widest translated label, not at a fixed 78.
        let widest = rows.map { Theme.measure(L.t($0.0), font: Theme.small).width }.max() ?? 60
        let valueX: CGFloat = labelX + widest + 10
        let rowH: CGFloat = 20
        for (k, v) in rows {
            Theme.drawLeft(L.t(k), x: labelX, midY: y + rowH / 2,
                           font: Theme.small, color: Theme.textDim)
            // Trim overlong values (lens names especially) rather than letting them spill.
            let text = Theme.truncate(v, font: Theme.normal, maxWidth: bounds.width - valueX - 10)
            Theme.drawLeft(text, x: valueX, midY: y + rowH / 2,
                           font: Theme.normal, color: Theme.text)
            y += rowH
        }
        if isLegacyPipeline {
            Theme.drawLeft("· " + L.t("舊版處理"), x: labelX, midY: y + rowH / 2,
                           font: Theme.small, color: Theme.copyBadge)
        }
    }
}
