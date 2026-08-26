use gpui::{InteractiveElement as _, Styled as _};
pub fn generated_view<Tokens, Assets>(
    tokens: &Tokens,
    assets: &Assets,
) -> impl gpui::IntoElement + use<Tokens, Assets>
where
    Tokens: figma_gpui_runtime::TokenResolver,
    Assets: figma_gpui_runtime::AssetResolver,
{
    let _ = tokens;
    node_0000(tokens, assets)
}
#[allow(clippy::too_many_lines)]
fn node_0000<Tokens, Assets>(
    tokens: &Tokens,
    assets: &Assets,
) -> impl gpui::IntoElement + use<Tokens, Assets>
where
    Tokens: figma_gpui_runtime::TokenResolver,
    Assets: figma_gpui_runtime::AssetResolver,
{
    let _ = tokens;
    let _ = assets;
    gpui::img(
            figma_gpui_runtime::AssetResolver::asset_path(
                assets,
                "asset-6e6f64653a31333a313a737667.svg",
            ),
        )
        .debug_selector(|| figma_gpui_runtime::source_selector(0usize))
        .relative()
        .w(gpui::px(40f32))
        .h(gpui::px(20f32))
        .flex_none()
}
