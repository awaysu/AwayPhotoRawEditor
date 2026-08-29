import AppKit
import AwayRawCore

/// One undoable gesture. A batch edit snapshots every affected photo, so undo restores
/// them together and cancels any sync that has not been flushed yet.
struct UndoStep {
    var current: ImageAdjustments
    var others: [String: ImageAdjustments] = [:]
}

/// The main editor screen. Layout mirrors the Windows build: a top bar, the preview
/// strip, then three columns — adjustments on the left, the viewer in the middle, and
/// histogram / info / tools on the right.
final class MainViewController: NSViewController {

    // ---- layout constants (the Windows design values) --------------------
    static let topBarH: CGFloat = 60
    static let stripH: CGFloat = 158
    static let leftW: CGFloat = 330
    static let rightW: CGFloat = 320
    static let viewerBarH: CGFloat = 36
    static let rightBottomH: CGFloat = 96

    // ---- services --------------------------------------------------------
    let loader = RawLoader()
    let scheduler = RenderScheduler()
    var settings: AppSettings { AppSettings.current }
    var exportSettings = ExportSettings.load()

    // ---- state -----------------------------------------------------------
    var folder: String = ""
    var items: [PhotoItem] = []
    var previewList = PreviewList()

    var current: PhotoItem?
    var adj = ImageAdjustments()
    var exif: ExifData?
    var proxy: FloatImageBuffer?
    var dirty = false

    /// Guards against a stale background load applying over a newer selection.
    var loadVersion = 0
    /// True between `loadPhoto` and `applyLoaded`. In that window `adj` still belongs to
    /// the previous photo, so an edit would be applied to the wrong picture and then
    /// discarded when the load lands; edits are dropped until it does.
    var isLoading = false
    /// De-duplicates repeated selections of the same item.
    var loadedKey: String?

    var undoStack: [UndoStep] = []
    /// Steps undone with ⌘Z, replayable with ⌘⇧Z. A fresh edit clears it. Redo is
    /// single-photo only: a batch sync is not replayed, matching the Windows build.
    var redoStack: [ImageAdjustments] = []
    /// The other selected photos a batch gesture will sync to, captured at gesture start —
    /// clicking a thumbnail collapses the selection *before* the commit runs, so grabbing
    /// them at commit time would find the wrong set.
    var syncTargets: [PhotoItem] = []
    var syncBaseline: ImageAdjustments?
    var syncPending = false

    var showOriginal = false
    var pickerActive = false

    /// Set when a different photo has been loaded, so the first render that arrives fits
    /// to the window. Later renders of the same photo must not disturb the user's zoom.
    var pendingFit = false

    /// How the current photo's cached proxy was decoded (see `RawLoader.DecodeSource`).
    var proxySource: DecodeSource = .libRaw

    /// Whether the most recent preview render actually ran on the GPU, for the status line.
    var lastRenderUsedGpu = false

    /// Debounces the live thumbnail redraw of the photo being edited.
    var thumbLiveTimer: Timer?
    var thumbLiveVersion = 0

    var copiedSettings: ImageAdjustments?
    var copySourceKey: String?

    // ---- views -----------------------------------------------------------
    let topBar = FlippedView()
    let menuButton = IconButton(glyph: "☰")
    let logoLabel = NSTextField(labelWithString: "AwayPhotoRawEditor")
    let openFolderButton = FlatButton(title: "📁  開啟資料夾")
    let folderLabel = NSTextField(labelWithString: "")
    let exportCurrentButton = FlatButton(title: "匯出目前照片")
    let exportAllButton = FlatButton(title: "匯出全部照片")

    let strip = ThumbnailStrip()

    let leftScroll = NSScrollView()
    let leftColumn = FlippedView()
    let basicPanel = BasicAdjustPanel()
    let colorPanel = ColorPanel()
    let detailPanel = DetailPanel()
    let resetBcdButton = FlatButton(title: "基本 / 色彩 / 細節 重設")
    let presetPanel = PresetPanel()

