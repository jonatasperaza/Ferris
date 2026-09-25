# Browser Tabs (2.10) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add multiple tabs to `ferris-compositor`, each with its own independent address bar, navigation history, and cached page content, controlled by a new tab strip (click, "+", "x", Ctrl+T/Ctrl+W/Ctrl+Tab).

**Architecture:** `App`'s single `Chrome`/`page_frame` (from 2.9) become `tabs: Vec<Tab>` + `active_tab: usize`, where each `Tab` bundles a `Chrome` and its own cached `page_frame`. `Chrome::history` changes from `NavigationHistory` to `Option<NavigationHistory>` so a freshly-opened blank tab can exist with no history at all until its first navigation. A new module `ferris-compositor/src/tabs.rs` (same pattern as `chrome.rs`: pure logic + `scene::DrawCommand` drawing, no external UI dependency) draws a fixed-width tab strip above the existing chrome bar.

**Tech Stack:** Rust, winit 0.30 (same event types already used in `main.rs`), existing `ferris-scene`/`ferris-loader`/`chrome` types. No new crates, no new external dependencies.

**Spec:** `docs/superpowers/specs/2026-09-25-browser-tabs-design.md`

## Global Constraints

- `App` holds `tabs: Vec<Tab>` + `active_tab: usize` instead of a single `Chrome`/`page_frame` pair; `Tab { chrome: Chrome, page_frame: Option<scene::Frame> }` (spec's Arquitetura).
- `Chrome::history` changes from `NavigationHistory` to `Option<NavigationHistory>`; `Chrome::new_blank()` is a new constructor for a tab with no page loaded — no history, address bar starts empty and already focused (spec's decision: new tabs open blank with the bar auto-focused).
- A new tab is always appended at the end of `tabs` and immediately becomes the active tab (spec's Fluxo de dados).
- Closing a tab adjusts `active_tab` to keep pointing at a valid tab; closing the last remaining tab exits the window (spec's decision).
- Switching tabs never reloads anything — the target tab's cached `page_frame` is reused directly (spec's Fluxo de dados).
- Shortcuts Ctrl+T (new tab), Ctrl+W (close the active tab), Ctrl+Tab (next tab, wrapping to the first after the last) all work regardless of whether the address bar is focused, and must never leak "t"/"w" as typed text (spec's decision, same discipline as 2.9's Ctrl+L/AltGr fix).
- The tab strip uses a FIXED width per tab (not dependent on window width) with an embedded close "x" in each tab and a "+" button right after the last tab — `TabStrip::hit_test` therefore does not need `window_width` (spec's Componentes, same simplification precedent as `Chrome::hit_test` from 2.9).
- Interactive navigation failure never exits the process, per-tab, same model as 2.9 (spec's Tratamento de erro).
- No new crate, no new external dependency (spec's Arquitetura).

## Review Focus

- Closing a tab in the middle (not the first, not the last) must leave the correct tab active — never an out-of-bounds index, never the wrong tab. → Task 3.
- Navigating for the first time from a blank tab (`history: None`) must correctly create the history rather than trying to call `.go()` on a history that doesn't exist yet (which would panic on an `Option::unwrap()` if done carelessly). → Task 1 + Task 3.
- A click exactly on the boundary between a tab's body and its embedded close "x" must fire the right action (`Select` vs. `Close`), and a click outside every tab and the "+" button must do nothing. → Task 2.
- Closing the only open tab must be detectable without indexing into an empty `Vec` out of bounds. → Task 3.
- Ctrl+T/Ctrl+W/Ctrl+Tab must work regardless of whether the address bar is focused, and must never leak "t"/"w" as typed characters into the address bar. → Task 3.

---

### Task 1: `Chrome::new_blank` and `history: Option<NavigationHistory>`

**Files:**
- Modify: `ferris-compositor/src/chrome.rs`

**Interfaces:**
- Consumes: `NavigationHistory`, `AddressBar` (both already in this file, unchanged).
- Produces: `Chrome.history` is now `Option<NavigationHistory>` (was `NavigationHistory`); `pub fn Chrome::new_blank() -> Self` — consumed by Task 3 (`main.rs`'s `new_tab`/`resumed`/navigation methods, all of which now handle `Option<NavigationHistory>`).

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block at the bottom of `ferris-compositor/src/chrome.rs` (after `source_display_text_for_url_shows_the_url`'s closing `}`, still inside `mod tests { ... }`):
```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --package ferris-compositor chrome::`
Expected: FAIL to compile — `Chrome::new_blank` does not exist yet, and `chrome.history.is_none()` doesn't type-check against a bare `NavigationHistory`.

- [ ] **Step 3: Change `Chrome.history` to `Option<NavigationHistory>` and add `new_blank`**

Replace the entire `Chrome` struct and its `impl` block (currently the section starting at `pub struct Chrome {` through the end of `impl Chrome { ... }`, right before the `translate_frame` free function) with:
```rust
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
```
(Only 3 things actually changed from the current code: the `history` field's type, the new `new_blank` constructor, and the two calls that used to say `self.history.can_go_back()`/`self.history.can_go_forward()` directly now go through the new private `self.can_go_back()`/`self.can_go_forward()` helpers, which centralize the `Option` handling in one place instead of repeating `self.history.as_ref().map(...).unwrap_or(false)` four times across `hit_test` and `frame`.)

- [ ] **Step 4: Fix the existing tests that construct history directly**

The existing `mod tests` block has two places that touch `chrome.history` as if it were a bare `NavigationHistory`, which no longer compiles now that it's `Option<NavigationHistory>`. Find and fix both:

In `chrome_with_history` (used by several `hit_test_*` tests), change:
```rust
    fn chrome_with_history(navigate_once: bool) -> Chrome {
        let mut chrome = Chrome::new(file("a.html"), "a.html".to_string());
        if navigate_once {
            chrome.history.go(file("b.html"));
        }
        chrome
    }
```
to:
```rust
    fn chrome_with_history(navigate_once: bool) -> Chrome {
        let mut chrome = Chrome::new(file("a.html"), "a.html".to_string());
        if navigate_once {
            chrome.history.as_mut().unwrap().go(file("b.html"));
        }
        chrome
    }
```

In `hit_test_forward_button_when_enabled_after_back`, change:
```rust
    #[test]
    fn hit_test_forward_button_when_enabled_after_back() {
        let mut chrome = chrome_with_history(true);
        chrome.history.back();
        let forward = forward_button_rect();
        assert_eq!(chrome.hit_test(forward.x + 1.0, forward.y + 1.0), Some(ChromeAction::Forward));
    }
```
to:
```rust
    #[test]
    fn hit_test_forward_button_when_enabled_after_back() {
        let mut chrome = chrome_with_history(true);
        chrome.history.as_mut().unwrap().back();
        let forward = forward_button_rect();
        assert_eq!(chrome.hit_test(forward.x + 1.0, forward.y + 1.0), Some(ChromeAction::Forward));
    }
```
No other existing test touches `.history` directly (the rest go through `chrome.hit_test(...)`, which is unaffected).

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --package ferris-compositor chrome::`
Expected: `test result: ok. 38 passed; 0 failed` (35 pre-existing + 3 new).

- [ ] **Step 6: Run the whole workspace build**

Run: `cargo build --workspace`
Expected: clean build. (`Chrome::new_blank` is not yet called from `main.rs` at this point in the plan — expected, it's wired up in Task 3.)

- [ ] **Step 7: Commit**

```bash
git add ferris-compositor/src/chrome.rs
git commit -m "feat: add Chrome::new_blank and make Chrome.history Option<NavigationHistory>"
```

---

### Task 2: `TabStrip` (fixed-width tab hit-testing and drawing)

**Files:**
- Create: `ferris-compositor/src/tabs.rs`
- Modify: `ferris-compositor/src/lib.rs`

**Interfaces:**
- Consumes: `crate::scene::{Frame, DrawCommand, RectCommand, TextCommand}` (existing, same pattern as `chrome.rs`).
- Produces: `pub enum TabAction { Select(usize), Close(usize), New }` (`Debug, Clone, Copy, PartialEq`); `pub struct TabStrip { pub height: f32 }` with `pub fn TabStrip::new() -> Self`, `pub fn hit_test(&self, tab_count: usize, x: f32, y: f32) -> Option<TabAction>`, `pub fn frame(&self, titles: &[String], active: usize, window_width: f32) -> scene::Frame` — all consumed by Task 3 (`main.rs` wiring).

- [ ] **Step 1: Register the new module**

Modify `ferris-compositor/src/lib.rs` — it currently reads:
```rust
pub mod chrome;
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
pub mod tabs;
```

- [ ] **Step 2: Write the failing tests**

Create `ferris-compositor/src/tabs.rs` with this content:
```rust
//! Tira de abas do ferris-compositor: uma aba por página aberta, com
//! largura FIXA (não depende da largura da janela), um "x" de fechar
//! embutido em cada aba, e um botão "+" logo depois da última. Mesmo
//! padrão de `chrome.rs`: lógica pura + desenho via `scene::DrawCommand`,
//! sem dependência de UI externa.

use crate::scene;

const TAB_STRIP_HEIGHT: f32 = 32.0;
const TAB_WIDTH: f32 = 160.0;
const TAB_CLOSE_BUTTON_WIDTH: f32 = 20.0;
const NEW_TAB_BUTTON_WIDTH: f32 = 32.0;
const TITLE_MAX_CHARS: usize = 18;

/// Trunca `title` pra no máximo `max_chars` caracteres, adicionando "…"
/// quando corta (aproximação por contagem de caracteres, não pixel a
/// pixel — este projeto ainda não tem medição de fonte disponível fora
/// de `ferris-text`, que `tabs.rs` não depende).
fn truncate_title(title: &str, max_chars: usize) -> String {
    let char_count = title.chars().count();
    if char_count <= max_chars {
        title.to_string()
    } else {
        let truncated: String = title.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}…")
    }
}

/// Uma ação que um clique na tira de abas pode disparar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TabAction {
    Select(usize),
    Close(usize),
    New,
}

pub struct TabStrip {
    pub height: f32,
}

impl TabStrip {
    pub fn new() -> Self {
        Self { height: TAB_STRIP_HEIGHT }
    }

    /// Converte coordenadas de clique (pixels lógicos) numa ação, ou
    /// `None` se o clique caiu fora de qualquer elemento da tira. Como
    /// cada aba tem largura fixa, a posição de cada aba (e do botão
    /// "+" logo depois da última) não depende da largura da janela —
    /// por isso `hit_test` não recebe `window_width`.
    pub fn hit_test(&self, tab_count: usize, x: f32, y: f32) -> Option<TabAction> {
        if y < 0.0 || y >= self.height {
            return None;
        }
        let new_tab_x = tab_count as f32 * TAB_WIDTH;
        if x >= new_tab_x && x < new_tab_x + NEW_TAB_BUTTON_WIDTH {
            return Some(TabAction::New);
        }
        if x < 0.0 || x >= new_tab_x {
            return None;
        }
        let index = (x / TAB_WIDTH) as usize;
        if index >= tab_count {
            return None;
        }
        let tab_start = index as f32 * TAB_WIDTH;
        let close_x = tab_start + TAB_WIDTH - TAB_CLOSE_BUTTON_WIDTH;
        if x >= close_x {
            return Some(TabAction::Close(index));
        }
        Some(TabAction::Select(index))
    }

    /// Desenha o fundo da tira, uma aba por título (truncado pra caber,
    /// destacando a ativa), o "x" de cada uma, e o botão "+" no fim.
    pub fn frame(&self, titles: &[String], active: usize, window_width: f32) -> scene::Frame {
        let mut frame = scene::Frame::new();

        frame.push(scene::DrawCommand::Rect(scene::RectCommand {
            x: 0.0, y: 0.0, width: window_width, height: self.height,
            color: [0.10, 0.10, 0.12, 1.0], corner_radius: 0.0,
        }));

        for (i, title) in titles.iter().enumerate() {
            let x = i as f32 * TAB_WIDTH;
            let is_active = i == active;
            frame.push(scene::DrawCommand::Rect(scene::RectCommand {
                x, y: 2.0, width: TAB_WIDTH - 2.0, height: self.height - 2.0,
                color: if is_active { [0.20, 0.20, 0.24, 1.0] } else { [0.13, 0.13, 0.15, 1.0] },
                corner_radius: 4.0,
            }));
            frame.push(scene::DrawCommand::Text(scene::TextCommand {
                x: x + 8.0, y: 8.0,
                content: truncate_title(title, TITLE_MAX_CHARS),
                size: 14.0,
                color: if is_active { [1.0, 1.0, 1.0, 1.0] } else { [0.7, 0.7, 0.7, 1.0] },
            }));
            frame.push(scene::DrawCommand::Text(scene::TextCommand {
                x: x + TAB_WIDTH - TAB_CLOSE_BUTTON_WIDTH + 4.0, y: 8.0,
                content: "x".to_string(), size: 14.0, color: [0.6, 0.6, 0.6, 1.0],
            }));
        }

        let new_tab_x = titles.len() as f32 * TAB_WIDTH;
        frame.push(scene::DrawCommand::Text(scene::TextCommand {
            x: new_tab_x + 8.0, y: 8.0, content: "+".to_string(), size: 18.0, color: [1.0, 1.0, 1.0, 1.0],
        }));

        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_title_short_text_unchanged() {
        assert_eq!(truncate_title("short.html", 18), "short.html");
    }

    #[test]
    fn truncate_title_long_text_gets_ellipsis() {
        let result = truncate_title("this-is-a-very-long-file-name.html", 18);
        assert_eq!(result.chars().count(), 18);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn hit_test_select_first_tab() {
        let strip = TabStrip::new();
        assert_eq!(strip.hit_test(3, 10.0, 10.0), Some(TabAction::Select(0)));
    }

    #[test]
    fn hit_test_select_second_tab() {
        let strip = TabStrip::new();
        assert_eq!(strip.hit_test(3, TAB_WIDTH + 10.0, 10.0), Some(TabAction::Select(1)));
    }

    #[test]
    fn hit_test_close_button_of_a_tab() {
        let strip = TabStrip::new();
        let close_x = TAB_WIDTH - TAB_CLOSE_BUTTON_WIDTH + 5.0;
        assert_eq!(strip.hit_test(3, close_x, 10.0), Some(TabAction::Close(0)));
    }

    #[test]
    fn hit_test_boundary_between_tab_body_and_close_button() {
        let strip = TabStrip::new();
        let last_body_pixel = TAB_WIDTH - TAB_CLOSE_BUTTON_WIDTH - 1.0;
        assert_eq!(strip.hit_test(3, last_body_pixel, 10.0), Some(TabAction::Select(0)));
        let first_close_pixel = TAB_WIDTH - TAB_CLOSE_BUTTON_WIDTH;
        assert_eq!(strip.hit_test(3, first_close_pixel, 10.0), Some(TabAction::Close(0)));
    }

    #[test]
    fn hit_test_new_tab_button() {
        let strip = TabStrip::new();
        let new_tab_x = 2.0 * TAB_WIDTH + 1.0;
        assert_eq!(strip.hit_test(2, new_tab_x, 10.0), Some(TabAction::New));
    }

    #[test]
    fn hit_test_outside_all_tabs_returns_none() {
        let strip = TabStrip::new();
        let past_new_button = 2.0 * TAB_WIDTH + NEW_TAB_BUTTON_WIDTH + 5.0;
        assert_eq!(strip.hit_test(2, past_new_button, 10.0), None);
    }

    #[test]
    fn hit_test_below_the_strip_returns_none() {
        let strip = TabStrip::new();
        assert_eq!(strip.hit_test(3, 10.0, TAB_STRIP_HEIGHT + 5.0), None);
    }

    #[test]
    fn frame_produces_a_tab_per_title_plus_new_button() {
        let strip = TabStrip::new();
        let titles = vec!["a.html".to_string(), "b.html".to_string()];
        let frame = strip.frame(&titles, 0, 800.0);
        let text_count = frame.commands.iter().filter(|c| matches!(c, scene::DrawCommand::Text(_))).count();
        // 1 title + 1 close "x" per tab, plus 1 "+" button text = 2*2 + 1 = 5
        assert_eq!(text_count, 5);
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --package ferris-compositor tabs::`
Expected: `test result: ok. 10 passed; 0 failed`

- [ ] **Step 4: Run the whole workspace build**

Run: `cargo build --workspace`
Expected: clean build. (`TabStrip` is not yet used outside its own tests — expected, wired up in Task 3.)

- [ ] **Step 5: Commit**

```bash
git add ferris-compositor/src/tabs.rs ferris-compositor/src/lib.rs
git commit -m "feat: add TabStrip, fixed-width tab hit-testing and drawing"
```

---

### Task 3: Wire tabs into `main.rs` and verify manually

**Files:**
- Modify: `ferris-compositor/src/main.rs`

**Interfaces:**
- Consumes: `chrome::{Chrome, ChromeAction, NavigationHistory, translate_frame, source_display_text}` (Task 1, sub-project 2.9), `tabs::{TabStrip, TabAction}` (Task 2), `ferris_loader::{Source, LoadError, parse_source, load_page}` (sub-project 2.8).
- Produces: nothing consumed by later tasks — this is the final task of the plan.

- [ ] **Step 1: Read the current file before editing**

Read `ferris-compositor/src/main.rs` in full to confirm it still matches the state left by sub-project 2.9's final review fix (it should have `App` with `chrome: Option<Chrome>`/`page_frame: Option<scene::Frame>` fields, `navigate_interactive` returning `bool`, `commit_address_bar`, `classify_key` with the AltGr fix and `Key::Named(NamedKey::Space)` handling, and a `mod tests` block with 12 tests).

- [ ] **Step 2: Replace `ferris-compositor/src/main.rs` in full**

Replace the entire file with:
```rust
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
                        Some(history) => chrome::source_display_text(history.current()),
                        None => "Nova aba".to_string(),
                    })
                    .collect();

                let mut frame = self.tab_strip.frame(&titles, self.active_tab, window_width);
                let mut bar_height = 0.0;

                if let Some(tab) = self.active_tab() {
                    bar_height = tab.chrome.bar_height;
                    let bar_frame = tab.chrome.frame(window_width);
                    frame.commands.extend(chrome::translate_frame(&bar_frame, self.tab_strip.height).commands);

                    let page = tab.page_frame.clone().unwrap_or_default();
                    let page_dy = self.tab_strip.height + bar_height;
                    frame.commands.extend(chrome::translate_frame(&page, page_dy).commands);
                }

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
```

- [ ] **Step 3: Run the workspace build**

Run: `cargo build --workspace`
Expected: clean build, no warnings.

- [ ] **Step 4: Run the workspace test suite**

Run: `cargo test --workspace`
Expected: 314 passed (291 baseline + 3 from Task 1 + 10 from Task 2 + 10 net-new in this task's `main.rs` test module — `commit_address_bar_on_a_blank_tab_creates_history_on_first_successful_navigation`, all 4 `close_tab_*` tests, `new_tab_appends_...`, `next_tab_wraps_...`, and the 3 `classify_key_ctrl_t/w/tab_*` tests — the other 12 tests in this file are carried over unchanged or rewritten to use `tabs`/`app_with_tabs`, not new; `main.rs`'s own test module goes from 12 to 22 total).

- [ ] **Step 5: Manual verification — open several tabs, navigate independently, close, switch**

Run: `cargo build --release -p ferris-compositor` once, then run it (no argument, from the workspace root, same as 2.9's own manual verification setup) and, using the same techniques already established in this project (PowerShell `Start-Process`/background run + `GetWindowRect`+`CopyFromScreen` for screenshots, `SendKeys`/`mouse_event` to simulate clicks/keystrokes):

- Confirm the window opens with ONE tab (the fixture, now labeled with its file path in the tab strip) and no tab strip weirdness (single tab, "+" button visible right after it).
- Open a new tab via the "+" button. Confirm it's blank, labeled "Nova aba", and the address bar is already focused. Type a real URL and press Enter — confirm it loads and the tab's label updates to that URL.
- Open a third tab via Ctrl+T. Navigate it to a different real page (a local file or another URL).
- Click back to the first tab (the fixture). Confirm its content is still the fixture (unaffected by the other tabs' navigation) and its address bar still shows the fixture path, not tab 2 or 3's URL.
- Use Ctrl+Tab repeatedly and confirm it cycles through all 3 tabs in order, wrapping back to the first after the last.
- Click the "x" on the middle tab (tab 2). Confirm it closes and the correct remaining tab becomes active (per this task's `close_tab` logic).
- Press Ctrl+W to close the now-active tab. Confirm one tab remains.
- Press Ctrl+W one more time. Confirm the window closes (last tab closed = window closes, per the spec's decision).

Note in the task's completion report exactly what was seen at each of these checks — a task reviewer for this task must ask the implementer to describe what appeared for each one, and treat an implementer that skipped any of them as not having completed the task's actual deliverable, same discipline as every prior sub-project's final GUI task in this project.

- [ ] **Step 6: Commit**

```bash
git add ferris-compositor/src/main.rs
git commit -m "feat: wire multiple tabs into ferris-compositor (open/close/switch, Ctrl+T/Ctrl+W/Ctrl+Tab)"
```

---
