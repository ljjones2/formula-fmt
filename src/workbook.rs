// A minimal in-memory workbook: a sparse grid of cell values plus defined
// names, so `eval` has something to resolve references and names against.
// There's no file format behind this yet - the CLI populates it from
// `--set` flags, and library callers populate it directly through this API.

use std::collections::HashMap;

use crate::eval::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CellKey {
    sheet: Option<String>,
    column: String,
    row: u32,
}

fn cell_key(sheet: Option<&str>, column: &str, row: u32) -> CellKey {
    CellKey {
        sheet: sheet.map(|s| s.to_ascii_uppercase()),
        column: column.to_ascii_uppercase(),
        row,
    }
}

pub struct Workbook {
    cells: HashMap<CellKey, Value>,
    names: HashMap<String, Value>,
}

impl Workbook {
    pub fn new() -> Self {
        Workbook {
            cells: HashMap::new(),
            names: HashMap::new(),
        }
    }

    pub fn set_cell(&mut self, sheet: Option<&str>, column: &str, row: u32, value: Value) {
        self.cells.insert(cell_key(sheet, column, row), value);
    }

    // Spreadsheets read an untouched cell as 0 in arithmetic; matching that
    // here means an unset reference resolves to a value instead of erroring,
    // which is the whole point of having a workbook.
    pub fn get_cell(&self, sheet: Option<&str>, column: &str, row: u32) -> Value {
        self.cells
            .get(&cell_key(sheet, column, row))
            .cloned()
            .unwrap_or(Value::Number(0.0))
    }

    pub fn set_name(&mut self, name: &str, value: Value) {
        self.names.insert(name.to_ascii_uppercase(), value);
    }

    // Unlike a cell, an unset name has no sensible default - a real
    // spreadsheet reports #NAME? for one, but only the caller knows whether
    // "not in this workbook" means "doesn't exist" or "not loaded yet", so
    // that's left to the caller rather than guessed at here.
    pub fn get_name(&self, name: &str) -> Option<Value> {
        self.names.get(&name.to_ascii_uppercase()).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_cell_reads_as_zero() {
        let wb = Workbook::new();
        assert_eq!(wb.get_cell(None, "A", 1), Value::Number(0.0));
    }

    #[test]
    fn set_cell_round_trips() {
        let mut wb = Workbook::new();
        wb.set_cell(None, "a", 1, Value::Number(5.0));
        assert_eq!(wb.get_cell(None, "A", 1), Value::Number(5.0));
    }

    #[test]
    fn sheet_qualified_cells_do_not_collide_with_unqualified() {
        let mut wb = Workbook::new();
        wb.set_cell(Some("Sheet1"), "A", 1, Value::Number(1.0));
        wb.set_cell(None, "A", 1, Value::Number(2.0));
        assert_eq!(wb.get_cell(Some("Sheet1"), "A", 1), Value::Number(1.0));
        assert_eq!(wb.get_cell(None, "A", 1), Value::Number(2.0));
    }

    #[test]
    fn sheet_names_match_case_insensitively() {
        let mut wb = Workbook::new();
        wb.set_cell(Some("Sheet1"), "A", 1, Value::Number(3.0));
        assert_eq!(wb.get_cell(Some("SHEET1"), "A", 1), Value::Number(3.0));
    }

    #[test]
    fn names_are_case_insensitive() {
        let mut wb = Workbook::new();
        wb.set_name("Total", Value::Number(9.0));
        assert_eq!(wb.get_name("TOTAL"), Some(Value::Number(9.0)));
    }

    #[test]
    fn unset_name_is_none() {
        let wb = Workbook::new();
        assert_eq!(wb.get_name("Missing"), None);
    }
}
