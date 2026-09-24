use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Cache, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

use crate::scene::TextCommand;

pub struct TextLayer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    buffers: Vec<GlyphonBuffer>,
}

impl TextLayer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let text_renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        Self { font_system, swash_cache, viewport, atlas, text_renderer, buffers: Vec::new() }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        texts: &[TextCommand],
    ) {
        self.buffers.clear();
        for cmd in texts {
            let mut buffer = GlyphonBuffer::new(&mut self.font_system, Metrics::new(cmd.size, cmd.size * 1.2));
            buffer.set_text(&mut self.font_system, &cmd.content, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
            buffer.set_size(&mut self.font_system, Some(width as f32), Some(height as f32));
            self.buffers.push(buffer);
        }

        self.viewport.update(queue, Resolution { width, height });

        let text_areas: Vec<TextArea> = self
            .buffers
            .iter()
            .zip(texts.iter())
            .map(|(buffer, cmd)| TextArea {
                buffer,
                left: cmd.x,
                top: cmd.y,
                scale: 1.0,
                bounds: TextBounds { left: 0, top: 0, right: width as i32, bottom: height as i32 },
                default_color: glyphon::Color::rgba(
                    (cmd.color[0] * 255.0) as u8,
                    (cmd.color[1] * 255.0) as u8,
                    (cmd.color[2] * 255.0) as u8,
                    (cmd.color[3] * 255.0) as u8,
                ),
                custom_glyphs: &[],
            })
            .collect();

        self.text_renderer
            .prepare(device, queue, &mut self.font_system, &mut self.atlas, &self.viewport, text_areas, &mut self.swash_cache)
            .expect("glyphon prepare failed");
    }

    pub fn render<'pass>(&'pass self, render_pass: &mut wgpu::RenderPass<'pass>) {
        self.text_renderer.render(&self.atlas, &self.viewport, render_pass).expect("glyphon render failed");
    }

    pub fn trim_atlas(&mut self) {
        self.atlas.trim();
    }
}
