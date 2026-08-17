//! The viewer application: playback, the frame cache, the display transform
//! and the egui panels.
//!
//! Decoded frames arrive from the worker in [`crate::decode`] and land in a
//! small LRU cache keyed by `(sequence index, subimage)`. The display
//! transform — exposure gain, channel isolation, sRGB encode — runs here on
//! the UI thread from the cached linear pixels, so exposure and channel
//! changes are instant and never touch the disk; the resulting texture is
//! re-uploaded only when the frame or those settings change, not on every
//! egui frame.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::decode::{self, DecodeReply, DecodeRequest, Frame};

/// Letterbox background behind the image, darker than the panels around it.
const LETTERBOX: egui::Color32 = egui::Color32::from_rgb(0x14, 0x14, 0x14);

/// Placeholder fill shown when the current file fails to decode.
const ERROR_BACKGROUND: egui::Color32 = egui::Color32::from_rgb(0x40, 0x00, 0x00);

/// Error text, legible against both the placeholder and the panels.
const ERROR_TEXT: egui::Color32 = egui::Color32::from_rgb(0xff, 0xb4, 0xb4);

/// At most this many decoded frames stay cached...
const CACHE_MAX_FRAMES: usize = 12;

/// Nearest magnification keeps zoomed-in pixels crisp for inspection;
/// linear minification keeps a zoomed-out image from shimmering with
/// aliasing, which nearest would cause by decimation.
const DISPLAY_TEXTURE: egui::TextureOptions = egui::TextureOptions {
    magnification: egui::TextureFilter::Nearest,
    minification: egui::TextureFilter::Linear,
    wrap_mode: egui::TextureWrapMode::ClampToEdge,
    mipmap_mode: None,
};

/// ...or this many bytes of linear pixels, whichever limit binds first.
const CACHE_MAX_BYTES: usize = 1_500_000_000;

/// Which channels of the frame reach the screen.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ChannelView {
    /// The colour image as decoded.
    Rgb,
    /// The first channel as grey.
    Red,
    /// The second channel as grey.
    Green,
    /// The third channel as grey.
    Blue,
    /// The alpha channel as grey; a frame without one shows solid white.
    Alpha,
    /// Rec. 709 luma of the linear pixels, as grey.
    Luma,
}

impl ChannelView {
    /// Every view with its radio label, in display order.
    const ALL: [(Self, &'static str); 6] = [
        (Self::Rgb, "RGB"),
        (Self::Red, "R"),
        (Self::Green, "G"),
        (Self::Blue, "B"),
        (Self::Alpha, "A"),
        (Self::Luma, "Luma"),
    ];
}

/// One cache slot: the decode result — errors are cached too, so a broken
/// file is not re-read on every repaint — with its bookkeeping.
struct CachedFrame {
    result: Result<Frame, String>,
    /// Monotonic id: a reload gives the same key a new revision, which is
    /// what tells the texture upload that its copy is stale.
    revision: u64,
    /// The access clock reading from the last time this entry was shown.
    last_used: u64,
}

impl CachedFrame {
    /// The memory this entry pins, for the byte half of the cache budget.
    fn byte_size(&self) -> usize {
        self.result.as_ref().map_or(0, Frame::byte_size)
    }
}

/// Everything the uploaded texture depends on; when this key changes, the
/// display transform runs again and the texture is replaced.
#[derive(Clone, Copy, PartialEq)]
struct DisplayKey {
    revision: u64,
    exposure_bits: u32,
    view: ChannelView,
    layer: usize,
}

/// One frame's worth of pressed shortcut keys, read in a single pass.
struct KeyIntents {
    quit: bool,
    open: bool,
    toggle_play: bool,
    step_forward: bool,
    step_back: bool,
    part_next: bool,
    part_previous: bool,
    layer_next: bool,
    exposure_up: bool,
    exposure_down: bool,
    exposure_reset: bool,
    fit: bool,
    one_to_one: bool,
    reload: bool,
}

/// The viewer state driven by [`eframe::App::ui`].
pub struct ViewerApp {
    /// The sequence in display order; empty until one is opened, and the
    /// window says so instead of showing pixels.
    paths: Vec<PathBuf>,
    /// What the last failed open attempt said, shown until the next one.
    notice: Option<String>,
    /// Index of the frame being shown.
    index: usize,
    /// Which subimage (EXR part) of the current file is shown.
    subimage: u32,
    /// Exposure in stops; the gain applied to the pixels is two to this power.
    exposure_stops: f32,
    /// Which channels reach the screen.
    channel_view: ChannelView,
    /// The chosen layer of a multichannel image, remembered by name so the
    /// choice follows the user across frames and parts; `None` is the
    /// first layer.
    layer_name: Option<String>,
    /// Whether playback is running.
    playing: bool,
    /// Playback rate in frames per second.
    fps: f32,
    /// When the next playback step is due; cleared whenever pacing restarts.
    next_frame_due: Option<Instant>,
    /// The direction of the last manual step, so prefetch can follow a user
    /// who is stepping backwards.
    step_direction: isize,
    /// Screen pixels per image pixel; only authoritative when `fit` is off.
    zoom: f32,
    /// Offset of the image centre from the panel centre, in screen points.
    pan: egui::Vec2,
    /// While set, zoom and pan are recomputed each frame to fit the window.
    fit: bool,
    /// Whether the inspector side panel is open.
    show_inspector: bool,
    /// Decoded frames and cached failures, keyed by `(index, subimage)`.
    cache: HashMap<(usize, u32), CachedFrame>,
    /// Keys with a decode in flight, so a frame is requested only once.
    pending: HashSet<(usize, u32)>,
    /// Stamped onto every request; bumped on reloads, scrub jumps and part
    /// changes so replies from before the change are recognised as stale.
    generation: u64,
    /// The worker's view of [`Self::generation`], checked before each
    /// decode so a whole backlog of stale requests dies in microseconds.
    shared_generation: Arc<AtomicU64>,
    /// Source of [`CachedFrame::revision`] values.
    next_revision: u64,
    /// Logical clock backing the cache's least-recently-used ordering.
    access_clock: u64,
    request_sender: Sender<DecodeRequest>,
    reply_receiver: Receiver<DecodeReply>,
    /// The uploaded display image, kept across egui frames.
    texture: Option<egui::TextureHandle>,
    /// What [`Self::texture`] currently holds, or `None` before any upload.
    uploaded: Option<DisplayKey>,
    /// The last title sent to the window, so it is only sent on change.
    last_title: String,
}

impl ViewerApp {
    /// Build the application and start its decode thread.
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Vec<PathBuf>) -> Self {
        // A review tool's surroundings should be darker than its image, so
        // the viewer commits to the dark theme.
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        let shared_generation = Arc::new(AtomicU64::new(0));
        let (request_sender, reply_receiver) =
            decode::spawn_decoder(cc.egui_ctx.clone(), Arc::clone(&shared_generation));
        Self {
            paths,
            notice: None,
            index: 0,
            subimage: 0,
            exposure_stops: 0.0,
            channel_view: ChannelView::Rgb,
            layer_name: None,
            playing: false,
            fps: 24.0,
            next_frame_due: None,
            step_direction: 1,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            fit: true,
            show_inspector: true,
            cache: HashMap::new(),
            pending: HashSet::new(),
            generation: 0,
            shared_generation,
            next_revision: 0,
            access_clock: 0,
            request_sender,
            reply_receiver,
            texture: None,
            uploaded: None,
            last_title: String::new(),
        }
    }

