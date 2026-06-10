//! Validation of levels and projects before export.

use serde::{Deserialize, Serialize};

use crate::model::level::Level;
use crate::model::project::Project;
use crate::rom::version::RomVersion;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub message: String,
    /// Human-readable location, e.g. "level 0 / room 1 / tile (3, 4)".
    pub context: String,
}

fn issue(severity: Severity, context: String, message: String) -> ValidationIssue {
    ValidationIssue { severity, message, context }
}

pub fn validate_level(level: &Level) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let metatile_count = level.metatiles.len() as u16;
    let room_ids: Vec<u32> = level.rooms.iter().map(|r| r.id).collect();

    for room in &level.rooms {
        let ctx = |what: String| format!("level {} / room {} / {what}", level.id, room.id);

        if room.tiles.len() != (room.width * room.height) as usize {
            issues.push(issue(
                Severity::Error,
                ctx("tile grid".into()),
                format!(
                    "tile count {} does not match {}x{} dimensions",
                    room.tiles.len(),
                    room.width,
                    room.height
                ),
            ));
        }

        for (i, t) in room.tiles.iter().enumerate() {
            if t.metatile >= metatile_count {
                let (x, y) = (i as u32 % room.width.max(1), i as u32 / room.width.max(1));
                issues.push(issue(
                    Severity::Error,
                    ctx(format!("tile ({x}, {y})")),
                    format!(
                        "metatile index {} out of range (set has {metatile_count})",
                        t.metatile
                    ),
                ));
            }
        }

        for o in &room.objects {
            if o.x >= room.pixel_width() || o.y >= room.pixel_height() {
                issues.push(issue(
                    Severity::Warning,
                    ctx(format!("object {} ({})", o.id, o.label)),
                    format!(
                        "position ({}, {}) is outside room bounds {}x{} px",
                        o.x,
                        o.y,
                        room.pixel_width(),
                        room.pixel_height()
                    ),
                ));
            }
        }

        for s in &room.enemy_spawns {
            if s.x >= room.pixel_width() || s.y >= room.pixel_height() {
                issues.push(issue(
                    Severity::Warning,
                    ctx(format!("enemy spawn {}", s.id)),
                    format!("position ({}, {}) is outside room bounds", s.x, s.y),
                ));
            }
        }

        for e in &room.exits {
            if e.target_level == level.id && !room_ids.contains(&e.target_room) {
                issues.push(issue(
                    Severity::Error,
                    ctx(format!("exit {}", e.id)),
                    format!("exit targets nonexistent room {}", e.target_room),
                ));
            }
        }

        if let Some(c) = &room.collision {
            if c.cells.len() != (c.width * c.height) as usize {
                issues.push(issue(
                    Severity::Error,
                    ctx("collision map".into()),
                    format!(
                        "collision cell count {} does not match {}x{}",
                        c.cells.len(),
                        c.width,
                        c.height
                    ),
                ));
            }
        }
    }
    issues
}

pub fn validate_project(project: &Project) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    match &project.rom {
        Some(rom) if rom.version == RomVersion::Unknown => issues.push(issue(
            Severity::Warning,
            "project / rom".to_string(),
            format!(
                "ROM hash is unknown (CRC32 {:08X}); structure assumptions may not hold \
                 and editing is unsafe",
                rom.crc32
            ),
        )),
        Some(_) => {}
        None => issues.push(issue(
            Severity::Info,
            "project / rom".to_string(),
            "no ROM associated with this project".to_string(),
        )),
    }
    for level in &project.levels {
        issues.extend(validate_level(level));
    }
    issues
}
