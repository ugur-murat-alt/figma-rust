use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};
pub fn generated_view<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div().child(node_0000(tokens)).child(node_0001(tokens))
}
#[allow(clippy::too_many_lines)]
fn node_0000<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(0usize))
        .relative()
        .left(gpui::px(0.75f32))
        .top(gpui::px(0.75f32))
        .overflow_hidden()
        .w(gpui::px(98.5f32))
        .h(gpui::px(98.5f32))
        .flex_none()
        .bg(gpui::rgba(0x2040_80ff))
}
#[allow(clippy::too_many_lines)]
fn node_0001<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(1usize))
        .relative()
        .overflow_hidden()
        .w(gpui::px(98.5f32))
        .h(gpui::px(98.5f32))
        .flex_none()
        .bg(gpui::rgba(0x8040_20ff))
}
