use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};
pub fn generated_view<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div().child(node_0000(tokens)).child(node_0006(tokens))
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
        .flex_row()
        .justify_start()
        .items_start()
        .w(gpui::px(200f32))
        .h(gpui::px(80f32))
        .flex_none()
        .child(node_0001(tokens))
        .child(node_0002(tokens))
        .child(node_0003(tokens))
        .child(node_0004(tokens))
        .child(node_0005(tokens))
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
        .w(gpui::px(20f32))
        .h(gpui::px(10f32))
        .flex_none()
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
        .self_start()
        .w(gpui::px(20f32))
        .h(gpui::px(10f32))
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
        .self_center()
        .w(gpui::px(20f32))
        .h(gpui::px(10f32))
        .flex_none()
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
        .self_end()
        .w(gpui::px(20f32))
        .h(gpui::px(10f32))
        .flex_none()
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
        .self_stretch()
        .w(gpui::px(20f32))
        .flex_none()
}
#[allow(clippy::too_many_lines)]
fn node_0006<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(6usize))
        .relative()
        .flex()
        .flex_col()
        .justify_start()
        .items_start()
        .w(gpui::px(80f32))
        .h(gpui::px(200f32))
        .flex_none()
        .child(node_0007(tokens))
        .child(node_0008(tokens))
        .child(node_0009(tokens))
        .child(node_0010(tokens))
        .child(node_0011(tokens))
}
#[allow(clippy::too_many_lines)]
fn node_0007<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(7usize))
        .relative()
        .w(gpui::px(20f32))
        .h(gpui::px(10f32))
        .flex_none()
}
#[allow(clippy::too_many_lines)]
fn node_0008<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(8usize))
        .relative()
        .self_start()
        .w(gpui::px(20f32))
        .h(gpui::px(10f32))
        .flex_none()
}
#[allow(clippy::too_many_lines)]
fn node_0009<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(9usize))
        .relative()
        .self_center()
        .w(gpui::px(20f32))
        .h(gpui::px(10f32))
        .flex_none()
}
#[allow(clippy::too_many_lines)]
fn node_0010<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(10usize))
        .relative()
        .self_end()
        .w(gpui::px(20f32))
        .h(gpui::px(10f32))
        .flex_none()
}
#[allow(clippy::too_many_lines)]
fn node_0011<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
        .debug_selector(|| figma_gpui_runtime::source_selector(11usize))
        .relative()
        .self_stretch()
        .h(gpui::px(10f32))
        .flex_none()
}
