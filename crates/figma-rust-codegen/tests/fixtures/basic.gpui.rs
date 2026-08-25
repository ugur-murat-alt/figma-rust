use gpui::{InteractiveElement as _, Styled as _};
pub fn generated_view(
    tokens: &impl figma_gpui_runtime::TokenResolver,
) -> impl gpui::IntoElement {
    let _ = tokens;
    node_0000(tokens)
}
fn node_0000(tokens: &impl figma_gpui_runtime::TokenResolver) -> impl gpui::IntoElement {
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(0usize))
        .relative()
        .w(gpui::px(120f32))
        .min_w(gpui::px(80f32))
        .max_w(gpui::px(240f32))
        .h(gpui::px(48f32))
        .flex_none()
        .bg(
            figma_gpui_runtime::TokenResolver::color(
                tokens,
                "color.brand",
                gpui::Hsla::from(gpui::rgba(0x2040_80ff)),
            ),
        )
        .rounded(gpui::px(8f32))
}
