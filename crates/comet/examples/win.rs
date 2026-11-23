mod common;

use comet::winit::window::WindowAttributes;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    comet::run(
        WindowAttributes::default().with_title("Comet"),
        common::app,
    )
}
