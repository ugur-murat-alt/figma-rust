use gpui::{InteractiveElement as _, Styled as _};
pub fn generated_view(
    tokens: &impl figma_gpui_runtime::TokenResolver,
    assets: &impl figma_gpui_runtime::AssetResolver,
) -> impl gpui::IntoElement {
    let _ = tokens;
    node_0000(tokens, assets)
}
#[allow(clippy::too_many_lines)]
fn node_0000(
    tokens: &impl figma_gpui_runtime::TokenResolver,
    assets: &impl figma_gpui_runtime::AssetResolver,
) -> impl gpui::IntoElement {
    let _ = tokens;
    gpui::svg()
        .external_path(
            figma_gpui_runtime::AssetResolver::asset_path(
                    assets,
                    "asset-6e6f64653a31333a313a737667.svg",
                )
                .to_string_lossy()
                .into_owned(),
        )
        .debug_selector(|| figma_gpui_runtime::source_selector(0usize))
        .relative()
        .w(gpui::px(40f32))
        .h(gpui::px(20f32))
        .flex_none()
}