    /// Take every decode reply that has arrived, dropping stale generations
    /// and trimming the cache after each insert.
    fn drain_replies(&mut self) {
        while let Ok(reply) = self.reply_receiver.try_recv() {
            if reply.generation != self.generation {
                // The user reloaded or jumped after this was requested; its
                // pending entry was already cleared along with the bump.
                continue;
            }
            let key = (reply.index, reply.subimage);
            self.pending.remove(&key);
            self.next_revision += 1;
            self.cache.insert(
                key,
                CachedFrame {
                    result: reply.result,
                    revision: self.next_revision,
                    last_used: self.access_clock,
                },
            );
            self.evict();
        }
    }

    /// Shrink the cache back under its frame and byte budgets, evicting the
    /// least recently shown entry first and never one of the wanted keys —
    /// evicting a frame [`Self::schedule_decodes`] is about to re-request
    /// would decode, evict and re-request it forever.
    fn evict(&mut self) {
        let protected = self.wanted_keys();
        loop {
            let over_frames = self.cache.len() > CACHE_MAX_FRAMES;
            let over_bytes = self
                .cache
                .values()
                .map(CachedFrame::byte_size)
                .sum::<usize>()
                > CACHE_MAX_BYTES;
            if !over_frames && !over_bytes {
                return;
            }
            let victim = self
                .cache
                .iter()
                .filter(|&(key, _)| !protected.contains(key))
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| *key);
            match victim {
                Some(key) => self.cache.remove(&key),
                // Only wanted frames are left; they stay whatever they cost.
                None => return,
            };
        }
    }

    /// Invalidate every request still queued or in flight. The cleared
    /// pending set lets the still-wanted frames be requested again under
    /// the new generation, and the shared copy lets the worker skip the
    /// stale requests without decoding them.
    fn bump_generation(&mut self) {
        self.generation += 1;
        self.shared_generation
            .store(self.generation, Ordering::Relaxed);
        self.pending.clear();
    }

    /// Ask the worker for a frame unless it is already cached or in flight.
    fn request_decode(&mut self, index: usize, subimage: u32) {
        let key = (index, subimage);
        if self.cache.contains_key(&key) || self.pending.contains(&key) {
            return;
        }
        let request = DecodeRequest {
            path: self.paths[index].clone(),
            index,
            subimage,
            generation: self.generation,
        };
        if self.request_sender.send(request).is_ok() {
            self.pending.insert(key);
        }
    }

    /// The keys the viewer wants resident: the frame on screen and the ones
    /// the user is heading towards, ordered least urgent first — the worker
    /// serves the newest request first, so the on-screen frame comes last.
    /// Neighbours are wanted at their first part, because part choice is
    /// per-file and stepping resets it.
    fn wanted_keys(&self) -> Vec<(usize, u32)> {
        let count = self.paths.len();
        let mut wanted = Vec::with_capacity(4);
        if count > 1 {
            let next = ((self.index + 1) % count, 0);
            if self.playing {
                wanted.push(((self.index + 2) % count, 0));
                wanted.push(next);
            } else if self.step_direction < 0 {
                // Heading backwards, the previous frame outranks the next.
                wanted.push(next);
                wanted.push(((self.index + count - 1) % count, 0));
            } else {
                wanted.push(next);
            }
        }
        wanted.push((self.index, self.subimage));
        wanted
    }

    /// Request every wanted frame that is neither cached nor in flight.
    fn schedule_decodes(&mut self) {
        if self.paths.is_empty() {
            return;
        }
        for (index, subimage) in self.wanted_keys() {
            self.request_decode(index, subimage);
        }
    }

    /// Replace the sequence with a freshly opened one and reset the view to
    /// its start. Display settings — exposure, channel view, the inspector —
    /// survive the switch; everything tied to the old files does not.
    fn set_sequence(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        self.paths = paths;
        self.notice = None;
        self.index = 0;
        self.subimage = 0;
        self.layer_name = None;
        self.playing = false;
        self.next_frame_due = None;
        self.step_direction = 1;
        self.fit = true;
        self.pan = egui::Vec2::ZERO;
        self.cache.clear();
        self.pending.clear();
        self.bump_generation();
        self.uploaded = None;
    }

    /// Ask for image files through the native dialog and open them as the
    /// sequence. The dialog blocks the interface, as modal pickers do.
    fn open_files_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter("images", crate::IMAGE_EXTENSIONS)
            .set_title("Open images")
            .pick_files();
        if let Some(mut files) = picked {
            files.sort_by(|a, b| crate::natural_order(a, b));
            self.set_sequence(files);
        }
    }

    /// Ask for a directory and open its sequence — dominant extension, then
    /// dominant name pattern, exactly as a directory argument resolves.
    fn open_folder_dialog(&mut self) {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Open a sequence")
            .pick_folder()
        {
            match crate::collect_sequence(&folder) {
                Ok(paths) => self.set_sequence(paths),
                Err(message) => self.notice = Some(message),
            }
        }
    }

    /// Open whatever was dropped onto the window: one directory stands for
    /// its sequence, anything else is taken as the files themselves.
    fn open_dropped(&mut self, dropped: Vec<PathBuf>) {
        if let [only] = dropped.as_slice() {
            if only.is_dir() {
                match crate::collect_sequence(only) {
                    Ok(paths) => self.set_sequence(paths),
                    Err(message) => self.notice = Some(message),
                }
                return;
            }
        }
        let mut files = dropped;
        files.retain(|path| path.is_file());
        if files.is_empty() {
            self.notice = Some("nothing dropped was an image file".to_owned());
            return;
        }
        files.sort_by(|a, b| crate::natural_order(a, b));
        self.set_sequence(files);
    }

    /// Move `delta` frames through the sequence, wrapping at both ends.
    fn step(&mut self, delta: isize) {
        if self.paths.is_empty() {
            return;
        }
        let length = self.paths.len() as isize;
        self.index = (self.index as isize + delta).rem_euclid(length) as usize;
        // A new file starts at its first part; part choice is per-file.
        self.subimage = 0;
        self.step_direction = delta.signum();
        self.next_frame_due = None;
    }

    /// Jump straight to `index`, as the scrubber does. Unlike a single step
    /// this bumps the generation: prefetches from around the old position
    /// are no longer worth keeping.
    fn jump(&mut self, index: usize) {
        if index == self.index {
            return;
        }
        self.step_direction = if index > self.index { 1 } else { -1 };
        self.index = index;
        self.subimage = 0;
        self.bump_generation();
        self.next_frame_due = None;
    }

    /// The decoded frame on screen, if its decode has landed and succeeded.
    fn current_frame(&self) -> Option<&Frame> {
        self.cache
            .get(&(self.index, self.subimage))
            .and_then(|entry| entry.result.as_ref().ok())
    }

    /// Which of `layers` the remembered layer name picks: the match, or
    /// the first layer when nothing is remembered or the name is gone —
    /// a sequence can change layer sets mid-flight.
    fn resolve_layer(&self, layers: &[decode::Layer]) -> usize {
        self.layer_name
            .as_deref()
            .and_then(|wanted| layers.iter().position(|layer| layer.name == wanted))
            .unwrap_or(0)
    }

    /// Remember the next layer of the current frame, wrapping around.
    fn cycle_layer(&mut self) {
        let Some(frame) = self.current_frame() else {
            return;
        };
        if frame.layers.len() > 1 {
            let next = (self.resolve_layer(&frame.layers) + 1) % frame.layers.len();
            self.layer_name = Some(frame.layers[next].name.clone());
        }
    }

    /// Show a neighbouring subimage of a multi-part file, wrapping around.
    /// The part count comes from the current entry, or from any decoded
    /// part of the same file when the current one failed — navigation must
    /// not dead-end on a broken part.
    fn cycle_part(&mut self, delta: i64) {
        let count = self
            .cache
            .get(&(self.index, self.subimage))
            .and_then(|entry| entry.result.as_ref().ok())
            .or_else(|| {
                self.cache
                    .iter()
                    .filter(|&(&(index, _), _)| index == self.index)
                    .find_map(|(_, entry)| entry.result.as_ref().ok())
            })
            .map(|frame| i64::from(frame.subimage_count));
        let Some(count) = count else { return };
        if count > 1 {
            self.subimage = (i64::from(self.subimage) + delta).rem_euclid(count) as u32;
            self.bump_generation();
        }
    }

    /// Drop every cached part of the current file and decode the shown one
    /// from disk again — a reload means the file changed, so a stale
    /// sibling part must not survive it.
    fn reload_current(&mut self) {
        let index = self.index;
        self.cache
            .retain(|&(entry_index, _), _| entry_index != index);
        self.bump_generation();
    }

    fn toggle_play(&mut self) {
        self.playing = !self.playing;
        // Pacing restarts cleanly on every transition.
        self.next_frame_due = None;
    }

    /// Act on the shortcut keys, unless a text field is being edited.
    /// The gate is text editing specifically — `egui_wants_keyboard_input`
    /// is true whenever anything has focus, and a focused button must not
    /// switch off every viewer shortcut.
    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.text_edit_focused() {
            return;
        }
        let keys = ctx.input(|input| KeyIntents {
            quit: input.key_pressed(egui::Key::Escape) || input.key_pressed(egui::Key::Q),
            open: input.modifiers.command && input.key_pressed(egui::Key::O),
            toggle_play: input.key_pressed(egui::Key::Space),
            step_forward: input.key_pressed(egui::Key::ArrowRight),
            step_back: input.key_pressed(egui::Key::ArrowLeft),
            part_next: input.key_pressed(egui::Key::S) || input.key_pressed(egui::Key::ArrowUp),
            part_previous: input.key_pressed(egui::Key::ArrowDown),
            layer_next: input.key_pressed(egui::Key::L),
            // Plus shares a key with equals, so both spellings work without
            // the shift key mattering; modifiers are ignored, so underscore
            // reaches the minus arm the same way.
            exposure_up: input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals),
            exposure_down: input.key_pressed(egui::Key::Minus),
            exposure_reset: input.key_pressed(egui::Key::Home),
            fit: input.key_pressed(egui::Key::F),
            one_to_one: input.key_pressed(egui::Key::Num1),
            reload: input.key_pressed(egui::Key::R),
        });
        if keys.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if keys.open {
            self.open_files_dialog();
        }
        if keys.toggle_play {
            self.toggle_play();
        }
        if keys.step_forward {
            self.step(1);
        }
        if keys.step_back {
            self.step(-1);
        }
        if keys.part_next {
            self.cycle_part(1);
        }
        if keys.part_previous {
            self.cycle_part(-1);
        }
        if keys.layer_next {
            self.cycle_layer();
        }
        if keys.exposure_up {
            self.exposure_stops = (self.exposure_stops + 1.0).min(10.0);
        }
        if keys.exposure_down {
            self.exposure_stops = (self.exposure_stops - 1.0).max(-10.0);
        }
        if keys.exposure_reset {
            self.exposure_stops = 0.0;
        }
        if keys.fit {
            self.fit = true;
            self.pan = egui::Vec2::ZERO;
        }
        if keys.one_to_one {
            // One image pixel per physical pixel, not per egui point, so the
            // mapping is exact on high-density displays too.
            self.fit = false;
            self.zoom = 1.0 / ctx.pixels_per_point();
        }
        if keys.reload {
            self.reload_current();
        }
    }

    /// Advance playback when a step is due and the next frame has decoded;
    /// otherwise hold the current frame — the central panel shows a
    /// "decoding…" badge — and try again as soon as the reply lands.
    fn advance_playback(&mut self, ctx: &egui::Context) {
        // A single file has nowhere to advance to.
        if !self.playing || self.paths.len() < 2 {
            return;
        }
        let frame_duration = Duration::from_secs_f32(1.0 / self.fps.clamp(1.0, 240.0));
        let now = Instant::now();
        let due = *self.next_frame_due.get_or_insert(now + frame_duration);
        if now >= due {
            let next = (self.index + 1) % self.paths.len();
            // A cached failure counts as ready: playback runs past broken
            // files rather than stalling on them.
            if self.cache.contains_key(&(next, 0)) {
                self.index = next;
                self.subimage = 0;
                self.step_direction = 1;
                // Pace from the previous deadline so drift does not
                // accumulate, but never schedule into the past after a stall.
                let mut next_due = due + frame_duration;
                if next_due < now {
                    next_due = now + frame_duration;
                }
                self.next_frame_due = Some(next_due);
            }
        }
        if let Some(due) = self.next_frame_due {
            // A deadline in the past means playback is stalled on a decode;
            // the worker's own repaint poke resumes it, so only a future
            // deadline needs a timer — a 1 ms retry here would busy-spin
            // the UI against the very thread it is waiting on.
            if due > now {
                ctx.request_repaint_after(due - now);
            }
        }
    }

    /// Re-run the display transform and upload the result, but only when the
    /// frame, the exposure or the channel view actually changed.
    fn refresh_texture(&mut self, ctx: &egui::Context) {
        self.access_clock += 1;
        let key = (self.index, self.subimage);
        let Some(entry) = self.cache.get_mut(&key) else {
            return;
        };
        entry.last_used = self.access_clock;
        let Ok(frame) = &entry.result else {
            return;
        };
        let layer = self
            .layer_name
            .as_deref()
            .and_then(|wanted| frame.layers.iter().position(|layer| layer.name == wanted))
            .unwrap_or(0);
        let display = DisplayKey {
            revision: entry.revision,
            exposure_bits: self.exposure_stops.to_bits(),
            view: self.channel_view,
            layer,
        };
        if self.uploaded == Some(display) {
            return;
        }
        let max_side = ctx.input(|input| input.max_texture_side).max(1);
        let image = build_display_image(
            frame,
            self.exposure_stops.exp2(),
            self.channel_view,
            max_side,
            &frame.layers[layer],
        );
        match &mut self.texture {
            Some(texture) => texture.set(image, DISPLAY_TEXTURE),
            None => self.texture = Some(ctx.load_texture("frame", image, DISPLAY_TEXTURE)),
        }
        self.uploaded = Some(display);
    }

    /// The menu bar: opening sequences and quitting.
    fn menu_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open files…\tCtrl+O").clicked() {
                        ui.close();
                        self.open_files_dialog();
                    }
                    if ui.button("Open folder…").clicked() {
                        ui.close();
                        self.open_folder_dialog();
                    }
                    ui.separator();
                    if ui.button("Quit\tEsc").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });
    }

    /// The transport bar: play/pause and stepping, the frame scrubber, the
    /// playback rate and the inspector toggle.
    fn transport_panel(&mut self, ui: &mut egui::Ui) {
        let frame_count = self.paths.len();
        if frame_count == 0 {
            return;
        }
        egui::Panel::bottom("transport").show(ui, |ui| {
            ui.horizontal(|ui| {
                // Every button surrenders focus the way the sliders do: a
                // focused button would double-handle Space and let the
                // arrow keys walk the focus ring instead of the frames.
                let play_label = if self.playing { "Pause" } else { "Play" };
                let play = ui.button(play_label).on_hover_text("Space");
                if play.clicked() {
                    self.toggle_play();
                }
                play.surrender_focus();
                let previous = ui.button("Prev").on_hover_text("Left");
                if previous.clicked() {
                    self.step(-1);
                }
                previous.surrender_focus();
                let next = ui.button("Next").on_hover_text("Right");
                if next.clicked() {
                    self.step(1);
                }
                next.surrender_focus();
                ui.separator();
                ui.label(format!("frame {}/{frame_count}", self.index + 1));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let toggle = if self.show_inspector {
                        "Hide panel"
                    } else {
                        "Show panel"
                    };
                    let toggle_response = ui.button(toggle);
                    if toggle_response.clicked() {
                        self.show_inspector = !self.show_inspector;
                    }
                    toggle_response.surrender_focus();
                    ui.add(
                        egui::DragValue::new(&mut self.fps)
                            .range(1.0..=120.0)
                            .speed(0.5)
                            .suffix(" fps"),
                    );
                    // The scrubber takes whatever width is left between the
                    // frame counter and the rate control.
                    ui.spacing_mut().slider_width = (ui.available_width() - 16.0).max(64.0);
                    let mut scrub = self.index;
                    let response = ui
                        .add(egui::Slider::new(&mut scrub, 0..=frame_count - 1).show_value(false));
                    if response.changed() {
                        self.jump(scrub);
                    }
                    // The arrow keys step frames globally; a focused slider
                    // would double-handle them.
                    response.surrender_focus();
                });
            });
        });
    }

    /// The collapsible inspector: display controls, the current frame's
    /// spec, and its metadata table.
    fn inspector_panel(&mut self, ui: &mut egui::Ui) {
        // The open flag goes through a local because the panel wants it
        // mutably — dragging the panel shut flips it — while the closure
        // needs the rest of `self`.
        let mut open = self.show_inspector;
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(300.0)
            .show_collapsible(ui, &mut open, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::CollapsingHeader::new("Display")
                        .default_open(true)
                        .show(ui, |ui| self.display_group(ui));
                    egui::CollapsingHeader::new("Image")
                        .default_open(true)
                        .show(ui, |ui| self.image_group(ui));
                    egui::CollapsingHeader::new("Metadata")
                        .default_open(true)
                        .show(ui, |ui| self.metadata_group(ui));
                });
            });
        self.show_inspector = open;
    }

    /// Exposure and channel isolation.
    fn display_group(&mut self, ui: &mut egui::Ui) {
        ui.label("Exposure (stops)");
        let response = ui.add(egui::Slider::new(&mut self.exposure_stops, -10.0..=10.0));
        // As with the scrubber, keyboard focus would fight the global keys.
        response.surrender_focus();
        ui.add_space(4.0);
        ui.label("Channels");
        ui.horizontal_wrapped(|ui| {
            for (view, label) in ChannelView::ALL {
                // Focus would let the arrow keys walk the radios instead of
                // stepping frames.
                ui.radio_value(&mut self.channel_view, view, label)
                    .surrender_focus();
            }
        });
        // The layer dropdown, for multichannel images whose channels group
        // into more than one layer — a Nuke-style EXR carries its AOVs as
        // `layer.channel` names, and each is a click away here. Switching
        // re-uses the decoded pixels, so it is as instant as exposure.
        let layers: Vec<(String, usize)> = self
            .current_frame()
            .map(|frame| {
                frame
                    .layers
                    .iter()
                    .map(|layer| {
                        let count = layer.color.len() + usize::from(layer.alpha.is_some());
                        (layer.name.clone(), count)
                    })
                    .collect()
            })
            .unwrap_or_default();
        if layers.len() > 1 {
            let resolved = self
                .layer_name
                .as_deref()
                .and_then(|wanted| layers.iter().position(|(name, _)| name == wanted))
                .unwrap_or(0);
            let mut chosen = resolved;
            ui.add_space(4.0);
            ui.label(format!("Layer ({})", layers.len()));
            egui::ComboBox::from_id_salt("layer")
                .selected_text(format!("{} ({})", layers[chosen].0, layers[chosen].1))
                .show_ui(ui, |ui| {
                    for (index, (name, count)) in layers.iter().enumerate() {
                        ui.selectable_value(&mut chosen, index, format!("{name} ({count})"));
                    }
                });
            if chosen != resolved {
                self.layer_name = Some(layers[chosen].0.clone());
            }
        }
    }

    /// The current frame's spec, with a part selector for multi-part files.
    fn image_group(&mut self, ui: &mut egui::Ui) {
        let Some(entry) = self.cache.get(&(self.index, self.subimage)) else {
            ui.label("decoding…");
            return;
        };
        let mut chosen_part = self.subimage;
        match &entry.result {
            Err(message) => {
                ui.colored_label(ERROR_TEXT, truncate_chars(message, 120));
            }
            Ok(frame) => {
                egui::Grid::new("image_info").num_columns(2).show(ui, |ui| {
                    ui.label("Size");
                    ui.label(format!("{} x {}", frame.width, frame.height));
                    ui.end_row();
                    ui.label("Channels");
                    let names = frame.spec.channel_names().join(", ");
                    ui.label(format!(
                        "{} ({})",
                        frame.channels,
                        truncate_chars(&names, 40)
                    ));
                    ui.end_row();
                    ui.label("Format");
                    ui.label(frame.spec.format().to_string());
                    ui.end_row();
                    // Ordinary files have equal windows; only a crop or an
                    // overscan is worth two extra rows.
                    let [full_width, full_height, _] = frame.spec.full_dimensions();
                    let [x, y, _] = frame.spec.origin();
                    let [full_x, full_y, _] = frame.spec.full_origin();
                    if (full_width > 0 && full_height > 0)
                        && ([full_width, full_height] != [frame.width, frame.height]
                            || [x, y] != [full_x, full_y])
                    {
                        ui.label("Data window");
                        ui.label(format!("{}x{} at {x},{y}", frame.width, frame.height));
                        ui.end_row();
                        ui.label("Display window");
                        ui.label(format!("{full_width}x{full_height} at {full_x},{full_y}"));
                        ui.end_row();
                    }
                    if let Some(colorspace) = frame
                        .spec
                        .attribute("oiio:ColorSpace")
                        .and_then(|value| value.as_str())
                    {
                        ui.label("Colorspace");
                        ui.label(colorspace.to_owned());
                        ui.end_row();
                    }
                    if frame.subimage_count > 1 {
                        ui.label("Part");
                        egui::ComboBox::from_id_salt("part")
                            .selected_text(format!(
                                "{}/{}: {}",
                                chosen_part + 1,
                                frame.subimage_count,
                                frame.part_label(chosen_part)
                            ))
                            .show_ui(ui, |ui| {
                                for part in 0..frame.subimage_count {
                                    let label = format!("{}: {}", part + 1, frame.part_label(part));
                                    ui.selectable_value(&mut chosen_part, part, label);
                                }
                            });
                        ui.end_row();
                    }
                });
            }
        }
        if chosen_part != self.subimage {
            self.subimage = chosen_part;
            self.bump_generation();
        }
    }

    /// Every attribute of the current frame's spec, name against value.
    fn metadata_group(&self, ui: &mut egui::Ui) {
        let Some(entry) = self.cache.get(&(self.index, self.subimage)) else {
            ui.label("decoding…");
            return;
        };
        let Ok(frame) = &entry.result else {
            ui.label("no metadata: the frame failed to decode");
            return;
        };
        let attributes = frame.spec.attributes();
        if attributes.is_empty() {
            ui.label("the file carries no attributes");
            return;
        }
        egui::Grid::new("metadata")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (name, value) in attributes {
                    // One line per attribute; a long value is cut short and
                    // shown whole on hover.
                    let full = value.to_string().replace(['\n', '\r'], " ");
                    let shown = truncate_chars(&full, 48);
                    ui.label(egui::RichText::new(name).monospace());
                    ui.label(egui::RichText::new(shown).monospace())
                        .on_hover_text(full);
                    ui.end_row();
                }
            });
    }

    /// Zoom about the cursor with the scroll wheel and pan by dragging; any
    /// hand adjustment leaves fit mode.
    fn interact_view(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: egui::Rect,
        image_size: egui::Vec2,
    ) {
        let fit_zoom =
            (rect.width() / image_size.x.max(1.0)).min(rect.height() / image_size.y.max(1.0));
        if self.fit {
            self.zoom = fit_zoom;
            self.pan = egui::Vec2::ZERO;
        }
        if let Some(cursor) = response.hover_pos() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let old_zoom = self.zoom;
                let low = (fit_zoom / 32.0).min(1.0);
                let high = 128.0_f32.max(fit_zoom);
                let new_zoom = (old_zoom * (scroll * 0.003).exp()).clamp(low, high);
                // The image point under the cursor stays under the cursor.
                let offset = cursor - rect.center() - self.pan;
                self.pan += offset * (1.0 - new_zoom / old_zoom);
                self.zoom = new_zoom;
                self.fit = false;
            }
        }
        let dragged = response.drag_delta();
        if dragged != egui::Vec2::ZERO {
            self.pan += dragged;
            self.fit = false;
        }
    }

    /// The image itself, or the placeholder for a failed or pending decode.
    fn central_panel_ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(LETTERBOX))
            .show(ui, |ui| {
                let (response, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
                let rect = response.rect;
                if self.paths.is_empty() {
                    let hint = match &self.notice {
                        Some(notice) => notice.clone(),
                        None => "File → Open, Ctrl+O, or drop images here".to_owned(),
                    };
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        hint,
                        egui::FontId::proportional(16.0),
                        egui::Color32::GRAY,
                    );
                    return;
                }
                // The cache lookup is split from the drawing so the view
                // state can be mutated without fighting the borrow.
                enum State {
                    Ready {
                        /// The display window — what fit and framing use.
                        view_size: egui::Vec2,
                        /// The data window: the pixels that actually exist.
                        data_size: egui::Vec2,
                        /// Data-window origin relative to the display window.
                        data_offset: egui::Vec2,
                    },
                    Failed(String),
                    Decoding,
                }
                let state = match self.cache.get(&(self.index, self.subimage)) {
                    Some(entry) => match &entry.result {
                        Ok(frame) => {
                            // An EXR's data window can crop into or overscan
                            // past its display window; framing follows the
                            // display window, as review tools do, with the
                            // pixels placed where they belong inside it.
                            let data_size = egui::vec2(frame.width as f32, frame.height as f32);
                            let [full_width, full_height, _] = frame.spec.full_dimensions();
                            let [x, y, _] = frame.spec.origin();
                            let [full_x, full_y, _] = frame.spec.full_origin();
                            let (view_size, data_offset) = if full_width > 0 && full_height > 0 {
                                (
                                    egui::vec2(full_width as f32, full_height as f32),
                                    egui::vec2((x - full_x) as f32, (y - full_y) as f32),
                                )
                            } else {
                                (data_size, egui::Vec2::ZERO)
                            };
                            State::Ready {
                                view_size,
                                data_size,
                                data_offset,
                            }
                        }
                        Err(message) => State::Failed(message.clone()),
                    },
                    None => State::Decoding,
                };
                match state {
                    State::Ready {
                        view_size,
                        data_size,
                        data_offset,
                    } => {
                        self.interact_view(ui, &response, rect, view_size);
                        let view_rect = egui::Rect::from_center_size(
                            rect.center() + self.pan,
                            view_size * self.zoom,
                        );
                        let image_rect = egui::Rect::from_min_size(
                            view_rect.min + data_offset * self.zoom,
                            data_size * self.zoom,
                        );
                        if let Some(texture) = &self.texture {
                            let uv = egui::Rect::from_min_max(
                                egui::pos2(0.0, 0.0),
                                egui::pos2(1.0, 1.0),
                            );
                            painter.image(texture.id(), image_rect, uv, egui::Color32::WHITE);
                        }
                        // When the windows differ, both get outlines the way
                        // review tools draw them: the display window as a
                        // solid frame, the data window as a dashed bounding
                        // box — its pixels often fade to black, so without
                        // the box its true extent is invisible.
                        if image_rect != view_rect {
                            painter.rect_stroke(
                                view_rect,
                                egui::CornerRadius::ZERO,
                                egui::Stroke::new(1.0, egui::Color32::from_gray(0x50)),
                                egui::StrokeKind::Outside,
                            );
                            let bbox =
                                egui::Stroke::new(1.0, egui::Color32::from_rgb(0xb8, 0x86, 0x3a));
                            let corners = [
                                image_rect.left_top(),
                                image_rect.right_top(),
                                image_rect.right_bottom(),
                                image_rect.left_bottom(),
                                image_rect.left_top(),
                            ];
                            for edge in corners.windows(2) {
                                painter.extend(egui::Shape::dashed_line(edge, bbox, 4.0, 4.0));
                            }
                        }
                    }
                    State::Failed(message) => {
                        painter.rect_filled(rect, egui::CornerRadius::ZERO, ERROR_BACKGROUND);
                        ui.put(
                            rect.shrink(24.0),
                            egui::Label::new(
                                egui::RichText::new(format!("load failed:\n{message}"))
                                    .color(ERROR_TEXT),
                            )
                            .wrap(),
                        );
                    }
                    State::Decoding => {
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "decoding…",
                            egui::FontId::proportional(16.0),
                            egui::Color32::GRAY,
                        );
                    }
                }
                // Playback holding for a slow decode says so in the corner.
                if self.playing && self.paths.len() > 1 {
                    let next = (self.index + 1) % self.paths.len();
                    if !self.cache.contains_key(&(next, 0)) {
                        painter.text(
                            rect.left_bottom() + egui::vec2(12.0, -12.0),
                            egui::Align2::LEFT_BOTTOM,
                            "decoding…",
                            egui::FontId::proportional(13.0),
                            egui::Color32::GRAY,
                        );
                    }
                }
            });
    }

    /// The window title: name, position in the sequence, exposure, part info
    /// for multi-part files, and the load error if there is one.
    fn window_title(&self) -> String {
        if self.paths.is_empty() {
            return "oiio-viewer".to_owned();
        }
        let current = &self.paths[self.index];
        let name = current
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| current.display().to_string());
        let mut title = format!(
            "oiio-viewer — {name} [frame {}/{}] [exposure {:+.1}]",
            self.index + 1,
            self.paths.len(),
            self.exposure_stops
        );
        match self
            .cache
            .get(&(self.index, self.subimage))
            .map(|entry| &entry.result)
        {
            Some(Ok(frame)) if frame.subimage_count > 1 => {
                title.push_str(&format!(
                    " [part {}/{}: {}]",
                    self.subimage + 1,
                    frame.subimage_count,
                    frame.part_label(self.subimage)
                ));
            }
            Some(Err(message)) => {
                // Titles are one line, so keep only the start of the error.
                let brief: String = message
                    .lines()
                    .next()
                    .unwrap_or("unknown error")
                    .chars()
                    .take(120)
                    .collect();
                title.push_str(&format!(" — load failed: {brief}"));
            }
            _ => {}
        }
        // The base layer goes without saying; any other is named.
        if let Some(frame) = self.current_frame() {
            let layer = self.resolve_layer(&frame.layers);
            if layer > 0 {
                title.push_str(&format!(" [layer {}]", frame.layers[layer].name));
            }
        }
        title
    }

    /// Push the title to the window, but only when it changed.
    fn update_title(&mut self, ctx: &egui::Context) {
        let title = self.window_title();
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain_replies();
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            self.open_dropped(dropped);
        }
        self.handle_keys(&ctx);
        self.advance_playback(&ctx);
        self.menu_panel(ui);
        self.transport_panel(ui);
        self.inspector_panel(ui);
        // Requests reflect everything the keys and panels changed above.
        self.schedule_decodes();
        self.refresh_texture(&ctx);
        self.central_panel_ui(ui);
        self.update_title(&ctx);
    }
}

