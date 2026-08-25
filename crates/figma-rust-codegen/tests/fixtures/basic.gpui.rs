use gpui::{InteractiveElement as _, Styled as _};
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
        .flex()
        .flex_row()
        .justify_start()
        .items_start()
        .gap(
            gpui::px(
                figma_gpui_runtime::TokenResolver::number_with_context(
                    tokens,
                    figma_gpui_runtime::TokenContext {
                        id: "number.layout",
                        collection_id: Some("collection.theme"),
                        mode_id: Some("mode.light"),
                        modes: &[("collection.theme", "mode.light")],
                    },
                    8f32,
                ),
            ),
        )
        .p(
            gpui::px(
                figma_gpui_runtime::TokenResolver::number_with_context(
                    tokens,
                    figma_gpui_runtime::TokenContext {
                        id: "number.layout",
                        collection_id: Some("collection.theme"),
                        mode_id: Some("mode.light"),
                        modes: &[("collection.theme", "mode.light")],
                    },
                    8f32,
                ),
            ),
        )
        .w(gpui::px(120f32))
        .min_w(gpui::px(80f32))
        .max_w(gpui::px(240f32))
        .h(gpui::px(48f32))
        .flex_none()
        .bg(
            figma_gpui_runtime::TokenResolver::color_with_context(
                tokens,
                figma_gpui_runtime::TokenContext {
                    id: "color.brand",
                    collection_id: Some("collection.theme"),
                    mode_id: Some("mode.light"),
                    modes: &[("collection.theme", "mode.light")],
                },
                gpui::Hsla::from(gpui::rgba(0x2040_80ff)),
            ),
        )
        .border_t(
            gpui::px(
                figma_gpui_runtime::TokenResolver::number_with_context(
                    tokens,
                    figma_gpui_runtime::TokenContext {
                        id: "stroke.top",
                        collection_id: Some("collection.theme"),
                        mode_id: Some("mode.light"),
                        modes: &[("collection.theme", "mode.light")],
                    },
                    1f32,
                ),
            ),
        )
        .border_r(
            gpui::px(
                figma_gpui_runtime::TokenResolver::number_with_context(
                    tokens,
                    figma_gpui_runtime::TokenContext {
                        id: "stroke.right",
                        collection_id: Some("collection.theme"),
                        mode_id: Some("mode.light"),
                        modes: &[("collection.theme", "mode.light")],
                    },
                    2f32,
                ),
            ),
        )
        .border_b(
            gpui::px(
                figma_gpui_runtime::TokenResolver::number_with_context(
                    tokens,
                    figma_gpui_runtime::TokenContext {
                        id: "stroke.bottom",
                        collection_id: Some("collection.theme"),
                        mode_id: Some("mode.light"),
                        modes: &[("collection.theme", "mode.light")],
                    },
                    3f32,
                ),
            ),
        )
        .border_l(
            gpui::px(
                figma_gpui_runtime::TokenResolver::number_with_context(
                    tokens,
                    figma_gpui_runtime::TokenContext {
                        id: "stroke.left",
                        collection_id: Some("collection.theme"),
                        mode_id: Some("mode.light"),
                        modes: &[("collection.theme", "mode.light")],
                    },
                    4f32,
                ),
            ),
        )
        .border_color(
            figma_gpui_runtime::TokenResolver::color_with_context(
                tokens,
                figma_gpui_runtime::TokenContext {
                    id: "color.brand",
                    collection_id: Some("collection.theme"),
                    mode_id: Some("mode.light"),
                    modes: &[("collection.theme", "mode.light")],
                },
                gpui::Hsla::from(gpui::rgba(0x2040_80ff)),
            ),
        )
        .rounded_tl(
            gpui::px(
                figma_gpui_runtime::TokenResolver::number_with_context(
                    tokens,
                    figma_gpui_runtime::TokenContext {
                        id: "radius.top-left",
                        collection_id: Some("collection.theme"),
                        mode_id: Some("mode.light"),
                        modes: &[("collection.theme", "mode.light")],
                    },
                    8f32,
                ),
            ),
        )
        .rounded_tr(
            gpui::px(
                figma_gpui_runtime::TokenResolver::number_with_context(
                    tokens,
                    figma_gpui_runtime::TokenContext {
                        id: "radius.top-right",
                        collection_id: Some("collection.theme"),
                        mode_id: Some("mode.light"),
                        modes: &[("collection.theme", "mode.light")],
                    },
                    6f32,
                ),
            ),
        )
        .rounded_br(
            gpui::px(
                figma_gpui_runtime::TokenResolver::number_with_context(
                    tokens,
                    figma_gpui_runtime::TokenContext {
                        id: "radius.bottom-right",
                        collection_id: Some("collection.theme"),
                        mode_id: Some("mode.light"),
                        modes: &[("collection.theme", "mode.light")],
                    },
                    4f32,
                ),
            ),
        )
        .rounded_bl(
            gpui::px(
                figma_gpui_runtime::TokenResolver::number_with_context(
                    tokens,
                    figma_gpui_runtime::TokenContext {
                        id: "radius.bottom-left",
                        collection_id: Some("collection.theme"),
                        mode_id: Some("mode.light"),
                        modes: &[("collection.theme", "mode.light")],
                    },
                    2f32,
                ),
            ),
        )
}
