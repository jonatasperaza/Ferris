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
}
