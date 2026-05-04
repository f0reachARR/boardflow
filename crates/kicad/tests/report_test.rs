use boardflow_kicad::report::{DrcReport, ErcReport};

#[test]
fn parse_erc_report_with_violations() {
    let json = r#"{
        "meta": {"version": 0},
        "sheets": [
            {
                "path": "/",
                "violations": [
                    {
                        "type": "pin_not_connected",
                        "description": "Pin not connected",
                        "severity": "error",
                        "items": [
                            {
                                "description": "Pin U1 pad 1",
                                "pos": {"x": 100.0, "y": 50.0}
                            }
                        ],
                        "excluded": false
                    },
                    {
                        "type": "power_pin_not_driven",
                        "description": "Power pin not driven",
                        "severity": "warning",
                        "items": [],
                        "excluded": false
                    }
                ]
            }
        ]
    }"#;

    let report = ErcReport::parse(json).unwrap();
    assert_eq!(report.sheets.len(), 1);
    assert_eq!(report.sheets[0].violations.len(), 2);
    assert_eq!(report.all_violations().len(), 2);
    assert_eq!(report.actionable_violations().len(), 2);
    assert!(report.has_errors());
}

#[test]
fn parse_erc_report_no_errors() {
    let json = r#"{
        "meta": {"version": 0},
        "sheets": [
            {
                "path": "/",
                "violations": [
                    {
                        "type": "pin_not_connected",
                        "description": "Pin not connected",
                        "severity": "warning",
                        "items": [],
                        "excluded": false
                    }
                ]
            }
        ]
    }"#;

    let report = ErcReport::parse(json).unwrap();
    assert!(!report.has_errors());
}

#[test]
fn parse_erc_report_excluded_violations_filtered() {
    let json = r#"{
        "meta": {"version": 0},
        "sheets": [
            {
                "path": "/",
                "violations": [
                    {
                        "type": "pin_not_connected",
                        "description": "Pin not connected",
                        "severity": "error",
                        "items": [],
                        "excluded": true
                    },
                    {
                        "type": "missing_power_flag",
                        "description": "Missing power flag",
                        "severity": "exclusion",
                        "items": [],
                        "excluded": false
                    }
                ]
            }
        ]
    }"#;

    let report = ErcReport::parse(json).unwrap();
    assert_eq!(report.all_violations().len(), 2);
    assert_eq!(report.actionable_violations().len(), 0);
    assert!(!report.has_errors());
}

#[test]
fn parse_erc_report_multiple_sheets() {
    let json = r#"{
        "meta": {"version": 0},
        "sheets": [
            {
                "path": "/",
                "violations": [
                    {
                        "type": "err1",
                        "description": "Error 1",
                        "severity": "error",
                        "items": [],
                        "excluded": false
                    }
                ]
            },
            {
                "path": "/sub",
                "violations": [
                    {
                        "type": "err2",
                        "description": "Error 2",
                        "severity": "error",
                        "items": [],
                        "excluded": false
                    }
                ]
            }
        ]
    }"#;

    let report = ErcReport::parse(json).unwrap();
    assert_eq!(report.all_violations().len(), 2);
}

#[test]
fn parse_erc_empty_sheets() {
    let json = r#"{"meta": {"version": 0}, "sheets": []}"#;
    let report = ErcReport::parse(json).unwrap();
    assert_eq!(report.all_violations().len(), 0);
    assert!(!report.has_errors());
}

#[test]
fn parse_drc_report_with_all_sections() {
    let json = r#"{
        "meta": {"version": 0},
        "violations": [
            {
                "type": "clearance",
                "description": "Clearance violation",
                "severity": "error",
                "items": [
                    {"description": "Pad to pad", "pos": {"x": 10.0, "y": 20.0}}
                ],
                "excluded": false
            }
        ],
        "unconnected_items": [
            {
                "type": "unconnected",
                "description": "Unconnected pad",
                "severity": "error",
                "items": [],
                "excluded": false
            }
        ],
        "schematic_parity": [
            {
                "type": "missing_footprint",
                "description": "Missing footprint",
                "severity": "warning",
                "items": [],
                "excluded": false
            }
        ]
    }"#;

    let report = DrcReport::parse(json).unwrap();
    assert_eq!(report.violations.len(), 1);
    assert_eq!(report.unconnected_items.len(), 1);
    assert_eq!(report.schematic_parity.len(), 1);
    assert_eq!(report.all_violations().len(), 3);
    assert!(report.has_errors());
}

#[test]
fn parse_drc_report_empty_optional_sections() {
    let json = r#"{
        "meta": {"version": 0},
        "violations": [
            {
                "type": "clearance",
                "description": "Clearance violation",
                "severity": "warning",
                "items": [],
                "excluded": false
            }
        ]
    }"#;

    let report = DrcReport::parse(json).unwrap();
    assert_eq!(report.unconnected_items.len(), 0);
    assert_eq!(report.schematic_parity.len(), 0);
    assert_eq!(report.all_violations().len(), 1);
    assert!(!report.has_errors());
}

#[test]
fn parse_drc_actionable_excludes_excluded() {
    let json = r#"{
        "meta": {"version": 0},
        "violations": [
            {
                "type": "clearance",
                "description": "Clearance violation",
                "severity": "error",
                "items": [],
                "excluded": true
            },
            {
                "type": "track_width",
                "description": "Track width violation",
                "severity": "error",
                "items": [],
                "excluded": false
            }
        ]
    }"#;

    let report = DrcReport::parse(json).unwrap();
    assert_eq!(report.all_violations().len(), 2);
    assert_eq!(report.actionable_violations().len(), 1);
    assert_eq!(
        report.actionable_violations()[0].violation_type,
        "track_width"
    );
}

#[test]
fn violation_item_position_parsing() {
    let json = r#"{
        "meta": {"version": 0},
        "sheets": [
            {
                "path": "/",
                "violations": [
                    {
                        "type": "test",
                        "description": "Test",
                        "severity": "error",
                        "items": [
                            {"description": "Item with pos", "pos": {"x": 1.5, "y": 2.5}},
                            {"description": "Item without pos"}
                        ],
                        "excluded": false
                    }
                ]
            }
        ]
    }"#;

    let report = ErcReport::parse(json).unwrap();
    let violation = &report.sheets[0].violations[0];
    assert_eq!(violation.items.len(), 2);
    let pos = violation.items[0].pos.as_ref().unwrap();
    assert_eq!(pos.x, 1.5);
    assert_eq!(pos.y, 2.5);
    assert!(violation.items[1].pos.is_none());
}

#[test]
fn parse_invalid_json_returns_error() {
    let result = ErcReport::parse("not json");
    assert!(result.is_err());

    let result = DrcReport::parse("{invalid}");
    assert!(result.is_err());
}
