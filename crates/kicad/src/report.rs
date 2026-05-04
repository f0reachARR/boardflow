use serde::Deserialize;

use crate::Result;

#[derive(Debug, Deserialize)]
pub struct ErcReport {
    pub sheets: Vec<ErcSheet>,
}

#[derive(Debug, Deserialize)]
pub struct ErcSheet {
    pub path: String,
    pub violations: Vec<Violation>,
}

#[derive(Debug, Deserialize)]
pub struct DrcReport {
    pub violations: Vec<Violation>,
    #[serde(default)]
    pub unconnected_items: Vec<Violation>,
    #[serde(default)]
    pub schematic_parity: Vec<Violation>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Violation {
    #[serde(rename = "type")]
    pub violation_type: String,
    pub description: String,
    pub severity: String,
    #[serde(default)]
    pub items: Vec<ViolationItem>,
    #[serde(default)]
    pub excluded: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ViolationItem {
    pub description: String,
    #[serde(default)]
    pub pos: Option<Position>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

impl ErcReport {
    pub fn parse(json: &str) -> Result<Self> {
        let report: Self = serde_json::from_str(json)?;
        Ok(report)
    }

    pub fn all_violations(&self) -> Vec<&Violation> {
        self.sheets.iter().flat_map(|s| &s.violations).collect()
    }

    pub fn actionable_violations(&self) -> Vec<&Violation> {
        self.all_violations()
            .into_iter()
            .filter(|v| v.is_actionable())
            .collect()
    }

    pub fn has_errors(&self) -> bool {
        self.actionable_violations()
            .iter()
            .any(|v| v.severity == "error")
    }
}

impl DrcReport {
    pub fn parse(json: &str) -> Result<Self> {
        let report: Self = serde_json::from_str(json)?;
        Ok(report)
    }

    pub fn all_violations(&self) -> Vec<&Violation> {
        self.violations
            .iter()
            .chain(&self.unconnected_items)
            .chain(&self.schematic_parity)
            .collect()
    }

    pub fn actionable_violations(&self) -> Vec<&Violation> {
        self.all_violations()
            .into_iter()
            .filter(|v| v.is_actionable())
            .collect()
    }

    pub fn has_errors(&self) -> bool {
        self.actionable_violations()
            .iter()
            .any(|v| v.severity == "error")
    }
}

impl Violation {
    pub fn is_actionable(&self) -> bool {
        !self.excluded && self.severity != "exclusion"
    }
}
