//! Compact layout for phone-width windows. The side docks collapse into a handle strip at each
//! window edge, so the drawing keeps the screen, and come up as slide-over drawers when a handle
//! is tapped, swiped inward or held: Tool Sets on the left, Layers above Properties on the right.
//! A tap on the dimmed drawing, or Escape, puts the drawer away again.

use std::time::Duration;

use craft_ui::docking::DockContent;
use egui::{Color32, Context, Id, Order, Pos2, Rect, Sense, Stroke, Ui, UiBuilder, Vec2, pos2, vec2};

use crate::docking::{Panel, Panels};

/// Windows narrower than this, in CSS pixels, use the compact layout.
pub const COMPACT_WIDTH: f32 = 720.0;
/// How much the whole interface is scaled up in the compact layout, so that its hand-drawn
/// controls (tabs, menus, Tool Sets icons, the command line) are finger-sized.
pub const COMPACT_ZOOM: f32 = 1.4;
/// Width of the handle strip at each window edge.
const HANDLE_WIDTH: f32 = 26.0;
/// How long a press on a handle must hold, in seconds, before its drawer opens.
const LONG_PRESS_SECS: f64 = 0.45;
/// How far a held press may drift, in points, and still count as a tap or a hold.
const PRESS_SLOP: f32 = 14.0;
/// How far, in points, a press on a handle must travel inward to open the drawer as a swipe.
const SWIPE_DISTANCE: f32 = 40.0;
/// A drawer's share of the window width, and its widest size in points.
const DRAWER_SHARE: f32 = 0.86;
const DRAWER_MAX_WIDTH: f32 = 340.0;
/// Layers' share of the right drawer's height; Properties takes the rest.
const LAYERS_SHARE: f32 = 0.45;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Drawer {
    #[default]
    Left,
    Right,
}

/// A press that began on a handle and is still down.
#[derive(Clone, Copy, Default)]
struct Press {
    side: Drawer,
    origin: Pos2,
    since: f64,
}

/// The drawer on screen. It ignores the dimmed drawing until the press that opened it is released.
#[derive(Clone, Copy, Default)]
struct Open {
    side: Drawer,
    armed: bool,
}

fn press_id() -> Id {
    Id::new("cadcraft-compact-press")
}

fn drawer_id() -> Id {
    Id::new("cadcraft-compact-drawer")
}

/// Whether a window this wide, in CSS pixels, uses the compact layout.
pub fn is_compact(width: f32) -> bool {
    width < COMPACT_WIDTH
}

/// The window's width in CSS pixels, whatever zoom is applied to the interface.
pub fn window_width(ctx: &Context) -> f32 {
    ctx.content_rect().width() * ctx.zoom_factor()
}

/// Whether the window is narrow enough for the compact layout. A window with no size yet is not.
pub fn window_is_compact(ctx: &Context) -> bool {
    let width = window_width(ctx);
    width > 50.0 && is_compact(width)
}

fn zoom_id() -> Id {
    Id::new("cadcraft-compact-zoom")
}

/// What to do with the interface zoom this frame.
#[derive(Debug, PartialEq)]
enum ZoomStep {
    /// The window just became compact: remember the zoom and scale up.
    Enter,
    /// The window just stopped being compact: put this zoom back.
    Leave(f32),
    Keep,
}

/// The zoom change for a window `width` CSS pixels wide, given the zoom saved on entering the
/// compact layout (`None` while it is not in use).
fn zoom_step(width: f32, saved: Option<f32>) -> ZoomStep {
    match (width > 50.0 && is_compact(width), saved) {
        (true, None) => ZoomStep::Enter,
        // A window with no size yet must not undo the zoom.
        (false, Some(before)) if width > 50.0 => ZoomStep::Leave(before),
        _ => ZoomStep::Keep,
    }
}

