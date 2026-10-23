//! Small widgets shared by the views: icon buttons, tooltips, and the window-drag handle.

use gpui::{AnyView, App, Context, Div, InteractiveElement, MouseButton, Stateful, Window, div, prelude::*, px, svg};

use crate::ui::theme::Theme;

gpui::actions!(widgets, [Press]);

/// Icon button of the rail and of panel headers. Tab reaches it, a ring shows it has the keys, Enter
/// or Space presses it; a click does not take the focus from the note.
pub fn button(id: impl Into<gpui::ElementId>, icon: &'static str, active: bool, t: Theme) -> Stateful<Div> {
    button_turned(id, icon, active, false, t)
}

/// The same, its icon turned half a way round: one drawing for the two ends of an arrow.
pub fn button_turned(id: impl Into<gpui::ElementId>, icon: &'static str, active: bool, turned: bool, t: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .tab_index(0)
        .focus(|s| s.border_2().border_color(t.accent))
        // Enter belongs to the button that has the keys, not to the panel around it: gpui presses
        // it when the key comes up.
        .key_context("Button")
        .on_action(|_: &Press, _, _| {})
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .hover(|s| s.bg(t.border))
        .active(|s| s.bg(t.selection))
        .when(active, |d| d.bg(t.selection))
        .child(
            svg()
                .path(icon)
                .size(px(16.))
                .flex_none()
                .when(turned, |s| s.with_transformation(gpui::Transformation::rotate(gpui::radians(std::f32::consts::PI))))
                .text_color(if active { t.accent } else { t.soft }),
        )
}

/// A shortcut ("Ctrl+Maj+C") as one badge per key, in a row; nothing when it is empty.
/// `on`: the row behind is selected, so the badges are lighter than it.
pub fn keycaps(keys: &str, t: Theme, on: bool) -> Div {
    let caps = crate::logic::commands::keycaps(keys).into_iter().map(|key| {
        div()
            .min_w(px(20.))
            .px(px(6.))
            .rounded(px(4.))
            .flex()
            .justify_center()
            .bg(if on { t.panel.opacity(0.7) } else { t.code_bg })
            .text_size(px(11.5))
            .text_color(if on { t.text } else { t.soft })
            .child(key)
    });
    div().flex().items_center().gap(px(3.)).children(caps)
}

/// Tooltip of an icon button: what it does and, if it has one, its shortcut. Without it,
/// a bare icon forces you to know the documentation to know where to click.
pub struct Tip {
    label: &'static str,
    keys: String,
    theme: Theme,
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        div()
            .px_2()
            .py(px(3.))
            .rounded(px(6.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .shadow_sm()
            .text_size(px(12.))
            .text_color(t.text)
            .flex()
            .items_center()
            .gap_2()
            .child(self.label)
            .child(keycaps(&self.keys, t, false))
    }
}

/// To put on a button: `.tooltip(tip(label, keys, theme))`.
pub fn tip(label: &'static str, keys: String, theme: Theme) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |_, cx| cx.new(|_| Tip { label, keys: keys.clone(), theme }).into()
}
/// Round icon button: window controls, note copy.
pub fn icon_button(id: &'static str, icon: &'static str, t: Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .hover(|s| s.bg(t.border))
        .active(|s| s.bg(t.selection))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(svg().path(icon).size(px(16.)).flex_none().text_color(t.text))
}

/// Turns an element into a window handle, where the app draws its title bar
/// (Linux): dragging moves it, double-click maximizes it, right-click opens the system menu.
pub fn drag_window<E: InteractiveElement>(element: E) -> E {
    element
        .on_mouse_down(MouseButton::Left, |e, window, _| {
            if e.click_count == 2 {
                window.zoom_window()
            } else {
                window.start_window_move()
            }
        })
        .on_mouse_down(MouseButton::Right, |e, window, _| window.show_window_menu(e.position))
}
