import AppKit
import AwayRawCore

/// The ☰ application menu, the thumbnail context menu, and export.
extension MainViewController {

    // ---- app menu --------------------------------------------------------

    func showAppMenu() {
        let menu = NSMenu()
        func add(_ title: String, _ action: Selector, enabled: Bool = true) {
            let item = NSMenuItem(title: L.t(title), action: action, keyEquivalent: "")
            item.target = self
            item.isEnabled = enabled
            menu.addItem(item)
        }
        add("開啟資料夾", #selector(menuOpenFolder))
        add("重新整理資料夾", #selector(menuRefreshFolder), enabled: !folder.isEmpty)
        add("關閉資料夾", #selector(menuCloseFolder), enabled: !folder.isEmpty)
        add("關閉資料夾並刪除快取縮圖", #selector(menuCloseAndClearCache), enabled: !folder.isEmpty)
        menu.addItem(.separator())
        add("匯出目前照片", #selector(menuExportCurrent), enabled: current != nil)
        add("匯出選取照片", #selector(menuExportSelection), enabled: !strip.selectedItems.isEmpty)
        add("匯出全部照片", #selector(menuExportAll), enabled: !items.isEmpty)
        menu.addItem(.separator())
        add("編輯風格檔…", #selector(menuPresetEditor))
        add("設定…", #selector(menuSettings))
        add("字體大小…", #selector(menuFontSize))
        menu.addItem(.separator())
        add(settings.showHiddenPhotos ? "不顯示隱藏" : "顯示全部", #selector(menuToggleHidden))
        add("恢復所有隱藏的照片", #selector(menuRestoreHidden), enabled: !previewList.hidden.isEmpty)
        menu.addItem(.separator())
        add("關於", #selector(menuAbout))

        let p = NSPoint(x: menuButton.bounds.minX, y: menuButton.bounds.maxY)
        menu.popUp(positioning: nil, at: p, in: menuButton)
    }

    @objc func menuOpenFolder() { pickFolder() }
    @objc func menuRefreshFolder() { refreshFolder() }
    @objc func menuCloseFolder() { closeFolder() }
    @objc func menuCloseAndClearCache() { closeFolderAndClearCache() }
    @objc func menuExportCurrent() { exportCurrent() }
    @objc func menuExportSelection() { exportPhotos(strip.selectedItems) }
    @objc func menuExportAll() { exportAll() }
    @objc func menuPresetEditor() { showPresetEditor() }
    @objc func menuSettings() { showSettings() }
    @objc func menuFontSize() { showFontSizeDialog() }
    @objc func menuAbout() { showAbout() }

    @objc func menuToggleHidden() {
        settings.showHiddenPhotos.toggle()
        settings.save()
        refreshStripKeepSelection()
    }

    @objc func menuRestoreHidden() {
        previewList.hidden.removeAll()
        savePreviewList()
        refreshStripKeepSelection()
    }

    // ---- thumbnail context menu -----------------------------------------

    func showThumbnailMenu(index: Int, event: NSEvent) {
        guard index >= 0, index < items.count else { return }
        let item = items[index]
        let menu = NSMenu()
        func add(_ title: String, _ action: Selector, enabled: Bool = true) {
            let m = NSMenuItem(title: L.t(title), action: action, keyEquivalent: "")
            m.target = self
            m.representedObject = item
            m.isEnabled = enabled
            menu.addItem(m)
        }
        add("複製設定", #selector(menuCopySettings(_:)))
        add("貼上設定", #selector(menuPasteSettings(_:)), enabled: copiedSettings != nil)
        menu.addItem(.separator())
        add("建立虛擬副本", #selector(menuCreateVirtualCopy(_:)))
        menu.addItem(.separator())
        add(item.isHidden ? "取消隱藏" : "從預覽列隱藏", #selector(menuToggleHide(_:)))
        add("刪除檔案…", #selector(menuDeleteFile(_:)))
        menu.addItem(.separator())
        add("升級處理版本", #selector(menuUpgradePipeline(_:)))
        NSMenu.popUpContextMenu(menu, with: event, for: strip)
    }

    @objc func menuCopySettings(_ sender: NSMenuItem) {
        guard let item = sender.representedObject as? PhotoItem else { return }
        // Take the live values when it is the photo being edited, so an unsaved change
        // is what gets copied.
        copiedSettings = (item.key == current?.key)
            ? adj
            : AdjustmentXmlStore.load(imagePath: item.sourcePath, copyIndex: item.virtualCopyIndex)
        copySourceKey = item.key
        for it in items { it.isCopySettingsSource = (it.key == copySourceKey) }
        strip.refreshBadges()
        statusLabel.stringValue = L.f("已複製「{0}」的設定", item.fileName)
    }

    @objc func menuPasteSettings(_ sender: NSMenuItem) {
        guard let source = copiedSettings else { return }
        let targets = strip.selectedItems
        guard !targets.isEmpty else { return }
        for t in targets {
            var a = source
            // Heal spots are position-specific; they never travel between photos.
            a.healSpots = []
            if t.key == current?.key {
                pushUndo()
                let keepVersion = adj.pipelineVersion
                adj = a
                adj.pipelineVersion = keepVersion
                rebindAll()
                onAdjustmentChanged(immediate: true)
            } else {
                let existing = AdjustmentXmlStore.load(imagePath: t.sourcePath,
                                                       copyIndex: t.virtualCopyIndex)
                a.pipelineVersion = existing?.pipelineVersion ?? ImageAdjustments.currentPipelineVersion
                AdjustmentXmlStore.save(imagePath: t.sourcePath, adjustments: a,
                                        copyIndex: t.virtualCopyIndex)
                t.isEdited = !a.isDefault
            }
        }
        refreshThumbnails(targets.filter { $0.key != current?.key })
        strip.refreshBadges()
    }

    @objc func menuCreateVirtualCopy(_ sender: NSMenuItem) {
        guard let item = sender.representedObject as? PhotoItem else { return }
        saveCurrentIfDirty()
        // Copy indices are per source file; take the next free one.
        let used = previewList.virtualCopies.filter { $0.path == item.sourcePath }.map(\.index)
        let next = (used.max() ?? 0) + 1
        previewList.virtualCopies.append(VirtualCopyEntry(path: item.sourcePath, index: next))
        savePreviewList()

        // Seed the copy with the source's current settings.
        let source = AdjustmentXmlStore.load(imagePath: item.sourcePath,
                                             copyIndex: item.virtualCopyIndex)
                     ?? ImageAdjustments()
        let e = AdjustmentXmlStore.loadExif(imagePath: item.sourcePath,
                                            copyIndex: item.virtualCopyIndex)
        AdjustmentXmlStore.save(imagePath: item.sourcePath, adjustments: source,
                                copyIndex: next, exif: e)

        refreshStripKeepSelection()
        if let i = strip.index(forKey: "\(item.sourcePath)|copy:\(next)") {
            strip.select(index: i)
        }
    }

    @objc func menuToggleHide(_ sender: NSMenuItem) {
        guard let item = sender.representedObject as? PhotoItem else { return }
        saveCurrentIfDirty()
        if item.isHidden {
            previewList.hidden.removeAll { $0 == item.key }
        } else {
            for t in strip.selectedItems where !previewList.hidden.contains(t.key) {
                previewList.hidden.append(t.key)
            }
        }
        savePreviewList()
        refreshStripKeepSelection()
    }

    @objc func menuDeleteFile(_ sender: NSMenuItem) {
        guard let item = sender.representedObject as? PhotoItem else { return }
        let targets = strip.selectedItems
        let alert = NSAlert()
        alert.alertStyle = .warning
        if item.isVirtualCopy {
            alert.messageText = L.t("刪除虛擬副本")
            alert.informativeText = L.f("將刪除「{0}」。原始檔案不會變動。", item.displayName)
        } else {
            alert.messageText = L.t("刪除檔案")
            alert.informativeText = L.f("將把 {0} 個檔案移到垃圾桶，連同其快取與調整設定。",
                                        targets.count)
        }
        alert.addButton(withTitle: L.t("刪除"))
        alert.addButton(withTitle: L.t("取消"))
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        for t in targets {
            if t.isVirtualCopy {
                previewList.virtualCopies.removeAll {
                    $0.path == t.sourcePath && $0.index == t.virtualCopyIndex
                }
                AdjustmentXmlStore.delete(imagePath: t.sourcePath, copyIndex: t.virtualCopyIndex)
            } else {
                AdjustmentXmlStore.delete(imagePath: t.sourcePath)
                CacheManager.deleteCacheFiles(t.sourcePath)
                // The Trash, not an unlink — deleting someone's photograph outright is
                // not a thing this app should do.
                try? FileManager.default.trashItem(at: URL(fileURLWithPath: t.sourcePath),
                                                  resultingItemURL: nil)
            }
            previewList.hidden.removeAll { $0 == t.key }
        }
        savePreviewList()
        loadedKey = nil
        current = nil
        refreshFolder()
    }

    @objc func menuUpgradePipeline(_ sender: NSMenuItem) { upgradeSelectedPipeline() }

    // ---- export ----------------------------------------------------------

    func exportCurrent() {
        guard let item = current else { return }
        exportPhotos([item])
    }

    func exportAll() { exportPhotos(items) }

    func exportPhotos(_ targets: [PhotoItem]) {
        guard !targets.isEmpty else { return }
        saveCurrentIfDirty()

        let dialog = ExportWindowController(settings: exportSettings, count: targets.count)
        dialog.onConfirm = { [weak self] settings in
            guard let self else { return }
            self.exportSettings = settings
            settings.save()
            self.runExport(targets, settings)
        }
        dialog.show(over: view.window)
    }

    private func runExport(_ targets: [PhotoItem], _ settings: ExportSettings) {
        let progress = ProgressWindowController(title: L.t("匯出照片"))
        progress.show(over: view.window)
        let token = CancelToken()
        progress.onCancel = { token.cancel() }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            do {
                let written = try Exporter.export(items: targets, settings: settings,
                                                  loader: self.loader,
                                                  progress: { p in
                    DispatchQueue.main.async {
                        progress.update(fraction: Double(p.done) / Double(max(1, p.total)),
                                        message: p.message)
                    }
                }, token: token)
                DispatchQueue.main.async {
                    progress.finish(message: L.f("已匯出 {0} 個檔案", written.count))
                    self.statusLabel.stringValue = L.f("已匯出 {0} 個檔案", written.count)
                }
            } catch is CancellationError {
                DispatchQueue.main.async { progress.close() }
            } catch {
                DispatchQueue.main.async {
                    progress.close()
                    let alert = NSAlert()
                    alert.alertStyle = .warning
                    alert.messageText = L.t("匯出失敗")
                    alert.informativeText = error.localizedDescription
                    alert.runModal()
                }
            }
        }
    }
}
