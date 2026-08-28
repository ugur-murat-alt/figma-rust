use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};
pub fn generated_view<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    node_0000(tokens)
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
        .w(gpui::px(100f32))
        .h(gpui::px(40f32))
        .flex_none()
        .child(node_0001(tokens))
}
#[allow(clippy::too_many_lines)]
fn node_0001<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(1usize))
        .absolute()
        .left(gpui::px(0f32))
        .top(gpui::px(0f32))
        .w(gpui::px(100f32))
        .h(gpui::px(40f32))
        .flex_none()
        .child(node_0002(tokens))
}
#[allow(clippy::too_many_lines)]
fn node_0002<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(2usize))
        .absolute()
        .left(gpui::px(0f32))
        .top(gpui::px(0f32))
        .w(gpui::px(20f32))
        .h(gpui::px(10f32))
        .flex_none()
}
