//! Background work. Everything slow — LibRaw decodes, cache files, thumbnail renders —
//! runs here and reports back over a channel; the UI thread only ever draws.

use awpr_core::pipeline::{apply_to_float, ProcessContext, SourceKind};
use awpr_core::{color::WhiteBalanceReference, libraw, FloatImage, ImageAdjustments};
use awpr_photo::loader::{self, DecodeSource, LoaderOptions, ThumbnailBase};
use awpr_photo::{codec, exif, paths, store, ExifData};
use eframe::egui;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

pub enum Msg {
    Thumb { key: String, image: egui::ColorImage, version: u64 },
    Loaded(Box<Loaded>),
    /// One more photo's caches are ready (folder generation `gen`).
    CacheProgress { gen: u64, done: usize, total: usize, name: String },
    CpuRendered { version: u64, image: Arc<FloatImage> },
    /// A 處理版本 3 RAW's linear proxy is built (its photos can switch to it).
    ProxyV3Ready { path: String },
}

pub struct Loaded {
    pub version: u64,
    pub adjustments: ImageAdjustments,
    pub exif: ExifData,
    pub proxy: Option<FloatImage>,
    pub source: DecodeSource,
    /// The linear camera proxy (處理版本 3 RAW) or the usual encoded one.
    pub source_kind: SourceKind,
    /// A source reload for the photo already open (after 升級處理版本): keep its undo
    /// state and its saved values.
    pub reload: bool,
    pub millis: u128,
}

/// A photo in the strip (original or virtual copy).
#[derive(Clone)]
pub struct Item {
    pub path: String,
    pub copy: i32,
    pub key: String,
    /// #number: position in the folder's full list, hidden photos included.
    pub number: usize,
    pub hidden: bool,
    pub edited: bool,
}

impl Item {
    pub fn name(&self) -> String {
        let n = paths::file_name(&self.path);
        if self.copy > 0 {
            format!("{n}  (copy {})", self.copy)
        } else {
            n
        }
    }
}

#[derive(Clone)]
pub struct Worker {
    tx: Sender<Msg>,
    ctx: egui::Context,
    /// Bumped when a folder is opened or closed: older cache jobs stop.
    pub folder_gen: Arc<AtomicU64>,
    thumb_queue: Arc<Mutex<Vec<ThumbJob>>>,
    thumb_signal: Sender<()>,
    /// Second-stage cache work: linear proxies for 處理版本 3 RAWs. The front is served
    /// first; a photo the user opens is put there.
    v3_queue: Arc<Mutex<std::collections::VecDeque<V3Job>>>,
    v3_signal: Sender<()>,
}

struct V3Job {
    path: String,
    opt: LoaderOptions,
    /// The folder generation of a second-stage job (None: asked for by the editor).
    gen: Option<u64>,
}

struct ThumbJob {
    item: Item,
    adjustments: Option<ImageAdjustments>,
    version: u64,
}

impl Worker {
    pub fn new(ctx: egui::Context) -> (Self, Receiver<Msg>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let (sig_tx, sig_rx) = std::sync::mpsc::channel::<()>();
        let (v3_tx, v3_rx) = std::sync::mpsc::channel::<()>();
        let w = Self {
            tx,
            ctx,
            folder_gen: Arc::new(AtomicU64::new(0)),
            thumb_queue: Arc::new(Mutex::new(Vec::new())),
            thumb_signal: sig_tx,
            v3_queue: Default::default(),
            v3_signal: v3_tx,
        };
        // One linear-proxy thread: a full LibRaw decode each, behind the first stage.
        let w3 = w.clone();
        std::thread::Builder::new()
            .name("cache-v3".into())
            .spawn(move || {
                while v3_rx.recv().is_ok() {
                    loop {
                        let job = w3.v3_queue.lock().unwrap().pop_front();
                        let Some(job) = job else { break };
                        w3.build_v3(job);
                    }
                }
            })
            .expect("v3 cache thread");
        // One thumbnail thread: renders are small, and newest-first ordering matters
        // more than parallelism (the live slider preview of the current photo).
        let w2 = w.clone();
        std::thread::Builder::new()
            .name("thumbnails".into())
            .spawn(move || {
                while sig_rx.recv().is_ok() {
                    loop {
                        let job = w2.thumb_queue.lock().unwrap().pop();
                        let Some(job) = job else { break };
                        w2.render_thumb(job);
                    }
                }
            })
            .expect("thumbnail thread");
        (w, rx)
    }

