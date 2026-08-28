import AppKit
import AwayRawCore

/// Editing: change notification, undo, multi-selection sync, presets, white balance,
/// and the geometry tools.
extension MainViewController {

    // ---- change pump -----------------------------------------------------

    func onAdjustmentChanged(immediate: Bool = false) {
        dirty = true
        // Any edit gesture cancels the before/after comparison.
        if showOriginal { showOriginal = false; compareButton.isPrimary = false }
        current?.isEdited = !adj.isDefault
        strip.refreshBadges()
        infoPanel.isLegacyPipeline = adj.isLegacyPipeline
        viewer.adjustments = adj
        scheduler.schedule(immediate: immediate)
        liveRefreshCurrentThumbnail()
        markBatchDirty()
        updateStatus()
    }

    // ---- undo ------------------------------------------------------------

    /// Called at the start of every edit gesture. As well as pushing the undo step this
    /// captures the batch sync targets — clicking a thumbnail collapses the selection to
    /// one item *before* the commit runs, so capturing at commit time finds the wrong set.
    func pushUndo() {
        var step = UndoStep(current: adj)

        let selected = strip.selectedItems.filter { $0.key != current?.key }
        if !selected.isEmpty {
            if syncTargets.isEmpty {
                syncTargets = selected
                syncBaseline = adj
            }
            // The first gesture of a batch snapshots every affected photo so undo can
            // restore them together.
            for t in syncTargets {
                if step.others[t.key] == nil {
                    step.others[t.key] = AdjustmentXmlStore.load(imagePath: t.sourcePath,
                                                                 copyIndex: t.virtualCopyIndex)
                                         ?? ImageAdjustments()
                }
            }
        }
        undoStack.append(step)
        if undoStack.count > 50 { undoStack.removeFirst() }
    }

    func doUndo() {
        guard let step = undoStack.popLast() else { return }
        adj = step.current
        dirty = true
        rebindAll()

        if !step.others.isEmpty {
            // Restore the other photos and cancel any sync that has not been flushed.
            for (key, a) in step.others {
                let (path, idx) = PhotoItem.parseKey(key)
                AdjustmentXmlStore.save(imagePath: path, adjustments: a, copyIndex: idx)
                if let item = items.first(where: { $0.key == key }) {
                    item.isEdited = !a.isDefault
                    refreshThumbnail(for: item, adjustments: a)
                }
            }
            syncTargets = []
            syncBaseline = nil
            syncPending = false
            strip.refreshBadges()
        }
        onAdjustmentChanged(immediate: true)
    }

    // ---- multi-selection sync -------------------------------------------

    private func markBatchDirty() {
        guard !syncTargets.isEmpty else { return }
        syncPending = true
        // Show the badge straight away as feedback; the write happens on commit so a
        // drag does not hammer a hundred files.
        for t in syncTargets { t.isEdited = true }
        strip.refreshBadges()
    }

    /// Write the synced fields to the other selected photos. Called from
    /// `saveCurrentIfDirty` — i.e. on photo change, export, folder close, or quit.
    func flushBatchSync() {
        guard syncPending, let baseline = syncBaseline, !syncTargets.isEmpty else {
            syncTargets = []; syncBaseline = nil; syncPending = false
            return
        }
        let edited = adj
        let targets = syncTargets
        syncTargets = []
        syncBaseline = nil
        syncPending = false

        for t in targets {
            var a = AdjustmentXmlStore.load(imagePath: t.sourcePath, copyIndex: t.virtualCopyIndex)
                    ?? ImageAdjustments()
            // Only the fields the user actually moved, and never heal spots or gradients —
            // those are position-specific and must not travel between photos.
            a.applyDelta(edited: edited, baseline: baseline)
            AdjustmentXmlStore.save(imagePath: t.sourcePath, adjustments: a,
                                    copyIndex: t.virtualCopyIndex)
            t.isEdited = !a.isDefault
        }
        refreshThumbnails(targets)
        strip.refreshBadges()
    }

