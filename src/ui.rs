#![allow(unsafe_op_in_unsafe_fn)]

use std::num::NonZeroU32;
use std::os::raw::c_int;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cosmic_text::{Attrs, Color, FontSystem, Metrics, Shaping, SwashCache};
use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{CursorIcon, Icon, Window, WindowId};

use crate::*;

static FONT_STATE: Mutex<Option<FontState>> = Mutex::new(None);

struct FontState {
    system: FontSystem,
    buffer: cosmic_text::Buffer,
    metrics: Metrics,
    cache: SwashCache,
}

fn with_font_state<T>(f: impl FnOnce(&mut FontState) -> T) -> T {
    let mut guard = FONT_STATE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(FontState::new());
    }
    f(guard.as_mut().unwrap())
}

impl FontState {
    fn new() -> Self {
        let mut system = FontSystem::new();
        let metrics = Metrics::new(13.0, 20.0);
        let buffer = cosmic_text::Buffer::new(&mut system, metrics);
        let cache = SwashCache::new();
        FontState {
            system,
            buffer,
            metrics,
            cache,
        }
    }

    fn shape(&mut self, text: &str, width: f32, height: f32) {
        self.buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
        self.buffer.set_size(Some(width), Some(height));
        self.buffer.shape_until_scroll(&mut self.system, true);
    }

    fn ascent_descent(&self) -> (f32, f32) {
        self.buffer
            .lines
            .first()
            .and_then(|line| line.layout_opt())
            .and_then(|layout| layout.first())
            .map(|line| (line.max_ascent, line.max_descent))
            .unwrap_or((13.0, 3.0))
    }

    fn line_width(&self) -> f32 {
        self.buffer
            .lines
            .first()
            .and_then(|line| line.layout_opt())
            .and_then(|layout| layout.first())
            .map(|line| line.w)
            .unwrap_or(0.0)
    }
}

pub fn text_width(text: &str) -> c_int {
    if text.is_empty() {
        return 0;
    }
    with_font_state(|state| {
        state.shape(text, 10000.0, 1000.0);
        state.line_width().ceil() as c_int
    })
}

fn text_metrics(text: &str) -> (f32, f32) {
    if text.is_empty() {
        return (13.0, 3.0);
    }
    with_font_state(|state| {
        state.shape(text, 10000.0, 1000.0);
        state.ascent_descent()
    })
}

fn centered_baseline(box_y: c_int, box_height: c_int, text: &str) -> c_int {
    let (ascent, descent) = text_metrics(text);
    box_y + ((box_height as f32 - (ascent + descent)) / 2.0).round() as c_int
        + ascent.round() as c_int
}

pub struct Canvas<'a> {
    pub width: c_int,
    pub height: c_int,
    buf: &'a mut [u32],
    fg: u32,
    origin_x: c_int,
    origin_y: c_int,
}

impl<'a> Canvas<'a> {
    pub fn new(width: c_int, height: c_int, buf: &'a mut [u32]) -> Self {
        Canvas {
            width,
            height,
            buf,
            fg: 0,
            origin_x: 0,
            origin_y: 0,
        }
    }

    pub fn with_origin<F, R>(&mut self, ox: c_int, oy: c_int, f: F) -> R
    where
        F: FnOnce(&mut Canvas) -> R,
    {
        let old = (self.origin_x, self.origin_y);
        self.origin_x += ox;
        self.origin_y += oy;
        let result = f(self);
        self.origin_x = old.0;
        self.origin_y = old.1;
        result
    }

    pub fn set_fg(&mut self, color: u32) {
        self.fg = color;
    }

    #[inline]
    pub fn put_pixel(&mut self, x: c_int, y: c_int, color: u32) {
        let x = x + self.origin_x;
        let y = y + self.origin_y;
        if x >= 0 && y >= 0 && x < self.width && y < self.height {
            self.buf[(y as usize) * self.width as usize + x as usize] = color;
        }
    }

    #[inline]
    pub fn fill_rect(&mut self, x: c_int, y: c_int, w: c_int, h: c_int) {
        if w <= 0 || h <= 0 {
            return;
        }
        let color = self.fg;
        let x0 = (x + self.origin_x).max(0);
        let y0 = (y + self.origin_y).max(0);
        let x1 = (x + w + self.origin_x).min(self.width);
        let y1 = (y + h + self.origin_y).min(self.height);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        for yy in y0..y1 {
            let start = (yy as usize) * self.width as usize + x0 as usize;
            let end = (yy as usize) * self.width as usize + x1 as usize;
            for slot in &mut self.buf[start..end] {
                *slot = color;
            }
        }
    }

    pub fn rect(&mut self, x: c_int, y: c_int, w: c_int, h: c_int) {
        self.fill_rect(x, y, w, 1);
        self.fill_rect(x, y + h - 1, w, 1);
        self.fill_rect(x, y, 1, h);
        self.fill_rect(x + w - 1, y, 1, h);
    }