/// Apply the display transform — exposure gain, channel isolation, sRGB
/// encode — to one layer of a linear frame, producing the image egui
/// uploads.
///
/// A frame larger than the GPU's texture limit is subsampled to fit rather
/// than refused; it is drawn at its logical size regardless, so zoom and
/// fit stay in image pixels and only the texels coarsen.
fn build_display_image(
    frame: &Frame,
    gain: f32,
    view: ChannelView,
    max_side: usize,
    layer: &decode::Layer,
) -> egui::ColorImage {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let channels = frame.channels as usize;
    let step = width.max(height).div_ceil(max_side).max(1);
    let out_width = width.div_ceil(step);
    let out_height = height.div_ceil(step);
    let alpha_index = layer.alpha;
    let mut rgb = Vec::with_capacity(out_width * out_height * 3);
    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            let texel = &frame.pixels[(y * width + x) * channels..][..channels];
            // One colour channel shows as grey, two as red and green, three
            // or more as RGB; a layer that is only an alpha shows that.
            let (r, g, b) = match layer.color.as_slice() {
                [] => {
                    let alpha = alpha_index.map_or(0.0, |index| texel[index]);
                    (alpha, alpha, alpha)
                }
                [only] => (texel[*only], texel[*only], texel[*only]),
                [first, second] => (texel[*first], texel[*second], 0.0),
                [first, second, third, ..] => (texel[*first], texel[*second], texel[*third]),
            };
            let encoded: [u8; 3] = match view {
                ChannelView::Rgb => [
                    decode::encode_channel(r, gain),
                    decode::encode_channel(g, gain),
                    decode::encode_channel(b, gain),
                ],
                ChannelView::Red => [decode::encode_channel(r, gain); 3],
                ChannelView::Green => [decode::encode_channel(g, gain); 3],
                ChannelView::Blue => [decode::encode_channel(b, gain); 3],
                ChannelView::Luma => {
                    // Rec. 709 weights on the linear values.
                    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    [decode::encode_channel(luma, gain); 3]
                }
                ChannelView::Alpha => {
                    // Alpha is coverage rather than light, so exposure leaves it
                    // alone; a frame without alpha is fully opaque.
                    let alpha = alpha_index.map_or(1.0, |index| texel[index]);
                    [decode::encode_channel(alpha, 1.0); 3]
                }
            };
            rgb.extend_from_slice(&encoded);
        }
    }
    egui::ColorImage::from_rgb([out_width, out_height], &rgb)
}

/// Cut `text` to at most `limit` characters, marking the cut with an ellipsis.
fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_owned()
    } else {
        let mut cut: String = text.chars().take(limit).collect();
        cut.push('…');
        cut
    }
}
