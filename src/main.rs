mod eval;
mod lexer;
mod parser;
mod printer;
mod workbook;

use std::env;
use std::io::{self, Read};
use std::process::ExitCode;

use workbook::Workbook;

fn main() -> ExitCode {
    let mut json_output = false;
    let mut eval_output = false;
    let mut formula_arg: Option<String> = None;
    let mut sets: Vec<String> = Vec::new();

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--json" {
            json_output = true;
        } else if arg == "--eval" {
            eval_output = true;
        } else if arg == "--set" {
            match args.next() {
                Some(assignment) => sets.push(assignment),
                None => {
                    eprintln!("error: --set requires an argument like 'A1=5'");
                    return ExitCode::FAILURE;
                }
            }
        } else if arg == "--help" || arg == "-h" {
            print_usage();
            return ExitCode::SUCCESS;
        } else if formula_arg.is_none() {
            formula_arg = Some(arg);
        } else {
            eprintln!("error: unexpected extra argument '{}'", arg);
            return ExitCode::FAILURE;
        }
    }

    let mut workbook = Workbook::new();
    for assignment in &sets {
        if let Err(message) = apply_set(&mut workbook, assignment) {
            eprintln!("error: {}", message);
            return ExitCode::FAILURE;
        }
    }

    let raw = match formula_arg {
        Some(f) => f,
        None => {
            let mut buf = String::new();
            match io::stdin().read_to_string(&mut buf) {
                Ok(_) => buf,
                Err(err) => {
                    eprintln!("error: failed to read formula from stdin: {}", err);
                    return ExitCode::FAILURE;
                }
            }
        }
    };

    let trimmed = raw.trim();
    let body = trimmed.strip_prefix('=').unwrap_or(trimmed);

    if body.trim().is_empty() {
        eprintln!("error: no formula given");
        return ExitCode::FAILURE;
    }

    match parser::parse(body) {
        Ok(expr) => {
            let canonical = format!("={}", printer::render(&expr));
            if json_output {
                let mut out = String::new();
                out.push_str("{\"valid\":true,\"input\":");
                printer::push_json_string(&mut out, trimmed);
                out.push_str(",\"canonical\":");
                printer::push_json_string(&mut out, &canonical);
                out.push_str(",\"ast\":");
                out.push_str(&printer::to_json(&expr));
                if eval_output {
                    match eval::eval(&expr, &workbook) {
                        Ok(value) => {
                            out.push_str(",\"value\":");
                            out.push_str(&eval::to_json(&value));
                        }
                        Err(unsupported) => {
                            out.push_str(",\"value\":null,\"valueError\":");
                            printer::push_json_string(&mut out, &unsupported.message());
                        }
                    }
                }
                out.push('}');
                println!("{}", out);
            } else {
                println!("valid");
                println!("{}", canonical);
                if eval_output {
                    match eval::eval(&expr, &workbook) {
                        Ok(value) => println!("value: {}", describe_value(&value)),
                        Err(unsupported) => println!("value: {}", unsupported.message()),
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            if json_output {
                let mut out = String::new();
                out.push_str("{\"valid\":false,\"input\":");
                printer::push_json_string(&mut out, trimmed);
                out.push_str(",\"error\":");
                printer::push_json_string(&mut out, &err.message);
                out.push_str(",\"position\":");
                out.push_str(&err.pos.to_string());
                out.push('}');
                println!("{}", out);
            } else {
                eprintln!("error at position {}: {}", err.pos, err.message);
                eprintln!("  {}", body);
                eprintln!("  {}^", " ".repeat(err.pos));
            }
            ExitCode::FAILURE
        }
    }
}

// Parses "TARGET=VALUE" and applies it to the workbook: TARGET is a cell
// reference (`A1`, `Sheet1!B2`) or a defined name, VALUE is any formula
// expression, evaluated against the workbook built from the `--set` flags
// seen so far so later assignments can refer to earlier ones.
fn apply_set(workbook: &mut Workbook, assignment: &str) -> Result<(), String> {
    let (target, value_text) = assignment
        .split_once('=')
        .ok_or_else(|| format!("--set '{}' is missing '='", assignment))?;

    let target_expr = parser::parse(target.trim())
        .map_err(|e| format!("--set target '{}': {}", target, e.message))?;
    let value_expr = parser::parse(value_text.trim())
        .map_err(|e| format!("--set value '{}': {}", value_text, e.message))?;
    let value = eval::eval(&value_expr, workbook)
        .map_err(|u| format!("--set value '{}': {}", value_text, u.message()))?;

    match target_expr {
        parser::Expr::Reference(cell) => {
            workbook.set_cell(cell.sheet.as_deref(), &cell.column, cell.row, value);
            Ok(())
        }
        parser::Expr::Name(name) => {
            workbook.set_name(&name, value);
            Ok(())
        }
        _ => Err(format!(
            "--set target '{}' is not a cell reference or a defined name",
            target
        )),
    }
}

fn describe_value(value: &eval::Value) -> String {
    match value {
        eval::Value::Number(n) => printer::format_number(*n),
        eval::Value::Text(s) => format!("\"{}\"", s),
        eval::Value::Boolean(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        eval::Value::Error(e) => e.clone(),
    }
}

fn print_usage() {
    println!("formula-fmt - validate and pretty-print spreadsheet formulas");
    println!();
    println!("usage:");
    println!("  formula-fmt \"=SUM(A1:A10)*2\"");
    println!("  echo \"=A1+B1\" | formula-fmt --json");
    println!("  formula-fmt --eval \"=1+2*3\"");
    println!("  formula-fmt --eval --set A1=5 --set B1=7 \"=A1+B1\"");
    println!();
    println!("options:");
    println!("  --json         emit machine-readable JSON instead of plain text");
    println!("  --eval         also compute and print the value");
    println!("  --set T=V      set cell/name T to the value of expression V before --eval");
    println!("                 (repeatable; later --set expressions can reference earlier ones)");
    println!("  --help         show this message");
}
