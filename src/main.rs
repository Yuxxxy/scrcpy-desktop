#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod scrcpy;

use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, hsla, px, size,
};
use gpui_component::theme::{Theme, ThemeMode};

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        gpui_component::init(cx);
        Theme::change(ThemeMode::Light, None, cx);
        Theme::update(cx, |theme| {
            // Two colors only: neutral ink/grey plus one accent.
            // gpui's hsla takes 0..1 components: 211deg / 100% / 50% = #007AFF.
            let primary = hsla(0.586, 1., 0.5, 1.);
            let primary_hover = hsla(0.586, 1., 0.42, 1.);
            let primary_active = hsla(0.586, 1., 0.35, 1.);
            theme.font_size = px(13.);
            theme.radius = px(6.);
            theme.primary = primary;
            // Switch and Checkbox read the token copy, not the field above.
            theme.tokens.primary = primary.into();
            theme.tokens.button_primary = primary.into();
            theme.tokens.button_primary_hover = primary_hover.into();
            theme.tokens.button_primary_active = primary_active.into();
        });

        let bounds = Bounds::centered(None, size(px(620.), px(340.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(520.), px(300.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Scrcpy 桌面版".into()),
                    ..Default::default()
                }),
                is_resizable: true,
                focus: true,
                ..Default::default()
            },
            |window, cx| cx.new(|cx| app::AppView::new(window, cx)),
        )
        .unwrap();

        cx.activate(true);
    });
}
