/// A rectangle draw command. `x`, `y`, `width`, `height`, and `corner_radius`
/// are all in logical pixels (CSS-like) — the renderer applies the display's
/// scale factor when converting to physical pixels for the GPU.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectCommand {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
    pub corner_radius: f32,
}

/// A text draw command. `x`, `y`, and `size` are in logical pixels (CSS-like)
/// — the renderer applies the display's scale factor when converting to
/// physical pixels for the GPU.
#[derive(Debug, Clone, PartialEq)]
pub struct TextCommand {
    pub x: f32,
    pub y: f32,
    pub content: String,
    pub size: f32,
    pub color: [f32; 4],
}

impl RectCommand {
    pub fn scaled(&self, factor: f32) -> RectCommand {
        RectCommand {
            x: self.x * factor,
            y: self.y * factor,
            width: self.width * factor,
            height: self.height * factor,
            color: self.color,
            corner_radius: self.corner_radius * factor,
        }
    }
}

impl TextCommand {
    pub fn scaled(&self, factor: f32) -> TextCommand {
        TextCommand {
            x: self.x * factor,
            y: self.y * factor,
            content: self.content.clone(),
            size: self.size * factor,
            color: self.color,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Rect(RectCommand),
    Text(TextCommand),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frame {
    pub commands: Vec<DrawCommand>,
}

impl Frame {
    pub fn new() -> Self {
        Self { commands: Vec::new() }
    }

    pub fn push(&mut self, cmd: DrawCommand) {
        self.commands.push(cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_scaled_multiplies_position_size_and_radius() {
        let rect = RectCommand { x: 10.0, y: 20.0, width: 30.0, height: 40.0, color: [1.0, 0.0, 0.0, 1.0], corner_radius: 5.0 };
        let scaled = rect.scaled(2.0);
        assert_eq!(scaled.x, 20.0);
        assert_eq!(scaled.y, 40.0);
        assert_eq!(scaled.width, 60.0);
        assert_eq!(scaled.height, 80.0);
        assert_eq!(scaled.corner_radius, 10.0);
        assert_eq!(scaled.color, [1.0, 0.0, 0.0, 1.0], "color must not be scaled");
    }

    #[test]
    fn rect_scaled_by_one_is_identity() {
        let rect = RectCommand { x: 10.0, y: 20.0, width: 30.0, height: 40.0, color: [0.5, 0.5, 0.5, 1.0], corner_radius: 5.0 };
        assert_eq!(rect.scaled(1.0), rect);
    }

    #[test]
    fn text_scaled_multiplies_position_and_size_not_content_or_color() {
        let text = TextCommand { x: 10.0, y: 20.0, content: "hi".to_string(), size: 16.0, color: [1.0, 1.0, 1.0, 1.0] };
        let scaled = text.scaled(1.5);
        assert_eq!(scaled.x, 15.0);
        assert_eq!(scaled.y, 30.0);
        assert_eq!(scaled.size, 24.0);
        assert_eq!(scaled.content, "hi");
        assert_eq!(scaled.color, [1.0, 1.0, 1.0, 1.0]);
    }
}
