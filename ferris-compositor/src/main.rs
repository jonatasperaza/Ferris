use ferris_compositor::{chrome, perf, renderer, scene, tabs};

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use renderer::Renderer;

use ferris_layout::layout;
use ferris_paint::paint::paint;
use ferris_style::resolve_styles;

use chrome::{Chrome, ChromeAction, NavigationHistory};
use tabs::{TabAction, TabStrip};

struct Tab {
    chrome: Chrome,
    page_frame: Option<scene::Frame>,
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Renderer>,
    frame_timer: perf::FrameTimer,
    last_frame_start: Option<std::time::Instant>,
    occluded: bool,
    initial_source: ferris_loader::Source,
    tabs: Vec<Tab>,
    active_tab: usize,
    tab_strip: TabStrip,
    modifiers: ModifiersState,
    cursor_position: (f32, f32),
}

impl App {
    fn new(initial_source: ferris_loader::Source) -> Self {
        Self {
            window: None,
            gpu: None,
            frame_timer: perf::FrameTimer::new(120),
            last_frame_start: None,
            occluded: false,
            initial_source,
            tabs: Vec::new(),
            active_tab: 0,
            tab_strip: TabStrip::new(),
            modifiers: ModifiersState::empty(),
            cursor_position: (0.0, 0.0),
        }
    }

    fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab)
    }

    fn new_tab(&mut self) {
        self.tabs.push(Tab { chrome: Chrome::new_blank(), page_frame: None });
        self.active_tab = self.tabs.len() - 1;
    }

    fn switch_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }

    fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
    }

    /// Remove a aba em `index`, ajustando `active_tab` pra continuar
    /// apontando pra uma aba válida. Não decide sozinho se a janela deve
    /// fechar quando fica sem nenhuma aba — quem chama confere
    /// `self.tabs.is_empty()` depois (ver `close_tab_and_maybe_exit`).
    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            return;
        }
        if index < self.active_tab {
            self.active_tab -= 1;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    fn close_tab_and_maybe_exit(&mut self, index: usize, event_loop: &ActiveEventLoop) {
        self.close_tab(index);
        if self.tabs.is_empty() {
            event_loop.exit();
        }
    }

    fn go_back(&mut self) {
        let Some(tab) = self.active_tab_mut() else { return };
        let Some(history) = tab.chrome.history.as_mut() else { return };
        let Some(source) = history.back().cloned() else { return };
        self.navigate_interactive(source);
    }

    fn go_forward(&mut self) {
        let Some(tab) = self.active_tab_mut() else { return };
        let Some(history) = tab.chrome.history.as_mut() else { return };
        let Some(source) = history.forward().cloned() else { return };
        self.navigate_interactive(source);
    }

    fn reload(&mut self) {
        let Some(tab) = self.active_tab() else { return };
        let Some(source) = tab.chrome.history.as_ref().map(|h| h.current().clone()) else { return };
        self.navigate_interactive(source);
    }

    /// Navega de forma interativa (fora do carregamento inicial): nunca
    /// mata o processo em caso de falha, só mostra o erro na barra de
    /// endereço da aba ativa. Não mexe no histórico — quem chama decide
    /// isso antes (Voltar/Avançar já moveram o índice; Recarregar não
    /// move nada; uma URL nova digitada, em `commit_address_bar`, cria
    /// ou atualiza o histórico só depois de confirmar sucesso aqui).
    fn navigate_interactive(&mut self, source: ferris_loader::Source) -> bool {
        match ferris_loader::load_page(&source) {
            Ok((root, stylesheet)) => {
                let bar_height = self.active_tab().map(|t| t.chrome.bar_height).unwrap_or(0.0);
                if let Some(gpu) = &self.gpu {
                    let height = gpu.logical_height() - bar_height - self.tab_strip.height;
                    let frame = build_page_frame(&root, &stylesheet, gpu.logical_width(), height);
                    if let Some(tab) = self.active_tab_mut() {
                        tab.page_frame = Some(frame);
                    }
                }
                if let Some(tab) = self.active_tab_mut() {
                    let text = chrome::source_display_text(&source);
                    tab.chrome.address_bar.set_text(&text);
                }
                true
            }
            Err(ferris_loader::LoadError::Fetch(msg)) => {
                log::warn!("navigation failed: {msg}");
                if let Some(tab) = self.active_tab_mut() {
                    tab.chrome.address_bar.set_error(Some(msg));
                }
                false
            }
        }
    }

    /// Confirma o texto digitado na barra de endereço (Enter). Só grava
    /// a nova URL/caminho no histórico quando a navegação de fato tem
    /// sucesso. Se a aba ainda não tinha histórico nenhum (aba em
    /// branco), a primeira navegação bem-sucedida CRIA o histórico em
    /// vez de tentar chamar `.go()` num histórico inexistente.
    fn commit_address_bar(&mut self) {
        let committed = self.active_tab_mut().and_then(|t| t.chrome.address_bar.commit());
        if let Some(source) = committed {
            if self.navigate_interactive(source.clone()) {
                if let Some(tab) = self.active_tab_mut() {
                    match tab.chrome.history.as_mut() {
                        Some(history) => history.go(source),
                        None => tab.chrome.history = Some(NavigationHistory::new(source)),
                    }
                }
            }
        }
    }

    fn handle_keyboard_input(&mut self, event_loop: &ActiveEventLoop, event: &winit::event::KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        let focused = self.active_tab().map(|t| t.chrome.address_bar.is_focused()).unwrap_or(false);
        for intent in classify_key(&event.logical_key, self.modifiers, focused) {
            match intent {
                KeyIntent::FocusAddressBar => {
                    if let Some(tab) = self.active_tab_mut() {
                        tab.chrome.address_bar.set_focused(true);
                    }
                }
                KeyIntent::Back => self.go_back(),
                KeyIntent::Forward => self.go_forward(),
                KeyIntent::Reload => self.reload(),
                KeyIntent::Commit => self.commit_address_bar(),
                KeyIntent::Backspace => {
                    if let Some(tab) = self.active_tab_mut() {
                        tab.chrome.address_bar.on_backspace();
                    }
                }
                KeyIntent::Cancel => {
                    if let Some(tab) = self.active_tab_mut() {
                        let text = tab.chrome.history.as_ref().map(|h| chrome::source_display_text(h.current())).unwrap_or_default();
                        tab.chrome.address_bar.cancel(&text);
                    }
                }
                KeyIntent::Type(c) => {
                    if let Some(tab) = self.active_tab_mut() {
                        tab.chrome.address_bar.on_char(c);
                    }
                }
                KeyIntent::NewTab => self.new_tab(),
                KeyIntent::CloseTab => {
                    let index = self.active_tab;
                    self.close_tab_and_maybe_exit(index, event_loop);
                }
                KeyIntent::NextTab => self.next_tab(),
                KeyIntent::Ignore => {}
            }
        }
    }
}

/// Uma intenção de teclado já classificada, independente de tipos do
/// winit — mantém `classify_key` testável sem precisar construir um
/// `KeyEvent` real (o winit não permite construir um fora do próprio
/// crate: seu campo `platform_specific` é `pub(crate)`).
#[derive(Debug, Clone, PartialEq)]
enum KeyIntent {
    FocusAddressBar,
    Back,
    Forward,
    Reload,
    Commit,
    Backspace,
    Cancel,
    Type(char),
    NewTab,
    CloseTab,
    NextTab,
    Ignore,
}

