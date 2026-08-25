use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};
pub fn generated_view(
    tokens: &impl figma_gpui_runtime::TokenResolver,
) -> impl gpui::IntoElement {
    let _ = tokens;
    node_0000(tokens)
}
#[allow(clippy::too_many_lines)]
fn node_0000(tokens: &impl figma_gpui_runtime::TokenResolver) -> impl gpui::IntoElement {
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(0usize))
        .relative()
        .w(gpui::px(100f32))
        .h(gpui::px(60f32))
        .flex_none()
        .child(node_0001(tokens))
        .child(node_0002(tokens))
}
#[allow(clippy::too_many_lines)]
fn node_0001(tokens: &impl figma_gpui_runtime::TokenResolver) -> impl gpui::IntoElement {
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(1usize))
        .absolute()
        .left(gpui::px(0f32))
        .top(gpui::px(0f32))
        .w(gpui::px(40f32))
        .h(gpui::px(30f32))
        .flex_none()
        .bg(gpui::rgba(0x2040_80ff))
}
#[allow(clippy::too_many_lines)]
fn node_0002(tokens: &impl figma_gpui_runtime::TokenResolver) -> impl gpui::IntoElement {
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(2usize))
        .absolute()
        .left(gpui::px(70f32))
        .top(gpui::px(40f32))
        .w(gpui::px(30f32))
        .h(gpui::px(20f32))
        .flex_none()
        .bg(gpui::rgba(0xe050_40ff))
}