    let centerColumn = FlippedView()
    let viewer = ImageViewer()
    let viewerBar = FlippedView()
    let fitButton = FlatButton(title: "適合")
    let zoom100Button = FlatButton(title: "100%")
    let zoom200Button = FlatButton(title: "200%")
    let compareButton = FlatButton(title: "對照原圖")
    let statusLabel = NSTextField(labelWithString: "")

    let rightScroll = NSScrollView()
    /// Scrolls; the reset / undo buttons below it stay pinned.
    let rightColumn = FlippedView()
    let histogramPanel = SectionPanel(title: "直方圖")
    let histogramView = HistogramView()
    let infoPanel = InfoPanel()
    let toolsPanel = ToolsPanel()
    let rightBottom = FlippedView()
    let resetAllButton = FlatButton(title: "全部重設")
    let undoButton = FlatButton(title: "恢復上一步")

    // ---- lifecycle -------------------------------------------------------

    override func loadView() {
        let root = FlippedView(frame: NSRect(x: 0, y: 0, width: 1500, height: 1040))
        root.onDropFolder = { [weak self] f in self?.openFolder(f) }
        view = root
        view.wantsLayer = true
        view.layer?.backgroundColor = Theme.windowBg.cgColor
        buildLayout()
        wirePanels()
        wireViewer()
        wireStrip()
        setupScheduler()
        setEditorEnabled(false)
        updateStatus()
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        // The window must exist before any background work posts back to it, so the
        // "reopen the last folder" step lives here rather than in loadView.
        // Not in a diagnostic run: a screenshot must show the folder it was asked for,
        // and touching a folder under Desktop/Documents can raise a TCC permission prompt
        // that a headless process can never answer.
        if !Shot.headless,
           !settings.lastFolder.isEmpty,
           FileManager.default.fileExists(atPath: settings.lastFolder),
           folder.isEmpty {
            openFolder(settings.lastFolder)
        }
    }

    // ---- layout ----------------------------------------------------------

    private func buildLayout() {
        // top bar
        topBar.wantsLayer = true
        topBar.layer?.backgroundColor = Theme.panelBg2.cgColor
        view.addSubview(topBar)

        menuButton.font = Theme.menuGlyph
        menuButton.drawsBorder = false
        menuButton.onClick = { [weak self] in self?.showAppMenu() }
        topBar.addSubview(menuButton)

        logoLabel.font = Theme.logo
        logoLabel.textColor = Theme.text
        topBar.addSubview(logoLabel)

        openFolderButton.onClick = { [weak self] in self?.pickFolder() }
        topBar.addSubview(openFolderButton)

        folderLabel.font = Theme.normal
        folderLabel.textColor = Theme.textDim
        folderLabel.lineBreakMode = .byTruncatingMiddle
        topBar.addSubview(folderLabel)

        exportCurrentButton.isPrimary = true
        exportCurrentButton.onClick = { [weak self] in self?.exportCurrent() }
        topBar.addSubview(exportCurrentButton)

        exportAllButton.isPrimary = true
        exportAllButton.onClick = { [weak self] in self?.exportAll() }
        topBar.addSubview(exportAllButton)

        // strip
        strip.showNumbers = settings.showThumbnailNumber
        view.addSubview(strip)

        // A folder (or a photo, meaning its folder) dropped anywhere on the window opens it.
        view.registerForDraggedTypes([.fileURL])

        // left column — scrollable so a short window clips nothing
        configureScroll(leftScroll, document: leftColumn)
        view.addSubview(leftScroll)
        for v in [basicPanel, colorPanel, detailPanel, presetPanel] as [NSView] {
            leftColumn.addSubview(v)
        }
        resetBcdButton.onClick = { [weak self] in self?.resetBasicColorDetail() }
        leftColumn.addSubview(resetBcdButton)

        // center column
        view.addSubview(centerColumn)
        centerColumn.addSubview(viewer)

        viewerBar.wantsLayer = true
        viewerBar.layer?.backgroundColor = Theme.panelBg2.cgColor
        centerColumn.addSubview(viewerBar)
        fitButton.onClick = { [weak self] in self?.viewer.zoomToFit() }
        zoom100Button.onClick = { [weak self] in self?.viewer.zoom100() }
        zoom200Button.onClick = { [weak self] in self?.viewer.zoom200() }
        compareButton.onClick = { [weak self] in self?.toggleShowOriginal() }
        for b in [fitButton, zoom100Button, zoom200Button, compareButton] { viewerBar.addSubview(b) }
        statusLabel.font = Theme.small
        statusLabel.textColor = Theme.textDim
        statusLabel.alignment = .right
        statusLabel.lineBreakMode = .byTruncatingHead
        viewerBar.addSubview(statusLabel)

        // right column — only the sections scroll; 全部重設 / 恢復上一步 stay pinned
        configureScroll(rightScroll, document: rightColumn)
        view.addSubview(rightScroll)
        histogramPanel.addSubview(histogramView)
        rightColumn.addSubview(histogramPanel)
        rightColumn.addSubview(infoPanel)
        rightColumn.addSubview(toolsPanel)

        rightBottom.wantsLayer = true
        rightBottom.layer?.backgroundColor = Theme.windowBg.cgColor
        view.addSubview(rightBottom)
        resetAllButton.onClick = { [weak self] in self?.resetAllAdjustments() }
        undoButton.onClick = { [weak self] in self?.doUndo() }
        rightBottom.addSubview(resetAllButton)
        rightBottom.addSubview(undoButton)
    }