    pub fn line(&mut self, x0: c_int, y0: c_int, x1: c_int, y1: c_int) {
        let color = self.fg;
        let dx = (x1 - x0).abs() as f64;
        let dy = (y1 - y0).abs() as f64;
        let steps = dx.max(dy) as c_int;
        if steps <= 0 {
            self.put_pixel(x0, y0, color);
            return;
        }
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let x = (x0 as f64 + t * (x1 - x0) as f64).round() as c_int;
            let y = (y0 as f64 + t * (y1 - y0) as f64).round() as c_int;
            self.put_pixel(x, y, color);
        }
    }

    pub fn polyline(&mut self, points: &[(c_int, c_int)], close: bool) {
        let n = points.len();
        for i in 0..n {
            let next = if i + 1 < n { i + 1 } else if close { 0 } else { continue };
            self.line(points[i].0, points[i].1, points[next].0, points[next].1);
        }
    }

    pub fn polygon(&mut self, points: &[(c_int, c_int)]) {
        if points.len() < 3 {
            return;
        }
        let color = self.fg;
        let min_y = points.iter().map(|p| p.1).min().unwrap();
        let max_y = points.iter().map(|p| p.1).max().unwrap();
        let mut xs = Vec::with_capacity(points.len());
        let n = points.len();
        for y in min_y..=max_y {
            xs.clear();
            for i in 0..n {
                let a = points[i];
                let b = points[(i + 1) % n];
                let (top, bottom) = if a.1 <= b.1 { (a, b) } else { (b, a) };
                if top.1 <= y && y < bottom.1 {
                    let t = (y - top.1) as f64 / (bottom.1 - top.1) as f64;
                    xs.push((top.0 as f64 + t * (bottom.0 - top.0) as f64).round() as c_int);
                }
            }
            xs.sort_unstable();
            for pair in xs.chunks(2) {
                if pair.len() == 2 {
                    let (a, b) = (pair[0].min(pair[1]), pair[0].max(pair[1]));
                    for x in a..=b {
                        self.put_pixel(x, y, color);
                    }
                }
            }
        }
    }

    pub fn arc(
        &mut self,
        cx: c_int,
        cy: c_int,
        rx: c_int,
        ry: c_int,
        start_deg: f64,
        sweep_deg: f64,
    ) {
        let steps = ((sweep_deg.abs()) / 2.0).ceil() as usize + 1;
        let mut prev: Option<(c_int, c_int)> = None;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let angle = (start_deg + sweep_deg * t).to_radians();
            let x = (cx as f64 + rx as f64 * angle.cos()).round() as c_int;
            let y = (cy as f64 - ry as f64 * angle.sin()).round() as c_int;
            if let Some(p) = prev {
                self.line(p.0, p.1, x, y);
            }
            prev = Some((x, y));
        }
    }

    pub fn circle(&mut self, cx: c_int, cy: c_int, r: c_int) {
        self.arc(cx, cy, r, r, 0.0, 360.0);
    }

    pub fn fill_circle(&mut self, cx: c_int, cy: c_int, r: c_int) {
        for dy in -r..=r {
            let half = ((r * r - dy * dy) as f64).sqrt().round() as c_int;
            self.fill_rect(cx - half, cy + dy, half * 2 + 1, 1);
        }
    }

    pub fn dim(&mut self, alpha: u32) {
        let inv = 255u32.saturating_sub(alpha);
        for slot in self.buf.iter_mut() {
            let dr = (*slot >> 16) & 0xFF;
            let dg = (*slot >> 8) & 0xFF;
            let db = *slot & 0xFF;
            *slot = ((dr * inv / 255) << 16) | ((dg * inv / 255) << 8) | (db * inv / 255);
        }
    }

    pub fn text_baseline(&mut self, x: c_int, baseline: c_int, text: &str) {
        if text.is_empty() {
            return;
        }
        let fg = self.fg;
        let canvas_width = self.width;
        let canvas_height = self.height;
        let origin_x = self.origin_x;
        let origin_y = self.origin_y;
        let mut pixel_spans: Vec<(c_int, c_int, Color)> = Vec::new();
        with_font_state(|state| {
            state.shape(text, 10000.0, canvas_height as f32);
            for run in state.buffer.lines[0].layout_runs(Some(10000.0), state.metrics.line_height) {
                for glyph in run.glyphs {
                    let physical = glyph.physical((0.0, baseline as f32), 1.0);
                    state.cache.with_pixels(
                        &mut state.system,
                        physical.cache_key,
                        Color(fg),
                        |px, py, color| {
                            pixel_spans.push((physical.x + px, physical.y + py, color));
                        },
                    );
                }
            }
        });
        for (px, py, color) in pixel_spans {
            let x = px + x + origin_x;
            let y = py + origin_y;
            if x < 0 || y < 0 || x >= canvas_width || y >= canvas_height {
                continue;
            }
            let c = color.0 as u32;
            let alpha = (c >> 24) & 0xFF;
            let index = (y as usize) * canvas_width as usize + x as usize;
            if alpha == 255 {
                self.buf[index] = c & 0xFFFFFF;
            } else if alpha > 0 {
                let dst = self.buf[index];
                let dr = (dst >> 16) & 0xFF;
                let dg = (dst >> 8) & 0xFF;
                let db = dst & 0xFF;
                let fr = (c >> 16) & 0xFF;
                let fg_ = (c >> 8) & 0xFF;
                let fb = c & 0xFF;
                let inv = 255 - alpha;
                self.buf[index] = ((fr * alpha + dr * inv) / 255) << 16
                    | ((fg_ * alpha + dg * inv) / 255) << 8
                    | ((fb * alpha + db * inv) / 255);
            }
        }
    }

    pub fn text_centered(&mut self, x: c_int, box_y: c_int, box_height: c_int, text: &str) {
        if text.is_empty() {
            return;
        }
        self.text_baseline(x, centered_baseline(box_y, box_height, text), text);
    }

    pub fn image(
        &mut self,
        rgba: &[u8],
        src_width: u32,
        src_height: u32,
        x: c_int,
        y: c_int,
        out_width: c_int,
        out_height: c_int,
        background: Option<u32>,
    ) {
        if out_width <= 0 || out_height <= 0 || src_width == 0 || src_height == 0 {
            return;
        }
        let sw = src_width as usize;
        let sh = src_height as usize;
        if rgba.len() < sw * sh * 4 {
            return;
        }
        let bg = background.map(|color| {
            (
                ((color >> 16) & 0xFF) as u32,
                ((color >> 8) & 0xFF) as u32,
                (color & 0xFF) as u32,
            )
        });
        let sw = src_width as usize;
        let sh = src_height as usize;
        for row in 0..out_height as usize {
            let src_row = (row * sh) / out_height as usize;
            let dst_y = row as c_int + y;
            if dst_y < 0 || dst_y >= self.height {
                continue;
            }
            for col in 0..out_width as usize {
                let src_col = (col * sw) / out_width as usize;
                let index = (src_row * sw + src_col) * 4;
                let r0 = rgba[index] as u32;
                let g0 = rgba[index + 1] as u32;
                let b0 = rgba[index + 2] as u32;
                let alpha = rgba[index + 3] as u32;
                let dst_x = col as c_int + x;
                if dst_x < 0 || dst_x >= self.width {
                    continue;
                }
                let (r, g, b) = if let Some((br, bg, bb)) = bg {
                    let inv = 255 - alpha;
                    (
                        (r0 * alpha + br * inv) / 255,
                        (g0 * alpha + bg * inv) / 255,
                        (b0 * alpha + bb * inv) / 255,
                    )
                } else if alpha == 255 {
                    (r0, g0, b0)
                } else {
                    let dst = self.buf[(dst_y as usize) * self.width as usize + dst_x as usize];
                    let dr = (dst >> 16) & 0xFF;
                    let dg = (dst >> 8) & 0xFF;
                    let db = dst & 0xFF;
                    let inv = 255 - alpha;
                    (
                        (r0 * alpha + dr * inv) / 255,
                        (g0 * alpha + dg * inv) / 255,
                        (b0 * alpha + db * inv) / 255,
                    )
                };
                self.buf[(dst_y as usize) * self.width as usize + dst_x as usize] =
                    (r << 16) | (g << 8) | b;
            }
        }
    }
}

enum Modal {
    Settings {
        working: Settings,
        url_focused: bool,
        url_cursor: usize,
    },
    Shield {
        state: ConnectionState,
        message: String,
        opened: Instant,
    },
    About {
        tab: u8,
    },
}

struct App {
    app: BrowserApp,
    modal: Option<Modal>,
    quit: bool,
    mods: KeyMods,
    window: Option<Arc<Window>>,
    context: Option<Context<Arc<Window>>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    mouse_x: f64,
    mouse_y: f64,
    scale: f64,
}

pub fn run_window(app: BrowserApp) {
    let mut handler = App {
        app,
        modal: None,
        quit: false,
        mods: KeyMods::default(),
        window: None,
        context: None,
        surface: None,
        mouse_x: 0.0,
        mouse_y: 0.0,
        scale: 1.0,
    };
    let event_loop = EventLoop::new().expect("failed to create winit event loop");
    let _ = event_loop.run_app(&mut handler);
}

fn sync_link_cursor(window: &Option<Arc<Window>>, over_link: bool) {
    if let Some(window) = window {
        let _ = window.set_cursor(if over_link {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        });
    }
}

fn paste_clipboard(app: &mut BrowserApp) {
    if !app.clipboard_text.is_empty() {
        let text = app.clipboard_text.clone();
        app.insert_text(&text);
    }
}

impl App {
    fn redraw(&mut self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn surface_size(&self) -> Option<(c_int, c_int)> {
        let window = self.window.as_ref()?;
        let size = window.inner_size();
        let scale = window.scale_factor();
        Some((
            ((size.width as f64 / scale).round() as c_int).max(1),
            ((size.height as f64 / scale).round() as c_int).max(1),
        ))
    }

    fn modal_origin(&self, w: c_int, h: c_int) -> (c_int, c_int) {
        let (sw, sh) = (
            self.app.window_width as c_int,
            self.app.window_height as c_int,
        );
        (((sw - w).max(0) / 2), ((sh - h).max(0) / 2))
    }

    fn draw_all(&mut self) {
        if self.quit {
            return;
        }
        let Some((width, height)) = self.surface_size() else {
            return;
        };
        let about_origin = self.modal_origin(ABOUT_W, ABOUT_H);
        let settings_origin = self.modal_origin(SETTINGS_W, SETTINGS_H);
        let shield_origin = self.modal_origin(SHIELD_W, SHIELD_H);
        let Some(surface) = &mut self.surface else {
            return;
        };
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };
        let len = (width as usize).saturating_mul(height as usize);
        let buflen = buffer.len();
        if len > buflen {
            return;
        }
        let buf = &mut buffer[..len];
        let mut canvas = Canvas::new(width, height, buf);
        match &self.modal {
            Some(Modal::About { tab }) => {
                draw_browser(&self.app, &mut canvas);
                canvas.dim(120);
                let (mx, my) = about_origin;
                canvas.with_origin(mx, my, |c| {
                    draw_about_content(*tab, c);
                    c.set_fg(pal(COLOR_PAGE_BORDER));
                    c.rect(0, 0, 520, 240);
                });
            }
            Some(Modal::Settings {
                working,
                url_focused,
                url_cursor,
            }) => {
                draw_browser(&self.app, &mut canvas);
                canvas.dim(120);
                let (mx, my) = settings_origin;
                canvas.with_origin(mx, my, |c| {
                    draw_settings_content(working, *url_focused, *url_cursor, c);
                    c.set_fg(pal(COLOR_PAGE_BORDER));
                    c.rect(0, 0, SETTINGS_W, SETTINGS_H);
                });
            }
            Some(Modal::Shield { state, message, .. }) => {
                draw_browser(&self.app, &mut canvas);
                canvas.dim(120);
                let (mx, my) = shield_origin;
                canvas.with_origin(mx, my, |c| {
                    draw_shield_content(*state, message, c);
                    c.set_fg(pal(COLOR_PAGE_BORDER));
                    c.rect(0, 0, SHIELD_W, SHIELD_H);
                });
            }
            None => draw_browser(&self.app, &mut canvas),
        }
        let _ = buffer.present();
    }

