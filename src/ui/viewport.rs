//! Central level canvas: tile grid, overlays, zoom/pan, selection, painting.
//!
//! ROM-backed levels render real reconstructed tile pixels — each metatile is
//! rasterised once (`rendering::tile_renderer::render_metatile_rgba`) and cached
//! as an egui texture keyed by metatile id. Synthetic / no-graphics levels fall
//! back to flat metatile colors (`metatile_color`).

use egui::{Color32, ColorImage, Pos2, Rect, Sense, Stroke, TextureOptions, Vec2};

use crate::app::DaffyApp;
use crate::editor::commands::EditorCommand;
use crate::editor::selection::Selection;
use crate::editor::tools::Tool;
use crate::model::level::{METATILE_PX, SCREEN_H_METATILES, SCREEN_W_METATILES};
use crate::rendering::tile_renderer::{metatile_color, render_metatile_rgba, METATILE_RENDER_PX};

/// Build/refresh the metatile texture cache for the active level, then return
/// whether the level has real tile graphics (and the world-space metatile edge
/// length to draw at). Textures are rebuilt lazily and only for metatiles that
/// appear in the cache miss set.
///
/// Shared with the metatile picker (`ui::panels`) so both the canvas and the
/// picker draw the same rasterised tiles from one cache.
pub fn ensure_tile_textures(app: &mut DaffyApp, ctx: &egui::Context) -> bool {
    let Some((level_id, count, has_gfx)) = app
        .level()
        .map(|level| (level.id, level.metatiles.len(), !level.gfx.is_empty()))
    else {
        return false;
    };
    if !has_gfx {
        return false;
    }
    if app.tile_textures_level != Some(level_id) {
        app.tile_textures.clear();
        app.tile_textures_level = Some(level_id);
    }
    for id in 0..count as u16 {
        if app.tile_textures.contains_key(&id) {
            continue;
        }
        // Short immutable borrow to rasterise; ends before we touch the cache.
        let image = {
            let level = app.level().expect("level exists");
            level.metatiles.get(id as usize).and_then(|m| {
                render_metatile_rgba(&level.gfx, &level.palette, m).map(|img| {
                    ColorImage::from_rgba_unmultiplied(
                        [METATILE_RENDER_PX, METATILE_RENDER_PX],
                        &img.pixels,
                    )
                })
            })
        };
        if let Some(image) = image {
            let tex = ctx.load_texture(format!("mt{id}"), image, TextureOptions::NEAREST);
            app.tile_textures.insert(id, tex);
        }
    }
    true
}

