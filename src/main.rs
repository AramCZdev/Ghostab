#![allow(unsafe_op_in_unsafe_fn)]
#![windows_subsystem = "windows"]
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Cursor;
use std::os::raw::{c_int, c_uint};
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

mod engine;
mod ui;

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

// Network and image-loading limits. These are safety rails: they prevent a
// single page from exhausting memory or being used to reach internal hosts.
const MAX_DOWNLOAD_BYTES: &str = "52428800"; // 50 MiB, enforced by curl --max-filesize
const MAX_DOWNLOAD_TIME_SECS: &str = "20";
const MAX_IMAGE_DIMENSION: u32 = 8192; // reject images wider/taller than this
const MAX_IMAGE_PIXELS: u64 = 40_000_000; // reject images with more pixels than this
const MAX_PAGE_IMAGES: usize = 64; // cap auto-loaded <img> per page

const COLOR_PAGE: u32 = 0x15181C;
const COLOR_PAGE_BORDER: u32 = 0x333A41;
const COLOR_SURFACE: u32 = 0x1E2228;
const COLOR_TITLE_BAR: u32 = 0x121417;
const COLOR_TITLE_LINE: u32 = 0x2C323A;
const COLOR_MUTED_TEXT: u32 = 0x8A93A0;
const COLOR_ADDRESS_BG: u32 = 0x23272D;
const COLOR_ADDRESS_FOCUS: u32 = 0xDF8D00;
const COLOR_ADDRESS_BORDER: u32 = 0x3A4048;
const COLOR_ADDRESS_TEXT: u32 = 0xE6E8EB;
const COLOR_BUTTON_BG: u32 = 0x2A2F36;
const COLOR_BUTTON_HOVER: u32 = 0x39404A;
const COLOR_BUTTON_BORDER: u32 = 0x414A54;
const COLOR_BUTTON_TEXT: u32 = 0xE4E7EA;
const COLOR_BODY_TEXT: u32 = 0xD8DCE1;
const COLOR_LINK: u32 = 0x6CB2FF;
const COLOR_SCROLLBAR_TRACK: u32 = 0x262A30;
const COLOR_SCROLLBAR_THUMB: u32 = 0x4A525C;
const COLOR_STATUS_BAR: u32 = 0x1E2228;
const COLOR_SELECTION_BG: u32 = 0x2B6CB0;
const COLOR_SELECTION_TEXT: u32 = 0xFFFFFF;
const COLOR_CARET: u32 = 0xF2F4F6;
const COLOR_GO_BG: u32 = 0xDF8D00;
const COLOR_GO_BORDER: u32 = 0xF0B25C;
const COLOR_GO_TEXT: u32 = 0x1A1A1A;
const COLOR_IMAGE_BORDER: u32 = 0x444D58;

const COLOR_TAB_STRIP: u32 = 0x171A1E;
const COLOR_TAB_ACTIVE_FILL: u32 = 0x1F2329;
const COLOR_TAB_ACTIVE_ORANGE: u32 = 0xDF8D00;
const COLOR_TAB_INACTIVE_FILL: u32 = 0x20242A;
const COLOR_TAB_BORDER: u32 = 0x3B424A;
const COLOR_TAB_TEXT: u32 = 0xE3E6EA;
const COLOR_TAB_TEXT_MUTED: u32 = 0x9098A4;
const COLOR_SHIELD_BLUE: u32 = 0x3B82F6;
const COLOR_SHIELD_OUTLINE: u32 = 0xF2F4F6;
const COLOR_SHIELD_DANGER: u32 = 0xD64545;

static mut LIGHT_MODE: bool = false;

fn light_mode_enabled() -> bool {
    unsafe { LIGHT_MODE }
}

