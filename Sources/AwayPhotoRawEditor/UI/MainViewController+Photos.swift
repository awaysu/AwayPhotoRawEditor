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
        panel.message = L.t("選擇相片資料夾")
        panel.prompt = L.t("確定")
        if !folder.isEmpty { panel.directoryURL = URL(fileURLWithPath: folder) }
        panel.begin { [weak self] resp in
            guard resp == .OK, let url = panel.url else { return }
            self?.openFolder(url.path)
        }
    }

    func openFolder(_ path: String) {
        saveCurrentIfDirty()
        // Nothing is current until the new folder's first photo loads: an edit made
        // during cache generation must not land on the previous folder's item, and a
        // stale loadedKey would make the first selection a no-op when the same folder is
        // reopened from the history.
        current = nil
        loadedKey = nil
        undoStack.removeAll()
        redoStack.removeAll()
        folder = path
        folderLabel.stringValue = path
        // A diagnostic screenshot run must not rewrite the user's last-opened folder.
        if !Shot.headless {
            settings.lastFolder = path
            settings.pushRecentFolder(path)
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
            // Unless the user already picked something while the caches were building.
            if !self.items.isEmpty, self.strip.currentIndex < 0 {
                self.strip.select(index: 0)
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
        isLoading = true
        proxy = nil               // nothing to render, pick or crop against until it lands
        // Selecting a different photo ends the undo history, matching Windows: a batch
        // undo is only valid until you move on. The redo branch goes too — replaying it
        // would paste the previous photo's edits onto this one.
        undoStack.removeAll()
        redoStack.removeAll()
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
            let source = self.loader.proxyDecodeSource(path: item.sourcePath)

            DispatchQueue.main.async {
                guard self.loadVersion == version else { return }   // superseded
                self.proxySource = source
                self.applyLoaded(item: item, adjustments: a, exif: e, proxy: proxy)
            }
        }
    }

    func applyLoaded(item: PhotoItem, adjustments: ImageAdjustments,
                     exif e: ExifData, proxy p: FloatImageBuffer?) {
        isLoading = false
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
        colorPanel.setTemperatureMode(isRaw: current == nil || AppPaths.isRaw(current!.sourcePath))
        colorPanel.bind(adj)
        detailPanel.bind(adj)
        toolsPanel.bind(adj)
        viewer.adjustments = adj
        infoPanel.exif = exif
        infoPanel.isLegacyPipeline = adj.isLegacyPipeline
    }

    func clearEditor() {
        isLoading = false
        current = nil
        loadedKey = nil
        proxy = nil
        exif = nil
        adj = ImageAdjustments()
        dirty = false
        undoStack.removeAll()
        redoStack.removeAll()
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
        // The user closed the folder on purpose → the next launch stays closed instead of
        // auto-reopening it (same as the Windows build's CloseFolder). A diagnostic run
        // never touches the user's settings file.
        settings.lastFolder = ""
        if !Shot.headless { settings.save() }
        items = []
        strip.setItems([])
        clearEditor()
        setEditorEnabled(false)
        updateStatus()
    }

    /// 關閉資料夾並刪除快取縮圖 — removes the folder's whole `RAW_TEMP` (thumbnails, proxies
    /// **and** the adjustment XMLs / preview_list). Deliberately more than the Windows build,
    /// which keeps the XMLs: the user wants the folder gone. It goes to the Trash so a slip
    /// is recoverable; volumes without a Trash fall back to a plain delete.
    func closeFolderAndClearCache() {
        guard !folder.isEmpty else { return }
        let dir = AppPaths.rawTempDir(folder)
        let alert = NSAlert()
        alert.messageText = L.t("關閉資料夾並刪除快取縮圖")
        alert.informativeText = L.f("將刪除整個 {0} 資料夾，包含所有調整設定、隱藏狀態與虛擬副本。此操作無法在程式內復原。", dir)
        alert.alertStyle = .warning
        alert.addButton(withTitle: L.t("刪除"))
        alert.addButton(withTitle: L.t("取消"))
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        closeFolder()
        let removed = Self.removeRawTempDir(dir)
        statusLabel.stringValue = removed
            ? L.f("已關閉資料夾並刪除 {0}", dir)
            : L.f("無法刪除 {0}", dir)
    }

    /// Trash (or, failing that, delete) a RAW_TEMP directory. Returns true when it is gone.
    @discardableResult
    static func removeRawTempDir(_ dir: String, toTrash: Bool = true) -> Bool {
        let fm = FileManager.default
        guard fm.fileExists(atPath: dir) else { return true }
        let url = URL(fileURLWithPath: dir)
        if !toTrash || (try? fm.trashItem(at: url, resultingItemURL: nil)) == nil {
            try? fm.removeItem(at: url)
        }
        return !fm.fileExists(atPath: dir)
    }

    func savePreviewList() {
        guard !folder.isEmpty else { return }
        PreviewListStore.save(imageFolder: folder, list: previewList)
    }

    func refreshFolder() {
        guard !folder.isEmpty else { return }
        saveCurrentIfDirty()
        refreshStripKeepSelection()
        statusLabel.stringValue = L.t("已重新整理資料夾")
    }

    /// Rebuild the strip while holding the selection — used after hide/delete, which
    /// clears every thumbnail image.
    ///
    /// `rebuildItems` creates new PhotoItem objects, so `current` is re-pointed at the
    /// one with the same key; otherwise badge updates would land on an orphan. When the
    /// current photo is gone (hidden, deleted) the nearest remaining one is selected, and
    /// an empty strip clears the editor.
    func refreshStripKeepSelection() {
        let key = current?.key
        let oldIndex = strip.currentIndex
        rebuildItems()
        if items.isEmpty { clearEditor(); setEditorEnabled(false); updateStatus(); return }
        if let key, let i = strip.index(forKey: key) {
            current = items[i]
            strip.select(index: i)
        } else {
            // The photo that was current is no longer listed.
            loadedKey = nil
            current = nil
            strip.select(index: min(max(oldIndex, 0), items.count - 1))
        }
    }

    /// 隱藏且不輸出 for every selected photo (the context menu and ⌫ share this).
    func hideSelected() {
        guard !folder.isEmpty else { return }
        let targets = strip.selectedItems.filter { !$0.isHidden }
        guard !targets.isEmpty else { return }
        // The current photo's edits must survive it leaving the strip.
        saveCurrentIfDirty()
        for t in targets where !previewList.hidden.contains(t.key) {
            previewList.hidden.append(t.key)
            t.isHidden = true
        }
        savePreviewList()
        if settings.showHiddenPhotos { strip.refreshBadges() } else { refreshStripKeepSelection() }
        statusLabel.stringValue = L.t("已隱藏（不輸出）")
    }

    func unhideSelected() {
        let targets = strip.selectedItems.filter { $0.isHidden }
        guard !targets.isEmpty else { return }
        for t in targets {
            previewList.hidden.removeAll { $0 == t.key }
            t.isHidden = false
        }
        savePreviewList()
        strip.refreshBadges()
        statusLabel.stringValue = L.t("已取消隱藏")
    }

    /// Switch 不顯示隱藏 / 顯示全部, rebuilding the strip and keeping the selection where
    /// it can be kept.
    func setShowHiddenMode(_ showAll: Bool) {
        guard settings.showHiddenPhotos != showAll else { return }
        settings.showHiddenPhotos = showAll
        if !Shot.headless { settings.save() }
        guard !folder.isEmpty else { return }
        saveCurrentIfDirty()
        refreshStripKeepSelection()
    }

    /// Un-hide everything hidden via 隱藏且不輸出.
    func restoreHiddenPhotos() {
        guard !folder.isEmpty else { return }
        guard !previewList.hidden.isEmpty else {
            statusLabel.stringValue = L.t("沒有已隱藏的照片")
            return
        }
        let n = previewList.hidden.count
        saveCurrentIfDirty()
        previewList.hidden.removeAll()
        savePreviewList()
        refreshStripKeepSelection()
        statusLabel.stringValue = L.f("已還原 {0} 張隱藏的照片", n)
    }
}
