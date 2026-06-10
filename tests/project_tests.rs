//! Project serialization and validation tests.

use daffy_editor::model::level::{synthetic_level, Exit, Object};
use daffy_editor::model::project::{PatchChange, Project, RomIdentity};
use daffy_editor::model::validation::{validate_level, validate_project, Severity};
use daffy_editor::rom::version::RomVersion;

fn sample_project() -> Project {
    Project {
        format_version: 1,
        name: "sample".to_string(),
        rom: Some(RomIdentity {
            crc32: 0x5F02_A044,
            sha1_hex: "0000000000000000000000000000000000000000".to_string(),
            size: 0x100000,
            had_copier_header: false,
            version: RomVersion::DaffyDuckMarvinMissionsUsa,
        }),
        levels: vec![synthetic_level()],
        changes: vec![PatchChange {
            offset: 0x8000,
            original: vec![0x00],
            modified: vec![0xEA],
            note: "example".to_string(),
        }],
    }
}

#[test]
fn project_json_round_trip() {
    let p = sample_project();
    let json = p.to_json().unwrap();
    let back = Project::from_json(&json).unwrap();
    assert_eq!(p, back);
}

#[test]
fn project_file_round_trip() {
    let p = sample_project();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.daffyproj.json");
    p.save_to_file(&path).unwrap();
    let back = Project::load_from_file(&path).unwrap();
    assert_eq!(p, back);
}

#[test]
fn project_from_invalid_json_errors() {
    assert!(Project::from_json("{not json").is_err());
    assert!(Project::from_json("{}").is_err());
}

#[test]
fn project_schema_snapshot() {
    // Guards against accidental breaking changes to the project file format.
    let mut p = sample_project();
    p.levels.clear(); // keep the snapshot small; level schema is covered by round-trip tests
    insta::assert_json_snapshot!(p);
}

#[test]
fn synthetic_level_validates_clean() {
    let level = synthetic_level();
    let issues = validate_level(&level);
    assert!(issues.is_empty(), "unexpected issues: {issues:?}");
}

#[test]
fn tile_index_out_of_range_is_error() {
    let mut level = synthetic_level();
    let bad = level.metatiles.len() as u16 + 5;
    level.rooms[0].set_tile(0, 0, bad).unwrap();
    let issues = validate_level(&level);
    assert!(issues.iter().any(|i| i.severity == Severity::Error));
}

#[test]
fn object_outside_room_bounds_is_warning() {
    let mut level = synthetic_level();
    let room = &mut level.rooms[0];
    room.objects.push(Object {
        id: 999,
        kind: 1,
        x: room.width * 16 + 100, // pixel coords way outside the room
        y: 0,
        params: vec![],
        label: "stray".to_string(),
    });
    let issues = validate_level(&level);
    assert!(issues
        .iter()
        .any(|i| i.severity == Severity::Warning && i.message.contains("bounds")));
}

#[test]
fn exit_to_nonexistent_room_is_error() {
    let mut level = synthetic_level();
    level.rooms[0].exits.push(Exit {
        id: 7,
        x: 0,
        y: 0,
        target_level: level.id,
        target_room: 12345,
    });
    let issues = validate_level(&level);
    assert!(issues
        .iter()
        .any(|i| i.severity == Severity::Error && i.message.contains("room")));
}

#[test]
fn unknown_rom_version_in_project_is_flagged() {
    let mut p = sample_project();
    p.rom.as_mut().unwrap().version = RomVersion::Unknown;
    let issues = validate_project(&p);
    assert!(issues
        .iter()
        .any(|i| i.severity == Severity::Warning && i.message.to_lowercase().contains("unknown")));
}