    fn send(&self, m: Msg) {
        let _ = self.tx.send(m);
        self.ctx.request_repaint();
    }

    // ---- thumbnails ------------------------------------------------------------

    /// Queue a strip thumbnail render. A newer request for the same key replaces an
    /// older one still waiting.
    pub fn thumbnail(&self, item: &Item, adjustments: Option<ImageAdjustments>, version: u64) {
        let mut q = self.thumb_queue.lock().unwrap();
        q.retain(|j| j.item.key != item.key);
        q.push(ThumbJob { item: item.clone(), adjustments, version });
        drop(q);
        let _ = self.thumb_signal.send(());
    }

    fn render_thumb(&self, job: ThumbJob) {
        let Some(base) = loader::load_thumbnail_base(&job.item.path) else { return };
        let img = render_thumbnail(&base, &job.item, job.adjustments);
        self.send(Msg::Thumb { key: job.item.key.clone(), image: to_color_image(&img), version: job.version });
    }

    // ---- folder caches ---------------------------------------------------------

    /// Build thumbnails, proxies and placeholder XMLs for a whole folder, two photos at
    /// a time (a full RAW decode each; more at once runs out of memory on 60 MP files).
    /// Two stages: first every photo's usual caches (thumbnail, 8-bit proxy, XML) — the
    /// folder is usable as soon as they exist — then, one by one in the background, the
    /// linear proxies of the 處理版本 3 RAWs (`build_v3`).
    pub fn generate_caches(&self, items: Vec<Item>, opt: LoaderOptions) {
        let gen = self.folder_gen.load(Ordering::SeqCst);
        let all: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            items.iter().filter(|i| seen.insert(i.path.clone())).map(|i| i.path.clone()).collect()
        };
        let mut seen = std::collections::HashSet::new();
        let sources: Vec<String> = items.iter().filter(|i| seen.insert(i.path.clone())).map(|i| i.path.clone()).collect();
        let total = sources.len();
        let queue = Arc::new(Mutex::new(sources.into_iter().rev().collect::<Vec<_>>()));
        let done = Arc::new(AtomicU64::new(0));
        let items = Arc::new(items);
        for t in 0..2 {
            let (w, queue, done, items, all) = (self.clone(), queue.clone(), done.clone(), items.clone(), all.clone());
            std::thread::Builder::new()
                .name(format!("cache{t}"))
                .spawn(move || loop {
                    if w.folder_gen.load(Ordering::SeqCst) != gen {
                        return;
                    }
                    let Some(path) = queue.lock().unwrap().pop() else { return };
                    loader::ensure_thumbnail_cache(&path, opt);
                    // Show the camera preview as soon as it exists, then the proxy cut.
                    for it in items.iter().filter(|i| i.path == path) {
                        w.thumbnail(it, None, 0);
                    }
                    // Seed the XML so the as-shot white balance is set once, here.
                    let mut e = exif::read(&path);
                    enrich_camera_color(&path, &mut e, opt);
                    store::ensure_default(&path, Some(&e), 0);
                    loader::ensure_proxy_cache(&path, opt);
                    for it in items.iter().filter(|i| i.path == path) {
                        w.thumbnail(it, None, 0);
                    }
                    let d = done.fetch_add(1, Ordering::SeqCst) as usize + 1;
                    w.send(Msg::CacheProgress { gen, done: d, total, name: paths::file_name(&path) });
                    if d == total {
                        // Stage 2, behind anything the editor already asked for.
                        crate::trace(&format!("cache stage 1 done ({total} photos); stage 2 queued"));
                        let mut q = w.v3_queue.lock().unwrap();
                        q.extend(all.iter().map(|p| V3Job { path: p.clone(), opt, gen: Some(gen) }));
                        drop(q);
                        let _ = w.v3_signal.send(());
                    }
                })
                .expect("cache thread");
        }
    }

    /// The editor opened a 處理版本 3 RAW whose linear proxy is not built yet: build it
    /// next, ahead of the second stage.
    pub fn request_v3(&self, path: &str, opt: LoaderOptions) {
        if std::env::var_os("AWPR_SHOT_HOLD_V3").is_some() {
            return; // screenshots of the waiting state
        }
        let mut q = self.v3_queue.lock().unwrap();
        q.retain(|j| j.path != path);
        q.push_front(V3Job { path: path.to_string(), opt, gen: None });
        drop(q);
        crate::trace(&format!("v3 proxy: {} moved to the front of the queue", paths::file_name(path)));
        let _ = self.v3_signal.send(());
    }

    /// One linear proxy, if the photo is a 處理版本 3 RAW that still needs it. When the
    /// usual proxy is missing too (the editor asked before stage 1 reached it), both come
    /// from one LibRaw unpack (`ensure_proxy_caches`).
    fn build_v3(&self, job: V3Job) {
        if std::env::var_os("AWPR_SHOT_HOLD_V3").is_some() {
            return;
        }
        if job.gen.is_some_and(|g| g != self.folder_gen.load(Ordering::SeqCst)) {
            return; // another folder now
        }
        if loader::proxy_v3_ready(&job.path) {
            return;
        }
        let (adj, exif, _) = store::load_all(&job.path, 0);
        let mut e = exif.unwrap_or_else(|| exif::read(&job.path));
        enrich_camera_color(&job.path, &mut e, job.opt);
        // Stage 2 checks the photo's version; the editor asks only for version-3 photos
        // (a virtual copy may be version 3 while the original is not).
        let v3 = job.gen.is_none() || adj.as_ref().is_some_and(ImageAdjustments::is_v3);
        if !v3 || !loader::linear_capable(&job.path, e.camera.as_ref(), job.opt) {
            return;
        }
        let t0 = std::time::Instant::now();
        loader::ensure_proxy_caches(&job.path, job.opt, e.camera.as_ref());
        if loader::proxy_v3_ready(&job.path) {
            crate::trace(&format!(
                "v3 proxy built: {} ({}, {} ms)",
                paths::file_name(&job.path),
                if job.gen.is_none() { "requested by the editor" } else { "stage 2" },
                t0.elapsed().as_millis()
            ));
            self.send(Msg::ProxyV3Ready { path: job.path });
        }
    }

    // ---- photo loading ------------------------------------------------------------

    /// Load a photo. `keep` = a reload of the photo already open with these (in-memory)
    /// adjustments: only the source changes.
    pub fn load_photo(&self, item: Item, version: u64, opt: LoaderOptions, keep: Option<ImageAdjustments>) {
        let w = self.clone();
        std::thread::Builder::new()
            .name("load".into())
            .spawn(move || {
                let t0 = std::time::Instant::now();
                let (stored, stored_exif, _) = store::load_all(&item.path, item.copy);
                let mut e = stored_exif.clone().unwrap_or_else(|| exif::read(&item.path));
                // Back-fill camera colour data into XMLs written before it existed.
                let enriched = enrich_camera_color(&item.path, &mut e, opt);
                let reload = keep.is_some();
                let adj = match (keep, stored) {
                    (Some(a), _) => a,
                    (None, Some(a)) => a,
                    (None, None) => store::ensure_default(&item.path, Some(&e), item.copy),
                };
                if enriched && !reload {
                    let _ = store::save(&item.path, &adj, item.copy, Some(&e));
                }
                let proxy = loader::load_proxy_for(&item.path, &adj, e.camera.as_ref(), opt);
                if adj.is_v3() && loader::linear_capable(&item.path, e.camera.as_ref(), opt) && !loader::proxy_v3_ready(&item.path) {
                    // Shown from the 8-bit proxy meanwhile; switched when it is built.
                    w.request_v3(&item.path, opt);
                }
                let (proxy, source, source_kind) = match proxy {
                    Some((p, s, k)) => (Some(p), s, k),
                    None => (None, DecodeSource::LibRaw, SourceKind::Encoded),
                };
                w.send(Msg::Loaded(Box::new(Loaded {
                    version,
                    adjustments: adj,
                    exif: e,
                    proxy,
                    source,
                    source_kind,
                    reload,
                    millis: t0.elapsed().as_millis(),
                })));
            })
            .expect("load thread");
    }

    // ---- CPU rendering (no usable GPU) -------------------------------------------

    pub fn cpu_render(&self, proxy: Arc<FloatImage>, adj: ImageAdjustments, ctx: ProcessContext, version: u64) {
        let w = self.clone();
        std::thread::spawn(move || {
            let out = apply_to_float(&proxy, &adj, &ctx);
            w.send(Msg::CpuRendered { version, image: Arc::new(out) });
        });
    }
}

