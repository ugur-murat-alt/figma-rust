//! Compile fixture for the deterministic GPUI emitted by figma-rust codegen.

use figma_gpui_runtime::source_selector;
use gpui::{BoxShadow, InteractiveElement, IntoElement, ParentElement, Styled, div, px, rgb};

/// Golden output emitted by `figma-rust-codegen` from the core raw-to-IR fixture.
pub mod generated_basic {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/figma-rust-codegen/tests/fixtures/basic.gpui.rs"
    ));
}

/// All-hidden output proves generated imports stay warning-free.
pub mod generated_all_hidden {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/figma-rust-codegen/tests/fixtures/all-hidden.gpui.rs"
    ));
}

/// Output compiled from the real Figma GROUP fixture.
pub mod generated_real_group {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../real-figma/generated/generated.rs"
    ));
}

/// Output compiled from the synthetic SVG fallback fixture.
pub mod generated_asset_fallback {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../asset-fallback/generated/generated.rs"
    ));
}

/// Output compiled from horizontal and vertical child-alignment fixtures.
pub mod generated_child_alignment {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../child-alignment/generated/generated.rs"
    ));
}

/// Asset-backed generated output must not borrow its local resolvers.
pub fn generated_asset_fallback_view() -> impl IntoElement {
    let assets = figma_gpui_runtime::DirectoryAssets::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../asset-fallback/generated"
    ));
    generated_asset_fallback::generated_view(&figma_gpui_runtime::FallbackTokens, &assets)
}

/// Representative generated view for the first codegen slice.
pub fn generated_view() -> impl IntoElement {
    div()
        .debug_selector(|| source_selector(0))
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .items_center()
        .justify_center()
        .w(px(320.0))
        .h(px(180.0))
        .p(px(16.0))
        .gap(px(12.0))
        .overflow_hidden()
        .bg(rgb(0x00f8_fafc))
        .border_1()
        .border_color(rgb(0x00cb_d5e1))
        .rounded(px(12.0))
        .opacity(0.96)
        .shadow(vec![
            BoxShadow::new(px(0.0), px(4.0), rgb(0x000f_172a).opacity(0.18).into())
                .blur_radius(px(12.0))
                .spread_radius(px(1.0)),
        ])
        .child(
            div()
                .debug_selector(|| source_selector(1))
                .absolute()
                .left(px(12.0))
                .top(px(12.0))
                .w(px(8.0))
                .h(px(8.0))
                .rounded(px(4.0))
                .bg(rgb(0x0025_63eb)),
        )
        .child(
            div()
                .debug_selector(|| source_selector(2))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .child("Generated from Figma"),
        )
}

#[cfg(test)]
mod tests {
    use figma_gpui_runtime::{DirectoryAssets, FallbackTokens};
    use gpui::{
        Bounds, Context, IntoElement, Pixels, Render, TestAppContext, VisualTestContext, Window,
        px, size,
    };
    use serde_json::{Value, json};

    use super::generated_view;