fn pal(color: u32) -> u32 {
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
    } else if let Ok(appdata) = env::var("APPDATA") {
        PathBuf::from(appdata)
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


fn main() {
    let args: Vec<String> = env::args().collect();
    let page = load_page(args.get(1).map(String::as_str));
    let mut app = BrowserApp::new(page);
    app.settings = load_settings();
    apply_settings(&app.settings);

    ui::run_window(app);
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
        if href.starts_with('#') {
            return;
        }
        // mailto: links are handed to the desktop mail client instead of
        // being treated as a navigation target.
        if href.starts_with("mailto:") {
            self.open_mailto(href);
            return;
        }
        let target = resolve_src(&self.page.source, href);
        // Only allow navigable web schemes. This blocks javascript:, data:,
        // file:, ftp:, tel: and anything else that could escape the browser
        // context or reach local files — except that a local page may link to
        // another local page, mirroring real browsers.
        let local_to_local =
            self.page.source.starts_with("file://") && target.starts_with("file://");
        if !local_to_local && !is_safe_link_scheme(href) {
            return;
        }
        self.navigate_new(&target);
    }

    fn open_mailto(&mut self, href: &str) {
        if !is_valid_mailto(href) {
            return;
        }
        if let Err(error) = open_external(href) {
            eprintln!("ghostab-log: could not open '{href}': {error}");
            self.navigate_new("ghostab:mailto-failed");
        }
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
        // Bound how many images a single page may auto-load.
        srcs.truncate(MAX_PAGE_IMAGES);
        let mut specs = HashMap::new();

        // When a remote page references an image on localhost or a private
        // address, do not fetch it. This blocks remote sites from probing
        // internal hosts (SSRF). Local and built-in pages are trusted.
        let remote_page = matches!(
            connection_state(&self.page.source),
            ConnectionState::Protected | ConnectionState::Unprotected
        );

        for raw in srcs {
            if raw.is_empty() || specs.contains_key(&raw) {
                continue;
            }
            let key = resolve_src(&self.page.source, &raw);

            if remote_page && is_private_host(url_host(&key)) {
                self.images.failed.insert(key.clone());
                continue;
            }

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
        Some("ghostab:mailto-failed") => BrowserPage {
            source: "ghostab:mailto-failed".to_string(),
            html: error_page(
                "Could not open mail client",
                "Ghostab could not find or start a mail program on this system.",
            ),
            title: "Could not open mail client".to_string(),
        },
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
        Some(target) if target.starts_with("file://") => load_file_url(target),
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
            title: "New Tab".to_string(),
        },
    }
}

fn is_url(target: &str) -> bool {
    target.starts_with("http://") || target.starts_with("https://")
}

