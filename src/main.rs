#![allow(unsafe_op_in_unsafe_fn)]
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::CString;
use std::fs;
use std::os::raw::{c_char, c_int, c_long, c_short, c_uint, c_ulong, c_void};
use std::path::PathBuf;
use std::process::Command;
use std::ptr;
use std::rc::Rc;

mod engine;

const SAMPLE_HTML: &str = r#"
<html>
  <body>
    <h1>Ghostab</h1>
    <p>A tiny browser engine seed written in Rust.</p>
    <p>Open a local HTML file or type a URL above. Try clicking <a href="https://example.com">example.com</a>.</p>
    <p>Images load from the web or your disk with the img tag, and links are clickable.</p>
  </body>
</html>
"#;

const NEWTAB_HTML: &str = r#"
<html>
  <body>
    <h1>Ghostab</h1>
    <p>A tiny browser built in Rust.</p>
    <p>Type a URL or a search term above to get going.</p>
  </body>
</html>
"#;

const WINDOW_WIDTH: usize = 960;
const WINDOW_HEIGHT: usize = 640;
const STATUS_BAR_HEIGHT: usize = 30;
const MARGIN_X: usize = 34;
const LINE_HEIGHT: usize = 22;
const CHAR_WIDTH: usize = 7;
const SCROLL_STEP: c_int = 72;

const MENU_BAR_HEIGHT: c_int = 20;
const TAB_BAR_HEIGHT: c_int = 28;
const TAB_BAR_Y: c_int = MENU_BAR_HEIGHT;
const ADDRESS_Y: c_int = TAB_BAR_Y + TAB_BAR_HEIGHT + 4;
const ADDRESS_HEIGHT: c_uint = 36;
const TITLE_BAR_HEIGHT: usize = (ADDRESS_Y + ADDRESS_HEIGHT as c_int + 8) as usize;
const MARGIN_Y: usize = TITLE_BAR_HEIGHT + 34;

const COLOR_PAGE: c_ulong = 0x15181C;
const COLOR_PAGE_BORDER: c_ulong = 0x333A41;
const COLOR_SURFACE: c_ulong = 0x1E2228;
const COLOR_TITLE_BAR: c_ulong = 0x121417;
const COLOR_TITLE_LINE: c_ulong = 0x2C323A;
const COLOR_MUTED_TEXT: c_ulong = 0x8A93A0;
const COLOR_ADDRESS_BG: c_ulong = 0x23272D;
const COLOR_ADDRESS_FOCUS: c_ulong = 0xDF8D00;
const COLOR_ADDRESS_BORDER: c_ulong = 0x3A4048;
const COLOR_ADDRESS_TEXT: c_ulong = 0xE6E8EB;
const COLOR_BUTTON_BG: c_ulong = 0x2A2F36;
const COLOR_BUTTON_HOVER: c_ulong = 0x39404A;
const COLOR_BUTTON_BORDER: c_ulong = 0x414A54;
const COLOR_BUTTON_TEXT: c_ulong = 0xE4E7EA;
const COLOR_BODY_TEXT: c_ulong = 0xD8DCE1;
const COLOR_LINK: c_ulong = 0x6CB2FF;
const COLOR_SCROLLBAR_TRACK: c_ulong = 0x262A30;
const COLOR_SCROLLBAR_THUMB: c_ulong = 0x4A525C;
const COLOR_STATUS_BAR: c_ulong = 0x1E2228;
const COLOR_SELECTION_BG: c_ulong = 0x2B6CB0;
const COLOR_SELECTION_TEXT: c_ulong = 0xFFFFFF;
const COLOR_CARET: c_ulong = 0xF2F4F6;
const COLOR_GO_BG: c_ulong = 0xDF8D00;
const COLOR_GO_BORDER: c_ulong = 0xF0B25C;
const COLOR_GO_TEXT: c_ulong = 0x1A1A1A;
const COLOR_IMAGE_BORDER: c_ulong = 0x444D58;

const COLOR_TAB_STRIP: c_ulong = 0x171A1E;
const COLOR_TAB_ACTIVE_FILL: c_ulong = 0x1F2329;
const COLOR_TAB_ACTIVE_ORANGE: c_ulong = 0xDF8D00;
const COLOR_TAB_INACTIVE_FILL: c_ulong = 0x20242A;
const COLOR_TAB_BORDER: c_ulong = 0x3B424A;
const COLOR_TAB_TEXT: c_ulong = 0xE3E6EA;
const COLOR_TAB_TEXT_MUTED: c_ulong = 0x9098A4;
const COLOR_SHIELD_BLUE: c_ulong = 0x3B82F6;
const COLOR_SHIELD_OUTLINE: c_ulong = 0xF2F4F6;
const COLOR_SHIELD_DANGER: c_ulong = 0xD64545;

static mut LIGHT_MODE: bool = false;

fn light_mode_enabled() -> bool {
    unsafe { LIGHT_MODE }
}

fn pal(color: c_ulong) -> c_ulong {
    if !light_mode_enabled() {
        return color;
    }
    match color {
        COLOR_PAGE => 0xF5F6F7,
        COLOR_PAGE_BORDER => 0xD5D8DC,
        COLOR_SURFACE => 0xE9EBED,
        COLOR_TITLE_BAR => 0xF0F2F4,
        COLOR_TITLE_LINE => 0xD0D4D8,
        COLOR_MUTED_TEXT => 0x7A828C,
        COLOR_ADDRESS_BG => 0xFFFFFF,
        COLOR_ADDRESS_FOCUS => COLOR_GO_BG,
        COLOR_ADDRESS_BORDER => 0xC8CCD2,
        COLOR_ADDRESS_TEXT => 0x1A1C1F,
        COLOR_BUTTON_BG => 0xE4E7EA,
        COLOR_BUTTON_HOVER => 0xD0D6DC,
        COLOR_BUTTON_BORDER => 0xBAC0C8,
        COLOR_BUTTON_TEXT => 0x1A1C1F,
        COLOR_BODY_TEXT => 0x23262A,
        COLOR_LINK => 0x1B66C4,
        COLOR_SCROLLBAR_TRACK => 0xE0E3E6,
        COLOR_SCROLLBAR_THUMB => 0xA8B0B8,
        COLOR_SELECTION_BG => 0xAED4FF,
        COLOR_SELECTION_TEXT => 0x001A33,
        COLOR_CARET => 0x111418,
        COLOR_GO_BORDER => COLOR_GO_BORDER,
        COLOR_GO_TEXT => 0x1A1A1A,
        COLOR_IMAGE_BORDER => 0xBAC0C8,
        COLOR_TAB_STRIP => 0xE7EAEE,
        COLOR_TAB_ACTIVE_FILL => 0xFFFFFF,
        COLOR_TAB_INACTIVE_FILL => 0xDDE1E6,
        COLOR_TAB_BORDER => 0xC0C6CE,
        COLOR_TAB_TEXT => 0x1A1C1F,
        COLOR_TAB_TEXT_MUTED => 0x7A828C,
        _ => color,
    }
}

fn apply_settings(settings: &Settings) {
    unsafe {
        LIGHT_MODE = settings.light_mode;
    }
}

fn config_file() -> Option<PathBuf> {
    let base = if let Ok(dir) = env::var("XDG_CONFIG_HOME") {
        if dir.is_empty() {
            return None;
        }
        PathBuf::from(dir)
    } else if let Ok(home) = env::var("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        return None;
    };
    Some(base.join("ghostab").join("config.txt"))
}

fn load_settings() -> Settings {
    let mut settings = Settings::default();
    if let Some(path) = config_file() {
        if let Ok(text) = fs::read_to_string(&path) {
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim().trim_matches('"');
                    match key {
                        "light_mode" => settings.light_mode = value == "true",
                        "search_engine" => {
                            settings.search_engine = if value == "custom" {
                                SearchEngine::Custom
                            } else {
                                SearchEngine::Startpage
                            };
                        }
                        "search_url" => settings.search_url = value.to_string(),
                        _ => {}
                    }
                }
            }
        }
    }
    settings
}

fn save_settings(settings: &Settings) {
    let Some(path) = config_file() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let engine = match settings.search_engine {
        SearchEngine::Startpage => "startpage",
        SearchEngine::Custom => "custom",
    };
    let text = format!(
        "# Ghostab settings\nlight_mode = {}\nsearch_engine = {}\nsearch_url = {}\n",
        settings.light_mode,
        engine,
        settings.search_url,
    );
    let _ = fs::write(&path, text);
}

const ADDRESS_X: c_int = 16;
const SHIELD_SIZE: c_int = 32;
const SHIELD_X: c_int = ADDRESS_X;
const ADDRESS_BAR_X: c_int = SHIELD_X + SHIELD_SIZE + NAV_BUTTON_GAP;
const ADDRESS_TEXT_X: c_int = ADDRESS_BAR_X + 14;
const GO_WIDTH: c_uint = 48;
const GO_HEIGHT: c_uint = 32;
const NAV_BUTTON_SIZE: c_int = 32;
const NAV_BUTTON_GAP: c_int = 6;
const TAB_CLOSE_WIDTH: c_int = 18;
const NEW_TAB_BUTTON_SIZE: c_int = 26;

const MENU_BUTTON_WIDTH: c_int = 52;
const MENU_ITEM_WIDTH: c_int = 150;
const MENU_ITEM_HEIGHT: c_int = 24;
const MENU_LABELS: [&str; 4] = ["File", "Edit", "View", "Help"];
const MENU_ITEMS: [&[&str]; 4] = [
    &["Reload", "Settings", "Quit"],
    &["Select All", "Copy", "Paste"],
    &["Scroll Top", "Scroll Up", "Scroll Down", "Scroll Bottom"],
    &["About"],
];

static mut XFT_DRAW: *mut XftDraw = ptr::null_mut();
static mut XFT_FONT: *mut XftFont = ptr::null_mut();
static mut CURRENT_FG: c_ulong = 0;
static mut DRAW_COUNT: u32 = 0;
static mut FT_LIB: *mut FT_LibraryRec = ptr::null_mut();
static mut FT_FACE: *mut FT_FaceRec = ptr::null_mut();
static mut FT_PIXELSIZE: f64 = 0.0;

fn main() {
    let args: Vec<String> = env::args().collect();
    let page = load_page(args.get(1).map(String::as_str));
    let mut app = BrowserApp::new(page);
    app.settings = load_settings();
    apply_settings(&app.settings);

    unsafe {
        run_x11_window(app);
    }
}

#[derive(Clone)]
struct BrowserPage {
    source: String,
    html: String,
    title: String,
}

struct DecodedImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Default, Clone)]
struct ImageCache {
    originals: HashMap<String, Rc<DecodedImage>>,
    failed: HashSet<String>,
}

struct BrowserApp {
    page: BrowserPage,
    layout: engine::LayoutBox,
    images: ImageCache,
    address_text: String,
    address_focused: bool,
    address_cursor: usize,
    address_anchor: Option<usize>,
    clipboard_text: String,
    owns_clipboard: bool,
    pending_paste: bool,
    paste_retry: bool,
    hover_href: Option<String>,
    open_menu: Option<usize>,
    menu_hover: Option<(usize, usize)>,
    history_back: Vec<String>,
    history_forward: Vec<String>,
    mouse_down: bool,
    scroll_y: c_int,
    window_width: usize,
    window_height: usize,
    tabs: Vec<TabSnapshot>,
    active_tab: usize,
    hover_button: Option<NavButton>,
    hover_close: Option<usize>,
    hover_shield: bool,
    settings: Settings,
}

#[derive(Clone)]
struct TabSnapshot {
    page: BrowserPage,
    layout: engine::LayoutBox,
    images: ImageCache,
    address_text: String,
    address_focused: bool,
    address_cursor: usize,
    address_anchor: Option<usize>,
    history_back: Vec<String>,
    history_forward: Vec<String>,
    scroll_y: c_int,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum NavButton {
    Back,
    Forward,
    Home,
    Refresh,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ConnectionState {
    Protected,
    Local,
    BuiltIn,
    Unprotected,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum SearchEngine {
    Startpage,
    Custom,
}

impl SearchEngine {
    fn label(self) -> &'static str {
        match self {
            Self::Startpage => "Startpage",
            Self::Custom => "Custom",
        }
    }

    fn all() -> [SearchEngine; 2] {
        [Self::Startpage, Self::Custom]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Settings {
    light_mode: bool,
    search_engine: SearchEngine,
    search_url: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            light_mode: false,
            search_engine: SearchEngine::Startpage,
            search_url: String::new(),
        }
    }
}

impl BrowserApp {
    fn new(page: BrowserPage) -> Self {
        let window_width = WINDOW_WIDTH;
        let window_height = WINDOW_HEIGHT;
        let address_text = page.source.clone();

        let mut app = Self {
            page,
            layout: engine::LayoutBox {
                rect: engine::Rect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                },
                text: None,
                href: None,
                links: Vec::new(),
                image: None,
                rule: false,
                children: Vec::new(),
            },
            images: ImageCache::default(),
            address_text,
            address_focused: false,
            address_cursor: 0,
            address_anchor: None,
            clipboard_text: String::new(),
            owns_clipboard: false,
            pending_paste: false,
            paste_retry: false,
            hover_href: None,
            open_menu: None,
            menu_hover: None,
            history_back: Vec::new(),
            history_forward: Vec::new(),
            mouse_down: false,
            scroll_y: 0,
            window_width,
            window_height,
            tabs: Vec::new(),
            active_tab: 0,
            hover_button: None,
            hover_close: None,
            hover_shield: false,
            settings: Settings::default(),
        };
        app.relayout();
        app.tabs.push(app.snapshot());
        app.address_cursor = app.address_text.len();
        app
    }

    fn snapshot(&self) -> TabSnapshot {
        TabSnapshot {
            page: self.page.clone(),
            layout: self.layout.clone(),
            images: self.images.clone(),
            address_text: self.address_text.clone(),
            address_focused: self.address_focused,
            address_cursor: self.address_cursor,
            address_anchor: self.address_anchor,
            history_back: self.history_back.clone(),
            history_forward: self.history_forward.clone(),
            scroll_y: self.scroll_y,
        }
    }

    fn restore(&mut self, tab: TabSnapshot) {
        self.page = tab.page;
        self.layout = tab.layout;
        self.images = tab.images;
        self.address_text = tab.address_text;
        self.address_focused = tab.address_focused;
        self.address_cursor = tab.address_cursor;
        self.address_anchor = tab.address_anchor;
        self.history_back = tab.history_back;
        self.history_forward = tab.history_forward;
        self.scroll_y = tab.scroll_y;
        self.hover_href = None;
        self.hover_close = None;
        self.address_cursor = self.address_cursor.min(self.address_text.len());
        self.relayout();
    }

    fn sync_active_tab(&mut self) {
        if self.active_tab < self.tabs.len() {
            self.tabs[self.active_tab] = self.snapshot();
        }
    }

    fn new_tab(&mut self) {
        self.sync_active_tab();
        let fresh = BrowserApp::new(load_page(None));
        self.tabs.push(fresh.snapshot());
        self.active_tab = self.tabs.len() - 1;
        self.restore(self.tabs[self.active_tab].clone());
        self.address_focused = false;
    }

    fn close_tab(&mut self, index: usize) -> bool {
        if self.tabs.len() <= 1 {
            return true;
        }
        self.sync_active_tab();
        self.tabs.remove(index);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.restore(self.tabs[self.active_tab].clone());
        self.address_focused = false;
        false
    }

    fn switch_tab(&mut self, index: usize) {
        if index == self.active_tab || index >= self.tabs.len() {
            return;
        }
        self.sync_active_tab();
        self.active_tab = index;
        self.restore(self.tabs[index].clone());
        self.address_focused = false;
    }

    fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.switch_tab((self.active_tab + 1) % self.tabs.len());
    }

