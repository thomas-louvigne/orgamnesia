//! The graph extension (beta), in the main area: every page as a ball in space,
//! a line for each link between two pages, drawn in perspective on a canvas
//! (thousands of SVG elements were too slow). A click opens the page; dragging
//! moves the graph, dragging with the right button turns around it (around the
//! page pressed, if any); the wheel or the bar zooms; play turns it on its own.

use leptos::{ev, prelude::*};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::{actions, i18n::t, invoke, state::AppCtx};
use orgamnesia_core::Graph;

use super::panel;

type P3 = [f64; 3];

/// Zoom limits, as powers of two of the zoom showing the whole graph.
const ZOOM_MIN: f64 = -3.0;
const ZOOM_MAX: f64 = 5.0;
/// How far the camera looks down at most, either way (radians): past it, up turns over.
const PITCH_MAX: f64 = 1.5;
/// Turn of the play button, per frame (radians).
const SPIN: f64 = 0.004;
/// Shades of the pages, from far (faint) to near: the pages of a shade and a colour are filled at once.
const SHADES: usize = 4;
/// Names drawn at most, the nearest ones.
const NAMES_MAX: usize = 300;

/// Radius of a page with `degree` links, in units of the drawing.
fn radius(degree: usize) -> f64 {
    4.0 + (degree as f64).sqrt() * 2.0
}

/// Where the camera is: looking at `target` from `distance` away, turned by
/// `yaw` around the vertical and tilted by `pitch`; `focal` makes the distance
/// of a point to the camera pixels.
#[derive(Clone, Copy, PartialEq)]
struct Camera {
    target: P3,
    yaw: f64,
    pitch: f64,
    distance: f64,
    focal: f64,
}

impl Camera {
    /// The point `p` as the camera sees it: (x, y) right and down from the
    /// middle of the screen, and z away from the camera.
    fn view(&self, p: &P3) -> P3 {
        let q: P3 = std::array::from_fn(|k| p[k] - self.target[k]);
        let (sy, cy, sp, cp) = (self.yaw.sin(), self.yaw.cos(), self.pitch.sin(), self.pitch.cos());
        let (x, z) = (q[0] * cy + q[2] * sy, -q[0] * sy + q[2] * cy);
        [x, q[1] * cp - z * sp, q[1] * sp + z * cp + self.distance]
    }

    /// Where `p` is on a screen `w` × `h`, and how many pixels a unit of the
    /// drawing makes there; None behind the camera.
    fn project(&self, p: &P3, w: f64, h: f64) -> Option<(f64, f64, f64)> {
        let [x, y, z] = self.view(p);
        (z > self.distance * 0.02).then(|| {
            let s = self.focal / z;
            (w / 2.0 + x * s, h / 2.0 + y * s, s)
        })
    }

    /// The move in space that shows as (dx, dy) pixels on the screen at the target.
    fn unproject(&self, dx: f64, dy: f64) -> P3 {
        let (x, y) = (dx * self.distance / self.focal, dy * self.distance / self.focal);
        let (sy, cy, sp, cp) = (self.yaw.sin(), self.yaw.cos(), self.pitch.sin(), self.pitch.cos());
        let (y1, z1) = (y * cp, -y * sp);
        [x * cy - z1 * sy, y1, x * sy + z1 * cy]
    }
}

/// The middle of the points, and how far from it the farthest one is.
fn ball(pos: &[P3]) -> (P3, f64) {
    if pos.is_empty() { return ([0.0; 3], 100.0); }
    let mid: P3 = std::array::from_fn(|k| pos.iter().map(|p| p[k]).sum::<f64>() / pos.len() as f64);
    let far = pos.iter().map(|p| (0..3).map(|k| (p[k] - mid[k]).powi(2)).sum::<f64>().sqrt()).fold(0.0, f64::max);
    (mid, far.max(60.0) + 30.0)
}