/// Attach LibRaw's colour data to a RAW's EXIF when it is missing. True when added.
pub fn enrich_camera_color(path: &str, e: &mut ExifData, opt: LoaderOptions) -> bool {
    if e.camera.as_ref().is_some_and(|c| c.is_valid()) || !paths::is_raw(path) || !opt.use_libraw {
        return false;
    }
    match libraw::read_camera_color(path) {
        Some(c) => {
            e.camera = Some(c);
            true
        }
        None => false,
    }
}

/// One strip thumbnail with its photo's adjustments applied. Port of the Swift
/// `renderThumbnail`, including the white-balance rebase for camera previews.
pub fn render_thumbnail(base: &ThumbnailBase, item: &Item, adjustments: Option<ImageAdjustments>) -> FloatImage {
    let (stored, exif, _) = store::load_all(&item.path, item.copy);
    let mut a = adjustments.or(stored).unwrap_or_default();
    // 處理版本 3 RAW with its linear proxy built: the thumbnail renders from the linear
    // strip thumbnail, the editor's own source (until then from the 8-bit one below).
    let camera = exif.as_ref().and_then(|e| e.camera.clone()).filter(|c| c.is_valid());
    if a.is_v3() && camera.is_some() && paths::is_raw(&item.path) {
        if let Some((lin, gain)) = loader::load_thumbnail_v3(&item.path) {
            let ctx = ProcessContext { camera, source_kind: SourceKind::LinearCamera { gain }, ..Default::default() };
            return apply_to_float(&lin, &a, &ctx);
        }
    }
    if store::is_default(&a) {
        return base.buffer.clone();
    }
    let mut ctx = ProcessContext { camera: exif.as_ref().and_then(|e| e.camera.clone()), ..Default::default() };
    if let Some(src) = base.proxy_source {
        ctx.white_balance_reference = src.white_balance_reference();
    } else if paths::is_raw(&item.path) {
        // The camera's own rendering, balanced to cam_mul: apply only the offset from
        // as-shot, or the white balance would be applied twice.
        ctx.white_balance_reference = WhiteBalanceReference::AsShot;
        if let Some(e) = exif.as_ref().filter(|e| e.has_as_shot_white_balance()) {
            if ctx.camera.is_none() {
                a.temperature = 5200.0 + (a.temperature - e.color_temperature);
            }
        }
    }
    apply_to_float(&base.buffer, &a, &ctx)
}

pub fn to_color_image(img: &FloatImage) -> egui::ColorImage {
    egui::ColorImage::from_rgb([img.width, img.height], &codec::to_rgb8(img))
}
