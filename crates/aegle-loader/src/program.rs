//! The `Shader` element: a view filled by a custom fragment program.

use aegle_widgets::ShaderView;

use crate::element;

/// An empty source empties the view; a failure is reported through the
/// view's `failed` event and `error` field, not to the binding.
fn source(view: &ShaderView, path: &str) {
    if path.is_empty() {
        view.clear();
    } else {
        let _ = view.load(path);
    }
}

element! {
    /// A view whose background is the WGSL program in the file at `source`,
    /// a path relative to the working directory, drawn with `values`; see
    /// `aegle_scene::Shader`. It has no size of its own. A program that does
    /// not compile empties the view and raises `failed`, with the file, line
    /// and column in `self.error`. Only GPU renderers draw programs.
    pub Shader(ShaderView) {
        create |parent| aegle_widgets::Widgets::shader_view(parent);
        set source: line => source;
        set values: numbers(8) => |view, values| view.set_values(&values.to_vec());
        event failed => |view, run| view.on_error(move |_| run());
        get error: string => |view| view.error().unwrap_or_default();
    }
}