    const REAL_GROUP_GEOMETRY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../real-figma/actual.geometry.json"
    ));
    const ROOT_SELECTOR: &str = "figma-node-0000";
    const CHILD_A_SELECTOR: &str = "figma-node-0001";
    const CHILD_B_SELECTOR: &str = "figma-node-0002";

    struct RealGroupView;

    struct ChildAlignmentView;

    impl Render for RealGroupView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            super::generated_real_group::generated_view(&FallbackTokens)
        }
    }

    impl Render for ChildAlignmentView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            super::generated_child_alignment::generated_view(&FallbackTokens)
        }
    }

    fn required_bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
        match cx.debug_bounds(selector) {
            Some(bounds) => bounds,
            None => panic!("missing GPUI debug bounds for {selector}"),
        }
    }

    fn geometry_node(parent: Option<&str>, sibling_index: u32, bounds: Bounds<Pixels>) -> Value {
        json!({
            "parent": parent,
            "sibling_index": sibling_index,
            "bounds": {
                "x": f32::from(bounds.origin.x),
                "y": f32::from(bounds.origin.y),
                "width": f32::from(bounds.size.width),
                "height": f32::from(bounds.size.height),
            },
            "clipped": false,
            "text_bounds": null,
        })
    }

    fn measured_geometry(
        root: Bounds<Pixels>,
        child_a: Bounds<Pixels>,
        child_b: Bounds<Pixels>,
    ) -> Value {
        json!({
            "capture": {
                "engine": "gpui-test-support",
                "gpui_revision": "5631830c564afa89b3aba679f45d9c3345f9460f",
                "selectors": {
                    "1:4": ROOT_SELECTOR,
                    "1:2": CHILD_A_SELECTOR,
                    "1:3": CHILD_B_SELECTOR,
                },
            },
            "viewport": [100.0, 60.0],
            "nodes": {
                "1:4": geometry_node(None, 0, root),
                "1:2": geometry_node(Some("1:4"), 0, child_a),
                "1:3": geometry_node(Some("1:4"), 1, child_b),
            },
        })
    }

    fn checked_in_geometry() -> Value {
        match serde_json::from_str(REAL_GROUP_GEOMETRY) {
            Ok(geometry) => geometry,
            Err(error) => panic!("invalid checked-in actual.geometry.json: {error}"),
        }
    }

    #[test]
    fn generated_view_constructs_without_an_application() {
        let _view = generated_view();
    }

    #[test]
    fn codegen_golden_constructs_without_an_application() {
        let _view = super::generated_basic::generated_view(&FallbackTokens);
    }

    #[test]
    fn all_hidden_codegen_constructs_without_unused_imports() {
        let _view = super::generated_all_hidden::generated_view(&FallbackTokens);
    }

    #[test]
    fn real_group_fixture_constructs_without_an_application() {
        let _view = super::generated_real_group::generated_view(&FallbackTokens);
    }

    #[test]
    fn asset_fallback_fixture_constructs_against_pinned_gpui() {
        let assets = DirectoryAssets::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../asset-fallback/generated"
        ));
        let _view = super::generated_asset_fallback::generated_view(&FallbackTokens, &assets);
    }

    #[test]
    fn child_alignment_fixture_constructs_against_pinned_gpui() {
        let _ =
            super::generated_child_alignment::generated_view(&FallbackTokens).into_any_element();
    }

    #[gpui::test]
    fn child_alignment_geometry_matches_counter_axis_contract(cx: &mut TestAppContext) {
        let window = cx.open_window(size(px(300.0), px(300.0)), |_, _| ChildAlignmentView);
        cx.run_until_parked();
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let horizontal_root = required_bounds(&mut cx, "figma-node-0000");
        let horizontal = [
            "figma-node-0001",
            "figma-node-0002",
            "figma-node-0003",
            "figma-node-0004",
            "figma-node-0005",
        ]
        .into_iter()
        .map(|selector| required_bounds(&mut cx, selector))
        .collect::<Vec<_>>();
        let vertical_root = required_bounds(&mut cx, "figma-node-0006");
        let vertical = [
            "figma-node-0007",
            "figma-node-0008",
            "figma-node-0009",
            "figma-node-0010",
            "figma-node-0011",
        ]
        .into_iter()
        .map(|selector| required_bounds(&mut cx, selector))
        .collect::<Vec<_>>();

        assert_eq!(
            horizontal
                .iter()
                .map(|bounds| (
                    f32::from(bounds.origin.y - horizontal_root.origin.y),
                    f32::from(bounds.size.height),
                ))
                .collect::<Vec<_>>(),
            vec![
                (0.0, 10.0),
                (0.0, 10.0),
                (35.0, 10.0),
                (70.0, 10.0),
                (0.0, 80.0)
            ],
        );
        assert_eq!(
            vertical
                .iter()
                .map(|bounds| (
                    f32::from(bounds.origin.x - vertical_root.origin.x),
                    f32::from(bounds.size.width),
                ))
                .collect::<Vec<_>>(),
            vec![
                (0.0, 20.0),
                (0.0, 20.0),
                (30.0, 20.0),
                (60.0, 20.0),
                (0.0, 80.0)
            ],
        );
    }

    #[gpui::test]
    fn real_group_geometry_is_measured_by_gpui(cx: &mut TestAppContext) {
        let window = cx.open_window(size(px(100.0), px(60.0)), |_, _| RealGroupView);
        cx.run_until_parked();
        let mut cx = VisualTestContext::from_window(window.into(), cx);

        let root = required_bounds(&mut cx, ROOT_SELECTOR);
        let child_a = required_bounds(&mut cx, CHILD_A_SELECTOR);
        let child_b = required_bounds(&mut cx, CHILD_B_SELECTOR);

        assert_eq!(
            measured_geometry(root, child_a, child_b),
            checked_in_geometry()
        );
    }
}
