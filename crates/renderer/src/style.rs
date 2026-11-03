use anyrender::Paint;
use comet_tree::style::define_style_props;
use peniko::Color;

define_style_props!(
    // Box draw styles
    pub Fill: Paint;
    pub BorderFill: Paint;

    // Text draw styles
    pub TextColor: Paint = Paint::Solid(Color::BLACK);
);
