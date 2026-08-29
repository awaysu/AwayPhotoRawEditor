import AppKit
import AwayRawCore

/// `--uitest <folder> [report]` — drives the real MainViewController through the editing
/// flows a screenshot cannot exercise: undo/redo, multi-selection batch sync, presets,
/// virtual copies, hide/unhide, crop aspect, copy/paste, delete. The folder is copied to a
/// temporary location first, so nothing the test does touches the caller's files.
///
/// Each step waits for the asynchronous load it caused, then asserts on controller state
/// and on what actually reached disk — the sidecar XML is the ground truth for batch
/// sync and undo of others.
enum UITest {

    private static var lines: [String] = []
    private static var failures = 0
    private static var reportPath: String?

    private static func line(_ s: String) { print(s); lines.append(s) }

    private static func check(_ name: String, _ ok: Bool, _ detail: String = "") {
        line("  [\(ok ? "OK" : "失敗")] \(name)\(detail.isEmpty ? "" : " — " + detail)")
        if !ok { failures += 1 }
    }

    /// Poll on the main thread until `cond` holds, then continue; a timeout fails the step.
    private static func waitUntil(_ what: String, timeout: TimeInterval = 30,
                                  _ cond: @escaping () -> Bool, then: @escaping () -> Void) {
        let deadline = Date().addingTimeInterval(timeout)
        func poll() {
            if cond() { then(); return }
            if Date() > deadline {
                check("等待：\(what)", false, "逾時 \(Int(timeout)) s")
                finish()
                return
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05, execute: poll)
        }
        poll()
    }