fn classify_key(logical_key: &Key, modifiers: ModifiersState, address_bar_focused: bool) -> Vec<KeyIntent> {
    if modifiers.control_key() && !modifiers.alt_key() {
        if let Key::Character(s) = logical_key {
            if s.as_str().eq_ignore_ascii_case("l") {
                return vec![KeyIntent::FocusAddressBar];
            }
            if s.as_str().eq_ignore_ascii_case("t") {
                return vec![KeyIntent::NewTab];
            }
            if s.as_str().eq_ignore_ascii_case("w") {
                return vec![KeyIntent::CloseTab];
            }
        }
        if *logical_key == Key::Named(NamedKey::Tab) {
            return vec![KeyIntent::NextTab];
        }
        return vec![KeyIntent::Ignore];
    }
    if modifiers.alt_key() && !modifiers.control_key() {
        return match logical_key {
            Key::Named(NamedKey::ArrowLeft) => vec![KeyIntent::Back],
            Key::Named(NamedKey::ArrowRight) => vec![KeyIntent::Forward],
            _ => vec![KeyIntent::Ignore],
        };
    }
    if *logical_key == Key::Named(NamedKey::F5) {
        return vec![KeyIntent::Reload];
    }

    if !address_bar_focused {
        return vec![KeyIntent::Ignore];
    }

    match logical_key {
        Key::Named(NamedKey::Enter) => vec![KeyIntent::Commit],
        Key::Named(NamedKey::Backspace) => vec![KeyIntent::Backspace],
        Key::Named(NamedKey::Escape) => vec![KeyIntent::Cancel],
        Key::Named(NamedKey::Space) => vec![KeyIntent::Type(' ')],
        Key::Character(s) => s.chars().map(KeyIntent::Type).collect(),
        _ => vec![KeyIntent::Ignore],
    }
}

/// Texto mais curto e apropriado pra exibir como título de uma aba na
/// tira: o nome do arquivo (não o caminho inteiro) pra `Source::File`,
/// o host (não a URL inteira) pra `Source::Url`. Diferente de
/// `chrome::source_display_text`, que devolve o texto completo — usado
/// na barra de endereço, onde o caminho/URL inteiro é o que se espera.
fn tab_title(source: &ferris_loader::Source) -> String {
    match source {
        ferris_loader::Source::File(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| chrome::source_display_text(source)),
        ferris_loader::Source::Url(url) => {
            let without_scheme = url.trim_start_matches("https://").trim_start_matches("http://");
            without_scheme.split('/').next().unwrap_or(without_scheme).to_string()
        }
    }
}

