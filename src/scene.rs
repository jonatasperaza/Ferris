#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectCommand {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
    pub corner_radius: f32,
}

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

#[derive(Debug, Clone, Default)]
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

pub fn bounce_position(elapsed_secs: f32, speed: f32, min: f32, max: f32, phase_offset: f32) -> f32 {
    let range = max - min;
    if range <= 0.0 || speed <= 0.0 {
        return min;
    }
    let half_period = range / speed;
    let period = 2.0 * half_period;
    let t = (elapsed_secs + phase_offset).rem_euclid(period);
    if t < half_period {
        min + t * speed
    } else {
        max - (t - half_period) * speed
    }
}

pub fn build_test_scene(elapsed_secs: f32, viewport_width: f32, viewport_height: f32) -> Frame {
    let mut frame = Frame::new();
    let rect_size = 40.0;
    let count = 20;
    for i in 0..count {
        let phase_offset = i as f32 * 0.35;
        let speed = 120.0 + (i as f32 * 8.0);
        let x = bounce_position(elapsed_secs, speed, 0.0, viewport_width - rect_size, phase_offset);
        let y = 80.0 + (i as f32 * (viewport_height - 160.0) / count as f32);
        let hue = i as f32 / count as f32;
        frame.push(DrawCommand::Rect(RectCommand {
            x,
            y,
            width: rect_size,
            height: rect_size,
            color: hsv_to_rgba(hue),
            corner_radius: 8.0,
        }));
    }
    frame.push(DrawCommand::Text(TextCommand {
        x: 20.0,
        y: 20.0,
        content: "Ferris compositor".to_string(),
        size: 24.0,
        color: [1.0, 1.0, 1.0, 1.0],
    }));
    frame
}

fn hsv_to_rgba(hue: f32) -> [f32; 4] {
    let h = hue * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as i32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [r, g, b, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounce_starts_at_min_with_zero_phase() {
        let pos = bounce_position(0.0, 100.0, 0.0, 200.0, 0.0);
        assert_eq!(pos, 0.0);
    }

    #[test]
    fn bounce_moves_toward_max_before_half_period() {
        let pos = bounce_position(0.5, 100.0, 0.0, 200.0, 0.0);
        assert_eq!(pos, 50.0);
    }

    #[test]
    fn bounce_reflects_after_reaching_max() {
        // half_period = range / speed = 200 / 100 = 2.0s
        let at_max = bounce_position(2.0, 100.0, 0.0, 200.0, 0.0);
        assert!((at_max - 200.0).abs() < 0.01);
        let past_max = bounce_position(2.5, 100.0, 0.0, 200.0, 0.0);
        assert!((past_max - 150.0).abs() < 0.01);
    }

    #[test]
    fn bounce_stays_within_bounds_over_time() {
        for i in 0..1000 {
            let t = i as f32 * 0.037;
            let pos = bounce_position(t, 137.0, 10.0, 90.0, 1.7);
            assert!(pos >= 10.0 - 0.01 && pos <= 90.0 + 0.01, "pos {pos} out of bounds at t={t}");
        }
    }

    #[test]
    fn test_scene_has_twenty_rects_and_one_label() {
        let frame = build_test_scene(0.0, 1280.0, 720.0);
        let rect_count = frame.commands.iter().filter(|c| matches!(c, DrawCommand::Rect(_))).count();
        let text_count = frame.commands.iter().filter(|c| matches!(c, DrawCommand::Text(_))).count();
        assert_eq!(rect_count, 20);
        assert_eq!(text_count, 1);
    }

    #[test]
    fn test_scene_rects_stay_within_viewport() {
        let frame = build_test_scene(3.7, 1280.0, 720.0);
        for cmd in &frame.commands {
            if let DrawCommand::Rect(r) = cmd {
                assert!(r.x >= -0.01 && r.x + r.width <= 1280.0 + 0.01);
                assert!(r.y >= 0.0 && r.y + r.height <= 720.0);
            }
        }
    }

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
