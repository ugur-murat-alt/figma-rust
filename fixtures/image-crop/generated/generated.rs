use gpui::{InteractiveElement as _, ParentElement as _, Styled as _, StyledImage as _};
pub fn generated_view<Tokens, Assets>(
    tokens: &Tokens,
    assets: &Assets,
) -> impl gpui::IntoElement + use<Tokens, Assets>
where
    Tokens: figma_gpui_runtime::TokenResolver,
    Assets: figma_gpui_runtime::AssetResolver,
{
    let _ = tokens;
    gpui::div()
        .child(node_0000(tokens, assets))
        .child(node_0001(tokens, assets))
        .child(node_0002(tokens, assets))
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
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(0usize))
        .relative()
        .overflow_hidden()
        .w(gpui::px(100f32))
        .h(gpui::px(80f32))
        .flex_none()
        .overflow_hidden()
        .child(
            gpui::img(
                    figma_gpui_runtime::AssetResolver::asset_path(
                        assets,
                        "asset-636865636b6572.png",
                    ),
                )
                .absolute()
                .opacity(1f32)
                .inset_0()
                .size_full()
                .object_fit(gpui::ObjectFit::Contain),
        )
}
#[allow(clippy::too_many_lines)]
fn node_0001<Tokens, Assets>(
    tokens: &Tokens,
    assets: &Assets,
) -> impl gpui::IntoElement + use<Tokens, Assets>
where
    Tokens: figma_gpui_runtime::TokenResolver,
    Assets: figma_gpui_runtime::AssetResolver,
{
    let _ = tokens;
    let _ = assets;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(1usize))
        .relative()
        .overflow_hidden()
        .w(gpui::px(100f32))
        .h(gpui::px(80f32))
        .flex_none()
        .overflow_hidden()
        .child(
            gpui::img(
                    figma_gpui_runtime::AssetResolver::asset_path(
                        assets,
                        "asset-636865636b6572.png",
                    ),
                )
                .absolute()
                .opacity(1f32)
                .inset_0()
                .size_full()
                .object_fit(gpui::ObjectFit::Cover),
        )
        .rounded(gpui::px(8f32))
}
#[allow(clippy::too_many_lines)]
fn node_0002<Tokens, Assets>(
    tokens: &Tokens,
    assets: &Assets,
) -> impl gpui::IntoElement + use<Tokens, Assets>
where
    Tokens: figma_gpui_runtime::TokenResolver,
    Assets: figma_gpui_runtime::AssetResolver,
{
    let _ = tokens;
    let _ = assets;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(2usize))
        .relative()
        .overflow_hidden()
        .w(gpui::px(100f32))
        .h(gpui::px(80f32))
        .flex_none()
        .overflow_hidden()
        .child(
            gpui::img(
                    figma_gpui_runtime::AssetResolver::asset_path(
                        assets,
                        "asset-636865636b6572.png",
                    ),
                )
                .absolute()
                .opacity(0.75f32)
                .object_fit(gpui::ObjectFit::Fill)
                .left(gpui::px(-50f32))
                .top(gpui::px(-10f32))
                .w(gpui::px(200f32))
                .h(gpui::px(100f32)),
        )
}
