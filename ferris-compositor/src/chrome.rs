//! UI de navegação (chrome) do ferris-compositor: barra de endereço,
//! histórico de navegação, e desenho da barra como `scene::DrawCommand`s.
//! Nenhuma dependência de UI externa — usa só as primitivas de
//! `ferris_scene` já existentes.

use crate::scene;

/// Histórico de navegação em memória de uma única aba: uma pilha linear
/// de `Source`s com um índice "atual". Navegar pra um `Source` novo
/// descarta qualquer histórico "futuro" (entradas depois do índice
/// atual), igual a qualquer navegador real.
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationHistory {
    entries: Vec<ferris_loader::Source>,
    current: usize,
}

impl NavigationHistory {
    pub fn new(initial: ferris_loader::Source) -> Self {
        Self { entries: vec![initial], current: 0 }
    }

    pub fn go(&mut self, source: ferris_loader::Source) {
        self.entries.truncate(self.current + 1);
        self.entries.push(source);
        self.current = self.entries.len() - 1;
    }

    pub fn back(&mut self) -> Option<&ferris_loader::Source> {
        if self.current == 0 {
            return None;
        }
        self.current -= 1;
        Some(&self.entries[self.current])
    }

    pub fn forward(&mut self) -> Option<&ferris_loader::Source> {
        if self.current + 1 >= self.entries.len() {
            return None;
        }
        self.current += 1;
        Some(&self.entries[self.current])
    }

    pub fn current(&self) -> &ferris_loader::Source {
        &self.entries[self.current]
    }

    pub fn can_go_back(&self) -> bool {
        self.current > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.current + 1 < self.entries.len()
    }
}

/// Estado do campo de endereço editável: texto atual, se está focado
/// (recebendo teclado), posição do cursor (em caracteres, não bytes —
/// necessário pra não quebrar texto multi-byte), e uma mensagem de erro
/// opcional (substitui a exibição do texto quando presente).
#[derive(Debug, Clone, PartialEq)]
pub struct AddressBar {
    text: String,
    focused: bool,
    cursor: usize,
    error: Option<String>,
}

impl AddressBar {
    pub fn new(initial_text: String) -> Self {
        let cursor = initial_text.chars().count();
        Self { text: initial_text, focused: false, cursor, error: None }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        if focused {
            self.error = None;
        }
    }

    pub fn on_char(&mut self, c: char) {
        self.error = None;
        let byte_idx = Self::byte_index(&self.text, self.cursor);
        self.text.insert(byte_idx, c);
        self.cursor += 1;
    }

    pub fn on_backspace(&mut self) {
        self.error = None;
        if self.cursor == 0 {
            return;
        }
        let remove_idx = Self::byte_index(&self.text, self.cursor - 1);
        self.text.remove(remove_idx);
        self.cursor -= 1;
    }

    /// Enter: devolve o `Source` a navegar (espaço em branco ao redor do
    /// texto é ignorado), ou `None` se, depois de aparado, o texto
    /// estiver vazio.
    pub fn commit(&self) -> Option<ferris_loader::Source> {
        let trimmed = self.text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(ferris_loader::parse_source(trimmed))
        }
    }

    /// Define o texto exibido, tira o foco, e limpa qualquer erro —
    /// usado tanto pelo Esc (`cancel`) quanto sempre que uma navegação
    /// bem-sucedida precisa sincronizar a barra com a página atual.
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.text.chars().count();
        self.focused = false;
        self.error = None;
    }

    /// Esc: reverte pro texto dado (tipicamente a URL/caminho da página
    /// atual) e sai do modo de edição.
    pub fn cancel(&mut self, reset_text: &str) {
        self.set_text(reset_text);
    }

    pub fn set_error(&mut self, message: Option<String>) {
        self.error = message;
    }

    fn byte_index(s: &str, char_idx: usize) -> usize {
        s.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(s.len())
    }
}