/// What is drawn: the pages and their links, where each page is, how many links it has.
struct Drawn {
    graph: Graph,
    degree: Vec<usize>,
    /// The middle of the graph and its radius.
    mid: P3,
    size: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum Load { Reading, Empty, Ready }

/// Count the frames the window draws, and put how many a second in `fps`, until it is gone.
fn count_frames(fps: RwSignal<u32>, frames: u32, since: f64) {
    request_animation_frame(move || {
        let now = js_sys::Date::now();
        if now - since < 1000.0 {
            if fps.try_get_untracked().is_some() { count_frames(fps, frames + 1, since); }
        } else if fps.try_set((frames + 1) * 1000 / (now - since) as u32).is_none() {
            count_frames(fps, 0, now);
        }
    });
}

#[component]
pub fn GraphPage() -> impl IntoView {
    let ctx = expect_context::<AppCtx>();
    let lang = move || ctx.lang.get();
    let close = move || ctx.ui.show_graph.set(false);
    let root = NodeRef::<leptos::html::Div>::new();
    panel::opened(ctx, root, close);

    // Not a signal: drawn by hand, never compared
    let drawn = StoredValue::new(None::<Drawn>);
    let load = RwSignal::new(Load::Reading);
    let canvas = NodeRef::<leptos::html::Canvas>::new();
    let size = move || canvas.get_untracked().map_or((1.0, 1.0), |el| (el.client_width().max(1) as f64, el.client_height().max(1) as f64));
    // The zoom (log2) sets the distance of the camera; the target can glide to a page
    let zoom = RwSignal::new(0.0_f64);
    let yaw = RwSignal::new(0.6_f64);
    let pitch = RwSignal::new(-0.35_f64);
    let target = RwSignal::new([0.0; 3]);
    let goal = StoredValue::new(None::<P3>);
    // The colours of the theme: border, page, lone page, accent, text
    let palette = StoredValue::new(None::<[String; 5]>);
    let playing = RwSignal::new(false);
    let hovered = RwSignal::new(None::<usize>);
    let fps = RwSignal::new(0_u32);
    count_frames(fps, 0, js_sys::Date::now());

    // The camera as it is now: the whole graph fills the screen at zoom 0
    let camera = move || {
        let (w, h) = size();
        let r = drawn.with_value(|d| d.as_ref().map_or(100.0, |d| d.size));
        let base = r * 3.0;
        Camera {
            target: target.get_untracked(),
            yaw: yaw.get_untracked(),
            pitch: pitch.get_untracked(),
            distance: base / zoom.get_untracked().exp2(),
            focal: w.min(h) * 0.45 * base / r,
        }
    };
    let reset = move || {
        if let Some(mid) = drawn.with_value(|d| d.as_ref().map(|d| d.mid)) { target.set(mid); }
        goal.set_value(None);
        palette.set_value(None);
        zoom.set(0.0);
        yaw.set(0.6);
        pitch.set(-0.35);
    };
    spawn_local(async move {
        match invoke::get_graph().await {
            Ok(graph) => {
                let mut degree = vec![0; graph.pages.len()];
                for &(a, b) in &graph.links { degree[a] += 1; degree[b] += 1; }
                let empty = graph.pages.is_empty();
                let (mid, size) = ball(&graph.pos);
                if drawn.try_set_value(Some(Drawn { graph, degree, mid, size })).is_some() { return; }
                reset();
                load.set(if empty { Load::Empty } else { Load::Ready });
            }
            Err(e) => {
                ctx.error("graph_error", &e);
                let _ = ctx.ui.show_graph.try_set(false);
            }
        }
    });
    let current = Memo::new(move |_| ctx.work.active_tab_data().map(|t| t.path));

    // The page under the pixel (x, y) of the canvas, the nearest to the camera
    let page_at = move |x: f64, y: f64| -> Option<usize> {
        let (cam, (w, h)) = (camera(), size());
        drawn.with_value(|d| {
            let d = d.as_ref()?;
            d.graph.pos.iter().enumerate()
                .filter_map(|(i, p)| {
                    let (px, py, s) = cam.project(p, w, h)?;
                    let r = (radius(d.degree[i]) * s).max(3.0) + 2.0;
                    ((px - x).powi(2) + (py - y).powi(2) <= r * r).then_some((i, s))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        })
    };

    // The button held (0 left, 2 right) and where the mouse was; whether it moved enough to be a drag
    let down = StoredValue::new(None::<(i16, i32, i32)>);
    let dragged = RwSignal::new(false);
    let paint = move || {
        let Some(el) = canvas.get_untracked() else { return };
        let (w, h) = size();
        let dpr = window().device_pixel_ratio();
        let (pw, ph) = ((w * dpr) as u32, (h * dpr) as u32);
        if el.width() != pw || el.height() != ph { el.set_width(pw); el.set_height(ph); }
        let Some(c) = el.get_context("2d").ok().flatten().and_then(|c| c.dyn_into::<web_sys::CanvasRenderingContext2d>().ok()) else { return };
        // Read from the theme once, not each frame
        if palette.with_value(Option::is_none) {
            let style = window().get_computed_style(&el).ok().flatten();
            let color = |name: &str| style.as_ref().and_then(|s| s.get_property_value(name).ok()).unwrap_or_default().trim().to_string();
            palette.set_value(Some(["--border", "--fg-dim", "--orange", "--accent", "--fg"].map(color)));
        }
        let Some([link_color, dim, lone, accent, text]) = palette.get_value() else { return };
        let _ = c.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
        c.clear_rect(0.0, 0.0, w, h);
        let cam = camera();
        // Pixels per unit at the target: what is nearer is drawn full, what is farther fainter
        let at_target = cam.focal / cam.distance;
        let (hover, shown) = (hovered.get_untracked(), current.get_untracked());
        let moving = playing.get_untracked() || goal.with_value(Option::is_some)
            || (dragged.get_untracked() && down.with_value(Option::is_some));
        drawn.with_value(|d| {
            let Some(d) = d.as_ref() else { return };
            let at: Vec<Option<(f64, f64, f64)>> = d.graph.pos.iter().map(|p| cam.project(p, w, h)).collect();
            // All the links in one stroke
            c.begin_path();
            for &(a, b) in &d.graph.links {
                let (Some((ax, ay, _)), Some((bx, by, _))) = (at[a], at[b]) else { continue };
                c.move_to(ax, ay);
                c.line_to(bx, by);
            }
            c.set_line_width(1.0);
            c.set_stroke_style_str(&link_color);
            c.stroke();
            // The pages by shade and colour, one fill each instead of one per page; the
            // far shades first, so the near pages are drawn over them
            let is_shown = |i: usize| shown.as_deref() == Some(d.graph.pages[i].path.as_str());
            let mut groups = vec![Vec::new(); SHADES * 3];
            for (i, p) in at.iter().enumerate() {
                let Some((x, y, s)) = *p else { continue };
                let r = (radius(d.degree[i]) * s).max(1.5);
                if x < -r || y < -r || x > w + r || y > h + r { continue; }
                let kind = if hover == Some(i) || is_shown(i) { 2 } else if d.degree[i] == 0 { 1 } else { 0 };
                let shade = (((s / at_target).clamp(0.3, 1.0) - 0.3) / 0.7 * (SHADES - 1) as f64).round() as usize;
                groups[shade * 3 + kind].push((x, y, r));
            }
            let colors = [&dim, &lone, &accent];
            for (g, dots) in groups.iter().enumerate() {
                if dots.is_empty() { continue; }
                c.set_global_alpha(0.3 + 0.7 * (g / 3) as f64 / (SHADES - 1) as f64);
                c.set_fill_style_str(colors[g % 3]);
                c.begin_path();
                for &(x, y, r) in dots {
                    // A dot of a few pixels: a square looks the same, and costs less than a circle
                    if r < 2.0 {
                        c.rect(x - r, y - r, 2.0 * r, 2.0 * r);
                    } else {
                        c.move_to(x + r, y);
                        let _ = c.arc(x, y, r, 0.0, std::f64::consts::TAU);
                    }
                }
                c.fill();
            }
            c.set_global_alpha(1.0);
            // The pages linked to the page shown, or linking to it: their names stay while it moves
            let mut near = vec![false; d.graph.pages.len()];
            if let Some(me) = (0..near.len()).find(|&i| is_shown(i)) {
                for &(a, b) in &d.graph.links {
                    if a == me { near[b] = true } else if b == me { near[a] = true }
                }
            }
            // The names big enough to be read (6 px), the nearest ones; while the graph
            // moves, only the hovered page's, the page shown's and its neighbours'
            let mut names: Vec<(usize, f64)> = at.iter().enumerate()
                .filter_map(|(i, p)| {
                    let (x, y, s) = (*p)?;
                    let pinned = hover == Some(i) || is_shown(i) || near[i];
                    let font = if pinned { (10.0 * s).clamp(13.0, 15.0) } else { (10.0 * s).min(15.0) }.round();
                    let seen = x >= -100.0 && y >= -20.0 && x <= w + 100.0 && y <= h + 20.0;
                    (seen && (pinned || (!moving && font >= 6.0))).then_some((i, font))
                })
                .collect();
            if names.len() > NAMES_MAX {
                names.sort_by(|a, b| b.1.total_cmp(&a.1));
                names.truncate(NAMES_MAX);
            }
            // By size, so the font changes only a few times
            names.sort_by(|a, b| a.1.total_cmp(&b.1));
            c.set_text_align("center");
            c.set_fill_style_str(&text);
            let mut font_now = 0.0;
            for (i, font) in names {
                let Some((x, y, s)) = at[i] else { continue };
                if font != font_now {
                    c.set_font(&format!("{font}px system-ui, sans-serif"));
                    font_now = font;
                }
                let _ = c.fill_text(&d.graph.pages[i].name, x, y + (radius(d.degree[i]) * s).max(1.5) + font);
            }
        });
    };
    // Drawn again at most once a frame, whatever changed in between; while
    // playing or gliding to a page, each frame moves on
    let scheduled = StoredValue::new(false);
    let redraw = move || {
        if scheduled.get_value() { return; }
        scheduled.set_value(true);
        request_animation_frame(move || {
            // Closed meanwhile
            if scheduled.try_set_value(false).is_some() { return; }
            if let Some(g) = goal.get_value() {
                let t = target.get_untracked();
                let next: P3 = std::array::from_fn(|k| t[k] + (g[k] - t[k]) * 0.2);
                let done = (0..3).all(|k| (g[k] - next[k]).abs() < 0.5);
                target.set(if done { g } else { next });
                if done { goal.set_value(None); }
            }
            if playing.get_untracked() { yaw.update(|y| *y += SPIN); }
            paint();
        });
    };
    Effect::new(move |_| {
        // Read for the changes they bring, not for their values
        let _ = (canvas.get(), load.get(), target.get(), yaw.get(), pitch.get(), zoom.get(), hovered.get(), current.get(), playing.get());
        redraw();
    });

    let zoom_by = move |step: f64| zoom.update(|z| *z = (*z + step).clamp(ZOOM_MIN, ZOOM_MAX));
    let on_wheel = move |e: ev::WheelEvent| {
        e.prevent_default();
        // A wheel notch (~100 px) zooms by about 10%; a touchpad's small steps, smoothly
        zoom_by(-e.delta_y().clamp(-100.0, 100.0) * 0.0015);
    };
    let on_move = move |e: ev::MouseEvent| {
        let Some((button, px, py)) = down.get_value() else {
            let at = page_at(e.offset_x() as f64, e.offset_y() as f64);
            if hovered.get_untracked() != at { hovered.set(at); }
            return;
        };
        let (dx, dy) = ((e.client_x() - px) as f64, (e.client_y() - py) as f64);
        if !dragged.get_untracked() && dx.abs() + dy.abs() < 4.0 { return; }
        down.set_value(Some((button, e.client_x(), e.client_y())));
        if !dragged.get_untracked() { dragged.set(true); hovered.set(None); }
        if button == 2 {
            yaw.update(|y| *y -= dx * 0.008);
            pitch.update(|p| *p = (*p + dy * 0.008).clamp(-PITCH_MAX, PITCH_MAX));
        } else {
            goal.set_value(None);
            let m = camera().unproject(dx, dy);
            target.update(|t| for k in 0..3 { t[k] -= m[k]; });
        }
    };
    let on_down = move |e: ev::MouseEvent| {
        let button = e.button();
        if button != 0 && button != 2 { return; }
        e.prevent_default();
        if let Some(c) = canvas.get_untracked() { let _ = c.focus(); }
        down.set_value(Some((button, e.client_x(), e.client_y())));
        dragged.set(false);
        // The right button on a page: turn around that page, brought to the middle
        if button == 2 && let Some(i) = page_at(e.offset_x() as f64, e.offset_y() as f64) {
            goal.set_value(drawn.with_value(|d| d.as_ref().map(|d| d.graph.pos[i])));
            redraw();
        }
    };
    let open = move |i: usize| {
        let file = drawn.with_value(|d| d.as_ref().map(|d| d.graph.pages[i].clone()));
        if let Some(file) = file { actions::open_file(ctx, file, None); }
    };
    let on_click = move |e: ev::MouseEvent| {
        if dragged.get_untracked() { return; }
        if let Some(i) = page_at(e.offset_x() as f64, e.offset_y() as f64) { open(i); }
    };
    let on_key = move |e: ev::KeyboardEvent| {
        match e.key().as_str() {
            // Turn around, as with the right button
            "ArrowLeft" => yaw.update(|y| *y += 0.1),
            "ArrowRight" => yaw.update(|y| *y -= 0.1),
            "ArrowUp" => pitch.update(|p| *p = (*p - 0.1).max(-PITCH_MAX)),
            "ArrowDown" => pitch.update(|p| *p = (*p + 0.1).min(PITCH_MAX)),
            "+" | "=" => zoom_by(0.25),
            "-" => zoom_by(-0.25),
            " " => playing.update(|p| *p = !*p),
            "Enter" => { if let Some(i) = hovered.get_untracked() { open(i); } }
            _ => return,
        }
        e.prevent_default();
    };

    let esc = window_event_listener(ev::keydown, move |e| {
        if e.key() == "Escape" && ctx.ui.confirm.with_untracked(Option::is_none) { close(); }
    });
    // The names come back once the graph stops moving
    let up = window_event_listener(ev::mouseup, move |_| if down.with_value(Option::is_some) {
        down.set_value(None);
        redraw();
    });
    let resize = window_event_listener(ev::resize, move |_| redraw());
    on_cleanup(move || { esc.remove(); up.remove(); resize.remove(); });

    view! {
        <div node_ref=root class="main-panel" tabindex="-1" role="region" aria-label=move || t("graph_title", lang())>
            <div class="modal-header">
                <h2>{move || t("graph_title", lang())} " " <span class="graph-beta">{move || t("graph_beta", lang())}</span></h2>
                <button class="btn-close" aria-label=move || t("close", lang()) on:click=move |_| close()>"×"</button>
            </div>
            <div class="modal-body graph-body">
                {move || match load.get() {
                    Load::Reading => view! { <p class="setting-hint">{t("graph_reading", lang())}</p> }.into_any(),
                    Load::Empty => view! { <p class="setting-hint">{t("graph_empty", lang())}</p> }.into_any(),
                    Load::Ready => view! {
                        <canvas node_ref=canvas class="graph-canvas" tabindex="0"
                            class:pointing=move || hovered.get().is_some()
                            class:dragging=move || dragged.get()
                            aria-label=move || t("graph_title", lang())
                            on:wheel=on_wheel on:mousemove=on_move on:mousedown=on_down on:click=on_click
                            on:keydown=on_key on:mouseleave=move |_| hovered.set(None) />
                        <div class="graph-fps" aria-hidden="true">{move || format!("{} {}", fps.get(), t("graph_fps", lang()))}</div>
                        <div class="graph-tools" role="group" aria-label=move || t("graph_zoom", lang())>
                            <button class="btn-secondary"
                                aria-pressed=move || playing.get().to_string()
                                aria-label=move || t(if playing.get() { "graph_stop" } else { "graph_play" }, lang())
                                on:click=move |_| playing.update(|p| *p = !*p)>
                                {move || if playing.get() { "■" } else { "▶" }}
                            </button>
                            <button class="btn-secondary" aria-label=move || t("graph_zoom_out", lang())
                                on:click=move |_| zoom_by(-0.25)>"−"</button>
                            <input type="range" min=ZOOM_MIN max=ZOOM_MAX step="0.05"
                                aria-label=move || t("graph_zoom", lang())
                                prop:value=move || zoom.get().to_string()
                                on:input=move |e| { if let Ok(z) = event_target_value(&e).parse::<f64>() { zoom.set(z.clamp(ZOOM_MIN, ZOOM_MAX)) } } />
                            <button class="btn-secondary" aria-label=move || t("graph_zoom_in", lang())
                                on:click=move |_| zoom_by(0.25)>"+"</button>
                            <button class="btn-secondary" on:click=move |_| reset()>{move || t("graph_reset", lang())}</button>
                        </div>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::Camera;

    fn cam(yaw: f64, pitch: f64) -> Camera {
        Camera { target: [10.0, 20.0, 30.0], yaw, pitch, distance: 500.0, focal: 400.0 }
    }

    #[test]
    fn the_target_is_in_the_middle_whatever_the_turn() {
        for (yaw, pitch) in [(0.0, 0.0), (1.2, -0.4), (-2.5, 1.4)] {
            let (x, y, s) = cam(yaw, pitch).project(&[10.0, 20.0, 30.0], 800.0, 600.0).unwrap();
            assert!((x - 400.0).abs() < 1e-9 && (y - 300.0).abs() < 1e-9 && (s - 0.8).abs() < 1e-9);
        }
    }

    /// Moving the target by what `unproject` gives moves the drawing by those pixels.
    #[test]
    fn dragging_moves_by_the_pixels_dragged() {
        for (yaw, pitch) in [(0.0, 0.0), (0.7, -0.5), (-2.0, 1.2)] {
            let c = cam(yaw, pitch);
            let m = c.unproject(30.0, -12.0);
            let moved = Camera { target: std::array::from_fn(|k| c.target[k] - m[k]), ..c };
            let (x, y, _) = moved.project(&c.target, 800.0, 600.0).unwrap();
            assert!((x - 430.0).abs() < 1e-6 && (y - 288.0).abs() < 1e-6, "{x} {y}");
        }
    }

    #[test]
    fn behind_the_camera_is_not_drawn() {
        assert!(cam(0.0, 0.0).project(&[10.0, 20.0, -1000.0], 800.0, 600.0).is_none());
    }
}
