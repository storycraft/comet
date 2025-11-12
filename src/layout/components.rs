use anyrender::Paint;
use kurbo::Affine;

#[derive(Clone)]
pub struct BoxTransform(pub Affine);

#[derive(Clone)]
pub struct TextFill(pub Paint);

#[derive(Clone)]
pub struct BoxFill(pub Paint);

#[derive(Clone)]
pub struct BoxBorderFill(pub Paint);