    func saveCurrentIfDirty() {
        if dirty, let item = current {
            AdjustmentXmlStore.save(imagePath: item.sourcePath, adjustments: adj,
                                    copyIndex: item.virtualCopyIndex, exif: exif)
            item.isEdited = !adj.isDefault
            // A saved photo gets its final thumbnail; this also supersedes any queued
            // live redraw.
            thumbLiveTimer?.invalidate()
            thumbLiveVersion += 1
            refreshThumbnail(for: item, adjustments: adj)
            dirty = false
        }
        flushBatchSync()
    }

    // ---- resets ----------------------------------------------------------

    func resetAllAdjustments() { resetSelected { $0.resetAll() } }
    func resetBasicColorDetail() { resetSelected { $0.resetBasicColorDetail() } }

    /// Apply a reset to the current photo live, and to every other selected photo
    /// immediately (load → reset → save) so their badges clear at once.
    private func resetSelected(_ reset: (inout ImageAdjustments) -> Void) {
        guard current != nil else { return }
        pushUndo()
        reset(&adj)
        rebindAll()
        onAdjustmentChanged(immediate: true)

        let others = strip.selectedItems.filter { $0.key != current?.key }
        guard !others.isEmpty else { return }
        for t in others {
            var a = AdjustmentXmlStore.load(imagePath: t.sourcePath, copyIndex: t.virtualCopyIndex)
                    ?? ImageAdjustments()
            reset(&a)
            AdjustmentXmlStore.save(imagePath: t.sourcePath, adjustments: a,
                                    copyIndex: t.virtualCopyIndex)
            t.isEdited = !a.isDefault
        }
        refreshThumbnails(others)
        strip.refreshBadges()
        // A reset supersedes any queued slider sync.
        syncTargets = []
        syncBaseline = nil
        syncPending = false
    }

    // ---- tools -----------------------------------------------------------

    func onToolChanged(_ mode: ToolMode) {
        viewer.tool = mode
        // The gradient and heal overlays live in the pre-geometry frame, and crop needs
        // the full frame behind the box, so the render context changes with the tool.
        scheduler.schedule(immediate: true)
        view.window?.invalidateCursorRects(for: viewer)
        updateStatus()
    }

    func addGradient() {
        viewer.addGradient()
        adj = viewer.adjustments ?? adj
        toolsPanel.adjustments = adj
        toolsPanel.rebindGradient()
        onAdjustmentChanged(immediate: true)
    }

    func rotate(clockwise: Bool) {
        pushUndo()
        adj.rotation = clockwise ? adj.rotation.next : adj.rotation.previous
        toolsPanel.adjustments = adj
        onAdjustmentChanged(immediate: true)
    }

    // ---- white balance ---------------------------------------------------

    func whiteBalancePicked(nx: Double, ny: Double) {
        guard let p = proxy else { return }
        let x = Int(nx * Double(p.width)), y = Int(ny * Double(p.height))
        guard x >= 0, y >= 0, x < p.width, y < p.height else { return }
        guard let patch = ImageStats.patchMean(p, x: x, y: y, radius: 3) else { return }

        pushUndo()
        if !adj.isLegacyPipeline, let cam = exif?.camera, cam.isValid,
           let mul = ColorScience.neutralizingCamMul(cam, r: patch.r, g: patch.g, b: patch.b,
                                                     reference: .decode),
           let kt = ColorScience.camMulToKelvinTint(cam, mul: mul) {
            // With camera colour data this is an exact solve.
            adj.temperature = min(max(kt.kelvin, ColorScience.minKelvin), ColorScience.maxKelvin)
            adj.tint = min(max(kt.tint, -100), 100)
        } else {
            // Otherwise search the black-body approximation — in linear for v1, on the
            // encoded values for legacy, matching what each actually multiplies.
            let (k, t) = searchNeutral(r: patch.r, g: patch.g, b: patch.b,
                                       linear: !adj.isLegacyPipeline)
            adj.temperature = k
            adj.tint = t
        }

        // One click is all the picker is for.
        pickerActive = false
        viewer.whiteBalancePickerActive = false
        colorPanel.setPickerActive(false)
        colorPanel.bind(adj)
        onAdjustmentChanged(immediate: true)
    }