/// Scale the interface up while the window is compact, and put the zoom back (including one the
/// person set themselves) when it is not. Only changes the zoom when the layout switches.
pub fn apply_zoom(ctx: &Context) {
    trace(ctx);
    let saved = ctx.data(|d| d.get_temp::<f32>(zoom_id()));
    match zoom_step(window_width(ctx), saved) {
        ZoomStep::Enter => {
            // Read the zoom first: egui's context lock is not re-entrant, so asking the context
            // for anything from inside `data_mut` panics ("Parking not supported") on the web.
            let before = ctx.zoom_factor();
            ctx.data_mut(|d| d.insert_temp(zoom_id(), before));
            ctx.set_zoom_factor(COMPACT_ZOOM);
            ctx.request_repaint();
        }
        ZoomStep::Leave(before) => {
            ctx.data_mut(|d| d.remove_temp::<f32>(zoom_id()));
            ctx.set_zoom_factor(before);
            ctx.request_repaint();
        }
        ZoomStep::Keep => {}
    }
}

/// Log the window size, zoom and pixel ratio whenever one changes (diagnostics for phone sizing).
fn trace(ctx: &Context) {
    let rect = ctx.content_rect();
    let text = format!(
        "content {:.1}x{:.1} zoom {} ppp {} native {:?} css-width {:.1}",
        rect.width(),
        rect.height(),
        ctx.zoom_factor(),
        ctx.pixels_per_point(),
        ctx.native_pixels_per_point(),
        window_width(ctx)
    );
    let id = Id::new("cadcraft-compact-trace");
    let last = ctx.data(|d| d.get_temp::<String>(id));
    if last.as_deref() != Some(text.as_str()) {
        log::info!("compact: {text}");
        ctx.data_mut(|d| d.insert_temp(id, text));
    }
}

/// The left and right handle strips of `rect`.
fn handles(rect: Rect) -> (Rect, Rect) {
    let size = vec2(HANDLE_WIDTH, rect.height());
    (Rect::from_min_size(rect.min, size), Rect::from_min_size(pos2(rect.max.x - HANDLE_WIDTH, rect.min.y), size))
}

fn clear_press(ctx: &Context) {
    ctx.data_mut(|d| d.remove_temp::<Press>(press_id()));
}

/// Track a press that begins on a handle. A tap, a swipe inward or a long hold opens that handle's
/// drawer. Also returns how far along the hold is (0 to 1), for the handle to show.
fn handle_gesture(ctx: &Context, left: Rect, right: Rect) -> (Option<Drawer>, f32) {
    let (pressed, down, released, pos, origin, now) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.pointer.latest_pos(),
            i.pointer.press_origin(),
            i.time,
        )
    });
    let mut held = ctx.data(|d| d.get_temp::<Press>(press_id()));
    if pressed {
        held = pos.or(origin).and_then(|p| {
            if left.contains(p) {
                Some(Press { side: Drawer::Left, origin: p, since: now })
            } else if right.contains(p) {
                Some(Press { side: Drawer::Right, origin: p, since: now })
            } else {
                None
            }
        });
    }
    let Some(press) = held else {
        return (None, 0.0);
    };
    let drift = pos.map_or(Vec2::ZERO, |p| p - press.origin);
    let inward = match press.side {
        Drawer::Left => drift.x,
        Drawer::Right => -drift.x,
    };
    if inward >= SWIPE_DISTANCE {
        clear_press(ctx);
        return (Some(press.side), 1.0);
    }
    // Drifting away along the edge, or outward, is neither a tap nor a hold.
    if drift.length() > PRESS_SLOP && inward < drift.length() * 0.7 {
        clear_press(ctx);
        return (None, 0.0);
    }
    if !down {
        clear_press(ctx);
        // Letting go without drifting away is a tap.
        return (released.then_some(press.side), 0.0);
    }
    let progress = ((now - press.since) / LONG_PRESS_SECS).clamp(0.0, 1.0) as f32;
    if progress >= 1.0 {
        clear_press(ctx);
        return (Some(press.side), 1.0);
    }
    ctx.data_mut(|d| d.insert_temp(press_id(), press));
    ctx.request_repaint_after(Duration::from_millis(16));
    (None, progress)
}

