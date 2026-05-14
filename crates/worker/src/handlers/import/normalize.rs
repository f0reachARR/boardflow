use boardflow_artifact::CoordinateMm;

/// Normalize finding severity to pass DB CHECK constraint.
/// Only "error", "warning", "notice" are accepted; anything else maps to "notice".
pub(super) fn normalize_severity(s: &str) -> &'static str {
    match s {
        "error" => "error",
        "warning" => "warning",
        "notice" => "notice",
        _ => "notice",
    }
}

/// Normalize subject_kind to pass DB CHECK constraint.
/// Only the five recognized kinds are accepted; anything else becomes `None`.
pub(super) fn normalize_subject_kind(sk: &str) -> Option<&'static str> {
    match sk {
        "schematic" => Some("schematic"),
        "pcb" => Some("pcb"),
        "net" => Some("net"),
        "footprint" => Some("footprint"),
        "symbol" => Some("symbol"),
        _ => None,
    }
}

/// Convert a coordinate from mm (floating-point) to µm (integer).
pub(super) fn pos_mm_to_um(pos: &CoordinateMm) -> (i32, i32) {
    let x = (pos.x * 1000.0).round() as i32;
    let y = (pos.y * 1000.0).round() as i32;
    (x, y)
}