const BAR_HEIGHT: f32 = 44.0;
const BUTTON_SIZE: f32 = 32.0;
const BUTTON_MARGIN: f32 = 6.0;
const BUTTON_Y: f32 = (BAR_HEIGHT - BUTTON_SIZE) / 2.0;
const BACK_X: f32 = BUTTON_MARGIN;
const FORWARD_X: f32 = BACK_X + BUTTON_SIZE + BUTTON_MARGIN;
const RELOAD_X: f32 = FORWARD_X + BUTTON_SIZE + BUTTON_MARGIN;
const ADDRESS_BAR_X: f32 = RELOAD_X + BUTTON_SIZE + BUTTON_MARGIN;
const ADDRESS_BAR_MARGIN_RIGHT: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct ButtonRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl ButtonRect {
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

fn back_button_rect() -> ButtonRect {
    ButtonRect { x: BACK_X, y: BUTTON_Y, width: BUTTON_SIZE, height: BUTTON_SIZE }
}

fn forward_button_rect() -> ButtonRect {
    ButtonRect { x: FORWARD_X, y: BUTTON_Y, width: BUTTON_SIZE, height: BUTTON_SIZE }
}

fn reload_button_rect() -> ButtonRect {
    ButtonRect { x: RELOAD_X, y: BUTTON_Y, width: BUTTON_SIZE, height: BUTTON_SIZE }
}

fn address_bar_rect(window_width: f32) -> ButtonRect {
    let width = (window_width - ADDRESS_BAR_X - ADDRESS_BAR_MARGIN_RIGHT).max(0.0);
    ButtonRect { x: ADDRESS_BAR_X, y: BUTTON_Y, width, height: BUTTON_SIZE }
}

fn button_color(enabled: bool) -> [f32; 4] {
    if enabled {
        [0.30, 0.30, 0.35, 1.0]
    } else {
        [0.20, 0.20, 0.22, 1.0]
    }
}

/// Uma ação que um clique na barra de chrome pode disparar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChromeAction {
    Back,
    Forward,
    Reload,
    FocusAddressBar,
}

/// A barra de chrome inteira: campo de endereço editável + histórico de
/// navegação. `bar_height` é a altura total reservada no topo da janela
/// (inclui a margem acima/abaixo dos botões). `history` é `None` numa
/// aba recém-aberta que ainda não carregou nenhuma página.
pub struct Chrome {
    pub address_bar: AddressBar,
    pub history: Option<NavigationHistory>,
    pub bar_height: f32,
}

impl Chrome {
    pub fn new(initial: ferris_loader::Source, initial_text: String) -> Self {
        Self {
            address_bar: AddressBar::new(initial_text),
            history: Some(NavigationHistory::new(initial)),
            bar_height: BAR_HEIGHT,
        }
    }

    /// Uma aba recém-aberta, sem página carregada ainda: sem histórico,
    /// texto vazio, barra já focada esperando o usuário digitar.
    pub fn new_blank() -> Self {
        let mut address_bar = AddressBar::new(String::new());
        address_bar.set_focused(true);
        Self {
            address_bar,
            history: None,
            bar_height: BAR_HEIGHT,
        }
    }

    fn can_go_back(&self) -> bool {
        self.history.as_ref().map(NavigationHistory::can_go_back).unwrap_or(false)
    }

    fn can_go_forward(&self) -> bool {
        self.history.as_ref().map(NavigationHistory::can_go_forward).unwrap_or(false)
    }