    /// A palette-coloured scroll view with an overlay scroller, so the sections stay
    /// full width until the content actually needs scrolling.
    private func configureScroll(_ sv: NSScrollView, document: NSView) {
        sv.documentView = document
        sv.hasVerticalScroller = true
        sv.hasHorizontalScroller = false
        sv.autohidesScrollers = true
        sv.scrollerStyle = .overlay
        sv.drawsBackground = true
        sv.backgroundColor = Theme.windowBg
        sv.verticalScrollElasticity = .allowed
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        layoutAll()
    }

    private func layoutAll() {
        let W = view.bounds.width, H = view.bounds.height
        let T = Self.topBarH, S = Self.stripH

        topBar.frame = NSRect(x: 0, y: 0, width: W, height: T)
        menuButton.frame = NSRect(x: 10, y: (T - 34) / 2, width: 34, height: 34)
        let logoW = Theme.measure(logoLabel.stringValue, font: Theme.logo).width + 8
        logoLabel.frame = NSRect(x: 54, y: (T - 26) / 2, width: logoW, height: 26)
        // Button widths follow their captions: German and French run well past the
        // 130 pt the Chinese layout was drawn for.
        func fit(_ title: String, min: CGFloat) -> CGFloat {
            max(min, Theme.measure(L.t(title), font: Theme.normal).width + 28)
        }
        let openW = fit(openFolderButton.title, min: 120)
        openFolderButton.frame = NSRect(x: 54 + logoW + 16, y: (T - 30) / 2, width: openW, height: 30)
        let folderX = 54 + logoW + 16 + openW + 12
        let exportAllW = fit(exportAllButton.title, min: 130)
        let exportCurW = fit(exportCurrentButton.title, min: 130)
        folderLabel.frame = NSRect(x: folderX, y: (T - 20) / 2,
                                   width: max(40, W - folderX - exportAllW - exportCurW - 40), height: 20)
        exportAllButton.frame = NSRect(x: W - exportAllW - 12, y: (T - 30) / 2, width: exportAllW, height: 30)
        exportCurrentButton.frame = NSRect(x: W - exportAllW - exportCurW - 20, y: (T - 30) / 2,
                                           width: exportCurW, height: 30)

        strip.frame = NSRect(x: 0, y: T, width: W, height: S)

        let bodyY = T + S
        let bodyH = max(0, H - bodyY)
        leftScroll.frame = NSRect(x: 0, y: bodyY, width: Self.leftW, height: bodyH)
        rightScroll.frame = NSRect(x: W - Self.rightW, y: bodyY,
                                   width: Self.rightW, height: max(0, bodyH - Self.rightBottomH))
        rightBottom.frame = NSRect(x: W - Self.rightW, y: H - Self.rightBottomH,
                                   width: Self.rightW, height: Self.rightBottomH)
        centerColumn.frame = NSRect(x: Self.leftW, y: bodyY,
                                    width: max(100, W - Self.leftW - Self.rightW), height: bodyH)

        // left column: fixed-height sections stacked with a small gap. The document is
        // sized to its content, so the scroll view only scrolls when it has to.
        let lx: CGFloat = 10
        let lw: CGFloat = Self.leftW - 20 - (leftScroll.verticalScroller?.isHidden == false ? 12 : 0)
        var ly: CGFloat = 4
        basicPanel.frame = NSRect(x: lx, y: ly, width: lw, height: 245); ly += 249
        colorPanel.frame = NSRect(x: lx, y: ly, width: lw, height: 210); ly += 214
        detailPanel.frame = NSRect(x: lx, y: ly, width: lw, height: 145); ly += 155
        resetBcdButton.frame = NSRect(x: lx, y: ly, width: lw, height: 30); ly += 40
        presetPanel.frame = NSRect(x: lx, y: ly, width: lw, height: 106); ly += 110
        leftColumn.frame = NSRect(x: 0, y: 0, width: Self.leftW, height: max(bodyH, ly))

        // centre column
        let cw = centerColumn.bounds.width, ch = centerColumn.bounds.height
        viewer.frame = NSRect(x: 0, y: 0, width: cw, height: max(0, ch - Self.viewerBarH))
        viewerBar.frame = NSRect(x: 0, y: ch - Self.viewerBarH, width: cw, height: Self.viewerBarH)
        var bx: CGFloat = 8
        for b in [fitButton, zoom100Button, zoom200Button] {
            let bw = max(60, Theme.measure(L.t(b.title), font: Theme.normal).width + 20)
            b.frame = NSRect(x: bx, y: 4, width: bw, height: Self.viewerBarH - 8)
            bx += bw + 4
        }
        let cw2 = max(92, Theme.measure(L.t(compareButton.title), font: Theme.normal).width + 20)
        compareButton.frame = NSRect(x: bx + 8, y: 4, width: cw2, height: Self.viewerBarH - 8)
        statusLabel.frame = NSRect(x: cw - 280, y: 8, width: 272, height: 20)

        // right column
        let rx: CGFloat = 10
        let rw: CGFloat = Self.rightW - 20 - (rightScroll.verticalScroller?.isHidden == false ? 12 : 0)
        var ry: CGFloat = 4
        histogramPanel.frame = NSRect(x: rx, y: ry, width: rw, height: 158); ry += 162
        histogramView.frame = NSRect(x: 10, y: histogramPanel.titleHeight,
                                     width: rw - 20, height: 158 - histogramPanel.titleHeight - 8)
        infoPanel.frame = NSRect(x: rx, y: ry, width: rw, height: 285); ry += 289
        // The tools ribbon needs 355 for its tallest page (the gradient sliders); give it
        // that much and let the column scroll rather than squeezing the page.
        let toolsH: CGFloat = 355
        toolsPanel.frame = NSRect(x: rx, y: ry, width: rw, height: toolsH); ry += toolsH + 4
        rightColumn.frame = NSRect(x: 0, y: 0, width: Self.rightW,
                                   height: max(rightScroll.bounds.height, ry))

        resetAllButton.frame = NSRect(x: rx, y: 14, width: Self.rightW - 20, height: 30)
        undoButton.frame = NSRect(x: rx, y: 52, width: Self.rightW - 20, height: 30)
    }