/// Converts a file:// URL into a local filesystem path. Accepts the empty
/// authority form (file:///path) and "localhost"; any other host is rejected.
/// Windows drive paths (file:///C:/...) lose their leading slash, and
/// percent-encoded segments are decoded.
fn file_path_from_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let (authority, tail) = rest.split_once('/')?;
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return None;
    }
    let mut path = percent_decode(&format!("/{tail}"));
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
        path.remove(0);
    }
    if path.is_empty() || path == "/" {
        return None;
    }
    Some(path)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 3 <= bytes.len() {
            let high = (bytes[i + 1] as char).to_digit(16);
            let low = (bytes[i + 2] as char).to_digit(16);
            if let (Some(high), Some(low)) = (high, low) {
                out.push((high * 16 + low) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// mailto links are handed to the operating system, so refuse anything
/// suspicious before it ever leaves the browser.
fn is_valid_mailto(href: &str) -> bool {
    match href.strip_prefix("mailto:") {
        Some(rest) => !rest.is_empty() && !rest.chars().any(|c| c.is_control() || c.is_whitespace()),
        None => false,
    }
}

#[cfg_attr(test, allow(unused_variables))]
fn open_external(url: &str) -> Result<(), String> {
    #[cfg(test)]
    return Ok(());
    #[cfg(not(test))]
    {
        #[cfg(target_os = "windows")]
        let program = "explorer";
        #[cfg(target_os = "macos")]
        let program = "open";
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let program = "xdg-open";
        Command::new(program)
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn load_file_url(url: &str) -> BrowserPage {
    let source = url.to_string();
    let Some(path) = file_path_from_url(url) else {
        return BrowserPage {
            source,
            html: error_page(
                "Could not read file",
                &format!("'{url}' is not a valid file URL."),
            ),
            title: "Could not read file".to_string(),
        };
    };
    match fs::read_to_string(&path) {
        Ok(contents) => BrowserPage {
            title: extract_title(&contents),
            source,
            html: contents,
        },
        Err(error) => BrowserPage {
            source,
            html: error_page("Could not read file", &format!("{path}: {error}")),
            title: "Could not read file".to_string(),
        },
    }
}

/// True if `href` uses a scheme Ghostab may navigate to. Relative references
/// (no colon) are always allowed; known web/internal schemes are allowed;
/// everything else (mailto, javascript, data, file, ftp, tel, ...) is not.
fn is_safe_link_scheme(href: &str) -> bool {
    let Some((scheme, _)) = href.split_once(':') else {
        return true;
    };
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "about" | "ghostab"
    )
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
        || trimmed.starts_with("file://")
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
    // Refuse anything that is not http/https before it ever reaches curl.
    if !is_url(url) {
        return BrowserPage {
            source: url.to_string(),
            html: error_page(
                "Could not load website",
                "Only http:// and https:// URLs are supported.",
            ),
            title: "Could not load website".to_string(),
        };
    }
    let mut cmd = Command::new("curl");
    cmd.args([
        "--location",
        "--max-time",
        MAX_DOWNLOAD_TIME_SECS,
        // Cap response size so one page cannot exhaust memory.
        "--max-filesize",
        MAX_DOWNLOAD_BYTES,
        // Allow only http/https, and refuse protocol changes on redirect
        // (blocks redirects to file://, ftp://, gopher://, etc.).
        "--proto",
        "=http,https",
        "--proto-redir",
        "=http,https",
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
    let bytes = if let Some(path) = file_path_from_url(key) {
        fs::read(path).map_err(|error| error.to_string())?
    } else if is_url(key) {
        fetch_bytes(key)?
    } else {
        fs::read(key).map_err(|error| error.to_string())?
    };
    // Check the header-declared dimensions before decoding so a crafted image
    // cannot allocate a huge buffer (decompression bomb).
    let (width, height) = image::io::Reader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?
        .into_dimensions()
        .map_err(|error| error.to_string())?;
    if image_exceeds_limits(width, height) {
        return Err(format!("image dimensions exceed limits ({width}x{height})"));
    }
    let image = image::load_from_memory(&bytes).map_err(|error| error.to_string())?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(DecodedImage {
        width,
        height,
        rgba: rgba.into_raw(),
    })
}

/// Returns true when an image is too large to decode safely. Both the raw
/// dimensions and the total pixel count are bounded, and a zero dimension is
/// rejected outright.
fn image_exceeds_limits(width: u32, height: u32) -> bool {
    width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || (width as u64) * (height as u64) > MAX_IMAGE_PIXELS
}

/// Parses a dotted-quad IPv4 address. Leading zeros are rejected because some
/// resolvers treat them as octal, which could let a literal bypass a blocklist.
fn parse_ipv4(host: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut octets = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty()
            || part.len() > 3
            || !part.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        if part.len() > 1 && part.starts_with('0') {
            return None;
        }
        let value: u16 = part.parse().ok()?;
        if value > 255 {
            return None;
        }
        octets[i] = value as u8;
    }
    Some(octets)
}

/// True when `host` is loopback, link-local, or a private-address literal.
/// Accepts an optional `:port` suffix and bracketed/unbracketed IPv6.
fn is_private_host(host: &str) -> bool {
    let host = host.trim();
    if host.is_empty() {
        return false;
    }
    // Bracketed IPv6, e.g. "[::1]" or "[::1]:8080".
    if let Some(inner) = host.strip_prefix('[') {
        let inner = inner.split(']').next().unwrap_or(inner);
        return is_private_ipv6(inner);
    }
    // Hostname or IPv4, possibly with a numeric :port suffix.
    let bare = strip_port(host);
    if bare.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Some(ip) = parse_ipv4(bare) {
        let [a, b, _, _] = ip;
        return a == 0
            || a == 10
            || a == 127
            || (a == 172 && (16..=31).contains(&b))
            || (a == 192 && b == 168)
            || (a == 169 && b == 254)
            || (a == 100 && (64..=127).contains(&b));
    }
    // Unbracketed IPv6 literal.
    is_private_ipv6(bare)
}

fn is_private_ipv6(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    if lower == "::1" || lower == "0:0:0:0:0:0:0:1" {
        return true;
    }
    lower.starts_with("fe80:")
}

/// Removes a trailing `:port` (digits) unless the host looks like IPv6.
fn strip_port(host: &str) -> &str {
    if host.contains("::") {
        return host;
    }
    host.rsplit_once(':').map_or(host, |(head, port)| {
        if !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()) {
            head
        } else {
            host
        }
    })
}

/// Extracts the host portion of a URL (no port, no brackets around IPv6).
fn url_host(url: &str) -> &str {
    let rest = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url);
    if rest.starts_with('[') {
        let end = rest.find(']').map(|index| index + 1).unwrap_or(rest.len());
        return &rest[..end];
    }
    rest.split(['/', ':']).next().unwrap_or("")
}

fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    if !is_url(url) {
        return Err("unsupported URL scheme (only http/https allowed)".to_string());
    }
    let output = Command::new("curl")
        .args([
            "--location",
            "--max-time",
            MAX_DOWNLOAD_TIME_SECS,
            "--max-filesize",
            MAX_DOWNLOAD_BYTES,
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "--silent",
            "--show-error",
            url,
        ])
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
    let w = crate::ui::text_width(&label) + TAB_CLOSE_WIDTH + 30;
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

fn visible_address(app: &BrowserApp, max_width: c_int) -> (String, usize) {
    let text = &app.address_text;
    if max_width <= 0 {
        return (String::new(), 0);
    }
    if text.is_empty() || crate::ui::text_width(text) <= max_width {
        return (text.clone(), 0);
    }
    let cursor = app.address_cursor.min(text.len());
    let mut start = cursor;
    loop {
        let previous = prev_char_index(text, start);
        if previous == 0 {
            break;
        }
        if crate::ui::text_width(&text[previous..cursor]) > max_width {
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
        if crate::ui::text_width(&text[start..next]) > max_width {
            break;
        }
        end = next;
    }
    (text[start..end].to_string(), start)
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
const SHIELD_H: c_int = 240;
const SHIELD_TEXT_Y: c_int = 100;
const SHIELD_LINE_STEP: c_int = 26;

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



fn shorten(text: &str, max_chars: usize) -> String {
    let mut shortened: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        shortened.push_str("...");
    }
    shortened
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
        assert_eq!(
            connection_state("file:///home/me/page.html"),
            ConnectionState::Local
        );
    }

    #[test]
    fn file_urls_map_to_local_paths() {
        assert_eq!(
            file_path_from_url("file:///home/aramcz/hi.html").as_deref(),
            Some("/home/aramcz/hi.html")
        );
        assert_eq!(
            file_path_from_url("file://localhost/tmp/a.html").as_deref(),
            Some("/tmp/a.html")
        );
        assert_eq!(
            file_path_from_url("file:///C:/Users/me/hi.html").as_deref(),
            Some("C:/Users/me/hi.html")
        );
        assert_eq!(
            file_path_from_url("file:///dir/we%20ird%20name.html").as_deref(),
            Some("/dir/we ird name.html")
        );
        assert_eq!(file_path_from_url("file://evil.com/x.html"), None);
        assert_eq!(file_path_from_url("https://example.com/a.html"), None);
    }

    #[test]
    fn normalize_navigation_target_keeps_file_urls() {
        let settings = Settings::default();
        assert_eq!(
            normalize_navigation_target("file:///home/aramcz/hi.html", &settings),
            "file:///home/aramcz/hi.html"
        );
    }

    #[test]
    fn load_page_reads_file_urls_and_reports_missing_files() {
        let page = load_page(Some("file:///ghostab-definitely-missing.html"));
        assert_eq!(page.title, "Could not read file");

        let dir = std::env::temp_dir().join(format!("ghostab-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hi.html");
        std::fs::write(&path, "<html><title>Hi from disk</title></html>").unwrap();
        let page = load_page(Some(&format!("file://{}", path.display())));
        assert_eq!(page.title, "Hi from disk");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn local_pages_may_link_to_other_local_pages_only() {
        let mut app = BrowserApp::new(load_page(Some("file:///dir/index.html")));
        app.open_link("next.html");
        assert_eq!(app.page.source, "file:///dir/next.html");

        let mut web = BrowserApp::new(load_page(None));
        web.page.source = "https://example.com/page.html".to_string();
        web.open_link("file:///etc/passwd");
        assert_eq!(web.page.source, "https://example.com/page.html");
    }

    #[test]
    fn mailto_validation_rejects_malformed_links() {
        assert!(is_valid_mailto("mailto:hi@example.com"));
        assert!(is_valid_mailto("mailto:hi@example.com?subject=Hello"));
        assert!(!is_valid_mailto("mailto:"));
        assert!(!is_valid_mailto("mailto:a b@example.com"));
        assert!(!is_valid_mailto("mailto:a\nb@example.com"));
        assert!(!is_valid_mailto("http://example.com"));
    }

    #[test]
    fn mailto_links_open_externally_without_navigating() {
        let mut app = BrowserApp::new(load_page(None));
        app.open_link("mailto:hi@example.com");
        assert_eq!(app.page.source, "ghostab:newpage");

        app.open_link("mailto:");
        assert_eq!(app.page.source, "ghostab:newpage");
    }

    #[test]
    fn mailto_failure_page_is_built_in() {
        let page = load_page(Some("ghostab:mailto-failed"));
        assert_eq!(page.title, "Could not open mail client");
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
        let (shown, start) = visible_address(&app, 140);
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
        let (shown, start) = visible_address(&app, 500);
        assert_eq!(shown, "about:sample");
        assert_eq!(start, 0);
    }

    #[test]
    fn foreign_schemes_are_rejected_in_links() {
        for href in [
            "mailto:someone@example.com",
            "javascript:alert(1)",
            "data:text/html,<b>hi</b>",
            "file:///etc/passwd",
            "ftp://example.com/x",
            "tel:+123456",
            "gopher://example.com",
        ] {
            assert!(!is_safe_link_scheme(href), "should reject {href}");
        }
        for href in [
            "https://example.com/",
            "HTTP://EXAMPLE.COM/x",
            "about:sample",
            "ghostab:newpage",
            "/relative/path.html",
            "page2.html",
        ] {
            assert!(is_safe_link_scheme(href), "should allow {href}");
        }
    }

    #[test]
    fn open_link_ignores_foreign_schemes() {
        let mut app = BrowserApp::new(load_page(None));
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "mailto:test@example.com",
            "data:text/plain,hi",
            "ftp://example.com/x",
        ] {
            app.open_link(bad);
            assert_eq!(app.page.source, "ghostab:newpage", "link not ignored: {bad}");
        }
        app.open_link("about:sample");
        assert_eq!(app.page.source, "about:sample");
    }

    #[test]
    fn fetch_rejects_unsupported_schemes_without_network() {
        assert!(fetch_bytes("file:///etc/passwd").is_err());
        assert!(fetch_bytes("ftp://example.com/x").is_err());
        assert!(fetch_bytes("data:text/plain,hi").is_err());
        assert!(fetch_bytes("gopher://example.com").is_err());
        let page = fetch_url("file:///etc/passwd");
        assert_eq!(page.title, "Could not load website");
        assert!(page.html.contains("http:// and https://"));
    }

    #[test]
    fn private_hosts_are_detected() {
        for host in [
            "localhost",
            "localhost:8080",
            "127.0.0.1",
            "127.0.0.1:9000",
            "0.0.0.0",
            "10.0.0.1",
            "10.255.255.255",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.1.1",
            "100.64.0.1",
            "100.127.255.255",
            "::1",
            "[::1]",
            "[::1]:8080",
            "fe80::1",
        ] {
            assert!(is_private_host(host), "expected private: {host}");
        }
        for host in [
            "example.com",
            "8.8.8.8",
            "172.32.0.1",
            "192.169.1.1",
            "169.253.1.1",
            "100.63.0.1",
            "100.128.0.1",
            "2001:4860:4860::8888",
        ] {
            assert!(!is_private_host(host), "expected public: {host}");
        }
    }

    #[test]
    fn url_host_extracts_host_without_port() {
        assert_eq!(url_host("https://example.com/path"), "example.com");
        assert_eq!(url_host("http://localhost:8090/x.png"), "localhost");
        assert_eq!(url_host("http://127.0.0.1:9000/"), "127.0.0.1");
        assert_eq!(url_host("http://[::1]:8080/"), "[::1]");
    }

    #[test]
    fn image_pixel_limits_are_enforced() {
        assert!(!image_exceeds_limits(100, 100));
        assert!(!image_exceeds_limits(5000, 5000));
        assert!(image_exceeds_limits(0, 100));
        assert!(image_exceeds_limits(9000, 100));
        assert!(image_exceeds_limits(8193, 100));
        assert!(image_exceeds_limits(10000, 10000));
        assert!(image_exceeds_limits(1, 50_000_000));
    }

    #[test]
    fn remote_page_skips_private_image_hosts() {
        let mut app = BrowserApp::new(load_page(None));
        app.page.source = "https://example.com/page".to_string();
        app.page.html =
            "<html><body><img src=\"http://localhost:8090/x.png\"></body></html>".to_string();
        app.relayout();
        assert!(app.images.failed.contains("http://localhost:8090/x.png"));
    }
}
