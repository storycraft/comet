use anyrender::Paint;
use comet_div::{define_style_props, style::PropHint};
use peniko::Color;

// TODO:: change names
define_style_props!(
    // Box draw styles
    pub Fill(PropHint::Paint): Paint,
    pub BorderFill(PropHint::Paint): Paint,

    // Text draw styles
    pub TextColor(PropHint::Paint): Paint = Paint::Solid(Color::BLACK),
);