    /// Converte coordenadas de clique (em pixels lógicos, sem escala de
    /// tela) numa ação, ou `None` se o clique caiu fora de qualquer
    /// elemento interativo da barra. Um botão desabilitado (ex: Voltar
    /// sem histórico, incluindo numa aba em branco sem histórico
    /// nenhum) nunca devolve uma ação, mesmo se o clique caiu
    /// exatamente em cima dele.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<ChromeAction> {
        if back_button_rect().contains(x, y) {
            return self.can_go_back().then_some(ChromeAction::Back);
        }
        if forward_button_rect().contains(x, y) {
            return self.can_go_forward().then_some(ChromeAction::Forward);
        }
        if reload_button_rect().contains(x, y) {
            return Some(ChromeAction::Reload);
        }
        if y >= BUTTON_Y && y < BUTTON_Y + BUTTON_SIZE && x >= ADDRESS_BAR_X {
            return Some(ChromeAction::FocusAddressBar);
        }
        None
    }

    /// Desenha o fundo da barra, os 3 botões (esmaecidos quando
    /// desabilitados), e o campo de endereço (mostrando o erro em
    /// vermelho quando houver um, em vez do texto normal).
    pub fn frame(&self, window_width: f32) -> scene::Frame {
        let mut frame = scene::Frame::new();

        frame.push(scene::DrawCommand::Rect(scene::RectCommand {
            x: 0.0, y: 0.0, width: window_width, height: self.bar_height,
            color: [0.15, 0.15, 0.18, 1.0], corner_radius: 0.0,
        }));

        let back = back_button_rect();
        frame.push(scene::DrawCommand::Rect(scene::RectCommand {
            x: back.x, y: back.y, width: back.width, height: back.height,
            color: button_color(self.can_go_back()), corner_radius: 4.0,
        }));
        frame.push(scene::DrawCommand::Text(scene::TextCommand {
            x: back.x + 10.0, y: back.y + 6.0, content: "<".to_string(), size: 18.0, color: [1.0, 1.0, 1.0, 1.0],
        }));

        let forward = forward_button_rect();
        frame.push(scene::DrawCommand::Rect(scene::RectCommand {
            x: forward.x, y: forward.y, width: forward.width, height: forward.height,
            color: button_color(self.can_go_forward()), corner_radius: 4.0,
        }));
        frame.push(scene::DrawCommand::Text(scene::TextCommand {
            x: forward.x + 10.0, y: forward.y + 6.0, content: ">".to_string(), size: 18.0, color: [1.0, 1.0, 1.0, 1.0],
        }));

        let reload = reload_button_rect();
        frame.push(scene::DrawCommand::Rect(scene::RectCommand {
            x: reload.x, y: reload.y, width: reload.width, height: reload.height,
            color: button_color(true), corner_radius: 4.0,
        }));
        frame.push(scene::DrawCommand::Text(scene::TextCommand {
            x: reload.x + 9.0, y: reload.y + 6.0, content: "R".to_string(), size: 18.0, color: [1.0, 1.0, 1.0, 1.0],
        }));

        let addr = address_bar_rect(window_width);
        frame.push(scene::DrawCommand::Rect(scene::RectCommand {
            x: addr.x, y: addr.y, width: addr.width, height: addr.height,
            color: [0.95, 0.95, 0.95, 1.0], corner_radius: 4.0,
        }));
        let (text, color) = match self.address_bar.error() {
            Some(msg) => (msg.to_string(), [0.8, 0.1, 0.1, 1.0]),
            None => (self.address_bar.text().to_string(), [0.05, 0.05, 0.05, 1.0]),
        };
        frame.push(scene::DrawCommand::Text(scene::TextCommand {
            x: addr.x + 8.0, y: addr.y + 6.0, content: text, size: 16.0, color,
        }));

        frame
    }
}

/// Desloca todo `RectCommand`/`TextCommand` de `frame` por `dy` no eixo
/// Y, preservando os demais campos. Usado pra empurrar o conteúdo da
/// página pra baixo da barra de chrome.
pub fn translate_frame(frame: &scene::Frame, dy: f32) -> scene::Frame {
    let commands = frame
        .commands
        .iter()
        .map(|cmd| match cmd {
            scene::DrawCommand::Rect(r) => scene::DrawCommand::Rect(scene::RectCommand { y: r.y + dy, ..*r }),
            scene::DrawCommand::Text(t) => scene::DrawCommand::Text(scene::TextCommand { y: t.y + dy, ..t.clone() }),
            scene::DrawCommand::Image(i) => scene::DrawCommand::Image(scene::ImageCommand { y: i.y + dy, ..i.clone() }),
        })
        .collect();
    scene::Frame { commands }
}

