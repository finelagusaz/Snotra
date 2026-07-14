#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceAction {
    Render,
    RenderThenReconfigure,
    Skip,
    Reconfigure,
    Recreate,
    FatalValidation,
}

pub fn is_renderable_extent(width: u32, height: u32) -> bool {
    width > 0 && height > 0
}

pub fn surface_action(texture: &wgpu::CurrentSurfaceTexture) -> SurfaceAction {
    match texture {
        wgpu::CurrentSurfaceTexture::Success(_) => SurfaceAction::Render,
        wgpu::CurrentSurfaceTexture::Suboptimal(_) => SurfaceAction::RenderThenReconfigure,
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
            SurfaceAction::Skip
        }
        wgpu::CurrentSurfaceTexture::Outdated => SurfaceAction::Reconfigure,
        wgpu::CurrentSurfaceTexture::Lost => SurfaceAction::Recreate,
        wgpu::CurrentSurfaceTexture::Validation => SurfaceAction::FatalValidation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_extent_never_reaches_surface_configuration() {
        assert!(!is_renderable_extent(0, 480));
        assert!(!is_renderable_extent(640, 0));
        assert!(is_renderable_extent(640, 480));
    }

    #[test]
    fn recoverable_surface_states_have_distinct_actions() {
        assert_eq!(
            surface_action(&wgpu::CurrentSurfaceTexture::Timeout),
            SurfaceAction::Skip
        );
        assert_eq!(
            surface_action(&wgpu::CurrentSurfaceTexture::Occluded),
            SurfaceAction::Skip
        );
        assert_eq!(
            surface_action(&wgpu::CurrentSurfaceTexture::Outdated),
            SurfaceAction::Reconfigure
        );
        assert_eq!(
            surface_action(&wgpu::CurrentSurfaceTexture::Lost),
            SurfaceAction::Recreate
        );
        assert_eq!(
            surface_action(&wgpu::CurrentSurfaceTexture::Validation),
            SurfaceAction::FatalValidation
        );
    }
}