/// Draw the drawing between two handle strips, and the open drawer over it.
pub fn show(app: &mut crate::CadApp, ui: &mut Ui) {
    let t = crate::theme::Tokens::get();
    let rect = ui.max_rect();
    let ctx = ui.ctx().clone();
    let (left, right) = handles(rect);
    let canvas = Rect::from_min_max(pos2(left.max.x, rect.min.y), pos2(right.min.x, rect.max.y));

    let drawer_open = ctx.data(|d| d.get_temp::<Open>(drawer_id())).is_some();
    let (fired, progress) = if drawer_open || app.shows_modal() {
        clear_press(&ctx);
        (None, 0.0)
    } else {
        handle_gesture(&ctx, left, right)
    };
    let pressing = ctx.data(|d| d.get_temp::<Press>(press_id()));
    for (strip, side) in [(left, Drawer::Left), (right, Drawer::Right)] {
        ui.painter().rect_filled(strip, 0.0, t.chrome);
        if pressing.is_some_and(|p| p.side == side) {
            ui.painter().rect_filled(strip, 0.0, t.accent.gamma_multiply(0.55 * progress));
        }
        let centre = strip.center();
        for step in [-1.0_f32, 0.0, 1.0] {
            let y = centre.y + step * 7.0;
            ui.painter().line_segment([pos2(centre.x - 5.0, y), pos2(centre.x + 5.0, y)], Stroke::new(2.0, t.text_dim));
        }
    }

    ui.scope_builder(UiBuilder::new().max_rect(canvas), |ui| {
        ui.set_clip_rect(canvas);
        Panels { app: &mut *app }.body(ui, &Panel::Canvas);
    });

    if let Some(side) = fired {
        ctx.data_mut(|d| d.insert_temp(drawer_id(), Open { side, armed: false }));
    }
    if let Some(open) = ctx.data(|d| d.get_temp::<Open>(drawer_id())) {
        drawer(app, &ctx, rect, open);
    }
}