fn build_page_frame(root: &ferris_dom::dom::Element, stylesheet: &ferris_css::stylesheet::Stylesheet, viewport_width: f32, viewport_height: f32) -> scene::Frame {
    let styled = resolve_styles(root, stylesheet);
    match layout::layout(&styled, viewport_width, viewport_height) {
        Some(layout_box) => paint(&layout_box),
        None => {
            log::warn!("page root has no visible layout (display:none); showing an empty frame");
            scene::Frame::default()
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = match event_loop.create_window(Window::default_attributes().with_title("Ferris")) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("failed to create window: {e:?}");
                event_loop.exit();
                return;
            }
        };
        let scale_factor = window.scale_factor() as f32;
        let uncapped = std::env::var("FERRIS_UNCAPPED").map(|v| v == "1").unwrap_or(false);
        match Renderer::new(window.clone(), scale_factor, uncapped) {
            Some(gpu) => {
                match ferris_loader::load_page(&self.initial_source) {
                    Ok((root, stylesheet)) => {
                        let chrome = Chrome::new(self.initial_source.clone(), chrome::source_display_text(&self.initial_source));
                        let height = gpu.logical_height() - chrome.bar_height - self.tab_strip.height;
                        let page_frame = Some(build_page_frame(&root, &stylesheet, gpu.logical_width(), height));
                        self.tabs.push(Tab { chrome, page_frame });
                        self.active_tab = 0;
                    }
                    Err(ferris_loader::LoadError::Fetch(msg)) => {
                        log::error!("failed to load initial page: {msg}");
                        std::process::exit(1);
                    }
                }
                self.gpu = Some(gpu);
                self.window = Some(window);
            }
            None => {
                log::error!("GPU initialization failed, exiting");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                let Some(gpu) = self.gpu.as_mut() else { return };
                gpu.resize(size.width, size.height);
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let Some(gpu) = self.gpu.as_mut() else { return };
                gpu.set_scale_factor(scale_factor as f32);
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                if !occluded {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let Some(gpu) = self.gpu.as_ref() else { return };
                let scale = gpu.scale_factor();
                self.cursor_position = (position.x as f32 / scale, position.y as f32 / scale);
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                if self.gpu.is_none() {
                    return;
                }
                let (x, y) = self.cursor_position;

                let tab_count = self.tabs.len();
                if let Some(action) = self.tab_strip.hit_test(tab_count, x, y) {
                    match action {
                        TabAction::Select(i) => self.switch_tab(i),
                        TabAction::Close(i) => self.close_tab_and_maybe_exit(i, event_loop),
                        TabAction::New => self.new_tab(),
                    }
                    return;
                }

                let chrome_y = y - self.tab_strip.height;
                let action = self.active_tab().and_then(|t| t.chrome.hit_test(x, chrome_y));
                if let Some(tab) = self.active_tab_mut() {
                    tab.chrome.address_bar.set_focused(matches!(action, Some(ChromeAction::FocusAddressBar)));
                }
                match action {
                    Some(ChromeAction::Back) => self.go_back(),
                    Some(ChromeAction::Forward) => self.go_forward(),
                    Some(ChromeAction::Reload) => self.reload(),
                    Some(ChromeAction::FocusAddressBar) | None => {}
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if self.gpu.is_none() {
                    return;
                }
                self.handle_keyboard_input(event_loop, &event);
            }
            WindowEvent::RedrawRequested => {
                let Some(gpu) = self.gpu.as_mut() else { return };
                let minimized = self.window.as_ref().and_then(|w| w.is_minimized()).unwrap_or(false);
                if self.occluded || minimized {
                    self.last_frame_start = None;
                    return;
                }

                let now = std::time::Instant::now();
                if let Some(last) = self.last_frame_start {
                    self.frame_timer.record(now - last);
                }
                self.last_frame_start = Some(now);

                let window_width = gpu.logical_width();
                let titles: Vec<String> = self
                    .tabs
                    .iter()
                    .map(|t| match t.chrome.history.as_ref() {
                        Some(history) => tab_title(history.current()),
                        None => "Nova aba".to_string(),
                    })
                    .collect();

                let mut bar_height = 0.0;
                let mut frame = scene::Frame::default();

                if let Some(tab) = self.tabs.get(self.active_tab) {
                    bar_height = tab.chrome.bar_height;
                    let page = tab.page_frame.clone().unwrap_or_default();
                    let page_dy = self.tab_strip.height + bar_height;
                    frame = chrome::translate_frame(&page, page_dy);

                    let bar_frame = tab.chrome.frame(window_width);
                    frame.commands.extend(chrome::translate_frame(&bar_frame, self.tab_strip.height).commands);
                }

                frame.commands.extend(self.tab_strip.frame(&titles, self.active_tab, window_width).commands);

                let overlay_y = self.tab_strip.height + bar_height + 6.0;
                let overlay = format!(
                    "{:.1} fps | {:.2} ms/frame | budget {}",
                    self.frame_timer.fps(),
                    self.frame_timer.average_frame_time().as_secs_f32() * 1000.0,
                    if self.frame_timer.meets_budget(perf::BUDGET_120FPS) { "OK" } else { "MISSED" },
                );
                frame.push(scene::DrawCommand::Text(scene::TextCommand {
                    x: 20.0, y: overlay_y, content: overlay, size: 18.0, color: [1.0, 0.9, 0.3, 1.0],
                }));

                match gpu.render_frame(&frame) {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        let (w, h) = (gpu.width(), gpu.height());
                        gpu.resize(w, h);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        log::error!("GPU out of memory, exiting");
                        event_loop.exit();
                    }
                    Err(e) => log::warn!("surface error: {e:?}"),
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let minimized = self.window.as_ref().and_then(|w| w.is_minimized()).unwrap_or(false);
        if self.occluded || minimized {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        event_loop.set_control_flow(ControlFlow::Poll);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    env_logger::init();
    let initial_source = std::env::args()
        .nth(1)
        .map(|arg| ferris_loader::parse_source(&arg))
        .unwrap_or_else(|| ferris_loader::Source::File(std::path::PathBuf::from("ferris-compositor/assets/fixture.html")));
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(initial_source);
    event_loop.run_app(&mut app).expect("event loop error");
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::SmolStr;

    fn file(name: &str) -> ferris_loader::Source {
        ferris_loader::Source::File(std::path::PathBuf::from(name))
    }

    fn app_with_tabs(sources: &[&str]) -> App {
        let mut app = App::new(file(sources[0]));
        for s in sources {
            let chrome = Chrome::new(file(s), (*s).to_string());
            app.tabs.push(Tab { chrome, page_frame: None });
        }
        app
    }

    // --- Review Focus: display:none page root must not panic ---
    #[test]
    fn build_page_frame_on_display_none_root_returns_empty_frame_without_panicking() {
        let root = ferris_dom::parser::parse_document("<html><body>oi</body></html>");
        let css_tokens = ferris_css::tokenizer::Tokenizer::tokenize("html { display: none; }");
        let stylesheet = ferris_css::parser::Parser::parse(&css_tokens);

        let frame = build_page_frame(&root, &stylesheet, 800.0, 600.0);

        assert!(frame.commands.is_empty());
    }

    // --- Review Focus: interactive navigation failure never exits the process ---
    #[test]
    fn navigate_interactive_on_failure_sets_the_address_bar_error_without_touching_the_page_frame() {
        let mut app = app_with_tabs(&["does-not-exist.html"]);
        let previous_frame = app.tabs[0].page_frame.clone();

        app.navigate_interactive(file("still-does-not-exist.html"));

        assert_eq!(app.tabs[0].page_frame, previous_frame, "a failed navigation must not touch the cached page frame");
        let error = app.tabs[0].chrome.address_bar.error().map(str::to_string);
        assert!(error.is_some(), "a failed navigation must set a visible error on the address bar");
    }

    // --- Review Focus: a failed typed navigation must not pollute history ---
    #[test]
    fn commit_address_bar_on_failed_navigation_does_not_add_to_history() {
        let mut app = app_with_tabs(&["does-not-exist.html"]);

        app.tabs[0].chrome.address_bar.set_focused(true);
        for c in "still-does-not-exist.html".chars() {
            app.tabs[0].chrome.address_bar.on_char(c);
        }
        app.commit_address_bar();

        assert!(
            !app.tabs[0].chrome.history.as_ref().unwrap().can_go_back(),
            "a failed typed navigation must not be added to history"
        );
    }

    // --- Review Focus: navigating from a blank tab must create its history, not panic ---
    #[test]
    fn commit_address_bar_on_a_blank_tab_creates_history_on_first_successful_navigation() {
        let dir = std::env::temp_dir().join("ferris_compositor_tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tab_blank_nav_test.html");
        std::fs::write(&path, "<p>hi</p>").unwrap();

        let mut app = App::new(ferris_loader::Source::File(path.clone()));
        app.tabs.push(Tab { chrome: Chrome::new_blank(), page_frame: None });

        for c in path.to_str().unwrap().chars() {
            app.tabs[0].chrome.address_bar.on_char(c);
        }
        app.commit_address_bar();

        assert!(app.tabs[0].chrome.history.is_some(), "the first successful navigation from a blank tab must create its history");
        assert_eq!(app.tabs[0].chrome.history.as_ref().unwrap().current(), &ferris_loader::Source::File(path));
    }

    // --- Final whole-branch review: a failed navigation from a blank tab must keep history: None ---
    #[test]
    fn commit_address_bar_on_a_blank_tab_failed_navigation_keeps_history_none() {
        let mut app = App::new(file("does-not-exist-anywhere.html"));
        app.tabs.push(Tab { chrome: Chrome::new_blank(), page_frame: None });

        app.tabs[0].chrome.address_bar.set_focused(true);
        for c in "does-not-exist-anywhere.html".chars() {
            app.tabs[0].chrome.address_bar.on_char(c);
        }
        app.commit_address_bar();

        assert!(
            app.tabs[0].chrome.history.is_none(),
            "a failed navigation from a blank tab must not create any history at all"
        );
    }

    // --- Final whole-branch review: tab-strip titles must not collide between different tabs ---
    #[test]
    fn tab_title_for_file_shows_only_the_file_name() {
        let source = ferris_loader::Source::File(std::path::PathBuf::from("C:\\pages\\a.html"));
        assert_eq!(tab_title(&source), "a.html");
    }

    #[test]
    fn tab_title_distinguishes_files_in_the_same_directory() {
        let a = ferris_loader::Source::File(std::path::PathBuf::from("C:\\pages\\a.html"));
        let b = ferris_loader::Source::File(std::path::PathBuf::from("C:\\pages\\b.html"));
        assert_ne!(tab_title(&a), tab_title(&b));
    }

    #[test]
    fn tab_title_for_url_shows_only_the_host() {
        let source = ferris_loader::Source::Url("https://example.com/some/deep/path.html".to_string());
        assert_eq!(tab_title(&source), "example.com");
    }

    // --- Review Focus: closing a tab must leave a valid tab active ---
    #[test]
    fn close_tab_keeps_the_next_tab_active_when_closing_the_active_tab_in_the_middle() {
        let mut app = app_with_tabs(&["a.html", "b.html", "c.html"]);
        app.active_tab = 1;
        app.close_tab(1);
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1, "closing the middle active tab should select what is now at the same index (formerly c.html)");
        assert_eq!(app.tabs[app.active_tab].chrome.history.as_ref().unwrap().current(), &file("c.html"));
    }

    #[test]
    fn close_tab_before_the_active_one_shifts_active_tab_left() {
        let mut app = app_with_tabs(&["a.html", "b.html", "c.html"]);
        app.active_tab = 2;
        app.close_tab(0);
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1, "the active tab's own index shifts left by one since a tab before it was removed");
        assert_eq!(app.tabs[app.active_tab].chrome.history.as_ref().unwrap().current(), &file("c.html"));
    }

    #[test]
    fn close_tab_clamps_active_tab_when_closing_the_last_active_tab() {
        let mut app = app_with_tabs(&["a.html", "b.html"]);
        app.active_tab = 1;
        app.close_tab(1);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.active_tab, 0);
    }

    // --- Review Focus: closing the only tab must not index out of bounds ---
    #[test]
    fn close_tab_on_the_only_tab_leaves_the_tabs_vector_empty() {
        let mut app = app_with_tabs(&["a.html"]);
        app.close_tab(0);
        assert!(app.tabs.is_empty());
    }

    #[test]
    fn new_tab_appends_a_blank_tab_and_switches_to_it() {
        let mut app = app_with_tabs(&["a.html"]);
        app.new_tab();
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1);
        assert!(app.tabs[1].chrome.history.is_none());
        assert!(app.tabs[1].chrome.address_bar.is_focused());
    }

    #[test]
    fn next_tab_wraps_around_to_the_first_tab() {
        let mut app = app_with_tabs(&["a.html", "b.html"]);
        app.active_tab = 1;
        app.next_tab();
        assert_eq!(app.active_tab, 0);
    }

    #[test]
    fn classify_key_ctrl_l_focuses_the_address_bar_and_does_not_type() {
        let key = Key::Character(SmolStr::new("l"));
        let intents = classify_key(&key, ModifiersState::CONTROL, false);
        assert_eq!(intents, vec![KeyIntent::FocusAddressBar]);
    }

    #[test]
    fn classify_key_ctrl_plus_other_letter_is_ignored_not_typed() {
        let key = Key::Character(SmolStr::new("c"));
        let intents = classify_key(&key, ModifiersState::CONTROL, true);
        assert_eq!(intents, vec![KeyIntent::Ignore]);
    }

    // --- Review Focus: Ctrl+T/Ctrl+W/Ctrl+Tab work regardless of focus, never leak "t"/"w" ---
    #[test]
    fn classify_key_ctrl_t_opens_a_new_tab() {
        let key = Key::Character(SmolStr::new("t"));
        assert_eq!(classify_key(&key, ModifiersState::CONTROL, false), vec![KeyIntent::NewTab]);
        assert_eq!(classify_key(&key, ModifiersState::CONTROL, true), vec![KeyIntent::NewTab], "must work even while the address bar is focused, and must not type 't'");
    }

    #[test]
    fn classify_key_ctrl_w_closes_the_current_tab() {
        let key = Key::Character(SmolStr::new("w"));
        assert_eq!(classify_key(&key, ModifiersState::CONTROL, false), vec![KeyIntent::CloseTab]);
        assert_eq!(classify_key(&key, ModifiersState::CONTROL, true), vec![KeyIntent::CloseTab], "must work even while the address bar is focused, and must not type 'w'");
    }

    #[test]
    fn classify_key_ctrl_tab_switches_to_the_next_tab() {
        let key = Key::Named(NamedKey::Tab);
        assert_eq!(classify_key(&key, ModifiersState::CONTROL, false), vec![KeyIntent::NextTab]);
    }

    #[test]
    fn classify_key_alt_left_is_back_even_when_unfocused() {
        let key = Key::Named(NamedKey::ArrowLeft);
        let intents = classify_key(&key, ModifiersState::ALT, false);
        assert_eq!(intents, vec![KeyIntent::Back]);
    }

    #[test]
    fn classify_key_alt_right_is_forward() {
        let key = Key::Named(NamedKey::ArrowRight);
        let intents = classify_key(&key, ModifiersState::ALT, false);
        assert_eq!(intents, vec![KeyIntent::Forward]);
    }

    #[test]
    fn classify_key_f5_is_reload_regardless_of_focus() {
        let key = Key::Named(NamedKey::F5);
        assert_eq!(classify_key(&key, ModifiersState::empty(), false), vec![KeyIntent::Reload]);
        assert_eq!(classify_key(&key, ModifiersState::empty(), true), vec![KeyIntent::Reload]);
    }

    #[test]
    fn classify_key_enter_backspace_escape_only_act_when_focused() {
        let empty_mods = ModifiersState::empty();
        assert_eq!(classify_key(&Key::Named(NamedKey::Enter), empty_mods, false), vec![KeyIntent::Ignore]);
        assert_eq!(classify_key(&Key::Named(NamedKey::Enter), empty_mods, true), vec![KeyIntent::Commit]);
        assert_eq!(classify_key(&Key::Named(NamedKey::Backspace), empty_mods, false), vec![KeyIntent::Ignore]);
        assert_eq!(classify_key(&Key::Named(NamedKey::Backspace), empty_mods, true), vec![KeyIntent::Backspace]);
        assert_eq!(classify_key(&Key::Named(NamedKey::Escape), empty_mods, false), vec![KeyIntent::Ignore]);
        assert_eq!(classify_key(&Key::Named(NamedKey::Escape), empty_mods, true), vec![KeyIntent::Cancel]);
    }

    #[test]
    fn classify_key_character_types_only_when_focused() {
        let key = Key::Character(SmolStr::new("z"));
        let empty_mods = ModifiersState::empty();
        assert_eq!(classify_key(&key, empty_mods, false), vec![KeyIntent::Ignore]);
        assert_eq!(classify_key(&key, empty_mods, true), vec![KeyIntent::Type('z')]);
    }

    #[test]
    fn classify_key_space_types_a_space_when_focused() {
        let key = Key::Named(NamedKey::Space);
        assert_eq!(classify_key(&key, ModifiersState::empty(), true), vec![KeyIntent::Type(' ')]);
        assert_eq!(classify_key(&key, ModifiersState::empty(), false), vec![KeyIntent::Ignore]);
    }

    #[test]
    fn classify_key_ctrl_alt_together_altgr_falls_through_to_typing_when_focused() {
        let key = Key::Character(SmolStr::new("/"));
        let both_mods = ModifiersState::CONTROL | ModifiersState::ALT;
        assert_eq!(classify_key(&key, both_mods, true), vec![KeyIntent::Type('/')], "AltGr (Ctrl+Alt together) must not be swallowed as a phantom shortcut");
        assert_eq!(classify_key(&key, both_mods, false), vec![KeyIntent::Ignore], "still ignored when unfocused, same as any other character");
    }
}