pub fn central_viewport(app: &mut DaffyApp, ctx: &egui::Context) {
    // Rasterise real tile graphics into the texture cache (no-op for synthetic).
    let has_gfx = ensure_tile_textures(app, ctx);
    egui::CentralPanel::default().show(ctx, |ui| {
        let (response, painter) =
            ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let origin = response.rect.min;
        let to_local = |p: Pos2| [p.x - origin.x, p.y - origin.y];

        // --- input: zoom (scroll around cursor) ---
        if let Some(hover) = response.hover_pos() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.1 {
                let factor = (scroll * 0.0035).exp();
                app.prefs.viewport.zoom_at(to_local(hover), factor);
            }
        }

        // --- input: pan (middle/right drag, or left drag with Select tool
        //     when not dragging an object) ---
        let drag = response.drag_delta();
        let panning = response.dragged_by(egui::PointerButton::Middle)
            || response.dragged_by(egui::PointerButton::Secondary)
            || (app.tool == Tool::Select
                && response.dragged_by(egui::PointerButton::Primary)
                && !matches!(app.selection, Selection::Object { .. }));
        if panning && drag != Vec2::ZERO {
            app.prefs.viewport.pan_screen([drag.x, drag.y]);
        }

        // --- gather room data (immutable pass) ---
        let room_idx = app.active_room;
        let Some(level_idx) = app.active_project_level_index() else { return };
        let Some(level) = app.project.levels.get(level_idx) else { return };
        let Some(room) = level.rooms.get(room_idx) else { return };

        let vp = app.prefs.viewport;
        // ROM levels draw 32px metatiles (real 4×4-tile pixels); synthetic uses 16.
        let tile_px = if has_gfx { METATILE_RENDER_PX as f32 } else { METATILE_PX as f32 };
        let room_px_w = room.width as f32 * tile_px;
        let room_px_h = room.height as f32 * tile_px;
        let to_screen = |wx: f32, wy: f32| -> Pos2 {
            let [sx, sy] = vp.world_to_screen([wx, wy]);
            Pos2::new(sx + origin.x, sy + origin.y)
        };

        // hovered tile for the status bar / coordinate inspector
        app.hovered_tile = response.hover_pos().and_then(|p| {
            vp.tile_at_screen(to_local(p), tile_px)
                .filter(|&(x, y)| x < room.width && y < room.height)
        });

        // --- draw: background ---
        painter.rect_filled(response.rect, 0.0, Color32::from_gray(24));

        // --- draw: tiles (only the visible range) ---
        let [w0, h0] = vp.screen_to_world([0.0, 0.0]);
        let [w1, h1] =
            vp.screen_to_world([response.rect.width(), response.rect.height()]);
        let x_min = (w0 / tile_px).floor().max(0.0) as u32;
        let y_min = (h0 / tile_px).floor().max(0.0) as u32;
        let x_max = ((w1 / tile_px).ceil() as i64).clamp(0, room.width as i64) as u32;
        let y_max = ((h1 / tile_px).ceil() as i64).clamp(0, room.height as i64) as u32;

        for ty in y_min..y_max {
            for tx in x_min..x_max {
                let Some(id) = room.tile(tx, ty) else { continue };
                let rect = Rect::from_min_max(
                    to_screen(tx as f32 * tile_px, ty as f32 * tile_px),
                    to_screen((tx + 1) as f32 * tile_px, (ty + 1) as f32 * tile_px),
                );
                if let Some(tex) = app.tile_textures.get(&id) {
                    painter.image(
                        tex.id(),
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                } else {
                    let rgba = level
                        .metatiles
                        .get(id as usize)
                        .map(|m| metatile_color(&level.palette, m))
                        .unwrap_or([255, 0, 255, 255]);
                    painter.rect_filled(
                        rect,
                        0.0,
                        Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]),
                    );
                }
                if app.prefs.show_collision {
                    if let Some(m) = level.metatiles.get(id as usize) {
                        if m.collision != 0 {
                            painter.rect_stroke(
                                rect.shrink(1.0),
                                0.0,
                                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 0, 0, 160)),
                            );
                        }
                    }
                }
                if app.prefs.show_grid && vp.zoom >= 0.75 {
                    painter.rect_stroke(rect, 0.0, Stroke::new(0.5, Color32::from_gray(48)));
                }
            }
        }

        // --- draw: room border & screen boundaries ---
        let room_rect = Rect::from_min_max(to_screen(0.0, 0.0), to_screen(room_px_w, room_px_h));
        painter.rect_stroke(room_rect, 0.0, Stroke::new(1.5, Color32::from_gray(140)));
        if app.prefs.show_screen_bounds {
            let sw = SCREEN_W_METATILES as f32 * tile_px;
            let sh = SCREEN_H_METATILES as f32 * tile_px;
            let mut x = sw;
            while x < room_px_w {
                painter.line_segment(
                    [to_screen(x, 0.0), to_screen(x, room_px_h)],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 160, 255, 120)),
                );
                x += sw;
            }
            let mut y = sh;
            while y < room_px_h {
                painter.line_segment(
                    [to_screen(0.0, y), to_screen(room_px_w, y)],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 160, 255, 120)),
                );
                y += sh;
            }
        }

        // --- draw: overlays (objects, spawns, exits, checkpoints) ---
        if app.prefs.show_objects {
            for (i, o) in room.objects.iter().enumerate() {
                let p = to_screen(o.x as f32, o.y as f32);
                let selected = app.selection == Selection::Object { room: room_idx, index: i };
                painter.circle_filled(p, 5.0 * vp.zoom.clamp(0.5, 2.0), Color32::from_rgb(80, 170, 255));
                if selected {
                    painter.circle_stroke(p, 7.0 * vp.zoom.clamp(0.5, 2.0), Stroke::new(2.0, Color32::WHITE));
                }
                painter.text(
                    p + Vec2::new(8.0, -8.0),
                    egui::Align2::LEFT_BOTTOM,
                    &o.label,
                    egui::FontId::proportional(11.0),
                    Color32::from_rgb(160, 210, 255),
                );
            }
            for s in &room.enemy_spawns {
                let p = to_screen(s.x as f32, s.y as f32);
                painter.circle_filled(p, 4.0 * vp.zoom.clamp(0.5, 2.0), Color32::from_rgb(240, 90, 90));
            }
            for e in &room.exits {
                let p = to_screen(e.x as f32, e.y as f32);
                painter.rect_stroke(
                    Rect::from_center_size(p, Vec2::splat(10.0 * vp.zoom.clamp(0.5, 2.0))),
                    2.0,
                    Stroke::new(2.0, Color32::from_rgb(120, 240, 120)),
                );
            }
            for c in &room.checkpoints {
                let p = to_screen(c.x as f32, c.y as f32);
                painter.line_segment([p, p - Vec2::new(0.0, 14.0)], Stroke::new(2.0, Color32::GOLD));
                painter.circle_filled(p - Vec2::new(0.0, 14.0), 3.0, Color32::GOLD);
            }
        }

        // --- draw: selected tile outline ---
        if let Selection::Tile { room: r, x, y } = app.selection {
            if r == room_idx {
                let rect = Rect::from_min_max(
                    to_screen(x as f32 * tile_px, y as f32 * tile_px),
                    to_screen((x + 1) as f32 * tile_px, (y + 1) as f32 * tile_px),
                );
                painter.rect_stroke(rect, 0.0, Stroke::new(2.0, Color32::WHITE));
            }
        }

        // --- input: click / paint / object drag (mutating pass) ---
        let click_pos = response.interact_pointer_pos();
        let room_w = room.width;
        let room_h = room.height;
        let object_hit = |p: Pos2| -> Option<usize> {
            room.objects.iter().enumerate().find_map(|(i, o)| {
                let op = to_screen(o.x as f32, o.y as f32);
                (op.distance(p) <= 10.0).then_some(i)
            })
        };

        if let Some(pos) = click_pos {
            let tile = vp
                .tile_at_screen(to_local(pos), tile_px)
                .filter(|&(x, y)| x < room_w && y < room_h);
            match app.tool {
                Tool::Paint if response.clicked() || response.dragged_by(egui::PointerButton::Primary) => {
                    if let Some((x, y)) = tile {
                        let metatile = app.active_metatile;
                        let already = room.tile(x, y) == Some(metatile);
                        if !already {
                            let cmd = EditorCommand::SetTile { room: room_idx, x, y, metatile };
                            let level = app.project.levels.get_mut(level_idx).expect("level exists");
                            if app.history.apply(level, cmd).is_ok() {
                                app.selection = Selection::Tile { room: room_idx, x, y };
                                app.revalidate();
                            }
                        }
                    }
                }
                Tool::Select if response.clicked() => {
                    if let Some(i) = object_hit(pos) {
                        app.selection = Selection::Object { room: room_idx, index: i };
                    } else if let Some((x, y)) = tile {
                        app.selection = Selection::Tile { room: room_idx, x, y };
                    } else {
                        app.selection = Selection::None;
                    }
                }
                Tool::Select
                    if response.drag_stopped_by(egui::PointerButton::Primary) =>
                {
                    // Drop a dragged object at the release position.
                    if let Selection::Object { room: r, index } = app.selection {
                        if r == room_idx {
                            let [wx, wy] = vp.screen_to_world(to_local(pos));
                            if wx >= 0.0 && wy >= 0.0 {
                                let cmd = EditorCommand::MoveObject {
                                    room: room_idx,
                                    object: index,
                                    x: wx as u32,
                                    y: wy as u32,
                                };
                                let level = app.project.levels.get_mut(level_idx).expect("level exists");
                                if app.history.apply(level, cmd).is_ok() {
                                    app.revalidate();
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    });
}
