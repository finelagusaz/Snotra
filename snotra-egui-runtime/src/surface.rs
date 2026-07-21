pub fn is_renderable_extent(width: u32, height: u32) -> bool {
    width > 0 && height > 0
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
}
