use ferris_compositor::{chrome, perf, renderer, scene};

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

use chrome::{Chrome, ChromeAction};

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Renderer>,
    frame_timer: perf::FrameTimer,
    last_frame_start: Option<std::time::Instant>,
    occluded: bool,
    page_frame: Option<scene::Frame>,
    initial_source: ferris_loader::Source,
    chrome: Option<Chrome>,
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
            page_frame: None,
            initial_source,
            chrome: None,
            modifiers: ModifiersState::empty(),
            cursor_position: (0.0, 0.0),
        }
    }

    fn go_back(&mut self) {
        let Some(chrome) = self.chrome.as_mut() else { return };
        if let Some(source) = chrome.history.back().cloned() {
            self.navigate_interactive(source);
        }
    }

    fn go_forward(&mut self) {
        let Some(chrome) = self.chrome.as_mut() else { return };
        if let Some(source) = chrome.history.forward().cloned() {
            self.navigate_interactive(source);
        }
    }

    fn reload(&mut self) {
        let Some(chrome) = self.chrome.as_ref() else { return };
        let source = chrome.history.current().clone();
        self.navigate_interactive(source);
    }

    /// Navega de forma interativa (fora do carregamento inicial): nunca
    /// mata o processo em caso de falha, só mostra o erro na barra de
    /// endereço. Não mexe no histórico — quem chama decide isso antes
    /// (Voltar/Avançar já moveram o índice; Recarregar não move nada;
    /// uma URL nova digitada já chamou `history.go` antes de chegar aqui).
    fn navigate_interactive(&mut self, source: ferris_loader::Source) -> bool {
        match ferris_loader::load_page(&source) {
            Ok((root, stylesheet)) => {
                if let (Some(gpu), Some(chrome)) = (&self.gpu, &self.chrome) {
                    let height = gpu.logical_height() - chrome.bar_height;
                    self.page_frame = Some(build_page_frame(&root, &stylesheet, gpu.logical_width(), height));
                }
                if let Some(chrome) = self.chrome.as_mut() {
                    let text = chrome::source_display_text(&source);
                    chrome.address_bar.set_text(&text);
                }
                true
            }
            Err(ferris_loader::LoadError::Fetch(msg)) => {
                log::warn!("navigation failed: {msg}");
                if let Some(chrome) = self.chrome.as_mut() {
                    chrome.address_bar.set_error(Some(msg));
                }
                false
            }
        }
    }

    /// Confirma o texto digitado na barra de endereço (Enter). Só grava a
    /// nova URL/caminho no histórico quando a navegação de fato tem
    /// sucesso — uma navegação digitada que falha não deve deixar
    /// entrada morta no histórico (ver doc-comment de `navigate_interactive`).
    fn commit_address_bar(&mut self) {
        let committed = self.chrome.as_mut().and_then(|c| c.address_bar.commit());
        if let Some(source) = committed {
            if self.navigate_interactive(source.clone()) {
                if let Some(chrome) = self.chrome.as_mut() {
                    chrome.history.go(source);
                }
            }
        }
    }

    fn handle_keyboard_input(&mut self, event: &winit::event::KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        let focused = self.chrome.as_ref().map(|c| c.address_bar.is_focused()).unwrap_or(false);
        for intent in classify_key(&event.logical_key, self.modifiers, focused) {
            match intent {
                KeyIntent::FocusAddressBar => {
                    if let Some(chrome) = self.chrome.as_mut() {
                        chrome.address_bar.set_focused(true);
                    }
                }
                KeyIntent::Back => self.go_back(),
                KeyIntent::Forward => self.go_forward(),
                KeyIntent::Reload => self.reload(),
                KeyIntent::Commit => self.commit_address_bar(),
                KeyIntent::Backspace => {
                    if let Some(chrome) = self.chrome.as_mut() {
                        chrome.address_bar.on_backspace();
                    }
                }
                KeyIntent::Cancel => {
                    if let Some(chrome) = self.chrome.as_mut() {
                        let text = chrome::source_display_text(chrome.history.current());
                        chrome.address_bar.cancel(&text);
                    }
                }
                KeyIntent::Type(c) => {
                    if let Some(chrome) = self.chrome.as_mut() {
                        chrome.address_bar.on_char(c);
                    }
                }
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
    Ignore,
}

fn classify_key(logical_key: &Key, modifiers: ModifiersState, address_bar_focused: bool) -> Vec<KeyIntent> {
    if modifiers.control_key() && !modifiers.alt_key() {
        if let Key::Character(s) = logical_key {
            if s.as_str().eq_ignore_ascii_case("l") {
                return vec![KeyIntent::FocusAddressBar];
            }
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
                        let height = gpu.logical_height() - chrome.bar_height;
                        self.page_frame = Some(build_page_frame(&root, &stylesheet, gpu.logical_width(), height));
                        self.chrome = Some(chrome);
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
                let action = self.chrome.as_ref().and_then(|c| c.hit_test(x, y));
                if let Some(chrome) = self.chrome.as_mut() {
                    chrome.address_bar.set_focused(matches!(action, Some(ChromeAction::FocusAddressBar)));
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
                self.handle_keyboard_input(&event);
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

                let page = self.page_frame.clone().unwrap_or_default();
                let mut frame = match &self.chrome {
                    Some(chrome) => {
                        let mut f = chrome::translate_frame(&page, chrome.bar_height);
                        f.commands.extend(chrome.frame(gpu.logical_width()).commands);
                        f
                    }
                    None => page,
                };

                let overlay = format!(
                    "{:.1} fps | {:.2} ms/frame | budget {}",
                    self.frame_timer.fps(),
                    self.frame_timer.average_frame_time().as_secs_f32() * 1000.0,
                    if self.frame_timer.meets_budget(perf::BUDGET_120FPS) { "OK" } else { "MISSED" },
                );
                frame.push(scene::DrawCommand::Text(scene::TextCommand {
                    x: 20.0, y: 50.0, content: overlay, size: 18.0, color: [1.0, 0.9, 0.3, 1.0],
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
        let mut app = App::new(ferris_loader::Source::File(std::path::PathBuf::from("does-not-exist.html")));
        app.chrome = Some(Chrome::new(app.initial_source.clone(), "does-not-exist.html".to_string()));
        let previous_frame = app.page_frame.clone();

        app.navigate_interactive(ferris_loader::Source::File(std::path::PathBuf::from("still-does-not-exist.html")));

        assert_eq!(app.page_frame, previous_frame, "a failed navigation must not touch the cached page frame");
        let error = app.chrome.as_ref().unwrap().address_bar.error().map(str::to_string);
        assert!(error.is_some(), "a failed navigation must set a visible error on the address bar");
    }

    // --- Review Focus: a failed typed navigation must not pollute history ---
    #[test]
    fn commit_address_bar_on_failed_navigation_does_not_add_to_history() {
        let mut app = App::new(ferris_loader::Source::File(std::path::PathBuf::from("does-not-exist.html")));
        app.chrome = Some(Chrome::new(app.initial_source.clone(), "does-not-exist.html".to_string()));

        app.chrome.as_mut().unwrap().address_bar.set_focused(true);
        for c in "still-does-not-exist.html".chars() {
            app.chrome.as_mut().unwrap().address_bar.on_char(c);
        }
        app.commit_address_bar();

        assert!(
            !app.chrome.as_ref().unwrap().history.can_go_back(),
            "a failed typed navigation must not be added to history"
        );
    }

    // --- Review Focus: shortcuts work regardless of focus; Ctrl+L never leaks "l" ---
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
