pub fn generated_view<Tokens>(tokens: &Tokens) -> impl gpui::IntoElement + use<Tokens>
where
    Tokens: figma_gpui_runtime::TokenResolver,
{
    let _ = tokens;
    gpui::div()
}
