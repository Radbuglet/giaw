pub fn lerp_f32(a: f32, b: f32, p: f32) -> f32 {
    a + (b - a) * p
}
