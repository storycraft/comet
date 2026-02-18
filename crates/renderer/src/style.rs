use anyrender::Paint;
use comet_div::{define_style_props, style::PropLevel};
use peniko::Color;

// TODO:: change names
define_style_props!(
    // Box draw styles
    pub Fill(PropLevel::Paint): Paint,
    pub BorderFill(PropLevel::Paint): Paint,

    // Text draw styles
    pub TextColor(PropLevel::Paint): Paint = Paint::Solid(Color::BLACK),
);