    // ---- wiring ----------------------------------------------------------

    private func wirePanels() {
        for p in [basicPanel, colorPanel, detailPanel] as [AdjustPanelBase] {
            p.onEditBegin = { [weak self] in self?.pushUndo() }
            p.onChanged = { [weak self] in
                guard let self else { return }
                // The panels edit their own copy; pull it back before rendering.
                self.adj = p.adjustments ?? self.adj
                self.onAdjustmentChanged()
            }
        }
        colorPanel.onPickerToggled = { [weak self] on in
            guard let self else { return }
            self.pickerActive = on
            self.viewer.whiteBalancePickerActive = on
            self.updateStatus()
        }
        colorPanel.onAsShot = { [weak self] in self?.useAsShotWhiteBalance() }
        presetPanel.onApply = { [weak self] name in self?.applyPresetToSelection(name) }

        toolsPanel.onEditBegin = { [weak self] in self?.pushUndo() }
        toolsPanel.onChanged = { [weak self] in
            guard let self else { return }
            self.adj = self.toolsPanel.adjustments ?? self.adj
            self.viewer.adjustments = self.adj
            self.onAdjustmentChanged()
        }
        toolsPanel.onToolChanged = { [weak self] mode in self?.onToolChanged(mode) }
        toolsPanel.onAddGradient = { [weak self] in self?.addGradient() }
        toolsPanel.onHealModeChanged = { [weak self] m in
            guard let self else { return }
            if self.viewer.setHealMode(m) { self.adj = self.viewer.adjustments ?? self.adj }
        }
        toolsPanel.onHealBrushSize = { [weak self] size in
            guard let self else { return }
            if self.viewer.setHealBrushSize(size) { self.adj = self.viewer.adjustments ?? self.adj }
        }
        toolsPanel.onRotate = { [weak self] cw in self?.rotate(clockwise: cw) }
        toolsPanel.onCropAspectChanged = { [weak self] aspect in self?.applyCropAspect(aspect) }
    }