/// Texto de exibição de um `Source` na barra de endereço: o caminho do
/// arquivo, ou a URL, como uma `String` simples.
pub fn source_display_text(source: &ferris_loader::Source) -> String {
    match source {
        ferris_loader::Source::File(path) => path.display().to_string(),
        ferris_loader::Source::Url(url) => url.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(name: &str) -> ferris_loader::Source {
        ferris_loader::Source::File(PathBuf::from(name))
    }

    #[test]
    fn new_history_has_the_initial_entry_as_current() {
        let history = NavigationHistory::new(file("a.html"));
        assert_eq!(history.current(), &file("a.html"));
        assert!(!history.can_go_back());
        assert!(!history.can_go_forward());
    }

    #[test]
    fn go_navigates_forward_and_updates_current() {
        let mut history = NavigationHistory::new(file("a.html"));
        history.go(file("b.html"));
        assert_eq!(history.current(), &file("b.html"));
        assert!(history.can_go_back());
        assert!(!history.can_go_forward());
    }

    #[test]
    fn back_moves_to_the_previous_entry() {
        let mut history = NavigationHistory::new(file("a.html"));
        history.go(file("b.html"));
        let back = history.back().cloned();
        assert_eq!(back, Some(file("a.html")));
        assert_eq!(history.current(), &file("a.html"));
    }

    #[test]
    fn back_at_the_start_returns_none_and_does_not_move() {
        let mut history = NavigationHistory::new(file("a.html"));
        assert_eq!(history.back(), None);
        assert_eq!(history.current(), &file("a.html"));
    }

    #[test]
    fn forward_at_the_end_returns_none_and_does_not_move() {
        let mut history = NavigationHistory::new(file("a.html"));
        assert_eq!(history.forward(), None);
        assert_eq!(history.current(), &file("a.html"));
    }

    #[test]
    fn forward_after_back_returns_to_the_later_entry() {
        let mut history = NavigationHistory::new(file("a.html"));
        history.go(file("b.html"));
        history.back();
        let fwd = history.forward().cloned();
        assert_eq!(fwd, Some(file("b.html")));
        assert!(!history.can_go_forward());
    }

    #[test]
    fn go_after_back_discards_the_forward_history() {
        let mut history = NavigationHistory::new(file("a.html"));
        history.go(file("b.html"));
        history.back();
        history.go(file("c.html"));
        assert_eq!(history.current(), &file("c.html"));
        assert!(!history.can_go_forward(), "navigating after back must discard b.html from forward history");
        history.back();
        assert_eq!(history.current(), &file("a.html"), "the discarded b.html must not reappear");
    }

    #[test]
    fn address_bar_new_starts_unfocused_with_cursor_at_the_end() {
        let bar = AddressBar::new("hello".to_string());
        assert_eq!(bar.text(), "hello");
        assert!(!bar.is_focused());
        assert_eq!(bar.cursor(), 5);
        assert_eq!(bar.error(), None);
    }

    #[test]
    fn on_char_appends_to_empty_text() {
        let mut bar = AddressBar::new(String::new());
        bar.on_char('a');
        bar.on_char('b');
        assert_eq!(bar.text(), "ab");
        assert_eq!(bar.cursor(), 2);
    }

    #[test]
    fn on_char_inserts_at_the_cursor_after_existing_text() {
        let mut bar = AddressBar::new("ac".to_string());
        bar.on_char('b');
        assert_eq!(bar.text(), "acb");
    }

    #[test]
    fn on_backspace_removes_the_last_character() {
        let mut bar = AddressBar::new("abc".to_string());
        bar.on_backspace();
        assert_eq!(bar.text(), "ab");
        assert_eq!(bar.cursor(), 2);
    }

    #[test]
    fn on_backspace_on_empty_text_does_not_panic_or_underflow() {
        let mut bar = AddressBar::new(String::new());
        bar.on_backspace();
        assert_eq!(bar.text(), "");
        assert_eq!(bar.cursor(), 0);
    }

    #[test]
    fn on_char_and_on_backspace_handle_multi_byte_characters() {
        let mut bar = AddressBar::new(String::new());
        bar.on_char('é');
        bar.on_char('中');
        assert_eq!(bar.text(), "é中");
        bar.on_backspace();
        assert_eq!(bar.text(), "é");
        bar.on_backspace();
        assert_eq!(bar.text(), "");
    }

    #[test]
    fn on_char_clears_a_previous_error() {
        let mut bar = AddressBar::new("a.html".to_string());
        bar.set_error(Some("not found".to_string()));
        bar.on_char('x');
        assert_eq!(bar.error(), None);
    }

    #[test]
    fn on_backspace_clears_a_previous_error() {
        let mut bar = AddressBar::new("abc".to_string());
        bar.set_error(Some("not found".to_string()));
        bar.on_backspace();
        assert_eq!(bar.error(), None);
    }

    #[test]
    fn set_focused_true_clears_a_previous_error() {
        let mut bar = AddressBar::new("a.html".to_string());
        bar.set_error(Some("not found".to_string()));
        bar.set_focused(true);
        assert_eq!(bar.error(), None);
    }

    #[test]
    fn commit_with_empty_text_returns_none() {
        let bar = AddressBar::new(String::new());
        assert_eq!(bar.commit(), None);
    }

    #[test]
    fn commit_with_whitespace_only_text_returns_none() {
        let bar = AddressBar::new("   ".to_string());
        assert_eq!(bar.commit(), None);
    }

    #[test]
    fn commit_with_text_returns_the_parsed_source() {
        let bar = AddressBar::new("https://example.com".to_string());
        assert_eq!(bar.commit(), Some(ferris_loader::parse_source("https://example.com")));
    }

    #[test]
    fn cancel_reverts_text_clears_focus_and_error() {
        let mut bar = AddressBar::new("a.html".to_string());
        bar.set_focused(true);
        bar.on_char('x');
        bar.set_error(Some("boom".to_string()));
        bar.cancel("a.html");
        assert_eq!(bar.text(), "a.html");
        assert!(!bar.is_focused());
        assert_eq!(bar.error(), None);
    }

    #[test]
    fn set_error_then_clear_error() {
        let mut bar = AddressBar::new(String::new());
        bar.set_error(Some("not found".to_string()));
        assert_eq!(bar.error(), Some("not found"));
        bar.set_error(None);
        assert_eq!(bar.error(), None);
    }

    fn chrome_with_history(navigate_once: bool) -> Chrome {
        let mut chrome = Chrome::new(file("a.html"), "a.html".to_string());
        if navigate_once {
            chrome.history.as_mut().unwrap().go(file("b.html"));
        }
        chrome
    }

    #[test]
    fn hit_test_back_button_when_enabled() {
        let chrome = chrome_with_history(true);
        let back = back_button_rect();
        assert_eq!(chrome.hit_test(back.x + 1.0, back.y + 1.0), Some(ChromeAction::Back));
    }

    #[test]
    fn hit_test_back_button_when_disabled_returns_none() {
        let chrome = chrome_with_history(false);
        let back = back_button_rect();
        assert_eq!(chrome.hit_test(back.x + 1.0, back.y + 1.0), None);
    }

    #[test]
    fn hit_test_forward_button_when_disabled_returns_none() {
        let chrome = chrome_with_history(false);
        let forward = forward_button_rect();
        assert_eq!(chrome.hit_test(forward.x + 1.0, forward.y + 1.0), None);
    }

    #[test]
    fn hit_test_forward_button_when_enabled_after_back() {
        let mut chrome = chrome_with_history(true);
        chrome.history.as_mut().unwrap().back();
        let forward = forward_button_rect();
        assert_eq!(chrome.hit_test(forward.x + 1.0, forward.y + 1.0), Some(ChromeAction::Forward));
    }

    #[test]
    fn hit_test_reload_button_always_enabled() {
        let chrome = chrome_with_history(false);
        let reload = reload_button_rect();
        assert_eq!(chrome.hit_test(reload.x + 1.0, reload.y + 1.0), Some(ChromeAction::Reload));
    }

    #[test]
    fn hit_test_address_bar_returns_focus_action() {
        let chrome = chrome_with_history(false);
        assert_eq!(chrome.hit_test(ADDRESS_BAR_X + 5.0, BUTTON_Y + 1.0), Some(ChromeAction::FocusAddressBar));
    }

    #[test]
    fn hit_test_below_the_bar_returns_none() {
        let chrome = chrome_with_history(false);
        assert_eq!(chrome.hit_test(ADDRESS_BAR_X + 5.0, BAR_HEIGHT + 5.0), None);
    }

    #[test]
    fn hit_test_in_the_margin_above_the_buttons_returns_none() {
        let chrome = chrome_with_history(false);
        assert_eq!(chrome.hit_test(BACK_X + 1.0, 0.0), None, "the margin above the buttons must not be clickable");
    }

    #[test]
    fn hit_test_one_pixel_left_of_the_back_button_returns_none() {
        let chrome = chrome_with_history(true);
        assert_eq!(chrome.hit_test(BACK_X - 1.0, BUTTON_Y + 1.0), None);
    }

    #[test]
    fn hit_test_at_the_last_pixel_of_the_back_button_still_hits() {
        let chrome = chrome_with_history(true);
        assert_eq!(chrome.hit_test(BACK_X + BUTTON_SIZE - 1.0, BUTTON_Y + 1.0), Some(ChromeAction::Back));
    }

    #[test]
    fn translate_frame_shifts_rect_and_text_y_by_dy() {
        let mut frame = scene::Frame::new();
        frame.push(scene::DrawCommand::Rect(scene::RectCommand {
            x: 1.0, y: 2.0, width: 3.0, height: 4.0, color: [1.0, 0.0, 0.0, 1.0], corner_radius: 5.0,
        }));
        frame.push(scene::DrawCommand::Text(scene::TextCommand {
            x: 6.0, y: 7.0, content: "hi".to_string(), size: 8.0, color: [0.0, 1.0, 0.0, 1.0],
        }));

        let translated = translate_frame(&frame, 10.0);

        let scene::DrawCommand::Rect(r) = &translated.commands[0] else { panic!("expected rect") };
        assert_eq!(r.y, 12.0);
        assert_eq!(r.x, 1.0);
        assert_eq!(r.width, 3.0);
        assert_eq!(r.height, 4.0);
        assert_eq!(r.corner_radius, 5.0);

        let scene::DrawCommand::Text(t) = &translated.commands[1] else { panic!("expected text") };
        assert_eq!(t.y, 17.0);
        assert_eq!(t.x, 6.0);
        assert_eq!(t.content, "hi");
        assert_eq!(t.size, 8.0);
    }

    #[test]
    fn translate_frame_on_empty_frame_returns_empty_frame() {
        let frame = scene::Frame::new();
        let translated = translate_frame(&frame, 5.0);
        assert!(translated.commands.is_empty());
    }

    #[test]
    fn source_display_text_for_file_shows_the_path() {
        let text = source_display_text(&file("pages/a.html"));
        assert_eq!(text, PathBuf::from("pages/a.html").display().to_string());
    }

    #[test]
    fn source_display_text_for_url_shows_the_url() {
        let text = source_display_text(&ferris_loader::Source::Url("https://example.com".to_string()));
        assert_eq!(text, "https://example.com");
    }

    #[test]
    fn chrome_new_blank_has_no_history_and_is_focused() {
        let chrome = Chrome::new_blank();
        assert!(chrome.history.is_none());
        assert!(chrome.address_bar.is_focused());
        assert_eq!(chrome.address_bar.text(), "");
    }

    #[test]
    fn hit_test_back_and_forward_on_blank_chrome_return_none() {
        let chrome = Chrome::new_blank();
        let back = back_button_rect();
        assert_eq!(chrome.hit_test(back.x + 1.0, back.y + 1.0), None);
        let forward = forward_button_rect();
        assert_eq!(chrome.hit_test(forward.x + 1.0, forward.y + 1.0), None);
    }

    #[test]
    fn frame_on_blank_chrome_does_not_panic() {
        let chrome = Chrome::new_blank();
        let frame = chrome.frame(800.0);
        assert!(!frame.commands.is_empty());
    }
}