    private static func after(_ seconds: Double, _ body: @escaping () -> Void) {
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: body)
    }

    private static func finish() {
        line("")
        line(failures == 0 ? "全部通過 ✅" : "有 \(failures) 項失敗 ❌")
        if let p = reportPath {
            try? lines.joined(separator: "\n").appending("\n").write(toFile: p, atomically: true, encoding: .utf8)
        }
        exit(failures == 0 ? 0 : 1)
    }

    /// A menu item carrying the photo the context-menu actions expect.
    private static func item(for photo: PhotoItem) -> NSMenuItem {
        let m = NSMenuItem()
        m.representedObject = photo
        return m
    }

    static func run(controller c: MainViewController, folder source: String, report: String?) {
        reportPath = report
        line("=== UI 流程測試 ===")

        // Work on a private copy.
        let tmp = (NSTemporaryDirectory() as NSString).appendingPathComponent("awpr_uitest_\(UUID().uuidString)")
        do {
            try FileManager.default.copyItem(atPath: source, toPath: tmp)
            try? FileManager.default.removeItem(atPath: AppPaths.rawTempDir(tmp))
        } catch {
            check("複製測試資料夾", false, error.localizedDescription); finish(); return
        }
        line("資料夾: \(tmp)")

        c.openFolder(tmp)
        waitUntil("開資料夾並載入第一張", { c.items.count >= 3 && c.current != nil && !c.isLoading && c.proxy != nil }) {
            let n = c.items.count
            line("")
            line("[1] 載入")
            check("照片數 \(n)", n >= 3)
            check("第一張為目前照片", c.strip.currentIndex == 0 && c.current?.key == c.items[0].key)
            check("proxy 已載入", c.proxy != nil)
            stepEdit(c)
        }
    }

    // ---- 2: single edit, undo, redo ---------------------------------------

    private static func stepEdit(_ c: MainViewController) {
        line("")
        line("[2] 單張編輯 / 復原 / 重做")
        let slider = c.basicPanel.sliders[0]           // 曝光
        slider.onEditBegin?()
        slider.value = 1.0
        check("曝光滑桿 → adj", c.adj.exposure == 1.0, "\(c.adj.exposure)")
        check("dirty", c.dirty)
        check("edited 標記", c.current?.isEdited == true)
        check("undo 有一步", c.undoStack.count == 1)

        c.doUndo()
        check("復原 → 曝光 0", c.adj.exposure == 0, "\(c.adj.exposure)")
        check("redo 有一步", c.redoStack.count == 1)
        c.doRedo()
        check("重做 → 曝光 1", c.adj.exposure == 1.0, "\(c.adj.exposure)")

        // Move to the next photo: the edit must reach disk, and the redo branch must die.
        let first = c.items[0]
        c.stepSelection(1)
        // Edit while the load is still in flight: it must be dropped, never applied to
        // the previous photo's adjustments.
        check("載入中", c.isLoading)
        slider.onEditBegin?(); slider.value = 3.0
        check("載入中的編輯被丟棄", !c.dirty && c.undoStack.isEmpty)
        waitUntil("切到第二張", { c.current?.key == c.items[1].key && !c.isLoading }) {
            let saved = AdjustmentXmlStore.load(imagePath: first.sourcePath)
            check("切圖時寫入 XML", saved?.exposure == 1.0, "\(saved?.exposure ?? -1)")
            check("切圖後 redo 清空", c.redoStack.isEmpty)
            check("切圖後 undo 清空", c.undoStack.isEmpty)
            stepBatch(c)
        }
    }

    // ---- 3: batch sync + batch undo -------------------------------------

    private static func stepBatch(_ c: MainViewController) {
        line("")
        line("[3] 多選批次同步")
        let n = c.items.count
        c.strip.selectAll()
        check("全選 \(n) 張", c.strip.selectedItems.count == n)
        check("目前仍是第二張", c.current?.key == c.items[1].key)

        let contrast = c.basicPanel.sliders[1]
        contrast.onEditBegin?()
        contrast.value = 30
        check("批次目標 = 其他 \(n - 1) 張", c.syncTargets.count == n - 1, "\(c.syncTargets.count)")
        check("其他照片先亮 edited", c.items[0].isEdited && c.items[2].isEdited)

        c.saveCurrentIfDirty()                          // the commit point
        let a0 = AdjustmentXmlStore.load(imagePath: c.items[0].sourcePath)
        let a2 = AdjustmentXmlStore.load(imagePath: c.items[2].sourcePath)
        check("flush：第一張對比 30", a0?.contrast == 30, "\(a0?.contrast ?? -1)")
        check("flush 只動改過的欄位：第一張曝光仍 1", a0?.exposure == 1.0, "\(a0?.exposure ?? -1)")
        check("flush：第三張對比 30", a2?.contrast == 30, "\(a2?.contrast ?? -1)")
        check("flush 後批次狀態清空", c.syncTargets.isEmpty && !c.syncPending)

        // The batch step is still on the undo stack: undo must revert the others too.
        c.doUndo()
        let b0 = AdjustmentXmlStore.load(imagePath: c.items[0].sourcePath)
        check("批次復原：目前對比 0", c.adj.contrast == 0)
        check("批次復原：第一張對比回 0", b0?.contrast == 0, "\(b0?.contrast ?? -1)")
        check("批次復原：第一張曝光保留 1", b0?.exposure == 1.0, "\(b0?.exposure ?? -1)")
        after(0.2) { stepPreset(c) }
    }

    // ---- 4: preset, reset ------------------------------------------------

    private static func stepPreset(_ c: MainViewController) {
        line("")
        line("[4] 風格檔 / 重設")
        c.strip.deselectAll()
        check("取消全選後只剩目前", c.strip.selectedItems.count == 1)
        let tempBefore = c.adj.temperature, tintBefore = c.adj.tint
        c.applyPresetToSelection("風景")
        check("套用風景：對比 18", c.adj.contrast == 18)
        check("套用風景：鮮豔度 28", c.adj.vibrance == 28)
        // A RAW is seeded to its as-shot Kelvin, so "untouched" means unchanged, not 5200.
        check("風格檔不碰色溫／色調", c.adj.temperature == tempBefore && c.adj.tint == tintBefore,
              "\(tempBefore) → \(c.adj.temperature)")
        c.resetAllAdjustments()
        check("全部重設 → 預設", c.adj.isDefault)
        check("重設後 edited 熄滅", c.current?.isEdited == false)
        stepCropAndRotate(c)
    }

    // ---- 5: crop aspect, rotate ------------------------------------------

    private static func stepCropAndRotate(_ c: MainViewController) {
        line("")
        line("[5] 裁切比例 / 旋轉")
        c.applyCropAspect("1:1")
        let sq = c.adj
        check("1:1 框為正方（像素）", abs(sq.cropWidth * Double(c.proxy!.width) - sq.cropHeight * Double(c.proxy!.height)) < 2,
              "\(sq.cropWidth) x \(sq.cropHeight)")
        check("1:1 置中", abs(sq.cropX - (1 - sq.cropWidth) / 2) < 1e-9 && abs(sq.cropY - (1 - sq.cropHeight) / 2) < 1e-9)
        c.applyCropAspect("Original")
        check("Original → 全幅", c.adj.cropWidth == 1 && c.adj.cropHeight == 1 && c.adj.cropX == 0)
        c.rotate(clockwise: true)
        check("右轉 → R90", c.adj.rotation == .r90)
        c.rotate(clockwise: false)
        check("左轉 → R0", c.adj.rotation == .r0)
        c.rotate(clockwise: true)
        stepVirtualCopy(c)
    }

    // ---- 6: virtual copy, copy/paste -------------------------------------

    private static func stepVirtualCopy(_ c: MainViewController) {
        line("")
        line("[6] 虛擬副本 / 複製貼上設定")
        guard let cur = c.current else { check("有目前照片", false); finish(); return }
        let before = c.items.count
        c.menuCreateVirtualCopy(item(for: cur))
        waitUntil("副本載入", { c.current?.isVirtualCopy == true && !c.isLoading }) {
            check("清單多一張", c.items.count == before + 1, "\(c.items.count)")
            check("preview_list 記錄副本", c.previewList.virtualCopies.count == 1)
            check("副本 XML 存在", AdjustmentXmlStore.exists(imagePath: cur.sourcePath, copyIndex: 1))
            check("副本繼承來源的旋轉", c.adj.rotation == .r90, "\(c.adj.rotation)")
            check("副本緊接在來源後", c.items[c.strip.currentIndex - 1].key == cur.key)

            // Copy from the copy, paste onto the first photo.
            c.menuCopySettings(item(for: c.current!))
            check("複製設定", c.copiedSettings?.rotation == .r90)
            c.strip.select(index: 0)
            waitUntil("回到第一張", { c.current?.key == c.items[0].key && !c.isLoading }) {
                c.menuPasteSettings(item(for: c.current!))
                check("貼上 → 旋轉 R90", c.adj.rotation == .r90)
                check("貼上後 edited", c.current?.isEdited == true)
                c.doUndo()
                check("貼上可復原", c.adj.rotation == .r0)
                stepHide(c)
            }
        }
    }

    // ---- 7: hide / unhide / show-all -------------------------------------

    private static func stepHide(_ c: MainViewController) {
        line("")
        line("[7] 隱藏且不輸出")
        let total = c.items.count
        let hidden = c.items[1]
        c.strip.select(index: 1)
        waitUntil("選到第二張", { c.current?.key == hidden.key && !c.isLoading }) {
            c.hideSelected()
            check("清單少一張", c.items.count == total - 1, "\(c.items.count)")
            check("preview_list 記錄隱藏", c.previewList.hidden.contains(hidden.key))
            check("目前照片換成鄰近一張", c.current != nil && c.current?.key != hidden.key)
            check("匯出目標排除隱藏", !c.items.contains { $0.key == hidden.key })
            let reload = PreviewListStore.load(imageFolder: c.folder)
            check("寫入磁碟", reload.hidden.contains(hidden.key))

            c.setShowHiddenMode(true)
            check("顯示全部 → 回到 \(total) 張", c.items.count == total, "\(c.items.count)")
            check("隱藏標記保留", c.items.first { $0.key == hidden.key }?.isHidden == true)
            c.restoreHiddenPhotos()
            check("還原 → 無隱藏", c.previewList.hidden.isEmpty)
            c.setShowHiddenMode(false)
            after(0.3) { stepDelete(c) }
        }
    }

    // ---- 8: delete the virtual copy --------------------------------------

    private static func stepDelete(_ c: MainViewController) {
        line("")
        line("[8] 刪除副本")
        guard let copy = c.items.first(where: { $0.isVirtualCopy }) else {
            check("找到副本", false); finish(); return
        }
        let total = c.items.count
        let src = copy.sourcePath
        c.deletePhotoFile(copy)
        check("清單少一張", c.items.count == total - 1, "\(c.items.count)")
        check("副本 XML 已刪", !AdjustmentXmlStore.exists(imagePath: src, copyIndex: copy.virtualCopyIndex))
        check("preview_list 無副本", c.previewList.virtualCopies.isEmpty)
        check("原檔仍在", FileManager.default.fileExists(atPath: src))
        check("仍有目前照片", c.current != nil)
        after(0.3) { stepClose(c) }
    }

    // ---- 9: close ----------------------------------------------------------

    private static func stepClose(_ c: MainViewController) {
        line("")
        line("[9] 關閉資料夾")
        let slider = c.basicPanel.sliders[0]
        slider.onEditBegin?(); slider.value = -0.5
        let cur = c.current!
        c.closeFolder()
        let saved = AdjustmentXmlStore.load(imagePath: cur.sourcePath, copyIndex: cur.virtualCopyIndex)
        check("關閉時寫入未存編輯", saved?.exposure == -0.5, "\(saved?.exposure ?? 9)")
        check("編輯區清空", c.current == nil && c.items.isEmpty && c.folder.isEmpty)
        finish()
    }
}