    private func wireViewer() {
        viewer.onEditBegin = { [weak self] in self?.pushUndo() }
        viewer.onEditChanged = { [weak self] in
            guard let self else { return }
            self.adj = self.viewer.adjustments ?? self.adj
            self.toolsPanel.adjustments = self.adj
            self.toolsPanel.rebindGradient()
            self.onAdjustmentChanged()
        }
        viewer.onWhiteBalancePicked = { [weak self] nx, ny in
            self?.whiteBalancePicked(nx: nx, ny: ny)
        }
        viewer.onViewChanged = { [weak self] in self?.updateStatus() }
        viewer.onGradientSelectionChanged = { [weak self] in
            guard let self else { return }
            self.adj = self.viewer.adjustments ?? self.adj
            self.toolsPanel.adjustments = self.adj
            self.toolsPanel.rebindGradient()
        }
    }

    private func wireStrip() {
        strip.onSelectionChanged = { [weak self] in
            guard let self, let item = self.strip.currentItem else { return }
            self.loadPhoto(item)
        }
        strip.onContextMenu = { [weak self] index, event in
            self?.showThumbnailMenu(index: index, event: event)
        }
        // Double-click forces a reload of the item even when it is already current.
        strip.onActivate = { [weak self] index in
            guard let self, index >= 0, index < self.items.count else { return }
            self.loadPhoto(self.items[index], force: true)
        }
    }

