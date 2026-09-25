//! UI de navegação (chrome) do ferris-compositor: barra de endereço,
//! histórico de navegação, e desenho da barra como `scene::DrawCommand`s.
//! Nenhuma dependência de UI externa — usa só as primitivas de
//! `ferris_scene` já existentes.

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
    }

    pub fn on_char(&mut self, c: char) {
        let byte_idx = Self::byte_index(&self.text, self.cursor);
        self.text.insert(byte_idx, c);
        self.cursor += 1;
    }

    pub fn on_backspace(&mut self) {
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
}
