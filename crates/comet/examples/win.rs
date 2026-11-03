mod common;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (ui, root) = common::sample_ui();

    comet::run(ui, root)
}
