//! Complete retained application, with its interface in a compiled markup file.
fn main() -> aegle::Result<()> {
    aegle::App::run_ui(aegle::ui!("examples/hello.aegle"))
}
