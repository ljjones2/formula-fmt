mod lexer;
mod parser;
mod printer;

use std::env;
use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut json_output = false;
    let mut formula_arg: Option<String> = None;

    for arg in env::args().skip(1) {
        if arg == "--json" {
            json_output = true;
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
                out.push('}');
                println!("{}", out);
            } else {
                println!("valid");
                println!("{}", canonical);
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

fn print_usage() {
    println!("formula-fmt - validate and pretty-print spreadsheet formulas");
    println!();
    println!("usage:");
    println!("  formula-fmt \"=SUM(A1:A10)*2\"");
    println!("  echo \"=A1+B1\" | formula-fmt --json");
    println!();
    println!("options:");
    println!("  --json    emit machine-readable JSON instead of plain text");
    println!("  --help    show this message");
}