    fn prev_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.switch_tab((self.active_tab + self.tabs.len() - 1) % self.tabs.len());
    }

    fn reload(&mut self) {
        let source = self.page.source.clone();
        self.navigate_to(&source);
    }

    fn navigate(&mut self) {
        let target = normalize_navigation_target(&self.address_text.clone(), &self.settings);
        eprintln!("ghostab-log: navigate target='{}'", target);
        self.navigate_new(&target);
    }

    fn open_link(&mut self, href: &str) {
        if href.starts_with("mailto:") || href.starts_with("javascript:") || href.starts_with('#') {
            return;
        }
        let target = resolve_src(&self.page.source, href);
        self.navigate_new(&target);
    }

    fn navigate_new(&mut self, target: &str) {
        self.history_back.push(self.page.source.clone());
        if self.history_back.len() > 100 {
            self.history_back.remove(0);
        }
        self.history_forward.clear();
        self.navigate_to(target);
    }

    fn go_back(&mut self) {
        if let Some(previous) = self.history_back.pop() {
            self.history_forward.push(self.page.source.clone());
            self.navigate_to(&previous);
        }
    }

    fn go_forward(&mut self) {
        if let Some(next) = self.history_forward.pop() {
            self.history_back.push(self.page.source.clone());
            self.navigate_to(&next);
        }
    }

    fn navigate_to(&mut self, target: &str) {
        self.page = load_page(Some(target));
        eprintln!("ghostab-log: navigate_to '{}' -> title='{}'", target, self.page.title);
        self.relayout();
        self.address_text = target.to_string();
        self.address_cursor = self.address_text.len();
        self.address_anchor = None;
        self.hover_href = None;
        self.scroll_y = 0;
        self.sync_active_tab();
    }

    fn resize(&mut self, width: usize, height: usize) {
        let width = width.max(1);
        let height = height.max(1);
        if width == self.window_width && height == self.window_height {
            return;
        }
        self.window_width = width;
        self.window_height = height;
        self.relayout();
        self.scroll_y = self.scroll_y.clamp(0, self.max_scroll_y());
    }

    fn relayout(&mut self) {
        let document = engine::parse_html(&self.page.html);
        let viewport = engine::Viewport {
            width: self.window_width.saturating_sub(MARGIN_X * 2) / CHAR_WIDTH,
            height: page_viewport_height(self.window_height),
        };
        let images = self.load_images(&document);
        self.layout = engine::layout_document(&document, viewport, &images);
    }

    fn load_images(&mut self, document: &engine::Document) -> HashMap<String, engine::ImageSpec> {
        let mut srcs = Vec::new();
        collect_img_srcs(&document.root, &mut srcs);
        let mut specs = HashMap::new();

        for raw in srcs {
            if raw.is_empty() || specs.contains_key(&raw) {
                continue;
            }
            let key = resolve_src(&self.page.source, &raw);

            let image = if let Some(image) = self.images.originals.get(&key) {
                Some(image.clone())
            } else if self.images.failed.contains(&key) {
                None
            } else {
                match load_decoded_image(&key) {
                    Ok(image) => {
                        let rc = Rc::new(image);
                        self.images.originals.insert(key.clone(), rc.clone());
                        Some(rc)
                    }
                    Err(_) => {
                        self.images.failed.insert(key.clone());
                        None
                    }
                }
            };

            if let Some(image) = image {
                let content_width = self.window_width.saturating_sub(MARGIN_X * 2).max(1) as u32;
                let (scaled_w, scaled_h) = scale_dims(image.width, image.height, content_width);
                specs.insert(
                    raw,
                    engine::ImageSpec {
                        key,
                        cell_width: (scaled_w as usize / CHAR_WIDTH).max(1),
                        cell_height: (scaled_h as usize / LINE_HEIGHT).max(1),
                        pixel_width: scaled_w,
                        pixel_height: scaled_h,
                    },
                );
            }
        }

        specs
    }

    fn scroll_by(&mut self, delta: c_int) {
        self.scroll_y = (self.scroll_y + delta).clamp(0, self.max_scroll_y());
    }

    fn scroll_home(&mut self) {
        self.scroll_y = 0;
    }

    fn scroll_end(&mut self) {
        self.scroll_y = self.max_scroll_y();
    }

    fn max_scroll_y(&self) -> c_int {
        let content_height = self.layout.rect.height as c_int * LINE_HEIGHT as c_int;
        let viewport_height = self.viewport_height() as c_int;
        content_height.saturating_sub(viewport_height).max(0)
    }

    fn viewport_height(&self) -> usize {
        page_viewport_height(self.window_height)
    }

    fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.address_anchor?;
        Some((
            anchor.min(self.address_cursor),
            anchor.max(self.address_cursor),
        ))
    }

    fn delete_backward(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            self.address_text.replace_range(start..end, "");
            self.address_cursor = start;
            self.address_anchor = None;
        } else if self.address_cursor > 0 {
            let start = prev_char_index(&self.address_text, self.address_cursor);
            self.address_text.replace_range(start..self.address_cursor, "");
            self.address_cursor = start;
        }
    }

    fn delete_forward(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            self.address_text.replace_range(start..end, "");
            self.address_cursor = start;
            self.address_anchor = None;
        } else if self.address_cursor < self.address_text.len() {
            let end = next_char_index(&self.address_text, self.address_cursor);
            self.address_text.replace_range(self.address_cursor..end, "");
        }
    }

    fn insert_text(&mut self, text: &str) {
        let (start, end) = self
            .selection_range()
            .unwrap_or((self.address_cursor, self.address_cursor));
        self.address_text.replace_range(start..end, text);
        self.address_cursor = start + text.len();
        self.address_anchor = None;
    }

    fn move_cursor(&mut self, right: bool, extend: bool) {
        let old = self.address_cursor;
        let new = if right {
            next_char_index(&self.address_text, old)
        } else {
            prev_char_index(&self.address_text, old)
        };
        if new == old {
            return;
        }
        if extend {
            if self.address_anchor.is_none() {
                self.address_anchor = Some(old);
            }
        } else {
            self.address_anchor = None;
        }
        self.address_cursor = new;
    }

    fn move_cursor_home(&mut self, extend: bool) {
        let old = self.address_cursor;
        if extend && self.address_anchor.is_none() {
            self.address_anchor = Some(old);
        } else if !extend {
            self.address_anchor = None;
        }
        self.address_cursor = 0;
    }

    fn move_cursor_end(&mut self, extend: bool) {
        let old = self.address_cursor;
        if extend && self.address_anchor.is_none() {
            self.address_anchor = Some(old);
        } else if !extend {
            self.address_anchor = None;
        }
        self.address_cursor = self.address_text.len();
    }

    fn select_all(&mut self) {
        self.address_anchor = Some(0);
        self.address_cursor = self.address_text.len();
    }

    fn copy_selection(&self) -> Option<String> {
        self.selection_range()
            .map(|(start, end)| self.address_text[start..end].to_string())
    }
}

fn page_viewport_height(window_height: usize) -> usize {
    window_height.saturating_sub(MARGIN_Y + STATUS_BAR_HEIGHT + 18)
}

fn load_page(target: Option<&str>) -> BrowserPage {
    match target {
        Some("ghostab:newpage") => BrowserPage {
            source: "ghostab:newpage".to_string(),
            html: NEWTAB_HTML.to_string(),
            title: "New Tab".to_string(),
        },
        Some("ghostab:imagedemo") => {
            examples_page("imagedemo.html", "ghostab:imagedemo", "Ghostab Image Demo")
        }
        Some("ghostab:linkdemo") => {
            examples_page("linkdemo.html", "ghostab:linkdemo", "Ghostab Link Demo")
        }
        Some(target) if target.starts_with("ghostab:") => BrowserPage {
            source: target.to_string(),
            html: error_page(
                "Unknown page",
                &format!("There is no Ghostab page named '{target}'."),
            ),
            title: "Unknown page".to_string(),
        },
        Some(target) if target.starts_with("about:") => match target {
            "about:sample" => BrowserPage {
                source: target.to_string(),
                html: SAMPLE_HTML.to_string(),
                title: "Ghostab Sample".to_string(),
            },
            "about:blank" => BrowserPage {
                source: target.to_string(),
                html: "<html><body></body></html>".to_string(),
                title: "about:blank".to_string(),
            },
            _ => BrowserPage {
                source: target.to_string(),
                html: error_page(
                    "Unknown page",
                    &format!("There is no about page named '{target}'."),
                ),
                title: "Unknown page".to_string(),
            },
        },
        Some(target) if is_url(target) => fetch_url(target),
        Some(path) => match fs::read_to_string(path) {
            Ok(contents) => BrowserPage {
                title: extract_title(&contents),
                source: path.to_string(),
                html: contents,
            },
            Err(error) => BrowserPage {
                source: path.to_string(),
                html: error_page("Could not read file", &format!("{path}: {error}")),
                title: "Could not read file".to_string(),
            },
        },
        None => BrowserPage {
            source: "ghostab:newpage".to_string(),
            html: NEWTAB_HTML.to_string(),
            title: "Ghostab New Tab".to_string(),
        },
    }
}

fn is_url(target: &str) -> bool {
    target.starts_with("http://") || target.starts_with("https://")
}

fn connection_state(source: &str) -> ConnectionState {
    if source.starts_with("ghostab:") {
        return ConnectionState::BuiltIn;
    }
    let host = source
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', ':']).next().unwrap_or(""))
        .unwrap_or("");
    if host == "localhost" || host == "127.0.0.1" || source.starts_with("localhost") {
        ConnectionState::Local
    } else if source.starts_with("https://") {
        ConnectionState::Protected
    } else if source.starts_with("http://") {
        ConnectionState::Unprotected
    } else if !is_url(source) {
        ConnectionState::Local
    } else {
        ConnectionState::Protected
    }
}

fn examples_dir() -> Option<PathBuf> {
    for candidate in [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples"),
        PathBuf::from("/usr/share/doc/ghostab/examples"),
        PathBuf::from("./examples"),
    ] {
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

fn examples_page(name: &str, source: &str, fallback_title: &str) -> BrowserPage {
    let Some(dir) = examples_dir() else {
        return BrowserPage {
            source: source.to_string(),
            html: error_page(
                "Missing examples",
                &format!("Could not locate the examples folder for '{name}'."),
            ),
            title: fallback_title.to_string(),
        };
    };
    let path = dir.join(name);
    match fs::read_to_string(&path) {
        Ok(contents) => {
            let dir_str = dir.to_string_lossy().into_owned();
            let html = absolutize_local_refs(&contents, &dir_str);
            let title = extract_title(&html);
            BrowserPage {
                source: source.to_string(),
                html,
                title,
            }
        }
        Err(error) => BrowserPage {
            source: source.to_string(),
            html: error_page("Could not read example", &format!("{path:?}: {error}")),
            title: fallback_title.to_string(),
        },
    }
}

fn absolutize_local_refs(html: &str, dir: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        let pos = match (rest.find("src=\""), rest.find("href=\"")) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => break,
        };
        let value_start = pos + 5;
        let Some(rel_end) = rest[value_start..].find('"') else {
            break;
        };
        let value_end = value_start + rel_end;
        let value = &rest[value_start..value_end];
        out.push_str(&rest[..value_start]);
        if is_relative_ref(value) {
            out.push_str(&format!("{dir}/{value}"));
        } else {
            out.push_str(value);
        }
        rest = &rest[value_end..];
    }
    out.push_str(rest);
    out
}

fn is_relative_ref(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with("http://")
        && !value.starts_with("https://")
        && !value.starts_with('/')
        && !value.starts_with('#')
        && !value.starts_with("about:")
        && !value.starts_with("ghostab:")
        && !value.starts_with("mailto:")
        && !value.starts_with("javascript:")
        && !value.starts_with("data:")
}

fn search_url_for(settings: &Settings, query: &str) -> String {
    let q = urlencode_query(query);
    match settings.search_engine {
        SearchEngine::Startpage => format!("https://www.startpage.com/sp/search?query={q}"),
        SearchEngine::Custom => {
            if settings.search_url.contains("%s") {
                settings.search_url.replace("%s", &q)
            } else {
                format!("https://www.startpage.com/sp/search?query={q}")
            }
        }
    }
}

fn normalize_navigation_target(input: &str, settings: &Settings) -> String {
    let trimmed = input.trim();

    if trimmed.is_empty()
        || is_url(trimmed)
        || trimmed.starts_with("about:")
        || trimmed.starts_with("ghostab:")
        || fs::metadata(trimmed).is_ok()
    {
        trimmed.to_string()
    } else if looks_like_localhost(trimmed) {
        format!("http://{trimmed}")
    } else if trimmed.contains('.') && !trimmed.chars().any(|c| c.is_whitespace()) {
        format!("https://{trimmed}")
    } else {
        search_url_for(settings, trimmed)
    }
}

fn looks_like_localhost(input: &str) -> bool {
    !input.chars().any(|c| c.is_whitespace())
        && (input.starts_with("localhost")
            || input.starts_with("127.0.0.1")
            || input.starts_with("0.0.0.0")
            || input.starts_with("[::1]"))
}

fn urlencode_query(input: &str) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn extract_title(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    if let Some(start) = lower.find("<title") {
        if let Some(gt) = html[start..].find('>') {
            let after = start + gt + 1;
            if let Some(end_rel) = lower[after..].find("</title") {
                let title = html[after..after + end_rel].trim();
                if !title.is_empty() {
                    return title.to_string();
                }
            }
        }
    }
    "Ghostab".to_string()
}

fn fetch_url(url: &str) -> BrowserPage {
    let mut cmd = Command::new("curl");
    cmd.args([
        "--location",
        "--max-time",
        "20",
        "--silent",
        "--show-error",
        "-A",
        "Mozilla/5.0 (X11; Linux x86_64; rv:115.0) Gecko/20100101 Firefox/115.0",
    ]);
    if let Some(query) = url.strip_prefix("https://www.startpage.com/sp/search?query=") {
        cmd.arg("-d").arg(format!("query={query}"));
        cmd.arg("https://www.startpage.com/sp/search");
    } else {
        cmd.arg(url);
    }
    let output = cmd.output();

    match output {
        Ok(output) if output.status.success() => {
            let html = String::from_utf8_lossy(&output.stdout).into_owned();
            BrowserPage {
                title: extract_title(&html),
                source: url.to_string(),
                html,
            }
        }
        Ok(output) => BrowserPage {
            source: url.to_string(),
            html: error_page(
                "Could not load website",
                &String::from_utf8_lossy(&output.stderr),
            ),
            title: "Could not load website".to_string(),
        },
        Err(error) => BrowserPage {
            source: url.to_string(),
            html: error_page(
                "Could not start network loader",
                &format!("Ghostab uses curl for this first network milestone: {error}"),
            ),
            title: "Could not start network loader".to_string(),
        },
    }
}

