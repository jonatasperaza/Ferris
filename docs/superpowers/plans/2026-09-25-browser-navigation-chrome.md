# Browser Navigation Chrome (2.9) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a navigation chrome (address bar, Back/Forward/Reload, in-memory history, keyboard shortcuts) to `ferris-compositor`, so the window can navigate between real pages instead of showing one fixed page for the process's whole lifetime.

**Architecture:** A new `ferris-compositor/src/chrome.rs` module holds all the pure, GPU-free UI logic (`NavigationHistory`, `AddressBar`, `Chrome`, hit-testing, drawing as `scene::DrawCommand`s, frame translation) built entirely on the existing `RectCommand`/`TextCommand` primitives — no new dependency. `main.rs` wires winit keyboard/mouse events into it and unifies the "no CLI argument" fixture path with the real `ferris_loader::load_page` path (the fixture becomes a real file on disk, loaded the same way as any other page).

**Tech Stack:** Rust, winit 0.30 (`ApplicationHandler`, `WindowEvent::{KeyboardInput,MouseInput,CursorMoved,ModifiersChanged}`, `winit::keyboard::{Key,NamedKey,ModifiersState}`), existing `ferris-scene`/`ferris-loader` types. No new crates, no new external dependencies.

**Spec:** `docs/superpowers/specs/2026-09-25-browser-navigation-chrome-design.md`

## Global Constraints

