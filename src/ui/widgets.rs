//! Small widgets shared by the views: icon buttons, tooltips, and the window-drag handle.

use gpui::{AnyView, App, Context, Div, InteractiveElement, MouseButton, SharedString, Stateful, Window, div, prelude::*, svg};

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
        .size(t.px(28.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(t.small())
        .cursor_pointer()
        .tab_index(0)
        .focus(|s| s.border_2().border_color(t.accent))
        // Enter belongs to the button that has the keys, not to the panel around it: gpui presses
        // it when the key comes up.
        .key_context("Button")
        .on_action(|_: &Press, _, _| {})
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .hover(|s| s.bg(t.hover))
        .active(|s| s.bg(t.pressed))
        .when(active, |d| d.bg(t.selection))
        .child(
            svg()
                .path(icon)
                .size(t.px(16.))
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
            .min_w(t.px(20.))
            .px(t.px(6.))
            .rounded(t.small())
            .flex()
            .justify_center()
            .bg(if on { t.panel.opacity(0.7) } else { t.code_bg })
            .text_size(t.px(11.5))
            .text_color(if on { t.text } else { t.soft })
            .child(key)
    });
    div().flex().items_center().gap(t.px(3.)).children(caps)
}

/// Tooltip of an icon button: what it does and, if it has one, its shortcut. Without it,
/// a bare icon forces you to know the documentation to know where to click.
pub struct Tip {
    label: SharedString,
    keys: String,
    theme: Theme,
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        div()
            .px_2()
            .py(t.px(3.))
            .rounded(t.small())
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .shadow(t.lifted())
            .text_size(t.px(12.))
            .text_color(t.text)
            .flex()
            .items_center()
            .gap_2()
            .child(self.label.clone())
            .child(keycaps(&self.keys, t, false))
    }
}

/// To put on a button: `.tooltip(tip(label, keys, theme))`.
pub fn tip(label: impl Into<SharedString>, keys: String, theme: Theme) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let label = label.into();
    move |_, cx| cx.new(|_| Tip { label: label.clone(), keys: keys.clone(), theme }).into()
}
/// Round icon button: window controls, note copy; 24 px, the smallest target a finger or a shaky hand hits.
pub fn icon_button(id: &'static str, icon: &'static str, t: Theme) -> gpui::Stateful<gpui::Div> {
    icon_button_over(id, icon, t.hover, t)
}

/// The same, with its own colour under the pointer (the close button's red).
pub fn icon_button_over(id: &'static str, icon: &'static str, over: gpui::Hsla, t: Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .size(t.px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .hover(|s| s.bg(over))
        .active(|s| s.bg(t.pressed))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(svg().path(icon).size(t.px(16.)).flex_none().text_color(t.text))
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

/// The frame of a one-line field (the find bar, the vault search): `accent` border when it has the keys, the
/// hint in `soft` while it is empty.
pub fn field_frame(id: &'static str, field: impl IntoElement, on: bool, empty: bool, hint: &'static str, t: Theme) -> Div {
    div()
        .debug_selector(move || id.to_string())
        .flex_1()
        .min_w(t.px(80.))
        .h(t.px(26.))
        .px_2()
        .rounded(t.small())
        .border_1()
        .bg(t.bg)
        .border_color(if on { t.accent } else { t.border })
        .relative()
        .flex()
        .items_center()
        .overflow_hidden()
        .child(field)
        .when(empty, |d| d.child(div().absolute().left_2().text_color(t.soft).child(hint)))
}

/// A search setting shown as its sign (`Aa`, `.*`): lit when on, its name and keys in the tooltip.
pub fn toggle(id: &'static str, label: &'static str, on: bool, name: &'static str, keys: String, t: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .flex_none()
        .h(t.px(26.))
        .min_w(t.px(26.))
        .px_1p5()
        .rounded(t.small())
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(if on { t.accent } else { t.soft })
        .when(on, |d| d.bg(t.selection))
        .hover(|s| s.bg(t.hover))
        .tooltip(tip(name, keys, t))
        .child(label)
}

/// The header of a group of rows (a note of the vault search, later a group of a collection): a chevron that
/// folds it, its name, how many rows it holds in `soft`.
pub fn group_header(id: impl Into<gpui::ElementId>, name: impl Into<SharedString>, count: usize, folded: bool, t: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .h(t.px(26.))
        .px_1()
        .flex()
        .items_center()
        .gap_1()
        .rounded(t.small())
        .cursor_pointer()
        .text_size(t.px(13.))
        .hover(|s| s.bg(t.hover))
        .child(svg().path(if folded { "chevron-right.svg" } else { "chevron-down.svg" }).size(t.px(14.)).flex_none().text_color(t.soft))
        .child(div().flex_1().min_w_0().truncate().text_color(t.text).child(name.into()))
        .child(div().flex_none().text_size(t.px(11.5)).text_color(t.soft).child(count.to_string()))
}

/// How long a fade lasts: under a key repeat, so a new gesture never waits for the last one.
pub const FADE: std::time::Duration = std::time::Duration::from_millis(130);

/// `element` fades in when it first appears under `id` (the state lives as long as it is drawn);
/// with `motion=off` it is drawn as is. Opacity only, so the layout never moves.
pub fn fade_in(element: impl IntoElement + Styled + 'static, id: impl Into<gpui::ElementId>, t: &Theme) -> gpui::AnyElement {
    use gpui::{Animation, AnimationExt, ease_out_quint};
    match t.motion {
        true => element
            .with_animation(id, Animation::new(FADE).with_easing(ease_out_quint()), |e, delta| e.opacity(delta))
            .into_any_element(),
        false => element.into_any_element(),
    }
}