    fn on_mouse_down(&mut self, button: MouseButton, x: c_int, y: c_int) {
        eprintln!(
            "ghostab-log: mouse_down button={:?} x={} y={}",
            button, x, y
        );
        let mut modal_close_save: Option<Settings> = None;
        let mut modal_close = false;
        let about_origin = self.modal_origin(ABOUT_W, ABOUT_H);
        let settings_origin = self.modal_origin(SETTINGS_W, SETTINGS_H);
        match &mut self.modal {
            Some(Modal::About { tab }) => {
                let (mx, my) = about_origin;
                let (lx, ly) = (x - mx, y - my);
                if lx < 0 || ly < 0 || lx >= ABOUT_W || ly >= ABOUT_H {
                    modal_close = true;
                } else if let Some(t) = about_tab_at(lx, ly) {
                    if t != *tab {
                        *tab = t;
                    }
                } else if about_close_at(lx, ly) {
                    modal_close = true;
                }
            }
            Some(Modal::Settings {
                working,
                url_focused,
                url_cursor,
            }) => {
                let (mx, my) = settings_origin;
                let (lx, ly) = (x - mx, y - my);
                if lx < 0 || ly < 0 || lx >= SETTINGS_W || ly >= SETTINGS_H {
                    modal_close = true;
                } else if settings_light_row_at(lx, ly) {
                    working.light_mode = !working.light_mode;
                    apply_settings(working);
                } else if let Some(engine) = settings_engine_at(lx, ly) {
                    working.search_engine = SearchEngine::all()[engine];
                    *url_cursor = working.search_url.len();
                    if working.search_engine != SearchEngine::Custom {
                        *url_focused = false;
                    }
                } else if settings_url_at(lx, ly) {
                    *url_focused = true;
                    *url_cursor = working.search_url.len();
                } else if settings_ok_at(lx, ly) {
                    modal_close_save = Some(working.clone());
                    modal_close = true;
                } else if settings_cancel_at(lx, ly) {
                    modal_close = true;
                }
            }
            Some(Modal::Shield { .. }) => {
                modal_close = true;
            }
            None => {}
        }
        if modal_close {
            if let Some(settings) = modal_close_save {
                self.app.settings = settings.clone();
                apply_settings(&settings);
                save_settings(&settings);
            } else {
                apply_settings(&self.app.settings);
            }
            self.modal = None;
            return;
        }
        if self.modal.is_some() {
            return;
        }

        let app = &mut self.app;
        if button == MouseButton::Middle {
            let should_quit = match tab_at(x, y, app.window_width, &tab_titles(app)) {
                Some(TabHit::Tab(index)) | Some(TabHit::Close(index)) => {
                    eprintln!("ghostab-log: middle-click -> close tab {index}");
                    app.close_tab(index)
                }
                _ => false,
            };
            if should_quit {
                self.quit = true;
            }
            return;
        }
        if button != MouseButton::Left {
            return;
        }

        let mut consumed = false;
        if let Some(menu) = app.open_menu {
            if let Some(item) = menu_item_at(menu, x, y) {
                let action = menu_item_action(menu, item);
                app.open_menu = None;
                app.menu_hover = None;
                consumed = true;
                let mut quit = false;
                match action {
                    MenuAction::Reload => app.reload(),
                    MenuAction::Settings => {
                        let working = app.settings.clone();
                        let url_cursor = working.search_url.len();
                        apply_settings(&working);
                        self.modal = Some(Modal::Settings {
                            working,
                            url_focused: false,
                            url_cursor,
                        });
                    }
                    MenuAction::Quit => quit = true,
                    MenuAction::SelectAll => {
                        if app.address_focused {
                            app.select_all();
                        }
                    }
                    MenuAction::Copy => {
                        if app.address_focused {
                            if let Some(selection) = app.copy_selection() {
                                app.clipboard_text = selection;
                            }
                        }
                    }
                    MenuAction::Paste => {
                        if app.address_focused {
                            paste_clipboard(app);
                        }
                    }
                    MenuAction::ScrollTop => app.scroll_home(),
                    MenuAction::ScrollUp => app.scroll_by(-SCROLL_STEP),
                    MenuAction::ScrollDown => app.scroll_by(SCROLL_STEP),
                    MenuAction::ScrollBottom => app.scroll_end(),
                    MenuAction::About => {
                        self.modal = Some(Modal::About { tab: 0 });
                    }
                    MenuAction::None => {}
                }
                if quit {
                    self.quit = true;
                }
            } else if let Some(btn) = point_in_menu_button(x, y) {
                app.open_menu = if btn == menu { None } else { Some(btn) };
                app.menu_hover = None;
                consumed = true;
            } else {
                app.open_menu = None;
                app.menu_hover = None;
                consumed = true;
            }
        }
        if !consumed {
            if let Some(btn) = point_in_menu_button(x, y) {
                app.open_menu = Some(btn);
                app.menu_hover = None;
            } else if let Some(hit) = tab_at(x, y, app.window_width, &tab_titles(app)) {
                eprintln!("ghostab-log: click -> tab {:?}", hit);
                let mut should_quit = false;
                match hit {
                    TabHit::Tab(index) => app.switch_tab(index),
                    TabHit::Close(index) => should_quit = app.close_tab(index),
                    TabHit::NewTab => app.new_tab(),
                }
                sync_link_cursor(&self.window, false);
                if should_quit {
                    self.quit = true;
                }
            } else if point_in_shield(x, y) {
                eprintln!("ghostab-log: click -> shield");
                let state = connection_state(&app.page.source);
                let message = shield_message(state);
                self.modal = Some(Modal::Shield {
                    state,
                    message,
                    opened: Instant::now(),
                });
                sync_link_cursor(&self.window, false);
            } else if let Some(button) = nav_button_at(x, y, app.window_width) {
                eprintln!("ghostab-log: click -> nav button {:?}", button);
                match button {
                    NavButton::Back => app.go_back(),
                    NavButton::Forward => app.go_forward(),
                    NavButton::Home => app.navigate_new("ghostab:newpage"),
                    NavButton::Refresh => app.reload(),
                }
                sync_link_cursor(&self.window, false);
            } else if point_in_go_button(x, y, app.window_width) {
                eprintln!("ghostab-log: click -> Go button");
                app.navigate();
                sync_link_cursor(&self.window, false);
            } else if point_in_address_bar(x, y, app.window_width) {
                eprintln!("ghostab-log: click -> address bar focused");
                app.address_focused = true;
                app.address_cursor = cursor_for_click(app, x);
                app.address_anchor = Some(app.address_cursor);
                app.mouse_down = true;
            } else if let Some(href) = find_link_at(&app.layout, x, y, app.scroll_y) {
                app.open_link(&href);
                sync_link_cursor(&self.window, false);
            } else {
                app.address_focused = false;
            }
        }
    }

    fn on_mouse_move(&mut self, x: c_int, y: c_int) {
        if self.modal.is_some() {
            return;
        }
        let app = &mut self.app;
        let mut changed = false;
        if let Some(menu) = app.open_menu {
            let hover = menu_item_at(menu, x, y).map(|item| (menu, item));
            if hover != app.menu_hover {
                app.menu_hover = hover;
                changed = true;
            }
        }
        if app.mouse_down && app.address_focused {
            let cursor = cursor_for_click(app, x);
            if cursor != app.address_cursor {
                app.address_cursor = cursor;
                changed = true;
            }
        }
        let hover = find_link_at(&app.layout, x, y, app.scroll_y);
        if hover != app.hover_href {
            let over_link = hover.is_some();
            app.hover_href = hover;
            sync_link_cursor(&self.window, over_link);
            changed = true;
        }
        let hover_button = nav_button_at(x, y, app.window_width);
        if hover_button != app.hover_button {
            app.hover_button = hover_button;
            changed = true;
        }
        let shield_hover = point_in_shield(x, y);
        if shield_hover != app.hover_shield {
            app.hover_shield = shield_hover;
            changed = true;
        }
        let hover_close = close_button_at(x, y, &tab_titles(app));
        if hover_close != app.hover_close {
            app.hover_close = hover_close;
            changed = true;
        }
        let _ = changed;
    }

    fn on_mouse_leave(&mut self) {
        self.app.hover_href = None;
        self.app.hover_button = None;
        self.app.hover_close = None;
        self.app.hover_shield = false;
        sync_link_cursor(&self.window, false);
    }

