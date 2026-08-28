use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};
use gpui::prelude::FluentBuilder as _;
pub fn generated_view<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div().child(node_0000(tokens)).child(node_0003(tokens))
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
        .flex()
        .flex_col()
        .justify_start()
        .items_start()
        .w(gpui::px(160f32))
        .h(gpui::px(48f32))
        .flex_none()
        .child(node_0001(tokens))
        .when(
            figma_gpui_runtime::TokenResolver::boolean_with_context(
                tokens,
                figma_gpui_runtime::TokenContext {
                    id: "VariableID:27:visible",
                    collection_id: Some("VariableCollectionId:27:content"),
                    mode_id: Some("mode-light"),
                    modes: &[("VariableCollectionId:27:content", "mode-light")],
                },
                true,
            ),
            |this| this.child(node_0002(tokens)),
        )
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
        .w(gpui::px(160f32))
        .h(gpui::px(24f32))
        .flex_none()
        .child(
            figma_gpui_runtime::TokenResolver::string_with_context(
                tokens,
                figma_gpui_runtime::TokenContext {
                    id: "VariableID:27:label",
                    collection_id: Some("VariableCollectionId:27:content"),
                    mode_id: Some("mode-light"),
                    modes: &[("VariableCollectionId:27:content", "mode-light")],
                },
                "Light label",
            ),
        )
}
#[allow(clippy::too_many_lines)]
fn node_0002<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(2usize))
        .relative()
        .w(gpui::px(24f32))
        .h(gpui::px(24f32))
        .flex_none()
}
#[allow(clippy::too_many_lines)]
fn node_0003<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(3usize))
        .relative()
        .flex()
        .flex_col()
        .justify_start()
        .items_start()
        .w(gpui::px(160f32))
        .h(gpui::px(48f32))
        .flex_none()
        .child(node_0004(tokens))
        .when(
            figma_gpui_runtime::TokenResolver::boolean_with_context(
                tokens,
                figma_gpui_runtime::TokenContext {
                    id: "VariableID:27:visible",
                    collection_id: Some("VariableCollectionId:27:content"),
                    mode_id: Some("mode-dark"),
                    modes: &[("VariableCollectionId:27:content", "mode-dark")],
                },
                false,
            ),
            |this| this.child(node_0005(tokens)),
        )
}
#[allow(clippy::too_many_lines)]
fn node_0004<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(4usize))
        .relative()
        .w(gpui::px(160f32))
        .h(gpui::px(24f32))
        .flex_none()
        .child(
            figma_gpui_runtime::TokenResolver::string_with_context(
                tokens,
                figma_gpui_runtime::TokenContext {
                    id: "VariableID:27:label",
                    collection_id: Some("VariableCollectionId:27:content"),
                    mode_id: Some("mode-dark"),
                    modes: &[("VariableCollectionId:27:content", "mode-dark")],
                },
                "Dark label",
            ),
        )
}
#[allow(clippy::too_many_lines)]
fn node_0005<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(5usize))
        .relative()
        .w(gpui::px(24f32))
        .h(gpui::px(24f32))
        .flex_none()
}