fn error_page(title: &str, message: &str) -> String {
    format!(
        "<html><body><h1>{}</h1><p>{}</p></body></html>",
        escape_html(title),
        escape_html(message)
    )
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn collect_img_srcs(node: &engine::dom::Node, out: &mut Vec<String>) {
    if let engine::dom::NodeKind::Element(element) = &node.kind {
        if element.tag_name == "img" {
            if let Some(src) = element.attributes.get("src") {
                out.push(src.clone());
            }
        }
    }
    for child in &node.children {
        collect_img_srcs(child, out);
    }
}

fn load_decoded_image(key: &str) -> Result<DecodedImage, String> {
    let bytes = if is_url(key) {
        fetch_bytes(key)?
    } else {
        fs::read(key).map_err(|error| error.to_string())?
    };
    let image = image::load_from_memory(&bytes).map_err(|error| error.to_string())?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(DecodedImage {
        width,
        height,
        rgba: rgba.into_raw(),
    })
}

fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("curl")
        .args(["--location", "--max-time", "20", "--silent", "--show-error", url])
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

fn scale_dims(source_width: u32, source_height: u32, max_width: u32) -> (u32, u32) {
    if source_width <= max_width {
        (source_width.max(1), source_height.max(1))
    } else {
        let height = ((source_height as u64 * max_width as u64) / source_width as u64).max(1) as u32;
        (max_width, height)
    }
}

fn resolve_src(base: &str, target: &str) -> String {
    if is_url(target) {
        return target.to_string();
    }
    if target.starts_with('/') {
        if let Some((scheme, rest)) = base.split_once("://") {
            let host = rest.split('/').next().unwrap_or("");
            return format!("{scheme}://{host}{target}");
        }
        return target.to_string();
    }
    let base_dir = base.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    if base_dir.is_empty() {
        return target.to_string();
    }
    format!("{base_dir}/{target}")
}

fn prev_char_index(text: &str, index: usize) -> usize {
    let prefix = &text[..index];
    match prefix.chars().next_back() {
        Some(ch) => prefix.len() - ch.len_utf8(),
        None => 0,
    }
}

fn next_char_index(text: &str, index: usize) -> usize {
    let suffix = &text[index..];
    match suffix.chars().next() {
        Some(ch) => index + ch.len_utf8(),
        None => index,
    }
}

static ICON_PNG: &[u8] = include_bytes!("../Ghostab.png");
static PROTECTED_PNG: &[u8] = include_bytes!("../assets/Protected.png");
static LOCALFILE_PNG: &[u8] = include_bytes!("../assets/Localfile.png");
static UNPROTECTED_PNG: &[u8] = include_bytes!("../assets/Unprotected.png");

unsafe fn set_window_icon(display: *mut Display, window: c_ulong) {
    let image = match image::load_from_memory(ICON_PNG) {
        Ok(img) => img.to_rgba8(),
        Err(_) => return,
    };
    let width = image.width() as usize;
    let height = image.height() as usize;
    let bytes = image.into_raw();
    let mut argb: Vec<c_ulong> = Vec::with_capacity(width * height);
    for pixel in bytes.chunks_exact(4) {
        let (r, g, b, a) = (
            pixel[0] as c_ulong,
            pixel[1] as c_ulong,
            pixel[2] as c_ulong,
            pixel[3] as c_ulong,
        );
        let alpha = if (r | g | b) == 0 { 0 } else { a };
        argb.push((alpha << 24) | (r << 16) | (g << 8) | b);
    }
    let out_size = 256usize;
    let scale = 0.75f64;
    let content = (out_size as f64 * scale).round() as usize;
    let inset = (out_size - content) / 2;
    let mut data: Vec<c_ulong> = Vec::with_capacity(2 + out_size * out_size);
    data.push(out_size as c_ulong);
    data.push(out_size as c_ulong);
    for y in 0..out_size {
        for x in 0..out_size {
            if x >= inset && x < inset + content && y >= inset && y < inset + content {
                let sx = (((x - inset) as f64) / scale).min((width - 1) as f64) as usize;
                let sy = (((y - inset) as f64) / scale).min((height - 1) as f64) as usize;
                data.push(argb[sy * width + sx]);
            } else {
                data.push(0);
            }
        }
    }
    let atom = intern_atom(display, "_NET_WM_ICON");
    let type_atom = intern_atom(display, "CARDINAL");
    XChangeProperty(
        display,
        window,
        atom,
        type_atom,
        32,
        PROP_MODE_REPLACE,
        data.as_ptr() as *const c_void,
        data.len() as c_int,
    );
}

unsafe fn run_x11_window(mut app: BrowserApp) {
    XSetErrorHandler(Some(ignore_x_error));
    let display = XOpenDisplay(ptr::null());
    if display.is_null() {
        eprintln!("ghostab: could not connect to the X display");
        std::process::exit(1);
    }
    let screen = XDefaultScreen(display);
    let root = XRootWindow(display, screen);
    let black = XBlackPixel(display, screen);
    let white = XWhitePixel(display, screen);

    let window = XCreateSimpleWindow(
        display,
        root,
        0,
        0,
        WINDOW_WIDTH as c_uint,
        WINDOW_HEIGHT as c_uint,
        1,
        black,
        white,
    );

    let title = CString::new("Ghostab").unwrap();
    XStoreName(display, window, title.as_ptr());
    XSelectInput(
        display,
        window,
        (EXPOSURE_MASK
            | KEY_PRESS_MASK
            | BUTTON_PRESS_MASK
            | BUTTON_RELEASE_MASK
            | POINTER_MOTION_MASK
            | LEAVE_WINDOW_MASK
            | STRUCTURE_NOTIFY_MASK) as c_long,
    );
    XMapWindow(display, window);
    set_window_icon(display, window);

    let hand_cursor = XCreateFontCursor(display, XC_HAND2);

    let wm_delete = {
        let atom_name = CString::new("WM_DELETE_WINDOW").unwrap();
        XInternAtom(display, atom_name.as_ptr(), 0)
    };
    XSetWMProtocols(display, window, &wm_delete as *const c_ulong as *mut c_ulong, 1);

    let gc = XCreateGC(display, window, 0, ptr::null_mut());
    let visual = XDefaultVisual(display, screen);
    let colormap = XDefaultColormap(display, screen);
    let xft_draw = XftDrawCreate(display, window, visual, colormap);
    XFT_DRAW = xft_draw;
    let font = load_system_font(display);
    XFT_FONT = font;
    eprintln!("ghostab-log: ft_init_for_font -> {}", ft_init_for_font(font));
    eprintln!(
        "ghostab-log: init window=0x{:x} xft_draw={:?} font={:?}",
        window, xft_draw, font
    );
    eprintln!(
        "ghostab-log: metrics text_width('about:sample')={} text_width('a')={} ascent={} descent={}",
        text_width(font, "about:sample"),
        text_width(font, "a"),
        if font.is_null() { -1 } else { (*font).ascent },
        if font.is_null() { -1 } else { (*font).descent },
    );

    loop {
        let mut event = std::mem::MaybeUninit::<XEvent>::zeroed();
        XNextEvent(display, event.as_mut_ptr());
        let event = event.assume_init();

        match event.get_type() {
            EXPOSE => {
                draw_browser(display, window, gc, font, screen, &app);
                XFlush(display);
            }
            BUTTON_PRESS => {
                let button = event.xbutton;
                let x = button.x;
                let y = button.y;
                eprintln!("ghostab-log: button_press button={} x={} y={}", button.button, x, y);
                if button.button == 4 {
                    app.scroll_by(-SCROLL_STEP);
                    draw_browser(display, window, gc, font, screen, &app);
                    XFlush(display);
                } else if button.button == 5 {
                    app.scroll_by(SCROLL_STEP);
                    draw_browser(display, window, gc, font, screen, &app);
                    XFlush(display);
                } else if button.button == 2 {
                    let should_quit = match tab_at(x, y, app.window_width, &tab_titles(&app)) {
                        Some(TabHit::Tab(index)) | Some(TabHit::Close(index)) => {
                            eprintln!("ghostab-log: middle-click -> close tab {index}");
                            app.close_tab(index)
                        }
                        _ => false,
                    };
                    if should_quit {
                        break;
                    }
                    draw_browser(display, window, gc, font, screen, &app);
                    XFlush(display);
                } else {
                    let mut consumed = false;
                    if let Some(menu) = app.open_menu {
                        if let Some(item) = menu_item_at(menu, x, y) {
                            let action = menu_item_action(menu, item);
                            app.open_menu = None;
                            app.menu_hover = None;
                            consumed = true;
                            let mut quit = false;
                            match action {
                                MenuAction::Reload => {
                                    app.reload();
                                }
                                MenuAction::Settings => {
                                    show_settings_window(display, root, black, white, &mut app.settings);
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
                                            claim_clipboard(display, window, &mut app);
                                        }
                                    }
                                }
                                MenuAction::Paste => {
                                    if app.address_focused {
                                        paste_clipboard(display, window, &mut app);
                                    }
                                }
                                MenuAction::ScrollTop => app.scroll_home(),
                                MenuAction::ScrollUp => app.scroll_by(-SCROLL_STEP),
                                MenuAction::ScrollDown => app.scroll_by(SCROLL_STEP),
                                MenuAction::ScrollBottom => app.scroll_end(),
                                MenuAction::About => {
                                    show_about_window(display, root, black, white);
                                }
                                MenuAction::None => {}
                            }
                            if quit {
                                break;
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
                        } else if let Some(hit) = tab_at(x, y, app.window_width, &tab_titles(&app)) {
                            eprintln!("ghostab-log: click -> tab {:?}", hit);
                            let mut should_quit = false;
                            match hit {
                                TabHit::Tab(index) => app.switch_tab(index),
                                TabHit::Close(index) => should_quit = app.close_tab(index),
                                TabHit::NewTab => app.new_tab(),
                            }
                            sync_link_cursor(display, window, hand_cursor, false);
                            if should_quit {
                                break;
                            }
                        } else if point_in_shield(x, y) {
                            eprintln!("ghostab-log: click -> shield");
                            show_shield_window(
                                display,
                                root,
                                black,
                                white,
                                connection_state(&app.page.source),
                            );
                            sync_link_cursor(display, window, hand_cursor, false);
                        } else if let Some(button) = nav_button_at(x, y, app.window_width) {
                            eprintln!("ghostab-log: click -> nav button {:?}", button);
                            match button {
                                NavButton::Back => app.go_back(),
                                NavButton::Forward => app.go_forward(),
                                NavButton::Home => app.navigate_new("ghostab:newpage"),
                                NavButton::Refresh => app.reload(),
                            }
                            sync_link_cursor(display, window, hand_cursor, false);
                        } else if point_in_go_button(x, y, app.window_width) {
                            eprintln!("ghostab-log: click -> Go button");
                            app.navigate();
                            sync_link_cursor(display, window, hand_cursor, false);
                        } else if point_in_address_bar(x, y, app.window_width) {
                            eprintln!("ghostab-log: click -> address bar focused");
                            app.address_focused = true;
                            app.address_cursor = cursor_for_click(&app, font, x);
                            app.address_anchor = Some(app.address_cursor);
                            app.mouse_down = true;
                        } else {
                            if let Some(href) = find_link_at(&app.layout, font, x, y, app.scroll_y) {
                                app.open_link(&href);
                                sync_link_cursor(display, window, hand_cursor, false);
                            } else {
                                app.address_focused = false;
                            }
                        }
                    }
                    draw_browser(display, window, gc, font, screen, &app);
                    XFlush(display);
                }
            }
            BUTTON_RELEASE => {
                app.mouse_down = false;
            }
            KEY_PRESS => {
                let (input, mods, keysym) = read_key(event.xkey);
                eprintln!("ghostab-log: key_press input={:?} ctrl={} address_focused={} menu={:?}",
                    log_input(&input), mods.ctrl, app.address_focused, app.open_menu);
                let mut redraw = true;
                if app.open_menu.is_some() {
                    match input {
                        KeyInput::Escape => {
                            app.open_menu = None;
                            app.menu_hover = None;
                        }
                        _ => redraw = false,
                    }
                } else if app.address_focused {
                    match input {
                        KeyInput::Escape => app.address_focused = false,
                        KeyInput::Enter => app.navigate(),
                        KeyInput::Backspace => app.delete_backward(),
                        KeyInput::Delete => app.delete_forward(),
                        KeyInput::Left => app.move_cursor(false, mods.shift),
                        KeyInput::Right => app.move_cursor(true, mods.shift),
                        KeyInput::Home => app.move_cursor_home(mods.shift),
                        KeyInput::End => app.move_cursor_end(mods.shift),
                        KeyInput::Text(text) => app.insert_text(&text),
                        KeyInput::Other if mods.ctrl => {
                            match keysym {
                                0x61 | 0x41 => app.select_all(),     // a/A
                                0x63 | 0x43 => {                     // c/C
                                    if let Some(selection) = app.copy_selection() {
                                        app.clipboard_text = selection;
                                        claim_clipboard(display, window, &mut app);
                                    }
                                }
                                0x76 | 0x56 => paste_clipboard(display, window, &mut app), // v/V
                                _ => redraw = false,
                            }
                        }
                        _ => redraw = false,
                    }
                } else if mods.ctrl {
                    match keysym {
                        0x74 | 0x54 => app.new_tab(),                                  // Ctrl+T
                        0x77 | 0x57 => {                                          // Ctrl+W
                            if app.close_tab(app.active_tab) {
                                break;
                            }
                        }
                        0x72 | 0x52 => app.reload(),                                   // Ctrl+R
                        0x6c | 0x4c => {                                               // Ctrl+L
                            app.address_focused = true;
                            app.select_all();
                        }
                        0xff09 => {                                                    // Ctrl+Tab
                            if mods.shift {
                                app.prev_tab();
                            } else {
                                app.next_tab();
                            }
                        }
                        _ => redraw = false,
                    }
                } else {
                    match input {
                        KeyInput::Escape => break,
                        KeyInput::PageUp => app.scroll_by(-(app.viewport_height() as c_int)),
                        KeyInput::PageDown => app.scroll_by(app.viewport_height() as c_int),
                        KeyInput::Home => app.scroll_home(),
                        KeyInput::End => app.scroll_end(),
                        _ => redraw = false,
                    }
                }
                if redraw {
                    draw_browser(display, window, gc, font, screen, &app);
                    XFlush(display);
                }
            }
            MOTION_NOTIFY => {
                let x = event.xmotion.x;
                let y = event.xmotion.y;
                let mut changed = false;
                if let Some(menu) = app.open_menu {
                    let hover = menu_item_at(menu, x, y).map(|item| (menu, item));
                    if hover != app.menu_hover {
                        app.menu_hover = hover;
                        changed = true;
                    }
                }
                if app.mouse_down && app.address_focused {
                    let cursor = cursor_for_click(&app, font, x);
                    if cursor != app.address_cursor {
                        app.address_cursor = cursor;
                        changed = true;
                    }
                }
                let hover = find_link_at(&app.layout, font, x, y, app.scroll_y);
                if hover != app.hover_href {
                    let over_link = hover.is_some();
                    app.hover_href = hover;
                    sync_link_cursor(display, window, hand_cursor, over_link);
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
                let hover_close = close_button_at(x, y, &tab_titles(&app));
                if hover_close != app.hover_close {
                    app.hover_close = hover_close;
                    changed = true;
                }
                if changed {
                    draw_browser(display, window, gc, font, screen, &app);
                    XFlush(display);
                }
            }
            LEAVE_NOTIFY => {
                if app.hover_href.is_some()
                    || app.hover_button.is_some()
                    || app.hover_close.is_some()
                    || app.hover_shield
                {
                    app.hover_href = None;
                    app.hover_button = None;
                    app.hover_close = None;
                    app.hover_shield = false;
                    sync_link_cursor(display, window, hand_cursor, false);
                    draw_browser(display, window, gc, font, screen, &app);
                    XFlush(display);
                }
            }
            SELECTION_REQUEST => {
                serve_selection_request(display, event.xselectionrequest, &app.clipboard_text);
            }
            SELECTION_CLEAR => {
                app.owns_clipboard = false;
            }
            SELECTION_NOTIFY => {
                handle_selection_notify(display, window, event.xselection, &mut app);
                draw_browser(display, window, gc, font, screen, &app);
                XFlush(display);
            }
            CLIENT_MESSAGE => {
                let data = event.xclient.data.longs[0] as c_ulong;
                if data == wm_delete {
                    break;
                }
            }
            CONFIGURE_NOTIFY => {
                let configure = event.xconfigure;
                let width = configure.width as usize;
                let height = configure.height as usize;
                if width != app.window_width || height != app.window_height {
                    app.resize(width, height);
                    draw_browser(display, window, gc, font, screen, &app);
                    XFlush(display);
                }
            }
            _ => {}
        }
    }

    XFreeGC(display, gc);
    if !xft_draw.is_null() {
        XftDrawDestroy(xft_draw);
    }
    if !font.is_null() {
        XftFontClose(display, font);
    }
    XFreeCursor(display, hand_cursor);
    XDestroyWindow(display, window);
    XCloseDisplay(display);
}

fn refresh_button_x(window_width: usize) -> c_int {
    window_width as c_int - NAV_BUTTON_SIZE - 16
}

fn home_button_x(window_width: usize) -> c_int {
    refresh_button_x(window_width) - NAV_BUTTON_SIZE - NAV_BUTTON_GAP
}

fn forward_button_x(window_width: usize) -> c_int {
    home_button_x(window_width) - NAV_BUTTON_SIZE - NAV_BUTTON_GAP
}

fn back_button_x(window_width: usize) -> c_int {
    forward_button_x(window_width) - NAV_BUTTON_SIZE - NAV_BUTTON_GAP
}

fn go_button_x(window_width: usize) -> c_int {
    back_button_x(window_width) - GO_WIDTH as c_int - NAV_BUTTON_GAP
}

fn nav_button_at(x: c_int, y: c_int, window_width: usize) -> Option<NavButton> {
    if y < ADDRESS_Y || y >= ADDRESS_Y + NAV_BUTTON_SIZE {
        return None;
    }
    let candidates = [
        (back_button_x(window_width), NavButton::Back),
        (forward_button_x(window_width), NavButton::Forward),
        (home_button_x(window_width), NavButton::Home),
        (refresh_button_x(window_width), NavButton::Refresh),
    ];
    for (bx, button) in candidates {
        if x >= bx && x < bx + NAV_BUTTON_SIZE {
            return Some(button);
        }
    }
    None
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum TabHit {
    Tab(usize),
    Close(usize),
    NewTab,
}

fn address_bar_width(window_width: usize) -> c_uint {
    (go_button_x(window_width) - NAV_BUTTON_GAP - ADDRESS_BAR_X).max(80) as c_uint
}

fn point_in_shield(x: c_int, y: c_int) -> bool {
    x >= SHIELD_X
        && x < SHIELD_X + SHIELD_SIZE
        && y >= ADDRESS_Y
        && y < ADDRESS_Y + SHIELD_SIZE
}

fn point_in_go_button(x: c_int, y: c_int, window_width: usize) -> bool {
    let go_x = go_button_x(window_width);
    x >= go_x
        && x < go_x + GO_WIDTH as c_int
        && y >= ADDRESS_Y
        && y < ADDRESS_Y + GO_HEIGHT as c_int
}

fn point_in_address_bar(x: c_int, y: c_int, window_width: usize) -> bool {
    x >= ADDRESS_BAR_X
        && x < ADDRESS_BAR_X + address_bar_width(window_width) as c_int
        && y >= ADDRESS_Y
        && y < ADDRESS_Y + ADDRESS_HEIGHT as c_int
}

fn point_in_tab_bar(y: c_int) -> bool {
    y >= TAB_BAR_Y && y < TAB_BAR_Y + TAB_BAR_HEIGHT
}

fn tab_width(label: &str) -> c_int {
    let label = shorten(label, 14);
    let w = unsafe { text_width(XFT_FONT, &label) } + TAB_CLOSE_WIDTH + 30;
    w.clamp(90, 220)
}

fn tab_xs(labels: &[String]) -> Vec<c_int> {
    let mut xs = Vec::with_capacity(labels.len());
    let mut cur = ADDRESS_X;
    for label in labels {
        xs.push(cur);
        cur += tab_width(label) + NAV_BUTTON_GAP;
    }
    xs
}

fn tab_titles(app: &BrowserApp) -> Vec<String> {
    app.tabs.iter().map(|tab| tab.page.title.clone()).collect()
}

fn new_tab_button_x(window_width: usize) -> c_int {
    window_width as c_int - NEW_TAB_BUTTON_SIZE - 16
}

fn tab_at(x: c_int, y: c_int, window_width: usize, labels: &[String]) -> Option<TabHit> {
    if !point_in_tab_bar(y) {
        return None;
    }
    let nx = new_tab_button_x(window_width);
    if x >= nx && x < nx + NEW_TAB_BUTTON_SIZE {
        return Some(TabHit::NewTab);
    }
    let xs = tab_xs(labels);
    for (index, tx) in xs.iter().enumerate() {
        let width = tab_width(&labels[index]);
        if x >= *tx && x < tx + width {
            if x >= tx + width - TAB_CLOSE_WIDTH {
                return Some(TabHit::Close(index));
            }
            return Some(TabHit::Tab(index));
        }
    }
    None
}

fn close_button_at(x: c_int, y: c_int, labels: &[String]) -> Option<usize> {
    if !point_in_tab_bar(y) {
        return None;
    }
    let xs = tab_xs(labels);
    for (index, tx) in xs.iter().enumerate() {
        let width = tab_width(&labels[index]);
        if x >= tx + width - TAB_CLOSE_WIDTH && x < tx + width {
            return Some(index);
        }
    }
    None
}

#[derive(Copy, Clone, PartialEq)]
enum MenuAction {
    Reload,
    Settings,
    Quit,
    SelectAll,
    Copy,
    Paste,
    ScrollTop,
    ScrollUp,
    ScrollDown,
    ScrollBottom,
    About,
    None,
}

fn menu_button_x(index: usize) -> c_int {
    8 + index as c_int * (MENU_BUTTON_WIDTH + 4)
}

fn menu_item_y(item: usize) -> c_int {
    MENU_BAR_HEIGHT + 2 + item as c_int * MENU_ITEM_HEIGHT
}

fn point_in_menu_button(x: c_int, y: c_int) -> Option<usize> {
    if y < 0 || y >= MENU_BAR_HEIGHT {
        return None;
    }
    for (i, _) in MENU_LABELS.iter().enumerate() {
        let bx = menu_button_x(i);
        if x >= bx && x < bx + MENU_BUTTON_WIDTH {
            return Some(i);
        }
    }
    None
}

fn menu_item_at(menu: usize, x: c_int, y: c_int) -> Option<usize> {
    let items = MENU_ITEMS.get(menu)?;
    let bx = menu_button_x(menu);
    if x < bx || x > bx + MENU_ITEM_WIDTH {
        return None;
    }
    for (i, _) in items.iter().enumerate() {
        let iy = menu_item_y(i);
        if y >= iy && y < iy + MENU_ITEM_HEIGHT {
            return Some(i);
        }
    }
    None
}

fn menu_item_action(menu: usize, item: usize) -> MenuAction {
    match (menu, item) {
        (0, 0) => MenuAction::Reload,
        (0, 1) => MenuAction::Settings,
        (0, 2) => MenuAction::Quit,
        (1, 0) => MenuAction::SelectAll,
        (1, 1) => MenuAction::Copy,
        (1, 2) => MenuAction::Paste,
        (2, 0) => MenuAction::ScrollTop,
        (2, 1) => MenuAction::ScrollUp,
        (2, 2) => MenuAction::ScrollDown,
        (2, 3) => MenuAction::ScrollBottom,
        (3, 0) => MenuAction::About,
        _ => MenuAction::None,
    }
}

unsafe fn find_link_at(
    node: &engine::LayoutBox,
    font: *mut XftFont,
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
        let ascent = if font.is_null() { 16 } else { (*font).ascent };
        let descent = if font.is_null() { 4 } else { (*font).descent };
        if x >= px && y >= py - ascent && y < py + descent {
            let rel = x - px;
            if !node.links.is_empty() {
                let hit = text_offset_at(text, font, rel);
                if let Some(hit) = hit {
                    for span in &node.links {
                        if span.start <= hit && hit < span.end {
                            return Some(span.href.clone());
                        }
                    }
                }
            } else if let Some(href) = &node.href {
                if rel < text_width(font, text) {
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
        if let Some(href) = find_link_at(child, font, x, y, scroll_y) {
            return Some(href);
        }
    }
    None
}

unsafe fn text_offset_at(text: &str, font: *mut XftFont, rel: c_int) -> Option<usize> {
    if text.is_empty() {
        return None;
    }
    let mut hit = 0usize;
    for (i, _) in text.char_indices() {
        if text_width(font, &text[..i]) > rel {
            break;
        }
        hit = i;
    }
    Some(hit)
}

unsafe fn draw_browser(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    font: *mut XftFont,
    screen: c_int,
    app: &BrowserApp,
) {
    DRAW_COUNT += 1;
    if DRAW_COUNT <= 5 {
        eprintln!(
            "ghostab-log: draw_browser #{} begin",
            unsafe { DRAW_COUNT }
        );
    }
    let width = app.window_width as c_uint;
    let height = app.window_height as c_uint;
    set_fg(display, gc, pal(COLOR_PAGE));
    XFillRectangle(display, window, gc, 0, 0, width, height);
    set_fg(display, gc, pal(COLOR_SURFACE));
    XFillRectangle(
        display,
        window,
        gc,
        18,
        TITLE_BAR_HEIGHT as c_int + 18,
        (app.window_width.saturating_sub(36)) as c_uint,
        (app.window_height.saturating_sub(TITLE_BAR_HEIGHT + STATUS_BAR_HEIGHT + 36)) as c_uint,
    );
    set_fg(display, gc, pal(COLOR_PAGE_BORDER));
    XDrawRectangle(
        display,
        window,
        gc,
        18,
        TITLE_BAR_HEIGHT as c_int + 18,
        (app.window_width.saturating_sub(36)) as c_uint,
        (app.window_height.saturating_sub(TITLE_BAR_HEIGHT + STATUS_BAR_HEIGHT + 36)) as c_uint,
    );
    draw_title_bar(display, window, gc, font, app);
    draw_menu_bar(display, window, gc, font, app);
    let clip = XRectangle {
        x: 0,
        y: TITLE_BAR_HEIGHT as i16,
        width: app.window_width.min(u16::MAX as usize) as u16,
        height: app
            .window_height
            .saturating_sub(TITLE_BAR_HEIGHT + STATUS_BAR_HEIGHT)
            .min(u16::MAX as usize) as u16,
    };
    XSetClipRectangles(display, gc, 0, 0, &clip, 1, 0);
    XftDrawSetClipRectangles(XFT_DRAW, 0, 0, &clip, 1);
    draw_box_x11(
        display,
        window,
        gc,
        font,
        &app.layout,
        app.scroll_y,
        app.window_height,
        &app.images,
        screen,
    );
    XftDrawSetClip(XFT_DRAW, ptr::null_mut());
    XSetClipMask(display, gc, 0);
    draw_scrollbar(display, window, gc, app);
    draw_status_bar(display, window, gc, app);
}

unsafe fn draw_title_bar(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    font: *mut XftFont,
    app: &BrowserApp,
) {
    let go_x = go_button_x(app.window_width);
    let address_width = address_bar_width(app.window_width);

    set_fg(display, gc, pal(COLOR_TITLE_BAR));
    XFillRectangle(
        display,
        window,
        gc,
        0,
        0,
        app.window_width as c_uint,
        TITLE_BAR_HEIGHT as c_uint,
    );
    set_fg(display, gc, pal(COLOR_TITLE_LINE));
    XFillRectangle(
        display,
        window,
        gc,
        0,
        TITLE_BAR_HEIGHT as c_int,
        app.window_width as c_uint,
        1,
    );

    draw_tab_bar(display, window, gc, font, app);

    draw_shield_button(display, window, gc, app);

    set_fg(display, gc, pal(COLOR_ADDRESS_BG));
    XFillRectangle(
        display,
        window,
        gc,
        ADDRESS_BAR_X,
        ADDRESS_Y,
        address_width,
        ADDRESS_HEIGHT,
    );
    set_fg(
        display,
        gc,
        if app.address_focused {
            pal(COLOR_ADDRESS_FOCUS)
        } else {
            pal(COLOR_ADDRESS_BORDER)
        },
    );
    XDrawRectangle(
        display,
        window,
        gc,
        ADDRESS_BAR_X,
        ADDRESS_Y,
        address_width,
        ADDRESS_HEIGHT,
    );

    let avail = address_width as c_int - 28;
    let (shown, start) = visible_address(app, font, avail);
    set_fg(display, gc, pal(COLOR_ADDRESS_TEXT));
    draw_string(
        display,
        window,
        gc,
        ADDRESS_TEXT_X,
        centered_baseline(font, ADDRESS_Y, ADDRESS_HEIGHT as c_int),
        &shown,
    );

    if let Some((sel_start, sel_end)) = app.selection_range() {
        let rel_start = sel_start.saturating_sub(start).min(shown.len());
        let rel_end = sel_end.saturating_sub(start).min(shown.len());
        if rel_end > rel_start {
            let x0 = ADDRESS_TEXT_X + text_width(font, &shown[..rel_start]);
            let x1 = ADDRESS_TEXT_X + text_width(font, &shown[..rel_end]);
            set_fg(display, gc, pal(COLOR_SELECTION_BG));
            XFillRectangle(
                display,
                window,
                gc,
                x0,
                ADDRESS_Y + 4,
                (x1 - x0) as c_uint,
                ADDRESS_HEIGHT - 8,
            );
            set_fg(display, gc, pal(COLOR_SELECTION_TEXT));
            draw_string(
                display,
                window,
                gc,
                x0,
                centered_baseline(font, ADDRESS_Y, ADDRESS_HEIGHT as c_int),
                &shown[rel_start..rel_end],
            );
        }
    }

    if app.address_focused {
        let rel_cursor = app.address_cursor - start;
        let caret_x = ADDRESS_TEXT_X + text_width(font, &shown[..rel_cursor]);
        eprintln!(
            "ghostab-log: caret cursor={} start={} rel={} shown_len={} caret_x={}",
            app.address_cursor,
            start,
            rel_cursor,
            shown.len(),
            caret_x
        );
        set_fg(display, gc, pal(COLOR_CARET));
        XDrawLine(
            display,
            window,
            gc,
            caret_x,
            ADDRESS_Y + 5,
            caret_x,
            ADDRESS_Y + ADDRESS_HEIGHT as c_int - 5,
        );
    }

    set_fg(display, gc, pal(COLOR_GO_BG));
    XFillRectangle(display, window, gc, go_x, ADDRESS_Y, GO_WIDTH, GO_HEIGHT);
    set_fg(display, gc, pal(COLOR_GO_BORDER));
    XDrawRectangle(display, window, gc, go_x, ADDRESS_Y, GO_WIDTH, GO_HEIGHT);
    set_fg(display, gc, pal(COLOR_GO_TEXT));
    draw_string(
        display,
        window,
        gc,
        go_x + 14,
        centered_baseline(font, ADDRESS_Y, GO_HEIGHT as c_int),
        "Go",
    );

    draw_nav_button(
        display,
        window,
        gc,
        app,
        back_button_x(app.window_width),
        !app.history_back.is_empty(),
        NavButton::Back,
    );
    draw_nav_button(
        display,
        window,
        gc,
        app,
        forward_button_x(app.window_width),
        !app.history_forward.is_empty(),
        NavButton::Forward,
    );
    draw_nav_button(
        display,
        window,
        gc,
        app,
        home_button_x(app.window_width),
        true,
        NavButton::Home,
    );
    draw_nav_button(
        display,
        window,
        gc,
        app,
        refresh_button_x(app.window_width),
        true,
        NavButton::Refresh,
    );
}

unsafe fn draw_nav_button(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    app: &BrowserApp,
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
    set_fg(display, gc, fill);
    XFillRectangle(
        display,
        window,
        gc,
        x,
        ADDRESS_Y,
        NAV_BUTTON_SIZE as c_uint,
        NAV_BUTTON_SIZE as c_uint,
    );
    set_fg(
        display,
        gc,
        if hovered { pal(COLOR_GO_BG) } else { pal(COLOR_BUTTON_BORDER) },
    );
    XDrawRectangle(
        display,
        window,
        gc,
        x,
        ADDRESS_Y,
        NAV_BUTTON_SIZE as c_uint,
        NAV_BUTTON_SIZE as c_uint,
    );
    let icon = if enabled { pal(COLOR_BUTTON_TEXT) } else { pal(COLOR_MUTED_TEXT) };
    let center_x = x + NAV_BUTTON_SIZE / 2;
    let center_y = ADDRESS_Y + NAV_BUTTON_SIZE / 2;
    match button {
        NavButton::Back => draw_chevron(display, window, gc, center_x, center_y, false, icon),
        NavButton::Forward => draw_chevron(display, window, gc, center_x, center_y, true, icon),
        NavButton::Home => draw_home_icon(display, window, gc, center_x, center_y, icon),
        NavButton::Refresh => draw_refresh_icon(display, window, gc, center_x, center_y, icon),
    }
}

unsafe fn draw_shield_button(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    app: &BrowserApp,
) {
    let hovered = app.hover_shield;
    let state = connection_state(&app.page.source);
    set_fg(
        display,
        gc,
        if hovered { pal(COLOR_BUTTON_HOVER) } else { pal(COLOR_BUTTON_BG) },
    );
    XFillRectangle(
        display,
        window,
        gc,
        SHIELD_X,
        ADDRESS_Y,
        SHIELD_SIZE as c_uint,
        SHIELD_SIZE as c_uint,
    );
    set_fg(
        display,
        gc,
        if hovered { pal(COLOR_GO_BG) } else { pal(COLOR_BUTTON_BORDER) },
    );
    XDrawRectangle(
        display,
        window,
        gc,
        SHIELD_X,
        ADDRESS_Y,
        SHIELD_SIZE as c_uint,
        SHIELD_SIZE as c_uint,
    );
    draw_shield_icon(
        display,
        window,
        gc,
        SHIELD_X + SHIELD_SIZE / 2,
        ADDRESS_Y + SHIELD_SIZE / 2,
        state,
    );
}

unsafe fn draw_shield_icon(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    cx: c_int,
    cy: c_int,
    state: ConnectionState,
) {
    let safe = state != ConnectionState::Unprotected;
    let mut points = [
        XPoint { x: (cx - 10) as c_short, y: (cy - 11) as c_short },
        XPoint { x: (cx + 10) as c_short, y: (cy - 11) as c_short },
        XPoint { x: (cx + 10) as c_short, y: (cy + 1) as c_short },
        XPoint { x: (cx + 4) as c_short, y: (cy + 8) as c_short },
        XPoint { x: cx as c_short, y: (cy + 15) as c_short },
        XPoint { x: (cx - 4) as c_short, y: (cy + 8) as c_short },
        XPoint { x: (cx - 10) as c_short, y: (cy + 1) as c_short },
        XPoint { x: (cx - 10) as c_short, y: (cy - 11) as c_short },
    ];
    set_fg(display, gc, pal(COLOR_SHIELD_BLUE));
    XFillPolygon(display, window, gc, points.as_mut_ptr(), 7, 1, 0);
    set_fg(display, gc, pal(COLOR_SHIELD_OUTLINE));
    XDrawLines(display, window, gc, points.as_mut_ptr(), 8, 0, 0);
    if !safe {
        set_fg(display, gc, pal(COLOR_SHIELD_DANGER));
        XDrawLine(display, window, gc, cx - 9, cy - 9, cx + 9, cy + 9);
        XDrawLine(display, window, gc, cx + 9, cy - 9, cx - 9, cy + 9);
    }
}

unsafe fn draw_chevron(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    cx: c_int,
    cy: c_int,
    forward: bool,
    color: c_ulong,
) {
    let r = 7;
    let (x1, _x2, x3) = if forward {
        (cx - r, cx + r, cx - r)
    } else {
        (cx + r, cx - r, cx + r)
    };
    let tip = cx + if forward { r } else { -r };
    set_fg(display, gc, color);
    let mut points = [
        XPoint {
            x: x1 as c_short,
            y: (cy - 6) as c_short,
        },
        XPoint {
            x: tip as c_short,
            y: cy as c_short,
        },
        XPoint {
            x: x3 as c_short,
            y: (cy + 6) as c_short,
        },
    ];
    XDrawLines(display, window, gc, points.as_mut_ptr(), 3, 0, 0);
}

unsafe fn draw_home_icon(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    cx: c_int,
    cy: c_int,
    color: c_ulong,
) {
    let w = 14;
    let h = 11;
    let x = cx - w / 2;
    let y = cy - h / 2 + 1;
    set_fg(display, gc, color);
    XDrawLine(display, window, gc, x, y + h / 2, cx, y - 2);
    XDrawLine(display, window, gc, cx, y - 2, x + w, y + h / 2);
    XDrawLine(display, window, gc, x + 2, y + h / 2, x + 2, y + h);
    XDrawLine(display, window, gc, x + 2, y + h, x + w - 2, y + h);
    XDrawLine(display, window, gc, x + w - 2, y + h, x + w - 2, y + h / 2);
}

unsafe fn draw_refresh_icon(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    cx: c_int,
    cy: c_int,
    color: c_ulong,
) {
    let r = 6;
    let angle = 50 * 64;
    let end_angle = 270 * 64;
    set_fg(display, gc, color);
    XDrawArc(
        display,
        window,
        gc,
        cx - r,
        cy - r,
        (2 * r) as c_uint,
        (2 * r) as c_uint,
        angle,
        end_angle,
    );
    XDrawLine(display, window, gc, cx + 4, cy - r - 1, cx + 6, cy - r + 3);
    XDrawLine(display, window, gc, cx + 6, cy - r + 3, cx + 2, cy - r + 3);
}

unsafe fn draw_tab_bar(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    font: *mut XftFont,
    app: &BrowserApp,
) {
    set_fg(display, gc, pal(COLOR_TAB_STRIP));
    XFillRectangle(
        display,
        window,
        gc,
        0,
        TAB_BAR_Y,
        app.window_width as c_uint,
        TAB_BAR_HEIGHT as c_uint,
    );
    let labels = tab_titles(app);
    let xs = tab_xs(&labels);
    for (index, x) in xs.iter().enumerate() {
        draw_tab(display, window, gc, font, app, index, *x, tab_width(&labels[index]));
    }
    draw_new_tab_button(display, window, gc, app);
}

unsafe fn draw_tab(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    font: *mut XftFont,
    app: &BrowserApp,
    index: usize,
    x: c_int,
    width: c_int,
) {
    let active = index == app.active_tab;
    let fill = if active {
        pal(COLOR_TAB_ACTIVE_FILL)
    } else {
        pal(COLOR_TAB_INACTIVE_FILL)
    };
    set_fg(display, gc, fill);
    XFillRectangle(
        display,
        window,
        gc,
        x,
        TAB_BAR_Y + 3,
        width as c_uint,
        (TAB_BAR_HEIGHT - 3) as c_uint,
    );
    set_fg(
        display,
        gc,
        if active {
            pal(COLOR_TAB_ACTIVE_ORANGE)
        } else {
            pal(COLOR_TAB_BORDER)
        },
    );
    XDrawRectangle(
        display,
        window,
        gc,
        x,
        TAB_BAR_Y + 3,
        width as c_uint,
        (TAB_BAR_HEIGHT - 3) as c_uint,
    );
    if active {
        set_fg(display, gc, pal(COLOR_TAB_ACTIVE_ORANGE));
        XFillRectangle(
            display,
            window,
            gc,
            x + 1,
            TAB_BAR_Y + 3,
            2,
            (TAB_BAR_HEIGHT - 3) as c_uint,
        );
        XDrawLine(
            display,
            window,
            gc,
            x,
            TAB_BAR_Y + TAB_BAR_HEIGHT - 1,
            x + width,
            TAB_BAR_Y + TAB_BAR_HEIGHT - 1,
        );
    }
    let label = shorten(&app.tabs[index].page.title, 14);
    set_fg(
        display,
        gc,
        if active { pal(COLOR_TAB_TEXT) } else { pal(COLOR_TAB_TEXT_MUTED) },
    );
    draw_string(
        display,
        window,
        gc,
        x + 10,
        centered_baseline(font, TAB_BAR_Y + 3, TAB_BAR_HEIGHT - 3),
        &label,
    );
    let close_x = x + width - TAB_CLOSE_WIDTH;
    let hovered_close = app.hover_close == Some(index);
    if hovered_close {
        set_fg(display, gc, pal(COLOR_TAB_BORDER));
        XFillRectangle(
            display,
            window,
            gc,
            close_x + 1,
            TAB_BAR_Y + 5,
            (TAB_CLOSE_WIDTH - 2) as c_uint,
            (TAB_BAR_HEIGHT - 8) as c_uint,
        );
    }
    let cx = close_x + TAB_CLOSE_WIDTH / 2;
    let cy = TAB_BAR_Y + 3 + (TAB_BAR_HEIGHT - 3) / 2;
    set_fg(
        display,
        gc,
        if hovered_close {
            pal(COLOR_TAB_ACTIVE_ORANGE)
        } else {
            pal(COLOR_TAB_TEXT_MUTED)
        },
    );
    let s = 4;
    XDrawLine(display, window, gc, cx - s, cy - s, cx + s, cy + s);
    XDrawLine(display, window, gc, cx + s, cy - s, cx - s, cy + s);
}

unsafe fn draw_new_tab_button(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    app: &BrowserApp,
) {
    let x = new_tab_button_x(app.window_width);
    set_fg(display, gc, pal(COLOR_TAB_INACTIVE_FILL));
    XFillRectangle(
        display,
        window,
        gc,
        x,
        TAB_BAR_Y + 3,
        NEW_TAB_BUTTON_SIZE as c_uint,
        (TAB_BAR_HEIGHT - 3) as c_uint,
    );
    set_fg(display, gc, pal(COLOR_TAB_BORDER));
    XDrawRectangle(
        display,
        window,
        gc,
        x,
        TAB_BAR_Y + 3,
        NEW_TAB_BUTTON_SIZE as c_uint,
        (TAB_BAR_HEIGHT - 3) as c_uint,
    );
    set_fg(display, gc, pal(COLOR_TAB_TEXT));
    let cx = x + NEW_TAB_BUTTON_SIZE / 2;
    let cy = TAB_BAR_Y + 3 + (TAB_BAR_HEIGHT - 3) / 2;
    XDrawLine(display, window, gc, cx - 5, cy, cx + 5, cy);
    XDrawLine(display, window, gc, cx, cy - 5, cx, cy + 5);
}

unsafe fn draw_menu_bar(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    font: *mut XftFont,
    app: &BrowserApp,
) {
    set_fg(display, gc, pal(COLOR_SURFACE));
    XFillRectangle(
        display,
        window,
        gc,
        0,
        0,
        app.window_width as c_uint,
        MENU_BAR_HEIGHT as c_uint,
    );
    set_fg(display, gc, pal(COLOR_PAGE_BORDER));
    XDrawLine(display, window, gc, 0, MENU_BAR_HEIGHT, app.window_width as c_int, MENU_BAR_HEIGHT);

    for (i, label) in MENU_LABELS.iter().enumerate() {
        let bx = menu_button_x(i);
        let active = app.open_menu == Some(i);
        set_fg(
            display,
            gc,
            if active {
                pal(COLOR_SELECTION_BG)
            } else {
                pal(COLOR_BUTTON_BG)
            },
        );
        XFillRectangle(display, window, gc, bx, 0, MENU_BUTTON_WIDTH as c_uint, MENU_BAR_HEIGHT as c_uint);
        set_fg(
            display,
            gc,
            if active {
                pal(COLOR_SELECTION_TEXT)
            } else {
                pal(COLOR_BUTTON_TEXT)
            },
        );
        draw_string(
            display,
            window,
            gc,
            bx + 10,
            centered_baseline(font, 0, MENU_BAR_HEIGHT),
            label,
        );
    }

    if let Some(menu) = app.open_menu {
        let bx = menu_button_x(menu);
        let items = MENU_ITEMS[menu];
        let panel_height = items.len() as c_int * MENU_ITEM_HEIGHT + 2;
        set_fg(display, gc, pal(COLOR_SURFACE));
        XFillRectangle(
            display,
            window,
            gc,
            bx,
            MENU_BAR_HEIGHT,
            MENU_ITEM_WIDTH as c_uint,
            panel_height as c_uint,
        );
        set_fg(display, gc, pal(COLOR_PAGE_BORDER));
        XDrawRectangle(
            display,
            window,
            gc,
            bx,
            MENU_BAR_HEIGHT,
            MENU_ITEM_WIDTH as c_uint,
            panel_height as c_uint,
        );
        for (j, item) in items.iter().enumerate() {
            let iy = menu_item_y(j);
            if app.menu_hover == Some((menu, j)) {
                set_fg(display, gc, pal(COLOR_SELECTION_BG));
                XFillRectangle(
                    display,
                    window,
                    gc,
                    bx + 1,
                    iy,
                    (MENU_ITEM_WIDTH - 2) as c_uint,
                    (MENU_ITEM_HEIGHT - 1) as c_uint,
                );
            }
            set_fg(display, gc, pal(COLOR_BODY_TEXT));
            draw_string(
                display,
                window,
                gc,
                bx + 10,
                centered_baseline(font, iy, MENU_ITEM_HEIGHT),
                item,
            );
        }
    }
}

unsafe fn draw_box_x11(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    font: *mut XftFont,
    node: &engine::LayoutBox,
    scroll_y: c_int,
    window_height: usize,
    images: &ImageCache,
    screen: c_int,
) {
    if let Some(text) = &node.text {
        let x = (MARGIN_X + node.rect.x * CHAR_WIDTH) as c_int;
        let y = (MARGIN_Y + node.rect.y * LINE_HEIGHT) as c_int - scroll_y;
        if y >= MARGIN_Y as c_int - LINE_HEIGHT as c_int
            && y <= (window_height - STATUS_BAR_HEIGHT - 10) as c_int
        {
            if !node.links.is_empty() {
                draw_text_with_links(display, window, gc, font, x, y, text, &node.links);
            } else if let Some(_) = &node.href {
                set_fg(display, gc, pal(COLOR_LINK));
                draw_string(display, window, gc, x, y, text);
                let pw = text_width(font, text);
                XDrawLine(display, window, gc, x, y + 2, x + pw, y + 2);
            } else {
                set_fg(display, gc, pal(COLOR_BODY_TEXT));
                draw_string(display, window, gc, x, y, text);
            }
        }
    }

    if node.rule {
        let x = (MARGIN_X + node.rect.x * CHAR_WIDTH) as c_int;
        let y = (MARGIN_Y + node.rect.y * LINE_HEIGHT) as c_int - scroll_y;
        let y = y + (LINE_HEIGHT / 2) as c_int;
        if y >= MARGIN_Y as c_int - LINE_HEIGHT as c_int
            && y <= (window_height - STATUS_BAR_HEIGHT - 10) as c_int
        {
            let line_width = (node.rect.width * CHAR_WIDTH) as c_int;
            set_fg(display, gc, pal(COLOR_PAGE_BORDER));
            XDrawLine(display, window, gc, x, y, x + line_width, y);
        }
    }

    if let Some(image) = &node.image {
        let x = (MARGIN_X + node.rect.x * CHAR_WIDTH) as c_int;
        let y = (MARGIN_Y + node.rect.y * LINE_HEIGHT) as c_int - scroll_y;
        let bottom = (window_height - STATUS_BAR_HEIGHT - 10) as c_int;
        if y + image.height_px as c_int >= MARGIN_Y as c_int - LINE_HEIGHT as c_int && y <= bottom {
            if let Some(original) = images.originals.get(&image.source) {
                draw_scaled_image(
                    display,
                    window,
                    gc,
                    original,
                    x,
                    y,
                    image.width_px,
                    image.height_px,
                    screen,
                    None,
                );
            } else {
                set_fg(display, gc, pal(COLOR_IMAGE_BORDER));
                XDrawRectangle(
                    display,
                    window,
                    gc,
                    x,
                    y,
                    image.width_px,
                    image.height_px,
                );
            }
        }
    }

    for child in &node.children {
        draw_box_x11(display, window, gc, font, child, scroll_y, window_height, images, screen);
    }
}

unsafe fn draw_text_with_links(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    font: *mut XftFont,
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
                set_fg(display, gc, pal(COLOR_BODY_TEXT));
                draw_string(display, window, gc, cx, y, segment);
                cx += text_width(font, segment);
            }
            let segment = &text[span.start..span.end];
            set_fg(display, gc, pal(COLOR_LINK));
            draw_string(display, window, gc, cx, y, segment);
            let pw = text_width(font, segment);
            XDrawLine(display, window, gc, cx, y + 2, cx + pw, y + 2);
            cx += pw;
            prev = span.end;
        }
    }
    if prev < text.len() {
        let segment = &text[prev..];
        set_fg(display, gc, pal(COLOR_BODY_TEXT));
        draw_string(display, window, gc, cx, y, segment);
    }
}

unsafe fn draw_scaled_image(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    original: &DecodedImage,
    x: c_int,
    y: c_int,
    out_width: u32,
    out_height: u32,
    screen: c_int,
    background: Option<c_ulong>,
) {
    if out_width == 0 || out_height == 0 {
        return;
    }

    let visual = XDefaultVisual(display, screen);
    let depth = XDefaultDepth(display, screen) as c_uint;
    let mut buffer = vec![0u8; (out_width as usize) * (out_height as usize) * 4];
    let image = XCreateImage(
        display,
        visual,
        depth,
        ZPIXMAP,
        0,
        buffer.as_mut_ptr() as *mut c_char,
        out_width,
        out_height,
        32,
        0,
    );
    if image.is_null() {
        return;
    }

    let data = (*image).data as *mut u8;
    let bpp = (*image).bits_per_pixel;
    let bytes_per_pixel = if bpp >= 32 {
        4
    } else if bpp >= 24 {
        3
    } else if bpp >= 16 {
        2
    } else {
        1
    };
    let stride = if (*image).bytes_per_line > 0 {
        (*image).bytes_per_line as usize
    } else {
        out_width as usize * bytes_per_pixel
    };
    let rshift = mask_shift((*image).red_mask);
    let gshift = mask_shift((*image).green_mask);
    let bshift = mask_shift((*image).blue_mask);
    let big_endian = (*image).byte_order == MSB_FIRST;

    let src = &original.rgba;
    let src_width = original.width as usize;
    let src_height = original.height as usize;
    let out_w = out_width as usize;
    let out_h = out_height as usize;
    let bg = background.map(|color| {
        (
            ((color >> 16) & 0xFF) as u32,
            ((color >> 8) & 0xFF) as u32,
            (color & 0xFF) as u32,
        )
    });

    for row in 0..out_h {
        let src_row = (row * src_height) / out_h;
        for col in 0..out_w {
            let src_col = (col * src_width) / out_w;
            let src_index = (src_row * src_width + src_col) * 4;
            let r0 = src[src_index] as u32;
            let g0 = src[src_index + 1] as u32;
            let b0 = src[src_index + 2] as u32;
            let (r, g, b) = if let Some((br, bg, bb)) = bg {
                let alpha = src[src_index + 3] as u32;
                let inv = 255 - alpha;
                (
                    (r0 * alpha + br * inv) / 255,
                    (g0 * alpha + bg * inv) / 255,
                    (b0 * alpha + bb * inv) / 255,
                )
            } else {
                (r0, g0, b0)
            };
            let value = (r << rshift) | (g << gshift) | (b << bshift);
            let offset = row * stride + col * bytes_per_pixel;
            for k in 0..bytes_per_pixel {
                let shift = if big_endian {
                    8 * (bytes_per_pixel - 1 - k)
                } else {
                    8 * k
                };
                *data.add(offset + k) = ((value >> shift) & 0xFF) as u8;
            }
        }
    }

    XPutImage(display, window, gc, image, 0, 0, x, y, out_width, out_height);
    XDestroyImage(image);
    std::mem::forget(buffer);
}

fn mask_shift(mask: c_ulong) -> u32 {
    if mask == 0 {
        0
    } else {
        mask.trailing_zeros()
    }
}

unsafe fn draw_scrollbar(display: *mut Display, window: c_ulong, gc: *mut GC, app: &BrowserApp) {
    let track_x = (app.window_width - 18) as c_int;
    let track_y = MARGIN_Y as c_int;
    let track_height = app.viewport_height() as c_int;
    let max_scroll = app.max_scroll_y();

    set_fg(display, gc, pal(COLOR_SCROLLBAR_TRACK));
    XFillRectangle(
        display,
        window,
        gc,
        track_x,
        track_y,
        8,
        track_height as c_uint,
    );

    if max_scroll == 0 {
        return;
    }

    let content_height = app.layout.rect.height as c_int * LINE_HEIGHT as c_int;
    let thumb_height = ((track_height * track_height) / content_height).clamp(32, track_height);
    let thumb_y = track_y + ((track_height - thumb_height) * app.scroll_y / max_scroll);

    set_fg(display, gc, pal(COLOR_SCROLLBAR_THUMB));
    XFillRectangle(
        display,
        window,
        gc,
        track_x,
        thumb_y,
        8,
        thumb_height as c_uint,
    );
}

unsafe fn draw_status_bar(display: *mut Display, window: c_ulong, gc: *mut GC, app: &BrowserApp) {
    let y = (app.window_height - STATUS_BAR_HEIGHT) as c_int;
    set_fg(display, gc, pal(COLOR_STATUS_BAR));
    XFillRectangle(
        display,
        window,
        gc,
        0,
        y,
        app.window_width as c_uint,
        STATUS_BAR_HEIGHT as c_uint,
    );
    set_fg(display, gc, pal(COLOR_MUTED_TEXT));
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
    draw_string(display, window, gc, 22, y + 20, &message);
}

unsafe fn visible_address(
    app: &BrowserApp,
    font: *mut XftFont,
    max_width: c_int,
) -> (String, usize) {
    let text = &app.address_text;
    if max_width <= 0 {
        return (String::new(), 0);
    }
    if text.is_empty() || text_width(font, text) <= max_width {
        return (text.clone(), 0);
    }
    let cursor = app.address_cursor.min(text.len());
    let mut start = cursor;
    loop {
        let previous = prev_char_index(text, start);
        if previous == 0 {
            break;
        }
        if text_width(font, &text[previous..cursor]) > max_width {
            break;
        }
        start = previous;
    }
    let mut end = cursor;
    loop {
        let next = next_char_index(text, end);
        if next > text.len() {
            break;
        }
        if text_width(font, &text[start..next]) > max_width {
            break;
        }
        end = next;
    }
    (text[start..end].to_string(), start)
}

unsafe fn cursor_for_click(app: &BrowserApp, font: *mut XftFont, click_x: c_int) -> usize {
    let address_width = address_bar_width(app.window_width) as c_int;
    let avail = address_width - 28;
    let (shown, start) = visible_address(app, font, avail);
    let rel_x = click_x - ADDRESS_TEXT_X;
    let mut best = 0usize;
    let mut best_diff = c_int::MAX;
    for (index, _) in shown.char_indices() {
        let width = text_width(font, &shown[..index]);
        let diff = (width - rel_x).abs();
        if diff < best_diff {
            best_diff = diff;
            best = index;
        }
    }
    let end_width = text_width(font, &shown);
    if (end_width - rel_x).abs() < best_diff {
        best = shown.len();
    }
    start + best
}

unsafe fn text_width(_font: *mut XftFont, text: &str) -> c_int {
    if text.is_empty() {
        return 0;
    }
    if FT_FACE.is_null() {
        return text.chars().count() as c_int * CHAR_WIDTH as c_int;
    }
    let mut total: i64 = 0;
    for ch in text.chars() {
        let gid = FT_Get_Char_Index(FT_FACE, ch as c_ulong);
        let mut advance: i64 = 0;
        FT_Get_Advance(FT_FACE, gid, 0, &mut advance);
        total += advance;
    }
    (total as f64 / 65536.0).round() as c_int
}

unsafe fn ft_init_for_font(font: *mut XftFont) -> bool {
    if font.is_null() || (*font).pattern.is_null() {
        return false;
    }
    let mut file_ptr: *const c_char = ptr::null();
    if FcPatternGetString(
        (*font).pattern,
        b"file\0".as_ptr(),
        0,
        &mut file_ptr,
    ) != 0 || file_ptr.is_null() {
        return false;
    }
    let mut px: f64 = 13.0;
    FcPatternGetDouble((*font).pattern, b"pixelsize\0".as_ptr(), 0, &mut px);

    if FT_LIB.is_null() && FT_Init_FreeType(ptr::addr_of_mut!(FT_LIB)) != 0 {
        return false;
    }
    let mut face: *mut FT_FaceRec = ptr::null_mut();
    if FT_New_Face(FT_LIB, file_ptr, 0, &mut face) != 0 || face.is_null() {
        return false;
    }
    if FT_Set_Char_Size(face, 0, (px * 64.0) as i64, 72, 72) != 0 {
        return false;
    }
    FT_FACE = face;
    FT_PIXELSIZE = px;
    true
}

unsafe fn show_about_window(
    display: *mut Display,
    root: c_ulong,
    black: c_ulong,
    white: c_ulong,
) {
    let window = XCreateSimpleWindow(display, root, 160, 160, 520, 240, 1, black, white);
    let title = CString::new("About Ghostab").unwrap();
    XStoreName(display, window, title.as_ptr());
    XSelectInput(
        display,
        window,
        (EXPOSURE_MASK | KEY_PRESS_MASK | BUTTON_PRESS_MASK | STRUCTURE_NOTIFY_MASK) as c_long,
    );
    XMapWindow(display, window);
    set_window_icon(display, window);

    let wm_delete = {
        let atom_name = CString::new("WM_DELETE_WINDOW").unwrap();
        XInternAtom(display, atom_name.as_ptr(), 0)
    };
    XSetWMProtocols(display, window, &wm_delete as *const c_ulong as *mut c_ulong, 1);

    let gc = XCreateGC(display, window, 0, ptr::null_mut());
    let visual = XDefaultVisual(display, XDefaultScreen(display));
    let colormap = XDefaultColormap(display, XDefaultScreen(display));
    let about_draw = XftDrawCreate(display, window, visual, colormap);
    let saved_draw = XFT_DRAW;
    XFT_DRAW = about_draw;

    let mut tab: u8 = 0;
    loop {
        let mut event = std::mem::MaybeUninit::<XEvent>::zeroed();
        XNextEvent(display, event.as_mut_ptr());
        let event = event.assume_init();

        match event.get_type() {
            EXPOSE => draw_about_content(display, window, gc, tab),
            BUTTON_PRESS => {
                if let Some(t) = about_tab_at(event.xbutton.x, event.xbutton.y) {
                    if t != tab {
                        tab = t;
                        draw_about_content(display, window, gc, tab);
                        XFlush(display);
                    }
                }
            }
            CLIENT_MESSAGE => {
                let data = event.xclient.data.longs[0] as c_ulong;
                if data == wm_delete {
                    break;
                }
            }
            _ => {}
        }
    }

    XFT_DRAW = saved_draw;
    if !about_draw.is_null() {
        XftDrawDestroy(about_draw);
    }
    XFreeGC(display, gc);
    XDestroyWindow(display, window);
}

unsafe fn draw_about_content(display: *mut Display, window: c_ulong, gc: *mut GC, tab: u8) {
    set_fg(display, gc, pal(COLOR_PAGE));
    XFillRectangle(display, window, gc, 0, 0, 520, 240);
    draw_about_tabs(display, window, gc, tab);
    set_fg(display, gc, pal(COLOR_BODY_TEXT));
    if tab == 0 {
        draw_string(display, window, gc, 28, 66, "Ghostab");
        draw_string(display, window, gc, 28, 94, "Engine: Ghost Engine Alpha 1.1.0");
        draw_string(
            display,
            window,
            gc,
            28,
            122,
            "A tiny experimental browser engine written in Rust.",
        );
        draw_string(
            display,
            window,
            gc,
            28,
            150,
            "Networking: HTTP/HTTPS loading through curl.",
        );
        draw_string(
            display,
            window,
            gc,
            28,
            178,
            "Rendering: simplified HTML text layout in an X11 window.",
        );
        draw_string(
            display,
            window,
            gc,
            28,
            206,
            "Privacy: clipboard stays inside the app.",
        );
    } else {
        draw_string(display, window, gc, 28, 66, "Credits");
        draw_string(display, window, gc, 28, 94, "Made by AramCZ");
        draw_string(
            display,
            window,
            gc,
            28,
            122,
            "Tools: Rust, C (X11/Xlib, Xft, FreeType, fontconfig),",
        );
        draw_string(
            display,
            window,
            gc,
            28,
            150,
            "curl, the image crate, dpkg, Bash.",
        );
        draw_string(
            display,
            window,
            gc,
            28,
            178,
            "Built with some assistance from Opencode.",
        );
    }
}

unsafe fn draw_about_tabs(display: *mut Display, window: c_ulong, gc: *mut GC, active: u8) {
    for (i, label) in ["About", "Credits"].iter().enumerate() {
        let i = i as u8;
        let x = 12 + (i as c_int) * 104;
        let selected = i == active;
        set_fg(
            display,
            gc,
            if selected { pal(COLOR_SURFACE) } else { pal(COLOR_BUTTON_BG) },
        );
        XFillRectangle(display, window, gc, x, 4, 96, 28);
        set_fg(display, gc, pal(COLOR_PAGE_BORDER));
        XDrawRectangle(display, window, gc, x, 4, 96, 28);
        set_fg(
            display,
            gc,
            if selected { pal(COLOR_BODY_TEXT) } else { pal(COLOR_MUTED_TEXT) },
        );
        draw_string(display, window, gc, x + 14, centered_baseline(XFT_FONT, 4, 28), label);
    }
    set_fg(display, gc, pal(COLOR_PAGE_BORDER));
    XDrawLine(display, window, gc, 0, 32, 520, 32);
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

const SETTINGS_W: c_int = 460;
const SETTINGS_H: c_int = 380;
const SETTINGS_OK_X: c_int = 244;
const SETTINGS_OK_Y: c_int = 316;
const SETTINGS_OK_W: c_int = 92;
const SETTINGS_OK_H: c_int = 34;
const SETTINGS_CANCEL_X: c_int = 344;
const SETTINGS_URL_X: c_int = 28;
const SETTINGS_URL_Y: c_int = 228;
const SETTINGS_URL_W: c_int = 404;
const SETTINGS_URL_H: c_int = 30;

fn edit_insert(text: &mut String, cursor: &mut usize, insertion: &str) {
    text.insert_str(*cursor, insertion);
    *cursor += insertion.len();
}

fn edit_backspace(text: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    let prev = prev_char_index(text, *cursor);
    text.replace_range(prev..*cursor, "");
    *cursor = prev;
}

fn edit_delete(text: &mut String, cursor: &mut usize) {
    if *cursor >= text.len() {
        return;
    }
    let next = next_char_index(text, *cursor);
    text.replace_range(*cursor..next, "");
}

fn edit_move(text: &str, cursor: &mut usize, right: bool) {
    *cursor = if right {
        next_char_index(text, *cursor)
    } else {
        prev_char_index(text, *cursor)
    };
}

unsafe fn show_settings_window(
    display: *mut Display,
    root: c_ulong,
    black: c_ulong,
    white: c_ulong,
    settings: &mut Settings,
) {
    let window = XCreateSimpleWindow(
        display,
        root,
        220,
        180,
        SETTINGS_W as c_uint,
        SETTINGS_H as c_uint,
        1,
        black,
        white,
    );
    eprintln!("ghostab-log: settings window created");
    let title = CString::new("Settings").unwrap();
    XStoreName(display, window, title.as_ptr());
    XSelectInput(
        display,
        window,
        (EXPOSURE_MASK | KEY_PRESS_MASK | BUTTON_PRESS_MASK | STRUCTURE_NOTIFY_MASK) as c_long,
    );
    XMapWindow(display, window);
    set_window_icon(display, window);
    eprintln!("ghostab-log: settings icon set");

    let wm_delete = {
        let atom_name = CString::new("WM_DELETE_WINDOW").unwrap();
        XInternAtom(display, atom_name.as_ptr(), 0)
    };
    XSetWMProtocols(display, window, &wm_delete as *const c_ulong as *mut c_ulong, 1);

    let gc = XCreateGC(display, window, 0, ptr::null_mut());
    let visual = XDefaultVisual(display, XDefaultScreen(display));
    let colormap = XDefaultColormap(display, XDefaultScreen(display));
    let settings_draw = XftDrawCreate(display, window, visual, colormap);
    let saved_draw = XFT_DRAW;
    XFT_DRAW = settings_draw;

    let mut working = settings.clone();
    apply_settings(&working);
    let mut url_focused = false;
    let mut url_cursor = working.search_url.len();
    eprintln!("ghostab-log: settings loop begin light={}", working.light_mode);

    loop {
        let mut event = std::mem::MaybeUninit::<XEvent>::zeroed();
        XNextEvent(display, event.as_mut_ptr());
        let event = event.assume_init();

        match event.get_type() {
            EXPOSE => {
                eprintln!("ghostab-log: settings expose");
                draw_settings_content(display, window, gc, &working, url_focused, url_cursor);
                XFlush(display);
            }
            BUTTON_PRESS => {
                eprintln!("ghostab-log: settings button x={} y={}", event.xbutton.x, event.xbutton.y);
                let x = event.xbutton.x;
                let y = event.xbutton.y;
                if settings_light_row_at(x, y) {
                    working.light_mode = !working.light_mode;
                    apply_settings(&working);
                    draw_settings_content(display, window, gc, &working, url_focused, url_cursor);
                    XFlush(display);
                } else if let Some(engine) = settings_engine_at(x, y) {
                    working.search_engine = SearchEngine::all()[engine];
                    url_cursor = working.search_url.len();
                    draw_settings_content(display, window, gc, &working, url_focused, url_cursor);
                    XFlush(display);
                } else if settings_url_at(x, y) {
                    url_focused = true;
                    url_cursor = working.search_url.len();
                    draw_settings_content(display, window, gc, &working, url_focused, url_cursor);
                    XFlush(display);
                } else if settings_ok_at(x, y) {
                    *settings = working;
                    apply_settings(settings);
                    save_settings(settings);
                    break;
                } else if settings_cancel_at(x, y) {
                    apply_settings(settings);
                    break;
                }
            }
            KEY_PRESS => {
                let (input, _mods, _keysym) = read_key(event.xkey);
                let mut changed = false;
                if url_focused {
                    match input {
                        KeyInput::Escape => url_focused = false,
                        KeyInput::Enter => {
                            *settings = working;
                            apply_settings(settings);
                            save_settings(settings);
                            break;
                        }
                        KeyInput::Backspace => {
                            edit_backspace(&mut working.search_url, &mut url_cursor);
                            changed = true;
                        }
                        KeyInput::Delete => {
                            edit_delete(&mut working.search_url, &mut url_cursor);
                            changed = true;
                        }
                        KeyInput::Left => edit_move(&working.search_url, &mut url_cursor, false),
                        KeyInput::Right => edit_move(&working.search_url, &mut url_cursor, true),
                        KeyInput::Home => url_cursor = 0,
                        KeyInput::End => url_cursor = working.search_url.len(),
                        KeyInput::Text(text) => {
                            edit_insert(&mut working.search_url, &mut url_cursor, &text);
                            changed = true;
                        }
                        _ => {}
                    }
                } else {
                    match input {
                        KeyInput::Escape => {
                            apply_settings(settings);
                            break;
                        }
                        _ => {}
                    }
                }
                if changed {
                    draw_settings_content(display, window, gc, &working, url_focused, url_cursor);
                    XFlush(display);
                }
            }
            CLIENT_MESSAGE => {
                let data = event.xclient.data.longs[0] as c_ulong;
                if data == wm_delete {
                    apply_settings(settings);
                    break;
                }
            }
            _ => {}
        }
    }

    XFT_DRAW = saved_draw;
    if !settings_draw.is_null() {
        XftDrawDestroy(settings_draw);
    }
    XFreeGC(display, gc);
    XDestroyWindow(display, window);
}

unsafe fn draw_settings_content(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    settings: &Settings,
    url_focused: bool,
    url_cursor: usize,
) {
    set_fg(display, gc, pal(COLOR_PAGE));
    XFillRectangle(display, window, gc, 0, 0, SETTINGS_W as c_uint, SETTINGS_H as c_uint);
    set_fg(display, gc, pal(COLOR_BODY_TEXT));
    draw_string(display, window, gc, 24, 46, "Settings");
    set_fg(display, gc, pal(COLOR_PAGE_BORDER));
    XDrawLine(display, window, gc, 16, 60, SETTINGS_W - 16, 60);

    set_fg(display, gc, pal(COLOR_MUTED_TEXT));
    draw_string(display, window, gc, 24, 84, "Appearance");
    draw_checkbox(display, window, gc, 28, 96, settings.light_mode);
    set_fg(display, gc, pal(COLOR_BODY_TEXT));
    draw_string(display, window, gc, 54, 110, "Light Mode");

    set_fg(display, gc, pal(COLOR_MUTED_TEXT));
    draw_string(display, window, gc, 24, 140, "Search Engine");
    for (i, engine) in SearchEngine::all().iter().enumerate() {
        let y = 162 + i as c_int * 28;
        draw_radio(display, window, gc, 28, y, *engine == settings.search_engine);
        set_fg(display, gc, pal(COLOR_BODY_TEXT));
        draw_string(display, window, gc, 54, y + 14, engine.label());
    }

    if settings.search_engine == SearchEngine::Custom {
        set_fg(display, gc, pal(COLOR_MUTED_TEXT));
        draw_string(display, window, gc, 28, 222, "Search URL (use %s for the query)");
        set_fg(
            display,
            gc,
            if url_focused {
                pal(COLOR_ADDRESS_FOCUS)
            } else {
                pal(COLOR_ADDRESS_BORDER)
            },
        );
        XFillRectangle(
            display,
            window,
            gc,
            SETTINGS_URL_X,
            SETTINGS_URL_Y,
            SETTINGS_URL_W as c_uint,
            SETTINGS_URL_H as c_uint,
        );
        set_fg(display, gc, pal(COLOR_ADDRESS_TEXT));
        let text_x = SETTINGS_URL_X + 8;
        draw_string(display, window, gc, text_x, centered_baseline(XFT_FONT, SETTINGS_URL_Y, SETTINGS_URL_H), &settings.search_url);
        if url_focused {
            let prefix = &settings.search_url[..url_cursor.min(settings.search_url.len())];
            let cx = text_x + text_width(XFT_FONT, prefix);
            set_fg(display, gc, pal(COLOR_CARET));
            XDrawLine(
                display,
                window,
                gc,
                cx,
                SETTINGS_URL_Y + 6,
                cx,
                SETTINGS_URL_Y + SETTINGS_URL_H - 6,
            );
        }
    }

    set_fg(display, gc, pal(COLOR_PAGE_BORDER));
    XDrawLine(display, window, gc, 16, SETTINGS_H - 52, SETTINGS_W - 16, SETTINGS_H - 52);
    draw_settings_button(
        display,
        window,
        gc,
        SETTINGS_OK_X,
        SETTINGS_OK_Y,
        SETTINGS_OK_W,
        SETTINGS_OK_H,
        "OK",
        true,
    );
    draw_settings_button(
        display,
        window,
        gc,
        SETTINGS_CANCEL_X,
        SETTINGS_OK_Y,
        SETTINGS_OK_W,
        SETTINGS_OK_H,
        "Cancel",
        false,
    );
}

unsafe fn draw_checkbox(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    x: c_int,
    y: c_int,
    checked: bool,
) {
    set_fg(
        display,
        gc,
        if checked {
            pal(COLOR_GO_BG)
        } else {
            pal(COLOR_BUTTON_BORDER)
        },
    );
    XFillRectangle(display, window, gc, x, y, 18, 18);
    set_fg(display, gc, pal(COLOR_PAGE_BORDER));
    XDrawRectangle(display, window, gc, x, y, 18, 18);
    if checked {
        set_fg(display, gc, pal(COLOR_GO_TEXT));
        XDrawLine(display, window, gc, x + 3, y + 9, x + 8, y + 14);
        XDrawLine(display, window, gc, x + 8, y + 14, x + 15, y + 4);
    }
}

unsafe fn draw_radio(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    x: c_int,
    y: c_int,
    selected: bool,
) {
    set_fg(display, gc, pal(COLOR_BUTTON_BORDER));
    XDrawArc(display, window, gc, x, y, 18, 18, 0, 360 * 64);
    if selected {
        set_fg(display, gc, pal(COLOR_GO_BG));
        XFillArc(display, window, gc, x + 4, y + 4, 10, 10, 0, 360 * 64);
    }}

unsafe fn draw_settings_button(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    label: &str,
    primary: bool,
) {
    set_fg(
        display,
        gc,
        if primary {
            pal(COLOR_GO_BG)
        } else {
            pal(COLOR_BUTTON_BG)
        },
    );
    XFillRectangle(display, window, gc, x, y, width as c_uint, height as c_uint);
    set_fg(display, gc, pal(COLOR_BUTTON_BORDER));
    XDrawRectangle(display, window, gc, x, y, width as c_uint, height as c_uint);
    set_fg(
        display,
        gc,
        if primary {
            pal(COLOR_GO_TEXT)
        } else {
            pal(COLOR_BUTTON_TEXT)
        },
    );
    let label_width = text_width(XFT_FONT, label);
    draw_string(
        display,
        window,
        gc,
        x + (width - label_width) / 2,
        centered_baseline(XFT_FONT, y, height),
        label,
    );
}

fn settings_light_row_at(x: c_int, y: c_int) -> bool {
    (96..=124).contains(&y) && (24..=220).contains(&x)
}

fn settings_engine_at(x: c_int, y: c_int) -> Option<usize> {
    for (i, _) in SearchEngine::all().iter().enumerate() {
        let ey = 162 + i as c_int * 28;
        if (ey..ey + 20).contains(&y) && (24..=200).contains(&x) {
            return Some(i);
        }
    }
    None
}

fn settings_url_at(x: c_int, y: c_int) -> bool {
    (SETTINGS_URL_X..SETTINGS_URL_X + SETTINGS_URL_W).contains(&x)
        && (SETTINGS_URL_Y..SETTINGS_URL_Y + SETTINGS_URL_H).contains(&y)
}

fn settings_ok_at(x: c_int, y: c_int) -> bool {
    (SETTINGS_OK_X..SETTINGS_OK_X + SETTINGS_OK_W).contains(&x)
        && (SETTINGS_OK_Y..SETTINGS_OK_Y + SETTINGS_OK_H).contains(&y)
}

fn settings_cancel_at(x: c_int, y: c_int) -> bool {
    (SETTINGS_CANCEL_X..SETTINGS_CANCEL_X + SETTINGS_OK_W).contains(&x)
        && (SETTINGS_OK_Y..SETTINGS_OK_Y + SETTINGS_OK_H).contains(&y)
}

const SHIELD_W: c_int = 560;
const SHIELD_H: c_int = 600;
const SHIELD_IMG_X: c_int = 88;
const SHIELD_IMG_Y: c_int = 84;
const SHIELD_IMG_SIZE: u32 = 384;
const SHIELD_TEXT_Y: c_int = 500;
const SHIELD_LINE_STEP: c_int = 26;

fn decode_image_bytes(bytes: &[u8]) -> Option<DecodedImage> {
    let image = image::load_from_memory(bytes).ok()?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(DecodedImage {
        width,
        height,
        rgba: rgba.into_raw(),
    })
}

fn load_asset_bytes(disk_path: &str, embedded: &[u8]) -> Option<DecodedImage> {
    fs::read(disk_path)
        .ok()
        .and_then(|bytes| decode_image_bytes(&bytes))
        .or_else(|| decode_image_bytes(embedded))
}

fn decode_ghostab_icon(bytes: &[u8]) -> Option<DecodedImage> {
    let image = image::load_from_memory(bytes).ok()?;
    let mut rgba = image.to_rgba8();
    for pixel in rgba.as_mut().chunks_exact_mut(4) {
        if pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0 {
            pixel[3] = 0;
        }
    }
    let (width, height) = rgba.dimensions();
    Some(DecodedImage {
        width,
        height,
        rgba: rgba.into_raw(),
    })
}

fn load_ghostab_image() -> Option<DecodedImage> {
    let bytes = fs::read("Ghostab.png").unwrap_or_else(|_| ICON_PNG.to_vec());
    decode_ghostab_icon(&bytes)
}

fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        if !current.is_empty() && current_len + 1 + word_len > max_chars {
            lines.push(std::mem::take(&mut current));
            current_len = 0;
        }
        if !current.is_empty() {
            current.push(' ');
            current_len += 1;
        }
        current.push_str(word);
        current_len += word_len;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

unsafe fn show_shield_window(
    display: *mut Display,
    root: c_ulong,
    black: c_ulong,
    white: c_ulong,
    state: ConnectionState,
) {
    let (disk_path, asset_bytes, message) = match state {
        ConnectionState::Protected => (
            "assets/Protected.png",
            PROTECTED_PNG,
            "This website encrypts what you send.",
        ),
        ConnectionState::Local => (
            "assets/Localfile.png",
            LOCALFILE_PNG,
            "This website is stored on your computer.",
        ),
        ConnectionState::BuiltIn => (
            "Ghostab.png",
            ICON_PNG,
            "This website is integrated into the browser.",
        ),
        ConnectionState::Unprotected => (
            "assets/Unprotected.png",
            UNPROTECTED_PNG,
            "This website does not encrypt what you send. Please do not send passwords, personal info, etc.",
        ),
    };
    let asset = if state == ConnectionState::BuiltIn {
        load_ghostab_image()
    } else {
        load_asset_bytes(disk_path, asset_bytes)
    };

    let window = XCreateSimpleWindow(
        display,
        root,
        200,
        60,
        SHIELD_W as c_uint,
        SHIELD_H as c_uint,
        1,
        black,
        white,
    );
    let title = CString::new("Connection Security").unwrap();
    XStoreName(display, window, title.as_ptr());
    XSelectInput(
        display,
        window,
        (EXPOSURE_MASK | STRUCTURE_NOTIFY_MASK) as c_long,
    );
    XMapWindow(display, window);
    set_window_icon(display, window);

    let wm_delete = {
        let atom_name = CString::new("WM_DELETE_WINDOW").unwrap();
        XInternAtom(display, atom_name.as_ptr(), 0)
    };
    XSetWMProtocols(display, window, &wm_delete as *const c_ulong as *mut c_ulong, 1);

    let gc = XCreateGC(display, window, 0, ptr::null_mut());
    let visual = XDefaultVisual(display, XDefaultScreen(display));
    let colormap = XDefaultColormap(display, XDefaultScreen(display));
    let shield_draw = XftDrawCreate(display, window, visual, colormap);
    let saved_draw = XFT_DRAW;
    XFT_DRAW = shield_draw;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    'shield: loop {
        while XPending(display) > 0 {
            let mut event = std::mem::MaybeUninit::<XEvent>::zeroed();
            XNextEvent(display, event.as_mut_ptr());
            let event = event.assume_init();

            match event.get_type() {
                EXPOSE => {
                    draw_shield_content(display, window, gc, state, message, asset.as_ref());
                    XFlush(display);
                }
                CLIENT_MESSAGE => {
                    let data = event.xclient.data.longs[0] as c_ulong;
                    if data == wm_delete {
                        break 'shield;
                    }
                }
                _ => {}
            }
        }
        XFlush(display);
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    XFT_DRAW = saved_draw;
    if !shield_draw.is_null() {
        XftDrawDestroy(shield_draw);
    }
    XFreeGC(display, gc);
    XDestroyWindow(display, window);
}

unsafe fn draw_shield_content(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    state: ConnectionState,
    message: &str,
    asset: Option<&DecodedImage>,
) {
    set_fg(display, gc, pal(COLOR_PAGE));
    XFillRectangle(display, window, gc, 0, 0, SHIELD_W as c_uint, SHIELD_H as c_uint);
    let headline = match state {
        ConnectionState::Protected => "Protected connection",
        ConnectionState::Local => "Local content",
        ConnectionState::BuiltIn => "Built into Ghostab",
        ConnectionState::Unprotected => "Unprotected connection",
    };
    set_fg(display, gc, pal(COLOR_BODY_TEXT));
    draw_string(display, window, gc, 28, 28, "Connection Security");
    set_fg(display, gc, pal(COLOR_GO_BG));
    draw_string(display, window, gc, 28, 54, headline);
    set_fg(display, gc, pal(COLOR_PAGE_BORDER));
    XDrawLine(display, window, gc, 16, 68, SHIELD_W - 16, 68);

    if let Some(image) = asset {
        let screen = XDefaultScreen(display);
        draw_scaled_image(
            display,
            window,
            gc,
            image,
            SHIELD_IMG_X,
            SHIELD_IMG_Y,
            SHIELD_IMG_SIZE,
            SHIELD_IMG_SIZE,
            screen,
            Some(pal(COLOR_PAGE)),
        );
    }

    set_fg(display, gc, pal(COLOR_BODY_TEXT));
    for (i, line) in wrap_text(message, 70).iter().enumerate() {
        draw_string(
            display,
            window,
            gc,
            28,
            SHIELD_TEXT_Y + i as c_int * SHIELD_LINE_STEP,
            line,
        );
    }
}

unsafe fn centered_baseline(font: *mut XftFont, box_y: c_int, box_height: c_int) -> c_int {
    if font.is_null() {
        return box_y + box_height / 2 + 4;
    }
    let ascent = (*font).ascent;
    let descent = (*font).descent;
    box_y + (box_height - (ascent + descent)) / 2 + ascent
}

unsafe fn draw_string(
    display: *mut Display,
    window: c_ulong,
    gc: *mut GC,
    x: c_int,
    y: c_int,
    text: &str,
) {
    let font = XFT_FONT;
    let draw = XFT_DRAW;
    if draw.is_null() || font.is_null() {
        if let Ok(c_text) = CString::new(text) {
            XDrawString(
                display,
                window,
                gc,
                x,
                y,
                c_text.as_ptr(),
                text.len() as c_int,
            );
        }
        return;
    }
    let fg = CURRENT_FG;
    let color = XftColor {
        pixel: fg,
        color: XRenderColor {
            red: (((fg >> 16) & 0xFF) as u16) << 8,
            green: (((fg >> 8) & 0xFF) as u16) << 8,
            blue: ((fg & 0xFF) as u16) << 8,
            alpha: 0xFFFF,
        },
    };
    XftDrawStringUtf8(draw, &color, font, x, y, text.as_ptr(), text.len() as c_int);
}

unsafe fn set_fg(display: *mut Display, gc: *mut GC, color: c_ulong) {
    CURRENT_FG = color;
    x_set_foreground(display, gc, color);
}

fn shorten(text: &str, max_chars: usize) -> String {
    let mut shortened: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        shortened.push_str("...");
    }
    shortened
}

unsafe fn load_system_font(display: *mut Display) -> *mut XftFont {
    let screen = XDefaultScreen(display);
    for name in [
        "sans-serif:size=13",
        "dejavu-sans:size=13",
        "liberation-sans:size=13",
        "cantarell:size=13",
        "ubuntu:size=13",
    ] {
        let font_name = CString::new(name).unwrap();
        let font = XftFontOpenName(display, screen, font_name.as_ptr());
        if !font.is_null() {
            return font;
        }
    }

    ptr::null_mut()
}

unsafe fn claim_clipboard(display: *mut Display, window: c_ulong, app: &mut BrowserApp) {
    app.owns_clipboard = true;
    XSetSelectionOwner(display, window, XA_CLIPBOARD, CURRENT_TIME);
    XFlush(display);
}

unsafe fn paste_clipboard(display: *mut Display, window: c_ulong, app: &mut BrowserApp) {
    if app.owns_clipboard && !app.clipboard_text.is_empty() {
        let text = app.clipboard_text.clone();
        app.insert_text(&text);
        return;
    }
    let property = intern_atom(display, "GHOSTAB_CLIPBOARD");
    app.pending_paste = true;
    app.paste_retry = false;
    XConvertSelection(display, XA_CLIPBOARD, XA_UTF8_STRING, property, window, CURRENT_TIME);
    XFlush(display);
}

unsafe fn handle_selection_notify(
    display: *mut Display,
    window: c_ulong,
    event: XSelectionNotifyEvent,
    app: &mut BrowserApp,
) {
    if !app.pending_paste || event.selection != XA_CLIPBOARD {
        return;
    }
    if event.property == 0 {
        if !app.paste_retry {
            app.paste_retry = true;
            let property = intern_atom(display, "GHOSTAB_CLIPBOARD");
            XConvertSelection(display, XA_CLIPBOARD, XA_STRING, property, window, CURRENT_TIME);
            XFlush(display);
            return;
        }
        app.pending_paste = false;
        return;
    }
    app.pending_paste = false;
    app.paste_retry = false;

    let mut actual_type: Atom = 0;
    let mut actual_format: c_int = 0;
    let mut nitems: c_ulong = 0;
    let mut bytes_after: c_ulong = 0;
    let mut data: *mut u8 = ptr::null_mut();

    XGetWindowProperty(
        display,
        window,
        event.property,
        0,
        1 << 20,
        1,
        0,
        &mut actual_type,
        &mut actual_format,
        &mut nitems,
        &mut bytes_after,
        &mut data,
    );

    if !data.is_null() && nitems > 0 {
        let bytes_per_item = match actual_format {
            16 => 2,
            32 => 4,
            _ => 1,
        };
        let total = (nitems as usize).saturating_mul(bytes_per_item);
        let bytes = std::slice::from_raw_parts(data, total);
        let text = String::from_utf8_lossy(bytes).into_owned();
        app.insert_text(&text);
        XFree(data as *mut c_void);
    }
}

unsafe fn serve_selection_request(
    display: *mut Display,
    event: XSelectionRequestEvent,
    clipboard: &str,
) {
    if event.selection != XA_CLIPBOARD && event.selection != XA_PRIMARY {
        return;
    }

    let mut reply = XSelectionNotifyEvent {
        type_: SELECTION_NOTIFY,
        serial: event.serial,
        send_event: 1,
        display,
        requestor: event.requestor,
        selection: event.selection,
        target: event.target,
        property: 0,
        time: event.time,
    };

    if event.property != 0 {
        if event.target == XA_TARGETS {
            let targets: [Atom; 2] = [XA_UTF8_STRING, XA_STRING];
            XChangeProperty(
                display,
                event.requestor,
                event.property,
                XA_ATOM,
                32,
                PROP_MODE_REPLACE,
                targets.as_ptr() as *const c_void,
                2,
            );
            reply.property = event.property;
        } else if event.target == XA_UTF8_STRING || event.target == XA_STRING {
            let bytes = clipboard.as_bytes();
            XChangeProperty(
                display,
                event.requestor,
                event.property,
                event.target,
                8,
                PROP_MODE_REPLACE,
                bytes.as_ptr() as *const c_void,
                bytes.len() as c_int,
            );
            reply.property = event.property;
        }
    }

    let mut notify = XEvent { xselection: reply };
    XSendEvent(display, event.requestor, 0, 0, &mut notify);
    XFlush(display);
}

unsafe fn intern_atom(display: *mut Display, name: &str) -> Atom {
    let c_name = CString::new(name).unwrap();
    XInternAtom(display, c_name.as_ptr(), 0)
}

extern "C" fn ignore_x_error(_display: *mut Display, event: *mut XErrorEvent) -> c_int {
    if !event.is_null() {
        let ev = unsafe { &*event };
        eprintln!(
            "ghostab-log: X error: code={} request={} minor={} resource=0x{:x} serial={}",
            ev.error_code, ev.request_code, ev.minor_code, ev.resourceid, ev.serial
        );
    }
    0
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

unsafe fn read_key(mut key_event: XKeyEvent) -> (KeyInput, KeyMods, KeySym) {
    let mut buffer = [0_i8; 32];
    let mut keysym: KeySym = 0;
    let length = XLookupString(
        &mut key_event,
        buffer.as_mut_ptr(),
        buffer.len() as c_int,
        &mut keysym,
        ptr::null_mut(),
    );

    let mods = KeyMods {
        ctrl: key_event.state & CONTROL_MASK != 0,
        shift: key_event.state & SHIFT_MASK != 0,
    };

    let input = match keysym {
        XK_ESCAPE => KeyInput::Escape,
        XK_RETURN | XK_KP_ENTER => KeyInput::Enter,
        XK_BACK_SPACE => KeyInput::Backspace,
        XK_DELETE => KeyInput::Delete,
        XK_PAGE_UP => KeyInput::PageUp,
        XK_PAGE_DOWN => KeyInput::PageDown,
        XK_HOME => KeyInput::Home,
        XK_END => KeyInput::End,
        XK_LEFT => KeyInput::Left,
        XK_RIGHT => KeyInput::Right,
        _ if length > 0 => {
            let bytes: Vec<u8> = buffer[..length as usize].iter().map(|byte| *byte as u8).collect();
            match String::from_utf8(bytes) {
                Ok(text) if text.chars().all(|ch| !ch.is_control()) => KeyInput::Text(text),
                _ => KeyInput::Other,
            }
        }
        _ => KeyInput::Other,
    };

    (input, mods, keysym)
}

#[repr(C)]
struct Display {
    _private: [u8; 0],
}

#[repr(C)]
struct Visual {
    _private: [u8; 0],
}

#[repr(C)]
struct XftFont {
    ascent: c_int,
    descent: c_int,
    height: c_int,
    max_advance_width: c_int,
    charset: *mut c_void,
    pattern: *mut c_void,
    core: c_int,
    xftfont: *mut c_void,
}

#[repr(C)]
struct XftDraw {
    _private: [u8; 0],
}

#[repr(C)]
struct FT_LibraryRec {
    _private: [u8; 0],
}

#[repr(C)]
struct FT_FaceRec {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XRenderColor {
    red: u16,
    green: u16,
    blue: u16,
    alpha: u16,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XftColor {
    pixel: c_ulong,
    color: XRenderColor,
}

#[repr(C)]
struct XImage {
    width: c_int,
    height: c_int,
    xoffset: c_int,
    format: c_int,
    data: *mut c_char,
    byte_order: c_int,
    bitmap_unit: c_int,
    bitmap_bit_order: c_int,
    bitmap_pad: c_int,
    depth: c_int,
    bytes_per_line: c_int,
    bits_per_pixel: c_int,
    red_mask: c_ulong,
    green_mask: c_ulong,
    blue_mask: c_ulong,
    obdata: *mut c_void,
    f: *mut c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XRectangle {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XPoint {
    x: c_short,
    y: c_short,
}

type GC = c_void;
type Atom = c_ulong;
type KeySym = c_ulong;

#[repr(C)]
#[derive(Copy, Clone)]
union ClientMessageData {
    longs: [c_long; 5],
    bytes: [c_char; 20],
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XClientMessageEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    window: c_ulong,
    message_type: Atom,
    format: c_int,
    data: ClientMessageData,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XKeyEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    window: c_ulong,
    root: c_ulong,
    subwindow: c_ulong,
    time: c_ulong,
    x: c_int,
    y: c_int,
    x_root: c_int,
    y_root: c_int,
    state: c_uint,
    keycode: c_uint,
    same_screen: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XButtonEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    window: c_ulong,
    root: c_ulong,
    subwindow: c_ulong,
    time: c_ulong,
    x: c_int,
    y: c_int,
    x_root: c_int,
    y_root: c_int,
    state: c_uint,
    button: c_uint,
    same_screen: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XMotionEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    window: c_ulong,
    root: c_ulong,
    subwindow: c_ulong,
    time: c_ulong,
    x: c_int,
    y: c_int,
    x_root: c_int,
    y_root: c_int,
    state: c_uint,
    is_hint: c_char,
    same_screen: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XConfigureEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    event: c_ulong,
    window: c_ulong,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    border_width: c_int,
    above: c_ulong,
    override_redirect: c_int,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XSelectionClearEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    window: c_ulong,
    selection: Atom,
    time: c_ulong,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XSelectionRequestEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    owner: c_ulong,
    requestor: c_ulong,
    selection: Atom,
    target: Atom,
    property: Atom,
    time: c_ulong,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct XSelectionNotifyEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut Display,
    requestor: c_ulong,
    selection: Atom,
    target: Atom,
    property: Atom,
    time: c_ulong,
}

#[repr(C)]
struct XErrorEvent {
    type_: c_int,
    display: *mut Display,
    resourceid: c_ulong,
    serial: c_ulong,
    error_code: u8,
    request_code: u8,
    minor_code: u8,
}

#[repr(C)]
#[derive(Copy, Clone)]
union XEvent {
    type_: c_int,
    xclient: XClientMessageEvent,
    xkey: XKeyEvent,
    xbutton: XButtonEvent,
    xmotion: XMotionEvent,
    xconfigure: XConfigureEvent,
    xselection: XSelectionNotifyEvent,
    xselectionrequest: XSelectionRequestEvent,
    xselectionclear: XSelectionClearEvent,
    pad: [c_long; 24],
}

impl XEvent {
    fn get_type(&self) -> c_int {
        unsafe { self.type_ }
    }
}

const EXPOSE: c_int = 12;
const KEY_PRESS: c_int = 2;
const BUTTON_PRESS: c_int = 4;
const BUTTON_RELEASE: c_int = 5;
const MOTION_NOTIFY: c_int = 6;
const LEAVE_NOTIFY: c_int = 8;
const CONFIGURE_NOTIFY: c_int = 22;
const SELECTION_CLEAR: c_int = 29;
const SELECTION_REQUEST: c_int = 30;
const SELECTION_NOTIFY: c_int = 31;
const CLIENT_MESSAGE: c_int = 33;

const EXPOSURE_MASK: c_long = 1 << 15;
const KEY_PRESS_MASK: c_long = 1 << 0;
const BUTTON_PRESS_MASK: c_long = 1 << 2;
const BUTTON_RELEASE_MASK: c_long = 1 << 3;
const POINTER_MOTION_MASK: c_long = 1 << 6;
const LEAVE_WINDOW_MASK: c_long = 1 << 9;
const STRUCTURE_NOTIFY_MASK: c_long = 1 << 17;

const CONTROL_MASK: c_uint = 1 << 2;
const SHIFT_MASK: c_uint = 1 << 0;

const CURRENT_TIME: c_ulong = 0;
const PROP_MODE_REPLACE: c_int = 0;
const ZPIXMAP: c_int = 2;
const MSB_FIRST: c_int = 1;

const XA_ATOM: Atom = 4;
const XA_STRING: Atom = 31;
const XA_UTF8_STRING: Atom = 45;
const XA_CLIPBOARD: Atom = 69;
const XA_PRIMARY: Atom = 1;
const XA_TARGETS: Atom = 161;

const XK_BACK_SPACE: KeySym = 0xFF08;
const XK_RETURN: KeySym = 0xFF0D;
const XK_KP_ENTER: KeySym = 0xFF8D;
const XK_ESCAPE: KeySym = 0xFF1B;
const XK_HOME: KeySym = 0xFF50;
const XK_LEFT: KeySym = 0xFF51;
const XK_RIGHT: KeySym = 0xFF53;
const XK_PAGE_UP: KeySym = 0xFF55;
const XK_PAGE_DOWN: KeySym = 0xFF56;
const XK_END: KeySym = 0xFF57;
const XK_DELETE: KeySym = 0xFFFF;
const XK_A: KeySym = 0x61;
const XK_C: KeySym = 0x63;
const XK_V: KeySym = 0x76;
const XK_a: KeySym = 0x61;
const XK_c: KeySym = 0x63;
const XK_v: KeySym = 0x76;
const XC_HAND2: c_uint = 60;

unsafe fn sync_link_cursor(display: *mut Display, window: c_ulong, hand_cursor: c_ulong, over_link: bool) {
    if over_link {
        XDefineCursor(display, window, hand_cursor);
    } else {
        XUndefineCursor(display, window);
    }
}

#[link(name = "X11")]
unsafe extern "C" {
    fn XOpenDisplay(display_name: *const c_char) -> *mut Display;
    fn XDefaultScreen(display: *mut Display) -> c_int;
    fn XDefaultVisual(display: *mut Display, screen_number: c_int) -> *mut Visual;
    fn XDefaultColormap(display: *mut Display, screen_number: c_int) -> c_ulong;
    fn XDefaultDepth(display: *mut Display, screen_number: c_int) -> c_int;
    fn XRootWindow(display: *mut Display, screen_number: c_int) -> c_ulong;
    fn XBlackPixel(display: *mut Display, screen_number: c_int) -> c_ulong;
    fn XWhitePixel(display: *mut Display, screen_number: c_int) -> c_ulong;
    fn XCreateSimpleWindow(
        display: *mut Display,
        parent: c_ulong,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
        border_width: c_uint,
        border: c_ulong,
        background: c_ulong,
    ) -> c_ulong;
    fn XStoreName(display: *mut Display, window: c_ulong, window_name: *const c_char) -> c_int;
    fn XSelectInput(display: *mut Display, window: c_ulong, event_mask: c_long) -> c_int;
    fn XMapWindow(display: *mut Display, window: c_ulong) -> c_int;
    fn XInternAtom(display: *mut Display, atom_name: *const c_char, only_if_exists: c_int) -> Atom;
    fn XSetWMProtocols(
        display: *mut Display,
        window: c_ulong,
        protocols: *mut Atom,
        count: c_int,
    ) -> c_int;
    fn XCreateGC(
        display: *mut Display,
        drawable: c_ulong,
        valuemask: c_ulong,
        values: *mut c_void,
    ) -> *mut GC;
    fn XNextEvent(display: *mut Display, event_return: *mut XEvent);
    fn XLookupString(
        event_struct: *mut XKeyEvent,
        buffer_return: *mut c_char,
        bytes_buffer: c_int,
        keysym_return: *mut KeySym,
        status_in_out: *mut c_void,
    ) -> c_int;
    #[link_name = "XSetForeground"]
    fn x_set_foreground(display: *mut Display, gc: *mut GC, foreground: c_ulong) -> c_int;
    fn XFillRectangle(
        display: *mut Display,
        drawable: c_ulong,
        gc: *mut GC,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
    ) -> c_int;
    fn XDrawRectangle(
        display: *mut Display,
        drawable: c_ulong,
        gc: *mut GC,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
    ) -> c_int;
    fn XDrawLine(
        display: *mut Display,
        drawable: c_ulong,
        gc: *mut GC,
        x1: c_int,
        y1: c_int,
        x2: c_int,
        y2: c_int,
    ) -> c_int;
    fn XDrawLines(
        display: *mut Display,
        drawable: c_ulong,
        gc: *mut GC,
        points: *mut XPoint,
        npoints: c_int,
        mode: c_int,
        join_style: c_int,
    ) -> c_int;
    fn XFillPolygon(
        display: *mut Display,
        drawable: c_ulong,
        gc: *mut GC,
        points: *mut XPoint,
        npoints: c_int,
        shape: c_int,
        mode: c_int,
    ) -> c_int;
    fn XDrawArc(
        display: *mut Display,
        drawable: c_ulong,
        gc: *mut GC,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
        angle1: c_int,
        angle2: c_int,
    ) -> c_int;
    fn XFillArc(
        display: *mut Display,
        drawable: c_ulong,
        gc: *mut GC,
        x: c_int,
        y: c_int,
        width: c_uint,
        height: c_uint,
        angle1: c_int,
        angle2: c_int,
    ) -> c_int;
    fn XDrawString(
        display: *mut Display,
        drawable: c_ulong,
        gc: *mut GC,
        x: c_int,
        y: c_int,
        string: *const c_char,
        length: c_int,
    ) -> c_int;
    fn XSetClipRectangles(
        display: *mut Display,
        gc: *mut GC,
        clip_x_origin: c_int,
        clip_y_origin: c_int,
        rectangles: *const XRectangle,
        n_rects: c_int,
        ordering: c_int,
    ) -> c_int;
    fn XSetClipMask(display: *mut Display, gc: *mut GC, pixmap: c_ulong) -> c_int;
    fn XFlush(display: *mut Display) -> c_int;
    fn XPending(display: *mut Display) -> c_int;
    fn XCreateImage(
        display: *mut Display,
        visual: *mut Visual,
        depth: c_uint,
        format: c_int,
        offset: c_int,
        data: *mut c_char,
        width: c_uint,
        height: c_uint,
        bitmap_pad: c_int,
        bytes_per_line: c_int,
    ) -> *mut XImage;
    fn XPutImage(
        display: *mut Display,
        drawable: c_ulong,
        gc: *mut GC,
        image: *mut XImage,
        src_x: c_int,
        src_y: c_int,
        dest_x: c_int,
        dest_y: c_int,
        width: c_uint,
        height: c_uint,
    ) -> c_int;
    fn XDestroyImage(image: *mut XImage) -> c_int;
    fn XSetSelectionOwner(
        display: *mut Display,
        window: c_ulong,
        selection: Atom,
        time: c_ulong,
    ) -> c_int;
    fn XSetErrorHandler(
        handler: Option<extern "C" fn(*mut Display, *mut XErrorEvent) -> c_int>,
    ) -> *mut c_void;
    fn XConvertSelection(
        display: *mut Display,
        selection: Atom,
        target: Atom,
        property: Atom,
        requestor: c_ulong,
        time: c_ulong,
    ) -> c_int;
    fn XGetWindowProperty(
        display: *mut Display,
        window: c_ulong,
        property: Atom,
        long_offset: c_long,
        long_length: c_long,
        delete: c_int,
        req_type: Atom,
        actual_type_return: *mut Atom,
        actual_format_return: *mut c_int,
        nitems_return: *mut c_ulong,
        bytes_after_return: *mut c_ulong,
        prop_return: *mut *mut u8,
    ) -> c_int;
    fn XChangeProperty(
        display: *mut Display,
        window: c_ulong,
        property: Atom,
        type_: Atom,
        format: c_int,
        mode: c_int,
        data: *const c_void,
        nelements: c_int,
    ) -> c_int;
    fn XSendEvent(
        display: *mut Display,
        window: c_ulong,
        propagate: c_int,
        event_mask: c_long,
        send_event: *mut XEvent,
    ) -> c_int;
    fn XFree(data: *mut c_void) -> c_int;
    fn XFreeGC(display: *mut Display, gc: *mut GC);
    fn XDestroyWindow(display: *mut Display, window: c_ulong) -> c_int;
    fn XCloseDisplay(display: *mut Display) -> c_int;
    fn XCreateFontCursor(display: *mut Display, shape: c_uint) -> c_ulong;
    fn XDefineCursor(display: *mut Display, window: c_ulong, cursor: c_ulong) -> c_int;
    fn XUndefineCursor(display: *mut Display, window: c_ulong) -> c_int;
    fn XFreeCursor(display: *mut Display, cursor: c_ulong) -> c_int;
}

#[link(name = "Xft")]
unsafe extern "C" {
    fn XftFontOpenName(display: *mut Display, screen: c_int, name: *const c_char) -> *mut XftFont;
    fn XftFontClose(display: *mut Display, font: *mut XftFont);
    fn XftDrawCreate(
        display: *mut Display,
        drawable: c_ulong,
        visual: *mut Visual,
        colormap: c_ulong,
    ) -> *mut XftDraw;
    fn XftDrawDestroy(draw: *mut XftDraw);
    fn XftDrawStringUtf8(
        draw: *mut XftDraw,
        color: *const XftColor,
        font: *mut XftFont,
        x: c_int,
        y: c_int,
        string: *const u8,
        len: c_int,
    );
    fn XftDrawSetClipRectangles(
        draw: *mut XftDraw,
        clip_x: c_int,
        clip_y: c_int,
        rects: *const XRectangle,
        nrects: c_int,
    );
    fn XftDrawSetClip(draw: *mut XftDraw, region: *mut c_void);
}

#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcPatternGetString(
        pattern: *mut c_void,
        object: *const u8,
        id: c_int,
        result: *mut *const c_char,
    ) -> c_int;
    fn FcPatternGetDouble(
        pattern: *mut c_void,
        object: *const u8,
        id: c_int,
        result: *mut f64,
    ) -> c_int;
}

#[link(name = "freetype")]
unsafe extern "C" {
    fn FT_Init_FreeType(library: *mut *mut FT_LibraryRec) -> c_int;
    fn FT_New_Face(
        library: *mut FT_LibraryRec,
        file: *const c_char,
        face_index: c_long,
        face: *mut *mut FT_FaceRec,
    ) -> c_int;
    fn FT_Set_Char_Size(
        face: *mut FT_FaceRec,
        char_width: c_long,
        char_height: c_long,
        hres: c_uint,
        vres: c_uint,
    ) -> c_int;
    fn FT_Get_Char_Index(face: *mut FT_FaceRec, charcode: c_ulong) -> c_uint;
    fn FT_Get_Advance(
        face: *mut FT_FaceRec,
        gindex: c_uint,
        load_flags: c_long,
        p_advance: *mut c_long,
    ) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_page_is_newtab() {
        let page = load_page(None);
        assert_eq!(page.source, "ghostab:newpage");
        assert!(page.html.contains("Ghostab"));
    }

    #[test]
    fn newtab_page_is_served_by_ghostab_scheme() {
        let page = load_page(Some("ghostab:newpage"));
        assert_eq!(page.source, "ghostab:newpage");
        assert!(page.html.contains("Type a URL or a search term"));
    }

    #[test]
    fn about_scheme_does_not_get_https_prefixed() {
        assert_eq!(normalize_navigation_target("about:sample", &Settings::default()), "about:sample");
        assert_eq!(normalize_navigation_target("about:blank", &Settings::default()), "about:blank");
        assert_eq!(normalize_navigation_target("about:whatever", &Settings::default()), "about:whatever");
    }

    #[test]
    fn ghostab_scheme_does_not_get_https_prefixed() {
        assert_eq!(
            normalize_navigation_target("ghostab:newpage", &Settings::default()),
            "ghostab:newpage"
        );
        assert_eq!(normalize_navigation_target("ghostab:foo", &Settings::default()), "ghostab:foo");
    }

    #[test]
    fn plain_input_still_gets_https_prefixed() {
        assert_eq!(
            normalize_navigation_target("example.com", &Settings::default()),
            "https://example.com"
        );
        assert_eq!(
            normalize_navigation_target("  example.com ", &Settings::default()),
            "https://example.com"
        );
    }

    #[test]
    fn localhost_input_gets_http_prefixed() {
        assert_eq!(normalize_navigation_target("localhost", &Settings::default()), "http://localhost");
        assert_eq!(
            normalize_navigation_target("localhost:8080", &Settings::default()),
            "http://localhost:8080"
        );
        assert_eq!(
            normalize_navigation_target("localhost:8080/foo/bar", &Settings::default()),
            "http://localhost:8080/foo/bar"
        );
        assert_eq!(
            normalize_navigation_target("127.0.0.1:3000", &Settings::default()),
            "http://127.0.0.1:3000"
        );
        assert_eq!(normalize_navigation_target("[::1]:8080", &Settings::default()), "http://[::1]:8080");
        assert_eq!(
            normalize_navigation_target("localhost settings", &Settings::default()),
            "https://www.startpage.com/sp/search?query=localhost+settings"
        );
    }

    #[test]
    fn resolve_src_handles_localhost_pages() {
        assert_eq!(
            resolve_src("http://localhost:8090/", "/page2.html"),
            "http://localhost:8090/page2.html"
        );
        assert_eq!(
            resolve_src("http://localhost:8090/dir/index.html", "sub.html"),
            "http://localhost:8090/dir/sub.html"
        );
        assert_eq!(
            resolve_src("http://localhost:8090/", "https://example.com/x"),
            "https://example.com/x"
        );
    }

    #[test]
    fn plain_search_input_goes_to_startpage() {
        assert_eq!(
            normalize_navigation_target("rust tutorial", &Settings::default()),
            "https://www.startpage.com/sp/search?query=rust+tutorial"
        );
        assert_eq!(
            normalize_navigation_target("c++ & rust", &Settings::default()),
            "https://www.startpage.com/sp/search?query=c%2B%2B+%26+rust"
        );
        assert_eq!(
            normalize_navigation_target("ghostab", &Settings::default()),
            "https://www.startpage.com/sp/search?query=ghostab"
        );
    }

    #[test]
    fn selected_search_engine_changes_the_search_url() {
        let mut settings = Settings::default();
        assert_eq!(
            search_url_for(&settings, "rust tutorial"),
            "https://www.startpage.com/sp/search?query=rust+tutorial"
        );
        settings.search_engine = SearchEngine::Custom;
        settings.search_url = "https://example.com/search?q=%s".to_string();
        assert_eq!(
            search_url_for(&settings, "rust tutorial"),
            "https://example.com/search?q=rust+tutorial"
        );
        assert_eq!(
            search_url_for(&settings, "c++ & rust"),
            "https://example.com/search?q=c%2B%2B+%26+rust"
        );
        assert_eq!(
            normalize_navigation_target("rust tutorial", &settings),
            "https://example.com/search?q=rust+tutorial"
        );
        settings.search_url = String::new();
        assert_eq!(
            search_url_for(&settings, "rust tutorial"),
            "https://www.startpage.com/sp/search?query=rust+tutorial"
        );
    }

    #[test]
    fn settings_default_to_dark_mode_and_startpage() {
        let settings = Settings::default();
        assert_eq!(settings.light_mode, false);
        assert_eq!(settings.search_engine, SearchEngine::Startpage);
        assert_eq!(settings.search_url, "");
    }

    #[test]
    fn navigate_uses_the_selected_search_engine() {
        let mut app = BrowserApp::new(load_page(None));
        app.settings.search_engine = SearchEngine::Custom;
        app.settings.search_url = "https://example.com/search?q=%s".to_string();
        app.address_text = "rust tutorial".to_string();
        app.navigate();
        assert_eq!(
            app.page.source,
            "https://example.com/search?q=rust+tutorial"
        );
    }

    #[test]
    fn settings_round_trip_through_config_file() {
        let dir = std::env::temp_dir().join(format!(
            "ghostab-test-{}",
            std::process::id()
        ));
        let cfg_dir = dir.join("ghostab");
        let path = cfg_dir.join("config.txt");
        let _ = std::fs::create_dir_all(&cfg_dir);
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &dir) };
        std::fs::write(
            &path,
            "light_mode = true\nsearch_engine = custom\nsearch_url = https://example.com/?q=%s\n",
        )
        .unwrap();
        let loaded = load_settings();
        assert_eq!(loaded.light_mode, true);
        assert_eq!(loaded.search_engine, SearchEngine::Custom);
        assert_eq!(loaded.search_url, "https://example.com/?q=%s");
        save_settings(&loaded);
        let reloaded = load_settings();
        assert_eq!(reloaded, loaded);
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn about_blank_loads_an_empty_page() {
        let page = load_page(Some("about:blank"));
        assert_eq!(page.source, "about:blank");
    }

    #[test]
    fn image_demo_page_has_text_and_image() {
        let page = load_page(Some("ghostab:imagedemo"));
        assert_eq!(page.source, "ghostab:imagedemo");
        assert!(page.html.contains("This is an image"));
        assert!(page.html.contains("<img src=\""));
    }

    #[test]
    fn link_demo_page_has_click_me_link() {
        let page = load_page(Some("ghostab:linkdemo"));
        assert_eq!(page.source, "ghostab:linkdemo");
        assert!(page.html.contains("click me"));
        assert!(page.html.contains("aramczdev.github.io"));
    }

    #[test]
    fn back_and_forward_walk_navigation_history() {
        let mut app = BrowserApp::new(load_page(None));
        assert!(app.history_back.is_empty());
        assert!(app.history_forward.is_empty());
        app.navigate_new("about:sample");
        app.navigate_new("about:blank");
        assert_eq!(app.page.source, "about:blank");
        assert_eq!(app.history_back.len(), 2);
        app.go_back();
        assert_eq!(app.page.source, "about:sample");
        app.go_back();
        assert_eq!(app.page.source, "ghostab:newpage");
        assert!(app.history_back.is_empty());
        app.go_back();
        assert_eq!(app.page.source, "ghostab:newpage");
        app.go_forward();
        assert_eq!(app.page.source, "about:sample");
        app.go_forward();
        assert_eq!(app.page.source, "about:blank");
        assert!(app.history_forward.is_empty());
    }

    #[test]
    fn new_navigation_clears_forward_history() {
        let mut app = BrowserApp::new(load_page(None));
        app.navigate_new("about:sample");
        app.go_back();
        assert!(!app.history_forward.is_empty());
        app.navigate_new("about:blank");
        assert!(app.history_forward.is_empty());
        assert_eq!(app.page.source, "about:blank");
    }

    #[test]
    fn new_tab_switches_to_a_fresh_new_tab() {
        let mut app = BrowserApp::new(load_page(None));
        app.navigate_new("about:sample");
        assert_eq!(app.tabs.len(), 1);
        app.new_tab();
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1);
        assert_eq!(app.page.source, "ghostab:newpage");
    }

    #[test]
    fn switching_tabs_keeps_each_pages_state() {
        let mut app = BrowserApp::new(load_page(None));
        app.navigate_new("about:sample");
        app.new_tab();
        app.navigate_new("about:blank");
        app.switch_tab(0);
        assert_eq!(app.page.source, "about:sample");
        app.switch_tab(1);
        assert_eq!(app.page.source, "about:blank");
    }

    #[test]
    fn closing_a_tab_removes_it() {
        let mut app = BrowserApp::new(load_page(None));
        app.new_tab();
        app.new_tab();
        assert_eq!(app.tabs.len(), 3);
        app.close_tab(1);
        assert_eq!(app.tabs.len(), 2);
    }

    #[test]
    fn closing_the_last_tab_signals_quit() {
        let mut app = BrowserApp::new(load_page(None));
        app.navigate_new("about:sample");
        assert_eq!(app.close_tab(0), true);
        assert_eq!(app.tabs.len(), 1);
    }

    #[test]
    fn closing_the_active_tab_activates_the_next_tab() {
        let mut app = BrowserApp::new(load_page(None));
        app.new_tab();
        app.switch_tab(0);
        assert_eq!(app.close_tab(0), false);
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.page.source, "ghostab:newpage");
    }

    #[test]
    fn nav_buttons_are_laid_out_right_of_the_go_button() {
        let width = 960;
        assert!(back_button_x(width) < forward_button_x(width));
        assert!(forward_button_x(width) < home_button_x(width));
        assert!(home_button_x(width) < refresh_button_x(width));
        assert_eq!(
            go_button_x(width),
            back_button_x(width) - GO_WIDTH as c_int - NAV_BUTTON_GAP
        );
        assert_eq!(
            nav_button_at(back_button_x(width) + 4, ADDRESS_Y + 4, width),
            Some(NavButton::Back)
        );
        assert_eq!(
            nav_button_at(refresh_button_x(width) + 4, ADDRESS_Y + 4, width),
            Some(NavButton::Refresh)
        );
        assert_eq!(nav_button_at(10, ADDRESS_Y + 4, width), None);
    }

    #[test]
    fn tab_hit_testing_returns_tabs_and_new_button() {
        let width = 960;
        let labels = vec!["New Tab".to_string(), "Second".to_string()];
        let xs = tab_xs(&labels);
        let w0 = tab_width(&labels[0]);
        let w1 = tab_width(&labels[1]);
        assert_eq!(
            tab_at(xs[0] + 5, TAB_BAR_Y + 5, width, &labels),
            Some(TabHit::Tab(0))
        );
        assert_eq!(
            tab_at(xs[1] + 5, TAB_BAR_Y + 5, width, &labels),
            Some(TabHit::Tab(1))
        );
        assert_eq!(
            tab_at(xs[0] + w0 - 2, TAB_BAR_Y + 5, width, &labels),
            Some(TabHit::Close(0))
        );
        assert_eq!(
            tab_at(xs[1] + w1 - 2, TAB_BAR_Y + 5, width, &labels),
            Some(TabHit::Close(1))
        );
        let nx = new_tab_button_x(width);
        assert_eq!(tab_at(nx + 2, TAB_BAR_Y + 5, width, &labels), Some(TabHit::NewTab));
        assert_eq!(tab_at(20, ADDRESS_Y + 5, width, &labels), None);
        assert_eq!(close_button_at(xs[0] + w0 - 2, TAB_BAR_Y + 5, &labels), Some(0));
        assert_eq!(close_button_at(xs[1] + w1 - 5, TAB_BAR_Y + 5, &labels), Some(1));
        assert_eq!(close_button_at(xs[0] + 10, TAB_BAR_Y + 5, &labels), None);
        assert_eq!(tab_at(xs[1] + w1 + 2, TAB_BAR_Y + 5, width, &labels), None);
        assert!(tab_width("A") < tab_width("A considerably longer tab title"));
    }

    #[test]
    fn connection_state_classifies_schemes() {
        assert_eq!(
            connection_state("https://example.com/"),
            ConnectionState::Protected
        );
        assert_eq!(
            connection_state("http://example.com/"),
            ConnectionState::Unprotected
        );
        assert_eq!(
            connection_state("http://localhost:8000/page"),
            ConnectionState::Local
        );
        assert_eq!(connection_state("localhost:8000/x"), ConnectionState::Local);
        assert_eq!(
            connection_state("https://127.0.0.1:8080/"),
            ConnectionState::Local
        );
        assert_eq!(
            connection_state("/home/me/index.html"),
            ConnectionState::Local
        );
        assert_eq!(connection_state("ghostab:newpage"), ConnectionState::BuiltIn);
        assert_eq!(connection_state("ghostab:imagedemo"), ConnectionState::BuiltIn);
        assert_eq!(connection_state("about:blank"), ConnectionState::Local);
    }

    #[test]
    fn shield_hit_testing_is_left_of_the_address_bar() {
        assert!(point_in_shield(SHIELD_X + 4, ADDRESS_Y + 4));
        assert!(!point_in_shield(SHIELD_X + SHIELD_SIZE + 2, ADDRESS_Y + 4));
        assert!(!point_in_shield(SHIELD_X + 4, ADDRESS_Y - 2));
        assert_eq!(ADDRESS_BAR_X, SHIELD_X + SHIELD_SIZE + NAV_BUTTON_GAP);
    }

    #[test]
    fn new_tab_does_not_reset_light_mode() {
        let mut app = BrowserApp::new(load_page(None));
        app.settings.light_mode = true;
        apply_settings(&app.settings);
        assert!(light_mode_enabled());
        app.new_tab();
        assert!(light_mode_enabled());
        apply_settings(&Settings::default());
    }

    #[test]
    fn visible_address_windows_around_the_caret() {
        let mut app = BrowserApp::new(load_page(None));
        app.address_text = "abcdefghijklmnopqrstuvwxyz0123456789".to_string();
        app.address_cursor = 10;
        let (shown, start) = unsafe { visible_address(&app, ptr::null_mut(), 140) };
        let end = start + shown.len();
        assert!(start <= app.address_cursor && app.address_cursor <= end);
        assert_eq!(app.address_text[start..end], shown);
        assert!(shown.len() < app.address_text.len());
        assert!(shown.contains("defghij"));
    }

    #[test]
    fn visible_address_keeps_whole_short_text() {
        let mut app = BrowserApp::new(load_page(None));
        app.address_text = "about:sample".to_string();
        app.address_cursor = 12;
        let (shown, start) = unsafe { visible_address(&app, ptr::null_mut(), 500) };
        assert_eq!(shown, "about:sample");
        assert_eq!(start, 0);
    }
}