    fn on_key(&mut self, event: &KeyEvent) {
        let (input, mods, keysym) = read_key_event(event, self.mods);
        eprintln!(
            "ghostab-log: key_press input={} ctrl={} address_focused={} menu={:?}",
            log_input(&input),
            mods.ctrl,
            self.app.address_focused,
            self.app.open_menu
        );
        let mut redraw = true;

        enum ModalAction {
            Close,
            Save(Settings),
        }
        let mut modal_action: Option<ModalAction> = None;
        if let Some(modal) = &mut self.modal {
            match modal {
                Modal::About { .. } | Modal::Shield { .. } => {
                    if matches!(&input, KeyInput::Escape) {
                        modal_action = Some(ModalAction::Close);
                    }
                }
                Modal::Settings {
                    working,
                    url_focused,
                    url_cursor,
                } => {
                    if *url_focused {
                        let mut changed = false;
                        match &input {
                            KeyInput::Escape => {
                                *url_focused = false;
                                redraw = true;
                            }
                            KeyInput::Enter => {
                                modal_action = Some(ModalAction::Save(working.clone()));
                            }
                            KeyInput::Backspace => {
                                edit_backspace(&mut working.search_url, url_cursor);
                                changed = true;
                            }
                            KeyInput::Delete => {
                                edit_delete(&mut working.search_url, url_cursor);
                                changed = true;
                            }
                            KeyInput::Left => {
                                edit_move(&working.search_url, url_cursor, false);
                            }
                            KeyInput::Right => {
                                edit_move(&working.search_url, url_cursor, true);
                            }
                            KeyInput::Home => *url_cursor = 0,
                            KeyInput::End => *url_cursor = working.search_url.len(),
                            KeyInput::Text(text) => {
                                edit_insert(&mut working.search_url, url_cursor, text);
                                changed = true;
                            }
                            _ => {}
                        }
                        redraw = redraw || changed;
                    } else {
                        match &input {
                            KeyInput::Escape => modal_action = Some(ModalAction::Close),
                            KeyInput::Enter => {
                                modal_action = Some(ModalAction::Save(working.clone()));
                            }
                            _ => redraw = false,
                        }
                    }
                }
            }
        }
        if let Some(action) = modal_action {
            match action {
                ModalAction::Save(settings) => {
                    self.app.settings = settings.clone();
                    apply_settings(&settings);
                    save_settings(&settings);
                    self.modal = None;
                }
                ModalAction::Close => {
                    apply_settings(&self.app.settings);
                    self.modal = None;
                }
            }
        } else if self.app.open_menu.is_some() {
            match &input {
                KeyInput::Escape => {
                    self.app.open_menu = None;
                    self.app.menu_hover = None;
                }
                _ => redraw = false,
            }
        } else if self.app.address_focused {
            match &input {
                KeyInput::Escape => self.app.address_focused = false,
                KeyInput::Enter => self.app.navigate(),
                KeyInput::Backspace => self.app.delete_backward(),
                KeyInput::Delete => self.app.delete_forward(),
                KeyInput::Left => self.app.move_cursor(false, mods.shift),
                KeyInput::Right => self.app.move_cursor(true, mods.shift),
                KeyInput::Home => self.app.move_cursor_home(mods.shift),
                KeyInput::End => self.app.move_cursor_end(mods.shift),
                KeyInput::Text(text) => self.app.insert_text(text),
                KeyInput::Other if mods.ctrl => match keysym {
                    Some(KeySymChar::A) => self.app.select_all(),
                    Some(KeySymChar::C) => {
                        if let Some(selection) = self.app.copy_selection() {
                            self.app.clipboard_text = selection;
                        }
                    }
                    Some(KeySymChar::V) => paste_clipboard(&mut self.app),
                    _ => redraw = false,
                },
                _ => redraw = false,
            }
        } else if mods.ctrl {
            match keysym {
                Some(KeySymChar::T) => self.app.new_tab(),
                Some(KeySymChar::W) => {
                    if self.app.close_tab(self.app.active_tab) {
                        self.quit = true;
                    }
                }
                Some(KeySymChar::R) => self.app.reload(),
                Some(KeySymChar::L) => {
                    self.app.address_focused = true;
                    self.app.select_all();
                }
                Some(KeySymChar::D) => {
                    if self.app.close_tab(self.app.active_tab) {
                        self.quit = true;
                    }
                }
                Some(KeySymChar::Tab) => {
                    if mods.shift {
                        self.app.prev_tab();
                    } else {
                        self.app.next_tab();
                    }
                }
                _ => redraw = false,
            }
        } else {
            match &input {
                KeyInput::Escape => self.quit = true,
                KeyInput::PageUp => self.app.scroll_by(-(self.app.viewport_height() as c_int)),
                KeyInput::PageDown => self.app.scroll_by(self.app.viewport_height() as c_int),
                KeyInput::Home => self.app.scroll_home(),
                KeyInput::End => self.app.scroll_end(),
                _ => redraw = false,
            }
        }

        if redraw {
            self.redraw();
        }
    }