    /// Brute-force the Kelvin/tint pair whose multipliers make the sampled patch neutral.
    private func searchNeutral(r: Double, g: Double, b: Double, linear: Bool)
        -> (kelvin: Double, tint: Double) {
        // The sample arrives linear; the legacy path multiplies encoded values, so
        // re-encode before comparing there.
        let (sr, sg, sb) = linear
            ? (r, g, b)
            : (Double(ColorScience.encode(Float(r))),
               Double(ColorScience.encode(Float(g))),
               Double(ColorScience.encode(Float(b))))

        var bestK = 5200.0, bestT = 0.0, bestErr = Double.greatestFiniteMagnitude
        var k = ColorScience.minKelvin
        while k <= ColorScience.maxKelvin {
            var t = -100.0
            while t <= 100.0 {
                let m = ImageProcessor.whiteBalanceMultipliers(temperature: k, tint: t)
                let rr = sr * m.r, gg = sg * m.g, bb = sb * m.b
                // Neutral means the three channels agree.
                let mean = (rr + gg + bb) / 3
                guard mean > 1e-9 else { t += 5; continue }
                let err = ((rr - mean) * (rr - mean) + (gg - mean) * (gg - mean)
                           + (bb - mean) * (bb - mean)) / (mean * mean)
                if err < bestErr { bestErr = err; bestK = k; bestT = t }
                t += 5
            }
            k += 50
        }
        return (bestK, bestT)
    }

    /// 拍攝時設定 — return temperature/tint to what the camera recorded.
    func useAsShotWhiteBalance() {
        guard current != nil else { return }
        pushUndo()
        if let cam = exif?.camera, cam.isValid, let shot = ColorScience.asShot(cam) {
            adj.temperature = min(max(shot.kelvin, ColorScience.minKelvin), ColorScience.maxKelvin)
            adj.tint = shot.tint
        } else if let e = exif, e.hasAsShotWhiteBalance {
            adj.temperature = e.colorTemperature
            adj.tint = 0
        } else {
            adj.temperature = 5200
            adj.tint = 0
        }
        colorPanel.bind(adj)
        onAdjustmentChanged(immediate: true)
    }

    // ---- presets ---------------------------------------------------------

    func applyPresetToSelection(_ name: String) {
        guard current != nil else { return }
        pushUndo()
        _ = PresetStore.apply(name: name, to: &adj)
        rebindAll()
        onAdjustmentChanged(immediate: true)

        let others = strip.selectedItems.filter { $0.key != current?.key }
        guard !others.isEmpty else { return }
        for t in others {
            var a = AdjustmentXmlStore.load(imagePath: t.sourcePath, copyIndex: t.virtualCopyIndex)
                    ?? ImageAdjustments()
            _ = PresetStore.apply(name: name, to: &a)
            AdjustmentXmlStore.save(imagePath: t.sourcePath, adjustments: a,
                                    copyIndex: t.virtualCopyIndex)
            t.isEdited = !a.isDefault
        }
        refreshThumbnails(others)
        strip.refreshBadges()
    }

    // ---- pipeline version ------------------------------------------------

