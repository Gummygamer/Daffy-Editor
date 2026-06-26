//! Editor command, undo/redo, dirty-state and selection tests.

use daffy_editor::editor::commands::EditorCommand;
use daffy_editor::editor::history::EditorHistory;
use daffy_editor::editor::selection::Selection;
use daffy_editor::model::level::synthetic_level;

#[test]
fn set_tile_applies() {
    let mut level = synthetic_level();
    let mut h = EditorHistory::new();
    let before = level.rooms[0].tile(2, 3).unwrap();
    let new = (before + 1) % level.metatiles.len() as u16;
    h.apply(
        &mut level,
        EditorCommand::SetTile {
            room: 0,
            x: 2,
            y: 3,
            metatile: new,
        },
    )
    .unwrap();
    assert_eq!(level.rooms[0].tile(2, 3).unwrap(), new);
    assert!(h.can_undo());
    assert!(!h.can_redo());
}

#[test]
fn undo_restores_previous_value() {
    let mut level = synthetic_level();
    let mut h = EditorHistory::new();
    let before = level.rooms[0].tile(1, 1).unwrap();
    h.apply(
        &mut level,
        EditorCommand::SetTile {
            room: 0,
            x: 1,
            y: 1,
            metatile: before + 1,
        },
    )
    .unwrap();
    assert!(h.undo(&mut level));
    assert_eq!(level.rooms[0].tile(1, 1).unwrap(), before);
    assert!(!h.can_undo());
    assert!(h.can_redo());
}

#[test]
fn redo_reapplies_edit() {
    let mut level = synthetic_level();
    let mut h = EditorHistory::new();
    let before = level.rooms[0].tile(1, 1).unwrap();
    h.apply(
        &mut level,
        EditorCommand::SetTile {
            room: 0,
            x: 1,
            y: 1,
            metatile: before + 1,
        },
    )
    .unwrap();
    h.undo(&mut level);
    assert!(h.redo(&mut level));
    assert_eq!(level.rooms[0].tile(1, 1).unwrap(), before + 1);
}

#[test]
fn undo_on_empty_history_is_noop() {
    let mut level = synthetic_level();
    let mut h = EditorHistory::new();
    assert!(!h.undo(&mut level));
    assert!(!h.redo(&mut level));
    assert_eq!(level, synthetic_level());
}

#[test]
fn new_edit_after_undo_clears_redo() {
    let mut level = synthetic_level();
    let mut h = EditorHistory::new();
    h.apply(
        &mut level,
        EditorCommand::SetTile {
            room: 0,
            x: 0,
            y: 0,
            metatile: 1,
        },
    )
    .unwrap();
    h.undo(&mut level);
    assert!(h.can_redo());
    h.apply(
        &mut level,
        EditorCommand::SetTile {
            room: 0,
            x: 0,
            y: 0,
            metatile: 2,
        },
    )
    .unwrap();
    assert!(!h.can_redo());
}

#[test]
fn move_object_round_trips_through_undo_redo() {
    let mut level = synthetic_level();
    let mut h = EditorHistory::new();
    let (ox, oy) = {
        let o = &level.rooms[0].objects[0];
        (o.x, o.y)
    };
    h.apply(
        &mut level,
        EditorCommand::MoveObject {
            room: 0,
            object: 0,
            x: ox + 8,
            y: oy + 8,
        },
    )
    .unwrap();
    assert_eq!(level.rooms[0].objects[0].x, ox + 8);
    h.undo(&mut level);
    assert_eq!(
        (level.rooms[0].objects[0].x, level.rooms[0].objects[0].y),
        (ox, oy)
    );
    h.redo(&mut level);
    assert_eq!(
        (level.rooms[0].objects[0].x, level.rooms[0].objects[0].y),
        (ox + 8, oy + 8)
    );
}

#[test]
fn move_enemy_spawn_round_trips_through_undo_redo() {
    let mut level = synthetic_level();
    let mut h = EditorHistory::new();
    let (ox, oy) = {
        let s = &level.rooms[0].enemy_spawns[0];
        (s.x, s.y)
    };
    h.apply(
        &mut level,
        EditorCommand::MoveEnemySpawn {
            room: 0,
            spawn: 0,
            x: ox + 8,
            y: oy + 8,
        },
    )
    .unwrap();
    assert_eq!(level.rooms[0].enemy_spawns[0].x, ox + 8);
    h.undo(&mut level);
    assert_eq!(
        (
            level.rooms[0].enemy_spawns[0].x,
            level.rooms[0].enemy_spawns[0].y
        ),
        (ox, oy)
    );
    h.redo(&mut level);
    assert_eq!(
        (
            level.rooms[0].enemy_spawns[0].x,
            level.rooms[0].enemy_spawns[0].y
        ),
        (ox + 8, oy + 8)
    );
}