- Everything lives inside `ferris-compositor` (new module `chrome.rs`) — no new crate, no new external dependency (spec's Arquitetura section).
- Address bar: click focuses it, typing edits it, Enter navigates, Esc reverts to the current page's text and defocuses (spec's Objetivo/Componentes).
- Back/Forward/Reload buttons are clickable; Back/Forward are only "live" (return an action from `hit_test`) when the history actually allows moving that direction (spec's Componentes).
- Keyboard shortcuts: Ctrl+L focuses the address bar, Alt+Left/Alt+Right go back/forward, F5 reloads — all work regardless of whether the address bar is currently focused (spec's Objetivo).
- Page content is never hidden behind the bar: the page is laid out with `viewport_height - bar_height`, and translated down by `bar_height` at render time (spec's Arquitetura, approved during brainstorming).
- A failure loading the **initial** page (from the CLI argument, or the default fixture path) still exits the process with `std::process::exit(1)` — unchanged from sub-project 2.8. A failure during **interactive** navigation (Enter, Back, Forward, Reload after the window is already open) never exits the process — it sets a visible error on the address bar instead (spec's Tratamento de erro).
- No tabs in this sub-project (explicitly deferred to a future 2.10, decided during brainstorming's scope cut).

## Review Focus

- Navigating (typing a URL, or clicking Back/Forward/Reload) to something that fails to load must never crash or exit the running window — it must show the error on the address bar and leave the previous page visible. → Task 4.
- Going Back and then navigating to something new must correctly discard the "forward" history, so Forward does not resurrect a page the user has already navigated away from a second time. → Task 1.
- Editing the address bar must never panic on an empty field (Backspace with nothing to delete) and must never corrupt multi-byte text (accented letters, CJK characters) when inserting or deleting one character at a time. → Task 2.
- A click exactly on the boundary of a button, and a click on a disabled button (Back with no earlier history), must be handled precisely — one pixel outside a button must miss it, and a disabled button must never fire its action even when clicked dead-center. → Task 3.
- The three keyboard shortcuts (F5, Alt+Left/Right, Ctrl+L) must work regardless of whether the address bar is currently focused, and Ctrl+L specifically must never leak the literal character "l" into the address bar's text. → Task 4.

---

### Task 1: `NavigationHistory` (in-memory navigation stack)

**Files:**
- Create: `ferris-compositor/src/chrome.rs`
- Modify: `ferris-compositor/src/lib.rs`

**Interfaces:**
- Consumes: `ferris_loader::Source` (existing, from sub-project 2.8 — `#[derive(Debug, Clone, PartialEq)] pub enum Source { File(PathBuf), Url(String) }`).
- Produces: `pub struct NavigationHistory` with `pub fn new(initial: ferris_loader::Source) -> Self`, `pub fn go(&mut self, source: ferris_loader::Source)`, `pub fn back(&mut self) -> Option<&ferris_loader::Source>`, `pub fn forward(&mut self) -> Option<&ferris_loader::Source>`, `pub fn current(&self) -> &ferris_loader::Source`, `pub fn can_go_back(&self) -> bool`, `pub fn can_go_forward(&self) -> bool` — consumed by Task 3 (`Chrome` struct) and Task 4 (`main.rs` wiring).

- [ ] **Step 1: Register the new module**

Modify `ferris-compositor/src/lib.rs` — it currently reads:
```rust
pub mod perf;
pub mod renderer;
pub mod scene;
```
Change to:
```rust
pub mod chrome;
pub mod perf;
pub mod renderer;
pub mod scene;
```

- [ ] **Step 2: Write the failing tests**

Create `ferris-compositor/src/chrome.rs` with this content:
```rust
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
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --package ferris-compositor chrome::`
Expected: `test result: ok. 7 passed; 0 failed`

- [ ] **Step 4: Run the whole workspace build**

Run: `cargo build --workspace`
Expected: clean build. (Note: `NavigationHistory` is not yet used outside its own tests — this is expected at this point in the plan, not a defect; it's consumed starting in Task 3.)

- [ ] **Step 5: Commit**

```bash
git add ferris-compositor/src/chrome.rs ferris-compositor/src/lib.rs
git commit -m "feat: add NavigationHistory, an in-memory back/forward navigation stack"
```

---

### Task 2: `AddressBar` (editable text field state)

**Files:**
- Modify: `ferris-compositor/src/chrome.rs`

**Interfaces:**
- Consumes: `ferris_loader::{Source, parse_source}` (existing).
- Produces: `pub struct AddressBar` with `pub fn new(initial_text: String) -> Self`, `pub fn text(&self) -> &str`, `pub fn is_focused(&self) -> bool`, `pub fn cursor(&self) -> usize`, `pub fn error(&self) -> Option<&str>`, `pub fn set_focused(&mut self, focused: bool)`, `pub fn on_char(&mut self, c: char)`, `pub fn on_backspace(&mut self)`, `pub fn commit(&self) -> Option<ferris_loader::Source>`, `pub fn set_text(&mut self, text: &str)`, `pub fn cancel(&mut self, reset_text: &str)`, `pub fn set_error(&mut self, message: Option<String>)` — consumed by Task 3 (`Chrome` struct) and Task 4 (`main.rs` wiring).

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block at the bottom of `ferris-compositor/src/chrome.rs` (right after `go_after_back_discards_the_forward_history`'s closing `}`, still inside `mod tests { ... }`):
```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --package ferris-compositor chrome::`
Expected: FAIL to compile — `AddressBar` does not exist yet.

- [ ] **Step 3: Implement `AddressBar`**

Add this above the `#[cfg(test)]` line in `ferris-compositor/src/chrome.rs`:
```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --package ferris-compositor chrome::`
Expected: `test result: ok. 18 passed; 0 failed` (7 from Task 1 + 11 new).

- [ ] **Step 5: Commit**

```bash
git add ferris-compositor/src/chrome.rs
git commit -m "feat: add AddressBar, the editable address-bar text field state"
```

---

### Task 3: `Chrome`, hit-testing, drawing, and frame translation

**Files:**
- Modify: `ferris-compositor/src/chrome.rs`

**Interfaces:**
- Consumes: `NavigationHistory` (Task 1), `AddressBar` (Task 2), `ferris_loader::Source`, `scene::{Frame, DrawCommand, RectCommand, TextCommand}` (existing, re-exported from `ferris-compositor/src/scene.rs`).
- Produces: `pub enum ChromeAction { Back, Forward, Reload, FocusAddressBar }` (`Debug, Clone, Copy, PartialEq`); `pub struct Chrome { pub address_bar: AddressBar, pub history: NavigationHistory, pub bar_height: f32 }` with `pub fn new(initial: ferris_loader::Source, initial_text: String) -> Self`, `pub fn hit_test(&self, x: f32, y: f32) -> Option<ChromeAction>`, `pub fn frame(&self, window_width: f32) -> scene::Frame`; free functions `pub fn translate_frame(frame: &scene::Frame, dy: f32) -> scene::Frame` and `pub fn source_display_text(source: &ferris_loader::Source) -> String` — all consumed by Task 4 (`main.rs` wiring).

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block at the bottom of `ferris-compositor/src/chrome.rs` (after `set_error_then_clear_error`'s closing `}`, still inside `mod tests { ... }`):
```rust
    fn chrome_with_history(navigate_once: bool) -> Chrome {
        let mut chrome = Chrome::new(file("a.html"), "a.html".to_string());
        if navigate_once {
            chrome.history.go(file("b.html"));
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
        chrome.history.back();
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --package ferris-compositor chrome::`
Expected: FAIL to compile — `Chrome`, `ChromeAction`, `translate_frame`, `source_display_text`, `back_button_rect` etc. do not exist yet.

- [ ] **Step 3: Implement `Chrome` and its supporting geometry/drawing code**

Add this above the `#[cfg(test)]` line in `ferris-compositor/src/chrome.rs` (after `AddressBar`'s `impl` block):
```rust
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
/// (inclui a margem acima/abaixo dos botões).
pub struct Chrome {
    pub address_bar: AddressBar,
    pub history: NavigationHistory,
    pub bar_height: f32,
}

impl Chrome {
    pub fn new(initial: ferris_loader::Source, initial_text: String) -> Self {
        Self {
            address_bar: AddressBar::new(initial_text),
            history: NavigationHistory::new(initial),
            bar_height: BAR_HEIGHT,
        }
    }

    /// Converte coordenadas de clique (em pixels lógicos, sem escala de
    /// tela) numa ação, ou `None` se o clique caiu fora de qualquer
    /// elemento interativo da barra. Um botão desabilitado (ex: Voltar
    /// sem histórico anterior) nunca devolve uma ação, mesmo se o
    /// clique caiu exatamente em cima dele.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<ChromeAction> {
        if back_button_rect().contains(x, y) {
            return self.history.can_go_back().then_some(ChromeAction::Back);
        }
        if forward_button_rect().contains(x, y) {
            return self.history.can_go_forward().then_some(ChromeAction::Forward);
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
            color: button_color(self.history.can_go_back()), corner_radius: 4.0,
        }));
        frame.push(scene::DrawCommand::Text(scene::TextCommand {
            x: back.x + 10.0, y: back.y + 6.0, content: "<".to_string(), size: 18.0, color: [1.0, 1.0, 1.0, 1.0],
        }));

        let forward = forward_button_rect();
        frame.push(scene::DrawCommand::Rect(scene::RectCommand {
            x: forward.x, y: forward.y, width: forward.width, height: forward.height,
            color: button_color(self.history.can_go_forward()), corner_radius: 4.0,
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --package ferris-compositor chrome::`
Expected: `test result: ok. 32 passed; 0 failed` (18 from Tasks 1-2 + 14 new).

- [ ] **Step 5: Run the whole workspace test suite**

Run: `cargo test --workspace`
Expected: 277 passed (245 baseline + 32 from `chrome.rs` across Tasks 1-3).

- [ ] **Step 6: Commit**

```bash
git add ferris-compositor/src/chrome.rs
git commit -m "feat: add Chrome (hit-testing, drawing, frame translation, source display text)"
```

---

### Task 4: Wire the chrome into `main.rs`, unify the fixture path, and verify manually

**Files:**
- Modify: `ferris-compositor/assets/fixture.html`
- Modify: `ferris-compositor/src/renderer/mod.rs`
- Modify: `ferris-compositor/src/main.rs`

**Interfaces:**
- Consumes: `chrome::{Chrome, ChromeAction, translate_frame, source_display_text}` (Tasks 1-3), `ferris_loader::{Source, LoadError, parse_source, load_page}` (sub-project 2.8), `ferris_dom::parser::parse_document` (sub-project 2.8).
- Produces: nothing consumed by later tasks — this is the final task of the plan.

- [ ] **Step 1: Unify the fixture with the real page-loading path**

Modify `ferris-compositor/assets/fixture.html` — it currently starts with:
```html
<div id="page">
```
Add one line before it, so the file starts:
```html
<link rel="stylesheet" href="fixture.css">
<div id="page">
```
(The rest of the file is unchanged.) This makes the fixture discoverable by `ferris_loader::load_page`'s existing `<link rel="stylesheet">` extraction, so the fixture no longer needs its own special-cased loading code in `main.rs` — it becomes just another `Source::File` like any real page.

- [ ] **Step 2: Add a public scale-factor getter to `Renderer`**

Modify `ferris-compositor/src/renderer/mod.rs` — find the existing `pub fn logical_width(&self) -> f32 { ... }` method and add this new method right after it (and after `logical_height`):
```rust
    /// The display's scale factor, e.g. `1.0` on a standard-DPI display or
    /// `1.5`/`2.0` on a HiDPI one. Used to convert a physical cursor
    /// position (from winit) into the logical-pixel coordinate space that
    /// `Chrome::hit_test` and `Chrome::frame` operate in.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }
```

- [ ] **Step 3: Run a build to confirm the getter compiles**

Run: `cargo build --package ferris-compositor`
Expected: clean build (the new method isn't called yet).

- [ ] **Step 4: Replace `ferris-compositor/src/main.rs` in full**

Read the current file first to confirm it still matches the state left by sub-project 2.8 (it should have an `App` struct with a `source: Option<ferris_loader::Source>` field, a `build_page_frame` function taking `&Element`/`&Stylesheet`, and one existing test `build_page_frame_on_display_none_root_returns_empty_frame_without_panicking`). Then replace the entire file with:
```rust
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
    fn navigate_interactive(&mut self, source: ferris_loader::Source) {
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
            }
            Err(ferris_loader::LoadError::Fetch(msg)) => {
                log::warn!("navigation failed: {msg}");
                if let Some(chrome) = self.chrome.as_mut() {
                    chrome.address_bar.set_error(Some(msg));
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
                KeyIntent::Commit => {
                    let committed = self.chrome.as_mut().and_then(|c| c.address_bar.commit());
                    if let Some(source) = committed {
                        if let Some(chrome) = self.chrome.as_mut() {
                            chrome.history.go(source.clone());
                        }
                        self.navigate_interactive(source);
                    }
                }
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
    if modifiers.control_key() {
        if let Key::Character(s) = logical_key {
            if s.as_str().eq_ignore_ascii_case("l") {
                return vec![KeyIntent::FocusAddressBar];
            }
        }
        return vec![KeyIntent::Ignore];
    }
    if modifiers.alt_key() {
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
}
```

Note on a borrow-checker subtlety in `handle_keyboard_input`'s `KeyIntent::Commit` arm: `self.chrome.as_mut().and_then(|c| c.address_bar.commit())` is deliberately written to finish and drop its borrow of `self.chrome` *before* `self.navigate_interactive(source)` is called (which needs a fresh `&mut self`) — `commit()` returns an owned `Source`, not a reference, so nothing borrowed survives past that line. If `cargo build` reports a borrow-checker error anywhere in this file, the fix is the same pattern: finish extracting owned values out of a `self.field.as_mut()` borrow before calling another `self.method(...)`, never call a `&mut self` method while such a borrow is still in scope.

- [ ] **Step 5: Run the workspace build**

Run: `cargo build --workspace`
Expected: clean build, no warnings.

- [ ] **Step 6: Run the workspace test suite**

Run: `cargo test --workspace`
Expected: 285 passed (277 from Task 3 + 8 new: `navigate_interactive_on_failure_...` + 7 `classify_key_...` tests; the pre-existing `build_page_frame_on_display_none_root_...` test is carried over unchanged, not new).

- [ ] **Step 7: Manual verification — no CLI argument (fixture, now loaded via `ferris_loader`, still renders)**

Run: `cargo run --release -p ferris-compositor` (no argument) from the **workspace root** (the default fixture path is relative to the current working directory).

Confirm, by looking at the window (or via the screenshot technique established in pieces 2.4-2.8: `Start-Process`/background run + `GetWindowRect`+`CopyFromScreen` via PowerShell, `SetProcessDpiAwarenessContext` if the capture comes out scaled wrong):
- A chrome bar appears at the top: dark background, `<`/`>`/`R` buttons (Back/Forward dimmed since there's no history yet), and an address bar showing a path ending in `ferris-compositor/assets/fixture.html`.
- Below the bar, the exact same fixture page pieces 2.5-2.8 already verified (white background, navy header, wrapped paragraph, colored inline text) — now starting below the bar, not overlapping it.

- [ ] **Step 8: Manual verification — typing a URL and navigating**

With the window from Step 7 still open: click the address bar (confirm it visually indicates focus, e.g. a cursor or highlighted border if you added one — a plain focus state with no extra visual cue is also acceptable, the important thing is that typing works), select all the existing text isn't required since typing will insert at wherever the cursor sits (this AddressBar's cursor always tracks the end of the text per this plan's design) — clear the field with Backspace, type a real URL such as `https://example.com`, press Enter.

Confirm: the page changes to show real content fetched from that URL (same as sub-project 2.8's own manual verification of a real URL), the address bar now shows `https://example.com`, and the Back button is now enabled (no longer dimmed).

- [ ] **Step 9: Manual verification — Back, Forward, Reload buttons and keyboard shortcuts**

With the window still open after Step 8:
- Click the Back button (`<`). Confirm the fixture page reappears, and the address bar's text reverts to the fixture path.
- Click the Forward button (`>`). Confirm `https://example.com`'s content reappears.
- Click Reload (`R`). Confirm the page reloads (no visible error, same content).
- Press F5. Confirm the same reload behavior.
- Press Alt+Left. Confirm it goes back to the fixture, same as clicking Back.
- Press Alt+Right. Confirm it goes forward again, same as clicking Forward.
- Press Ctrl+L. Confirm the address bar becomes focused (and does **not** have the literal character "l" inserted into its text — this is the specific bug this plan's Review Focus flagged).

- [ ] **Step 10: Manual verification — a failed navigation does not crash the window**

With the window still open: focus the address bar (click it or Ctrl+L), clear it, type a path to a file that does not exist (e.g. `this-file-does-not-exist-anywhere.html`), press Enter.

Confirm: the window stays open (does not close, does not panic — check the terminal for a clean absence of a panic backtrace), the address bar shows a visible error message in place of the URL text, and the previously-loaded page content is still visible below the bar (unchanged). Then navigate to a real URL or Back to confirm the browser is still fully functional after the error — the process must not have exited.

Close the window once all of Steps 7-10 are confirmed. Note in the task's completion report exactly what was seen at each step — a task reviewer for this task must ask the implementer to describe what appeared for all four manual-verification steps, and treat an implementer that skipped any of them as not having completed the task's actual deliverable, same discipline as every prior sub-project's final GUI task in this project.

- [ ] **Step 11: Commit**

```bash
git add ferris-compositor/assets/fixture.html ferris-compositor/src/renderer/mod.rs ferris-compositor/src/main.rs
git commit -m "feat: add navigation chrome (address bar, back/forward/reload, keyboard shortcuts)"
```

---