/// The open drawer over a dimmed drawing; it closes on a tap outside it or on Escape.
fn drawer(app: &mut crate::CadApp, ctx: &Context, rect: Rect, open: Open) {
    let t = crate::theme::Tokens::get();
    let down = ctx.input(|i| i.pointer.primary_down());
    let width = (rect.width() * DRAWER_SHARE).min(DRAWER_MAX_WIDTH);
    let panel = match open.side {
        Drawer::Left => Rect::from_min_size(rect.min, vec2(width, rect.height())),
        Drawer::Right => Rect::from_min_size(pos2(rect.max.x - width, rect.min.y), vec2(width, rect.height())),
    };

    let scrim = egui::Area::new(Id::new("cadcraft-compact-scrim")).order(Order::Foreground).fixed_pos(rect.min).show(ctx, |ui| {
        let response = ui.allocate_rect(rect, Sense::click());
        ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(120));
        response
    });
    let mut close = open.armed && scrim.inner.clicked();
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }

    egui::Area::new(Id::new("cadcraft-compact-drawer-panel")).order(Order::Tooltip).fixed_pos(panel.min).show(ctx, |ui| {
        egui::Frame::NONE.fill(t.panel).stroke(Stroke::new(1.0, t.border)).show(ui, |ui| {
            ui.set_min_size(panel.size());
            ui.set_max_size(panel.size());
            let full = Rect::from_min_size(ui.next_widget_position(), panel.size());
            match open.side {
                Drawer::Left => {
                    ui.scope_builder(UiBuilder::new().max_rect(full), |ui| {
                        ui.set_clip_rect(full);
                        Panels { app: &mut *app }.body(ui, &Panel::ToolSets);
                    });
                }
                Drawer::Right => {
                    let split_y = full.min.y + full.height() * LAYERS_SHARE;
                    let top = Rect::from_min_max(full.min, pos2(full.max.x, split_y));
                    let bottom = Rect::from_min_max(pos2(full.min.x, split_y), full.max);
                    ui.scope_builder(UiBuilder::new().max_rect(top), |ui| {
                        ui.set_clip_rect(top);
                        Panels { app: &mut *app }.body(ui, &Panel::Layers);
                    });
                    ui.scope_builder(UiBuilder::new().max_rect(bottom), |ui| {
                        ui.set_clip_rect(bottom);
                        Panels { app: &mut *app }.body(ui, &Panel::Properties);
                    });
                    ui.painter().line_segment([top.left_bottom(), top.right_bottom()], Stroke::new(1.0, t.border));
                }
            }
        });
    });

    if close {
        ctx.data_mut(|d| d.remove_temp::<Open>(drawer_id()));
    } else {
        let next = Open { armed: open.armed || !down, ..open };
        ctx.data_mut(|d| d.insert_temp(drawer_id(), next));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ctx: &Context, events: Vec<egui::Event>, time: f64, f: &mut dyn FnMut(&Context)) {
        let raw =
            egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 800.0))), events, time: Some(time), ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| f(ui.ctx()));
        out.textures_delta.clear();
    }

    fn button(at: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::default() }
    }

    fn strips() -> (Rect, Rect) {
        handles(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 800.0)))
    }

    #[test]
    fn windows_under_the_threshold_are_compact() {
        assert!(is_compact(390.0));
        assert!(is_compact(COMPACT_WIDTH - 1.0));
        assert!(!is_compact(COMPACT_WIDTH));
        assert!(!is_compact(1400.0));
    }

    #[test]
    fn the_compact_zoom_comes_on_and_goes_off_with_the_window_width() {
        assert_eq!(zoom_step(1200.0, None), ZoomStep::Keep);
        assert_eq!(zoom_step(400.0, None), ZoomStep::Enter);
        // Still compact at the zoomed size: nothing to do.
        assert_eq!(zoom_step(400.0, Some(1.0)), ZoomStep::Keep);
        // Widening past the threshold puts back the zoom saved on the way in, even a custom one.
        assert_eq!(zoom_step(1200.0, Some(1.25)), ZoomStep::Leave(1.25));
        // A window with no size yet changes nothing, either way.
        assert_eq!(zoom_step(0.0, None), ZoomStep::Keep);
        assert_eq!(zoom_step(0.0, Some(1.0)), ZoomStep::Keep);
    }

    #[test]
    fn handles_sit_on_the_window_edges() {
        let (left, right) = strips();
        assert_eq!(left.min, Pos2::ZERO);
        assert_eq!(right.max, pos2(400.0, 800.0));
        assert!((left.width() - HANDLE_WIDTH).abs() < f32::EPSILON);
        assert!((right.width() - HANDLE_WIDTH).abs() < f32::EPSILON);
        assert!(left.max.x < right.min.x);
    }

    #[test]
    fn a_held_press_on_a_handle_opens_that_drawer() {
        let ctx = Context::default();
        let (left, right) = strips();
        let at = left.center();
        let mut seen = (None, 0.0);
        frame(&ctx, vec![egui::Event::PointerMoved(at)], 0.0, &mut |_| {});
        frame(&ctx, vec![button(at, true)], 0.1, &mut |c| seen = handle_gesture(c, left, right));
        assert_eq!(seen.0, None);
        frame(&ctx, Vec::new(), 0.3, &mut |c| seen = handle_gesture(c, left, right));
        assert_eq!(seen.0, None);
        assert!(seen.1 > 0.0 && seen.1 < 1.0);
        frame(&ctx, Vec::new(), 0.7, &mut |c| seen = handle_gesture(c, left, right));
        assert_eq!(seen.0, Some(Drawer::Left));
    }

    #[test]
    fn a_held_press_on_the_right_handle_opens_the_right_drawer() {
        let ctx = Context::default();
        let (left, right) = strips();
        let at = right.center();
        let mut seen = (None, 0.0);
        frame(&ctx, vec![egui::Event::PointerMoved(at)], 0.0, &mut |_| {});
        frame(&ctx, vec![button(at, true)], 0.1, &mut |c| seen = handle_gesture(c, left, right));
        frame(&ctx, Vec::new(), 0.7, &mut |c| seen = handle_gesture(c, left, right));
        assert_eq!(seen.0, Some(Drawer::Right));
    }

    #[test]
    fn a_press_on_the_drawing_never_opens_a_drawer() {
        let ctx = Context::default();
        let (left, right) = strips();
        let at = pos2(200.0, 400.0);
        let mut seen = (None, 0.0);
        frame(&ctx, vec![egui::Event::PointerMoved(at)], 0.0, &mut |_| {});
        frame(&ctx, vec![button(at, true)], 0.1, &mut |c| seen = handle_gesture(c, left, right));
        frame(&ctx, Vec::new(), 1.5, &mut |c| seen = handle_gesture(c, left, right));
        assert_eq!(seen.0, None);
    }

    #[test]
    fn a_tap_on_a_handle_opens_that_drawer() {
        let ctx = Context::default();
        let (left, right) = strips();
        let at = right.center();
        let mut seen = (None, 0.0);
        frame(&ctx, vec![egui::Event::PointerMoved(at)], 0.0, &mut |_| {});
        frame(&ctx, vec![button(at, true)], 0.1, &mut |c| seen = handle_gesture(c, left, right));
        assert_eq!(seen.0, None);
        frame(&ctx, vec![button(at, false)], 0.2, &mut |c| seen = handle_gesture(c, left, right));
        assert_eq!(seen.0, Some(Drawer::Right));
    }

    #[test]
    fn a_press_and_release_in_one_frame_is_a_tap() {
        let ctx = Context::default();
        let (left, right) = strips();
        let at = left.center();
        let mut seen = (None, 0.0);
        frame(&ctx, vec![egui::Event::PointerMoved(at)], 0.0, &mut |_| {});
        frame(&ctx, vec![button(at, true), button(at, false)], 0.1, &mut |c| seen = handle_gesture(c, left, right));
        assert_eq!(seen.0, Some(Drawer::Left));
    }

    #[test]
    fn a_swipe_inward_from_a_handle_opens_its_drawer() {
        let ctx = Context::default();
        let (left, right) = strips();
        let at = left.center();
        let mut seen = (None, 0.0);
        frame(&ctx, vec![egui::Event::PointerMoved(at)], 0.0, &mut |_| {});
        frame(&ctx, vec![button(at, true)], 0.1, &mut |c| seen = handle_gesture(c, left, right));
        frame(&ctx, vec![egui::Event::PointerMoved(at + vec2(SWIPE_DISTANCE + 4.0, 0.0))], 0.15, &mut |c| seen = handle_gesture(c, left, right));
        assert_eq!(seen.0, Some(Drawer::Left));
    }

    #[test]
    fn dragging_along_the_edge_or_outward_does_not_open() {
        let ctx = Context::default();
        let (left, right) = strips();
        let at = left.center();
        let mut seen = (None, 0.0);
        frame(&ctx, vec![egui::Event::PointerMoved(at)], 0.0, &mut |_| {});
        frame(&ctx, vec![button(at, true)], 0.1, &mut |c| seen = handle_gesture(c, left, right));
        frame(&ctx, vec![egui::Event::PointerMoved(at + vec2(0.0, PRESS_SLOP + 20.0))], 0.15, &mut |c| seen = handle_gesture(c, left, right));
        frame(&ctx, vec![button(at + vec2(0.0, PRESS_SLOP + 20.0), false)], 0.2, &mut |c| seen = handle_gesture(c, left, right));
        frame(&ctx, Vec::new(), 1.0, &mut |c| seen = handle_gesture(c, left, right));
        assert_eq!(seen.0, None);
    }
}