    func upgradeSelectedPipeline() {
        let targets = strip.selectedItems.filter {
            let a = AdjustmentXmlStore.load(imagePath: $0.sourcePath, copyIndex: $0.virtualCopyIndex)
            return a?.isLegacyPipeline ?? false
        }
        guard !targets.isEmpty else {
            statusLabel.stringValue = L.t("選取的照片已是最新處理版本")
            return
        }

        let alert = NSAlert()
        alert.messageText = L.t("升級處理版本")
        alert.informativeText = L.f("將 {0} 張照片改用新的色彩管線，滑桿值會換算成盡量接近目前的樣子。",
                                    targets.count)
        alert.addButton(withTitle: L.t("確定"))
        alert.addButton(withTitle: L.t("取消"))
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        for t in targets {
            _ = PipelineUpgrade.upgrade(imagePath: t.sourcePath,
                                        copyIndex: t.virtualCopyIndex, loader: loader)
        }
        refreshThumbnails(targets)
        strip.refreshBadges()
        // Reload the current photo so it picks up its converted values.
        if let cur = current, targets.contains(where: { $0.key == cur.key }) {
            dirty = false
            loadPhoto(cur, force: true)
        }
        statusLabel.stringValue = L.f("已升級 {0} 張照片的處理版本", targets.count)
    }

    // ---- comparison ------------------------------------------------------

    func toggleShowOriginal() {
        guard current != nil else { return }
        showOriginal.toggle()
        compareButton.isPrimary = showOriginal
        scheduler.schedule(immediate: true)
        updateStatus()
    }

    /// Esc: drop the picker, or fall back to no tool.
    func cancelPickerOrTool() {
        if pickerActive {
            pickerActive = false
            viewer.whiteBalancePickerActive = false
            colorPanel.setPickerActive(false)
            updateStatus()
            return
        }
        if viewer.tool != .none {
            viewer.tool = .none
            scheduler.schedule(immediate: true)
            updateStatus()
        }
    }

    func stepSelection(_ delta: Int) {
        guard !items.isEmpty else { return }
        let i = strip.currentIndex < 0 ? 0 : strip.currentIndex + delta
        strip.select(index: min(max(i, 0), items.count - 1))
    }

    // ---- status ----------------------------------------------------------

    func updateStatus() {
        var parts: [String] = []
        if let item = current {
            parts.append(item.displayName)
            parts.append("\(Int(viewer.zoomPercent.rounded()))%")
        }
        if loader.libRawAvailable && AppPaths.isRaw(current?.sourcePath ?? "") {
            parts.append("LibRaw \(LibRawBridge.version)")
        }
        if adj.isLegacyPipeline && current != nil { parts.append(L.t("舊版處理")) }
        if lastRenderUsedGpu && current != nil { parts.append(L.t("GPU 算圖")) }
        if showOriginal { parts.append(L.t("對照原圖")) }
        if pickerActive { parts.append(L.t("白平衡選擇器")) }
        let n = strip.selectedIndices.count
        if n > 1 { parts.append(L.f("已選 {0} 張", n)) }
        statusLabel.stringValue = parts.joined(separator: "  ·  ")
    }
}

// MARK: - Keyboard

extension MainViewController {

    /// Window-level shortcuts. Mirrors the Windows key handling: Esc cancels the picker
    /// or the active tool, the arrow keys step the selection, and `\` toggles the
    /// before/after comparison.
    override func keyDown(with event: NSEvent) {
        // A field being edited owns the keyboard.
        if view.window?.firstResponder is NSText { super.keyDown(with: event); return }

        switch event.keyCode {
        case 53:                                    // esc
            cancelPickerOrTool()
        case 123:                                   // left
            stepSelection(-1)
        case 124:                                   // right
            stepSelection(1)
        case 42 where event.modifierFlags.intersection(.deviceIndependentFlagsMask).isEmpty:
            toggleShowOriginal()                    // backslash
        default:
            // Number keys pick a tool, matching the ribbon order.
            switch event.charactersIgnoringModifiers {
            case "c": toolsPanel.tabs.selectedIndex = 0; onToolChanged(.crop)
            case "g": toolsPanel.tabs.selectedIndex = 1; onToolChanged(.gradient)
            case "h": toolsPanel.tabs.selectedIndex = 2; onToolChanged(.heal)
            default: super.keyDown(with: event)
            }
        }
    }
}
