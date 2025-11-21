use kurbo::Affine;
use peniko::BlendMode;

pub struct Layer {
    pub transform: Affine,
    pub blend: BlendMode,
    pub opacity: f32,
}
