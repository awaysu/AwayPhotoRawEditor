import AppKit
import AwayRawCore

/// Opening folders, building caches, and loading a photo into the editor.
extension MainViewController {

    // ---- folder ----------------------------------------------------------

    func pickFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = L.t("開啟資料夾")
        if !folder.isEmpty { panel.directoryURL = URL(fileURLWithPath: folder) }
        panel.begin { [weak self] resp in
            guard resp == .OK, let url = panel.url else { return }
            self?.openFolder(url.path)
        }
    }

    func openFolder(_ path: String) {
        saveCurrentIfDirty()
        folder = path
        folderLabel.stringValue = path
        // A diagnostic screenshot run must not rewrite the user's last-opened folder.
        if !Shot.headless {
            settings.lastFolder = path
            settings.save()
        }

        previewList = PreviewListStore.load(imageFolder: path)
        rebuildItems()

        guard !items.isEmpty else {
            clearEditor()
            setEditorEnabled(false)
            statusLabel.stringValue = L.t("此資料夾沒有支援的影像")
            return
        }

        // Building the proxy for every photo up front is slow (a full RAW decode each),
        // but it is what makes selecting a photo instant afterwards.
        let progress = ProgressWindowController(
            title: L.t("產生快取（縮圖＋預覽）"),
            subtitle: L.t("第一次產生快取與縮圖檔案需要一些時間\n請稍等..."))
        progress.show(over: view.window)

        generateCaches(items: items, progress: progress) { [weak self] in
            guard let self else { return }
            progress.finish(message: L.t("完成，可以開始編輯"))
            self.setEditorEnabled(true)
            if let first = self.items.first {
                self.strip.select(index: 0)
                _ = first
            }
        }
    }

    /// Rebuild the strip's items from disk plus the per-folder preview list, honouring
    /// the hidden filter and re-creating virtual copies.
    func rebuildItems() {
        let files = AppPaths.imagesInFolder(folder)
        var all: [PhotoItem] = files.map { PhotoItem(sourcePath: $0) }

        // Virtual copies are inserted right after their original.
        for e in previewList.virtualCopies.sorted(by: { $0.index < $1.index }) {
            guard let at = all.lastIndex(where: { $0.sourcePath == e.path }) else { continue }
            all.insert(PhotoItem(sourcePath: e.path, virtualCopyIndex: e.index), at: at + 1)
        }

        let hidden = Set(previewList.hidden)
        for it in all { it.isHidden = hidden.contains(it.key) }

        // #numbers count hidden photos too, so a hidden #2 leaves the strip showing #1, #3.
        for (i, it) in all.enumerated() { it.displayNumber = i + 1 }

        items = settings.showHiddenPhotos ? all : all.filter { !$0.isHidden }
        for it in items {
            let (a, _, placeholder) = AdjustmentXmlStore.loadAll(imagePath: it.sourcePath,
                                                                 copyIndex: it.virtualCopyIndex)
            it.isEdited = !(placeholder || a == nil || a!.isDefault)
            it.isCopySettingsSource = (it.key == copySourceKey)
        }
        strip.setItems(items)
        loadCachedThumbnails()
    }

    /// Fill the strip with whatever thumbnails are already on disk, so it never shows
    /// empty cells while the background work catches up.
    private func loadCachedThumbnails() {
        let snapshot = items
        DispatchQueue.global(qos: .utility).async { [weak self] in
            for it in snapshot {
                guard let self else { return }
                guard let buf = self.loader.loadThumbnailCache(path: it.sourcePath) else { continue }
                let rendered = self.renderThumbnail(base: buf, for: it)
                guard let img = ImageIOCodec.toCGImage(rendered) else { continue }
                DispatchQueue.main.async { self.strip.setImage(img, forKey: it.key) }
            }
        }
    }

    /// Generate the thumbnail and proxy caches for a whole folder.
    func generateCaches(items list: [PhotoItem], progress: ProgressWindowController,
                        completion: @escaping () -> Void) {
        let total = list.count
        // A small semaphore: a full-resolution RAW decode per photo will exhaust memory
        // if they all run at once.
        let limit = DispatchSemaphore(value: 2)
        let group = DispatchGroup()
        var done = 0
        let lock = NSLock()
        // Virtual copies share their original's decode, so de-duplicate by source path.
        var seen = Set<String>()
        let uniqueSources = list.filter { seen.insert($0.sourcePath).inserted }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            for it in uniqueSources {
                guard let self, !progress.cancelled else { break }
                group.enter()
                limit.wait()
                DispatchQueue.global(qos: .userInitiated).async {
                    defer { limit.signal(); group.leave() }
                    guard !progress.cancelled else { return }
                    _ = self.loader.ensureThumbnailCache(path: it.sourcePath, maxW: 240, maxH: 160)
                    _ = self.loader.ensureProxyCache(path: it.sourcePath)

                    // Seed the adjustment XML so as-shot white balance is set once, here,
                    // rather than on first selection.
                    var e: ExifData? = ExifReader.read(path: it.sourcePath)
                    _ = self.loader.enrichCameraColor(path: it.sourcePath, exif: &e)
                    _ = AdjustmentXmlStore.ensureDefault(imagePath: it.sourcePath, exif: e)

                    if let buf = self.loader.loadThumbnailCache(path: it.sourcePath) {
                        for target in list where target.sourcePath == it.sourcePath {
                            let rendered = self.renderThumbnail(base: buf, for: target)
                            if let img = ImageIOCodec.toCGImage(rendered) {
                                DispatchQueue.main.async { self.strip.setImage(img, forKey: target.key) }
                            }
                        }
                    }
                    lock.lock(); done += 1; let d = done; lock.unlock()
                    DispatchQueue.main.async {
                        progress.update(fraction: Double(d) / Double(max(1, total)),
                                        message: it.fileName)
                    }
                }
            }
            group.wait()
            DispatchQueue.main.async { completion() }
        }
    }

    // ---- thumbnails ------------------------------------------------------

    /// Render one strip thumbnail with its photo's adjustments applied.
    ///
    /// The base is the cached `_thumb.jpg`, which for a RAW file came from the camera's
    /// embedded preview — so the camera's white balance is *already baked in*. The main
    /// preview's proxy goes through LibRaw with `use_camera_wb` unset, which is why the
    /// temperature slider is the only white balance there. Applying `adj.temperature`
    /// verbatim to a thumbnail would therefore apply white balance twice (a 3200 K
    /// tungsten shot would come out solidly blue). Instead the *offset from as-shot* is
    /// re-based onto the neutral, and only for RAW files that have an as-shot reading.
    func renderThumbnail(base: FloatImageBuffer, for item: PhotoItem,
                         overrideAdjustments: ImageAdjustments? = nil) -> FloatImageBuffer {
        let (stored, exif, _) = AdjustmentXmlStore.loadAll(imagePath: item.sourcePath,
                                                           copyIndex: item.virtualCopyIndex)
        var a = overrideAdjustments ?? stored ?? ImageAdjustments()
        guard !a.isDefault else { return base }

        let ctx = ProcessContext()
        ctx.camera = exif?.camera
        if AppPaths.isRaw(item.sourcePath) {
            // The thumbnail is the camera's own rendering, so it is balanced to cam_mul.
            ctx.whiteBalanceReference = .asShot
            if let e = exif, e.hasAsShotWhiteBalance, ctx.camera == nil {
                a.temperature = 5200 + (a.temperature - e.colorTemperature)
            }
        }
        // No watermark: at 240×160 it is only noise.
        ctx.watermark = nil
        return (try? ImageProcessor.applyToFloat(base, a, ctx)) ?? base
    }

    func refreshThumbnail(for item: PhotoItem, adjustments: ImageAdjustments? = nil) {
        DispatchQueue.global(qos: .utility).async { [weak self] in
            guard let self,
                  let base = self.loader.loadThumbnailCache(path: item.sourcePath) else { return }
            let rendered = self.renderThumbnail(base: base, for: item,
                                                overrideAdjustments: adjustments)
            guard let img = ImageIOCodec.toCGImage(rendered) else { return }
            DispatchQueue.main.async { self.strip.setImage(img, forKey: item.key) }
        }
    }

    func refreshThumbnails(_ list: [PhotoItem]) {
        for it in list { refreshThumbnail(for: it) }
    }

    /// Redraw the *current* photo's thumbnail from the unsaved adjustments, debounced,
    /// so the strip tracks the sliders live without writing XML on every drag.
    func liveRefreshCurrentThumbnail() {
        thumbLiveTimer?.invalidate()
        thumbLiveTimer = Timer.scheduledTimer(withTimeInterval: 0.2, repeats: false) { [weak self] _ in
            guard let self, let item = self.current else { return }
            self.thumbLiveVersion += 1
            let version = self.thumbLiveVersion
            let snapshot = self.adj
            DispatchQueue.global(qos: .utility).async {
                guard let base = self.loader.loadThumbnailCache(path: item.sourcePath) else { return }
                let rendered = self.renderThumbnail(base: base, for: item,
                                                    overrideAdjustments: snapshot)
                guard let img = ImageIOCodec.toCGImage(rendered) else { return }
                DispatchQueue.main.async {
                    // A late render from an older gesture must not overwrite a newer one.
                    guard self.thumbLiveVersion == version else { return }
                    self.strip.setImage(img, forKey: item.key)
                }
            }
        }
    }

    // ---- photo loading ---------------------------------------------------

    func loadPhoto(_ item: PhotoItem, force: Bool = false) {
        if !force, item.key == loadedKey { return }
        saveCurrentIfDirty()

        loadVersion += 1
        let version = loadVersion
        loadedKey = item.key
        current = item
        // Selecting a different photo ends the undo history, matching Windows: a batch
        // undo is only valid until you move on.
        undoStack.removeAll()
        showOriginal = false
        compareButton.isPrimary = false

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            var (storedAdj, storedExif, _) = AdjustmentXmlStore.loadAll(
                imagePath: item.sourcePath, copyIndex: item.virtualCopyIndex)

            var e = storedExif ?? ExifReader.read(path: item.sourcePath)
            var eOpt: ExifData? = e
            // Back-fill camera colour data into XMLs written before it existed.
            let enriched = self.loader.enrichCameraColor(path: item.sourcePath, exif: &eOpt)
            e = eOpt ?? e

            let a = storedAdj ?? AdjustmentXmlStore.ensureDefault(
                imagePath: item.sourcePath, exif: e, copyIndex: item.virtualCopyIndex)
            if enriched {
                AdjustmentXmlStore.save(imagePath: item.sourcePath, adjustments: a,
                                        copyIndex: item.virtualCopyIndex, exif: e)
            }
            storedAdj = a

            // The proxy is already cached from the folder open, so this is a fast read.
            let proxy = self.loader.loadProxyFloat(path: item.sourcePath)

            DispatchQueue.main.async {
                guard self.loadVersion == version else { return }   // superseded
                self.applyLoaded(item: item, adjustments: a, exif: e, proxy: proxy)
            }
        }
    }

    func applyLoaded(item: PhotoItem, adjustments: ImageAdjustments,
                     exif e: ExifData, proxy p: FloatImageBuffer?) {
        adj = adjustments
        exif = e
        proxy = p
        dirty = false
        setEditorEnabled(true)
        rebindAll()
        // A new photo fits to the window — but only once the render actually arrives,
        // since fitting against a nil image would just leave the scale at 1.
        pendingFit = true
        viewer.setImage(nil, resetView: true)
        scheduler.schedule(immediate: true)
        updateStatus()
    }

    func rebindAll() {
        basicPanel.bind(adj)
        colorPanel.bind(adj)
        detailPanel.bind(adj)
        toolsPanel.bind(adj)
        viewer.adjustments = adj
        infoPanel.exif = exif
        infoPanel.isLegacyPipeline = adj.isLegacyPipeline
    }

    func clearEditor() {
        current = nil
        loadedKey = nil
        proxy = nil
        exif = nil
        adj = ImageAdjustments()
        dirty = false
        undoStack.removeAll()
        viewer.setImage(nil, resetView: true)
        viewer.adjustments = nil
        infoPanel.exif = nil
        histogramView.histogram = nil
        histogramView.mean = nil
        rebindAll()
    }

    func setEditorEnabled(_ on: Bool) {
        for v in [basicPanel, colorPanel, detailPanel, presetPanel, toolsPanel] as [NSView] {
            v.alphaValue = on ? 1 : 0.45
        }
        for b in [resetBcdButton, resetAllButton, undoButton,
                  exportCurrentButton, exportAllButton, compareButton] {
            b.isEnabledButton = on
        }
    }

    // ---- closing ---------------------------------------------------------

    func closeFolder() {
        saveCurrentIfDirty()
        savePreviewList()
        folder = ""
        folderLabel.stringValue = ""
        items = []
        strip.setItems([])
        clearEditor()
        setEditorEnabled(false)
        updateStatus()
    }

    /// 關閉資料夾並刪除快取縮圖 — used after changing the RAW precision setting, since
    /// existing caches do not regenerate on their own.
    func closeFolderAndClearCache() {
        guard !folder.isEmpty else { return }
        let dir = AppPaths.rawTempDir(folder)
        let alert = NSAlert()
        alert.messageText = L.t("關閉資料夾並刪除快取縮圖")
        alert.informativeText = L.f("將刪除 {0} 內的縮圖與預覽快取（調整設定會保留）。", dir)
        alert.addButton(withTitle: L.t("確定"))
        alert.addButton(withTitle: L.t("取消"))
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        let sources = Set(items.map(\.sourcePath))
        closeFolder()
        DispatchQueue.global(qos: .utility).async {
            for s in sources { CacheManager.deleteCacheFiles(s) }
        }
    }

    func savePreviewList() {
        guard !folder.isEmpty else { return }
        PreviewListStore.save(imageFolder: folder, list: previewList)
    }

    func refreshFolder() {
        guard !folder.isEmpty else { return }
        let key = current?.key
        rebuildItems()
        if let key, let i = strip.index(forKey: key) {
            strip.select(index: i)
        } else if !items.isEmpty {
            strip.select(index: 0)
        } else {
            clearEditor()
        }
    }

    /// Rebuild the strip while holding the selection — used after hide/delete, which
    /// clears every thumbnail image.
    func refreshStripKeepSelection() {
        let key = current?.key
        rebuildItems()
        if let key, let i = strip.index(forKey: key) { strip.select(index: i) }
    }
}
