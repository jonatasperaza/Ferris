pub mod quad;
// pub mod text;

pub fn choose_present_mode(supported: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    const PREFERENCE: [wgpu::PresentMode; 3] = [
        wgpu::PresentMode::Immediate,
        wgpu::PresentMode::Mailbox,
        wgpu::PresentMode::Fifo,
    ];
    for mode in PREFERENCE {
        if supported.contains(&mode) {
            return mode;
        }
    }
    wgpu::PresentMode::Fifo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_immediate_when_available() {
        let supported = [
            wgpu::PresentMode::Fifo,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::Immediate,
        ];
        assert_eq!(choose_present_mode(&supported), wgpu::PresentMode::Immediate);
    }

    #[test]
    fn falls_back_to_mailbox_when_no_immediate() {
        let supported = [wgpu::PresentMode::Fifo, wgpu::PresentMode::Mailbox];
        assert_eq!(choose_present_mode(&supported), wgpu::PresentMode::Mailbox);
    }

    #[test]
    fn falls_back_to_fifo_when_nothing_else_supported() {
        let supported = [wgpu::PresentMode::Fifo];
        assert_eq!(choose_present_mode(&supported), wgpu::PresentMode::Fifo);
    }
}