    fn on_resized(&mut self, width: u32, height: u32) {
        let scale = self
            .window
            .as_ref()
            .map(|window| window.scale_factor())
            .unwrap_or(1.0);
        let logical_w = (width as f64 / scale).round().max(1.0) as u32;
        let logical_h = (height as f64 / scale).round().max(1.0) as u32;
        self.app.resize(logical_w as usize, logical_h as usize);
        if let Some(surface) = &mut self.surface {
            let _ = surface.resize(
                NonZeroU32::new(width.max(1)).unwrap(),
                NonZeroU32::new(height.max(1)).unwrap(),
            );
        }
        self.redraw();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Ghostab")
                        .with_inner_size(LogicalSize::new(
                            WINDOW_WIDTH as f64,
                            WINDOW_HEIGHT as f64,
                        )),
                )
                .expect("failed to create window"),
        );
        if let Some(icon) = window_icon() {
            let _ = window.set_window_icon(Some(icon));
        }
        let context = Context::new(window.clone()).expect("failed to create render context");
        let size = window.inner_size();
        let mut surface = Surface::new(&context, window.clone())
            .expect("failed to create render surface");
        let _ = surface.resize(
            NonZeroU32::new(size.width.max(1)).unwrap(),
            NonZeroU32::new(size.height.max(1)).unwrap(),
        );
        self.scale = window.scale_factor();
        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
        self.redraw();
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.quit = true;
            }
            WindowEvent::Resized(size) => {
                self.on_resized(size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                self.draw_all();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if state == ElementState::Pressed {
                    self.on_mouse_down(
                        button,
                        self.mouse_x.round() as c_int,
                        self.mouse_y.round() as c_int,
                    );
                } else {
                    self.app.mouse_down = false;
                }
                self.redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let lx = (position.x / self.scale).round() as c_int;
                let ly = (position.y / self.scale).round() as c_int;
                self.mouse_x = position.x / self.scale;
                self.mouse_y = position.y / self.scale;
                self.on_mouse_move(lx, ly);
                self.redraw();
            }
            WindowEvent::CursorLeft { .. } => {
                self.on_mouse_leave();
                self.redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta_y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (y * 40.0) as c_int,
                    MouseScrollDelta::PixelDelta(position) => position.y.round() as c_int,
                };
                if delta_y != 0 {
                    self.app.scroll_by(-delta_y);
                    self.redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.mods = KeyMods {
                    ctrl: state.control_key(),
                    shift: state.shift_key(),
                };
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    self.on_key(&event);
                    self.redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.quit {
            event_loop.exit();
            return;
        }
        let mut close_shield = false;
        match &self.modal {
            Some(Modal::Shield { opened, .. }) => {
                let elapsed = opened.elapsed();
                if elapsed >= Duration::from_secs(8) {
                    close_shield = true;
                } else {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + (Duration::from_secs(8) - elapsed),
                    ));
                }
            }
            _ => {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
        if close_shield {
            self.modal = None;
            self.redraw();
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn window_icon() -> Option<Icon> {
    load_ghostab_image().and_then(|image| {
        Icon::from_rgba(image.rgba, image.width, image.height).ok()
    })
}

fn shield_message(state: ConnectionState) -> String {
    match state {
        ConnectionState::Protected => "This website encrypts what you send.".to_string(),
        ConnectionState::Local => "This website is stored on your computer.".to_string(),
        ConnectionState::BuiltIn => "This website is integrated into the browser.".to_string(),
        ConnectionState::Unprotected => {
            "This website does not encrypt what you send. Please do not send passwords, personal info, etc.".to_string()
        }
    }
}

fn draw_browser(app: &BrowserApp, canvas: &mut Canvas) {
    let width = app.window_width as c_int;
    let height = app.window_height as c_int;
    canvas.set_fg(pal(COLOR_PAGE));
    canvas.fill_rect(0, 0, width, height);
    canvas.set_fg(pal(COLOR_SURFACE));
    canvas.fill_rect(
        18,
        TITLE_BAR_HEIGHT as c_int + 18,
        (width - 36).max(0),
        (height - TITLE_BAR_HEIGHT as c_int - STATUS_BAR_HEIGHT as c_int - 36).max(0),
    );
    canvas.set_fg(pal(COLOR_PAGE_BORDER));
    canvas.rect(
        18,
        TITLE_BAR_HEIGHT as c_int + 18,
        (width - 36).max(0),
        (height - TITLE_BAR_HEIGHT as c_int - STATUS_BAR_HEIGHT as c_int - 36).max(0),
    );
    draw_title_bar(app, canvas);
    draw_menu_bar(app, canvas);
    draw_box(app, canvas, &app.layout, app.scroll_y, height);
    draw_scrollbar(app, canvas);
    draw_status_bar(app, canvas);
}

fn draw_title_bar(app: &BrowserApp, canvas: &mut Canvas) {
    let go_x = go_button_x(app.window_width);
    let address_width = address_bar_width(app.window_width) as c_int;

    canvas.set_fg(pal(COLOR_TITLE_BAR));
    canvas.fill_rect(0, 0, app.window_width as c_int, TITLE_BAR_HEIGHT as c_int);
    canvas.set_fg(pal(COLOR_TITLE_LINE));
    canvas.fill_rect(
        0,
        TITLE_BAR_HEIGHT as c_int,
        app.window_width as c_int,
        1,
    );

    draw_tab_bar(app, canvas);
    draw_shield_button(app, canvas);

    canvas.set_fg(pal(COLOR_ADDRESS_BG));
    canvas.fill_rect(ADDRESS_BAR_X, ADDRESS_Y, address_width, ADDRESS_HEIGHT as c_int);
    canvas.set_fg(pal(if app.address_focused {
        COLOR_ADDRESS_FOCUS
    } else {
        COLOR_ADDRESS_BORDER
    }));
    canvas.rect(ADDRESS_BAR_X, ADDRESS_Y, address_width, ADDRESS_HEIGHT as c_int);

    let avail = address_width - 28;
    let (shown, start) = visible_address(app, avail);
    canvas.set_fg(pal(COLOR_ADDRESS_TEXT));
    canvas.text_centered(
        ADDRESS_TEXT_X,
        ADDRESS_Y,
        ADDRESS_HEIGHT as c_int,
        &shown,
    );

    if let Some((sel_start, sel_end)) = app.selection_range() {
        let rel_start = sel_start.saturating_sub(start).min(shown.len());
        let rel_end = sel_end.saturating_sub(start).min(shown.len());
        if rel_end > rel_start {
            let x0 = ADDRESS_TEXT_X + text_width(&shown[..rel_start]);
            let x1 = ADDRESS_TEXT_X + text_width(&shown[..rel_end]);
            canvas.set_fg(pal(COLOR_SELECTION_BG));
            canvas.fill_rect(x0, ADDRESS_Y + 4, (x1 - x0).max(0), ADDRESS_HEIGHT as c_int - 8);
            canvas.set_fg(pal(COLOR_SELECTION_TEXT));
            canvas.text_centered(
                x0,
                ADDRESS_Y,
                ADDRESS_HEIGHT as c_int,
                &shown[rel_start..rel_end],
            );
        }
    }

    if app.address_focused {
        let rel_cursor = app.address_cursor.saturating_sub(start);
        let caret_x = ADDRESS_TEXT_X + text_width(&shown[..rel_cursor.min(shown.len())]);
        eprintln!(
            "ghostab-log: caret cursor={} start={} rel={} shown_len={} caret_x={}",
            app.address_cursor,
            start,
            rel_cursor,
            shown.len(),
            caret_x
        );
        canvas.set_fg(pal(COLOR_CARET));
        canvas.line(
            caret_x,
            ADDRESS_Y + 5,
            caret_x,
            ADDRESS_Y + ADDRESS_HEIGHT as c_int - 5,
        );
    }

    canvas.set_fg(pal(COLOR_GO_BG));
    canvas.fill_rect(go_x, ADDRESS_Y, GO_WIDTH as c_int, GO_HEIGHT as c_int);
    canvas.set_fg(pal(COLOR_GO_BORDER));
    canvas.rect(go_x, ADDRESS_Y, GO_WIDTH as c_int, GO_HEIGHT as c_int);
    canvas.set_fg(pal(COLOR_GO_TEXT));
    canvas.text_centered(go_x + 14, ADDRESS_Y, GO_HEIGHT as c_int, "Go");

    draw_nav_button(app, canvas, back_button_x(app.window_width), !app.history_back.is_empty(), NavButton::Back);
    draw_nav_button(app, canvas, forward_button_x(app.window_width), !app.history_forward.is_empty(), NavButton::Forward);
    draw_nav_button(app, canvas, home_button_x(app.window_width), true, NavButton::Home);
    draw_nav_button(app, canvas, refresh_button_x(app.window_width), true, NavButton::Refresh);
}

fn draw_nav_button(
    app: &BrowserApp,
    canvas: &mut Canvas,
    x: c_int,
    enabled: bool,
    button: NavButton,
) {
    let hovered = enabled && app.hover_button == Some(button);
    let fill = if !enabled {
        pal(COLOR_ADDRESS_BG)
    } else if hovered {
        pal(COLOR_BUTTON_HOVER)
    } else {
        pal(COLOR_BUTTON_BG)
    };
    canvas.set_fg(fill);
    canvas.fill_rect(x, ADDRESS_Y, NAV_BUTTON_SIZE, NAV_BUTTON_SIZE);
    canvas.set_fg(pal(if hovered { COLOR_GO_BG } else { COLOR_BUTTON_BORDER }));
    canvas.rect(x, ADDRESS_Y, NAV_BUTTON_SIZE, NAV_BUTTON_SIZE);
    let icon = pal(if enabled { COLOR_BUTTON_TEXT } else { COLOR_MUTED_TEXT });
    let center_x = x + NAV_BUTTON_SIZE / 2;
    let center_y = ADDRESS_Y + NAV_BUTTON_SIZE / 2;
    match button {
        NavButton::Back => draw_chevron(canvas, center_x, center_y, false, icon),
        NavButton::Forward => draw_chevron(canvas, center_x, center_y, true, icon),
        NavButton::Home => draw_home_icon(canvas, center_x, center_y, icon),
        NavButton::Refresh => draw_refresh_icon(canvas, center_x, center_y, icon),
    }
}

fn draw_shield_button(app: &BrowserApp, canvas: &mut Canvas) {
    let hovered = app.hover_shield;
    let state = connection_state(&app.page.source);
    canvas.set_fg(pal(if hovered { COLOR_BUTTON_HOVER } else { COLOR_BUTTON_BG }));
    canvas.fill_rect(SHIELD_X, ADDRESS_Y, SHIELD_SIZE, SHIELD_SIZE);
    canvas.set_fg(pal(if hovered { COLOR_GO_BG } else { COLOR_BUTTON_BORDER }));
    canvas.rect(SHIELD_X, ADDRESS_Y, SHIELD_SIZE, SHIELD_SIZE);
    draw_shield_icon(
        canvas,
        SHIELD_X + SHIELD_SIZE / 2,
        ADDRESS_Y + SHIELD_SIZE / 2,
        state,
    );
}

fn draw_shield_icon(canvas: &mut Canvas, cx: c_int, cy: c_int, state: ConnectionState) {
    let safe = state != ConnectionState::Unprotected;
    let points = [
        (cx - 10, cy - 11),
        (cx + 10, cy - 11),
        (cx + 10, cy + 1),
        (cx + 4, cy + 8),
        (cx, cy + 15),
        (cx - 4, cy + 8),
        (cx - 10, cy + 1),
    ];
    canvas.set_fg(pal(COLOR_SHIELD_BLUE));
    canvas.polygon(&points);
    canvas.set_fg(pal(COLOR_SHIELD_OUTLINE));
    canvas.polyline(&points, true);
    if !safe {
        canvas.set_fg(pal(COLOR_SHIELD_DANGER));
        canvas.line(cx - 9, cy - 9, cx + 9, cy + 9);
        canvas.line(cx + 9, cy - 9, cx - 9, cy + 9);
    }
}

fn draw_chevron(canvas: &mut Canvas, cx: c_int, cy: c_int, forward: bool, color: u32) {
    canvas.set_fg(color);
    let r = 7;
    let (x1, x3) = if forward { (cx - r, cx - r) } else { (cx + r, cx + r) };
    let tip = cx + if forward { r } else { -r };
    canvas.line(x1, cy - 6, tip, cy);
    canvas.line(tip, cy, x3, cy + 6);
}

fn draw_home_icon(canvas: &mut Canvas, cx: c_int, cy: c_int, color: u32) {
    canvas.set_fg(color);
    let w = 14;
    let h = 11;
    let x = cx - w / 2;
    let y = cy - h / 2 + 1;
    canvas.line(x, y + h / 2, cx, y - 2);
    canvas.line(cx, y - 2, x + w, y + h / 2);
    canvas.line(x + 2, y + h / 2, x + 2, y + h);
    canvas.line(x + 2, y + h, x + w - 2, y + h);
    canvas.line(x + w - 2, y + h, x + w - 2, y + h / 2);
}

fn draw_refresh_icon(canvas: &mut Canvas, cx: c_int, cy: c_int, color: u32) {
    canvas.set_fg(color);
    let r = 6;
    canvas.arc(cx, cy, r, r, 50.0, 270.0);
    canvas.line(cx + 4, cy - r - 1, cx + 6, cy - r + 3);
    canvas.line(cx + 6, cy - r + 3, cx + 2, cy - r + 3);
}

fn draw_tab_bar(app: &BrowserApp, canvas: &mut Canvas) {
    canvas.set_fg(pal(COLOR_TAB_STRIP));
    canvas.fill_rect(0, TAB_BAR_Y, app.window_width as c_int, TAB_BAR_HEIGHT);
    let labels = tab_titles(app);
    let xs = tab_xs(&labels);
    for (index, x) in xs.iter().enumerate() {
        draw_tab(app, canvas, index, *x, tab_width(&labels[index]));
    }
    draw_new_tab_button(app, canvas);
}

fn draw_tab(app: &BrowserApp, canvas: &mut Canvas, index: usize, x: c_int, width: c_int) {
    let active = index == app.active_tab;
    let fill = if active {
        pal(COLOR_TAB_ACTIVE_FILL)
    } else {
        pal(COLOR_TAB_INACTIVE_FILL)
    };
    canvas.set_fg(fill);
    canvas.fill_rect(x, TAB_BAR_Y + 3, width, TAB_BAR_HEIGHT - 3);
    canvas.set_fg(pal(if active {
        COLOR_TAB_ACTIVE_ORANGE
    } else {
        COLOR_TAB_BORDER
    }));
    canvas.rect(x, TAB_BAR_Y + 3, width, TAB_BAR_HEIGHT - 3);
    if active {
        canvas.set_fg(pal(COLOR_TAB_ACTIVE_ORANGE));
        canvas.fill_rect(x + 1, TAB_BAR_Y + 3, 2, TAB_BAR_HEIGHT - 3);
        canvas.line(
            x,
            TAB_BAR_Y + TAB_BAR_HEIGHT - 1,
            x + width,
            TAB_BAR_Y + TAB_BAR_HEIGHT - 1,
        );
    }
    let label = shorten(&app.tabs[index].page.title, 14);
    canvas.set_fg(pal(if active {
        COLOR_TAB_TEXT
    } else {
        COLOR_TAB_TEXT_MUTED
    }));
    canvas.text_centered(x + 10, TAB_BAR_Y + 3, TAB_BAR_HEIGHT - 3, &label);
    let close_x = x + width - TAB_CLOSE_WIDTH;
    let hovered_close = app.hover_close == Some(index);
    if hovered_close {
        canvas.set_fg(pal(COLOR_TAB_BORDER));
        canvas.fill_rect(close_x + 1, TAB_BAR_Y + 5, TAB_CLOSE_WIDTH - 2, TAB_BAR_HEIGHT - 8);
    }
    let cx = close_x + TAB_CLOSE_WIDTH / 2;
    let cy = TAB_BAR_Y + 3 + (TAB_BAR_HEIGHT - 3) / 2;
    canvas.set_fg(pal(if hovered_close {
        COLOR_TAB_ACTIVE_ORANGE
    } else {
        COLOR_TAB_TEXT_MUTED
    }));
    let s = 4;
    canvas.line(cx - s, cy - s, cx + s, cy + s);
    canvas.line(cx + s, cy - s, cx - s, cy + s);
}

fn draw_new_tab_button(app: &BrowserApp, canvas: &mut Canvas) {
    let x = new_tab_button_x(app.window_width);
    canvas.set_fg(pal(COLOR_TAB_INACTIVE_FILL));
    canvas.fill_rect(x, TAB_BAR_Y + 3, NEW_TAB_BUTTON_SIZE, TAB_BAR_HEIGHT - 3);
    canvas.set_fg(pal(COLOR_TAB_BORDER));
    canvas.rect(x, TAB_BAR_Y + 3, NEW_TAB_BUTTON_SIZE, TAB_BAR_HEIGHT - 3);
    canvas.set_fg(pal(COLOR_TAB_TEXT));
    let cx = x + NEW_TAB_BUTTON_SIZE / 2;
    let cy = TAB_BAR_Y + 3 + (TAB_BAR_HEIGHT - 3) / 2;
    canvas.line(cx - 5, cy, cx + 5, cy);
    canvas.line(cx, cy - 5, cx, cy + 5);
}

fn draw_menu_bar(app: &BrowserApp, canvas: &mut Canvas) {
    canvas.set_fg(pal(COLOR_SURFACE));
    canvas.fill_rect(0, 0, app.window_width as c_int, MENU_BAR_HEIGHT);
    canvas.set_fg(pal(COLOR_PAGE_BORDER));
    canvas.line(0, MENU_BAR_HEIGHT, app.window_width as c_int, MENU_BAR_HEIGHT);

    for (i, label) in MENU_LABELS.iter().enumerate() {
        let bx = menu_button_x(i);
        let active = app.open_menu == Some(i);
        canvas.set_fg(pal(if active {
            COLOR_SELECTION_BG
        } else {
            COLOR_BUTTON_BG
        }));
        canvas.fill_rect(bx, 0, MENU_BUTTON_WIDTH, MENU_BAR_HEIGHT);
        canvas.set_fg(pal(if active {
            COLOR_SELECTION_TEXT
        } else {
            COLOR_BUTTON_TEXT
        }));
        canvas.text_centered(bx + 10, 0, MENU_BAR_HEIGHT, label);
    }

    if let Some(menu) = app.open_menu {
        let bx = menu_button_x(menu);
        let items = MENU_ITEMS[menu];
        let panel_height = items.len() as c_int * MENU_ITEM_HEIGHT + 2;
        canvas.set_fg(pal(COLOR_SURFACE));
        canvas.fill_rect(bx, MENU_BAR_HEIGHT, MENU_ITEM_WIDTH, panel_height);
        canvas.set_fg(pal(COLOR_PAGE_BORDER));
        canvas.rect(bx, MENU_BAR_HEIGHT, MENU_ITEM_WIDTH, panel_height);
        for (j, item) in items.iter().enumerate() {
            let iy = menu_item_y(j);
            if app.menu_hover == Some((menu, j)) {
                canvas.set_fg(pal(COLOR_SELECTION_BG));
                canvas.fill_rect(bx + 1, iy, MENU_ITEM_WIDTH - 2, MENU_ITEM_HEIGHT - 1);
            }
            canvas.set_fg(pal(COLOR_BODY_TEXT));
            canvas.text_centered(bx + 10, iy, MENU_ITEM_HEIGHT, item);
        }
    }
}

fn draw_box(
    app: &BrowserApp,
    canvas: &mut Canvas,
    node: &engine::LayoutBox,
    scroll_y: c_int,
    window_height: c_int,
) {
    if let Some(text) = &node.text {
        let x = (MARGIN_X + node.rect.x * CHAR_WIDTH) as c_int;
        let y = (MARGIN_Y + node.rect.y * LINE_HEIGHT) as c_int - scroll_y;
        if y >= MARGIN_Y as c_int - LINE_HEIGHT as c_int
            && y <= window_height - STATUS_BAR_HEIGHT as c_int - 10
        {
            if !node.links.is_empty() {
                draw_text_with_links(canvas, x, y, text, &node.links);
            } else if node.href.is_some() {
                canvas.set_fg(pal(COLOR_LINK));
                canvas.text_baseline(x, y, text);
                let pw = text_width(text);
                canvas.line(x, y + 2, x + pw, y + 2);
            } else {
                canvas.set_fg(pal(COLOR_BODY_TEXT));
                canvas.text_baseline(x, y, text);
            }
        }
    }

    if node.rule {
        let x = (MARGIN_X + node.rect.x * CHAR_WIDTH) as c_int;
        let y = (MARGIN_Y + node.rect.y * LINE_HEIGHT) as c_int - scroll_y;
        let y = y + (LINE_HEIGHT / 2) as c_int;
        if y >= MARGIN_Y as c_int - LINE_HEIGHT as c_int
            && y <= window_height - STATUS_BAR_HEIGHT as c_int - 10
        {
            let line_width = (node.rect.width * CHAR_WIDTH) as c_int;
            canvas.set_fg(pal(COLOR_PAGE_BORDER));
            canvas.line(x, y, x + line_width, y);
        }
    }

    if let Some(image) = &node.image {
        let x = (MARGIN_X + node.rect.x * CHAR_WIDTH) as c_int;
        let y = (MARGIN_Y + node.rect.y * LINE_HEIGHT) as c_int - scroll_y;
        let bottom = window_height - STATUS_BAR_HEIGHT as c_int - 10;
        if y + image.height_px as c_int >= MARGIN_Y as c_int - LINE_HEIGHT as c_int && y <= bottom {
            if let Some(original) = app.images.originals.get(&image.source) {
                canvas.image(
                    &original.rgba,
                    original.width,
                    original.height,
                    x,
                    y,
                    image.width_px as c_int,
                    image.height_px as c_int,
                    None,
                );
            } else {
                canvas.set_fg(pal(COLOR_IMAGE_BORDER));
                canvas.rect(x, y, image.width_px as c_int, image.height_px as c_int);
            }
        }
    }

    for child in &node.children {
        draw_box(app, canvas, child, scroll_y, window_height);
    }
}

fn draw_text_with_links(
    canvas: &mut Canvas,
    x: c_int,
    y: c_int,
    text: &str,
    links: &[engine::LinkSpan],
) {
    let mut cx = x;
    let mut prev = 0usize;
    for span in links {
        if span.end > span.start && span.start >= prev {
            if span.start > prev {
                let segment = &text[prev..span.start];
                canvas.set_fg(pal(COLOR_BODY_TEXT));
                canvas.text_baseline(cx, y, segment);
                cx += text_width(segment);
            }
            let segment = &text[span.start..span.end];
            canvas.set_fg(pal(COLOR_LINK));
            canvas.text_baseline(cx, y, segment);
            let pw = text_width(segment);
            canvas.line(cx, y + 2, cx + pw, y + 2);
            cx += pw;
            prev = span.end;
        }
    }
    if prev < text.len() {
        let segment = &text[prev..];
        canvas.set_fg(pal(COLOR_BODY_TEXT));
        canvas.text_baseline(cx, y, segment);
    }
}

fn draw_scrollbar(app: &BrowserApp, canvas: &mut Canvas) {
    let track_x = app.window_width.saturating_sub(18) as c_int;
    let track_y = MARGIN_Y as c_int;
    let track_height = app.viewport_height() as c_int;
    let max_scroll = app.max_scroll_y();

    canvas.set_fg(pal(COLOR_SCROLLBAR_TRACK));
    canvas.fill_rect(track_x, track_y, 8, track_height);

    if max_scroll == 0 {
        return;
    }

    let content_height = app.layout.rect.height as c_int * LINE_HEIGHT as c_int;
    let thumb_height = ((track_height * track_height) / content_height).clamp(32, track_height);
    let thumb_y = track_y + ((track_height - thumb_height) * app.scroll_y / max_scroll);

    canvas.set_fg(pal(COLOR_SCROLLBAR_THUMB));
    canvas.fill_rect(track_x, thumb_y, 8, thumb_height);
}

fn draw_status_bar(app: &BrowserApp, canvas: &mut Canvas) {
    let y = app.window_height.saturating_sub(STATUS_BAR_HEIGHT) as c_int;
    canvas.set_fg(pal(COLOR_STATUS_BAR));
    canvas.fill_rect(0, y, app.window_width as c_int, STATUS_BAR_HEIGHT as c_int);
    canvas.set_fg(pal(COLOR_MUTED_TEXT));
    let message = if let Some(href) = &app.hover_href {
        format!("Link: {}", shorten(href, 110))
    } else {
        format!(
            "{} lines rendered | scroll {} / {}",
            app.layout.rect.height,
            app.scroll_y,
            app.max_scroll_y()
        )
    };
    canvas.text_centered(22, y, STATUS_BAR_HEIGHT as c_int, &message);
}

fn find_link_at(
    node: &engine::LayoutBox,
    x: c_int,
    y: c_int,
    scroll_y: c_int,
) -> Option<String> {
    if y < TITLE_BAR_HEIGHT as c_int {
        return None;
    }
    if let Some(text) = &node.text {
        let px = (MARGIN_X + node.rect.x * CHAR_WIDTH) as c_int;
        let py = (MARGIN_Y + node.rect.y * LINE_HEIGHT) as c_int - scroll_y;
        if x >= px && y >= py - 16 && y < py + 4 {
            let rel = x - px;
            if !node.links.is_empty() {
                let hit = text_offset_at(text, rel);
                if let Some(hit) = hit {
                    for span in &node.links {
                        if span.start <= hit && hit < span.end {
                            return Some(span.href.clone());
                        }
                    }
                }
            } else if let Some(href) = &node.href {
                if rel < text_width(text) {
                    return Some(href.clone());
                }
            }
        }
    }
    if let Some(image) = &node.image {
        let px = (MARGIN_X + node.rect.x * CHAR_WIDTH) as c_int;
        let py = (MARGIN_Y + node.rect.y * LINE_HEIGHT) as c_int - scroll_y;
        if x >= px
            && x < px + image.width_px as c_int
            && y >= py
            && y < py + image.height_px as c_int
        {
            return node.href.clone();
        }
    }
    for child in &node.children {
        if let Some(href) = find_link_at(child, x, y, scroll_y) {
            return Some(href);
        }
    }
    None
}

fn text_offset_at(text: &str, rel: c_int) -> Option<usize> {
    if text.is_empty() {
        return None;
    }
    let mut hit = 0usize;
    for (i, _) in text.char_indices() {
        if text_width(&text[..i]) > rel {
            break;
        }
        hit = i;
    }
    Some(hit)
}

fn cursor_for_click(app: &BrowserApp, click_x: c_int) -> usize {
    let address_width = address_bar_width(app.window_width) as c_int;
    let avail = address_width - 28;
    let (shown, start) = visible_address(app, avail);
    let rel_x = click_x - ADDRESS_TEXT_X;
    let mut best = 0usize;
    let mut best_diff = c_int::MAX;
    for (index, _) in shown.char_indices() {
        let width = text_width(&shown[..index]);
        let diff = (width - rel_x).abs();
        if diff < best_diff {
            best_diff = diff;
            best = index;
        }
    }
    let end_width = text_width(&shown);
    if (end_width - rel_x).abs() < best_diff {
        best = shown.len();
    }
    start + best
}

const ABOUT_W: c_int = 520;
const ABOUT_H: c_int = 240;
const ABOUT_CLOSE_X: c_int = ABOUT_W - 16 - SETTINGS_OK_W;
const ABOUT_CLOSE_Y: c_int = ABOUT_H - 42;

fn draw_about_content(tab: u8, canvas: &mut Canvas) {
    canvas.set_fg(pal(COLOR_PAGE));
    canvas.fill_rect(0, 0, ABOUT_W, ABOUT_H);
    draw_about_tabs(tab, canvas);
    canvas.set_fg(pal(COLOR_BODY_TEXT));
    if tab == 0 {
        canvas.text_baseline(28, 58, "Ghostab");
        canvas.text_baseline(28, 82, "Engine: Ghost Engine 2.0.0-alpha");
        canvas.text_baseline(28, 106, "A tiny experimental browser engine written in Rust.");
        canvas.text_baseline(28, 130, "Networking: HTTP/HTTPS loading through curl.");
        canvas.text_baseline(28, 154, "Rendering: simplified HTML text layout in a winit window.");
        canvas.text_baseline(28, 178, "Privacy: clipboard is app-only and never touches the OS.");
    } else {
        canvas.text_baseline(28, 58, "Credits");
        canvas.text_baseline(28, 82, "Made by AramCZ");
        canvas.text_baseline(28, 106, "Tools: Rust, C (curl), winit, softbuffer, cosmic-text,");
        canvas.text_baseline(28, 130, "the image crate, dpkg, Bash.");
        canvas.text_baseline(28, 154, "Built with some assistance from Opencode.");
    }
    canvas.set_fg(pal(COLOR_PAGE_BORDER));
    canvas.line(16, ABOUT_H - 52, ABOUT_W - 16, ABOUT_H - 52);
    draw_settings_button(
        canvas,
        ABOUT_CLOSE_X,
        ABOUT_CLOSE_Y,
        SETTINGS_OK_W,
        SETTINGS_OK_H,
        "Close",
        false,
    );
}

fn draw_about_tabs(active: u8, canvas: &mut Canvas) {
    for (i, label) in ["About", "Credits"].iter().enumerate() {
        let i = i as u8;
        let x = 12 + (i as c_int) * 104;
        let selected = i == active;
        canvas.set_fg(pal(if selected {
            COLOR_SURFACE
        } else {
            COLOR_BUTTON_BG
        }));
        canvas.fill_rect(x, 4, 96, 28);
        canvas.set_fg(pal(COLOR_PAGE_BORDER));
        canvas.rect(x, 4, 96, 28);
        canvas.set_fg(pal(if selected {
            COLOR_BODY_TEXT
        } else {
            COLOR_MUTED_TEXT
        }));
        canvas.text_centered(x + 14, 4, 28, label);
    }
    canvas.set_fg(pal(COLOR_PAGE_BORDER));
    canvas.line(0, 32, 520, 32);
}

fn about_tab_at(x: c_int, y: c_int) -> Option<u8> {
    if !(4..=32).contains(&y) {
        return None;
    }
    if (12..108).contains(&x) {
        Some(0)
    } else if (116..212).contains(&x) {
        Some(1)
    } else {
        None
    }
}

fn about_close_at(x: c_int, y: c_int) -> bool {
    (ABOUT_CLOSE_X..ABOUT_CLOSE_X + SETTINGS_OK_W).contains(&x)
        && (ABOUT_CLOSE_Y..ABOUT_CLOSE_Y + SETTINGS_OK_H).contains(&y)
}

fn draw_settings_content(
    settings: &Settings,
    url_focused: bool,
    url_cursor: usize,
    canvas: &mut Canvas,
) {
    canvas.set_fg(pal(COLOR_PAGE));
    canvas.fill_rect(0, 0, SETTINGS_W, SETTINGS_H);
    canvas.set_fg(pal(COLOR_BODY_TEXT));
    canvas.text_baseline(24, 46, "Settings");
    canvas.set_fg(pal(COLOR_PAGE_BORDER));
    canvas.line(16, 60, SETTINGS_W - 16, 60);

    canvas.set_fg(pal(COLOR_MUTED_TEXT));
    canvas.text_baseline(24, 84, "Appearance");
    draw_checkbox(canvas, 28, 96, settings.light_mode);
    canvas.set_fg(pal(COLOR_BODY_TEXT));
    canvas.text_centered(54, 84, 32, "Light Mode");

    canvas.set_fg(pal(COLOR_MUTED_TEXT));
    canvas.text_baseline(24, 140, "Search Engine");
    for (i, engine) in SearchEngine::all().iter().enumerate() {
        let y = 162 + i as c_int * 28;
        draw_radio(canvas, 28, y, *engine == settings.search_engine);
        canvas.set_fg(pal(COLOR_BODY_TEXT));
        canvas.text_centered(54, y - 2, 32, engine.label());
    }

    if settings.search_engine == SearchEngine::Custom {
        canvas.set_fg(pal(COLOR_MUTED_TEXT));
        canvas.text_baseline(28, 222, "Search URL (use %s for the query)");
        canvas.set_fg(pal(if url_focused {
            COLOR_ADDRESS_FOCUS
        } else {
            COLOR_ADDRESS_BORDER
        }));
        canvas.fill_rect(
            SETTINGS_URL_X,
            SETTINGS_URL_Y,
            SETTINGS_URL_W,
            SETTINGS_URL_H,
        );
        canvas.set_fg(pal(COLOR_ADDRESS_TEXT));
        let text_x = SETTINGS_URL_X + 8;
        canvas.text_centered(
            text_x,
            SETTINGS_URL_Y,
            SETTINGS_URL_H,
            &settings.search_url,
        );
        if url_focused {
            let prefix = &settings.search_url[..url_cursor.min(settings.search_url.len())];
            let cx = text_x + text_width(prefix);
            canvas.set_fg(pal(COLOR_CARET));
            canvas.line(cx, SETTINGS_URL_Y + 6, cx, SETTINGS_URL_Y + SETTINGS_URL_H - 6);
        }
    }

    canvas.set_fg(pal(COLOR_PAGE_BORDER));
    canvas.line(16, SETTINGS_H - 52, SETTINGS_W - 16, SETTINGS_H - 52);
    draw_settings_button(
        canvas,
        SETTINGS_OK_X,
        SETTINGS_OK_Y,
        SETTINGS_OK_W,
        SETTINGS_OK_H,
        "OK",
        true,
    );
    draw_settings_button(
        canvas,
        SETTINGS_CANCEL_X,
        SETTINGS_OK_Y,
        SETTINGS_OK_W,
        SETTINGS_OK_H,
        "Close",
        false,
    );
}

fn draw_checkbox(canvas: &mut Canvas, x: c_int, y: c_int, checked: bool) {
    canvas.set_fg(pal(if checked {
        COLOR_GO_BG
    } else {
        COLOR_BUTTON_BORDER
    }));
    canvas.fill_rect(x, y, 18, 18);
    canvas.set_fg(pal(COLOR_PAGE_BORDER));
    canvas.rect(x, y, 18, 18);
    if checked {
        canvas.set_fg(pal(COLOR_GO_TEXT));
        canvas.line(x + 3, y + 9, x + 8, y + 14);
        canvas.line(x + 8, y + 14, x + 15, y + 4);
    }
}

fn draw_radio(canvas: &mut Canvas, x: c_int, y: c_int, selected: bool) {
    canvas.set_fg(pal(COLOR_BUTTON_BORDER));
    canvas.circle(x + 9, y + 9, 9);
    if selected {
        canvas.set_fg(pal(COLOR_GO_BG));
        canvas.fill_circle(x + 9, y + 9, 5);
    }
}

fn draw_settings_button(
    canvas: &mut Canvas,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    label: &str,
    primary: bool,
) {
    canvas.set_fg(pal(if primary {
        COLOR_GO_BG
    } else {
        COLOR_BUTTON_BG
    }));
    canvas.fill_rect(x, y, width, height);
    canvas.set_fg(pal(COLOR_BUTTON_BORDER));
    canvas.rect(x, y, width, height);
    canvas.set_fg(pal(if primary {
        COLOR_GO_TEXT
    } else {
        COLOR_BUTTON_TEXT
    }));
    let label_width = text_width(label);
    canvas.text_centered(x + (width - label_width) / 2, y, height, label);
}

const SHIELD_CLOSE_X: c_int = SHIELD_W - 16 - SETTINGS_OK_W;
const SHIELD_CLOSE_Y: c_int = SHIELD_H - 42;

fn draw_shield_content(state: ConnectionState, message: &str, canvas: &mut Canvas) {
    canvas.set_fg(pal(COLOR_PAGE));
    canvas.fill_rect(0, 0, SHIELD_W, SHIELD_H);
    let headline = match state {
        ConnectionState::Protected => "Protected connection",
        ConnectionState::Local => "Local content",
        ConnectionState::BuiltIn => "Built into Ghostab",
        ConnectionState::Unprotected => "Unprotected connection",
    };
    canvas.set_fg(pal(COLOR_BODY_TEXT));
    canvas.text_baseline(28, 28, "Connection Security");
    canvas.set_fg(pal(COLOR_GO_BG));
    canvas.text_baseline(28, 54, headline);
    canvas.set_fg(pal(COLOR_PAGE_BORDER));
    canvas.line(16, 68, SHIELD_W - 16, 68);

    canvas.set_fg(pal(COLOR_BODY_TEXT));
    for (i, line) in wrap_text(message, 70).iter().enumerate() {
        canvas.text_baseline(28, SHIELD_TEXT_Y + i as c_int * SHIELD_LINE_STEP, line);
    }
    draw_settings_button(
        canvas,
        SHIELD_CLOSE_X,
        SHIELD_CLOSE_Y,
        SETTINGS_OK_W,
        SETTINGS_OK_H,
        "Close",
        false,
    );
}

enum KeyInput {
    Escape,
    Enter,
    Backspace,
    Delete,
    PageUp,
    PageDown,
    Home,
    End,
    Left,
    Right,
    Text(String),
    Other,
}

fn log_input(input: &KeyInput) -> String {
    match input {
        KeyInput::Text(t) => format!("Text({})", t),
        KeyInput::Escape => "Escape".into(),
        KeyInput::Enter => "Enter".into(),
        KeyInput::Backspace => "Backspace".into(),
        KeyInput::Delete => "Delete".into(),
        KeyInput::PageUp => "PageUp".into(),
        KeyInput::PageDown => "PageDown".into(),
        KeyInput::Home => "Home".into(),
        KeyInput::End => "End".into(),
        KeyInput::Left => "Left".into(),
        KeyInput::Right => "Right".into(),
        KeyInput::Other => "Other".into(),
    }
}

#[derive(Copy, Clone, Default)]
struct KeyMods {
    ctrl: bool,
    shift: bool,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum KeySymChar {
    A,
    C,
    D,
    L,
    R,
    T,
    V,
    W,
    Tab,
}

fn physical_char(code: KeyCode) -> Option<KeySymChar> {
    use winit::keyboard::KeyCode::*;
    Some(match code {
        KeyA => KeySymChar::A,
        KeyC => KeySymChar::C,
        KeyD => KeySymChar::D,
        KeyL => KeySymChar::L,
        KeyR => KeySymChar::R,
        KeyT => KeySymChar::T,
        KeyV => KeySymChar::V,
        KeyW => KeySymChar::W,
        _ => return None,
    })
}

fn read_key_event(event: &KeyEvent, mods: KeyMods) -> (KeyInput, KeyMods, Option<KeySymChar>) {
    let keysym = match event.physical_key {
        PhysicalKey::Code(code) => {
            if mods.ctrl && code == KeyCode::Tab {
                Some(KeySymChar::Tab)
            } else {
                physical_char(code)
            }
        }
        _ => None,
    };
    let input = match event.logical_key {
        Key::Named(NamedKey::Escape) => KeyInput::Escape,
        Key::Named(NamedKey::Enter) => KeyInput::Enter,
        Key::Named(NamedKey::Backspace) => KeyInput::Backspace,
        Key::Named(NamedKey::Delete) => KeyInput::Delete,
        Key::Named(NamedKey::PageUp) => KeyInput::PageUp,
        Key::Named(NamedKey::PageDown) => KeyInput::PageDown,
        Key::Named(NamedKey::Home) => KeyInput::Home,
        Key::Named(NamedKey::End) => KeyInput::End,
        Key::Named(NamedKey::ArrowLeft) => KeyInput::Left,
        Key::Named(NamedKey::ArrowRight) => KeyInput::Right,
        _ => {
            if let Some(text) = &event.text {
                if !text.is_empty() && text.chars().all(|ch| !ch.is_control()) {
                    KeyInput::Text(text.to_string())
                } else {
                    KeyInput::Other
                }
            } else {
                KeyInput::Other
            }
        }
    };
    (input, mods, keysym)
}
