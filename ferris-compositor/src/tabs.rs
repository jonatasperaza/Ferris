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