    private func setupScheduler() {
        scheduler.jobFactory = { [weak self] in
            guard let self, let proxy = self.proxy else { return nil }
            // Snapshot everything on the main thread; the worker must not touch UI state.
            let a = self.renderAdjustments()
            let ctx = ProcessContext()
            ctx.camera = self.exif?.camera
            // A proxy that ImageIO decoded already has the camera's white balance baked
            // in; one from LibRaw is balanced to pre_mul. Handing the matrix the wrong
            // reference tints the whole photo.
            ctx.whiteBalanceReference = self.proxySource.whiteBalanceReference
            ctx.watermark = self.exportSettings.buildWatermark()
            // The watermark is authored at full resolution, so scale it to the proxy.
            // EXIF width can be 0 (a non-RAW without a size tag), which must not be
            // mistaken for "full size is zero".
            let exifW = self.exif?.width ?? 0, exifH = self.exif?.height ?? 0
            let fullLong = (exifW > 0 && exifH > 0) ? max(exifW, exifH) : max(proxy.width, proxy.height)
            ctx.watermarkScale = Double(max(proxy.width, proxy.height)) / Double(fullLong)
            switch self.viewer.tool {
            case .gradient, .heal:
                // Their handles live in the pre-geometry frame, so the preview must show
                // the full frame or the overlay coordinates drift.
                ctx.skipGeometry = true
            case .crop:
                ctx.skipCropRect = true
            case .none:
                break
            }
            return { token in
                ctx.token = token
                let out = try ImageProcessor.applyToFloat(proxy, a, ctx)
                guard var img = ImageIOCodec.toCGImage(out) else { return nil }
                img = Watermark.apply(img, ctx)
                DispatchQueue.main.async { [weak self] in self?.lastRenderUsedGpu = ctx.usedGpu }
                return img
            }
        }
        scheduler.completed = { [weak self] img in self?.onRenderDone(img) }
        scheduler.failed = { [weak self] err in
            self?.statusLabel.stringValue = err.localizedDescription
        }
    }

    /// What to actually render: the real adjustments, or — while 對照原圖 is held — a
    /// neutral set that keeps only the geometry, so the comparison frames identically.
    func renderAdjustments() -> ImageAdjustments {
        guard showOriginal else { return adj }
        var a = ImageAdjustments()
        a.pipelineVersion = adj.pipelineVersion
        a.cropAspectRatio = adj.cropAspectRatio
        a.cropAngle = adj.cropAngle
        a.cropX = adj.cropX; a.cropY = adj.cropY
        a.cropWidth = adj.cropWidth; a.cropHeight = adj.cropHeight
        a.rotation = adj.rotation
        a.distortion = adj.distortion
        // As-shot white balance, so "original" means the camera's rendering rather than
        // a 5200 K neutral that would look wrong on every indoor shot.
        if let cam = exif?.camera, cam.isValid, let shot = ColorScience.asShot(cam) {
            a.temperature = min(max(shot.kelvin, ColorScience.minKelvin), ColorScience.maxKelvin)
            a.tint = shot.tint
        } else if let e = exif, e.hasAsShotWhiteBalance {
            a.temperature = e.colorTemperature
        }
        return a
    }

    func onRenderDone(_ img: CGImage) {
        viewer.setImage(img, resetView: pendingFit)
        pendingFit = false
        if let buf = ImageIOCodec.toFloatBuffer(img) {
            histogramView.histogram = ImageStats.computeHistogram(buf)
            histogramView.mean = ImageStats.meanColor(buf)
        }
        updateStatus()
    }
}

/// A top-left-origin container, so every frame in this file reads the same way the
/// Windows layout code does. The main window's root also accepts a dropped folder.
final class FlippedView: NSView {
    override var isFlipped: Bool { true }

    /// Set by the main view controller; nil on the plain containers.
    var onDropFolder: ((String) -> Void)?

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        onDropFolder != nil && droppedFolder(sender) != nil ? .copy : []
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        guard let f = droppedFolder(sender) else { return false }
        onDropFolder?(f)
        return true
    }

    /// The folder a drop refers to: a directory as is (RAW_TEMP → its parent), or a
    /// supported image's containing folder.
    private func droppedFolder(_ sender: NSDraggingInfo) -> String? {
        guard let urls = sender.draggingPasteboard.readObjects(forClasses: [NSURL.self],
                                                               options: nil) as? [URL],
              let u = urls.first else { return nil }
        let p = u.path
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: p, isDirectory: &isDir) else { return nil }
        if isDir.boolValue {
            return AppPaths.isRawTemp(p) ? (p as NSString).deletingLastPathComponent : p
        }
        return AppPaths.isSupported(p) ? (p as NSString).deletingLastPathComponent : nil
    }
}
