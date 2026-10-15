//! The part of [`Catalog::check`] that holds screen parts to their player: on its screen, in its
//! pixel format, and to its touch layout's shape; and a player's touch records described once,
//! across its screens (the XDJ-700's deck screens share one touch layout).

use super::check::{Catalog, CatalogError, player_entry, screen_entry};
use super::player::{Player, TouchLayout};
use super::screen::Screen;
use crate::image::{MAX_IMAGE_BYTES, image_bytes};

/// One touch zone of a player's screens, for comparing records across them.
struct Zone<'a> {
    start: u64,
    end: u64,
    screen: &'a str,
    part: &'a str,
    described: (u32, u32, u32, u32, u32),
}

impl Catalog {
    pub(super) fn check_screen_parts(&self) -> Result<(), CatalogError> {
        for player in &self.players {
            let mut zones = Vec::new();
            for screen in self.screens.iter().filter(|s| s.player == player.id) {
                check_against_player(screen, player)?;
                let Some(layout) = &player.touch_layout else {
                    continue;
                };
                for (name, part) in &screen.parts {
                    for zone in &part.touch {
                        let start = u64::from(zone.record.0);
                        let area = &zone.area;
                        zones.push(Zone {
                            start,
                            end: start + u64::from(layout.record_bytes),
                            screen: &screen.id,
                            part: name,
                            described: (area.x, area.y, area.w, area.h, zone.handler.0),
                        });
                    }
                }
            }
            check_records_described_once(player, zones)?;
        }
        Ok(())
    }
}

fn check_against_player(screen: &Screen, player: &Player) -> Result<(), CatalogError> {
    let (width, height) = (player.screen_class.width, player.screen_class.height);
    let pixel = player.screen_class.pixels.bytes_per_pixel();
    for (name, part) in &screen.parts {
        let problem = |what: String| CatalogError::Inconsistent {
            entry: screen_entry(screen),
            problem: format!("part {name}: {what}"),
        };
        if !part.touch.is_empty() && player.touch_layout.is_none() {
            return Err(problem(format!(
                "player {} does not describe its touch layout",
                player.id
            )));
        }
        let layout = player.touch_layout.as_ref();
        for (index, zone) in part.touch.iter().enumerate() {
            let TouchLayout {
                record_alignment,
                handler_alignment,
                ..
            } = layout.expect("checked above");
            if zone.record.0 % record_alignment != 0 {
                return Err(problem(format!(
                    "touch[{index}].record is not {record_alignment}-byte aligned"
                )));
            }
            if zone.handler.0 % handler_alignment != 0 {
                return Err(problem(format!(
                    "touch[{index}].handler is not {handler_alignment}-byte aligned"
                )));
            }
            if !zone.area.fits(width, height) {
                return Err(problem(format!(
                    "touch[{index}] is not on the {width}x{height} screen"
                )));
            }
        }
        for (place, drawn) in part.drawn.iter().enumerate() {
            for (index, image) in drawn.images.iter().enumerate() {
                let field = format!("drawn[{place}].images[{index}]");
                if image.offset.0 % pixel != 0 {
                    return Err(problem(format!("{field} is not {pixel}-byte aligned")));
                }
                let bytes = image_bytes(image.width, image.height, pixel);
                if bytes.is_none_or(|bytes| bytes > MAX_IMAGE_BYTES) {
                    return Err(problem(format!("{field} is over 1 MiB")));
                }
                if !image.area(drawn.at).fits(width, height) {
                    return Err(problem(format!(
                        "{field} is not on the {width}x{height} screen"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Within a screen, no two zones name one record or overlapping records; across screens, two
/// zones that name overlapping records name the same record and describe it alike.
fn check_records_described_once(
    player: &Player,
    mut zones: Vec<Zone<'_>>,
) -> Result<(), CatalogError> {
    zones.sort_unstable_by(|a, b| (a.start, a.screen, a.part).cmp(&(b.start, b.screen, b.part)));
    for pair in zones.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if b.start >= a.end {
            continue;
        }
        let agree = a.start == b.start && a.described == b.described;
        if a.screen == b.screen || !agree {
            return Err(CatalogError::Inconsistent {
                entry: player_entry(&player.id),
                problem: format!(
                    "the touch records of {}/{} ({:#x}) and {}/{} ({:#x}) overlap{}",
                    a.screen,
                    a.part,
                    a.start,
                    b.screen,
                    b.part,
                    b.start,
                    if a.screen == b.screen {
                        ""
                    } else {
                        " and are described differently"
                    }
                ),
            });
        }
    }
    Ok(())
}
