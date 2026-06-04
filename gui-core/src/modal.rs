//! §BEAUTY §6.7 — modal scrim + entrance motion.
//!
//! Stock egui `Window` modals float over live, undimmed content with no backdrop
//! (recon confirmed no scrim anywhere). This adds the two things that make a
//! modal read as *modal*: a translucent backdrop that dims and inerts the page,
//! and an eased entrance the caller fades the dialog content in with.
//!
//! `&mut`-free, `BuilderState`-free — both apps call it from their dialog router.

use egui::{Area, Color32, Context, Id, Order, Pos2, Sense};

use crate::design;

/// Paint a fading translucent backdrop for a modal and return the eased entrance
/// factor `t` in `[0,1]`.
///
/// Call **every frame** from the dialog router, passing whether a modal is
/// currently open. While `open`, the scrim sits in [`Order::Middle`] — above the
/// [`Order::Background`] panels it dims, and below the modal window, **which the
/// caller MUST place in [`Order::Foreground`]** (`Window::new(..).order(Order::Foreground)`).
/// egui's default `Window` order is also `Order::Middle`, and any newly-shown or
/// clicked `Area` auto-bubbles to the top of its order tier, so a `Middle` window
/// loses the z-fight to this scrim and ends up buried under an (invisible, during
/// fade-in) backdrop; `Foreground` is immune because the order enum is the primary
/// z-sort key. The scrim swallows pointer input so the page is
/// inert, and `t` ramps 0→1; when the modal closes it ramps back to 0 and fades
/// out, so the next open animates afresh. Drive the dialog's content opacity with
/// the returned `t` — `ui.set_opacity(t)` at the top of the `Window::show`
/// closure — for a fade-in entrance.
///
/// egui 0.29 has no blur, so this is a flat low-alpha black wash: restrained, not
/// a full black-out.
pub fn scrim(ctx: &Context, open: bool) -> f32 {
    let t = design::ease_in_out_cubic(ctx.animate_bool_with_time(
        Id::new("gui_core_modal_scrim"),
        open,
        design::MOTION_SLOW,
    ));
    if t <= 0.004 {
        return 0.0;
    }
    let alpha = (150.0 * t) as u8;
    Area::new(Id::new("gui_core_modal_scrim_area"))
        .order(Order::Middle)
        // Capture clicks only while genuinely open, so a brief fade-out doesn't
        // swallow the click that follows closing the dialog.
        .interactable(open)
        .fixed_pos(Pos2::ZERO)
        .show(ctx, |ui| {
            let screen = ctx.screen_rect();
            ui.painter()
                .rect_filled(screen, 0.0, Color32::from_black_alpha(alpha));
            if open {
                let _ = ui.allocate_rect(screen, Sense::click());
            }
        });
    t
}