#[test]
fn delete_and_insert_object_are_undoable() {
    let mut level = synthetic_level();
    let mut h = EditorHistory::new();
    let removed = level.rooms[0].objects[1].clone();

    h.apply(
        &mut level,
        EditorCommand::DeleteObject { room: 0, object: 1 },
    )
    .unwrap();
    assert_eq!(level.rooms[0].objects.len(), 1);

    h.undo(&mut level);
    assert_eq!(level.rooms[0].objects[1], removed);

    h.redo(&mut level);
    assert_eq!(level.rooms[0].objects.len(), 1);
}

#[test]
fn delete_and_insert_enemy_spawn_are_undoable() {
    let mut level = synthetic_level();
    let mut h = EditorHistory::new();
    let removed = level.rooms[0].enemy_spawns[1].clone();

    h.apply(
        &mut level,
        EditorCommand::DeleteEnemySpawn { room: 0, spawn: 1 },
    )
    .unwrap();
    assert_eq!(level.rooms[0].enemy_spawns.len(), 1);

    h.undo(&mut level);
    assert_eq!(level.rooms[0].enemy_spawns[1], removed);

    h.redo(&mut level);
    assert_eq!(level.rooms[0].enemy_spawns.len(), 1);
}

#[test]
fn dirty_state_tracks_save_point() {
    let mut level = synthetic_level();
    let mut h = EditorHistory::new();
    assert!(!h.is_dirty());

    h.apply(
        &mut level,
        EditorCommand::SetTile {
            room: 0,
            x: 0,
            y: 0,
            metatile: 1,
        },
    )
    .unwrap();
    assert!(h.is_dirty());

    h.mark_saved();
    assert!(!h.is_dirty());

    h.undo(&mut level);
    assert!(
        h.is_dirty(),
        "undoing past the save point makes the document dirty"
    );

    h.redo(&mut level);
    assert!(
        !h.is_dirty(),
        "redoing back to the save point makes it clean again"
    );
}

#[test]
fn command_on_invalid_target_errors_and_changes_nothing() {
    let mut level = synthetic_level();
    let pristine = level.clone();
    let mut h = EditorHistory::new();

    assert!(h
        .apply(
            &mut level,
            EditorCommand::SetTile {
                room: 99,
                x: 0,
                y: 0,
                metatile: 0
            }
        )
        .is_err());
    assert!(h
        .apply(
            &mut level,
            EditorCommand::SetTile {
                room: 0,
                x: 9999,
                y: 0,
                metatile: 0
            }
        )
        .is_err());
    assert!(h
        .apply(
            &mut level,
            EditorCommand::MoveObject {
                room: 0,
                object: 9999,
                x: 0,
                y: 0
            }
        )
        .is_err());
    assert!(h
        .apply(
            &mut level,
            EditorCommand::MoveEnemySpawn {
                room: 0,
                spawn: 9999,
                x: 0,
                y: 0
            }
        )
        .is_err());
    assert!(h
        .apply(
            &mut level,
            EditorCommand::DeleteEnemySpawn {
                room: 0,
                spawn: 9999
            }
        )
        .is_err());

    assert_eq!(level, pristine);
    assert!(!h.can_undo(), "failed commands must not enter history");
    assert!(!h.is_dirty());
}

#[test]
fn selection_transitions() {
    let mut sel = Selection::None;
    assert!(!sel.is_some());

    sel = Selection::Tile {
        room: 0,
        x: 3,
        y: 4,
    };
    assert!(sel.is_some());

    sel = Selection::Object { room: 0, index: 1 };
    assert!(matches!(sel, Selection::Object { index: 1, .. }));

    sel = Selection::EnemySpawn { room: 0, index: 1 };
    assert!(matches!(sel, Selection::EnemySpawn { index: 1, .. }));

    sel.clear();
    assert_eq!(sel, Selection::None);
}
